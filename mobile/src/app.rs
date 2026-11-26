//! Casca do Flounder mobile: topo, barra do mês, navegação, telas e a sessão
//! real com o backend (worker do desktop + pareamento por código; fase 1 vira demonstração).

use egui::{Align, Align2, Color32, Frame, Layout, Ui, Vec2};
use flounder::backend::{self, PairMsg, Worker};
use flounder::model::{BillRef, Data, Kind, Month, Person};
use flounder::widgets as w;
use egui_phosphor::regular as icon;
use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;

use crate::converter;
use crate::mock::{self, MEMBROS};
use crate::theme;

pub enum Tab {
    Inicio,
    Fluxo,
    Cartoes,
    Casa,
    Consultor,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Filtro {
    Todas,
    Pendentes,
    Pagas,
}

/// Folha do FAB: novo lançamento.
#[derive(Clone, Default)]
pub struct SheetEntrada {
    pub desc: String,
    pub valor: String,
    pub dia: String,
    pub tried: bool,
}

/// Folha do fechamento de um cartão conectado (aba Cartões, modo real).
#[derive(Clone, Default)]
pub struct FolhaFechamento {
    /// Id do cartão conectado (`ConnectedCard.id`), alvo do `Cmd::ClosingDay`.
    pub card: String,
    pub banco: String,
    /// Rascunho do dia (1-31; vazio = sem fechamento).
    pub dia: String,
    pub tried: bool,
}

#[derive(Clone)]
pub enum Sheet {
    /// Novo lançamento completo — UI e estado em `screens/lancamento.rs`.
    Lancamento(crate::screens::lancamento::Folha),
    /// Ajuste do dia de fechamento de um cartão conectado.
    Fechamento(FolhaFechamento),
    /// Ajustes (menu ☰): quem usa o celular, versão.
    Menu,
}

/// Sessão real com o backend (worker do desktop reutilizado).
pub struct Conexao {
    /// `client-N` autenticado; `None` até escolher ("Quem é você?").
    pub me: Option<String>,
    pub token: String,
    pub members: Vec<flounder::api::Member>,
    pub loaded: bool,
    pub worker: Option<Worker>,
    /// Falha de rede/credencial; some na próxima carga bem-sucedida.
    pub offline: Option<String>,
}

pub struct MobileApp {
    #[cfg(target_os = "android")]
    pub(crate) android_app: Option<winit::platform::android::activity::AndroidApp>,
    pub y: i32,
    pub m: u8,
    pub tab: Tab,
    /// Posição de quem usa o celular (muda o "você" nas listas).
    pub eu: usize,
    pub filtro_casa: Filtro,
    /// View-model das telas: convertida do `data` real ou do mock (demonstração).
    pub dados: mock::Mes,
    pub meses_demo: BTreeMap<Month, mock::Mes>,
    pub sheet: Option<Sheet>,
    /// Modo demonstração: dados do mock, nada gravado.
    pub demo: bool,
    /// Sessão real com o backend; `None` = tela de pareamento.
    pub sessao: Option<Conexao>,
    /// Tela de pareamento: campo de código e a chamada em andamento.
    pub parear_code: String,
    pub pareio: Option<Receiver<PairMsg>>,
    pub parear_erro: Option<String>,
    /// Dados reais (espelho do backend, como no desktop).
    pub data: Data,
    /// Cartões conectados (vêm na API, como no desktop).
    pub connected_cards: Option<Vec<flounder::connected::ConnectedCard>>,
    /// Faturas conectadas do espelho (para alternar pago nas contas).
    pub connected_invoices: Vec<flounder::model::ConnectedInvoice>,
    /// Agente GLM (análises e conversa), como no desktop.
    pub agent: flounder::assistant::Agent,
    /// Orçamentos mensais do domicílio (espelho do `api::State.budgets`).
    pub budgets: Vec<flounder::api::Budget>,
    /// Auditoria das ações do assistente (`Cmd::AssistantActions` → `Msg::Actions`).
    pub acoes: Option<Result<flounder::api::ActionLog, String>>,
    /// Já pediu as ações do assistente nesta sessão (pede uma vez só).
    pub acoes_pedidas: bool,
    /// Relatório da rotina do mês (`Cmd::Routine` → `Msg::Routine`), JSON tolerante.
    pub rotina: Option<Result<serde_json::Value, String>>,
    /// Esperando a rotina do mês rodar no backend.
    pub rotina_esperando: bool,
    /// Histórico da conversa já pedido ao backend (uma vez só por sessão).
    pub historico_pedido: bool,
}

impl Default for MobileApp {
    fn default() -> Self {
        Self::load(None)
    }
}

impl MobileApp {
    /// Sessão: token pareado (storage) → token do arquivo de config (dev) → pareamento.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        // Sessão: token pareado (storage) → token do arquivo de config (dev) → pareamento.
        let (client, token) = match storage.and_then(|s| s.get_string("member_token")) {
            Some(token) => (storage.and_then(|s| s.get_string("member_client")), token),
            None => match backend::env_token("client-1") {
                Some(token) => (None, token),
                None => (None, String::new()),
            },
        };
        let sessao = (!token.is_empty()).then(|| Conexao {
            me: client,
            token,
            members: Vec::new(),
            loaded: false,
            worker: None,
            offline: None,
        });
        let hoje = flounder::model::today();
        MobileApp {
            #[cfg(target_os = "android")]
            android_app: None,
            y: hoje.y,
            m: hoje.m as u8,
            // FLOUNDER_TAB=<aba> abre direto na aba (útil para testar a janela real).
            tab: match std::env::var("FLOUNDER_TAB").as_deref() {
                Ok("fluxo") => Tab::Fluxo,
                Ok("cartoes") => Tab::Cartoes,
                Ok("casa") => Tab::Casa,
                Ok("consultor") => Tab::Consultor,
                _ => Tab::Inicio,
            },
            eu: 0,
            filtro_casa: Filtro::Todas,
            dados: mock::para(hoje.y, hoje.m as u8),
            meses_demo: BTreeMap::new(),
            sheet: None,
            connected_cards: None,
            connected_invoices: Vec::new(),
            agent: Default::default(),
            // Demonstração começa com os orçamentos da história (mercado, lazer).
            budgets: sessao.is_none().then(mock::orcamentos_demo).unwrap_or_default(),
            acoes: None,
            acoes_pedidas: false,
            rotina: None,
            rotina_esperando: false,
            historico_pedido: false,
            demo: sessao.is_none(),
            sessao,
            parear_code: String::new(),
            pareio: None,
            parear_erro: None,
            data: Data::empty(),
        }
    }

