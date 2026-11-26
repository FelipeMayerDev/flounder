//! Tela Início — saldo previsto do mês com a repartição do salário e as contas a pagar por dia.
//! Acima de tudo, o aviso de fechamento de cartão; abaixo, os orçamentos do mês e os
//! alertas de preço das contas fixas.

use egui::{Align, Align2, Color32, Frame, Layout, Margin, Sense, Vec2, pos2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{MobileApp, fmt_money};
use crate::mock;
use crate::theme::{self, Cores};

pub fn show(ui: &mut egui::Ui, app: &mut MobileApp) {
    let c = theme::cores();
    // Aviso de fechamento: fatura de um cartão conectado fecha em ≤ 3 dias.
    if let Some((banco, dias)) = app.dados.fechamento_alerta.clone() {
        banner_fechamento(ui, &banco, dias, &c);
        ui.add_space(2.0);
    }
    card_saldo(ui, app, &c);
    if !app.dados.orcamentos.is_empty() {
        card_orcamentos(ui, app, &c);
    }
    if !app.dados.alertas_preco.is_empty() {
        card_alertas(ui, app, &c);
    }
    card_pagar(ui, app, &c);
}

/// Rótulo amigável da categoria do orçamento (chave `Category::key`).
pub(crate) fn rotulo_categoria(key: &str) -> &'static str {
    match key {
        "groceries" => "Mercado",
        "transport" => "Transporte",
        "utilities" => "Luz/Água",
        "housing" => "Moradia",
        "health" => "Saúde",
        "leisure" => "Lazer",
        "shopping" => "Compras",
        "services" => "Serviços",
        "education" => "Educação",
        "pets" => "Pets",
        "other" => "Outros",
        _ => "Categoria",
    }
}

/// Faixa de aviso do fechamento: vermelho quando fecha hoje, âmbar nos outros dias.
fn banner_fechamento(ui: &mut egui::Ui, banco: &str, dias: u32, c: &Cores) {
    let (cor, fundo) = if dias == 0 { (c.red, c.red_soft) } else { (c.orange, c.orange_soft) };
    let prazo = match dias {
        0 => "fecha hoje".to_owned(),
        1 => "fecha amanhã".to_owned(),
        n => format!("fecha em {n} dias"),
    };
    let detalhe = match dias {
        0 => "Compras de hoje já vão para a próxima fatura.",
        1 => "Compras de amanhã já vão para a próxima fatura.",
        _ => "Compras no dia do fechamento já vão para a próxima fatura.",
    };
    Frame::new()
        .fill(fundo)
        .corner_radius(16)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(icon::WARNING).font(w::bold(16.0)).color(cor));
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Fatura {banco} {prazo}"))
                            .font(w::semibold(13.5))
                            .color(cor),
                    );
                    ui.label(egui::RichText::new(detalhe).font(w::regular(12.0)).color(cor));
                });
            });
        });
}

/// Card "Orçamento do mês": uma linha por categoria com barra de progresso.
/// Vermelho em 100% ou mais, âmbar a partir de 80%.
fn card_orcamentos(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    let orcamentos: Vec<(String, i64, i64)> = app.dados.orcamentos.clone();
    theme::card(ui, |ui| {
        cabecalho(ui, "Orçamento do mês", "Quanto do limite você já usou por categoria.", w::Art::MoneyBag, c);
        ui.add_space(10.0);
        for (categoria, gasto, limite) in &orcamentos {
            let frac = if *limite > 0 { *gasto as f32 / *limite as f32 } else { 0.0 };
            let pct = (frac * 100.0).round() as i64;
            let cor = if pct >= 100 {
                c.red
            } else if pct >= 80 {
                c.orange
            } else {
                c.green
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(rotulo_categoria(categoria)).font(w::semibold(14.0)).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Ordem right-to-left: % fica na ponta, depois o limite, o gasto à esquerda.
                    ui.label(egui::RichText::new(format!("{pct}%")).font(w::semibold(12.0)).color(cor));
                    ui.label(
                        egui::RichText::new(format!("de R$ {}", fmt_money(*limite)))
                            .font(w::regular(12.0))
                            .color(c.muted),
                    );
                    theme::money(ui, *gasto, 14.0, cor);
                });
            });
            ui.add_space(3.0);
            theme::barra(ui, frac, cor, 7.0);
            ui.add_space(10.0);
        }
    });
}

