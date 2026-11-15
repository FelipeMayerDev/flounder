use super::*;
use crate::connected::{ConnectedBill, ConnectedCard, cents, identify, iso_date};
use tokio::sync::{MappedMutexGuard, MutexGuard};
use tokio_postgres::{Client, NoTls, Row};

const MIGRATIONS: [(i32, &str); 8] = [
    (1, include_str!("../migrations/001_initial.sql")),
    (
        2,
        include_str!("../migrations/002_household_and_imported_cards.sql"),
    ),
    (3, include_str!("../migrations/003_sync_and_settlements.sql")),
    (4, include_str!("../migrations/004_more_categories.sql")),
    (5, include_str!("../migrations/005_past_invoices_paid.sql")),
    (
        6,
        include_str!("../migrations/006_assistant_and_split.sql"),
    ),
    (
        7,
        include_str!("../migrations/007_card_closing_day.sql"),
    ),
    (
        8,
        include_str!("../migrations/008_budgets_chat_actions.sql"),
    ),
];
pub(super) const HOUSEHOLD: &str = "(SELECT household_id FROM members WHERE client_user_id = $1)";
pub(super) const ENTRY: &str = "e.id::text, e.description, e.amount_cents, to_char(e.entry_date, 'YYYY-MM-DD'), \
    e.kind, e.category_key, e.shared, m.position, e.paid, \
    (SELECT a.external_id FROM cards c JOIN bank_accounts a ON a.id = c.account_id WHERE c.id = e.card_id), \
    EXISTS (SELECT 1 FROM bank_transactions t WHERE t.entry_id = e.id), \
    e.recurring";
const NO_CURRENCY: &str = "Moeda não informada";

pub(super) async fn connect(
    config: &tokio_postgres::Config,
) -> Result<Client, tokio_postgres::Error> {
    let (client, connection) = config.connect(NoTls).await?;
    tokio::spawn(connection);
    client
        .batch_execute("SET search_path TO finance, public")
        .await?;
    Ok(client)
}

/// Aplica, em ordem, as migrações embutidas que ainda não constam em `schema_migrations`.
pub(super) async fn migrate(client: &Client) -> Result<(), tokio_postgres::Error> {
    let exists: bool = client
        .query_one(
            "SELECT to_regclass('finance.schema_migrations') IS NOT NULL",
            &[],
        )
        .await?
        .get(0);
    let applied: Vec<i32> = if exists {
        client
            .query("SELECT version FROM finance.schema_migrations", &[])
            .await?
            .iter()
            .map(|row| row.get(0))
            .collect()
    } else {
        Vec::new()
    };
    for (version, sql) in MIGRATIONS {
        if !applied.contains(&version) {
            client.batch_execute(sql).await?;
        }
    }
    Ok(())
}

impl From<tokio_postgres::Error> for ApiError {
    fn from(error: tokio_postgres::Error) -> Self {
        // Só o SQLSTATE: a mensagem do PostgreSQL pode conter valores gravados.
        eprintln!(
            "Erro no PostgreSQL: {}",
            error.code().map_or("sem código", |code| code.code())
        );
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Não foi possível acessar o banco de dados.",
        )
    }
}

fn import_error(_: impl std::fmt::Display) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Dados consultados, mas não foi possível atualizar os cartões do aplicativo.",
    )
}

pub(super) fn not_member() -> ApiError {
    ApiError(
        StatusCode::FORBIDDEN,
        "Cliente sem membro cadastrado no banco de dados.",
    )
}

pub(super) fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Recurso não encontrado.")
}

pub(super) fn bad(message: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message)
}

// ponytail: código fora do ISO (ausente ou malformado) vira XXX ("sem moeda") e volta como
// "Moeda não informada"; guardar o texto original exigiria afrouxar o CHECK das colunas.
fn db_currency(currency: &str) -> &str {
    if currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase()) {
        currency
    } else {
        "XXX"
    }
}

fn app_currency(currency: String) -> String {
    if currency == "XXX" {
        NO_CURRENCY.into()
    } else {
        currency
    }
}

fn rgb(hex: &str) -> [u8; 3] {
    let channel = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
    };
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [48, 64, 88],
    }
}

fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

