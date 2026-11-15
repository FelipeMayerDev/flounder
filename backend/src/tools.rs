//! Ferramentas do assistente: o GLM pede uma ação e este módulo executa com as MESMAS regras
//! (validação e SQL) das rotas do aplicativo — nada de caminho próprio no banco. Cada execução
//! devolve o resultado em JSON para o modelo reagir e uma etiqueta curta em português para o
//! selo de ação no chat. Privacidade: `buscar_lancamentos` só traz os lançamentos de quem
//! conversa e os compartilhados; o restante segue as regras das rotas, como a interface.

use super::db::{self, ENTRY, HOUSEHOLD};
use super::*;
use flounder_core::model::{Month, money};
use tokio_postgres::Client;

/// Limite de lançamentos que `buscar_lancamentos` devolve numa chamada.
const MAX_BUSCA: i64 = 50;

/// Esquemas das ferramentas no formato OpenAI (`tools` do Z.ai): descrições em português,
/// porque o modelo é instruído todo em português.
pub(super) fn schemas() -> Value {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": {"type": "object", "properties": properties, "required": required},
            },
        })
    };
    Value::Array(vec![
        tool(
            "buscar_lancamentos",
            "Busca lançamentos do domicílio que você pode ver: os de quem conversa com você e os compartilhados do casal — nunca os pessoais do parceiro. Use SEMPRE antes de editar ou excluir, para achar os ids. Filtros opcionais: mês (AAAA-MM), texto na descrição, categoria (salary, housing, utilities, groceries, transport, health, leisure, reserve, family, shopping, services, education, pets, other) e limite (padrão 20, máximo 50).",
            json!({
                "mes": {"type": "string", "description": "Filtro pelo mês, no formato AAAA-MM (ex.: 2026-11). Opcional."},
                "texto": {"type": "string", "description": "Texto que deve aparecer na descrição. Opcional."},
                "categoria": {"type": "string", "description": "Chave da categoria do app. Opcional."},
                "limite": {"type": "integer", "description": "Quantos resultados trazer, de 1 a 50. Padrão 20."},
            }),
            &[],
        ),
        tool(
            "criar_lancamento",
            "Cria um lançamento (despesa ou receita) no nome de quem está conversando com você. valor_centavos é em centavos (R$ 10,00 = 1000). data é AAAA-MM-DD. tipo é income (receita) ou expense (despesa). categoria é a chave do app. compartilhado só vale para despesa (conta do casal). cartao_id é o id externo de um cartão conectado, só para despesa (a conta dele é a fatura). fixa marca conta que repete todo mês.",
            json!({
                "descricao": {"type": "string", "description": "Descrição curta do lançamento (até 200 caracteres)."},
                "valor_centavos": {"type": "integer", "description": "Valor em centavos, sempre >= 0 (R$ 1.200,00 = 120000)."},
                "data": {"type": "string", "description": "Data do lançamento, AAAA-MM-DD."},
                "tipo": {"type": "string", "enum": ["income", "expense"], "description": "income = receita, expense = despesa."},
                "categoria": {"type": "string", "description": "Chave da categoria: salary, housing, utilities, groceries, transport, health, leisure, reserve, family, shopping, services, education, pets ou other."},
                "compartilhado": {"type": "boolean", "description": "Despesa do casal? Padrão: não."},
                "pago": {"type": "boolean", "description": "Já pago? Padrão: não."},
                "cartao_id": {"type": "string", "description": "Id do cartão conectado (compra no cartão). Opcional, só despesa."},
                "fixa": {"type": "boolean", "description": "Conta fixa que repete todo mês? Padrão: não."},
            }),
            &["descricao", "valor_centavos", "data", "tipo", "categoria"],
        ),
        tool(
            "editar_lancamento",
            "Edita um lançamento pelo id (ache antes com buscar_lancamentos). Só os campos enviados mudam; mover para outro dia é mudar a data. Funciona também em lançamentos importados do banco/cartão, com as mesmas regras da interface.",
            json!({
                "id": {"type": "string", "description": "Id (uuid) do lançamento."},
                "descricao": {"type": "string", "description": "Nova descrição. Opcional."},
                "valor_centavos": {"type": "integer", "description": "Novo valor em centavos. Opcional."},
                "data": {"type": "string", "description": "Nova data AAAA-MM-DD (é assim que se move um lançamento). Opcional."},
                "categoria": {"type": "string", "description": "Nova chave de categoria. Opcional."},
                "compartilhado": {"type": "boolean", "description": "Tornar (ou deixar de ser) despesa do casal. Opcional."},
                "pago": {"type": "boolean", "description": "Marcar ou desmarcar como pago. Opcional."},
                "cartao_id": {"type": "string", "description": "Id do cartão conectado; null tira do cartão. Opcional."},
                "fixa": {"type": "boolean", "description": "Marcar ou desmarcar como conta fixa. Opcional."},
            }),
            &["id"],
        ),
        tool(
            "excluir_lancamento",
            "Exclui um lançamento pelo id (ache antes com buscar_lancamentos). Excluir um importado faz a transação não voltar nas próximas importações.",
            json!({
                "id": {"type": "string", "description": "Id (uuid) do lançamento."},
            }),
            &["id"],
        ),
        tool(
            "marcar_pago",
            "Marca como pago (ou volta para não pago): um lançamento pelo id, OU a fatura inteira de um cartão conectado, com fatura = {cartao_id, due}. Informe UM dos dois, nunca os dois juntos.",
            json!({
                "id": {"type": "string", "description": "Id (uuid) do lançamento, para marcar só ele. Opcional."},
                "fatura": {
                    "type": "object",
                    "description": "Para marcar a fatura de um cartão conectado. Opcional.",
                    "properties": {
                        "cartao_id": {"type": "string", "description": "Id do cartão conectado."},
                        "due": {"type": "string", "description": "Vencimento da fatura, AAAA-MM-DD."},
                    },
                },
                "pago": {"type": "boolean", "description": "true = pago, false = não pago."},
            }),
            &["pago"],
        ),
        tool(
            "criar_acerto",
            "Registra um acerto entre o casal: a transferência que alguém fez para equilibrar a divisão da casa. de é a posição de quem pagou (0 ou 1; padrão: quem está conversando); o outro membro recebe.",
            json!({
                "valor_centavos": {"type": "integer", "description": "Valor em centavos, maior que zero."},
                "data": {"type": "string", "description": "Data da transferência, AAAA-MM-DD."},
                "de": {"type": "integer", "enum": [0, 1], "description": "Posição de quem pagou. Padrão: quem conversa."},
            }),
            &["valor_centavos", "data"],
        ),
        tool(
            "excluir_acerto",
            "Exclui um acerto pelo id (uuid).",
            json!({
                "id": {"type": "string", "description": "Id (uuid) do acerto."},
            }),
            &["id"],
        ),
        tool(
            "definir_reserva",
            "Substitui a meta única da reserva do domicílio. meta_centavos é o objetivo; abertura_centavos é o saldo inicial já guardado (padrão 0) e nome é o título da meta (padrão \"Reserva\").",
            json!({
                "meta_centavos": {"type": "integer", "description": "Valor alvo em centavos (R$ 10.000,00 = 1000000)."},
                "abertura_centavos": {"type": "integer", "description": "Saldo inicial já guardado, em centavos. Padrão 0."},
                "nome": {"type": "string", "description": "Título da meta (até 60 caracteres). Padrão \"Reserva\"."},
            }),
            &["meta_centavos"],
        ),
        tool(
            "renomear_membro",
            "Renomeia um dos dois membros do domicílio pela posição (0 ou 1), com nome de até 40 caracteres.",
            json!({
                "posicao": {"type": "integer", "enum": [0, 1], "description": "Posição do membro: 0 ou 1."},
                "nome": {"type": "string", "description": "Novo nome (até 40 caracteres)."},
            }),
            &["posicao", "nome"],
        ),
        tool(
            "definir_instituicao_conexao",
            "Define ou limpa o banco de uma conexão Pluggy (cartões e contas dela passam a usar o banco escolhido). chave_instituicao é a chave da instituição (ex.: nubank, inter) ou null para limpar a escolha.",
            json!({
                "item_id": {"type": "string", "description": "Id do item (conexão) na Pluggy."},
                "chave_instituicao": {"type": ["string", "null"], "description": "Chave da instituição, ou null para limpar."},
            }),
            &["item_id"],
        ),
        tool(
            "alternar_rateio",
            "Liga ou desliga, para o domicílio inteiro, o rateio das despesas compartilhadas pela renda. desligar=true: cada despesa compartilhada pesa por inteiro para quem pagou (sem rateio). desligar=false: o rateio pela renda volta.",
            json!({
                "desligar": {"type": "boolean", "description": "true desliga o rateio, false liga de volta."},
            }),
            &["desligar"],
        ),
        tool(
            "copiar_mes_anterior",
            "Copia os lançamentos manuais (sem cartão) do mês anterior para o mês informado, como o botão \"Copiar do mês anterior\" do app: mesmos dias, valores e donos, já como não pagos. Os que já existirem no mês alvo (mesma descrição, valor e dono) são pulados.",
            json!({
                "mes": {"type": "string", "description": "Mês alvo da cópia, no formato AAAA-MM."},
            }),
            &["mes"],
        ),
        tool(
            "definir_dia_fechamento",
            "Ajusta o dia de fechamento da fatura de um cartão conectado: dia entre 1 e 31, ou null para voltar ao dia que vem do banco.",
            json!({
                "cartao_id": {"type": "string", "description": "Id do cartão conectado."},
                "dia": {"type": ["integer", "null"], "description": "Dia do fechamento (1-31), ou null para limpar o ajuste."},
            }),
            &["cartao_id"],
        ),
        tool(
            "definir_orcamento",
            "Define o orçamento mensal do domicílio (dos dois) para uma categoria de despesa: monthly_cents em centavos por mês. null, 0 ou negativo remove o orçamento da categoria.",
            json!({
                "categoria": {"type": "string", "description": "Chave da categoria: housing, utilities, groceries, transport, health, leisure, reserve, family, shopping, services, education, pets ou other."},
                "monthly_cents": {"type": ["integer", "null"], "description": "Limite mensal em centavos (R$ 300,00 = 30000). null ou <= 0 remove."},
            }),
            &["categoria"],
        ),
        tool(
            "fechar_mes",
            "Fecha o mês (o botão \"Fechar o mês\" do app): copia os lançamentos manuais do mês anterior para o mês informado (padrão: o mês atual), lista as faturas que vencem, sugere o acerto do casal, aponta contas fixas fora da média e os próximos fechamentos de cartão. O relatório completo vem no resultado.",
            json!({
                "mes": {"type": "string", "description": "Mês alvo, no formato AAAA-MM. Opcional: sem ele, fecha o mês atual."},
            }),
            &[],
        ),
    ])
}

