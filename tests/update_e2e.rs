//! Ponta a ponta da atualização com HTTP de verdade, sem sair da máquina: um servidor local
//! finge ser o `releases/latest` do GitHub, o worker de verdade verifica, baixa o pacote e
//! "Reiniciar agora" troca o AppImage falso por rename e reabre o app.
//! `FLOUNDER_RELEASES_URL` e `APPIMAGE` apontam para o servidor e para um arquivo temporário.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use flounder::app::App;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);
impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> { self.0.get(key).cloned() }
    fn set_string(&mut self, key: &str, value: String) { self.0.insert(key.into(), value); }
    fn remove_string(&mut self, key: &str) { self.0.remove(key); }
    fn flush(&mut self) {}
}

/// Servidor HTTP mínimo: `GET /releases` → JSON da release; `GET /download` → o "pacote".
fn serve(body: Vec<u8>, stop: Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("porta livre");
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let base = url.clone();
    std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop.load(Ordering::SeqCst) {
            let (mut conn, _) = match listener.accept() {
                Ok(c) => c,
                Err(_) => { std::thread::sleep(std::time::Duration::from_millis(50)); continue; }
            };
            conn.set_nonblocking(false).unwrap();
            let mut buf = [0u8; 4096];
            let n = loop {
                match conn.read(&mut buf) {
                    Ok(n) if String::from_utf8_lossy(&buf[..n]).contains("\r\n\r\n") => break n,
                    Ok(n) if n > 0 => continue,
                    _ => break 0,
                }
            };
            if n == 0 { continue; }
            let path = String::from_utf8_lossy(&buf[..n]).split_whitespace().nth(1).unwrap_or("/").to_string();
            let (ctype, payload) = if path.contains("/releases") {
                ("application/json", release_json(&base).into_bytes())
            } else {
                ("application/octet-stream", body.clone())
            };
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
            let _ = conn.write_all(head.as_bytes());
            let _ = conn.write_all(&payload);
        }
    });
    url
}

fn release_json(url: &str) -> String {
    // Pacote de 3 MiB para o progresso andar de verdade.
    let size = 3 * 1024 * 1024;
    format!(r#"{{"tag_name":"v0.3.0","assets":[{{"name":"Flounder-0.3.0-x86_64.AppImage","size":{size},"browser_download_url":"{url}/download"}}]}}"#)
}

#[test]
fn update_checks_downloads_and_swaps_the_appimage_end_to_end() {
    let dir = std::env::temp_dir().join(format!("flounder-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let appimage = dir.join("Flounder-0.2.0-x86_64.AppImage");
    std::fs::write(&appimage, "#!/bin/sh\necho antiga\n").unwrap();

    // O "pacote" é um script executável: depois do rename, o spawn dele prova a reabertura.
    let marker = dir.join("reaberto");
    let mut package = format!("#!/bin/sh\ntouch {}\n", marker.display()).into_bytes();
    package.resize(3 * 1024 * 1024, 0);
    let stop = Arc::new(AtomicBool::new(false));
    let url = serve(package.clone(), stop.clone());

    // unsafe: env de processo; nenhum outro teste lê estas variáveis (os workers deles são falsos).
    unsafe { std::env::set_var("FLOUNDER_RELEASES_URL", format!("{url}/releases")) };
    unsafe { std::env::set_var("APPIMAGE", &appimage) };

    let mut storage = MemoryStorage::default();
    storage.0.insert("member_client".into(), "client-1".into());
    let mut app = App::load(Some(&storage));
    let (tx, _commands) = std::sync::mpsc::channel();
    let (replies, rx) = std::sync::mpsc::channel();
    app.remote.as_mut().unwrap().worker = Some(flounder::backend::Worker { client: "client-1".into(), tx, rx, pending: 0 });
    replies.send(flounder::backend::Msg::State(Box::new(flounder::api::State {
        members: vec![
            flounder::api::Member { position: 0, name: "Ana".into(), client: "client-1".into() },
            flounder::api::Member { position: 1, name: "Bruno".into(), client: "client-2".into() },
        ],
        ..Default::default()
    }), vec![])).unwrap();
    // Sem worker de atualização falso de propósito: o de verdade é que está sendo testado.

    let mut hs = Harness::builder().with_size([1440.0, 1600.0]).with_max_steps(600).build_eframe(|cc| {
        flounder::widgets::install(&cc.egui_ctx);
        app
    });
    hs.run();
    hs.state_mut().screen = flounder::app::Screen::Settings;
    hs.run();
    assert!(hs.query_by_label_contains("Nova versão 0.3.0 baixada").is_some(), "verificou, baixou e ficou pronto");
    hs.get_by_label_contains("Reiniciar agora").click();
    // O harness do kittest não encerra no ViewportCommand::Close (numa janela real o evento
    // fecha o app): rodar alguns passos basta para o clique aplicar a troca.
    hs.run_steps(3);

    assert_eq!(std::fs::read(&appimage).unwrap(), package, "o AppImage falso virou o pacote baixado");
    // O spawn é assíncrono: esperar o marcador que a versão nova escreve.
    let mut reabriu = false;
    for _ in 0..40 {
        if marker.exists() { reabriu = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(reabriu, "a versão nova reabriu sozinha");

    stop.store(true, Ordering::SeqCst);
    unsafe { std::env::remove_var("FLOUNDER_RELEASES_URL") };
    unsafe { std::env::remove_var("APPIMAGE") };
    let _ = std::fs::remove_dir_all(&dir);
}
