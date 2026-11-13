//! Atualização automática: verifica a última release no GitHub, baixa o pacote desta plataforma
//! em segundo plano e troca o binário por clique ("Reiniciar agora").
//! A interface só troca mensagens por canal; nada de rede na thread da interface.
//! Funciona como AppImage (Linux) e executável solto (Windows); nas outras instalações
//! (tar.gz, cargo run) só avisa que existe versão nova.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// O backend de vocês publica os pacotes e o índice de release (`/releases/latest`).
const GITHUB: &str = "https://api.github.com/repos/FelipeMayerDev/flounder";
/// GitHub exige User-Agent em qualquer chamada à API.
const USER_AGENT: &str = concat!("flounder/", env!("CARGO_PKG_VERSION"));

/// Onde a verificação parou; a interface mostra o card de acordo.
#[derive(Default)]
pub enum UpdateState {
    /// Ainda sem resposta (ou sem worker: demonstração).
    #[default]
    Idle,
    Checking,
    /// Verificado agora: está em dia.
    Checked,
    /// Versão nova, mas sem instalação automática (Linux sem AppImage): atualizar na mão.
    Available { version: String },
    /// Baixando a versão nova; `frac` vai de 0 a 1.
    Downloading { version: String, frac: f32 },
    /// Pacote baixado: "Reiniciar agora" troca o binário e reabre o app.
    Ready { version: String, path: PathBuf },
    /// Falha da última verificação; "Tentar de novo" refaz.
    Failed(String),
}

pub enum Msg {
    /// Verificado agora: está em dia.
    UpToDate,
    /// Versão nova, mas sem instalação automática (Linux sem AppImage): atualizar na mão.
    Available { version: String },
    /// Download começou (versão nova com pacote para esta instalação).
    Downloading { version: String },
    Progress(f32),
    /// Pacote baixado em `path`; "Reiniciar agora" troca o binário e reabre o app.
    Ready { version: String, path: PathBuf },
    /// Falha ao verificar ou baixar; some no próximo "Verificar".
    Failed(String),
}

/// Thread de atualização. Soltar a struct encerra a thread.
pub struct Worker {
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Msg>,
}

pub enum Cmd {
    /// Refaz a verificação (e o download, se houver versão nova).
    Check,
}

impl Worker {
    /// Verifica já na abertura; depois responde um `Cmd::Check` por vez.
    pub fn start(ctx: egui::Context) -> Worker {
        let (tx, commands) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        std::thread::spawn(move || run(&ctx, &replies, &commands));
        Worker { tx, rx }
    }
}

fn run(ctx: &egui::Context, replies: &Sender<Msg>, commands: &Receiver<Cmd>) {
    let send = |m| {
        let _ = replies.send(m);
        ctx.request_repaint();
    };
    // Sobra de atualização interrompida no meio: `.part` esquecido ou `.old` do Windows.
    cleanup();
    check(&send);
    while matches!(commands.recv(), Ok(Cmd::Check)) {
        check(&send);
    }
}

