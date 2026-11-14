//! Casca do Flounder mobile: topo, barra do mês, tab bar flutuante, FAB, telas e a sessão
//! real com o backend (worker do desktop + pareamento por código; fase 1 vira demonstração).

use egui::{Align, Align2, Color32, Frame, Layout, Ui, Vec2, pos2};
use flounder::backend::{self, PairMsg, Worker};
use flounder::model::{BillRef, Data, Entry, Kind, Month, Person};
use flounder::widgets as w;
use egui_phosphor::regular as icon;
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

/// Folha de novo cartão (aba Cartões, só na demonstração: cartões reais vêm da Pluggy).
#[derive(Clone, Default)]
pub struct SheetCartao {
    pub banco: String,
    pub limite: String,
    pub venc: String,
    pub fecha: String,
    pub tried: bool,
}

#[derive(Clone)]
pub enum Sheet {
    /// Novo lançamento completo — UI e estado em `screens/lancamento.rs`.
    Lancamento(crate::screens::lancamento::Folha),
    Cartao(SheetCartao),
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
    pub y: i32,
    pub m: u8,
    pub tab: Tab,
    /// Posição de quem usa o celular (muda o "você" nas listas).
    pub eu: usize,
    pub filtro_casa: Filtro,
    /// View-model das telas: convertida do `data` real ou do mock (demonstração).
    pub dados: mock::Mes,
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
            sheet: None,
            connected_cards: None,
            connected_invoices: Vec::new(),
            agent: Default::default(),
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

    // ─── Ganchos de escrita usados pelas telas (demo grava no mock; real no backend) ──

    /// Salva um lançamento novo ou editado e espelha na view-model.
    pub fn salvar_entrada(&mut self, entrada: flounder::model::Entry) {
        if self.demo {
            let dia = entrada.date.d;
            let mock_e = mock::Entrada {
                id: 0,
                desc: entrada.desc.clone(),
                centavos: if entrada.kind == Kind::Income { entrada.amount } else { -entrada.amount },
                membro: entrada.owner.idx(),
                shared: entrada.shared,
                card: entrada.connected.as_deref().and_then(|id| {
                    self.connected_cards.as_ref().and_then(|cs| cs.iter().find(|c| c.id == *id)).map(|c| c.institution.clone())
                }),
                paid: entrada.paid,
            };
            match self.dados.fluxo.iter_mut().find(|d| d.dia == dia as u8) {
                Some(d) => d.entradas.push(mock_e),
                None => self.dados.fluxo.push(mock::DiaFluxo { dia: dia as u8, entradas: vec![mock_e] }),
            }
            self.dados.fluxo.sort_by_key(|d| d.dia);
            self.atualizar_mes();
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
            for d in &mut self.dados.fluxo {
                d.entradas.retain(|e| e.id != id);
            }
            self.dados.fluxo.retain(|d| !d.entradas.is_empty());
            self.atualizar_mes();
            return;
        }
        self.data.entries.retain(|e| e.id != id);
        self.enviar(backend::Cmd::Delete(id));
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
            self.dados.casa.acertos.push(mock::Acerto {
                id: format!("local-demo-{}", self.dados.casa.acertos.len()),
                de,
                valor_cents,
                dia: flounder::model::today().d as u8,
                mes: self.m,
                ano: self.y,
            });
            self.atualizar_mes();
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
            self.dados.casa.acertos.retain(|a| a.id != id);
            self.atualizar_mes();
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
            self.atualizar_mes();
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
            converter::mes(&self.data, cartoes, month, eu)
        };
    }

    /// Navega de mês; o worker continua de olho no backend e a view-model se recalcula.
    pub fn andar_mes(&mut self, delta: i32) {
        self.m = ((self.m as i32 - 1 + delta).rem_euclid(12) + 1) as u8;
        if delta < 0 && self.m == 12 {
            self.y -= 1;
        }
        if delta > 0 && self.m == 1 {
            self.y += 1;
        }
        self.atualizar_mes();
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
pub fn fmt_money(cents: i64) -> String {
    let abs = cents.abs();
    // Centavos por aritmética inteira: f64 to_string come o zero à direita ("7999.7").
    let centavos = format!("{:02}", abs % 100);
    let inteiro = (abs / 100).to_string();
    let mut grupos = String::new();
    for (i, c) in inteiro.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grupos.push('.');
        }
        grupos.push(c);
    }
    let inteiro = grupos.chars().rev().collect::<String>();
    format!("{inteiro},{centavos}")
}

