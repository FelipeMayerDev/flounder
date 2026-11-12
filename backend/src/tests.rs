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
            json!({"results": [bill], "page": query["page"], "pageSize": query["pageSize"]})
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    Json(value).into_response()
}

fn account(id: &str) -> Value {
    json!({"id": id, "type": "CREDIT", "name":"platinum","marketingName":null,"number":"123456781234","currencyCode":"BRL","balance":123.45,
        "updatedAt":"2026-11-10T12:34:56.789Z","creditData": {"creditLimit": 1000.0,"availableCreditLimit":876.55,"brand":"MASTERCARD","balanceDueDate":"2026-11-25T00:00:00Z"}})
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
    json!({"dueDate":"2026-11-25","totalAmount": 123.45,"totalAmountCurrencyCode":"BRL"})
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
    assert_eq!(health, json!({"status": "ok", "pluggyConfigured": false}));
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