/// O que uma ferramenta devolve: o JSON para o modelo e as etiquetas dos selos no chat
/// (na maioria, uma só; `fechar_mes` traz uma por cópia mais o resumo).
pub(super) struct Outcome {
    pub result: Value,
    pub ops: Vec<api::Op>,
}

impl Outcome {
    fn new(tool: &str, label: String, result: Value) -> Outcome {
        Outcome { result, ops: vec![api::Op { tool: tool.to_owned(), label }] }
    }
}

/// Executa a ferramenta pedida pelo modelo, com a conexão pronta e a posição de quem conversa
/// (`me`). Erros voltam como texto em português, para o modelo se corrigir na conversa.
pub(super) async fn run(db: &Client, user: User, me: u8, name: &str, args: &Value) -> Result<Outcome, String> {
    match name {
        "buscar_lancamentos" => buscar_lancamentos(db, user, me, args).await,
        "criar_lancamento" => criar_lancamento(db, user, me, args).await,
        "editar_lancamento" => editar_lancamento(db, user, args).await,
        "excluir_lancamento" => excluir_lancamento(db, user, args).await,
        "marcar_pago" => marcar_pago(db, user, args).await,
        "criar_acerto" => criar_acerto(db, user, me, args).await,
        "excluir_acerto" => excluir_acerto(db, user, args).await,
        "definir_reserva" => definir_reserva(db, user, args).await,
        "renomear_membro" => renomear_membro(db, user, args).await,
        "definir_instituicao_conexao" => definir_instituicao(db, user, args).await,
        "alternar_rateio" => alternar_rateio(db, user, args).await,
        "copiar_mes_anterior" => copiar_mes_anterior(db, user, args).await,
        "definir_dia_fechamento" => definir_dia_fechamento(db, user, args).await,
        "definir_orcamento" => definir_orcamento(db, user, args).await,
        "fechar_mes" => fechar_mes(db, user, me, args).await,
        _ => Err(format!("A ferramenta \"{name}\" não existe.")),
    }
}

