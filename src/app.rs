//! Estado da interface, navegação, cabeçalho e formulários.

use crate::model::*;
use crate::backend::{Msg, Worker};
use crate::dashboard::DashboardLayout;
use crate::widgets::{self as w, *};
use egui::{vec2, Align, Key, Layout, Modifiers, Ui};
use egui_phosphor::regular as icon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Home,
    CashFlow,
    Cards,
    House,
    Reserve,
    Settings,
}

const SCREENS: [(Screen, &str, &str); 6] = [
    (Screen::Home, icon::SQUARES_FOUR, "Visão geral"),
    (Screen::CashFlow, icon::ARROWS_LEFT_RIGHT, "Fluxo de caixa"),
    (Screen::Cards, icon::CREDIT_CARD, "Cartões"),
    (Screen::House, icon::HOUSE, "Nossa casa"),
    (Screen::Reserve, icon::PIGGY_BANK, "Reserva"),
    (Screen::Settings, icon::GEAR, "Ajustes"),
];

pub struct Form {
    pub id: u32,
    pub desc: String,
    pub amount: String,
    pub date: String,
    pub kind: Kind,
    pub category: Category,
    pub shared: bool,
    pub owner: Person,
    pub card: Option<usize>,
    pub connected: Option<String>,
    pub imported: bool,
    pub paid: bool,
    /// Conta fixa (repete todo mês e entra na previsão dos meses futuros).
    pub recurring: bool,
    /// Criar/marcar parcelas: gera os próximos lançamentos com " (i/n)" na descrição.
    pub parcelar: bool,
    pub parcels: String,
    /// Só na edição: o número da parcela que este lançamento já é.
    pub parcela_n: String,
    pub tried: bool,
    /// "Excluir" clicado: o formulário pede confirmação.
    pub confirm_delete: bool,
}

impl Form {
    fn new(m: Month, me: Person) -> Form {
        let date = if m == Month::of(today()) { today() } else { m.day(1) };
        Form {
            id: 0,
            desc: String::new(),
            amount: String::new(),
            date: date.fmt(),
            kind: Kind::Expense,
            category: Category::Other,
            shared: false,
            owner: me,
            card: None,
            connected: None,
            imported: false,
            paid: false,
            recurring: false,
            parcelar: false,
            parcels: String::new(),
            parcela_n: String::new(),
            tried: false,
            confirm_delete: false,
        }
    }
    fn edit(e: &Entry) -> Form {
        Form {
            id: e.id,
            desc: e.desc.clone(),
            amount: input_money(e.amount),
            date: e.date.fmt(),
            kind: e.kind,
            category: e.category,
            shared: e.shared,
            owner: e.owner,
            card: e.card,
            connected: e.connected.clone(),
            imported: e.imported,
            paid: e.paid,
            recurring: e.recurring,
            parcelar: false,
            parcels: String::new(),
            parcela_n: String::new(),
            tried: false,
            confirm_delete: false,
        }
    }
    /// Erros de descrição, valor, data e parcelas.
    pub fn errors(&self) -> [Option<&'static str>; 5] {
        [
            self.desc.trim().is_empty().then_some("Informe uma descrição."),
            match parse_money(&self.amount) {
                None if self.amount.trim().is_empty() => Some("Informe o valor."),
                None => Some("Use um valor como 1.234,56."),
                Some(0) => Some("O valor precisa ser maior que zero."),
                Some(_) => None,
            },
            Date::parse(&self.date).is_none().then_some("Use uma data válida no formato dd/mm/aaaa."),
            (self.parcelar && self.parcela_of().is_none()).then_some("Use um número de 2 a 99."),
            (self.id != 0 && self.parcelar && self.parcela_atual().is_none())
                .then_some("De 1 até o total de parcelas."),
        ]
    }
    /// (parcela atual, total) quando o lançamento é uma parcela válida.
    fn parcela_of(&self) -> Option<(u32, u32)> {
        if !self.parcelar {
            return None;
        }
        let of: u32 = self.parcels.trim().parse().ok()?;
        let n = if self.id == 0 { 1 } else { self.parcela_atual()? };
        (n >= 1 && n <= of && (2..=99).contains(&of)).then_some((n, of))
    }
    fn parcela_atual(&self) -> Option<u32> {
        self.parcela_n.trim().parse().ok()
    }
    /// Descrição com o sufixo de parcela aplicado, se for o caso.
    fn titled(&self) -> String {
        let desc = self.desc.trim().to_string();
        match self.parcela_of() {
            None => desc,
            Some((n, of)) => {
                let base = split_installment(&desc).map(|(b, _, _)| b).unwrap_or(desc);
                format!("{base} ({n}/{of})")
            }
        }
    }
    fn entry(&self, me: Person) -> Option<Entry> {
        if self.errors().iter().any(Option::is_some) {
            return None;
        }
        let shared = self.kind == Kind::Expense && self.shared;
        Some(Entry {
            id: self.id,
            desc: self.titled(),
            amount: parse_money(&self.amount)?,
            date: Date::parse(&self.date)?,
            kind: self.kind,
            category: if self.kind == Kind::Income { Category::Salary } else { self.category },
            owner: if shared { self.owner } else if self.id == 0 { me } else { self.owner },
            shared,
            card: if self.kind == Kind::Income { None } else { self.card },
            connected: if self.kind == Kind::Income { None } else { self.connected.clone() },
            imported: self.imported,
            paid: self.paid,
            recurring: self.recurring && self.card.is_none() && self.connected.is_none(),
        })
    }
    /// Parcelas futuras (da seguinte até a última), quando o lançamento é uma parcela.
    /// Pagamento por mês via PIX/comprovante: o valor informado é o de cada parcela.
    pub fn future_entries(&self, e: &Entry) -> Vec<Entry> {
        let Some((n, of)) = self.parcela_of() else { return Vec::new() };
        let (base, _, _) = split_installment(&e.desc).unwrap_or((e.desc.clone(), 1, 1));
        let m = Month::of(e.date);
        ((n + 1)..=of)
            .map(|i| Entry {
                id: 0,
                desc: format!("{base} ({i}/{of})"),
                date: m.add((i - n) as i32).day(e.date.d),
                paid: false,
                recurring: false,
                ..e.clone()
            })
            .collect()
    }
}

