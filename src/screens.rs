//! Telas de Fluxo de caixa, Cartões, Nossa casa e Reserva, e os blocos novos de Ajustes.

use crate::app::App;
use crate::backend::Cmd;
use crate::connected::ConnectedCard;
use crate::dashboard::{budget_block, card_face, connected_face, connected_money, housing_share, split_block};
use crate::model::*;
use crate::widgets::{self as w, NORMAL};
use egui::{pos2, vec2, Align, Align2, CornerRadius, Layout, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType};
use egui_phosphor::regular as icon;
use std::cmp::Reverse;

/// Filtros do Fluxo de caixa (ficam ao trocar de mês).
#[derive(Default)]
pub struct Flow {
    pub kind: Option<Kind>,
    pub category: Option<Category>,
    pub origin: Option<Origin>,
    pub search: String,
    /// Resultado do último "Copiar do mês anterior".
    pub note: Option<String>,
}

impl Flow {
    fn keeps(&self, e: &Entry) -> bool {
        let needle = self.search.trim().to_lowercase();
        self.kind.is_none_or(|k| e.kind == k)
            && self.category.is_none_or(|c| e.category == c)
            && self.origin.is_none_or(|o| Origin::of(e) == o)
            && (needle.is_empty() || e.desc.to_lowercase().contains(&needle))
    }
}

/// Conteúdo do tooltip de "Atualizar contas": o caminho da compra até o app e como adiantar.
pub(crate) fn sync_tip(ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    ui.set_max_width(320.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    w::text(ui, "Reimporta bancos e cartões da Pluggy", w::semibold(13.0), colors.text);
    w::caption(ui, "Pode levar até um minuto.");
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    for (ic, title, detail) in [
        (icon::BANK, "Banco → Meu Pluggy", "atualiza sozinho a cada 24 h (limite do Open Finance por banco)."),
        (icon::ARROWS_CLOCKWISE, "Meu Pluggy → Flounder", "reimporta a cada 6 h — e na hora, clicando aqui."),
    ] {
        ui.horizontal(|ui| {
            w::text(ui, ic, w::regular(15.0), colors.blue);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                w::text(ui, title, w::semibold(12.0), colors.text);
                w::caption(ui, detail);
            });
        });
        ui.add_space(2.0);
    }
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(egui::RichText::new(format!("{} Para adiantar: atualize a conexão no meu.pluggy.ai e depois clique aqui.", icon::SPARKLE)).font(w::regular(12.0)).color(colors.blue));
    ui.label(egui::RichText::new("Sem novidade em 24–48 h? Veja status.pluggy.ai ou refaça o compartilhamento no app do banco.").font(w::regular(12.0)).color(colors.muted));
}

const WEEKDAYS: [&str; 7] = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];

fn day_title(d: Date) -> String {
    let m = Month::of(d);
    format!("{}, {} de {}", WEEKDAYS[((m.first_weekday() + d.d - 1) % 7) as usize], d.d, m.name().to_lowercase())
}

pub fn cash_flow(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let (me, couple, month) = (app.data.me, app.mode == Mode::Couple, app.month);
    let names = app.data.names.clone();
    let all: Vec<Entry> = app.data.in_month(month).filter(|e| couple || e.owner == me).cloned().collect();
    let mut rows: Vec<Entry> = all.iter().filter(|e| app.flow.keeps(e)).cloned().collect();
    rows.sort_by_key(|e| Reverse(e.date));
    let sum = |k: Kind| rows.iter().filter(|e| e.kind == k).map(|e| e.amount).sum::<Cents>();
    let (income, out) = (sum(Kind::Income), sum(Kind::Expense));
    let mut breakdown = Summary { expenses: out, ..Default::default() };
    for e in rows.iter().filter(|e| e.kind == Kind::Expense) {
        match breakdown.by_category.iter_mut().find(|(c, _)| *c == e.category) {
            Some((_, v)) => *v += e.amount,
            None => breakdown.by_category.push((e.category, e.amount)),
        }
    }
    breakdown.by_category.sort_by_key(|(_, v)| Reverse(*v));

    let (mut copy, mut add, mut clear, mut open) = (false, false, false, None);
    w::card(ui, NORMAL, |ui| {
        w::title_with_art(ui, "Fluxo de caixa", Some(w::Art::Bars));
        ui.columns(3, |c| {
            let result = income - out;
            for (ui, (label, v, color)) in c.iter_mut().zip([
                ("Entradas", income, colors.green),
                ("Saídas", -out, colors.text),
                ("Resultado", result, if result < 0 { colors.red } else { colors.green }),
            ]) {
                w::group(ui, |ui| {
                    w::caption(ui, label);
                    w::amount(ui, v, w::semibold(20.0), color);
                });
            }
        });
        account_balances(app, ui);
        ui.add_space(w::GROUP_GAP);
        ui.horizontal_wrapped(|ui| {
            let b = ui.add(egui::Button::new(format!("{} Copiar do mês anterior", icon::COPY)).min_size(vec2(0.0, 32.0)));
            b.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Copiar do mês anterior"));
            copy = b.on_hover_text("Traz os lançamentos manuais (sem cartão) do mês anterior, ainda não pagos.")
                .clicked();
            if let Some(note) = &app.flow.note {
                w::caption(ui, note.as_str());
            }
        });
    });

    // Contas fixas que este mês ainda não tem: moradia, luz, gás… com o valor estimado
    // pela média das últimas contas. Um toque lança a conta real no mês.
    if month >= Month::of(today()) {
        let forecast: Vec<Recurring> = app.data.forecast(month, (!couple).then_some(me));
        if !forecast.is_empty() {
            let mut create = Vec::new();
            w::card(ui, NORMAL, |ui| {
                w::title_with_art(ui, &format!("Previsão das contas fixas de {}", month.name().to_lowercase()), Some(w::Art::Calendar));
                w::caption(ui, "Média das últimas contas marcadas como fixa. Toque em + para lançar a conta real deste mês.");
                ui.add_space(w::GROUP_GAP);
                w::list(ui, |ui| {
                    for f in &forecast {
                        forecast_row(ui, f, month, couple.then(|| names[f.owner.idx()].as_str()), &mut create);
                    }
                });
                let total: Cents = forecast.iter().map(|f| f.estimate).sum();
                w::caption(ui, format!("Se tudo se repetir: {} de contas fixas neste mês.", money(total)));
            });
            for e in create {
                let id = app.data.upsert(e);
                app.push(id);
            }
        }
    }

    w::cards_row(ui, NORMAL, &[2.0, 1.0], 300.0, |i, ui| {
        if i == 1 {
            if breakdown.by_category.is_empty() {
                w::title(ui, "Saídas por categoria");
                return w::empty_note(ui, "Nenhuma saída na lista.");
            }
            return budget_block(&breakdown, ui, "Saídas por categoria", 8);
        }
        w::title(ui, &format!("Lançamentos de {}", month.name().to_lowercase()));
        let f = &mut app.flow;
        ui.horizontal_wrapped(|ui| {
            w::segmented(ui, &mut f.kind, &[(None, "Todos"), (Some(Kind::Income), "Entradas"), (Some(Kind::Expense), "Saídas")]);
            w::segmented(ui, &mut f.origin, &[(None, "Todas"), (Some(Origin::Bank), "Banco"), (Some(Origin::Card), "Cartão"), (Some(Origin::Manual), "Manual")]);
            egui::ComboBox::from_id_salt("flow-category")
                .selected_text(f.category.map_or("Todas as categorias", Category::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.category, None, "Todas as categorias");
                    for c in Category::ALL {
                        ui.selectable_value(&mut f.category, Some(c), c.label());
                    }
                });
            ui.add(w::text_input(&mut f.search, &format!("{} Buscar descrição", icon::MAGNIFYING_GLASS)).desired_width(200.0));
        });
        ui.add_space(w::GROUP_GAP);
        if all.is_empty() {
            w::empty_note(ui, &format!("Nenhum lançamento em {}.", month.name().to_lowercase()));
            add = w::primary_button(ui, &format!("{} Adicionar lançamento", icon::PLUS)).clicked();
            w::caption(ui, "Ou traga as contas fixas com \"Copiar do mês anterior\".");
            return;
        }
        if rows.is_empty() {
            w::empty_note(ui, "Nenhum lançamento com esses filtros.");
            clear = ui.button("Limpar filtros").clicked();
            return;
        }
        for (k, e) in rows.iter().enumerate() {
            if k == 0 || rows[k - 1].date != e.date {
                ui.add_space(if k == 0 { 0.0 } else { 8.0 });
                w::text(ui, day_title(e.date), w::semibold(13.0), colors.muted);
            }
            let who = couple.then(|| names[e.owner.idx()].as_str());
            if entry_row(ui, e, who).clicked() {
                open = Some(e.id);
            }
        }
    });

    if copy {
        let ids = app.data.copy_previous(month, (!couple).then_some(me));
        for &id in &ids {
            app.push(id);
        }
        let prev = month.add(-1).name().to_lowercase();
        app.flow.note = Some(match ids.len() {
            0 => format!("Nada a copiar de {prev}: os lançamentos manuais já estão aqui."),
            1 => format!("1 lançamento copiado de {prev}."),
            n => format!("{n} lançamentos copiados de {prev}."),
        });
    }
    if clear {
        app.flow = Flow { note: app.flow.note.take(), ..Default::default() };
    }
    if add {
        app.add();
    }
    if let Some(id) = open {
        app.open_entry(id);
    }
}

