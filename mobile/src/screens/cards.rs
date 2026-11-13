//! Tela Cartões — total das faturas do mês e um bloco por cartão de crédito.

use egui::{Align, CornerRadius, Layout, Rect, Sense, Vec2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{fmt_money, MobileApp};
use crate::mock::Cartao;
use crate::theme::{self, Cores};

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    // Mês por extenso em minúsculas ("dezembro") para o subtítulo do resumo.
    let mes = app.mes_label().split_whitespace().next().unwrap_or("").to_lowercase();

    // Resumo do mês: total das faturas + ilustração.
    let total = app.mock.cartoes.total_faturas_cents;
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                theme::money(ui, total, 28.0, c.ink);
                ui.label(
                    egui::RichText::new(format!("total das faturas que vencem em {mes}"))
                        .font(w::regular(13.0))
                        .color(c.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new("💳").size(42.0));
            });
        });
    });

    // Um bloco por cartão do mês.
    for cartao in &app.mock.cartoes.cards {
        let limite_txt = fmt_money(cartao.limite_cents);
        item(ui, cartao, &limite_txt, &c);
    }
}

/// Cartão de um banco: faixa colorida, header, fatura e limite disponível.
fn item(ui: &mut egui::Ui, cartao: &Cartao, limite_txt: &str, c: &Cores) {
    theme::card(ui, |ui| {
        // Faixa fina da cor do banco, colada no topo do conteúdo.
        let topo = ui.available_rect_before_wrap();
        ui.painter().rect_filled(
            Rect::from_min_size(topo.min, Vec2::new(topo.width(), 4.0)),
            CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 },
            cartao.cor,
        );
        ui.add_space(8.0);

        // Header: bolinha com a inicial, nome do banco, chip de status e caret.
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            let inicial = cartao.banco.chars().next().unwrap_or('?').to_string();
            theme::avatar(ui, rect.center(), 13.0, &inicial, cartao.cor);
            ui.label(egui::RichText::new(cartao.banco.as_str()).font(w::semibold(15.0)).color(c.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new(icon::CARET_RIGHT).font(w::regular(16.0)).color(c.muted));
                if cartao.paga {
                    theme::chip(ui, "Paga", c.green, c.green_soft);
                } else {
                    theme::chip(ui, "Aberta", c.blue, c.blue_soft);
                }
            });
        });
        ui.add_space(2.0);

        // Prazo: mês abreviado vem do próprio label da fatura ("Fatura de dezembro" -> "dez").
        let mes_abr: String = cartao
            .fatura_label
            .rsplit(' ')
            .next()
            .unwrap_or("dez")
            .chars()
            .take(3)
            .collect();
        let prazo = if cartao.aberta {
            format!("fecha em {} dias · vence {} {mes_abr}", cartao.fecha_dias, cartao.vence_dia)
        } else {
            format!("fechada · vence {} {mes_abr}", cartao.vence_dia)
        };
        ui.label(egui::RichText::new(prazo).font(w::regular(12.0)).color(c.muted));
        ui.add_space(10.0);

        // Valor da fatura em destaque.
        ui.label(egui::RichText::new(cartao.fatura_label.as_str()).font(w::regular(13.0)).color(c.muted));
        ui.add_space(2.0);
        theme::money(ui, cartao.total_cents, 26.0, c.ink);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(format!("{} compras", cartao.compras))
                .font(w::regular(13.0))
                .color(c.muted),
        );
        ui.add_space(12.0);

        // Disponível: verde acima de 50% do limite, senão laranja.
        let cor_disp = if cartao.limite_cents > 0 && cartao.disponivel_cents * 2 > cartao.limite_cents {
            c.green
        } else {
            c.orange
        };
        ui.horizontal(|ui| {
            theme::money(ui, cartao.disponivel_cents, 14.0, cor_disp);
            ui.label(
                egui::RichText::new(format!(" de R$ {limite_txt}"))
                    .font(w::regular(13.0))
                    .color(c.muted),
            );
        });
        ui.add_space(6.0);
        let frac = if cartao.limite_cents > 0 {
            cartao.disponivel_cents as f32 / cartao.limite_cents as f32
        } else {
            0.0
        };
        theme::barra(ui, frac, cor_disp, 8.0);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("de R$ {limite_txt}"))
                .font(w::regular(12.0))
                .color(c.muted),
        );
    });
}
