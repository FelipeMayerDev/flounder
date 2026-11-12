//! Estado e cálculos. Nada aqui conhece egui.

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

/// Divide centavos sem perder nenhum: A fica com o centavo ímpar.
pub fn halves(x: Cents) -> [Cents; 2] {
    [x - x / 2, x / 2]
}

impl Data {
    pub fn in_month(&self, m: Month) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| Month::of(e.date) == m)
    }

    /// Parte da despesa que pesa no orçamento de quem usa o app.
    pub fn my_part(&self, e: &Entry, mode: Mode) -> Cents {
        match (mode, e.shared) {
            (Mode::Couple, true) => halves(e.amount)[self.me.idx()],
            _ if e.owner == self.me => e.amount,
            _ => 0,
        }
    }

    pub fn invoice(&self, card: usize, m: Month) -> Invoice {
        // ponytail: compras do mês caem na fatura do mesmo mês; fechamento real vem com a tela de Cartões.
        Invoice {
            card,
            total: self.in_month(m).filter(|e| e.card == Some(card)).map(|e| e.amount).sum(),
            due: m.day(self.cards[card].due_day),
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
                for (o, h) in s.owed.iter_mut().zip(halves(e.amount)) {
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
    /// adianta a metade do outro (`halves`); cada acerto abate o que quem pagou devia.
    /// > 0: B deve a A; < 0: A deve a B.
    // ponytail: soma desde o primeiro lançamento a cada quadro; guardar por mês se ficar lento.
    pub fn house_balance(&self, m: Month) -> Cents {
        let shared = self.entries.iter().filter(|e| e.shared && e.kind == Kind::Expense && Month::of(e.date) <= m).map(|e| match e.owner {
            Person::A => halves(e.amount)[1],
            Person::B => -halves(e.amount)[0],
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
            next_id: 1 << 31,
        }
    }

    pub fn demo() -> Data {
        let mut d = Data {
            names: ["Lucas".into(), "Marina".into()],
            cards: vec![
                Card { name: "Azul".into(), owner: Person::A, due_day: 25, limit: 1_500_000 },
                Card { name: "da casa".into(), owner: Person::B, due_day: 20, limit: 800_000 },
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
                d.upsert(Entry { id: 0, desc: desc.into(), amount, date, kind, category, owner, shared, card, connected: None, imported: false, paid });
            }
            if k < 0 {
                d.paid_invoices.extend([(0, m), (1, m)]);
            }
        }
        // Dezembro com viagem: mês de saldo negativo.
        let dec = Month::of(DEMO_TODAY).add(1);
        for (desc, amount, day, cat) in [
            ("Viagem de fim de ano — passagens e hospedagem em Florianópolis", 680_000, 4, C::Leisure),
            ("Presentes de Natal", 190_000, 18, C::Other),
        ] {
            d.upsert(Entry { id: 0, desc: desc.into(), amount, date: dec.day(day), kind: Kind::Expense, category: cat, owner: A, shared: false, card: Some(0), connected: None, imported: false, paid: false });
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
        let e = |owner, amount| Entry { id: 0, desc: "x".into(), amount, date: NOV.day(1), kind: Kind::Expense, category: Category::Housing, owner, shared: true, card: None, connected: None, imported: false, paid: false };
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
        let e = |owner, amount, m: Month| Entry { id: 0, desc: "x".into(), amount, date: m.day(1), kind: Kind::Expense, category: Category::Housing, owner, shared: true, card: None, connected: None, imported: false, paid: true };
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
        let dep = |owner, amount, m: Month, paid| Entry { id: 0, desc: "Reserva".into(), amount, date: m.day(5), kind: Kind::Expense, category: Category::Reserve, owner, shared: false, card: None, connected: None, imported: false, paid };
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
        let buy = Entry { id: 0, desc: "Mercado (1/3)".into(), amount: 30_000, date: NOV.day(4), kind: Kind::Expense, category: Category::Groceries, owner: d.me, shared: false, card: None, connected: Some("c".into()), imported: true, paid: true };
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
        let bank = Entry { id: 0, desc: "PIX".into(), amount: 5_000, date: NOV.day(31), kind: Kind::Expense, category: Category::Other, owner: Person::A, shared: false, card: None, connected: None, imported: true, paid: true };
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
        d.upsert(Entry { id: 0, desc: "Aluguel".into(), amount: 1, date: Month { y: 2027, m: 1 }.day(31), kind: Kind::Expense, category: Category::Housing, owner: Person::A, shared: true, card: None, connected: None, imported: false, paid: true });
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
        let e = |desc: &str, amount, date| Entry { id: 0, desc: desc.into(), amount, date, kind: Kind::Expense, category: Category::Shopping, owner: Person::A, shared: false, card: None, connected: Some("c".into()), imported: true, paid: true };
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
        assert!(d.summary(NOV.add(2), Mode::Couple).empty);
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
}
