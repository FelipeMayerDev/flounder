//! Painel equilibrado com calendário de vencimentos e evolução do saldo.

use crate::app::App;
use crate::model::*;
use crate::widgets as w;
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType};
use egui_phosphor::regular as icon;

const TITLES: [&str; 7] = [
    "Saldo previsto", "Contas a pagar", "Calendário de vencimentos",
    "Evolução do saldo previsto", "Divisão da casa", "Cartões", "Moradia",
];
const GAP: f32 = 16.0;
const PAD: f32 = 16.0;
const PAD_Y: f32 = 12.0;
const ROW: f32 = 40.0;
const MAX_HEIGHT: f32 = ROW * 40.0 - GAP;

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardLayout {
    pub order: [usize; 7],
    /// Zero mantém a largura responsiva até o primeiro ajuste horizontal.
    pub spans: [u8; 7],
    pub heights: [f32; 7],
}

impl Default for DashboardLayout {
    fn default() -> Self {
        Self {
            order: [0, 1, 2, 3, 4, 5, 6],
            spans: [0; 7],
            heights: [0.0; 7],
        }
    }
}

impl DashboardLayout {
    fn minimum(card: usize) -> egui::Vec2 {
        match card {
            0 => vec2(300.0, 200.0),
            1 => vec2(220.0, 220.0),
            2 => vec2(300.0, 340.0),
            3 => vec2(300.0, 360.0),
            5 => vec2(240.0, 240.0),
            _ => vec2(220.0, 160.0),
        }
    }

    pub fn encode(&self) -> String {
        self.order.iter().map(|&card| format!("{card}:{}:{}", self.spans[card], self.heights[card]))
            .collect::<Vec<_>>().join(",")
    }

    pub fn decode(value: &str) -> Option<Self> {
        let mut layout = Self::default();
        let entries: Vec<_> = value.split(',').collect();
        if entries.len() != TITLES.len() { return None; }
        let mut seen = [false; 7];
        for (index, entry) in entries.iter().enumerate() {
            let mut fields = entry.split(':');
            let card: usize = fields.next()?.parse().ok()?;
            let span: u8 = fields.next()?.parse().ok()?;
            let height: f32 = fields.next()?.parse().ok()?;
            if card >= TITLES.len() || seen[card] || fields.next().is_some()
                || span > 12 || !height.is_finite() || height < 0.0 {
                return None;
            }
            seen[card] = true;
            layout.order[index] = card;
            layout.spans[card] = span;
            // As alturas automáticas antigas deixavam espaço vazio dentro dos cards.
            let old_default = [384.0, 384.0, 504.0, 504.0, 424.0, 424.0, 424.0][card];
            layout.heights[card] = if height == 0.0 || height == old_default { 0.0 } else {
                (((height + GAP) / ROW).round() * ROW - GAP)
                    .clamp(((Self::minimum(card).y + GAP) / ROW).ceil() * ROW - GAP, MAX_HEIGHT)
            };
        }
        Some(layout)
    }

    fn move_to(&mut self, card: usize, target: usize) {
        let from = self.order.iter().position(|&c| c == card).unwrap();
        let to = self.order.iter().position(|&c| c == target).unwrap();
        if from < to { self.order[from..=to].rotate_left(1); }
        else { self.order[to..=from].rotate_right(1); }
    }

    fn resize(&mut self, card: usize, start: egui::Vec2, delta: egui::Vec2, width: f32) {
        let column = (width + GAP) / 12.0;
        let min_span = ((Self::minimum(card).x.min(width) + GAP) / column).ceil().min(12.0);
        if delta.x != 0.0 {
            self.spans[card] = (start.x + delta.x / column).round().clamp(min_span, 12.0) as u8;
        }
        self.heights[card] = (((start.y + delta.y + GAP) / ROW).round() * ROW - GAP)
            .clamp(((Self::minimum(card).y + GAP) / ROW).ceil() * ROW - GAP, MAX_HEIGHT);
    }

    fn rows(&self, width: f32) -> Vec<Vec<(usize, f32)>> {
        let column = (width + GAP) / 12.0;
        let mut rows = Vec::new();
        let mut row = Vec::new();
        let mut used = 0;
        for &card in &self.order {
            let minimum = ((Self::minimum(card).x.min(width) + GAP) / column).ceil().min(12.0) as u8;
            let span = if self.spans[card] == 0 { [8, 4, 7, 5, 4, 4, 4][card] } else { self.spans[card] };
            let span = span.max(minimum);
            if used + span > 12 && !row.is_empty() {
                rows.push(row);
                row = Vec::new();
                used = 0;
            }
            row.push((card, span));
            used += span;
        }
        if !row.is_empty() { rows.push(row); }
        for row in &mut rows {
            let mut remaining = 12 - row.iter().map(|(_, span)| span).sum::<u8>();
            while remaining > 0 && row.iter().any(|(card, _)| self.spans[*card] == 0) {
                for (card, span) in row.iter_mut() {
                    if remaining > 0 && self.spans[*card] == 0 { *span += 1; remaining -= 1; }
                }
            }
        }
        rows.into_iter().map(|row| row.into_iter()
            .map(|(card, span)| (card, column * span as f32 - GAP)).collect()).collect()
    }
}

