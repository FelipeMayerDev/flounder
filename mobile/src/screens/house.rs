//! Tela Casa: contas da casa em meio a meio — filtro, resumo e lista com bolinha de pago,
//! e a seção de acertos (saldo entre o casal, adiantamentos do mês e registro de novo acerto).

use egui::{Align, Align2, Color32, Frame, Layout, Margin, Sense, Stroke, Ui, Vec2};
use egui_phosphor::regular as icon;
use flounder::widgets as w;

use crate::app::{Filtro, MobileApp};
use crate::mock::{self, MEMBROS};
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
                // Total inteiro à direita; a nota trunca com "…" em vez de quebrar a linha feio.
                theme::money(ui, total, 13.0, c.ink);
                ui.add(egui::Label::new(egui::RichText::new(nota).font(w::regular(13.0)).color(c.muted)).truncate());
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

    secao_acertos(ui, app);
}

// ─── Acertos ─────────────────────────────────────────────────────────────────────

/// Estado do mini-formulário de registro (memória do egui: sobrevive entre frames).
#[derive(Clone, Default)]
struct EstadoAcerto {
    /// Formulário aberto no lugar do botão "Registrar acerto".
    aberto: bool,
    /// Valor digitado ("150,00").
    valor: String,
    /// Quem pagou a mais (0 ou 1).
    de: usize,
    /// Tentou salvar com valor inválido (mostra o erro).
    tried: bool,
}

/// Card "Acertos": saldo acumulado entre o casal, adiantamentos do mês e o registro.
fn secao_acertos(ui: &mut Ui, app: &mut MobileApp) {
    let c = theme::cores();
    let id_estado = egui::Id::new("casa_acertos");
    let mut est = ui.ctx().data_mut(|d| d.get_temp::<EstadoAcerto>(id_estado).unwrap_or_default());
    let nomes = nomes_membros(app);
    // Ações marcadas dentro dos closures (precisam de &mut app) e efetivadas depois.
    let mut acao_salvar: Option<(i64, usize)> = None;
    let mut acao_desfazer: Option<String> = None;

    theme::card(ui, |ui| {
        ui.label(egui::RichText::new("Acertos").font(w::bold(16.0)).color(c.ink));
        ui.label(
            egui::RichText::new("Adiantamentos para equilibrar as contas")
                .font(w::regular(13.0))
                .color(c.muted),
        );
        ui.add_space(12.0);

        saldo_acumulado(ui, app.dados.casa.saldo_acerto, &nomes);
        ui.add_space(12.0);

        // Adiantamentos registrados no mês aberto (o converter/mock já entrega só o mês).
        let total = app.dados.casa.acertos.len();
        if total == 0 {
            if app.dados.casa.saldo_acerto != 0 {
                ui.label(
                    egui::RichText::new("Nenhum acerto registrado neste mês.")
                        .font(w::regular(12.0))
                        .color(c.muted),
                );
            }
        } else {
            for n in 0..total {
                // Copia os campos para soltar o empréstimo antes do possível clique.
                let (id, de, valor, dia, mes) = {
                    let a = &app.dados.casa.acertos[n];
                    (a.id.clone(), a.de, a.valor_cents, a.dia, a.mes)
                };
                if linha_acerto(ui, app.eu, de, valor, dia, mes, &nomes) {
                    acao_desfazer = Some(id);
                }
                if n != total - 1 {
                    divisoria(ui);
                }
            }
        }
        ui.add_space(12.0);

        if est.aberto {
            // Mini-formulário: quem pagou a mais + valor + salvar/cancelar.
            ui.label(egui::RichText::new("Quem pagou a mais?").font(w::semibold(12.0)).color(c.muted));
            ui.add_space(4.0);
            Frame::new()
                .fill(c.line)
                .corner_radius(13)
                .inner_margin(Margin::same(3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let largura = ui.available_width() / 2.0;
                        for (pos, nome) in [(0usize, nomes[0].clone()), (1usize, nomes[1].clone())] {
                            let on = est.de == pos;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(largura, 32.0), Sense::click());
                            if on {
                                // Pílula branca com sombrinha discreta (como o filtro de contas).
                                ui.painter().rect_filled(rect.translate(Vec2::new(0.0, 1.0)), 11.0, Color32::from_rgba_unmultiplied(23, 43, 99, 30));
                                ui.painter().rect_filled(rect, 11.0, Color32::WHITE);
                            }
                            let cor = if on { c.ink } else { c.muted };
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, nome, w::semibold(13.0), cor);
                            if resp.clicked() {
                                est.de = pos;
                            }
                        }
                    });
                });
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Valor (R$)").font(w::semibold(12.0)).color(c.muted));
            w::money_input(ui, &mut est.valor);
            ui.add_space(4.0);
            if est.tried {
                ui.label(
                    egui::RichText::new("Informe um valor maior que zero.")
                        .font(w::regular(12.0))
                        .color(c.red),
                );
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Duas metades iguais com 10 px de respiro (o espaçamento é contado na largura).
                ui.spacing_mut().item_spacing.x = 10.0;
                let metade = (ui.available_width() - 10.0) / 2.0;
                if ui
                    .add_sized(
                        [metade, 40.0],
                        egui::Button::new(egui::RichText::new("Cancelar").font(w::semibold(14.0)).color(c.ink))
                            .fill(c.line)
                            .corner_radius(12),
                    )
                    .clicked()
                {
                    est.aberto = false;
                    est.valor.clear();
                    est.tried = false;
                }
                if ui
                    .add_sized(
                        [ui.available_width(), 40.0],
                        egui::Button::new(egui::RichText::new("Salvar").font(w::bold(14.0)).color(Color32::WHITE))
                            .fill(c.blue)
                            .corner_radius(12),
                    )
                    .clicked()
                {
                    // Mesmo parse do lançamento: "1.234,56" → 123456 centavos.
                    let bruto = est.valor.trim().replace('.', "").replace(',', ".");
                    let centavos = (bruto.parse::<f64>().unwrap_or(0.0) * 100.0).round() as i64;
                    if centavos > 0 {
                        acao_salvar = Some((centavos, est.de));
                        est.aberto = false;
                        est.valor.clear();
                        est.tried = false;
                    } else {
                        est.tried = true;
                    }
                }
            });
        } else if botao_registrar(ui) {
            est.aberto = true;
            est.tried = false;
        }
    });

    // Efetiva as ações e grava o estado do formulário para o próximo frame.
    if let Some((valor, de)) = acao_salvar {
        registrar_acerto(app, valor, de);
    }
    if let Some(id) = acao_desfazer {
        desfazer_acerto(app, &id);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id_estado, est));
}