/// Card "Alertas de preço": contas fixas que vieram bem acima da média histórica.
fn card_alertas(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    let alertas: Vec<(String, i64, i64)> = app.dados.alertas_preco.clone();
    theme::card(ui, |ui| {
        cabecalho(ui, "Alertas de preço", "Contas acima da média dos meses anteriores.", w::Art::SpiralCalendar, c);
        ui.add_space(10.0);
        for (desc, recente, media) in &alertas {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{desc}:")).font(w::semibold(14.0)).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(" (média R$ {})", fmt_money(*media)))
                            .font(w::regular(12.0))
                            .color(c.muted),
                    );
                    theme::money(ui, *recente, 14.0, c.orange);
                });
            });
            ui.add_space(6.0);
        }
    });
}

fn cabecalho(ui: &mut egui::Ui, titulo: &str, subtitulo: &str, arte: w::Art, c: &Cores) {
    ui.horizontal_top(|ui| {
        let largura = ui.available_width() - 44.0 - ui.spacing().item_spacing.x;
        ui.allocate_ui_with_layout(Vec2::new(largura, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(largura);
            ui.label(egui::RichText::new(titulo).font(w::bold(16.0)).color(c.ink));
            ui.add(egui::Label::new(egui::RichText::new(subtitulo).font(w::regular(12.0)).color(c.muted)).wrap());
        });
        w::art(ui, arte, 44.0);
    });
}

/// Card "Saldo previsto": total, barra da repartição e campo da renda do mês no rodapé.
fn card_saldo(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    let negativo = app.dados.saldo.centavos < 0;
    theme::card(ui, |ui| {
        // Cabeçalho: título + ⓘ à esquerda; a ilustração 3D alinhada pelo topo com o título.
        ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Saldo previsto").font(w::bold(16.0)).color(c.ink));
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(icon::INFO).font(w::regular(14.0)).color(c.faint));
                });
                ui.add_space(4.0);
                theme::money(ui, app.dados.saldo.centavos, 30.0, if negativo { c.red } else { c.ink });
                ui.add_space(6.0);
                // Saldo negativo: quanto falta para fechar o mês, em vermelho (como no desktop).
                ui.horizontal(|ui| {
                    if negativo {
                        ui.label(egui::RichText::new(icon::WARNING_CIRCLE).font(w::bold(12.0)).color(c.red));
                        ui.label(
                            egui::RichText::new(format!(
                                "Faltam R$ {} para fechar o mês",
                                fmt_money(-app.dados.saldo.centavos)
                            ))
                            .font(w::semibold(12.0))
                            .color(c.red),
                        );
                    } else {
                        ui.label(egui::RichText::new(icon::TREND_UP).font(w::bold(12.0)).color(c.green));
                        ui.label(
                            egui::RichText::new(format!("{}% do salário livre", app.dados.saldo.pct_livre))
                                .font(w::semibold(12.0))
                                .color(c.green),
                        );
                    }
                });
            });
            w::art(ui, w::Art::MoneyBag, 54.0);
        });
        ui.add_space(12.0);

        // Barra empilhada com a repartição do salário (Sobra fica vermelha se negativa).
        let segmentos: Vec<(f32, Color32)> = app
            .dados
            .reparticao
            .iter()
            .map(|r| {
                let cor = if r.label == "Sobra" && r.centavos < 0 { c.red } else { r.cor };
                (r.pct as f32 / 100.0, cor)
            })
            .collect();
        theme::barra_empilhada(ui, &segmentos);
        ui.add_space(12.0);

        // Uma linha por pedaço: bolinha da cor, label, valor e pct.
        for r in &app.dados.reparticao {
            let sobra_negativa = r.label == "Sobra" && r.centavos < 0;
            let cor_valor = if sobra_negativa {
                c.red
            } else if r.label == "Sobra" {
                c.green
            } else {
                c.ink
            };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, r.cor);
                ui.add_space(4.0);
                // Nome truncado ("…") para nunca invadir o valor alinhado à direita.
                let largura_nome = (ui.available_width() - 140.0).max(60.0);
                let fonte_nome = w::regular(14.0);
                let nome = truncar(&r.label, fonte_nome.clone(), c.ink, ui, largura_nome);
                ui.label(egui::RichText::new(nome).font(fonte_nome).color(c.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{}%", r.pct)).font(w::regular(12.0)).color(c.muted));
                    theme::money(ui, r.centavos, 14.0, cor_valor);
                });
            });
            ui.add_space(8.0);
        }

        // Rodapé: renda lançada no mês (pode diferir do salário fixo).
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(c.line)
            .corner_radius(14)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Renda do mês").font(w::regular(13.0)).color(c.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::money(ui, app.dados.salario_cents, 15.0, c.ink);
                    });
                });
            });
    });
}

