// Aurelia Launch UI. Event-driven only: no timers, no polling, no animation loops.
// The shell hides the window on blur, so nothing here runs while it is hidden.
'use strict';

const $ = (id) => document.getElementById(id);
const q = $('q');
const list = $('results');
const home = $('home');
const none = $('none');
const statusEl = $('status');
const statusText = $('statusText');
const actionHint = $('actionHint');
const actionLabel = $('actionLabel');

const MAX_ROWS = 20; // virtualization cap: never more than 20 option nodes
const tauri = window.__TAURI__?.core?.invoke ? window.__TAURI__.core : null;
if (!tauri) document.body.classList.add('web');

const KIND = {
  App: { group: 'Applications', meta: 'Application', action: 'Open' },
  File: { group: 'Files', meta: 'File', action: 'Open' },
  Calc: { group: 'Calculator', meta: 'Calculator', action: 'Copy' },
  WebSearch: { group: 'Web', meta: 'Web search', action: 'Search' },
  SystemCommand: { group: 'System', meta: 'Command', action: 'Run' },
};

let items = [];
let rows = [];
let sel = -1;
let seq = 0;

// ---------- backend ----------

function call(cmd, args) {
  return tauri ? tauri.invoke(cmd, args) : stub(cmd, args);
}

// Browser fallback so the UI can be previewed without the shell.
function stub(cmd, args) {
  if (cmd === 'search') return Promise.resolve(stubSearch(args.input.trim()));
  if (cmd === 'copy_text') return navigator.clipboard?.writeText(args.text) ?? Promise.resolve();
  if (cmd === 'open_url') { window.open(args.url, '_blank', 'noopener'); return Promise.resolve(); }
  return Promise.resolve();
}

function stubSearch(input) {
  if (!input) return [];
  const n = input.toLowerCase();
  const out = [];
  const apps = [['Terminal', '/System/Applications/Utilities'], ['Safari', '/Applications'], ['Finder', '/System/Library/CoreServices'], ['AureliaHub', 'sibling checkout']];
  for (const [t, s] of apps) if (t.toLowerCase().includes(n)) out.push({ title: t, subtitle: s, kind: 'App' });
  const v = stubCalc(input);
  if (v !== null) out.unshift({ title: `= ${v}`, subtitle: 'Calculator — Enter to copy', kind: 'Calc' });
  const m = /^(g|gh|yt) (.+)$/.exec(input);
  const base = { g: 'https://www.google.com/search?q=', gh: 'https://github.com/search?q=', yt: 'https://www.youtube.com/results?search_query=' };
  if (m) out.push({ title: `Search ${m[1]}: ${m[2].trim()}`, subtitle: base[m[1]] + encodeURIComponent(m[2].trim()), kind: 'WebSearch' });
  const cmds = [['lock', 'Lock the screen'], ['sleep', 'Put the Mac to sleep'], ['restart', 'Restart the Mac'], ['shutdown', 'Shut down the Mac'], ['emptytrash', 'Empty the Trash']];
  for (const [t, s] of cmds) if (t.includes(n)) out.push({ title: t, subtitle: s, kind: 'SystemCommand' });
  return out;
}

// Tiny recursive-descent calculator (no eval: CSP is script-src 'self').
function stubCalc(src) {
  if (!/[\d)]\s*[-+*/%^]|^\s*\(/.test(src)) return null;
  let i = 0;
  const peek = () => { while (src[i] === ' ') i++; return src[i]; };
  const num = () => {
    const c = peek();
    if (c === '(') { i++; const v = add(); if (peek() !== ')') throw 0; i++; return v; }
    if (c === '-') { i++; return -num(); }
    const m = /^\d*\.?\d+/.exec(src.slice(i));
    if (!m) throw 0;
    i += m[0].length;
    return parseFloat(m[0]);
  };
  const pow = () => { const b = num(); if (peek() === '^') { i++; return b ** pow(); } return b; };
  const mul = () => { let v = pow(); for (let c; (c = peek()) === '*' || c === '/' || c === '%';) { i++; const r = pow(); v = c === '*' ? v * r : c === '/' ? v / r : v % r; } return v; };
  const add = () => { let v = mul(); for (let c; (c = peek()) === '+' || c === '-';) { i++; const r = mul(); v = c === '+' ? v + r : v - r; } return v; };
  try { const v = add(); return peek() === undefined && Number.isFinite(v) ? +v.toPrecision(12) : null; } catch { return null; }
}

// ---------- rendering ----------

// Group by kind, keeping the core's ranking for group order and within groups.
function grouped(res) {
  const order = [];
  const byKind = new Map();
  for (const it of res) {
    if (!byKind.has(it.kind)) { byKind.set(it.kind, []); order.push(it.kind); }
    byKind.get(it.kind).push(it);
  }
  return order.flatMap((k) => byKind.get(k)).slice(0, MAX_ROWS);
}

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

function icon(kind) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', `#i-${KIND[kind] ? kind : 'File'}`);
  svg.setAttribute('aria-hidden', 'true');
  svg.appendChild(use);
  return svg;
}

// Title with the first case-insensitive match of the query highlighted.
function titleNode(text, needle) {
  const t = el('span', 'title');
  const at = needle ? text.toLowerCase().indexOf(needle.toLowerCase()) : -1;
  if (at < 0) { t.textContent = text; return t; }
  t.append(text.slice(0, at), el('mark', null, text.slice(at, at + needle.length)), text.slice(at + needle.length));
  return t;
}