/// "Saldo em conta" por membro, em reais; outras moedas aparecem à parte, sem conversão.
fn account_balances(app: &App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let me = app.data.me.idx() as u8;
    let accounts: Vec<_> = app.accounts.iter().filter(|a| app.mode == Mode::Couple || a.owner == me).collect();
    if accounts.is_empty() {
        return;
    }
    ui.add_space(w::GROUP_GAP);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        w::text(ui, format!("{} Saldo em conta", icon::BANK), w::semibold(14.0), colors.text);
        for p in 0..2u8 {
            let brl: Vec<i64> = accounts.iter().filter(|a| a.owner == p && a.currency == "BRL").filter_map(|a| a.balance).collect();
            if !brl.is_empty() {
                ui.add_space(8.0);
                w::person_dot(ui, &app.data.names[p as usize], colors.person[p as usize]);
                w::text(ui, format!("{} {}", app.data.names[p as usize], money(brl.iter().sum())), w::regular(14.0), colors.text);
            }
        }
    });
    for a in accounts.iter().filter(|a| a.currency != "BRL") {
        w::caption(ui, format!("{} · {} ({}): {}", a.institution, a.name, app.data.names[a.owner.min(1) as usize], connected_money(a.balance, &a.currency)));
    }
}

/// Linha da previsão: descrição, vencimento típico, variação recente e botão de lançar.
fn forecast_row(ui: &mut Ui, f: &Recurring, month: Month, who: Option<&str>, create: &mut Vec<Entry>) {
    let colors = w::palette(ui.ctx());
    let income = f.kind == Kind::Income;
    let estimate = format!("{}{}", if income { "+" } else { "−" }, money(f.estimate));
    let mut details = format!("vence por volta do dia {} · {}", f.day, f.range());
    if let Some(who) = who {
        details = format!("{details} · {who}");
    }
    if f.shared {
        details = format!("{details} · Casa");
    }
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Lançar {}: {estimate} · {details}", f.desc)));
    let hover = ui.ctx().animate_bool_with_time(r.id.with("h"), r.hovered(), 0.15);
    let click = ui.allocate_rect(
        Rect::from_min_size(pos2(rect.right() - 90.0, rect.center().y - 14.0), vec2(84.0, 28.0)),
        Sense::click(),
    ).on_hover_cursor(egui::CursorIcon::PointingHand);
    click.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Lançar {}", f.desc)));
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect.expand2(vec2(6.0, 0.0)), CornerRadius::same(10), colors.hover.gamma_multiply(hover));
    }
    let c = pos2(rect.left() + 14.0, rect.center().y);
    let (ic, fg, bg) = if income { (icon::ARROW_DOWN_LEFT, colors.green, colors.green_soft) } else { (icon::ARROW_UP_RIGHT, colors.muted, colors.faint) };
    p.circle_filled(c, 14.0, bg);
    p.text(c, Align2::CENTER_CENTER, ic, w::regular(16.0), fg);
    let amt = p.layout_no_wrap(estimate, w::semibold(14.0), if income { colors.green } else { colors.text });
    let st = p.layout_no_wrap("lançar".into(), w::regular(12.0), colors.blue);
    let right = rect.right() - amt.size().x.max(st.size().x);
    p.galley(pos2(rect.right() - amt.size().x, rect.center().y - amt.size().y), amt, colors.text);
    p.galley(pos2(rect.right() - st.size().x, rect.center().y + 2.0), st, colors.blue);
    if click.clicked() {
        create.push(f.entry(month));
    }
    let x0 = c.x + 24.0;
    let mut job = egui::text::LayoutJob::simple_singleline(f.desc.clone(), w::regular(14.0), colors.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - 16.0 - x0).max(20.0));
    let g = p.layout_job(job);
    p.galley(pos2(x0, rect.center().y - g.size().y), g, colors.text);
    let mut job = egui::text::LayoutJob::simple_singleline(details, w::regular(12.0), colors.muted);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - 16.0 - x0 - 6.0).max(20.0));
    let g = p.layout_job(job);
    p.galley(pos2(x0, rect.center().y + 3.0), g, colors.muted);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, colors.line));
}

/// Linha de lançamento: tipo, descrição, categoria/pessoa/origem, valor com sinal e situação.
fn entry_row(ui: &mut Ui, e: &Entry, who: Option<&str>) -> egui::Response {
    let colors = w::palette(ui.ctx());
    let origin = Origin::of(e);
    let income = e.kind == Kind::Income;
    let (state, state_color) = match (origin == Origin::Card, income, status(e.paid, e.date)) {
        (true, ..) => ("Na fatura", colors.muted),
        (_, true, Status::Paid) => ("Recebido", colors.green),
        (_, true, _) => ("A receber", colors.muted),
        (_, false, Status::Paid) => ("Pago", colors.green),
        (_, false, Status::Overdue) => ("Vencido", colors.red),
        (_, false, Status::Due) => ("A pagar", colors.muted),
    };
    let value = format!("{}{}", if income { "+" } else { "−" }, money(e.amount));
    let mut details = vec![e.category.label().to_string()];
    details.extend(who.map(str::to_string));
    if e.shared {
        details.push("Casa".into());
    }
    if e.recurring {
        details.push("Fixa".into());
    }
    let details = details.join(" · ");
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Abrir {}: {value} · {details} · {} · {state}", e.desc, origin.label())));
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let hover = ui.ctx().animate_bool_with_time(r.id.with("h"), r.hovered() || r.has_focus(), 0.15);
    if hover > 0.0 {
        p.rect_filled(rect.expand2(vec2(6.0, 0.0)), CornerRadius::same(10), colors.hover.gamma_multiply(hover));
    }
    if r.has_focus() {
        p.rect_stroke(rect.expand2(vec2(6.0, 0.0)), CornerRadius::same(8), Stroke::new(2.0, colors.blue), egui::StrokeKind::Inside);
    }
    let c = pos2(rect.left() + 14.0, rect.center().y);
    let (ic, fg, bg) = if income { (icon::ARROW_DOWN_LEFT, colors.green, colors.green_soft) } else { (icon::ARROW_UP_RIGHT, colors.muted, colors.faint) };
    p.circle_filled(c, 14.0, bg);
    p.text(c, Align2::CENTER_CENTER, ic, w::regular(16.0), fg);
    let amt = p.layout_no_wrap(value, w::semibold(14.0), if income { colors.green } else { colors.text });
    let st = p.layout_no_wrap(state.into(), w::regular(12.0), state_color);
    let right = rect.right() - amt.size().x.max(st.size().x);
    p.galley(pos2(rect.right() - amt.size().x, rect.center().y - amt.size().y), amt, colors.text);
    p.galley(pos2(rect.right() - st.size().x, rect.center().y + 2.0), st, state_color);
    let x0 = c.x + 24.0;
    let mut job = egui::text::LayoutJob::simple_singleline(e.desc.clone(), w::regular(14.0), colors.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - 16.0 - x0).max(20.0));
    let g = p.layout_job(job);
    p.galley(pos2(x0, rect.center().y - g.size().y), g, colors.text);
    // Selo de origem, depois os detalhes (truncados).
    let (ofg, obg) = match origin {
        Origin::Bank => (colors.blue, colors.blue_soft),
        Origin::Card => (colors.coral, colors.coral_soft),
        Origin::Manual => (colors.muted, colors.faint),
    };
    let og = p.layout_no_wrap(origin.label().into(), w::semibold(11.0), ofg);
    let chip = Rect::from_min_size(pos2(x0, rect.center().y + 3.0), og.size() + vec2(10.0, 2.0));
    p.rect_filled(chip, CornerRadius::same(255), obg);
    p.galley(chip.center() - og.size() / 2.0, og, ofg);
    let mut job = egui::text::LayoutJob::simple_singleline(details, w::regular(12.0), colors.muted);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - 16.0 - chip.right() - 6.0).max(20.0));
    let g = p.layout_job(job);
    p.galley(pos2(chip.right() + 6.0, chip.center().y - g.size().y / 2.0), g, colors.muted);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, colors.line));
    r
}