    /// Envia um comando pela conexão (demo: sem efeito).
    pub fn enviar(&mut self, cmd: backend::Cmd) {
        if let Some(w) = self.sessao.as_mut().and_then(|c| c.worker.as_mut()) {
            w.save(cmd);
        }
    }

    /// Consulta ao worker (histórico, análises, conversa, ações, rotina), como o `ask` do
    /// desktop: esses comandos respondem com `Msg::History`/`Reports`/`Chat`/`Actions`/
    /// `Routine` e nunca com `Msg::Done`, então não podem contar em `pending` do worker
    /// (contador de gravações sem resposta). Demo: sem efeito.
    pub fn consultar(&mut self, cmd: backend::Cmd) {
        if let Some(w) = self.sessao.as_mut().and_then(|c| c.worker.as_mut()) {
            w.ask(cmd);
        }
    }

    // ─── Ganchos de escrita usados pelas telas (demo grava no mock; real no backend) ──

    /// Salva um lançamento novo ou editado e espelha na view-model.
    pub fn salvar_entrada(&mut self, entrada: flounder::model::Entry) {
        if self.demo {
            let dia = entrada.date.d;
            let id = if entrada.id == 0 {
                self.dados.fluxo.iter().flat_map(|d| &d.entradas).map(|e| e.id).max().unwrap_or(0) + 1
            } else {
                self.dados.remover_lancamento(entrada.id);
                entrada.id
            };
            let mock_e = mock::Entrada {
                id,
                desc: entrada.desc.clone(),
                categoria: entrada.category,
                centavos: if entrada.kind == Kind::Income { entrada.amount } else { -entrada.amount },
                membro: entrada.owner.idx(),
                shared: entrada.shared,
                card: entrada.connected.as_deref().and_then(|id| {
                    self.connected_cards.as_ref().and_then(|cs| cs.iter().find(|c| c.id == *id)).map(|c| c.institution.clone())
                }),
                paid: entrada.paid,
            };
            self.dados.ajustar_fluxo(mock_e.centavos, 1);
            match self.dados.fluxo.iter_mut().find(|d| d.dia == dia as u8) {
                Some(d) => d.entradas.push(mock_e),
                None => self.dados.fluxo.push(mock::DiaFluxo { dia: dia as u8, entradas: vec![mock_e] }),
            }
            self.dados.fluxo.sort_by_key(|d| d.dia);
            return;
        }
        let id = self.data.upsert(entrada);
        if let Some(e) = self.data.entry(id) {
            self.enviar(backend::Cmd::Entry(e.clone()));
        }
        self.atualizar_mes();
    }

