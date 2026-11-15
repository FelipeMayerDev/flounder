//! Folha de Ajustes (menu ☰): quem usa o celular, orçamentos por categoria, auditoria das
//! ações do assistente, rotina do mês, sobre o app e fechar.

use egui::{Align, Color32, Frame, Layout, Margin, Sense, Ui, Vec2};
use egui_phosphor::regular as icon;
use flounder::backend;
use flounder::widgets as w;
use std::collections::BTreeMap;

use crate::app::{MobileApp, fmt_money};
use crate::mock::MEMBROS;
use crate::screens::home::rotulo_categoria;
use crate::theme::{self, Cores};

/// Fallback quando o espelho real ainda não trouxe os nomes (demo).
const MEMBRO_FALLBACK: [&str; 2] = ["Felipe", "Larissa"];

/// Categorias que aceitam orçamento (despesas de consumo; chaves `Category::key`).
const CATEGORIAS_ORCAMENTO: [&str; 11] = [
    "groceries",
    "transport",
    "utilities",
    "housing",
    "health",
    "leisure",
    "shopping",
    "services",
    "education",
    "pets",
    "other",
];

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
        egui::RichText::new("Quem usa este celular, orçamentos e o assistente.")
            .font(w::regular(13.0))
            .color(c.muted),
    );
    ui.add_space(10.0);

    // As seções novas deixaram a folha comprida: rola quando passa da tela.
    egui::ScrollArea::vertical()
        .auto_shrink([true, true])
        .max_height(ui.ctx().content_rect().height() * 0.62)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            secao_quem(ui, eu, &nomes, &mut pedir, &mut falha, &c);
            separador(ui);
            secao_orcamentos(ui, app, &c);
            separador(ui);
            secao_acoes(ui, app, &c);
            separador(ui);
            secao_rotina(ui, app, &nomes, &c);
            separador(ui);
            secao_sobre(ui, &c);
        });

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

    ui.add_space(8.0);
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

fn separador(ui: &mut Ui) {
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);
}

/// "Quem é você?" — troca de membro, como antes. O toque vai no `pedir`; quem efetiva
/// é a `folha`, fora dos closures (a troca precisa de `&mut app` inteiro).
fn secao_quem(
    ui: &mut Ui,
    eu: usize,
    nomes: &[String; 2],
    pedir: &mut Option<(String, usize)>,
    falha: &mut Option<String>,
    c: &Cores,
) {
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
            *pedir = Some((client.to_owned(), pos));
        }
        ui.add_space(6.0);
    }
    if let Some(nome) = falha {
        ui.label(
            egui::RichText::new(format!(
                "Sem token de {nome} neste dispositivo — use o código de pareamento."
            ))
            .font(w::regular(12.0))
            .color(c.red),
        );
    }
}