pub fn show(app: &mut App, s: &Summary, ui: &mut Ui) {
    let width = ui.available_width();
    let left = ui.cursor().left();
    let origin = ui.cursor().top();
    let mut top = origin;
    let colors = w::palette(ui.ctx());
    let mut moved = None;
    let dragged = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|c| *c);
    let resizing = (0..7).any(|card| ui.ctx().is_being_dragged(egui::Id::new(("dashboard-resize", card))));
    if dragged.is_some() || resizing {
        let clip = ui.clip_rect();
        for column in 0..=12 {
            let x = left + (width + GAP) * column as f32 / 12.0;
            ui.painter().line_segment([pos2(x, clip.top()), pos2(x, clip.bottom())], Stroke::new(1.0, colors.blue_soft));
        }
        let first = ((clip.top() - origin) / ROW).floor() as i32;
        for row in first..=first + (clip.height() / ROW).ceil() as i32 + 1 {
            let y = origin + row as f32 * ROW;
            ui.painter().line_segment([pos2(left, y), pos2(left + width, y)], Stroke::new(1.0, colors.blue_soft));
        }
    }
    let mut ghost = None;
    for row in app.dashboard_layout.rows(width) {
        let mut x = left;
        let mut bottom = top;
        for (card, card_width) in row {
            let dragging = dragged == Some(card);
            let motion = egui::Id::new(("dashboard-motion", card));
            let tx = x - left;
            let ty = top - origin;
            let px = if dragging { tx } else { w::tween(ui.ctx(), motion.with("x"), tx, 0.28, tx) };
            let py = if dragging { ty } else { w::tween(ui.ctx(), motion.with("y"), ty, 0.28, ty) };
            let pw = if dragging { card_width } else { w::tween(ui.ctx(), motion.with("w"), card_width, 0.28, card_width) }.min(width);
            let position = pos2(left + px.clamp(0.0, (width - pw).max(0.0)), origin + py);
            let bg = ui.painter().add(egui::Shape::Noop);
            let inner = Rect::from_min_size(position + vec2(PAD, PAD_Y), vec2(pw - 2.0 * PAD, 100_000.0));
            let mut child = ui.new_child(egui::UiBuilder::new()
                .id_salt(("dashboard-card", card)).max_rect(inner).layout(Layout::top_down(Align::Min)));
            child.set_width(inner.width());
            if dragging { child.set_clip_rect(Rect::EVERYTHING); }
            let handle_rect = Rect::from_min_size(pos2(position.x + pw - 36.0, position.y + 8.0), vec2(28.0, 24.0));
            let handle = child.interact(handle_rect, egui::Id::new(("dashboard-move", card)), Sense::drag());
            let handle = handle.on_hover_and_drag_cursor(egui::CursorIcon::Grab)
                .on_hover_text("Arraste para mover. Com foco, use ←/→ para mudar a ordem.");
            handle.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Mover {}", TITLES[card])));
            child.painter().text(handle_rect.center(), Align2::CENTER_CENTER, icon::DOTS_SIX, w::regular(18.0), colors.muted);
            handle.dnd_set_drag_payload(card);
            if handle.has_focus() {
                let direction = child.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) { -1 }
                    else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) { 1 }
                    else { 0 }
                });
                if direction != 0 {
                    let index = app.dashboard_layout.order.iter().position(|&c| c == card).unwrap();
                    let target = (index as isize + direction).clamp(0, 6) as usize;
                    moved = Some((card, app.dashboard_layout.order[target]));
                }
            }
            let desired_height = app.dashboard_layout.heights[card];
            match card {
                0 => balance_card(app, s, &mut child),
                1 => bills_card(app, s, &mut child, 4, true),
                2 => calendar(app, s, &mut child, desired_height),
                3 => {
                    evolution(app, &mut child, (desired_height - 240.0).max(184.0));
                    child.add_space(w::GROUP_GAP);
                    child.separator();
                    day_block(app, s, &mut child);
                }
                4 => split_block(app, s, &mut child, true),
                5 => cards_block(app, s, &mut child, true),
                _ => housing_block(app, s, &mut child, true),
            }
            let natural = child.min_rect().bottom() + PAD_Y - position.y;
            let height = ((desired_height.max(natural) + GAP) / ROW).ceil() * ROW - GAP;
            let ph = if dragging { height } else { w::tween(ui.ctx(), motion.with("h"), height, 0.28, height) }.max(natural);
            let rect = Rect::from_min_size(position, vec2(pw, ph));
            let drop = ui.interact(rect, egui::Id::new(("dashboard-drop", card)), Sense::hover());
            let over = drop.dnd_hover_payload::<usize>().is_some_and(|source| *source != card);
            ui.painter().set(bg, egui::Frame::new().fill(colors.card).corner_radius(16)
                .stroke(Stroke::new(if over || dragging { 2.0 } else { 1.0 }, if over || dragging { colors.blue } else { colors.line }))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(16) })
                .paint(rect));
            if let Some(source) = drop.dnd_release_payload::<usize>() { moved = Some((*source, card)); }
            let corner = Rect::from_min_size(rect.right_bottom() - vec2(24.0, 24.0), vec2(24.0, 24.0));
            let resize = ui.interact(corner, egui::Id::new(("dashboard-resize", card)), Sense::drag())
                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeNwSe)
                .on_hover_text("Arraste para redimensionar. Com foco, use as setas.");
            resize.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Redimensionar {}", TITLES[card])));
            ui.painter().text(corner.center(), Align2::CENTER_CENTER, icon::ARROWS_OUT_SIMPLE, w::regular(16.0), colors.muted);
            if resize.drag_started() {
                ui.ctx().data_mut(|d| d.insert_temp(resize.id.with("start"), vec2((pw + GAP) * 12.0 / (width + GAP), ph)));
            }
            if let Some(delta) = resize.total_drag_delta() {
                if let Some(start) = ui.ctx().data(|d| d.get_temp::<egui::Vec2>(resize.id.with("start"))) {
                    app.dashboard_layout.resize(card, start, delta, width);
                    ui.ctx().request_repaint();
                }
            }
            if resize.has_focus() {
                let delta = ui.input_mut(|i| {
                    let mut delta = egui::Vec2::ZERO;
                    for (key, change) in [(egui::Key::ArrowLeft, vec2(-(width + GAP) / 12.0, 0.0)),
                        (egui::Key::ArrowRight, vec2((width + GAP) / 12.0, 0.0)),
                        (egui::Key::ArrowUp, vec2(0.0, -ROW)), (egui::Key::ArrowDown, vec2(0.0, ROW))] {
                        if i.consume_key(egui::Modifiers::NONE, key) { delta += change; }
                    }
                    delta
                });
                if delta != egui::Vec2::ZERO {
                    app.dashboard_layout.resize(card, vec2((card_width + GAP) * 12.0 / (width + GAP), height), delta, width);
                    ui.ctx().request_repaint();
                }
            }
            if handle.has_focus() || resize.has_focus() {
                ui.painter().rect_stroke(rect, CornerRadius::same(16), Stroke::new(2.0, colors.blue), egui::StrokeKind::Inside);
            }
            if handle.drag_started() {
                let grab = ui.input(|i| i.pointer.press_origin()).unwrap_or(handle_rect.center()) - rect.min;
                ui.ctx().data_mut(|d| d.insert_temp(motion.with("grab"), grab));
            }
            if dragging || handle.drag_started() {
                let layer = ui.layer_id();
                let shapes = ui.ctx().graphics_mut(|g| {
                    let list = g.entry(layer);
                    let end = list.next_idx();
                    let shapes: Vec<_> = list.all_entries().skip(bg.0).cloned().collect();
                    for index in bg.0..end.0 { list.reset_shape(egui::layers::ShapeIdx(index)); }
                    shapes
                });
                ui.painter().rect(rect, CornerRadius::same(16), colors.blue_soft.gamma_multiply(0.5),
                    Stroke::new(1.5, colors.blue), egui::StrokeKind::Inside);
                ghost = Some((card, rect, shapes));
            }
            x += card_width + GAP;
            bottom = bottom.max(top + height).max(rect.bottom());
        }
        top = bottom + GAP;
    }
    if let Some((card, rect, shapes)) = ghost {
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            let motion = egui::Id::new(("dashboard-motion", card));
            let grab = ui.ctx().data(|d| d.get_temp::<egui::Vec2>(motion.with("grab"))).unwrap_or(vec2(rect.width() - PAD, PAD));
            let at = pointer - grab;
            let preview = Rect::from_min_size(at, rect.size());
            for (axis, value) in [("x", at.x - left), ("y", at.y - origin), ("w", rect.width()), ("h", rect.height())] {
                w::snap_tween(ui.ctx(), motion.with(axis), value);
            }
            paint_dragged_card(ui, rect, preview, shapes);
            if !ui.input(|i| i.pointer.any_released()) {
                let clip = ui.clip_rect();
                let scroll = if pointer.y > clip.bottom() - 32.0 { -12.0 }
                    else if pointer.y < clip.top() + 32.0 { 12.0 } else { 0.0 };
                if scroll != 0.0 {
                    ui.scroll_with_delta(vec2(0.0, scroll));
                    ui.ctx().request_repaint();
                }
            }
        }
    }
    if let Some((source, target)) = moved {
        app.dashboard_layout.move_to(source, target);
        ui.ctx().request_repaint();
    }
    ui.advance_cursor_after_rect(Rect::from_min_max(pos2(left, ui.cursor().top()), pos2(left + width, top)));
}