    /// Marca/desmarca pago (lançamento ou fatura) e grava via worker.
    pub fn alternar_pago(&mut self, acao: &mock::Acao) {
        let month = Month { y: self.y, m: self.m as u32 };
        match acao {
            mock::Acao::Entrada(id) => {
                self.data.toggle(BillRef::Entry(*id), month);
                if let Some(e) = self.data.entry(*id) {
                    self.enviar(backend::Cmd::Entry(e.clone()));
                }
            }
            mock::Acao::Fatura { card, due } => {
                let Some(pos) = self.connected_invoices.iter().position(|inv| {
                    inv.card == *card && format!("{:04}-{:02}-{:02}", inv.due.y, inv.due.m, inv.due.d) == *due
                }) else { return };
                self.data.toggle(BillRef::Connected(pos), month);
                let pago = self.data.connected_invoices[pos].paid;
                self.enviar(backend::Cmd::BillPaid(flounder::api::BillPaid {
                    card: card.clone(),
                    due: due.clone(),
                    paid: pago,
                }));
            }
            mock::Acao::Nenhuma => {}
        }
        self.atualizar_mes();
    }

    /// Exclui um lançamento e grava via worker.
    pub fn excluir_entrada(&mut self, id: u32) {
        if self.demo {
            self.dados.remover_lancamento(id);
            return;
        }
        self.data.entries.retain(|e| e.id != id);
        self.enviar(backend::Cmd::Delete(id));
        self.atualizar_mes();
    }

    /// Dia de fechamento de um cartão conectado (`None` volta ao mês civil): otimista no
    /// espelho (a interface já mostra o dia novo) e gravado via worker, como no desktop.
    pub fn definir_fechamento(&mut self, card: &str, day: Option<u32>) {
        if let Some(c) = self.connected_cards.as_mut().and_then(|cs| cs.iter_mut().find(|c| c.id == card)) {
            c.closing_day = day;
        }
        self.enviar(backend::Cmd::ClosingDay { card: card.to_owned(), day });
        self.atualizar_mes();
    }

    /// Copia os lançamentos manuais do mês anterior (como o desktop).
    pub fn copiar_mes_anterior(&mut self) {
        if self.demo {
            return;
        }
        let month = Month { y: self.y, m: self.m as u32 };
        for id in self.data.copy_previous(month, None) {
            if let Some(e) = self.data.entry(id) {
                self.enviar(backend::Cmd::Entry(e.clone()));
            }
        }
        self.atualizar_mes();
    }

    /// Registra um acerto (quem pagou a mais) e grava via worker.
    pub fn registrar_acerto(&mut self, valor_cents: i64, de: usize) {
        if self.demo {
            self.dados.casa.saldo_acerto += if de == 0 { valor_cents } else { -valor_cents };
            let proximo = self.dados.casa.acertos.iter()
                .filter_map(|a| a.id.strip_prefix("local-demo-")?.parse::<u32>().ok())
                .max().unwrap_or(0) + 1;
            self.dados.casa.acertos.push(mock::Acerto {
                id: format!("local-demo-{proximo}"),
                de,
                valor_cents,
                dia: flounder::model::today().d as u8,
                mes: self.m,
                ano: self.y,
            });
            return;
        }
        let hoje = flounder::model::today();
        let de_p = if de == 1 { Person::B } else { Person::A };
        let acerto = flounder::model::Settlement { id: String::new(), date: hoje, amount: valor_cents, from: de_p };
        let id = self.data.add_settlement(acerto.clone());
        self.enviar(backend::Cmd::Settle(flounder::model::Settlement { id, date: hoje, amount: valor_cents, from: de_p }));
        self.atualizar_mes();
    }

    /// Desfaz um acerto pelo id e grava via worker.
    pub fn desfazer_acerto(&mut self, id: String) {
        if self.demo {
            if let Some(pos) = self.dados.casa.acertos.iter().position(|a| a.id == id) {
                let acerto = self.dados.casa.acertos.remove(pos);
                self.dados.casa.saldo_acerto -= if acerto.de == 0 { acerto.valor_cents } else { -acerto.valor_cents };
            }
            return;
        }
        self.data.settlements.retain(|a| a.id != id);
        self.enviar(backend::Cmd::Unsettle(id));
        self.atualizar_mes();
    }

    /// Troca quem usa o celular quando há token do membro; senão pede re-pareamento.
    pub fn trocar_membro(&mut self, client: String) -> bool {
        if self.demo {
            self.eu = if client == "client-2" { 1 } else { 0 };
            self.data.me = if client == "client-2" { Person::B } else { Person::A };
            return true;
        }
        let Some(cx) = self.sessao.as_mut() else { return false };
        let token = backend::env_token(&client).unwrap_or_default();
        if token.is_empty() {
            return false;
        }
        *cx = Conexao { me: Some(client), token, ..Default::default() };
        self.eu = if cx.me.as_deref() == Some("client-2") { 1 } else { 0 };
        self.atualizar_mes();
        true
    }