pub fn update(app: &mut MobileApp, ui: &mut Ui) {
    app.poll_pareio();
    app.poll_remoto(ui.ctx());
    // Trocou aba ou mês: a cascata de entrada dos cartões reinicia.
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (app.y, app.m, std::mem::discriminant(&app.tab)).hash(&mut hash);
    flounder::widgets::begin_frame(ui.ctx(), hash.finish());
    topo(ui, app);
    // Estados: pareamento (sem credencial) → conectando (sessão sem carga) → conteúdo.
    let pareando = app.sessao.is_none() && !app.demo;
    let carregando = !pareando && !app.demo && !app.sessao.as_ref().is_some_and(|c| c.loaded);
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
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if app.dados.vazio {
                    theme::vazio(ui, "🐟", &format!("Nada por aqui em {}.", app.mes_label()));
                    return;
                }
                match app.tab {
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
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        update(self, ui);
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

/// Topo: menu, avatares, nome e sininho — faixa branca inteira.
fn topo(parent: &mut Ui, app: &mut MobileApp) {
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
                    ui.set_min_size(Vec2::new(ui.available_width() - 44.0, 0.0));
                    ui.label(egui::RichText::new("Flounder").font(w::bold(17.0)).color(c.ink));
                    ui.label(egui::RichText::new("Finanças em casal").font(w::regular(12.0)).color(c.muted));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let badge = "10";
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::click());
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::BELL.to_string(), w::regular(20.0), c.ink);
                    ui.painter().circle_filled(pos2(rect.right() - 8.0, rect.top() + 8.0), 9.0, c.red);
                    ui.painter().text(pos2(rect.right() - 8.0, rect.top() + 8.0), Align2::CENTER_CENTER, badge, w::bold(9.0), Color32::WHITE);
                    if resp.clicked() {
                        // Avisos chegam na fase com API.
                    }
                });
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

