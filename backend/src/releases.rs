//! Publicação dos pacotes: o diretório `RELEASES_DIR` (montado pelo Compose) é a fonte.
//! `/releases/latest` monta o índice no mesmo formato do GitHub (tag_name + assets) e
//! `/releases/{arquivo}` entrega o pacote — é o que a atualização automática consome.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use axum::body::Bytes;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{ApiError, ApiResult, Backend};

/// Padrões de nome dos pacotes (mesmos do empacotamento); `{v}` é a versão.
const PADROES: [&str; 4] = [
    "Flounder-{v}-x86_64.AppImage",
    "flounder-{v}-linux-x86_64.tar.gz",
    "flounder-{v}-windows-x86_64.zip",
    "flounder-{v}-windows-x86_64.exe",
];

fn nenhuma() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Nenhuma versão publicada ainda.")
}

/// Versão que o nome carrega, se seguir um dos padrões do empacotamento.
fn versao_do(nome: &str) -> Option<String> {
    for padrao in PADROES {
        let Some((antes, depois)) = padrao.split_once("{v}") else { continue };
        if let Some(resto) = nome.strip_prefix(antes)
            && let Some(v) = resto.strip_suffix(depois)
            && !v.is_empty()
            && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            return Some(v.to_owned());
        }
    }
    None
}

/// Chave de ordenação da versão ("0.2.10" > "0.2.9"); nomes estranhos não publicam.
fn chave(v: &str) -> Option<Vec<u64>> {
    v.split('.').map(|p| p.parse().ok()).collect()
}

/// Índice da versão mais recente entre os pacotes do diretório, no formato do GitHub.
pub async fn latest(State(state): State<Arc<Backend>>) -> ApiResult {
    let Some(dir) = state.config.releases_dir.clone() else { return Err(nenhuma()) };
    let mut achados: Vec<(Vec<u64>, String, String)> = Vec::new(); // (chave, versão, nome)
    for entrada in std::fs::read_dir(&dir).map_err(|_| nenhuma())?.flatten() {
        let nome = entrada.file_name().to_string_lossy().to_string();
        if let Some(v) = versao_do(&nome)
            && let Some(k) = chave(&v)
        {
            achados.push((k, v, nome));
        }
    }
    achados.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let Some((_, version, _)) = achados.first() else { return Err(nenhuma()) };
    let assets: Vec<Value> = achados
        .iter()
        .filter(|(_, v, _)| v == version)
        .map(|(_, _, nome)| {
            json!({
                "name": nome,
                "size": std::fs::metadata(dir.join(nome)).map(|m| m.len()).unwrap_or(0),
                // Caminho relativo: o app junta com a origem do índice que ele consultou.
                "browser_download_url": format!("/releases/{nome}"),
            })
        })
        .collect();
    Ok(Json(json!({"tag_name": format!("v{version}"), "assets": assets})))
}

/// Entrega um pacote do diretório de releases. Nome sem truque de caminho.
pub async fn pacote(
    State(state): State<Arc<Backend>>,
    Path(nome): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let Some(dir) = state.config.releases_dir.clone() else { return Err(nenhuma()) };
    if nome.len() > 128
        || nome.starts_with('.')
        || !nome
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Arquivo inválido."));
    }
    let bytes = tokio::fs::read(dir.join(&nome)).await.map_err(|_| nenhuma())?;
    let mime = if nome.ends_with(".zip") {
        "application/zip"
    } else if nome.ends_with(".exe") {
        "application/vnd.microsoft.portable-executable"
    } else if nome.ends_with(".AppImage") {
        "application/vnd.appimage"
    } else if nome.ends_with(".tar.gz") {
        "application/gzip"
    } else {
        "application/octet-stream"
    };
    Ok(([(header::CONTENT_TYPE, mime)], Bytes::from(bytes)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comes_from_the_packaging_names_only() {
        assert_eq!(versao_do("Flounder-0.2.1-x86_64.AppImage").as_deref(), Some("0.2.1"));
        assert_eq!(versao_do("flounder-0.2.1-windows-x86_64.exe").as_deref(), Some("0.2.1"));
        assert_eq!(versao_do("flounder-0.2.1-linux-x86_64.tar.gz").as_deref(), Some("0.2.1"));
        assert_eq!(versao_do("flounder-0.2.1-windows-x86_64.zip").as_deref(), Some("0.2.1"));
        assert_eq!(versao_do("Flounder-0.2.1-x86_64.AppImage.part"), None, "download incompleto não publica");
        assert_eq!(versao_do("README.md"), None);
        assert_eq!(versao_do("Flounder-x.y-x86_64.AppImage"), None);
    }

    #[test]
    fn chave_orders_semver_numerically() {
        let mut versoes = [chave("0.2.9").unwrap(), chave("0.2.10").unwrap(), chave("0.10.0").unwrap()];
        versoes.sort();
        assert_eq!(versoes, [vec![0, 2, 9], vec![0, 2, 10], vec![0, 10, 0]]);
        assert_eq!(chave("0.x"), None);
    }
}