/// Dados reais via backend (`App::load`). `App::new` não tem: demonstração.
#[derive(Default)]
pub struct Remote {
    /// `client-N` de quem usa este computador; `None` até responder "Quem é você?".
    pub me: Option<String>,
    pub members: Vec<crate::api::Member>,
    pub loaded: bool,
    /// Falha ao buscar o estado; some na próxima busca bem-sucedida.
    pub offline: Option<String>,
    /// Falha ao gravar; fica até a pessoa fechar o aviso.
    pub failed: Option<String>,
    /// Criada no primeiro quadro; `None` reinicia a sincronização.
    pub worker: Option<Worker>,
    /// `POST /api/sync` em andamento.
    pub syncing: bool,
    /// Resultado da última atualização das contas; `true` = houve falha.
    pub sync_note: Option<(String, bool)>,
}

pub struct App {
    pub connected_invoice: Option<String>,
    /// Cartões importados da Pluggy; `None` mostra os cartões manuais (demonstração).
    pub connected_cards: Option<Vec<crate::connected::ConnectedCard>>,
    /// Contas correntes/poupança importadas, dos dois membros.
    pub accounts: Vec<crate::api::Account>,
    /// Conexões Pluggy do domicílio e o catálogo de bancos para escolher o de cada uma.
    pub connections: Vec<crate::api::Connection>,
    pub institutions: Vec<crate::api::Institution>,
    /// Janelas de Nossa casa, Reserva e Ajustes.
    pub extra: crate::screens::Extra,
    pub remote: Option<Remote>,
    /// Atualiza as contas uma vez depois da primeira carga (só com backend).
    pub auto_sync: bool,
    pub flow: crate::screens::Flow,
    pub data: Data,
    pub month: Month,
    pub mode: Mode,
    pub screen: Screen,
    pub theme: Theme,
    pub dashboard_layout: DashboardLayout,
    pub sel_day: u32,
    pub form: Option<Form>,
    pub invoice: Option<usize>,
    /// Contas marcadas/desmarcadas neste mês: ficam no lugar nas listas "a pagar" até trocar de mês,
    /// para a linha não sumir debaixo do cursor.
    pub touched: Vec<BillRef>,
    /// Agente flutuante: bolha, análises e conversa com o GLM.
    pub agent: crate::assistant::Agent,
}

impl App {
    pub fn new() -> App {
        App {
            connected_invoice: None,
            connected_cards: None,
            accounts: Vec::new(),
            connections: Vec::new(),
            institutions: Vec::new(),
            extra: Default::default(),
            remote: None,
            auto_sync: false,
            flow: Default::default(),
            data: Data::demo(),
            month: Month::of(today()),
            mode: Mode::Couple,
            screen: Screen::Home,
            theme: Theme::Light,
            dashboard_layout: DashboardLayout::default(),
            sel_day: today().d,
            form: None,
            invoice: None,
            touched: Vec::new(),
            agent: Default::default(),
        }
    }

    pub fn load(storage: Option<&dyn eframe::Storage>) -> App {
        let mut app = Self::new();
        app.data = Data::empty();
        app.connected_cards = Some(Vec::new());
        app.remote = Some(Remote { me: storage.and_then(|s| s.get_string("member_client")), ..Remote::default() });
        app.auto_sync = true;
        if let Some(key) = storage.and_then(|s| s.get_string("theme")) {
            app.theme = Theme::from_key(&key);
        }
        if let Some(value) = storage.and_then(|s| s.get_string("dashboard_layout")) {
            app.dashboard_layout = DashboardLayout::decode(&value).unwrap_or_default();
        }
        app
    }

