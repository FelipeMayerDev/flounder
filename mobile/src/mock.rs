//! Dados de demonstração do mobile — os números do mockup (Dezembro 2026) e um novembro
//! mais simples. Fase 2 troca por `flounder::model::Data` vindo da API do backend.

use egui::Color32;

pub const MEMBROS: [(&str, Color32); 2] = [("Felipe", Color32::from_rgb(47, 108, 255)), ("Larissa", Color32::from_rgb(240, 68, 56))];

pub struct Mes {
    pub vazio: bool,
    pub salario_cents: i64,
    pub saldo: Saldo,
    pub reparticao: Vec<Reparte>,
    pub pagar: Pagar,
    pub cartoes: Cartoes,
    pub casa: Casa,
    pub fluxo: Vec<DiaFluxo>,
}

pub struct Saldo {
    pub centavos: i64,
    /// "19% do salário livre".
    pub pct_livre: u8,
}

pub struct Reparte {
    pub label: String,
    pub centavos: i64,
    pub pct: u8,
    pub cor: Color32,
}

pub struct Pagar {
    /// Dia em destaque na tira ("Contas a pagar · dia 05").
    pub dia: u8,
    pub total_cents: i64,
    /// (dia, qtd, total) da tira horizontal.
    pub dias: Vec<(u8, u8, i64)>,
}

pub struct Cartoes {
    pub total_faturas_cents: i64,
    pub cards: Vec<Cartao>,
}

pub struct Cartao {
    pub banco: String,
    pub cor: Color32,
    pub aberta: bool,
    /// "fecha em 18 dias" (0 = fatura fechada).
    pub fecha_dias: u32,
    pub vence_dia: u8,
    pub fatura_label: String,
    pub total_cents: i64,
    pub compras: u32,
    pub disponivel_cents: i64,
    pub limite_cents: i64,
    pub paga: bool,
}

pub struct Casa {
    pub total_contas: u8,
    pub pendentes_qtd: u8,
    pub pendentes_total_cents: i64,
    pub bills: Vec<ContaCasa>,
}

pub struct ContaCasa {
    pub nome: String,
    pub dia: u8,
    pub total_cents: i64,
    /// "R$ ≈ 2.332,54" (conta variável).
    pub aproximado: bool,
    /// Posição do membro que pagou.
    pub pagador: usize,
    pub parte_cents: i64,
    pub paga: bool,
}

pub struct DiaFluxo {
    pub dia: u8,
    pub entradas: Vec<Entrada>,
}

pub struct Entrada {
    pub desc: String,
    pub centavos: i64,
    pub membro: usize,
    pub shared: bool,
    pub card: Option<String>,
    pub paid: bool,
}

/// Dados do mês; meses sem nada voltam `Mes::VAZIO`.
pub fn para(y: i32, m: u8) -> Mes {
    match (y, m) {
        (2026, 12) => dezembro(),
        (2026, 11) => novembro(),
        _ => Mes { vazio: true, salario_cents: 0, saldo: Saldo { centavos: 0, pct_livre: 0 }, reparticao: vec![], pagar: Pagar { dia: 1, total_cents: 0, dias: vec![] }, cartoes: Cartoes { total_faturas_cents: 0, cards: vec![] }, casa: Casa { total_contas: 0, pendentes_qtd: 0, pendentes_total_cents: 0, bills: vec![] }, fluxo: vec![] },
    }
}

impl Mes {
    pub fn vazio() -> Mes {
        para(0, 1)
    }
}

