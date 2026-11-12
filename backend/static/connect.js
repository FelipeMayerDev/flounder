'use strict';
const element = id => document.getElementById(id);
let configured = false;
let widget = null;
let nextCursor = null;
let inspectedCard = null;
let sdk = null;

function status(message) { element('status').textContent = message; }
function show(name, value) {
  let section = document.getElementById(`result-${name}`);
  if (!section) {
    section = document.createElement('section');
    section.id = `result-${name}`;
    section.className = 'panel';
    const heading = document.createElement('h2');
    heading.textContent = name;
    section.append(heading, document.createElement('pre'));
    element('results').append(section);
  }
  section.querySelector('pre').textContent = JSON.stringify(value, null, 2);
}
async function api(path, options = {}) {
  const token = element('token').value.trim();
  if (!token) throw new Error('Preencha o token do cliente com APP_TOKEN_1 ou APP_TOKEN_2.');
  const response = await fetch(path, {
    ...options, headers: { 'Content-Type': 'application/json', ...options.headers, Authorization: `Bearer ${token}` },
    cache: 'no-store', signal: AbortSignal.timeout(60000),
  });
  const body = await response.json().catch(() => ({}));
  if (element('token').value.trim() !== token) throw new Error('Cliente alterado. Refaça a consulta com o token atual.');
  if (!response.ok) throw new Error(body.error || `A consulta falhou (HTTP ${response.status}).`);
  return body;
}
function widgetSdk() {
  if (globalThis.PluggyConnect) return Promise.resolve();
  if (!sdk) sdk = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = 'https://cdn.pluggy.ai/pluggy-connect/v2.8.2/pluggy-connect.js';
    script.referrerPolicy = 'no-referrer';
    script.onload = resolve;
    script.onerror = () => { sdk = null; reject(new Error('Não foi possível carregar o Pluggy Connect. Confira a conexão com a internet.')); };
    document.head.append(script);
  });
  return sdk;
}
async function accounts() {
  const id = element('item').value.trim();
  if (!id) throw new Error('Informe o itemId da conexão.');
  const data = await api(`/api/items/${encodeURIComponent(id)}/accounts`);
  const cards = (Array.isArray(data.results) ? data.results : []).filter(card => card.type === 'CREDIT');
  element('card').replaceChildren();
  for (const card of cards) element('card').add(new Option(`${card.name || 'Cartão'} · ${card.number || card.id}`, card.id));
  if (!cards.length) element('card').add(new Option('Nenhum cartão disponível nesta conexão', ''));
  element('card').disabled = !cards.length;
  element('inspect').disabled = !cards.length;
  nextCursor = null;
  inspectedCard = null;
  element('next').disabled = true;
  element('results').replaceChildren();
  show('Cartões', cards);
  status(cards.length ? `${cards.length} cartão(ões) disponível(is). No modo local, os cards do dashboard foram atualizados. Selecione um para consultar.` : 'Nenhum cartão disponível. A sincronização da instituição pode ainda estar em andamento.');
}
function cursor(data) {
  nextCursor = typeof data.next === 'string' ? new URLSearchParams(data.next).get('after') : null;
  element('next').disabled = !nextCursor;
}
element('connect').addEventListener('click', async () => {
  element('connect').disabled = true;
  try {
    status('Preparando a conexão segura…');
    const token = await api('/api/connect-token', { method: 'POST', body: '{}' });
    await widgetSdk();
    const sandbox = element('sandbox').checked;
    widget = new globalThis.PluggyConnect({
      connectToken: token.accessToken, includeSandbox: sandbox,
      ...(!sandbox ? { connectorIds: [200], selectedConnectorId: 200 } : {}),
      onSuccess: async data => {
        const id = data?.item?.id ?? data?.id;
        if (typeof id !== 'string') { status('A conexão terminou sem um itemId. Tente novamente.'); return; }
        element('item').value = id;
        try { await accounts(); } catch (error) { status(error.message); }
      },
      onError: error => {
        const message = typeof error?.message === 'string' ? error.message : '';
        status(/sandbox|contas de teste|test accounts/i.test(message)
          ? 'Para uso pessoal, conecte os bancos no Meu Pluggy e autorize pelo conector MeuPluggy. Confira se ele está habilitado em Customização → Conectores no Dashboard.'
          : 'A conexão com a instituição não foi concluída. Confira a mensagem no Pluggy Connect.');
      },
      onClose: () => { widget = null; element('connect').disabled = !configured; },
    });
    widget.init();
  } catch (error) {
    widget = null;
    status(error.message);
    element('connect').disabled = !configured;
  }
});
element('sandbox').addEventListener('change', () => { element('connect').textContent = element('sandbox').checked ? 'Conectar em sandbox' : 'Autorizar Meu Pluggy'; });
element('accounts').addEventListener('click', async () => {
  element('accounts').disabled = true;
  try { await accounts(); } catch (error) { status(error.message); }
  finally { element('accounts').disabled = !configured; }
});
element('inspect').addEventListener('click', async () => {
  const id = element('card').value;
  if (!id) return;
  inspectedCard = id;
  nextCursor = null;
  element('inspect').disabled = true;
  element('next').disabled = true;
  element('results').replaceChildren();
  status('Consultando os dados do cartão…');
  const routes = [['Conta e limites', ''], ['Transações', '/transactions'], ['Faturas', '/bills']];
  const results = await Promise.allSettled(routes.map(([, suffix]) => api(`/api/accounts/${encodeURIComponent(id)}${suffix}`)));
  for (let i = 0; i < results.length; i++) {
    const result = results[i];
    show(routes[i][0], result.status === 'fulfilled' ? result.value : { error: result.reason.message });
    if (i === 1 && result.status === 'fulfilled') cursor(result.value);
  }
  element('inspect').disabled = false;
  status(results.some(result => result.status === 'rejected') ? 'Consulta concluída com campos indisponíveis. Veja o retorno de cada seção.' : 'Dados recebidos. Valores e metadados foram preservados como retornados pela Pluggy.');
});
element('card').addEventListener('change', () => { nextCursor = null; element('next').disabled = true; });
element('token').addEventListener('input', () => {
  element('card').replaceChildren(new Option('Carregue os cartões deste cliente', ''));
  element('card').disabled = true;
  element('inspect').disabled = true;
  element('next').disabled = true;
  element('results').replaceChildren();
  nextCursor = null;
});
element('next').addEventListener('click', async () => {
  if (!nextCursor || !inspectedCard) return;
  element('next').disabled = true;
  try {
    const data = await api(`/api/accounts/${encodeURIComponent(inspectedCard)}/transactions?${new URLSearchParams({ after: nextCursor })}`);
    show('Transações', data);
    cursor(data);
    status('Próxima página de transações recebida.');
  } catch (error) { status(error.message); element('next').disabled = !nextCursor; }
});
// O app abre esta página com #token=… para ninguém precisar colar o token; tira-o do endereço.
const tokenFromApp = new URLSearchParams(location.hash.slice(1)).get('token');
if (tokenFromApp) {
  element('token').value = tokenFromApp;
  history.replaceState(null, '', location.pathname + location.search);
}
fetch('/health', { cache: 'no-store' }).then(response => response.json()).then(health => {
  configured = health.pluggyConfigured === true;
  element('connect').disabled = !configured;
  element('accounts').disabled = !configured;
  status(configured ? (element('token').value ? 'Backend local ativo. Use Autorizar Meu Pluggy para conectar um banco ou cartão.' : 'Backend local ativo. Informe o token do cliente para começar.') : 'Backend local ativo. Preencha CLIENT_ID e CLIENT_SECRET em backend/.env e reinicie o serviço para conectar à Pluggy.');
}).catch(() => status('Não foi possível acessar o backend local.'));
