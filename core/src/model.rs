//! Estado e cálculos. Nada aqui conhece egui.

use crate::connected::ConnectedBill;

pub type Cents = i64;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Month {
    pub y: i32,
    pub m: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

/// Data fixa da demonstração e dos testes, para capturas reprodutíveis.
pub const DEMO_TODAY: Date = Date { y: 2026, m: 11, d: 10 };

static REAL_CLOCK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Liga o relógio real (só o executável); sem isso `today()` é `DEMO_TODAY`.
pub fn use_real_clock() {
    REAL_CLOCK.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn today() -> Date {
    if !REAL_CLOCK.load(std::sync::atomic::Ordering::Relaxed) {
        return DEMO_TODAY;
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // ponytail: America/Sao_Paulo como UTC−3 fixo (sem horário de verão desde 2019); usar base de fusos se ele voltar.
    date_from_days((secs - 3 * 3600).div_euclid(86_400))
}

/// Dias desde 01/01/1970 → data civil (civil_from_days, de Howard Hinnant).
fn date_from_days(z: i64) -> Date {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Date { y: (yoe + era * 400 + (m <= 2) as i64) as i32, m: m as u32, d: (doy - (153 * mp + 2) / 5 + 1) as u32 }
}

pub const MONTHS: [&str; 12] = [
    "Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro",
    "Outubro", "Novembro", "Dezembro",
];

impl Month {
    pub fn of(d: Date) -> Self {
        Month { y: d.y, m: d.m }
    }
    pub fn add(self, n: i32) -> Self {
        let i = self.y * 12 + self.m as i32 - 1 + n;
        Month { y: i.div_euclid(12), m: i.rem_euclid(12) as u32 + 1 }
    }
    pub fn days(self) -> u32 {
        match self.m {
            2 if (self.y % 4 == 0 && self.y % 100 != 0) || self.y % 400 == 0 => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
    pub fn name(self) -> &'static str {
        MONTHS[self.m as usize - 1]
    }
    pub fn short(self) -> String {
        format!("{} {:02}", &self.name()[..3], self.y % 100)
    }
    pub fn day(self, d: u32) -> Date {
        Date { y: self.y, m: self.m, d: d.min(self.days()) }
    }
    /// 0 = domingo.
    pub fn first_weekday(self) -> u32 {
        // Zeller simplificado (Sakamoto).
        let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.m < 3 { self.y - 1 } else { self.y };
        ((y + y / 4 - y / 100 + y / 400 + t[self.m as usize - 1] + 1).rem_euclid(7)) as u32
    }
}

impl Date {
    pub fn parse(s: &str) -> Option<Date> {
        let mut p = s.trim().split('/');
        let d: u32 = p.next()?.trim().parse().ok()?;
        let m: u32 = p.next()?.trim().parse().ok()?;
        let y: i32 = p.next()?.trim().parse().ok()?;
        if p.next().is_some() || !(1..=12).contains(&m) || !(1900..=2100).contains(&y) {
            return None;
        }
        (d >= 1 && d <= Month { y, m }.days()).then_some(Date { y, m, d })
    }
    /// "AAAA-MM-DD".
    pub fn iso(s: &str) -> Option<Date> {
        let (y, md) = s.split_once('-')?;
        let (m, d) = md.split_once('-')?;
        Date::parse(&format!("{d}/{m}/{y}"))
    }
    pub fn fmt(self) -> String {
        format!("{:02}/{:02}/{}", self.d, self.m, self.y)
    }
    pub fn short(self) -> String {
        format!("{:02}/{:02}", self.d, self.m)
    }
}

/// Fatura a que a compra pertence: feita no dia do fechamento em diante, cai na fatura do
/// mês seguinte. O fechamento do mês é o dia clampado ao fim dele (fechamento dia 31 vira
/// 28/29 em fevereiro). `closing_day` 0/inválido vira mês civil.
pub fn invoice_month(date: Date, closing_day: u32) -> Month {
    if !(1..=31).contains(&closing_day) {
        return Month::of(date);
    }
    let m = Month::of(date);
    if days_from_civil(date) >= days_from_civil(m.day(closing_day)) {
        m.add(1)
    } else {
        m
    }
}

/// Janela de compras da fatura que VENCE em `m`: [inclusivo, exclusivo). O ciclo fecha no
/// mês `m` se o vencimento cai depois do fechamento (`due_day > closing_day`), senão no mês
/// anterior; a janela vai do fechamento anterior até a véspera do fechamento do ciclo — a
/// compra feita no dia do fechamento já é da fatura seguinte (`invoice_month`).
pub fn invoice_window(m: Month, closing_day: u32, due_day: Option<u32>) -> (Date, Date) {
    let cycle = match due_day {
        Some(d) if d > closing_day => m,
        _ => m.add(-1),
    };
    (cycle.add(-1).day(closing_day), cycle.day(closing_day))
}

/// Dias até a próxima ocorrência do fechamento (0 = fecha hoje). Fechamento inválido
/// (0/>31) → 0. Para o contador, o dia do fechamento É o fechamento — diferente do
/// `invoice_month`, que já rola as compras desse dia para a fatura seguinte.
pub fn days_to_closing(closing_day: u32, hoje: Date) -> u32 {
    if !(1..=31).contains(&closing_day) {
        return 0;
    }
    let m = Month::of(hoje);
    let close = if days_from_civil(hoje) > days_from_civil(m.day(closing_day)) {
        m.add(1).day(closing_day)
    } else {
        m.day(closing_day)
    };
    (days_from_civil(close) - days_from_civil(hoje)) as u32
}

/// Janela de compras da fatura que vence em `m` usando os fechamentos reais das bills da
/// Pluggy (`ConnectedBill.closing`) quando existem; onde faltar, sintetiza pelo dia do
/// fechamento do cartão. `None` se nenhuma bill vence em `m`.
pub fn invoice_window_billed(bills: &[ConnectedBill], m: Month, closing_day: Option<u32>, due_day: Option<u32>) -> Option<(Date, Date)> {
    let prefix = format!("{:04}-{:02}-", m.y, m.m);
    let bill = bills.iter().find(|b| b.due.starts_with(&prefix))?;
    let synth = closing_day.filter(|c| (1..=31).contains(c)).map(|c| invoice_window(m, c, due_day));
    let end = bill.closing.as_deref().and_then(Date::iso).or(synth.map(|w| w.1))?;
    let prev = bills
        .iter()
        .filter(|b| b.due.as_str() < bill.due.as_str())
        .filter_map(|b| b.closing.as_deref().and_then(Date::iso))
        .max_by_key(|d| days_from_civil(*d))
        .filter(|d| days_from_civil(*d) < days_from_civil(end));
    Some((prev.unwrap_or_else(|| synth.map_or_else(|| Month { y: end.y, m: end.m }.add(-1).day(end.d), |w| w.0)), end))
}

/// Janela da fatura em formação (ainda sem bill publicada): começa no fechamento mais
/// recente conhecido (bill real, se recente o bastante; senão o sintetizado) e termina na
/// véspera do próximo fechamento — a compra feita no dia do fechamento já é dela.
/// `None` sem nenhum fechamento utilizável.
pub fn forming_invoice_window(bills: &[ConnectedBill], closing_day: Option<u32>, hoje: Date) -> Option<(Date, Date)> {
    let c = closing_day.filter(|c| (1..=31).contains(c))?;
    let m = Month::of(hoje);
    let end = if days_from_civil(hoje) >= days_from_civil(m.day(c)) { m.add(1).day(c) } else { m.day(c) };
    let synth = Month { y: end.y, m: end.m }.add(-1).day(end.d);
    let last = bills
        .iter()
        .filter_map(|b| b.closing.as_deref().and_then(Date::iso))
        .max_by_key(|d| days_from_civil(*d))
        .filter(|d| days_from_civil(*d) <= days_from_civil(hoje) && days_from_civil(*d) >= days_from_civil(synth));
    Some((last.unwrap_or(synth), end))
}

/// "R$ 1.234,56"; negativo com sinal de menos tipográfico.
pub fn money(c: Cents) -> String {
    let (int, frac) = split_money(c);
    format!("{}R$ {int},{frac}", if c < 0 { "−" } else { "" })
}

/// Parte inteira com milhar e centavos, sem sinal.
pub fn split_money(c: Cents) -> (String, String) {
    let a = c.unsigned_abs();
    let digits = (a / 100).to_string();
    let mut int = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            int.push('.');
        }
        int.push(ch);
    }
    (int, format!("{:02}", a % 100))
}

/// Aceita "1.234,56", "1234,56", "1234.56", "R$ 50".
pub fn parse_money(s: &str) -> Option<Cents> {
    let s: String = s.trim().trim_start_matches("R$").chars().filter(|c| !c.is_whitespace()).collect();
    let (int, frac) = match (s.rfind(','), s.rfind('.')) {
        (Some(i), _) => (s[..i].replace('.', ""), &s[i + 1..]),
        (None, Some(i)) if s.len() - i - 1 <= 2 && s.matches('.').count() == 1 => (s[..i].to_string(), &s[i + 1..]),
        _ => (s.replace('.', ""), ""),
    };
    if int.is_empty() && frac.is_empty() || frac.len() > 2 || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let i: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let f: i64 = if frac.is_empty() { 0 } else { format!("{frac:0<2}").parse().ok()? };
    i.checked_mul(100)?.checked_add(f)
}

pub fn input_money(c: Cents) -> String {
    let (i, f) = split_money(c);
    format!("{i},{f}")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Income,
    Expense,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Individual,
    Couple,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Person {
    A,
    B,
}

impl Person {
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn other(self) -> Person {
        match self {
            Person::A => Person::B,
            Person::B => Person::A,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Category {
    Salary,
    Housing,
    Utilities,
    Groceries,
    Transport,
    Health,
    Leisure,
    Reserve,
    Family,
    Shopping,
    Services,
    Education,
    Pets,
    Other,
}

impl Category {
    pub const ALL: [Category; 14] = [
        Category::Salary,
        Category::Housing,
        Category::Utilities,
        Category::Groceries,
        Category::Transport,
        Category::Health,
        Category::Leisure,
        Category::Reserve,
        Category::Family,
        Category::Shopping,
        Category::Services,
        Category::Education,
        Category::Pets,
        Category::Other,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Salary => "Renda",
            Category::Housing => "Moradia",
            Category::Utilities => "Contas da casa",
            Category::Groceries => "Mercado",
            Category::Transport => "Transporte",
            Category::Health => "Saúde",
            Category::Leisure => "Lazer",
            Category::Reserve => "Reserva",
            Category::Family => "Família",
            Category::Shopping => "Compras",
            Category::Services => "Serviços",
            Category::Education => "Educação",
            Category::Pets => "Pets",
            Category::Other => "Outros",
        }
    }
    /// Chave em `finance.categories` no banco.
    pub fn key(self) -> &'static str {
        match self {
            Category::Salary => "salary",
            Category::Housing => "housing",
            Category::Utilities => "utilities",
            Category::Groceries => "groceries",
            Category::Transport => "transport",
            Category::Health => "health",
            Category::Leisure => "leisure",
            Category::Reserve => "reserve",
            Category::Family => "family",
            Category::Shopping => "shopping",
            Category::Services => "services",
            Category::Education => "education",
            Category::Pets => "pets",
            Category::Other => "other",
        }
    }
}

/// Compra parcelada em andamento: a última parcela lançada até o mês e o que falta.
#[derive(Clone, Debug, PartialEq)]
pub struct Installment {
    pub desc: String,
    /// Cartão conectado da compra, se houver.
    pub card: Option<String>,
    pub owner: Person,
    /// Valor da última parcela lançada.
    pub amount: Cents,
    pub n: u32,
    pub of: u32,
    pub last: Date,
}

impl Installment {
    pub fn left(&self) -> u32 {
        self.of - self.n
    }
    pub fn remaining(&self) -> Cents {
        self.amount * self.left() as Cents
    }
    /// Mês previsto da última parcela.
    pub fn ends(&self) -> Month {
        Month::of(self.last).add(self.left() as i32)
    }
    /// Parcela prevista para `m` (depois da última lançada, até o fim).
    pub fn due_in(&self, m: Month) -> bool {
        m > Month::of(self.last) && m <= self.ends()
    }
}

/// "Loja (2/10)", "Loja 2/10" ou "Loja - Parcela 2/10" → ("Loja", 2, 10).
pub fn split_installment(desc: &str) -> Option<(String, u32, u32)> {
    let token = desc.split(|c: char| c.is_whitespace() || c == '(' || c == ')').rfind(|t| !t.is_empty())?;
    let (n, of) = token.split_once('/')?;
    let (n, of): (u32, u32) = (n.parse().ok()?, of.parse().ok()?);
    if !(2..=99).contains(&of) || n == 0 || n > of {
        return None;
    }
    let trim = |s: &str| s.trim_end_matches(|c: char| c.is_whitespace() || c == '(' || c == '-').to_string();
    let base = trim(&desc[..desc.rfind(token)?]);
    let base = trim(base.strip_suffix("Parcela").or_else(|| base.strip_suffix("parcela")).unwrap_or(&base));
    (!base.is_empty()).then_some((base, n, of))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u32,
    pub desc: String,
    pub amount: Cents,
    pub date: Date,
    pub kind: Kind,
    pub category: Category,
    /// Receita: quem recebe. Despesa pessoal: de quem é. Compartilhada: quem paga (responsável).
    pub owner: Person,
    pub shared: bool,
    /// Compra no cartão manual (`Data::cards`): o pagamento é a fatura, não a compra.
    pub card: Option<usize>,
    /// Compra num cartão conectado (`ConnectedCard.id`): também paga pela fatura importada.
    pub connected: Option<String>,
    /// Veio da importação de transações do banco/cartão.
    pub imported: bool,
    pub paid: bool,
    /// Conta fixa (aluguel, luz, assinatura…): repete todo mês e alimenta a previsão
    /// dos meses futuros, com o valor estimado pela média das últimas ocorrências.
    pub recurring: bool,
}

/// De onde veio o lançamento (filtro e selo no Fluxo de caixa).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Bank,
    Card,
    Manual,
}

impl Origin {
    pub fn of(e: &Entry) -> Origin {
        if e.card.is_some() || e.connected.is_some() {
            Origin::Card
        } else if e.imported {
            Origin::Bank
        } else {
            Origin::Manual
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Origin::Bank => "Banco",
            Origin::Card => "Cartão",
            Origin::Manual => "Manual",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Card {
    pub name: String,
    pub owner: Person,
    pub due_day: u32,
    pub limit: Cents,
    /// Dia do fechamento; `None` = não informado e o mês civil vira a fatura.
    pub closing_day: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BillRef {
    Entry(u32),
    Invoice(usize),
    /// Índice em `Data::connected_invoices`.
    Connected(usize),
}

/// Fatura de um cartão importado da Pluggy: conta a pagar de quem é dono do cartão.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectedInvoice {
    /// `ConnectedCard.id`.
    pub card: String,
    pub label: String,
    pub owner: Person,
    pub amount: Cents,
    pub due: Date,
    pub paid: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Paid,
    Overdue,
    Due,
}

#[derive(Clone, Debug)]
pub struct Bill {
    pub target: BillRef,
    pub label: String,
    pub amount: Cents,
    pub date: Date,
    pub status: Status,
}

#[derive(Clone, Debug)]
pub struct Invoice {
    pub card: usize,
    pub total: Cents,
    pub due: Date,
    pub paid: bool,
}

/// Acerto entre o casal: `from` transferiu `amount` para o outro.
#[derive(Clone, Debug, PartialEq)]
pub struct Settlement {
    /// UUID do banco, ou "local-N" até o backend responder (ver `backend.rs`).
    pub id: String,
    pub date: Date,
    pub amount: Cents,
    pub from: Person,
}

pub struct Data {
    pub names: [String; 2],
    /// Quem usa o app.
    pub me: Person,
    pub entries: Vec<Entry>,
    pub cards: Vec<Card>,
    pub paid_invoices: Vec<(usize, Month)>,
    pub connected_invoices: Vec<ConnectedInvoice>,
    pub reserve_start: Cents,
    pub reserve_goal: Cents,
    pub reserve_name: String,
    pub settlements: Vec<Settlement>,
    /// Despesa compartilhada vale por inteiro para quem pagou: nada de rateio nem acertos.
    pub shared_split_off: bool,
    next_id: u32,
}

/// Tudo que a tela inicial mostra, calculado uma vez por quadro.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub empty: bool,
    pub income: Cents,
    /// Parte da pessoa nas despesas da casa (casal) — zero no modo individual.
    pub shared_part: Cents,
    pub personal: Cents,
    pub expenses: Cents,
    pub balance: Cents,
    pub shared_total: Cents,
    pub paid_by: [Cents; 2],
    pub owed: [Cents; 2],
    /// Saldo entre o casal acumulado até o mês, já com os acertos (`Data::house_balance`).
    /// > 0: B deve a A; < 0: A deve a B. Zero no modo individual.
    pub settlement: Cents,
    pub bills: Vec<Bill>,
    pub to_pay: Cents,
    pub paid_count: usize,
    pub overdue_count: usize,
    pub by_category: Vec<(Category, Cents)>,
    pub invoices: Vec<Invoice>,
    pub reserve: Cents,
    pub housing: Cents,
}

impl Summary {
    pub fn pct(&self, v: Cents) -> f32 {
        if self.income <= 0 { 0.0 } else { v as f32 / self.income as f32 * 100.0 }
    }
}

/// Entradas e saídas de um conjunto de lançamentos (do mês, ambos os membros), na ordem
/// (entradas, saídas). Receita soma na entrada; despesa soma na saída — ambas positivas,
/// como `Data::summary` espera (quem exibe decide o sinal na tela).
pub fn cash_flow<'a>(entries: impl Iterator<Item = &'a Entry>) -> (Cents, Cents) {
    let mut income = 0;
    let mut expense = 0;
    for e in entries {
        match e.kind {
            Kind::Income => income += e.amount,
            Kind::Expense => expense += e.amount,
        }
    }
    (income, expense)
}

/// Divide centavos sem perder nenhum: A fica com o centavo ímpar.
pub fn halves(x: Cents) -> [Cents; 2] {
    [x - x / 2, x / 2]
}

/// Conta fixa estimada para os meses que ainda não a têm: descrição, valor previsto pela
/// média das últimas ocorrências e a variação recente (mínimo–máximo).
#[derive(Clone, Debug, PartialEq)]
pub struct Recurring {
    pub desc: String,
    pub kind: Kind,
    pub category: Category,
    pub owner: Person,
    pub shared: bool,
    /// Dia típico do vencimento (o da ocorrência mais recente).
    pub day: u32,
    /// Média das últimas ocorrências.
    pub estimate: Cents,
    pub min: Cents,
    pub max: Cents,
    /// Valor da ocorrência mais recente.
    pub last: Cents,
    /// Quantas ocorrências alimentaram a estimativa (1 a 3).
    pub samples: usize,
}

impl Recurring {
    /// Lançamento real a partir da previsão, já no mês escolhido.
    pub fn entry(&self, m: Month) -> Entry {
        Entry {
            id: 0,
            desc: self.desc.clone(),
            amount: self.estimate,
            date: m.day(self.day),
            kind: self.kind,
            category: self.category,
            owner: self.owner,
            shared: self.shared,
            card: None,
            connected: None,
            imported: false,
            paid: false,
            recurring: true,
        }
    }
    /// "entre X e Y" quando houve variação; "sempre X" quando o valor não mudou.
    pub fn range(&self) -> String {
        if self.min == self.max {
            return format!("sempre {}", money(self.estimate));
        }
        format!("entre {} e {}", money(self.min), money(self.max))
    }
}

impl Data {
    pub fn in_month(&self, m: Month) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| Month::of(e.date) == m)
    }

    /// Compras num cartão conectado que pertencem à fatura que vence em `m`.
    /// Sem fechamento informado, vale o mês civil da compra (comportamento antigo).
    pub fn connected_purchases<'a>(&'a self, m: Month, card_id: &str, closing: Option<u32>, due: Option<u32>) -> impl Iterator<Item = &'a Entry> {
        // ponytail: sem fechamento, compras do mês civil caem na fatura do próprio mês.
        let window = closing.filter(|c| (1..=31).contains(c)).map(|c| invoice_window(m, c, due));
        self.entries.iter().filter(move |e| {
            e.connected.as_deref() == Some(card_id)
                && match window {
                    Some((start, end)) => e.date >= start && e.date < end,
                    None => Month::of(e.date) == m,
                }
        })
    }

    /// Compras já fechadas em `m`: vão para a fatura em formação, que fecha em `m.add(1)`
    /// (vence no próprio `m.add(1)` quando o vencimento cai depois do fechamento, senão no
    /// mês seguinte ao dele — ver `invoice_window`).
    pub fn next_invoice_purchases<'a>(&'a self, m: Month, card_id: &str, closing: Option<u32>) -> impl Iterator<Item = &'a Entry> {
        let next = m.add(1);
        let closing = closing.filter(|c| (1..=31).contains(c));
        self.entries.iter().filter(move |e| {
            e.connected.as_deref() == Some(card_id)
                && Month::of(e.date) == m
                && closing.is_some_and(|c| invoice_month(e.date, c) == next)
        })
    }

    /// Compras num cartão conectado dentro da janela [início, fim) — o mesmo filtro das
    /// janelas de `invoice_window`/`invoice_window_billed`/`forming_invoice_window`.
    pub fn connected_purchases_in<'a>(&'a self, card_id: &str, window: (Date, Date)) -> impl Iterator<Item = &'a Entry> {
        self.entries.iter().filter(move |e| {
            e.connected.as_deref() == Some(card_id) && e.date >= window.0 && e.date < window.1
        })
    }

    /// Totais projetados das próximas `n` faturas (vencimentos `m.add(1)..=m.add(n)`): soma
    /// das compras na janela de cada fatura — pelas bills reais quando existirem, pelas
    /// janelas sintetizadas nas futuras (a Pluggy ainda não as publicou).
    pub fn next_invoices(&self, card_id: &str, bills: &[ConnectedBill], closing: Option<u32>, due: Option<u32>, m: Month, n: u32) -> Vec<(Month, Cents)> {
        (1..=n).map(|i| {
            let month = m.add(i as i32);
            let window = invoice_window_billed(bills, month, closing, due)
                .or_else(|| closing.filter(|c| (1..=31).contains(c)).map(|c| invoice_window(month, c, due)));
            let total = window.map(|w| self.connected_purchases_in(card_id, w).map(|e| e.amount).sum()).unwrap_or(0);
            (month, total)
        }).collect()
    }

    /// Proporção da renda de cada um nos últimos 3 meses (até `m`, inclusive).
    /// `None` sem nenhuma renda registrada: o rateio volta a ser metade para cada um.
    pub fn income_weights(&self, m: Month) -> Option<[f64; 2]> {
        let months = [m.add(-2), m.add(-1), m];
        let mut income = [0 as Cents; 2];
        for e in &self.entries {
            if e.kind == Kind::Income && months.contains(&Month::of(e.date)) {
                income[e.owner.idx()] += e.amount;
            }
        }
        let total = income.iter().sum::<Cents>();
        (total > 0).then(|| income.map(|v| v as f64 / total as f64))
    }

    /// Como uma despesa compartilhada se divide entre o casal: proporcional à renda
    /// registrada do mês dela (50/50 sem renda) ou, com o rateio desligado, por inteiro
    /// para quem pagou. A soma das partes é sempre o valor exato da despesa.
    pub fn shared_split(&self, e: &Entry) -> [Cents; 2] {
        if self.shared_split_off {
            let mut part = [0; 2];
            part[e.owner.idx()] = e.amount;
            return part;
        }
        match self.income_weights(Month::of(e.date)) {
            None => halves(e.amount),
            Some([wa, _]) => {
                let a = (e.amount as f64 * wa).round() as Cents;
                [a, e.amount - a]
            }
        }
    }

    /// Parte da despesa que pesa no orçamento de quem usa o app.
    pub fn my_part(&self, e: &Entry, mode: Mode) -> Cents {
        match (mode, e.shared) {
            (Mode::Couple, true) => self.shared_split(e)[self.me.idx()],
            _ if e.owner == self.me => e.amount,
            _ => 0,
        }
    }

    pub fn invoice(&self, card: usize, m: Month) -> Invoice {
        // ponytail: sem fechamento, compras do mês caem na fatura do mesmo mês; com fechamento,
        // vale a janela do ciclo (a compra no dia do fechamento em diante vai para a fatura seguinte).
        let c = &self.cards[card];
        let total = match c.closing_day {
            Some(day) if (1..=31).contains(&day) => {
                let (start, end) = invoice_window(m, day, Some(c.due_day));
                self.entries.iter().filter(|e| e.card == Some(card) && e.date >= start && e.date < end).map(|e| e.amount).sum()
            }
            _ => self.in_month(m).filter(|e| e.card == Some(card)).map(|e| e.amount).sum(),
        };
        Invoice {
            card,
            total,
            due: m.day(c.due_day),
            paid: self.paid_invoices.contains(&(card, m)),
        }
    }

    pub fn summary(&self, m: Month, mode: Mode) -> Summary {
        let mut s = Summary { empty: self.in_month(m).next().is_none(), ..Default::default() };
        let mut cats: Vec<(Category, Cents)> = Vec::new();
        for e in self.in_month(m) {
            if e.kind == Kind::Income {
                if e.owner == self.me {
                    s.income += e.amount;
                }
                continue;
            }
            let part = self.my_part(e, mode);
            if mode == Mode::Couple && e.shared {
                s.shared_total += e.amount;
                s.shared_part += part;
                s.paid_by[e.owner.idx()] += e.amount;
                for (o, h) in s.owed.iter_mut().zip(self.shared_split(e)) {
                    *o += h;
                }
            } else {
                s.personal += part;
            }
            if e.category == Category::Housing {
                s.housing += part;
            }
            if part != 0 {
                match cats.iter_mut().find(|(c, _)| *c == e.category) {
                    Some((_, v)) => *v += part,
                    None => cats.push((e.category, part)),
                }
            }
            // Compras no cartão (manual ou conectado) viram uma única conta: a fatura.
            // Transação importada já aconteceu: é gasto realizado, não conta a pagar.
            if e.card.is_none() && e.connected.is_none() && !e.imported && e.owner == self.me {
                s.bills.push(Bill {
                    target: BillRef::Entry(e.id),
                    label: e.desc.clone(),
                    amount: e.amount,
                    date: e.date,
                    status: status(e.paid, e.date),
                });
            }
        }
        for c in 0..self.cards.len() {
            let inv = self.invoice(c, m);
            if inv.total == 0 {
                continue;
            }
            if self.cards[c].owner == self.me {
                s.bills.push(Bill {
                    target: BillRef::Invoice(c),
                    label: format!("Fatura {}", self.cards[c].name),
                    amount: inv.total,
                    date: inv.due,
                    status: status(inv.paid, inv.due),
                });
            }
            s.invoices.push(inv);
        }
        // A fatura importada é só a conta a pagar: as compras dela já pesam nas despesas como lançamentos.
        for (i, inv) in self.connected_invoices.iter().enumerate() {
            if inv.owner == self.me && Month::of(inv.due) == m {
                s.bills.push(Bill {
                    target: BillRef::Connected(i),
                    label: inv.label.clone(),
                    amount: inv.amount,
                    date: inv.due,
                    status: status(inv.paid, inv.due),
                });
            }
        }
        s.bills.sort_by_key(|b| (b.date, std::cmp::Reverse(b.amount)));
        cats.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        s.by_category = cats;
        s.expenses = s.shared_part + s.personal;
        s.balance = s.income - s.expenses;
        if mode == Mode::Couple {
            s.settlement = self.house_balance(m);
        }
        s.to_pay = s.bills.iter().filter(|b| b.status != Status::Paid).map(|b| b.amount).sum();
        s.paid_count = s.bills.iter().filter(|b| b.status == Status::Paid).count();
        s.overdue_count = s.bills.iter().filter(|b| b.status == Status::Overdue).count();
        s.reserve = self.reserve_start + self.deposits(mode).filter(|e| Month::of(e.date) <= m).map(|e| e.amount).sum::<Cents>();
        s
    }

    /// Saldo entre o casal acumulado até o fim de `m`: em cada despesa compartilhada, quem pagou
    /// adianta a parte do outro (`shared_split`); cada acerto abate o que quem pagou devia.
    /// Com o rateio desligado, tudo fica zerado. > 0: B deve a A; < 0: A deve a B.
    // ponytail: soma desde o primeiro lançamento a cada quadro; guardar por mês se ficar lento.
    pub fn house_balance(&self, m: Month) -> Cents {
        let shared = self.entries.iter().filter(|e| e.shared && e.kind == Kind::Expense && Month::of(e.date) <= m).map(|e| match e.owner {
            Person::A => self.shared_split(e)[1],
            Person::B => -self.shared_split(e)[0],
        });
        let settled = self.settlements.iter().filter(|s| Month::of(s.date) <= m).map(|s| match s.from {
            Person::A => s.amount,
            Person::B => -s.amount,
        });
        shared.chain(settled).sum()
    }

    /// Aportes pagos na categoria Reserva: do domicílio (casal) ou só de quem usa o app (individual).
    pub fn deposits(&self, mode: Mode) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| {
            e.category == Category::Reserve && e.kind == Kind::Expense && e.paid && (mode == Mode::Couple || e.owner == self.me)
        })
    }

    /// Aportes por mês, de `m - n + 1` até `m`.
    pub fn monthly_deposits(&self, m: Month, mode: Mode, n: i32) -> Vec<(Month, Cents)> {
        (1 - n..=0).map(|k| m.add(k)).map(|k| (k, self.deposits(mode).filter(|e| Month::of(e.date) == k).map(|e| e.amount).sum())).collect()
    }

    /// Média dos aportes dos últimos 3 meses (até `m`, inclusive).
    pub fn reserve_pace(&self, m: Month, mode: Mode) -> Cents {
        self.monthly_deposits(m, mode, 3).iter().map(|d| d.1).sum::<Cents>() / 3
    }

    /// Grava com id local ("local-N") e devolve o id.
    pub fn add_settlement(&mut self, mut s: Settlement) -> String {
        s.id = format!("local-{}", self.next_id);
        self.next_id += 1;
        self.settlements.push(s);
        self.settlements.last().map(|s| s.id.clone()).unwrap_or_default()
    }

    /// "Copiar do mês anterior": lançamentos manuais sem cartão de `m - 1` (de `only`, se informado)
    /// viram não pagos em `m`, no mesmo dia (limitado ao fim do mês). Pula os que já existem em `m`
    /// com a mesma descrição, valor e dono. Devolve os ids criados.
    pub fn copy_previous(&mut self, m: Month, only: Option<Person>) -> Vec<u32> {
        let copies: Vec<Entry> = self
            .in_month(m.add(-1))
            .filter(|e| Origin::of(e) == Origin::Manual && only.is_none_or(|p| e.owner == p))
            .filter(|e| !self.in_month(m).any(|o| o.desc == e.desc && o.amount == e.amount && o.owner == e.owner))
            .map(|e| Entry { id: 0, date: m.day(e.date.d), paid: false, ..e.clone() })
            .collect();
        copies.into_iter().map(|e| self.upsert(e)).collect()
    }

    /// Compras parceladas com parcelas por vir, pelo que foi lançado até o fim de `m`.
    // ponytail: agrupa por descrição + número de parcelas + cartão + pessoa; duas compras iguais
    // na mesma loja no mesmo mês viram uma só.
    pub fn installments(&self, m: Month, only: Option<Person>) -> Vec<Installment> {
        let mut out: Vec<Installment> = Vec::new();
        for e in self.entries.iter().filter(|e| e.kind == Kind::Expense && Month::of(e.date) <= m && only.is_none_or(|p| e.owner == p)) {
            let Some((desc, n, of)) = split_installment(&e.desc) else { continue };
            let card = e.connected.clone();
            match out.iter_mut().find(|i| i.desc == desc && i.of == of && i.card == card && i.owner == e.owner) {
                Some(i) if i.n >= n => {}
                Some(i) => (i.n, i.amount, i.last) = (n, e.amount, e.date),
                None => out.push(Installment { desc, card, owner: e.owner, amount: e.amount, n, of, last: e.date }),
            }
        }
        // Sem parcela nova há tempo demais: já terminou (só faltou importar o fim).
        out.retain(|i| i.n < i.of && i.ends() >= m);
        out.sort_by_key(|i| (i.ends(), std::cmp::Reverse(i.remaining())));
        out
    }

    /// Contas fixas (`recurring`) lançadas até o fim de `m`, agrupadas por descrição, tipo,
    /// dono e compartilhamento. Cartões ficam de fora: o pagamento delas é a fatura, e a
    /// previsão duplicaria as compras. A estimativa usa as últimas 3 ocorrências.
    pub fn recurring_defs(&self, m: Month, only: Option<Person>) -> Vec<Recurring> {
        let mut out: Vec<(String, Kind, Person, bool, Vec<(Date, Cents)>)> = Vec::new();
        for e in self.entries.iter().filter(|e| {
            e.recurring && e.card.is_none() && e.connected.is_none() && Month::of(e.date) <= m
                && only.is_none_or(|p| e.owner == p)
        }) {
            match out.iter_mut().find(|(d, k, o, s, _)| *d == e.desc && *k == e.kind && *o == e.owner && *s == e.shared) {
                Some((_, _, _, _, vals)) => vals.push((e.date, e.amount)),
                None => out.push((e.desc.clone(), e.kind, e.owner, e.shared, vec![(e.date, e.amount)])),
            }
        }
        let mut defs: Vec<Recurring> = out.into_iter().map(|(desc, kind, owner, shared, mut vals)| {
            vals.sort_by_key(|(d, _)| *d);
            let recent: Vec<Cents> = vals.iter().rev().take(3).map(|(_, v)| *v).collect();
            let samples = recent.len();
            let (min, max) = (recent.iter().min().copied().unwrap_or(0), recent.iter().max().copied().unwrap_or(0));
            let estimate = recent.iter().sum::<Cents>() / samples.max(1) as Cents;
            let last = vals.last().map(|(_, v)| *v).unwrap_or(0);
            Recurring { desc, kind, category: Category::Other, owner, shared, day: vals.last().map(|(d, _)| d.d).unwrap_or(1), estimate, min, max, last, samples }
        }).collect();
        // A categoria vem da ocorrência mais recente (a estimativa é o mês que vem dela).
        for def in &mut defs {
            if let Some(e) = self.entries.iter().filter(|e| e.recurring && e.desc == def.desc && e.kind == def.kind && e.owner == def.owner && e.shared == def.shared && e.card.is_none() && e.connected.is_none()).max_by_key(|e| e.date) {
                def.category = e.category;
            }
        }
        defs.sort_by_key(|d| (d.day, std::cmp::Reverse(d.estimate)));
        defs
    }

    /// Previsão para o mês `m`: as contas fixas que ainda não têm lançamento nele.
    /// Na ordem do vencimento típico.
    pub fn forecast(&self, m: Month, only: Option<Person>) -> Vec<Recurring> {
        self.recurring_defs(m, only).into_iter().filter(|d| {
            !self.in_month(m).any(|e| e.desc == d.desc && e.kind == d.kind && e.owner == d.owner && e.shared == d.shared)
        }).collect()
    }

    /// Contas fixas cuja ocorrência mais recente foge da média histórica (`threshold` como
    /// fração: 0.20 = 20%). Só despesas, na ordem de vencimento típico: (descrição, valor
    /// recente, média).
    pub fn price_alerts(&self, m: Month, threshold: f64) -> Vec<(String, Cents, Cents)> {
        self.recurring_defs(m, None).into_iter()
            .filter(|r| r.kind == Kind::Expense && r.estimate > 0)
            .filter(|r| (r.last - r.estimate).unsigned_abs() as f64 / r.estimate as f64 >= threshold)
            .map(|r| (r.desc, r.last, r.estimate))
            .collect()
    }

    pub fn entry(&self, id: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn toggle(&mut self, b: BillRef, m: Month) {
        match b {
            BillRef::Entry(id) => {
                if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                    e.paid = !e.paid;
                }
            }
            BillRef::Invoice(c) => match self.paid_invoices.iter().position(|p| *p == (c, m)) {
                Some(i) => _ = self.paid_invoices.remove(i),
                None => self.paid_invoices.push((c, m)),
            },
            BillRef::Connected(i) => {
                if let Some(inv) = self.connected_invoices.get_mut(i) {
                    inv.paid = !inv.paid;
                }
            }
        }
    }

    /// Grava (novo ou editado) e devolve o id.
    pub fn upsert(&mut self, mut e: Entry) -> u32 {
        if let Some(old) = self.entries.iter_mut().find(|o| o.id == e.id) {
            *old = e;
            old.id
        } else {
            e.id = self.next_id;
            self.next_id += 1;
            self.entries.push(e);
            self.next_id - 1
        }
    }

    /// Sem dados. Ids criados na interface começam em 2^31, longe dos que a sincronização
    /// dá aos lançamentos do banco (de 1 para cima, ver `backend.rs`).
    pub fn empty() -> Data {
        Data {
            names: Default::default(),
            me: Person::A,
            entries: vec![],
            cards: vec![],
            paid_invoices: vec![],
            connected_invoices: vec![],
            reserve_start: 0,
            reserve_goal: 0,
            reserve_name: String::new(),
            settlements: vec![],
            shared_split_off: false,
            next_id: 1 << 31,
        }
    }

    pub fn demo() -> Data {
        let mut d = Data {
            names: ["Lucas".into(), "Marina".into()],
            cards: vec![
                Card { name: "Azul".into(), owner: Person::A, due_day: 25, limit: 1_500_000, closing_day: Some(20) },
                Card { name: "da casa".into(), owner: Person::B, due_day: 20, limit: 800_000, closing_day: Some(15) },
            ],
            reserve_start: 1_840_000,
            reserve_goal: 3_000_000,
            next_id: 1,
            ..Data::empty()
        };
        use Category as C;
        use Person::{A, B};
        for k in -4..=1 {
            let m = Month::of(DEMO_TODAY).add(k);
            let v = |base: Cents, step: Cents| base + step * ((k + 4) % 3) as Cents;
            #[rustfmt::skip]
            let rows: Vec<(&str, Cents, u32, Kind, C, Person, bool, Option<usize>)> = vec![
                ("Salário", 1_250_000, 5, Kind::Income, C::Salary, A, false, None),
                ("Salário", 980_000, 5, Kind::Income, C::Salary, B, false, None),
                ("Academia", 12_990, 5, Kind::Expense, C::Health, A, false, None),
                ("Guardar na reserva", 150_000, 6, Kind::Expense, C::Reserve, A, false, None),
                ("Plano de saúde", 69_000, 8, Kind::Expense, C::Health, A, false, None),
                ("Aluguel", 380_000, 10, Kind::Expense, C::Housing, A, true, None),
                ("Condomínio", 89_000, 10, Kind::Expense, C::Housing, B, true, None),
                ("Internet", 12_990, 12, Kind::Expense, C::Utilities, A, true, None),
                ("Luz", v(24_000, 3_850), 15, Kind::Expense, C::Utilities, B, true, None),
                ("Ajuda pra família", 80_000, 15, Kind::Expense, C::Family, A, false, None),
                ("Plano de saúde", 59_000, 8, Kind::Expense, C::Health, B, false, None),
                ("Curso de inglês", 45_000, 12, Kind::Expense, C::Other, B, false, None),
                ("Mercado do mês", v(138_000, 6_420), 3, Kind::Expense, C::Groceries, B, true, Some(1)),
                ("Farmácia", 18_040, 14, Kind::Expense, C::Health, B, true, Some(1)),
                ("Restaurantes", v(38_000, 4_200), 9, Kind::Expense, C::Leisure, A, false, Some(0)),
                ("Transporte por aplicativo", 26_000, 11, Kind::Expense, C::Transport, A, false, Some(0)),
                ("Streaming", 5_590, 2, Kind::Expense, C::Leisure, A, false, Some(0)),
            ];
            for (desc, amount, day, kind, category, owner, shared, card) in rows {
                let date = m.day(day);
                // Passado pago; no mês atual só até o dia 6 (o plano de saúde do dia 8 fica vencido).
                let paid = m < Month::of(DEMO_TODAY) || (m == Month::of(DEMO_TODAY) && day <= 6);
                // Contas fixas de moradia, utilidades, saúde e reserva alimentam a previsão dos meses futuros.
                let recurring = card.is_none() && kind == Kind::Expense && matches!(category, C::Housing | C::Utilities | C::Health | C::Reserve);
                d.upsert(Entry { id: 0, desc: desc.into(), amount, date, kind, category, owner, shared, card, connected: None, imported: false, paid, recurring });
            }
            if k < 0 {
                d.paid_invoices.extend([(0, m), (1, m)]);
            }
        }
        // Viagem com os amigos: PIX manual parcelado — parcela 2 de 4 agora, o resto nos meses que vêm.
        let nov = Month::of(DEMO_TODAY);
        for (n, m, paid) in [(2, nov, true), (3, nov.add(1), false), (4, nov.add(2), false)] {
            d.upsert(Entry { id: 0, desc: format!("Viagem com os amigos ({n}/4)"), amount: 120_000, date: m.day(20), kind: Kind::Expense, category: C::Leisure, owner: A, shared: false, card: None, connected: None, imported: false, paid, recurring: false });
        }
        // Dezembro com viagem: mês de saldo negativo.
        let dec = Month::of(DEMO_TODAY).add(1);
        for (desc, amount, day, cat) in [
            ("Viagem de fim de ano — passagens e hospedagem em Florianópolis", 680_000, 4, C::Leisure),
            ("Presentes de Natal", 190_000, 18, C::Other),
        ] {
            d.upsert(Entry { id: 0, desc: desc.into(), amount, date: dec.day(day), kind: Kind::Expense, category: cat, owner: A, shared: false, card: Some(0), connected: None, imported: false, paid: false, recurring: false });
        }
        d
    }
}