/// Card "Contas a pagar": pendente do dia selecionado, tira de dias tocável e a lista
/// das contas do dia com bolinha de pago (toque alterna; demo inverte localmente).
fn card_pagar(ui: &mut egui::Ui, app: &mut MobileApp, c: &Cores) {
    // Dia selecionado na tira (lembra entre frames; padrão = destaque do mock).
    let dia_id = ui.make_persistent_id("pagar-dia");
    let lembrado = ui.memory(|m| m.data.get_temp::<u8>(dia_id));
    let dia_sel = match lembrado {
        Some(d) if app.dados.pagar.dias.iter().any(|x| x.0 == d) => d,
        _ => app.dados.pagar.dia,
    };
    // Contas do dia selecionado, com o índice original em `pagar.contas` (a demo
    // inverte `paga` por índice). Cópias donas: o card muta `app` ao alternar,
    // então não pode segurar empréstimo da view-model durante o desenho.
    let contas_do_dia: Vec<(usize, String, i64, bool, mock::Acao)> = app
        .dados
        .pagar
        .contas
        .iter()
        .enumerate()
        .filter(|(_, ct)| ct.dia == dia_sel)
        .map(|(i, ct)| (i, ct.label.clone(), ct.total_cents, ct.paga, ct.acao.clone()))
        .collect();
    let tem_contas = !contas_do_dia.is_empty();
    // Resumo do dia: total não pago, recalculado das contas do dia (a tira não se
    // atualiza na demo, onde alternar é só local; sem contas, cai no total da tira).
    let pendente_dia = if tem_contas {
        contas_do_dia.iter().filter(|ct| !ct.3).map(|ct| ct.2).sum::<i64>()
    } else {
        app.dados
            .pagar
            .dias
            .iter()
            .find(|x| x.0 == dia_sel)
            .map(|x| x.2)
            .unwrap_or(app.dados.pagar.total_cents)
    };
    let todo_pago = tem_contas && pendente_dia == 0;

    theme::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
            ui.label(
                egui::RichText::new(format!("Contas a pagar · dia {dia_sel}"))
                    .font(w::bold(16.0))
                    .color(c.ink),
            );
            w::art(ui, w::Art::SpiralCalendar, 46.0);
        });
        ui.add_space(8.0);
        // Resumo do dia: verde quando o dia já está todo pago.
        theme::money(ui, pendente_dia, 28.0, if todo_pago { c.green } else { c.ink });
        if tem_contas {
            ui.add_space(2.0);
            if todo_pago {
                ui.label(egui::RichText::new("Tudo pago neste dia.").font(w::semibold(12.0)).color(c.green));
            } else {
                ui.label(egui::RichText::new("Pendente neste dia").font(w::regular(12.0)).color(c.muted));
            }
        }
        ui.add_space(16.0);

        // Tira horizontal de dias; toque seleciona.
        egui::ScrollArea::horizontal().id_salt("tira-pagar").show(ui, |ui| {
            ui.set_min_height(60.0);
            ui.horizontal(|ui| {
                for (dia, _, total) in &app.dados.pagar.dias {
                    if chip_dia(ui, *dia, *total, dia_sel, c).clicked() {
                        ui.memory_mut(|m| m.data.insert_temp(dia_id, *dia));
                    }
                    ui.add_space(8.0);
                }
            });
        });
        ui.add_space(10.0);

        // Contas do dia selecionado; toque na bolinha alterna pago.
        if contas_do_dia.is_empty() {
            ui.label(egui::RichText::new("Nenhuma conta neste dia.").font(w::regular(12.0)).color(c.muted));
        } else {
            for (idx, label, total_cents, paga, acao) in &contas_do_dia {
                ui.horizontal(|ui| {
                    let bolinha = theme::bolinha_check(ui, *paga, c.green);
                    if bolinha.clicked() {
                        alternar_conta(app, *idx, acao);
                    }
                    ui.add_space(2.0);
                    // Nome truncado ("…") para nunca invadir o valor alinhado à direita;
                    // pago fica mudo, como nas contas da casa.
                    let cor = if *paga { c.muted } else { c.ink };
                    let largura_nome = (ui.available_width() - 120.0).max(60.0);
                    let fonte_nome = w::regular(14.0);
                    let nome = truncar(label, fonte_nome.clone(), cor, ui, largura_nome);
                    ui.label(egui::RichText::new(nome).font(fonte_nome).color(cor));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::money(ui, *total_cents, 14.0, cor);
                    });
                });
                ui.add_space(8.0);
            }
        }
    });
}

