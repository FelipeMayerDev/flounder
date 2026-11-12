//! Contrato JSON entre desktop e backend. Usado pelos dois crates via `#[path]`.
//! Todas as rotas exigem `Authorization: Bearer APP_TOKEN_N`; o token define o membro (client-N).

use crate::connected::ConnectedCard;
use serde::{Deserialize, Serialize};

/// `GET /api/state`: tudo que o desktop mostra, do domicílio de quem está autenticado.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// Posição (0/1) do membro autenticado.
    pub me: u8,
    /// Ordenados por posição.
    pub members: Vec<Member>,
    /// Lançamentos manuais do domicílio (ambos os membros).
    pub entries: Vec<Entry>,
    pub reserve: Option<Reserve>,
    /// Cartões importados da Pluggy, de ambos os membros (`owner` diz de quem é).
    pub cards: Vec<ConnectedCard>,
    /// Contas correntes/poupança importadas (tipo BANK), de ambos os membros.
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// Conexões Pluggy (itens) do domicílio.
    #[serde(default)]
    pub connections: Vec<Connection>,
    /// Catálogo `finance.institutions`, para escolher o banco de uma conexão.
    #[serde(default)]
    pub institutions: Vec<Institution>,
    /// Acertos entre o casal (quem transferiu para quem, para zerar a divisão da casa).
    #[serde(default)]
    pub settlements: Vec<Settlement>,
    /// Despesas compartilhadas contam por inteiro para quem paga (sem rateio pela renda).
    #[serde(default)]
    pub shared_split_off: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Account {
    /// Id externo (Pluggy) da conta.
    pub id: String,
    pub item_id: String,
    pub owner: u8,
    pub name: String,
    pub institution: String,
    pub color: [u8; 3],
    /// ISO de 3 letras ou "Moeda não informada".
    pub currency: String,
    pub balance: Option<i64>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Connection {
    pub item_id: String,
    pub owner: u8,
    /// Nome resolvido (banco escolhido, ou o que o conector informa).
    pub institution: String,
    /// Chave em `institutions` escolhida pelo usuário; `None` = não informada.
    pub institution_key: Option<String>,
    /// Última importação bem-sucedida, ISO 8601 UTC.
    pub synced_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Institution {
    pub key: String,
    pub name: String,
    pub color: [u8; 3],
}

/// `POST /api/settlements` (sem `id`) devolve o salvo; `DELETE /api/settlements/{id}` → 204.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settlement {
    #[serde(default)]
    pub id: String,
    /// "AAAA-MM-DD".
    pub date: String,
    /// Centavos > 0.
    pub amount_cents: i64,
    /// Posição de quem pagou o acerto.
    pub from: u8,
    /// Posição de quem recebeu.
    pub to: u8,
}

/// Resposta de `POST /api/sync`: reimporta da Pluggy todas as conexões do domicílio
/// (cartões, contas, faturas e transações). Pode levar dezenas de segundos.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SyncResult {
    pub synced: u32,
    pub failed: u32,
}

/// Corpo de `PUT /api/members/{position}` → 204.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MemberName {
    pub name: String,
}

/// Corpo de `PUT /api/connections/{item_id}/institution` → 204. Aplica-se aos cartões e
/// contas da conexão e vale nas próximas importações (substitui `institutions.json`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConnectionInstitution {
    pub institution_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub position: u8,
    pub name: String,
    /// "client-1" ou "client-2": qual APP_TOKEN autentica este membro.
    pub client: String,
}

/// Corpo de `POST /api/entries` e `PUT /api/entries/{id}` (sem `id`) e item de resposta (com `id`).
/// `DELETE /api/entries/{id}` remove. Respostas de POST/PUT devolvem o `Entry` salvo.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// UUID do banco; vazio no corpo de criação.
    #[serde(default)]
    pub id: String,
    pub description: String,
    /// Centavos, sempre >= 0; o sinal vem de `kind`.
    pub amount_cents: i64,
    /// "AAAA-MM-DD".
    pub date: String,
    /// "income" | "expense".
    pub kind: String,
    /// Chave de `finance.categories`: salary, housing, utilities, groceries, transport,
    /// health, leisure, reserve, family, other.
    pub category: String,
    /// Só despesas podem ser compartilhadas.
    pub shared: bool,
    /// Posição (0/1) do membro: quem recebe, de quem é, ou quem paga a compartilhada.
    pub owner: u8,
    pub paid: bool,
    /// Compra no cartão importado (`ConnectedCard.id`): pesa nas despesas, mas a conta a pagar
    /// é a fatura. Só despesas. Validado contra os cartões do domicílio.
    #[serde(default)]
    pub card: Option<String>,
    /// Criado pela importação de transações (somente leitura; o backend ignora no corpo).
    /// Pode ser editado; excluir um importado faz a transação não voltar nas próximas importações.
    #[serde(default)]
    pub imported: bool,
    /// Conta fixa: repete todo mês e entra na previsão dos meses futuros.
    #[serde(default)]
    pub recurring: bool,
}

/// `GET /api/state` → `reserve`; `PUT /api/reserve` (corpo `Reserve`) cria ou substitui a meta
/// única do domicílio → 204.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reserve {
    pub opening_cents: i64,
    pub target_cents: i64,
    #[serde(default)]
    pub name: String,
}

/// Corpo de `PUT /api/card-bills/paid` (204; 404 se o cartão/vencimento não for do domicílio):
/// marca ou desmarca a fatura importada de um cartão no vencimento indicado.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BillPaid {
    /// `ConnectedCard.id`.
    pub card: String,
    /// "AAAA-MM-DD".
    pub due: String,
    pub paid: bool,
}

/// Corpo de `PUT /api/settings` (204): ajustes do domicílio, valem para os dois membros.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Desconsidera o rateio: despesa compartilhada pesa por inteiro para quem pagou,
    /// e as telas de rateio e acertos saem do aplicativo.
    #[serde(default)]
    pub shared_split_off: bool,
}

// ───────────────────────── Assistente (GLM via Z.ai) ─────────────────────────

/// Análise gerada pelo backend: uma por pessoa (`subject` 0/1) e uma para o casal (`subject` 2).
/// O backend só devolve ao membro a própria e a do casal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AssistantReport {
    /// 0 = membro na posição 0, 1 = posição 1, 2 = casal.
    pub subject: u8,
    /// Conteúdo em texto simples, bem-humorado, gerado pelo GLM.
    pub content: String,
    /// ISO 8601 UTC da geração.
    pub generated_at: String,
    /// Período analisado, "AAAA-MM-DD".
    pub period_start: String,
    pub period_end: String,
}

/// Resposta de `GET /api/assistant/reports`, já filtrada pela privacidade de quem pergunta.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AssistantReports {
    /// Falso quando o backend não tem `ZAI_TOKEN` configurado.
    pub enabled: bool,
    /// Geração em andamento no backend (análise diária ou "Analisar agora").
    pub busy: bool,
    pub reports: Vec<AssistantReport>,
}

/// Uma mensagem do histórico de conversa com o assistente.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatMsg {
    /// "user" ou "assistant".
    pub role: String,
    pub content: String,
}

/// Corpo de `POST /api/assistant/chat` (todo o histórico, mais recente por último).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMsg>,
}

/// Resposta de `POST /api/assistant/chat`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatReply {
    pub content: String,
}
