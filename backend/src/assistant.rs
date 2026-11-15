//! Assistente financeiro: análises periódicas e conversa com o GLM (Z.ai), na voz do Flounder.
//! O token vive só no backend (`ZAI_TOKEN`); a interface conversa por estas rotas, e a
//! privacidade é garantida aqui dentro: cada membro recebe a própria análise e a do casal,
//! nunca a individual do outro.

use super::db::{self, bad, HOUSEHOLD, UTC};
use super::*;
use flounder_core::model::{budget_usage, days_to_closing, forming_invoice_window, Category, Cents, Date, Month};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// Assunto da análise: 0 e 1 são as posições dos membros; 2 é o casal.
const COUPLE: i16 = 2;
/// Janela analisada: mês atual (em andamento) + os três anteriores.
const PERIOD_MONTHS: i32 = 4;
/// Histórico máximo enviado numa conversa.
const MAX_CHAT_MESSAGES: usize = 12;
const MAX_CHAT_CHARS: usize = 4000;

// ───────────────────────── Cliente Z.ai (GLM) ─────────────────────────

pub(super) struct Zai {
    http: reqwest::Client,
    base: String,
    token: String,
    model: String,
}

/// Uma chamada de ferramenta pedida pelo modelo (`choices[0].message.tool_calls`).
struct ToolCall {
    id: String,
    name: String,
    /// Os argumentos chegam como texto JSON; quem executa valida.
    arguments: String,
}

/// A mensagem do modelo: texto final e/ou pedidos de ferramenta.
struct AiMessage {
    content: String,
    tool_calls: Vec<ToolCall>,
}

impl Zai {
    pub(super) fn new(token: String, base: String, model: String) -> Result<Zai, reqwest::Error> {
        Ok(Zai {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()?,
            base,
            token,
            model,
        })
    }

    /// Uma resposta em texto: o sistema define a persona e traz os dados; `messages` é o histórico.
    async fn chat(&self, system: &str, messages: &[api::ChatMsg]) -> Result<String, ApiError> {
        let mut body = Vec::with_capacity(messages.len() + 1);
        body.push(json!({"role": "system", "content": system}));
        for m in messages {
            body.push(json!({"role": m.role, "content": m.content}));
        }
        let message = self.round(&body, None).await?;
        Ok(clean_text(&message.content))
    }

    /// Uma rodada de conversa, com ou sem ferramentas (`tools` no formato OpenAI do Z.ai;
    /// `tool_choice: auto` deixa o modelo decidir entre responder e agir).
    async fn round(&self, messages: &[Value], tools: Option<&Value>) -> Result<AiMessage, ApiError> {
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.8,
            "max_tokens": 4096,
            "stream": false,
        });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
            body["tool_choice"] = json!("auto");
        }
        let value: Value = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|_| ai_down())?
            .error_for_status()
            .map_err(|err| ai_status(err.status()))?
            .json()
            .await
            .map_err(|_| ai_down())?;
        message_of(&value)
    }
}

/// Lê `choices[0].message`; sem texto e sem pedidos de ferramenta, o Z.ai falhou de algum jeito.
fn message_of(value: &Value) -> Result<AiMessage, ApiError> {
    let message = &value["choices"][0]["message"];
    let content = message["content"].as_str().unwrap_or("").trim().to_owned();
    let tool_calls: Vec<ToolCall> = message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .filter_map(|c| {
                    Some(ToolCall {
                        id: c["id"].as_str()?.to_owned(),
                        name: c["function"]["name"].as_str()?.to_owned(),
                        arguments: c["function"]["arguments"].as_str().unwrap_or("{}").to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if content.is_empty() && tool_calls.is_empty() {
        return Err(ai_down());
    }
    Ok(AiMessage { content, tool_calls })
}

fn ai_down() -> ApiError {
    ApiError(
        StatusCode::BAD_GATEWAY,
        "O assistente (Z.ai) não respondeu. Tente de novo em instantes.",
    )
}

/// Erros do Z.ai com causa clara: token de outro plano, saldo/limite esgotado, etc.
fn ai_status(status: Option<StatusCode>) -> ApiError {
    match status {
        Some(StatusCode::UNAUTHORIZED) | Some(StatusCode::FORBIDDEN) => ApiError(
            StatusCode::BAD_GATEWAY,
            "O Z.ai recusou o ZAI_TOKEN do backend (chave inválida ou de outro plano).",
        ),
        Some(StatusCode::TOO_MANY_REQUESTS) => ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "O Z.ai recusou a chamada: sem saldo ou com o limite do plano esgotado.",
        ),
        _ => ai_down(),
    }
}

fn not_configured() -> ApiError {
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "O assistente não está configurado no backend: defina ZAI_TOKEN e reinicie o serviço.",
    )
}

/// Texto simples e civilizado: sem caracteres de controle (exceto a quebra de linha) nem
/// marcações de markdown, com tamanho máximo de papo.
fn clean_text(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    let text = text.replace("**", "").replace("### ", "").replace("## ", "").replace("# ", "");
    text.trim().chars().take(6000).collect::<String>().trim_end().to_string()
}

/// "R$ 1.234,56", com sinal simples para negativos.
fn brl(cents: i64) -> String {
    let digits = (cents.unsigned_abs() / 100).to_string();
    let mut int = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            int.push('.');
        }
        int.push(ch);
    }
    format!("{}R$ {int},{:02}", if cents < 0 { "-" } else { "" }, cents.unsigned_abs() % 100)
}

fn category_label(key: &str) -> &'static str {
    match key {
        "salary" => "Renda",
        "housing" => "Moradia",
        "utilities" => "Contas da casa",
        "groceries" => "Mercado",
        "transport" => "Transporte",
        "health" => "Saúde",
        "leisure" => "Lazer",
        "reserve" => "Reserva",
        "family" => "Família",
        "shopping" => "Compras",
        "services" => "Serviços",
        "education" => "Educação",
        "pets" => "Pets",
        _ => "Outros",
    }
}

/// "2026-08" → "ago/26".
fn month_label(key: &str) -> String {
    const NAMES: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    let y: u32 = key.get(2..4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: usize = key.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 12);
    format!("{}/{:02}", NAMES[m - 1], y)
}

/// "2026-11-25" → "nov" (nome curto do mês da fatura).
fn mes_curto(due: &str) -> &str {
    const NAMES: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    match due.get(5..7).and_then(|s| s.parse::<usize>().ok()) {
        Some(m) if (1..=12).contains(&m) => NAMES[m - 1],
        _ => "",
    }
}