fn paint_dragged_card(ui: &Ui, source: Rect, preview: Rect, mut shapes: Vec<egui::epaint::ClippedShape>) {
    let moving = w::motion(ui.ctx());
    let angle = if moving { ui.input(|i| (i.time as f32 * 10.0).sin()) * 0.025 } else { 0.0 };
    let rotation = egui::emath::Rot2::from_angle(angle);
    let center = source.center();
    let delta = preview.center() - center;
    for clipped in &mut shapes {
        if clipped.clip_rect == ui.clip_rect() || clipped.clip_rect == Rect::EVERYTHING {
            clipped.clip_rect = source.expand(20.0);
        }
    }
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dashboard-card-drag")));
    for primitive in ui.ctx().tessellate(shapes, ui.ctx().pixels_per_point()) {
        if let egui::epaint::Primitive::Mesh(mut mesh) = primitive.primitive {
            mesh.rotate(rotation, center);
            mesh.translate(delta);
            let clip = primitive.clip_rect.translate(-center.to_vec2()).rotate_bb(rotation)
                .translate(center.to_vec2() + delta);
            painter.with_clip_rect(clip).add(egui::Shape::mesh(mesh));
        }
    }
    if moving { ui.ctx().request_repaint(); }
}

fn balance_card(app: &mut App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    hero(app, s, ui, 40.0, true);
    ui.add_space(8.0);
    distribution(app, s, ui);
    ui.add_space(8.0);
    ui.separator();
    let next = s.bills.iter().find(|b| b.status != Status::Paid).cloned();
    let compact = ui.available_width() < 480.0;
    ui.columns(if compact { 1 } else { 3 }, |c| {
        mini(&mut c[0], icon::ARROW_UP_RIGHT, "Despesas do mês", s.expenses, colors.text);
        if compact { c[0].add_space(w::GROUP_GAP); }
        mini(&mut c[if compact { 0 } else { 1 }], icon::PIGGY_BANK, "Reserva", s.reserve, colors.blue);
        let ui = &mut c[if compact { 0 } else { 2 }];
        if compact { ui.add_space(w::GROUP_GAP); }
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            w::text(ui, icon::CLOCK, w::regular(14.0), colors.muted);
            w::caption(ui, "Próxima conta");
        });
        match next {
            Some(b) => _ = w::text(ui, format!("{} · {}", b.label, b.date.short()), w::semibold(16.0), colors.text),
            None if s.bills.is_empty() => _ = w::text(ui, "Nenhuma conta", w::semibold(16.0), colors.muted),
            None => _ = w::text(ui, "Tudo pago", w::semibold(16.0), colors.green),
        }
    });
}

// ───────────────────────── blocos ─────────────────────────

