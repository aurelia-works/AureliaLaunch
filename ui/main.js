// Thin, idle-free UI: event-driven only, no timers/polling.
// The window hides on blur (see shell), so no suspend logic is needed here.
const q = document.getElementById('q');
const list = document.getElementById('results');
let items = [];
let sel = 0;

async function invoke(cmd, args) {
  // Tauri 2 global API when running in the shell; fallback to stub in a browser.
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke(cmd, args);
  return [];
}

let seq = 0;
q.addEventListener('input', async () => {
  const my = ++seq;
  const res = await invoke('search', { input: q.value });
  if (my !== seq) return; // drop stale responses
  items = res; sel = 0; render();
});

q.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowDown') { sel = Math.min(sel + 1, items.length - 1); render(); e.preventDefault(); }
  if (e.key === 'ArrowUp') { sel = Math.max(sel - 1, 0); render(); e.preventDefault(); }
  if (e.key === 'Enter' && items[sel]) { activate(items[sel]); }
  if (e.key === 'Escape') { q.value = ''; items = []; render(); }
});

function render() {
  list.innerHTML = '';
  // Cap DOM nodes: virtualize by rendering top 20 only.
  for (const [i, it] of items.slice(0, 20).entries()) {
    const li = document.createElement('li');
    if (i === sel) li.className = 'sel';
    li.innerHTML = `<span></span><small></small>`;
    li.children[0].textContent = it.title;
    li.children[1].textContent = it.subtitle || '';
    list.appendChild(li);
  }
}

function activate(item) {
  // M1: copy calculator results, open URLs; app launching lands with the
  // macOS backend (NSWorkspace) in the next milestone.
  if (item.kind === 'Calc') navigator.clipboard?.writeText(item.title.replace(/^= /, ''));
  else if (item.kind === 'WebSearch' && item.subtitle.startsWith('http')) {
    window.open(item.subtitle, '_blank');
  }
}
