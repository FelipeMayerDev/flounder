//! Tema e componentes repetidos: cartões, valores, indicadores, linhas de conta, navegação, campos.

use crate::model::{money, split_money, Bill, Cents, Status};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Pos2, Rect, Response, Sense, Shadow, Shape,
    Stroke, Ui, UiBuilder, WidgetInfo, WidgetType,
};
use egui_phosphor::regular as icon;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Pink,
}

impl Theme {
    pub fn key(self) -> &'static str {
        match self { Self::Light => "light", Self::Dark => "dark", Self::Pink => "pink" }
    }

    pub fn from_key(key: &str) -> Self {
        match key { "dark" => Self::Dark, "pink" => Self::Pink, _ => Self::Light }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub blue: Color32,
    pub navy: Color32,
    pub blue_soft: Color32,
    pub green: Color32,
    pub green_soft: Color32,
    pub coral: Color32,
    pub coral_soft: Color32,
    pub red: Color32,
    pub red_soft: Color32,
    pub amber: Color32,
    pub amber_soft: Color32,
    pub person: [Color32; 2],
    pub hover: Color32,
}

impl Theme {
    fn palette(self) -> Palette {
        let mut p = Palette {
            bg: Color32::from_rgb(0xF2, 0xF4, 0xF7),
            card: Color32::WHITE,
            text: Color32::from_rgb(0x14, 0x18, 0x22),
            muted: Color32::from_rgb(0x5B, 0x63, 0x73),
            faint: Color32::from_rgb(0xE6, 0xE9, 0xEF),
            line: Color32::from_rgb(0xEE, 0xF0, 0xF4),
            blue: Color32::from_rgb(0x1F, 0x5E, 0xF0),
            navy: Color32::from_rgb(0x1B, 0x2E, 0x6B),
            blue_soft: Color32::from_rgb(0xE8, 0xEF, 0xFE),
            green: Color32::from_rgb(0x12, 0x8A, 0x45),
            green_soft: Color32::from_rgb(0xE3, 0xF5, 0xEA),
            coral: Color32::from_rgb(0xE8, 0x4A, 0x5F),
            coral_soft: Color32::from_rgb(0xFD, 0xE8, 0xEB),
            red: Color32::from_rgb(0xC6, 0x28, 0x28),
            red_soft: Color32::from_rgb(0xFC, 0xE8, 0xE8),
            amber: Color32::from_rgb(0xA8, 0x5D, 0x00),
            amber_soft: Color32::from_rgb(0xFF, 0xF2, 0xD6),
            person: [Color32::from_rgb(0x1F, 0x5E, 0xF0), Color32::from_rgb(0xE8, 0x4A, 0x5F)],
            hover: Color32::from_rgb(0xDC, 0xE1, 0xEA),
        };
        match self {
            Self::Light => {},
            Self::Dark => {
                p.bg = Color32::from_rgb(18, 21, 29);
                p.card = Color32::from_rgb(28, 33, 44);
                p.text = Color32::from_rgb(239, 242, 248);
                p.muted = Color32::from_rgb(173, 184, 202);
                p.faint = Color32::from_rgb(42, 49, 64);
                p.line = Color32::from_rgb(53, 62, 79);
                p.hover = Color32::from_rgb(57, 67, 87);
                p.blue = Color32::from_rgb(104, 159, 255);
                p.navy = Color32::from_rgb(163, 181, 255);
                p.blue_soft = Color32::from_rgb(35, 54, 86);
                p.green = Color32::from_rgb(102, 218, 152);
                p.green_soft = Color32::from_rgb(28, 65, 47);
                p.coral = Color32::from_rgb(255, 140, 161);
                p.coral_soft = Color32::from_rgb(75, 38, 51);
                p.red = Color32::from_rgb(255, 138, 138);
                p.red_soft = Color32::from_rgb(75, 36, 42);
                p.amber = Color32::from_rgb(245, 196, 105);
                p.amber_soft = Color32::from_rgb(70, 54, 28);
            },
            Self::Pink => {
                p.bg = Color32::from_rgb(255, 241, 247);
                p.card = Color32::from_rgb(255, 251, 253);
                p.text = Color32::from_rgb(65, 25, 47);
                p.muted = Color32::from_rgb(127, 79, 103);
                p.faint = Color32::from_rgb(248, 221, 234);
                p.line = Color32::from_rgb(244, 219, 232);
                p.hover = Color32::from_rgb(242, 199, 220);
                p.blue = Color32::from_rgb(181, 28, 105);
                p.navy = Color32::from_rgb(113, 34, 104);
                p.blue_soft = Color32::from_rgb(252, 221, 237);
            },
        }
        p.person = [p.blue, p.coral];
        p
    }
}

pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp::<Theme>(Id::new("theme"))).unwrap_or_default().palette()
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn install(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions};
    use std::sync::Arc;
    let mut f = FontDefinitions::empty();
    for (k, b) in [
        ("inter", &include_bytes!("../assets/fonts/Inter-Regular.ttf")[..]),
        ("inter-sb", include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        ("inter-b", include_bytes!("../assets/fonts/Inter-Bold.ttf")),
        ("phosphor", egui_phosphor::Variant::Regular.font_bytes()),
        // Emojis do assistente (Noto Emoji monocromático, OFL — ver LICENSE-NotoEmoji.txt).
        ("noto-emoji", include_bytes!("../assets/fonts/NotoEmoji.ttf")),
    ] {
        f.font_data.insert(k.into(), Arc::new(FontData::from_static(b)));
    }
    for (fam, main) in [
        (FontFamily::Proportional, "inter"),
        (FontFamily::Monospace, "inter"),
        (FontFamily::Name("semibold".into()), "inter-sb"),
        (FontFamily::Name("bold".into()), "inter-b"),
    ] {
        // A Inter incorporada teve a área privada do Unicode removida (ver assets/fonts/README.md),
        // senão os glifos dela encobririam os ícones Phosphor. O emoji entra por último: só atende
        // o que Inter e Phosphor não têm.
        f.families.insert(fam, vec![main.into(), "phosphor".into(), "noto-emoji".into()]);
    }
    ctx.set_fonts(f);
    load_art(ctx);
    apply_theme(ctx, Theme::Light);
}

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let colors = theme.palette();
    ctx.data_mut(|d| d.insert_temp(Id::new("theme"), theme));
    ctx.set_theme(if theme == Theme::Dark { egui::Theme::Dark } else { egui::Theme::Light });
    let reduced_motion = std::env::var_os("FLOUNDER_REDUCED_MOTION").is_some_and(|v| v != "0");
    ctx.all_styles_mut(|s| {
        s.visuals = if theme == Theme::Dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        s.animation_time = if reduced_motion { 0.0 } else { 0.18 };
        use egui::TextStyle::*;
        s.text_styles = [
            (Small, regular(12.0)),
            (Body, regular(14.0)),
            (Button, semibold(14.0)),
            (Heading, bold(22.0)),
            (Monospace, regular(14.0)),
        ]
        .into();
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        // Altura mínima de linha: 20 (texto 14 + folga). Botões definem a própria altura (36).
        s.spacing.interact_size.y = 20.0;
        let v = &mut s.visuals;
        v.panel_fill = colors.bg;
        v.window_fill = colors.card;
        v.extreme_bg_color = colors.bg;
        v.override_text_color = None;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.line);
        let r = CornerRadius::same(8);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = r;
            w.fg_stroke = Stroke::new(1.0, colors.text);
            w.expansion = 0.0;
        }
        v.widgets.inactive.weak_bg_fill = colors.faint;
        v.widgets.inactive.bg_fill = colors.faint;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, colors.line);
        v.widgets.hovered.weak_bg_fill = colors.hover;
        v.widgets.hovered.bg_fill = colors.hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, colors.muted);
        // Foco: anel de 2pt na cor de destaque, sempre visível.
        v.widgets.active.weak_bg_fill = colors.blue_soft;
        v.widgets.active.bg_fill = colors.blue_soft;
        v.widgets.active.bg_stroke = Stroke::new(2.0, colors.blue);
        v.selection.bg_fill = colors.blue_soft;
        v.selection.stroke = Stroke::new(2.0, colors.blue);
        v.window_corner_radius = CornerRadius::same(16);
        v.window_shadow = Shadow { offset: [0, 8], blur: 32, spread: 0, color: Color32::from_black_alpha(40) };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(30) };
        v.window_stroke = Stroke::NONE;
    });
}

// Medidas (impeccable/layout). Espaçamento em base 4:
//   4  rótulo ↔ valor (`group`)        8  entre itens de um bloco (padrão dos cartões)
//   16 entre grupos e título → conteúdo  24 padding do cartão e margem da página
//   Linhas de lista: coladas, separadas por fio (`list`)
// Tipografia, razão ~1,2 (Inter): 12 micro (chips, eixos, status) · 13 legenda/rótulo · 14 corpo ·
//   16 título de cartão e valores · 20 título de janela · 24 mês · 28 métrica · 40/44 saldo.
pub const GROUP_GAP: f32 = 8.0; // somado ao espaçamento de itens (8) = 16

/// Rótulo, valor e nota que se leem juntos.
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        add(ui)
    })
    .inner
}

/// Linhas de lista: altura própria, sem espaço extra entre elas.
pub fn list<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui)
    })
    .inner
}

// ───────────────────────── movimento ─────────────────────────
// Só para comunicar estado: entrada da visão, valores que mudam, seleção, pagamento, hover.
// Ease-out cúbico; desligado com FLOUNDER_REDUCED_MOTION=1 (animation_time = 0).

pub fn motion(ctx: &egui::Context) -> bool {
    ctx.global_style().animation_time > 0.0
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
    dur: f32,
}

impl Tween {
    fn at(&self, now: f64) -> f32 {
        self.from + (self.to - self.from) * ease_out((now - self.start) as f32 / self.dur)
    }
}

/// Vai até `target` em `dur` segundos; na primeira vez parte de `first` (efeito de entrada).
pub fn tween(ctx: &egui::Context, id: Id, target: f32, dur: f32, first: f32) -> f32 {
    if !motion(ctx) {
        return target;
    }
    let now = ctx.input(|i| i.time);
    let tw = match ctx.data(|d| d.get_temp::<Tween>(id)) {
        Some(t) if t.to == target => t,
        prev => {
            let from = prev.map_or(first, |t| t.at(now));
            let t = Tween { from, to: target, start: now, dur };
            ctx.data_mut(|d| d.insert_temp(id, t));
            t
        }
    };
    if now - tw.start < tw.dur as f64 {
        ctx.request_repaint();
    }
    tw.at(now)
}

