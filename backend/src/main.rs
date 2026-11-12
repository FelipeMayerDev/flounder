use flounder_backend::{Config, router};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let config = match args.next().as_deref() {
        None => Config::from_env(),
        Some("--local") => {
            let path = args.next().unwrap_or_else(|| ".env".into());
            if args.next().is_some() {
                return Err("Uso: flounder-backend [--local arquivo.env]".into());
            }
            Config::local_from_file(std::path::Path::new(&path))
        }
        _ => return Err("Uso: flounder-backend [--local arquivo.env]".into()),
    }
    .map_err(std::io::Error::other)?;
    let (bind, pluggy_configured) = (config.bind_address(), config.pluggy_configured());
    let app = router(config).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!(
        "Flounder backend: http://{}/connect",
        listener.local_addr()?
    );
    if !pluggy_configured {
        eprintln!(
            "Pluggy pendente: preencha CLIENT_ID e CLIENT_SECRET no arquivo local e reinicie."
        );
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
