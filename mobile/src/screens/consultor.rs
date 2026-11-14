//! Tela Consultor: as análises do GLM (sua + do casal) e a conversa, como no desktop.
//! O estado vive em `flounder::assistant::Agent` e o worker já traz as análises a cada
//! ciclo junto com o estado; a conversa vai por `Cmd::Chat` e "Analisar agora" por
//! `Cmd::Analyze` — os resultados voltam por `Msg::Reports` e `Msg::Chat`.

use egui::{Align, Color32, Frame, Layout, Vec2};
use egui_phosphor::regular as icon;
use flounder::assistant::Tab;
use flounder::backend;
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::theme;

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let (nome, outro) = (&app.data.names[app.eu], &app.data.names[1 - app.eu]);
    let tab = app.agent.tab;
    let ocupado = app.agent.waiting_analysis || app.agent.reports.as_ref().is_some_and(|r| r.busy);
    let subject = if tab == Tab::Mine { app.eu as u8 } else { 2 };
    let reports = app.agent.reports.clone();
    let chat = app.agent.chat.clone();
    let (waiting_chat, chat_erro) = (app.agent.waiting_chat, app.agent.chat_error.clone());

    // Ações coletadas nos closures, aplicadas fora (o Agent é mutável no app).
    let (mut analisar, mut escolher_tab, mut enviar_chat) = (false, None::<Tab>, false);

    // ─── Cabeçalho: abas + "Analisar agora" ────────────────
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Consultor").font(w::bold(16.0)).color(c.ink));
                ui.label(
                    egui::RichText::new(format!("Análises de {} e do casal", nome))
                        .font(w::regular(12.0))
                        .color(c.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let rotulo = if ocupado {
                    format!("{} gerando…", icon::HOURGLASS)
                } else {
                    format!("{} Analisar agora", icon::SPARKLE)
                };
                if ui
                    .add_sized(
                        [ui.available_width().min(150.0), 34.0],
                        egui::Button::new(
                            egui::RichText::new(rotulo)
                                .font(w::semibold(12.0))
                                .color(if ocupado { c.muted } else { c.ink }),
                        )
                        .fill(if ocupado { c.line } else { c.blue_soft })
                        .corner_radius(12),
                    )
                    .clicked()
                    && !ocupado
                {
                    analisar = true;
                }
            });
        });
        ui.add_space(8.0);
        let opcoes = [
            (Tab::Mine, format!("{} (você)", nome)),
            (Tab::Couple, format!("{} e {}", nome, outro)),
        ];
        ui.horizontal(|ui| {
            for (aba, rotulo) in &opcoes {
                let on = tab == *aba;
                if ui
                    .add_sized(
                        [ui.available_width() / 2.0 - 4.0, 32.0],
                        egui::Button::new(
                            egui::RichText::new(rotulo.clone())
                                .font(w::semibold(12.0))
                                .color(if on { c.ink } else { c.muted }),
                        )
                        .fill(if on { c.card } else { c.line })
                        .corner_radius(11),
                    )
                    .clicked()
                {
                    escolher_tab = Some(*aba);
                }
            }
        });
    });

    // ─── Análise do assunto escolhido ──────────────────────
    if let Some(reports) = &reports {
        if !reports.enabled {
            theme::card(ui, |ui| {
                w::caption(ui, "O assistente não está configurado no backend (defina ZAI_TOKEN e reinicie o serviço).");
            });
        } else {
            theme::card(ui, |ui| {
                let rotulo = if subject == 2 {
                    "Análise do casal"
                } else if subject == app.eu as u8 {
                    "Sua análise"
                } else {
                    "Análise do parceiro"
                };
                ui.label(egui::RichText::new(rotulo).font(w::semibold(14.0)).color(c.ink));
                ui.add_space(6.0);
                match reports.reports.iter().find(|r| r.subject == subject) {
                    Some(analise) => {
                        w::caption(ui, format!("Período de {} a {}", analise.period_start, analise.period_end));
                        ui.add_space(8.0);
                        for paragrafo in analise.content.split("\n\n") {
                            ui.label(egui::RichText::new(paragrafo).font(w::regular(14.0)).color(c.ink));
                            ui.add_space(8.0);
                        }
                    }
                    None if reports.busy || ocupado => {
                        w::caption(ui, format!("{} O consultor está nadando… a análise chega em instantes.", icon::HOURGLASS));
                    }
                    None => {
                        w::caption(ui, "Nenhuma análise para esta aba ainda. Use \"Analisar agora\".");
                    }
                }
            });
        }
    } else {
        theme::card(ui, |ui| {
            w::caption(ui, format!("{} Buscando as análises…", icon::HOURGLASS));
        });
    }

    // ─── Conversa com o consultor ──────────────────────────
    theme::card(ui, |ui| {
        ui.label(egui::RichText::new("Conversa").font(w::bold(16.0)).color(c.ink));
        ui.add_space(6.0);
        if chat.is_empty() {
            w::caption(ui, "Pergunte o que quiser: seus gastos, os do casal, ou o que melhorar.");
            ui.add_space(8.0);
        }
        for msg in &chat {
            let minha = msg.role == "user";
            let alinhamento = if minha { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
            ui.with_layout(alinhamento, |ui| {
                Frame::new()
                    .fill(if minha { c.blue_soft } else { c.line })
                    .corner_radius(14)
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_max_width(320.0);
                        ui.label(egui::RichText::new(&msg.content).font(w::regular(13.5)).color(c.ink));
                    });
            });
            ui.add_space(6.0);
        }
        if waiting_chat {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                w::caption(ui, format!("{} consultando o GLM…", icon::HOURGLASS));
            });
        }
        if let Some(erro) = &chat_erro {
            ui.label(egui::RichText::new(erro.clone()).font(w::regular(12.0)).color(c.red));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let largura_campo = ui.available_width() - 46.0;
            let mut enviar = ui
                .add_sized(
                    [largura_campo, 40.0],
                    egui::TextEdit::singleline(&mut app.agent.input)
                        .hint_text(egui::RichText::new("Pergunte qualquer coisa…").color(c.faint))
                        .font(w::regular(14.0)),
                )
                .lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add_sized(
                    [40.0, 40.0],
                    egui::Button::new(
                        egui::RichText::new(icon::PAPER_PLANE_RIGHT).font(w::regular(16.0)).color(Color32::WHITE),
                    )
                    .fill(c.blue)
                    .corner_radius(12),
                )
                .clicked()
            {
                enviar = true;
            }
            if enviar && !app.agent.input.trim().is_empty() {
                let texto = app.agent.input.trim().to_owned();
                app.agent.chat.push(flounder::api::ChatMsg { role: "user".into(), content: texto });
                app.agent.input.clear();
                app.agent.waiting_chat = true;
                app.enviar(backend::Cmd::Chat(app.agent.chat.clone()));
            }
        });
    });

    // ─── Ações fora dos closures ───────────────────────────
    if analisar {
        app.enviar(backend::Cmd::Analyze);
        app.agent.waiting_analysis = true;
    }
    if let Some(aba) = escolher_tab {
        app.agent.tab = aba;
    }
}