fn money_color(ui: &Ui, c: Cents) -> Color32 {
    let colors = w::palette(ui.ctx());
    if c < 0 { colors.red } else { colors.text }
}

fn hero(_app: &App, s: &Summary, ui: &mut Ui, size: f32, art: bool) {
    let colors = w::palette(ui.ctx());
    let a = if s.balance < 0 { w::Art::MoneyWings } else { w::Art::MoneyBag };
    w::title_with_art(ui, "Saldo previsto", art.then_some(a));
    // Nota centrada na altura do número (o layout horizontal tem altura limitada; Align::Max aqui
    // alinharia ao fundo da área "infinita" do cartão).
    let compact = ui.available_width() < 480.0;
    let contents = |ui: &mut Ui| {
        w::big_money(ui, s.balance, size, money_color(ui, s.balance));
        ui.add_space(if compact { 4.0 } else { 16.0 });
        w::group(ui, |ui| {
            if s.balance >= 0 {
                w::text(ui, format!("{} {} da renda livre", icon::TREND_UP, w::pct(s.pct(s.balance))), w::semibold(14.0), colors.green);
                w::caption(ui, "valor estimado após pagar todas as contas");
            } else {
                w::text(ui, format!("{} Faltam {} para fechar o mês", icon::WARNING_CIRCLE, money(-s.balance)), w::semibold(14.0), colors.red);
                w::caption(ui, "as despesas previstas passam da renda");
            }
        });
    };
    if compact { ui.vertical(contents); } else { ui.horizontal(contents); }
}

fn distribution(app: &App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let couple = app.mode == Mode::Couple;
    w::seg_bar(ui, &[(s.shared_part, colors.navy), (s.personal, colors.blue), (s.balance, colors.green)], 12.0);
    ui.add_space(4.0);
    let mut cols: Vec<(&str, Cents, Option<Color32>)> = vec![("Renda", s.income, None)];
    if couple {
        cols.push(("Sua parte da casa", -s.shared_part, Some(colors.navy)));
    }
    cols.push((if couple { "Contas pessoais" } else { "Despesas" }, -s.personal, Some(colors.blue)));
    cols.push(("Sobra", s.balance, Some(if s.balance < 0 { colors.red } else { colors.green })));
    let n = cols.len();
    let per_row = if ui.available_width() < 480.0 { 2 } else { n };
    for (row_index, row) in cols.chunks(per_row).enumerate() {
        ui.columns(row.len(), |c| {
        for (i, (label, v, dot)) in row.iter().enumerate() {
            let ui = &mut c[i];
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                if let Some(d) = dot {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, *d);
                }
                w::caption(ui, *label);
            });
            let col = if row_index * per_row + i == n - 1 { if *v < 0 { colors.red } else { colors.green } } else { colors.text };
            w::amount(ui, *v, w::semibold(16.0), col);
            if row_index * per_row + i > 0 {
                w::caption(ui, w::pct(s.pct(v.abs())));
            }
        }
    });
    }
}

fn bills_card(app: &mut App, s: &Summary, ui: &mut Ui, limit: usize, art: bool) {
    let colors = w::palette(ui.ctx());
    w::title_with_art(ui, "Contas a pagar", art.then_some(w::Art::SpiralCalendar));
    let mut note = format!("{} de {} pagas", s.paid_count, s.bills.len());
    if s.overdue_count > 0 {
        note += &format!(" · {} vencida(s)", s.overdue_count);
    }
    w::group(ui, |ui| {
        w::big_money(ui, s.to_pay, 28.0, colors.text);
        w::caption(ui, note);
    });
    ui.add_space(w::GROUP_GAP);
    if s.bills.is_empty() {
        w::empty_note(ui, "Nenhuma conta neste mês.");
    }
    let mut list: Vec<Bill> = s.bills.iter().filter(|b| app.pending(b)).cloned().collect();
    list.extend(s.bills.iter().filter(|b| !app.pending(b)).cloned());
    w::list(ui, |ui| {
        for b in list.iter().take(limit) {
            app.bill(ui, b, true);
        }
    });
    if list.len() > limit {
        w::caption(ui, format!("+ {} outras contas", list.len() - limit));
    }
}

fn mini(ui: &mut Ui, ic: &str, label: &str, v: Cents, color: Color32) {
    let colors = w::palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        w::text(ui, ic, w::regular(14.0), colors.muted);
        w::caption(ui, label);
    });
    w::amount(ui, v, w::semibold(16.0), color);
}

/// Casal: quem pagou x quem deveria pagar. Individual: orçamento por categoria.
pub(crate) fn split_block(app: &mut App, s: &Summary, ui: &mut Ui, art: bool) {
    let colors = w::palette(ui.ctx());
    if app.mode == Mode::Individual {
        return budget_block(s, ui, "Distribuição do orçamento", 6);
    }
    w::title_with_art(ui, "Divisão da casa", art.then_some(w::Art::Scale));
    if app.data.shared_split_off {
        w::caption(ui, "Rateio desligado: cada despesa compartilhada é de quem pagou. Para dividir pela renda de novo, procure \"Gastos compartilhados\" em Ajustes.");
        return;
    }
    w::group(ui, |ui| {
        w::caption(ui, "Total das contas compartilhadas");
        w::big_money(ui, s.shared_total, 28.0, colors.text);
    });
    ui.add_space(w::GROUP_GAP);
    let names = app.data.names.clone();
    ui.columns(2, |c| {
        for (p, ui) in c.iter_mut().enumerate() {
            let align = if p == 0 { Align::Min } else { Align::Max };
            ui.with_layout(Layout::top_down(align), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                w::caption(ui, format!("{} pagou", names[p]));
                w::amount(ui, s.paid_by[p], w::semibold(if ui.available_width() < 120.0 { 14.0 } else { 16.0 }), colors.text);
            });
        }
    });
    let r = w::seg_bar(ui, &[(s.paid_by[0], colors.person[0]), (s.paid_by[1], colors.person[1])], 20.0);
    if s.shared_total > 0 {
        let p0 = s.paid_by[0] as f32 / s.shared_total as f32 * 100.0;
        let pr = ui.painter();
        pr.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, w::pct(p0), w::semibold(12.0), colors.bg);
        pr.text(r.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, w::pct(100.0 - p0), w::semibold(12.0), colors.bg);
        // A linha fica na parte que cabe a cada um (pela renda), não no meio da barra.
        let mid = r.left() + r.width() * (s.owed[0] as f32 / s.shared_total as f32).clamp(0.0, 1.0);
        pr.line_segment([pos2(mid, r.top() - 4.0), pos2(mid, r.bottom() + 4.0)], Stroke::new(2.0, colors.text));
    }
    w::caption(ui, "A linha marca a parte que cabe a cada um, pela renda registrada.");
    ui.add_space(w::GROUP_GAP);
    settlement_line(app, s, ui);
}

