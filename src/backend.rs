//! Cliente do backend: busca `GET /api/state` a cada 5 s e grava lançamentos, acertos, reserva,
//! nomes e bancos das conexões, numa thread própria.
//! A interface só troca mensagens por canal; nada de rede na thread da interface.

use crate::api;
use crate::model::{Category, Date, Entry, Kind, Person, Settlement};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_secs(5);
/// `POST /api/sync` reimporta tudo da Pluggy e pode levar dezenas de segundos.
const SYNC_TIMEOUT: Duration = Duration::from_secs(90);
const RUN_LOCAL: &str = "Execute backend/run-local.sh e tente novamente.";
const UNEXPECTED: &str = "Resposta inesperada do backend.";

pub enum Msg {
    /// Estado do domicílio; os lançamentos já vêm convertidos, com id local estável por UUID.
    State(Box<api::State>, Vec<Entry>),
    /// Fim de uma gravação; `Some` com a mensagem em caso de erro (a thread recarrega em seguida).
    Done(Option<String>),
    /// Falha ao buscar o estado.
    Offline(String),
    /// Fim de `Cmd::Sync` (a thread recarrega o estado em seguida).
    Synced(Result<api::SyncResult, String>),
}

/// Thread de sincronização de um membro (`client-N`). Soltar a struct encerra a thread
/// depois de gravar o que já estava na fila.
/// Gravação pendente, enviada na ordem em que foi feita.
pub enum Cmd {
    /// Cria (id ainda sem UUID) ou atualiza o lançamento.
    Entry(Entry),
    BillPaid(api::BillPaid),
    /// Exclui o lançamento (id local).
    Delete(u32),
    /// Reimporta as conexões do domicílio; não conta em `pending` (responde com `Msg::Synced`).
    Sync,
    /// Registra o acerto (id "local-N"; a thread guarda o UUID devolvido).
    Settle(Settlement),
    /// Exclui o acerto (UUID ou "local-N").
    Unsettle(String),
    Reserve(api::Reserve),
    /// Renomeia o membro da posição.
    Member(u8, String),
    /// Banco escolhido para a conexão (`item_id`); `None` = não informado.
    Institution(String, Option<String>),
}

pub struct Worker {
    /// `client-N` cujo token esta thread usa.
    pub client: String,
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Msg>,
    /// Gravações enviadas e ainda sem resposta: estados buscados nesse meio-tempo são descartados.
    pub pending: usize,
}

impl Worker {
    pub fn start(client: String, ctx: egui::Context) -> Worker {
        let (tx, commands) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        let worker = Worker { client: client.clone(), tx, rx, pending: 0 };
        std::thread::spawn(move || run(&client, &ctx, &replies, &commands));
        worker
    }

    pub fn save(&mut self, cmd: Cmd) {
        self.pending += 1;
        let _ = self.tx.send(cmd);
    }
}