// ───────────────────────── leitura dos argumentos ─────────────────────────

fn faltou(key: &str, tipo: &str) -> String {
    format!("Faltou o parâmetro \"{key}\" ({tipo}).")
}

fn req_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| faltou(key, "texto"))
}

fn req_i64(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key).and_then(Value::as_i64).ok_or_else(|| faltou(key, "número inteiro"))
}

fn req_bool(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key).and_then(Value::as_bool).ok_or_else(|| faltou(key, "verdadeiro/falso"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn opt_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

/// Posição de membro (0 ou 1) obrigatória; qualquer outra coisa é recusada.
fn req_posicao(args: &Value, key: &str) -> Result<u8, String> {
    match args.get(key).and_then(Value::as_u64) {
        Some(0) => Ok(0),
        Some(1) => Ok(1),
        _ => Err(format!("\"{key}\" deve ser 0 ou 1.")),
    }
}

/// "AAAA-MM" → Month; qualquer outra coisa é recusada. Também usada pela rotina do mês.
pub(super) fn mes_de(texto: &str) -> Result<Month, String> {
    let mes = (|| Some(Month {
        y: texto.get(..4)?.parse().ok()?,
        m: texto.get(5..7)?.parse().ok()?,
    }))();
    match mes {
        Some(m) if (1..=12).contains(&m.m) && texto.len() == 7 => Ok(m),
        _ => Err("Use o mês no formato AAAA-MM.".into()),
    }
}

fn dia_iso(m: Month, d: u32) -> String {
    let day = m.day(d);
    format!("{:04}-{:02}-{:02}", day.y, day.m, day.d)
}

/// "2026-11-15" → "15/11", o formato das etiquetas (também usado pela rotina do mês).
pub(super) fn dia_br(iso: &str) -> String {
    match (iso.get(8..10), iso.get(5..7)) {
        (Some(d), Some(m)) => format!("{d}/{m}"),
        _ => iso.to_owned(),
    }
}

/// "2026-11" → "nov/26", o formato das etiquetas.
fn mes_br(m: Month) -> String {
    const NOMES: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    format!("{}/{:02}", NOMES[m.m as usize - 1], m.y.rem_euclid(100))
}

/// Erros das rotas viram texto claro para o modelo (a mensagem já é em português).
fn msg(e: ApiError) -> String {
    e.1.to_owned()
}

// ───────────────────────── ferramentas ─────────────────────────

async fn buscar_lancamentos(db: &Client, user: User, me: u8, args: &Value) -> Result<Outcome, String> {
    let mes = opt_str(args, "mes").map(|m| mes_de(&m)).transpose()?;
    let texto = opt_str(args, "texto").filter(|t| !t.trim().is_empty());
    let categoria = opt_str(args, "categoria").filter(|c| !c.trim().is_empty());
    let limite = opt_i64(args, "limite").unwrap_or(20).clamp(1, MAX_BUSCA);
    let rows = db
        .query(
            &format!(
                "SELECT e.id::text, to_char(e.entry_date, 'YYYY-MM-DD'), e.description, e.amount_cents, e.kind,
                        e.category_key, e.shared, m.position, m.display_name, e.paid, e.recurring,
                        (SELECT a.external_id FROM cards c JOIN bank_accounts a ON a.id = c.account_id WHERE c.id = e.card_id),
                        EXISTS (SELECT 1 FROM bank_transactions t WHERE t.entry_id = e.id)
                 FROM entries e JOIN members m ON m.id = e.owner_id
                 WHERE e.household_id = {HOUSEHOLD} AND (m.position = $2 OR e.shared)
                     AND ($3::text IS NULL OR to_char(e.entry_date, 'YYYY-MM') = $3)
                     AND ($4::text IS NULL OR e.description ILIKE '%' || $4 || '%')
                     AND ($5::text IS NULL OR e.category_key = $5)
                 ORDER BY e.entry_date DESC, e.id LIMIT $6"
            ),
            &[
                &user.0,
                &(me as i16),
                &mes.map(|m| format!("{:04}-{:02}", m.y, m.m)),
                &texto,
                &categoria,
                &limite,
            ],
        )
        .await
        .map_err(db_msg)?;
    let lancamentos: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, String>(0),
                "data": r.get::<_, String>(1),
                "descricao": r.get::<_, String>(2),
                "valor_centavos": r.get::<_, i64>(3),
                "tipo": r.get::<_, String>(4),
                "categoria": r.get::<_, String>(5),
                "compartilhado": r.get::<_, bool>(6),
                "dono": r.get::<_, String>(8),
                "dono_posicao": r.get::<_, i16>(7),
                "pago": r.get::<_, bool>(9),
                "fixa": r.get::<_, bool>(10),
                "cartao": r.get::<_, Option<String>>(11),
                "importada": r.get::<_, bool>(12),
            })
        })
        .collect();
    let total = lancamentos.len();
    Ok(Outcome::new(
        "buscar_lancamentos",
        format!("buscou lançamentos ({total} encontrado{})", if total == 1 { "" } else { "s" }),
        json!({"total": total, "lancamentos": lancamentos}),
    ))
}