/// Tab bar flutuante branca + FAB azul, ancorados no rodapé.
fn tabbar(parent: &mut Ui, app: &mut MobileApp) {
    egui::Panel::bottom("tabbar")
        .frame(Frame::new().fill(theme::cores().bg).inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 12 }))
        .exact_size(74.0)
        .show(parent, |ui| {
            ui.horizontal(|ui| {
                let c = theme::cores();
                // Pílula branca com as 5 abas (deixa espaço para o FAB).
                Frame::new()
                    .fill(c.card)
                    .corner_radius(255)
                    .shadow(egui::Shadow { offset: [0, 12], blur: 34, spread: 0, color: Color32::from_rgba_unmultiplied(23, 43, 99, 45) })
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        // Limita min E max: nada dentro da pílula estica e empurra o FAB pra fora.
                        let largura = (ui.available_width() - 100.0).max(0.0);
                        ui.set_min_size(Vec2::new(largura, 50.0));
                        ui.set_max_size(Vec2::new(largura, 50.0));
                        let aba_largura = largura / 5.0;
                        let abas = [
                            (Tab::Inicio, icon::HOUSE_LINE, "Início"),
                            (Tab::Fluxo, icon::CHART_BAR, "Fluxo"),
                            (Tab::Cartoes, icon::CREDIT_CARD, "Cartões"),
                            (Tab::Casa, icon::HOUSE_SIMPLE, "Casa"),
                            (Tab::Consultor, icon::SPARKLE, "Consultor"),
                        ];
                        // Pílula da seleção: slot criado antes do loop (fica por baixo dos ícones) e
                        // preenchido no fim com o rect real da aba escolhida — mesmo idioma da sidebar
                        // do desktop. O rect gravado evita recalcular margens e larguras na mão.
                        let pill = ui.painter().add(egui::Shape::Noop);
                        let mut rect_sel = None;
                        // Layout explícito e abas sem espaçamento entre si: o conteúdo herdaria o
                        // layout horizontal de fora com 8 px entre cada aba (a pílula "andava" 8 px
                        // por aba e a pílula branca esticava 32 px a mais que o combinado com o FAB).
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for (tab, glifo, label) in abas {
                                let on = std::mem::discriminant(&app.tab) == std::mem::discriminant(&tab);
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(aba_largura, 50.0), egui::Sense::click());
                                if on {
                                    rect_sel = Some(rect);
                                }
                                let centro = rect.center();
                                let cor = if on { Color32::WHITE } else { c.faint };
                                let (y_icone, y_label) = if on { (centro.y - 5.0, centro.y + 12.0) } else { (centro.y - 7.0, centro.y + 13.0) };
                                ui.painter().text(pos2(centro.x, y_icone), Align2::CENTER_CENTER, glifo.to_string(), w::regular(if on { 15.0 } else { 19.0 }), cor);
                                // Rótulo longo ("Consultor") não cabe em aba estreita: reduz a fonte se precisar.
                                let fonte = if ui.painter().layout_no_wrap(label.to_owned(), w::semibold(9.0), cor).size().x > rect.width() - 6.0 {
                                    8.5
                                } else {
                                    9.0
                                };
                                ui.painter().text(pos2(centro.x, y_label), Align2::CENTER_CENTER, label.to_string(), w::semibold(fonte), cor);
                                if resp.clicked() {
                                    app.tab = tab;
                                }
                            }
                        });
                        // Pílula desliza até o left da aba escolhida (0,28 s, como no desktop).
                        if let Some(alvo) = rect_sel {
                            let x = w::tween(ui.ctx(), egui::Id::new("tab_pill"), alvo.left(), 0.28, alvo.left());
                            let pr = egui::Rect::from_min_size(pos2(x, alvo.top()), alvo.size());
                            ui.painter().set(pill, egui::Shape::rect_filled(pr, corner_radius_pill(), c.pill));
                        }
                    });
                // FAB azul colado na borda direita: right_to_left garante o respiro da pílula em 360.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(62.0), egui::Sense::click());
                    ui.painter().circle_filled(rect.center(), 31.0, c.blue);
                    ui.painter().circle_filled(rect.center(), 31.0, Color32::from_rgba_unmultiplied(47, 108, 255, 110));
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS.to_string(), w::bold(24.0), Color32::WHITE);
                    if resp.clicked() {
                        app.sheet = Some(match app.tab {
                            // Cartões só têm folha própria na demonstração (cartões reais vêm da Pluggy).
                            Tab::Cartoes if app.demo => Sheet::Cartao(SheetCartao::default()),
                            _ => Sheet::Lancamento(crate::screens::lancamento::Folha::default()),
                        });
                    }
                });
            });
        });
}

fn corner_radius_pill() -> egui::CornerRadius {
    egui::CornerRadius::same(255)
}