/// Orçamentos por categoria: limite mensal com campo de dinheiro e "Salvar"
/// (`Cmd::Budget`; valor vazio/0 remove). Mostra o uso do mês quando há gasto.
fn secao_orcamentos(ui: &mut Ui, app: &mut MobileApp, c: &Cores) {
    ui.label(egui::RichText::new("Orçamentos").font(w::semibold(13.0)).color(c.ink));
    ui.label(
        egui::RichText::new("Limite mensal por categoria. Vazio ou 0 remove.")
            .font(w::regular(12.0))
            .color(c.muted),
    );
    ui.add_space(6.0);

    // Uso do mês (categoria, gasto de quem usa o app, limite), copiado da view-model.
    let usos: Vec<(String, i64, i64)> = app.dados.orcamentos.clone();
    // Buffers de edição por categoria na memória do egui: nascem com o orçamento atual
    // e sobrevivem entre frames (o campo não "pisa" no que está sendo digitado).
    let id_bufs = egui::Id::new("orcamento_bufs");
    let mut bufs: BTreeMap<String, String> = ui.ctx().data_mut(|d| d.get_temp(id_bufs).unwrap_or_default());
    for key in CATEGORIAS_ORCAMENTO {
        let atual = app.budgets.iter().find(|b| b.category == *key).map(|b| b.monthly_cents).unwrap_or(0);
        bufs.entry((*key).to_owned()).or_insert_with(|| {
            if atual > 0 {
                flounder::model::input_money(atual)
            } else {
                String::new()
            }
        });
    }
    let mut salvar: Option<(&str, i64)> = None;
    for key in CATEGORIAS_ORCAMENTO {
        let uso = usos.iter().find(|(cat, _, _)| *cat == *key);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(rotulo_categoria(key)).font(w::semibold(13.0)).color(c.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let largura_salvar = 72.0;
                if ui
                    .add_sized(
                        [largura_salvar, 32.0],
                        egui::Button::new(egui::RichText::new("Salvar").font(w::semibold(12.0)).color(c.ink))
                            .fill(c.line)
                            .corner_radius(10),
                    )
                    .clicked()
                {
                    // Mesmo parser inteiro do resto do app ("1.234,56", "R$ 50"); vazio = 0 = remove.
                    let cents = flounder::model::parse_money(&bufs[key]).unwrap_or(0);
                    salvar = Some((key, cents));
                }
                let largura_campo = ui.available_width();
                // BTreeMap não tem IndexMut: pega o buffer por get_mut (criado acima).
                let buf = bufs.get_mut(key).expect("buffer do orçamento criado acima");
                ui.add_sized(
                    [largura_campo, 32.0],
                    egui::TextEdit::singleline(buf)
                        .hint_text(egui::RichText::new("Sem orçamento").color(c.faint))
                        .font(w::regular(13.0)),
                );
            });
        });
        // Uso do mês: % ao lado do rótulo, nas mesmas cores da Home (vermelho ≥ 100%).
        if let Some((_, gasto, limite)) = uso
            && *gasto > 0
            && *limite > 0
        {
            let pct = (*gasto * 100 / *limite).clamp(0, 999);
            let cor = if pct >= 100 {
                c.red
            } else if pct >= 80 {
                c.orange
            } else {
                c.green
            };
            ui.label(egui::RichText::new(format!("{}% usado", pct)).font(w::regular(11.5)).color(cor));
        }
        ui.add_space(6.0);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id_bufs, bufs));
    if let Some((key, cents)) = salvar {
        // Otimista: espelha aqui; o worker recarrega o estado e o `aplicar` confirma.
        if cents > 0 {
            match app.budgets.iter_mut().find(|b| b.category == key) {
                Some(b) => b.monthly_cents = cents,
                None => app.budgets.push(flounder::api::Budget { category: key.to_owned(), monthly_cents: cents }),
            }
        } else {
            app.budgets.retain(|b| b.category != key);
        }
        app.enviar(backend::Cmd::Budget(flounder::api::Budget { category: key.to_owned(), monthly_cents: cents }));
        app.atualizar_mes();
    }
}

/// Ações do assistente: auditoria do que o GLM executou no app (pede uma vez quando a
/// seção aparece; "Atualizar" repete). Registro: rótulo + dia/hora local, ferramenta miúda.
fn secao_acoes(ui: &mut Ui, app: &mut MobileApp, c: &Cores) {
    ui.label(egui::RichText::new("Ações do assistente").font(w::semibold(13.0)).color(c.ink));
    ui.add_space(6.0);
    if !app.demo && !app.acoes_pedidas {
        app.acoes_pedidas = true;
        app.consultar(backend::Cmd::AssistantActions);
        app.acoes = None;
    }
    let mut atualizar = false;
    match &app.acoes {
        Some(Ok(log)) if log.actions.is_empty() => {
            w::caption(ui, "O assistente ainda não agiu.");
        }
        Some(Ok(log)) => {
            for acao in &log.actions {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&acao.label).font(w::regular(13.0)).color(c.ink));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(hora_local(&acao.at))
                                .font(w::regular(11.5))
                                .color(c.muted),
                        );
                        ui.label(egui::RichText::new(&acao.tool).font(w::regular(11.0)).color(c.faint));
                    });
                });
                ui.add_space(4.0);
            }
        }
        Some(Err(erro)) => {
            ui.label(egui::RichText::new(erro.clone()).font(w::regular(12.0)).color(c.red));
        }
        // Demo não tem backend; na sessão real, aguardando a primeira resposta.
        None if app.demo => {
            w::caption(ui, "O assistente ainda não agiu.");
        }
        None => {
            w::caption(ui, format!("{} Buscando as ações…", icon::HOURGLASS));
        }
    }
    if !app.demo {
        ui.add_space(4.0);
        if ui
            .add_sized(
                [110.0, 30.0],
                egui::Button::new(egui::RichText::new("Atualizar").font(w::semibold(12.0)).color(c.ink))
                    .fill(c.line)
                    .corner_radius(10),
            )
            .clicked()
        {
            atualizar = true;
        }
    }
    if atualizar {
        app.acoes = None;
        app.consultar(backend::Cmd::AssistantActions);
    }
}