/// "2026-11-12" → "12/11/2026".
fn date_br(iso: &str) -> String {
    match (iso.get(8..10), iso.get(5..7), iso.get(0..4)) {
        (Some(d), Some(m), Some(y)) => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

/// "2026-11-25" → "25/11" (o vencimento das faturas, sem o ano).
fn dia_curto(iso: &str) -> String {
    match (iso.get(8..10), iso.get(5..7)) {
        (Some(d), Some(m)) => format!("{d}/{m}"),
        _ => iso.to_string(),
    }
}

// ───────────────────────── Dados para o modelo ─────────────────────────

/// Linha bruta de lançamento, também usada pela rotina do mês (`routine.rs`).
pub(super) struct Row {
    pub date: String,
    pub amount: i64,
    pub kind: String,
    pub category: String,
    pub shared: bool,
    pub owner: u8,
    pub desc: String,
    pub paid: bool,
    pub recurring: bool,
    /// Id externo do cartão conectado (compra nele); `None` = lançamento manual.
    pub card: Option<String>,
}

/// Cartão conectado no snapshot: o que o prompt precisa para avisar de fechamento e fatura.
#[derive(Clone)]
struct SnapCard {
    /// Id externo (Pluggy), o mesmo que liga as compras do cartão.
    external_id: String,
    name: String,
    owner: u8,
    closing_day: Option<u32>,
    bills: Vec<connected::ConnectedBill>,
    /// Total da fatura em formação; `None` sem fechamento utilizável.
    forming: Option<i64>,
}

/// Tudo que os prompts recebem, já agregado. Chaves de mês são "AAAA-MM".
struct Snapshot {
    period_start: String,
    period_end: String,
    /// "AAAA-MM-DD" de hoje no fuso do app, para os avisos de fechamento.
    hoje: String,
    months: Vec<String>,
    names: [String; 2],
    shared_split_off: bool,
    income: BTreeMap<(u8, String), i64>,
    expense: BTreeMap<(u8, String), i64>,
    shared_paid: BTreeMap<(u8, String), i64>,
    /// Despesas pessoais (não compartilhadas) por (dono, mês, categoria).
    personal: BTreeMap<(u8, String, String), i64>,
    /// Despesas compartilhadas por (dono, mês, categoria) — para o uso dos orçamentos.
    shared_cat: BTreeMap<(u8, String, String), i64>,
    /// Aportes pagos na reserva, por mês (domicílio).
    reserve: BTreeMap<String, i64>,
    /// (data, valor, posição de quem pagou).
    settlements: Vec<(String, i64, u8)>,
    /// Parcelas lançadas no período: (descrição, atual, total, valor da parcela, dono).
    installments: Vec<(String, u32, u32, i64, u8)>,
    /// Contas fixas com média do período: (descrição, categoria, média, dono).
    recurring: Vec<(String, String, i64, u8)>,
    /// Orçamentos do domicílio: (chave da categoria, limite em centavos).
    budgets: Vec<(String, i64)>,
    /// Cartões conectados do domicílio (o dono decide quem vê).
    cards: Vec<SnapCard>,
    /// Alertas de preço do núcleo (`Data::price_alerts`): (descrição, valor recente, média).
    alerts: Vec<(String, i64, i64)>,
    /// Proporção da renda de cada um na janela de 3 meses, pela regra do núcleo
    /// (`Data::income_weights`); `None` = sem renda (o rateio volta a 50/50).
    income_weights: Option<[f64; 2]>,
}

/// "Loja (2/10)" → ("Loja", 2, 10); a regra é a do núcleo, a mesma que a interface usa.
fn split_parcela(desc: &str) -> Option<(String, u32, u32)> {
    flounder_core::model::split_installment(desc)
}

/// O que `load` traz: o snapshot pronto para o prompt e as linhas brutas, para o hash.
struct Loaded {
    snap: Snapshot,
    rows: Vec<Row>,
}

impl Loaded {
    fn hash(&self) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for r in &self.rows {
            (
                r.date.as_str(), r.amount, r.kind.as_str(), r.category.as_str(), r.shared, r.owner,
                r.desc.as_str(), r.paid, r.recurring,
            )
                .hash(&mut h);
        }
        for s in &self.snap.settlements {
            s.hash(&mut h);
        }
        format!("{:016x}", h.finish())
    }
}

impl Snapshot {
    /// Bloco de texto para o modelo. O do casal inclui os dois; o individual inclui só a
    /// pessoa e os totais compartilhados — nunca os detalhes do outro.
    fn render(&self, subject: u8) -> String {
        let couple = subject == COUPLE as u8;
        let mut out = vec![format!(
            "PERÍODO: {} a {} (meses: {}). {}",
            date_br(&self.period_start),
            date_br(&self.period_end),
            self.months.join(", "),
            if couple {
                format!("Análise do CASAL: {} e {}.", self.names[0], self.names[1])
            } else {
                format!("Análise INDIVIDUAL de {}.", self.names[subject as usize])
            }
        )];
        out.push(format!(
            "DIVISÃO DA RENDA: {}",
            self.income_weights.map_or("sem renda registrada (rateio 50/50)".into(), |[wa, wb]| format!(
                "{} {:.0}% / {} {:.0}%",
                self.names[0],
                wa * 100.0,
                self.names[1],
                wb * 100.0
            ))
        ));
        out.push(format!(
            "RENDA POR MÊS ({} / {}): {}",
            self.names[0],
            self.names[1],
            self.months
                .iter()
                .map(|m| format!(
                    "{}: {} / {}",
                    month_label(m),
                    brl(self.income.get(&(0, m.clone())).copied().unwrap_or(0)),
                    brl(self.income.get(&(1, m.clone())).copied().unwrap_or(0)),
                ))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        let expense_of = |owner: u8, m: &str| self.expense.get(&(owner, m.to_string())).copied().unwrap_or(0);
        if couple {
            out.push(format!(
                "DESPESAS TOTAIS DO DOMICÍLIO POR MÊS: {}",
                self.months
                    .iter()
                    .map(|m| format!("{}: {}", month_label(m), brl(expense_of(0, m) + expense_of(1, m))))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        } else {
            out.push(format!(
                "DESPESAS QUE {} PAGOU POR MÊS: {}",
                self.names[subject as usize],
                self.months
                    .iter()
                    .map(|m| format!("{}: {}", month_label(m), brl(expense_of(subject, m))))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        // Maiores categorias pessoais do período (quem analisa é quem gasta).
        let mut cats: BTreeMap<String, i64> = BTreeMap::new();
        for ((owner, _, category), value) in &self.personal {
            if couple || *owner == subject {
                *cats.entry(category.clone()).or_default() += value;
            }
        }
        let mut cats: Vec<(String, i64)> = cats.into_iter().collect();
        cats.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        if !cats.is_empty() {
            out.push(format!(
                "MAIORES CATEGORIAS PESSOAIS NO PERÍODO: {}",
                cats.iter().take(6).map(|(c, v)| format!("{} {}", category_label(c), brl(*v))).collect::<Vec<_>>().join(" · ")
            ));
        }
        let shared: i64 = self.shared_paid.values().sum();
        if shared > 0 {
            let by_owner: Vec<String> = (0..2)
                .map(|p| {
                    format!(
                        "{} {}",
                        self.names[p as usize],
                        brl(self.shared_paid.iter().filter(|((o, _), _)| *o == p).map(|(_, v)| v).sum())
                    )
                })
                .collect();
            out.push(format!(
                "DESPESAS COMPARTILHADAS NO PERÍODO: total {} · pagas por: {}{}",
                brl(shared),
                by_owner.join(" · "),
                if self.shared_split_off {
                    " · rateio desligado (cada despesa é de quem pagou)"
                } else {
                    ""
                }
            ));
        }
        let reserve: i64 = self.reserve.values().sum();
        if reserve > 0 {
            out.push(format!(
                "RESERVA NO PERÍODO: aportes de {} (média de {} por mês com aporte).",
                brl(reserve),
                brl(reserve / self.reserve.len().max(1) as i64)
            ));
        }
        if !self.settlements.is_empty() {
            let net: i64 = self.settlements.iter().map(|(_, v, f)| if *f == 0 { *v } else { -*v }).sum();
            out.push(format!(
                "ACERTOS NO PERÍODO: {} transferência(s); {} recebeu {} líquido de acertos.",
                self.settlements.len(),
                if net >= 0 { self.names[0].as_str() } else { self.names[1].as_str() },
                brl(net.abs())
            ));
        }
        // Parcelas em aberto: o grupo mais adiantado por (descrição, total, dono) e o valor
        // da parcela mais recente.
        let mut groups: BTreeMap<(String, u32, u8), (u32, i64)> = BTreeMap::new();
        for (desc, n, of, amount, owner) in &self.installments {
            let entry = groups.entry((desc.clone(), *of, *owner)).or_insert((*n, *amount));
            if *n > entry.0 {
                entry.0 = *n;
            }
            entry.1 = *amount;
        }
        let open: Vec<String> = groups
            .iter()
            .filter_map(|((desc, of, owner), (n, amount))| {
                (*n < *of).then(|| format!("{} ({n}/{of}), faltam {} de {} ({})", desc, of - n, brl(*amount), self.names[*owner as usize]))
            })
            .collect();
        if !open.is_empty() {
            out.push(format!("PARCELAS EM ABERTO: {}", open.join(" · ")));
        }
        if !self.recurring.is_empty() {
            let defs: Vec<String> = self
                .recurring
                .iter()
                .take(8)
                .map(|(desc, category, avg, _)| format!("{} ({}, média {})", desc, category_label(category), brl(*avg)))
                .collect();
            out.push(format!("CONTAS FIXAS DETECTADAS: {}", defs.join(" · ")));
        }
        // Cartões conectados: fechamento próximo, fatura em formação e a fatura do mês —
        // só os de quem lê (os do parceiro ficam de fora).
        let hoje = Date::iso(&self.hoje);
        let mes_atual = self.hoje.get(..7).unwrap_or("").to_owned();
        let cards: Vec<String> = self
            .cards
            .iter()
            .filter(|c| couple || c.owner == subject)
            .filter(|c| c.closing_day.is_some() || !c.bills.is_empty())
            .filter_map(|c| {
                let mut parts: Vec<String> = Vec::new();
                if let (Some(day), Some(h)) = (c.closing_day.filter(|d| (1..=31).contains(d)), hoje) {
                    parts.push(match days_to_closing(day, h) {
                        0 => format!("fecha dia {day} (hoje)"),
                        n => format!("fecha dia {day} (em {n} dias)"),
                    });
                }
                if let Some(total) = c.forming.filter(|t| *t > 0) {
                    parts.push(format!("fatura em formação {}", brl(total)));
                }
                if let Some(bill) = c.bills.iter().find(|b| b.due.starts_with(&mes_atual)) {
                    parts.push(format!("fatura de {} vence {}", mes_curto(&bill.due), dia_curto(&bill.due)));
                }
                (!parts.is_empty()).then(|| format!("{}: {}", c.name, parts.join(" · ")))
            })
            .collect();
        if !cards.is_empty() {
            out.push(format!("CARTÕES: {}", cards.join("; ")));
        }
        // Orçamentos do domicílio (ambos veem): gasto = o meu + o compartilhado do casal
        // no mês atual; para o casal, tudo.
        if !self.budgets.is_empty() {
            let m = self.months.last().cloned().unwrap_or_default();
            let gasto = |s: u8, cat: &str| -> Cents {
                let own: Cents = if s == COUPLE as u8 {
                    (0..2)
                        .map(|o| self.personal.get(&(o, m.clone(), cat.to_string())).copied().unwrap_or(0))
                        .sum()
                } else {
                    self.personal.get(&(s, m.clone(), cat.to_string())).copied().unwrap_or(0)
                };
                let shared: Cents = (0..2)
                    .map(|o| self.shared_cat.get(&(o, m.clone(), cat.to_string())).copied().unwrap_or(0))
                    .sum();
                own + shared
            };
            let budgets: Vec<api::Budget> = self
                .budgets
                .iter()
                .map(|(c, limite)| api::Budget { category: c.clone(), monthly_cents: *limite })
                .collect();
            let by_category: Vec<(Category, Cents)> =
                self.budgets.iter().map(|(c, _)| (category_of(c), gasto(subject, c))).collect();
            let usage = budget_usage(&by_category, &budgets);
            out.push(format!(
                "ORÇAMENTOS: {}",
                usage
                    .iter()
                    .map(|(c, spent, limite)| format!(
                        "{}: {} de {} ({}%)",
                        category_label(c),
                        brl(*spent),
                        brl(*limite),
                        spent * 100 / limite.max(&1)
                    ))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        // Contas fixas fora da média no mês atual, pela regra do núcleo.
        if !self.alerts.is_empty() {
            out.push(format!(
                "PREÇOS: {}",
                self.alerts
                    .iter()
                    .map(|(desc, recente, media)| format!("{desc}: {} vs média {}", brl(*recente), brl(*media)))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        out.join("\n")
    }
}

/// Carrega entradas, acertos, nomes e ajustes do domicílio para montar o snapshot.
async fn load(state: &Backend, home: &str) -> Result<Loaded, ApiError> {
    let db = state.db().await?;
    let (period_start, period_end): (String, String) = {
        let row = db
            .query_one(
                "SELECT to_char((date_trunc('month', now() AT TIME ZONE 'America/Sao_Paulo') - interval '3 months')::date, 'YYYY-MM-DD'),
                        to_char(now() AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD')",
                &[],
            )
            .await?;
        (row.get(0), row.get(1))
    };
    let names: Vec<String> = db
        .query(
            "SELECT display_name FROM members WHERE household_id = $1::text::uuid ORDER BY position",
            &[&home],
        )
        .await?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let shared_split_off: bool = db
        .query_one("SELECT shared_split_off FROM households WHERE id = $1::text::uuid", &[&home])
        .await?
        .get(0);
    let rows: Vec<Row> = db
        .query(
            "SELECT to_char(e.entry_date, 'YYYY-MM-DD'), e.amount_cents, e.kind, e.category_key, e.shared,
                    m.position, e.description, e.paid, e.recurring,
                    (SELECT a.external_id FROM cards c JOIN bank_accounts a ON a.id = c.account_id WHERE c.id = e.card_id)
                 FROM entries e JOIN members m ON m.id = e.owner_id
                 WHERE e.household_id = $1::text::uuid AND e.entry_date >= $2::text::date
                 ORDER BY e.entry_date",
            &[&home, &period_start],
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
    let settlements: Vec<(String, i64, u8)> = db
        .query(
            "SELECT to_char(s.settled_on, 'YYYY-MM-DD'), s.amount_cents, f.position
                 FROM settlements s JOIN members f ON f.id = s.from_id
                 WHERE s.household_id = $1::text::uuid AND s.settled_on >= $2::text::date",
            &[&home, &period_start],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get::<_, i16>(2) as u8))
        .collect();
    // Hoje no fuso do app, orçamentos do domicílio e cartões conectados (com as faturas),
    // para os avisos de fechamento, orçamento e preço no snapshot.
    let hoje: String = db
        .query_one(
            "SELECT to_char(now() AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD')",
            &[],
        )
        .await?
        .get(0);
    let budgets: Vec<(String, i64)> = db
        .query(
            "SELECT category_key, monthly_cents FROM budgets WHERE household_id = $1::text::uuid ORDER BY category_key",
            &[&home],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let cards: Vec<SnapCard> = {
        let bills: Vec<(String, String, Option<String>, Option<i64>, String, bool)> = db
            .query(
                "SELECT card_id::text, to_char(due_date, 'YYYY-MM-DD'), to_char(closing_date, 'YYYY-MM-DD'), total_cents, currency, paid
                 FROM card_invoices WHERE household_id = $1::text::uuid AND source = 'pluggy' ORDER BY due_date, currency",
                &[&home],
            )
            .await?
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5)))
            .collect();
        db.query(
            "SELECT c.id::text, a.external_id, c.name, m.position,
                    COALESCE(c.manual_closing_day, c.closing_day,
                        (SELECT EXTRACT(DAY FROM inv.closing_date)::smallint FROM card_invoices inv
                         WHERE inv.card_id = c.id AND inv.closing_date IS NOT NULL
                         ORDER BY inv.due_date DESC LIMIT 1))
                 FROM cards c JOIN bank_accounts a ON a.id = c.account_id JOIN members m ON m.id = c.owner_id
                 WHERE c.household_id = $1::text::uuid ORDER BY m.position, a.external_id",
            &[&home],
        )
        .await?
        .iter()
        .map(|r| {
            let id: String = r.get(0);
            SnapCard {
                bills: bills
                    .iter()
                    .filter(|b| b.0 == id)
                    .map(|(_, due, closing, total, currency, paid)| connected::ConnectedBill {
                        due: due.clone(),
                        closing: closing.clone(),
                        total: *total,
                        currency: currency.clone(),
                        paid: *paid,
                    })
                    .collect(),
                external_id: r.get(1),
                name: r.get(2),
                owner: r.get::<_, i16>(3) as u8,
                closing_day: r.get::<_, Option<i16>>(4).map(|d| d as u32),
                forming: None,
            }
        })
        .collect()
    };
    let snap = build_snapshot(&period_start, &period_end, names, shared_split_off, &rows, settlements, &hoje, budgets, cards);
    Ok(Loaded { snap, rows })
}

/// Agregação pura (testável): soma por mês, por dono e por categoria.
fn build_snapshot(
    period_start: &str,
    period_end: &str,
    names: Vec<String>,
    shared_split_off: bool,
    rows: &[Row],
    settlements: Vec<(String, i64, u8)>,
    hoje: &str,
    budgets: Vec<(String, i64)>,
    cards: Vec<SnapCard>,
) -> Snapshot {
    let start_month = &period_start[..7];
    let (sy, sm): (i32, i32) = (
        start_month.get(..4).and_then(|s| s.parse().ok()).unwrap_or(2026),
        start_month.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1),
    );
    let months: Vec<String> = (0..PERIOD_MONTHS)
        .map(|k| {
            let i = sy * 12 + sm - 1 + k;
            format!("{:04}-{:02}", i.div_euclid(12), i.rem_euclid(12) + 1)
        })
        .collect();
    let mut snap = Snapshot {
        period_start: period_start.to_owned(),
        period_end: period_end.to_owned(),
        hoje: hoje.to_owned(),
        months,
        names: [names.first().cloned().unwrap_or_default(), names.get(1).cloned().unwrap_or_default()],
        shared_split_off,
        income: BTreeMap::new(),
        expense: BTreeMap::new(),
        shared_paid: BTreeMap::new(),
        personal: BTreeMap::new(),
        shared_cat: BTreeMap::new(),
        reserve: BTreeMap::new(),
        settlements,
        installments: Vec::new(),
        recurring: Vec::new(),
        budgets,
        cards,
        alerts: Vec::new(),
        income_weights: None,
    };
    let month_of = |date: &str| date.get(..7).unwrap_or("").to_string();
    for r in rows {
        let m = month_of(&r.date);
        if r.kind == "income" {
            *snap.income.entry((r.owner, m.clone())).or_default() += r.amount;
            continue;
        }
        *snap.expense.entry((r.owner, m.clone())).or_default() += r.amount;
        if r.shared {
            *snap.shared_paid.entry((r.owner, m.clone())).or_default() += r.amount;
            *snap.shared_cat.entry((r.owner, m.clone(), r.category.clone())).or_default() += r.amount;
        } else {
            *snap.personal.entry((r.owner, m.clone(), r.category.clone())).or_default() += r.amount;
            if r.category == "reserve" && r.paid {
                *snap.reserve.entry(m.clone()).or_default() += r.amount;
            }
        }
        if let Some((base, n, of)) = split_parcela(&r.desc) {
            snap.installments.push((base, n, of, r.amount, r.owner));
        }
    }
    // Rateio pela renda e contas fixas passam pelas regras do núcleo (`Data`), as mesmas
    // da interface: janela de 3 meses para o rateio e média das últimas 3 ocorrências.
    let end_month = Month {
        y: period_end.get(..4).and_then(|s| s.parse().ok()).unwrap_or(2026),
        m: period_end.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1),
    };
    let data = core_data(&snap.names, rows, &snap.settlements, shared_split_off);
    snap.income_weights = data.income_weights(end_month);
    snap.recurring = data
        .recurring_defs(end_month, None)
        .into_iter()
        .map(|d| (d.desc, d.category.key().to_owned(), d.estimate, d.owner.idx() as u8))
        .collect();
    // Mesma ordem de antes (descrição, categoria, dono) para o prompt ficar estável.
    snap.recurring.sort_by(|a, b| (&a.0, &a.1, a.3).cmp(&(&b.0, &b.1, b.3)));
    // Fatura em formação de cada cartão e alertas de preço do mês atual (regras do núcleo).
    let hoje = Date::iso(hoje);
    for card in &mut snap.cards {
        card.forming = hoje
            .and_then(|h| forming_invoice_window(&card.bills, card.closing_day, h))
            .map(|w| data.connected_purchases_in(&card.external_id, w).map(|e| e.amount).sum());
    }
    snap.alerts = data.price_alerts(end_month, 0.20);
    snap
}

/// Chave da categoria no banco → `Category` do núcleo (o mesmo mapa da interface).
pub(super) fn category_of(key: &str) -> Category {
    match key {
        "salary" => Category::Salary,
        "housing" => Category::Housing,
        "utilities" => Category::Utilities,
        "groceries" => Category::Groceries,
        "transport" => Category::Transport,
        "health" => Category::Health,
        "leisure" => Category::Leisure,
        "reserve" => Category::Reserve,
        "family" => Category::Family,
        "shopping" => Category::Shopping,
        "services" => Category::Services,
        "education" => Category::Education,
        "pets" => Category::Pets,
        _ => Category::Other,
    }
}

/// Monta o `Data` do núcleo com as linhas do domicílio, para reusar as regras de rateio,
/// contas fixas, saldo do casal e janelas de fatura (`income_weights`, `recurring_defs`,
/// `house_balance`, `connected_purchases_in`) sem reimplementá-las aqui.
pub(super) fn core_data(
    names: &[String],
    rows: &[Row],
    settlements: &[(String, i64, u8)],
    shared_split_off: bool,
) -> flounder_core::model::Data {
    use flounder_core::model::{Data, Date, Entry, Kind, Person, Settlement};
    let person = |p: u8| if p == 0 { Person::A } else { Person::B };
    let kind = |k: &str| if k == "income" { Kind::Income } else { Kind::Expense };
    let mut data = Data::empty();
    data.names = [names.first().cloned().unwrap_or_default(), names.get(1).cloned().unwrap_or_default()];
    data.entries = rows
        .iter()
        .filter_map(|r| {
            Some(Entry {
                id: 0,
                desc: r.desc.clone(),
                amount: r.amount,
                date: Date::iso(&r.date)?,
                kind: kind(&r.kind),
                category: category_of(&r.category),
                owner: person(r.owner),
                shared: r.shared,
                card: None,
                connected: r.card.clone(),
                imported: false,
                paid: r.paid,
                recurring: r.recurring,
            })
        })
        .collect();
    data.settlements = settlements
        .iter()
        .filter_map(|(d, a, f)| Some(Settlement { id: String::new(), date: Date::iso(d)?, amount: *a, from: person(*f) }))
        .collect();
    data.shared_split_off = shared_split_off;
    data
}

// ───────────────────────── Prompts ─────────────────────────

const PERSONA: &str = "\
Você é o Flounder, um peixinho tropical que virou consultor financeiro de um aplicativo de \
finanças em casal. Estilo: bem-humorado, caloroso, direto ao ponto, nunca julga ninguém. \
Você fala português do Brasil, usa 2 a 3 emojis espalhados pelo texto e adora uma metáfora \
marinha. Regras de formato: texto simples (nada de markdown, títulos ou tabelas), no máximo \
8 linhas curtas. Estrutura: (1) como está a situação no período, dizendo com clareza se \
melhorou ou piorou frente aos meses anteriores e citando números em R$; (2) o maior gasto ou \
a maior mudança que você notou nos dados; (3) UMA dica prática e específica; (4) uma frase \
final de incentivo com uma zoeira leve. Não invente nenhum número que não esteja nos dados.";

fn analysis_prompt(names: &[String; 2], subject: u8, snapshot: &str) -> (String, String) {
    let who = if subject == COUPLE as u8 {
        "o casal (os dois leem esta análise)".to_string()
    } else {
        format!(
            "{} (apenas {} lê esta análise; os dados individuais do outro ficam de fora)",
            names[subject as usize],
            names[subject as usize]
        )
    };
    let system = format!("{PERSONA}\n\nPúblico desta análise: {who}.");
    let user = format!(
        "Dados do aplicativo:\n\n{snapshot}\n\nEscreva agora a análise {}.",
        if subject == COUPLE as u8 { "do casal" } else { "individual" }
    );
    (system, user)
}

const CHAT_RULES: &str = "\
Responda em português do Brasil, em até 6 linhas curtas, texto simples, com no máximo 1 emoji. \
Perguntas sobre os dados: use os números do resumo e seja específico. Perguntas fora das \
finanças: responda rápido e com bom humor, sem abandonar o seu papel. Se perguntarem sobre os \
gastos individuais de quem não está no resumo, explique que cada um só vê os próprios dados \
e os do casal.";

/// O que o modelo pode fazer no app e como deve agir (o executor aplica as mesmas regras das
/// rotas; executar é imediato, sem confirmação).
const CHAT_ACTIONS: &str = "\
Você também age no aplicativo pelas ferramentas (function calling): criar, editar, mover, \
marcar pago e excluir lançamentos, registrar e excluir acertos, definir a reserva, renomear \
membros, escolher o banco das conexões, ligar/desligar o rateio, copiar o mês anterior, \
definir orçamentos por categoria, fechar o mês e ajustar o fechamento de cartões. A execução \
é imediata, sem pedir confirmação. Antes de editar ou excluir, ache o id com \
buscar_lancamentos. Faça exatamente o que foi pedido, sem ações extras por conta própria; \
depois, confirme em uma frase o que fez, usando os números que as ferramentas devolveram — \
nunca invente valores. Se os dados mostrarem orçamento estourado, fatura fechando ou preço \
fora da média, avise.";

// ───────────────────────── Geração ─────────────────────────

/// Gera (ou reaproveita) as três análises do domicílio. Quem chama segura `Backend::assist`.
async fn generate_for(state: &Backend, home: &str) -> Result<(), ApiError> {
    let zai = state.zai.as_ref().ok_or_else(not_configured)?;
    let loaded = load(state, home).await?;
    let hash = loaded.hash();
    let db = state.db().await?;
    for subject in 0..=COUPLE {
        let existing = db
            .query_opt(
                "SELECT 1 FROM assistant_reports
                 WHERE household_id = $1::text::uuid AND subject = $2 AND period_start = $3::text::date
                     AND period_end = $4::text::date AND data_hash = $5 LIMIT 1",
                &[&home, &subject, &loaded.snap.period_start, &loaded.snap.period_end, &hash],
            )
            .await?
            .is_some();
        if existing {
            continue;
        }
        let (system, user) = analysis_prompt(&loaded.snap.names, subject as u8, &loaded.snap.render(subject as u8));
        let content = zai
            .chat(&system, &[api::ChatMsg { role: "user".into(), content: user, ops: Vec::new() }])
            .await?;
        db.execute(
            "INSERT INTO assistant_reports(household_id, subject, period_start, period_end, content, data_hash)
             VALUES ($1::text::uuid, $2, $3::text::date, $4::text::date, $5, $6)
             ON CONFLICT (household_id, subject, period_start, period_end, data_hash)
             DO UPDATE SET content = EXCLUDED.content, created_at = now()",
            &[&home, &subject, &loaded.snap.period_start, &loaded.snap.period_end, &content, &hash],
        )
        .await?;
    }
    Ok(())
}

/// Rotação em segundo plano: gera as análises de cada domicílio quando os dados mudaram ou
/// quando a última análise é de outro dia. Falhas não derrubam nada; só aparecem no log.
pub(super) fn spawn_daily(state: Arc<Backend>) {
    if state.config.zai_token.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut first = true;
        loop {
            // Primeira checagem em 10 minutos (não segura a inicialização); depois a cada 6 h.
            tokio::time::sleep(if first { Duration::from_secs(10 * 60) } else { Duration::from_secs(6 * 3600) }).await;
            first = false;
            let homes: Vec<String> = match state.db().await {
                Ok(db) => db
                    .query("SELECT id::text FROM households", &[])
                    .await
                    .map(|rows| rows.iter().map(|r| r.get(0)).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            for home in homes {
                if let Err(error) = generate_if_stale(&state, &home).await {
                    eprintln!("Assistente: geração adiada ({}).", error.1);
                }
            }
        }
    });
}

/// Regera quando não há análise do dia ou quando o hash dos dados mudou desde a última.
async fn generate_if_stale(state: &Backend, home: &str) -> Result<(), ApiError> {
    let hash = load(state, home).await?.hash();
    let db = state.db().await?;
    let stale = db
        .query_opt(
            "SELECT data_hash, created_at < date_trunc('day', now() AT TIME ZONE 'America/Sao_Paulo') AS old
                 FROM assistant_reports WHERE household_id = $1::text::uuid
                 ORDER BY created_at DESC LIMIT 1",
            &[&home],
        )
        .await?
        .map(|r| r.get::<_, String>(0) != hash || r.get::<_, bool>(1))
        .unwrap_or(true);
    drop(db);
    if stale {
        let _guard = state.assist.lock().await;
        generate_for(state, home).await?;
    }
    Ok(())
}

// ───────────────────────── Rotas ─────────────────────────

/// `GET /api/assistant/reports`: a análise individual de quem pergunta + a do casal.
pub(super) async fn get_reports(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<Json<api::AssistantReports>, ApiError> {
    let busy = state.assist.try_lock().is_err();
    if state.zai.is_none() {
        return Ok(Json(api::AssistantReports { enabled: false, busy, reports: Vec::new() }));
    }
    let db = state.db().await?;
    let me: i16 = db
        .query_one("SELECT position FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    let rows = db
        .query(
            &format!(
                "SELECT DISTINCT ON (subject) subject, content, to_char(created_at AT TIME ZONE 'UTC', {UTC}),
                        to_char(period_start, 'YYYY-MM-DD'), to_char(period_end, 'YYYY-MM-DD')
                 FROM assistant_reports WHERE household_id = {HOUSEHOLD} AND subject IN ($2::smallint, {COUPLE}::smallint)
                 ORDER BY subject, created_at DESC"
            ),
            &[&user.0, &me],
        )
        .await?;
    let reports = rows
        .iter()
        .map(|r| api::AssistantReport {
            subject: r.get::<_, i16>(0) as u8,
            content: r.get(1),
            generated_at: r.get(2),
            period_start: r.get(3),
            period_end: r.get(4),
        })
        .collect();
    Ok(Json(api::AssistantReports { enabled: true, busy, reports }))
}

/// `POST /api/assistant/refresh` → 202: gera as análises em segundo plano.
pub(super) async fn post_refresh(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<StatusCode, ApiError> {
    if state.zai.is_none() {
        return Err(not_configured());
    }
    let home: String = state
        .db()
        .await?
        .query_one("SELECT household_id::text FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    let task = state.clone();
    tokio::spawn(async move {
        let _guard = task.assist.lock().await;
        if let Err(error) = generate_for(&task, &home).await {
            eprintln!("Assistente: falhou ao gerar análises ({}).", error.1);
        }
    });
    Ok(StatusCode::ACCEPTED)
}

/// `GET /api/assistant/history`: a conversa persistida de quem pede (mais antigas primeiro).
pub(super) async fn get_history(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<Json<api::ChatHistory>, ApiError> {
    let db = state.db().await?;
    let me: i16 = db
        .query_one("SELECT position FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    Ok(Json(db::chat_history(&db, user, me).await?))
}

/// `DELETE /api/assistant/history` → 204: apaga a conversa de quem pede (só a dele).
pub(super) async fn delete_history(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<StatusCode, ApiError> {
    let db = state.db().await?;
    let me: i16 = db
        .query_one("SELECT position FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    db::clear_chat_history(&db, user, me).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/assistant/actions`: auditoria das ações do assistente de quem pede, mais
/// recentes primeiro.
pub(super) async fn get_actions(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
) -> Result<Json<api::ActionLog>, ApiError> {
    let db = state.db().await?;
    let me: i16 = db
        .query_one("SELECT position FROM members WHERE client_user_id = $1", &[&user.0])
        .await?
        .get(0);
    Ok(Json(db::assistant_actions(&db, user, me).await?))
}

/// Máximo de rodadas de ferramentas numa conversa; depois, o modelo responde em texto.
const MAX_TOOL_ROUNDS: usize = 6;

/// `POST /api/assistant/chat`: conversa livre com o Flounder. O contexto leva só os dados de
/// quem pergunta e os do casal. O modelo pode agir no app pelas ferramentas: cada rodada com
/// `tool_calls` é executada na hora e devolvida como mensagem `tool`, até o texto final — e as
/// ações realizadas voltam como `ops` para o selo no chat.
pub(super) async fn post_chat(
    State(state): State<Arc<Backend>>,
    Extension(user): Extension<User>,
    Json(input): Json<api::ChatRequest>,
) -> Result<Json<api::ChatReply>, ApiError> {
    let Some(zai) = &state.zai else { return Err(not_configured()) };
    let messages: Vec<api::ChatMsg> = input
        .messages
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .collect();
    if messages.last().map(|m| m.role.as_str()) != Some("user") {
        return Err(bad("A última mensagem do histórico deve ser sua."));
    }
    let messages: Vec<api::ChatMsg> = messages
        .into_iter()
        .rev()
        .take(MAX_CHAT_MESSAGES)
        .map(|mut m| {
            m.content = m.content.chars().take(MAX_CHAT_CHARS).collect();
            m
        })
        .rev()
        .collect();
    let db = state.db().await?;
    let (me, home): (i16, String) = {
        let row = db
            .query_one("SELECT position, household_id::text FROM members WHERE client_user_id = $1", &[&user.0])
            .await
            .map_err(|_| super::db::not_member())?;
        (row.get(0), row.get(1))
    };
    // A conversa persiste: a mensagem de quem fala entra antes do modelo (vale mesmo se ele
    // falhar); a resposta final, com as ações, entra no fim. Falha de gravação não derruba o papo.
    if let Some(m) = messages.last() {
        if let Err(error) = db::store_chat_message(&db, user, me, &m.role, &m.content, &[]).await {
            eprintln!("Assistente: falhou ao gravar a conversa ({}).", error.1);
        }
    }
    drop(db);
    let loaded = load(&state, &home).await?;
    let other = (1 - me) as usize;
    let system = format!(
        "{PERSONA}\n\n{CHAT_RULES}\n\n{CHAT_ACTIONS}\n\nQuem está conversando com você: {} (a conversa e os dados \
         são dele/dela e do casal; nunca revele ou estime números individuais de {}). Quem usa \
         o app agora está na posição {me}.\n\nDados atuais:\n{}",
        loaded.snap.names[me as usize],
        loaded.snap.names[other],
        loaded.snap.render(me as u8)
    );
    let mut convo: Vec<Value> = vec![json!({"role": "system", "content": system})];
    convo.extend(messages.iter().map(|m| json!({"role": m.role, "content": m.content})));
    let me = me as u8;
    let mut ops: Vec<api::Op> = Vec::new();
    let mut rounds = 0;
    let content = loop {
        // Estourou o limite de ferramentas: sem `tools`, com o lembrete de responder em texto.
        let sem_ferramentas = rounds >= MAX_TOOL_ROUNDS;
        if sem_ferramentas {
            convo.push(json!({"role": "system", "content": "Você já usou ferramentas o bastante nesta rodada. \
                Responda AGORA em texto simples o que conseguiu fazer, sem chamar ferramentas."}));
        }
        let tools = (!sem_ferramentas).then(tools::schemas);
        let message = zai.round(&convo, tools.as_ref()).await?;
        if sem_ferramentas || message.tool_calls.is_empty() {
            let content = clean_text(&message.content);
            if content.is_empty() {
                return Err(ai_down());
            }
            break content;
        }
        convo.push(json!({
            "role": "assistant",
            "content": message.content,
            "tool_calls": message.tool_calls.iter().map(|c| json!({
                "id": c.id, "type": "function",
                "function": {"name": c.name, "arguments": c.arguments},
            })).collect::<Vec<_>>(),
        }));
        for call in &message.tool_calls {
            let resultado = match executar(&state, user, me, call).await {
                Ok(outcome) => {
                    ops.extend(outcome.ops);
                    outcome.result
                }
                // O erro vai ao modelo como JSON, para reagir em vez de derrubar a conversa.
                Err(erro) => json!({"erro": erro}),
            };
            convo.push(json!({"role": "tool", "tool_call_id": call.id, "content": resultado.to_string()}));
        }
        rounds += 1;
    };
    // Resposta final da rodada, com as ações executadas, entra na conversa persistida.
    {
        let db = state.db().await?;
        if let Err(error) = db::store_chat_message(&db, user, me as i16, "assistant", &content, &ops).await {
            eprintln!("Assistente: falhou ao gravar a conversa ({}).", error.1);
        }
    }
    Ok(Json(api::ChatReply { content, ops }))
}

/// Executa uma ferramenta pedida pelo modelo, com as regras das rotas e a posição de quem
/// conversa (`me` define o dono de lançamentos novos e quem paga os acertos). Cada ação
/// executada entra na auditoria do membro; falha de auditoria não derruba a conversa.
async fn executar(state: &Backend, user: User, me: u8, call: &ToolCall) -> Result<tools::Outcome, String> {
    let args: Value = serde_json::from_str(&call.arguments)
        .map_err(|_| "Os argumentos da ferramenta não são um JSON válido.".to_string())?;
    let args = if args.is_null() { json!({}) } else { args };
    if !args.is_object() {
        return Err("Os argumentos da ferramenta devem ser um objeto JSON.".into());
    }
    let db = state.db().await.map_err(|e| e.1.to_owned())?;
    let outcome = tools::run(&db, user, me, &call.name, &args).await?;
    for op in &outcome.ops {
        if let Err(error) = db::record_action(&db, user, me as i16, &op.tool, &op.label, &args).await {
            eprintln!("Assistente: falhou ao registrar a ação ({}).", error.1);
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, amount: i64, kind: &str, category: &str, shared: bool, owner: u8, desc: &str, paid: bool, recurring: bool) -> Row {
        Row { date: date.into(), amount, kind: kind.into(), category: category.into(), shared, owner, desc: desc.into(), paid, recurring, card: None }
    }

    fn card_row(date: &str, amount: i64, owner: u8, desc: &str, card: &str) -> Row {
        Row { date: date.into(), amount, kind: "expense".into(), category: "shopping".into(), shared: false, owner, desc: desc.into(), paid: false, recurring: false, card: Some(card.into()) }
    }

    fn card(name: &str, owner: u8, closing_day: Option<u32>, bills: Vec<connected::ConnectedBill>) -> SnapCard {
        SnapCard { external_id: format!("card-{owner}"), name: name.into(), owner, closing_day, bills, forming: None }
    }

    fn bill(due: &str, closing: &str) -> connected::ConnectedBill {
        connected::ConnectedBill { due: due.into(), closing: Some(closing.into()), total: Some(12_345), currency: "BRL".into(), paid: false }
    }

    #[test]
    fn snapshot_sums_income_expenses_and_parcelas() {
        let rows = vec![
            row("2026-08-05", 3_000_000, "income", "salary", false, 0, "Salário", true, false),
            row("2026-08-05", 1_000_000, "income", "salary", false, 1, "Salário", true, false),
            row("2026-08-10", 1_000_000, "expense", "housing", true, 0, "Aluguel", true, true),
            row("2026-08-12", 300_000, "expense", "groceries", false, 1, "Mercado", true, false),
            row("2026-08-14", 90_000, "expense", "leisure", false, 0, "TV (4/10)", true, false),
            // A divisão usa a janela de 3 meses do núcleo (set–nov): o salário de agosto
            // fica de fora e o rateio volta a 50/50.
            row("2026-09-05", 1_000_000, "income", "salary", false, 0, "Salário", true, false),
            row("2026-09-05", 1_000_000, "income", "salary", false, 1, "Salário", true, false),
        ];
        let snap = build_snapshot("2026-08-01", "2026-11-12", vec!["Ana".into(), "Bia".into()], false, &rows, vec![], "2026-11-12", Vec::new(), Vec::new());
        assert_eq!(snap.income.get(&(0, "2026-08".into())), Some(&3_000_000));
        assert_eq!(snap.expense.get(&(1, "2026-08".into())), Some(&300_000));
        assert_eq!(snap.shared_paid.get(&(0, "2026-08".into())), Some(&1_000_000));
        assert_eq!(snap.personal.get(&(1, "2026-08".into(), "groceries".to_string())), Some(&300_000));
        // Parcela e conta fixa detectadas.
        assert!(snap.installments.iter().any(|(d, n, of, _, _)| d == "TV" && *n == 4 && *of == 10));
        assert!(snap.recurring.iter().any(|(d, _, _, _)| d == "Aluguel"));
        let text = snap.render(2);
        assert!(text.contains("PERÍODO: 01/08/2026 a 12/11/2026"));
        assert!(text.contains("DIVISÃO DA RENDA: Ana 50% / Bia 50%"));
        assert!(text.contains("TV (4/10), faltam 6 de R$ 900,00 (Ana)"));
        assert!(text.contains("Aluguel (Moradia, média R$ 10.000,00)"));
        // Sem cartões, orçamentos ou preço fora da média, as seções novas não aparecem.
        assert!(!text.contains("CARTÕES") && !text.contains("ORÇAMENTOS") && !text.contains("PREÇOS"));
        // A análise individual não vê as despesas pessoais do outro.
        let mine = snap.render(0);
        assert!(mine.contains("Análise INDIVIDUAL de Ana"));
        assert!(!mine.contains("Mercado"));
        assert!(snap.render(1).contains("Mercado"));
    }

    #[test]
    fn parcela_parser_matches_the_desktop_rules() {
        assert_eq!(split_parcela("Loja (2/10)"), Some(("Loja".into(), 2, 10)));
        assert_eq!(split_parcela("Loja 3/4"), Some(("Loja".into(), 3, 4)));
        assert_eq!(split_parcela("Mês único 1/1"), None);
        assert_eq!(split_parcela("Sem parcela"), None);
    }

    #[test]
    fn snapshot_warns_about_cards_budgets_and_price_changes() {
        let rows = vec![
            // Luz em conta fixa: a conta de novembro subiu 58% sobre a média (5.000 + 19.000).
            row("2026-09-05", 5_000, "expense", "utilities", false, 0, "Luz", true, true),
            row("2026-11-05", 19_000, "expense", "utilities", false, 0, "Luz", true, true),
            // Compra no cartão conectado do Felipe, dentro da fatura em formação.
            card_row("2026-11-01", 84_000, 0, "Loja", "card-0"),
            // Lanche de Felipe no orçamento do mercado (R$ 240 de R$ 300).
            row("2026-11-06", 24_000, "expense", "groceries", false, 0, "Lanche", true, false),
        ];
        let cards = vec![card("Nubank", 0, Some(20), vec![bill("2026-11-25", "2026-11-20")])];
        let budgets = vec![("groceries".to_owned(), 30_000)];
        let snap = build_snapshot("2026-08-01", "2026-11-12", vec!["Ana".into(), "Bia".into()], false, &rows, vec![], "2026-11-12", budgets, cards.clone());
        let ana = snap.render(0);
        // Fechamento em 8 dias, fatura em formação com a compra do cartão e a fatura do mês.
        assert!(ana.contains("CARTÕES: Nubank: fecha dia 20 (em 8 dias) · fatura em formação R$ 840,00 · fatura de nov vence 25/11"), "{ana}");
        assert!(ana.contains("ORÇAMENTOS: Mercado: R$ 240,00 de R$ 300,00 (80%)"), "{ana}");
        assert!(ana.contains("PREÇOS: Luz: R$ 190,00 vs média R$ 120,00"), "{ana}");
        // No dia do fechamento o aviso vira "(hoje)".
        let hoje = build_snapshot("2026-08-01", "2026-11-20", vec!["Ana".into(), "Bia".into()], false, &rows, vec![], "2026-11-20", Vec::new(), cards.clone());
        assert!(hoje.render(0).contains("fecha dia 20 (hoje)"), "{}", hoje.render(0));
        // Bia não vê o cartão do Felipe (privacidade) e o orçamento fica zerado para ela;
        // os avisos de conta fixa (como as contas fixas) são do domicílio.
        let bia = snap.render(1);
        assert!(!bia.contains("CARTÕES"), "{bia}");
        assert!(bia.contains("ORÇAMENTOS: Mercado: R$ 0,00 de R$ 300,00 (0%)"), "{bia}");
        assert!(bia.contains("PREÇOS: Luz: R$ 190,00 vs média R$ 120,00"), "{bia}");
        // O casal vê os dois cartões.
        let couple = snap.render(2);
        assert!(couple.contains("CARTÕES: Nubank:"), "{couple}");
        // Despesa compartilhada entra na usage do orçamento de quem lê e do casal.
        let rows = vec![
            card_row("2026-11-01", 84_000, 0, "Loja", "card-0"),
            row("2026-11-02", 60_000, "expense", "groceries", true, 1, "Feira", true, false),
        ];
        let snap = build_snapshot("2026-08-01", "2026-11-12", vec!["Ana".into(), "Bia".into()], false, &rows, vec![], "2026-11-12", vec![("groceries".to_owned(), 50_000)], cards.clone());
        assert!(snap.render(0).contains("ORÇAMENTOS: Mercado: R$ 600,00 de R$ 500,00 (120%)"), "{}", snap.render(0));
        assert!(snap.render(2).contains("ORÇAMENTOS: Mercado: R$ 600,00 de R$ 500,00 (120%)"), "{}", snap.render(2));
    }

    #[test]
    fn card_without_closing_or_bills_and_empty_sections_stay_out_of_the_prompt() {
        let rows = vec![row("2026-11-06", 24_000, "expense", "groceries", false, 0, "Lanche", true, false)];
        // Sem fechamento e sem faturas: o cartão não tem nada a dizer e some.
        let snap = build_snapshot("2026-08-01", "2026-11-12", vec!["Ana".into(), "Bia".into()], false, &rows, vec![], "2026-11-12", Vec::new(), vec![card("Inter", 0, None, Vec::new())]);
        assert!(!snap.render(0).contains("CARTÕES"), "{}", snap.render(0));
    }

    #[test]
    fn money_and_dates_in_brazilian_format() {
        assert_eq!(brl(1_234_567), "R$ 12.345,67");
        assert_eq!(brl(-250), "-R$ 2,50");
        assert_eq!(date_br("2026-11-12"), "12/11/2026");
        assert_eq!(month_label("2026-08"), "ago/26");
        assert_eq!(clean_text("oi\n**negrito**\t\u{0} fim "), "oi\nnegrito fim");
    }
}
