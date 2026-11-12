//! Assistente financeiro: análises periódicas e conversa com o GLM (Z.ai), na voz do Flounder.
//! O token vive só no backend (`ZAI_TOKEN`); a interface conversa por estas rotas, e a
//! privacidade é garantida aqui dentro: cada membro recebe a própria análise e a do casal,
//! nunca a individual do outro.

use super::db::{bad, HOUSEHOLD, UTC};
use super::*;
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

    /// Uma resposta: o sistema define a persona e traz os dados; `messages` é o histórico.
    async fn chat(&self, system: &str, messages: &[api::ChatMsg]) -> Result<String, ApiError> {
        let mut body = Vec::with_capacity(messages.len() + 1);
        body.push(json!({"role": "system", "content": system}));
        for m in messages {
            body.push(json!({"role": m.role, "content": m.content}));
        }
        let value: Value = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.token)
            .json(&json!({
                "model": self.model,
                "messages": body,
                "temperature": 0.8,
                "max_tokens": 4096,
                "stream": false,
            }))
            .send()
            .await
            .map_err(|_| ai_down())?
            .error_for_status()
            .map_err(|err| ai_status(err.status()))?
            .json()
            .await
            .map_err(|_| ai_down())?;
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(ai_down)?;
        Ok(clean_text(content))
    }
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

