//! Tela Cartões — total das faturas do mês e um bloco por cartão de crédito:
//! fatura do mês com bolinha de pago (toque alterna) e as compras do mês.

use egui::{Align, Color32, CornerRadius, Layout, Rect, Sense, Vec2, pos2};
use egui_phosphor::regular as icon;
use flounder::model::{Kind, Month};
use flounder::widgets as w;

use crate::app::{fmt_money, MobileApp};
use crate::mock;
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
                w::art(ui, w::Art::Card, 46.0);
            });
        });
    });

    // Dados de cada cartão (fatura e compras do mês) copiados ANTES do desenho: o toque
    // na bolinha alterna depois, com &mut app — nada emprestado de app.dados pendura.
    let linhas = linhas(app);
    let mut tocada = None;
    for linha in &linhas {
        tocada = item(ui, linha, &mes_abr, &mes, &c).or(tocada);
    }
    if let Some((card, due)) = tocada {
        // Alterna no espelho e grava via worker; recalcula a view-model dos cartões
        // APÓS o alternar_pago (atualizar_mes é &mut self) e fora dos blocos de desenho.
        app.alternar_pago(&mock::Acao::Fatura { card, due });
        app.atualizar_mes();
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

/// Campos do cartão copiados para fora do `app`, mais a fatura e as compras do mês.
struct Linha {
    banco: String,
    cor: Color32,
    aberta: bool,
    fecha_dias: u32,
    vence_dia: u8,
    fatura_label: String,
    total_cents: i64,
    compras: u32,
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
                // Real: entradas do mês ligadas ao cartão conectado pelo id.
                Some(cc) => {
                    let mut v: Vec<Compra> = app
                        .data
                        .in_month(month)
                        .filter(|e| e.connected.as_deref() == Some(cc.id.as_str()))
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
                banco: cartao.banco.clone(),
                cor: cartao.cor,
                aberta: cartao.aberta,
                fecha_dias: cartao.fecha_dias,
                vence_dia: cartao.vence_dia,
                fatura_label: cartao.fatura_label.clone(),
                total_cents: cartao.total_cents,
                compras: cartao.compras,
                disponivel_cents: cartao.disponivel_cents,
                limite_cents: cartao.limite_cents,
                paga: cartao.paga,
                fatura,
                compras_mes,
            }
        })
        .collect()
}