    pub fn set_month(&mut self, m: Month) {
        self.month = m;
        self.touched.clear();
        let first_bill = self.data.summary(m, self.mode).bills.first().map(|b| b.date.d);
        self.sel_day = if m == Month::of(today()) { today().d } else { first_bill.unwrap_or(1) };
    }

    pub fn me(&self) -> &str {
        &self.data.names[self.data.me.idx()]
    }
    pub fn partner(&self) -> &str {
        &self.data.names[self.data.me.other().idx()]
    }

    /// Recebe o que a thread do backend mandou. Estado buscado com gravação pendente ou com
    /// formulário aberto é descartado: a próxima busca (5 s) já traz tudo.
    fn sync(&mut self, ctx: &egui::Context) {
        let Some(r) = &mut self.remote else { return };
        let w = r.worker.get_or_insert_with(|| Worker::start(r.me.clone().unwrap_or_else(|| "client-1".into()), ctx.clone()));
        let mut latest = None;
        while let Ok(msg) = w.rx.try_recv() {
            match msg {
                Msg::State(state, entries) => {
                    r.offline = None;
                    if w.pending == 0 {
                        latest = Some((state, entries));
                    }
                }
                Msg::Done(error) => {
                    w.pending = w.pending.saturating_sub(1);
                    if error.is_some() {
                        r.failed = error;
                    }
                }
                Msg::Offline(error) => r.offline = Some(error),
                Msg::Synced(result) => {
                    r.syncing = false;
                    r.sync_note = Some(match result {
                        Ok(res) if res.failed == 0 => (format!("{} {} atualizada{}", res.synced, if res.synced == 1 { "conexão" } else { "conexões" }, if res.synced == 1 { "" } else { "s" }), false),
                        Ok(res) => (format!("{} atualizada(s), {} com falha. Tente de novo mais tarde.", res.synced, res.failed), true),
                        Err(e) => (format!("Não foi possível atualizar. {e}"), true),
                    });
                }
                Msg::Reports(reports) => self.agent.on_reports(reports),
                Msg::Chat(result) => self.agent.on_chat(result),
            }
        }
        if latest.is_some() && r.me.is_some() && std::mem::take(&mut self.auto_sync) && w.tx.send(crate::backend::Cmd::Sync).is_ok() {
            r.syncing = true;
        }
        if let Some((state, entries)) = latest && self.form.is_none() {
            self.apply(*state, entries);
        }
    }

    fn apply(&mut self, state: crate::api::State, entries: Vec<Entry>) {
        for m in &state.members {
            if let Some(name) = self.data.names.get_mut(m.position as usize) {
                name.clone_from(&m.name);
            }
        }
        self.data.me = if state.me == 1 { Person::B } else { Person::A };
        self.data.entries = entries;
        (self.data.reserve_start, self.data.reserve_goal, self.data.reserve_name) =
            state.reserve.map_or((0, 0, String::new()), |r| (r.opening_cents, r.target_cents, r.name));
        self.data.settlements = state.settlements.iter().filter(|s| s.from != s.to && s.from <= 1 && s.to <= 1).filter_map(|s| Some(Settlement {
            id: s.id.clone(),
            date: Date::iso(&s.date)?,
            amount: s.amount_cents,
            from: if s.from == 1 { Person::B } else { Person::A },
        })).collect();
        self.connections = state.connections;
        self.institutions = state.institutions;
        // Só faturas em reais com valor viram contas: moedas não são convertidas.
        self.data.connected_invoices = state.cards.iter().flat_map(|c| c.bills.iter().filter(|b| b.currency == "BRL").filter_map(|b| Some(ConnectedInvoice {
            card: c.id.clone(),
            label: format!("Fatura {}", c.institution),
            owner: if c.owner == 1 { Person::B } else { Person::A },
            amount: b.total?,
            due: Date::iso(&b.due)?,
            paid: b.paid,
        }))).collect();
        self.connected_cards = Some(state.cards);
        self.accounts = state.accounts;
        self.data.shared_split_off = state.shared_split_off;
        if let Some(r) = &mut self.remote {
            r.members = state.members;
            r.loaded = true;
        }
    }

    /// Recomeça a sincronização do zero (outra pessoa ou nova tentativa).
    fn restart(&mut self) {
        if let Some(r) = &mut self.remote {
            *r = Remote { me: r.me.take(), ..Remote::default() };
        }
        // Ids locais mudam com a nova thread.
        self.form = None;
        self.touched.clear();
    }

    /// Lembra quem usa este computador (`client-N`) e passa a usar o token dessa pessoa.
    pub fn choose(&mut self, client: String) {
        let Some(r) = &mut self.remote else { return };
        let same = r.worker.as_ref().is_some_and(|w| w.client == client);
        r.me = Some(client);
        if !same {
            self.restart();
        }
    }

    /// "Atualizar contas": reimporta bancos e cartões no backend.
    pub fn refresh(&mut self) {
        if let Some(r) = &mut self.remote && let Some(w) = &r.worker && !r.syncing && w.tx.send(crate::backend::Cmd::Sync).is_ok() {
            r.syncing = true;
            r.sync_note = None;
        }
    }

