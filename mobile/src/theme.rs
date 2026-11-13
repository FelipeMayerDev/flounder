//! Tema do mobile: paleta do mockup, cartões arredondados e dinheiro com centavos menores.

use egui::{Align2, Color32, CornerRadius, Frame, Layout, Pos2, Response, Sense, Stroke, Ui, Vec2, pos2};
use flounder::widgets as w;

pub struct Cores {
    pub bg: Color32,
    pub card: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub blue: Color32,
    pub blue_deep: Color32,
    pub blue_soft: Color32,
    pub green: Color32,
    pub green_soft: Color32,
    pub red: Color32,
    pub red_soft: Color32,
    pub orange: Color32,
    pub orange_soft: Color32,
    pub pill: Color32,
}

pub fn cores() -> Cores {
    Cores {
        bg: Color32::from_rgb(238, 241, 247),
        card: Color32::WHITE,
        ink: Color32::from_rgb(16, 24, 40),
        muted: Color32::from_rgb(102, 112, 133),
        faint: Color32::from_rgb(152, 162, 179),
        line: Color32::from_rgb(238, 241, 247),
        blue: Color32::from_rgb(47, 108, 255),
        blue_deep: Color32::from_rgb(23, 61, 156),
        blue_soft: Color32::from_rgb(232, 239, 255),
        green: Color32::from_rgb(18, 183, 106),
        green_soft: Color32::from_rgb(231, 247, 239),
        red: Color32::from_rgb(240, 68, 56),
        red_soft: Color32::from_rgb(254, 235, 233),
        orange: Color32::from_rgb(247, 144, 9),
        orange_soft: Color32::from_rgb(254, 244, 227),
        pill: Color32::from_rgb(16, 24, 40),
    }
}

/// Cartão branco arredondado do mockup; já adiciona o espaço entre cartões.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let c = cores();
    Frame::new()
        .fill(c.card)
        .corner_radius(22)
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_rgba_unmultiplied(23, 43, 99, 18) })
        .inner_margin(18.0)
        .show(ui, |ui| add(ui));
    ui.add_space(12.0);
}

/// "R$ 5.304,28" — inteiro bold grande, centavos menores (como no mockup).
pub fn money(ui: &mut Ui, cents: i64, size: f32, cor: Color32) -> Response {
    let neg = cents < 0;
    let valor = crate::app::fmt_money(cents.abs());
    let (inteiro, centavos) = valor.split_once(',').unwrap_or((valor.as_str(), "00"));
    let mut job = egui::text::LayoutJob::default();
    let f = |font, color, valign| egui::TextFormat { font_id: font, color, valign, ..Default::default() };
    if neg {
        job.append("− ", 0.0, f(w::semibold(size * 0.8), cor, egui::Align::BOTTOM));
    }
    job.append("R$ ", 0.0, f(w::semibold(size * 0.55), cor, egui::Align::BOTTOM));
    job.append(inteiro, 0.0, f(w::bold(size), cor, egui::Align::BOTTOM));
    job.append(&format!(",{centavos}"), 1.0, f(w::semibold(size * 0.62), cor, egui::Align::BOTTOM));
    ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
    ui.response()
}

/// Chip de status ("Aberta").
pub fn chip(ui: &mut Ui, texto: &str, fg: Color32, bg: Color32) -> Response {
    let g = ui.painter().layout_no_wrap(texto.to_owned(), w::semibold(12.0), fg);
    let (rect, resp) = ui.allocate_exact_size(g.size() + Vec2::new(16.0, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), bg);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
    resp
}

/// Barra fina de progresso (disponível do cartão).
pub fn barra(ui: &mut Ui, frac: f32, cor: Color32, altura: f32) {
    let c = cores();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), altura), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same((altura / 2.0) as u8), c.line);
    if frac > 0.0 {
        let r = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac.clamp(0.0, 1.0), altura));
        ui.painter().rect_filled(r, CornerRadius::same((altura / 2.0) as u8), cor);
    }
}

/// Barra empilhada do "Saldo previsto": segmentos lado a lado.
pub fn barra_empilhada(ui: &mut Ui, segmentos: &[(f32, Color32)]) {
    let c = cores();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 10.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(5), c.line);
    let total: f32 = segmentos.iter().map(|(f, _)| *f).sum();
    let mut x = rect.left();
    for (frac, cor) in segmentos {
        let w = rect.width() * (frac / total.max(0.001));
        let r = egui::Rect::from_min_max(pos2(x, rect.top()), pos2(x + w, rect.bottom()));
        ui.painter().rect_filled(r, CornerRadius::same(5), *cor);
        x += w;
    }
}

/// Botão circular de avatar com a inicial do membro.
pub fn avatar(ui: &mut Ui, centro: Pos2, raio: f32, inicial: &str, cor: Color32) {
    ui.painter().circle_filled(centro, raio, cor);
    ui.painter().text(
        centro,
        Align2::CENTER_CENTER,
        inicial,
        w::bold(raio * 0.9),
        Color32::WHITE,
    );
}

/// Nota de tela vazia ("Nada por aqui…").
pub fn vazio(ui: &mut Ui, emoji: &str, texto: &str) {
    let c = cores();
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.label(egui::RichText::new(emoji).size(40.0));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(texto).font(w::regular(14.0)).color(c.muted));
        ui.add_space(30.0);
    });
}

/// Linha "legenda ..... valor" das listas.
pub fn linha_valor(ui: &mut Ui, legenda: &str, cents: i64, cor_valor: Color32, cor_legenda: Color32) {
    // horizontal limita a altura (with_layout direto no ScrollArea estica e "afunda" o conteúdo).
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(legenda).font(w::regular(14.0)).color(cor_legenda));
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            money(ui, cents, 14.0, cor_valor);
        });
    });
}

/// Circulinho de checkbox (preenchido = paga).
pub fn bolinha_check(ui: &mut Ui, marcada: bool, cor: Color32) -> Response {
    let c = cores();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let centro = rect.center();
    if marcada {
        ui.painter().circle_filled(centro, 12.0, cor);
        ui.painter().text(centro, Align2::CENTER_CENTER, "✓", w::bold(13.0), Color32::WHITE);
    } else {
        ui.painter().circle_stroke(centro, 11.0, Stroke::new(1.8, c.faint));
    }
    resp
}