/// Meses até a meta no ritmo `pace` por mês: `Some(0)` já atingida, `None` sem aportes.
pub fn months_to_goal(current: Cents, goal: Cents, pace: Cents) -> Option<i64> {
    let left = goal - current;
    if left <= 0 {
        Some(0)
    } else if pace <= 0 {
        None
    } else {
        Some((left + pace - 1) / pace)
    }
}

/// "AAAA-MM-DDTHH:MM…Z" (UTC) → data e "HH:MM" em São Paulo (UTC−3 fixo, como `today()`).
pub fn local_time(iso: &str) -> Option<(Date, String)> {
    let date = Date::iso(iso.get(..10)?)?;
    let h: i64 = iso.get(11..13)?.parse().ok()?;
    let min: i64 = iso.get(14..16)?.parse().ok()?;
    let minutes = days_from_civil(date) * 1440 + h * 60 + min - 3 * 60;
    let local = minutes.rem_euclid(1440);
    Some((date_from_days(minutes.div_euclid(1440)), format!("{:02}:{:02}", local / 60, local % 60)))
}

/// Dias desde 01/01/1970; inverso de `date_from_days` (days_from_civil, de Howard Hinnant).
pub fn days_from_civil(d: Date) -> i64 {
    let y = d.y as i64 - (d.m <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((d.m as i64 + 9) % 12) + 2) / 5 + d.d as i64 - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Consumo dos orçamentos mensais: (categoria, gasto de quem usa o app, orçamento), do mais
/// estourado ao mais folgado. `by_category` vem de `Summary::by_category` (parte de quem
/// usa o app) e `budgets` dos orçamentos do domicílio.
pub fn budget_usage(by_category: &[(Category, Cents)], budgets: &[crate::api::Budget]) -> Vec<(String, Cents, i64)> {
    let mut out: Vec<(String, Cents, i64)> = budgets.iter().map(|b| {
        let spent = by_category.iter().find(|(c, _)| c.key() == b.category).map_or(0, |(_, v)| *v);
        (b.category.clone(), spent, b.monthly_cents)
    }).collect();
    out.sort_by_key(|(_, spent, budget)| std::cmp::Reverse(spent * 1_000 / budget.max(&1)));
    out
}

pub fn status(paid: bool, due: Date) -> Status {
    match (paid, due < today()) {
        (true, _) => Status::Paid,
        (false, true) => Status::Overdue,
        _ => Status::Due,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOV: Month = Month { y: 2026, m: 11 };

    #[test]
    fn money_round_trip() {
        assert_eq!(money(530_428), "R$ 5.304,28");
        assert_eq!(money(-123_456_789), "−R$ 1.234.567,89");
        assert_eq!(money(5), "R$ 0,05");
        assert_eq!(parse_money("1.234,56"), Some(123_456));
        assert_eq!(parse_money("R$ 50"), Some(5_000));
        assert_eq!(parse_money("12.5"), Some(1_250));
        assert_eq!(parse_money("1.000"), Some(100_000));
        assert_eq!(parse_money("abc"), None);
        assert_eq!(parse_money(""), None);
        assert_eq!(parse_money("1,234"), None);
        assert_eq!(parse_money(&input_money(98_765)), Some(98_765));
    }

    #[test]
    fn dates() {
        assert_eq!(Date::parse("29/02/2028"), Some(Date { y: 2028, m: 2, d: 29 }));
        assert_eq!(Date::parse("29/02/2027"), None);
        assert_eq!(Date::parse("31/04/2026"), None);
        assert_eq!(Date::parse("10/11"), None);
        assert_eq!(NOV.add(2), Month { y: 2027, m: 1 });
        assert_eq!(NOV.add(-11), Month { y: 2025, m: 12 });
        assert_eq!(NOV.first_weekday(), 0); // 01/11/2026 é domingo
        assert_eq!(date_from_days(0), Date { y: 1970, m: 1, d: 1 });
        assert_eq!(date_from_days(-1), Date { y: 1969, m: 12, d: 31 });
        assert_eq!(date_from_days(20_767), DEMO_TODAY);
        assert_eq!(date_from_days(21_243), Date { y: 2028, m: 2, d: 29 });
        assert_eq!(today(), DEMO_TODAY);
    }

    #[test]
    fn halves_keep_every_cent() {
        for x in [0, 1, 99, 12_345, 380_000] {
            assert_eq!(halves(x).iter().sum::<Cents>(), x);
        }
    }

    #[test]
    fn couple_balance_is_income_minus_my_parts() {
        let d = Data::demo();
        let s = d.summary(NOV, Mode::Couple);
        let manual: Cents = d.in_month(NOV).filter(|e| e.kind == Kind::Expense).map(|e| d.my_part(e, Mode::Couple)).sum();
        assert_eq!(s.income, 1_250_000);
        assert_eq!(s.expenses, manual);
        assert_eq!(s.balance, s.income - s.shared_part - s.personal);
        assert_eq!(s.shared_part, s.owed[0]);
        assert_eq!(s.owed[0] + s.owed[1], s.shared_total);
        assert_eq!(s.paid_by[0] + s.paid_by[1], s.shared_total);
        assert_eq!(s.by_category.iter().map(|c| c.1).sum::<Cents>(), s.expenses);
        assert!(s.balance > 0);
    }

    #[test]
    fn personal_and_shared_are_separate() {
        let d = Data::demo();
        let s = d.summary(NOV, Mode::Couple);
        // Despesas pessoais de Marina não entram; as compartilhadas entram pela metade.
        let shared: Cents = d.in_month(NOV).filter(|e| e.shared).map(|e| e.amount).sum();
        let mine: Cents = d.in_month(NOV).filter(|e| e.kind == Kind::Expense && !e.shared && e.owner == d.me).map(|e| e.amount).sum();
        assert_eq!(s.shared_total, shared);
        assert_eq!(s.personal, mine);
        // Individual: só o que sai da conta de Lucas, por inteiro; sem rateio.
        let i = d.summary(NOV, Mode::Individual);
        let out: Cents = d.in_month(NOV).filter(|e| e.kind == Kind::Expense && e.owner == d.me).map(|e| e.amount).sum();
        assert_eq!(i.expenses, out);
        assert_eq!(i.shared_total, 0);
        assert_eq!(i.settlement, 0);
    }

    #[test]
    fn settlement_matches_who_paid() {
        let mut d = Data::demo();
        d.entries.clear();
        let e = |owner, amount| Entry { id: 0, desc: "x".into(), amount, date: NOV.day(1), kind: Kind::Expense, category: Category::Housing, owner, shared: true, card: None, connected: None, imported: false, paid: false, recurring: false };
        d.upsert(e(Person::A, 100_000));
        d.upsert(e(Person::B, 20_000));
        let s = d.summary(NOV, Mode::Couple);
        // Cada um deve 600; Lucas pagou 1.000 → Marina devolve 400.
        assert_eq!(s.owed, [60_000, 60_000]);
        assert_eq!(s.settlement, 40_000);
    }

    #[test]
    fn house_balance_is_cumulative_and_counts_settlements_both_ways() {
        let mut d = Data::empty();
        let e = |owner, amount, m: Month| Entry { id: 0, desc: "x".into(), amount, date: m.day(1), kind: Kind::Expense, category: Category::Housing, owner, shared: true, card: None, connected: None, imported: false, paid: true, recurring: false };
        let oct = NOV.add(-1);
        // Outubro: Lucas paga 1.000,01 (centavo ímpar fica com A: B deve 500,00).
        d.upsert(e(Person::A, 100_001, oct));
        assert_eq!(d.house_balance(oct), 50_000);
        // Novembro: Marina paga 300,01 → A deve a metade dela (150,01, com o centavo).
        d.upsert(e(Person::B, 30_001, NOV));
        assert_eq!(d.house_balance(oct), 50_000, "o mês seguinte não pesa antes");
        assert_eq!(d.house_balance(NOV), 50_000 - 15_001);
        // Despesa pessoal e receita não entram.
        d.upsert(Entry { shared: false, ..e(Person::A, 9_999, NOV) });
        assert_eq!(d.house_balance(NOV), 34_999);
        // Marina acerta 400 em novembro; Lucas devolve 100 a mais em dezembro.
        let id = d.add_settlement(Settlement { id: String::new(), date: NOV.day(20), amount: 40_000, from: Person::B });
        assert!(id.starts_with("local-"));
        d.add_settlement(Settlement { id: String::new(), date: NOV.add(1).day(2), amount: 10_000, from: Person::A });
        assert_eq!(d.house_balance(oct), 50_000);
        assert_eq!(d.house_balance(NOV), -5_001);
        assert_eq!(d.house_balance(NOV.add(1)), 4_999);
        assert_eq!(d.summary(NOV, Mode::Couple).settlement, -5_001);
        assert_eq!(d.summary(NOV, Mode::Individual).settlement, 0);
        d.settlements.retain(|s| s.id != id);
        assert_eq!(d.house_balance(NOV), 34_999);
    }

    #[test]
    fn reserve_is_household_wide_in_couple_and_projects_the_goal() {
        let mut d = Data::empty();
        d.reserve_start = 100_000;
        d.reserve_goal = 1_000_000;
        let dep = |owner, amount, m: Month, paid| Entry { id: 0, desc: "Reserva".into(), amount, date: m.day(5), kind: Kind::Expense, category: Category::Reserve, owner, shared: false, card: None, connected: None, imported: false, paid, recurring: false };
        d.upsert(dep(Person::A, 30_000, NOV.add(-2), true));
        d.upsert(dep(Person::B, 60_000, NOV.add(-1), true));
        d.upsert(dep(Person::A, 0, NOV, true));
        d.upsert(dep(Person::A, 50_000, NOV, false)); // não pago: não conta
        d.upsert(dep(Person::A, 70_000, NOV.add(-5), true)); // fora da média
        assert_eq!(d.summary(NOV, Mode::Couple).reserve, 100_000 + 160_000);
        assert_eq!(d.summary(NOV, Mode::Individual).reserve, 100_000 + 100_000);
        let months = d.monthly_deposits(NOV, Mode::Couple, 12);
        assert_eq!(months.len(), 12);
        assert_eq!(months.last().unwrap(), &(NOV, 0));
        assert_eq!(months.iter().map(|m| m.1).sum::<Cents>(), 160_000);
        assert_eq!(d.reserve_pace(NOV, Mode::Couple), 30_000);
        assert_eq!(d.reserve_pace(NOV, Mode::Individual), 10_000);
        // Faltam 740.000 a 30.000/mês → 25 meses (arredonda para cima).
        assert_eq!(months_to_goal(260_000, 1_000_000, 30_000), Some(25));
        assert_eq!(months_to_goal(1_000_000, 1_000_000, 0), Some(0));
        assert_eq!(months_to_goal(10, 1_000_000, 0), None);
        assert_eq!(Data::empty().reserve_pace(NOV, Mode::Couple), 0);
    }

    #[test]
    fn sync_time_is_shown_in_sao_paulo() {
        assert_eq!(local_time("2026-11-10T17:30:00.000Z"), Some((NOV.day(10), "14:30".into())));
        assert_eq!(local_time("2026-11-01T01:05:00Z"), Some((Month { y: 2026, m: 10 }.day(31), "22:05".into())));
        assert_eq!(local_time("2026-03-01T02:00:00Z"), Some((Date { y: 2026, m: 2, d: 28 }, "23:00".into())));
        assert_eq!(local_time("ontem"), None);
        for days in [-1, 0, 20_767, 21_243] {
            assert_eq!(days_from_civil(date_from_days(days)), days);
        }
    }

    #[test]
    fn invoices_are_not_double_counted() {
        let d = Data::demo();
        for mode in [Mode::Couple, Mode::Individual] {
            let s = d.summary(NOV, mode);
            // Nenhuma compra no cartão aparece como conta própria; a fatura aparece uma vez.
            assert!(s.bills.iter().all(|b| match b.target {
                BillRef::Entry(id) => d.entry(id).unwrap().card.is_none(),
                BillRef::Invoice(_) | BillRef::Connected(_) => true,
            }));
            let card_mine: Cents = d.in_month(NOV).filter(|e| e.card == Some(0)).map(|e| e.amount).sum();
            let inv = s.bills.iter().find(|b| b.target == BillRef::Invoice(0)).unwrap();
            assert_eq!(inv.amount, card_mine);
            // As contas de Lucas somam exatamente o que sai da conta dele.
            let out: Cents = d.in_month(NOV).filter(|e| e.kind == Kind::Expense && e.owner == d.me).map(|e| e.amount).sum();
            assert_eq!(s.bills.iter().map(|b| b.amount).sum::<Cents>(), out);
        }
    }

    #[test]
    fn connected_invoice_is_a_bill_of_its_owner_only_and_toggles() {
        let mut d = Data::demo();
        let before = d.summary(NOV, Mode::Couple);
        let inv = |owner, due| ConnectedInvoice { card: "c".into(), label: "Fatura Nubank".into(), owner, amount: 12_345, due, paid: false };
        d.connected_invoices = vec![inv(Person::A, NOV.day(25)), inv(Person::B, NOV.day(20)), inv(Person::A, NOV.add(1).day(25))];
        let s = d.summary(NOV, Mode::Couple);
        assert_eq!(s.bills.len(), before.bills.len() + 1);
        assert_eq!(s.to_pay, before.to_pay + 12_345);
        assert_eq!((s.expenses, s.balance), (before.expenses, before.balance));
        d.toggle(BillRef::Connected(0), NOV);
        assert!(d.connected_invoices[0].paid && !d.connected_invoices[2].paid);
        assert_eq!(d.summary(NOV, Mode::Couple).to_pay, before.to_pay);
        assert_eq!(Date::iso("2026-11-25"), Some(NOV.day(25)));
        assert_eq!(Date::iso("2026-02-30"), None);
    }

    #[test]
    fn connected_card_purchases_are_expenses_but_not_bills() {
        let mut d = Data::demo();
        let before = d.summary(NOV, Mode::Individual);
        let buy = Entry { id: 0, desc: "Mercado (1/3)".into(), amount: 30_000, date: NOV.day(4), kind: Kind::Expense, category: Category::Groceries, owner: d.me, shared: false, card: None, connected: Some("c".into()), imported: true, paid: true, recurring: false };
        let id = d.upsert(buy);
        let s = d.summary(NOV, Mode::Individual);
        assert_eq!(s.expenses, before.expenses + 30_000);
        assert_eq!(s.bills.len(), before.bills.len());
        assert!(s.bills.iter().all(|b| b.target != BillRef::Entry(id)));
        assert_eq!(Origin::of(d.entry(id).unwrap()), Origin::Card);
        // Editar devolve o próprio id.
        let edited = Entry { amount: 31_000, ..d.entry(id).unwrap().clone() };
        assert_eq!(d.upsert(edited), id);
    }

    #[test]
    fn copy_previous_month_brings_manual_entries_once() {
        let mut d = Data::demo();
        let dec = NOV.add(1);
        let bank = Entry { id: 0, desc: "PIX".into(), amount: 5_000, date: NOV.day(31), kind: Kind::Expense, category: Category::Other, owner: Person::A, shared: false, card: None, connected: None, imported: true, paid: true, recurring: false };
        d.upsert(bank.clone());
        d.upsert(Entry { connected: Some("c".into()), desc: "Loja".into(), ..bank });
        let manual: Vec<Entry> = d.in_month(NOV).filter(|e| Origin::of(e) == Origin::Manual).cloned().collect();
        let already = d.in_month(dec).filter(|e| manual.iter().any(|m| m.desc == e.desc && m.amount == e.amount && m.owner == e.owner)).count();
        let ids = d.copy_previous(dec, None);
        assert_eq!(ids.len(), manual.len() - already);
        for id in &ids {
            let e = d.entry(*id).unwrap();
            assert!(!e.paid && Month::of(e.date) == dec && Origin::of(e) == Origin::Manual);
        }
        assert!(d.in_month(dec).all(|e| e.desc != "PIX" && e.desc != "Loja"));
        // Segunda vez não duplica; individual copia só os de quem usa o app.
        assert!(d.copy_previous(dec, None).is_empty());
        let jan = dec.add(1);
        let mine = d.copy_previous(jan, Some(Person::B));
        assert!(!mine.is_empty() && mine.iter().all(|id| d.entry(*id).unwrap().owner == Person::B));
        // Dia 31 vira o último dia do mês seguinte.
        let mut d = Data::empty();
        d.upsert(Entry { id: 0, desc: "Aluguel".into(), amount: 1, date: Month { y: 2027, m: 1 }.day(31), kind: Kind::Expense, category: Category::Housing, owner: Person::A, shared: true, card: None, connected: None, imported: false, paid: true, recurring: false });
        let id = d.copy_previous(Month { y: 2027, m: 2 }, None)[0];
        assert_eq!(d.entry(id).unwrap().date, Date { y: 2027, m: 2, d: 28 });
    }

    #[test]
    fn installments_track_the_latest_parcel_and_what_is_left() {
        assert_eq!(split_installment("Loja (2/10)"), Some(("Loja".into(), 2, 10)));
        assert_eq!(split_installment("Loja - Parcela 3/4"), Some(("Loja".into(), 3, 4)));
        assert_eq!(split_installment("Loja 1/1"), None);
        assert_eq!(split_installment("Loja 5/3"), None);
        assert_eq!(split_installment("Mercado"), None);
        let mut d = Data::empty();
        let e = |desc: &str, amount, date| Entry { id: 0, desc: desc.into(), amount, date, kind: Kind::Expense, category: Category::Shopping, owner: Person::A, shared: false, card: None, connected: Some("c".into()), imported: true, paid: true, recurring: false };
        d.upsert(e("TV (1/3)", 10_000, NOV.add(-1).day(5)));
        d.upsert(e("TV (2/3)", 10_000, NOV.day(5)));
        d.upsert(e("Sofá 1/2", 5_000, NOV.add(-6).day(5)));
        d.upsert(e("Fone (3/3)", 2_000, NOV.day(9)));
        let list = d.installments(NOV, None);
        assert_eq!(list.len(), 1, "Sofá parou há meses e Fone terminou");
        let tv = &list[0];
        assert_eq!((tv.desc.as_str(), tv.n, tv.left(), tv.remaining(), tv.ends()), ("TV", 2, 1, 10_000, NOV.add(1)));
        assert!(tv.due_in(NOV.add(1)) && !tv.due_in(NOV) && !tv.due_in(NOV.add(2)));
        // Visto de outubro, a TV ainda tem duas parcelas por vir.
        assert_eq!(d.installments(NOV.add(-1), None)[0].remaining(), 20_000);
        assert!(d.installments(NOV, Some(Person::B)).is_empty());
    }

    #[test]
    fn demo_states() {
        let mut d = Data::demo();
        let s = d.summary(NOV, Mode::Couple);
        assert!(s.overdue_count >= 1 && s.paid_count >= 1);
        assert!(d.summary(NOV.add(1), Mode::Couple).balance < 0);
        assert!(d.summary(NOV.add(3), Mode::Couple).empty);
        // Pagar atualiza resumo e reserva.
        let res = s.reserve;
        let id = d.in_month(NOV).find(|e| e.category == Category::Reserve).unwrap().id;
        d.toggle(BillRef::Entry(id), NOV);
        let s2 = d.summary(NOV, Mode::Couple);
        assert_eq!(s2.paid_count, s.paid_count - 1);
        assert_eq!(s2.reserve, res - 150_000);
        d.toggle(BillRef::Invoice(0), NOV);
        assert_eq!(d.summary(NOV, Mode::Couple).paid_count, s.paid_count);
    }

    #[test]
    fn shared_expenses_follow_the_income_share() {
        let mut d = Data::empty();
        let inc = |owner, amount, m: Month| Entry { id: 0, desc: "Salário".into(), amount, date: m.day(5), kind: Kind::Income, category: Category::Salary, owner, shared: false, card: None, connected: None, imported: false, paid: true, recurring: false };
        let rent = |amount| Entry { id: 0, desc: "Aluguel".into(), amount, date: NOV.day(10), kind: Kind::Expense, category: Category::Housing, owner: Person::A, shared: true, card: None, connected: None, imported: false, paid: true, recurring: false };
        // Renda 90/10 nos três meses: o aluguel de 1.000 divide 900/100, sem perder centavo.
        for m in [NOV.add(-2), NOV.add(-1), NOV] {
            d.upsert(inc(Person::A, 3_000_000, m));
            d.upsert(inc(Person::B, 333_333, m));
        }
        let id = d.upsert(rent(1_000_000));
        let e = d.entry(id).unwrap().clone();
        let split = d.shared_split(&e);
        assert_eq!(split.iter().sum::<Cents>(), 1_000_000);
        assert_eq!(split, [900_000, 100_000]);
        let s = d.summary(NOV, Mode::Couple);
        assert_eq!(s.owed, split);
        // Quem pagou tudo adiantou só a parte do outro: Marina deve 100 a Lucas.
        assert_eq!(d.house_balance(NOV), 100_000);
        assert_eq!(s.settlement, 100_000);
        // Marina (10%) custa 100 para o orçamento dela; Lucas, o resto.
        assert_eq!(d.my_part(&e, Mode::Couple), 900_000);
        d.me = Person::B;
        assert_eq!(d.my_part(&e, Mode::Couple), 100_000);
        // Rateio desligado: a despesa é de quem pagou, o saldo entre eles zera.
        d.me = Person::A;
        d.shared_split_off = true;
        assert_eq!(d.shared_split(&e), [1_000_000, 0]);
        assert_eq!(d.my_part(&e, Mode::Couple), 1_000_000);
        assert_eq!(d.summary(NOV, Mode::Couple).settlement, 0);
        assert_eq!(d.house_balance(NOV), 0);
        d.shared_split_off = false;
        // Sem renda registrada: volta a metade para cada um.
        let mut none = Data::empty();
        none.upsert(rent(101));
        assert_eq!(none.shared_split(none.entry(1 << 31).unwrap()), [51, 50]);
        // A divisão usa a renda do mês da despesa: aluguel antigo segue o rateio de antes.
        d.upsert(Entry { date: NOV.add(-4).day(10), ..rent(1_000_000) });
        let old = d.entries.iter().find(|e| Month::of(e.date) == NOV.add(-4)).unwrap().clone();
        assert_eq!(d.shared_split(&old), halves(1_000_000), "sem renda na janela de 3 meses, é metade");
    }

    #[test]
    fn recurring_forecast_estimates_the_months_ahead() {
        let mut d = Data::empty();
        let luz = |amount, m: Month| Entry { id: 0, desc: "Luz".into(), amount, date: m.day(15), kind: Kind::Expense, category: Category::Utilities, owner: Person::A, shared: true, card: None, connected: None, imported: false, paid: true, recurring: true };
        d.upsert(luz(10_000, NOV.add(-2)));
        d.upsert(luz(12_000, NOV.add(-1)));
        d.upsert(luz(11_000, NOV));
        let defs = d.recurring_defs(NOV, None);
        assert_eq!(defs.len(), 1);
        let def = &defs[0];
        assert_eq!((def.estimate, def.min, def.max, def.day, def.samples, def.category), (11_000, 10_000, 12_000, 15, 3, Category::Utilities));
        assert_eq!(def.range(), "entre R$ 100,00 e R$ 120,00");
        // Novembro já tem a conta; dezembro não: é ela que aparece na previsão.
        assert!(d.forecast(NOV, None).is_empty());
        assert_eq!(d.forecast(NOV.add(1), None)[0].estimate, 11_000);
        // A previsão de quem é só de quem usa o app no modo individual.
        assert!(d.forecast(NOV.add(1), Some(Person::B)).is_empty());
        // Lançado o previsto, sai da lista e nasce marcado como recorrente.
        let id = d.upsert(def.entry(NOV.add(1)));
        let made = d.entry(id).unwrap();
        assert!(made.recurring && made.amount == def.estimate && made.date == NOV.add(1).day(15) && !made.paid);
        assert!(d.forecast(NOV.add(1), None).is_empty());
        // Compra no cartão não vira previsão: o pagamento dela é a fatura.
        d.upsert(Entry { card: Some(0), recurring: true, desc: "Streaming".into(), ..luz(5_000, NOV) });
        assert!(!d.recurring_defs(NOV, None).iter().any(|f| f.desc == "Streaming"));
        // O dia cai no fim do mês quando o típico não existe nele (31/01 → 28/02).
        let mut d = Data::empty();
        d.upsert(Entry { id: 0, desc: "Aluguel".into(), amount: 500_000, date: Month { y: 2027, m: 1 }.day(31), kind: Kind::Expense, category: Category::Housing, owner: Person::A, shared: false, card: None, connected: None, imported: false, paid: true, recurring: true });
        assert_eq!(d.forecast(Month { y: 2027, m: 2 }, None)[0].entry(Month { y: 2027, m: 2 }).date, Date { y: 2027, m: 2, d: 28 });
    }

    #[test]
    fn invoice_month_sends_the_closing_day_to_the_next_invoice() {
        // Fechamento dia 20: compra no dia cai na fatura seguinte; na véspera, na do mês.
        assert_eq!(invoice_month(NOV.day(20), 20), NOV.add(1));
        assert_eq!(invoice_month(NOV.day(19), 20), NOV);
        assert_eq!(invoice_month(NOV.day(30), 20), NOV.add(1));
        // Fechamento dia 31 em fevereiro: o dia clampado é 28 (2026 não é bissexto).
        assert_eq!(invoice_month(Date { y: 2026, m: 2, d: 28 }, 31), Month { y: 2026, m: 3 });
        assert_eq!(invoice_month(Date { y: 2026, m: 2, d: 27 }, 31), Month { y: 2026, m: 2 });
        // Fechamento 0/inválido: mês civil.
        assert_eq!(invoice_month(NOV.day(31), 0), NOV);
        assert_eq!(invoice_month(NOV.day(1), 32), NOV);
    }

    #[test]
    fn invoice_window_runs_from_the_previous_closing_to_the_cycle_one() {
        // Fechamento 25 com vencimento dia 5: a fatura de janeiro fechou em dezembro.
        assert_eq!(invoice_window(Month { y: 2027, m: 1 }, 25, Some(5)), (Date { y: 2026, m: 11, d: 25 }, Date { y: 2026, m: 12, d: 25 }));
        // Vencimento depois do fechamento: o ciclo fecha no próprio mês.
        assert_eq!(invoice_window(NOV, 20, Some(25)), (Date { y: 2026, m: 10, d: 20 }, Date { y: 2026, m: 11, d: 20 }));
        // Vencimento igual ao fechamento (ou sem vencimento): ciclo no mês anterior.
        assert_eq!(invoice_window(NOV, 20, Some(20)), (Date { y: 2026, m: 9, d: 20 }, Date { y: 2026, m: 10, d: 20 }));
        assert_eq!(invoice_window(NOV, 20, None), (Date { y: 2026, m: 9, d: 20 }, Date { y: 2026, m: 10, d: 20 }));
        // Fechamento dia 31 clampado no mês curto: a janela de março termina em 28/02.
        assert_eq!(invoice_window(Month { y: 2027, m: 3 }, 31, Some(5)), (Date { y: 2027, m: 1, d: 31 }, Date { y: 2027, m: 2, d: 28 }));
        // A compra pertence à fatura de `m` se cai na janela: fechamento entra na seguinte.
        let (start, end) = invoice_window(NOV, 20, Some(25));
        for day in [21, 31] {
            assert!(invoice_month(Date { y: 2026, m: 10, d: day }, 20) == NOV && Date { y: 2026, m: 10, d: day } >= start && Date { y: 2026, m: 10, d: day } < end);
        }
        for day in [1, 19] {
            assert!(invoice_month(NOV.day(day), 20) == NOV && NOV.day(day) >= start && NOV.day(day) < end);
        }
        assert!(invoice_month(NOV.day(20), 20) == NOV.add(1) && !(NOV.day(20) >= start && NOV.day(20) < end));
    }

    #[test]
    fn connected_purchases_follow_the_closing_window() {
        let mut d = Data::empty();
        let buy = |day: u32| Entry { id: 0, desc: "Loja".into(), amount: 1_000, date: NOV.day(day), kind: Kind::Expense, category: Category::Shopping, owner: Person::A, shared: false, card: None, connected: Some("c".into()), imported: true, paid: true, recurring: false };
        for day in [5, 19, 20, 25] {
            d.upsert(buy(day));
        }
        // Sem fechamento: mês civil da compra.
        assert_eq!(d.connected_purchases(NOV, "c", None, Some(25)).count(), 4);
        // Com fechamento 20 e vencimento 25: janela (out 20, nov 20) — o dia 20 já é da fatura seguinte.
        let in_window: Vec<u32> = d.connected_purchases(NOV, "c", Some(20), Some(25)).map(|e| e.date.d).collect();
        assert_eq!(in_window, [5, 19]);
        // As fechadas em novembro formam a fatura de dezembro.
        let next: Vec<u32> = d.next_invoice_purchases(NOV, "c", Some(20)).map(|e| e.date.d).collect();
        assert_eq!(next, [20, 25]);
        // Sem fechamento não há "próxima fatura" para mostrar.
        assert_eq!(d.next_invoice_purchases(NOV, "c", None).count(), 0);
        // Outro cartão e fechamento inválido valem mês civil.
        d.upsert(Entry { connected: Some("outro".into()), ..buy(19) });
        assert_eq!(d.connected_purchases(NOV, "c", Some(0), None).count(), 4);
        assert_eq!(d.connected_purchases(NOV, "outro", Some(20), Some(25)).count(), 1);
    }

    #[test]
    fn days_to_closing_counts_down_and_hits_zero_on_the_day() {
        assert_eq!(days_to_closing(20, NOV.day(10)), 10);
        assert_eq!(days_to_closing(20, NOV.day(20)), 0);
        assert_eq!(days_to_closing(20, NOV.day(21)), 29);
        // Clamp: fechamento dia 31 em fevereiro vira 28 (2026 não é bissexto).
        assert_eq!(days_to_closing(31, Date { y: 2026, m: 2, d: 27 }), 1);
        assert_eq!(days_to_closing(0, NOV.day(1)), 0);
    }

    #[test]
    fn billed_windows_prefer_the_real_closing_dates() {
        let bill = |due: &str, closing: Option<&str>| ConnectedBill {
            due: due.into(), closing: closing.map(str::to_owned), total: None, currency: "BRL".into(), paid: false,
        };
        let bills = vec![
            bill("2026-10-25", Some("2026-09-20")),
            bill("2026-11-25", Some("2026-10-20")),
            bill("2026-12-25", None),
        ];
        // Fatura que vence em novembro fechou em 20/10 e abre no fechamento anterior (20/09).
        assert_eq!(invoice_window_billed(&bills, NOV, Some(20), Some(25)), Some((NOV.add(-2).day(20), NOV.add(-1).day(20))));
        // A de dezembro perdeu o closing na bill: sintetiza, mas abre no fechamento real anterior.
        assert_eq!(invoice_window_billed(&bills, NOV.add(1), Some(20), Some(25)), Some((NOV.add(-1).day(20), NOV.add(1).day(20))));
        // Sem bill vencendo no mês: nada a mostrar.
        assert_eq!(invoice_window_billed(&bills, Month { y: 2027, m: 1 }, Some(20), Some(25)), None);
    }

    #[test]
    fn the_forming_invoice_runs_from_the_last_closing_to_the_next_one() {
        let mut d = Data::empty();
        let bills = [ConnectedBill {
            due: "2026-11-25".into(), closing: Some("2026-10-20".into()), total: None, currency: "BRL".into(), paid: false,
        }];
        let buy = |(m, day): (u32, u32)| Entry {
            id: 0, desc: "Loja".into(), amount: 1_000, date: Date { y: 2026, m, d: day }, kind: Kind::Expense,
            category: Category::Shopping, owner: Person::A, shared: false, card: None, connected: Some("c".into()),
            imported: true, paid: true, recurring: false,
        };
        for day in [(10, 15), (10, 20), (11, 10)] {
            d.upsert(buy(day));
        }
        // Em 12/11, a fatura em formação vai de 20/10 (fechado) até 19/12.
        let w = forming_invoice_window(&bills, Some(20), Date { y: 2026, m: 11, d: 12 }).unwrap();
        let forming: Vec<(u32, u32)> = d.connected_purchases_in("c", w).map(|e| (e.date.m, e.date.d)).collect();
        assert_eq!(forming, [(10, 20), (11, 10)]);
        // No dia do fechamento, a formação já começa nele (a compra do dia entra nela).
        let w = forming_invoice_window(&bills, Some(20), Date { y: 2026, m: 10, d: 20 }).unwrap();
        assert_eq!(w.0, Date { y: 2026, m: 10, d: 20 });
        // Sem fechamento informado, não há "fatura em formação".
        assert!(forming_invoice_window(&bills, None, Date { y: 2026, m: 11, d: 12 }).is_none());
    }

    #[test]
    fn projected_invoices_sum_each_future_window() {
        let mut d = Data::empty();
        // Banco com vencimento 5 dias após o fechamento: fatura vencendo em M fecha em M (dia 20).
        let bills: Vec<ConnectedBill> = [("", ""), ("2026-11-25", "2026-11-20")].into_iter()
            .skip(1)
            .map(|(due, closing)| ConnectedBill {
                due: due.into(), closing: Some(closing.into()), total: None, currency: "BRL".into(), paid: false,
            })
            .collect();
        let buy = |(m, day): (u32, u32), amount: Cents| Entry {
            id: 0, desc: "Loja".into(), amount, date: Date { y: 2026, m, d: day }, kind: Kind::Expense,
            category: Category::Shopping, owner: Person::A, shared: false, card: None, connected: Some("c".into()),
            imported: true, paid: true, recurring: false,
        };
        for (day, amount) in [((11, 25), 500), ((12, 5), 4_000), ((12, 18), 700), ((12, 25), 6_000)] {
            d.upsert(buy(day, amount));
        }
        // Projeção olha os PRÓXIMOS vencimentos (dez, jan) — a de novembro já é a "do mês".
        // Dezembro sem bill ainda: janela sintetizada [20/11, 20/12) pega 25/11 + 05/12 + 18/12.
        // Janeiro fecha em 20/12: pega só a compra de 25/12.
        assert_eq!(
            d.next_invoices("c", &bills, Some(20), Some(25), NOV, 2),
            [(NOV.add(1), 5_200), (NOV.add(2), 6_000)]
        );
        // Com a bill de dezembro publicada fechando em 18/12, a compra do dia 18 sai da janela.
        let mut com_dez = bills.clone();
        com_dez.push(ConnectedBill {
            due: "2026-12-25".into(), closing: Some("2026-12-18".into()), total: None, currency: "BRL".into(), paid: false,
        });
        assert_eq!(
            d.next_invoices("c", &com_dez, Some(20), Some(25), NOV, 2),
            [(NOV.add(1), 4_500), (NOV.add(2), 6_000)]
        );
    }

    #[test]
    fn price_alerts_flag_recurring_bills_that_jump() {
        let mut d = Data::empty();
        let conta = |m: u32, cents: Cents| Entry {
            id: 0, desc: "Luz".into(), amount: cents, date: Date { y: 2026, m, d: 10 }, kind: Kind::Expense,
            category: Category::Utilities, owner: Person::A, shared: false, card: None, connected: None,
            imported: false, paid: true, recurring: true,
        };
        for (m, v) in [(8, 100_00), (9, 110_00), (10, 190_00)] {
            d.upsert(conta(m, v));
        }
        // Última ocorrência (190) foge 42% da média (133).
        assert_eq!(d.price_alerts(NOV, 0.20), [("Luz".into(), 190_00, 133_33)]);
        // Com tolerância maior, nada a sinalizar.
        assert!(d.price_alerts(NOV, 0.50).is_empty());
    }

    #[test]
    fn budget_usage_ranks_by_fraction_spent() {
        let by_cat = vec![(Category::Groceries, 500_00), (Category::Transport, 90_00)];
        let budgets = vec![
            crate::api::Budget { category: "transport".into(), monthly_cents: 100_00 },
            crate::api::Budget { category: "groceries".into(), monthly_cents: 1_000_00 },
        ];
        // Transporte a 90% vem antes do mercado a 50%.
        assert_eq!(
            budget_usage(&by_cat, &budgets),
            [("transport".into(), 90_00, 100_00), ("groceries".into(), 500_00, 1_000_00)]
        );
    }
}