async fn criar_lancamento(db: &Client, user: User, me: u8, args: &Value) -> Result<Outcome, String> {
    let mut input = api::Entry {
        id: String::new(),
        description: req_str(args, "descricao")?,
        amount_cents: req_i64(args, "valor_centavos")?,
        date: req_str(args, "data")?,
        kind: req_str(args, "tipo")?,
        category: req_str(args, "categoria")?,
        shared: opt_bool(args, "compartilhado").unwrap_or(false),
        owner: me,
        paid: opt_bool(args, "pago").unwrap_or(false),
        card: opt_str(args, "cartao_id"),
        imported: false,
        recurring: opt_bool(args, "fixa").unwrap_or(false),
    };
    let saved = db::save_entry(db, user, &mut input).await.map_err(msg)?;
    let tipo = if saved.kind == "income" { "receita" } else { "despesa" };
    let mut result = json!({"criado": entry_json(&saved)});
    if let Some(aviso) = aviso_orcamento(db, user, &saved).await? {
        result["orcamento"] = aviso;
    }
    Ok(Outcome::new(
        "criar_lancamento",
        format!("criou {} {} {} em {}", tipo, saved.description, money(saved.amount_cents), dia_br(&saved.date)),
        result,
    ))
}

async fn editar_lancamento(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let id = req_str(args, "id")?;
    let mut atual = carregar_lancamento(db, user, &id).await?;
    let antes = atual.clone();
    if let Some(v) = opt_str(args, "descricao") {
        atual.description = v;
    }
    if let Some(v) = opt_i64(args, "valor_centavos") {
        atual.amount_cents = v;
    }
    if let Some(v) = opt_str(args, "data") {
        atual.date = v;
    }
    if let Some(v) = opt_str(args, "categoria") {
        atual.category = v;
    }
    if let Some(v) = opt_bool(args, "compartilhado") {
        atual.shared = v;
    }
    if let Some(v) = opt_bool(args, "pago") {
        atual.paid = v;
    }
    if args.get("cartao_id").is_some() {
        atual.card = opt_str(args, "cartao_id");
    }
    if let Some(v) = opt_bool(args, "fixa") {
        atual.recurring = v;
    }
    let mut mudas: Vec<String> = Vec::new();
    if antes.description != atual.description {
        mudas.push(format!("descrição para \"{}\"", atual.description));
    }
    if antes.amount_cents != atual.amount_cents {
        mudas.push(format!("valor para {}", money(atual.amount_cents)));
    }
    if antes.date != atual.date {
        mudas.push(format!("data para {}", dia_br(&atual.date)));
    }
    if antes.category != atual.category {
        mudas.push(format!("categoria para {}", atual.category));
    }
    if antes.paid != atual.paid {
        mudas.push(if atual.paid { "marcado como pago" } else { "marcado como não pago" }.into());
    }
    if antes.shared != atual.shared {
        mudas.push(if atual.shared { "virou do casal" } else { "virou pessoal" }.into());
    }
    if antes.card != atual.card {
        mudas.push(match &atual.card {
            Some(cartao) => format!("entrou no cartão {cartao}"),
            None => "saiu do cartão".into(),
        });
    }
    if antes.recurring != atual.recurring {
        mudas.push(if atual.recurring { "virou conta fixa" } else { "saiu das contas fixas" }.into());
    }
    let saved = db::save_edited_entry(db, user, &id, &mut atual).await.map_err(msg)?;
    let label = if mudas.len() == 1 && mudas[0].starts_with("data") && antes.date != saved.date {
        format!("moveu {} para {}", saved.description, dia_br(&saved.date))
    } else if mudas.is_empty() {
        format!("editou {}", saved.description)
    } else {
        format!("editou {}: {}", saved.description, mudas.join(", "))
    };
    let mut result = json!({"editado": entry_json(&saved)});
    if let Some(aviso) = aviso_orcamento(db, user, &saved).await? {
        result["orcamento"] = aviso;
    }
    Ok(Outcome::new("editar_lancamento", label, result))
}

