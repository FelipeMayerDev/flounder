//! Tela Fluxo: resumo do caixa do mês e os lançamentos dia a dia (toque alterna pago).

use egui::{Align, Align2, Color32, CornerRadius, Layout, Sense, Vec2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::mock::{DiaFluxo, MEMBROS};
use crate::theme;

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    // Dias sempre em ordem crescente (o FAB já ordena, mas por garantia).
    app.mock.fluxo.sort_by_key(|d| d.dia);

    // Mês aberto sem nenhum lançamento.
    if app.mock.fluxo.is_empty() {
        theme::vazio(ui, "🐟", "Nenhum lançamento neste mês.");
        return;
    }

    resumo(ui, app);

    // Toques acumulados no desenho e aplicados no fim (mock por &mut).
    let mut alternar: Vec<(usize, usize)> = Vec::new();
    dias(ui, app, &mut alternar);
    for (i, j) in alternar {
        let e = &mut app.mock.fluxo[i].entradas[j];
        e.paid = !e.paid;
    }
}

/// Cartão-resumo "Fluxo de caixa" do mês: entradas, saídas e saldo.
fn resumo(ui: &mut egui::Ui, app: &MobileApp) {
    let c = theme::cores();
    let (entradas, saidas) = totais(&app.mock.fluxo);
    let saldo = entradas - saidas;
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            // Selo do tema, igual aos badges das linhas de baixo.
            let (brect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
            ui.painter().rect_filled(brect, CornerRadius::same(10), c.blue_soft);
            ui.painter().text(brect.center(), Align2::CENTER_CENTER, icon::TREND_UP.to_string(), w::semibold(13.0), c.blue);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Fluxo de {}", app.mes_label())).font(w::semibold(15.0)).color(c.ink));
        });
        ui.add_space(12.0);
        ui.columns(3, |cols| {
            coluna(&mut cols[0], "Entradas", entradas, c.green);
            coluna(&mut cols[1], "Saídas", saidas, c.red);
            coluna(&mut cols[2], "Saldo", saldo, c.blue);
        });
    });
}

/// Coluna do resumo: rótulo pequeno + valor colorido.
fn coluna(ui: &mut egui::Ui, rotulo: &str, cents: i64, cor: Color32) {
    let c = theme::cores();
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(rotulo).font(w::semibold(11.0)).color(c.muted));
        ui.add_space(3.0);
        theme::money(ui, cents, 15.0, cor);
    });
}

/// (entradas, saídas) do mês, somando positivo/negativo do fluxo.
fn totais(fluxo: &[DiaFluxo]) -> (i64, i64) {
    let mut ent = 0i64;
    let mut sai = 0i64;
    for d in fluxo {
        for e in &d.entradas {
            if e.centavos >= 0 {
                ent += e.centavos;
            } else {
                sai -= e.centavos;
            }
        }
    }
    (ent, sai)
}

/// Um cartão por dia; guarda os índices das linhas tocadas.
fn dias(ui: &mut egui::Ui, app: &MobileApp, alternar: &mut Vec<(usize, usize)>) {
    for i in 0..app.mock.fluxo.len() {
        let dia = app.mock.fluxo[i].dia;
        let saldo_dia: i64 = app.mock.fluxo[i].entradas.iter().map(|e| e.centavos).sum();
        theme::card(ui, |ui| {
            let c = theme::cores();
            // Cabeçalho do dia: rótulo à esquerda, saldo à direita.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Dia {dia}")).font(w::semibold(14.0)).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let cor = if saldo_dia > 0 { c.green } else if saldo_dia < 0 { c.red } else { c.muted };
                    theme::money(ui, saldo_dia, 13.0, cor);
                });
            });
            ui.add_space(8.0);
            for j in 0..app.mock.fluxo[i].entradas.len() {
                if j > 0 {
                    divisor(ui);
                }
                linha(ui, i, j, app, alternar);
            }
        });
    }
}

/// Divisória fina entre lançamentos do mesmo dia.
fn divisor(ui: &mut egui::Ui) {
    let c = theme::cores();
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(1), c.line);
    ui.add_space(8.0);
}

/// Uma entrada do fluxo; tocar na linha alterna pago (guardado em `alternar`).
fn linha(ui: &mut egui::Ui, i: usize, j: usize, app: &MobileApp, alternar: &mut Vec<(usize, usize)>) {
    let c = theme::cores();
    let e = &app.mock.fluxo[i].entradas[j];
    let receita = e.centavos >= 0;
    let (glifo, cor_valor, suave) = if receita {
        (icon::ARROW_UP_RIGHT, c.green, c.green_soft)
    } else {
        (icon::ARROW_DOWN_LEFT, c.red, c.red_soft)
    };
    let (nome, cor_membro) = MEMBROS[e.membro];
    let quem = if e.membro == app.eu { "Você" } else { nome };
    let inicial = nome.chars().next().unwrap_or('?').to_string();

    let conteudo = ui.horizontal(|ui| {
        // Seta pequena: receita sobe (verde), despesa desce (vermelho).
        let (brect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
        ui.painter().rect_filled(brect, CornerRadius::same(10), suave);
        ui.painter().text(brect.center(), Align2::CENTER_CENTER, glifo.to_string(), w::semibold(13.0), cor_valor);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            // Descrição + valor; pendente leva o chip "a pagar" ao lado do valor.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(e.desc.as_str()).font(w::semibold(14.0)).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::money(ui, e.centavos, 14.0, cor_valor);
                    if !e.paid {
                        ui.add_space(6.0);
                        chipzinho(ui, "a pagar", c.orange, c.orange_soft);
                    }
                });
            });
            // Quem lançou, cartão usado e compartilhamento.
            ui.horizontal(|ui| {
                let (drect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                theme::avatar(ui, drect.center(), 8.0, &inicial, cor_membro);
                ui.label(egui::RichText::new(quem).font(w::regular(12.0)).color(c.muted));
                if let Some(cartao) = e.card.as_deref() {
                    ui.add_space(3.0);
                    chipzinho(ui, cartao, c.blue, c.blue_soft);
                }
                if e.shared {
                    ui.label(egui::RichText::new("· compartilhada").font(w::regular(12.0)).color(c.muted));
                }
            });
        });
    });

    // Linha inteira tocável (pintas o realce por cima, quase invisível).
    let rect = conteudo.response.rect;
    let resp = ui.interact(rect.expand2(Vec2::new(6.0, 4.0)), egui::Id::new(("fluxo-linha", i, j)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(10), Color32::from_rgba_unmultiplied(47, 108, 255, 16));
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        alternar.push((i, j));
    }
}

/// Chip pequeno (11) para cartão e "a pagar".
fn chipzinho(ui: &mut egui::Ui, texto: &str, fg: Color32, bg: Color32) {
    let g = ui.painter().layout_no_wrap(texto.to_owned(), w::semibold(11.0), fg);
    let (rect, _) = ui.allocate_exact_size(g.size() + Vec2::new(12.0, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), bg);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
}