/// Saldo entre o casal acumulado até o mês, já com os acertos ("Bruno deve R$ X a Ana").
fn settlement_line(app: &App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let (from, to, v) = if s.settlement >= 0 { (1, 0, s.settlement) } else { (0, 1, -s.settlement) };
    let names = &app.data.names;
    ui.horizontal_wrapped(|ui| {
        if v == 0 {
            w::text(ui, format!("{} Contas equilibradas", icon::CHECK_CIRCLE), w::semibold(14.0), colors.green);
            return;
        }
        w::person_dot(ui, &names[from], colors.person[from]);
        w::text(ui, icon::ARROW_RIGHT, w::regular(14.0), colors.muted);
        w::person_dot(ui, &names[to], colors.person[to]);
        ui.label(egui::RichText::new(format!("{} deve {} a {}", names[from], money(v), names[to])).font(w::semibold(14.0)).color(colors.text));
    });
    w::caption(ui, format!("Saldo acumulado até {} de {}, já com os acertos.", app.month.name().to_lowercase(), app.month.y));
}

pub(crate) fn budget_block(s: &Summary, ui: &mut Ui, title: &str, n: usize) {
    let colors = w::palette(ui.ctx());
    w::title(ui, title);
    let total = s.expenses.max(1) as f32;
    let palette = [colors.blue, colors.navy, colors.green, colors.coral, colors.amber, colors.muted];
    for (i, (cat, v)) in s.by_category.iter().take(n).enumerate() {
        let col = palette[i % palette.len()];
        w::group(ui, |ui| {
            w::kv_dot(ui, Some(col), cat.label(), *v, colors.text);
            w::progress(ui, *v as f32 / total, col, 6.0, None);
        });
        ui.add_space(4.0);
    }
    if s.by_category.len() > n {
        let rest: Cents = s.by_category.iter().skip(n).map(|c| c.1).sum();
        w::kv(ui, "Demais categorias", rest, colors.muted);
    }
}

pub(crate) struct CardFace {
    title: String,
    status: String,
    number: String,
    due: String,
    footer: String,
    amount: String,
    available: String,
    colors: (Color32, Color32),
    accessible: String,
}

pub(crate) fn card_face(ui: &mut Ui, face: CardFace) -> egui::Response {
    let colors = w::palette(ui.ctx());
    let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 124.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &face.accessible));
    let lift = ui.ctx().animate_bool_with_time(response.id.with("lift"), response.hovered() || response.has_focus(), 0.18);
    let rect = slot.translate(vec2(0.0, -3.0 * lift));
    let p = ui.painter();
    let (c1, c2) = face.colors;
    p.add(egui::Shadow { offset: [0, (6.0 + 6.0 * lift) as i8], blur: (16.0 + 10.0 * lift) as u8, spread: 0, color: c1.gamma_multiply(0.35) }
        .as_shape(rect, CornerRadius::same(14)));
    w::gradient_rrect(p, rect, 14.0, c1, c2);
    let glow = p.with_clip_rect(rect.shrink(1.0));
    glow.circle_filled(rect.right_top() + vec2(-20.0, 10.0), 70.0, Color32::from_white_alpha(18));
    glow.circle_filled(rect.right_top() + vec2(-70.0, -20.0), 46.0, Color32::from_white_alpha(12));
    // Marca d'água do banco: logo grande e translúcido no fundo do cartão (lado direito).
    if let Some(key) = w::logo_key(&face.title)
        && let Some(logo) = w::logo_textura(key, true)
        && logo.size()[0] > 0
    {
        let altura = 92.0;
        let largura = altura * logo.size()[0] as f32 / logo.size()[1] as f32;
        let marca = Rect::from_min_size(
            pos2(rect.right() - largura - 12.0, rect.center().y - altura / 2.0),
            vec2(largura, altura),
        );
        glow.image(logo.id(), marca, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha(70));
    }
    if response.has_focus() {
        p.rect_stroke(rect.expand(3.0), CornerRadius::same(16), Stroke::new(2.0, colors.blue), egui::StrokeKind::Outside);
    }
    let white = Color32::WHITE;
    let soft = Color32::from_white_alpha(210);
    let m = rect.shrink(16.0);
    let status = p.layout_no_wrap(face.status, w::semibold(12.0), white);
    let pill = Rect::from_min_size(pos2(m.right() - status.size().x - 16.0, m.top() - 3.0), status.size() + vec2(16.0, 6.0));
    p.rect_filled(pill, CornerRadius::same(255), Color32::from_white_alpha(38));
    p.galley(pill.center() - status.size() / 2.0, status, white);
    let title = p.layout_no_wrap(face.title, w::semibold(14.0), white);
    p.with_clip_rect(Rect::from_min_max(m.min, pos2(pill.left() - 8.0, m.top() + 22.0))).galley(m.min, title, white);
    let chip = Rect::from_min_size(m.left_top() + vec2(0.0, 30.0), vec2(34.0, 26.0));
    w::gradient_rrect(p, chip, 6.0, Color32::from_rgb(0xF6, 0xD3, 0x6B), Color32::from_rgb(0xC8, 0x95, 0x2E));
    for k in 1..3 {
        let y = chip.top() + chip.height() * k as f32 / 3.0;
        p.line_segment([pos2(chip.left() + 4.0, y), pos2(chip.right() - 4.0, y)], Stroke::new(1.0, Color32::from_black_alpha(40)));
    }
    p.line_segment([pos2(chip.center().x, chip.top() + 4.0), pos2(chip.center().x, chip.bottom() - 4.0)], Stroke::new(1.0, Color32::from_black_alpha(40)));
    p.text(chip.right_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, face.number, w::semibold(13.0), soft);
    let available = p.layout_no_wrap(face.available, w::regular(12.0), soft);
    let footer = p.layout_no_wrap(face.footer, w::semibold(12.0), white);
    let due = p.layout_no_wrap(face.due, w::regular(12.0), soft);
    let mut amount = p.layout_no_wrap(face.amount.clone(), w::bold(20.0), white);
    let amount_width = (m.width() - due.size().x - 10.0).max(80.0);
    if amount.size().x > amount_width {
        amount = p.layout_no_wrap(face.amount, w::bold((20.0 * amount_width / amount.size().x).clamp(12.0, 20.0)), white);
    }
    p.galley(m.left_bottom() - vec2(0.0, 16.0 + due.size().y), due, soft);
    p.galley(m.right_bottom() - vec2(amount.size().x, 16.0 + amount.size().y), amount, white);
    if footer.size().x + available.size().x + 12.0 <= m.width() {
        p.galley(m.left_bottom() - vec2(0.0, footer.size().y), footer, white);
    }
    p.galley(m.right_bottom() - available.size(), available, soft);
    response
}