/// O arraste vira a origem da próxima animação de encaixe.
pub fn snap_tween(ctx: &egui::Context, id: Id, value: f32) {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(id, Tween { from: value, to: value, start: now, dur: 0.28 }));
}

/// Chame uma vez por quadro: reinicia a cascata de entrada quando a visão (mês, modo, tela) muda.
pub fn begin_frame(ctx: &egui::Context, view: u64) {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        d.insert_temp(Id::new("card_n"), 0usize);
        if d.get_temp::<u64>(Id::new("view")) != Some(view) {
            d.insert_temp(Id::new("view"), view);
            d.insert_temp(Id::new("view_start"), now);
        }
    });
}

/// Progresso (0–1) da entrada do próximo cartão: cada um começa 40 ms depois do anterior.
fn entrance(ctx: &egui::Context) -> f32 {
    if !motion(ctx) {
        return 1.0;
    }
    let (n, start) = ctx.data_mut(|d| {
        let n = d.get_temp_mut_or(Id::new("card_n"), 0usize);
        *n += 1;
        (*n - 1, d.get_temp::<f64>(Id::new("view_start")).unwrap_or(0.0))
    });
    let t = ((ctx.input(|i| i.time) - start) as f32 - n.min(12) as f32 * 0.04) / 0.42;
    if t < 1.0 {
        ctx.request_repaint();
    }
    ease_out(t)
}

// ───────────────────────── ilustrações 3D ─────────────────────────

/// Ilustrações Fluent Emoji 3D (MIT, ver assets/art). Ícones de interface continuam Phosphor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Art {
    Fish,
    MoneyBag,
    MoneyWings,
    Banknote,
    Calendar,
    SpiralCalendar,
    Scale,
    Handshake,
    Card,
    House,
    Pig,
    ChartUp,
    Bars,
    Receipt,
    Warning,
    Party,
}

impl Art {
    const ALL: [Art; 16] = [
        Art::Fish,
        Art::MoneyBag,
        Art::MoneyWings,
        Art::Banknote,
        Art::Calendar,
        Art::SpiralCalendar,
        Art::Scale,
        Art::Handshake,
        Art::Card,
        Art::House,
        Art::Pig,
        Art::ChartUp,
        Art::Bars,
        Art::Receipt,
        Art::Warning,
        Art::Party,
    ];

    fn source(self) -> (&'static str, &'static [u8]) {
        macro_rules! png {
            ($f:literal) => {
                ($f, include_bytes!(concat!("../assets/art/", $f, ".png")))
            };
        }
        match self {
            Art::Fish => png!("tropical_fish"),
            Art::MoneyBag => png!("money_bag"),
            Art::MoneyWings => png!("money_with_wings"),
            Art::Banknote => png!("dollar_banknote"),
            Art::Calendar => png!("calendar"),
            Art::SpiralCalendar => png!("spiral_calendar"),
            Art::Scale => png!("balance_scale"),
            Art::Handshake => png!("handshake"),
            Art::Card => png!("credit_card"),
            Art::House => png!("house_with_garden"),
            Art::Pig => png!("pig_face"),
            Art::ChartUp => png!("chart_increasing"),
            Art::Bars => png!("bar_chart"),
            Art::Receipt => png!("receipt"),
            Art::Warning => png!("warning"),
            Art::Party => png!("party_popper"),
        }
    }
}

#[derive(Clone)]
struct ArtTextures(std::sync::Arc<Vec<egui::TextureHandle>>);

/// Decodifica as ilustrações uma vez, na abertura: sem carregamento assíncrono nem spinner na primeira aparição.
fn load_art(ctx: &egui::Context) {
    let tex = Art::ALL
        .iter()
        .map(|a| {
            let (name, bytes) = a.source();
            let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).expect("PNG embutido válido").to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, &img), egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR })
        })
        .collect();
    ctx.data_mut(|d| d.insert_temp(Id::new("art_textures"), ArtTextures(std::sync::Arc::new(tex))));
}

/// Pinta a ilustração centrada em `center`, com sombra de contato; sobe e cresce um pouco com o hover do cartão.
pub fn paint_art(ui: &Ui, a: Art, center: Pos2, size: f32) {
    let lift = ui.data(|d| d.get_temp::<f32>(Id::new("card_lift"))).unwrap_or(0.0);
    let pop = tween(ui.ctx(), ui.id().with(("art", a as u8)), 1.0, 0.55, 0.82);
    let s = size * pop * (1.0 + 0.06 * lift);
    let c = center - vec2(0.0, 3.0 * lift);
    let shadow_w = s * (0.62 - 0.08 * lift);
    ui.painter().add(egui::epaint::EllipseShape::filled(
        center + vec2(0.0, size * 0.44),
        vec2(shadow_w / 2.0, size * 0.05),
        Color32::from_black_alpha((26.0 * (1.0 - 0.4 * lift)) as u8),
    ));
    if let Some(ArtTextures(tex)) = ui.data(|d| d.get_temp::<ArtTextures>(Id::new("art_textures"))) {
        let i = Art::ALL.iter().position(|x| *x == a).unwrap_or(0);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(tex[i].id(), Rect::from_center_size(c, vec2(s, s)), uv, Color32::WHITE);
    }
}