fn run(client: &str, ctx: &egui::Context, replies: &Sender<Msg>, commands: &Receiver<Cmd>) {
    let send = |m| {
        let _ = replies.send(m);
        ctx.request_repaint();
    };
    let token = match token(client) {
        Ok(t) => t,
        Err(e) => return send(Msg::Offline(e)),
    };
    let base = base_url();
    let base = base.trim_end_matches('/');
    let auth = format!("Bearer {token}");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(5))).build().into();
    let slow: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(SYNC_TIMEOUT)).build().into();
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut next_id = 1;
    // "local-N" → UUID dos acertos criados por este aparelho.
    let mut settled: HashMap<String, String> = HashMap::new();
    let put = |path: String, body: String| -> Result<(), String> {
        agent.put(format!("{base}{path}")).header("Authorization", &auth).header("Content-Type", "application/json")
            .send(&body).map(drop).map_err(|err| describe(err, base))
    };
    loop {
        let state = agent.get(format!("{base}/api/state")).header("Authorization", &auth).call()
            .and_then(|mut r| r.body_mut().read_to_string())
            .map_err(|e| describe(e, base))
            .and_then(|body| serde_json::from_str::<api::State>(&body).map_err(|_| UNEXPECTED.to_string()));
        match state {
            Ok(mut state) => {
                let entries = state.entries.drain(..).filter_map(|e| {
                    let id = *ids.entry(e.id.clone()).or_insert_with(|| { next_id += 1; next_id - 1 });
                    to_model(&e, id)
                }).collect();
                send(Msg::State(Box::new(state), entries));
            }
            Err(e) => send(Msg::Offline(e)),
        }
        let until = Instant::now() + POLL;
        loop {
            match commands.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(Cmd::BillPaid(bill)) => {
                    let body = serde_json::to_string(&bill).expect("BillPaid serializa");
                    let result = agent.put(format!("{base}/api/card-bills/paid")).header("Authorization", &auth)
                        .header("Content-Type", "application/json").send(&body).map(drop).map_err(|err| describe(err, base));
                    let failed = result.is_err();
                    send(Msg::Done(result.err().map(|err| format!("Não foi possível salvar a fatura. {err} Os dados foram recarregados."))));
                    if failed {
                        break;
                    }
                }
                // ponytail: a sincronização ocupa a thread; gravações feitas nesse meio-tempo esperam na fila.
                Ok(Cmd::Sync) => {
                    let result = slow.post(format!("{base}/api/sync")).header("Authorization", &auth).send_empty()
                        .and_then(|mut r| r.body_mut().read_to_string())
                        .map_err(|err| describe(err, base))
                        .and_then(|body| serde_json::from_str::<api::SyncResult>(&body).map_err(|_| UNEXPECTED.to_string()));
                    send(Msg::Synced(result));
                    break;
                }
                Ok(Cmd::Delete(id)) => {
                    let uuid = ids.iter().find(|(_, local)| **local == id).map(|(uuid, _)| uuid.clone());
                    let result = match &uuid {
                        // Nunca chegou ao banco: não há o que excluir.
                        None => Ok(()),
                        Some(uuid) => match agent.delete(format!("{base}/api/entries/{uuid}")).header("Authorization", &auth).call() {
                            // Já excluído (por exemplo, pelo outro membro).
                            Ok(_) | Err(ureq::Error::StatusCode(404)) => Ok(()),
                            Err(err) => Err(describe(err, base)),
                        },
                    };
                    if result.is_ok() && let Some(uuid) = uuid {
                        ids.remove(&uuid);
                    }
                    let failed = result.is_err();
                    send(Msg::Done(result.err().map(|err| format!("Não foi possível excluir o lançamento. {err} Os dados foram recarregados."))));
                    if failed {
                        break;
                    }
                }
                Ok(Cmd::Entry(e)) => {
                    let body = serde_json::to_string(&to_api(&e)).expect("Entry serializa");
                    let uuid = ids.iter().find(|(_, id)| **id == e.id).map(|(uuid, _)| uuid.clone());
                    let result = match uuid {
                        Some(uuid) => agent.put(format!("{base}/api/entries/{uuid}")).header("Authorization", &auth)
                            .header("Content-Type", "application/json").send(&body).map(drop).map_err(|err| describe(err, base)),
                        None => agent.post(format!("{base}/api/entries")).header("Authorization", &auth)
                            .header("Content-Type", "application/json").send(&body)
                            .and_then(|mut r| r.body_mut().read_to_string())
                            .map_err(|err| describe(err, base))
                            .and_then(|body| serde_json::from_str::<api::Entry>(&body).map_err(|_| UNEXPECTED.to_string()))
                            .map(|saved| _ = ids.insert(saved.id, e.id)),
                    };
                    let failed = result.is_err();
                    send(Msg::Done(result.err().map(|err| format!("Não foi possível salvar \"{}\". {err} Os dados foram recarregados.", e.desc))));
                    if failed {
                        break;
                    }
                }
                Ok(cmd @ (Cmd::Settle(_) | Cmd::Unsettle(_) | Cmd::Reserve(_) | Cmd::Member(..) | Cmd::Institution(..))) => {
                    let (what, result) = match cmd {
                        Cmd::Settle(s) => {
                            let body = serde_json::to_string(&api::Settlement {
                                id: String::new(),
                                date: format!("{:04}-{:02}-{:02}", s.date.y, s.date.m, s.date.d),
                                amount_cents: s.amount,
                                from: s.from.idx() as u8,
                                to: s.from.other().idx() as u8,
                            }).expect("Settlement serializa");
                            let result = agent.post(format!("{base}/api/settlements")).header("Authorization", &auth)
                                .header("Content-Type", "application/json").send(&body)
                                .and_then(|mut r| r.body_mut().read_to_string())
                                .map_err(|err| describe(err, base))
                                .and_then(|body| serde_json::from_str::<api::Settlement>(&body).map_err(|_| UNEXPECTED.to_string()))
                                .map(|saved| _ = settled.insert(s.id, saved.id));
                            ("Não foi possível registrar o acerto.", result)
                        }
                        Cmd::Unsettle(id) => {
                            let uuid = settled.get(&id).cloned().or_else(|| (!id.starts_with("local-")).then_some(id));
                            let result = match uuid {
                                // Nunca chegou ao banco: não há o que excluir.
                                None => Ok(()),
                                Some(uuid) => match agent.delete(format!("{base}/api/settlements/{uuid}")).header("Authorization", &auth).call() {
                                    Ok(_) | Err(ureq::Error::StatusCode(404)) => Ok(()),
                                    Err(err) => Err(describe(err, base)),
                                },
                            };
                            ("Não foi possível excluir o acerto.", result)
                        }
                        Cmd::Reserve(r) => ("Não foi possível salvar a meta da reserva.", put("/api/reserve".into(), serde_json::to_string(&r).expect("Reserve serializa"))),
                        Cmd::Member(pos, name) => (
                            "Não foi possível salvar o nome.",
                            put(format!("/api/members/{pos}"), serde_json::to_string(&api::MemberName { name }).expect("MemberName serializa")),
                        ),
                        Cmd::Institution(item, key) => (
                            "Não foi possível salvar o banco da conexão.",
                            put(format!("/api/connections/{item}/institution"), serde_json::to_string(&api::ConnectionInstitution { institution_key: key }).expect("serializa")),
                        ),
                        _ => unreachable!(),
                    };
                    let failed = result.is_err();
                    send(Msg::Done(result.err().map(|err| format!("{what} {err} Os dados foram recarregados."))));
                    if failed {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// Mensagem em português; nunca inclui o token.
fn describe(e: ureq::Error, base: &str) -> String {
    match e {
        ureq::Error::StatusCode(401) => "O backend recusou o token deste membro (backend/.env).".into(),
        ureq::Error::StatusCode(code) => format!("O backend respondeu com erro {code}."),
        _ => format!("Não foi possível conectar ao backend em {base}. {RUN_LOCAL}"),
    }
}

/// Abre no navegador a página /connect já com o token de `client` (no fragmento, que não vai
/// ao servidor; a página o apaga do endereço).
// ponytail: o token aparece por instantes na linha de comando do xdg-open/navegador, visível a outros
// usuários desta máquina; ok para o uso pessoal local. Windows não tem abridor aqui.
pub fn open_connect(client: &str) -> Result<(), String> {
    let base = base_url();
    let url = format!("{}/connect#token={}", base.trim_end_matches('/'), token(client)?);
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener).arg(url).spawn().map(drop)
        .map_err(|_| format!("Não foi possível abrir o navegador. Abra {base}/connect e informe o token."))
}

/// `client-N` → valor de `APP_TOKEN_N`, lendo só essa linha do `backend/.env` local.
// ponytail: modo local lê o .env do backend desta máquina; no servidor, cada máquina precisa de configuração própria do token.
fn token(client: &str) -> Result<String, String> {
    let n = client.strip_prefix("client-").filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("Membro desconhecido: {client}."))?;
    let path = env_path();
    let missing = || format!("Não foi possível ler APP_TOKEN_{n} em {}. {RUN_LOCAL}", path.display());
    let text = std::fs::read_to_string(&path).map_err(|_| missing())?;
    find_token(&text, n).ok_or_else(missing)
}

/// `FLOUNDER_BACKEND_ENV`, senão o `backend/.env` do projeto (máquina de desenvolvimento), senão
/// `flounder.env` ao lado do executável (pacote em outra máquina: só tokens e endereço).
fn env_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("FLOUNDER_BACKEND_ENV") {
        return path.into();
    }
    let dev = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/backend/.env"));
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("flounder.env")));
    match beside {
        Some(path) if !dev.exists() => path,
        _ => dev,
    }
}