async fn excluir_lancamento(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let id = req_str(args, "id")?;
    let atual = carregar_lancamento(db, user, &id).await?;
    db::remove_entry(db, user, &id).await.map_err(msg)?;
    Ok(Outcome::new(
        "excluir_lancamento",
        format!("excluiu {} {} de {}", atual.description, money(atual.amount_cents), dia_br(&atual.date)),
        json!({"excluido": entry_json(&atual)}),
    ))
}

async fn marcar_pago(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let pago = req_bool(args, "pago")?;
    let id = opt_str(args, "id");
    let fatura = args.get("fatura").filter(|v| !v.is_null());
    match (id, fatura) {
        (Some(id), None) => {
            let mut atual = carregar_lancamento(db, user, &id).await?;
            atual.paid = pago;
            let saved = db::save_edited_entry(db, user, &id, &mut atual).await.map_err(msg)?;
            Ok(Outcome::new(
                "marcar_pago",
                format!("marcou {} como {}", saved.description, if pago { "pago" } else { "não pago" }),
                json!({"marcado": entry_json(&saved)}),
            ))
        }
        (None, Some(fatura)) => {
            let cartao = fatura.get("cartao_id").and_then(Value::as_str).ok_or("Na fatura, informe cartao_id.")?.to_owned();
            let due = fatura.get("due").and_then(Value::as_str).ok_or("Na fatura, informe o vencimento (due) em AAAA-MM-DD.")?.to_owned();
            db::mark_bill_paid(db, user, &api::BillPaid { card: cartao.clone(), due: due.clone(), paid: pago })
                .await
                .map_err(msg)?;
            let nome = nome_do_cartao(db, user, &cartao).await;
            Ok(Outcome::new(
                "marcar_pago",
                format!("marcou Fatura {} como {}", nome, if pago { "paga" } else { "não paga" }),
                json!({"fatura": {"cartao": cartao, "nome": nome, "vencimento": due, "paga": pago}}),
            ))
        }
        _ => Err("Informe o id do lançamento OU a fatura (cartao_id e due), mas não os dois — e nem nenhum.".into()),
    }
}