    /// Recalcula a view-model do mês aberto (real ou demo).
    pub fn atualizar_mes(&mut self) {
        let month = Month { y: self.y, m: self.m as u32 };
        self.dados = if self.demo {
            mock::para(self.y, self.m)
        } else {
            let eu = self.eu;
            let cartoes = self.connected_cards.as_deref();
            converter::mes(&self.data, cartoes, month, eu, &self.budgets)
        };
    }

    /// Navega de mês; o worker continua de olho no backend e a view-model se recalcula.
    pub fn andar_mes(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let atual = Month { y: self.y, m: self.m.into() };
        let novo = atual.add(delta);
        self.y = novo.y;
        self.m = novo.m as u8;
        if self.demo {
            let dados = self.meses_demo.remove(&novo).unwrap_or_else(|| mock::para(self.y, self.m));
            self.meses_demo.insert(atual, std::mem::replace(&mut self.dados, dados));
        } else {
            self.atualizar_mes();
        }
    }

    pub fn mes_label(&self) -> String {
        const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
        format!("{} {}", MESES[(self.m - 1) as usize], self.y)
    }

    /// Espelho do `App::apply` do desktop: `api::State` → `Data`.
    fn aplicar(&mut self, state: flounder::api::State, entries: Vec<flounder::model::Entry>) {
        for m in &state.members {
            if let Some(n) = self.data.names.get_mut(m.position as usize) {
                n.clone_from(&m.name);
            }
        }
        self.data.me = if state.me == 1 { Person::B } else { Person::A };
        self.eu = self.data.me.idx();
        self.data.entries = entries;
        self.data.settlements = state
            .settlements
            .iter()
            .filter(|s| s.from != s.to && s.from <= 1 && s.to <= 1)
            .filter_map(|s| Some(flounder::model::Settlement {
                id: s.id.clone(),
                date: flounder::model::Date::iso(&s.date)?,
                amount: s.amount_cents,
                from: if s.from == 1 { Person::B } else { Person::A },
            }))
            .collect();
        self.connected_cards = Some(state.cards);
        self.data.shared_split_off = state.shared_split_off;
        self.budgets = state.budgets.clone();
        if let Some(cx) = self.sessao.as_mut() {
            cx.members = state.members.clone();
            cx.loaded = true;
            cx.me = state.members.get(state.me as usize).map(|m| m.client.clone());
        }
        self.atualizar_mes();
    }

    /// Drena o worker e reflete o estado no `data` (mesmo fluxo do desktop).
    fn poll_remoto(&mut self, ctx: &egui::Context) {
        // Worker nasce no primeiro quadro (client e token resolvidos uma vez).
        if self.sessao.as_ref().and_then(|c| c.worker.as_ref()).is_none() {
            let Some(cx) = self.sessao.as_mut() else { return };
            let client = cx.me.clone().unwrap_or_else(|| "client-1".into());
            let token = if cx.token.is_empty() { backend::env_token(&client).unwrap_or_default() } else { cx.token.clone() };
            if token.is_empty() {
                cx.offline = Some("Não há token deste membro neste computador.".into());
                return;
            }
            cx.worker = Some(Worker::start(client, token, backend::base_url(), ctx.clone()));
        }
        let mut carregar = None;
        let mut nota = None;
        if let Some(w) = self.sessao.as_mut().and_then(|c| c.worker.as_mut()) {
            while let Ok(msg) = w.rx.try_recv() {
                match msg {
                    backend::Msg::State(state, entries) => carregar = Some((*state, entries)),
                    backend::Msg::Offline(e) | backend::Msg::Done(Some(e)) => nota = Some(e),
                    backend::Msg::Reports(r) => self.agent.on_reports(r),
                    backend::Msg::Chat(result) => self.agent.on_chat(result),
                    // Histórico da conversa persistido: o agente enche o chat se o local está vazio.
                    backend::Msg::History(result) => self.agent.on_history(result),
                    // Auditoria das ações do assistente e relatório da rotina: ficam com a
                    // folha de Ajustes (que desenha o Err como aviso, no lugar).
                    backend::Msg::Actions(result) => self.acoes = Some(result),
                    backend::Msg::Routine(result) => {
                        self.rotina = Some(result);
                        self.rotina_esperando = false;
                    }
                    _ => {}
                }
            }
        }
        if let Some(cx) = self.sessao.as_mut() {
            if let Some(e) = nota {
                cx.offline = Some(e);
            }
        }
        if let Some((state, entries)) = carregar {
            self.aplicar(state, entries);
        }
    }