/// Consulta a última release e, se for mais nova, baixa o pacote desta instalação.
fn check(send: &impl Fn(Msg)) {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let current = env!("CARGO_PKG_VERSION");
    let body = agent.get(&releases_url()).header("User-Agent", USER_AGENT).call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| match e {
            // Sem release publicada ainda: a API responde 404.
            ureq::Error::StatusCode(404) => "Nenhuma versão publicada ainda.".into(),
            ureq::Error::StatusCode(code) => format!("O GitHub respondeu com erro {code} ao verificar atualizações."),
            _ => "Não foi possível verificar atualizações. Sem conexão?".into(),
        });
    let body = match body {
        Ok(body) => body,
        Err(e) => return send(Msg::Failed(e)),
    };
    let release = match latest(&body) {
        Ok(r) => r,
        Err(e) => return send(Msg::Failed(e)),
    };
    let version = release.tag_name.trim_start_matches('v').to_string();
    if !newer(current, &version) {
        return send(Msg::UpToDate);
    }
    let Some(kind) = kind() else {
        // tar.gz, cargo run e afins: só dá para avisar.
        return send(Msg::Available { version });
    };
    let Some(asset) = asset_for(&kind, &version, &release.assets) else {
        return send(Msg::Failed(format!("A versão {version} não tem pacote para este sistema. Atualize na página de releases.")));
    };
    // O servidor de releases pode devolver caminho relativo (`/releases/…`): junta com a origem.
    let mut asset = asset.clone();
    if asset.url.starts_with('/') {
        asset.url = format!("{}{}", origin(&releases_url()), asset.url);
    }
    send(Msg::Downloading { version: version.clone() });
    let target = match &kind {
        Kind::AppImage(path) | Kind::WindowsExe(path) => path.clone(),
    };
    // O download não tem prazo: o pacote é grande e a conexão pode ser lenta.
    let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
    match download(&agent, &asset, &target, send) {
        Ok(path) => send(Msg::Ready { version, path }),
        Err(e) => send(Msg::Failed(e)),
    }
}

/// Baixa o pacote para um arquivo temporário no diretório do binário (mesmo sistema de
/// arquivos: o troque por rename é atômico). Confere o tamanho no fim.
fn download(agent: &ureq::Agent, asset: &Asset, target: &Path, send: &impl Fn(Msg)) -> Result<PathBuf, String> {
    let fail = |temp: &Path, e: String| -> String {
        let _ = std::fs::remove_file(temp);
        e
    };
    let temp = temp_path(target);
    let resp = agent.get(&asset.url).header("User-Agent", USER_AGENT).call()
        .map_err(|e| fail(&temp, format!("Não foi possível baixar a versão nova. {e}")))?;
    // Tamanho esperado: Content-Length; sem ele, o `size` que a API da release informou.
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok())
        .or((asset.size > 0).then_some(asset.size));
    let mut file = std::fs::File::create(&temp).map_err(|e| fail(&temp, format!("Não foi possível gravar o download ao lado do Flounder: {e}")))?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = [0u8; 64 * 1024];
    let (mut count, mut last) = (0u64, 0.0f32);
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = std::io::Write::write_all(&mut file, &buf[..n]) {
                    return Err(fail(&temp, format!("O download foi interrompido. {e}")));
                }
                count += n as u64;
                let frac = total.filter(|t| *t > 0).map_or(0.0, |t| count as f32 / t as f32);
                if frac - last >= 0.005 {
                    last = frac;
                    send(Msg::Progress(frac));
                }
            }
            Err(e) => return Err(fail(&temp, format!("O download foi interrompido. {e}"))),
        }
    }
    if let Some(total) = total && count != total {
        return Err(fail(&temp, format!("O download veio incompleto ({count} de {total} bytes). Tente de novo.")));
    }
    Ok(temp)
}

/// Troca o binário pelo pacote baixado e reabre o app. Quem chama fecha a janela em
/// seguida, para o eframe gravar o estado normalmente.
pub fn install(path: &Path) -> Result<(), String> {
    match kind().ok_or("Não sei como instalar nesta instalação (nem AppImage nem executável solto).")? {
        Kind::AppImage(target) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("Não foi possível marcar o pacote como executável: {e}"))?;
            }
            std::fs::rename(path, &target).map_err(|e| format!("Não foi possível substituir o AppImage: {e}"))?;
        }
        Kind::WindowsExe(target) => {
            let old = target.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&target, &old).map_err(|e| format!("Não foi possível mover o Flounder atual: {e}"))?;
            if let Err(e) = std::fs::rename(path, &target) {
                // Devolve o antigo para não deixar a instalação quebrada.
                let _ = std::fs::rename(&old, &target);
                return Err(format!("Não foi possível instalar a versão nova: {e}"));
            }
        }
    }
    match std::process::Command::new(kind_path()).spawn() {
        Ok(_) => Ok(()),
        Err(_) => Err("A versão nova já está no lugar. Feche e abra o Flounder para começar a usar.".into()),
    }
}

