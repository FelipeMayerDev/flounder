//! Dados de demonstração do mobile — os números do mockup (Dezembro 2026) e um novembro
//! mais simples. Fase 2 troca por `flounder::model::Data` vindo da API do backend.

use egui::Color32;

pub const MEMBROS: [(&str, Color32); 2] = [("Felipe", Color32::from_rgb(47, 108, 255)), ("Larissa", Color32::from_rgb(240, 68, 56))];

/// Cores de marca para cartões novos (Santander, Nubank, Itaú, Inter, Caixa, Bradesco, C6, PicPay).
pub const CORES_BANCOS: [Color32; 8] = [
    Color32::from_rgb(234, 29, 37),
    Color32::from_rgb(130, 10, 209),
    Color32::from_rgb(236, 112, 0),
    Color32::from_rgb(255, 122, 0),
    Color32::from_rgb(0, 112, 184),
    Color32::from_rgb(165, 7, 30),
    Color32::from_rgb(20, 20, 20),
    Color32::from_rgb(33, 194, 94),
];

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
    /// Contas do mês com a ação de cada uma ao marcar paga.
    pub contas: Vec<ContaPagar>,
}

#[derive(Clone)]
pub enum Acao {
    /// Lançamento simples: alterna `paid` e grava por `Cmd::Entry`.
    Entrada(u32),
    /// Fatura de cartão conectado: `Cmd::BillPaid`.
    Fatura { card: String, due: String },
    /// Fatura manual (demo): nada a gravar.
    Nenhuma,
}

pub struct ContaPagar {
    pub dia: u8,
    pub label: String,
    pub total_cents: i64,
    pub paga: bool,
    pub acao: Acao,
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
    /// Acerto acumulado: > 0 = {nome[1]} deve para {nome[0]}, < 0 o contrário.
    pub saldo_acerto: i64,
    /// Acertos registrados (mais recente primeiro).
    pub acertos: Vec<Acerto>,
}

pub struct Acerto {
    pub id: String,
    pub de: usize,
    pub valor_cents: i64,
    pub dia: u8,
    pub mes: u8,
    pub ano: i32,
}