/// Saldo acumulado entre o casal: caixinha com a frase "quem deve o quê".
/// > 0: {names[1]} deve para {names[0]}; < 0 o contrário; 0: em dia.
fn saldo_acumulado(ui: &mut Ui, saldo: i64, nomes: &[String; 2]) {
    let c = theme::cores();
    let fundo = if saldo == 0 { c.green_soft } else { c.blue_soft };
    Frame::new()
        .fill(fundo)
        .corner_radius(12)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            if saldo == 0 {
                ui.label(
                    egui::RichText::new("Ninguém deve nada — em dia 🐟")
                        .font(w::semibold(14.0))
                        .color(c.green),
                );
            } else {
                let (devedor, credor, valor) = if saldo > 0 {
                    (&nomes[1], &nomes[0], saldo)
                } else {
                    (&nomes[0], &nomes[1], -saldo)
                };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(format!("{devedor} deve")).font(w::semibold(14.0)).color(c.ink))
                            .truncate(),
                    );
                    theme::money(ui, valor, 14.0, c.ink);
                    ui.add(
                        egui::Label::new(egui::RichText::new(format!("para {credor}")).font(w::semibold(14.0)).color(c.ink))
                            .truncate(),
                    );
                });
            }
        });
}

/// Linha "Felipe adiantou R$ 50,00 · 14/11" com o botão de desfazer à direita.
/// Devolve true quando o desfazer foi tocado.
fn linha_acerto(ui: &mut Ui, eu: usize, de: usize, valor: i64, dia: u8, mes: u8, nomes: &[String; 2]) -> bool {
    let c = theme::cores();
    let nome = &nomes[de];
    let rotulo = if de == eu { "Você adiantou".to_owned() } else { format!("{nome} adiantou") };
    let (nome_membro, cor) = MEMBROS[de];
    let inicial = nome_membro.chars().next().unwrap();
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
        theme::avatar(ui, rect.center(), 10.0, &inicial.to_string(), cor);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let desfazer = botao_desfazer(ui);
            ui.label(egui::RichText::new(format!("· {dia}/{mes}")).font(w::regular(12.0)).color(c.faint));
            theme::money(ui, valor, 14.0, c.ink);
            ui.add(egui::Label::new(egui::RichText::new(rotulo).font(w::semibold(14.0)).color(c.ink)).truncate());
            desfazer
        })
        .inner
    })
    .inner
}