/// "2026-11-12" → "12/11/2026".
fn date_br(iso: &str) -> String {
    match (iso.get(8..10), iso.get(5..7), iso.get(0..4)) {
        (Some(d), Some(m), Some(y)) => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

// ───────────────────────── Dados para o modelo ─────────────────────────

struct Row {
    date: String,
    amount: i64,
    kind: String,
    category: String,
    shared: bool,
    owner: u8,
    desc: String,
    paid: bool,
    recurring: bool,
}

/// Tudo que os prompts recebem, já agregado. Chaves de mês são "AAAA-MM".
struct Snapshot {
    period_start: String,
    period_end: String,
    months: Vec<String>,
    names: [String; 2],
    shared_split_off: bool,
    income: BTreeMap<(u8, String), i64>,
    expense: BTreeMap<(u8, String), i64>,
    shared_paid: BTreeMap<(u8, String), i64>,
    /// Despesas pessoais (não compartilhadas) por (dono, mês, categoria).
    personal: BTreeMap<(u8, String, String), i64>,
    /// Aportes pagos na reserva, por mês (domicílio).
    reserve: BTreeMap<String, i64>,
    /// (data, valor, posição de quem pagou).
    settlements: Vec<(String, i64, u8)>,
    /// Parcelas lançadas no período: (descrição, atual, total, valor da parcela, dono).
    installments: Vec<(String, u32, u32, i64, u8)>,
    /// Contas fixas com média do período: (descrição, categoria, média, dono).
    recurring: Vec<(String, String, i64, u8)>,
}

/// "Loja (2/10)" → ("Loja", 2, 10); as mesmas regras da interface para agrupar parcelas.
fn split_parcela(desc: &str) -> Option<(String, u32, u32)> {
    let token = desc
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .rfind(|t| !t.is_empty())?;
    let (n, of) = token.split_once('/')?;
    let (n, of): (u32, u32) = (n.parse().ok()?, of.parse().ok()?);
    if !(2..=999).contains(&of) || n == 0 || n > of {
        return None;
    }
    let base = desc[..desc.rfind(token)?]
        .trim_end_matches(|c: char| c.is_whitespace() || c == '(' || c == '-')
        .trim()
        .to_string();
    (!base.is_empty()).then_some((base, n, of))
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
    /// Proporção da renda de cada um no período inteiro.
    fn income_weights(&self) -> Option<[f64; 2]> {
        let total: i64 = self.income.values().sum();
        (total > 0).then(|| {
            let a: i64 = self.income.iter().filter(|((o, _), _)| *o == 0).map(|(_, v)| v).sum();
            [a as f64 / total as f64, 1.0 - a as f64 / total as f64]
        })
    }

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
            self.income_weights().map_or("sem renda registrada (rateio 50/50)".into(), |[wa, wb]| format!(
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
                    m.position, e.description, e.paid, e.recurring
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
    let snap = build_snapshot(&period_start, &period_end, names, shared_split_off, &rows, settlements);
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
        months,
        names: [names.first().cloned().unwrap_or_default(), names.get(1).cloned().unwrap_or_default()],
        shared_split_off,
        income: BTreeMap::new(),
        expense: BTreeMap::new(),
        shared_paid: BTreeMap::new(),
        personal: BTreeMap::new(),
        reserve: BTreeMap::new(),
        settlements,
        installments: Vec::new(),
        recurring: Vec::new(),
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
        } else {
            *snap.personal.entry((r.owner, m.clone(), r.category.clone())).or_default() += r.amount;
            if r.category == "reserve" && r.paid {
                *snap.reserve.entry(m.clone()).or_default() += r.amount;
            }
        }
        if let Some((base, n, of)) = split_parcela(&r.desc) {
            snap.installments.push((base, n, of, r.amount, r.owner));
        }
        if r.recurring {
            snap.recurring.push((r.desc.clone(), r.category.clone(), r.amount, r.owner));
        }
    }
    // Média das contas fixas: as últimas 3 ocorrências por descrição.
    let mut defs: BTreeMap<(String, String, u8), Vec<i64>> = BTreeMap::new();
    for (desc, category, amount, owner) in snap.recurring.drain(..) {
        defs.entry((desc, category, owner)).or_default().push(amount);
    }
    snap.recurring = defs
        .into_iter()
        .map(|((desc, category, owner), mut vals)| {
            vals.sort_unstable();
            let avg = vals.iter().rev().take(3).sum::<i64>() / vals.len().clamp(1, 3) as i64;
            (desc, category, avg, owner)
        })
        .collect();
    snap
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
            .chat(&system, &[api::ChatMsg { role: "user".into(), content: user }])
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

/// `POST /api/assistant/chat`: conversa livre com o Flounder. O contexto leva só os dados de
/// quem pergunta e os do casal.
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
    drop(db);
    let loaded = load(&state, &home).await?;
    let other = (1 - me) as usize;
    let system = format!(
        "{PERSONA}\n\n{CHAT_RULES}\n\nQuem está conversando com você: {} (a conversa e os dados \
         são dele/dela e do casal; nunca revele ou estime números individuais de {}). Quem usa \
         o app agora está na posição {me}.\n\nDados atuais:\n{}",
        loaded.snap.names[me as usize],
        loaded.snap.names[other],
        loaded.snap.render(me as u8)
    );
    let content = zai.chat(&system, &messages).await?;
    Ok(Json(api::ChatReply { content }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, amount: i64, kind: &str, category: &str, shared: bool, owner: u8, desc: &str, paid: bool, recurring: bool) -> Row {
        Row { date: date.into(), amount, kind: kind.into(), category: category.into(), shared, owner, desc: desc.into(), paid, recurring }
    }

    #[test]
    fn snapshot_sums_income_expenses_and_parcelas() {
        let rows = vec![
            row("2026-08-05", 3_000_000, "income", "salary", false, 0, "Salário", true, false),
            row("2026-08-05", 1_000_000, "income", "salary", false, 1, "Salário", true, false),
            row("2026-08-10", 1_000_000, "expense", "housing", true, 0, "Aluguel", true, true),
            row("2026-08-12", 300_000, "expense", "groceries", false, 1, "Mercado", true, false),
            row("2026-08-14", 90_000, "expense", "leisure", false, 0, "TV (4/10)", true, false),
        ];
        let snap = build_snapshot("2026-08-01", "2026-11-12", vec!["Ana".into(), "Bia".into()], false, &rows, vec![]);
        assert_eq!(snap.income.get(&(0, "2026-08".into())), Some(&3_000_000));
        assert_eq!(snap.expense.get(&(1, "2026-08".into())), Some(&300_000));
        assert_eq!(snap.shared_paid.get(&(0, "2026-08".into())), Some(&1_000_000));
        assert_eq!(snap.personal.get(&(1, "2026-08".into(), "groceries".to_string())), Some(&300_000));
        // Parcela e conta fixa detectadas.
        assert!(snap.installments.iter().any(|(d, n, of, _, _)| d == "TV" && *n == 4 && *of == 10));
        assert!(snap.recurring.iter().any(|(d, _, _, _)| d == "Aluguel"));
        let text = snap.render(2);
        assert!(text.contains("PERÍODO: 01/08/2026 a 12/11/2026"));
        assert!(text.contains("DIVISÃO DA RENDA: Ana 75% / Bia 25%"));
        assert!(text.contains("TV (4/10), faltam 6 de R$ 900,00 (Ana)"));
        assert!(text.contains("Aluguel (Moradia, média R$ 10.000,00)"));
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
    fn money_and_dates_in_brazilian_format() {
        assert_eq!(brl(1_234_567), "R$ 12.345,67");
        assert_eq!(brl(-250), "-R$ 2,50");
        assert_eq!(date_br("2026-11-12"), "12/11/2026");
        assert_eq!(month_label("2026-08"), "ago/26");
        assert_eq!(clean_text("oi\n**negrito**\t\u{0} fim "), "oi\nnegrito fim");
    }
}