pub fn cards(app: &mut App, ui: &mut Ui) {
    let me = app.data.me.idx() as u8;
    let couple = app.mode == Mode::Couple;
    let cards: Vec<ConnectedCard> = app.connected_cards.iter().flatten().filter(|c| couple || c.owner == me).cloned().collect();
    if cards.is_empty() {
        return w::card(ui, NORMAL, |ui| {
            w::title_with_art(ui, "Cartões", Some(w::Art::Card));
            w::empty_note(ui, "Nenhum cartão conectado.");
            w::caption(ui, "Conecte primeiro o banco no Meu Pluggy (meu.pluggy.ai) e depois autorize aqui.");
            connect_button(app, ui);
        });
    }
    for card in &cards {
        w::card(ui, NORMAL, |ui| ui.push_id(&card.id, |ui| card_section(app, card, ui)).inner);
    }
    installments_block(app, ui);
}

/// Compras parceladas com parcelas por vir e quanto já está comprometido nos próximos meses.
fn installments_block(app: &App, ui: &mut Ui) {
    let couple = app.mode == Mode::Couple;
    let list = app.data.installments(app.month, (!couple).then_some(app.data.me));
    let colors = w::palette(ui.ctx());
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Parcelas em andamento");
        if list.is_empty() {
            return w::empty_note(ui, "Nenhuma compra parcelada com parcelas por vir.");
        }
        let total: Cents = list.iter().map(Installment::remaining).sum();
        w::caption(ui, format!(
            "{} {} · faltam {} · estimado pelo valor da última parcela, até {} {}.",
            list.len(), if list.len() == 1 { "compra" } else { "compras" }, money(total), app.month.name().to_lowercase(), app.month.y,
        ));
        ui.add_space(w::GROUP_GAP);
        w::text(ui, "Comprometido nos próximos meses", w::semibold(14.0), colors.text);
        ui.horizontal_wrapped(|ui| {
            for k in 1..=6 {
                let m = app.month.add(k);
                let v: Cents = list.iter().filter(|i| i.due_in(m)).map(|i| i.amount).sum();
                ui.vertical(|ui| {
                    w::caption(ui, m.short());
                    w::text(ui, money(v), w::semibold(14.0), if v == 0 { colors.muted } else { colors.text });
                });
                ui.add_space(20.0);
            }
        });
        ui.add_space(w::GROUP_GAP);
        w::list(ui, |ui| {
            for i in &list {
                installment_row(ui, i, couple.then(|| app.data.names[i.owner.idx()].as_str()));
            }
        });
    });
}

fn installment_row(ui: &mut Ui, i: &Installment, who: Option<&str>) {
    let colors = w::palette(ui.ctx());
    let mut details = format!("parcela {}/{} · faltam {} · até {}", i.n, i.of, i.left(), i.ends().short());
    if let Some(who) = who {
        details = format!("{details} · {who}");
    }
    let per = format!("{}/mês", money(i.amount));
    let rest = format!("resta {}", money(i.remaining()));
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, format!("{}: {details} · {per} · {rest}", i.desc)));
    let p = ui.painter();
    let amt = p.layout_no_wrap(per, w::semibold(14.0), colors.text);
    let st = p.layout_no_wrap(rest, w::regular(12.0), colors.muted);
    let right = rect.right() - amt.size().x.max(st.size().x);
    p.galley(pos2(rect.right() - amt.size().x, rect.center().y - amt.size().y), amt, colors.text);
    p.galley(pos2(rect.right() - st.size().x, rect.center().y + 2.0), st, colors.muted);
    for (text, font, color, y) in [(i.desc.clone(), w::regular(14.0), colors.text, -1.0), (details, w::regular(12.0), colors.muted, 0.0)] {
        let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width((right - 16.0 - rect.left()).max(20.0));
        let g = p.layout_job(job);
        let top = if y < 0.0 { rect.center().y - g.size().y } else { rect.center().y + 2.0 };
        p.galley(pos2(rect.left(), top), g, color);
    }
    // Linha de baixo vira barra de progresso das parcelas pagas.
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, colors.line));
    let done = rect.left() + rect.width() * i.n as f32 / i.of as f32;
    p.line_segment([rect.left_bottom(), pos2(done, rect.bottom())], Stroke::new(2.0, colors.blue));
}