/// Botão circular de desfazer (↺): cinza, vermelho ao passar o mouse.
fn botao_desfazer(ui: &mut Ui) -> bool {
    let c = theme::cores();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8.0, c.line);
    }
    let cor = if resp.hovered() { c.red } else { c.muted };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon::ARROW_COUNTER_CLOCKWISE.to_string(),
        w::semibold(15.0),
        cor,
    );
    resp.clicked()
}

/// Botão "Registrar acerto": pílula branca com borda fina (como "Editar lista").
fn botao_registrar(ui: &mut Ui) -> bool {
    let c = theme::cores();
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, c.line))
        .corner_radius(255)
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{}  Registrar acerto", icon::PLUS))
                    .font(w::semibold(13.0))
                    .color(c.ink),
            );
        })
        .response
        .interact(Sense::click())
        .clicked()
}

/// Nomes dos membros: espelho real (`data.names`) com fallback para os do mock (demo).
fn nomes_membros(app: &MobileApp) -> [String; 2] {
    let mut nomes = [String::new(), String::new()];
    for i in 0..2 {
        let nome = app.data.names[i].trim();
        nomes[i] = if nome.is_empty() { MEMBROS[i].0.to_owned() } else { app.data.names[i].clone() };
    }
    nomes
}

/// Registra o acerto. Na demonstração grava direto na view-model (o `atualizar_mes` do
/// método recriaria o mock e descartaria o acerto — mesmo padrão do clique na bolinha de
/// pago). Na sessão real, grava no `data` e no backend via `MobileApp::registrar_acerto`.
fn registrar_acerto(app: &mut MobileApp, valor_cents: i64, de: usize) {
    if app.demo {
        app.dados.casa.saldo_acerto += if de == 0 { valor_cents } else { -valor_cents };
        app.dados.casa.acertos.push(mock::Acerto {
            id: format!("local-demo-{}", app.dados.casa.acertos.len()),
            de,
            valor_cents,
            dia: flounder::model::today().d as u8,
            mes: app.m,
            ano: app.y,
        });
        return;
    }
    app.registrar_acerto(valor_cents, de);
}

/// Desfaz um acerto pelo id. Na demonstração devolve o saldo e tira só o acerto da lista
/// (o método recriaria o mock e zeraria a lista inteira). Na real, `MobileApp::desfazer_acerto`.
fn desfazer_acerto(app: &mut MobileApp, id: &str) {
    if app.demo {
        if let Some(a) = app.dados.casa.acertos.iter().find(|a| a.id == id) {
            app.dados.casa.saldo_acerto -= if a.de == 0 { a.valor_cents } else { -a.valor_cents };
        }
        app.dados.casa.acertos.retain(|a| a.id != id);
        return;
    }
    app.desfazer_acerto(id.to_owned());
}

// ─── Contas da casa (como antes) ─────────────────────────────────────────────────

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
            // O valor fica sempre inteiro à direita; o nome encolhe com "…" em nomes longos.
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::money(ui, total, 14.0, c.ink);
                    if aprox {
                        ui.label(egui::RichText::new("≈").font(w::semibold(13.0)).color(c.muted));
                    }
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&nome)
                                .font(w::semibold(14.0))
                                .color(if paga { c.muted } else { c.ink }),
                        )
                        .truncate(),
                    );
                });
            });
            ui.add_space(3.0);
            // Linha 2: dot do pagador, "quem · dia" e a sua parte à direita.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                theme::avatar(ui, rect.center(), 9.0, &inicial.to_string(), cor_pagador);
                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::money(ui, parte, 12.0, c.muted);
                    ui.label(egui::RichText::new("sua parte").font(w::regular(12.0)).color(c.faint));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{nome_pagador} · dia {dia}"))
                                .font(w::regular(12.0))
                                .color(c.muted),
                        )
                        .truncate(),
                    );
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