    /// Exclui na hora; se o backend recusar, aparece o aviso e os dados são recarregados.
    pub fn delete(&mut self, id: u32) {
        self.data.entries.retain(|e| e.id != id);
        if let Some(w) = self.remote.as_mut().and_then(|r| r.worker.as_mut()) {
            w.save(crate::backend::Cmd::Delete(id));
        }
    }

    /// Grava em segundo plano (só com backend); a interface já mostra a mudança.
    pub(crate) fn send(&mut self, cmd: crate::backend::Cmd) {
        if let Some(w) = self.remote.as_mut().and_then(|r| r.worker.as_mut()) {
            w.save(cmd);
        }
    }

    pub(crate) fn push(&mut self, id: u32) {
        if let Some(w) = self.remote.as_mut().and_then(|r| r.worker.as_mut()) && let Some(e) = self.data.entry(id) {
            w.save(crate::backend::Cmd::Entry(e.clone()));
        }
    }

    pub fn open(&mut self, b: BillRef) {
        match b {
            BillRef::Entry(id) => self.open_entry(id),
            BillRef::Invoice(c) => self.invoice = Some(c),
            BillRef::Connected(i) => self.connected_invoice = self.data.connected_invoices.get(i).map(|inv| inv.card.clone()),
        }
    }
    pub fn open_entry(&mut self, id: u32) {
        if let Some(e) = self.data.entry(id) {
            self.form = Some(Form::edit(e));
        }
    }
    pub fn add(&mut self) {
        self.form = Some(Form::new(self.month, self.data.me));
    }
    /// Conta que entra nas listas de pendências: não paga, ou mexida agora há pouco.
    pub fn pending(&self, b: &Bill) -> bool {
        b.status != Status::Paid || self.touched.contains(&b.target)
    }
    pub fn toggle(&mut self, b: BillRef) {
        self.data.toggle(b, self.month);
        match b {
            BillRef::Entry(id) => self.push(id),
            BillRef::Connected(i) => {
                if let Some(w) = self.remote.as_mut().and_then(|r| r.worker.as_mut()) && let Some(inv) = self.data.connected_invoices.get(i) {
                    let due = format!("{:04}-{:02}-{:02}", inv.due.y, inv.due.m, inv.due.d);
                    w.save(crate::backend::Cmd::BillPaid(crate::api::BillPaid { card: inv.card.clone(), due, paid: inv.paid }));
                }
            }
            BillRef::Invoice(_) => {}
        }
    }
    /// Linha de conta já ligada às ações.
    pub fn bill(&mut self, ui: &mut Ui, b: &Bill, show_date: bool) {
        let ev = w::bill_row(ui, b, show_date, true);
        if ev.toggled {
            self.toggle(b.target);
            if !self.touched.contains(&b.target) {
                self.touched.push(b.target);
            }
        } else if ev.opened {
            self.open(b.target);
        }
    }

    pub fn history(&self, n: i32) -> Vec<(String, Cents)> {
        (1 - n..=0)
            .map(|k| self.month.add(k))
            .map(|m| (m, self.data.summary(m, self.mode)))
            .filter(|(_, s)| !s.empty)
            .map(|(m, s)| (m.short(), s.balance))
            .collect()
    }

