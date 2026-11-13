// Dados de demonstração do Flounder mobile — mesmos números do mockup (Dezembro 2026).
// Fase 2 troca este módulo pela API do backend; as telas só conhecem esta forma.

const MEMBERS = [
  { pos: 0, name: "Felipe", color: "#2f6cff" },
  { pos: 1, name: "Larissa", color: "#f04438" },
];

// chaves "a-m": a = ano, m = mês 1..12.
const MESES = {
  "2026-12": {
    salarioCents: 2_800_000,
    saldo: { centavos: 530_428, pctLivre: 19 },
    reparticao: [
      { label: "Sua meta da casa", centavos: 766_622, pct: 27, color: "#173d9c" },
      { label: "Contas pessoais", centavos: 1_452_950, pct: 52, color: "#2f6cff" },
      { label: "Dia a dia", centavos: 50_000, pct: 2, color: "#8fb0ff" },
      { label: "Sobra", centavos: 530_428, pct: 18, color: "#12b76a" },
    ],
    contasAPagar: {
      dia: 5,
      totalCents: 1_937_950,
      dias: [
        { dia: 5, qtd: 4, totalCents: 1_937_950 },
        { dia: 8, qtd: 1, totalCents: 84_900 },
        { dia: 10, qtd: 2, totalCents: 205_400 },
        { dia: 12, qtd: 1, totalCents: 559_000 },
        { dia: 15, qtd: 2, totalCents: 1_438_200 },
        { dia: 20, qtd: 1, totalCents: 63_000 },
        { dia: 25, qtd: 1, totalCents: 129_900 },
      ],
    },
    cartoes: {
      totalFaturasCents: 982_970,
      cards: [
        {
          banco: "Santander Unique", cor: "#ea1d25", aberta: true, fechaDias: 18, venceDia: 5,
          faturaLabel: "Fatura de dezembro", totalCents: 799_970, compras: 7,
          disponivelCents: 1_387_030, limiteCents: 6_500_000,
        },
        {
          banco: "Nubank", cor: "#820ad1", aberta: true, fechaDias: 24, venceDia: 12,
          faturaLabel: "Fatura de dezembro", totalCents: 183_000, compras: 4,
          disponivelCents: 530_000, limiteCents: 1_000_000,
        },
      ],
    },
    casa: {
      totalContas: 7,
      pendentes: { qtd: 7, totalCents: 1_533_244 },
      bills: [
        { nome: "Apartamento Vila Mariana", dia: 1, totalCents: 233_254, aproximado: true, pagador: 1, parteCents: 116_627, paga: false },
        { nome: "Aluguel", dia: 1, totalCents: 480_000, pagador: 1, parteCents: 240_000, paga: false },
        { nome: "Condomínio", dia: 1, totalCents: 158_000, pagador: 0, parteCents: 79_000, paga: false },
        { nome: "Luz", dia: 5, totalCents: 34_000, pagador: 0, parteCents: 17_000, paga: false },
        { nome: "Internet", dia: 10, totalCents: 12_990, pagador: 0, parteCents: 6_495, paga: false },
        { nome: "Plano de saúde", dia: 15, totalCents: 560_300, pagador: 1, parteCents: 280_150, paga: false },
        { nome: "Água", dia: 20, totalCents: 54_700, pagador: 0, parteCents: 27_350, paga: false },
      ],
    },
    fluxo: {
      dias: [
        { dia: 1, entradas: [
          { desc: "Apartamento Vila Mariana", centavos: -233_254, membro: 1, shared: true, paid: true },
          { desc: "Aluguel", centavos: -480_000, membro: 1, shared: true, paid: true },
          { desc: "Condomínio", centavos: -158_000, membro: 0, shared: true, paid: true },
        ] },
        { dia: 3, entradas: [{ desc: "iFood", centavos: -8_460, membro: 0, card: "Santander Unique" }] },
        { dia: 5, entradas: [
          { desc: "Salário", centavos: 2_800_000, membro: 0, paid: true },
          { desc: "Luz", centavos: -34_000, membro: 0, shared: true, paid: true },
        ] },
        { dia: 6, entradas: [{ desc: "Mercado do bairro", centavos: -31_270, membro: 1, card: "Santander Unique" }] },
        { dia: 10, entradas: [
          { desc: "Internet", centavos: -12_990, membro: 0, shared: true, paid: true },
          { desc: "Netflix", centavos: -5_590, membro: 1, card: "Nubank" },
        ] },
        { dia: 12, entradas: [
          { desc: "Freelance de design", centavos: 150_000, membro: 1, paid: true },
          { desc: "Farmácia", centavos: -8_730, membro: 0, card: "Nubank" },
        ] },
        { dia: 15, entradas: [{ desc: "Plano de saúde", centavos: -560_300, membro: 1, shared: true, paid: false }] },
        { dia: 20, entradas: [{ desc: "Água", centavos: -54_700, membro: 0, shared: true, paid: false }] },
      ],
    },
  },
  "2026-11": {
    salarioCents: 2_800_000,
    saldo: { centavos: 312_650, pctLivre: 11 },
    reparticao: [
      { label: "Sua meta da casa", centavos: 766_622, pct: 27, color: "#173d9c" },
      { label: "Contas pessoais", centavos: 1_401_878, pct: 50, color: "#2f6cff" },
      { label: "Dia a dia", centavos: 318_850, pct: 12, color: "#8fb0ff" },
      { label: "Sobra", centavos: 312_650, pct: 11, color: "#12b76a" },
    ],
    contasAPagar: {
      dia: 1,
      totalCents: 871_254,
      dias: [
        { dia: 1, qtd: 3, totalCents: 871_254 },
        { dia: 5, qtd: 1, totalCents: 34_000 },
      ],
    },
    cartoes: {
      totalFaturasCents: 921_400,
      cards: [
        { banco: "Santander Unique", cor: "#ea1d25", aberta: false, fechaDias: 0, venceDia: 5, faturaLabel: "Fatura de novembro", totalCents: 742_100, compras: 9, disponivelCents: 1_451_900, limiteCents: 6_500_000, paga: true },
        { banco: "Nubank", cor: "#820ad1", aberta: false, fechaDias: 0, venceDia: 12, faturaLabel: "Fatura de novembro", totalCents: 179_300, compras: 5, disponivelCents: 533_700, limiteCents: 1_000_000, paga: true },
      ],
    },
    casa: {
      totalContas: 7,
      pendentes: { qtd: 0, totalCents: 0 },
      bills: [
        { nome: "Apartamento Vila Mariana", dia: 1, totalCents: 233_254, aproximado: true, pagador: 1, parteCents: 116_627, paga: true },
        { nome: "Aluguel", dia: 1, totalCents: 480_000, pagador: 1, parteCents: 240_000, paga: true },
        { nome: "Condomínio", dia: 1, totalCents: 158_000, pagador: 0, parteCents: 79_000, paga: true },
        { nome: "Luz", dia: 5, totalCents: 34_000, pagador: 0, parteCents: 17_000, paga: true },
        { nome: "Internet", dia: 10, totalCents: 12_990, pagador: 0, parteCents: 6_495, paga: true },
        { nome: "Plano de saúde", dia: 15, totalCents: 560_300, pagador: 1, parteCents: 280_150, paga: true },
        { nome: "Água", dia: 20, totalCents: 54_700, pagador: 0, parteCents: 27_350, paga: true },
      ],
    },
    fluxo: {
      dias: [
        { dia: 1, entradas: [
          { desc: "Apartamento Vila Mariana", centavos: -233_254, membro: 1, shared: true, paid: true },
          { desc: "Aluguel", centavos: -480_000, membro: 1, shared: true, paid: true },
        ] },
        { dia: 5, entradas: [
          { desc: "Salário", centavos: 2_800_000, membro: 0, paid: true },
          { desc: "Luz", centavos: -34_000, membro: 0, shared: true, paid: true },
        ] },
        { dia: 12, entradas: [{ desc: "Presente de aniversário", centavos: -12_000, membro: 1 }] },
      ],
    },
  },
};

export const DATA = {
  members: MEMBERS,
  MESES,
  /** Dados do mês; `null` para meses sem nada (as telas mostram vazio). */
  month(y, m) {
    return MESES[`${y}-${String(m).padStart(2, "0")}`] ?? null;
  },
};

export const MONTH_NAMES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
