//! Tela Casa: contas da casa em meio a meio — filtro, resumo e lista com bolinha de pago.

use egui::{Align, Align2, Color32, Frame, Layout, Margin, Sense, Stroke, Ui, Vec2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{Filtro, MobileApp};
use crate::mock::MEMBROS;
use crate::theme;

pub fn show(ui: &mut Ui, app: &mut MobileApp) {
    // Resumo recalculado pelas contas atuais (clique na bolinha mantém o mock coerente).
    let qtd_pend = app.dados.casa.bills.iter().filter(|b| !b.paga).count() as u8;
    let total_pend: i64 = app.dados.casa.bills.iter().filter(|b| !b.paga).map(|b| b.total_cents).sum();
    let qtd_pagas = app.dados.casa.bills.iter().filter(|b| b.paga).count() as u8;
    let total_pagas: i64 = app.dados.casa.bills.iter().filter(|b| b.paga).map(|b| b.total_cents).sum();
    app.dados.casa.pendentes_qtd = qtd_pend;
    app.dados.casa.pendentes_total_cents = total_pend;

    let c = theme::cores();

    theme::card(ui, |ui| {
        // Cabeçalho: título + ⓘ e subtítulo com o total de contas.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Contas da casa").font(w::bold(16.0)).color(c.ink));
            ui.add_space(5.0);
            ui.label(egui::RichText::new(icon::INFO).font(w::regular(13.0)).color(c.faint));
        });
        ui.label(
            egui::RichText::new(format!("{} contas · meio a meio", app.dados.casa.total_contas))
                .font(w::regular(13.0))
                .color(c.muted),
        );
        ui.add_space(12.0);

        segmentado(ui, app);
        ui.add_space(10.0);
        botao_editar(ui);
        ui.add_space(12.0);

        // Resumo da seção: muda com o filtro (Pagas mostra o total pago).
        let (titulo, nota, total) = if app.filtro_casa == Filtro::Pagas {
            ("Pagas", format!("{qtd_pagas} pagas · "), total_pagas)
        } else {
            ("A pagar", format!("{qtd_pend} pendentes · "), total_pend)
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(titulo).font(w::semibold(15.0)).color(c.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                theme::money(ui, total, 13.0, c.ink);
                ui.label(egui::RichText::new(nota).font(w::regular(13.0)).color(c.muted));
            });
        });
        ui.add_space(8.0);

        // Lista filtrada pelo segmento escolhido.
        let indices: Vec<usize> = app
            .dados
            .casa
            .bills
            .iter()
            .enumerate()
            .filter(|(_, b)| match app.filtro_casa {
                Filtro::Todas => true,
                Filtro::Pendentes => !b.paga,
                Filtro::Pagas => b.paga,
            })
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            match app.filtro_casa {
                Filtro::Pagas => theme::vazio(ui, "✅", "Nenhuma conta paga ainda neste mês."),
                Filtro::Pendentes => theme::vazio(ui, "🎉", "Nada pendente — todas as contas estão pagas."),
                Filtro::Todas => theme::vazio(ui, "🏠", "Nenhuma conta da casa neste mês."),
            };
            return;
        }
        let ultima = indices.len() - 1;
        for (n, &idx) in indices.iter().enumerate() {
            linha_conta(ui, app, idx);
            if n != ultima {
                divisoria(ui);
            }
        }
    });
}

/// Controle segmentado Todas | Pendentes | Pagas: fundo cinza, pílula branca no ativo.
fn segmentado(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    Frame::new()
        .fill(c.line)
        .corner_radius(13)
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let largura = ui.available_width() / 3.0;
                for (filtro, rotulo) in [
                    (Filtro::Todas, "Todas"),
                    (Filtro::Pendentes, "Pendentes"),
                    (Filtro::Pagas, "Pagas"),
                ] {
                    let on = app.filtro_casa == filtro;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(largura, 32.0), Sense::click());
                    if on {
                        // Pílula branca com sombrinha discreta.
                        ui.painter().rect_filled(rect.translate(Vec2::new(0.0, 1.0)), 11.0, Color32::from_rgba_unmultiplied(23, 43, 99, 30));
                        ui.painter().rect_filled(rect, 11.0, Color32::WHITE);
                    }
                    let cor = if on { c.ink } else { c.muted };
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, rotulo, w::semibold(13.0), cor);
                    if resp.clicked() {
                        app.filtro_casa = filtro;
                    }
                }
            });
        });
}

/// Botão "Editar lista": pílula branca com borda fina (estático na fase 1).
fn botao_editar(ui: &mut Ui) {
    let c = theme::cores();
    let resp = Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, c.line))
        .corner_radius(255)
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{}  Editar lista", icon::PENCIL_SIMPLE))
                    .font(w::semibold(13.0))
                    .color(c.ink),
            );
        })
        .response;
    let _ = resp; // clique sem ação por enquanto
}

/// Uma conta da casa: bolinha de pago, nome + valor, pagador/dia e a sua parte.
fn linha_conta(ui: &mut Ui, app: &mut MobileApp, idx: usize) {
    let c = theme::cores();
    // Copia os campos para soltar o empréstimo antes do possível clique.
    let (nome, dia, total, aprox, pagador, parte, paga) = {
        let b = &app.dados.casa.bills[idx];
        (b.nome.clone(), b.dia, b.total_cents, b.aproximado, b.pagador, b.parte_cents, b.paga)
    };
    let (nome_membro, cor_pagador) = MEMBROS[pagador];
    // Regra do "você": quem usa o celular vê o próprio nome como "Você".
    let nome_pagador = if pagador == app.eu { "Você" } else { nome_membro };
    let inicial = nome_membro.chars().next().unwrap();

    ui.horizontal(|ui| {
        // Bolinha de pago: clique alterna e o resumo recalcula no próximo frame.
        let resp = theme::bolinha_check(ui, paga, c.green);
        if resp.clicked() {
            if app.demo {
                app.dados.casa.bills[idx].paga = !app.dados.casa.bills[idx].paga;
            } else {
                // Real: inverte a entrada e grava via worker (mesmo fluxo do desktop).
                let id = app.dados.casa.bills[idx].id;
                let month = flounder::model::Month { y: app.y, m: app.m as u32 };
                app.data.toggle(flounder::model::BillRef::Entry(id), month);
                if let Some(e) = app.data.entry(id) {
                    app.enviar(flounder::backend::Cmd::Entry(e.clone()));
                }
                app.atualizar_mes();
            }
        }
        ui.add_space(6.0);
        ui.vertical(|ui| {
            // Linha 1: nome (mudo quando paga) e valor (≈ antes se aproximado).
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&nome)
                        .font(w::semibold(14.0))
                        .color(if paga { c.muted } else { c.ink }),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::money(ui, total, 14.0, c.ink);
                    if aprox {
                        ui.label(egui::RichText::new("≈").font(w::semibold(13.0)).color(c.muted));
                    }
                });
            });
            ui.add_space(3.0);
            // Linha 2: dot do pagador, "quem · dia" e a sua parte à direita.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                theme::avatar(ui, rect.center(), 9.0, &inicial.to_string(), cor_pagador);
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("{nome_pagador} · dia {dia}"))
                        .font(w::regular(12.0))
                        .color(c.muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::money(ui, parte, 12.0, c.muted);
                    ui.label(egui::RichText::new("sua parte").font(w::regular(12.0)).color(c.faint));
                });
            });
        });
    });
}

/// Divisória fininha entre contas da lista.
fn divisoria(ui: &mut Ui) {
    let c = theme::cores();
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, c.line);
    ui.add_space(4.0);
}