function display(it, query) {
  if (it.kind === 'Calc') return { title: it.title.replace(/^=\s*/, ''), sub: `= ${query}`, mark: '' };
  if (it.kind === 'WebSearch') {
    let host = it.subtitle;
    try { host = new URL(it.subtitle).hostname.replace(/^www\./, ''); } catch { /* keep raw */ }
    return { title: it.title.replace(/^Search \w+:\s*/, ''), sub: host, mark: '' };
  }
  return { title: it.title, sub: it.subtitle, mark: query };
}

function render() {
  const query = q.value.trim();
  const frag = document.createDocumentFragment();
  rows = [];
  let lastKind = null;
  items.forEach((it, i) => {
    const k = KIND[it.kind] || KIND.File;
    if (it.kind !== lastKind) {
      lastKind = it.kind;
      const h = el('li', 'group', k.group);
      h.setAttribute('role', 'presentation');
      frag.appendChild(h);
    }
    const d = display(it, query);
    const li = el('li', `row k-${it.kind}`);
    li.id = `o-${i}`;
    li.dataset.i = i;
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', 'false');
    const tile = el('span', 'tile');
    tile.appendChild(icon(it.kind));
    const text = el('span', 'text');
    text.append(titleNode(d.title, d.mark), el('span', 'sub', d.sub));
    const act = el('span', 'act', k.action);
    act.appendChild(el('kbd', null, '↵'));
    li.append(tile, text, el('span', 'meta', k.meta), act);
    rows.push(li);
    frag.appendChild(li);
  });
  list.replaceChildren(frag);
  list.scrollTop = 0;

  const hasQuery = query.length > 0;
  list.hidden = items.length === 0;
  home.hidden = hasQuery;
  none.hidden = !hasQuery || items.length > 0;
  if (!none.hidden) $('noneQuery').textContent = query;
  q.setAttribute('aria-expanded', String(items.length > 0));
  sel = -1;
  select(items.length ? 0 : -1);
}

// Selection changes touch only two nodes; the list is never rebuilt for it.
function select(i) {
  if (sel >= 0 && rows[sel]) rows[sel].setAttribute('aria-selected', 'false');
  sel = i;
  const row = rows[sel];
  if (row) {
    row.setAttribute('aria-selected', 'true');
    row.scrollIntoView({ block: 'nearest' });
    q.setAttribute('aria-activedescendant', row.id);
  } else {
    q.removeAttribute('aria-activedescendant');
  }
  const query = q.value.trim();
  const k = row ? KIND[items[sel].kind] : null;
  actionHint.hidden = !k && !query;
  actionLabel.textContent = k ? k.action : 'Search web';
  actionHint.nextElementSibling.hidden = actionHint.hidden;
}

function status(msg, warn = false) {
  statusText.textContent = msg || 'Aurelia Launch';
  statusEl.classList.toggle('warn', !!warn);
}

// ---------- actions ----------

async function hide() {
  if (tauri) await call('hide_window').catch(() => {});
}

async function activate(it) {
  try {
    if (it.kind === 'Calc') {
      const value = it.title.replace(/^=\s*/, '');
      await call('copy_text', { text: value });
      status(`Copied ${value}`);
      await hide();
    } else if (it.kind === 'WebSearch') {
      await call('open_url', { url: it.subtitle });
      await hide();
    } else {
      status('Launching lands with the macOS search backend (M2)', true);
    }
  } catch (err) {
    status(`Failed: ${err}`, true);
  }
}

function searchWeb(query) {
  activate({ kind: 'WebSearch', title: '', subtitle: `https://www.google.com/search?q=${encodeURIComponent(query)}` });
}

// ---------- events ----------

q.addEventListener('input', async () => {
  const my = ++seq;
  const input = q.value;
  status('');
  if (!input.trim()) { items = []; render(); return; }
  let res;
  try {
    res = await call('search', { input });
  } catch (err) {
    if (my === seq) status(`Search failed: ${err}`, true);
    return;
  }
  if (my !== seq) return; // a newer keystroke owns the list: drop stale results
  items = grouped(Array.isArray(res) ? res : []);
  render();
});

q.addEventListener('keydown', (e) => {
  const down = e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n');
  const up = e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p');
  if (down || up) {
    e.preventDefault();
    if (items.length) select(Math.max(0, Math.min(items.length - 1, sel + (down ? 1 : -1))));
  } else if (e.key === 'Enter') {
    e.preventDefault();
    if (e.isComposing) return;
    if (items[sel]) activate(items[sel]);
    else if (q.value.trim()) searchWeb(q.value.trim());
  } else if (e.key === 'Escape') {
    e.preventDefault();
    if (q.value) { q.value = ''; q.dispatchEvent(new Event('input')); }
    else hide();
  }
});

list.addEventListener('pointermove', (e) => {
  const row = e.target.closest('.row');
  if (row && +row.dataset.i !== sel) select(+row.dataset.i);
});
list.addEventListener('mousedown', (e) => e.preventDefault()); // keep focus in the input
list.addEventListener('click', (e) => {
  const row = e.target.closest('.row');
  if (row) activate(items[+row.dataset.i]);
});

home.addEventListener('click', (e) => {
  const b = e.target.closest('button[data-fill]');
  if (!b) return;
  q.value = b.dataset.fill;
  q.focus();
  q.setSelectionRange(q.value.length, q.value.length);
  q.dispatchEvent(new Event('input'));
});

// Focus the input whenever the window is shown again.
function onShow() { q.focus(); q.select(); }
window.addEventListener('focus', onShow);
document.addEventListener('visibilitychange', () => { if (!document.hidden) onShow(); });

render();
onShow();