/// Cartão de um banco: faixa colorida, header, fatura (com bolinha de pago), compras
/// do mês e limite disponível. Devolve o alvo do toggle se a bolinha foi tocada.
fn item(ui: &mut egui::Ui, linha: &Linha, mes_abr: &str, mes: &str, c: &Cores) -> Option<(String, String)> {
    let mut tocada = None;
    theme::card(ui, |ui| {
        // Faixa fina da cor do banco, colada no topo do conteúdo.
        let topo = ui.available_rect_before_wrap();
        ui.painter().rect_filled(
            Rect::from_min_size(topo.min, Vec2::new(topo.width(), 4.0)),
            CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 },
            linha.cor,
        );
        ui.add_space(8.0);

        // Marca d'água do banco: logo grande e translúcido no fundo do bloco (lado direito).
        if let Some(key) = w::logo_key(&linha.banco)
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
            let inicial = linha.banco.chars().next().unwrap_or('?').to_string();
            theme::avatar(ui, rect.center(), 13.0, &inicial, linha.cor);
            // Nome truncado ("…") para nunca invadir o chip de status.
            let largura_nome = (ui.available_width() - 130.0).max(60.0);
            let fonte_nome = w::semibold(15.0);
            let nome = truncar(&linha.banco, fonte_nome.clone(), c.ink, ui, largura_nome);
            ui.label(egui::RichText::new(nome).font(fonte_nome).color(c.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new(icon::CARET_RIGHT).font(w::regular(16.0)).color(c.muted));
                // Chip padronizado: Paga (fatura paga), Aberta (fatura do mês em aberto)
                // ou Sem fatura (não há fatura neste mês — só o saldo utilizado).
                if linha.paga {
                    theme::chip(ui, "Paga", c.green, c.green_soft);
                } else if linha.aberta {
                    theme::chip(ui, "Aberta", c.blue, c.blue_soft);
                } else {
                    theme::chip(ui, "Sem fatura", c.muted, c.line);
                }
            });
        });
        ui.add_space(2.0);

        // Prazo: coerente com o chip (a abreviação do mês vem da navegação, não do label).
        let prazo = if linha.paga {
            format!("fechada · vence {} {mes_abr}", linha.vence_dia)
        } else if linha.aberta {
            format!("fecha em {} dias · vence {} {mes_abr}", linha.fecha_dias, linha.vence_dia)
        } else {
            format!("sem fatura neste mês · vence {} {mes_abr}", linha.vence_dia)
        };
        ui.label(egui::RichText::new(prazo).font(w::regular(12.0)).color(c.muted));
        ui.add_space(12.0);

        // Fatura do mês: bolinha de pago ao lado do rótulo (só quando há fatura conectada;
        // sem fatura no mês não há o que alternar — o chip "Sem fatura" já diz).
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(linha.fatura_label.as_str()).font(w::regular(13.0)).color(c.muted));
            if let Some(f) = &linha.fatura {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::bolinha_check(ui, f.paga, c.green).clicked() {
                        tocada = Some((f.card.clone(), f.due.clone()));
                    }
                });
            }
        });
        ui.add_space(2.0);
        theme::money(ui, linha.total_cents, 26.0, c.ink);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(format!("{} compras", linha.compras))
                .font(w::regular(13.0))
                .color(c.muted),
        );
        ui.add_space(12.0);

        // Disponível: verde acima de 50% do limite, senão laranja.
        let cor_disp = if linha.limite_cents > 0 && linha.disponivel_cents * 2 > linha.limite_cents {
            c.green
        } else {
            c.orange
        };
        ui.horizontal(|ui| {
            theme::money(ui, linha.disponivel_cents, 14.0, cor_disp);
            ui.label(
                egui::RichText::new(format!(" de R$ {}", fmt_money(linha.limite_cents)))
                    .font(w::regular(13.0))
                    .color(c.muted),
            );
        });
        ui.add_space(8.0);
        let frac = if linha.limite_cents > 0 {
            linha.disponivel_cents as f32 / linha.limite_cents as f32
        } else {
            0.0
        };
        theme::barra(ui, frac, cor_disp, 8.0);

        // Compras do mês neste cartão: descrição + categoria/dia, valor à direita.
        ui.add_space(14.0);
        divisoria(ui);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("Compras de {mes}"))
                .font(w::semibold(13.0))
                .color(c.ink),
        );
        ui.add_space(4.0);
        if linha.compras_mes.is_empty() {
            ui.label(
                egui::RichText::new("Nenhuma compra neste mês.")
                    .font(w::regular(12.0))
                    .color(c.faint),
            );
        } else {
            for (n, comp) in linha.compras_mes.iter().enumerate() {
                if n > 0 {
                    ui.add_space(7.0);
                    divisoria(ui);
                }
                linha_compra(ui, comp, c);
            }
        }
    });
    tocada
}

/// Uma compra do mês: descrição (trunca com "…") com a nota miudinha ao lado e o valor
/// inteiro à direita. Despesa em ink com o −; estorno/receita em verde.
fn linha_compra(ui: &mut egui::Ui, comp: &Compra, c: &Cores) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &comp.desc,
        0.0,
        egui::TextFormat { font_id: w::semibold(13.0), color: c.ink, valign: Align::BOTTOM, ..Default::default() },
    );
    if !comp.nota.is_empty() {
        job.append(
            &format!("· {}", comp.nota),
            6.0,
            egui::TextFormat { font_id: w::regular(11.0), color: c.faint, valign: Align::BOTTOM, ..Default::default() },
        );
    }
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let cor = if comp.centavos >= 0 { c.green } else { c.ink };
            theme::money(ui, comp.centavos, 13.0, cor);
            ui.add(egui::Label::new(job).truncate());
        });
    });
}

/// Divisória fininha da lista de compras.
fn divisoria(ui: &mut egui::Ui) {
    let c = theme::cores();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, c.line);
}
