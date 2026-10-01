// Ponto de entrada: barra superior, roteamento e eventos em tempo real.
import { h, icon, mount } from './util.js';
import { state, on, emit, loadInfo, loadAvds, loadPackages } from './store.js';
import { connectEvents } from './api.js';
import { initLang, t, currentLang } from './i18n.js';
import { post } from './api.js';
import { toast } from './modal.js';
import { initTasks } from './tasks.js';
import { mountDevices } from './views/devices.js';
import { mountSdk } from './views/sdk.js';
import { mountHealth } from './views/health.js';
import { settingsDialog, applyTheme } from './views/settings.js';

const ROUTES = {
  devices: { icon: 'devices', label: 'nav.devices', mount: mountDevices },
  sdk: { icon: 'sdk', label: 'nav.sdk', mount: mountSdk },
  health: { icon: 'health', label: 'nav.health', mount: mountHealth },
};

let unmount = null;
let current = '';

function route() {
  const name = (location.hash.replace(/^#\/?/, '') || 'devices').split('/')[0];
  return ROUTES[name] ? name : 'devices';
}

function renderTopbar() {
  const r = route();
  const nav = h('nav', { class: 'nav' }, Object.entries(ROUTES).map(([k, v]) =>
    h('a', { class: k === r ? 'active' : '', href: '#/' + k }, icon(v.icon, 18), t(v.label))));
  mount(document.getElementById('topbar'),
    h('div', { class: 'brand' }, h('img', { src: 'logo.svg', alt: '' }), 'AVD Menu'),
    nav,
    h('div', { class: 'right' },
      state.connected ? null : h('span', { class: 'pill err' }, icon('error', 14), t('conn.lost')),
      h('button', { class: 'btn ghost icon', title: t('nav.settings'), onclick: settingsDialog }, icon('settings', 20))));
}

function show() {
  const name = route();
  renderTopbar();
  if (unmount) { try { unmount(); } catch (e) { console.error(e); } unmount = null; }
  current = name;
  const root = document.getElementById('view');
  root.scrollTop = 0;
  unmount = ROUTES[name].mount(root);
}

async function boot() {
  try {
    await loadInfo();
  } catch (e) {
    document.getElementById('view').textContent = 'AVD Menu: ' + e.message;
    return;
  }
  initLang(state.info.config.lang);
  post('/api/lang', { lang: currentLang() }).catch(() => {});
  applyTheme(state.info.config.theme);
  initTasks();
  on('lang', show);
  window.addEventListener('hashchange', show);
  show();
  loadAvds().catch(() => {});

  connectEvents({
    running(map) {
      state.running = map || {};
      for (const a of state.avds) a.running = state.running[a.name] || undefined;
      for (const n of [...state.stopping]) if (!state.running[n]) state.stopping.delete(n);
      emit('avds');
    },
    task(tk) {
      const prev = state.tasks.get(tk.id);
      state.tasks.set(tk.id, tk);
      emit('tasks');
      const was = prev && (prev.state === 'running' || prev.state === 'queued');
      const now = tk.state !== 'running' && tk.state !== 'queued';
      if ((!prev || was) && now) onTaskFinished(tk);
    },
    exited(x) {
      // O emulador abriu e fechou sozinho: mostra o fim do log (disco cheio, biblioteca faltando...).
      toast(t('avd.exited', { name: x.name }), 'err', (x.log || '').trim().split('\n').slice(-8).join('\n'));
      loadAvds().catch(() => {});
    },
    refresh() {
      loadInfo().catch(() => {});
      loadAvds().catch(() => {});
      loadPackages(false).catch(() => {});
    },
    _status(ok) {
      if (state.connected !== ok) { state.connected = ok; renderTopbar(); }
    },
  });
}

function onTaskFinished(tk) {
  if (tk.state === 'done') toast(t(tk.kind === 'uninstall' ? 'task.finished.uninstall' : 'task.finished.install', { title: tk.title }), 'ok');
  else if (tk.state === 'error') toast(t('task.finished.failed', { title: tk.title }), 'err', tk.error);
  loadInfo().catch(() => {});
  loadAvds().catch(() => {});
  if (state.packages) loadPackages(false).catch(() => {});
}

boot();