    /// Drena a resposta do pareamento e abre a sessão com o token recebido.
    fn poll_pareio(&mut self) {
        let Some(rx) = self.pareio.take() else { return };
        let mut ok = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                PairMsg::Paired { client, token } => ok = Some((client, token)),
                PairMsg::Failed(e) => {
                    self.parear_erro = Some(e);
                }
            }
        }
        if let Some((client, token)) = ok {
            self.parear_code.clear();
            self.parear_erro = None;
            self.sessao = Some(Conexao {
                me: Some(client),
                token,
                members: Vec::new(),
                loaded: false,
                worker: None,
                offline: None,
            });
        } else if self.parear_erro.is_none() {
            // Ainda esperando o servidor: devolve o receiver para o próximo quadro.
            self.pareio = Some(rx);
        }
    }

    /// Escolhe quem usa o celular (token do arquivo de config por membro).
    pub fn escolher(&mut self, client: String) {
        let Some(cx) = self.sessao.as_mut() else { return };
        let same = cx.me.as_deref() == Some(client.as_str());
        cx.me = Some(client);
        if same {
            return;
        }
        *cx = Conexao {
            me: cx.me.take(),
            token: cx.token.clone(),
            ..Default::default()
        };
        if cx.token.is_empty() {
            let client = cx.me.clone().unwrap_or_default();
            cx.token = backend::env_token(&client).unwrap_or_default();
        }
        self.eu = 0;
    }
}

impl Default for Conexao {
    fn default() -> Self {
        Conexao { me: None, token: String::new(), members: Vec::new(), loaded: false, worker: None, offline: None }
    }
}

/// Formata centavos como "1.234,56" (sinal só na chamada com −).
/// Delega ao núcleo: mesma aritmética inteira do desktop (`model::input_money`).
pub fn fmt_money(cents: i64) -> String {
    flounder::model::input_money(cents)
}

pub fn update(app: &mut MobileApp, ui: &mut Ui) {
    app.poll_pareio();
    app.poll_remoto(ui.ctx());
    // Trocou aba ou mês: a cascata de entrada dos cartões reinicia.
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (app.y, app.m, std::mem::discriminant(&app.tab)).hash(&mut hash);
    flounder::widgets::begin_frame(ui.ctx(), hash.finish());
    // Estados: pareamento (sem credencial) → conectando (sessão sem carga) → conteúdo.
    let pareando = app.sessao.is_none() && !app.demo;
    let carregando = !pareando && !app.demo && !app.sessao.as_ref().is_some_and(|c| c.loaded);
    topo(ui, app, !pareando);
    if !pareando {
        tabbar(ui, app);
    }
    egui::CentralPanel::default()
        .frame(Frame::new().fill(theme::cores().bg).inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 10 }))
        .show(ui, |ui| {
            if pareando {
                return tela_pareamento(ui, app);
            }
            if carregando {
                return tela_conectando(ui, app);
            }
            mes_barra(ui, app);
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .id_salt((std::mem::discriminant(&app.tab), app.y, app.m))
                .auto_shrink(false)
                .show(ui, |ui| {
                    match app.tab {
                        Tab::Inicio if app.dados.vazio => {
                            theme::vazio(ui, "🐟", &format!("Nada por aqui em {}.", app.mes_label()));
                        }
                        Tab::Inicio => crate::screens::home::show(ui, app),
                        Tab::Fluxo => crate::screens::flow::show(ui, app),
                        Tab::Cartoes => crate::screens::cards::show(ui, app),
                        Tab::Casa => crate::screens::house::show(ui, app),
                        Tab::Consultor => crate::screens::consultor::show(ui, app),
                    }
                });
        });
    if !pareando {
        modal_quem(ui, app);
        folha(ui, app);
    }
}

/// Tela de pareamento: código de acesso (uma vez só) ou demonstração.
fn tela_pareamento(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let mut conectar = false;
    let mut demo = false;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(40.0);
        theme::card(ui, |ui| {
            ui.vertical_centered(|ui| {
                w::art(ui, w::Art::Fish, 80.0);
                w::text(ui, "Conectar ao servidor", w::bold(20.0), c.ink);
                w::caption(ui, "Cole o código de acesso que quem te convidou mandou. Ele fica guardado neste celular — isso aqui é uma vez só.");
                ui.add_space(14.0);
                if app.pareio.is_some() {
                    w::caption(ui, format!("{} Conectando…", icon::HOURGLASS));
                } else {
                    w::field(ui, "Código de acesso", None, |ui| {
                        let response = ui.add(w::text_input(&mut app.parear_code, "Ex.: MARIA-2026-FLOUNDER"));
                        if app.parear_code.is_empty() {
                            response.request_focus();
                        }
                        response
                    });
                    if let Some(e) = &app.parear_erro {
                        ui.label(egui::RichText::new(e.clone()).font(w::regular(13.0)).color(c.red));
                    }
                    ui.add_space(10.0);
                    conectar = w::primary_button(ui, "Conectar").clicked();
                    ui.add_space(6.0);
                    demo = ui.button(egui::RichText::new("Explorar demonstração").font(w::semibold(13.0)).color(c.blue)).clicked();
                }
            });
        });
    });
    if conectar {
        if app.parear_code.trim().is_empty() {
            app.parear_erro = Some("Informe o código de acesso.".into());
        } else {
            app.parear_erro = None;
            app.pareio = Some(backend::send_pair(&backend::base_url(), &app.parear_code, ui.ctx().clone()));
        }
    }
    if demo {
        app.demo = true;
        if app.budgets.is_empty() {
            app.budgets = mock::orcamentos_demo();
        }
        app.atualizar_mes();
    }
}