fn cards_block(app: &mut App, s: &Summary, ui: &mut Ui, art: bool) {
    w::title_with_art(ui, "Cartões", art.then_some(w::Art::Card));
    if let Some(cards) = &app.connected_cards {
        let mut opened = None;
        let me = app.data.me.idx() as u8;
        let cards: Vec<_> = cards.iter().filter(|c| app.mode == Mode::Couple || c.owner == me).collect();
        if cards.is_empty() { w::empty_note(ui, "Nenhum cartão conectado."); }
        for card in &cards {
            let face = connected_face(card, app.month);
            if ui.push_id(&card.id, |ui| card_face(ui, face)).inner.clicked() { opened = Some(card.id.clone()); }
            ui.add_space(8.0);
        }
        if !cards.is_empty() { w::caption(ui, "Clique no cartão para consultar limites e faturas."); }
        if opened.is_some() { app.connected_invoice = opened; }
        return;
    }
    let mine: Vec<&Invoice> = s.invoices.iter().filter(|i| app.mode == Mode::Couple || app.data.cards[i.card].owner == app.data.me).collect();
    if mine.is_empty() { w::empty_note(ui, "Nenhuma fatura neste mês."); }
    for inv in mine {
        let card = &app.data.cards[inv.card];
        let owner = card.owner.idx();
        let colors = if owner == 0 {
            (Color32::from_rgb(0x14, 0x23, 0x5C), Color32::from_rgb(0x2F, 0x6B, 0xF2))
        } else {
            (Color32::from_rgb(0x8E, 0x23, 0x3E), Color32::from_rgb(0xF0, 0x7A, 0x5A))
        };
        let face = CardFace {
            title: format!("Cartão {}", card.name),
            status: if inv.paid { format!("{} Paga", icon::CHECK) } else if inv.due < today() { format!("{} Vencida", icon::WARNING_CIRCLE) } else { "Aberta".into() },
            number: format!("•••• {}", 4821 + inv.card * 3537), due: format!("vence {}", inv.due.short()), footer: app.data.names[owner].clone(),
            amount: money(inv.total), available: format!("limite livre {}", money(card.limit - inv.total)), colors,
            accessible: format!("Ver fatura do cartão {}: {}", card.name, money(inv.total)),
        };
        if card_face(ui, face).clicked() { app.invoice = Some(inv.card); }
    }
    w::caption(ui, "Clique no cartão para ver a fatura e seus lançamentos.");
}

/// Face do cartão conectado: fatura que vence no mês (ou o saldo utilizado, se não houver).
pub(crate) fn connected_face(card: &crate::connected::ConnectedCard, month: Month) -> CardFace {
    let bill = card.bills.iter().find(|bill| bill.due.starts_with(&format!("{:04}-{:02}-", month.y, month.m)));
    let value = bill.map(|b| connected_money(b.total, &b.currency)).unwrap_or_else(|| connected_money(card.balance, &card.currency));
    let due = bill.map(|b| b.due.as_str()).or(card.due.as_deref());
    let color = Color32::from_rgb(card.color[0], card.color[1], card.color[2]);
    let color = if color.r() as u32 * 299 + color.g() as u32 * 587 + color.b() as u32 * 114 > 145_000 { color.gamma_multiply(0.6) } else { color };
    let number = card.last_four.as_ref().map(|n| format!("•••• {n}")).unwrap_or_else(|| "Final não informado".into());
    let brand = card.brand.as_deref().unwrap_or("Bandeira não informada");
    let label = if bill.is_some() { "Fatura do mês" } else { "Saldo utilizado" };
    let available = format!("limite livre {}", connected_money(card.available, &card.currency));
    CardFace {
        title: card.institution.clone(), status: if bill.is_some() { "Fatura".into() } else { "Saldo".into() }, number: number.clone(),
        due: due.map(|d| format!("vence {}", short_iso(d))).unwrap_or_else(|| "Venc. não informado".into()),
        footer: brand.into(), amount: value.clone(), available: available.clone(), colors: (color.gamma_multiply(0.65), color),
        accessible: format!("Ver cartão {} · {brand} · {number} · {label}: {value} · {available}", card.institution),
    }
}