impl Backend {
    /// ponytail: uma conexão compartilhada atrás de um Mutex serializa as consultas; use um pool
    /// (deadpool-postgres) se houver mais que dois usuários simultâneos.
    pub(super) async fn db(&self) -> Result<MappedMutexGuard<'_, Client>, ApiError> {
        let mut client = self.db.lock().await;
        if client.as_ref().is_none_or(Client::is_closed) {
            *client = Some(connect(&self.config.database).await?);
        }
        Ok(MutexGuard::map(client, |c| c.as_mut().unwrap()))
    }

    fn institution_override(&self, item_id: &str) -> Result<Option<String>, ApiError> {
        let Some(path) = &self.config.institutions else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(
                serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&bytes)
                    .map_err(import_error)?
                    .remove(item_id),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(import_error(e)),
        }
    }

    /// Consulta o item na Pluggy (conferindo o dono) e grava, numa transação, a conexão, as contas
    /// (BANK e CREDIT), cartões, faturas e transações. Usado por `/connect` e por `POST /api/sync`.
    pub(super) async fn import_item(&self, user: User, item_id: &str) -> Result<Value, ApiError> {
        let item = self.pluggy.owned_item(item_id, user).await?;
        let accounts = self
            .pluggy
            .call(Method::GET, "/accounts", json!({"itemId": item_id}), None)
            .await?;
        let rows = accounts["results"]
            .as_array()
            .ok_or_else(|| upstream_error("accounts"))?;
        let mut fetched = Vec::new();
        for account in rows
            .iter()
            .filter(|a| a["type"] == "CREDIT" || a["type"] == "BANK")
        {
            let id = account["id"]
                .as_str()
                .ok_or_else(|| upstream_error("account id"))?;
            valid_id(id)?;
            let bills = if account["type"] == "CREDIT" {
                self.card_bills(id).await.ok()
            } else {
                None
            };
            fetched.push(Fetched {
                account,
                card: ConnectedCard::from_pluggy(item_id, &item["connector"], account, bills.as_ref()),
                bills,
                transactions: self.account_transactions(id).await?,
            });
        }
        self.store(user, item_id, &item, &fetched).await?;
        drop(fetched);
        Ok(accounts)
    }

    async fn store(
        &self,
        user: User,
        item_id: &str,
        item: &Value,
        fetched: &[Fetched<'_>],
    ) -> Result<(), ApiError> {
        // Banco informado em institutions.json (modo local) só preenche a conexão sem banco escolhido.
        let file_institution = self.institution_override(item_id)?;
        let mut db = self.db().await?;
        let tx = db.transaction().await?;
        let member = tx
            .query_opt(
                "SELECT household_id::text, id::text FROM members WHERE client_user_id = $1",
                &[&user.0],
            )
            .await?
            .ok_or_else(not_member)?;
        let (home, owner): (String, String) = (member.get(0), member.get(1));
        let connector_id = item["connector"]["id"]
            .as_i64()
            .and_then(|id| i32::try_from(id).ok());
        let connector_name = clean(&item["connector"]["name"], 120);
        let connection: String = tx
            .query_opt(
                "INSERT INTO bank_connections(household_id, owner_id, external_id, institution_key, connector_id, connector_name, status, synced_at)
                 VALUES ($1::text::uuid, $2::text::uuid, $3, (SELECT key FROM institutions WHERE lower(name) = lower($4)), $5, $6, $7, now())
                 ON CONFLICT (external_id) DO UPDATE SET
                     institution_key = COALESCE(bank_connections.institution_key, EXCLUDED.institution_key),
                     connector_id = EXCLUDED.connector_id, connector_name = EXCLUDED.connector_name,
                     status = EXCLUDED.status, synced_at = EXCLUDED.synced_at
                 WHERE bank_connections.owner_id = EXCLUDED.owner_id
                 RETURNING id::text",
                &[&home, &owner, &item_id, &file_institution, &connector_id, &connector_name, &item["status"].as_str()],
            )
            .await?
            .ok_or(ApiError(
                StatusCode::CONFLICT,
                "Esta conexão já pertence ao outro membro do domicílio.",
            ))?
            .get(0);

        // Contas que a Pluggy não devolve mais saem com cartões e faturas; as transações vão junto
        // (ON DELETE CASCADE) e os lançamentos ficam, sem o vínculo.
        // ponytail: lista vazia é tratada como falha momentânea e não apaga nada; senão as transações
        // sairiam e voltariam como lançamentos novos (duplicados). Conta encerrada de fato some
        // quando o item devolver ao menos outra conta.
        let ids: Vec<&str> = fetched.iter().filter_map(|f| f.account["id"].as_str()).collect();
        let stale = [
            "DELETE FROM card_invoices WHERE card_id IN (SELECT c.id FROM cards c JOIN bank_accounts a ON a.id = c.account_id
                 WHERE a.connection_id = $1::text::uuid AND a.external_id <> ALL($2::text[]))",
            "DELETE FROM cards c USING bank_accounts a
                 WHERE a.id = c.account_id AND a.connection_id = $1::text::uuid AND a.external_id <> ALL($2::text[])",
            "DELETE FROM bank_accounts WHERE connection_id = $1::text::uuid AND external_id <> ALL($2::text[])",
        ];
        if !ids.is_empty() {
            for sql in stale {
                tx.execute(sql, &[&connection, &ids]).await?;
            }
        }
        let cutoff: String = tx
            .query_one("SELECT to_char(current_date - 365, 'YYYY-MM-DD')", &[])
            .await?
            .get(0);

        for f in fetched {
            let external = f.account["id"].as_str().unwrap_or_default();
            let kind = if f.card.is_some() { "CREDIT" } else { "BANK" };
            let (name, currency, balance, updated_at, institution, color) = match &f.card {
                Some(card) => (
                    card.name.clone(),
                    card.currency.clone(),
                    card.balance,
                    card.updated_at.clone(),
                    card.institution.clone(),
                    card.color,
                ),
                None => {
                    let (institution, color) = identify(&item["connector"], f.account);
                    (
                        clean(&f.account["marketingName"], 120)
                            .or_else(|| clean(&f.account["name"], 120))
                            .unwrap_or_else(|| "Conta".into()),
                        f.account["currencyCode"].as_str().unwrap_or("").to_owned(),
                        cents(&f.account["balance"]),
                        clean(&f.account["updatedAt"], 40),
                        institution,
                        color,
                    )
                }
            };
            let currency = db_currency(&currency);
            let updated_at = updated_at.as_deref().filter(|s| iso_date(s).is_some());
            let (key, institution_name, color_hex) = institution_columns(&tx, &institution, color).await?;
            let account: String = tx
                .query_opt(
                    "INSERT INTO bank_accounts(household_id, owner_id, connection_id, external_id, kind, name, currency,
                         balance_cents, updated_at, institution_key, institution_name, color_hex)
                     VALUES ($1::text::uuid, $2::text::uuid, $3::text::uuid, $4, $5, $6, $7, $8, $9::text::timestamptz, $10, $11, $12)
                     ON CONFLICT (external_id) DO UPDATE SET name = EXCLUDED.name, currency = EXCLUDED.currency,
                         balance_cents = EXCLUDED.balance_cents, updated_at = EXCLUDED.updated_at,
                         institution_key = EXCLUDED.institution_key, institution_name = EXCLUDED.institution_name,
                         color_hex = EXCLUDED.color_hex
                     WHERE bank_accounts.connection_id = EXCLUDED.connection_id AND bank_accounts.kind = EXCLUDED.kind
                     RETURNING id::text",
                    &[
                        &home, &owner, &connection, &external, &kind, &name, &currency, &balance, &updated_at,
                        &key, &institution_name, &color_hex,
                    ],
                )
                .await?
                .ok_or(ApiError(
                    StatusCode::CONFLICT,
                    "Esta conta já está vinculada a outra conexão.",
                ))?
                .get(0);

            let mut card_id = None;
            if let Some(card) = &f.card {
                // Dia do mês de fechamento/vencimento da Pluggy; `manual_closing_day` (ajuste do
                // usuário) fica de fora do upsert e sobrevive às reimportações.
                let (closing_day, due_day) = (card.closing_day.map(|d| d as i16), card.due_day.map(|d| d as i16));
                let id: String = tx
                    .query_one(
                        "INSERT INTO cards(household_id, owner_id, account_id, name, institution_key, institution_name, color_hex,
                             last_four, brand, currency, credit_limit_cents, available_limit_cents, due_date, closing_day, due_day, bills_available)
                         VALUES ($1::text::uuid, $2::text::uuid, $3::text::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::text::date, $14, $15, $16)
                         ON CONFLICT (account_id) DO UPDATE SET name = EXCLUDED.name, institution_key = EXCLUDED.institution_key,
                             institution_name = EXCLUDED.institution_name, color_hex = EXCLUDED.color_hex,
                             last_four = EXCLUDED.last_four, brand = EXCLUDED.brand, currency = EXCLUDED.currency,
                             credit_limit_cents = EXCLUDED.credit_limit_cents, available_limit_cents = EXCLUDED.available_limit_cents,
                             due_date = EXCLUDED.due_date, closing_day = EXCLUDED.closing_day, due_day = EXCLUDED.due_day,
                             bills_available = EXCLUDED.bills_available
                         RETURNING id::text",
                        &[
                            &home, &owner, &account, &card.name, &key, &institution_name, &color_hex, &card.last_four,
                            &card.brand, &currency, &card.limit, &card.available, &card.due, &closing_day, &due_day,
                            &card.bills_available,
                        ],
                    )
                    .await?
                    .get(0);
                let (mut dues, mut currencies) = (Vec::new(), Vec::new());
                for bill in &card.bills {
                    let currency = db_currency(&bill.currency);
                    // Id da fatura na Pluggy: liga as compras pela `creditCardMetadata.billId`.
                    let bill_id = f
                        .bills
                        .as_ref()
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find(|b| b["dueDate"].as_str().and_then(iso_date) == Some(bill.due.as_str()))
                        .and_then(|b| b["id"].as_str());
                    tx.execute(
                        // Fatura nova de mês anterior (cartão recém-conectado) já foi paga fora do app;
                        // depois disso, "paga" é do usuário e a reimportação não mexe.
                        "INSERT INTO card_invoices(household_id, card_id, due_date, closing_date, currency, total_cents, source, external_id, paid)
                         VALUES ($1::text::uuid, $2::text::uuid, $3::text::date, $4::text::date, $5, $6, 'pluggy', $7,
                             $3::text::date < date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo')::date)
                         ON CONFLICT (card_id, due_date, currency) DO UPDATE SET total_cents = EXCLUDED.total_cents,
                             source = 'pluggy', external_id = EXCLUDED.external_id, closing_date = EXCLUDED.closing_date",
                        &[&home, &id, &bill.due, &bill.closing, &currency, &bill.total, &bill_id],
                    )
                    .await?;
                    dues.push(bill.due.as_str());
                    currencies.push(currency);
                }
                tx.execute(
                    "DELETE FROM card_invoices WHERE card_id = $1::text::uuid AND source = 'pluggy'
                     AND (to_char(due_date, 'YYYY-MM-DD'), currency) NOT IN (SELECT * FROM unnest($2::text[], $3::text[]))",
                    &[&id, &dues, &currencies],
                )
                .await?;
                card_id = Some(id);
            }

            let mut seen = Vec::new();
            for t in &f.transactions {
                let Some(id) = t["id"].as_str().filter(|id| valid_id(id).is_ok()) else {
                    continue;
                };
                seen.push(id);
                let Some(date) = t["date"]
                    .as_str()
                    .and_then(iso_date)
                    .filter(|date| *date >= cutoff.as_str())
                else {
                    continue;
                };
                let (amount, tx_currency) = match cents(&t["amountInAccountCurrency"]) {
                    Some(amount) => (amount, currency),
                    None => match cents(&t["amount"]) {
                        Some(amount) => (amount, db_currency(t["currencyCode"].as_str().unwrap_or(currency))),
                        None => continue,
                    },
                };
                let meta = &t["creditCardMetadata"];
                let bill = meta["billId"].as_str();
                let metadata = json!({
                    "installmentNumber": meta["installmentNumber"], "totalInstallments": meta["totalInstallments"],
                    "purchaseDate": meta["purchaseDate"], "billId": bill,
                })
                .to_string();
                let status = if t["status"] == "PENDING" { "PENDING" } else { "POSTED" };
                let description = describe(t);
                // ponytail: a linha é atualizada a cada importação, mas o lançamento não: mudança de
                // valor de PENDING → POSTED com o mesmo id não chega ao lançamento (pode ter sido editado).
                let Some(row) = tx
                    .query_opt(
                        "INSERT INTO bank_transactions(household_id, account_id, external_id, description, amount_cents,
                             currency, transaction_date, status, credit_card_metadata, invoice_id, updated_at)
                         VALUES ($1::text::uuid, $2::text::uuid, $3, $4, $5, $6, $7::text::date, $8, $9::text::jsonb,
                             (SELECT id FROM card_invoices WHERE external_id = $10 AND household_id = $1::text::uuid), now())
                         ON CONFLICT (external_id) DO UPDATE SET description = EXCLUDED.description,
                             amount_cents = EXCLUDED.amount_cents, currency = EXCLUDED.currency,
                             transaction_date = EXCLUDED.transaction_date, status = EXCLUDED.status,
                             credit_card_metadata = EXCLUDED.credit_card_metadata, invoice_id = EXCLUDED.invoice_id,
                             updated_at = now()
                         WHERE bank_transactions.account_id = EXCLUDED.account_id
                         RETURNING id::text, xmax = 0",
                        &[&home, &account, &id, &description, &amount, &tx_currency, &date, &status, &metadata, &bill],
                    )
                    .await?
                else {
                    continue;
                };
                let Some((entry_kind, category)) = classify(kind, t, amount) else {
                    continue;
                };
                // Lançamento só nasce na primeira vez que a transação aparece: depois é do usuário
                // (editado ou excluído, não volta). A categoria acompanha uma sugestão nova
                // enquanto o usuário não a tiver trocado.
                if !row.get::<_, bool>(1) {
                    tx.execute(
                        "WITH t AS (
                             UPDATE bank_transactions bt SET suggested_category = $2 FROM bank_transactions old
                             WHERE bt.id = $1::text::uuid AND old.id = bt.id
                                 AND old.suggested_category IS DISTINCT FROM $2
                             RETURNING old.suggested_category AS before, bt.entry_id)
                         UPDATE entries e SET category_key = $2 FROM t
                         WHERE e.id = t.entry_id AND e.category_key = t.before",
                        &[&row.get::<_, String>(0), &category],
                    )
                    .await?;
                    continue;
                }
                tx.execute(
                    "WITH e AS (
                         INSERT INTO entries(household_id, owner_id, description, amount_cents, currency, entry_date,
                             kind, category_key, shared, card_id, paid)
                         VALUES ($1::text::uuid, $2::text::uuid, $3, $4, $5, $6::text::date, $7, $8, false, $9::text::uuid, true)
                         RETURNING id)
                     UPDATE bank_transactions SET entry_id = (SELECT id FROM e), suggested_category = $8
                     WHERE id = $10::text::uuid",
                    &[
                        &home, &owner, &description, &amount.abs(), &tx_currency, &date, &entry_kind, &category,
                        &card_id, &row.get::<_, String>(0),
                    ],
                )
                .await?;
            }
            // ponytail: transação pendente que some da Pluggy (cancelada ou reemitida com outro id ao
            // ser lançada) leva o próprio lançamento, mesmo editado; evita contar a compra duas vezes.
            tx.execute(
                "WITH gone AS (DELETE FROM bank_transactions WHERE account_id = $1::text::uuid AND status = 'PENDING'
                     AND external_id <> ALL($2::text[]) RETURNING entry_id)
                 DELETE FROM entries WHERE id IN (SELECT entry_id FROM gone)",
                &[&account, &seen],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn card_bills(&self, id: &str) -> Result<Value, ApiError> {
        let mut bills = Vec::new();
        for page in 1..=100 {
            let value = self
                .pluggy
                .call(
                    Method::GET,
                    "/bills",
                    json!({"accountId": id,"page":page,"pageSize":500}),
                    None,
                )
                .await?;
            let rows = value["results"]
                .as_array()
                .ok_or_else(|| upstream_error("bills"))?;
            bills.extend(rows.iter().cloned());
            if page >= value["totalPages"].as_u64().unwrap_or(1) {
                return Ok(Value::Array(bills));
            }
        }
        Err(upstream_error("bill page limit"))
    }

    /// Todas as transações da conta pela API v2, seguindo o cursor `next`.
    async fn account_transactions(&self, id: &str) -> Result<Vec<Value>, ApiError> {
        let mut all = Vec::new();
        let mut query = json!({"accountId": id});
        for _ in 0..100 {
            let mut page = self
                .pluggy
                .call(Method::GET, "/v2/transactions", query.clone(), None)
                .await?;
            let Value::Array(rows) = page["results"].take() else {
                return Err(upstream_error("transactions"));
            };
            all.extend(rows);
            let Some(next) = page["next"].as_str().filter(|s| !s.is_empty()) else {
                return Ok(all);
            };
            // `next` é uma query string (?accountId=…&after=…); só o `after` decodificado é reenviado.
            let after = reqwest::Url::parse("http://next.invalid/")
                .and_then(|base| base.join(next))
                .ok()
                .and_then(|url| url.query_pairs().find(|(k, _)| k == "after").map(|(_, v)| v.into_owned()))
                .ok_or_else(|| upstream_error("cursor"))?;
            query["after"] = json!(after);
        }
        Err(upstream_error("transaction page limit"))
    }
}