fn card_section(app: &mut App, card: &ConnectedCard, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let month = app.month;
    let owner = &app.data.names[card.owner.min(1) as usize];
    w::title(ui, &format!("{} · {}", card.institution, owner));
    let wide = ui.available_width() >= 640.0;
    let mut opened = false;
    if wide {
        ui.horizontal_top(|ui| {
            opened = ui.allocate_ui(vec2(320.0, 124.0), |ui| card_face(ui, connected_face(card, month)).clicked()).inner;
            ui.add_space(24.0);
            ui.vertical(|ui| limits(ui, card));
        });
    } else {
        opened = card_face(ui, connected_face(card, month)).clicked();
        ui.add_space(w::GROUP_GAP);
        limits(ui, card);
    }
    if opened {
        app.connected_invoice = Some(card.id.clone());
    }
    ui.add_space(w::GROUP_GAP);
    ui.separator();
    let mut toggle = None;
    let mut select = None;
    let mut open = None;
    let mut invoices = |ui: &mut Ui| {
        w::text(ui, "Faturas", w::semibold(14.0), colors.text);
        w::caption(ui, "Clique numa fatura para ver as compras do mês dela.");
        let mut bills: Vec<(usize, &ConnectedInvoice)> = app.data.connected_invoices.iter().enumerate().filter(|(_, i)| i.card == card.id).collect();
        bills.sort_by_key(|(_, i)| Reverse(i.due));
        w::list(ui, |ui| {
            for (i, inv) in bills {
                let b = Bill {
                    target: BillRef::Connected(i),
                    label: format!("{} · {}", inv.label, Month::of(inv.due).short()),
                    amount: inv.amount,
                    date: inv.due,
                    status: status(inv.paid, inv.due),
                };
                let ev = w::bill_row(ui, &b, true, true);
                if ev.toggled {
                    toggle = Some(b.target);
                } else if ev.opened {
                    select = Some(Month::of(inv.due));
                }
            }
        });
        // Sem valor ou em outra moeda: só consulta (não viram contas a pagar).
        for b in card.bills.iter().filter(|b| b.currency != "BRL" || b.total.is_none()) {
            w::caption(ui, format!("Vence {} · {}", crate::dashboard::short_iso(&b.due), connected_money(b.total, &b.currency)));
        }
        if card.bills.is_empty() {
            w::empty_note(ui, if card.bills_available { "Nenhuma fatura recebida." } else { "Faturas não disponibilizadas pela instituição." });
        }
    };
    let mut purchases = |ui: &mut Ui| {
        w::text(ui, format!("Compras de {} {}", month.name().to_lowercase(), month.y), w::semibold(14.0), colors.text);
        let mut items: Vec<&Entry> = app.data.in_month(month).filter(|e| e.connected.as_deref() == Some(card.id.as_str())).collect();
        items.sort_by_key(|e| Reverse(e.date));
        let total: Cents = items.iter().map(|e| if e.kind == Kind::Income { -e.amount } else { e.amount }).sum();
        let prefix = format!("{:04}-{:02}-", month.y, month.m);
        let bill = card.bills.iter().find(|b| b.due.starts_with(&prefix));
        // ponytail: compras do mês civil, sem data de fechamento; a soma pode diferir da fatura.
        w::caption(ui, match bill {
            Some(b) => format!("Total {} · fatura que vence em {}: {}", money(total), month.name().to_lowercase(), connected_money(b.total, &b.currency)),
            None => format!("Total {} · nenhuma fatura vence em {}", money(total), month.name().to_lowercase()),
        });
        if items.is_empty() {
            return w::empty_note(ui, "Nenhuma compra neste mês.");
        }
        w::list(ui, |ui| {
            for e in items {
                // O dia na linha: compras repetidas (mercado, lanchonete…) ficam reconhecíveis.
                if entry_row(ui, e, Some(&format!("dia {}", e.date.d))).clicked() {
                    open = Some(e.id);
                }
            }
        });
    };
    if ui.available_width() >= 760.0 {
        ui.columns(2, |c| {
            let gutter = |left, right| egui::Frame::new().inner_margin(egui::Margin { left, right, top: 0, bottom: 0 });
            gutter(0, 12).show(&mut c[0], |ui| invoices(ui));
            gutter(12, 0).show(&mut c[1], |ui| purchases(ui));
        });
    } else {
        invoices(ui);
        ui.add_space(w::GROUP_GAP);
        purchases(ui);
    }
    if let Some(t) = toggle {
        app.toggle(t);
    }
    if let Some(m) = select {
        app.set_month(m);
    }
    if let Some(id) = open {
        app.open_entry(id);
    }
}

fn limits(ui: &mut Ui, card: &ConnectedCard) {
    let colors = w::palette(ui.ctx());
    let used = card.limit.zip(card.available).map(|(l, a)| l - a).or(card.balance);
    w::text(ui, "Limite", w::semibold(14.0), colors.text);
    if let (Some(limit), Some(used)) = (card.limit, used) && limit > 0 {
        let frac = used as f32 / limit as f32;
        w::progress(ui, frac, if frac > 0.9 { colors.red } else if frac > 0.7 { colors.amber } else { colors.blue }, 10.0, None);
    }
    for (label, v) in [("Limite total", card.limit), ("Usado", used), ("Livre", card.available)] {
        ui.horizontal(|ui| {
            w::caption(ui, label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                w::text(ui, connected_money(v, &card.currency), w::semibold(14.0), colors.text);
            });
        });
    }
}

// ───────────────────────── Nossa casa ─────────────────────────

/// Janelas abertas a partir de Nossa casa, Reserva e Ajustes.
#[derive(Default)]
pub struct Extra {
    pub settle: Option<SettleForm>,
    /// Id do acerto cuja exclusão espera confirmação.
    pub unsettle: Option<String>,
    pub goal: Option<GoalForm>,
    /// Nomes em edição em Ajustes; `None` mostra os salvos.
    pub names: Option<[String; 2]>,
    pub names_tried: bool,
}

impl Extra {
    pub fn open(&self) -> bool {
        self.settle.is_some() || self.unsettle.is_some() || self.goal.is_some()
    }
}

pub struct SettleForm {
    pub amount: String,
    pub date: String,
    pub from: Person,
    pub tried: bool,
}

impl SettleForm {
    fn errors(&self) -> [Option<&'static str>; 2] {
        [money_error(&self.amount, true), Date::parse(&self.date).is_none().then_some("Use uma data válida no formato dd/mm/aaaa.")]
    }
}

pub struct GoalForm {
    pub name: String,
    pub target: String,
    pub opening: String,
    pub tried: bool,
}

impl GoalForm {
    fn errors(&self) -> [Option<&'static str>; 3] {
        [
            (self.name.trim().chars().count() > 60).then_some("Use até 60 caracteres."),
            money_error(&self.target, true),
            if self.opening.trim().is_empty() { None } else { money_error(&self.opening, false) },
        ]
    }
}

fn money_error(s: &str, positive: bool) -> Option<&'static str> {
    match parse_money(s) {
        None if s.trim().is_empty() => Some("Informe o valor."),
        None => Some("Use um valor como 1.234,56."),
        Some(0) if positive => Some("O valor precisa ser maior que zero."),
        Some(_) => None,
    }
}

/// Mesmas regras do backend: 1 a 40 caracteres, sem caracteres de controle.
pub fn name_error(s: &str) -> Option<&'static str> {
    let s = s.trim();
    if s.is_empty() {
        Some("Informe um nome.")
    } else if s.chars().count() > 40 {
        Some("Use até 40 caracteres.")
    } else if s.chars().any(char::is_control) {
        Some("Remova caracteres especiais.")
    } else {
        None
    }
}

/// Casal: despesas compartilhadas do mês, quem pagou, saldo acumulado entre os dois e acertos.
/// Com o rateio desligado, mostra as despesas da casa sem divisão. Individual ("Meu orçamento"):
/// orçamento pessoal por categoria e moradia.
pub fn house(app: &mut App, s: &Summary, ui: &mut Ui) {
    if app.mode == Mode::Individual {
        return my_budget(app, s, ui);
    }
    if app.data.shared_split_off {
        return house_without_split(app, ui);
    }
    let colors = w::palette(ui.ctx());
    let month = app.month;
    let names = app.data.names.clone();
    let weights = app.data.income_weights(month);
    let mut rows: Vec<Entry> = app.data.in_month(month).filter(|e| e.shared && e.kind == Kind::Expense).cloned().collect();
    rows.sort_by_key(|e| Reverse(e.date));
    let mut settlements: Vec<Settlement> = app.data.settlements.iter().filter(|x| Month::of(x.date) <= month).cloned().collect();
    settlements.sort_by_key(|x| Reverse(x.date));
    let (mut settle, mut open, mut unsettle) = (false, None, None);
    w::cards_row(ui, NORMAL, &[2.0, 1.0], 300.0, |i, ui| {
        if i == 1 {
            housing_share(s, ui, true);
            ui.add_space(w::GROUP_GAP);
            ui.separator();
            w::text(ui, format!("Parte de cada um em {}", month.name().to_lowercase()), w::semibold(14.0), colors.text);
            for ((name, owed), color) in names.iter().zip(s.owed).zip(colors.person) {
                w::kv_dot(ui, Some(color), name, owed, colors.text);
            }
            w::caption(ui, match weights {
                Some([wa, wb]) => format!("Rateio pela renda: {} {:.0}% · {} {:.0}%.", names[0], wa * 100.0, names[1], wb * 100.0),
                None => "Sem renda registrada: metade para cada um.".to_string(),
            });
            let (ahead, v) = if s.paid_by[0] >= s.owed[0] { (0, s.paid_by[0] - s.owed[0]) } else { (1, s.owed[0] - s.paid_by[0]) };
            w::caption(ui, if v == 0 { "Neste mês cada um pagou a sua parte.".to_string() } else { format!("Neste mês, {} pagou {} além da própria parte.", names[ahead], money(v)) });
            return;
        }
        split_block(app, s, ui, true);
        ui.add_space(w::GROUP_GAP);
        settle = w::primary_button(ui, &format!("{} Registrar acerto", icon::HANDSHAKE)).clicked();
    });
    w::cards_row(ui, NORMAL, &[2.0, 1.0], 300.0, |i, ui| {
        if i == 1 {
            w::title(ui, "Acertos");
            w::caption(ui, "Transferências entre vocês para zerar o saldo da casa.");
            if settlements.is_empty() {
                return w::empty_note(ui, "Nenhum acerto registrado.");
            }
            w::list(ui, |ui| {
                for x in &settlements {
                    if settlement_row(ui, x, &names) {
                        unsettle = Some(x.id.clone());
                    }
                }
            });
            return;
        }
        w::title(ui, &format!("Despesas da casa em {}", month.name().to_lowercase()));
        if rows.is_empty() {
            w::empty_note(ui, &format!("Nenhuma despesa da casa em {}.", month.name().to_lowercase()));
            w::caption(ui, "Ao adicionar um lançamento, marque \"Despesa compartilhada da casa\".");
            return;
        }
        for (k, e) in rows.iter().enumerate() {
            if k == 0 || rows[k - 1].date != e.date {
                ui.add_space(if k == 0 { 0.0 } else { 8.0 });
                w::text(ui, day_title(e.date), w::semibold(13.0), colors.muted);
            }
            if entry_row(ui, e, Some(&format!("{} pagou", names[e.owner.idx()]))).clicked() {
                open = Some(e.id);
            }
        }
    });
    if settle {
        let (from, v) = if s.settlement >= 0 { (Person::B, s.settlement) } else { (Person::A, -s.settlement) };
        let date = if month == Month::of(today()) { today() } else { month.day(31) };
        app.extra.settle = Some(SettleForm { amount: if v > 0 { input_money(v) } else { String::new() }, date: date.fmt(), from, tried: false });
    }
    if let Some(id) = open {
        app.open_entry(id);
    }
    if unsettle.is_some() {
        app.extra.unsettle = unsettle;
    }
}