/// Folha do FAB: descrição + valor + dia (demo grava no mock; real grava via worker).
fn folha(parent: &mut Ui, app: &mut MobileApp) {
    let Some(sheet) = app.sheet.clone() else { return };
    let c = theme::cores();
    egui::Area::new(egui::Id::new("folha-fundo"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(parent, |ui| {
            // Tela inteira: o rect herdado da Area pode vir recortado pelo layout.
            let rect = parent.max_rect();
            let resp = ui.allocate_rect(rect, egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(16, 24, 40, 115));
            if resp.clicked() {
                app.sheet = None;
            }
        });
    egui::Area::new(egui::Id::new("folha"))
        .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(parent, |ui| {
            Frame::new()
                .fill(c.card)
                .corner_radius(egui::CornerRadius { nw: 24, ne: 24, ..Default::default() })
                .inner_margin(egui::Margin { left: 18, right: 18, top: 18, bottom: 20 })
                .show(ui, |ui| {
                    // Folha sempre de borda a borda: min e max fixados pela LARGURA DA TELA (não
                    // pelo tamanho "lembrado" da Area, que só cresce — se algum conteúdo estoura
                    // uma linha, a folha escapava pela direita e ficava larga para sempre).
                    let largura = ui.ctx().content_rect().width() - 36.0; // 18 de margem de cada lado
                    ui.set_min_width(largura);
                    ui.set_max_width(largura);
                    match &sheet {
                        Sheet::Lancamento(folha) => crate::screens::lancamento::folha(ui, app, folha),
                        Sheet::Cartao(sheet) => folha_cartao(ui, app, sheet),
                        Sheet::Menu => crate::screens::ajustes::folha(ui, app),
                    }
                });
        });
}

fn folha_cartao(ui: &mut Ui, app: &mut MobileApp, sheet: &SheetCartao) {
    let c = theme::cores();
    ui.label(egui::RichText::new("Novo cartão").font(w::bold(17.0)).color(c.ink));
    ui.label(egui::RichText::new("Só na demonstração: cartões reais chegam da Pluggy.").font(w::regular(13.0)).color(c.muted));
    ui.add_space(10.0);
    // Bancos com logo: toque escolhe; o campo de texto continua para nomes fora da lista.
    let bancos = ["Santander", "Nubank", "Inter", "Bradesco", "Caixa", "C6 Bank", "PicPay", "Banco do Brasil"];
    let mut banco_escolhido: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let limite_linha = ui.max_rect().right();
        let inicio_linha = ui.min_rect().left();
        for banco in bancos {
            let logo = w::logo_key(banco).and_then(|key| w::logo_textura(key, false));
            // Frame não participa da quebra de linha do egui (aloca o rect pronto por cima do
            // layout, e a Area cresce pra caber): medimos o chip e quebramos a linha na mão.
            let largura_chip = match &logo {
                Some(logo) => (16.0 * logo.size()[0] as f32 / logo.size()[1] as f32).min(90.0) + 16.0,
                None => ui.painter().layout_no_wrap(banco.to_owned(), w::semibold(12.0), c.ink).size().x + 16.0,
            };
            if ui.cursor().left() > inicio_linha && ui.cursor().left() + largura_chip > limite_linha {
                ui.end_row();
            }
            let escolhido = sheet.banco == *banco;
            let resp = Frame::new()
                .fill(if escolhido { c.blue_soft } else { c.line })
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| match &logo {
                    Some(logo) => {
                        let nat = logo.size();
                        let altura = 16.0;
                        let largura = (altura * nat[0] as f32 / nat[1] as f32).min(90.0);
                        ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(logo, Vec2::new(largura, altura))));
                    }
                    None => {
                        ui.label(egui::RichText::new(banco.to_string()).font(w::semibold(12.0)).color(c.ink));
                    }
                })
                .response
                .interact(egui::Sense::click());
            if resp.clicked() {
                banco_escolhido = Some((*banco).to_owned());
            }
        }
    });
    ui.add_space(10.0);
    let mut banco = banco_escolhido.unwrap_or_else(|| sheet.banco.clone());
    let mut limite = sheet.limite.clone();
    let mut venc = sheet.venc.clone();
    let mut fecha = sheet.fecha.clone();
    campo(ui, "Banco (toque acima ou escreva)", &mut banco, "Ex.: Santander Unique");
    campo(ui, "Limite (R$)", &mut limite, "0,00");
    ui.columns(2, |c| {
        campo(&mut c[0], "Fecha no dia", &mut fecha, "1-31");
        campo(&mut c[1], "Vence no dia", &mut venc, "1-31");
    });
    if let Some(Sheet::Cartao(s)) = app.sheet.as_mut() {
        s.banco = banco.clone();
        s.limite = limite.clone();
        s.venc = venc.clone();
        s.fecha = fecha.clone();
    }
    if sheet.tried {
        ui.label(egui::RichText::new("Preencha banco, limite e dias válidos.").font(w::regular(12.0)).color(c.red));
    }
    ui.add_space(14.0);
    acoes_folha(ui, app, |app| salvar_cartao(app));
}