/// Uma conta da Pluggy já consultada, pronta para gravar.
struct Fetched<'a> {
    account: &'a Value,
    /// Só para contas CREDIT.
    card: Option<ConnectedCard>,
    bills: Option<Value>,
    transactions: Vec<Value>,
}

/// Banco conhecido vira `institution_key`; desconhecido guarda nome e cor próprios.
async fn institution_columns(
    tx: &tokio_postgres::Transaction<'_>,
    name: &str,
    [r, g, b]: [u8; 3],
) -> Result<(Option<String>, Option<String>, Option<String>), ApiError> {
    let key: Option<String> = tx
        .query_opt("SELECT key FROM institutions WHERE lower(name) = lower($1)", &[&name])
        .await?
        .map(|row| row.get(0));
    Ok(match key {
        Some(key) => (Some(key), None, None),
        None => (None, Some(name.into()), Some(format!("{r:02x}{g:02x}{b:02x}"))),
    })
}

fn clean(value: &Value, max: usize) -> Option<String> {
    let text: String = value.as_str()?.chars().filter(|c| !c.is_control()).collect();
    let text: String = text.trim().chars().take(max).collect();
    (!text.is_empty()).then_some(text)
}

/// Descrição do lançamento importado (até 200 caracteres, o limite da edição), com a parcela.
fn describe(t: &Value) -> String {
    let mut text = clean(&t["description"], 185).unwrap_or_else(|| "Transação importada".into());
    let meta = &t["creditCardMetadata"];
    if let (Some(n), Some(total)) = (meta["installmentNumber"].as_u64(), meta["totalInstallments"].as_u64())
        && (2..1000).contains(&total)
        && n <= total
        && !text.contains(&format!("{n}/{total}"))
    {
        text += &format!(" ({n}/{total})");
    }
    text
}