pub(crate) fn connected_money(amount: Option<Cents>, currency: &str) -> String {
    amount.map(|amount| if currency == "BRL" { money(amount) } else {
        let (integer, fraction) = split_money(amount);
        format!("{}{currency} {integer},{fraction}", if amount < 0 { "−" } else { "" })
    }).unwrap_or_else(|| "Não informado".into())
}

pub(crate) fn short_iso(date: &str) -> String {
    crate::connected::iso_date(date).map(|s| format!("{}/{}", &s[8..10], &s[5..7])).unwrap_or_else(|| "Não informado".into())
}

fn housing_block(app: &mut App, s: &Summary, ui: &mut Ui, art: bool) {
    housing_share(s, ui, art);
    ui.add_space(w::GROUP_GAP);
    ui.separator();
    reserve_block(app, s, ui);
}

/// Quanto da renda vai para moradia, contra a referência de 30%.
pub(crate) fn housing_share(s: &Summary, ui: &mut Ui, art: bool) {
    let colors = w::palette(ui.ctx());
    w::title_with_art(ui, "Moradia", art.then_some(w::Art::House));
    let p = s.pct(s.housing);
    let ok = p <= 30.0;
    w::group(ui, |ui| {
        w::caption(ui, "Quanto você gasta para morar");
        w::big_money(ui, s.housing, 28.0, colors.text);
        w::text(
            ui,
            format!("{} {} da renda · referência: até 30%", if ok { icon::CHECK } else { icon::WARNING_CIRCLE }, w::pct(p)),
            w::semibold(13.0),
            if ok { colors.green } else { colors.amber },
        );
    });
    ui.add_space(w::GROUP_GAP);
    w::group(ui, |ui| {
        w::progress(ui, p / 60.0, if ok { colors.green } else { colors.amber }, 10.0, Some(0.5));
        ui.horizontal(|ui| {
            w::caption(ui, "0%");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| w::caption(ui, "60% da renda"));
        });
    });
}

fn reserve_block(app: &App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        w::art(ui, w::Art::Pig, 24.0);
        w::text(ui, "Reserva", w::semibold(14.0), colors.text);
    });
    w::amount(ui, s.reserve, w::semibold(14.0), colors.text);
    let goal = app.data.reserve_goal;
    if goal <= 0 {
        w::caption(ui, "Nenhuma meta de reserva definida.");
        return;
    }
    w::progress(ui, s.reserve as f32 / goal as f32, colors.blue, 8.0, None);
    w::caption(ui, format!("{} da meta de {}", w::pct(s.reserve as f32 / goal as f32 * 100.0), money(goal)));
}

fn evolution(app: &App, ui: &mut Ui, height: f32) {
    let colors = w::palette(ui.ctx());
    w::group(ui, |ui| {
        w::text(ui, "Evolução do saldo previsto", w::semibold(16.0), colors.text);
        w::caption(ui, "Últimos seis meses");
    });
    ui.add_space(w::GROUP_GAP);
    let h = app.history(6);
    if h.len() < 2 {
        return w::empty_note(ui, "Ainda não há meses suficientes para comparar.");
    }
    w::line_chart(ui, &h, h.len() - 1, height);
}

fn calendar(app: &mut App, s: &Summary, ui: &mut Ui, height: f32) {
    let colors = w::palette(ui.ctx());
    w::title_with_art(ui, "Calendário de vencimentos", Some(w::Art::Calendar));
    let m = app.month;
    let gap = 8.0;
    let cw = (ui.available_width() - gap * 6.0) / 7.0;
    ui.spacing_mut().item_spacing = vec2(gap, gap);
    ui.horizontal(|ui| {
        for d in ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"] {
            let (r, _) = ui.allocate_exact_size(vec2(cw, 16.0), Sense::hover());
            ui.painter().text(r.center(), Align2::CENTER_CENTER, d, w::semibold(12.0), colors.muted);
        }
    });
    let first = m.first_weekday();
    let weeks = (first + m.days()).div_ceil(7);
    let day_height = ((height - 144.0) / weeks as f32 - gap).max(48.0);
    for wk in 0..weeks {
        ui.horizontal(|ui| {
            for col in 0..7 {
                let idx = wk * 7 + col;
                let (rect, r) = ui.allocate_exact_size(vec2(cw, day_height), if idx >= first && idx < first + m.days() { Sense::click() } else { Sense::hover() });
                if idx < first || idx >= first + m.days() {
                    continue;
                }
                let day = idx + 1 - first;
                let bills: Vec<&Bill> = s.bills.iter().filter(|b| b.date.d == day).collect();
                let overdue = bills.iter().any(|b| b.status == Status::Overdue);
                let all_paid = !bills.is_empty() && bills.iter().all(|b| b.status == Status::Paid);
                let sel = app.sel_day == day;
                let today = m.day(day) == today();
                let p = ui.painter();
                let fill = if sel { colors.blue } else if r.hovered() { colors.faint } else { colors.bg };
                p.rect_filled(rect, CornerRadius::same(8), fill);
                if today && !sel {
                    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.5, colors.blue), egui::StrokeKind::Inside);
                }
                if r.has_focus() {
                    p.rect_stroke(rect.expand(2.0), CornerRadius::same(10), Stroke::new(2.0, colors.text), egui::StrokeKind::Inside);
                }
                let fg = if sel { colors.bg } else { colors.text };
                p.text(rect.center_top() + vec2(0.0, 8.0), Align2::CENTER_TOP, day.to_string(), w::semibold(14.0), fg);
                let (mark, mc) = if overdue {
                    (icon::WARNING_CIRCLE.to_string(), colors.red)
                } else if all_paid {
                    (icon::CHECK_CIRCLE.to_string(), colors.green)
                } else if !bills.is_empty() {
                    ("•".repeat(bills.len().min(4)), colors.blue)
                } else {
                    (String::new(), colors.muted)
                };
                p.text(rect.center_bottom() - vec2(0.0, 6.0), Align2::CENTER_BOTTOM, mark, w::bold(13.0), if sel { colors.bg } else { mc });
                let label = format!(
                    "{day} de {}{}: {} conta(s){}{}",
                    m.name().to_lowercase(),
                    if today { ", hoje" } else { "" },
                    bills.len(),
                    if overdue { ", vencida" } else { "" },
                    if all_paid { ", paga" } else { "" }
                );
                r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, sel, &label));
                if r.clicked() {
                    app.sel_day = day;
                    ui.ctx().request_repaint();
                }
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        for (t, c) in [
            (format!("{} Vencida", icon::WARNING_CIRCLE), colors.red),
            (format!("{} Paga", icon::CHECK_CIRCLE), colors.green),
            ("• A vencer".into(), colors.blue),
            (format!("{} Hoje (contorno)", icon::SQUARE), colors.blue),
        ] {
            w::text(ui, t, w::regular(12.0), c);
            ui.add_space(8.0);
        }
    });
}