/// Endereço do backend: variável `FLOUNDER_BACKEND_URL`, senão a mesma chave no `.env` local
/// (aponta o app para o servidor sem mexer no atalho), senão o backend local.
fn base_url() -> String {
    let url = std::env::var("FLOUNDER_BACKEND_URL").ok()
        .or_else(|| env_value(&std::fs::read_to_string(env_path()).ok()?, "FLOUNDER_BACKEND_URL"))
        .unwrap_or_else(|| "http://127.0.0.1:3001".into());
    url.trim_end_matches('/').to_string()
}

fn find_token(env: &str, n: &str) -> Option<String> {
    env_value(env, &format!("APP_TOKEN_{n}"))
}

fn env_value(env: &str, key: &str) -> Option<String> {
    env.lines()
        .filter_map(|line| line.trim().split_once('='))
        .find(|(name, _)| name.trim().trim_start_matches("export ").trim() == key)
        .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_string())
        .filter(|t| !t.is_empty())
}

pub fn to_api(e: &Entry) -> api::Entry {
    api::Entry {
        id: String::new(),
        description: e.desc.clone(),
        amount_cents: e.amount,
        date: format!("{:04}-{:02}-{:02}", e.date.y, e.date.m, e.date.d),
        kind: if e.kind == Kind::Income { "income" } else { "expense" }.into(),
        category: e.category.key().into(),
        shared: e.shared,
        owner: e.owner.idx() as u8,
        paid: e.paid,
        card: e.connected.clone(),
        imported: e.imported,
    }
}