fn dezembro() -> Mes {
    Mes {
        vazio: false,
        salario_cents: 2_800_000,
        saldo: Saldo { centavos: 530_428, pct_livre: 19 },
        reparticao: vec![
            Reparte { label: "Sua meta da casa".into(), centavos: 766_622, pct: 27, cor: Color32::from_rgb(23, 61, 156) },
            Reparte { label: "Contas pessoais".into(), centavos: 1_452_950, pct: 52, cor: Color32::from_rgb(47, 108, 255) },
            Reparte { label: "Dia a dia".into(), centavos: 50_000, pct: 2, cor: Color32::from_rgb(143, 176, 255) },
            Reparte { label: "Sobra".into(), centavos: 530_428, pct: 18, cor: Color32::from_rgb(18, 183, 106) },
        ],
        pagar: Pagar {
            dia: 5,
            total_cents: 1_937_950,
            dias: vec![
                (5, 4, 1_937_950),
                (8, 1, 84_900),
                (10, 2, 205_400),
                (12, 1, 559_000),
                (15, 2, 1_438_200),
                (20, 1, 63_000),
                (25, 1, 129_900),
            ],
        },
        cartoes: Cartoes {
            total_faturas_cents: 982_970,
            cards: vec![
                Cartao {
                    banco: "Santander Unique".into(),
                    cor: Color32::from_rgb(234, 29, 37),
                    aberta: true,
                    fecha_dias: 18,
                    vence_dia: 5,
                    fatura_label: "Fatura de dezembro".into(),
                    total_cents: 799_970,
                    compras: 7,
                    disponivel_cents: 1_387_030,
                    limite_cents: 6_500_000,
                    paga: false,
                },
                Cartao {
                    banco: "Nubank".into(),
                    cor: Color32::from_rgb(130, 10, 209),
                    aberta: true,
                    fecha_dias: 24,
                    vence_dia: 12,
                    fatura_label: "Fatura de dezembro".into(),
                    total_cents: 183_000,
                    compras: 4,
                    disponivel_cents: 530_000,
                    limite_cents: 1_000_000,
                    paga: false,
                },
            ],
        },
        casa: Casa {
            total_contas: 7,
            pendentes_qtd: 7,
            pendentes_total_cents: 1_533_244,
            bills: vec![
                ContaCasa { nome: "Apartamento Vila Mariana".into(), dia: 1, total_cents: 233_254, aproximado: true, pagador: 1, parte_cents: 116_627, paga: false },
                ContaCasa { nome: "Aluguel".into(), dia: 1, total_cents: 480_000, aproximado: false, pagador: 1, parte_cents: 240_000, paga: false },
                ContaCasa { nome: "Condomínio".into(), dia: 1, total_cents: 158_000, aproximado: false, pagador: 0, parte_cents: 79_000, paga: false },
                ContaCasa { nome: "Luz".into(), dia: 5, total_cents: 34_000, aproximado: false, pagador: 0, parte_cents: 17_000, paga: false },
                ContaCasa { nome: "Internet".into(), dia: 10, total_cents: 12_990, aproximado: false, pagador: 0, parte_cents: 6_495, paga: false },
                ContaCasa { nome: "Plano de saúde".into(), dia: 15, total_cents: 560_300, aproximado: false, pagador: 1, parte_cents: 280_150, paga: false },
                ContaCasa { nome: "Água".into(), dia: 20, total_cents: 54_700, aproximado: false, pagador: 0, parte_cents: 27_350, paga: false },
            ],
        },
        fluxo: vec![
            DiaFluxo { dia: 1, entradas: vec![
                Entrada { desc: "Apartamento Vila Mariana".into(), centavos: -233_254, membro: 1, shared: true, card: None, paid: true },
                Entrada { desc: "Aluguel".into(), centavos: -480_000, membro: 1, shared: true, card: None, paid: true },
                Entrada { desc: "Condomínio".into(), centavos: -158_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 3, entradas: vec![
                Entrada { desc: "iFood".into(), centavos: -8_460, membro: 0, shared: false, card: Some("Santander Unique".into()), paid: false },
            ] },
            DiaFluxo { dia: 5, entradas: vec![
                Entrada { desc: "Salário".into(), centavos: 2_800_000, membro: 0, shared: false, card: None, paid: true },
                Entrada { desc: "Luz".into(), centavos: -34_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 6, entradas: vec![
                Entrada { desc: "Mercado do bairro".into(), centavos: -31_270, membro: 1, shared: false, card: Some("Santander Unique".into()), paid: false },
            ] },
            DiaFluxo { dia: 10, entradas: vec![
                Entrada { desc: "Internet".into(), centavos: -12_990, membro: 0, shared: true, card: None, paid: true },
                Entrada { desc: "Netflix".into(), centavos: -5_590, membro: 1, shared: false, card: Some("Nubank".into()), paid: false },
            ] },
            DiaFluxo { dia: 12, entradas: vec![
                Entrada { desc: "Freelance de design".into(), centavos: 150_000, membro: 1, shared: false, card: None, paid: true },
                Entrada { desc: "Farmácia".into(), centavos: -8_730, membro: 0, shared: false, card: Some("Nubank".into()), paid: false },
            ] },
            DiaFluxo { dia: 15, entradas: vec![
                Entrada { desc: "Plano de saúde".into(), centavos: -560_300, membro: 1, shared: true, card: None, paid: false },
            ] },
            DiaFluxo { dia: 20, entradas: vec![
                Entrada { desc: "Água".into(), centavos: -54_700, membro: 0, shared: true, card: None, paid: false },
            ] },
        ],
    }
}

fn novembro() -> Mes {
    Mes {
        vazio: false,
        salario_cents: 2_800_000,
        saldo: Saldo { centavos: 312_650, pct_livre: 11 },
        reparticao: vec![
            Reparte { label: "Sua meta da casa".into(), centavos: 766_622, pct: 27, cor: Color32::from_rgb(23, 61, 156) },
            Reparte { label: "Contas pessoais".into(), centavos: 1_401_878, pct: 50, cor: Color32::from_rgb(47, 108, 255) },
            Reparte { label: "Dia a dia".into(), centavos: 318_850, pct: 12, cor: Color32::from_rgb(143, 176, 255) },
            Reparte { label: "Sobra".into(), centavos: 312_650, pct: 11, cor: Color32::from_rgb(18, 183, 106) },
        ],
        pagar: Pagar { dia: 1, total_cents: 871_254, dias: vec![(1, 3, 871_254), (5, 1, 34_000)] },
        cartoes: Cartoes {
            total_faturas_cents: 921_400,
            cards: vec![
                Cartao { banco: "Santander Unique".into(), cor: Color32::from_rgb(234, 29, 37), aberta: false, fecha_dias: 0, vence_dia: 5, fatura_label: "Fatura de novembro".into(), total_cents: 742_100, compras: 9, disponivel_cents: 1_451_900, limite_cents: 6_500_000, paga: true },
                Cartao { banco: "Nubank".into(), cor: Color32::from_rgb(130, 10, 209), aberta: false, fecha_dias: 0, vence_dia: 12, fatura_label: "Fatura de novembro".into(), total_cents: 179_300, compras: 5, disponivel_cents: 533_700, limite_cents: 1_000_000, paga: true },
            ],
        },
        casa: Casa {
            total_contas: 7,
            pendentes_qtd: 0,
            pendentes_total_cents: 0,
            bills: dezembro().casa.bills.into_iter().map(|mut b| { b.paga = true; b }).collect(),
        },
        fluxo: vec![
            DiaFluxo { dia: 1, entradas: vec![
                Entrada { desc: "Apartamento Vila Mariana".into(), centavos: -233_254, membro: 1, shared: true, card: None, paid: true },
                Entrada { desc: "Aluguel".into(), centavos: -480_000, membro: 1, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 5, entradas: vec![
                Entrada { desc: "Salário".into(), centavos: 2_800_000, membro: 0, shared: false, card: None, paid: true },
                Entrada { desc: "Luz".into(), centavos: -34_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 12, entradas: vec![
                Entrada { desc: "Presente de aniversário".into(), centavos: -12_000, membro: 1, shared: false, card: None, paid: false },
            ] },
        ],
    }
}