/// Regras de importação: `Some((kind, categoria))` vira lançamento; `None` só registra a transação.
/// Categorias da Pluggy (`categoryId`): https://api.pluggy.ai/categories.
fn classify(account_kind: &str, t: &Value, amount: i64) -> Option<(&'static str, &'static str)> {
    let category = t["categoryId"].as_str().unwrap_or("");
    // `type` independe do sinal (que no cartão é invertido: compra positiva, pagamento negativo).
    let debit = match t["type"].as_str() {
        Some("DEBIT") => true,
        Some("CREDIT") => false,
        _ if account_kind == "CREDIT" => amount > 0,
        _ => amount < 0,
    };
    // Cartão: créditos são pagamento da fatura, estorno ou crédito contratado; não são renda.
    // ponytail: estorno não abate a compra original; o usuário edita ou exclui a compra.
    if account_kind == "CREDIT" && !debit {
        return None;
    }
    // Pagamento da fatura na conta corrente: a despesa já está nas compras do cartão. O Nubank
    // categoriza como transferência genérica (05000000) com "Pagamento de fatura" na descrição.
    let description = t["description"].as_str().unwrap_or("").to_lowercase();
    if category == "05100000" || (category.starts_with("05") && description.contains("fatura")) {
        return None;
    }
    // Transferência entre contas do mesmo titular.
    if category.starts_with("04") {
        return None;
    }
    // Investimentos: só a aplicação vira "reserve"; resgate, rendimento e margem são ignorados.
    // ponytail: resgates não abatem a reserva; o usuário registra a retirada se quiser.
    if category.starts_with("03") {
        return (debit && category != "03050000" && category != "03060000").then_some(("expense", "reserve"));
    }
    if !debit {
        // Empréstimo ou financiamento recebido não é renda.
        if category.starts_with("02") {
            return None;
        }
        return Some(("income", if category.starts_with("01") { "salary" } else { "other" }));
    }
    const EXPENSES: [(&str, &str); 23] = [
        ("17020", "utilities"),
        ("07010", "utilities"),
        ("17", "housing"),
        ("20020", "housing"),
        ("10", "groceries"),
        ("19", "transport"),
        ("20040", "transport"),
        ("18", "health"),
        ("20030", "health"),
        ("07030", "health"),
        ("11", "leisure"),
        ("07040", "leisure"),
        ("09", "leisure"),
        ("12", "leisure"),
        ("14", "leisure"),
        ("21", "leisure"),
        ("06020", "family"),
        ("08050", "family"),
        // Gerais por último: os prefixos acima, mais específicos, vencem.
        ("08030", "pets"),
        ("08060", "education"),
        ("07020", "education"),
        ("08", "shopping"),
        ("07", "services"),
    ];
    Some((
        "expense",
        EXPENSES
            .iter()
            .find(|(prefix, _)| category.starts_with(prefix))
            .map_or("other", |(_, key)| key),
    ))
}


pub(super) fn entry(row: &Row) -> api::Entry {
    api::Entry {
        id: row.get(0),
        description: row.get(1),
        amount_cents: row.get(2),
        date: row.get(3),
        kind: row.get(4),
        category: row.get(5),
        shared: row.get(6),
        owner: row.get::<_, i16>(7) as u8,
        paid: row.get(8),
        card: row.get(9),
        imported: row.get(10),
        recurring: row.get(11),
    }
}

pub(super) const UTC: &str = r#"'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'"#;

