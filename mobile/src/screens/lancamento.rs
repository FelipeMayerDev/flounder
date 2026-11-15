//! Folha de novo/editar lançamento: tipo, descrição, valor, dia, categoria, cartão
//! conectado, compartilhada, paga e excluir. Salva via `MobileApp::salvar_entrada`;
//! `folha_editar` abre a edição a partir de uma entrada do Fluxo.

use egui::Color32;
use egui_phosphor::regular as icon;
use flounder::model::{Category, Entry, Kind};
use flounder::widgets as w;

use crate::app::MobileApp;
use crate::theme;

/// Estado da folha de lançamento (novo ou editando uma entrada existente).
#[derive(Clone, Default)]
pub struct Folha {
    /// Id da entrada em edição; 0 = nova.
    pub id: u32,
    pub tipo_despesa: bool,
    pub desc: String,
    pub valor: String,
    pub dia: String,
    pub categoria: Option<Category>,
    pub shared: bool,
    pub paga: bool,
    /// Compra num cartão conectado (`ConnectedCard.id`): o pagamento é a fatura.
    pub connected_id: Option<String>,
    /// Cartão manual do desktop (`Data::cards`), preservado na edição (a folha não mexe nele).
    pub card_manual: Option<usize>,
    /// Quem lançou/pagou; `None` = novo lançamento (quem usa o celular).
    pub owner: Option<flounder::model::Person>,
    /// Veio da importação do banco; preservado na edição (a folha não mexe nele).
    pub imported: bool,
    /// Conta fixa que repete todo mês; preservado na edição (a folha não mexe nele).
    pub recurring: bool,
    pub tried: bool,
}

/// Abre a folha preenchida com os campos de uma entrada existente (toque na linha do Fluxo).
pub fn folha_editar(app: &mut MobileApp, id: u32) {
    let folha = if app.demo {
        // Mock: procura o dia junto (o mock guarda o dia no DiaFluxo, não na entrada).
        let Some((dia, e)) = app.dados.fluxo.iter().find_map(|d| {
            d.entradas.iter().find(|e| e.id == id).map(|e| (d.dia, e))
        }) else {
            return;
        };
        Folha {
            id: e.id,
            // Centavos com sinal: receita entra positivo, despesa negativo.
            tipo_despesa: e.centavos < 0,
            desc: e.desc.clone(),
            valor: crate::app::fmt_money(e.centavos.abs()),
            dia: dia.to_string(),
            // O mock não carrega categoria: a edição começa em Outros.
            categoria: Some(Category::Other),
            shared: e.shared,
            paga: e.paid,
            connected_id: cartao_conectado(app, e.card.as_deref()),
            card_manual: None,
            owner: Some(if e.membro == 1 { flounder::model::Person::B } else { flounder::model::Person::A }),
            imported: false,
            recurring: false,
            tried: false,
        }
    } else {
        let Some(e) = app.data.entry(id) else { return };
        Folha {
            id: e.id,
            tipo_despesa: e.kind == Kind::Expense,
            desc: e.desc.clone(),
            valor: crate::app::fmt_money(e.amount),
            dia: e.date.d.to_string(),
            categoria: Some(e.category),
            shared: e.shared,
            paga: e.paid,
            connected_id: e.connected.clone(),
            card_manual: e.card,
            owner: Some(e.owner),
            imported: e.imported,
            recurring: e.recurring,
            tried: false,
        }
    };
    app.sheet = Some(crate::app::Sheet::Lancamento(folha));
}

/// Id do cartão conectado pela instituição (o mock guarda só o nome do banco).
fn cartao_conectado(app: &MobileApp, instituicao: Option<&str>) -> Option<String> {
    let instituicao = instituicao?;
    app.connected_cards
        .as_ref()?
        .iter()
        .find(|c| c.institution == instituicao)
        .map(|c| c.id.clone())
}

