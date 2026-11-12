const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

async function page(respond, configured = true, hash = '') {
  const nodes = new Map();
  class Element {
    constructor() { this.children = []; this.handlers = {}; this.value = ''; this.checked = true; this.disabled = true; this.textContent = ''; }
    set id(value) { this._id = value; nodes.set(value, this); }
    get id() { return this._id; }
    set innerHTML(_) { throw new Error('API data must be rendered as text'); }
    addEventListener(name, handler) { this.handlers[name] = handler; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; this.value = children[0]?.value || ''; }
    add(option) { this.children.push(option); if (!this.value) this.value = option.value; }
    querySelector() { return this.children[1]; }
  }
  for (const id of ['status', 'token', 'sandbox', 'connect', 'item', 'accounts', 'card', 'inspect', 'next', 'results']) {
    const node = new Element(); node.id = id;
  }
  nodes.get('sandbox').checked = false;
  const state = { requests: [], options: null, opened: 0 };
  const document = { getElementById: id => nodes.get(id), createElement: () => new Element(), head: new Element() };
  const fetch = async (url, options = {}) => {
    state.requests.push({ url, options });
    const result = url === '/health' ? { status: 200, body: { status: 'ok', pluggyConfigured: configured } } : await respond(url, options);
    return { ok: result.status >= 200 && result.status < 300, status: result.status, json: async () => result.body };
  };
  class PluggyConnect { constructor(options) { state.options = options; } init() { state.opened++; } }
  class Option extends Element { constructor(text, value) { super(); this.textContent = text; this.value = value; } }
  const location = { hash, pathname: '/connect', search: '' };
  const history = { replaceState: (_state, _title, url) => { state.replaced = url; location.hash = ''; } };
  vm.runInNewContext(fs.readFileSync(`${__dirname}/connect.js`, 'utf8'), { document, fetch, AbortSignal, URLSearchParams, PluggyConnect, Option, location, history });
  await new Promise(resolve => setImmediate(resolve));
  return { nodes, state };
}

test('setup page reports missing credentials without pretending to be connected', async () => {
  const { nodes, state } = await page(() => { throw new Error('No API request expected'); }, false);
  assert.equal(nodes.get('connect').disabled, true);
  assert.match(nodes.get('status').textContent, /CLIENT_ID e CLIENT_SECRET/);
  assert.equal(state.requests.length, 1);
});

test('connect widget receives only the short token and credit accounts render as text', async () => {
  const { nodes, state } = await page(async (url, options) => {
    assert.equal(options.headers.Authorization, 'Bearer test-client-token');
    if (url === '/api/connect-token') return { status: 200, body: { accessToken: 'short-connect-token' } };
    if (url === '/api/items/item-one/accounts') return { status: 200, body: { results: [
      { id: 'bank-one', type: 'BANK', name: 'Conta corrente' },
      { id: 'card-one', type: 'CREDIT', name: '<img onerror=bad>', number: '1234' },
    ] } };
    throw new Error('Unexpected path');
  });
  nodes.get('token').value = 'test-client-token';
  await nodes.get('connect').handlers.click();
  assert.equal(state.opened, 1);
  assert.equal(state.options.connectToken, 'short-connect-token');
  assert.equal(state.options.includeSandbox, false);
  assert.deepEqual(Array.from(state.options.connectorIds), [200]);
  assert.equal(state.options.selectedConnectorId, 200);
  await state.options.onSuccess({ item: { id: 'item-one' } });
  assert.equal(nodes.get('item').value, 'item-one');
  assert.equal(nodes.get('card').children.length, 1);
  assert.match(nodes.get('card').children[0].textContent, /<img onerror=bad>/);
  assert.equal(nodes.get('inspect').disabled, false);
  state.options.onError({ message: 'Contas de teste só podem conectar conectores sandbox' });
  assert.match(nodes.get('status').textContent, /conector MeuPluggy/);
  assert.equal(state.requests.find(r => r.url === '/api/connect-token').options.body, '{}');
});

test('failed authentication does not open a widget and permits retry', async () => {
  const { nodes, state } = await page(async () => ({ status: 401, body: { error: 'Acesso não autorizado.' } }));
  nodes.get('token').value = 'wrong-client-token';
  await nodes.get('connect').handlers.click();
  assert.equal(state.opened, 0);
  assert.equal(nodes.get('connect').disabled, false);
  assert.equal(nodes.get('status').textContent, 'Acesso não autorizado.');
});


test('changing client while loading never renders the previous client data', async () => {
  let complete;
  const { nodes } = await page(async () => new Promise(resolve => { complete = resolve; }));
  nodes.get('token').value = 'first-client-token';
  nodes.get('item').value = 'item-one';
  const loading = nodes.get('accounts').handlers.click();
  nodes.get('token').value = 'second-client-token';
  nodes.get('token').handlers.input();
  complete({ status: 200, body: { results: [{ id: 'card-one', type: 'CREDIT', name: 'Private card' }] } });
  await loading;
  assert.equal(nodes.get('results').children.length, 0);
  assert.match(nodes.get('status').textContent, /Cliente alterado/);
});

test('token opened from the app fills the field and leaves the address bar', async () => {
  const { nodes, state } = await page(() => { throw new Error('No API request expected'); }, true, '#token=abc123');
  assert.equal(nodes.get('token').value, 'abc123');
  assert.equal(state.replaced, '/connect');
  assert.match(nodes.get('status').textContent, /Autorizar Meu Pluggy/);
});