/// `.part` ao lado do AppImage/executável (mesma pasta: rename atômico).
fn temp_path(target: &Path) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    target.with_file_name(format!(".{name}.part"))
}

/// Limpa o que sobrou de uma atualização interrompida.
fn cleanup() {
    let Some(target) = kind().map(|k| match k {
        Kind::AppImage(path) | Kind::WindowsExe(path) => path,
    }) else { return };
    let _ = std::fs::remove_file(temp_path(&target));
    if cfg!(windows) {
        let _ = std::fs::remove_file(target.with_extension("exe.old"));
    }
}

/// Como este Flounder foi instalado; `None` não se atualiza sozinho.
pub enum Kind {
    /// O runtime do AppImage exporta `APPIMAGE` com o caminho do `.AppImage`.
    AppImage(PathBuf),
    WindowsExe(PathBuf),
}

pub fn kind() -> Option<Kind> {
    if let Some(path) = std::env::var_os("APPIMAGE") {
        return Some(Kind::AppImage(path.into()));
    }
    if cfg!(windows) {
        return std::env::current_exe().ok().map(Kind::WindowsExe);
    }
    None
}

fn kind_path() -> PathBuf {
    match kind().expect("install só roda com kind") {
        Kind::AppImage(path) | Kind::WindowsExe(path) => path,
    }
}

/// Padrão: o próprio servidor de vocês (`/releases/latest`); o GitHub é o fallback quando
/// `FLOUNDER_RELEASES_URL` aponta para ele (e nos testes, para um servidor local).
fn releases_url() -> String {
    let url = std::env::var("FLOUNDER_RELEASES_URL")
        .unwrap_or_else(|_| if crate::backend::dev_checkout() { format!("{GITHUB}/releases/latest") } else { format!("{}/releases/latest", crate::backend::SERVIDOR) });
    url.trim_end_matches('/').to_string()
}

/// Origem de uma URL (`https://host[:porta]`), para resolver caminhos relativos dos ativos.
fn origin(url: &str) -> &str {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let host = rest.split('/').next().unwrap_or(rest);
    &url[..scheme.len() + 3 + host.len()]
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, serde::Deserialize)]
struct Asset {
    name: String,
    #[serde(rename = "browser_download_url")]
    url: String,
    #[serde(default)]
    size: u64,
}

fn latest(json: &str) -> Result<Release, String> {
    serde_json::from_str(json).map_err(|_| "Resposta inesperada do GitHub ao verificar atualizações.".into())
}

/// true se `latest` é mais nova que `current` ("v" opcional; números por componente,
/// componente que falta vale 0; algo que não é número não atualiza).
fn newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Option<Vec<u64>> {
        v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok()).collect()
    };
    let (Some(current), Some(latest)) = (parse(current), parse(latest)) else { return false };
    for i in 0..current.len().max(latest.len()) {
        let (a, b) = (current.get(i).copied().unwrap_or(0), latest.get(i).copied().unwrap_or(0));
        if a != b {
            return b > a;
        }
    }
    false
}