pub fn folha(ui: &mut egui::Ui, app: &mut MobileApp, folha: &Folha) {
    let c = theme::cores();
    ui.label(
        egui::RichText::new(if folha.id == 0 { "Novo lançamento" } else { "Editar lançamento" })
            .font(w::bold(17.0))
            .color(c.ink),
    );
    ui.label(
        egui::RichText::new(format!("Entra no fluxo de {}", app.mes_label()))
            .font(w::regular(13.0))
            .color(c.muted),
    );
    ui.add_space(10.0);

    // Tipo: despesa ou receita.
    let mut tipo_despesa = folha.tipo_despesa;
    let mut categoria = folha.categoria;
    let mut shared = folha.shared;
    let mut paga = folha.paga;
    let mut connected_id = folha.connected_id.clone();
    ui.horizontal(|ui| {
        for (despesa, rotulo, cor) in [
            (true, format!("{} Despesa", icon::ARROW_UP_RIGHT), c.red),
            (false, format!("{} Receita", icon::ARROW_DOWN_LEFT), c.green),
        ] {
            let on = tipo_despesa == despesa;
            if ui
                .add_sized(
                    [ui.available_width() / 2.0 - 4.0, 36.0],
                    egui::Button::new(
                        egui::RichText::new(rotulo)
                            .font(w::semibold(13.0))
                            .color(if on { Color32::WHITE } else { c.muted }),
                    )
                    .fill(if on { cor } else { c.line })
                    .corner_radius(12),
                )
                .clicked()
            {
                tipo_despesa = despesa;
                if !despesa {
                    // Receita não é compra: nada de cartão.
                    connected_id = None;
                }
            }
        }
    });
    ui.add_space(10.0);

    let mut desc = folha.desc.clone();
    let mut valor = folha.valor.clone();
    let mut dia = folha.dia.clone();
    campo(ui, "Descrição", &mut desc, "Ex.: Conta de luz");
    campo(ui, "Valor (R$)", &mut valor, "0,00");
    campo(ui, "Dia do mês", &mut dia, "1-31");
    if let Some(crate::app::Sheet::Lancamento(f)) = app.sheet.as_mut() {
        f.desc = desc.clone();
        f.valor = valor.clone();
        f.dia = dia.clone();
        f.categoria = folha.categoria;
        f.shared = folha.shared;
        f.paga = folha.paga;
    }

    // Categoria (só despesa): chips em grade.
    if folha.tipo_despesa {
        ui.label(egui::RichText::new("Categoria").font(w::semibold(12.0)).color(c.muted));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.spacing_mut().item_spacing.y = 4.0;
            for cat in Category::ALL {
                let on = categoria == Some(cat) || (categoria.is_none() && cat == Category::Other);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(cat.label())
                                .font(w::semibold(11.5))
                                .color(if on { Color32::WHITE } else { c.ink }),
                        )
                        .fill(if on { c.blue } else { c.line })
                        .corner_radius(255),
                    )
                    .clicked()
                {
                    categoria = Some(cat);
                }
            }
        });
        ui.add_space(8.0);
        ui.checkbox(&mut shared, "Despesa da casa (compartilhada)");
        // Compra no cartão conectado (despesa pessoal): chips "À vista" + os cartões.
        if !shared {
            let cartoes: Vec<(&str, &str)> = app
                .connected_cards
                .iter()
                .flatten()
                .map(|k| (k.institution.as_str(), k.id.as_str()))
                .collect();
            if !cartoes.is_empty() {
                ui.label(egui::RichText::new("No cartão").font(w::semibold(12.0)).color(c.muted));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for (rotulo, id) in std::iter::once(("À vista", None)).chain(cartoes.iter().map(|(b, i)| (*b, Some(*i)))) {
                        let on = connected_id.as_deref() == id;
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(rotulo)
                                        .font(w::semibold(11.5))
                                        .color(if on { Color32::WHITE } else { c.ink }),
                                )
                                .fill(if on { c.blue } else { c.line })
                                .corner_radius(255),
                            )
                            .clicked()
                        {
                            connected_id = id.map(str::to_owned);
                        }
                    }
                });
            }
        }
        ui.add_space(6.0);
        if connected_id.is_some() {
            // Compra de cartão: quem paga é a fatura, não a compra.
            ui.label(
                egui::RichText::new(format!("{} Compra no cartão: o pagamento é feito pela fatura.", icon::CREDIT_CARD))
                    .font(w::regular(12.0))
                    .color(c.muted),
            );
        } else {
            ui.checkbox(&mut paga, "Já está paga");
        }
    }
    if let Some(crate::app::Sheet::Lancamento(f)) = app.sheet.as_mut() {
        f.tipo_despesa = tipo_despesa;
        f.categoria = categoria;
        f.shared = shared;
        f.paga = paga;
        f.connected_id = connected_id.clone();
        f.card_manual = folha.card_manual;
    }
    if folha.tried {
        ui.label(egui::RichText::new("Preencha descrição, valor e dia válidos.").font(w::regular(12.0)).color(c.red));
    }
    ui.add_space(14.0);
    let excluir = folha.id != 0;
    ui.horizontal(|ui| {
        let partes = if excluir { 3.0 } else { 2.0 };
        if ui
            .add_sized(
                [ui.available_width() / partes - 5.0, 44.0],
                egui::Button::new(egui::RichText::new("Cancelar").font(w::semibold(15.0)).color(c.ink)).fill(c.line).corner_radius(14),
            )
            .clicked()
        {
            app.sheet = None;
        }
        if excluir {
            let largura = ui.available_width() / 2.0 - 5.0;
            if ui
                .add_sized(
                    [largura, 44.0],
                    egui::Button::new(
                        egui::RichText::new(format!("{} Excluir", icon::TRASH)).font(w::semibold(15.0)).color(c.red),
                    )
                    .fill(c.red_soft)
                    .corner_radius(14),
                )
                .clicked()
            {
                let id = folha.id;
                app.excluir_entrada(id);
            }
        }
        let largura_salvar = ui.available_width();
        if ui
            .add_sized(
                [largura_salvar, 44.0],
                egui::Button::new(egui::RichText::new("Salvar").font(w::bold(15.0)).color(Color32::WHITE)).fill(c.blue).corner_radius(14),
            )
            .clicked()
        {
            salvar(app);
        }
    });
}

