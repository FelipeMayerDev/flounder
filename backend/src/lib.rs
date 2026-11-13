mod local;
mod db;
mod assistant;
mod pairing;
mod releases;
#[allow(dead_code)] // Snapshot só é usado pelo desktop.
#[path = "../../shared/cards.rs"]
mod connected;
#[path = "../../shared/api.rs"]
mod api;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

type ApiResult = Result<Json<Value>, ApiError>;

const CLIENTS: [&str; 2] = ["client-1", "client-2"];

pub struct Config {
    client_id: String,
    client_secret: String,
    tokens: [String; 2],
    /// Códigos fixos de pareamento (`PAIRING_CODE_N`, já normalizados); `None` = desligado.
    pairing_codes: [Option<String>; 2],
    /// Diretório com os pacotes publicados (`RELEASES_DIR`); `None` = sem releases aqui.
    releases_dir: Option<std::path::PathBuf>,
    bind: std::net::SocketAddr,
    database: tokio_postgres::Config,
    /// Modo local: `backend/.local/institutions.json` (item → banco) para o proxy MeuPluggy.
    institutions: Option<std::path::PathBuf>,
    /// Assistente (GLM via Z.ai): token opcional; sem ele, as rotas respondem que o
    /// assistente não está configurado.
    zai_token: Option<String>,
    zai_base: String,
    zai_model: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::read(|name| std::env::var(name).ok(), false)
    }

    pub fn local_from_file(path: &std::path::Path) -> Result<Self, String> {
        let values = local::values(path)?;
        let mut config = Self::read(
            |name| {
                std::env::var(name)
                    .ok()
                    .or_else(|| values.get(name).cloned())
            },
            true,
        )?;
        config.institutions = Some(path.with_file_name(".local").join("institutions.json"));
        Ok(config)
    }

    fn read(lookup: impl Fn(&str) -> Option<String>, local: bool) -> Result<Self, String> {
        let required = |name: &str| {
            lookup(name)
                .filter(|s| !s.trim().is_empty() && !s.starts_with("replace_"))
                .ok_or_else(|| format!("Defina {name} no ambiente do servidor."))
        };
        let bind: std::net::SocketAddr = lookup("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:3001".into())
            .parse()
            .map_err(|_| "BIND_ADDR deve ser IP:porta.".to_owned())?;
        if local && !bind.ip().is_loopback() {
            return Err("O modo --local deve usar um endereço loopback.".into());
        }
        let mut database: tokio_postgres::Config = required("DATABASE_URL")?
            .parse()
            .map_err(|_| "DATABASE_URL inválida: use host=… port=… user=… password=… dbname=… ou postgres://…".to_owned())?;
        if database.get_connect_timeout().is_none() {
            database.connect_timeout(Duration::from_secs(5));
        }
        let config = Self {
            client_id: if local {
                required("CLIENT_ID").unwrap_or_default()
            } else {
                required("CLIENT_ID")?
            },
            client_secret: if local {
                required("CLIENT_SECRET").unwrap_or_default()
            } else {
                required("CLIENT_SECRET")?
            },
            tokens: [required("APP_TOKEN_1")?, required("APP_TOKEN_2")?],
            pairing_codes: [lookup("PAIRING_CODE_1"), lookup("PAIRING_CODE_2")].map(|code| {
                code.map(|s| pairing::normalize(&s)).filter(|s| !s.is_empty() && !s.starts_with("replace_"))
            }),
            releases_dir: lookup("RELEASES_DIR").map(|s| std::path::PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty()),
            bind,
            database,
            institutions: None,
            zai_token: lookup("ZAI_TOKEN")
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty() && !s.starts_with("replace_")),
            zai_base: lookup("ZAI_BASE_URL")
                .map(|s| s.trim().trim_end_matches('/').to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.z.ai/api/paas/v4".into()),
            zai_model: lookup("ZAI_MODEL")
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "glm-5.3".into()),
        };
        if config
            .tokens
            .iter()
            .any(|s| s.len() < 32 || !s.is_ascii() || s.contains(char::is_whitespace))
            || config.tokens[0] == config.tokens[1]
        {
            return Err("APP_TOKEN_1 e APP_TOKEN_2 devem ser diferentes e ter pelo menos 32 caracteres sem espaços.".into());
        }
        for (n, code) in [("PAIRING_CODE_1", &config.pairing_codes[0]), ("PAIRING_CODE_2", &config.pairing_codes[1])] {
            if let Some(code) = code
                && (code.len() < 8
                    || !code.is_ascii()
                    || config.tokens.iter().any(|token| pairing::normalize(token) == *code))
            {
                return Err(format!("{n} precisa de pelo menos 8 caracteres (sem acentos) e ser diferente dos APP_TOKEN_*."));
            }
        }
        if let (Some(a), Some(b)) = (&config.pairing_codes[0], &config.pairing_codes[1])
            && a == b
        {
            return Err("PAIRING_CODE_1 e PAIRING_CODE_2 devem ser diferentes.".into());
        }
        Ok(config)
    }

    pub fn bind_address(&self) -> std::net::SocketAddr {
        self.bind
    }
    pub fn pluggy_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
    pub fn assistant_configured(&self) -> bool {
        self.zai_token.is_some()
    }
    pub fn pairing_configured(&self) -> bool {
        self.pairing_codes.iter().any(Option::is_some)
    }
}