/// Casal com o rateio desligado: as despesas compartilhadas do mês, cada uma valendo por
/// inteiro para quem pagou. Sem divisão, sem saldo entre vocês e sem acertos.
fn house_without_split(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let month = app.month;
    let names = app.data.names.clone();
    let mut rows: Vec<Entry> = app.data.in_month(month).filter(|e| e.shared && e.kind == Kind::Expense).cloned().collect();
    rows.sort_by_key(|e| Reverse(e.date));
    let mut open = None;
    w::card(ui, NORMAL, |ui| {
        w::title_with_art(ui, "Despesas da casa", Some(w::Art::House));
        w::caption(ui, "O rateio compartilhado está desligado: cada despesa vale por inteiro para quem pagou e nada é dividido entre vocês. Para dividir pela renda de novo, procure \"Gastos compartilhados\" em Ajustes.");
        ui.add_space(w::GROUP_GAP);
        if rows.is_empty() {
            return w::empty_note(ui, &format!("Nenhuma despesa da casa em {}.", month.name().to_lowercase()));
        }
        for (k, e) in rows.iter().enumerate() {
            if k == 0 || rows[k - 1].date != e.date {
                ui.add_space(if k == 0 { 0.0 } else { 8.0 });
                w::text(ui, day_title(e.date), w::semibold(13.0), colors.muted);
            }
            if entry_row(ui, e, Some(&format!("{} pagou", names[e.owner.idx()]))).clicked() {
                open = Some(e.id);
            }
        }
    });
    if let Some(id) = open {
        app.open_entry(id);
    }
}

/// Linha de acerto: data, quem pagou → quem recebeu, valor e excluir. Devolve true ao clicar em excluir.
fn settlement_row(ui: &mut Ui, x: &Settlement, names: &[String; 2]) -> bool {
    let colors = w::palette(ui.ctx());
    let (from, to) = (x.from.idx(), x.from.other().idx());
    let mut delete = false;
    ui.push_id(&x.id, |ui| {
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 44.0), Layout::left_to_right(Align::Center), |ui| {
            w::person_dot(ui, &names[from], colors.person[from]);
            w::text(ui, icon::ARROW_RIGHT, w::regular(14.0), colors.muted);
            w::person_dot(ui, &names[to], colors.person[to]);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = format!("Excluir acerto de {} em {}", money(x.amount), x.date.fmt());
                delete = w::icon_button(ui, icon::TRASH, &label).clicked();
                w::text(ui, money(x.amount), w::semibold(14.0), colors.text);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    w::text(ui, x.date.fmt(), w::regular(13.0), colors.muted);
                });
            });
        });
    });
    let r = ui.min_rect();
    ui.painter().line_segment([pos2(r.left(), ui.cursor().top()), pos2(r.right(), ui.cursor().top())], Stroke::new(1.0, colors.line));
    delete
}

fn my_budget(app: &mut App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let month = app.month;
    w::card(ui, NORMAL, |ui| {
        w::title_with_art(ui, "Meu orçamento", Some(w::Art::Bars));
        ui.columns(3, |c| {
            for (ui, (label, v, color)) in c.iter_mut().zip([
                ("Renda", s.income, colors.green),
                ("Despesas", -s.expenses, colors.text),
                ("Sobra", s.balance, if s.balance < 0 { colors.red } else { colors.green }),
            ]) {
                w::group(ui, |ui| {
                    w::caption(ui, label);
                    w::amount(ui, v, w::semibold(20.0), color);
                });
            }
        });
        w::caption(ui, "Tudo que sai da sua conta, inclusive as contas da casa que você paga.");
    });
    w::cards_row(ui, NORMAL, &[2.0, 1.0], 300.0, |i, ui| {
        if i == 1 {
            return housing_share(s, ui, true);
        }
        if s.by_category.is_empty() {
            w::title(ui, "Despesas por categoria");
            return w::empty_note(ui, &format!("Nenhuma despesa em {}.", month.name().to_lowercase()));
        }
        budget_block(s, ui, "Despesas por categoria", 10);
    });
}

// ───────────────────────── Reserva ─────────────────────────