    fn shortcuts(&mut self, ui: &Ui) {
        if self.form.is_some() || self.invoice.is_some() || self.connected_invoice.is_some() || self.extra.open() {
            return;
        }
        let (prev, next, add) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::ALT, Key::ArrowLeft),
                i.consume_key(Modifiers::ALT, Key::ArrowRight),
                i.consume_key(Modifiers::COMMAND, Key::N),
            )
        });
        if prev {
            self.set_month(self.month.add(-1));
        }
        if next {
            self.set_month(self.month.add(1));
        }
        if add {
            self.add();
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if ui.ctx().data(|d| d.get_temp::<Theme>(egui::Id::new("theme"))) != Some(self.theme) {
            w::apply_theme(ui.ctx(), self.theme);
            ui.set_style(ui.ctx().global_style());
        }
        self.sync(ui.ctx());
        if self.remote.as_ref().is_some_and(|r| !r.loaded || r.me.is_none()) {
            return self.connecting(ui);
        }
        let colors = w::palette(ui.ctx());
        self.shortcuts(ui);
        // Mudou mês, modo ou tela: os cartões entram de novo em cascata.
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.month, self.mode as u8, self.screen as u8).hash(&mut h);
        w::begin_frame(ui.ctx(), h.finish());
        egui::Panel::left("nav")
            .exact_size(232.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(colors.card).inner_margin(16))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors.bg).inner_margin(24))
            .show(ui, |ui| {
                self.banner(ui);
                self.header(ui);
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let s = self.data.summary(self.month, self.mode);
                    match self.screen {
                        Screen::Home => crate::dashboard::show(self, &s, ui),
                        Screen::Settings => self.settings(ui),
                        Screen::CashFlow => crate::screens::cash_flow(self, ui),
                        Screen::Cards => crate::screens::cards(self, ui),
                        Screen::House => crate::screens::house(self, &s, ui),
                        Screen::Reserve => crate::screens::reserve(self, &s, ui),
                    }
                    ui.add_space(8.0);
                });
            });
        self.form_modal(ui);
        crate::screens::modals(self, ui);
        self.invoice_modal(ui);
        self.connected_card_modal(ui);
        // O agente flutuante vem por último: fica por cima de tudo.
        if self.remote.as_ref().is_some_and(|r| r.loaded) {
            crate::assistant::show(self, ui);
        }
    }

    /// Antes dos dados: carregando, erro com nova tentativa, ou "Quem é você?".
    fn connecting(&mut self, ui: &mut Ui) {
        let colors = w::palette(ui.ctx());
        let Some(r) = &self.remote else { return };
        let (mut retry, mut chosen) = (false, None);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(colors.bg).inner_margin(24)).show(ui, |ui| {
            if r.loaded {
                return;
            }
            w::card(ui, ROOMY, |ui| {
                ui.vertical_centered(|ui| {
                    w::art(ui, w::Art::Fish, 80.0);
                    match &r.offline {
                        Some(error) => {
                            w::text(ui, "Sem conexão com o backend", w::bold(20.0), colors.text);
                            w::caption(ui, error.as_str());
                            ui.add_space(8.0);
                            retry = w::primary_button(ui, "Tentar novamente").clicked();
                        }
                        None => {
                            w::text(ui, "Carregando seus dados…", w::bold(20.0), colors.text);
                            w::caption(ui, "Conectando ao backend local.");
                        }
                    }
                });
            });
        });
        if r.loaded {
            egui::Modal::new(egui::Id::new("who"))
                .frame(egui::Frame::new().fill(colors.card).corner_radius(16).inner_margin(24))
                .show(ui.ctx(), |ui| {
                    ui.set_width(360.0);
                    w::text(ui, "Quem é você?", w::bold(20.0), colors.text);
                    w::caption(ui, "Escolha quem usa este computador. Dá para trocar depois em Ajustes.");
                    ui.add_space(w::GROUP_GAP);
                    ui.horizontal(|ui| {
                        for m in &r.members {
                            if w::primary_button(ui, &m.name).clicked() {
                                chosen = Some(m.client.clone());
                            }
                        }
                    });
                });
        }
        if retry {
            self.restart();
        }
        if let Some(client) = chosen {
            self.choose(client);
        }
    }

    /// Aviso de falha do backend sobre o painel.
    fn banner(&mut self, ui: &mut Ui) {
        let Some(r) = &mut self.remote else { return };
        let Some(msg) = r.failed.clone().or_else(|| r.offline.clone()) else { return };
        let failed = r.failed.is_some();
        let colors = w::palette(ui.ctx());
        let mut act = false;
        w::card(ui, Dens { pad: 12.0, gap: 8.0 }, |ui| {
            ui.horizontal_wrapped(|ui| {
                w::text(ui, icon::WARNING_CIRCLE, w::regular(18.0), colors.red);
                ui.label(msg);
                act = ui.button(if failed { "Fechar aviso" } else { "Tentar novamente" }).clicked();
            });
        });
        ui.add_space(w::GROUP_GAP);
        if act && failed {
            r.failed = None;
        } else if act {
            self.restart();
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let colors = w::palette(ui.ctx());
        ui.horizontal(|ui| {
            w::art(ui, w::Art::Fish, 40.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                w::text(ui, "Flounder", w::bold(16.0), colors.text);
                w::caption(ui, if self.mode == Mode::Couple { "Finanças em casal" } else { "Finanças pessoais" });
            });
        });
        ui.add_space(24.0);
        w::text(ui, "Menu", w::semibold(12.0), colors.muted);
        ui.spacing_mut().item_spacing.y = 4.0;
        // Pílula da seleção: pintada por baixo dos itens e deslizando até o item escolhido.
        let pill = ui.painter().add(egui::Shape::Noop);
        let mut sel = None;
        for (s, ic, label) in SCREENS {
            if s == Screen::Settings {
                ui.add_space(16.0);
                w::text(ui, "Ajustes", w::semibold(12.0), colors.muted);
            }
            let label = if s == Screen::House && self.mode == Mode::Individual { "Meu orçamento" } else { label };
            let r = w::nav_item(ui, ic, label, self.screen == s);
            if self.screen == s {
                sel = Some(r.rect);
            }
            if r.clicked() {
                self.screen = s;
            }
        }
        if let Some(r) = sel {
            let y = w::tween(ui.ctx(), egui::Id::new("nav_pill"), r.top(), 0.28, r.top());
            let pr = egui::Rect::from_min_size(egui::pos2(r.left(), y), r.size());
            ui.painter().set(pill, egui::Shape::rect_filled(pr, egui::CornerRadius::same(10), colors.blue_soft));
        }
        let mut refresh = false;
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.horizontal(|ui| {
                w::person_dot(ui, self.me(), colors.person[self.data.me.idx()]);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    w::text(ui, self.me().to_string(), w::semibold(14.0), colors.text);
                    let partner = self.partner().to_string();
                    w::caption(ui, if self.mode == Mode::Couple { format!("Com {partner}") } else { "Conta individual".into() });
                });
            });
            let Some(r) = &self.remote else { return };
            ui.add_space(16.0);
            if let Some((note, failed)) = &r.sync_note {
                w::text(ui, note.as_str(), w::regular(12.0), if *failed { colors.red } else { colors.muted }).on_hover_text(note.as_str());
            }
            let label = if r.syncing { format!("{} Atualizando…", icon::HOURGLASS) } else { format!("{} Atualizar contas", icon::ARROWS_CLOCKWISE) };
            let b = ui.add_enabled(!r.syncing, egui::Button::new(label).min_size(vec2(ui.available_width(), 36.0)));
            b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !r.syncing, if r.syncing { "Atualizando contas" } else { "Atualizar contas" }));
            refresh = b.on_hover_ui(crate::screens::sync_tip).clicked();
        });
        if refresh {
            self.refresh();
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let colors = w::palette(ui.ctx());
        w::card(ui, Dens { pad: 16.0, gap: 16.0 }, |ui| {
            ui.spacing_mut().interact_size.y = 36.0; // altura dos botões do cabeçalho
            ui.horizontal(|ui| {
                if w::icon_button(ui, icon::CARET_LEFT, "Mês anterior").clicked() {
                    self.set_month(self.month.add(-1));
                }
                if w::icon_button(ui, icon::CARET_RIGHT, "Próximo mês").clicked() {
                    self.set_month(self.month.add(1));
                }
                let mut job = egui::text::LayoutJob::default();
                let f = |font, color| egui::TextFormat { font_id: font, color, ..Default::default() };
                job.append(self.month.name(), 0.0, f(w::bold(24.0), colors.text));
                job.append(&format!(" {}", self.month.y), 0.0, f(w::regular(24.0), colors.muted));
                ui.label(job);
                if self.month == Month::of(today()) {
                    w::chip(ui, "Mês atual", colors.blue, colors.blue_soft);
                } else if ui.button("Hoje").on_hover_text("Voltar ao mês atual").clicked() {
                    self.set_month(Month::of(today()));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::primary_button(ui, &format!("{} Adicionar", icon::PLUS)).on_hover_text("Ctrl+N").clicked() {
                        self.add();
                    }
                    ui.add_space(8.0);
                    w::segmented(ui, &mut self.mode, &[(Mode::Individual, "Individual"), (Mode::Couple, "Casal")]);
                });
            });
        });
    }

    fn settings(&mut self, ui: &mut Ui) {
        if let Some(r) = &self.remote {
            let mut chosen = None;
            let mut pos = r.members.iter().position(|m| r.me.as_ref() == Some(&m.client)).unwrap_or(0);
            let opts: Vec<(usize, &str)> = r.members.iter().enumerate().map(|(i, m)| (i, m.name.as_str())).collect();
            w::card(ui, NORMAL, |ui| {
                w::title(ui, "Quem usa este computador");
                w::caption(ui, "Os lançamentos ficam no banco do backend local. Trocar de pessoa recarrega os dados.");
                ui.add_space(w::GROUP_GAP);
                if w::segmented(ui, &mut pos, &opts) {
                    chosen = Some(r.members[pos].client.clone());
                }
            });
            if let Some(client) = chosen {
                self.choose(client);
            }
        }
        w::card(ui, NORMAL, |ui| {
            w::title(ui, "Aparência");
            w::caption(ui, "Escolha o tema do aplicativo.");
            ui.add_space(w::GROUP_GAP);
            if w::segmented(ui, &mut self.theme, &[
                (Theme::Light, "Claro"),
                (Theme::Dark, "Escuro"),
                (Theme::Pink, "Pink"),
            ]) {
                ui.ctx().request_repaint();
            }
            ui.add_space(w::GROUP_GAP);
            w::caption(ui, "O tema será lembrado na próxima abertura.");
        });
        crate::screens::settings(self, ui);
    }

    fn form_modal(&mut self, ui: &mut Ui) {
        let colors = w::palette(ui.ctx());
        let Some(f) = self.form.as_mut() else { return };
        let couple = self.mode == Mode::Couple;
        let names = self.data.names.clone();
        let me = self.data.me;
        let mut close = false;
        let mut save = false;
        let mut delete = false;
        let resp = egui::Modal::new(egui::Id::new("form"))
            .frame(egui::Frame::new().fill(colors.card).corner_radius(16).inner_margin(24))
            .show(ui.ctx(), |ui| {
                ui.set_width(420.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                w::text(ui, if f.id == 0 { "Novo lançamento" } else { "Editar lançamento" }, w::bold(20.0), colors.text);
                if f.imported {
                    w::caption(ui, if f.connected.is_some() { "Compra importada do cartão." } else { "Importado do banco." });
                }
                ui.add_space(w::GROUP_GAP);
                if f.confirm_delete {
                    w::text(ui, format!("Excluir \"{}\"?", f.desc.trim()), w::semibold(14.0), colors.text);
                    w::caption(ui, if f.imported {
                        "O lançamento some dos dois aparelhos e não volta na próxima importação do banco."
                    } else {
                        "O lançamento some dos dois aparelhos. Não dá para desfazer."
                    });
                    ui.add_space(w::GROUP_GAP);
                    ui.horizontal(|ui| {
                        let b = ui.add(egui::Button::new(egui::RichText::new("Excluir definitivamente").font(w::semibold(14.0)).color(colors.bg)).fill(colors.red).min_size(vec2(0.0, 36.0)));
                        delete = b.clicked();
                        if ui.add(egui::Button::new("Voltar").min_size(vec2(0.0, 36.0))).clicked() {
                            f.confirm_delete = false;
                        }
                    });
                    return;
                }
                w::segmented(ui, &mut f.kind, &[(Kind::Expense, "Despesa"), (Kind::Income, "Receita")]);
                ui.add_space(w::GROUP_GAP);
                let err = if f.tried { f.errors() } else { [None; 5] };
                let r = w::field(ui, "Descrição", err[0], |ui| ui.add(w::text_input(&mut f.desc, "Ex.: Conta de luz")));
                if f.id == 0 && ui.memory(|m| m.focused().is_none()) {
                    r.request_focus();
                }
                ui.columns(2, |c| {
                    w::field(&mut c[0], "Valor", err[1], |ui| w::money_input(ui, &mut f.amount));
                    w::field(&mut c[1], "Data", err[2], |ui| ui.add(w::text_input(&mut f.date, "dd/mm/aaaa")));
                });
                if f.kind == Kind::Expense {
                    w::group(ui, |ui| {
                        w::text(ui, "Categoria", w::semibold(13.0), colors.text);
                        egui::ComboBox::from_id_salt("cat").width(ui.available_width()).selected_text(f.category.label()).show_ui(ui, |ui| {
                            for c in Category::ALL.iter().filter(|c| **c != Category::Salary) {
                                ui.selectable_value(&mut f.category, *c, c.label());
                            }
                        });
                    });
                    ui.add_space(w::GROUP_GAP);
                    if couple || f.shared {
                        ui.checkbox(&mut f.shared, "Despesa compartilhada da casa");
                    }
                    if f.shared {
                        w::group(ui, |ui| {
                            w::text(ui, "Responsável pelo pagamento", w::semibold(13.0), colors.text);
                            w::segmented(ui, &mut f.owner, &[(Person::A, &names[0]), (Person::B, &names[1])]);
                        });
                    }
                    if f.card.is_none() && f.connected.is_none() {
                        ui.checkbox(&mut f.paid, "Já está paga");
                    } else {
                        w::caption(ui, "Compra no cartão: o pagamento é feito pela fatura.");
                    }
                }
                if f.card.is_none() && f.connected.is_none() {
                    ui.add_space(w::GROUP_GAP);
                    ui.checkbox(&mut f.recurring, "Conta fixa (repete todo mês)")
                        .on_hover_text("Aluguel, luz, internet… aparece como previsão nos meses que ainda não têm essa conta.");
                    if f.kind == Kind::Expense && split_installment(&f.desc).is_none() {
                        ui.checkbox(&mut f.parcelar, if f.id == 0 { "Parcelar em vários meses" } else { "Marcar como parcela de uma compra" })
                            .on_hover_text("O valor informado é o de cada parcela; os próximos meses recebem as demais.");
                        if f.parcelar {
                            ui.columns(2, |c| {
                                w::field(&mut c[0], "Total de parcelas", err[3], |ui| ui.add(w::text_input(&mut f.parcels, "Ex.: 4")));
                                if f.id != 0 {
                                    w::field(&mut c[1], "Esta é a parcela nº", err[4], |ui| ui.add(w::text_input(&mut f.parcela_n, "Ex.: 1")));
                                }
                            });
                            if let (Some(amount), Some((_, of))) = (parse_money(&f.amount), f.parcela_of()) {
                                w::caption(ui, format!("{} pagamentos mensais de {} · total {}", of, money(amount), money(amount * of as Cents)));
                            }
                        }
                    } else if f.kind == Kind::Expense {
                        if let Some((_, n, of)) = split_installment(&f.desc) {
                            w::caption(ui, format!("Compra parcelada: esta é a parcela {n} de {of}."));
                        }
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if w::primary_button(ui, "Salvar").clicked() {
                        save = true;
                    }
                    if ui.add(egui::Button::new("Cancelar").min_size(vec2(0.0, 36.0))).clicked() {
                        close = true;
                    }
                    if f.id != 0 {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let b = ui.add(egui::Button::new(egui::RichText::new(format!("{} Excluir", icon::TRASH)).color(colors.red)).min_size(vec2(0.0, 36.0)));
                            b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Excluir"));
                            if b.clicked() {
                                f.confirm_delete = true;
                            }
                        });
                    }
                });
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    save = true;
                }
            });
        if delete {
            let id = f.id;
            self.form = None;
            return self.delete(id);
        }
        let mut saved = None;
        if save && !f.confirm_delete {
            f.tried = true;
            if let Some(e) = f.entry(me) {
                let extras = f.future_entries(&e);
                saved = Some(self.data.upsert(e));
                for extra in extras {
                    let id = self.data.upsert(extra);
                    self.push(id);
                }
                close = true;
            }
        }
        if let Some(id) = saved {
            self.push(id);
        }
        if close || resp.should_close() {
            self.form = None;
        }
    }

    fn connected_card_modal(&mut self, ui: &mut Ui) {
        let Some(id) = &self.connected_invoice else { return };
        let Some(card) = self.connected_cards.iter().flatten().find(|c| &c.id == id).cloned() else {
            self.connected_invoice = None;
            return;
        };
        let colors = w::palette(ui.ctx());
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("connected-card"))
            .frame(egui::Frame::new().fill(colors.card).corner_radius(16).inner_margin(24))
            .show(ui.ctx(), |ui| {
                ui.set_width(440.0);
                w::text(ui, &card.institution, w::bold(20.0), colors.text);
                w::caption(ui, &card.name);
                w::caption(ui, format!("{} · {}", card.brand.as_deref().unwrap_or("Bandeira não informada"), card.last_four.as_ref().map(|n| format!("•••• {n}")).unwrap_or_else(|| "Final não informado".into())));
                ui.add_space(w::GROUP_GAP);
                for (label, value) in [("Limite total", card.limit), ("Limite disponível", card.available), ("Saldo utilizado", card.balance)] {
                    ui.label(format!("{label}: {}", crate::dashboard::connected_money(value, &card.currency)));
                }
                ui.add_space(w::GROUP_GAP);
                w::title(ui, "Faturas");
                if card.bills.is_empty() {
                    w::caption(ui, if card.bills_available { "Nenhuma fatura recebida." } else { "Faturas não disponibilizadas pela instituição." });
                }
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    for bill in &card.bills {
                        ui.label(format!("{} · {}", crate::dashboard::short_iso(&bill.due), crate::dashboard::connected_money(bill.total, &bill.currency)));
                    }
                });
                ui.add_space(w::GROUP_GAP);
                if ui.button("Fechar").clicked() { close = true; }
            });
        if close || response.should_close() { self.connected_invoice = None; }
    }

    fn invoice_modal(&mut self, ui: &mut Ui) {
        let colors = w::palette(ui.ctx());
        let Some(c) = self.invoice else { return };
        if self.form.is_some() {
            return;
        }
        let inv = self.data.invoice(c, self.month);
        let card = self.data.cards[c].clone();
        let mut items: Vec<Entry> = self.data.in_month(self.month).filter(|e| e.card == Some(c)).cloned().collect();
        items.sort_by_key(|e| e.date);
        let mut open = None;
        let mut toggle = false;
        let resp = egui::Modal::new(egui::Id::new("invoice"))
            .frame(egui::Frame::new().fill(colors.card).corner_radius(16).inner_margin(24))
            .show(ui.ctx(), |ui| {
                ui.set_width(480.0);
                w::group(ui, |ui| {
                    w::text(ui, format!("Fatura do cartão {}", card.name), w::bold(20.0), colors.text);
                    w::caption(ui, format!("{} · vence {}", self.month.name(), inv.due.fmt()));
                });
                ui.add_space(w::GROUP_GAP);
                w::big_money(ui, inv.total, 28.0, colors.text);
                ui.add_space(w::GROUP_GAP);
                ui.spacing_mut().item_spacing.y = 0.0;
                for e in &items {
                    let b = Bill {
                        target: BillRef::Entry(e.id),
                        label: format!("{} · {}", e.date.short(), e.desc),
                        amount: e.amount,
                        date: e.date,
                        status: if inv.paid { Status::Paid } else { Status::Due },
                    };
                    let ev = w::bill_row(ui, &b, false, false);
                    if ev.opened || ev.toggled {
                        open = Some(e.id);
                    }
                }
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    if w::primary_button(ui, if inv.paid { "Desmarcar pagamento da fatura" } else { "Marcar fatura como paga" }).clicked() {
                        toggle = true;
                    }
                    if ui.add(egui::Button::new("Fechar").min_size(vec2(0.0, 36.0))).clicked() {
                        self.invoice = None;
                    }
                });
            });
        if toggle {
            self.data.toggle(BillRef::Invoice(c), self.month);
        }
        if let Some(id) = open {
            self.open_entry(id);
        }
        if resp.should_close() {
            self.invoice = None;
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(client) = self.remote.as_ref().and_then(|r| r.me.clone()) {
            storage.set_string("member_client", client);
        }
        storage.set_string("theme", self.theme.key().to_owned());
        storage.set_string("dashboard_layout", self.dashboard_layout.encode());
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        App::ui(self, ui);
    }
}