fn day_block(app: &mut App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let date = app.month.day(app.sel_day);
    let today = if date == today() { " · hoje" } else { "" };
    w::title(ui, &format!("{} de {}{today}", date.d, app.month.name().to_lowercase()));
    let bills: Vec<Bill> = s.bills.iter().filter(|b| b.date == date).cloned().collect();
    if bills.is_empty() {
        w::empty_note(ui, "Nenhuma conta vence neste dia.");
        if let Some(n) = s.bills.iter().find(|b| b.date > date && b.status != Status::Paid) {
            w::caption(ui, format!("Próxima: {} em {}", n.label, n.date.short()));
        }
    } else {
        let total: Cents = bills.iter().map(|b| b.amount).sum();
        w::group(ui, |ui| {
            w::big_money(ui, total, 28.0, colors.text);
            w::caption(ui, "Use o círculo para marcar ou desmarcar o pagamento.");
        });
        ui.add_space(w::GROUP_GAP);
        w::list(ui, |ui| {
            for b in &bills {
                app.bill(ui, b, false);
            }
        });
    }
}


#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn saved_layout_round_trips_and_rejects_invalid_data() {
        let mut layout = DashboardLayout::default();
        layout.move_to(2, 0);
        layout.spans[2] = 9;
        layout.heights[2] = 624.0;
        assert_eq!(DashboardLayout::decode(&layout.encode()), Some(layout));
        for value in ["", "0:8:400", "0:8:400,0:4:400,2:7:500,3:5:500,4:4:430,5:4:430,6:4:430",
            "7:8:400,1:4:400,2:7:500,3:5:500,4:4:430,5:4:430,6:4:430",
            "0:13:400,1:4:400,2:7:500,3:5:500,4:4:430,5:4:430,6:4:430",
            "0:8:NaN,1:4:400,2:7:500,3:5:500,4:4:430,5:4:430,6:4:430",
            "0:8:inf,1:4:400,2:7:500,3:5:500,4:4:430,5:4:430,6:4:430"] {
            assert!(DashboardLayout::decode(value).is_none(), "{value}");
        }
    }

    #[test]
    fn three_cards_fit_a_row_at_supported_widths() {
        let mut layout = DashboardLayout::default();
        layout.spans = [4; 7];
        layout.order = [0, 2, 3, 1, 4, 6, 5];
        let rows = layout.rows(968.0);
        assert_eq!(rows[0].iter().map(|(card, _)| *card).collect::<Vec<_>>(), vec![0, 2, 3]);
        layout.order = [1, 4, 6, 0, 2, 3, 5];
        assert_eq!(layout.rows(728.0)[0].len(), 3);
    }

    #[test]
    fn old_default_heights_become_compact_without_resetting_custom_sizes() {
        let layout = DashboardLayout::decode("0:8:384,1:4:384,2:7:504,3:5:504,4:4:424,5:4:624,6:4:424").unwrap();
        assert_eq!(layout.heights, [0.0, 0.0, 0.0, 0.0, 0.0, 624.0, 0.0]);
        assert_eq!(layout.spans, [8, 4, 7, 5, 4, 4, 4]);
    }

    #[test]
    fn resized_cards_keep_minimum_sizes_and_fit_the_page() {
        let mut layout = DashboardLayout::default();
        let before = layout.rows(736.0)[0][0].1;
        let start_span = (before + GAP) / ((736.0 + GAP) / 12.0);
        layout.resize(0, vec2(start_span, 400.0), vec2(-125.0, 0.0), 736.0);
        assert!(layout.rows(736.0)[0][0].1 < before);
        for width in [736.0, 1160.0] {
            for card in 0..7 {
                layout.resize(card, vec2(8.0, 500.0), vec2(-10_000.0, -10_000.0), width);
                let minimum = DashboardLayout::minimum(card);
                assert!(layout.heights[card] >= minimum.y);
                for row in layout.rows(width) {
                    let occupied: f32 = row.iter().map(|(_, w)| w).sum::<f32>() + GAP * (row.len() - 1) as f32;
                    assert!(occupied <= width + 0.1);
                    for (id, w) in row {
                        assert!(w + 0.1 >= DashboardLayout::minimum(id).x);
                        let columns = (w + GAP) / ((width + GAP) / 12.0);
                        assert!((columns - columns.round()).abs() < 0.001);
                    }
                    assert!(((layout.heights[card] + GAP) / ROW).fract().abs() < 0.001);
                }
            }
            layout.resize(2, vec2(7.0, 500.0), vec2(10_000.0, 10_000.0), width);
            assert_eq!(layout.spans[2], 12);
            assert_eq!(layout.heights[2], MAX_HEIGHT);
        }
    }
}