async fn criar_acerto(db: &Client, user: User, me: u8, args: &Value) -> Result<Outcome, String> {
    let de = match args.get("de") {
        None | Some(Value::Null) => me,
        Some(_) => req_posicao(args, "de")?,
    };
    let input = api::Settlement {
        id: String::new(),
        date: req_str(args, "data")?,
        amount_cents: req_i64(args, "valor_centavos")?,
        from: de,
        to: 1 - de,
    };
    let saved = db::save_settlement(db, user, input).await.map_err(msg)?;
    let (nome_de, nome_to) = (
        nome_do_membro(db, user, de).await.unwrap_or_default(),
        nome_do_membro(db, user, saved.to).await.unwrap_or_default(),
    );
    Ok(Outcome::new(
        "criar_acerto",
        format!("registrou acerto de {} de {} para {}", money(saved.amount_cents), nome_de, nome_to),
        json!({"acerto": {"id": saved.id, "data": saved.date, "valor_centavos": saved.amount_cents,
            "de": saved.from, "para": saved.to}}),
    ))
}

async fn excluir_acerto(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let id = req_str(args, "id")?;
    let acerto = db
        .query_opt(
            &format!(
                "SELECT s.amount_cents, to_char(s.settled_on, 'YYYY-MM-DD')
                 FROM settlements s WHERE s.id = $2::text::uuid AND s.household_id = {HOUSEHOLD}"
            ),
            &[&user.0, &id],
        )
        .await
        .map_err(db_msg)?
        .map(|r| (r.get::<_, i64>(0), r.get::<_, String>(1)))
        .ok_or_else(|| "Acerto não encontrado neste domicílio.".to_string());
    let (valor, data) = acerto?;
    db::remove_settlement(db, user, &id).await.map_err(msg)?;
    Ok(Outcome::new(
        "excluir_acerto",
        format!("excluiu o acerto de {} de {}", money(valor), dia_br(&data)),
        json!({"excluido": {"id": id, "valor_centavos": valor, "data": data}}),
    ))
}

async fn definir_reserva(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let input = api::Reserve {
        opening_cents: opt_i64(args, "abertura_centavos").unwrap_or(0),
        target_cents: req_i64(args, "meta_centavos")?,
        name: opt_str(args, "nome").unwrap_or_else(|| "Reserva".into()),
    };
    db::save_reserve(db, user, &input).await.map_err(msg)?;
    Ok(Outcome::new(
        "definir_reserva",
        format!("definiu a meta da reserva para {}", money(input.target_cents)),
        json!({"reserva": {"nome": input.name, "abertura_centavos": input.opening_cents, "meta_centavos": input.target_cents}}),
    ))
}

async fn renomear_membro(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let posicao = req_posicao(args, "posicao")?;
    let nome = req_str(args, "nome")?;
    let antigo = nome_do_membro(db, user, posicao).await.unwrap_or_else(|| format!("membro {posicao}"));
    db::rename_position(db, user, posicao, &nome).await.map_err(msg)?;
    Ok(Outcome::new(
        "renomear_membro",
        format!("renomeou {antigo} para {nome}"),
        json!({"membro": {"posicao": posicao, "antes": antigo, "agora": nome}}),
    ))
}

async fn definir_instituicao(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let item_id = req_str(args, "item_id")?;
    let chave = opt_str(args, "chave_instituicao").filter(|c| !c.is_empty());
    db::set_institution(db, user, &item_id, chave.as_deref()).await.map_err(msg)?;
    let label = match &chave {
        Some(chave) => format!("definiu {chave} como banco da conexão {item_id}"),
        None => format!("limpou o banco da conexão {item_id}"),
    };
    Ok(Outcome::new(
        "definir_instituicao_conexao",
        label,
        json!({"conexao": {"item_id": item_id, "instituicao": chave}}),
    ))
}

async fn alternar_rateio(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let desligar = req_bool(args, "desligar")?;
    db::save_settings(db, user, &api::Settings { shared_split_off: desligar }).await.map_err(msg)?;
    Ok(Outcome::new(
        "alternar_rateio",
        if desligar { "desligou o rateio pela renda" } else { "ligou o rateio pela renda" }.into(),
        json!({"rateio_compartilhado_desligado": desligar}),
    ))
}