pub struct ContaCasa {
    /// Id local da entrada (fase 2: alterna paga e grava via worker).
    pub id: u32,
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
    /// Id local da entrada (fase 2: tocar para editar/excluir).
    pub id: u32,
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
        _ => Mes { vazio: true, salario_cents: 0, saldo: Saldo { centavos: 0, pct_livre: 0 }, reparticao: vec![], pagar: Pagar { dia: 1, total_cents: 0, contas: vec![], dias: vec![] }, cartoes: Cartoes { total_faturas_cents: 0, cards: vec![] }, casa: Casa { total_contas: 0, pendentes_qtd: 0, pendentes_total_cents: 0, saldo_acerto: 0, acertos: vec![], bills: vec![] }, fluxo: vec![] },
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
            contas: vec![],
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
            saldo_acerto: 0,
            acertos: vec![],
            bills: vec![
                ContaCasa { id: 0, nome: "Apartamento Vila Mariana".into(), dia: 1, total_cents: 233_254, aproximado: true, pagador: 1, parte_cents: 116_627, paga: false },
                ContaCasa { id: 0, nome: "Aluguel".into(), dia: 1, total_cents: 480_000, aproximado: false, pagador: 1, parte_cents: 240_000, paga: false },
                ContaCasa { id: 0, nome: "Condomínio".into(), dia: 1, total_cents: 158_000, aproximado: false, pagador: 0, parte_cents: 79_000, paga: false },
                ContaCasa { id: 0, nome: "Luz".into(), dia: 5, total_cents: 34_000, aproximado: false, pagador: 0, parte_cents: 17_000, paga: false },
                ContaCasa { id: 0, nome: "Internet".into(), dia: 10, total_cents: 12_990, aproximado: false, pagador: 0, parte_cents: 6_495, paga: false },
                ContaCasa { id: 0, nome: "Plano de saúde".into(), dia: 15, total_cents: 560_300, aproximado: false, pagador: 1, parte_cents: 280_150, paga: false },
                ContaCasa { id: 0, nome: "Água".into(), dia: 20, total_cents: 54_700, aproximado: false, pagador: 0, parte_cents: 27_350, paga: false },
            ],
        },
        fluxo: vec![
            DiaFluxo { dia: 1, entradas: vec![
                Entrada { id: 0, desc: "Apartamento Vila Mariana".into(), centavos: -233_254, membro: 1, shared: true, card: None, paid: true },
                Entrada { id: 0, desc: "Aluguel".into(), centavos: -480_000, membro: 1, shared: true, card: None, paid: true },
                Entrada { id: 0, desc: "Condomínio".into(), centavos: -158_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 3, entradas: vec![
                Entrada { id: 0, desc: "iFood".into(), centavos: -8_460, membro: 0, shared: false, card: Some("Santander Unique".into()), paid: false },
            ] },
            DiaFluxo { dia: 5, entradas: vec![
                Entrada { id: 0, desc: "Salário".into(), centavos: 2_800_000, membro: 0, shared: false, card: None, paid: true },
                Entrada { id: 0, desc: "Luz".into(), centavos: -34_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 6, entradas: vec![
                Entrada { id: 0, desc: "Mercado do bairro".into(), centavos: -31_270, membro: 1, shared: false, card: Some("Santander Unique".into()), paid: false },
            ] },
            DiaFluxo { dia: 10, entradas: vec![
                Entrada { id: 0, desc: "Internet".into(), centavos: -12_990, membro: 0, shared: true, card: None, paid: true },
                Entrada { id: 0, desc: "Netflix".into(), centavos: -5_590, membro: 1, shared: false, card: Some("Nubank".into()), paid: false },
            ] },
            DiaFluxo { dia: 12, entradas: vec![
                Entrada { id: 0, desc: "Freelance de design".into(), centavos: 150_000, membro: 1, shared: false, card: None, paid: true },
                Entrada { id: 0, desc: "Farmácia".into(), centavos: -8_730, membro: 0, shared: false, card: Some("Nubank".into()), paid: false },
            ] },
            DiaFluxo { dia: 15, entradas: vec![
                Entrada { id: 0, desc: "Plano de saúde".into(), centavos: -560_300, membro: 1, shared: true, card: None, paid: false },
            ] },
            DiaFluxo { dia: 20, entradas: vec![
                Entrada { id: 0, desc: "Água".into(), centavos: -54_700, membro: 0, shared: true, card: None, paid: false },
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
        pagar: Pagar { dia: 1, total_cents: 871_254, contas: vec![], dias: vec![(1, 3, 871_254), (5, 1, 34_000)] },
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
            saldo_acerto: 0,
            acertos: vec![],
            bills: dezembro().casa.bills.into_iter().map(|mut b| { b.id = 0; b.paga = true; b }).collect(),
        },
        fluxo: vec![
            DiaFluxo { dia: 1, entradas: vec![
                Entrada { id: 0, desc: "Apartamento Vila Mariana".into(), centavos: -233_254, membro: 1, shared: true, card: None, paid: true },
                Entrada { id: 0, desc: "Aluguel".into(), centavos: -480_000, membro: 1, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 5, entradas: vec![
                Entrada { id: 0, desc: "Salário".into(), centavos: 2_800_000, membro: 0, shared: false, card: None, paid: true },
                Entrada { id: 0, desc: "Luz".into(), centavos: -34_000, membro: 0, shared: true, card: None, paid: true },
            ] },
            DiaFluxo { dia: 12, entradas: vec![
                Entrada { id: 0, desc: "Presente de aniversário".into(), centavos: -12_000, membro: 1, shared: false, card: None, paid: false },
            ] },
        ],
    }
}