/// Ilustração como widget (ocupa espaço no layout).
pub fn art(ui: &mut Ui, a: Art, size: f32) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint_art(ui, a, rect.center(), size);
    r
}

/// Retângulo arredondado com gradiente diagonal (malha em leque + contorno suavizado na borda).
pub fn gradient_rrect(p: &egui::Painter, rect: Rect, r: f32, a: Color32, b: Color32) {
    let col = |q: Pos2| {
        let t = (q.x - rect.left()) / rect.width() * 0.7 + (q.y - rect.top()) / rect.height() * 0.3;
        a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
    };
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), col(rect.center()));
    for (c, start) in corners {
        for k in 0..=8 {
            let ang = (start + 90.0 * k as f32 / 8.0).to_radians();
            let q = c + r * vec2(ang.cos(), ang.sin());
            mesh.colored_vertex(q, col(q));
        }
    }
    let n = mesh.vertices.len() as u32 - 1;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
    p.rect_stroke(rect, CornerRadius::same(r as u8), Stroke::new(1.0, a.lerp_to_gamma(b, 0.5)), egui::StrokeKind::Inside);
}

/// Densidade dos cartões: margem interna e espaço entre cartões.
#[derive(Clone, Copy)]
pub struct Dens {
    pub pad: f32,
    pub gap: f32,
}
pub const NORMAL: Dens = Dens { pad: 24.0, gap: 16.0 };
pub const ROOMY: Dens = Dens { pad: 32.0, gap: 24.0 };

/// Fundo do cartão; `lift` (hover) aumenta a sombra, `alpha` acompanha a entrada.
fn card_bg(ctx: &egui::Context, rect: Rect, lift: f32, alpha: f32) -> Shape {
    let colors = palette(ctx);
    let r = CornerRadius::same(16);
    let shadow = Shadow {
        offset: [0, (2.0 + 6.0 * lift) as i8],
        blur: (12.0 + 16.0 * lift) as u8,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(20, 40, 90, (16.0 + 18.0 * lift) as u8).gamma_multiply(alpha),
    };
    Shape::Vec(vec![shadow.as_shape(rect, r).into(), Shape::rect_filled(rect, r, colors.card.gamma_multiply(alpha))])
}

/// Linha de cartões com alturas iguais. Se alguma coluna ficar mais estreita que `min_w`, empilha.
pub fn cards_row(ui: &mut Ui, d: Dens, weights: &[f32], min_w: f32, mut add: impl FnMut(usize, &mut Ui)) {
    let n = weights.len();
    let total = ui.available_width();
    let sum: f32 = weights.iter().sum();
    let ws: Vec<f32> = weights.iter().map(|w| (total - d.gap * (n - 1) as f32) * w / sum).collect();
    let stacked = ws.iter().any(|w| *w < min_w);
    let left = ui.cursor().left();
    let row_top = ui.cursor().top();
    let row_id = ui.next_auto_id();
    let (mut x, mut top, mut bottom) = (left, row_top, row_top);
    let mut slots = Vec::new();
    for i in 0..n {
        let w = if stacked { total } else { ws[i] };
        let cid = row_id.with(i);
        // Hover pelo retângulo do quadro anterior (o atual só se conhece depois do conteúdo).
        let prev = ui.data(|d| d.get_temp::<Rect>(cid));
        let hovered = prev.is_some_and(|r| ui.rect_contains_pointer(r));
        let lift = ui.ctx().animate_bool_with_time(cid.with("lift"), hovered, 0.18);
        let enter = entrance(ui.ctx());
        // Desce 16 pt na entrada e sobe 2 pt no hover; o layout seguinte não se move.
        let dy = (1.0 - enter) * 16.0 - 2.0 * lift;
        let bg = ui.painter().add(Shape::Noop);
        let inner = Rect::from_min_size(pos2(x, top + dy), vec2(w, 100_000.0)).shrink(d.pad);
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(Align::Min)));
        child.set_width(inner.width());
        child.multiply_opacity(enter);
        child.spacing_mut().item_spacing.y = 8.0;
        child.data_mut(|d| d.insert_temp(Id::new("card_lift"), lift));
        add(i, &mut child);
        child.data_mut(|d| d.insert_temp(Id::new("card_lift"), 0.0f32));
        let h = child.min_rect().bottom() + d.pad - dy;
        if stacked {
            let r = Rect::from_min_max(pos2(x, top), pos2(x + w, h));
            ui.painter().set(bg, card_bg(ui.ctx(), r.translate(vec2(0.0, dy)), lift, enter));
            ui.data_mut(|d| d.insert_temp(cid, r));
            top = h + d.gap;
            bottom = h;
        } else {
            bottom = bottom.max(h);
            slots.push((bg, x, w, cid, dy, lift, enter));
            x += w + d.gap;
        }
    }
    for (bg, x, w, cid, dy, lift, enter) in slots {
        let r = Rect::from_min_max(pos2(x, row_top), pos2(x + w, bottom));
        ui.painter().set(bg, card_bg(ui.ctx(), r.translate(vec2(0.0, dy)), lift, enter));
        ui.data_mut(|d| d.insert_temp(cid, r));
    }
    ui.advance_cursor_after_rect(Rect::from_min_max(pos2(left, row_top), pos2(left + total, bottom)));
    ui.add_space(d.gap - ui.spacing().item_spacing.y);
}

