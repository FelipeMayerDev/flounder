//! Ponta a ponta da atualização com HTTP de verdade, sem sair da máquina: um servidor local
//! finge ser o `releases/latest` do GitHub, o worker de verdade verifica, baixa o pacote e
//! "Reiniciar agora" troca o AppImage falso por rename e reabre o app.
//! `FLOUNDER_RELEASES_URL` e `APPIMAGE` apontam para o servidor e para um arquivo temporário.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use flounder::app::App;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    // As seções novas de Ajustes empurram o card para baixo: rolar até o botão antes do clique.
    hs.get_by_label_contains("Reiniciar agora").scroll_to_me();
    hs.run();
    hs.get_by_label_contains("Reiniciar agora").click();
    // O harness do kittest não encerra no ViewportCommand::Close (numa janela real o evento
    // fecha o app): rodar alguns passos basta para o clique aplicar a troca.
    hs.run_steps(3);

    assert_eq!(std::fs::read(&appimage).unwrap(), package, "o AppImage falso virou o pacote baixado");
    // O spawn é assíncrono: esperar o marcador que a versão nova escreve. Só no unix —
    // no Windows o stub `#!/bin/sh` não executa: a troca acontece, mas o install devolve
    // o erro de reabertura (que o app mostra em Ajustes).
    #[cfg(unix)]
    {
        let mut reabriu = false;
        for _ in 0..40 {
            if marker.exists() { reabriu = true; break; }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(reabriu, "a versão nova reabriu sozinha");
    }

    stop.store(true, Ordering::SeqCst);
    unsafe { std::env::remove_var("FLOUNDER_RELEASES_URL") };
    unsafe { std::env::remove_var("APPIMAGE") };
    let _ = std::fs::remove_dir_all(&dir);
}

/// Pareamento ponta a ponta com HTTP de verdade: o app novo digita o código, o servidor
/// falso responde com o token (`POST /api/pair`) e a sincronização busca o estado
/// (`GET /api/state`) — tudo sem arquivo de configuração.
#[test]
fn pairing_exchanges_the_code_over_real_http() {
    let stop = Arc::new(AtomicBool::new(false));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    let running = stop.clone();
    std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !running.load(Ordering::SeqCst) {
            let (mut conn, _) = match listener.accept() {
                Ok(c) => c,
                Err(_) => { std::thread::sleep(std::time::Duration::from_millis(50)); continue; }
            };
            conn.set_nonblocking(false).unwrap();
            let mut buf = [0u8; 8192];
            let n = loop {
                match conn.read(&mut buf) {
                    Ok(n) if String::from_utf8_lossy(&buf[..n]).contains("\r\n\r\n") => break n,
                    Ok(n) if n > 0 => continue,
                    _ => break 0,
                }
            };
            if n == 0 { continue; }
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            counted.fetch_add(1, Ordering::SeqCst);
            let (status, payload) = if request.starts_with("POST /api/pair") {
                let ok = request.contains("MARIA-2026-FLOUNDER");
                if ok {
                    ("200 OK", r#"{"client":"client-2","token":"token-pareado-2"}"#.to_owned())
                } else {
                    ("401 UNAUTHORIZED", r#"{"error":"Código de acesso inválido."}"#.to_owned())
                }
            } else {
                ("200 OK", r#"{"me":1,"members":[{"position":1,"name":"Larissa","client":"client-2"},{"position":0,"name":"Felipe","client":"client-1"}],"entries":[],"reserve":null,"cards":[]}"#.to_owned())
            };
            let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
            let _ = conn.write_all(head.as_bytes());
            let _ = conn.write_all(payload.as_bytes());
        }
    });

    unsafe { std::env::set_var("FLOUNDER_BACKEND_URL", &url) };
    unsafe { std::env::set_var("FLOUNDER_BACKEND_ENV", "/não-existe/flounder.env") };

    let mut app = App::load(None);
    let mut hs = Harness::builder().with_size([1024.0, 700.0]).with_max_steps(600).build_eframe(|cc| {
        flounder::widgets::install(&cc.egui_ctx);
        app
    });
    hs.run();
    assert!(hs.query_by_label_contains("Conectar ao servidor").is_some(), "sem arquivo e sem pareamento: tela de código");
    // A resposta do servidor chega assíncrona: rodar quadros até o texto aparecer.
    let espera = |hs: &mut Harness<App>, texto: &str| {
        for _ in 0..100 {
            hs.run();
            if hs.query_all_by_label_contains(texto).count() >= 1 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    };
    // Código errado: o servidor recusa e o app mostra o erro.
    hs.state_mut().pair.code = "ERRADO-1234".into();
    hs.get_by_label("Conectar").click();
    assert!(espera(&mut hs, "Código de acesso inválido."), "401 do servidor vira mensagem clara");
    // Código certo: pareia, busca o estado e entra direto como Larissa.
    hs.state_mut().pair.code = "MARIA-2026-FLOUNDER".into();
    hs.get_by_label("Conectar").click();
    assert!(espera(&mut hs, "Larissa"), "pareia e busca o estado por HTTP");
    assert!(hs.query_by_label_contains("Conectar ao servidor").is_none(), "pareou: tela de código some");
    assert!(hs.query_by_label("Quem é você?").is_none(), "o código já diz quem é");
    assert_eq!(hs.state().remote.as_ref().unwrap().member_token.as_deref(), Some("token-pareado-2"));
    assert!(requests.load(Ordering::SeqCst) >= 3, "pair (2×) + state por HTTP de verdade");

    stop.store(true, Ordering::SeqCst);
    unsafe { std::env::remove_var("FLOUNDER_BACKEND_URL") };
    unsafe { std::env::remove_var("FLOUNDER_BACKEND_ENV") };
}