pub(super) async fn state(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<Json<api::State>, ApiError> {
    let db = state.db().await?;
    let members: Vec<api::Member> = db
        .query(
            &format!("SELECT position, display_name, client_user_id FROM members WHERE household_id = {HOUSEHOLD} ORDER BY position"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| api::Member {
            position: row.get::<_, i16>(0) as u8,
            name: row.get(1),
            client: row.get(2),
        })
        .collect();
    let me = members
        .iter()
        .find(|m| m.client == user.0)
        .ok_or_else(not_member)?
        .position;
    let entries = db
        .query(
            &format!("SELECT {ENTRY} FROM entries e JOIN members m ON m.id = e.owner_id WHERE e.household_id = {HOUSEHOLD} ORDER BY e.entry_date, e.id"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(entry)
        .collect();
    let reserve = db
        .query_opt(
            &format!("SELECT opening_balance_cents, target_cents, name FROM reserve_goals WHERE household_id = {HOUSEHOLD}"),
            &[&user.0],
        )
        .await?
        .map(|row| api::Reserve {
            opening_cents: row.get(0),
            target_cents: row.get(1),
            name: row.get(2),
        });
    let bills = db
        .query(
            &format!("SELECT card_id::text, to_char(due_date, 'YYYY-MM-DD'), total_cents, currency, paid,
                    to_char(closing_date, 'YYYY-MM-DD') FROM card_invoices
                 WHERE household_id = {HOUSEHOLD} AND source = 'pluggy' ORDER BY due_date, currency"),
            &[&user.0],
        )
        .await?;
    // Banco: o escolhido na conexão (ci) tem precedência sobre o detectado na importação (i/nome próprio).
    // Dia de fechamento: manual do usuário vence o da Pluggy; sem os dois, cai no dia da fatura
    // mais recente. Vencimento: coluna nova, depois `cards.due_date`, depois a fatura mais recente.
    let cards = db
        .query(
            &format!("SELECT c.id::text, a.external_id, cn.external_id,
                    COALESCE(ci.name, i.name, c.institution_name, 'Cartão conectado'),
                    COALESCE(ci.color_hex, i.color_hex, c.color_hex, '304058'),
                    c.name, c.last_four, c.brand, c.currency, a.balance_cents, c.credit_limit_cents, c.available_limit_cents,
                    COALESCE(c.manual_closing_day, c.closing_day,
                        (SELECT EXTRACT(DAY FROM inv.closing_date)::smallint FROM card_invoices inv
                         WHERE inv.card_id = c.id AND inv.closing_date IS NOT NULL
                         ORDER BY inv.due_date DESC LIMIT 1)),
                    COALESCE(c.due_day, EXTRACT(DAY FROM c.due_date)::smallint,
                        (SELECT EXTRACT(DAY FROM inv.due_date)::smallint FROM card_invoices inv
                         WHERE inv.card_id = c.id ORDER BY inv.due_date DESC LIMIT 1)),
                    to_char(c.due_date, 'YYYY-MM-DD'), to_char(a.updated_at AT TIME ZONE 'UTC', {UTC}),
                    c.bills_available, m.position
                 FROM cards c JOIN bank_accounts a ON a.id = c.account_id JOIN bank_connections cn ON cn.id = a.connection_id
                 JOIN members m ON m.id = c.owner_id LEFT JOIN institutions i ON i.key = c.institution_key
                 LEFT JOIN institutions ci ON ci.key = cn.institution_key
                 WHERE c.household_id = {HOUSEHOLD} ORDER BY m.position, cn.external_id, a.external_id"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| {
            let id: String = row.get(0);
            ConnectedCard {
                id: row.get(1),
                item_id: row.get(2),
                institution: row.get(3),
                color: rgb(row.get(4)),
                name: row.get(5),
                last_four: row.get(6),
                brand: row.get(7),
                currency: app_currency(row.get(8)),
                balance: row.get(9),
                limit: row.get(10),
                available: row.get(11),
                closing_day: row.get::<_, Option<i16>>(12).map(|d| d as u32),
                due_day: row.get::<_, Option<i16>>(13).map(|d| d as u32),
                due: row.get(14),
                updated_at: row.get(15),
                bills: bills
                    .iter()
                    .filter(|bill| bill.get::<_, &str>(0) == id)
                    .map(|bill| ConnectedBill {
                        due: bill.get(1),
                        closing: bill.get(5),
                        total: bill.get(2),
                        currency: app_currency(bill.get(3)),
                        paid: bill.get(4),
                    })
                    .collect(),
                bills_available: row.get(16),
                owner: row.get::<_, i16>(17) as u8,
            }
        })
        .collect();
    let accounts = db
        .query(
            &format!("SELECT a.external_id, cn.external_id, m.position, a.name,
                    COALESCE(ci.name, i.name, a.institution_name, 'Conta conectada'),
                    COALESCE(ci.color_hex, i.color_hex, a.color_hex, '304058'),
                    a.currency, a.balance_cents, to_char(a.updated_at AT TIME ZONE 'UTC', {UTC})
                 FROM bank_accounts a JOIN bank_connections cn ON cn.id = a.connection_id JOIN members m ON m.id = a.owner_id
                 LEFT JOIN institutions i ON i.key = a.institution_key LEFT JOIN institutions ci ON ci.key = cn.institution_key
                 WHERE a.household_id = {HOUSEHOLD} AND a.kind = 'BANK' ORDER BY m.position, cn.external_id, a.external_id"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| api::Account {
            id: row.get(0),
            item_id: row.get(1),
            owner: row.get::<_, i16>(2) as u8,
            name: row.get(3),
            institution: row.get(4),
            color: rgb(row.get(5)),
            currency: app_currency(row.get(6)),
            balance: row.get(7),
            updated_at: row.get(8),
        })
        .collect();
    let connections = db
        .query(
            &format!("SELECT cn.external_id, m.position, COALESCE(i.name, cn.connector_name, 'Instituição não informada'),
                    cn.institution_key, to_char(cn.synced_at AT TIME ZONE 'UTC', {UTC})
                 FROM bank_connections cn JOIN members m ON m.id = cn.owner_id LEFT JOIN institutions i ON i.key = cn.institution_key
                 WHERE cn.household_id = {HOUSEHOLD} ORDER BY m.position, cn.external_id"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| api::Connection {
            item_id: row.get(0),
            owner: row.get::<_, i16>(1) as u8,
            institution: row.get(2),
            institution_key: row.get(3),
            synced_at: row.get(4),
        })
        .collect();
    let institutions = db
        .query("SELECT key, name, color_hex FROM institutions ORDER BY name", &[])
        .await?
        .iter()
        .map(|row| api::Institution {
            key: row.get(0),
            name: row.get(1),
            color: rgb(row.get(2)),
        })
        .collect();
    let settlements = db
        .query(
            &format!("SELECT s.id::text, to_char(s.settled_on, 'YYYY-MM-DD'), s.amount_cents, f.position, t.position
                 FROM settlements s JOIN members f ON f.id = s.from_id JOIN members t ON t.id = s.to_id
                 WHERE s.household_id = {HOUSEHOLD} ORDER BY s.settled_on, s.id"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| api::Settlement {
            id: row.get(0),
            date: row.get(1),
            amount_cents: row.get(2),
            from: row.get::<_, i16>(3) as u8,
            to: row.get::<_, i16>(4) as u8,
        })
        .collect();
    let shared_split_off: bool = db
        .query_one(
            &format!("SELECT h.shared_split_off FROM households h WHERE h.id = {HOUSEHOLD}"),
            &[&user.0],
        )
        .await?
        .get(0);
    let budgets = db
        .query(
            &format!("SELECT category_key, monthly_cents FROM budgets WHERE household_id = {HOUSEHOLD} ORDER BY category_key"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|row| api::Budget { category: row.get(0), monthly_cents: row.get(1) })
        .collect();
    Ok(Json(api::State {
        me,
        members,
        entries,
        reserve,
        cards,
        accounts,
        connections,
        institutions,
        settlements,
        shared_split_off,
        budgets,
    }))
}

/// Valida o corpo na fronteira de confiança; categoria, dono e cartão são conferidos no banco.
/// Devolve o id interno do cartão. `imported` do corpo é ignorado (vem do vínculo com a transação).
async fn validate(
    db: &Client,
    user: User,
    entry: &mut api::Entry,
) -> Result<Option<String>, ApiError> {
    entry.description = entry.description.trim().to_owned();
    if entry.description.is_empty()
        || entry.description.chars().count() > 200
        || entry.description.chars().any(char::is_control)
    {
        return Err(bad("Informe uma descrição de até 200 caracteres."));
    }
    if entry.amount_cents < 0 {
        return Err(bad("O valor não pode ser negativo."));
    }
    if iso_date(&entry.date) != Some(entry.date.as_str()) {
        return Err(bad("Use a data no formato AAAA-MM-DD."));
    }
    if !["income", "expense"].contains(&entry.kind.as_str()) {
        return Err(bad("O tipo deve ser income ou expense."));
    }
    if entry.kind == "income" && entry.shared {
        return Err(bad("Só despesas podem ser compartilhadas."));
    }
    if entry.kind == "income" && entry.card.is_some() {
        return Err(bad("Só despesas podem ser do cartão."));
    }
    let row = db
        .query_one(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM categories WHERE key = $2),
                 EXISTS (SELECT 1 FROM members WHERE household_id = {HOUSEHOLD} AND position = $3),
                 (SELECT c.id::text FROM cards c JOIN bank_accounts a ON a.id = c.account_id
                  WHERE a.external_id = $4 AND c.household_id = {HOUSEHOLD})"
            ),
            &[&user.0, &entry.category, &i16::from(entry.owner), &entry.card],
        )
        .await?;
    if !row.get::<_, bool>(0) {
        return Err(bad("Categoria inexistente."));
    }
    if !row.get::<_, bool>(1) {
        return Err(bad("O dono deve ser um membro do domicílio."));
    }
    let card: Option<String> = row.get(2);
    if entry.card.is_some() && card.is_none() {
        return Err(bad("Cartão inexistente no domicílio."));
    }
    Ok(card)
}

/// Grava um lançamento novo (validado como `validate()` manda). Núcleo de `POST /api/entries`,
/// reusado pelas ferramentas do assistente.
pub(super) async fn save_entry(
    db: &Client,
    user: User,
    input: &mut api::Entry,
) -> Result<api::Entry, ApiError> {
    let card = validate(db, user, input).await?;
    let row = db
        .query_one(
            &format!("WITH saved AS (
                     INSERT INTO entries(household_id, owner_id, description, amount_cents, entry_date, kind, category_key, shared, paid, card_id, recurring)
                     SELECT household_id, id, $3, $4, $5::text::date, $6, $7, $8, $9, $10::text::uuid, $11
                     FROM members WHERE household_id = {HOUSEHOLD} AND position = $2
                     RETURNING *)
                 SELECT {ENTRY} FROM saved e JOIN members m ON m.id = e.owner_id"),
            &[
                &user.0, &i16::from(input.owner), &input.description, &input.amount_cents,
                &input.date, &input.kind, &input.category, &input.shared, &input.paid, &card, &input.recurring,
            ],
        )
        .await?;
    Ok(entry(&row))
}

/// Grava a edição completa de um lançamento (mesma validação da criação). Núcleo de
/// `PUT /api/entries/{id}`, reusado pelas ferramentas do assistente.
pub(super) async fn save_edited_entry(
    db: &Client,
    user: User,
    id: &str,
    input: &mut api::Entry,
) -> Result<api::Entry, ApiError> {
    if !is_uuid(id) {
        return Err(not_found());
    }
    let card = validate(db, user, input).await?;
    let row = db
        .query_opt(
            &format!("WITH saved AS (
                     UPDATE entries e SET owner_id = m.id, description = $4, amount_cents = $5, entry_date = $6::text::date,
                         kind = $7, category_key = $8, shared = $9, paid = $10, card_id = $11::text::uuid, recurring = $12
                     FROM members m
                     WHERE e.id = $2::text::uuid AND e.household_id = {HOUSEHOLD}
                         AND m.household_id = e.household_id AND m.position = $3
                     RETURNING e.*)
                 SELECT {ENTRY} FROM saved e JOIN members m ON m.id = e.owner_id"),
            &[
                &user.0, &id, &i16::from(input.owner), &input.description, &input.amount_cents,
                &input.date, &input.kind, &input.category, &input.shared, &input.paid, &card, &input.recurring,
            ],
        )
        .await?
        .ok_or_else(not_found)?;
    Ok(entry(&row))
}

/// Exclui o lançamento do domicílio; 404 se não existir. Núcleo de `DELETE /api/entries/{id}`.
pub(super) async fn remove_entry(db: &Client, user: User, id: &str) -> Result<(), ApiError> {
    if !is_uuid(id) {
        return Err(not_found());
    }
    let deleted = db
        .execute(
            &format!(
                "DELETE FROM entries WHERE id = $2::text::uuid AND household_id = {HOUSEHOLD}"
            ),
            &[&user.0, &id],
        )
        .await?;
    if deleted == 0 {
        return Err(not_found());
    }
    Ok(())
}

/// Marca ou desmarca a fatura importada de um cartão no vencimento dado. Núcleo de
/// `PUT /api/card-bills/paid`.
pub(super) async fn mark_bill_paid(db: &Client, user: User, input: &api::BillPaid) -> Result<(), ApiError> {
    if iso_date(&input.due) != Some(input.due.as_str()) {
        return Err(bad("Use uma data AAAA-MM-DD."));
    }
    let updated = db
        .execute(
            &format!(
                "UPDATE card_invoices ci SET paid = $4 FROM cards c JOIN bank_accounts a ON a.id = c.account_id
                 WHERE ci.card_id = c.id AND a.external_id = $2 AND ci.due_date = $3::text::date
                     AND ci.household_id = {HOUSEHOLD} AND c.household_id = ci.household_id"
            ),
            &[&user.0, &input.card, &input.due, &input.paid],
        )
        .await?;
    if updated == 0 {
        return Err(not_found());
    }
    Ok(())
}

pub(super) async fn create_entry(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(mut input): Json<api::Entry>,
) -> Result<Json<api::Entry>, ApiError> {
    let db = state.db().await?;
    Ok(Json(save_entry(&db, user, &mut input).await?))
}

pub(super) async fn update_entry(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(mut input): Json<api::Entry>,
) -> Result<Json<api::Entry>, ApiError> {
    if !is_uuid(&id) {
        return Err(not_found());
    }
    let db = state.db().await?;
    Ok(Json(save_edited_entry(&db, user, &id, &mut input).await?))
}

/// Excluir um lançamento importado mantém a transação sem vínculo (FK ON DELETE SET NULL), então
/// ela não volta nas próximas importações.
pub(super) async fn delete_entry(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !is_uuid(&id) {
        return Err(not_found());
    }
    let db = state.db().await?;
    remove_entry(&db, user, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn set_bill_paid(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::BillPaid>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    mark_bill_paid(&db, user, &input).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ajuste manual do dia do fechamento do cartão (PUT /api/cards/{account_id}/closing-day): vence o
/// dia vindo da Pluggy na leitura e sobrevive às reimportações; `None` limpa o ajuste. Núcleo
/// reusado pelas ferramentas do assistente.
pub(super) async fn set_closing_day(db: &Client, user: User, card_id: &str, day: Option<u32>) -> Result<(), ApiError> {
    valid_id(card_id)?;
    if day.is_some_and(|day| !(1..=31).contains(&day)) {
        return Err(bad("Informe um dia de fechamento entre 1 e 31."));
    }
    let manual = day.map(|day| day as i16);
    let updated = db
        .execute(
            &format!(
                "UPDATE cards c SET manual_closing_day = $3 FROM bank_accounts a
                 WHERE a.id = c.account_id AND a.external_id = $2 AND c.household_id = {HOUSEHOLD}"
            ),
            &[&user.0, &card_id, &manual],
        )
        .await?;
    if updated == 0 {
        return Err(not_found());
    }
    Ok(())
}

pub(super) async fn set_card_closing_day(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(card_id): Path<String>,
    Json(input): Json<api::ClosingDay>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    set_closing_day(&db, user, &card_id, input.day).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reimporta todas as conexões do domicílio (dos dois membros), uma por vez. A falha de uma
/// conexão é contada e não interrompe as outras.
pub(super) async fn sync(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<Json<api::SyncResult>, ApiError> {
    Ok(Json(sync_connections(&state, Some(&user)).await?))
}

/// Importa todas as conexões do domicílio de `user` — ou, com `None`, de todos os domicílios
/// (usado pela sincronização automática em segundo plano).
async fn sync_connections(state: &Backend, user: Option<&User>) -> Result<api::SyncResult, ApiError> {
    // ponytail: uma sincronização por vez no processo todo; suficiente para um casal.
    let _running = state.sync.lock().await;
    state.pluggy.api_key().await?;
    let db = state.db().await?;
    let rows = match user {
        Some(user) => {
            db.query(
                &format!("SELECT c.external_id, m.client_user_id FROM bank_connections c JOIN members m ON m.id = c.owner_id
                     WHERE c.household_id = {HOUSEHOLD} ORDER BY m.position, c.external_id"),
                &[&user.0],
            )
            .await?
        }
        None => {
            db.query(
                "SELECT c.external_id, m.client_user_id FROM bank_connections c JOIN members m ON m.id = c.owner_id
                 ORDER BY m.position, c.external_id",
                &[],
            )
            .await?
        }
    };
    let items: Vec<(String, String)> = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
    drop(db);
    let mut result = api::SyncResult::default();
    for (item, client) in items {
        // O item é consultado como o seu dono: a Pluggy confere o clientUserId.
        let owner = match client.as_str() {
            "client-1" => User("client-1"),
            "client-2" => User("client-2"),
            _ => {
                result.failed += 1;
                continue;
            }
        };
        match state.import_item(owner, &item).await {
            Ok(_) => result.synced += 1,
            Err(error) => {
                eprintln!("Sincronização: uma conexão falhou ({}).", error.0);
                result.failed += 1;
            }
        }
    }
    Ok(result)
}

/// Em segundo plano: reimporta todas as conexões periodicamente, para as compras novas
/// chegarem sozinhas (o desktop também sincroniza ao abrir e pelo botão "Atualizar contas").
/// Falhas são só registradas no log; a próxima rodada tenta de novo.
pub(super) fn spawn_sync_loop(state: Arc<Backend>) {
    if !state.config.pluggy_configured() {
        return;
    }
    tokio::spawn(async move {
        let mut first = true;
        loop {
            // Primeira passada 5 minutos depois do boot; depois a cada 6 horas.
            tokio::time::sleep(if first { Duration::from_secs(5 * 60) } else { Duration::from_secs(6 * 3600) }).await;
            first = false;
            match sync_connections(&state, None).await {
                Ok(result) if result.failed == 0 => {
                    println!("Sincronização automática: {} conexão(ões) atualizada(s).", result.synced)
                }
                Ok(result) => eprintln!("Sincronização automática: {} ok, {} com falha.", result.synced, result.failed),
                Err(error) => eprintln!("Sincronização automática adiada ({}).", error.1),
            }
        }
    });
}

/// Registra o acerto entre os membros das posições `from` → `to`. Núcleo de
/// `POST /api/settlements`, reusado pelas ferramentas do assistente.
pub(super) async fn save_settlement(db: &Client, user: User, input: api::Settlement) -> Result<api::Settlement, ApiError> {
    if iso_date(&input.date) != Some(input.date.as_str()) {
        return Err(bad("Use a data no formato AAAA-MM-DD."));
    }
    if input.amount_cents <= 0 {
        return Err(bad("O valor do acerto deve ser maior que zero."));
    }
    if input.from == input.to {
        return Err(bad("O acerto deve ser entre membros diferentes."));
    }
    let id: String = db
        .query_opt(
            &format!("INSERT INTO settlements(household_id, from_id, to_id, settled_on, amount_cents)
                 SELECT f.household_id, f.id, t.id, $4::text::date, $5
                 FROM members f JOIN members t ON t.household_id = f.household_id
                 WHERE f.household_id = {HOUSEHOLD} AND f.position = $2 AND t.position = $3
                 RETURNING id::text"),
            &[&user.0, &i16::from(input.from), &i16::from(input.to), &input.date, &input.amount_cents],
        )
        .await?
        .ok_or_else(|| bad("Quem paga e quem recebe devem ser membros do domicílio."))?
        .get(0);
    Ok(api::Settlement { id, ..input })
}

/// Exclui o acerto do domicílio; 404 se não existir. Núcleo de `DELETE /api/settlements/{id}`.
pub(super) async fn remove_settlement(db: &Client, user: User, id: &str) -> Result<(), ApiError> {
    if !is_uuid(id) {
        return Err(not_found());
    }
    let deleted = db
        .execute(
            &format!("DELETE FROM settlements WHERE id = $2::text::uuid AND household_id = {HOUSEHOLD}"),
            &[&user.0, &id],
        )
        .await?;
    if deleted == 0 {
        return Err(not_found());
    }
    Ok(())
}

pub(super) async fn create_settlement(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::Settlement>,
) -> Result<Json<api::Settlement>, ApiError> {
    let db = state.db().await?;
    Ok(Json(save_settlement(&db, user, input).await?))
}

pub(super) async fn delete_settlement(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !is_uuid(&id) {
        return Err(not_found());
    }
    let db = state.db().await?;
    remove_settlement(&db, user, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Nome curto, sem controles; devolve o texto aparado.
fn short_name(name: &str, max: usize, message: &'static str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.chars().count() > max || name.chars().any(char::is_control) {
        return Err(bad(message));
    }
    Ok(name.into())
}

/// Cria ou substitui a meta única da reserva do domicílio. Núcleo de `PUT /api/reserve`.
pub(super) async fn save_reserve(db: &Client, user: User, input: &api::Reserve) -> Result<(), ApiError> {
    if input.opening_cents < 0 || input.target_cents < 0 {
        return Err(bad("Os valores da reserva não podem ser negativos."));
    }
    let name = short_name(&input.name, 60, "Informe um nome de até 60 caracteres.")?;
    let saved = db
        .execute(
            "INSERT INTO reserve_goals(household_id, owner_id, name, opening_balance_cents, target_cents)
             SELECT household_id, id, $2, $3, $4 FROM members WHERE client_user_id = $1
             ON CONFLICT (household_id) DO UPDATE SET owner_id = EXCLUDED.owner_id, name = EXCLUDED.name,
                 opening_balance_cents = EXCLUDED.opening_balance_cents, target_cents = EXCLUDED.target_cents",
            &[&user.0, &name, &input.opening_cents, &input.target_cents],
        )
        .await?;
    if saved == 0 {
        return Err(not_member());
    }
    Ok(())
}

/// Renomeia o membro da posição. Núcleo de `PUT /api/members/{position}`.
pub(super) async fn rename_position(db: &Client, user: User, position: u8, name: &str) -> Result<(), ApiError> {
    let name = short_name(name, 40, "Informe um nome de 1 a 40 caracteres.")?;
    if name.is_empty() {
        return Err(bad("Informe um nome de 1 a 40 caracteres."));
    }
    let updated = db
        .execute(
            &format!("UPDATE members SET display_name = $3 WHERE household_id = {HOUSEHOLD} AND position = $2"),
            &[&user.0, &i16::from(position), &name],
        )
        .await?;
    if updated == 0 {
        return Err(not_found());
    }
    Ok(())
}

/// Escolhe (ou limpa, com `None`) o banco da conexão. Vale para os cartões e contas dela já na
/// leitura do estado e nas próximas importações. Núcleo de `PUT /api/connections/{item}/institution`.
pub(super) async fn set_institution(db: &Client, user: User, item_id: &str, key: Option<&str>) -> Result<(), ApiError> {
    valid_id(item_id)?;
    if let Some(key) = key {
        let exists: bool = db
            .query_one("SELECT EXISTS (SELECT 1 FROM institutions WHERE key = $1)", &[&key])
            .await?
            .get(0);
        if !exists {
            return Err(bad("Instituição inexistente."));
        }
    }
    let updated = db
        .execute(
            &format!("UPDATE bank_connections SET institution_key = $3 WHERE external_id = $2 AND household_id = {HOUSEHOLD}"),
            &[&user.0, &item_id, &key],
        )
        .await?;
    if updated == 0 {
        return Err(not_found());
    }
    Ok(())
}

/// Ajustes do domicílio (PUT /api/settings): por ora, o interruptor do rateio compartilhado.
/// Núcleo da rota, reusado pelas ferramentas do assistente.
pub(super) async fn save_settings(db: &Client, user: User, input: &api::Settings) -> Result<(), ApiError> {
    let updated = db
        .execute(
            &format!("UPDATE households SET shared_split_off = $2 WHERE id = {HOUSEHOLD}"),
            &[&user.0, &input.shared_split_off],
        )
        .await?;
    if updated == 0 {
        return Err(not_member());
    }
    Ok(())
}

pub(super) async fn put_reserve(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::Reserve>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    save_reserve(&db, user, &input).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn rename_member(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(position): Path<u8>,
    Json(input): Json<api::MemberName>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    rename_position(&db, user, position, &input.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Escolhe (ou limpa, com `null`) o banco da conexão. Vale para os cartões e contas dela já na
/// leitura do estado e nas próximas importações.
pub(super) async fn set_connection_institution(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Path(item_id): Path<String>,
    Json(input): Json<api::ConnectionInstitution>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    set_institution(&db, user, &item_id, input.institution_key.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ajustes do domicílio (PUT /api/settings): por ora, o interruptor do rateio compartilhado.
pub(super) async fn put_settings(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::Settings>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    save_settings(&db, user, &input).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── Orçamentos, conversa e auditoria do assistente ─────────────────────────

/// Define (ou, com valor <= 0, remove) o orçamento mensal do domicílio para uma categoria.
/// Núcleo de `PUT /api/budgets`, reusado pela ferramenta do assistente.
pub(super) async fn set_budget(db: &Client, user: User, budget: &api::Budget) -> Result<(), ApiError> {
    let exists: bool = db
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM categories WHERE key = $1)",
            &[&budget.category],
        )
        .await?
        .get(0);
    if !exists {
        return Err(bad("Categoria inexistente."));
    }
    if budget.monthly_cents <= 0 {
        db.execute(
            &format!("DELETE FROM budgets WHERE household_id = {HOUSEHOLD} AND category_key = $2"),
            &[&user.0, &budget.category],
        )
        .await?;
        return Ok(());
    }
    let saved = db
        .execute(
            &format!(
                "INSERT INTO budgets(household_id, category_key, monthly_cents)
                 SELECT household_id, $2::text, $3::bigint FROM members WHERE client_user_id = $1
                 ON CONFLICT (household_id, category_key) DO UPDATE SET monthly_cents = EXCLUDED.monthly_cents"
            ),
            &[&user.0, &budget.category, &budget.monthly_cents],
        )
        .await?;
    if saved == 0 {
        return Err(not_member());
    }
    Ok(())
}

/// `PUT /api/budgets`: grava (ou remove, com valor <= 0) o orçamento da categoria.
pub(super) async fn put_budget(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::Budget>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    set_budget(&db, user, &input).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Grava uma mensagem na conversa do membro e poda o histórico acima de 200 linhas.
pub(super) async fn store_chat_message(
    db: &Client,
    user: User,
    me: i16,
    role: &str,
    content: &str,
    ops: &[api::Op],
) -> Result<(), ApiError> {
    let ops = serde_json::to_string(ops).unwrap_or_else(|_| "[]".into());
    db.execute(
        &format!(
            "INSERT INTO chat_messages(household_id, member_position, role, content, ops)
             VALUES ({HOUSEHOLD}, $2, $3, $4, $5::text::jsonb)"
        ),
        &[&user.0, &me, &role, &content, &ops],
    )
    .await?;
    db.execute(
        &format!(
            "DELETE FROM chat_messages WHERE household_id = {HOUSEHOLD} AND member_position = $2
             AND id NOT IN (SELECT id FROM chat_messages WHERE household_id = {HOUSEHOLD} AND member_position = $2
                 ORDER BY id DESC LIMIT 200)"
        ),
        &[&user.0, &me],
    )
    .await?;
    Ok(())
}

/// As últimas 50 mensagens da conversa do membro, mais antigas primeiro. Núcleo de
/// `GET /api/assistant/history`.
pub(super) async fn chat_history(db: &Client, user: User, me: i16) -> Result<api::ChatHistory, ApiError> {
    let rows = db
        .query(
            &format!(
                "SELECT role, content, ops::text FROM (
                     SELECT id, role, content, ops FROM chat_messages
                     WHERE household_id = {HOUSEHOLD} AND member_position = $2
                     ORDER BY id DESC LIMIT 50) recentes ORDER BY recentes.id"
            ),
            &[&user.0, &me],
        )
        .await?;
    let messages = rows
        .iter()
        .map(|row| {
            let ops: String = row.get(2);
            api::ChatMsg {
                role: row.get(0),
                content: row.get(1),
                ops: serde_json::from_str(&ops).unwrap_or_default(),
            }
        })
        .collect();
    Ok(api::ChatHistory { messages })
}

/// Apaga a conversa persistida do membro. Núcleo de `DELETE /api/assistant/history`.
pub(super) async fn clear_chat_history(db: &Client, user: User, me: i16) -> Result<(), ApiError> {
    db.execute(
        &format!("DELETE FROM chat_messages WHERE household_id = {HOUSEHOLD} AND member_position = $2"),
        &[&user.0, &me],
    )
    .await?;
    Ok(())
}

/// As últimas 100 ações do assistente para o membro, mais recentes primeiro. Núcleo de
/// `GET /api/assistant/actions`.
pub(super) async fn assistant_actions(db: &Client, user: User, me: i16) -> Result<api::ActionLog, ApiError> {
    let rows = db
        .query(
            &format!(
                "SELECT tool, label, to_char(created_at AT TIME ZONE 'UTC', {UTC}) FROM assistant_actions
                 WHERE household_id = {HOUSEHOLD} AND member_position = $2
                 ORDER BY id DESC LIMIT 100"
            ),
            &[&user.0, &me],
        )
        .await?;
    let actions = rows
        .iter()
        .map(|row| api::ActionRecord {
            tool: row.get(0),
            label: row.get(1),
            at: row.get(2),
        })
        .collect();
    Ok(api::ActionLog { actions })
}

/// Registra uma ação executada pelo assistente (auditoria do membro). Payload grande demais
/// (p.ex. o listão da busca) é cortado em ~2 KB; se o corte quebrar o JSON, vai sem payload.
pub(super) async fn record_action(
    db: &Client,
    user: User,
    me: i16,
    tool: &str,
    label: &str,
    payload: &Value,
) -> Result<(), ApiError> {
    let mut payload = serde_json::to_string(payload).unwrap_or_default();
    if payload.len() > 2048 {
        payload = payload.chars().take(2048).collect();
        payload = serde_json::from_str::<Value>(&payload)
            .map(|value| value.to_string())
            .unwrap_or_default();
    }
    let payload = (!payload.is_empty()).then_some(payload);
    db.execute(
        &format!(
            "INSERT INTO assistant_actions(household_id, member_position, tool, label, payload)
             VALUES ({HOUSEHOLD}, $2, $3, $4, $5::text::jsonb)"
        ),
        &[&user.0, &me, &tool, &label, &payload],
    )
    .await?;
    Ok(())
}
