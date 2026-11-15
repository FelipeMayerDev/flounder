//! Agente flutuante: a bolha animada com o peixe, as análises (a sua e a do casal — a do
//! parceiro nunca sai do backend) e a conversa livre com o GLM. As respostas chegam pela
//! thread do backend (`Msg::Reports`/`Msg::Chat`); aqui é só interface e animação.

use crate::app::App;
use crate::backend::Cmd;
use crate::model::local_time;
use crate::widgets::{self as w};
use egui::{pos2, vec2, Align, Align2, CornerRadius, Id, Layout, Sense, Stroke, Ui, Vec2};
use egui_phosphor::regular as icon;
use std::time::{Duration, Instant};

/// Velocidade da revelação do texto da análise, em caracteres por segundo.
const REVEAL_SPEED: f32 = 320.0;
const MAX_HISTORY: usize = 12;
/// Circunferência da bolha e da janela.
const BUBBLE: f32 = 64.0;

/// Aba da análise: a de quem usa o app ou a do casal.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Mine,
    Couple,
}

#[derive(Default)]
pub struct Agent {
    /// Janela aberta (a bolha fica sempre à vista).
    pub open: bool,
    pub tab: Tab,
    pub reports: Option<crate::api::AssistantReports>,
    pub chat: Vec<crate::api::ChatMsg>,
    pub input: String,
    pub waiting_chat: bool,
    pub waiting_analysis: bool,
    pub chat_error: Option<String>,
    /// Revelação do texto: (aba, conteúdo) que está sendo revelado e o instante do início.
    reveal: Option<(Tab, String, Instant)>,
    /// Análise nova chegou com a janela fechada: ponto piscando na bolha.
    unseen: bool,
}

impl Agent {
    pub fn on_reports(&mut self, r: crate::api::AssistantReports) {
        let size = |reports: &[crate::api::AssistantReport]| reports.iter().map(|x| x.content.len()).sum::<usize>();
        let changed = self.reports.as_ref().map(|old| size(&old.reports)) != Some(size(&r.reports));
        if !r.busy {
            self.waiting_analysis = false;
        }
        self.reports = Some(r);
        if changed {
            self.reveal = None;
            if !self.open {
                self.unseen = true;
            }
        }
    }

    pub fn on_chat(&mut self, result: Result<crate::api::ChatReply, String>) {
        self.waiting_chat = false;
        match result {
            Ok(reply) => self.chat.push(crate::api::ChatMsg {
                role: "assistant".into(),
                content: reply.content,
                ops: reply.ops,
            }),
            Err(error) => self.chat_error = Some(error),
        }
    }

    /// Histórico persistido chegou do backend: enche a conversa só se a local está vazia
    /// (não pisca no que o usuário já mandou nesta sessão).
    pub fn on_history(&mut self, history: Result<crate::api::ChatHistory, String>) {
        match history {
            Ok(h) if self.chat.is_empty() => self.chat = h.messages,
            Ok(_) => {}
            Err(error) => self.chat_error = Some(error),
        }
    }
}

/// Bolha + janela, chamado uma vez por quadro, depois dos modais.
pub fn show(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    bubble(app, &ctx);
    if app.agent.open {
        // Janela aberta com conversa vazia: pede o histórico uma vez por abertura.
        if app.agent.chat.is_empty() && !app.history_requested {
            app.history_requested = true;
            app.ask(Cmd::History);
        }
        window(app, &ctx);
    } else {
        // Fechou: a próxima abertura com conversa vazia pede o histórico de novo.
        app.history_requested = false;
    }
}

// ───────────────────────── bolha ─────────────────────────