pub fn card(ui: &mut Ui, d: Dens, add: impl FnOnce(&mut Ui)) {
    let mut add = Some(add);
    cards_row(ui, d, &[1.0], 0.0, |_, ui| (add.take().unwrap())(ui));
}

pub fn text(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(s).font(font).color(color)).truncate())
}

pub fn title(ui: &mut Ui, s: &str) {
    let colors = palette(ui.ctx());
    ui.allocate_ui(vec2((ui.available_width() - 40.0).max(40.0), 0.0), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(s).font(semibold(16.0)).color(colors.text)).wrap());
    });
    ui.add_space(GROUP_GAP);
}

pub fn caption(ui: &mut Ui, s: impl Into<String>) -> Response {
    let colors = palette(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(s).font(regular(13.0)).color(colors.muted)).wrap())
}

/// Título compacto com espaço para a alça à direita.
pub fn title_with_art(ui: &mut Ui, s: &str, a: Option<Art>) {
    let colors = palette(ui.ctx());
    let Some(a) = a else { return title(ui, s) };
    let width = ui.available_width();
    let size = if width < 280.0 { 36.0 } else { 48.0 };
    let text_width = (width - size - 48.0).max(40.0);
    let r = ui.allocate_ui_with_layout(vec2(width, 36.0), egui::Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(vec2(width, 36.0));
        ui.allocate_ui(vec2(text_width, 0.0), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(s).font(semibold(16.0)).color(colors.text)).wrap());
        });
    }).response.rect;
    paint_art(ui, a, pos2(r.right() - 40.0 - size / 2.0, r.center().y), size);
    ui.add_space(GROUP_GAP);
}

/// "R$ 5.304,28" com prefixo e centavos menores, como na referência.
pub fn money_job(ctx: &egui::Context, c: Cents, size: f32, color: Color32) -> LayoutJob {
    let colors = palette(ctx);
    let (int, frac) = split_money(c);
    let mut job = LayoutJob::default();
    let fmt = |font: FontId, color: Color32| TextFormat { font_id: font, color, valign: Align::BOTTOM, ..Default::default() };
    let sign = if c < 0 { "−" } else { "" };
    job.append(&format!("{sign}R$ "), 0.0, fmt(semibold(size * 0.62), if c < 0 { color } else { colors.muted }));
    job.append(&int, 0.0, fmt(bold(size), color));
    job.append(&format!(",{frac}"), 0.0, fmt(semibold((size * 0.42).max(12.0)), color));
    job
}

/// Valor de destaque: conta até o valor na entrada e desliza entre valores quando muda.
pub fn big_money(ui: &mut Ui, c: Cents, size: f32, color: Color32) -> Response {
    let shown = tween(ui.ctx(), ui.next_auto_id().with("money"), c as f32, 0.7, 0.0).round() as Cents;
    let r = ui.add(egui::Label::new(money_job(ui.ctx(), shown, size, color)));
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, money(c)));
    r
}

/// Valor alinhado à direita, nunca truncado.
pub fn amount(ui: &mut Ui, c: Cents, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(money(c)).font(font).color(color)).extend())
}

/// Linha "rótulo ............ valor": o rótulo trunca, o valor não.
pub fn kv(ui: &mut Ui, label: &str, c: Cents, color: Color32) {
    kv_dot(ui, None, label, c, color);
}

pub fn kv_dot(ui: &mut Ui, dot: Option<Color32>, label: &str, c: Cents, color: Color32) {
    let colors = palette(ui.ctx());
    ui.horizontal(|ui| {
        if let Some(col) = dot {
            let (r, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, col);
        }
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            amount(ui, c, semibold(14.0), color);
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                text(ui, label, regular(14.0), colors.text);
            });
        });
    });
}

/// Barra segmentada. Segmentos com valor ≤ 0 são ignorados.
/// Barra segmentada. Segmentos com valor ≤ 0 são ignorados. Cresce na entrada e desliza quando muda.
pub fn seg_bar(ui: &mut Ui, segs: &[(Cents, Color32)], h: f32) -> Rect {
    let colors = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let total: Cents = segs.iter().map(|s| s.0.max(0)).sum();
    ui.painter().rect_filled(rect, CornerRadius::same((h / 2.0) as u8), colors.faint);
    if total <= 0 {
        return rect;
    }
    let grow = tween(ui.ctx(), resp.id.with("grow"), 1.0, 0.8, 0.0);
    let mut x = rect.left();
    let n = segs.iter().filter(|s| s.0 > 0).count();
    let gap = 2.0;
    let usable = rect.width() - gap * (n.saturating_sub(1)) as f32;
    for (i, (v, c)) in segs.iter().enumerate().filter(|s| s.1 .0 > 0) {
        let frac = tween(ui.ctx(), resp.id.with(i), *v as f32 / total as f32, 0.5, 0.0);
        let w = usable * frac * grow;
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(w, h));
        ui.painter().rect_filled(r, CornerRadius::same((h / 2.0).min(w / 2.0) as u8), *c);
        x += w + gap;
    }
    rect
}