/// Rotina do mês: roda no backend (copia os fixos, fecha faturas, sugere acerto) e o
/// relatório JSON vem tolerante — seção só aparece se a chave existir.
fn secao_rotina(ui: &mut Ui, app: &mut MobileApp, nomes: &[String; 2], c: &Cores) {
    ui.label(egui::RichText::new("Rotina do mês").font(w::semibold(13.0)).color(c.ink));
    ui.label(
        egui::RichText::new("Copia os lançamentos fixos do mês passado e sugere acertos.")
            .font(w::regular(12.0))
            .color(c.muted),
    );
    ui.add_space(6.0);
    let esperando = app.rotina_esperando;
    let rotulo = if esperando {
        format!("{} Fechando o mês…", icon::HOURGLASS)
    } else {
        format!("{} Fechar o mês", icon::CALENDAR_CHECK)
    };
    let clicou = !esperando
        && !app.demo
        && ui
            .add_sized(
                [ui.available_width(), 40.0],
                egui::Button::new(
                    egui::RichText::new(rotulo)
                        .font(w::semibold(13.0))
                        .color(if app.demo { c.muted } else { c.blue }),
                )
                .fill(c.blue_soft)
                .corner_radius(12),
            )
            .clicked();
    if clicou {
        app.rotina = None;
        app.rotina_esperando = true;
        // Mês vazio = o mês atual, decidido no backend.
        app.consultar(backend::Cmd::Routine(String::new()));
    }
    if app.demo {
        w::caption(ui, "A rotina roda no backend — só na sessão real.");
    }
    match &app.rotina {
        Some(Ok(relatorio)) => relatorio_rotina(ui, relatorio, nomes, c),
        Some(Err(erro)) => {
            ui.label(egui::RichText::new(erro.clone()).font(w::regular(12.0)).color(c.red));
        }
        None if esperando && !app.demo => {
            w::caption(ui, format!("{} Rodando a rotina no backend…", icon::HOURGLASS));
        }
        None => {}
    }
}