/// Tela de conexão: carregando ou erro com nova tentativa.
fn tela_conectando(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let Some(cx) = app.sessao.as_ref() else { return };
    let mut tentar = false;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(40.0);
        theme::card(ui, |ui| {
            ui.vertical_centered(|ui| {
                w::art(ui, w::Art::Fish, 80.0);
                match &cx.offline {
                    Some(erro) => {
                        w::text(ui, "Sem conexão com o backend", w::bold(20.0), c.ink);
                        w::caption(ui, erro.clone());
                        ui.add_space(8.0);
                        tentar = w::primary_button(ui, "Tentar novamente").clicked();
                    }
                    None => {
                        w::text(ui, "Carregando seus dados…", w::bold(20.0), c.ink);
                        w::caption(ui, format!("Conectando ao backend em {}", backend::base_url()));
                    }
                }
            });
        });
    });
    if tentar {
        if let Some(cx) = app.sessao.as_mut() {
            cx.worker = None;
            cx.offline = None;
        }
    }
}

/// "Quem é você?" — só na sessão por arquivo de config, antes de escolher.
fn modal_quem(ui: &mut Ui, app: &mut MobileApp) {
    let Some(cx) = app.sessao.as_ref() else { return };
    if cx.me.is_some() || !cx.loaded || cx.members.is_empty() {
        return;
    }
    let c = theme::cores();
    let mut escolhido = None;
    // Modal centraliza sozinho e bloqueia o que está por baixo (padrão do desktop, fn form_modal).
    egui::Modal::new(egui::Id::new("quem"))
        .frame(Frame::new().fill(c.card).corner_radius(egui::CornerRadius { nw: 24, ne: 24, sw: 24, se: 24 }).inner_margin(egui::Margin::same(20)))
        .show(ui.ctx(), |ui| {
            ui.set_width(320.0);
            ui.label(egui::RichText::new("Quem é você?").font(w::bold(18.0)).color(c.ink));
            ui.label(egui::RichText::new("Escolha quem usa este celular.").font(w::regular(13.0)).color(c.muted));
            ui.add_space(10.0);
            for m in &cx.members {
                if w::primary_button(ui, &m.name).clicked() {
                    escolhido = Some(m.client.clone());
                }
            }
        });
    if let Some(client) = escolhido {
        app.escolher(client);
    }
}

impl eframe::App for MobileApp {
    #[cfg(target_os = "android")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let (Some(app), Some(screen)) = (&self.android_app, input.screen_rect) else { return };
        let rect = app.content_rect();
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return;
        }
        let scale = input.viewport().native_pixels_per_point.unwrap_or(1.0) * ctx.zoom_factor();
        input.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: rect.left.max(0) as f32 / scale,
            right: (screen.width() - rect.right as f32 / scale).max(0.0),
            top: rect.top.max(0) as f32 / scale,
            bottom: (screen.height() - rect.bottom as f32 / scale).max(0.0),
        }));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let seguro = ui.ctx().content_rect();
        if seguro == ui.max_rect() {
            update(self, ui);
            return;
        }
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::cores().pill);
        ui.scope_builder(egui::UiBuilder::new().max_rect(seguro), |ui| {
            ui.set_clip_rect(seguro);
            update(self, ui);
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(cx) = self.sessao.as_ref() {
            if let Some(me) = &cx.me {
                storage.set_string("member_client", me.clone());
            }
            storage.set_string("member_token", cx.token.clone());
        }
    }
}