fn bubble(app: &mut App, ctx: &egui::Context) {
    let moving = w::motion(ctx);
    let t = ctx.input(|i| i.time) as f32;
    // Nado suave: sobe e desce e respira; com movimento reduzido, tudo parado.
    let (bob, breath, pulse) = if moving {
        ((t * 1.8).sin() * 3.0, (t * 1.1).sin() * 1.5, ((t * 1.3).sin() + 1.0) / 2.0)
    } else {
        (0.0, 0.0, 0.5)
    };
    let colors = w::palette(ctx);
    let resp = egui::Area::new(Id::new("flounder_bubble"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -18.0 - bob))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_min_size(Vec2::splat(BUBBLE));
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(BUBBLE), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                if app.agent.open { "Flounder aberto — clique na bolha para fechar" } else { "Flounder, consultor financeiro — clique para conversar" },
            ));
            let p = ui.painter();
            let hover = ctx.animate_bool_with_time(r.id.with("h"), r.hovered(), 0.18);
            // Brilho pulsante atrás do peixe.
            let glow = colors.blue.gamma_multiply(0.16 + pulse * 0.14);
            p.circle_filled(rect.center(), 26.0 + pulse * 3.0 + hover * 2.0, glow);
            p.circle_filled(rect.center(), 23.0, colors.card);
            let fish = 40.0 + breath + hover * 2.0;
            w::paint_art(ui, w::Art::Fish, rect.center(), fish);
            if app.agent.unseen && !app.agent.open {
                let dot = pos2(rect.right() - 8.0, rect.top() + 8.0);
                p.circle_filled(dot, 7.0 + pulse * 1.5, colors.coral);
                p.circle_filled(dot, 3.5, colors.card);
            }
            if r.clicked() {
                app.agent.open = !app.agent.open;
                app.agent.unseen = false;
            }
            r.on_hover_cursor(egui::CursorIcon::PointingHand)
        }).inner;
    if resp.hovered() || app.agent.unseen {
        ctx.request_repaint_after(Duration::from_millis(60));
    }
}

// ───────────────────────── janela ─────────────────────────

fn window(app: &mut App, ctx: &egui::Context) {
    let colors = w::palette(ctx);
    let names = app.data.names.clone();
    let me = app.data.me.idx();
    let mut close = false;
    let mut analyze = false;
    let mut send = false;
    let mut switched = false;
    egui::Window::new(" ")
        .id(Id::new("flounder_agent"))
        .title_bar(false)
        .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -BUBBLE - 30.0))
        .order(egui::Order::Foreground)
        .default_size(vec2(480.0, 580.0))
        .min_width(420.0)
        .frame(egui::Frame::new().fill(colors.card).corner_radius(16).inner_margin(20).shadow(egui::epaint::Shadow { offset: [0, 12], blur: 32, spread: 0, color: egui::Color32::from_black_alpha(70) }))
        .show(ctx, |ui| {
            let t = ctx.input(|i| i.time) as f32;
            let (bob, breath) = if w::motion(ctx) { ((t * 1.6).sin() * 2.5, (t * 1.1).sin() * 1.5) } else { (0.0, 0.0) };
            ui.horizontal(|ui| {
                ui.allocate_ui(vec2(46.0, 46.0), |ui| {
                    let center = ui.min_rect().left_top() + vec2(23.0, 23.0 + bob);
                    w::paint_art(ui, w::Art::Fish, center, 42.0 + breath);
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    w::text(ui, "Flounder", w::bold(16.0), colors.text);
                    w::caption(ui, format!("Consultor de {} e do casal", names[me]));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::icon_button(ui, icon::X, "Fechar o Flounder").clicked() {
                        close = true;
                    }
                    let enabled = app.agent.reports.as_ref().is_some_and(|r| r.enabled);
                    if enabled {
                        w::chip(ui, "GLM · online", colors.blue, colors.blue_soft);
                    } else {
                        w::chip(ui, "desligado", colors.muted, colors.faint);
                    }
                    ui.add_space(8.0);
                });
            });
            ui.add_space(w::GROUP_GAP);
            let mut tab = app.agent.tab;
            let labels = [(Tab::Mine, format!("{} (você)", names[me])), (Tab::Couple, "Casal".to_string())];
            let opts: Vec<(Tab, &str)> = labels.iter().map(|(t, s)| (*t, s.as_str())).collect();
            if w::segmented(ui, &mut tab, &opts) {
                switched = true;
            }
            app.agent.tab = tab;
            ui.add_space(w::GROUP_GAP);
            let Some(reports) = &app.agent.reports else {
                w::empty_note(ui, "Ainda não conversou com o backend: as análises chegam em instantes.");
                return;
            };
            if !reports.enabled {
                w::empty_note(ui, "O assistente está desligado no backend.");
                w::caption(ui, "Defina ZAI_TOKEN no backend/.env (ou flounder.env do servidor) e reinicie o serviço.");
                return;
            }
            // Análise da aba, com revelação progressiva (efeito de digitando).
            let subject = if tab == Tab::Mine { me as u8 } else { 2 };
            let report = reports.reports.iter().find(|r| r.subject == subject);
            let busy = reports.busy || app.agent.waiting_analysis;
            w::card(ui, w::Dens { pad: 16.0, gap: 8.0 }, |ui| {
                match report {
                    Some(report) => {
                        ui.horizontal(|ui| {
                            w::text(ui, icon::SPARKLE, w::regular(15.0), colors.blue);
                            w::text(ui, if tab == Tab::Mine { "Sua análise" } else { "Análise do casal" }, w::semibold(14.0), colors.text);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if let Some((d, hm)) = local_time(&report.generated_at) {
                                    w::caption(ui, format!("{} às {}", d.short(), hm));
                                }
                            });
                        });
                        if switched {
                            app.agent.reveal = None;
                        }
                        let content = report.content.clone();
                        let shown = reveal_chars(ctx, &mut app.agent.reveal, tab, &content);
                        let text: String = content.chars().take(shown).collect();
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for line in text.lines() {
                            ui.label(egui::RichText::new(line).font(w::regular(13.0)).color(colors.text));
                        }
                        if shown < content.chars().count() {
                            ctx.request_repaint_after(Duration::from_millis(50));
                        }
                        w::caption(ui, format!("Período de {} a {}", short_month(&report.period_start), short_month(&report.period_end)));
                    }
                    None if busy => {
                        w::caption(ui, "O Flounder está analisando os gastos… nada de nadar sem olhar o mapa primeiro.");
                    }
                    None => {
                        w::empty_note(ui, "Ainda sem análise por aqui.");
                        w::caption(ui, "Use \"Analisar agora\": os gastos dos últimos meses viram um resumo divertido.");
                    }
                }
            });
            ui.add_space(w::GROUP_GAP);
            ui.horizontal(|ui| {
                let b = ui.add_enabled(!busy, egui::Button::new(format!("{} {}", if busy { icon::HOURGLASS } else { icon::SPARKLE }, if busy { "Analisando…" } else { "Analisar agora" })).min_size(vec2(0.0, 32.0)));
                if b.clicked() {
                    analyze = true;
                }
                if tab == Tab::Couple {
                    w::caption(ui, "Os dois veem esta.");
                } else {
                    w::caption(ui, "Só você vê esta. A do parceiro é dele.");
                }
            });
            ui.add_space(w::GROUP_GAP);
            ui.separator();
            chat(app, ui, &mut send);
        });
    if switched {
        app.agent.reveal = None;
    }
    if analyze {
        app.agent.waiting_analysis = true;
        app.ask(Cmd::Analyze);
    }
    if send {
        send_chat(app);
    }
    if close {
        app.agent.open = false;
        app.agent.unseen = false;
    }
}