/// Mesma cópia do app (`Data::copy_previous` com `only = None`, como o celular faz): manuais
/// (sem cartão e sem vínculo com o banco) do mês anterior viram não pagos no mesmo dia, e os
/// que já existem no mês alvo (mesma descrição, valor e dono) ficam de fora. Núcleo da
/// ferramenta e da rotina "fechar o mês"; devolve (criados, pulados).
pub(super) async fn copiar_mes(db: &Client, user: User, alvo: Month) -> Result<(Vec<Value>, usize), ApiError> {
    let anterior = alvo.add(-1);
    let (inicio_anterior, inicio_alvo, fim_alvo) =
        (dia_iso(anterior, 1), dia_iso(alvo, 1), dia_iso(alvo.add(1), 1));
    let rows = db
        .query(
            &format!(
                "SELECT e.description, e.amount_cents, to_char(e.entry_date, 'YYYY-MM-DD'), e.kind, e.category_key,
                        e.shared, m.position, e.recurring
                 FROM entries e JOIN members m ON m.id = e.owner_id
                 WHERE e.household_id = {HOUSEHOLD} AND e.card_id IS NULL
                     AND NOT EXISTS (SELECT 1 FROM bank_transactions t WHERE t.entry_id = e.id)
                     AND e.entry_date >= $2::text::date AND e.entry_date < $3::text::date
                 ORDER BY e.entry_date, e.id"
            ),
            &[&user.0, &inicio_anterior, &inicio_alvo],
        )
        .await?;
    let existentes: Vec<(String, i64, i16)> = db
        .query(
            &format!(
                "SELECT e.description, e.amount_cents, m.position
                 FROM entries e JOIN members m ON m.id = e.owner_id
                 WHERE e.household_id = {HOUSEHOLD}
                     AND e.entry_date >= $2::text::date AND e.entry_date < $3::text::date"
            ),
            &[&user.0, &inicio_alvo, &fim_alvo],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    let mut criados: Vec<Value> = Vec::new();
    let mut pulados: usize = 0;
    for r in &rows {
        let (desc, amount, date, kind, category, shared, owner, recurring): (String, i64, String, String, String, bool, i16, bool) =
            (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5), r.get(6), r.get(7));
        if existentes.iter().any(|(d, a, o)| *d == desc && *a == amount && *o == owner) {
            pulados += 1;
            continue;
        }
        let mut input = api::Entry {
            id: String::new(),
            description: desc,
            amount_cents: amount,
            date: dia_iso(alvo, date.get(8..).and_then(|d| d.parse().ok()).unwrap_or(1)),
            kind,
            category,
            shared,
            owner: owner as u8,
            paid: false,
            card: None,
            imported: false,
            recurring,
        };
        let saved = db::save_entry(db, user, &mut input).await?;
        criados.push(entry_json(&saved));
    }
    Ok((criados, pulados))
}

/// Ferramenta `copiar_mes_anterior`: o núcleo `copiar_mes` com etiqueta e contagem.
async fn copiar_mes_anterior(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let alvo = mes_de(&req_str(args, "mes")?)?;
    let (criados, pulados) = copiar_mes(db, user, alvo).await.map_err(msg)?;
    let total = criados.len();
    Ok(Outcome::new(
        "copiar_mes_anterior",
        format!("copiou {total} lançamento(s) de {} para {}", mes_br(alvo.add(-1)), mes_br(alvo)),
        json!({"criados": criados, "total": total, "pulados_ja_existentes": pulados}),
    ))
}

/// Define (ou, com null/valor <= 0, remove) o orçamento mensal do domicílio para a categoria.
async fn definir_orcamento(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let categoria = req_str(args, "categoria")?;
    let valor = match args.get("monthly_cents") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_i64().ok_or("\"monthly_cents\" deve ser um número inteiro em centavos, ou null para remover.")?,
    };
    db::set_budget(db, user, &api::Budget { category: categoria.clone(), monthly_cents: valor })
        .await
        .map_err(msg)?;
    let label = if valor <= 0 {
        format!("removeu o orçamento da categoria {categoria}")
    } else {
        format!("definiu orçamento de {categoria}: {} por mês", money(valor))
    };
    Ok(Outcome::new(
        "definir_orcamento",
        label,
        json!({"orcamento": {"categoria": categoria, "monthly_cents": valor}}),
    ))
}

/// Ferramenta `fechar_mes`: roda a rotina e devolve o relatório; os selos cobrem cada
/// lançamento copiado mais o resumo do mês.
async fn fechar_mes(db: &Client, user: User, me: u8, args: &Value) -> Result<Outcome, String> {
    let mes = opt_str(args, "mes").filter(|m| !m.trim().is_empty());
    let report = routine::run_routine(db, user, me as i16, mes).await.map_err(msg)?;
    let copiados = report["copiados"].as_array().cloned().unwrap_or_default();
    let mut ops: Vec<api::Op> = copiados.iter().map(routine::op_de_copia).collect();
    ops.push(api::Op {
        tool: "fechar_mes".into(),
        label: format!("fechou o mês: {} lançamento(s) copiado(s)", copiados.len()),
    });
    Ok(Outcome { result: report, ops })
}

