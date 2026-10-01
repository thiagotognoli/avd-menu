// Tela principal: lista de dispositivos virtuais (Device Manager).
import { h, icon, mount, fmtBytes, ltr, CATEGORY_ICON } from '../util.js';
import { state, on, loadAvds, loadInfo } from '../store.js';
import { popMenu, errToast } from '../modal.js';
import { t } from '../i18n.js';
import * as act from '../avdactions.js';
import { openWizard } from './wizard.js';
import { openEditor } from './editor.js';
import { shortcutDialog, removeShortcut } from './shortcut.js';
import { setupNeeded, setupCard } from './setup.js';

export function mountDevices(root) {
  let filter = '';
  const search = h('input', { type: 'search', placeholder: t('devices.search'), oninput: (e) => { filter = e.target.value.toLowerCase(); renderList(); } });
  const listBox = h('div', { class: 'avd-list' });
  const top = h('div');
  const refreshBtn = h('button', { class: 'btn ghost icon', title: t('refresh'), onclick: async () => {
    refreshBtn.firstChild.classList.add('spin');
    try { await Promise.all([loadAvds(), loadInfo()]); } catch (e) { errToast(e); }
    refreshBtn.firstChild.classList.remove('spin');
  } }, icon('refresh', 20));
  const createBtn = h('button', { class: 'btn primary', onclick: () => openWizard() }, icon('add', 20), t('devices.create'));
  const head = h('div', { class: 'page-head' },
    h('h1', null, t('devices.title')),
    h('span', { class: 'grow' }),
    h('div', { class: 'search', style: { width: '240px' } }, icon('search', 18), search),
    refreshBtn, createBtn);
  mount(root, h('div', { class: 'page' }, head, top, listBox));

  function renderTop() {
    mount(top, setupNeeded() ? setupCard() : null);
  }

  function renderList() {
    const all = state.avds;
    if (!state.avdsLoaded) {
      mount(listBox, [1, 2, 3].map(() => h('div', { class: 'skeleton', style: { height: '74px' } })));
      return;
    }
    const items = all.filter((a) => !filter || (a.displayName + ' ' + a.name + ' ' + a.deviceName + ' ' + a.api).toLowerCase().includes(filter));
    if (!all.length) {
      mount(listBox, h('div', { class: 'card empty' },
        h('div', { class: 'big' }, icon('phone', 48)),
        h('h2', null, t('devices.empty.title')),
        h('p', null, t('devices.empty.text')),
        h('button', { class: 'btn primary', onclick: () => openWizard() }, icon('add', 20), t('devices.create'))));
      return;
    }
    if (!items.length) { mount(listBox, h('p', { class: 'muted', style: { padding: '20px' } }, t('devices.nomatch'))); return; }
    const sorted = items.slice().sort((a, b) => (state.running[b.name] ? 1 : 0) - (state.running[a.name] ? 1 : 0));
    mount(listBox, sorted.map(row));
  }

  const offs = [on('avds', () => { renderTop(); renderList(); }), on('info', renderTop), on('lang', () => {}), on('running', renderList)];
  renderTop();
  renderList();
  return () => offs.forEach((f) => f());
}