/// "nov/26" para o período da análise.
fn short_month(iso: &str) -> String {
    const NAMES: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    let m: usize = iso.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 12);
    format!("{}/{:0>2}", NAMES[m - 1], iso.get(2..4).unwrap_or(""))
}

/// Quantos caracteres já aparecem da análise (animação de digitação, sem retrabalho).
fn reveal_chars(ctx: &egui::Context, reveal: &mut Option<(Tab, String, Instant)>, tab: Tab, content: &str) -> usize {
    let total = content.chars().count();
    if !w::motion(ctx) {
        return total;
    }
    let (key, start) = match reveal {
        Some((t, text, start)) if *t == tab && text == content => (true, *start),
        _ => (false, Instant::now()),
    };
    if !key {
        *reveal = Some((tab, content.to_string(), start));
    }
    let shown = (start.elapsed().as_secs_f32() * REVEAL_SPEED) as usize;
    shown.min(total)
}

// ───────────────────────── conversa ─────────────────────────

fn chat(app: &mut App, ui: &mut Ui, send: &mut bool) {
    let colors = w::palette(ui.ctx());
    w::text(ui, "Conversa", w::semibold(14.0), colors.text);
    w::caption(ui, "Pergunte o que quiser: seus gastos, os do casal, ou o que melhorar.");
    ui.add_space(w::GROUP_GAP);
    let height = (ui.available_height() - 44.0).max(120.0);
    egui::ScrollArea::vertical().auto_shrink(false).max_height(height).stick_to_bottom(true).show(ui, |ui| {
        if app.agent.chat.is_empty() {
            bubble_line(ui, &colors, false, "Oi! Sou o Flounder 🐠 Já li os lançamentos de vocês. Me pergunte qualquer coisa — do saldo do mês àquela conta que sempre esquece.");
            ui.add_space(w::GROUP_GAP);
        }
        for msg in &app.agent.chat {
            let mine = msg.role == "user";
            bubble_line(ui, &colors, mine, &msg.content);
            if !msg.ops.is_empty() {
                ops_line(ui, &colors, &msg.ops);
            }
            ui.add_space(w::GROUP_GAP);
        }
        if app.agent.waiting_chat {
            thinking(ui, &colors);
        }
        if let Some(error) = &app.agent.chat_error {
            ui.label(egui::RichText::new(error.clone()).font(w::regular(12.0)).color(colors.red));
        }
    });
    ui.add_space(w::GROUP_GAP);
    ui.horizontal(|ui| {
        let row = ui.available_width();
        let input = egui::Frame::new()
            .fill(colors.faint)
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(10, 7))
            .stroke(Stroke::new(1.0, colors.line))
            .show(ui, |ui| {
                ui.add(w::text_input(&mut app.agent.input, "Pergunte qualquer coisa…").frame(egui::Frame::new()).desired_width(row - 124.0))
            }).inner;
        if w::icon_button(ui, icon::ERASER, "Limpar conversa").clicked() {
            app.agent.chat.clear();
            app.agent.chat_error = None;
            app.send(Cmd::ClearHistory);
        }
        let send_btn = egui::Button::new(egui::RichText::new(icon::PAPER_PLANE_RIGHT).font(w::semibold(15.0)).color(colors.bg))
            .fill(colors.blue).corner_radius(CornerRadius::same(10)).min_size(vec2(36.0, 36.0));
        let pressed = ui.add(send_btn).clicked() || input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if pressed && !app.agent.input.trim().is_empty() && !app.agent.waiting_chat {
            *send = true;
        }
        if pressed && app.agent.input.trim().is_empty() {
            input.request_focus();
        }
    });
}