fn campo(ui: &mut egui::Ui, rotulo: &str, buf: &mut String, dica: &str) {
    let c = theme::cores();
    ui.label(egui::RichText::new(rotulo).font(w::semibold(12.0)).color(c.muted));
    ui.add_sized(
        [ui.available_width(), 42.0],
        egui::TextEdit::singleline(buf).hint_text(egui::RichText::new(dica).color(c.faint)).font(w::regular(15.0)),
    );
    ui.add_space(6.0);
}

/// Monta a `Entry` do estado da folha e salva (novo: upsert atribui id; edição: mantém).
fn salvar(app: &mut MobileApp) {
    let Some(folha) = app.sheet.as_ref().and_then(|s| match s {
        crate::app::Sheet::Lancamento(f) => Some(f.clone()),
        _ => None,
    }) else {
        return;
    };
    // Mesmo parser inteiro do desktop: "1.234,56", "1234.56", "R$ 50". Valor inválido
    // vira zero e cai na validação de baixo, como antes.
    let centavos = flounder::model::parse_money(&folha.valor).unwrap_or(0);
    let dia: u32 = folha.dia.trim().parse().unwrap_or(0);
    if folha.desc.trim().is_empty() || centavos == 0 || dia == 0 || dia > 31 {
        if let Some(crate::app::Sheet::Lancamento(f)) = app.sheet.as_mut() {
            f.tried = true;
        }
        return;
    }
    let owner = folha
        .owner
        .unwrap_or_else(|| if app.eu == 1 { flounder::model::Person::B } else { flounder::model::Person::A });
    // Compra no cartão conectado (só despesa): entra na fatura, nunca "paga" direto.
    let connected = if folha.tipo_despesa { folha.connected_id.clone() } else { None };
    // Conta fixa não convive com compra de cartão (o pagamento é a fatura).
    let fixa = folha.recurring && folha.card_manual.is_none() && connected.is_none();
    let entrada = Entry {
        id: folha.id,
        desc: folha.desc.trim().to_owned(),
        amount: centavos,
        date: flounder::model::Date { y: app.y, m: app.m as u32, d: dia },
        kind: if folha.tipo_despesa { Kind::Expense } else { Kind::Income },
        category: folha.categoria.unwrap_or(Category::Other),
        owner,
        shared: folha.tipo_despesa && folha.shared,
        paid: if connected.is_some() { false } else { folha.paga },
        card: if folha.tipo_despesa { folha.card_manual } else { None },
        connected,
        imported: folha.imported,
        recurring: fixa,
    };
    app.sheet = None;
    app.salvar_entrada(entrada);
    app.tab = crate::app::Tab::Fluxo;
}
