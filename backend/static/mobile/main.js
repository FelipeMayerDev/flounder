// Flounder mobile — casca do app: abas, mês, folhas (FAB e menu) e helpers das telas.
// Fase 1 é tudo mock (data.js); fase 2 troca os dados pela API do backend.

import { DATA, MONTH_NAMES } from "./data.js";
import * as home from "./screens/home.js";
import * as flow from "./screens/flow.js";
import * as cards from "./screens/cards.js";
import * as house from "./screens/house.js";

const TELAS = { home, flow, cards, house };
const state = { tab: "home", y: 2026, m: 12, eu: 0 };

// ─── Helpers que as telas recebem no ctx ────────────────
export function fmtMoney(cents) {
  return (Math.abs(cents) / 100).toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** "R$ 5.304,28" com centavos menores; sinal vira − na frente. */
export function money(cents, { cor } = {}) {
  const neg = cents < 0;
  const s = fmtMoney(cents);
  const [inteiro, centavos] = s.split(",");
  const style = cor ? ` style="color:${cor}"` : "";
  return `<span class="money"${style}>${neg ? "−" : ""}<span class="cur">R$</span> <span class="int">${inteiro}</span><span class="cents">,${centavos}</span></span>`;
}

export function art(nome) {
  return `/m/art/${nome}.png`;
}

export function el(html) {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
}

export function toast(msg) {
  document.querySelector(".toast")?.remove();
  const t = el(`<div class="toast">${msg}</div>`);
  document.body.appendChild(t);
  setTimeout(() => t.remove(), 2200);
}

export function member(pos) {
  return DATA.members[pos];
}

// ─── Render principal ───────────────────────────────────
const app = document.getElementById("app");

function render() {
  const mes = DATA.month(state.y, state.m);
  document.getElementById("mes-title").innerHTML =
    `${MONTH_NAMES[state.m - 1]} <small>${state.y}</small>`;
  app.innerHTML = "";
  const root = el(`<div class="screen"></div>`);
  app.appendChild(root);
  const ctx = { data: DATA, mes, eu: state.eu, fmtMoney, money, art, el, toast, member, irPara };
  const tela = TELAS[state.tab];
  if (mes) {
    tela.render(root, ctx);
  } else {
    root.appendChild(el(`<div class="card"><div class="vazio"><span class="emoji">🐟</span>Nada por aqui em ${MONTH_NAMES[state.m - 1]} de ${state.y}.</div></div>`));
  }
}

function irPara(tab) {
  state.tab = tab;
  document.querySelectorAll(".tabbar .tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  render();
  window.scrollTo({ top: 0 });
}

// ─── Navegação ──────────────────────────────────────────
document.getElementById("tabbar").addEventListener("click", (e) => {
  const b = e.target.closest("[data-tab]");
  if (b) irPara(b.dataset.tab);
});
document.getElementById("mes-antes").addEventListener("click", () => {
  state.m -= 1;
  if (state.m === 0) { state.m = 12; state.y -= 1; }
  render();
});
document.getElementById("mes-depois").addEventListener("click", () => {
  state.m += 1;
  if (state.m === 13) { state.m = 1; state.y += 1; }
  render();
});
document.getElementById("bell").addEventListener("click", () => toast("Os avisos chegam na fase com API 💬"));
document.getElementById("mes-title").addEventListener("click", () => toast("Escolha de mês chega junto com a API 📆"));

// ─── FAB: novo lançamento (mock gravando em data.js em memória) ──
document.getElementById("fab").addEventListener("click", () => abrirNovo());

function fecharFolha() {
  document.querySelector(".sheet-back")?.remove();
  document.querySelector(".sheet")?.remove();
}

function abrirNovo() {
  fecharFolha();
  const mesLabel = `${MONTH_NAMES[state.m - 1]}/${state.y}`;
  document.body.appendChild(el(`<div class="sheet-back"></div>`));
  const sheet = el(`
    <div class="sheet">
      <h3>Novo lançamento</h3>
      <div class="sub">Entra no fluxo de ${mesLabel} (demo: fica só nesta abertura).</div>
      <label>Descrição</label>
      <input id="novo-desc" placeholder="Ex.: Conta de luz">
      <label>Valor (R$)</label>
      <input id="novo-valor" inputmode="decimal" placeholder="0,00">
      <div class="acoes">
        <button class="cancelar">Cancelar</button>
        <button class="salvar">Salvar</button>
      </div>
    </div>`);
  document.body.appendChild(sheet);
  sheet.querySelector(".cancelar").addEventListener("click", fecharFolha);
  document.querySelector(".sheet-back").addEventListener("click", fecharFolha);
  sheet.querySelector(".salvar").addEventListener("click", () => {
    const desc = sheet.querySelector("#novo-desc").value.trim();
    const bruto = sheet.querySelector("#novo-valor").value.trim().replace(/\./g, "").replace(",", ".");
    const centavos = Math.round(Number(bruto) * 100);
    if (!desc || !Number.isFinite(centavos) || centavos === 0) {
      toast("Preencha descrição e valor 🐟");
      return;
    }
    const chave = `${state.y}-${String(state.m).padStart(2, "0")}`;
    const mes = DATA.MESES[chave] ?? (DATA.MESES[chave] = {
      salarioCents: 0, saldo: { centavos: 0, pctLivre: 0 }, reparticao: [],
      contasAPagar: { dia: 1, totalCents: 0, dias: [] }, cartoes: { totalFaturasCents: 0, cards: [] },
      casa: { totalContas: 0, pendentes: { qtd: 0, totalCents: 0 }, bills: [] }, fluxo: { dias: [] },
    });
    const hoje = new Date().getDate();
    let dia = mes.fluxo.dias.find((d) => d.dia === hoje);
    if (!dia) { dia = { dia: hoje, entradas: [] }; mes.fluxo.dias.push(dia); mes.fluxo.dias.sort((a, b) => a.dia - b.dia); }
    dia.entradas.push({ desc, centavos, membro: state.eu, paid: false });
    fecharFolha();
    toast(`${desc} anotado ✅`);
    render();
  });
  sheet.querySelector("#novo-desc").focus();
}

// ─── Menu: quem está usando (troca o "Você") ────────────
document.getElementById("menu").addEventListener("click", () => {
  fecharFolha();
  document.body.appendChild(el(`<div class="sheet-back"></div>`));
  const sheet = el(`
    <div class="sheet">
      <h3>Quem é você?</h3>
      <div class="sub">Muda o destaque de "você" nas listas (demo).</div>
      <div class="seg" id="quem">
        ${DATA.members.map((m, i) => `<button data-pos="${i}" class="${i === state.eu ? "on" : ""}">${m.name}${i === state.eu ? " (você)" : ""}</button>`).join("")}
      </div>
    </div>`);
  document.body.appendChild(sheet);
  document.querySelector(".sheet-back").addEventListener("click", fecharFolha);
  sheet.querySelector("#quem").addEventListener("click", (e) => {
    const b = e.target.closest("[data-pos]");
    if (!b) return;
    state.eu = Number(b.dataset.pos);
    fecharFolha();
    toast(`Agora você é ${DATA.members[state.eu].name} 🐠`);
    render();
  });
});

render();