/// Toque na bolinha de uma conta: na demo (ou quando a conta não tem ação gravável)
/// inverte `paga` localmente, para a demonstração ficar viva; na sessão real chama
/// `alternar_pago`, que grava via worker e recalcula a view-model.
fn alternar_conta(app: &mut MobileApp, idx: usize, acao: &mock::Acao) {
    if app.demo || matches!(acao, mock::Acao::Nenhuma) {
        if let Some(conta) = app.dados.pagar.contas.get_mut(idx) {
            conta.paga = !conta.paga;
        }
        return;
    }
    app.alternar_pago(acao);
}

/// Caixinha da tira: dia em cima, total curto embaixo; azul quando selecionada.
/// Largura mínima de 56 px e cresce conforme o total, para nada ficar cortado.
fn chip_dia(ui: &mut egui::Ui, dia: u8, total_cents: i64, selecionado: u8, c: &Cores) -> egui::Response {
    let on = dia == selecionado;
    let (cor_dia, cor_total) = if on {
        (Color32::WHITE, Color32::from_rgba_unmultiplied(255, 255, 255, 200))
    } else {
        (c.ink, c.muted)
    };
    let total_txt = total_curto(total_cents);
    let largura_total = ui
        .painter()
        .layout_no_wrap(total_txt.clone(), w::regular(10.5), cor_total)
        .size()
        .x;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new((largura_total + 20.0).max(56.0), 56.0), Sense::click());
    ui.painter().rect_filled(rect, 14.0, if on { c.blue } else { c.line });
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
        total_txt,
        w::regular(10.5),
        cor_total,
    );
    resp
}

/// Total curto para a tira: corta os centavos ("R$ 849", "R$ 8.712") e abrevia
/// milhares longos ("R$ 19k") para caber na caixinha.
fn total_curto(cents: i64) -> String {
    let reais = cents.abs() / 100;
    if reais > 9_999 {
        format!("R$ {}k", reais / 1000)
    } else {
        let completo = fmt_money(cents);
        let inteiro = completo.split(',').next().unwrap_or("0");
        format!("R$ {inteiro}")
    }
}

/// Corta o texto com "…" para caber na largura dada (mede com a própria fonte),
/// para nomes longos nunca invadirem o valor alinhado à direita.
pub(crate) fn truncar(texto: &str, fonte: egui::FontId, cor: Color32, ui: &egui::Ui, largura: f32) -> String {
    let pintor = ui.painter().clone();
    let cabe = |t: &str| pintor.layout_no_wrap(t.to_owned(), fonte.clone(), cor).size().x <= largura;
    if cabe(texto) {
        return texto.to_owned();
    }
    let chars: Vec<char> = texto.chars().collect();
    for fim in (1..chars.len()).rev() {
        let candidato: String = chars[..fim].iter().collect::<String>() + "…";
        if cabe(&candidato) {
            return candidato;
        }
    }
    "…".to_owned()
}