/// Pacote certo para esta instalação, na versão nova (mesmos nomes do empacotamento).
fn asset_for<'a>(kind: &Kind, version: &str, assets: &'a [Asset]) -> Option<&'a Asset> {
    let name = match kind {
        Kind::AppImage(_) => format!("Flounder-{version}-x86_64.AppImage"),
        Kind::WindowsExe(_) => format!("flounder-{version}-windows-x86_64.exe"),
    };
    assets.iter().find(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_compares_numbers_per_component() {
        assert!(newer("0.2.0", "0.3.0"));
        assert!(newer("0.2.0", "1.0.0"));
        assert!(newer("0.2.0", "0.2.1"));
        assert!(newer("0.2", "0.2.1"));
        assert!(!newer("0.3.0", "0.2.9"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.2.0", "v0.2.0"));
        assert!(!newer("0.2.0", "0.1.0"));
        assert!(!newer("0.2.0", "não é versão"));
        assert!(!newer("", "0.3.0"));
    }

    #[test]
    fn asset_matches_the_packaging_names() {
        let assets = |names: &[&str]| -> Vec<Asset> {
            names.iter().map(|n| Asset { name: n.to_string(), url: format!("https://x/{n}"), size: 1 }).collect()
        };
        let a = assets(&["flounder-0.3.0-linux-x86_64.tar.gz", "Flounder-0.3.0-x86_64.AppImage", "flounder-0.3.0-windows-x86_64.zip"]);
        assert_eq!(asset_for(&Kind::AppImage("x".into()), "0.3.0", &a).unwrap().name, "Flounder-0.3.0-x86_64.AppImage");
        assert!(asset_for(&Kind::WindowsExe("x".into()), "0.3.0", &a).is_none(), "sem .exe solto ainda");
        let b = assets(&["flounder-0.3.0-windows-x86_64.zip", "flounder-0.3.0-windows-x86_64.exe"]);
        assert_eq!(asset_for(&Kind::WindowsExe("x".into()), "0.3.0", &b).unwrap().name, "flounder-0.3.0-windows-x86_64.exe");
        let c = assets(&["Flounder-0.2.0-x86_64.AppImage"]);
        assert!(asset_for(&Kind::AppImage("x".into()), "0.3.0", &c).is_none(), "versão antiga não serve");
    }

    #[test]
    fn reads_release_json_from_github() {
        let json = r#"{
            "url": "…", "tag_name": "v0.3.0", "name": "0.3.0", "draft": false, "prerelease": false,
            "assets": [
                {"url": "…", "id": 1, "name": "Flounder-0.3.0-x86_64.AppImage", "size": 42424242,
                 "browser_download_url": "https://github.com/FelipeMayerDev/flounder/releases/download/v0.3.0/Flounder-0.3.0-x86_64.AppImage"},
                {"url": "…", "id": 2, "name": "flounder-0.3.0-windows-x86_64.exe", "size": 21212121,
                 "browser_download_url": "https://github.com/FelipeMayerDev/flounder/releases/download/v0.3.0/flounder-0.3.0-windows-x86_64.exe"}
            ]
        }"#;
        let r = latest(json).unwrap();
        assert_eq!(r.tag_name, "v0.3.0");
        assert_eq!(r.assets.len(), 2);
        let kind = Kind::WindowsExe("x".into());
        let a = asset_for(&kind, "0.3.0", &r.assets).unwrap();
        assert_eq!(a.url, "https://github.com/FelipeMayerDev/flounder/releases/download/v0.3.0/flounder-0.3.0-windows-x86_64.exe");
        assert_eq!(a.size, 21212121);
        assert!(latest("<html>taxa demais</html>").is_err());
    }

    #[test]
    fn origin_of_the_releases_url_resolves_relative_assets() {
        assert_eq!(origin("https://finance.felipemayer.com.br/releases/latest"), "https://finance.felipemayer.com.br");
        assert_eq!(origin("http://127.0.0.1:8080/releases/latest"), "http://127.0.0.1:8080");
        assert_eq!(origin("https://api.github.com/repos/x/releases/latest"), "https://api.github.com");
    }

    #[test]
    fn temp_file_hides_beside_the_target() {
        #[cfg(unix)]
        assert_eq!(temp_path(Path::new("/apps/Flounder-0.2.0-x86_64.AppImage")), Path::new("/apps/.Flounder-0.2.0-x86_64.AppImage.part"));
        #[cfg(windows)]
        assert_eq!(temp_path(Path::new(r"C:\Apps\flounder.exe")), Path::new(r"C:\Apps\.flounder.exe.part"));
    }
}
