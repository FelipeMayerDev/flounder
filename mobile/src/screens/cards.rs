//! Tela Cartões — total das faturas do mês e um bloco por cartão de crédito:
//! fatura do mês com status de pagamento, as compras do mês, o fechamento
//! do ciclo (chip abre o ajuste no modo real) e a fatura em formação ("Próxima fatura").

use egui::{Align, Align2, Color32, Frame, Layout, Margin, Sense, Vec2};
use egui_phosphor::regular as icon;
use flounder::model::{Kind, Month};
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::converter;
use crate::mock;
use crate::theme::{self, Cores};

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let mes = Month { y: app.y, m: app.m.into() };

    let total = app.dados.cartoes.total_faturas_cents;
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Total das faturas").font(w::semibold(13.0)).color(c.muted),
                );
                theme::money_inline(ui, total, 28.0, c.ink);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::hover());
                ui.painter().rect_filled(rect, 14.0, c.blue_soft);
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::CREDIT_CARD, w::regular(23.0), c.blue);
            });
        });
    });

    let linhas = linhas(app);
    if linhas.is_empty() {
        theme::vazio(ui, icon::CREDIT_CARD, "Nenhum cartão neste mês.");
        return;
    }
    let mut tocada = None;
    let mut fecho = None;
    for (i, linha) in linhas.iter().enumerate() {
        let toque = ui.push_id((i, &linha.banco), |ui| item(ui, linha, mes, &c)).inner;
        match toque {
            Some(Toque::Pago { card, due }) => tocada = Some((card, due)),
            Some(Toque::Fechamento(id)) => fecho = Some(id),
            None => {}
        }
    }
    if let Some((card, due)) = tocada {
        app.alternar_pago(&mock::Acao::Fatura { card, due });
    }
    if let Some(card) = fecho {
        // Folha de ajuste do fechamento: começa com o dia atual do cartão conectado.
        if let Some(cc) = app.connected_cards.as_ref().and_then(|cs| cs.iter().find(|c| c.id == card)) {
            app.sheet = Some(crate::app::Sheet::Fechamento(crate::app::FolhaFechamento {
                card: cc.id.clone(),
                banco: cc.institution.clone(),
                dia: cc.closing_day.filter(|d| (1..=31).contains(d)).map(|d| d.to_string()).unwrap_or_default(),
                tried: false,
            }));
        }
    }
}

/// Fatura do mês de um cartão conectado — alvo do toggle de pago.
struct FaturaMes {
    card: String,
    /// Vencimento ISO ("2026-12-05"), como o backend espera no `Cmd::BillPaid`.
    due: String,
    paga: bool,
}

/// Uma compra do mês no cartão, com sinal (estorno/receita entra positivo).
struct Compra {
    dia: u8,
    desc: String,
    /// Categoria (real) ou "dia N" (demonstração), miudinha ao lado da descrição.
    nota: String,
    centavos: i64,
}

/// Toque num cartão: alterna o pago da fatura ou abre o ajuste do fechamento.
enum Toque {
    Pago { card: String, due: String },
    /// Id do cartão conectado cujo fechamento vai ser ajustado.
    Fechamento(String),
}

/// Campos do cartão copiados para fora do `app`, mais a fatura e as compras do mês.
struct Linha {
    /// Id do cartão conectado (None na demonstração): habilita o ajuste do fechamento.
    id: Option<String>,
    banco: String,
    cor: Color32,
    aberta: bool,
    fecha_dias: u32,
    fechamento_dia: Option<u8>,
    vence_dia: u8,
    fatura_label: String,
    total_cents: i64,
    compras: u32,
    /// Fatura em formação: total e qtd do que já cai na fatura seguinte.
    proximo_total_cents: i64,
    proximo_qtd: u32,
    /// Totais projetados dos próximos vencimentos ("dez 26", centavos).
    proximas: Vec<(String, i64)>,
    disponivel_cents: i64,
    limite_cents: i64,
    paga: bool,
    /// Fatura do mês (só em cartão conectado com bill neste mês): habilita o toggle.
    fatura: Option<FaturaMes>,
    compras_mes: Vec<Compra>,
}

