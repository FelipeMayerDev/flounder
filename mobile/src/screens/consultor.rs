//! Tela Consultor: as análises do GLM (sua + do casal) e a conversa, como no desktop.
//! O estado vive em `flounder::assistant::Agent` e o worker já traz as análises a cada
//! ciclo junto com o estado; a conversa vai por `Cmd::Chat` e "Analisar agora" por
//! `Cmd::Analyze` — os resultados voltam por `Msg::Reports` e `Msg::Chat`.

use egui::{Align, Color32, Frame, Layout};
use egui_phosphor::regular as icon;
use flounder::assistant::Tab;
use flounder::backend;
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::theme;

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    if app.demo {
        theme::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new("Consultor").font(w::bold(16.0)).color(c.ink));
            w::caption(ui, "Conecte sua conta para conversar sobre seus gastos e gerar análises.");
            ui.add_space(10.0);
            if w::primary_button(ui, "Conectar conta").clicked() {
                app.demo = false;
            }
        });
        return;
    }
    let (nome, outro) = (nome_membro(app, app.eu), nome_membro(app, 1 - app.eu));
    let tab = app.agent.tab;
    let ocupado = app.agent.waiting_analysis || app.agent.reports.as_ref().is_some_and(|r| r.busy);
    let subject = if tab == Tab::Mine { app.eu as u8 } else { 2 };
    let reports = app.agent.reports.clone();
    let chat = app.agent.chat.clone();
    let (waiting_chat, chat_erro) = (app.agent.waiting_chat, app.agent.chat_error.clone());

    // Primeira abertura da aba: pede o histórico persistido da conversa (uma vez só;
    // o agente só enche o chat se o local está vazio, para não pisar o que já foi dito).
    if !app.historico_pedido {
        app.historico_pedido = true;
        app.consultar(backend::Cmd::History);
    }

    // Ações coletadas nos closures, aplicadas fora (o Agent é mutável no app).
    let (mut analisar, mut escolher_tab) = (false, None::<Tab>);

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
            let largura = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            for (aba, rotulo) in &opcoes {
                let on = tab == *aba;
                if ui
                    .add_sized(
                        [largura, 36.0],
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
                    "Análise do casal".to_owned()
                } else if subject == app.eu as u8 {
                    "Sua análise".to_owned()
                } else {
                    // Rótulo com o nome de quem é a análise ("Análise de Larissa") em vez de "do parceiro".
                    format!("Análise de {}", nome_membro(app, subject as usize))
                };
                ui.label(egui::RichText::new(rotulo).font(w::semibold(14.0)).color(c.ink));
                ui.add_space(6.0);
                match reports.reports.iter().find(|r| r.subject == subject) {
                    Some(analise) => {
                        w::caption(ui, format!("Período de {} a {}", dia_mes(&analise.period_start), dia_mes(&analise.period_end)));
                        ui.add_space(8.0);
                        // Análise longa rola dentro do cartão (até ~300 px) em vez de empurrar a conversa.
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, true])
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for paragrafo in analise.content.split("\n\n") {
                                    ui.label(egui::RichText::new(paragrafo).font(w::regular(14.0)).color(c.ink));
                                    ui.add_space(8.0);
                                }
                            });
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
                        // Bolha com no máximo ~78% da largura disponível (nunca a tela inteira).
                        ui.set_max_width((ui.available_width() * 0.78).max(180.0));
                        ui.label(egui::RichText::new(&msg.content).font(w::regular(13.5)).color(c.ink));
                    });
            });
            // Selos das operações que o consultor executou ao responder ("criou despesa
            // Aluguel R$ 1.200 em 15/11"): etiquetas prontas do backend, uma por linha.
            for op in &msg.ops {
                Frame::new()
                    .fill(c.green_soft)
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_max_width((ui.available_width() * 0.78).max(180.0));
                        ui.label(
                            egui::RichText::new(format!("{} {}", icon::CHECK_CIRCLE, op.label))
                                .font(w::regular(12.0))
                                .color(c.green),
                        );
                    });
                ui.add_space(2.0);
            }
            ui.add_space(4.0);
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
            // Campo + lixeira + botão somem exatamente a largura da linha: alinhamento perfeito.
            let largura_botao = 40.0;
            let largura_lixeira = 34.0;
            let espaco = ui.spacing().item_spacing.x;
            let largura_campo = ui.available_width() - largura_botao - largura_lixeira - 2.0 * espaco;
            let mut limpar = false;
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
                    [largura_lixeira, 40.0],
                    egui::Button::new(egui::RichText::new(icon::TRASH).font(w::regular(14.0)).color(c.muted))
                        .fill(c.line)
                        .corner_radius(12),
                )
                .clicked()
            {
                limpar = true;
            }
            if ui
                .add_sized(
                    [largura_botao, 40.0],
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
            if limpar {
                // Local e backend: zera a conversa aqui e apaga o histórico persistido.
                app.agent.chat.clear();
                app.agent.chat_error = None;
                app.enviar(backend::Cmd::ClearHistory);
            }
            if enviar && !app.agent.waiting_chat && !app.agent.input.trim().is_empty() {
                let texto = app.agent.input.trim().to_owned();
                app.agent.chat.push(flounder::api::ChatMsg { role: "user".into(), content: texto, ops: Vec::new() });
                app.agent.input.clear();
                app.agent.waiting_chat = true;
                app.agent.chat_error = None;
                app.consultar(backend::Cmd::Chat(app.agent.chat.clone()));
            }
        });
    });

    // ─── Ações fora dos closures ───────────────────────────
    if analisar {
        app.consultar(backend::Cmd::Analyze);
        app.agent.waiting_analysis = true;
    }
    if let Some(aba) = escolher_tab {
        app.agent.tab = aba;
    }
}

/// Nome do membro pela posição: dados reais do backend, com fallback para o mock
/// na demonstração (o `data` vazio deixaria "Análises de  e do casal").
fn nome_membro(app: &MobileApp, pos: usize) -> String {
    let nome = app.data.names.get(pos).map(|s| s.trim()).unwrap_or("");
    if nome.is_empty() {
        crate::mock::MEMBROS.get(pos).map(|(n, _)| *n).unwrap_or("Parceiro").to_owned()
    } else {
        nome.to_owned()
    }
}

/// "2026-08-01" → "01/08" (ISO cru é feio no celular); fora do formato, devolve o texto como veio.
fn dia_mes(iso: &str) -> String {
    let iso = iso.trim();
    let ano: i32 = iso.get(..4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mes: u32 = iso.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(0);
    let dia: u32 = iso.get(8..10).and_then(|s| s.parse().ok()).unwrap_or(0);
    if ano > 0 && (1..=12).contains(&mes) && (1..=31).contains(&dia) {
        format!("{dia:0>2}/{mes:0>2}")
    } else {
        iso.to_owned()
    }
}
