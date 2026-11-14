//! Folha de Ajustes (menu ☰): quem usa o celular (troca de membro), sobre o app e fechar.

use egui::{Align, Color32, Frame, Layout, Margin, Sense, Ui, Vec2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::mock::MEMBROS;
use crate::theme;

/// Fallback quando o espelho real ainda não trouxe os nomes (demo).
const MEMBRO_FALLBACK: [&str; 2] = ["Felipe", "Larissa"];

pub fn folha(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let eu = app.eu;
    let nomes = nomes_membros(app);
    // Falha do último toque de troca (memória do egui: sobrevive entre frames da folha).
    let id_falha = egui::Id::new("ajustes_sem_token");
    let mut falha: Option<String> = ui.ctx().data_mut(|d| d.get_temp::<Option<String>>(id_falha).unwrap_or(None));
    // Troca pedida por toque; efetivada fora dos closures (precisa de &mut app).
    let mut pedir: Option<(String, usize)> = None;

    ui.label(egui::RichText::new("Ajustes").font(w::bold(17.0)).color(c.ink));
    ui.label(
        egui::RichText::new("Quem usa este celular e dados do app.")
            .font(w::regular(13.0))
            .color(c.muted),
    );
    ui.add_space(14.0);

    ui.label(egui::RichText::new("Quem é você?").font(w::semibold(13.0)).color(c.ink));
    ui.add_space(6.0);
    for (client, pos) in [("client-1", 0usize), ("client-2", 1usize)] {
        let nome = &nomes[pos];
        let atual = pos == eu;
        let (nome_membro, cor) = MEMBROS[pos];
        let inicial = nome_membro.chars().next().unwrap();
        let resp = Frame::new()
            .fill(if atual { c.blue_soft } else { c.line })
            .corner_radius(12)
            .inner_margin(Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                    theme::avatar(ui, rect.center(), 15.0, &inicial.to_string(), cor);
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(nome).font(w::semibold(14.0)).color(c.ink));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if atual {
                            theme::chip(ui, "este celular", c.blue, Color32::WHITE);
                        } else {
                            ui.label(egui::RichText::new("Trocar").font(w::semibold(12.0)).color(c.blue));
                        }
                    });
                });
            })
            .response
            .interact(Sense::click());
        if resp.clicked() && !atual {
            pedir = Some((client.to_owned(), pos));
        }
        ui.add_space(6.0);
    }

    // Troca: na demonstração troca na hora; na sessão real precisa do token do membro
    // neste dispositivo — sem ele, pede o código de pareamento.
    if let Some((client, pos)) = pedir {
        if app.trocar_membro(client) {
            falha = None;
        } else {
            falha = Some(nomes[pos].clone());
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id_falha, falha.clone()));
    if let Some(nome) = &falha {
        ui.label(
            egui::RichText::new(format!(
                "Sem token de {nome} neste dispositivo — use o código de pareamento."
            ))
            .font(w::regular(12.0))
            .color(c.red),
        );
    }

    ui.add_space(14.0);
    ui.separator();
    ui.add_space(10.0);

    ui.label(egui::RichText::new("Sobre").font(w::semibold(13.0)).color(c.ink));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} Flounder mobile", icon::SPARKLE))
                .font(w::regular(13.0))
                .color(c.muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .font(w::regular(13.0))
                    .color(c.muted),
            );
        });
    });

    ui.add_space(14.0);
    if ui
        .add_sized(
            [ui.available_width(), 44.0],
            egui::Button::new(egui::RichText::new("Fechar").font(w::semibold(15.0)).color(c.ink))
                .fill(c.line)
                .corner_radius(14),
        )
        .clicked()
    {
        app.sheet = None;
    }
}

/// Nomes reais do espelho (`data.names`) com fallback para os membros do mock.
fn nomes_membros(app: &MobileApp) -> [String; 2] {
    let mut nomes = [String::new(), String::new()];
    for (i, fallback) in MEMBRO_FALLBACK.iter().enumerate() {
        let nome = app.data.names[i].trim();
        nomes[i] = if nome.is_empty() { (*fallback).to_owned() } else { app.data.names[i].clone() };
    }
    nomes
}
