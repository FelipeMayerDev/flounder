//! Casca do Flounder mobile: topo, barra do mês, tab bar flutuante, FAB e despacho das telas.

use egui::{Align, Align2, Color32, Frame, Layout, Ui, Vec2, pos2};
use flounder::widgets as w;
use egui_phosphor::regular as icon;

use crate::mock::{self, MEMBROS};
use crate::theme;

pub enum Tab {
    Inicio,
    Fluxo,
    Cartoes,
    Casa,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Filtro {
    Todas,
    Pendentes,
    Pagas,
}

/// Folha do FAB: novo lançamento (mock, gravando no mês aberto).
#[derive(Clone, Default)]
pub struct SheetEntrada {
    pub desc: String,
    pub valor: String,
    pub dia: String,
    pub tried: bool,
}

/// Folha de novo cartão (aba Cartões): banco, limite, fechamento e vencimento.
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
    Entrada(SheetEntrada),
    Cartao(SheetCartao),
}

pub struct MobileApp {
    pub y: i32,
    pub m: u8,
    pub tab: Tab,
    /// Posição de quem usa o celular (muda o "você" nas listas).
    pub eu: usize,
    pub filtro_casa: Filtro,
    pub mock: mock::Mes,
    pub sheet: Option<Sheet>,
}

impl Default for MobileApp {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileApp {
    pub fn new() -> Self {
        MobileApp {
            y: 2026,
            m: 12,
            tab: Tab::Inicio,
            eu: 0,
            filtro_casa: Filtro::Todas,
            mock: mock::para(2026, 12),
            sheet: None,
        }
    }

    /// Navega de mês; recarrega o mock (fase 2: busca na API).
    pub fn andar_mes(&mut self, delta: i32) {
        self.m = ((self.m as i32 - 1 + delta).rem_euclid(12) + 1) as u8;
        if delta < 0 && self.m == 12 {
            self.y -= 1;
        }
        if delta > 0 && self.m == 1 {
            self.y += 1;
        }
        self.mock = mock::para(self.y, self.m);
    }

    pub fn mes_label(&self) -> String {
        const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
        format!("{} {}", MESES[(self.m - 1) as usize], self.y)
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
    topo(ui, app);
    tabbar(ui, app);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(theme::cores().bg).inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 10 }))
        .show(ui, |ui| {
            mes_barra(ui, app);
            ui.add_space(12.0);
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if app.mock.vazio {
                    theme::vazio(ui, "🐟", &format!("Nada por aqui em {}.", app.mes_label()));
                    return;
                }
                match app.tab {
                    Tab::Inicio => crate::screens::home::show(ui, app),
                    Tab::Fluxo => crate::screens::flow::show(ui, app),
                    Tab::Cartoes => crate::screens::cards::show(ui, app),
                    Tab::Casa => crate::screens::house::show(ui, app),
                }
            });
        });
    folha(ui, app);
}

impl eframe::App for MobileApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        update(self, ui);
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
                    // Alternar quem usa o celular: recarrega o mock do mês (as listas trocam o "você").
                    app.eu = (app.eu + 1) % 2;
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
                // Pílula branca com as 4 abas (deixa 72 px para o FAB).
                Frame::new()
                    .fill(c.card)
                    .corner_radius(255)
                    .shadow(egui::Shadow { offset: [0, 12], blur: 34, spread: 0, color: Color32::from_rgba_unmultiplied(23, 43, 99, 45) })
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        // Limita min E max: o with_layout filho senão estica até a borda e esconde o FAB.
                        let largura = ui.available_width() - 100.0;
                        ui.set_min_size(Vec2::new(largura, 50.0));
                        ui.set_max_size(Vec2::new(largura, 50.0));
                        let largura = ui.available_width() / 4.0;
                        for (tab, glifo, label) in [
                            (Tab::Inicio, icon::HOUSE_LINE, "Início"),
                            (Tab::Fluxo, icon::CHART_BAR, "Fluxo"),
                            (Tab::Cartoes, icon::CREDIT_CARD, "Cartões"),
                            (Tab::Casa, icon::HOUSE_SIMPLE, "Casa"),
                        ] {
                            let on = std::mem::discriminant(&app.tab) == std::mem::discriminant(&tab);
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(largura, 50.0), egui::Sense::click());
                            let centro = rect.center();
                            if on {
                                ui.painter().rect_filled(rect, CornerRadiusPill(), c.pill);
                            }
                            let cor = if on { Color32::WHITE } else { c.faint };
                            let (y_icone, y_label) = if on { (centro.y - 5.0, centro.y + 12.0) } else { (centro.y - 7.0, centro.y + 13.0) };
                            ui.painter().text(pos2(centro.x, y_icone), Align2::CENTER_CENTER, glifo.to_string(), w::regular(if on { 15.0 } else { 19.0 }), cor);
                            ui.painter().text(pos2(centro.x, y_label), Align2::CENTER_CENTER, label.to_string(), w::semibold(9.5), cor);
                            if resp.clicked() {
                                app.tab = tab;
                            }
                        }
                    });
                ui.add_space(10.0);
                // FAB azul.
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(62.0), egui::Sense::click());
                ui.painter().circle_filled(rect.center(), 31.0, c.blue);
                ui.painter().circle_filled(rect.center(), 31.0, Color32::from_rgba_unmultiplied(47, 108, 255, 110));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS.to_string(), w::bold(24.0), Color32::WHITE);
                if resp.clicked() {
                    app.sheet = Some(match app.tab {
                        Tab::Cartoes => Sheet::Cartao(SheetCartao::default()),
                        _ => Sheet::Entrada(SheetEntrada::default()),
                    });
                }
            });
        });
}