/// Barra de progresso com marcador opcional (0–1).
pub fn progress(ui: &mut Ui, frac: f32, color: Color32, h: f32, marker: Option<f32>) {
    let colors = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let rr = CornerRadius::same((h / 2.0) as u8);
    ui.painter().rect_filled(rect, rr, colors.faint);
    let f = tween(ui.ctx(), resp.id, frac.clamp(0.0, 1.0), 0.8, 0.0);
    if f > 0.0 {
        let r = Rect::from_min_size(rect.min, vec2(rect.width() * f, h));
        ui.painter().rect_filled(r, rr, color);
    }
    if let Some(m) = marker {
        let x = rect.left() + rect.width() * m.clamp(0.0, 1.0);
        ui.painter().line_segment([pos2(x, rect.top() - 4.0), pos2(x, rect.bottom() + 4.0)], Stroke::new(2.0, colors.text));
    }
}

/// Chip de status: sempre ícone + texto, além da cor.
pub fn status_chip(ctx: &egui::Context, st: Status, due: crate::model::Date) -> (String, Color32, Color32) {
    let colors = palette(ctx);
    match st {
        Status::Paid => (format!("{} Paga", icon::CHECK), colors.green, colors.green_soft),
        Status::Overdue => (format!("{} Vencida {}", icon::WARNING_CIRCLE, due.short()), colors.red, colors.red_soft),
        Status::Due => (format!("{} Vence {}", icon::CLOCK, due.short()), colors.muted, colors.faint),
    }
}

pub fn chip(ui: &mut Ui, s: &str, fg: Color32, bg: Color32) -> Response {
    let g = ui.painter().layout_no_wrap(s.into(), semibold(12.0), fg);
    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(16.0, 6.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(255), bg);
    ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
    resp
}

pub struct RowEvent {
    pub toggled: bool,
    pub opened: bool,
}

/// Linha de conta: marcar como paga (círculo), abrir detalhes (linha), status textual e valor.
/// `checkable = false`: compra de cartão, paga pela fatura — sem círculo de pagamento.
pub fn bill_row(ui: &mut Ui, b: &Bill, show_date: bool, checkable: bool) -> RowEvent {
    let colors = palette(ui.ctx());
    let paid = b.status == Status::Paid;
    let width = ui.available_width();
    let amt = ui.painter().layout_no_wrap(money(b.amount), semibold(14.0), if paid { colors.muted } else { colors.text });
    let (st, fg, _) = status_chip(ui.ctx(), b.status, b.date);
    let st = if show_date || b.status != Status::Due { st } else { String::new() };
    let stg = ui.painter().layout_no_wrap(st, regular(12.0), fg);
    let desc_w = ui.painter().layout_no_wrap(b.label.clone(), regular(14.0), colors.text).size().x;
    // Sem espaço para descrição + status + valor numa linha: status desce para baixo da descrição.
    let one_line = width - 38.0 - amt.size().x - stg.size().x - 28.0 >= desc_w.min(140.0);
    let h = if one_line || stg.is_empty() { 44.0 } else { 56.0 };
    let (rect, row) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Abrir {}: {}", b.label, money(b.amount))));
    let chk_rect = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(28.0, 28.0));
    let chk = ui.interact(chk_rect, row.id.with("chk"), if checkable { Sense::click() } else { Sense::hover() });
    if checkable {
        chk.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, paid, format!("Marcar {} como paga", b.label)));
    }
    let chk = if checkable {
        chk.on_hover_text(if paid { "Desmarcar pagamento" } else { "Marcar como paga" }).on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        chk
    };
    let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = ui.ctx().animate_bool_with_time(row.id.with("h"), row.hovered() || row.has_focus(), 0.15);
    // Pagamento: o círculo vira um check verde que "estala" de 50% a 100%.
    let k = ui.ctx().animate_bool_with_time(row.id.with("paid"), paid, 0.28);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect.expand2(vec2(6.0, 0.0)), CornerRadius::same(10), colors.hover.gamma_multiply(hover));
    }
    if row.has_focus() {
        p.rect_stroke(rect.expand2(vec2(6.0, 0.0)), CornerRadius::same(8), Stroke::new(2.0, colors.blue), egui::StrokeKind::Inside);
    }
    if chk.has_focus() || chk.hovered() {
        p.circle_stroke(chk_rect.center(), 13.0, Stroke::new(2.0, colors.blue));
    }
    let c = chk_rect.center();
    if !checkable {
        p.text(c, Align2::CENTER_CENTER, icon::CREDIT_CARD, regular(22.0), colors.muted);
    } else {
        let (ic, col) = if b.status == Status::Overdue || (paid && b.date < crate::model::today() && k < 1.0) {
            (icon::WARNING_CIRCLE, colors.red)
        } else {
            (icon::CIRCLE, colors.muted)
        };
        if k < 1.0 {
            p.text(c, Align2::CENTER_CENTER, ic, regular(22.0), col.gamma_multiply(1.0 - k));
        }
        if k > 0.0 {
            let pop = 0.5 + 0.5 * ease_out(k);
            p.circle_filled(c, 10.5 * pop, colors.green.gamma_multiply(k));
            p.text(c, Align2::CENTER_CENTER, icon::CHECK, bold(13.0 * pop), colors.bg.gamma_multiply(k));
        }
    }
    // valor à direita, nunca truncado
    let amt_x = rect.right() - amt.size().x;
    p.galley(pos2(amt_x, rect.center().y - amt.size().y / 2.0), amt, colors.text);
    let x0 = chk_rect.right() + 10.0;
    let (desc_right, desc_y) = if h == 44.0 {
        let st_x = amt_x - 16.0 - stg.size().x;
        p.galley(pos2(st_x, rect.center().y - stg.size().y / 2.0), stg, fg);
        (st_x - 12.0, rect.center().y)
    } else {
        p.galley(pos2(x0, rect.center().y + 2.0), stg, fg);
        (amt_x - 12.0, rect.center().y - 9.0)
    };
    // descrição truncada
    let mut job = LayoutJob::simple_singleline(b.label.clone(), regular(14.0), if paid { colors.muted } else { colors.text });
    job.wrap = TextWrapping::truncate_at_width((desc_right - x0).max(20.0));
    let g = p.layout_job(job);
    p.galley(pos2(x0, desc_y - g.size().y / 2.0), g, colors.text);
    p.line_segment([pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())], Stroke::new(1.0, colors.line));
    RowEvent { toggled: chk.clicked(), opened: row.clicked() }
}

