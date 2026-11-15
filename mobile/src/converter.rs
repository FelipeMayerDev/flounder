//! Converte o estado real (`flounder::model::Data`) na view-model das telas (`mock::Mes`).
//! Mesmas contas do desktop: `Data::summary` + rateio `shared_split` + faturas dos cartões.

use flounder::connected::ConnectedCard;
use flounder::model::{Category, Data, Kind, Month};
use crate::mock::{self, ContaCasa, Cartao, DiaFluxo, Entrada};

/// Categorias de consumo corrente ("Dia a dia" da repartição).
const DIA_A_DIA: [Category; 9] = [
    Category::Groceries,
    Category::Transport,
    Category::Health,
    Category::Leisure,
    Category::Shopping,
    Category::Services,
    Category::Education,
    Category::Pets,
    Category::Other,
];

pub fn mes(
    data: &Data,
    connected_cards: Option<&[ConnectedCard]>,
    month: Month,
    eu: usize,
    budgets: &[flounder::api::Budget],
) -> mock::Mes {
    let hoje = flounder::model::today();
    let s = data.summary(month, flounder::model::Mode::Couple);

    // Totais do mês pela mesma regra do desktop (`model::cash_flow`).
    let (entradas_cents, saidas_cents) = flounder::model::cash_flow(data.in_month(month));

    // Repartição: parte da casa, contas pessoais, dia a dia (consumo corrente) e sobra.
    let pct = |v: i64| s.pct(v).round() as u8;
    let dia_a_dia: i64 = s
        .by_category
        .iter()
        .filter(|(cat, _)| DIA_A_DIA.contains(cat))
        .map(|(_, v)| *v)
        .sum();
    let reparticao = vec![
        mock::Reparte { label: "Sua parte da casa".into(), centavos: s.shared_part, pct: pct(s.shared_part), cor: egui::Color32::from_rgb(23, 61, 156) },
        mock::Reparte { label: "Contas pessoais".into(), centavos: s.personal, pct: pct(s.personal), cor: egui::Color32::from_rgb(47, 108, 255) },
        mock::Reparte { label: "Dia a dia".into(), centavos: dia_a_dia, pct: pct(dia_a_dia), cor: egui::Color32::from_rgb(143, 176, 255) },
        mock::Reparte { label: "Sobra".into(), centavos: s.balance, pct: pct(s.balance), cor: egui::Color32::from_rgb(18, 183, 106) },
    ];

    // Contas a pagar: bills do mês (entradas + faturas) agrupadas por dia, só as não pagas.
    let mut dias: Vec<(u8, u8, i64)> = Vec::new();
    let mut contas: Vec<mock::ContaPagar> = Vec::new();
    for bill in &s.bills {
        let acao = match &bill.target {
            flounder::model::BillRef::Entry(id) => mock::Acao::Entrada(*id),
            flounder::model::BillRef::Connected(i) => data
                .connected_invoices
                .get(*i)
                .map(|inv| mock::Acao::Fatura { card: inv.card.clone(), due: format!("{:04}-{:02}-{:02}", inv.due.y, inv.due.m, inv.due.d) })
                .unwrap_or(mock::Acao::Nenhuma),
            _ => mock::Acao::Nenhuma,
        };
        contas.push(mock::ContaPagar {
            dia: bill.date.d as u8,
            label: bill.label.clone(),
            total_cents: bill.amount,
            paga: bill.status == flounder::model::Status::Paid,
            acao,
        });
        if bill.status == flounder::model::Status::Paid {
            continue;
        }
        match dias.iter_mut().find(|(d, _, _)| *d == bill.date.d as u8) {
            Some((_, qtd, total)) => {
                *qtd += 1;
                *total += bill.amount;
            }
            None => dias.push((bill.date.d as u8, 1, bill.amount)),
        }
    }
    dias.sort_by_key(|(d, _, _)| *d);
    contas.sort_by_key(|c| (c.dia, std::cmp::Reverse(c.total_cents)));
    let pagar = mock::Pagar {
        dia: dias.first().map(|(d, _, _)| *d).unwrap_or(1),
        total_cents: s.to_pay,
        contas,
        dias,
    };

    // Cartões conectados: fatura do mês (ou saldo), compras da fatura, o que já cai na
    // seguinte (fatura em formação) e limite livre.
    let prefixo = format!("{:04}-{:02}-", month.y, month.m);
    let mes_nome = app_mes_nome(month).to_lowercase();
    let cards: Vec<Cartao> = connected_cards
        .unwrap_or(&[])
        .iter()
        .map(|c| {
            let bill = c.bills.iter().find(|b| b.due.starts_with(&prefixo));
            let vence_dia = bill
                .and_then(|b| flounder::model::Date::iso(&b.due))
                .or_else(|| c.due.as_deref().and_then(flounder::model::Date::iso))
                .map(|d| d.d as u8)
                .unwrap_or(1);
            let paga = bill.is_some_and(|b| b.paid);
            let aberta = bill.is_some_and(|b| !b.paid);
            let fechamento_dia = c.closing_day.filter(|d| (1..=31).contains(d)).map(|d| d as u8);
            let fecha_dias = fecha_em_dias(c.closing_day, hoje);
            // Compras da fatura que vence no mês (pela janela do fechamento, não o mês civil).
            let compras = data.connected_purchases(month, &c.id, c.closing_day, c.due_day).count() as u32;
            // Compras no/do fechamento em diante: já estão na fatura em formação, que fecha
            // no mês seguinte (a que vence depois — ver `next_invoice_purchases`).
            let mut proximo_qtd = 0u32;
            let mut proximo_total_cents = 0i64;
            for e in data.next_invoice_purchases(month, &c.id, c.closing_day) {
                proximo_qtd += 1;
                proximo_total_cents += if e.kind == Kind::Income { -e.amount } else { e.amount };
            }
            // Próximos vencimentos ("dez 26", jan 27…): só os meses com total, do mês seguinte em diante.
            let proximas = data
                .next_invoices(&c.id, &c.bills, c.closing_day, c.due_day, month, 3)
                .into_iter()
                .filter(|(_, total)| *total != 0)
                .map(|(m, total)| (m.short().to_lowercase(), total))
                .collect();
            Cartao {
                banco: c.institution.clone(),
                cor: egui::Color32::from_rgb(c.color[0], c.color[1], c.color[2]),
                aberta,
                fecha_dias,
                fechamento_dia,
                vence_dia,
                fatura_label: if bill.is_some() { format!("Fatura de {mes_nome}") } else { "Saldo utilizado".into() },
                total_cents: bill.and_then(|b| b.total).or(c.balance).unwrap_or(0),
                compras,
                proximo_total_cents,
                proximo_qtd,
                proximas,
                disponivel_cents: c.available.unwrap_or(0),
                limite_cents: c.limit.unwrap_or(0),
                paga,
            }
        })
        .collect();
    let cartoes = mock::Cartoes {
        total_faturas_cents: cards
            .iter()
            .map(|c| {
                connected_cards
                    .unwrap_or(&[])
                    .iter()
                    .find(|cc| cc.institution == c.banco)
                    .and_then(|cc| cc.bills.iter().find(|b| b.due.starts_with(&prefixo)))
                    .and_then(|b| b.total)
                    .unwrap_or(0)
            })
            .sum(),
        cards,
    };

    // Casa: compartilhadas do mês, quem pagou e a parte de quem usa o celular.
    let mut casa_bills: Vec<ContaCasa> = data
        .in_month(month)
        .filter(|e| e.shared && e.kind == Kind::Expense)
        .map(|e| ContaCasa {
            id: e.id,
            nome: e.desc.clone(),
            dia: e.date.d as u8,
            total_cents: e.amount,
            aproximado: false,
            pagador: e.owner.idx(),
            parte_cents: data.shared_split(e)[eu],
            paga: e.paid,
        })
        .collect();
    casa_bills.sort_by_key(|b| (b.dia, std::cmp::Reverse(b.total_cents)));
    let casa = mock::Casa {
        total_contas: casa_bills.len() as u8,
        pendentes_qtd: casa_bills.iter().filter(|b| !b.paga).count() as u8,
        pendentes_total_cents: casa_bills.iter().filter(|b| !b.paga).map(|b| b.total_cents).sum(),
        saldo_acerto: s.settlement,
        acertos: data
            .settlements
            .iter()
            .filter(|a| flounder::model::Month::of(a.date) == month)
            .map(|a| mock::Acerto {
                id: a.id.clone(),
                de: a.from.idx(),
                valor_cents: a.amount,
                dia: a.date.d as u8,
                mes: a.date.m as u8,
                ano: a.date.y,
            })
            .collect(),
        bills: casa_bills,
    };

    // Fluxo por dia, com o nome do cartão resolvido pela instituição.
    let mut fluxo: Vec<DiaFluxo> = Vec::new();
    let mut entradas_do_mes: Vec<_> = data.in_month(month).collect::<Vec<_>>();
    entradas_do_mes.sort_by_key(|e| e.date);
    for e in entradas_do_mes {
        let centavos = if e.kind == Kind::Income { e.amount } else { -e.amount };
        let card = e
            .connected
            .as_deref()
            .and_then(|id| connected_cards.unwrap_or(&[]).iter().find(|c| c.id == *id))
            .map(|c| c.institution.clone());
        let entrada = Entrada {
            id: e.id,
            desc: e.desc.clone(),
            centavos,
            membro: e.owner.idx(),
            shared: e.shared,
            card,
            paid: e.paid,
        };
        match fluxo.iter_mut().find(|d| d.dia == e.date.d as u8) {
            Some(d) => d.entradas.push(entrada),
            None => fluxo.push(DiaFluxo { dia: e.date.d as u8, entradas: vec![entrada] }),
        }
    }

    // Orçamentos de quem usa o app por categoria (do mais estourado ao mais folgado) e
    // contas fixas fora da média histórica (fogem ≥ 20%).
    let orcamentos = flounder::model::budget_usage(&s.by_category, budgets);
    let alertas_preco = data.price_alerts(month, 0.20);

    // Aviso de fechamento: o primeiro cartão conectado com fechamento válido que fecha
    // em ≤ 3 dias (0 = fecha hoje; o núcleo já devolve 0 para dia inválido, que o filtro
    // descarta). O mobile mostra as contas do domicílio, então qualquer cartão avisa.
    let fechamento_alerta = connected_cards
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| c.closing_day.filter(|d| (1..=31).contains(d)).map(|d| (&c.institution, d)))
        .map(|(banco, d)| (banco.clone(), flounder::model::days_to_closing(d, hoje)))
        .find(|(_, dias)| *dias <= 3);

    mock::Mes {
        vazio: s.empty,
        salario_cents: s.income,
        entradas_cents,
        saidas_cents,
        saldo: mock::Saldo { centavos: s.balance, pct_livre: pct(s.balance) },
        reparticao,
        pagar,
        cartoes,
        casa,
        fluxo,
        orcamentos,
        alertas_preco,
        fechamento_alerta,
    }
}

/// Dias até a próxima ocorrência do fechamento, a partir de `hoje` (0 = fecha hoje).
/// Delega ao núcleo (`model::days_to_closing`); sem fechamento informado, 0 — o mesmo
/// valor de "fecha hoje", então quem mostra usa `fechamento_dia` para distinguir.
pub fn fecha_em_dias(closing_day: Option<u32>, hoje: flounder::model::Date) -> u32 {
    closing_day.map(|d| flounder::model::days_to_closing(d, hoje)).unwrap_or(0)
}

/// Palavra do prazo do fechamento: "fecha hoje", "fecha amanhã", "fecha em N dias".
/// Sem fechamento informado, o texto fica neutro (nada a contar).
pub fn prazo_fechamento(fecha_dias: u32, fechamento_dia: Option<u8>) -> String {
    match fechamento_dia {
        Some(_) => match fecha_dias {
            0 => "fecha hoje".into(),
            1 => "fecha amanhã".into(),
            n => format!("fecha em {n} dias"),
        },
        None => "sem fechamento informado".into(),
    }
}

fn app_mes_nome(month: Month) -> String {
    const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
    MESES[(month.m - 1) as usize].to_owned()
}