struct Backend {
    config: Config,
    pluggy: Pluggy,
    /// Assistente GLM (Z.ai); `None` sem `ZAI_TOKEN`.
    zai: Option<assistant::Zai>,
    db: Mutex<Option<tokio_postgres::Client>>,
    /// Serializa `POST /api/sync`.
    sync: Mutex<()>,
    /// Serializa a geração de análises do assistente.
    assist: Mutex<()>,
    /// Tentativas erradas de código de pareamento (pairing.rs).
    pair_throttle: Mutex<pairing::Throttle>,
}

struct Pluggy {
    http: reqwest::Client,
    base: String,
    client_id: String,
    client_secret: String,
    key: Mutex<Option<(String, Instant)>>,
}

/// Conecta ao PostgreSQL, aplica as migrações pendentes e monta as rotas.
pub async fn router(config: Config) -> Result<Router, Box<dyn std::error::Error>> {
    let client = db::connect(&config.database)
        .await
        .map_err(|e| format!("Não foi possível conectar ao PostgreSQL (DATABASE_URL): {e}"))?;
    db::migrate(&client)
        .await
        .map_err(|e| format!("Falha ao aplicar as migrações: {e}"))?;
    Ok(build_router(config, "https://api.pluggy.ai", Some(client))?)
}

fn build_router(
    config: Config,
    base: &str,
    client: Option<tokio_postgres::Client>,
) -> Result<Router, reqwest::Error> {
    let pluggy = Pluggy {
        http: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        base: base.into(),
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
        key: Mutex::new(None),
    };
    let zai = match &config.zai_token {
        Some(token) => Some(assistant::Zai::new(
            token.clone(),
            config.zai_base.clone(),
            config.zai_model.clone(),
        )?),
        None => None,
    };
    let state = Arc::new(Backend {
        config,
        pluggy,
        zai,
        db: Mutex::new(client),
        sync: Mutex::new(()),
        assist: Mutex::new(()),
        pair_throttle: Mutex::new(pairing::Throttle::default()),
    });
    assistant::spawn_daily(state.clone());
    db::spawn_sync_loop(state.clone());
    let private = Router::new()
        .route("/api/connect-token", post(connect_token))
        .route("/api/items/{id}/accounts", get(accounts))
        .route("/api/accounts/{id}", get(account))
        .route("/api/accounts/{id}/transactions", get(transactions))
        .route("/api/accounts/{id}/bills", get(bills))
        .route("/api/state", get(db::state))
        .route("/api/entries", post(db::create_entry))
        .route("/api/entries/{id}", put(db::update_entry).delete(db::delete_entry))
        .route("/api/card-bills/paid", put(db::set_bill_paid))
        .route("/api/sync", post(db::sync))
        .route("/api/settlements", post(db::create_settlement))
        .route("/api/settlements/{id}", delete(db::delete_settlement))
        .route("/api/reserve", put(db::put_reserve))
        .route("/api/members/{position}", put(db::rename_member))
        .route("/api/connections/{item_id}/institution", put(db::set_connection_institution))
        .route("/api/settings", put(db::put_settings))
        .route("/api/assistant/reports", get(assistant::get_reports))
        .route("/api/assistant/refresh", post(assistant::post_refresh))
        .route("/api/pairing-code", get(pairing::codes))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    // A conversa pode levar o histórico inteiro: corpo maior só nela.
    let chat = Router::new()
        .route("/api/assistant/chat", post(assistant::post_chat))
        .layer(DefaultBodyLimit::max(65_536))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    Ok(Router::new()
        .route("/", get(|| async { Redirect::temporary("/connect") }))
        .route(
            "/connect",
            get(|| async { Html(include_str!("../static/connect.html")) }),
        )
        .route(
            "/connect.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../static/connect.js"),
                )
            }),
        )
        .route(
            "/health",
            get(|State(state): State<Arc<Backend>>| async move {
                Json(json!({
                    "status": "ok",
                    "pluggyConfigured": state.config.pluggy_configured(),
                    "assistant": state.config.assistant_configured(),
                    "pairing": state.config.pairing_configured(),
                }))
            }),
        )
        .route("/api/pair", post(pairing::pair))
        .route("/releases/latest", get(releases::latest))
        .route("/releases/{nome}", get(releases::pacote))
        .merge(private)
        .merge(chat)
        .with_state(state)
        .layer(DefaultBodyLimit::max(2048))
        .layer(middleware::from_fn(no_store)))
}