async fn definir_dia_fechamento(db: &Client, user: User, args: &Value) -> Result<Outcome, String> {
    let cartao = req_str(args, "cartao_id")?;
    let dia = match args.get("dia") {
        Some(Value::Number(n)) => {
            let d = n.as_u64().ok_or("\"dia\" deve ser um dia do mês (1-31) ou null.")? as u32;
            if !(1..=31).contains(&d) {
                return Err("O dia de fechamento deve ser entre 1 e 31.".into());
            }
            Some(d)
        }
        _ => None,
    };
    db::set_closing_day(db, user, &cartao, dia).await.map_err(msg)?;
    let nome = nome_do_cartao(db, user, &cartao).await;
    let label = match dia {
        Some(d) => format!("ajustou o fechamento do cartão {nome} para o dia {d}"),
        None => format!("limpou o ajuste de fechamento do cartão {nome}"),
    };
    Ok(Outcome::new(
        "definir_dia_fechamento",
        label,
        json!({"cartao": {"id": cartao, "nome": nome, "fechamento": dia}}),
    ))
}

// ───────────────────────── consultas de apoio ─────────────────────────

/// Aviso de orçamento estourado: existe orçamento para a categoria do lançamento e o total
/// do DOMICÍLIO na categoria no mês dele (despesas dos dois) passou do limite. Vai no
/// resultado da ferramenta com o nome explícito (`gasto_casal_cents`) para o modelo avisar.
async fn aviso_orcamento(db: &Client, user: User, saved: &api::Entry) -> Result<Option<Value>, String> {
    if saved.kind != "expense" || saved.date.len() < 7 {
        return Ok(None);
    }
    let mes = saved.date[..7].to_owned();
    let row = db
        .query_opt(
            &format!(
                "SELECT b.monthly_cents,
                        COALESCE((SELECT SUM(e.amount_cents)::bigint FROM entries e
                          WHERE e.household_id = {HOUSEHOLD} AND e.category_key = b.category_key
                              AND e.kind = 'expense' AND to_char(e.entry_date, 'YYYY-MM') = $3), 0)
                     FROM budgets b WHERE b.household_id = {HOUSEHOLD} AND b.category_key = $2"
            ),
            &[&user.0, &saved.category, &mes],
        )
        .await
        .map_err(db_msg)?;
    Ok(row.filter(|r| {
        let (limite, gasto): (i64, i64) = (r.get(0), r.get(1));
        gasto > limite
    })
    .map(|r| {
        json!({
            "categoria": saved.category,
            "gasto_casal_cents": r.get::<_, i64>(1),
            "limite_cents": r.get::<_, i64>(0),
        })
    }))
}

/// O lançamento do domicílio, com os mesmos campos da leitura do estado (para editar por cima
/// sem perder nada e para as etiquetas).
async fn carregar_lancamento(db: &Client, user: User, id: &str) -> Result<api::Entry, String> {
    let row = db
        .query_opt(
            &format!(
                "SELECT {ENTRY} FROM entries e JOIN members m ON m.id = e.owner_id
                 WHERE e.id = $2::text::uuid AND e.household_id = {HOUSEHOLD}"
            ),
            &[&user.0, &id],
        )
        .await
        .map_err(db_msg)?;
    row.map(|r| db::entry(&r)).ok_or_else(|| "Lançamento não encontrado neste domicílio.".into())
}

async fn nome_do_cartao(db: &Client, user: User, cartao: &str) -> String {
    db.query_opt(
        &format!(
            "SELECT c.name FROM cards c JOIN bank_accounts a ON a.id = c.account_id
             WHERE a.external_id = $2 AND c.household_id = {HOUSEHOLD}"
        ),
        &[&user.0, &cartao],
    )
    .await
    .ok()
    .flatten()
    .map(|r| r.get::<_, String>(0))
    .unwrap_or_else(|| cartao.to_owned())
}

async fn nome_do_membro(db: &Client, user: User, position: u8) -> Option<String> {
    db.query_opt(
        &format!("SELECT display_name FROM members WHERE household_id = {HOUSEHOLD} AND position = $2"),
        &[&user.0, &(position as i16)],
    )
    .await
    .ok()
    .flatten()
    .map(|r| r.get::<_, String>(0))
}

/// Erros do PostgreSQL viram texto genérico (a mensagem pode conter dados gravados).
fn db_msg(_: tokio_postgres::Error) -> String {
    "Não foi possível gravar no banco de dados.".into()
}

/// O lançamento em JSON enxuto, no vocabulário das ferramentas.
fn entry_json(e: &api::Entry) -> Value {
    json!({
        "id": e.id,
        "data": e.date,
        "descricao": e.description,
        "valor_centavos": e.amount_cents,
        "tipo": e.kind,
        "categoria": e.category,
        "compartilhado": e.shared,
        "dono_posicao": e.owner,
        "pago": e.paid,
        "cartao": e.card,
        "fixa": e.recurring,
        "importada": e.imported,
    })
}