/// Topo: menu, avatares, nome do app e ação de adicionar.
fn topo(parent: &mut Ui, app: &mut MobileApp, mostrar_adicionar: bool) {
    egui::Panel::top("topo")
        .frame(Frame::new().fill(theme::cores().card).inner_margin(egui::Margin { left: 16, right: 16, top: 10, bottom: 12 }))
        .show(parent, |ui| {
            ui.horizontal(|ui| {
                let c = theme::cores();
                if botao_icone(ui, icon::LIST, "Menu") {
                    app.sheet = Some(Sheet::Menu);
                }
                let (nome_eu, cor_eu) = MEMBROS[app.eu];
                let (nome_outro, cor_outro) = MEMBROS[1 - app.eu];
                let (inicial_eu, inicial_outro) = (nome_eu.chars().next().unwrap(), nome_outro.chars().next().unwrap());
                ui.add_space(6.0);
                let y = ui.cursor().center().y;
                theme::avatar(ui, egui::pos2(ui.cursor().left() + 14.0, y), 13.0, &inicial_eu.to_string(), cor_eu);
                theme::avatar(ui, egui::pos2(ui.cursor().left() + 30.0, y), 13.0, &inicial_outro.to_string(), cor_outro);
                ui.allocate_space(Vec2::splat(30.0));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Flounder").font(w::bold(17.0)).color(c.ink));
                    ui.label(egui::RichText::new("Finanças em casal").font(w::regular(12.0)).color(c.muted));
                });
                if mostrar_adicionar {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let adicionar = egui::Button::new(
                            egui::RichText::new(icon::PLUS).font(w::bold(22.0)).color(Color32::WHITE),
                        ).fill(c.blue).corner_radius(14);
                        if ui.add_sized([44.0, 44.0], adicionar).on_hover_text("Novo lançamento").clicked() {
                            app.sheet = Some(Sheet::Lancamento(crate::screens::lancamento::Folha {
                                tipo_despesa: true,
                                ..Default::default()
                            }));
                        }
                    });
                }
            });
        });
}

fn botao_icone(ui: &mut Ui, glifo: &str, _dica: &str) -> bool {
    let c = theme::cores();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 12.0, c.line);
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glifo.to_string(), w::regular(20.0), c.ink);
    resp.clicked()
}

/// Barra branca arredondada: ‹ mês ›.
fn mes_barra(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    Frame::new()
        .fill(c.card)
        .corner_radius(18)
        .shadow(egui::Shadow { offset: [0, 8], blur: 22, spread: 0, color: Color32::from_rgba_unmultiplied(23, 43, 99, 14) })
        .inner_margin(egui::Margin::symmetric(6, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if botao_seta(ui, icon::CARET_LEFT) {
                        app.andar_mes(-1);
                    }
                });
                ui.with_layout(Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                    let mut job = egui::text::LayoutJob::default();
                    job.append(&app.mes_label(), 0.0, egui::TextFormat { font_id: w::bold(19.0), color: c.ink, ..Default::default() });
                    ui.add(egui::Label::new(job));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if botao_seta(ui, icon::CARET_RIGHT) {
                        app.andar_mes(1);
                    }
                });
            });
        });
}

fn botao_seta(ui: &mut Ui, glifo: &str) -> bool {
    let c = theme::cores();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::click());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glifo.to_string(), w::bold(16.0), c.muted);
    resp.clicked()
}

/// Navegação inferior com cinco abas em uma linha.
fn tabbar(parent: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    egui::Panel::bottom("tabbar")
        .frame(Frame::new().fill(c.bg).inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 8 }))
        .exact_size(64.0)
        .show(parent, |ui| {
            Frame::new()
                .fill(c.card)
                .corner_radius(20)
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let abas = [
                            (Tab::Inicio, icon::HOUSE_LINE, "Início"),
                            (Tab::Fluxo, icon::CHART_BAR, "Fluxo"),
                            (Tab::Cartoes, icon::CREDIT_CARD, "Cartões"),
                            (Tab::Casa, icon::HOUSE_SIMPLE, "Casa"),
                            (Tab::Consultor, icon::SPARKLE, "Consultor"),
                        ];
                        let largura = ui.available_width() / abas.len() as f32;
                        for (tab, glifo, rotulo) in abas {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(largura, 44.0), egui::Sense::click());
                            let ativa = std::mem::discriminant(&app.tab) == std::mem::discriminant(&tab);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, ativa, rotulo));
                            if ativa {
                                let destaque = egui::Rect::from_center_size(rect.center(), Vec2::new(largura.min(52.0), 40.0));
                                ui.painter().rect_filled(destaque, 14.0, c.pill);
                            }
                            let cor = if ativa { Color32::WHITE } else { c.muted };
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, glifo, w::regular(22.0), cor);
                            if resp.on_hover_text(rotulo).clicked() {
                                app.tab = tab;
                            }
                        }
                    });
                });
        });
}