#[derive(Clone, Copy)]
struct User(&'static str);

async fn authorize(
    State(state): State<Arc<Backend>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("");
    let index = state
        .config
        .tokens
        .iter()
        .position(|expected| bool::from(token.as_bytes().ct_eq(expected.as_bytes())));
    let Some(index) = index else {
        return ApiError(StatusCode::UNAUTHORIZED, "Acesso não autorizado.").into_response();
    };
    request
        .extensions_mut()
        .insert(User(CLIENTS[index]));
    next.run(request).await
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

fn upstream_error(_: impl std::fmt::Display) -> ApiError {
    // Não devolva mensagens da API externa: podem conter dados financeiros ou credenciais.
    ApiError(
        StatusCode::BAD_GATEWAY,
        "Não foi possível consultar a Pluggy.",
    )
}

fn valid_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Identificador inválido."));
    }
    Ok(())
}

impl Pluggy {
    async fn api_key(&self) -> Result<String, ApiError> {
        if self.client_id.is_empty() || self.client_secret.is_empty() {
            return Err(ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Configure CLIENT_ID e CLIENT_SECRET no backend e reinicie o serviço.",
            ));
        }
        let mut cached = self.key.lock().await;
        if let Some((key, _)) = cached
            .as_ref()
            .filter(|(_, expires)| *expires > Instant::now())
        {
            return Ok(key.clone());
        }
        let response = self
            .http
            .post(format!("{}/auth", self.base))
            .json(&json!({"clientId": self.client_id, "clientSecret": self.client_secret}))
            .send()
            .await
            .map_err(upstream_error)?;
        if !response.status().is_success() {
            return Err(upstream_error("auth"));
        }
        let value: Value = response.json().await.map_err(upstream_error)?;
        let key = value
            .get("apiKey")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| upstream_error("key"))?
            .to_owned();
        *cached = Some((key.clone(), Instant::now() + Duration::from_secs(110 * 60)));
        Ok(key)
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        query: Value,
        body: Option<Value>,
    ) -> Result<Value, ApiError> {
        for attempt in 0..2 {
            let key = self.api_key().await?;
            let mut request = self
                .http
                .request(method.clone(), format!("{}{path}", self.base))
                .header("X-API-KEY", key)
                .query(&query);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(upstream_error)?;
            if response.status() == StatusCode::UNAUTHORIZED && attempt == 0 {
                *self.key.lock().await = None;
                continue;
            }
            if response.status() == StatusCode::NOT_FOUND {
                return Err(ApiError(StatusCode::NOT_FOUND, "Recurso não encontrado."));
            }
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                return Err(ApiError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Limite da Pluggy atingido. Tente novamente mais tarde.",
                ));
            }
            if !response.status().is_success() {
                return Err(upstream_error("status"));
            }
            return response.json().await.map_err(upstream_error);
        }
        Err(upstream_error("auth"))
    }

    async fn owned_item(&self, id: &str, user: User) -> Result<Value, ApiError> {
        valid_id(id)?;
        let item = self
            .call(Method::GET, &format!("/items/{id}"), json!({}), None)
            .await?;
        if item.get("clientUserId").and_then(Value::as_str) != Some(user.0) {
            return Err(ApiError(StatusCode::NOT_FOUND, "Recurso não encontrado."));
        }
        Ok(item)
    }

    async fn owned_account(&self, id: &str, user: User) -> Result<Value, ApiError> {
        valid_id(id)?;
        let value = self
            .call(Method::GET, &format!("/accounts/{id}"), json!({}), None)
            .await?;
        let item = value
            .get("itemId")
            .and_then(Value::as_str)
            .ok_or_else(|| upstream_error("itemId"))?;
        self.owned_item(item, user).await?;
        Ok(value)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectRequest {
    client_user_id: Option<String>,
}

async fn connect_token(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<ConnectRequest>,
) -> ApiResult {
    if input
        .client_user_id
        .as_deref()
        .is_some_and(|id| id != user.0)
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "clientUserId deve corresponder ao cliente autenticado.",
        ));
    }
    let value = state
        .pluggy
        .call(
            Method::POST,
            "/connect_token",
            json!({}),
            Some(json!({"options": {"clientUserId": user.0}})),
        )
        .await?;
    let access_token = value
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| upstream_error("connect"))?;
    Ok(Json(json!({"accessToken": access_token})))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Page {
    page: Option<u32>,
    page_size: Option<u16>,
}

