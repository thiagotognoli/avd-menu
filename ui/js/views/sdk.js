// SDK Manager: plataformas, imagens de sistema e ferramentas.
import { h, icon, mount, fmtBytes } from '../util.js';
import { post } from '../api.js';
import { state, on, loadPackages, loadInfo, emit } from '../store.js';
import { confirmDialog, errToast } from '../modal.js';
import { t } from '../i18n.js';
import { installPackages, uninstallPackages } from '../sdkops.js';
import { sdkLocationDialog } from './settings.js';

const TOOL_ORDER = ['emulator', 'platform-tools', 'cmdline-tools', 'build-tools', 'ndk', 'cmake', 'extras', 'sources', 'other'];

export function mountSdk(root) {
  let tab = 'platforms';
  let showAll = false;
  let loading = !state.packages;
  const pending = new Map();
  const expanded = new Set(['emulator', 'platform-tools', 'cmdline-tools']);
  const apiOpen = new Set();

  const content = h('div');
  const bar = h('div');
  const head = h('div');
  mount(root, h('div', { class: 'page' }, head, content, bar));

  function pkgs() { return (state.packages && state.packages.packages) || []; }

  function renderHead() {
    const s = state.info.sdk;
    const refresh = h('button', { class: 'btn ghost icon', title: t('refresh'), onclick: async () => {
      loading = true; render();
      try { await loadPackages(true); } catch (e) { errToast(e); }
      loading = false; render();
    } }, icon('refresh', 20));
    const prev = h('input', { type: 'checkbox', checked: state.info.config.showPreview || null, onchange: async (e) => {
      await post('/api/settings', { showPreview: e.target.checked });
      await loadInfo(); loading = true; render();
      try { await loadPackages(true); } catch (err) { errToast(err); }
      loading = false; render();
    } });
    const all = h('input', { type: 'checkbox', checked: showAll || null, onchange: (e) => { showAll = e.target.checked; render(); } });
    mount(head,
      h('div', { class: 'page-head' }, h('h1', null, t('sdk.title')), h('span', { class: 'grow' }), refresh),
      h('div', { class: 'card pad', style: { marginBottom: '16px', padding: '14px 18px' } },
        h('div', { class: 'row wrap' },
          icon('folder', 20),
          h('div', { class: 'grow' }, h('div', { class: 'small muted' }, t('sdk.location')), h('code', null, s.root)),
          h('button', { class: 'btn outline sm', onclick: () => sdkLocationDialog() }, t('sdk.change')))),
      state.packages && state.packages.error ? h('div', { class: 'banner warn' }, icon('warning', 20), h('div', null, t('sdk.offline.none'), h('div', { class: 'small muted' }, state.packages.error))) : null,
      state.packages && state.packages.offline && !state.packages.error ? h('div', { class: 'banner warn' }, icon('warning', 20), h('div', null, t('sdk.offline.cache'))) : null,
      h('div', { class: 'row wrap', style: { marginBottom: '6px' } },
        h('div', { class: 'tabs grow', style: { marginBottom: 0, borderBottom: 0 } }, [['platforms', t('sdk.tab.platforms')], ['tools', t('sdk.tab.tools')]].map(([k, l]) =>
          h('button', { class: tab === k ? 'on' : '', onclick: () => { tab = k; render(); } }, l))),
        h('label', { class: 'check' }, all, h('span', null, t('sdk.showall'))),
        h('label', { class: 'check' }, prev, h('span', null, t('sdk.preview')))));
  }

  function checkbox(p) {
    const pend = pending.get(p.path);
    const on_ = pend === 'install' ? true : pend === 'uninstall' ? false : p.installed;
    const cb = h('input', { type: 'checkbox', checked: on_ || null, disabled: (!p.installed && !p.available) || null, onchange: () => {
      const want = cb.checked;
      if (want === p.installed) pending.delete(p.path); else pending.set(p.path, want ? 'install' : 'uninstall');
      render();
    } });
    return cb;
  }

  function statusCell(p) {
    const pend = pending.get(p.path);
    if (pend === 'install') return h('span', { class: 'pill info' }, p.installed ? t('sdk.will.update') : t('sdk.will.install'));
    if (pend === 'uninstall') return h('span', { class: 'pill err' }, t('sdk.will.remove'));
    if (p.installed && p.update) return h('span', { class: 'row', style: { gap: '8px' } },
      h('span', { class: 'pill warn' }, t('sdk.update.avail')),
      h('button', { class: 'btn outline sm', onclick: () => { pending.set(p.path, 'install'); render(); } }, t('sdk.update')));
    if (p.installed) return h('span', { class: 'pill ok' }, icon('check', 14), t('sdk.installed'));
    if (!p.available) return h('span', { class: 'muted small' }, t('wiz.unavailable'));
    return h('span', { class: 'muted small' }, t('sdk.notinstalled'));
  }

  function pkgRow(p, indent = 0) {
    const label = p.type === 'sysimg' ? `${p.tagDisplay || p.tag} · ${p.abi}` : p.name;
    return h('tr', { class: p.compat === 'no' ? 'dim' : '' },
      h('td', { style: { width: '42px', textAlign: 'center' } }, checkbox(p)),
      h('td', { style: { paddingInlineStart: (12 + indent * 22) + 'px' } },
        h('span', { class: 'tag-badge' }, p.type === 'sysimg' && p.playStore ? icon('store', 16) : null, h('b', { style: { fontWeight: indent ? 550 : 650 } }, label)),
        h('div', { class: 'small muted' }, p.path)),
      h('td', null, p.installed ? p.installedRevision : p.revision, p.channel && p.channel !== 'stable' ? h('span', { class: 'pill warn', style: { marginInlineStart: '6px' } }, p.channel) : null,
        p.compat === 'slow' ? h('span', { class: 'pill warn', style: { marginInlineStart: '6px' }, title: t('wiz.slow.tip') }, t('wiz.slow')) : null),
      h('td', null, statusCell(p)),
      h('td', { class: 'num muted' }, p.size ? fmtBytes(p.size) : ''));
  }

  function groupRow(key, label, sub, items, openSet) {
    const open = openSet.has(key);
    const inst = items.filter((i) => i.installed).length;
    return h('tr', { class: 'group', onclick: () => { open ? openSet.delete(key) : openSet.add(key); render(); } },
      h('td', { style: { textAlign: 'center' } }, icon(open ? 'expand' : 'chevron', 20)),
      h('td', { colspan: 2 }, label, sub ? h('span', { class: 'muted small', style: { marginInlineStart: '10px', fontWeight: 500 } }, sub) : null),
      h('td', null, inst ? h('span', { class: 'pill ok' }, t('sdk.n.installed', { n: inst })) : ''),
      h('td'));
  }

  function renderPlatforms() {
    const byApi = new Map();
    for (const p of pkgs()) {
      if (!['platform', 'sysimg', 'sources'].includes(p.type)) continue;
      if (p.obsolete && !showAll) continue;
      if (p.type === 'sysimg' && p.compat === 'no' && !showAll && !p.installed) continue;
      let api = p.api;
      if (p.type === 'sources') api = p.path.replace('sources;android-', '');
      if (!api) continue;
      if (!byApi.has(api)) byApi.set(api, []);
      byApi.get(api).push(p);
    }
    const keys = [...byApi.keys()].sort((a, b) => parseFloat(b) - parseFloat(a));
    const rows = [];
    for (const api of keys) {
      const items = byApi.get(api).sort((a, b) => ({ platform: 0, sysimg: 1, sources: 2 }[a.type] - { platform: 0, sysimg: 1, sources: 2 }[b.type]) || a.path.localeCompare(b.path));
      const name = (items.find((i) => i.apiName) || {}).apiName;
      rows.push(groupRow(api, name ? `${name}` : `API ${api}`, `API ${api}`, items, apiOpen));
      if (apiOpen.has(api)) items.forEach((p) => rows.push(pkgRow(p, 1)));
    }
    return rows;
  }

  function renderTools() {
    const groups = new Map();
    for (const p of pkgs()) {
      if (['platform', 'sysimg'].includes(p.type)) continue;
      if (p.type === 'sources') continue;
      if (p.obsolete && !showAll && !p.installed) continue;
      if (!groups.has(p.type)) groups.set(p.type, []);
      groups.get(p.type).push(p);
    }
    const rows = [];
    const order = TOOL_ORDER.filter((k) => groups.has(k)).concat([...groups.keys()].filter((k) => !TOOL_ORDER.includes(k)));
    for (const k of order) {
      const items = groups.get(k).sort((a, b) => b.path.localeCompare(a.path, undefined, { numeric: true }));
      if (items.length === 1 && ['emulator', 'platform-tools'].includes(k)) { rows.push(pkgRow(items[0], 0)); continue; }
      rows.push(groupRow(k, t('sdk.group.' + k), t('sdk.n.versions', { n: items.length }), items, expanded));
      if (expanded.has(k)) items.forEach((p) => rows.push(pkgRow(p, 1)));
    }
    return rows;
  }

  function renderBar() {
    const inst = [...pending].filter(([, v]) => v === 'install').map(([k]) => k);
    const rem = [...pending].filter(([, v]) => v === 'uninstall').map(([k]) => k);
    if (!pending.size) { mount(bar); return; }
    const size = inst.reduce((s, k) => s + ((pkgs().find((p) => p.path === k) || {}).size || 0), 0);
    mount(bar, h('div', { class: 'bottom-bar' },
      icon('info', 20),
      h('div', { class: 'grow' }, [inst.length ? t('sdk.bar.install', { n: inst.length, size: fmtBytes(size) }) : '', rem.length ? t('sdk.bar.remove', { n: rem.length }) : ''].filter(Boolean).join(' · ')),
      h('button', { class: 'btn ghost', onclick: () => { pending.clear(); render(); } }, t('discard')),
      h('button', { class: 'btn primary', onclick: apply }, icon('check', 18), t('apply'))));
  }

  async function apply() {
    const inst = [...pending].filter(([, v]) => v === 'install').map(([k]) => k);
    const rem = [...pending].filter(([, v]) => v === 'uninstall').map(([k]) => k);
    if (rem.length) {
      const ok = await confirmDialog({ title: t('sdk.remove.title'), message: t('sdk.remove.msg', { n: rem.length }), confirm: t('remove'), danger: true,
        details: h('ul', { class: 'small', style: { margin: '10px 0 0', paddingInlineStart: '18px' } }, rem.map((p) => h('li', null, p))) });
      if (!ok) return;
    }
    if (rem.length) await uninstallPackages(rem);
    if (inst.length) {
      const task = await installPackages(inst);
      if (!task && !rem.length) return; // cancelado no diálogo
    }
    pending.clear(); render();
  }

  function render() {
    renderHead();
    if (loading) {
      mount(content, [1, 2, 3, 4, 5, 6].map(() => h('div', { class: 'skeleton', style: { height: '44px', marginBottom: '8px' } })));
      renderBar(); return;
    }
    const rows = tab === 'platforms' ? renderPlatforms() : renderTools();
    mount(content, h('div', { class: 'table-wrap' }, h('table', { class: 'table' },
      h('thead', null, h('tr', null, h('th'), h('th', null, t('sdk.col.name')), h('th', null, t('sdk.col.rev')), h('th', null, t('sdk.col.status')), h('th', { class: 'num' }, t('wiz.col.dl')))),
      h('tbody', null, rows.length ? rows : h('tr', null, h('td', { colspan: 5, class: 'muted', style: { padding: '28px', textAlign: 'center' } }, t('sdk.empty')))))));
    renderBar();
  }

  const offs = [on('packages', () => { loading = false; render(); }), on('info', render)];
  render();
  if (!state.packages) loadPackages(false).then(() => { loading = false; render(); }).catch((e) => { loading = false; errToast(e); render(); });
  else if (!loading) render();
  // abre o primeiro grupo de plataformas por padrão
  setTimeout(() => { const first = pkgs().find((p) => p.type === 'platform' || p.type === 'sysimg'); if (first && !apiOpen.size) { const installed = pkgs().find((p) => p.installed && (p.type === 'sysimg' || p.type === 'platform')); apiOpen.add((installed || first).api); render(); } }, 0);
  return () => offs.forEach((f) => f());
}