/// Casal: reserva do domicílio (aportes dos dois). Individual: saldo inicial + os seus aportes.
pub fn reserve(app: &mut App, s: &Summary, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let (month, mode) = (app.month, app.mode);
    let goal = app.data.reserve_goal;
    let current = s.reserve;
    let pace = app.data.reserve_pace(month, mode);
    let months = app.data.monthly_deposits(month, mode, 12);
    let name = app.data.reserve_name.trim();
    let name = if name.is_empty() { "Reserva" } else { name }.to_string();
    let mut edit = false;
    w::card(ui, NORMAL, |ui| {
        w::title_with_art(ui, &name, Some(w::Art::Pig));
        if goal <= 0 {
            w::text(ui, "Defina uma meta", w::bold(20.0), colors.text);
            w::caption(ui, "Escolha quanto quer guardar (por exemplo, seis meses de despesas) e quanto já existe guardado. Os aportes são lançamentos pagos na categoria Reserva.");
            if current > 0 {
                w::caption(ui, format!("Guardado até agora: {}", money(current)));
            }
            ui.add_space(w::GROUP_GAP);
            edit = w::primary_button(ui, &format!("{} Definir meta", icon::TARGET)).clicked();
            return;
        }
        let reached = current >= goal;
        w::group(ui, |ui| {
            w::caption(ui, if mode == Mode::Couple {
                format!("Guardado pelo casal até {} de {}", month.name().to_lowercase(), month.y)
            } else {
                format!("Saldo inicial + seus aportes até {} de {}", month.name().to_lowercase(), month.y)
            });
            w::big_money(ui, current, 40.0, colors.text);
        });
        ui.add_space(w::GROUP_GAP);
        let frac = current as f32 / goal as f32;
        w::progress(ui, frac, if reached { colors.green } else { colors.blue }, 12.0, None);
        w::caption(ui, format!("{} da meta de {}", w::pct(frac * 100.0), money(goal)));
        ui.add_space(w::GROUP_GAP);
        ui.columns(3, |c| {
            for (ui, (label, v)) in c.iter_mut().zip([("Meta", goal), ("Saldo inicial", app.data.reserve_start), ("Falta", (goal - current).max(0))]) {
                w::group(ui, |ui| {
                    w::caption(ui, label);
                    w::amount(ui, v, w::semibold(16.0), colors.text);
                });
            }
        });
        ui.add_space(w::GROUP_GAP);
        ui.separator();
        match months_to_goal(current, goal, pace) {
            Some(0) => _ = w::text(ui, format!("{} Meta atingida", icon::CHECK_CIRCLE), w::semibold(14.0), colors.green),
            Some(n) => {
                let when = if n > 120 { "em mais de 10 anos".to_string() } else {
                    let end = month.add(n as i32);
                    format!("em {n} {} ({} de {})", if n == 1 { "mês" } else { "meses" }, end.name().to_lowercase(), end.y)
                };
                w::text(ui, format!("{} No ritmo atual, meta {when}", icon::TREND_UP), w::semibold(14.0), colors.text);
                w::caption(ui, format!("Média de {} por mês nos últimos 3 meses.", money(pace)));
            }
            None => {
                w::text(ui, format!("{} Sem aportes recentes", icon::CLOCK), w::semibold(14.0), colors.muted);
                w::caption(ui, "Nenhum aporte pago nos últimos 3 meses; a projeção aparece com o próximo.");
            }
        }
        ui.add_space(w::GROUP_GAP);
        let b = ui.add(egui::Button::new(format!("{} Editar meta", icon::PENCIL_SIMPLE)).min_size(vec2(0.0, 36.0)));
        b.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Editar meta"));
        edit = b.on_hover_text("Nome, valor da meta e saldo inicial").clicked();
    });
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Aportes por mês");
        w::caption(ui, format!("Últimos 12 meses · total {}", money(months.iter().map(|m| m.1).sum())));
        ui.add_space(w::GROUP_GAP);
        bars(ui, &months, month);
    });
    if edit {
        let d = &app.data;
        app.extra.goal = Some(GoalForm {
            name: d.reserve_name.clone(),
            target: if d.reserve_goal > 0 { input_money(d.reserve_goal) } else { String::new() },
            opening: input_money(d.reserve_start),
            tried: false,
        });
    }
}

/// Barras mensais simples, com o mês selecionado destacado.
fn bars(ui: &mut Ui, months: &[(Month, Cents)], current: Month) {
    let colors = w::palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 160.0), Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, months.iter().map(|(m, v)| format!("{}: {}", m.short(), money(*v))).collect::<Vec<_>>().join("; ")));
    let p = ui.painter();
    let plot = Rect::from_min_max(rect.min + vec2(0.0, 16.0), rect.max - vec2(0.0, 22.0));
    let max = months.iter().map(|m| m.1).max().unwrap_or(0).max(1) as f32;
    let slot = rect.width() / months.len().max(1) as f32;
    let mut tip = None;
    for (i, (m, v)) in months.iter().enumerate() {
        let x = rect.left() + slot * (i as f32 + 0.5);
        let h = (*v as f32 / max * plot.height()).max(2.0);
        let bar = Rect::from_center_size(pos2(x, plot.bottom() - h / 2.0), vec2((slot * 0.6).min(36.0), h));
        let sel = *m == current;
        p.rect_filled(bar, CornerRadius::same(4), if *v <= 0 { colors.faint } else if sel { colors.blue } else { colors.blue.gamma_multiply(0.45) });
        let value = p.layout_no_wrap(money(*v), w::regular(11.0), colors.muted);
        if *v > 0 && value.size().x < slot - 4.0 {
            p.galley(pos2(x - value.size().x / 2.0, bar.top() - value.size().y - 2.0), value, colors.muted);
        }
        p.text(pos2(x, rect.bottom() - 8.0), Align2::CENTER_CENTER, m.short(), if sel { w::semibold(12.0) } else { w::regular(12.0) }, if sel { colors.text } else { colors.muted });
        if resp.hover_pos().is_some_and(|h| (h.x - x).abs() < slot / 2.0) {
            tip = Some(format!("{}: {}", m.short(), money(*v)));
        }
    }
    p.line_segment([plot.left_bottom(), plot.right_bottom()], Stroke::new(1.0, colors.line));
    if let Some(t) = tip {
        resp.on_hover_text(t);
    }
}

// ───────────────────────── Janelas ─────────────────────────

pub fn modals(app: &mut App, ui: &mut Ui) {
    settle_modal(app, ui);
    unsettle_modal(app, ui);
    goal_modal(app, ui);
}

fn modal_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new().fill(w::palette(ui.ctx()).card).corner_radius(16).inner_margin(24)
}

/// Salvar/Cancelar; Enter também salva. Devolve (salvar, cancelar).
fn modal_buttons(ui: &mut Ui, save: &str) -> (bool, bool) {
    ui.add_space(8.0);
    let r = ui.horizontal(|ui| {
        let s = w::primary_button(ui, save).clicked();
        (s, ui.add(egui::Button::new("Cancelar").min_size(vec2(0.0, 36.0))).clicked())
    }).inner;
    (r.0 || ui.input(|i| i.key_pressed(egui::Key::Enter)), r.1)
}

fn settle_modal(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let names = app.data.names.clone();
    let Some(f) = app.extra.settle.as_mut() else { return };
    let (mut save, mut close) = (false, false);
    let resp = egui::Modal::new(egui::Id::new("settle")).frame(modal_frame(ui)).show(ui.ctx(), |ui| {
        ui.set_width(400.0);
        ui.spacing_mut().item_spacing.y = 8.0;
        w::text(ui, "Registrar acerto", w::bold(20.0), colors.text);
        w::caption(ui, "Transferência entre vocês para zerar o saldo da casa.");
        ui.add_space(w::GROUP_GAP);
        let err = if f.tried { f.errors() } else { [None; 2] };
        ui.columns(2, |c| {
            w::field(&mut c[0], "Valor", err[0], |ui| w::money_input(ui, &mut f.amount));
            w::field(&mut c[1], "Data", err[1], |ui| ui.add(w::text_input(&mut f.date, "dd/mm/aaaa")));
        });
        w::group(ui, |ui| {
            w::text(ui, "Quem pagou → quem recebeu", w::semibold(13.0), colors.text);
            let a = format!("{} → {}", names[0], names[1]);
            let b = format!("{} → {}", names[1], names[0]);
            w::segmented(ui, &mut f.from, &[(Person::A, &a), (Person::B, &b)]);
        });
        (save, close) = modal_buttons(ui, "Registrar");
    });
    if save {
        f.tried = true;
        if let (Some(amount), Some(date)) = (parse_money(&f.amount).filter(|v| *v > 0), Date::parse(&f.date)) {
            let from = f.from;
            let id = app.data.add_settlement(Settlement { id: String::new(), date, amount, from });
            app.send(Cmd::Settle(Settlement { id, date, amount, from }));
            close = true;
        }
    }
    if close || resp.should_close() {
        app.extra.settle = None;
    }
}

fn unsettle_modal(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let Some(id) = app.extra.unsettle.clone() else { return };
    let Some(x) = app.data.settlements.iter().find(|x| x.id == id).cloned() else {
        app.extra.unsettle = None;
        return;
    };
    let names = &app.data.names;
    let (mut delete, mut back) = (false, false);
    let resp = egui::Modal::new(egui::Id::new("unsettle")).frame(modal_frame(ui)).show(ui.ctx(), |ui| {
        ui.set_width(400.0);
        w::text(ui, "Excluir acerto?", w::bold(20.0), colors.text);
        w::caption(ui, format!("{} → {} · {} em {}. O saldo entre vocês volta a contar esse valor.", names[x.from.idx()], names[x.from.other().idx()], money(x.amount), x.date.fmt()));
        ui.add_space(w::GROUP_GAP);
        ui.horizontal(|ui| {
            delete = ui.add(egui::Button::new(egui::RichText::new("Excluir acerto").font(w::semibold(14.0)).color(colors.bg)).fill(colors.red).min_size(vec2(0.0, 36.0))).clicked();
            back = ui.add(egui::Button::new("Voltar").min_size(vec2(0.0, 36.0))).clicked();
        });
    });
    if delete {
        app.data.settlements.retain(|s| s.id != id);
        app.send(Cmd::Unsettle(id));
    }
    if delete || back || resp.should_close() {
        app.extra.unsettle = None;
    }
}