/// Um balão de conversa: o do usuário alinhado à direita, o do peixe à esquerda com avatar.
fn bubble_line(ui: &mut Ui, colors: &w::Palette, mine: bool, text: &str) {
    let row = ui.available_width();
    ui.with_layout(if mine { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) }, |ui| {
        if !mine {
            w::art(ui, w::Art::Fish, 26.0);
            ui.add_space(2.0);
        }
        let (fill, corner) = if mine {
            (colors.blue_soft, CornerRadius { nw: 14, ne: 4, sw: 14, se: 14 })
        } else {
            (colors.faint, CornerRadius { nw: 4, ne: 14, sw: 14, se: 14 })
        };
        egui::Frame::new().fill(fill).corner_radius(corner).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_max_width((row * 0.72).max(180.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(text).font(w::regular(13.0)).color(colors.text));
        });
    });
}

/// Os selos das ações que o assistente executou no app, sob a bolha da resposta, alinhados
/// com o texto dela (o mesmo recuo do avatar).
fn ops_line(ui: &mut Ui, colors: &w::Palette, ops: &[crate::api::Op]) {
    ui.with_layout(Layout::left_to_right(Align::Min).with_main_wrap(true), |ui| {
        ui.add_space(28.0);
        for op in ops {
            w::chip(ui, &format!("{} {}", icon::CHECK_CIRCLE, op.label), colors.green, colors.green_soft);
            ui.add_space(4.0);
        }
    });
}

/// Três pontinhos animados enquanto o GLM pensa.
fn thinking(ui: &mut Ui, colors: &w::Palette) {
    let ctx = ui.ctx().clone();
    let t = ctx.input(|i| i.time) as f32;
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        w::art(ui, w::Art::Fish, 28.0);
        egui::Frame::new().fill(colors.faint).corner_radius(CornerRadius::same(12)).inner_margin(10).show(ui, |ui| {
            ui.horizontal(|ui| {
                for k in 0..3 {
                    let bounce = ((t * 3.0 + k as f32 * 0.5).sin().max(0.0)) * 3.0;
                    let (rect, _) = ui.allocate_exact_size(vec2(6.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(pos2(rect.center().x, rect.center().y - bounce), 3.0, colors.muted);
                }
            });
        });
        ctx.request_repaint_after(Duration::from_millis(50));
    });
}

fn send_chat(app: &mut App) {
    let question = app.agent.input.trim().to_string();
    app.agent.chat.push(crate::api::ChatMsg { role: "user".into(), content: question, ops: Vec::new() });
    if app.agent.chat.len() > MAX_HISTORY {
        app.agent.chat.drain(..app.agent.chat.len() - MAX_HISTORY);
    }
    app.ask(Cmd::Chat(app.agent.chat.clone()));
    app.agent.input.clear();
    app.agent.waiting_chat = true;
    app.agent.chat_error = None;
}