/// Item de navegação lateral.
pub fn nav_item(ui: &mut Ui, ic: &str, label: &str, selected: bool) -> Response {
    let colors = palette(ui.ctx());
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let hover = ui.ctx().animate_bool_with_time(r.id.with("h"), r.hovered() && !selected, 0.15);
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(10), colors.faint.gamma_multiply(hover));
    }
    if r.has_focus() {
        p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(2.0, colors.blue), egui::StrokeKind::Inside);
    }
    let fg = if selected { colors.blue } else { colors.text };
    p.text(pos2(rect.left() + 22.0, rect.center().y), Align2::CENTER_CENTER, ic, regular(18.0), fg);
    p.text(pos2(rect.left() + 42.0, rect.center().y), Align2::LEFT_CENTER, label, if selected { semibold(14.0) } else { regular(14.0) }, fg);
    r
}

/// Controle segmentado. Devolve true se mudou.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, opts: &[(T, &str)]) -> bool {
    let colors = palette(ui.ctx());
    let mut changed = false;
    egui::Frame::new().fill(colors.faint).corner_radius(10).inner_margin(3).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 30.0;
        ui.spacing_mut().item_spacing.x = 2.0;
        let id = ui.next_auto_id();
        // O indicador branco desliza até a opção escolhida.
        let thumb = ui.painter().add(Shape::Noop);
        let mut sel_rect = None;
        ui.horizontal(|ui| {
            for (v, label) in opts {
                let sel = *value == *v;
                let g = ui.painter().layout_no_wrap(label.to_string(), semibold(13.0), if sel { colors.text } else { colors.muted });
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 30.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                resp.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, sel, *label));
                if sel {
                    sel_rect = Some(r);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(8), colors.hover);
                }
                if resp.has_focus() {
                    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, colors.blue), egui::StrokeKind::Inside);
                }
                ui.painter().galley(r.center() - g.size() / 2.0, g, colors.text);
                if resp.clicked() && !sel {
                    *value = *v;
                    changed = true;
                }
            }
        });
        if let Some(r) = sel_rect {
            let x = tween(ui.ctx(), id.with("x"), r.left(), 0.25, r.left());
            let w = tween(ui.ctx(), id.with("w"), r.width(), 0.25, r.width());
            let tr = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
            ui.painter().set(
                thumb,
                Shape::Vec(vec![
                    Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha(20) }.as_shape(tr, CornerRadius::same(8)).into(),
                    Shape::rect_filled(tr, CornerRadius::same(8), colors.card),
                ]),
            );
        }
    });
    changed
}

/// Botão só com ícone, com rótulo acessível e dica.
pub fn icon_button(ui: &mut Ui, ic: &str, label: &str) -> Response {
    let r = ui.add(egui::Button::new(egui::RichText::new(ic).font(regular(18.0))).min_size(vec2(36.0, 36.0)).frame_when_inactive(false));
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    r.on_hover_text(label)
}

pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let colors = palette(ui.ctx());
    let r = ui.add(
        egui::Button::new(egui::RichText::new(label).font(semibold(14.0)).color(colors.bg))
            .fill(colors.blue)
            .min_size(vec2(0.0, 36.0)),
    );
    // Rótulo acessível sem o glifo do ícone.
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label.trim_start_matches(|c: char| !c.is_alphanumeric())));
    r
}

/// Campo com rótulo acima e mensagem de erro logo abaixo.
/// Rótulo colado ao campo (4), erro logo abaixo, 16 até o próximo campo.
pub fn field(ui: &mut Ui, label: &str, err: Option<&str>, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    let colors = palette(ui.ctx());
    let r = group(ui, |ui| {
        text(ui, label, semibold(13.0), colors.text);
        let r = add(ui);
        if let Some(e) = err {
            text(ui, format!("{} {e}", icon::WARNING_CIRCLE), regular(12.0), colors.red);
            ui.painter().rect_stroke(r.rect, CornerRadius::same(8), Stroke::new(1.5, colors.red), egui::StrokeKind::Outside);
        }
        r
    });
    ui.add_space(GROUP_GAP);
    r
}

pub fn text_input<'a>(buf: &'a mut String, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(buf).hint_text(hint).margin(vec2(10.0, 8.0)).desired_width(f32::INFINITY)
}