/// Copia os cartões do mês com a fatura e as compras de cada um. O casamento com o
/// cartão conectado é pela instituição (mesma chave usada no converter e no Fluxo).
fn linhas(app: &MobileApp) -> Vec<Linha> {
    let month = Month { y: app.y, m: app.m as u32 };
    let prefixo = format!("{:04}-{:02}-", app.y, app.m);
    app.dados
        .cartoes
        .cards
        .iter()
        .map(|cartao| {
            let conectado = app
                .connected_cards
                .as_ref()
                .and_then(|cs| cs.iter().find(|cc| cc.institution == cartao.banco));
            // Fatura do mês: bill cujo vencimento cai no mês aberto (paid vem do espelho).
            let fatura = conectado.and_then(|cc| {
                cc.bills
                    .iter()
                    .find(|b| b.due.starts_with(&prefixo))
                    .map(|b| FaturaMes { card: cc.id.clone(), due: b.due.clone(), paga: b.paid })
            });
            let compras_mes = match conectado {
                // Real: compras da fatura que vence no mês (janela do fechamento, como no
                // desktop e no contador da view-model), ligadas ao cartão pelo id.
                Some(cc) => {
                    let mut v: Vec<Compra> = app
                        .data
                        .connected_purchases(month, &cc.id, cc.closing_day, cc.due_day)
                        .map(|e| Compra {
                            dia: e.date.d as u8,
                            desc: e.desc.clone(),
                            nota: e.category.label().to_owned(),
                            centavos: if e.kind == Kind::Income { e.amount } else { -e.amount },
                        })
                        .collect();
                    v.sort_by_key(|x| x.dia);
                    v
                }
                // Demonstração: as compras moram no fluxo do mock, ligadas pelo nome do banco.
                None => {
                    let mut v: Vec<Compra> = app
                        .dados
                        .fluxo
                        .iter()
                        .flat_map(|d| {
                            d.entradas
                                .iter()
                                .filter(|e| e.card.as_deref() == Some(cartao.banco.as_str()))
                                .map(|e| Compra {
                                    dia: d.dia,
                                    desc: e.desc.clone(),
                                    nota: format!("dia {}", d.dia),
                                    centavos: e.centavos,
                                })
                        })
                        .collect();
                    v.sort_by_key(|x| x.dia);
                    v
                }
            };
            Linha {
                id: conectado.map(|cc| cc.id.clone()),
                banco: cartao.banco.clone(),
                cor: cartao.cor,
                aberta: cartao.aberta,
                fecha_dias: cartao.fecha_dias,
                fechamento_dia: cartao.fechamento_dia,
                vence_dia: cartao.vence_dia,
                fatura_label: cartao.fatura_label.clone(),
                total_cents: cartao.total_cents,
                compras: cartao.compras,
                proximo_total_cents: cartao.proximo_total_cents,
                proximo_qtd: cartao.proximo_qtd,
                proximas: cartao.proximas.clone(),
                disponivel_cents: cartao.disponivel_cents,
                limite_cents: cartao.limite_cents,
                paga: cartao.paga,
                fatura,
                compras_mes,
            }
        })
        .collect()
}

/// Resumo do cartão e detalhes da fatura.
fn item(ui: &mut egui::Ui, linha: &Linha, mes: Month, c: &Cores) -> Option<Toque> {
    let mut tocada = None;
    theme::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            let (marca, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            let inicial = linha.banco.chars().next().unwrap_or('?').to_string();
            theme::avatar(ui, marca.center(), 18.0, &inicial, linha.cor);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if linha.paga {
                    theme::chip(ui, "Paga", c.green, c.green_soft);
                } else if linha.aberta {
                    theme::chip(ui, "Em aberto", c.blue, c.blue_soft);
                } else {
                    theme::chip(ui, "Sem fatura", c.muted, c.line);
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), marca.height()),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new(&linha.banco).font(w::bold(16.0)).color(c.ink),
                        ).truncate());
                    },
                );
            });
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&linha.fatura_label).font(w::regular(13.0)).color(c.muted));
            if let Some(fatura) = &linha.fatura {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let rotulo = if fatura.paga { "Desmarcar pagamento" } else { "Marcar como paga" };
                    if ui.add(egui::Button::new(rotulo).min_size(Vec2::new(0.0, 36.0))).clicked() {
                        tocada = Some(Toque::Pago { card: fatura.card.clone(), due: fatura.due.clone() });
                    }
                });
            }
        });
        theme::money_inline(ui, linha.total_cents, 24.0, c.ink);
        ui.horizontal_wrapped(|ui| {
            if linha.paga || linha.aberta {
                let mes_abr = mes.name()[..3].to_lowercase();
                ui.label(egui::RichText::new(format!("Vencimento: {} {mes_abr}", linha.vence_dia))
                    .font(w::regular(12.0)).color(c.muted));
            }
            if let Some(id) = &linha.id {
                if chip_fechamento(ui, linha, c) {
                    tocada = Some(Toque::Fechamento(id.clone()));
                }
            } else if let Some(dia) = linha.fechamento_dia {
                ui.label(egui::RichText::new(format!("Fecha dia {dia}")).font(w::regular(12.0)).color(c.muted));
            }
        });
        ui.add_space(6.0);

        if linha.limite_cents > 0 || linha.disponivel_cents != 0 {
            limite(ui, linha, c);
        }
        compras(ui, linha, c);
        proximas(ui, linha, mes, c);
    });
    tocada
}

