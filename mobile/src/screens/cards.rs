//! Tela Cartões — total das faturas do mês e um bloco por cartão de crédito.

use egui::{Align, Color32, CornerRadius, Layout, Rect, Sense, Vec2, pos2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{fmt_money, MobileApp};
use crate::mock::Cartao;
use crate::screens::home::truncar;
use crate::theme::{self, Cores};

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    // Mês por extenso em minúsculas ("dezembro") e abreviado ("dez") para os subtítulos.
    let mes = app.mes_label().split_whitespace().next().unwrap_or("").to_lowercase();
    let mes_abr = mes.get(..3).unwrap_or(&mes).to_owned();

    // Resumo do mês: total das faturas + ilustração.
    let total = app.dados.cartoes.total_faturas_cents;
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
    for cartao in &app.dados.cartoes.cards {
        let limite_txt = fmt_money(cartao.limite_cents);
        item(ui, cartao, &limite_txt, &mes_abr, &c);
    }

    // Adicionar cartão novo: só na demonstração (cartões reais chegam da Pluggy).
    if app.demo && ui
        .add_sized(
            [ui.available_width(), 56.0],
            egui::Button::new(
                egui::RichText::new(format!("{}  Adicionar cartão", icon::PLUS))
                    .font(w::semibold(15.0))
                    .color(c.blue),
            )
            .fill(c.blue_soft)
            .corner_radius(22),
        )
        .clicked()
    {
        app.sheet = Some(crate::app::Sheet::Cartao(crate::app::SheetCartao::default()));
    }
}

/// Cartão de um banco: faixa colorida, header, fatura e limite disponível.
fn item(ui: &mut egui::Ui, cartao: &Cartao, limite_txt: &str, mes_abr: &str, c: &Cores) {
    theme::card(ui, |ui| {
        // Faixa fina da cor do banco, colada no topo do conteúdo.
        let topo = ui.available_rect_before_wrap();
        ui.painter().rect_filled(
            Rect::from_min_size(topo.min, Vec2::new(topo.width(), 4.0)),
            CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 },
            cartao.cor,
        );
        ui.add_space(8.0);

        // Marca d'água do banco: logo grande e translúcido no fundo do bloco (lado direito).
        if let Some(key) = w::logo_key(&cartao.banco)
            && let Some(logo) = w::logo_textura(key, false)
            && logo.size()[0] > 0
        {
            let altura = 110.0;
            let largura = altura * logo.size()[0] as f32 / logo.size()[1] as f32;
            let marca = Rect::from_min_size(
                pos2(topo.right() - largura - 12.0, topo.min.y + 14.0),
                Vec2::new(largura, altura),
            );
            ui.painter().image(logo.id(), marca, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha(70));
        }

        // Header: bolinha com a inicial, nome do banco, chip de status e caret.
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            let inicial = cartao.banco.chars().next().unwrap_or('?').to_string();
            theme::avatar(ui, rect.center(), 13.0, &inicial, cartao.cor);
            // Nome truncado ("…") para nunca invadir o chip de status.
            let largura_nome = (ui.available_width() - 130.0).max(60.0);
            let fonte_nome = w::semibold(15.0);
            let nome = truncar(&cartao.banco, fonte_nome.clone(), c.ink, ui, largura_nome);
            ui.label(egui::RichText::new(nome).font(fonte_nome).color(c.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new(icon::CARET_RIGHT).font(w::regular(16.0)).color(c.muted));
                // Chip padronizado: Paga (fatura paga), Aberta (fatura do mês em aberto)
                // ou Sem fatura (não há fatura neste mês — só o saldo utilizado).
                if cartao.paga {
                    theme::chip(ui, "Paga", c.green, c.green_soft);
                } else if cartao.aberta {
                    theme::chip(ui, "Aberta", c.blue, c.blue_soft);
                } else {
                    theme::chip(ui, "Sem fatura", c.muted, c.line);
                }
            });
        });
        ui.add_space(2.0);

        // Prazo: coerente com o chip (a abreviação do mês vem da navegação, não do label).
        let prazo = if cartao.paga {
            format!("fechada · vence {} {mes_abr}", cartao.vence_dia)
        } else if cartao.aberta {
            format!("fecha em {} dias · vence {} {mes_abr}", cartao.fecha_dias, cartao.vence_dia)
        } else {
            format!("sem fatura neste mês · vence {} {mes_abr}", cartao.vence_dia)
        };
        ui.label(egui::RichText::new(prazo).font(w::regular(12.0)).color(c.muted));
        ui.add_space(12.0);

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
        ui.add_space(8.0);
        let frac = if cartao.limite_cents > 0 {
            cartao.disponivel_cents as f32 / cartao.limite_cents as f32
        } else {
            0.0
        };
        theme::barra(ui, frac, cor_disp, 8.0);
    });
}