/// `None` se o registro não fizer sentido (data, tipo ou membro inválidos). Categoria desconhecida vira Outros.
pub fn to_model(e: &api::Entry, id: u32) -> Option<Entry> {
    Some(Entry {
        id,
        desc: e.description.clone(),
        amount: e.amount_cents,
        date: Date::iso(&e.date)?,
        kind: match e.kind.as_str() {
            "income" => Kind::Income,
            "expense" => Kind::Expense,
            _ => return None,
        },
        category: Category::ALL.into_iter().find(|c| c.key() == e.category).unwrap_or(Category::Other),
        owner: match e.owner {
            0 => Person::A,
            1 => Person::B,
            _ => return None,
        },
        shared: e.shared,
        card: None,
        connected: if e.kind == "expense" { e.card.clone() } else { None },
        imported: e.imported,
        paid: e.paid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_the_api_contract() {
        let d = crate::model::Data::demo();
        for e in &d.entries {
            let e = Entry { card: None, ..e.clone() };
            let json = serde_json::to_string(&to_api(&e)).unwrap();
            let back: api::Entry = serde_json::from_str(&json).unwrap();
            assert_eq!(to_model(&back, e.id).as_ref(), Some(&e));
        }
        let mut bad = to_api(&d.entries[0]);
        bad.date = "2026-02-30".into();
        assert!(to_model(&bad, 1).is_none());
        // Compra importada num cartão conectado: `card` e `imported` voltam iguais.
        let bought = Entry { connected: Some("card-1".into()), imported: true, paid: true, ..d.entries[2].clone() };
        let json = serde_json::to_string(&to_api(&bought)).unwrap();
        assert!(json.contains(r#""card":"card-1""#) && json.contains(r#""imported":true"#));
        assert_eq!(to_model(&serde_json::from_str(&json).unwrap(), bought.id), Some(bought));
        let mut odd = to_api(&d.entries[0]);
        odd.category = "nova".into();
        assert_eq!(to_model(&odd, 1).unwrap().category, Category::Other);
        odd.owner = 2;
        assert!(to_model(&odd, 1).is_none());
    }

    #[test]
    fn token_reads_only_the_requested_member() {
        let env = "CLIENT_SECRET=x\nAPP_TOKEN_1=one\nexport APP_TOKEN_2 = \"two\"\nAPP_TOKEN_3=\n";
        assert_eq!(find_token(env, "1").as_deref(), Some("one"));
        assert_eq!(find_token(env, "2").as_deref(), Some("two"));
        assert_eq!(find_token(env, "3"), None);
        assert!(token("admin").is_err());
    }
}