fn goal_modal(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let Some(f) = app.extra.goal.as_mut() else { return };
    let (mut save, mut close) = (false, false);
    let resp = egui::Modal::new(egui::Id::new("goal")).frame(modal_frame(ui)).show(ui.ctx(), |ui| {
        ui.set_width(420.0);
        ui.spacing_mut().item_spacing.y = 8.0;
        w::text(ui, "Meta da reserva", w::bold(20.0), colors.text);
        w::caption(ui, "Uma meta para o domicílio; vale para os dois.");
        ui.add_space(w::GROUP_GAP);
        let err = if f.tried { f.errors() } else { [None; 3] };
        w::field(ui, "Nome", err[0], |ui| ui.add(w::text_input(&mut f.name, "Ex.: Reserva de emergência")));
        ui.columns(2, |c| {
            w::field(&mut c[0], "Meta", err[1], |ui| w::money_input(ui, &mut f.target));
            w::field(&mut c[1], "Saldo inicial", err[2], |ui| w::money_input(ui, &mut f.opening));
        });
        w::caption(ui, "Saldo inicial: o que já estava guardado antes dos aportes lançados no app.");
        (save, close) = modal_buttons(ui, "Salvar");
    });
    if save {
        f.tried = true;
        if f.errors().iter().all(Option::is_none) {
            let r = crate::api::Reserve {
                opening_cents: parse_money(&f.opening).unwrap_or(0),
                target_cents: parse_money(&f.target).unwrap_or(0),
                name: f.name.trim().to_string(),
            };
            (app.data.reserve_start, app.data.reserve_goal, app.data.reserve_name) = (r.opening_cents, r.target_cents, r.name.clone());
            app.send(Cmd::Reserve(r));
            close = true;
        }
    }
    if close || resp.should_close() {
        app.extra.goal = None;
    }
}

// ───────────────────────── Ajustes ─────────────────────────

/// Nomes do casal, contas conectadas e como conectar.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let names = app.data.names.clone();
    let me = app.data.me.idx();
    let mut save = false;
    shared_settings(app, ui);    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Nomes");
        w::caption(ui, "Como cada um aparece no app, nos dois computadores.");
        ui.add_space(w::GROUP_GAP);
        let tried = app.extra.names_tried;
        let buf = app.extra.names.get_or_insert_with(|| names.clone());
        ui.columns(2, |c| {
            for (p, ui) in c.iter_mut().enumerate() {
                let label = format!("Pessoa {}{}", p + 1, if p == me { " (você)" } else { "" });
                let err = if tried { name_error(&buf[p]) } else { None };
                w::field(ui, &label, err, |ui| ui.add(w::text_input(&mut buf[p], "Nome")));
            }
        });
        let changed = buf.iter().zip(&names).any(|(a, b)| a.trim() != b);
        save = ui.add_enabled(changed, egui::Button::new("Salvar nomes").min_size(vec2(0.0, 36.0))).clicked();
    });
    if save && let Some(buf) = app.extra.names.clone() {
        app.extra.names_tried = true;
        if buf.iter().all(|n| name_error(n).is_none()) {
            for (p, name) in buf.iter().map(|n| n.trim().to_string()).enumerate() {
                if name != app.data.names[p] {
                    app.data.names[p] = name.clone();
                    if let Some(m) = app.remote.as_mut().and_then(|r| r.members.iter_mut().find(|m| m.position as usize == p)) {
                        m.name = name.clone();
                    }
                    app.send(Cmd::Member(p as u8, name));
                }
            }
            app.extra.names_tried = false;
        }
    }
    // Sem edição pendente, os campos acompanham os nomes salvos (inclusive os vindos do outro aparelho).
    if app.extra.names.as_ref().is_some_and(|b| *b == app.data.names) {
        app.extra.names = None;
    }

    let mut chosen = None;
    let mut refresh = false;
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Contas conectadas");
        w::caption(ui, "Bancos e cartões importados. Escolha o banco de cada conexão para corrigir nome e cor.");
        ui.add_space(w::GROUP_GAP);
        if app.connections.is_empty() {
            w::empty_note(ui, "Nenhuma conta conectada.");
        }
        w::list(ui, |ui| {
            for c in &app.connections {
                ui.push_id(&c.item_id, |ui| {
                    let catalog = c.institution_key.as_ref().and_then(|k| app.institutions.iter().find(|i| &i.key == k));
                    let rgb = catalog.map(|i| i.color)
                        .or_else(|| app.accounts.iter().find(|a| a.item_id == c.item_id).map(|a| a.color))
                        .or_else(|| app.connected_cards.iter().flatten().find(|k| k.item_id == c.item_id).map(|k| k.color));
                    let owner = &app.data.names[c.owner.min(1) as usize];
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 48.0), Layout::left_to_right(Align::Center), |ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 6.0, rgb.map_or(colors.muted, |[r, g, b]| egui::Color32::from_rgb(r, g, b)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mut key = c.institution_key.clone();
                            let combo = egui::ComboBox::from_id_salt("bank").width(200.0)
                                .selected_text(catalog.map_or("Não informado", |i| i.name.as_str()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut key, None, "Não informado");
                                    for i in &app.institutions {
                                        ui.selectable_value(&mut key, Some(i.key.clone()), &i.name);
                                    }
                                });
                            combo.response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, format!("Banco da conexão {} de {owner}", c.institution)));
                            if key != c.institution_key {
                                chosen = Some((c.item_id.clone(), key));
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                w::text(ui, &c.institution, w::semibold(14.0), colors.text);
                                w::text(ui, format!("{owner} · {}", synced(c.synced_at.as_deref())), w::regular(13.0), colors.muted);
                            });
                        });
                    });
                    let y = ui.cursor().top();
                    ui.painter().line_segment([pos2(ui.min_rect().left(), y), pos2(ui.max_rect().right(), y)], Stroke::new(1.0, colors.line));
                });
            }
        });
        if let Some(r) = &app.remote {
            ui.add_space(w::GROUP_GAP);
            ui.horizontal_wrapped(|ui| {
                let label = if r.syncing { format!("{} Atualizando…", icon::HOURGLASS) } else { format!("{} Atualizar contas", icon::ARROWS_CLOCKWISE) };
                let b = ui.add_enabled(!r.syncing, egui::Button::new(label).min_size(vec2(0.0, 36.0)));
                b.widget_info(|| WidgetInfo::labeled(WidgetType::Button, !r.syncing, if r.syncing { "Atualizando contas" } else { "Atualizar contas" }));
                refresh = b.on_hover_ui(sync_tip).clicked();
                if let Some((note, failed)) = &r.sync_note {
                    w::text(ui, note.as_str(), w::regular(13.0), if *failed { colors.red } else { colors.muted });
                }
            });
        }
        ui.add_space(w::GROUP_GAP);
        ui.separator();
        w::text(ui, "Como conectar", w::semibold(14.0), colors.text);
        w::caption(ui, "1. No Meu Pluggy (meu.pluggy.ai), conecte o banco ou cartão. 2. Aqui, use \"Conectar banco ou cartão\" e, na página que abrir, \"Autorizar Meu Pluggy\" escolhendo a conexão nova. 3. Use \"Atualizar contas\": cartões, contas e transações aparecem sozinhos. Cada pessoa conecta os próprios bancos, com o próprio acesso.");
        connect_button(app, ui);
    });
    if let Some((item, key)) = chosen {
        if let Some(c) = app.connections.iter_mut().find(|c| c.item_id == item) {
            if let Some(i) = key.as_ref().and_then(|k| app.institutions.iter().find(|i| &i.key == k)) {
                c.institution.clone_from(&i.name);
            }
            c.institution_key.clone_from(&key);
        }
        app.send(Cmd::Institution(item, key));
    }
    if refresh {
        app.refresh();
    }
    access_card(app, ui);
    update_card(app, ui);
}

