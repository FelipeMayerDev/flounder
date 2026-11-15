//! Rotina "fechar o mês": copia os lançamentos manuais do mês anterior, lista as faturas que
//! vencem no mês, sugere o acerto do casal, aponta contas fixas fora da média e os próximos
//! fechamentos de cartão — tudo num relatório só, para a conversa e para a rota.

use super::assistant::{core_data, Row};
use super::db::{self, bad, HOUSEHOLD};
use super::*;
use flounder_core::model::{days_to_closing, money, Date};
use tokio_postgres::Client;

/// Corpo de `POST /api/routine/month`: o mês é opcional (vazio ou ausente = o mês atual).
#[derive(Deserialize)]
struct RoutineMonth {
    month: Option<String>,
}

/// `POST /api/routine/month` com `{"month": "AAAA-MM"}` (opcional): roda a rotina e devolve o
/// relatório. Cada lançamento copiado entra na auditoria do membro.
pub(super) async fn post_month(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    let month = serde_json::from_str::<RoutineMonth>(&body).ok().and_then(|input| input.month);
    let db = state.db().await?;
    let me: i16 = db
        .query_one("SELECT position FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    let report = run_routine(&db, user, me, month).await?;
    for entry in report["copiados"].as_array().cloned().unwrap_or_default() {
        let op = op_de_copia(&entry);
        if let Err(error) = db::record_action(&db, user, me, &op.tool, &op.label, &entry).await {
            eprintln!("Rotina: falhou ao registrar a ação ({}).", error.1);
        }
    }
    Ok(Json(report))
}

/// A ação de um lançamento copiado: o selo no chat e o registro na auditoria.
pub(super) fn op_de_copia(entry: &Value) -> api::Op {
    api::Op {
        tool: "copiar_mes_anterior".into(),
        label: format!(
            "copiou {} {} em {}",
            entry["descricao"].as_str().unwrap_or("lançamento"),
            money(entry["valor_centavos"].as_i64().unwrap_or(0)),
            entry["data"].as_str().map(tools::dia_br).unwrap_or_default(),
        ),
    }
}

/// Fecha o mês `month` (`None`/vazio = o mês atual, em America/Sao_Paulo) e devolve o
/// relatório completo. O saldo do casal e os alertas olham o mês ANTES da cópia (as cópias
/// não duplicam o acerto nem distorcem a média). Quem chama grava as ações: a rota registra
/// uma por cópia; a conversa registra pelos selos que a ferramenta devolve.
pub(super) async fn run_routine(
    db: &Client,
    user: User,
    me: i16,
    month: Option<String>,
) -> Result<Value, ApiError> {
    let mes = match month.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => tools::mes_de(m).map_err(|_| bad("Use o mês no formato AAAA-MM."))?,
        None => {
            let atual: String = db
                .query_one(
                    "SELECT to_char(date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo'), 'YYYY-MM')",
                    &[],
                )
                .await?
                .get(0);
            tools::mes_de(&atual).map_err(|_| bad("Use o mês no formato AAAA-MM."))?
        }
    };
    // Linhas de TODA a história (o saldo entre o casal é cumulativo) + nomes e ajustes.
    let rows: Vec<Row> = db
        .query(
            &format!(
                "SELECT to_char(e.entry_date, 'YYYY-MM-DD'), e.amount_cents, e.kind, e.category_key, e.shared,
                        m.position, e.description, e.paid, e.recurring,
                        (SELECT a.external_id FROM cards c JOIN bank_accounts a ON a.id = c.account_id WHERE c.id = e.card_id)
                     FROM entries e JOIN members m ON m.id = e.owner_id
                     WHERE e.household_id = {HOUSEHOLD} ORDER BY e.entry_date"
            ),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|r| Row {
            date: r.get(0),
            amount: r.get(1),
            kind: r.get(2),
            category: r.get(3),
            shared: r.get(4),
            owner: r.get::<_, i16>(5) as u8,
            desc: r.get(6),
            paid: r.get(7),
            recurring: r.get(8),
            card: r.get(9),
        })
        .collect();
    let names: Vec<String> = db
        .query(
            &format!("SELECT display_name FROM members WHERE household_id = {HOUSEHOLD} ORDER BY position"),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let shared_split_off: bool = db
        .query_one(&format!("SELECT shared_split_off FROM households WHERE id = {HOUSEHOLD}"), &[&user.0])
        .await?
        .get(0);
    let settlements: Vec<(String, i64, u8)> = db
        .query(
            &format!(
                "SELECT to_char(s.settled_on, 'YYYY-MM-DD'), s.amount_cents, f.position
                 FROM settlements s JOIN members f ON f.id = s.from_id
                 WHERE s.household_id = {HOUSEHOLD}"
            ),
            &[&user.0],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get::<_, i16>(2) as u8))
        .collect();
    let data = core_data(&names, &rows, &settlements, shared_split_off);
    // Saldo entre o casal acumulado até o fim do mês: > 0 → B deve a A; < 0 → A deve a B.
    let saldo = data.house_balance(mes);
    let alertas: Vec<Value> = data
        .price_alerts(mes, 0.20)
        .into_iter()
        .map(|(desc, recente, media)| json!({"desc": desc, "recent_cents": recente, "average_cents": media}))
        .collect();

    // Copia o mês anterior (a mesma cópia do botão e da ferramenta) e lista as faturas do mês.
    let (criados, _pulados) = tools::copiar_mes(db, user, mes).await?;
    let faturas: Vec<Value> = db
        .query(
            &format!(
                "SELECT 'Fatura ' || c.name, to_char(ci.due_date, 'YYYY-MM-DD'), ci.total_cents, ci.paid
                 FROM card_invoices ci JOIN cards c ON c.id = ci.card_id
                 WHERE ci.household_id = {HOUSEHOLD} AND to_char(ci.due_date, 'YYYY-MM') = $2
                 ORDER BY ci.paid, ci.due_date, c.name"
            ),
            &[&user.0, &format!("{:04}-{:02}", mes.y, mes.m)],
        )
        .await?
        .iter()
        .map(|r| {
            json!({
                "label": r.get::<_, String>(0),
                "due": r.get::<_, String>(1),
                "total_cents": r.get::<_, Option<i64>>(2),
                "paid": r.get::<_, bool>(3),
            })
        })
        .collect();

    // Fechamentos dos cartões de quem rodou (os do parceiro não são problema dele).
    let hoje: String = db
        .query_one("SELECT to_char(now() AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD')", &[])
        .await?
        .get(0);
    let cards = db
        .query(
            &format!(
                "SELECT c.name, COALESCE(c.manual_closing_day, c.closing_day)
                 FROM cards c JOIN bank_accounts a ON a.id = c.account_id JOIN members m ON m.id = c.owner_id
                 WHERE c.household_id = {HOUSEHOLD} AND m.position = $2
                     AND COALESCE(c.manual_closing_day, c.closing_day) IS NOT NULL
                 ORDER BY c.name, a.external_id"
            ),
            &[&user.0, &me],
        )
        .await?;
    let fechamentos: Vec<Value> = cards
        .iter()
        .filter_map(|r| {
            let dia = u32::try_from(r.get::<_, i16>(1)).ok()?;
            let dias = Date::iso(&hoje).map(|h| days_to_closing(dia, h))?;
            Some(json!({"nome": r.get::<_, String>(0), "dia_fechamento": dia, "dias": dias}))
        })
        .collect();

    Ok(json!({
        "copiados": criados,
        "faturas": faturas,
        "acerto_sugerido": if saldo == 0 {
            Value::Null
        } else {
            json!({"valor_cents": saldo.abs(), "de": if saldo > 0 { 1 } else { 0 }})
        },
        "alertas_preco": alertas,
        "fechamentos": fechamentos,
    }))
}