function row(a) {
  const inst = state.running[a.name];
  const starting = state.starting.has(a.name);
  const stopping = state.stopping.has(a.name) && inst;
  const broken = !!a.problem && !a.imageInstalled;
  let status;
  if (stopping) status = h('span', { class: 'pill warn' }, icon('refresh', 14, 'spin'), t('status.stopping'));
  else if (starting && !inst) status = h('span', { class: 'pill info' }, icon('refresh', 14, 'spin'), t('status.starting'));
  else if (inst) status = h('span', { class: 'pill ok', title: inst.serial || '' }, h('span', { class: 'dot pulse' }), t('status.running'), inst.serial ? h('span', { style: { opacity: .75, fontWeight: 500 } }, inst.serial.replace('emulator-', ':')) : null);
  else if (broken) status = h('span', { class: 'pill warn', title: a.problem }, icon('warning', 14), t('status.noimage'));
  else if (a.problem) status = h('span', { class: 'pill err', title: a.problem }, icon('error', 14), t('status.problem'));
  else status = h('span', { class: 'pill' }, t('status.stopped'));

  let primary;
  if (inst) primary = h('button', { class: 'btn stop', disabled: stopping || null, onclick: () => act.stopAvd(a) }, icon('stop', 18), t('stop'));
  else if (broken) primary = h('button', { class: 'btn primary', onclick: () => act.downloadImage(a) }, icon('download', 18), t('avd.download.image'));
  else primary = h('button', { class: 'btn primary', disabled: starting || a.problem ? true : null, onclick: () => act.startAvd(a) }, icon(starting ? 'refresh' : 'play', 18, starting ? 'spin' : ''), t('start'));

  const more = h('button', { class: 'btn icon ghost', title: t('more'), onclick: (e) => popMenu(e.currentTarget, menuItems(a, !!inst)) }, icon('more', 20));

  const cat = CATEGORY_ICON[a.category] || 'phone';
  return h('div', { class: 'avd' + (inst ? ' running' : '') },
    h('div', { class: 'thumb' }, icon(cat, 26), a.shortcut && a.shortcut.exists ? h('span', { class: 'sc', title: t('shortcut.exists') }, icon('shortcut', 12)) : null),
    h('div', { style: { minWidth: 0 } },
      h('div', { class: 'name', title: a.displayName }, a.displayName),
      h('div', { class: 'sub' }, [a.deviceName, a.width && a.height ? ltr(`${a.width}×${a.height}`) : '', a.density ? ltr(`${a.density} dpi`) : ''].filter(Boolean).join(' · '))),
    h('div', { class: 'c-api' }, h('b', null, a.api ? 'API ' + a.api : '—'), h('div', { class: 'sub' }, a.apiName || '')),
    h('div', { class: 'c-tag' }, h('span', { class: 'tag-badge' }, a.playStore ? icon('store', 16) : null, a.tagDisplay || a.tag || ''), h('div', { class: 'sub' }, a.abi || '')),
    h('div', { class: 'c-size muted' }, fmtBytes(a.sizeBytes)),
    h('div', { class: 'c-status' }, status),
    h('div', { class: 'actions' }, primary, more));
}

function menuItems(a, running) {
  const info = state.info || {};
  const hasSc = a.shortcut && a.shortcut.exists;
  const canStart = !running && !a.problem;
  return [
    { label: t('menu.cold'), icon: 'snow', disabled: !canStart, onClick: () => act.startAvd(a, { mode: 'cold' }) },
    { label: t('menu.wipestart'), icon: 'wipe', disabled: !canStart, onClick: () => act.startAvd(a, { mode: 'wipe' }) },
    info.dgpu ? { label: t('menu.dgpu'), icon: 'gpu', disabled: !canStart, onClick: () => act.startAvd(a, { gpu: 'dedicated' }) } : null,
    { label: t('menu.headless'), icon: 'headless', disabled: !canStart, onClick: () => act.startAvd(a, { headless: true }) },
    '-',
    { label: t('menu.edit'), icon: 'edit', disabled: running, onClick: () => openEditor(a) },
    { label: t('menu.duplicate'), icon: 'copy', disabled: running, onClick: () => act.duplicateAvd(a) },
    { label: t('menu.show'), icon: 'folder', onClick: () => act.showOnDisk(a) },
    { label: t('menu.log'), icon: 'log', onClick: () => act.viewLog(a) },
    info.shortcuts ? '-' : null,
    info.shortcuts ? { label: hasSc ? t('menu.shortcut.edit') : t('menu.shortcut.create'), icon: 'shortcut', onClick: () => shortcutDialog(a) } : null,
    info.shortcuts && hasSc ? { label: t('menu.shortcut.remove'), icon: 'close', onClick: () => removeShortcut(a) } : null,
    '-',
    { label: t('menu.wipe'), icon: 'wipe', disabled: running, onClick: () => act.wipeAvd(a) },
    { label: t('menu.delete'), icon: 'trash', danger: true, disabled: running, onClick: () => act.deleteAvd(a) },
  ];
}