/// Relatório da rotina: N lançamentos copiados (lista breve), faturas do mês
/// (pago/aberto), acerto sugerido, alertas de preço e fechamentos. Cada bloco
/// tolera chave ausente — o relatório é JSON de outra equipe.
fn relatorio_rotina(ui: &mut Ui, v: &serde_json::Value, nomes: &[String; 2], c: &Cores) {
    ui.add_space(8.0);
    // Lançamentos copiados.
    if let Some(lista) = v.get("copiados").and_then(|x| x.as_array()) {
        if lista.is_empty() {
            w::caption(ui, "Nenhum lançamento copiado do mês passado.");
        } else {
            w::caption(ui, format!("{} lançamentos copiados do mês passado:", lista.len()));
            ui.add_space(2.0);
            for item in lista.iter().take(5) {
                let desc = item.get("descricao").and_then(|d| d.as_str()).unwrap_or("lançamento");
                let renda = item.get("tipo").and_then(|k| k.as_str()).is_some_and(|k| k == "income");
                let valor = item.get("valor_centavos").and_then(|x| x.as_i64()).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(desc).font(w::regular(12.5)).color(c.ink));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::money(ui, if renda { valor } else { -valor }, 12.5, c.muted);
                    });
                });
                ui.add_space(2.0);
            }
            if lista.len() > 5 {
                w::caption(ui, format!("+{} outros.", lista.len() - 5));
            }
        }
        ui.add_space(6.0);
    }
    // Faturas do mês: pago/aberto.
    if let Some(lista) = v.get("faturas").and_then(|x| x.as_array()) {
        if !lista.is_empty() {
            w::caption(ui, "Faturas do mês:");
            ui.add_space(2.0);
            for item in lista {
                let label = item.get("label").and_then(|x| x.as_str()).unwrap_or("fatura");
                let total = item.get("total_cents").and_then(|x| x.as_i64()).unwrap_or(0);
                let paga = item.get("paid").and_then(|x| x.as_bool()).unwrap_or(false);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label).font(w::regular(12.5)).color(c.ink));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(if paga { "paga" } else { "aberta" })
                                .font(w::semibold(11.5))
                                .color(if paga { c.green } else { c.orange }),
                        );
                        theme::money(ui, total, 12.5, c.muted);
                    });
                });
                ui.add_space(2.0);
            }
            ui.add_space(6.0);
        }
    }
    // Acerto sugerido: "R$ X de {nome} para {outro}" (nomes do espelho, fallback do mock).
    if let Some(a) = v.get("acerto_sugerido") {
        if a.is_null() {
            w::caption(ui, "Nenhum acerto sugerido este mês.");
        } else if let (Some(valor), Some(de)) = (
            a.get("valor_cents").and_then(|x| x.as_i64()),
            a.get("de").and_then(|x| x.as_i64()),
        ) {
            let de = de.clamp(0, 1) as usize;
            ui.label(
                egui::RichText::new(format!(
                    "Acerto sugerido: R$ {} de {} para {}.",
                    fmt_money(valor),
                    nomes[de],
                    nomes[1 - de]
                ))
                .font(w::regular(12.5))
                .color(c.ink),
            );
        }
        ui.add_space(6.0);
    }
    // Alertas de preço da rotina.
    if let Some(lista) = v.get("alertas_preco").and_then(|x| x.as_array()) {
        if !lista.is_empty() {
            w::caption(ui, "Contas fora da média:");
            ui.add_space(2.0);
            for item in lista {
                let desc = item.get("desc").and_then(|x| x.as_str()).unwrap_or("conta");
                let recente = item.get("recent_cents").and_then(|x| x.as_i64()).unwrap_or(0);
                let media = item.get("average_cents").and_then(|x| x.as_i64()).unwrap_or(0);
                ui.label(
                    egui::RichText::new(format!("{desc}: R$ {} (média R$ {})", fmt_money(recente), fmt_money(media)))
                        .font(w::regular(12.5))
                        .color(c.orange),
                );
                ui.add_space(2.0);
            }
            ui.add_space(6.0);
        }
    }
    // Fechamentos próximos.
    if let Some(lista) = v.get("fechamentos").and_then(|x| x.as_array()) {
        if !lista.is_empty() {
            w::caption(ui, "Fechamentos:");
            ui.add_space(2.0);
            for item in lista {
                let banco = item.get("nome").and_then(|x| x.as_str()).unwrap_or("cartão");
                let dias = item.get("dias").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let prazo = match dias {
                    0 => "fecha hoje".to_owned(),
                    1 => "fecha amanhã".to_owned(),
                    n => format!("fecha em {n} dias"),
                };
                ui.label(
                    egui::RichText::new(format!("{banco} {prazo}."))
                        .font(w::regular(12.5))
                        .color(c.ink),
                );
                ui.add_space(2.0);
            }
        }
    }
}

fn secao_sobre(ui: &mut Ui, c: &Cores) {
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

/// "2026-11-14T17:30:00Z" → "14/11 17:30" (hora local do núcleo); fora do formato, o cru.
fn hora_local(iso: &str) -> String {
    match flounder::model::local_time(iso) {
        Some((data, hh_mm)) => format!("{} {}", data.short(), hh_mm),
        None => iso.to_owned(),
    }
}
