//! Tela Início — saldo previsto do mês com a repartição do salário e as contas a pagar por dia.

use egui::{Align, Align2, Color32, Layout, Sense, Vec2, pos2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{MobileApp, fmt_money};
use crate::mock::ContaCasa;
use crate::theme::{self, Cores};

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    card_saldo(ui, app, &c);
    card_pagar(ui, app, &c);
}

/// Card "Saldo previsto": total, barra da repartição e campo do salário no rodapé.
fn card_saldo(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    theme::card(ui, |ui| {
        // Cabeçalho: título + ⓘ e o 👛 encostado no topo direito.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Saldo previsto").font(w::bold(16.0)).color(c.ink));
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(icon::INFO).font(w::regular(14.0)).color(c.faint));
                });
                ui.add_space(4.0);
                theme::money(ui, app.dados.saldo.centavos, 30.0, c.ink);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon::TREND_UP).font(w::bold(12.0)).color(c.green));
                    ui.label(
                        egui::RichText::new(format!("{}% do salário livre", app.dados.saldo.pct_livre))
                            .font(w::semibold(12.0))
                            .color(c.green),
                    );
                });
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                ui.label(egui::RichText::new("👛").size(42.0));
            });
        });
        ui.add_space(12.0);

        // Barra empilhada com a repartição do salário.
        let segmentos: Vec<(f32, Color32)> = app
            .dados
            .reparticao
            .iter()
            .map(|r| (r.pct as f32 / 100.0, r.cor))
            .collect();
        theme::barra_empilhada(ui, &segmentos);
        ui.add_space(12.0);

        // Uma linha por pedaço: bolinha da cor, label, valor e pct.
        for r in &app.dados.reparticao {
            let cor_valor = if r.label == "Sobra" { c.green } else { c.ink };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, r.cor);
                ui.add_space(4.0);
                ui.label(egui::RichText::new(r.label.as_str()).font(w::regular(14.0)).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{}%", r.pct)).font(w::regular(12.0)).color(c.muted));
                    theme::money(ui, r.centavos, 14.0, cor_valor);
                });
            });
            ui.add_space(7.0);
        }

        // Rodapé: campo do salário (estático na fase 1).
        ui.add_space(2.0);
        egui::Frame::new()
            .fill(c.line)
            .corner_radius(14)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Salário").font(w::regular(13.0)).color(c.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(egui::RichText::new(icon::PENCIL_SIMPLE).font(w::regular(16.0)).color(c.muted));
                        theme::money(ui, app.dados.salario_cents, 15.0, c.ink);
                    });
                });
            });
    });
}

/// Card "Contas a pagar": total do dia em destaque, tira de dias e as contas daquele dia.
fn card_pagar(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    // Dia selecionado na tira (lembra entre frames; padrão = destaque do mock).
    let lembrado = ui.memory(|m| m.data.get_temp::<u8>(egui::Id::new("pagar-dia")));
    let dia_sel = match lembrado {
        Some(d) if app.dados.pagar.dias.iter().any(|x| x.0 == d) => d,
        _ => app.dados.pagar.dia,
    };
    let total_dia = app
        .dados
        .pagar
        .dias
        .iter()
        .find(|x| x.0 == dia_sel)
        .map(|x| x.2)
        .unwrap_or(app.dados.pagar.total_cents);

    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Contas a pagar · dia {dia_sel}"))
                    .font(w::bold(16.0))
                    .color(c.ink),
            );
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                ui.label(egui::RichText::new("📅").size(36.0));
            });
        });
        ui.add_space(6.0);
        theme::money(ui, total_dia, 28.0, c.ink);
        ui.add_space(14.0);

        // Tira horizontal de dias; toque seleciona.
        egui::ScrollArea::horizontal().id_salt("tira-pagar").show(ui, |ui| {
            ui.set_min_height(60.0);
            ui.horizontal(|ui| {
                for (dia, _, total) in &app.dados.pagar.dias {
                    chip_dia(ui, *dia, *total, dia_sel, c);
                    ui.add_space(8.0);
                }
            });
        });
        ui.add_space(8.0);

        // Contas do dia selecionado; sem contas, mostra as próximas do mês.
        let do_dia: Vec<&ContaCasa> = app.dados.casa.bills.iter().filter(|b| b.dia == dia_sel).collect();
        if do_dia.is_empty() {
            ui.label(egui::RichText::new("próximas contas").font(w::regular(12.0)).color(c.muted));
            ui.add_space(2.0);
            let mut todas: Vec<&ContaCasa> = app.dados.casa.bills.iter().collect();
            todas.sort_by_key(|b| b.dia);
            for b in todas.into_iter().take(3) {
                linha_conta(ui, b, c);
            }
        } else {
            for b in do_dia {
                linha_conta(ui, b, c);
            }
        }
    });
}

/// Caixinha da tira: dia em cima, total curto embaixo; azul quando selecionada.
fn chip_dia(ui: &mut egui::Ui, dia: u8, total_cents: i64, selecionado: u8, c: &Cores) {
    let on = dia == selecionado;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::click());
    ui.painter().rect_filled(rect, 14.0, if on { c.blue } else { c.line });
    let (cor_dia, cor_total) = if on {
        (Color32::WHITE, Color32::from_rgba_unmultiplied(255, 255, 255, 200))
    } else {
        (c.ink, c.muted)
    };
    ui.painter().text(
        pos2(rect.center().x, rect.top() + 19.0),
        Align2::CENTER_CENTER,
        format!("{dia:02}"),
        w::bold(15.0),
        cor_dia,
    );
    ui.painter().text(
        pos2(rect.center().x, rect.bottom() - 14.0),
        Align2::CENTER_CENTER,
        total_curto(total_cents),
        w::regular(10.5),
        cor_total,
    );
    if resp.clicked() {
        ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("pagar-dia"), dia));
    }
}

/// Linha "nome ..... R$ valor" de uma conta da casa.
fn linha_conta(ui: &mut egui::Ui, b: &ContaCasa, c: &Cores) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(b.nome.as_str()).font(w::regular(14.0)).color(c.ink));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            theme::money(ui, b.total_cents, 14.0, c.ink);
            if b.aproximado {
                ui.label(egui::RichText::new("≈").font(w::semibold(13.0)).color(c.muted));
            }
        });
    });
    ui.add_space(7.0);
}

/// Total curto para a tira ("R$ 1.937"): corta os centavos.
fn total_curto(cents: i64) -> String {
    let completo = fmt_money(cents);
    let inteiro = completo.split(',').next().unwrap_or("0");
    format!("R$ {inteiro}")
}