impl Page {
    fn query(&self) -> Result<Value, ApiError> {
        let page = self.page.unwrap_or(1);
        let size = self.page_size.unwrap_or(500);
        if page == 0 || !(1..=500).contains(&size) {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Use page >= 1 e pageSize entre 1 e 500.",
            ));
        }
        Ok(json!({"page": page, "pageSize": size}))
    }
}

async fn accounts(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult {
    Ok(Json(state.import_item(user, &id).await?))
}

async fn account(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult {
    Ok(Json(state.pluggy.owned_account(&id, user).await?))
}

#[derive(Deserialize)]
struct Cursor {
    after: Option<String>,
}

async fn transactions(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> ApiResult {
    if cursor
        .after
        .as_ref()
        .is_some_and(|s| s.len() > 4096 || s.starts_with('?'))
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Use apenas o valor do cursor after.",
        ));
    }
    state.pluggy.owned_account(&id, user).await?;
    let mut query = json!({"accountId": id});
    if let Some(after) = cursor.after {
        query["after"] = json!(after);
    }
    Ok(Json(
        state
            .pluggy
            .call(Method::GET, "/v2/transactions", query, None)
            .await?,
    ))
}

async fn bills(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Query(page): Query<Page>,
) -> ApiResult {
    let mut query = page.query()?;
    state.pluggy.owned_account(&id, user).await?;
    query["accountId"] = json!(id);
    Ok(Json(
        state
            .pluggy
            .call(Method::GET, "/bills", query, None)
            .await?,
    ))
}

#[cfg(test)]
mod tests;