fn CornerRadiusPill() -> egui::CornerRadius {
    egui::CornerRadius::same(255)
}

/// Folha do FAB: descrição + valor + dia (mock gravando no mês aberto).
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
                    ui.set_width(ui.available_width());
                    match &sheet {
                        Sheet::Entrada(sheet) => folha_entrada(ui, app, sheet),
                        Sheet::Cartao(sheet) => folha_cartao(ui, app, sheet),
                    }
                });
        });
}

fn folha_entrada(ui: &mut Ui, app: &mut MobileApp, sheet: &SheetEntrada) {
    let c = theme::cores();
    ui.label(egui::RichText::new("Novo lançamento").font(w::bold(17.0)).color(c.ink));
    ui.label(egui::RichText::new(format!("Entra no fluxo de {}", app.mes_label())).font(w::regular(13.0)).color(c.muted));
    ui.add_space(10.0);
    let mut desc = sheet.desc.clone();
    let mut valor = sheet.valor.clone();
    let mut dia = sheet.dia.clone();
    campo(ui, "Descrição", &mut desc, "Ex.: Conta de luz");
    campo(ui, "Valor (R$)", &mut valor, "0,00");
    campo(ui, "Dia do mês", &mut dia, "1-31");
    if let Some(Sheet::Entrada(s)) = app.sheet.as_mut() {
        s.desc = desc.clone();
        s.valor = valor.clone();
        s.dia = dia.clone();
    }
    if sheet.tried {
        ui.label(egui::RichText::new("Preencha descrição, valor e dia.").font(w::regular(12.0)).color(c.red));
    }
    ui.add_space(14.0);
    acoes_folha(ui, app, |app| salvar_lancamento(app));
}

fn folha_cartao(ui: &mut Ui, app: &mut MobileApp, sheet: &SheetCartao) {
    let c = theme::cores();
    ui.label(egui::RichText::new("Novo cartão").font(w::bold(17.0)).color(c.ink));
    ui.label(egui::RichText::new(format!("Aparece na lista de cartões de {}.", app.mes_label())).font(w::regular(13.0)).color(c.muted));
    ui.add_space(10.0);
    let mut banco = sheet.banco.clone();
    let mut limite = sheet.limite.clone();
    let mut venc = sheet.venc.clone();
    let mut fecha = sheet.fecha.clone();
    campo(ui, "Banco ou nome do cartão", &mut banco, "Ex.: Santander Unique");
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
        if ui.add_sized([ui.available_width() / 2.0 - 5.0, 44.0], egui::Button::new(egui::RichText::new("Cancelar").font(w::semibold(15.0)).color(c.ink)).fill(c.line).corner_radius(14)).clicked() {
            app.sheet = None;
        }
        if ui.add_sized([ui.available_width(), 44.0], egui::Button::new(egui::RichText::new("Salvar").font(w::bold(15.0)).color(Color32::WHITE)).fill(c.blue).corner_radius(14)).clicked() {
            salvar(app);
        }
    });
}

fn campo(ui: &mut Ui, rotulo: &str, buf: &mut String, dica: &str) {
    let c = theme::cores();
    ui.label(egui::RichText::new(rotulo).font(w::semibold(12.0)).color(c.muted));
    ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(buf).hint_text(egui::RichText::new(dica).color(c.faint)).font(w::regular(15.0)));
    ui.add_space(6.0);
}

fn salvar_lancamento(app: &mut MobileApp) {
    let Some(Sheet::Entrada(sheet)) = app.sheet.as_ref().cloned() else { return };
    let bruto = sheet.valor.trim().replace('.', "").replace(',', ".");
    let centavos = (bruto.parse::<f64>().unwrap_or(0.0) * 100.0).round() as i64;
    let dia: u8 = sheet.dia.trim().parse().unwrap_or(0);
    if sheet.desc.trim().is_empty() || centavos == 0 || dia == 0 || dia > 31 {
        if let Some(Sheet::Entrada(s)) = app.sheet.as_mut() {
            s.tried = true;
        }
        return;
    }
    let entrada = mock::Entrada {
        desc: sheet.desc.trim().to_owned(),
        centavos, // fase 1: tudo despesa; receita depois com o seletor
        membro: app.eu,
        shared: false,
        card: None,
        paid: false,
    };
    if let Some(d) = app.mock.fluxo.iter_mut().find(|d| d.dia == dia) {
        d.entradas.push(entrada);
    } else {
        app.mock.fluxo.push(mock::DiaFluxo { dia, entradas: vec![entrada] });
        app.mock.fluxo.sort_by_key(|d| d.dia);
    }
    app.sheet = None;
    app.tab = Tab::Fluxo;
}

/// Novo cartão na cor da marca seguinte; fatura aberta zerada e disponível = limite.
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
    let cor = mock::CORES_BANCOS[app.mock.cartoes.cards.len() % mock::CORES_BANCOS.len()];
    let mes = app.mes_label().split_whitespace().next().unwrap_or("do mês").to_lowercase();
    app.mock.cartoes.cards.push(mock::Cartao {
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
    app.mock.cartoes.total_faturas_cents = app.mock.cartoes.cards.iter().map(|c| c.total_cents).sum();
    app.sheet = None;
    app.tab = Tab::Cartoes;
}