// ───────────────────────── Acesso ─────────────────────────

/// Códigos fixos de pareamento do servidor: mande com o link de download para quem vai
/// usar o app num computador novo — a pessoa digita uma vez e nunca mais configura nada.
fn access_card(app: &mut App, ui: &mut Ui) {
    let Some(client) = app.remote.as_ref().and_then(|r| r.me.clone()) else { return };
    // Busca os códigos uma vez por sessão, quando o card aparece.
    if app.pair.codes.is_none() && app.pair.codes_error.is_none() && app.pair.codes_fetching.is_none()
        && let Some(token) = app.remote.as_ref().and_then(|r| r.token_for(&client))
    {
        app.pair.codes_fetching = Some(crate::backend::fetch_pairing_codes(&crate::backend::base_url(), &token, ui.ctx().clone()));
    }
    if let Some(rx) = app.pair.codes_fetching.take() {
        let mut loaded = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                crate::backend::CodesMsg::Loaded(codes) => loaded = Some(codes),
                crate::backend::CodesMsg::Failed(e) => {
                    app.pair.codes_error = Some(e);
                }
            }
        }
        match loaded {
            Some(codes) => app.pair.codes = Some(codes),
            // Ainda buscando: devolve o receiver para o próximo quadro.
            None if app.pair.codes_error.is_none() => app.pair.codes_fetching = Some(rx),
            None => {}
        }
    }
    let colors = w::palette(ui.ctx());
    let names = app.data.names.clone();
    let (codes, error) = (app.pair.codes.clone(), app.pair.codes_error.clone());
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Acesso");
        w::caption(ui, "Código fixo para conectar o app num computador novo: mande o link de download junto com o código de quem vai usar, e a pessoa digita uma vez só.");
        ui.add_space(w::GROUP_GAP);
        if let Some(codes) = codes {
            for (i, code) in codes.iter().enumerate() {
                match code {
                    Some(code) => {
                        ui.horizontal(|ui| {
                            w::text(ui, format!("{}: {}", names.get(i).map(String::as_str).unwrap_or("Pessoa"), code), w::semibold(14.0), colors.text);
                            if w::icon_button(ui, icon::COPY, "Copiar código").clicked() {
                                ui.ctx().copy_text(code.clone());
                            }
                        });
                    }
                    None => {
                        w::caption(ui, format!("{}: sem código no servidor (defina PAIRING_CODE_{} no backend/.env).", names.get(i).map(String::as_str).unwrap_or("Pessoa"), i + 1));
                    }
                }
            }
        } else if let Some(error) = error {
            w::text(ui, error, w::regular(13.0), colors.red);
        } else {
            w::caption(ui, format!("{} Buscando os códigos…", icon::HOURGLASS));
        }
    });
}

// ───────────────────────── Atualização ─────────────────────────

/// Versão atual, verificação, download e reinício. O worker só existe com backend;
/// na demonstração (e em qualquer estado anterior à 1ª verificação) o card fica quieto.
fn update_card(app: &mut App, ui: &mut Ui) {
    let colors = w::palette(ui.ctx());
    let (mut check, mut restart) = (false, false);
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Atualização");
        let current = env!("CARGO_PKG_VERSION");
        use crate::update::UpdateState as S;
        match &app.update.state {
            S::Idle | S::Checking => {
                w::caption(ui, format!("Versão {current}.{}", match app.update.state {
                    S::Checking => " Verificando atualizações…",
                    _ => "",
                }));
            }
            S::Checked => {
                w::caption(ui, format!("Versão {current}. Você está em dia."));
                check = ui.add(egui::Button::new(format!("{} Verificar novamente", icon::ARROWS_CLOCKWISE)).min_size(vec2(0.0, 36.0))).clicked();
            }
            S::Available { version } => {
                w::caption(ui, format!("Nova versão {version} disponível. Para atualizar, baixe o pacote novo em github.com/FelipeMayerDev/flounder/releases e substitua este."));
            }
            S::Downloading { version, frac } => {
                w::caption(ui, format!("Nova versão {version} — baixando…"));
                ui.add_space(w::GROUP_GAP);
                w::progress(ui, *frac, colors.blue, 8.0, None);
            }
            S::Ready { version, .. } => {
                w::caption(ui, format!("Nova versão {version} baixada. Reinicie para começar a usar."));
                restart = w::primary_button(ui, &format!("{} Reiniciar agora", icon::ARROWS_CLOCKWISE)).clicked();
            }
            S::Failed(e) => {
                w::text(ui, e.as_str(), w::regular(13.0), colors.red);
                check = ui.add(egui::Button::new("Tentar de novo").min_size(vec2(0.0, 36.0))).clicked();
            }
        }
    });
    if check {
        app.check_update();
    }
    if restart {
        app.restart_update();
    }
}

/// Card "Gastos compartilhados" em Ajustes: divisão pela renda e o interruptor do rateio.
fn shared_settings(app: &mut App, ui: &mut Ui) {
    let names = app.data.names.clone();
    let mut split_off = app.data.shared_split_off;
    w::card(ui, NORMAL, |ui| {
        w::title(ui, "Gastos compartilhados");
        ui.add_space(w::GROUP_GAP);
        let weights = app.data.income_weights(Month::of(today()));
        w::caption(ui, match weights {
            Some([wa, wb]) => format!(
                "Divisão pela renda registrada nos últimos 3 meses: {} paga {} e {} paga {} das contas compartilhadas.",
                names[0], w::pct(wa as f32 * 100.0), names[1], w::pct(wb as f32 * 100.0)
            ),
            None => "Sem renda registrada nos últimos 3 meses: as contas compartilhadas dividem metade para cada um.".to_string(),
        });
        w::caption(ui, "A renda vem dos lançamentos de Receita de vocês; quando um ganha mais, ele cobre mais da conta.");
        ui.add_space(w::GROUP_GAP);
        ui.checkbox(&mut split_off, "Remover renda compartilhada")
            .on_hover_text("Quando ligado, desconsidera tudo relacionado a gasto compartilhado: cada despesa vale por inteiro para quem pagou, sem rateio pela renda e sem acertos.");
        ui.add_space(w::GROUP_GAP);
        w::caption(ui, "O ajuste vale para os dois computadores.");
    });
    if split_off != app.data.shared_split_off {
        app.data.shared_split_off = split_off;
        app.send(Cmd::Settings(crate::api::Settings { shared_split_off: split_off }));
    }
}

/// "hoje às 14:30", "ontem às 09:10", "há 3 dias (07/11/2026 às 14:30)".
fn synced(iso: Option<&str>) -> String {
    let Some((d, hm)) = iso.and_then(local_time) else { return "ainda não atualizada".into() };
    match days_from_civil(today()) - days_from_civil(d) {
        0 => format!("atualizada hoje às {hm}"),
        1 => format!("atualizada ontem às {hm}"),
        n if n > 1 => format!("atualizada há {n} dias ({} às {hm})", d.fmt()),
        _ => format!("atualizada em {} às {hm}", d.fmt()),
    }
}

/// Abre a página /connect do backend já autenticada como quem usa este computador.
fn connect_button(app: &mut App, ui: &mut Ui) {
    let Some(r) = app.remote.as_mut() else { return };
    let Some(client) = r.me.clone() else { return };
    let clicked = ui.button(format!("{} Conectar banco ou cartão", icon::PLUS)).clicked();
    let Some(token) = r.token_for(&client) else {
        if clicked {
            r.failed = Some("Não há token de acesso deste membro neste computador.".into());
        }
        return;
    };
    if clicked && let Err(e) = crate::backend::open_connect(&crate::backend::base_url(), &token) {
        r.failed = Some(e);
    }
}