fn limite(ui: &mut egui::Ui, linha: &Linha, c: &Cores) {
    Frame::new().fill(c.line).corner_radius(14).inner_margin(Margin::same(12))
        .show(ui, |ui| {
            linha_valor(ui, "Limite disponível", linha.disponivel_cents, c.muted, c.ink);
            if linha.limite_cents <= 0 {
                return;
            }
            let frac = linha.disponivel_cents as f32 / linha.limite_cents as f32;
            let cor = if frac > 0.5 { c.green } else { c.orange };
            ui.label(egui::RichText::new(format!("Limite total: {}", flounder::model::money(linha.limite_cents)))
                .font(w::regular(12.0)).color(c.muted));
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
            ui.painter().rect_filled(rect, 3.0, c.card);
            let disponivel = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac.clamp(0.0, 1.0), rect.height()));
            ui.painter().rect_filled(disponivel, 3.0, cor);
        });
}

fn quantidade_compras(quantidade: usize) -> String {
    if quantidade == 1 { "1 compra".into() } else { format!("{quantidade} compras") }
}

fn compras(ui: &mut egui::Ui, linha: &Linha, c: &Cores) {
    let quantidade = (linha.compras as usize).max(linha.compras_mes.len());
    if quantidade == 0 {
        return;
    }
    ui.add_space(4.0);
    let titulo = egui::RichText::new(quantidade_compras(quantidade)).font(w::semibold(13.0)).color(c.ink);
    if linha.compras_mes.is_empty() {
        ui.label(titulo);
        ui.label(egui::RichText::new("Detalhes das compras indisponíveis.").font(w::regular(12.0)).color(c.muted));
        return;
    }
    egui::CollapsingHeader::new(titulo).id_salt("compras").show_unindented(ui, |ui| {
        if linha.compras_mes.len() < quantidade {
            ui.label(egui::RichText::new(format!("Detalhes de {} de {quantidade} compras", linha.compras_mes.len()))
                .font(w::regular(12.0)).color(c.muted));
        }
        for comp in &linha.compras_mes {
            linha_valor(ui, &comp.desc, comp.centavos, c.ink, if comp.centavos >= 0 { c.green } else { c.ink });
            ui.label(egui::RichText::new(&comp.nota).font(w::regular(11.5)).color(c.muted));
        }
    });
}

fn proximas(ui: &mut egui::Ui, linha: &Linha, mes: Month, c: &Cores) {
    if linha.proximo_qtd == 0 && linha.proximas.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.separator();
    ui.label(egui::RichText::new("Próximas faturas").font(w::semibold(13.0)).color(c.ink));
    let mes_seguinte = mes.add(1).short().to_lowercase();
    let em_formacao = linha.proximo_qtd > 0;
    let mesma_fatura = em_formacao
        && linha.proximas.first().is_some_and(|(mes, total)| mes == &mes_seguinte && *total == linha.proximo_total_cents);
    if em_formacao {
        let rotulo = if mesma_fatura { &mes_seguinte } else { "Em formação" };
        let rotulo = format!("{rotulo} · {}", quantidade_compras(linha.proximo_qtd as usize));
        linha_valor(ui, &rotulo, linha.proximo_total_cents, c.muted, c.ink);
    }
    for (mes, total) in linha.proximas.iter().skip(usize::from(mesma_fatura)) {
        linha_valor(ui, mes, *total, c.muted, c.ink);
    }
}

fn linha_valor(ui: &mut egui::Ui, rotulo: &str, centavos: i64, cor_rotulo: Color32, cor_valor: Color32) {
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 22.0), Layout::right_to_left(Align::Center), |ui| {
        theme::money_inline(ui, centavos, 13.0, cor_valor);
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 22.0), Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(rotulo).font(w::regular(13.0)).color(cor_rotulo)).truncate());
        });
    });
}

/// Chip clicável do fechamento ("Fechamento dia 20 ⚙"): abre a folha de ajuste.
/// Faltam ≤ 3 dias (0 = hoje)? Destaca em âmbar/vermelho com o prazo por extenso.
fn chip_fechamento(ui: &mut egui::Ui, linha: &Linha, c: &Cores) -> bool {
    let (rotulo, fg, bg) = match linha.fechamento_dia {
        Some(_) if linha.fecha_dias <= 3 => (
            format!("{}  {}", converter::prazo_fechamento(linha.fecha_dias, linha.fechamento_dia), icon::GEAR),
            if linha.fecha_dias == 0 { c.red } else { c.orange },
            if linha.fecha_dias == 0 { c.red_soft } else { c.orange_soft },
        ),
        Some(d) => (format!("Fechamento dia {d}  {}", icon::GEAR), c.blue, c.blue_soft),
        None => (format!("Sem fechamento  {}", icon::GEAR), c.muted, c.line),
    };
    ui.add(
        egui::Button::new(egui::RichText::new(rotulo).font(w::semibold(11.5)).color(fg))
            .fill(bg)
            .corner_radius(255),
    )
    .clicked()
}