/// Folha de lançamento, cartão ou ajustes.
fn folha(parent: &mut Ui, app: &mut MobileApp) {
    let Some(sheet) = app.sheet.clone() else { return };
    let c = theme::cores();
    let area_segura = parent.ctx().content_rect();
    let id = egui::Id::new(("folha", std::mem::discriminant(&sheet)));
    let area = egui::Modal::default_area(id)
        .default_size(area_segura.size())
        .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO);
    let frame = Frame::new()
        .fill(c.card)
        .corner_radius(egui::CornerRadius { nw: 24, ne: 24, ..Default::default() })
        .inner_margin(egui::Margin { left: 18, right: 18, top: 18, bottom: 20 });
    let modal = egui::Modal::new(id)
        .area(area)
        .frame(frame)
        .backdrop_color(Color32::from_rgba_unmultiplied(16, 24, 40, 115))
        .show(parent.ctx(), |ui| {
            ui.set_width(area_segura.width() - frame.inner_margin.sum().x);
            let altura = (area_segura.height() - 86.0).max(100.0);
            ui.set_max_height(altura);
            egui::ScrollArea::vertical()
                .max_height(altura)
                .auto_shrink([false, true])
                .show(ui, |ui| match &sheet {
                    Sheet::Lancamento(folha) => crate::screens::lancamento::folha(ui, app, folha),
                    Sheet::Fechamento(sheet) => folha_fechamento(ui, app, sheet),
                    Sheet::Menu => crate::screens::ajustes::folha(ui, app),
                });
        });
    if modal.should_close() {
        app.sheet = None;
    }
}

/// Folha do fechamento de um cartão conectado: dia (1-31), "Aplicar" e "Remover ajuste"
/// (mesmo conteúdo do modal do desktop). Grava em `MobileApp::definir_fechamento`.
fn folha_fechamento(ui: &mut Ui, app: &mut MobileApp, sheet: &FolhaFechamento) {
    let c = theme::cores();
    ui.label(egui::RichText::new("Fechamento do cartão").font(w::bold(17.0)).color(c.ink));
    ui.label(egui::RichText::new(&sheet.banco).font(w::regular(13.0)).color(c.muted));
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("Compras feitas no dia do fechamento ou depois caem na fatura seguinte.")
            .font(w::regular(12.0))
            .color(c.muted),
    );
    ui.add_space(8.0);
    let mut dia = sheet.dia.clone();
    theme::campo(ui, "Fecha no dia (1-31; vazio = mês civil)", &mut dia, "Ex.: 20", false);
    if let Some(Sheet::Fechamento(s)) = app.sheet.as_mut() {
        s.dia = dia;
    }
    if sheet.tried {
        ui.label(egui::RichText::new("Use um dia de 1 a 31.").font(w::regular(12.0)).color(c.red));
    }
    ui.add_space(14.0);
    // "Remover ajuste" só quando o cartão tem fechamento aplicado (igual ao desktop).
    let com_ajuste = app
        .connected_cards
        .as_ref()
        .and_then(|cs| cs.iter().find(|c| c.id == sheet.card))
        .is_some_and(|cc| cc.closing_day.is_some());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let partes = if com_ajuste { 3.0 } else { 2.0 };
        if ui.add_sized([ui.available_width() / partes - 5.0, 44.0], egui::Button::new(egui::RichText::new("Cancelar").font(w::semibold(15.0)).color(c.ink)).fill(c.line).corner_radius(14)).clicked() {
            app.sheet = None;
        }
        if com_ajuste {
            let largura = ui.available_width() / 2.0 - 5.0;
            if ui.add_sized([largura, 44.0], egui::Button::new(egui::RichText::new(format!("{} Remover ajuste", icon::TRASH)).font(w::semibold(13.5)).color(c.red)).fill(c.red_soft).corner_radius(14)).clicked() {
                let card = sheet.card.clone();
                app.sheet = None;
                app.definir_fechamento(&card, None);
            }
        }
        let largura = ui.available_width();
        if ui.add_sized([largura, 44.0], egui::Button::new(egui::RichText::new("Aplicar").font(w::bold(15.0)).color(Color32::WHITE)).fill(c.blue).corner_radius(14)).clicked() {
            aplicar_fechamento(app);
        }
    });
}

/// Aplica o dia da folha do fechamento (1-31); inválido mostra o aviso e a folha fica aberta.
fn aplicar_fechamento(app: &mut MobileApp) {
    let Some(Sheet::Fechamento(sheet)) = app.sheet.as_ref().cloned() else { return };
    let dia: u32 = sheet.dia.trim().parse().unwrap_or(0);
    if dia == 0 || dia > 31 {
        if let Some(Sheet::Fechamento(s)) = app.sheet.as_mut() {
            s.tried = true;
        }
        return;
    }
    app.sheet = None;
    app.definir_fechamento(&sheet.card, Some(dia));
}