fn acoes_folha(ui: &mut Ui, app: &mut MobileApp, salvar: impl FnOnce(&mut MobileApp)) {
    let c = theme::cores();
    ui.horizontal(|ui| {
        // Duas metades iguais com 10 px de respiro (o espaçamento é contado na largura).
        ui.spacing_mut().item_spacing.x = 10.0;
        let metade = (ui.available_width() - 10.0) / 2.0;
        if ui.add_sized([metade, 44.0], egui::Button::new(egui::RichText::new("Cancelar").font(w::semibold(15.0)).color(c.ink)).fill(c.line).corner_radius(14)).clicked() {
            app.sheet = None;
        }
        if ui.add_sized([ui.available_width(), 44.0], egui::Button::new(egui::RichText::new("Salvar").font(w::bold(15.0)).color(Color32::WHITE)).fill(c.blue).corner_radius(14)).clicked() {
            salvar(app);
        }
    });
}

fn campo(ui: &mut Ui, rotulo: &str, buf: &mut String, dica: &str) -> egui::Response {
    let c = theme::cores();
    ui.label(egui::RichText::new(rotulo).font(w::semibold(12.0)).color(c.muted));
    let resp = ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(buf).hint_text(egui::RichText::new(dica).color(c.faint)).font(w::regular(15.0)));
    ui.add_space(6.0);
    resp
}

/// Novo lançamento: demo grava no mock; real faz upsert no `data` e grava via worker.
/// Novo cartão (só demonstração: cartões reais vêm da Pluggy).
fn salvar_cartao(app: &mut MobileApp) {
    let Some(Sheet::Cartao(sheet)) = app.sheet.as_ref().cloned() else { return };
    let bruto = sheet.limite.trim().replace('.', "").replace(',', ".");
    let limite_cents = (bruto.parse::<f64>().unwrap_or(0.0) * 100.0).round() as i64;
    let vence: u8 = sheet.venc.trim().parse().unwrap_or(0);
    let fecha: u8 = sheet.fecha.trim().parse().unwrap_or(0);
    if sheet.banco.trim().is_empty() || limite_cents <= 0 || vence == 0 || vence > 31 || fecha == 0 || fecha > 31 {
        if let Some(Sheet::Cartao(s)) = app.sheet.as_mut() {
            s.tried = true;
        }
        return;
    }
    let fecha_dias = (vence as i32 - fecha as i32).rem_euclid(31).max(1) as u32;
    let cor = mock::CORES_BANCOS[app.dados.cartoes.cards.len() % mock::CORES_BANCOS.len()];
    let mes = app.mes_label().split_whitespace().next().unwrap_or("do mês").to_lowercase();
    app.dados.cartoes.cards.push(mock::Cartao {
        banco: sheet.banco.trim().to_owned(),
        cor,
        aberta: true,
        fecha_dias,
        vence_dia: vence,
        fatura_label: format!("Fatura de {mes}"),
        total_cents: 0,
        compras: 0,
        disponivel_cents: limite_cents,
        limite_cents,
        paga: false,
    });
    app.dados.cartoes.total_faturas_cents = app.dados.cartoes.cards.iter().map(|c| c.total_cents).sum();
    app.sheet = None;
    app.tab = Tab::Cartoes;
}