/// Campo monetário: "R$" à esquerda, alinhado à direita.
pub fn money_input(ui: &mut Ui, buf: &mut String) -> Response {
    let colors = palette(ui.ctx());
    // A altura de linha é lida ao criar o layout: 34 = altura do campo, "R$" centrado nele.
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 34.0;
        ui.horizontal(|ui| {
            text(ui, "R$", semibold(14.0), colors.muted);
            ui.add(text_input(buf, "0,00").horizontal_align(Align::Max))
        })
        .inner
    })
    .inner
}

/// Pequeno gráfico de linha (valores mensais), com o mês atual destacado.
pub fn line_chart(ui: &mut Ui, pts: &[(String, Cents)], current: usize, h: f32) {
    let colors = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    // A linha se desenha da esquerda para a direita na entrada.
    let reveal = tween(ui.ctx(), resp.id.with("reveal"), 1.0, 0.9, 0.0);
    let p = ui.painter();
    // 24 pt nas laterais: os rótulos dos meses das pontas cabem inteiros.
    let plot = Rect::from_min_max(rect.min + vec2(24.0, 8.0), rect.max - vec2(24.0, 22.0));
    let hi = pts.iter().map(|x| x.1).max().unwrap_or(0);
    let lo = pts.iter().map(|x| x.1).min().unwrap_or(0);
    // O zero entra na escala: sem eixo rotulado, cortar o zero exagera variações pequenas.
    let (lo, hi) = (lo.min(0), hi.max(0));
    let pad = ((hi - lo) / 8).max(10_000);
    let (min, max) = (if lo < 0 { lo - pad } else { 0 }, hi + pad);
    let y = |v: Cents| plot.bottom() - (v - min) as f32 / (max - min) as f32 * plot.height();
    let x = |i: usize| plot.left() + plot.width() * i as f32 / (pts.len().max(2) - 1) as f32;
    if min < 0 && max > 0 {
        p.line_segment([pos2(plot.left(), y(0)), pos2(plot.right(), y(0))], Stroke::new(1.0, colors.faint));
        p.text(pos2(plot.left(), y(0) - 2.0), Align2::LEFT_BOTTOM, "R$ 0", regular(12.0), colors.muted);
    }
    p.line_segment([pos2(plot.left(), plot.bottom()), pos2(plot.right(), plot.bottom())], Stroke::new(1.0, colors.line));
    let line: Vec<_> = pts
        .iter()
        .enumerate()
        .map(|(i, v)| pos2(x(i), tween(ui.ctx(), resp.id.with(i), y(v.1), 0.5, plot.bottom())))
        .collect();
    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * reveal + 1.0, rect.bottom())));
    // Área sob a linha: azul que some em direção à base.
    let mut area = egui::Mesh::default();
    for w in line.windows(2) {
        let i = area.vertices.len() as u32;
        for q in [w[0], w[1]] {
            area.colored_vertex(q, colors.blue.gamma_multiply(0.16));
            area.colored_vertex(pos2(q.x, plot.bottom()), colors.blue.gamma_multiply(0.0));
        }
        area.add_triangle(i, i + 1, i + 2);
        area.add_triangle(i + 1, i + 2, i + 3);
    }
    clip.add(Shape::mesh(area));
    clip.add(Shape::line(line.clone(), Stroke::new(2.5, colors.blue)));
    let p = clip;
    let mut tip = None;
    for (i, (lbl, v)) in pts.iter().enumerate() {
        let c = line[i];
        let cur = i == current;
        let col = if *v < 0 { colors.red } else { colors.blue };
        p.circle_filled(c, if cur { 5.0 } else { 3.5 }, if cur { col } else { colors.card });
        p.circle_stroke(c, if cur { 5.0 } else { 3.5 }, Stroke::new(2.0, col));
        p.text(pos2(c.x, rect.bottom() - 8.0), Align2::CENTER_CENTER, lbl, if cur { semibold(12.0) } else { regular(12.0) }, if cur { colors.text } else { colors.muted });
        if resp.hover_pos().is_some_and(|h| (h.x - c.x).abs() < plot.width() / pts.len() as f32 / 2.0) || (cur && resp.hover_pos().is_none()) {
            tip = Some((c, money(*v)));
        }
    }
    if let Some((c, s)) = tip {
        let g = p.layout_no_wrap(s, semibold(12.0), colors.text);
        let pos = pos2((c.x - g.size().x / 2.0).clamp(rect.left(), rect.right() - g.size().x), (c.y - 26.0).max(rect.top()));
        p.rect_filled(Rect::from_min_size(pos, g.size()).expand(4.0), CornerRadius::same(6), colors.card);
        p.rect_stroke(Rect::from_min_size(pos, g.size()).expand(4.0), CornerRadius::same(6), Stroke::new(1.0, colors.faint), egui::StrokeKind::Outside);
        p.galley(pos, g, colors.text);
    }
    resp.widget_info(|| {
        WidgetInfo::labeled(WidgetType::Other, true, pts.iter().map(|(l, v)| format!("{l}: {}", money(*v))).collect::<Vec<_>>().join("; "))
    });
}

pub fn person_dot(ui: &mut Ui, name: &str, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 12.0, color);
    let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, semibold(12.0), palette(ui.ctx()).bg);
}

pub fn empty_note(ui: &mut Ui, s: &str) {
    let colors = palette(ui.ctx());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        text(ui, icon::TRAY, regular(18.0), colors.muted);
        caption(ui, s);
    });
    ui.add_space(8.0);
}

pub fn pct(v: f32) -> String {
    format!("{:.0}%", v)
}
