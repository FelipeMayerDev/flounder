use super::*;
use axum::body::to_bytes;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const TOKEN_1: &str = "private-client-one-0123456789abcdef";
const TOKEN_2: &str = "private-client-two-0123456789abcdef";

#[derive(Default)]
struct Mock {
    auth: AtomicUsize,
    transactions: AtomicUsize,
    reject: AtomicBool,
    fail: AtomicBool,
    extra_card: AtomicBool,
    /// Inclui uma compra pendente no cartão.
    pending: AtomicBool,
    /// Inclui uma fatura antiga (de mês anterior) em `/bills`.
    old_bill: AtomicBool,
    options: Mutex<Vec<Value>>,
}

struct Running {
    url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn spawn(router: Router) -> Running {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Running { url, task }
}

async fn mock(State(state): State<Arc<Mock>>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let Query(query) = Query::<HashMap<String, String>>::try_from_uri(request.uri()).unwrap();
    if path == "/auth" {
        let value: Value =
            serde_json::from_slice(&to_bytes(request.into_body(), 2048).await.unwrap()).unwrap();
        assert_eq!(value["clientSecret"], "test-secret");
        let count = state.auth.fetch_add(1, Ordering::SeqCst) + 1;
        return Json(json!({"apiKey": format!("test-api-key-{count}")})).into_response();
    }
    assert!(
        request
            .headers()
            .get("X-API-KEY")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("test-api-key-")
    );
    if state.reject.swap(false, Ordering::SeqCst) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if state.fail.swap(false, Ordering::SeqCst) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"secret": "test-secret", "apiKey": "do-not-expose"})),
        )
            .into_response();
    }
    let value = match path.as_str() {
        "/connect_token" => {
            let value: Value =
                serde_json::from_slice(&to_bytes(request.into_body(), 2048).await.unwrap())
                    .unwrap();
            state.options.lock().await.push(value);
            json!({"accessToken": "test-connect-token", "apiKey": "do-not-expose", "clientSecret": "test-secret"})
        }
        "/items/item-one" => {
            json!({"id": "item-one", "clientUserId": "client-1", "connector":{"id":200,"name":"MeuPluggy","primaryColor":"ef294b"}})
        }
        "/items/item-three" => {
            json!({"id": "item-three", "clientUserId": "client-1", "connector":{"name":"Outro Banco","primaryColor":"123456"}})
        }
        "/items/item-two" => json!({"id": "item-two", "clientUserId": "client-2"}),
        "/items/unbound" => json!({"id": "unbound", "clientUserId": null}),
        "/accounts/card-one" => {
            json!({"id": "card-one", "itemId": "item-one", "type": "CREDIT", "balance": 123.45, "creditData": {"creditLimit": 1000.0, "futureField": true}})
        }
        "/accounts/card-two" => json!({"id": "card-two", "itemId": "item-two", "type": "CREDIT"}),
        "/accounts" => {
            let mut results = vec![
                account(&format!("card-{}", query["itemId"])),
                json!({"id": format!("bank-{}", query["itemId"]), "type": "BANK", "subtype": "CHECKING_ACCOUNT",
                    "name": "Conta", "marketingName": null, "currencyCode": "BRL", "balance": 1500.25,
                    "updatedAt": "2026-11-10T12:34:56.789Z"}),
            ];
            if state.extra_card.load(Ordering::SeqCst) {
                results.push(account(&format!("old-{}", query["itemId"])));
            }
            json!({"results": results, "page": 1, "total": results.len(), "totalPages": 1})
        }
        "/v2/transactions" if query["accountId"].contains("-item-") => transactions(
            &query["accountId"],
            query.get("after").map(String::as_str),
            state.pending.load(Ordering::SeqCst),
        ),
        "/v2/transactions" => {
            state.transactions.fetch_add(1, Ordering::SeqCst);
            json!({"results": [{"id": "transaction-one", "amount": -123.45, "creditCardMetadata": {"installmentNumber": 2}}], "next": null, "query": query})
        }
        "/bills" => {
            let mut bill = bill();
            bill["id"] = json!(format!("bill-{}", query["accountId"]));
            let mut results = vec![bill];
            if state.old_bill.load(Ordering::SeqCst) {
                results.insert(0, json!({"id": format!("old-{}", query["accountId"]), "dueDate":"2020-01-25","totalAmount": 99.0,"totalAmountCurrencyCode":"BRL"}));
            }
            json!({"results": results, "page": query["page"], "pageSize": query["pageSize"]})
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    Json(value).into_response()
}

fn account(id: &str) -> Value {
    json!({"id": id, "type": "CREDIT", "name":"platinum","marketingName":null,"number":"123456781234","currencyCode":"BRL","balance":123.45,
        "updatedAt":"2026-11-10T12:34:56.789Z","creditData": {"creditLimit": 1000.0,"availableCreditLimit":876.55,"brand":"MASTERCARD",
        "balanceDueDate":"2026-11-25T00:00:00Z","balanceCloseDate":"2026-11-20T00:00:00Z"}})
}

/// Data UTC de `days_ago` dias atrás (AAAA-MM-DD), para ficar dentro da janela de 12 meses.
fn day(days_ago: i64) -> String {
    let days = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as i64
        - days_ago;
    // Conversão de dias desde 1970 para data civil (algoritmo de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Transações fictícias: conta corrente com salário, pagamento da fatura, transferência própria,
/// aplicação, mercado e uma antiga; cartão em duas páginas (cursor) com compra parcelada,
/// pagamento, estorno, pendente opcional e táxi.
fn transactions(account: &str, after: Option<&str>, pending: bool) -> Value {
    let t = |id: &str, kind: &str, amount: f64, category: &str, description: &str| {
        json!({"id": format!("{id}-{account}"), "type": kind, "amount": amount, "categoryId": category,
            "description": description, "date": format!("{}T03:00:00.000Z", day(10)), "status": "POSTED",
            "currencyCode": "BRL"})
    };
    if account.starts_with("bank-") {
        let mut old = t("old", "DEBIT", -1.0, "10000000", "Antiga");
        old["date"] = json!(day(400));
        return json!({"results": [
            t("salary", "CREDIT", 5000.0, "01010000", "Salário"),
            t("bill", "DEBIT", -800.0, "05000000", "Pagamento de fatura"),
            t("own", "DEBIT", -100.0, "04020000", "Transferência mesma titularidade"),
            t("invest", "DEBIT", -300.0, "03010000", "Aplicação"),
            t("market", "DEBIT", -50.5, "10000000", "Mercado"),
            old,
        ], "next": null});
    }
    if let Some(after) = after {
        assert_eq!(after, "page+2");
        return json!({"results": [t("taxi", "DEBIT", 30.0, "19010000", "Táxi")], "next": null});
    }
    let mut purchase = t("purchase", "DEBIT", 100.0, "08000000", "Loja");
    purchase["creditCardMetadata"] =
        json!({"installmentNumber": 2, "totalInstallments": 3, "billId": format!("bill-{account}"), "cardNumber": "5555"});
    let mut results = vec![
        purchase,
        t("payment", "CREDIT", -800.0, "05100000", "Pagamento recebido"),
        t("refund", "CREDIT", -10.0, "08000000", "Estorno"),
    ];
    if pending {
        let mut p = t("pending", "DEBIT", 20.0, "11010000", "Restaurante");
        p["status"] = json!("PENDING");
        results.push(p);
    }
    json!({"results": results, "next": format!("?accountId={account}&after=page%2B2")})
}

fn bill() -> Value {
    json!({"dueDate":"2099-11-25","closeDate":"2099-11-20","totalAmount": 123.45,"totalAmountCurrencyCode":"BRL"})
}

fn unused_database() -> tokio_postgres::Config {
    "host=127.0.0.1 port=1 user=none dbname=none"
        .parse()
        .unwrap()
}

async fn setup() -> (Running, Arc<Mock>, Running) {
    setup_with(unused_database(), None).await
}

async fn setup_with(
    database: tokio_postgres::Config,
    institutions: Option<std::path::PathBuf>,
) -> (Running, Arc<Mock>, Running) {
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: "test-id".into(),
        client_secret: "test-secret".into(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        bind: "127.0.0.1:3001".parse().unwrap(),
        database,
        institutions,
        zai_token: None,
        zai_base: "https://api.z.ai/api/paas/v4".into(),
        zai_model: "glm-5.3".into(),
        pairing_codes: [None, None],
        releases_dir: None,
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    (api, state, upstream)
}

#[tokio::test]
async fn token_requires_auth_and_binds_the_authenticated_user() {
    let (api, state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    let response = http
        .post(format!("{}/api/connect-token", api.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(state.auth.load(Ordering::SeqCst), 0);
    let response = http
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(TOKEN_2)
        .json(&json!({"clientUserId": "client-1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(state.auth.load(Ordering::SeqCst), 0);
    let response = http
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(TOKEN_1)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"accessToken": "test-connect-token"})
    );
    assert_eq!(
        state.options.lock().await[0],
        json!({"options": {"clientUserId": "client-1"}})
    );
}

#[tokio::test]
async fn clients_cannot_read_each_others_items_or_accounts() {
    let (api, state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    for path in [
        "/api/items/item-two/accounts",
        "/api/accounts/card-two",
        "/api/accounts/card-two/transactions",
        "/api/accounts/card-two/bills",
        "/api/items/unbound/accounts",
    ] {
        let response = http
            .get(format!("{}{path}", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    assert_eq!(state.transactions.load(Ordering::SeqCst), 0);
    let response = http
        .get(format!("{}/api/accounts/card-two", api.url))
        .bearer_auth(TOKEN_2)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value = http
        .get(format!("{}/api/accounts/card-one", api.url))
        .bearer_auth(TOKEN_1)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(value["creditData"]["futureField"], true);
    assert_eq!(value["balance"], 123.45);
}

#[tokio::test]
async fn transactions_use_cursor_and_bills_keep_page_pagination() {
    let (api, _state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    let value = http
        .get(format!("{}/api/accounts/card-one/transactions", api.url))
        .bearer_auth(TOKEN_1)
        .query(&[("after", "opaque+cursor/=")])
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(value["query"]["after"], "opaque+cursor/=");
    assert_eq!(value["query"]["accountId"], "card-one");
    assert_eq!(value["results"][0]["amount"], -123.45);
    assert_eq!(
        value["results"][0]["creditCardMetadata"]["installmentNumber"],
        2
    );
    assert!(value["next"].is_null());
    let value = http
        .get(format!(
            "{}/api/accounts/card-one/bills?page=2&pageSize=10",
            api.url
        ))
        .bearer_auth(TOKEN_1)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(value["page"], "2");
    assert_eq!(value["pageSize"], "10");
}

#[tokio::test]
async fn api_key_is_cached_and_refreshed_after_expiry_response() {
    let (api, state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    for _ in 0..2 {
        let response = http
            .get(format!("{}/api/accounts/card-one", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(state.auth.load(Ordering::SeqCst), 1);
    state.reject.store(true, Ordering::SeqCst);
    let response = http
        .get(format!("{}/api/accounts/card-one", api.url))
        .bearer_auth(TOKEN_1)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.auth.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn upstream_failures_never_echo_secrets() {
    let (api, state, _upstream) = setup().await;
    state.fail.store(true, Ordering::SeqCst);
    let response = reqwest::Client::new()
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(TOKEN_1)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = response.text().await.unwrap();
    assert!(!body.contains("test-secret"));
    assert!(!body.contains("do-not-expose"));
}

#[tokio::test]
async fn invalid_ids_pages_cursors_and_oversized_bodies_are_rejected() {
    let (api, state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    for path in [
        "/api/accounts/bad%2Fid",
        "/api/accounts/card-one/bills?page=0",
        "/api/accounts/card-one/bills?pageSize=501",
        "/api/accounts/card-one/transactions?after=%3FaccountId%3Dcard-one",
    ] {
        let response = http
            .get(format!("{}{path}", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }
    let response = http
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(TOKEN_1)
        .json(&json!({"clientUserId": "x".repeat(3000)}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(state.auth.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn local_server_reports_missing_credentials_without_using_fake_upstream_data() {
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: String::new(),
        client_secret: String::new(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        bind: "127.0.0.1:3001".parse().unwrap(),
        database: unused_database(),
        institutions: None,
        zai_token: None,
        zai_base: "https://api.z.ai/api/paas/v4".into(),
        zai_model: "glm-5.3".into(),
        pairing_codes: [None, None],
        releases_dir: None,
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    let http = reqwest::Client::new();
    let health = http
        .get(format!("{}/health", api.url))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(health["status"], "ok");
    assert_eq!(health["pluggyConfigured"], false);
    assert_eq!(health["assistant"], false, "sem ZAI_TOKEN, o assistente fica desligado");
    let response = http
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(TOKEN_1)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(state.auth.load(Ordering::SeqCst), 0);
    let page = http
        .get(format!("{}/connect", api.url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Conectar cartões"));
    assert!(!page.contains(TOKEN_1));
    assert!(!page.contains("test-secret"));
}

#[test]
fn local_setup_stays_on_loopback_and_production_still_requires_credentials() {
    let mut values = HashMap::from([
        ("APP_TOKEN_1", TOKEN_1),
        ("APP_TOKEN_2", TOKEN_2),
        ("BIND_ADDR", "127.0.0.1:3001"),
        (
            "DATABASE_URL",
            "host=127.0.0.1 port=5433 user=flounder dbname=flounder",
        ),
    ]);
    let lookup = |name: &str| values.get(name).map(|s| s.to_string());
    assert!(Config::read(lookup, false).is_err());
    assert!(!Config::read(lookup, true).unwrap().pluggy_configured());
    values.insert("BIND_ADDR", "0.0.0.0:3001");
    assert!(Config::read(|name| values.get(name).map(|s| s.to_string()), true).is_err());
    values.insert("CLIENT_ID", "test-id");
    values.insert("CLIENT_SECRET", "test-secret");
    assert!(
        Config::read(|name| values.get(name).map(|s| s.to_string()), false)
            .unwrap()
            .pluggy_configured()
    );
}

/// Cria um banco descartável no servidor de `TEST_DATABASE_URL`, aplica as migrações, roda o
/// teste e apaga o banco mesmo se o teste falhar. Sem a variável, o teste é ignorado com aviso.
async fn with_database<F, Fut>(test: F)
where
    F: FnOnce(tokio_postgres::Config, tokio_postgres::Client) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("AVISO: TEST_DATABASE_URL não definida; teste com PostgreSQL ignorado.");
        return;
    };
    let admin_config: tokio_postgres::Config = url.parse().expect("TEST_DATABASE_URL inválida");
    let admin = db::connect(&admin_config)
        .await
        .expect("PostgreSQL de teste indisponível");
    let mut random = [0u8; 8];
    getrandom::getrandom(&mut random).unwrap();
    let name = format!(
        "flounder_test_{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .await
        .unwrap();
    let mut config = admin_config.clone();
    config.dbname(&name);
    let outcome = tokio::spawn(async move {
        let client = db::connect(&config).await.unwrap();
        db::migrate(&client).await.unwrap();
        db::migrate(&client).await.unwrap();
        test(config, client).await
    })
    .await;
    admin
        .batch_execute(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .await
        .unwrap();
    if let Err(error) = outcome {
        std::panic::resume_unwind(error.into_panic());
    }
}

async fn api_state(api: &Running, token: &str) -> api::State {
    let response = reqwest::Client::new()
        .get(format!("{}/api/state", api.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

async fn count(db: &tokio_postgres::Client, table: &str) -> i64 {
    db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn card_import_persists_per_owner_replaces_on_reload_and_never_reassigns_items() {
    with_database(|database, db| async move {
        let directory = std::env::temp_dir().join(format!("flounder-import-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let institutions = directory.join("institutions.json");
        std::fs::write(&institutions, br#"{"item-one":"Nubank"}"#).unwrap();
        let (api, state, _upstream) = setup_with(database, Some(institutions)).await;
        let http = reqwest::Client::new();
        let load = |item: &'static str, token: &'static str| {
            let http = http.clone();
            let url = format!("{}/api/items/{item}/accounts", api.url);
            async move { http.get(url).bearer_auth(token).send().await.unwrap().status() }
        };
        assert_eq!(load("item-three", TOKEN_1).await, StatusCode::OK);
        state.extra_card.store(true, Ordering::SeqCst);
        assert_eq!(load("item-one", TOKEN_2).await, StatusCode::NOT_FOUND);
        assert_eq!(load("item-one", TOKEN_1).await, StatusCode::OK);
        assert_eq!(count(&db, "cards").await, 3);
        state.extra_card.store(false, Ordering::SeqCst);
        assert_eq!(load("item-one", TOKEN_1).await, StatusCode::OK);
        assert_eq!(load("item-one", TOKEN_1).await, StatusCode::OK);
        for table in ["cards", "card_invoices", "bank_connections"] {
            assert_eq!(count(&db, table).await, 2, "{table}");
        }
        assert_eq!(count(&db, "bank_accounts").await, 4);

        // O outro membro vê os cartões do domicílio, com o dono e todos os campos preservados.
        let cards = api_state(&api, TOKEN_2).await.cards;
        assert_eq!(cards.len(), 2);
        let mut expected = connected::ConnectedCard::from_pluggy(
            "item-one",
            &json!({"name": "Nubank"}),
            &account("card-item-one"),
            Some(&json!([bill()])),
        )
        .unwrap();
        expected.owner = 0;
        assert_eq!(
            serde_json::to_value(&cards[0]).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!((cards[0].institution.as_str(), cards[0].color), ("Nubank", [130, 10, 209]));
        assert_eq!(cards[0].bills[0].total, Some(12345));
        assert_eq!(cards[0].updated_at.as_deref(), Some("2026-11-10T12:34:56.789Z"));
        assert_eq!((cards[1].institution.as_str(), cards[1].color), ("Outro Banco", [18, 52, 86]));
        // O banco do institutions.json é gravado na conexão (banco escolhido), não no cartão.
        let keys: Vec<(String, Option<String>)> = db
            .query("SELECT external_id, institution_key FROM bank_connections ORDER BY external_id", &[])
            .await
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        assert_eq!(keys, [("item-one".into(), Some("nubank".into())), ("item-three".into(), None)]);

        // "Paga" é do app: sobrevive a recarregar; vencimento inexistente ou data inválida são recusados.
        let paid = |card: &'static str, due: &'static str| {
            let http = http.clone();
            let url = format!("{}/api/card-bills/paid", api.url);
            async move {
                http.put(url).bearer_auth(TOKEN_2).json(&json!({"card": card, "due": due, "paid": true}))
                    .send().await.unwrap().status()
            }
        };
        let due = cards[0].bills[0].due.clone();
        assert_eq!(paid("card-item-one", due.clone().leak()).await, StatusCode::NO_CONTENT);
        assert_eq!(paid("card-item-one", "2001-01-01").await, StatusCode::NOT_FOUND);
        assert_eq!(paid("card-item-one", "amanhã").await, StatusCode::BAD_REQUEST);
        assert_eq!(load("item-one", TOKEN_1).await, StatusCode::OK);
        let cards = api_state(&api, TOKEN_1).await.cards;
        assert!(cards[0].bills[0].paid && !cards[1].bills[0].paid);

        // Fatura de mês anterior que aparece pela primeira vez (cartão recém-conectado) já vem paga;
        // a futura não. Desmarcada pelo usuário, a reimportação não volta a marcar.
        state.old_bill.store(true, Ordering::SeqCst);
        assert_eq!(load("item-three", TOKEN_1).await, StatusCode::OK);
        let bills = |cards: Vec<connected::ConnectedCard>| -> Vec<(String, bool)> {
            cards.into_iter().find(|c| c.item_id == "item-three").unwrap().bills.into_iter().map(|b| (b.due, b.paid)).collect()
        };
        let got = bills(api_state(&api, TOKEN_1).await.cards);
        assert_eq!(got, [("2020-01-25".to_owned(), true), ("2099-11-25".to_owned(), false)]);
        db.execute("UPDATE card_invoices SET paid = false WHERE due_date = '2020-01-25'", &[]).await.unwrap();
        assert_eq!(load("item-three", TOKEN_1).await, StatusCode::OK);
        assert!(!bills(api_state(&api, TOKEN_1).await.cards)[0].1);
        state.old_bill.store(false, Ordering::SeqCst);
        assert_eq!(load("item-three", TOKEN_1).await, StatusCode::OK);

        // Item já gravado para outro membro: recusado, sem reatribuir nem gravar cartões.
        db.execute(
            "INSERT INTO bank_connections(household_id, owner_id, external_id)
             SELECT household_id, id, 'item-two' FROM members WHERE client_user_id = 'client-1'",
            &[],
        )
        .await
        .unwrap();
        assert_eq!(load("item-two", TOKEN_2).await, StatusCode::CONFLICT);
        let owner: String = db
            .query_one(
                "SELECT m.client_user_id FROM bank_connections c JOIN members m ON m.id = c.owner_id WHERE c.external_id = 'item-two'",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(owner, "client-1");
        assert_eq!(count(&db, "cards").await, 2);
        std::fs::remove_dir_all(directory).unwrap();
    })
    .await;
}

#[tokio::test]
async fn closing_day_flows_from_pluggy_and_the_manual_override_wins() {
    with_database(|database, db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);

        // Pluggy: fechamento no dia 20, vencimento no dia 25; a fatura carrega a data de fechamento.
        let cards = api_state(&api, TOKEN_1).await.cards;
        assert_eq!((cards[0].closing_day, cards[0].due_day), (Some(20), Some(25)));
        assert_eq!(cards[0].bills[0].closing.as_deref(), Some("2099-11-20"));

        // Ajuste manual vence o valor da Pluggy e sobrevive à reimportação.
        let put = |day: Value| {
            let http = http.clone();
            let url = format!("{}/api/cards/card-item-one/closing-day", api.url);
            async move { http.put(url).bearer_auth(TOKEN_1).json(&json!({"day": day})).send().await.unwrap() }
        };
        assert_eq!(put(json!(10)).await.status(), StatusCode::NO_CONTENT);
        let cards = api_state(&api, TOKEN_1).await.cards;
        assert_eq!((cards[0].closing_day, cards[0].due_day), (Some(10), Some(25)));
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);
        assert_eq!(api_state(&api, TOKEN_1).await.cards[0].closing_day, Some(10));

        // Sem ajuste, volta ao valor vindo da Pluggy.
        assert_eq!(put(json!(null)).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(api_state(&api, TOKEN_1).await.cards[0].closing_day, Some(20));

        // Dia inválido e cartão inexistente ou de outro domicílio: recusados.
        for day in [0, 32] {
            let response = put(json!(day)).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{day}");
            assert!(response.json::<Value>().await.unwrap()["error"].is_string());
        }
        assert_eq!(api_state(&api, TOKEN_1).await.cards[0].closing_day, Some(20));
        let response = http
            .put(format!("{}/api/cards/card-unknown/closing-day", api.url))
            .bearer_auth(TOKEN_1)
            .json(&json!({"day": 10}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Sem o dia da Pluggy e sem ajuste, o dia cai na fatura mais recente
        // (fechamento e vencimento dela).
        db.execute("UPDATE card_invoices SET closing_date = '2099-11-18'", &[]).await.unwrap();
        db.execute("UPDATE cards SET closing_day = NULL, due_day = NULL, due_date = NULL", &[]).await.unwrap();
        let cards = api_state(&api, TOKEN_1).await.cards;
        assert_eq!(cards[0].closing_day, Some(18), "fechamento vem da fatura mais recente");
        assert_eq!(cards[0].due_day, Some(25), "vencimento vem da fatura mais recente");
    })
    .await;
}

#[tokio::test]
async fn entries_round_trip_inside_the_household_and_reject_invalid_or_foreign_input() {
    with_database(|database, db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        let response = http.get(format!("{}/api/state", api.url)).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let state = api_state(&api, TOKEN_1).await;
        assert_eq!(state.me, 0);
        assert_eq!(
            state.members,
            [
                api::Member { position: 0, name: "Felipe".into(), client: "client-1".into() },
                api::Member { position: 1, name: "Larissa".into(), client: "client-2".into() },
            ]
        );
        assert!(state.entries.is_empty() && state.cards.is_empty() && state.reserve.is_none());
        assert_eq!(api_state(&api, TOKEN_2).await.me, 1);

        let body = json!({"description": "  Mercado  ", "amount_cents": 12345, "date": "2026-11-10", "kind": "expense",
            "category": "groceries", "shared": true, "owner": 1, "paid": false});
        let response = http.post(format!("{}/api/entries", api.url)).bearer_auth(TOKEN_1).json(&body).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let mut created: api::Entry = response.json().await.unwrap();
        assert_eq!(created.id.len(), 36);
        assert_eq!((created.description.as_str(), created.owner, created.amount_cents), ("Mercado", 1, 12345));
        assert_eq!(api_state(&api, TOKEN_2).await.entries, [created.clone()]);

        created.paid = true;
        created.owner = 0;
        created.date = "2026-12-01".into();
        let url = format!("{}/api/entries/{}", api.url, created.id);
        let response = http.put(&url).bearer_auth(TOKEN_2).json(&created).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json::<api::Entry>().await.unwrap(), created);
        assert_eq!(api_state(&api, TOKEN_1).await.entries, [created.clone()]);

        for (field, value) in [
            ("description", json!("   ")),
            ("description", json!("x".repeat(201))),
            ("amount_cents", json!(-1)),
            ("date", json!("2026-02-30")),
            ("date", json!("2026-11-10T00:00:00Z")),
            ("kind", json!("transfer")),
            ("category", json!("unknown")),
            ("owner", json!(2)),
        ] {
            let mut invalid = body.clone();
            invalid[field] = value;
            for request in [http.post(format!("{}/api/entries", api.url)), http.put(&url)] {
                let response = request.bearer_auth(TOKEN_1).json(&invalid).send().await.unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
                assert!(response.json::<Value>().await.unwrap()["error"].is_string());
            }
        }
        let mut income = body.clone();
        income["kind"] = json!("income");
        let response = http.post(format!("{}/api/entries", api.url)).bearer_auth(TOKEN_1).json(&income).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Lançamento de outro domicílio: invisível e intocável (404, sem revelar que existe).
        let foreign: String = db
            .query_one(
                "WITH h AS (INSERT INTO households(name) VALUES ('Outro') RETURNING id),
                      m AS (INSERT INTO members(household_id, client_user_id, display_name, position) SELECT id, 'client-x', 'Outro', 0 FROM h RETURNING id, household_id)
                 INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key)
                 SELECT household_id, id, 'Alheio', 100, '2026-11-10', 'expense', 'other' FROM m RETURNING id::text",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        for id in [foreign.as_str(), "not-a-uuid", "00000000-0000-4000-8000-000000000000"] {
            let url = format!("{}/api/entries/{id}", api.url);
            let response = http.put(&url).bearer_auth(TOKEN_1).json(&body).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{id}");
            let response = http.delete(&url).bearer_auth(TOKEN_1).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{id}");
        }
        assert_eq!(api_state(&api, TOKEN_1).await.entries.len(), 1);
        assert_eq!(count(&db, "entries").await, 2);

        db.execute(
            "INSERT INTO reserve_goals(household_id, owner_id, name, opening_balance_cents, target_cents)
             SELECT household_id, id, 'Reserva', 1000, 5000 FROM members WHERE client_user_id = 'client-1'",
            &[],
        )
        .await
        .unwrap();
        assert_eq!(
            api_state(&api, TOKEN_2).await.reserve,
            Some(api::Reserve { opening_cents: 1000, target_cents: 5000, name: "Reserva".into() })
        );

        let response = http.delete(&url).bearer_auth(TOKEN_2).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = http.delete(&url).bearer_auth(TOKEN_2).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(api_state(&api, TOKEN_1).await.entries.is_empty());
    })
    .await;
}

async fn send(request: reqwest::RequestBuilder, token: &str, body: Value) -> reqwest::Response {
    request.bearer_auth(token).json(&body).send().await.unwrap()
}

async fn load_item(api: &Running, item: &str, token: &str) -> StatusCode {
    reqwest::Client::new()
        .get(format!("{}/api/items/{item}/accounts", api.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .status()
}

async fn sync(api: &Running, token: &str) -> api::SyncResult {
    let response = reqwest::Client::new()
        .post(format!("{}/api/sync", api.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

/// Outro domicílio com dois membros, uma conexão, um cartão e um acerto; devolve o id do acerto.
async fn foreign_household(db: &tokio_postgres::Client) -> String {
    db.batch_execute(
        "WITH h AS (INSERT INTO households(name) VALUES ('Outro') RETURNING id)
         INSERT INTO members(household_id, client_user_id, display_name, position)
         SELECT id, 'client-x', 'X', 0 FROM h UNION ALL SELECT id, 'client-y', 'Y', 1 FROM h;
         WITH c AS (INSERT INTO bank_connections(household_id, owner_id, external_id)
                    SELECT household_id, id, 'item-foreign' FROM members WHERE client_user_id = 'client-x'
                    RETURNING id, household_id, owner_id),
              a AS (INSERT INTO bank_accounts(household_id, owner_id, connection_id, external_id, kind, name, currency)
                    SELECT household_id, owner_id, id, 'card-foreign', 'CREDIT', 'Alheio', 'BRL' FROM c
                    RETURNING id, household_id, owner_id)
         INSERT INTO cards(household_id, owner_id, account_id, name) SELECT household_id, owner_id, id, 'Alheio' FROM a;",
    )
    .await
    .unwrap();
    db.query_one(
        "INSERT INTO settlements(household_id, from_id, to_id, settled_on, amount_cents)
         SELECT x.household_id, x.id, y.id, '2026-11-10', 100 FROM members x, members y
         WHERE x.client_user_id = 'client-x' AND y.client_user_id = 'client-y' RETURNING id::text",
        &[],
    )
    .await
    .unwrap()
    .get(0)
}

#[tokio::test]
async fn imported_transactions_become_entries_once_and_keep_user_edits() {
    with_database(|database, db| async move {
        let (api, state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        state.pending.store(true, Ordering::SeqCst);
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);

        let snapshot = api_state(&api, TOKEN_2).await;
        let mut got: Vec<(String, String, String, i64, Option<String>)> = snapshot
            .entries
            .iter()
            .map(|e| (e.description.clone(), e.kind.clone(), e.category.clone(), e.amount_cents, e.card.clone()))
            .collect();
        got.sort();
        let card = Some("card-item-one".to_owned());
        assert_eq!(
            got,
            [
                ("Aplicação".into(), "expense".into(), "reserve".into(), 30000, None),
                ("Loja (2/3)".into(), "expense".into(), "shopping".into(), 10000, card.clone()),
                ("Mercado".into(), "expense".into(), "groceries".into(), 5050, None),
                ("Restaurante".into(), "expense".into(), "leisure".into(), 2000, card.clone()),
                ("Salário".into(), "income".into(), "salary".into(), 500000, None),
                ("Táxi".into(), "expense".into(), "transport".into(), 3000, card.clone()),
            ]
        );
        assert!(snapshot.entries.iter().all(|e| e.imported && e.paid && !e.shared && e.owner == 0));
        // Pagamento da fatura, transferência própria, estorno e a transação antiga não viram lançamento;
        // a antiga (fora dos 12 meses) nem é gravada.
        assert_eq!(count(&db, "bank_transactions").await, 10);
        assert_eq!(count(&db, "bank_transactions WHERE invoice_id IS NOT NULL").await, 1);
        assert_eq!(count(&db, "bank_transactions WHERE credit_card_metadata ? 'cardNumber'").await, 0);
        assert_eq!(snapshot.accounts.len(), 1);
        let account = &snapshot.accounts[0];
        assert_eq!((account.id.as_str(), account.owner, account.balance), ("bank-item-one", 0, Some(150025)));
        assert_eq!((account.name.as_str(), account.currency.as_str()), ("Conta", "BRL"));
        assert_eq!(snapshot.connections.len(), 1);
        assert_eq!(snapshot.connections[0].institution, "MeuPluggy");
        assert!(snapshot.connections[0].synced_at.as_ref().unwrap().ends_with('Z'));
        assert_eq!(snapshot.institutions.len(), 10);

        // O usuário edita a categoria e exclui um importado; a pendente some da Pluggy.
        let find = |name: &str| snapshot.entries.iter().find(|e| e.description == name).unwrap().clone();
        let mut market = find("Mercado");
        market.category = "leisure".into();
        let response = send(http.put(format!("{}/api/entries/{}", api.url, market.id)), TOKEN_1, json!(market)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json::<api::Entry>().await.unwrap(), market);
        let response = http
            .delete(format!("{}/api/entries/{}", api.url, find("Táxi").id))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        // Como antes da migração 004: tudo sugerido como "outros"; só quem ainda está com a
        // sugestão antiga é reclassificado na próxima importação.
        db.execute("UPDATE bank_transactions SET suggested_category = 'other' WHERE entry_id IS NOT NULL", &[]).await.unwrap();
        db.execute("UPDATE entries SET category_key = 'other' WHERE description = 'Loja (2/3)'", &[]).await.unwrap();
        state.pending.store(false, Ordering::SeqCst);
        assert_eq!(sync(&api, TOKEN_2).await, api::SyncResult { synced: 1, failed: 0 });
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);

        let entries = api_state(&api, TOKEN_1).await.entries;
        let mut names: Vec<&str> = entries.iter().map(|e| e.description.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Aplicação", "Loja (2/3)", "Mercado", "Salário"]);
        assert_eq!(entries.iter().find(|e| e.description == "Mercado").unwrap().category, "leisure");
        assert_eq!(entries.iter().find(|e| e.description == "Loja (2/3)").unwrap().category, "shopping");
        assert_eq!(entries.iter().find(|e| e.description == "Salário").unwrap().category, "salary");
        assert_eq!(count(&db, "bank_transactions").await, 9);

        // Lançamento manual: `imported` do corpo é ignorado; cartão só em despesa e do domicílio.
        let body = json!({"description": "Presente", "amount_cents": 100, "date": "2026-11-10", "kind": "expense",
            "category": "family", "shared": false, "owner": 0, "paid": false, "card": "card-item-one", "imported": true});
        let response = send(http.post(format!("{}/api/entries", api.url)), TOKEN_2, body.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let saved: api::Entry = response.json().await.unwrap();
        assert_eq!((saved.card.as_deref(), saved.imported), (Some("card-item-one"), false));
        foreign_household(&db).await;
        for (field, value) in [("card", json!("card-foreign")), ("card", json!("nope")), ("kind", json!("income"))] {
            let mut invalid = body.clone();
            invalid[field] = value;
            for request in [
                http.post(format!("{}/api/entries", api.url)),
                http.put(format!("{}/api/entries/{}", api.url, saved.id)),
            ] {
                assert_eq!(send(request, TOKEN_1, invalid.clone()).await.status(), StatusCode::BAD_REQUEST, "{field}");
            }
        }
    })
    .await;
}

#[tokio::test]
async fn sync_imports_both_members_items_and_counts_failures() {
    with_database(|database, db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);
        assert_eq!(load_item(&api, "item-two", TOKEN_2).await, StatusCode::OK);
        // Item que a Pluggy não encontra mais e conexão de outro domicílio (nunca sincronizada).
        db.execute(
            "INSERT INTO bank_connections(household_id, owner_id, external_id)
             SELECT household_id, id, 'item-gone' FROM members WHERE client_user_id = 'client-2'",
            &[],
        )
        .await
        .unwrap();
        foreign_household(&db).await;
        let entries = count(&db, "entries").await;
        let response = reqwest::Client::new().post(format!("{}/api/sync", api.url)).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (a, b) = tokio::join!(sync(&api, TOKEN_1), sync(&api, TOKEN_2));
        assert_eq!(a, api::SyncResult { synced: 2, failed: 1 });
        assert_eq!(a, b);
        assert_eq!(count(&db, "entries").await, entries);
        let owners: Vec<u8> = api_state(&api, TOKEN_1).await.entries.iter().map(|e| e.owner).collect();
        assert!(owners.contains(&0) && owners.contains(&1));
        let synced = api_state(&api, TOKEN_2).await.connections;
        assert_eq!(synced.len(), 3);
        assert!(synced.iter().find(|c| c.item_id == "item-gone").unwrap().synced_at.is_none());
    })
    .await;
}

#[tokio::test]
async fn settlements_reserve_members_and_institution_stay_in_the_household() {
    with_database(|database, db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        let foreign_settlement = foreign_household(&db).await;

        // Acertos.
        let body = json!({"date": "2026-11-10", "amount_cents": 5000, "from": 1, "to": 0});
        let response = send(http.post(format!("{}/api/settlements", api.url)), TOKEN_1, body.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let saved: api::Settlement = response.json().await.unwrap();
        assert_eq!(saved.id.len(), 36);
        assert_eq!(api_state(&api, TOKEN_2).await.settlements, [saved.clone()]);
        for (field, value) in [
            ("amount_cents", json!(0)),
            ("amount_cents", json!(-1)),
            ("to", json!(1)),
            ("to", json!(2)),
            ("date", json!("2026-13-01")),
        ] {
            let mut invalid = body.clone();
            invalid[field] = value;
            let response = send(http.post(format!("{}/api/settlements", api.url)), TOKEN_1, invalid).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
        }
        for id in [foreign_settlement.as_str(), "x", "00000000-0000-4000-8000-000000000000"] {
            let response = http.delete(format!("{}/api/settlements/{id}", api.url)).bearer_auth(TOKEN_1).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{id}");
        }
        let url = format!("{}/api/settlements/{}", api.url, saved.id);
        assert_eq!(http.delete(&url).bearer_auth(TOKEN_2).send().await.unwrap().status(), StatusCode::NO_CONTENT);
        assert_eq!(http.delete(&url).bearer_auth(TOKEN_2).send().await.unwrap().status(), StatusCode::NOT_FOUND);
        assert_eq!(count(&db, "settlements").await, 1);

        // Reserva: upsert da meta única; dono = quem salvou.
        let reserve = json!({"opening_cents": 1000, "target_cents": 5000, "name": " Viagem "});
        let response = send(http.put(format!("{}/api/reserve", api.url)), TOKEN_2, reserve).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let reserve = json!({"opening_cents": 0, "target_cents": 9000, "name": "Casa"});
        let response = send(http.put(format!("{}/api/reserve", api.url)), TOKEN_1, reserve).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            api_state(&api, TOKEN_2).await.reserve,
            Some(api::Reserve { opening_cents: 0, target_cents: 9000, name: "Casa".into() })
        );
        assert_eq!(count(&db, "reserve_goals").await, 1);
        let owner: String = db
            .query_one("SELECT m.client_user_id FROM reserve_goals r JOIN members m ON m.id = r.owner_id", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(owner, "client-1");
        for invalid in [
            json!({"opening_cents": -1, "target_cents": 0}),
            json!({"opening_cents": 0, "target_cents": -1}),
            json!({"opening_cents": 0, "target_cents": 0, "name": "x".repeat(61)}),
        ] {
            let response = send(http.put(format!("{}/api/reserve", api.url)), TOKEN_1, invalid).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        // Nome dos membros.
        let response = send(http.put(format!("{}/api/members/1", api.url)), TOKEN_1, json!({"name": "  Lari "})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(api_state(&api, TOKEN_2).await.members[1].name, "Lari");
        for name in ["   ", &"x".repeat(41), "a\nb"] {
            let response = send(http.put(format!("{}/api/members/0", api.url)), TOKEN_1, json!({"name": name})).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name}");
        }
        let response = send(http.put(format!("{}/api/members/2", api.url)), TOKEN_1, json!({"name": "Outro"})).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let names: Vec<String> = db
            .query("SELECT display_name FROM members ORDER BY client_user_id", &[])
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(names, ["Felipe", "Lari", "X", "Y"]);

        // Banco da conexão: vale na hora para cartões e contas, e sobrevive à reimportação.
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);
        let snapshot = api_state(&api, TOKEN_2).await;
        assert_eq!(snapshot.cards[0].institution, "platinum");
        assert_eq!(snapshot.connections[0].institution_key, None);
        let url = format!("{}/api/connections/item-one/institution", api.url);
        let response = send(http.put(&url), TOKEN_2, json!({"institution_key": "inter"})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(load_item(&api, "item-one", TOKEN_1).await, StatusCode::OK);
        let snapshot = api_state(&api, TOKEN_1).await;
        assert_eq!((snapshot.cards[0].institution.as_str(), snapshot.cards[0].color), ("Inter", [220, 83, 0]));
        assert_eq!(snapshot.accounts[0].institution, "Inter");
        assert_eq!(
            (snapshot.connections[0].institution.as_str(), snapshot.connections[0].institution_key.as_deref()),
            ("Inter", Some("inter"))
        );
        let response = send(http.put(&url), TOKEN_1, json!({"institution_key": "nope"})).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        for item in ["item-foreign", "item-unknown"] {
            let url = format!("{}/api/connections/{item}/institution", api.url);
            let response = send(http.put(&url), TOKEN_1, json!({"institution_key": "inter"})).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{item}");
        }
        let response = send(http.put(&url), TOKEN_1, json!({"institution_key": null})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(api_state(&api, TOKEN_1).await.cards[0].institution, "platinum");
        let foreign: Option<String> = db
            .query_one("SELECT institution_key FROM bank_connections WHERE external_id = 'item-foreign'", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(foreign, None);
    })
    .await;
}

// ───────────────────────── Ajustes, contas fixas e assistente ─────────────────────────

#[tokio::test]
async fn settings_are_household_wide_and_recurring_entries_survive_the_round_trip() {
    with_database(|database, _db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        // Conta fixa: grava e volta na leitura, para os dois membros.
        let body = json!({"description": "Aluguel", "amount_cents": 100000, "date": "2026-11-10", "kind": "expense",
            "category": "housing", "shared": true, "owner": 0, "paid": false, "recurring": true});
        let created: api::Entry = send(http.post(format!("{}/api/entries", api.url)), TOKEN_1, body.clone())
            .await
            .json()
            .await
            .unwrap();
        assert!(created.recurring);
        assert_eq!(api_state(&api, TOKEN_2).await.entries[0].recurring, true);
        // Editar para não recorrente também funciona.
        let mut edited = created.clone();
        edited.recurring = false;
        let response = send(http.put(format!("{}/api/entries/{}", api.url, edited.id)), TOKEN_2, serde_json::to_value(&edited).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.json::<api::Entry>().await.unwrap().recurring);

        // O interruptor do rateio vale para o domicílio inteiro, por qualquer membro.
        let response = send(http.put(format!("{}/api/settings", api.url)), TOKEN_1, json!({"shared_split_off": true})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(api_state(&api, TOKEN_2).await.shared_split_off);
        let response = send(http.put(format!("{}/api/settings", api.url)), TOKEN_2, json!({"shared_split_off": false})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(!api_state(&api, TOKEN_1).await.shared_split_off);
        let response = send(http.put(format!("{}/api/settings", api.url)), TOKEN_1, json!({"shared_split_off": true})).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    })
    .await;
}

/// Z.ai falso: devolve sempre a mesma resposta e conta as chamadas (para provar que nada é
/// regerado sem mudança nos dados).
async fn setup_with_zai(database: tokio_postgres::Config, calls: Arc<AtomicUsize>) -> (Running, Running, Running) {
    let zai = spawn(Router::new().fallback(move |headers: axum::http::HeaderMap, body: String| {
        let calls = calls.clone();
        async move {
            assert_eq!(
                headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()),
                Some("Bearer zai-test-token"),
                "o token do GLM vai só ao Z.ai"
            );
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "glm-5.3");
            assert!(body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "system"));
            calls.fetch_add(1, Ordering::SeqCst);
            Json(json!({"choices": [{"message": {"role": "assistant", "content": "Análise falsa 🐠 Tudo nadando bem!"}}]})).into_response()
        }
    }))
    .await;
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: "test-id".into(),
        client_secret: "test-secret".into(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        bind: "127.0.0.1:3001".parse().unwrap(),
        database,
        institutions: None,
        zai_token: Some("zai-test-token".into()),
        zai_base: zai.url.clone(),
        zai_model: "glm-5.3".into(),
        pairing_codes: [None, None],
        releases_dir: None,
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    (api, upstream, zai)
}

async fn reports_of(api: &Running, token: &str) -> api::AssistantReports {
    reqwest::Client::new()
        .get(format!("{}/api/assistant/reports", api.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Z.ai falso por roteiro: devolve as respostas na ordem (a última se repete) e guarda os
/// corpos dos pedidos, para provar o vai-e-vem das ferramentas entre o backend e o modelo.
async fn setup_with_zai_script(
    database: tokio_postgres::Config,
    script: Arc<Mutex<Vec<Value>>>,
    calls: Arc<Mutex<Vec<Value>>>,
) -> (Running, Running, Running) {
    let zai = spawn(Router::new().fallback(move |headers: axum::http::HeaderMap, body: String| {
        let script = script.clone();
        let calls = calls.clone();
        async move {
            assert_eq!(
                headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()),
                Some("Bearer zai-test-token"),
                "o token do GLM vai só ao Z.ai"
            );
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "glm-5.3");
            assert!(body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "system"));
            calls.lock().await.push(body);
            let mut script = script.lock().await;
            let next = if script.len() > 1 { script.remove(0) } else { script[0].clone() };
            Json(next).into_response()
        }
    }))
    .await;
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: "test-id".into(),
        client_secret: "test-secret".into(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        bind: "127.0.0.1:3001".parse().unwrap(),
        database,
        institutions: None,
        zai_token: Some("zai-test-token".into()),
        zai_base: zai.url.clone(),
        zai_model: "glm-5.3".into(),
        pairing_codes: [None, None],
        releases_dir: None,
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    (api, upstream, zai)
}

#[tokio::test]
async fn chat_with_tools_executes_actions_reports_ops_and_sends_results_back() {
    with_database(|database, _db| async move {
        let script = Arc::new(Mutex::new(vec![
            // 1ª resposta: o modelo pede a criação do lançamento.
            json!({"choices": [{"message": {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "criar_lancamento",
                    "arguments": r#"{"descricao":"Aluguel","valor_centavos":120000,"data":"2026-11-15","tipo":"expense","categoria":"housing","compartilhado":true}"#},
            }]}}]}),
            // 2ª resposta: o texto final, já sabendo o resultado da ferramenta.
            json!({"choices": [{"message": {"role": "assistant", "content": "Feito! Criei a despesa Aluguel (R$ 1.200,00) em 15/11, já do casal. 🐠"}}]}),
        ]));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (api, _upstream, _zai) = setup_with_zai_script(database, script, calls.clone()).await;
        let http = reqwest::Client::new();
        let reply: api::ChatReply = send(
            http.post(format!("{}/api/assistant/chat", api.url)),
            TOKEN_1,
            json!({"messages": [{"role": "user", "content": "Crie a despesa Aluguel de R$ 1.200,00 no dia 15, do casal."}]}),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(reply.content, "Feito! Criei a despesa Aluguel (R$ 1.200,00) em 15/11, já do casal. 🐠");
        assert_eq!(
            reply.ops,
            [api::Op { tool: "criar_lancamento".into(), label: "criou despesa Aluguel R$ 1.200,00 em 15/11".into() }]
        );

        // O lançamento está no estado, criado por quem conversou (dono = Felipe, posição 0).
        let entries = api_state(&api, TOKEN_1).await.entries;
        let [aluguel] = entries.as_slice() else {
            panic!("esperava um lançamento criado pela ferramenta, veio: {entries:?}");
        };
        assert_eq!(
            (aluguel.description.as_str(), aluguel.amount_cents, aluguel.shared, aluguel.owner, aluguel.date.as_str(), aluguel.paid),
            ("Aluguel", 120000, true, 0, "2026-11-15", false)
        );

        // O segundo pedido ao Z.ai carrega o pedido de ferramenta e o resultado dela (com o id).
        let calls = calls.lock().await;
        assert_eq!(calls.len(), 2, "uma rodada de ferramenta + a resposta final");
        assert_eq!(calls[0]["tool_choice"], "auto");
        assert!(
            calls[0]["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "criar_lancamento"),
            "o primeiro pedido apresenta as ferramentas"
        );
        let messages = calls[1]["messages"].as_array().unwrap();
        assert!(messages.iter().any(|m| m["role"] == "assistant" && m["tool_calls"][0]["id"] == "call-1"));
        let tool = messages.iter().find(|m| m["role"] == "tool").expect("o resultado da ferramenta volta ao modelo");
        assert_eq!(tool["tool_call_id"], "call-1");
        let resultado: Value = serde_json::from_str(tool["content"].as_str().unwrap()).unwrap();
        assert_eq!(resultado["criado"]["id"], aluguel.id);
        assert_eq!(resultado["criado"]["valor_centavos"], 120000);
    })
    .await;
}

/// A regra de privacidade na execução: `buscar_lancamentos` de um membro nunca devolve os
/// lançamentos individuais do outro — só os dele e os compartilhados do casal.
#[tokio::test]
async fn buscar_lancamentos_hides_the_partners_individual_entries() {
    with_database(|_database, db| async move {
        db.batch_execute(
            "INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key, shared)
             SELECT household_id, id, d.descricao, d.valor, '2026-11-10', 'expense', d.categoria, d.compartilhado
             FROM members, (VALUES ('Mercado do Felipe', 5000, false, 'groceries'), ('Aluguel', 100000, true, 'housing'),
                 ('Mercado da Larissa', 6000, false, 'groceries')) AS d(descricao, valor, compartilhado, categoria)
             WHERE client_user_id = CASE WHEN d.descricao = 'Mercado da Larissa' THEN 'client-2' ELSE 'client-1' END",
        )
        .await
        .unwrap();
        // Felipe (posição 0) vê os próprios e o compartilhado; nunca o mercado pessoal de Larissa.
        let resultado = tools::run(&db, User("client-1"), 0, "buscar_lancamentos", &json!({})).await.unwrap();
        let descricoes: Vec<&str> = resultado.result["lancamentos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["descricao"].as_str().unwrap())
            .collect();
        assert!(descricoes.contains(&"Mercado do Felipe") && descricoes.contains(&"Aluguel"), "{descricoes:?}");
        assert!(!descricoes.contains(&"Mercado da Larissa"), "{descricoes:?}");
        // Larissa vê o espelho: os dela e o compartilhado, com o filtro de texto aplicado.
        let resultado = tools::run(&db, User("client-2"), 1, "buscar_lancamentos", &json!({"texto": "Mercado"})).await.unwrap();
        let descricoes: Vec<&str> = resultado.result["lancamentos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["descricao"].as_str().unwrap())
            .collect();
        assert_eq!(descricoes, ["Mercado da Larissa"], "{descricoes:?}");
        // O resultado da busca traz o que a conversa precisa para editar e excluir.
        let resultado = tools::run(&db, User("client-1"), 0, "buscar_lancamentos", &json!({"categoria": "housing", "mes": "2026-11"})).await.unwrap();
        let lancamentos = resultado.result["lancamentos"].as_array().unwrap();
        let [aluguel] = lancamentos.as_slice() else {
            panic!("esperava só o Aluguel na categoria housing, veio: {lancamentos:?}");
        };
        assert_eq!(
            (
                aluguel["id"].is_string(),
                aluguel["data"].clone(),
                aluguel["valor_centavos"].clone(),
                aluguel["tipo"].clone(),
                aluguel["compartilhado"].clone(),
                aluguel["dono"].clone()
            ),
            (true, json!("2026-11-10"), json!(100000), json!("expense"), json!(true), json!("Felipe"))
        );
        assert_eq!(resultado.ops[0].tool, "buscar_lancamentos");
    })
    .await;
}

#[tokio::test]
async fn assistant_keeps_reports_private_generates_once_and_chats_with_the_mock_glm() {
    with_database(|database, _db| async move {
        let calls = Arc::new(AtomicUsize::new(0));
        let (api, _upstream, _zai) = setup_with_zai(database, calls.clone()).await;
        let http = reqwest::Client::new();
        let reports = reports_of(&api, TOKEN_1).await;
        assert!(reports.enabled && reports.reports.is_empty() && !reports.busy);

        // "Analisar agora": responde na hora e gera em segundo plano.
        let response = http
            .post(format!("{}/api/assistant/refresh", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let mut reports = None;
        for _ in 0..200 {
            let r = reports_of(&api, TOKEN_1).await;
            if r.reports.len() == 2 && !r.busy {
                reports = Some(r);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let reports = reports.expect("as análises de Felipe e do casal chegaram");
        let mut subjects: Vec<u8> = reports.reports.iter().map(|r| r.subject).collect();
        subjects.sort_unstable();
        assert_eq!(subjects, [0, 2], "Felipe vê a própria e a do casal, nunca a de Larissa");
        assert!(reports.reports.iter().all(|r| r.content.contains("Análise falsa")));
        let mut subjects: Vec<u8> = reports_of(&api, TOKEN_2).await.reports.iter().map(|r| r.subject).collect();
        subjects.sort_unstable();
        assert_eq!(subjects, [1, 2], "Larissa vê a própria e a do casal, nunca a de Felipe");
        let couple = reports.reports.iter().find(|r| r.subject == 2).unwrap();
        assert_eq!((couple.period_start.len(), couple.period_end.len()), (10, 10));
        assert!(couple.generated_at.contains('T') && couple.generated_at.ends_with('Z'), "geração em ISO UTC: {}", couple.generated_at);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "uma chamada ao GLM por análise");

        // Sem mudança nos dados, "Analisar de novo" reaproveita as três.
        let response = http
            .post(format!("{}/api/assistant/refresh", api.url))
            .bearer_auth(TOKEN_2)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        for _ in 0..200 {
            if calls.load(Ordering::SeqCst) == 3 && !reports_of(&api, TOKEN_1).await.busy {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3, "dados iguais: nenhuma chamada nova ao GLM");

        // Conversa: o contexto vai ao modelo e a resposta volta ao membro.
        let reply: api::ChatReply = send(
            http.post(format!("{}/api/assistant/chat", api.url)),
            TOKEN_1,
            json!({"messages": [
                {"role": "user", "content": "Como estou?"},
                {"role": "assistant", "content": "Bem!"},
                {"role": "user", "content": "E o casal?"}
            ]}),
        )
        .await
        .json()
        .await
        .unwrap();
        assert!(reply.content.contains("Análise falsa"));
        assert!(reply.ops.is_empty(), "conversa sem ferramentas não executa ação nenhuma");
        assert!(calls.load(Ordering::SeqCst) >= 4);
        // Histórico que não termina com mensagem do usuário: erro claro, sem chamar o GLM.
        let before = calls.load(Ordering::SeqCst);
        let response = send(
            http.post(format!("{}/api/assistant/chat", api.url)),
            TOKEN_1,
            json!({"messages": [{"role": "assistant", "content": "oi"}]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(calls.load(Ordering::SeqCst), before);
    })
    .await;
}

// ───────────────────────── Orçamentos, histórico e rotina do mês ─────────────────────────

/// Orçamentos: upsert pela categoria, leitura no estado (do domicílio, para os dois),
/// remoção com valor <= 0 e categoria inválida recusada.
#[tokio::test]
async fn budgets_upsert_remove_and_validate_by_category() {
    with_database(|database, _db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        let put = |token: &'static str, body: Value| {
            let http = http.clone();
            let url = format!("{}/api/budgets", api.url);
            async move { send(http.put(url), token, body).await }
        };
        assert_eq!(put(TOKEN_1, json!({"category": "groceries", "monthly_cents": 30000})).await.status(), StatusCode::NO_CONTENT);
        // Upsert: mesma categoria com outro limite substitui o anterior.
        assert_eq!(put(TOKEN_2, json!({"category": "groceries", "monthly_cents": 50000})).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(put(TOKEN_2, json!({"category": "transport", "monthly_cents": 100000})).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            api_state(&api, TOKEN_1).await.budgets,
            [
                api::Budget { category: "groceries".into(), monthly_cents: 50000 },
                api::Budget { category: "transport".into(), monthly_cents: 100000 },
            ]
        );
        // Valor <= 0 remove o orçamento da categoria.
        assert_eq!(put(TOKEN_1, json!({"category": "groceries", "monthly_cents": 0})).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(put(TOKEN_1, json!({"category": "transport", "monthly_cents": -1})).await.status(), StatusCode::NO_CONTENT);
        assert!(api_state(&api, TOKEN_2).await.budgets.is_empty());
        // Categoria inexistente no catálogo: recusada.
        let response = put(TOKEN_1, json!({"category": "feijoada", "monthly_cents": 500})).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response.json::<Value>().await.unwrap()["error"].is_string());
    })
    .await;
}

/// A conversa persiste por membro (histórico, com os ops, na ordem; apagar é só o meu) e
/// cada ação executada pelas ferramentas entra na auditoria do membro.
#[tokio::test]
async fn chat_persists_history_per_member_and_audits_actions() {
    with_database(|database, _db| async move {
        let script = Arc::new(Mutex::new(vec![
            // 1ª resposta: o modelo pede o orçamento do mercado.
            json!({"choices": [{"message": {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "definir_orcamento", "arguments": r#"{"categoria":"groceries","monthly_cents":30000}"#},
            }]}}]}),
            // 2ª resposta: o texto final.
            json!({"choices": [{"message": {"role": "assistant", "content": "Orçamento de Mercado definido em R$ 300,00 por mês. 🐠"}}]}),
        ]));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (api, _upstream, _zai) = setup_with_zai_script(database, script, calls.clone()).await;
        let http = reqwest::Client::new();
        let history_of = |token: &'static str| {
            let http = http.clone();
            let url = format!("{}/api/assistant/history", api.url);
            async move {
                http.get(&url).bearer_auth(token).send().await.unwrap().json::<api::ChatHistory>().await.unwrap()
            }
        };
        let reply: api::ChatReply = send(
            http.post(format!("{}/api/assistant/chat", api.url)),
            TOKEN_1,
            json!({"messages": [{"role": "user", "content": "Define o orçamento do mercado em 300 reais."}]}),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(reply.ops.len(), 1);

        // Histórico do membro: a pergunta e a resposta (com os ops), mais antigas primeiro.
        let history = history_of(TOKEN_1).await;
        assert_eq!(history.messages.len(), 2);
        assert_eq!(
            (history.messages[0].role.as_str(), history.messages[0].content.as_str()),
            ("user", "Define o orçamento do mercado em 300 reais.")
        );
        assert!(history.messages[0].ops.is_empty());
        assert_eq!(history.messages[1].role, "assistant");
        assert_eq!(history.messages[1].content, "Orçamento de Mercado definido em R$ 300,00 por mês. 🐠");
        assert_eq!(history.messages[1].ops, reply.ops);
        // A ferramenta executada valendo: o orçamento está no estado, para os dois.
        assert_eq!(
            api_state(&api, TOKEN_2).await.budgets,
            [api::Budget { category: "groceries".into(), monthly_cents: 30000 }]
        );

        // Larissa conversa em outro papo: histórico separado por membro.
        let _: api::ChatReply = send(
            http.post(format!("{}/api/assistant/chat", api.url)),
            TOKEN_2,
            json!({"messages": [{"role": "user", "content": "Oi!"}]}),
        )
        .await
        .json()
        .await
        .unwrap();
        let dela = history_of(TOKEN_2).await;
        assert_eq!(dela.messages.len(), 2);
        assert_eq!(dela.messages[0].content, "Oi!");
        assert_eq!(history_of(TOKEN_1).await.messages.len(), 2, "a conversa de Felipe não viu a de Larissa");

        // Auditoria: a ação da ferramenta de Felipe, mais recente primeiro e em ISO UTC.
        let actions: api::ActionLog = http
            .get(format!("{}/api/assistant/actions", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            actions.actions,
            [api::ActionRecord {
                tool: "definir_orcamento".into(),
                label: "definiu orçamento de groceries: R$ 300,00 por mês".into(),
                at: actions.actions[0].at.clone(),
            }]
        );
        assert!(actions.actions[0].at.contains('T') && actions.actions[0].at.ends_with('Z'), "{}", actions.actions[0].at);
        assert!(http.get(format!("{}/api/assistant/actions", api.url)).bearer_auth(TOKEN_2).send().await.unwrap().json::<api::ActionLog>().await.unwrap().actions.is_empty());

        // Limpar a conversa: só a de quem pede; a auditoria fica.
        assert_eq!(http.delete(format!("{}/api/assistant/history", api.url)).bearer_auth(TOKEN_1).send().await.unwrap().status(), StatusCode::NO_CONTENT);
        assert!(history_of(TOKEN_1).await.messages.is_empty());
        assert_eq!(history_of(TOKEN_2).await.messages.len(), 2);
        let actions: api::ActionLog = http
            .get(format!("{}/api/assistant/actions", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(actions.actions.len(), 1, "apagar a conversa não apaga a auditoria");
    })
    .await;
}

/// Rotina "fechar o mês": copia o mês anterior (pulando duplicados), lista as faturas do mês
/// (não pagas primeiro), sugere o acerto do casal, aponta a conta fixa fora da média e os
/// fechamentos dos cartões de quem pede; mês explícito é respeitado e cada cópia entra na
/// auditoria.
#[tokio::test]
async fn routine_month_copies_bills_settlement_and_alerts() {
    with_database(|database, db| async move {
        let (api, _state, _upstream) = setup_with(database, None).await;
        let http = reqwest::Client::new();
        // Meses de referência no fuso do app: atual, anterior e o anterior deste.
        let (mes, anterior, tras): (String, String, String) = {
            let row = db
                .query_one(
                    "SELECT to_char(date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo'), 'YYYY-MM'),
                            to_char((date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo') - interval '1 month'), 'YYYY-MM'),
                            to_char((date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo') - interval '2 months'), 'YYYY-MM')",
                    &[],
                )
                .await
                .unwrap();
            (row.get(0), row.get(1), row.get(2))
        };
        let hoje: String = db
            .query_one("SELECT to_char(now() AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD')", &[])
            .await
            .unwrap()
            .get(0);
        // Sementes: Luz em conta fixa (barata no mês retrás, cara no anterior → alerta),
        // aluguel compartilhado (gera saldo de acerto), lançamentos a copiar e a fatura do
        // cartão do Felipe no mês atual. Internet já existe no mês atual (duplicada a pular).
        db.batch_execute(&format!(
            "INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key, shared, paid, recurring)
             SELECT household_id, id, v.descricao, v.valor, v.dia::date, 'expense', v.categoria, v.compartilhada, true, v.fixa
             FROM members, (VALUES
                 ('Luz', 5000, '{tras}-05', 'utilities', false, true),
                 ('Aluguel', 100000, '{tras}-10', 'housing', true, true),
                 ('Streaming', 2990, '{tras}-15', 'leisure', false, false),
                 ('Luz', 19000, '{anterior}-05', 'utilities', false, true),
                 ('Internet', 9900, '{anterior}-08', 'services', false, true),
                 ('Academia', 5000, '{anterior}-20', 'health', false, false)
             ) AS v(descricao, valor, dia, categoria, compartilhada, fixa)
             WHERE client_user_id = CASE WHEN v.descricao = 'Academia' THEN 'client-2' ELSE 'client-1' END;
             INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key, paid)
             SELECT household_id, id, 'Internet', 9900, '{mes}-08'::date, 'expense', 'services', true
             FROM members WHERE client_user_id = 'client-1';
             WITH c AS (INSERT INTO bank_connections(household_id, owner_id, external_id)
                        SELECT household_id, id, 'item-rot' FROM members WHERE client_user_id = 'client-1'
                        RETURNING id, household_id, owner_id),
                  a AS (INSERT INTO bank_accounts(household_id, owner_id, connection_id, external_id, kind, name, currency)
                        SELECT household_id, owner_id, id, 'card-rot', 'CREDIT', 'Cartão', 'BRL' FROM c
                        RETURNING id, household_id, owner_id),
                  k AS (INSERT INTO cards(household_id, owner_id, account_id, name, closing_day, due_day)
                        SELECT household_id, owner_id, id, 'Nubank', 20, 25 FROM a
                        RETURNING id, household_id)
             INSERT INTO card_invoices(household_id, card_id, due_date, closing_date, currency, total_cents, source, paid)
             SELECT k.household_id, k.id, d.due::date, d.fech::date, 'BRL', d.total, 'pluggy', d.paga FROM k,
                    (VALUES ('{mes}-25', '{mes}-20', 84000, false), ('{mes}-10', '{mes}-05', 50000, true))
                        AS d(due, fech, total, paga);"
        ))
        .await
        .unwrap();

        // Primeira rodada: sem mês no corpo → fecha o mês atual.
        let report: Value = send(http.post(format!("{}/api/routine/month", api.url)), TOKEN_1, json!({"month": ""}))
            .await
            .json()
            .await
            .unwrap();
        let copiados = report["copiados"].as_array().unwrap();
        let mut nomes: Vec<&str> = copiados.iter().map(|c| c["descricao"].as_str().unwrap()).collect();
        nomes.sort();
        assert_eq!(nomes, ["Academia", "Luz"], "Internet já existia no mês e Streaming não é do mês anterior");
        assert!(copiados.iter().all(|c| c["pago"] == json!(false)), "{copiados:?}");
        // A cópia cai no mês atual, no mesmo dia e não paga, visível no estado.
        let entries = api_state(&api, TOKEN_1).await.entries;
        let luz = entries.iter().find(|e| e.description == "Luz" && e.date.starts_with(&mes)).unwrap();
        assert_eq!((luz.date.as_str(), luz.amount_cents, luz.paid, luz.recurring), (format!("{mes}-05").as_str(), 19000, false, true));
        let academia = entries.iter().find(|e| e.description == "Academia" && e.date.starts_with(&mes)).unwrap();
        assert_eq!((academia.owner, academia.paid), (1, false));
        // Faturas do mês: não pagas primeiro (a de 25 vem antes da paga de 10).
        assert_eq!(
            report["faturas"],
            json!([
                {"label": "Fatura Nubank", "due": format!("{mes}-25"), "total_cents": 84000, "paid": false},
                {"label": "Fatura Nubank", "due": format!("{mes}-10"), "total_cents": 50000, "paid": true},
            ])
        );
        // Acerto: metade do aluguel compartilhado do mês retrás — Bia deve a Felipe.
        assert_eq!(report["acerto_sugerido"], json!({"valor_cents": 50000, "de": 1}));
        // A Luz do mês anterior subiu 58% sobre a média das duas últimas.
        assert_eq!(report["alertas_preco"], json!([{"desc": "Luz", "recent_cents": 19000, "average_cents": 12000}]));
        // Fechamento do cartão de quem pediu, com os dias contados de hoje.
        let dias = flounder_core::model::days_to_closing(20, flounder_core::model::Date::iso(&hoje).unwrap());
        assert_eq!(report["fechamentos"], json!([{"nome": "Nubank", "dia_fechamento": 20, "dias": dias}]));

        // Segunda rodada: nada a copiar (as cópias já estão lá).
        let report: Value = send(http.post(format!("{}/api/routine/month", api.url)), TOKEN_1, json!({}))
            .await
            .json()
            .await
            .unwrap();
        assert!(report["copiados"].as_array().unwrap().is_empty(), "{}", report["copiados"]);

        // Mês inválido: erro claro.
        let response = send(http.post(format!("{}/api/routine/month", api.url)), TOKEN_1, json!({"month": "que"})).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Mês explícito: copia de {tras} para {anterior} (o parâmetro manda).
        let report: Value = send(http.post(format!("{}/api/routine/month", api.url)), TOKEN_1, json!({"month": anterior.clone()}))
            .await
            .json()
            .await
            .unwrap();
        let copiados = report["copiados"].as_array().unwrap();
        assert_eq!(copiados.len(), 3, "Luz (outro valor), Aluguel e Streaming: {}", copiados.len());
        let streaming = copiados.iter().find(|c| c["descricao"] == "Streaming").unwrap();
        assert_eq!(streaming["data"], json!(format!("{anterior}-15")));
        let aluguel = copiados.iter().find(|c| c["descricao"] == "Aluguel").unwrap();
        assert_eq!(aluguel["compartilhado"], json!(true));

        // Auditoria: uma ação por lançamento copiado, da mais recente para a mais antiga.
        let actions: api::ActionLog = http
            .get(format!("{}/api/assistant/actions", api.url))
            .bearer_auth(TOKEN_1)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(actions.actions.len(), 5, "2 cópias + 3 cópias da segunda rodada");
        assert!(actions.actions.iter().all(|a| a.tool == "copiar_mes_anterior"));
        assert!(actions.actions[0].label.contains("Streaming"), "{}", actions.actions[0].label);
        assert!(actions.actions[0].at.contains('T') && actions.actions[0].at.ends_with('Z'));
        // A auditoria é por membro: Larissa não vê as cópias rodadas por Felipe.
        assert!(http
            .get(format!("{}/api/assistant/actions", api.url))
            .bearer_auth(TOKEN_2)
            .send()
            .await
            .unwrap()
            .json::<api::ActionLog>()
            .await
            .unwrap()
            .actions
            .is_empty());
    })
    .await;
}

/// Ferramentas: criar/editar avisam quando o total do casal estoura o orçamento da categoria,
/// e `fechar_mes` devolve o relatório com um selo por cópia mais o resumo do mês.
#[tokio::test]
async fn tools_warn_when_the_budget_bursts_and_close_the_month() {
    with_database(|_database, db| async move {
        db::set_budget(&db, User("client-1"), &api::Budget { category: "groceries".into(), monthly_cents: 10000 })
            .await
            .unwrap();
        async fn criar(db: &tokio_postgres::Client, user: &'static str, me: u8, desc: &str, valor: i64, dia: &str) -> Result<tools::Outcome, String> {
            tools::run(db, User(user), me, "criar_lancamento", &json!({
                "descricao": desc, "valor_centavos": valor, "data": dia, "tipo": "expense", "categoria": "groceries",
            }))
            .await
        }
        // Dentro do limite: sem aviso.
        let resultado = criar(&db, "client-1", 0, "Feira", 8000, "2026-11-10").await.unwrap();
        assert!(resultado.result.get("orcamento").is_none(), "{}", resultado.result);
        let feira = resultado.result["criado"]["id"].as_str().unwrap().to_owned();
        // Estourou: o resultado traz o total do DOMICÍLIO contra o limite.
        let resultado = criar(&db, "client-1", 0, "Mercado", 5000, "2026-11-12").await.unwrap();
        assert_eq!(
            resultado.result["orcamento"],
            json!({"categoria": "groceries", "gasto_casal_cents": 13000, "limite_cents": 10000})
        );
        // O gasto do parceiro também conta no total do casal.
        let resultado = criar(&db, "client-2", 1, "Padaria", 3000, "2026-11-13").await.unwrap();
        assert_eq!(resultado.result["orcamento"]["gasto_casal_cents"], json!(16000));
        let padaria = resultado.result["criado"]["id"].as_str().unwrap().to_owned();
        // Editar para um valor que mantém o estouro segue avisando.
        let resultado = tools::run(&db, User("client-2"), 1, "editar_lancamento", &json!({"id": padaria, "valor_centavos": 6000}))
            .await
            .unwrap();
        assert_eq!(resultado.result["orcamento"]["gasto_casal_cents"], json!(19000));
        // Editar a Feira sem tirar o total do limite segue avisando (13.000 + 6.000 > 10.000).
        let resultado = tools::run(&db, User("client-1"), 0, "editar_lancamento", &json!({"id": feira, "valor_centavos": 8000}))
            .await
            .unwrap();
        assert!(resultado.result.get("orcamento").is_some(), "{}", resultado.result);

        // fechar_mes: semeia o mês anterior e roda sem mês explícito (mês atual).
        let (mes, anterior): (String, String) = {
            let row = db
                .query_one(
                    "SELECT to_char(date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo'), 'YYYY-MM'),
                            to_char((date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo') - interval '1 month'), 'YYYY-MM')",
                    &[],
                )
                .await
                .unwrap();
            (row.get(0), row.get(1))
        };
        db.batch_execute(&format!(
            "INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key, paid, recurring)
             SELECT household_id, id, v.d, v.v, ('{anterior}-' || v.dd)::date, 'expense', 'housing', true, true
             FROM members, (VALUES ('Internet', 9900, '08'), ('Luz', 19000, '05')) AS v(d, v, dd)
             WHERE client_user_id = 'client-1'"
        ))
        .await
        .unwrap();
        let resultado = tools::run(&db, User("client-1"), 0, "fechar_mes", &json!({})).await.unwrap();
        let copiados = resultado.result["copiados"].as_array().unwrap();
        assert_eq!(copiados.len(), 2, "{}", resultado.result);
        assert!(copiados.iter().all(|c| c["data"].as_str().unwrap().starts_with(&mes)), "as cópias caem no mês atual");
        assert!(copiados.iter().all(|c| c["pago"] == json!(false)));
        assert_eq!(resultado.ops.len(), 3, "um selo por cópia + o resumo");
        assert!(resultado.ops.iter().take(2).all(|op| op.tool == "copiar_mes_anterior"));
        assert_eq!(resultado.ops[2].tool, "fechar_mes");
        assert!(resultado.ops[2].label.contains("fechou o mês: 2 lançamento(s) copiado(s)"), "{}", resultado.ops[2].label);
        // O relatório completo está no resultado da ferramenta, com as chaves da rotina.
        for chave in ["copiados", "faturas", "acerto_sugerido", "alertas_preco", "fechamentos"] {
            assert!(resultado.result.get(chave).is_some(), "faltou {chave}");
        }
        // Segunda rodada: nada a copiar, só o resumo.
        let resultado = tools::run(&db, User("client-1"), 0, "fechar_mes", &json!({})).await.unwrap();
        assert_eq!(resultado.ops.len(), 1);
        assert!(resultado.ops[0].label.contains("0 lançamento(s) copiado(s)"), "{}", resultado.ops[0].label);
        // As cópias estão no estado do domicílio, não pagas, no mês atual (a auditoria fica
        // por conta de quem chama: a conversa e a rota registram; `tools::run` direto não).
        let copiadas: i64 = db
            .query_one(
                &format!(
                    "SELECT count(*) FROM entries e
                     WHERE e.household_id = (SELECT household_id FROM members WHERE client_user_id = 'client-1')
                         AND to_char(e.entry_date, 'YYYY-MM') = $1
                         AND e.paid = false AND e.description IN ('Internet', 'Luz')"
                ),
                &[&mes],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(copiadas, 2);
    })
    .await;
}

async fn setup_pairing(codes: [Option<&str>; 2]) -> (Running, Arc<Mock>, Running) {
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: "test-id".into(),
        client_secret: "test-secret".into(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        pairing_codes: codes.map(|c| c.map(str::to_owned)),
        releases_dir: None,
        bind: "127.0.0.1:3001".parse().unwrap(),
        database: unused_database(),
        institutions: None,
        zai_token: None,
        zai_base: "https://api.z.ai/api/paas/v4".into(),
        zai_model: "glm-5.3".into(),
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    (api, state, upstream)
}

#[tokio::test]
async fn pair_exchanges_the_fixed_code_for_a_working_member_token() {
    let (api, _state, _upstream) =
        setup_pairing([None, Some("MARIA-2026-FLOUNDER")]).await;
    let http = reqwest::Client::new();
    // Minúsculas, espaços e traços: a comparação normaliza.
    let value: Value = http
        .post(format!("{}/api/pair", api.url))
        .json(&json!({"code": "maria 2026 flounder"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(value["client"], "client-2");
    assert_eq!(value["token"], TOKEN_2, "devolve o token do membro do código");
    // O token devolvido autentica a API normalmente.
    let authorized = http
        .post(format!("{}/api/connect-token", api.url))
        .bearer_auth(value["token"].as_str().unwrap())
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    // Código errado: 401 claro.
    let wrong = http
        .post(format!("{}/api/pair", api.url))
        .json(&json!({"code": "outra-coisa"}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pair_throttles_repeated_wrong_codes() {
    let (api, _state, _upstream) =
        setup_pairing([Some("CODIGO-VALIDO-01"), None]).await;
    let http = reqwest::Client::new();
    let wrong = || async {
        http.post(format!("{}/api/pair", api.url))
            .json(&json!({"code": "errou"}))
            .send()
            .await
            .unwrap()
            .status()
    };
    // As 19 primeiras falhas respondem 401; a 20ª atinge o limite e já devolve 429.
    for _ in 0..19 {
        assert_eq!(wrong().await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(wrong().await, StatusCode::TOO_MANY_REQUESTS, "a falha que atinge o limite bloqueia");
    assert_eq!(wrong().await, StatusCode::TOO_MANY_REQUESTS);
    // Nem o código certo passa durante o bloqueio.
    let correct = http
        .post(format!("{}/api/pair", api.url))
        .json(&json!({"code": "codigo-valido-01"}))
        .send()
        .await
        .unwrap();
    assert_eq!(correct.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn pairing_codes_are_visible_to_members_and_flagged_on_health() {
    let (api, _state, _upstream) =
        setup_pairing([Some("CODIGO-DO-UM"), Some("CODIGO-DO-DOIS")]).await;
    let http = reqwest::Client::new();
    let health: Value = http
        .get(format!("{}/health", api.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["pairing"], true);
    let anonymous = http
        .get(format!("{}/api/pairing-code", api.url))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let codes: Value = http
        .get(format!("{}/api/pairing-code", api.url))
        .bearer_auth(TOKEN_1)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(codes["client-1"], "CODIGO-DO-UM");
    assert_eq!(codes["client-2"], "CODIGO-DO-DOIS");
}

#[test]
fn pairing_codes_must_be_long_distinct_and_different_from_tokens() {
    let base: &[(&str, &str)] = &[
        ("CLIENT_ID", "test-id"),
        ("CLIENT_SECRET", "test-secret"),
        ("APP_TOKEN_1", TOKEN_1),
        ("APP_TOKEN_2", TOKEN_2),
        ("DATABASE_URL", "host=127.0.0.1 port=1 user=none dbname=none"),
    ];
    let read = |overrides: &[(&str, &str)]| {
        Config::read(
            |name| {
                overrides
                    .iter()
                    .find(|(n, _)| *n == name)
                    .or_else(|| base.iter().find(|(n, _)| *n == name))
                    .map(|(_, v)| v.to_string())
            },
            false,
        )
    };
    let ok = read(&[("PAIRING_CODE_2", "MARIA-2026-FLOUNDER")]).unwrap();
    assert_eq!(ok.pairing_codes[1].as_deref(), Some("MARIA2026FLOUNDER"), "já vem normalizado");
    assert!(read(&[("PAIRING_CODE_2", "curta")]).is_err(), "menos de 8 caracteres");
    assert!(read(&[("PAIRING_CODE_1", "CODIGO-IGUAL"), ("PAIRING_CODE_2", "codigo-igual")]).is_err(), "iguais depois de normalizar");
    assert!(read(&[("PAIRING_CODE_2", TOKEN_2)]).is_err(), "não pode ser o próprio token");
    assert!(read(&[]).unwrap().pairing_codes.iter().all(Option::is_none), "sem variáveis, o pareamento fica desligado");
}

#[tokio::test]
async fn releases_publish_the_index_and_serve_the_packages() {
    let dir = std::env::temp_dir().join(format!("flounder-rel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Flounder-0.2.1-x86_64.AppImage"), b"APPIMAGE-NOVA").unwrap();
    std::fs::write(dir.join("flounder-0.2.0-windows-x86_64.exe"), b"velho").unwrap();
    let state = Arc::new(Mock::default());
    let upstream = spawn(Router::new().fallback(mock).with_state(state.clone())).await;
    let config = Config {
        client_id: "test-id".into(),
        client_secret: "test-secret".into(),
        tokens: [TOKEN_1.into(), TOKEN_2.into()],
        pairing_codes: [None, None],
        releases_dir: Some(dir.clone()),
        bind: "127.0.0.1:3001".parse().unwrap(),
        database: unused_database(),
        institutions: None,
        zai_token: None,
        zai_base: "https://api.z.ai/api/paas/v4".into(),
        zai_model: "glm-5.3".into(),
    };
    let api = spawn(build_router(config, &upstream.url, None).unwrap()).await;
    let http = reqwest::Client::new();
    // Índice no formato do GitHub, com a versão mais recente e o caminho relativo do ativo.
    let index: Value = http
        .get(format!("{}/releases/latest", api.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(index["tag_name"], "v0.2.1");
    let assets = index["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1, "só a versão mais recente aparece");
    assert_eq!(assets[0]["name"], "Flounder-0.2.1-x86_64.AppImage");
    assert_eq!(assets[0]["browser_download_url"], "/releases/Flounder-0.2.1-x86_64.AppImage");
    assert_eq!(assets[0]["size"], 13);
    // Download do pacote.
    let pacote = http
        .get(format!("{}/releases/Flounder-0.2.1-x86_64.AppImage", api.url))
        .send()
        .await
        .unwrap();
    assert_eq!(pacote.status(), StatusCode::OK);
    assert_eq!(pacote.bytes().await.unwrap().as_ref(), b"APPIMAGE-NOVA");
    // Truque de caminho não passa.
    let traversal = http
        .get(format!("{}/releases/..%2F.env", api.url))
        .send()
        .await
        .unwrap();
    assert!(traversal.status() == StatusCode::BAD_REQUEST || traversal.status() == StatusCode::NOT_FOUND);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn releases_without_a_directory_answer_that_there_is_nothing_published() {
    let (api, _state, _upstream) = setup().await;
    let http = reqwest::Client::new();
    let response = http.get(format!("{}/releases/latest", api.url)).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["error"], "Nenhuma versão publicada ainda.");
}
