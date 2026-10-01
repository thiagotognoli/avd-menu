// Assistente "Create Virtual Device": hardware -> imagem de sistema -> configuração.
import { h, icon, mount, fmtBytes, ltr, CATEGORY_ICON } from '../util.js';
import { state, on, loadDevices, loadPackages, loadAvds, taskFor, emit } from '../store.js';
import { get, post, ApiError } from '../api.js';
import { openModal, toast, errToast } from '../modal.js';
import { t } from '../i18n.js';
import { installPackages } from '../sdkops.js';
import { avdForm } from './avdform.js';
import { shortcutFields } from './shortcut.js';
import { startAvd } from '../avdactions.js';

const CATS = ['phone', 'tablet', 'wear', 'tv', 'automotive', 'desktop', 'xr'];

export async function openWizard() {
  const W = {
    step: 0, device: null, image: null, cat: 'phone', search: '', showOld: false, tab: 'recommended',
    pkgs: null, pkgErr: null, loadingPkgs: false, preview: null, form: null, sc: null, createSc: false, startAfter: false,
    avdId: '', idTouched: false, displayTouched: false,
  };
  const offs = [];
  let modal;
  const stepNames = [t('wiz.step.hw'), t('wiz.step.image'), t('wiz.step.config')];

  try { if (!state.devices.length) await loadDevices(); } catch (e) { errToast(e); return; }
  W.device = state.devices.find((d) => d.id === 'pixel_9') || state.devices.find((d) => d.category === 'phone' && !d.deprecated) || state.devices[0];
  if (W.device) W.cat = W.device.category;

  const back = h('button', { class: 'btn ghost', onclick: () => go(W.step - 1) }, t('back'));
  const next = h('button', { class: 'btn primary', onclick: () => go(W.step + 1) }, t('next'), icon('chevron', 18));
  const finish = h('button', { class: 'btn primary', onclick: doCreate }, icon('check', 18), t('finish'));
  const cancel = h('button', { class: 'btn ghost', onclick: () => modal.close() }, t('cancel'));
  const startAfterCb = h('input', { type: 'checkbox', onchange: (e) => { W.startAfter = e.target.checked; } });
  const startAfterLbl = h('label', { class: 'check left hidden' }, startAfterCb, h('span', null, t('wiz.startafter')));

  modal = openModal({
    title: t('wiz.title'), width: 1020, tall: true, onClose: () => offs.forEach((f) => f()),
    body: h('div', { id: 'wiz-body', style: { height: '100%', display: 'flex', flexDirection: 'column' } }),
    footer: [startAfterLbl, cancel, back, next, finish],
  });
  modal.body.style.overflow = 'hidden';
  modal.body.style.display = 'flex';
  modal.body.style.flexDirection = 'column';
  const bodyEl = modal.body.firstChild;

  offs.push(on('tasks', () => { if (W.step === 1) renderImages(); }));
  offs.push(on('packages', () => { if (W.step === 1) { W.pkgs = state.packages; renderImages(); } }));

  function validate() {
    if (W.step === 0) return !!W.device;
    if (W.step === 1) return !!(W.image && W.image.installed);
    if (W.step === 2) return !!W.avdId && /^[A-Za-z0-9._-]+$/.test(W.avdId) && !state.avds.some((a) => a.name === W.avdId);
    return true;
  }
  function refreshButtons() {
    back.classList.toggle('hidden', W.step === 0);
    next.classList.toggle('hidden', W.step === 2);
    finish.classList.toggle('hidden', W.step !== 2);
    startAfterLbl.classList.toggle('hidden', W.step !== 2);
    next.disabled = !validate();
    finish.disabled = !validate();
    modal.setSteps(stepNames, W.step);
  }

  async function go(n) {
    if (n < 0 || n > 2) return;
    if (n > W.step && !validate()) return;
    W.step = n;
    if (n === 0) renderHardware();
    if (n === 1) await enterImages();
    if (n === 2) await enterConfig();
    refreshButtons();
  }

  // ---------- etapa 1: hardware ----------
  function renderHardware() {
    const counts = {};
    state.devices.forEach((d) => { if (W.showOld || !d.deprecated) counts[d.category] = (counts[d.category] || 0) + 1; });
    const visible = () => state.devices.filter((d) => d.category === W.cat && (W.showOld || !d.deprecated || d.id === W.device?.id)
      && (!W.search || (d.name + ' ' + d.id + ' ' + d.manufacturer).toLowerCase().includes(W.search)));
    const side = h('div', { class: 'sidebar-list' }, CATS.filter((c) => counts[c] || c === W.cat).map((c) =>
      h('button', { class: c === W.cat ? 'on' : '', onclick: () => { W.cat = c; renderHardware(); } }, icon(CATEGORY_ICON[c], 20), t('cat.' + c), h('span', { class: 'grow' }), h('span', { class: 'muted small' }, counts[c] || 0))));
    const tbody = h('tbody');
    const fill = () => {
      mount(tbody, visible().map((d) => h('tr', { class: 'pick' + (W.device && W.device.id === d.id ? ' sel' : ''), onclick: () => { W.device = d; W.image = null; W.pkgs = null; fill(); refreshButtons(); }, ondblclick: () => go(1) },
        h('td', null, h('b', null, d.name), d.user ? h('span', { class: 'pill info', style: { marginInlineStart: '8px' } }, t('wiz.custom')) : null, d.deprecated ? h('span', { class: 'pill', style: { marginInlineStart: '8px' } }, t('wiz.old')) : null),
        h('td', { style: { textAlign: 'center' } }, d.playstore ? h('span', { title: 'Google Play', style: { color: 'var(--primary)' } }, icon('store', 18)) : ''),
        h('td', { class: 'num' }, d.diag ? d.diag.toFixed(1) + '″' : '—'),
        h('td', { class: 'num' }, ltr(`${d.w}×${d.h}`)),
        h('td', { class: 'num' }, ltr(`${d.density} dpi`)))));
      if (!tbody.children.length) tbody.append(h('tr', null, h('td', { colspan: 5, class: 'muted', style: { padding: '24px', textAlign: 'center' } }, t('wiz.nodevices'))));
    };
    fill();
    const search = h('input', { type: 'search', placeholder: t('wiz.search'), value: W.search, oninput: (e) => { W.search = e.target.value.toLowerCase(); fill(); } });
    const old = h('input', { type: 'checkbox', checked: W.showOld || null, onchange: (e) => { W.showOld = e.target.checked; renderHardware(); } });
    mount(bodyEl,
      h('p', { class: 'muted', style: { marginBottom: '12px' } }, t('wiz.hw.intro')),
      h('div', { class: 'split', style: { flex: 1, minHeight: 0 } }, side,
        h('div', { class: 'main' },
          h('div', { class: 'row wrap' },
            h('div', { class: 'search grow' }, icon('search', 18), search),
            h('label', { class: 'check' }, old, h('span', null, t('wiz.showold'))),
            h('button', { class: 'btn outline sm', onclick: newProfile }, icon('add', 16), t('wiz.newprofile'))),
          h('div', { class: 'table-wrap', style: { flex: 1, minHeight: 0 } }, h('table', { class: 'table' },
            h('thead', null, h('tr', null, h('th', null, t('wiz.col.name')), h('th', { style: { width: '70px', textAlign: 'center' } }, 'Play'), h('th', { class: 'num', style: { width: '80px' } }, t('wiz.col.size')), h('th', { class: 'num', style: { width: '120px' } }, t('wiz.col.res')), h('th', { class: 'num', style: { width: '100px' } }, t('wiz.col.density')))),
            tbody)))));
    refreshButtons();
  }

  function newProfile() {
    const f = { name: '', category: W.cat, w: 1080, h: 2400, density: 420, ramMB: 2048, playstore: false };
    const inp = (k, type = 'text', extra = {}) => h('input', { type, value: f[k], ...extra, oninput: (e) => { f[k] = type === 'number' ? parseInt(e.target.value, 10) || 0 : e.target.value; } });
    const catSel = h('select', { onchange: (e) => { f.category = e.target.value; } }, CATS.map((c) => h('option', { value: c, selected: c === f.category || null }, t('cat.' + c))));
    const play = h('input', { type: 'checkbox', onchange: (e) => { f.playstore = e.target.checked; } });
    const m2 = openModal({
      title: t('profile.title'), width: 520,
      body: h('div', { style: { display: 'flex', flexDirection: 'column', gap: '14px' } },
        h('div', { class: 'field' }, h('label', null, t('profile.name')), inp('name')),
        h('div', { class: 'cols' },
          h('div', { class: 'field' }, h('label', null, t('profile.category')), catSel),
          h('div', { class: 'field' }, h('label', null, t('form.ram') + ' (MB)'), inp('ramMB', 'number', { min: 256, step: 128 }))),
        h('div', { class: 'cols three' },
          h('div', { class: 'field' }, h('label', null, t('profile.width')), inp('w', 'number', { min: 100 })),
          h('div', { class: 'field' }, h('label', null, t('profile.height')), inp('h', 'number', { min: 100 })),
          h('div', { class: 'field' }, h('label', null, t('profile.density')), inp('density', 'number', { min: 100 }))),
        h('label', { class: 'check' }, play, h('span', null, t('profile.play')))),
      footer: [h('button', { class: 'btn ghost', onclick: () => m2.close() }, t('cancel')),
        h('button', { class: 'btn primary', onclick: async () => {
          try {
            const r = await post('/api/devices', { name: f.name, category: f.category, w: f.w, h: f.h, density: f.density, ramMB: f.ramMB, playstore: f.playstore });
            await loadDevices();
            W.device = state.devices.find((d) => d.id === r.device.id); W.cat = W.device.category;
            m2.close(); renderHardware();
          } catch (e) { errToast(e); }
        } }, t('save'))],
    });
  }

  // ---------- etapa 2: imagem de sistema ----------
  async function enterImages() {
    W.loadingPkgs = true; W.pkgErr = null;
    renderImages();
    try {
      W.pkgs = await loadPackages(false, W.device.id);
    } catch (e) { W.pkgErr = e.message; }
    W.loadingPkgs = false;
    renderImages();
  }

  function imageLists() {
    const all = ((W.pkgs && W.pkgs.packages) || []).filter((p) => p.type === 'sysimg' && p.allowed !== false);
    const okList = all.filter((p) => p.compat === 'ok');
    const isPreview = (p) => !/^\d/.test(p.api || '');
    const isVariant = (p) => (p.tagDisplay || '').includes('·') || /-ext\d+;/.test(p.path) || /_atd$/.test(p.tag);
    const rank = { google_apis_playstore: 0, google_apis: 1, default: 2 };
    const apis = [...new Set(okList.filter((p) => !isPreview(p) && !isVariant(p)).map((p) => p.api))].sort((a, b) => parseFloat(b) - parseFloat(a)).slice(0, 5);
    const rec = okList.filter((p) => (apis.includes(p.api) && !isVariant(p)) || (p.installed && !isVariant(p)))
      .sort((a, b) => parseFloat(b.api) - parseFloat(a.api) || (rank[a.tag] ?? 9) - (rank[b.tag] ?? 9));
    const compat = okList.slice().sort((a, b) => parseFloat(b.api) - parseFloat(a.api) || a.tag.localeCompare(b.tag));
    const other = all.filter((p) => p.compat !== 'ok').sort((a, b) => parseFloat(b.api) - parseFloat(a.api) || a.tag.localeCompare(b.tag));
    return { recommended: rec, all: compat, other };
  }

  function renderImages() {
    if (W.step !== 1) return;
    if (W.loadingPkgs) {
      mount(bodyEl, h('p', { class: 'muted', style: { marginBottom: '12px' } }, t('wiz.image.loading')),
        ...[1, 2, 3, 4, 5].map(() => h('div', { class: 'skeleton', style: { height: '44px', marginBottom: '8px' } })));
      return;
    }
    const lists = imageLists();
    const cur = lists[W.tab] || [];
    const tabs = h('div', { class: 'tabs' }, [['recommended', t('wiz.tab.rec')], ['all', t('wiz.tab.all')], ['other', t('wiz.tab.other')]].map(([k, label]) =>
      h('button', { class: W.tab === k ? 'on' : '', onclick: () => { W.tab = k; renderImages(); } }, label, h('span', { class: 'muted small', style: { marginInlineStart: '6px' } }, lists[k].length))));
    const rows = cur.map((p) => {
      const task = taskFor(p.path);
      const sel = W.image && W.image.path === p.path;
      let action;
      if (p.installed) action = h('span', { class: 'pill ok' }, icon('check', 14), t('wiz.installed'));
      else if (task) action = h('div', { style: { minWidth: '150px' } }, h('div', { class: 'bar' + (task.progress > 0 ? '' : ' indet') }, h('i', { style: { width: Math.round((task.progress || 0) * 100) + '%' } })), h('div', { class: 'small muted' }, task.state === 'queued' ? t('task.queued') : Math.round((task.progress || 0) * 100) + '% · ' + t('phase.' + (task.phase || 'download'))));
      else if (!p.available) action = h('span', { class: 'muted small' }, t('wiz.unavailable'));
      else action = h('button', { class: 'btn outline sm', onclick: (e) => { e.stopPropagation(); download(p); } }, icon('download', 16), t('wiz.download'));
      return h('tr', { class: 'pick' + (sel ? ' sel' : '') + (p.installed ? '' : ''), onclick: () => { W.image = p; renderImages(); refreshButtons(); } },
        h('td', null, h('b', null, p.apiName || 'API ' + p.api), h('div', { class: 'small muted' }, 'API ' + p.api + (p.ext ? ' · ext ' + p.ext : ''))),
        h('td', null, h('span', { class: 'tag-badge' }, p.playStore ? icon('store', 16) : null, p.tagDisplay || p.tag)),
        h('td', null, p.abi, p.compat === 'slow' ? h('span', { class: 'pill warn', style: { marginInlineStart: '6px' }, title: t('wiz.slow.tip') }, t('wiz.slow')) : p.compat === 'no' ? h('span', { class: 'pill err', style: { marginInlineStart: '6px' } }, t('wiz.incompat')) : null),
        h('td', { class: 'num' }, fmtBytes(p.size)),
        h('td', { style: { width: '190px' } }, action));
    });
    const offline = W.pkgs && (W.pkgs.offline || W.pkgs.error);
    mount(bodyEl,
      h('div', { class: 'row', style: { marginBottom: '6px' } }, h('p', { class: 'muted grow' }, t('wiz.image.intro', { device: W.device.name })),
        h('button', { class: 'btn ghost sm', onclick: async () => { W.loadingPkgs = true; renderImages(); try { W.pkgs = await loadPackages(true, W.device.id); } catch (e) { errToast(e); } W.loadingPkgs = false; renderImages(); } }, icon('refresh', 16), t('refresh'))),
      offline ? h('div', { class: 'banner warn' }, icon('warning', 20), h('div', null, W.pkgs.error ? t('wiz.offline.none') : t('wiz.offline.cache'))) : null,
      tabs,
      h('div', { class: 'table-wrap', style: { flex: 1, minHeight: 0 } }, h('table', { class: 'table' },
        h('thead', null, h('tr', null, h('th', null, t('wiz.col.release')), h('th', null, t('wiz.col.target')), h('th', null, 'ABI'), h('th', { class: 'num' }, t('wiz.col.dl')), h('th', null, ''))),
        h('tbody', null, rows.length ? rows : h('tr', null, h('td', { colspan: 5, class: 'muted', style: { padding: '28px', textAlign: 'center' } }, t('wiz.noimages')))))));
    refreshButtons();
  }

  async function download(p) {
    W.image = p; // seleciona para seguir direto após o download
    await installPackages([p.path]);
    renderImages();
  }

  // ---------- etapa 3: configuração ----------
  async function enterConfig() {
    mount(bodyEl, h('div', { class: 'skeleton', style: { height: '320px' } }));
    try {
      W.preview = await post('/api/avds/preview', { deviceId: W.device.id, imagePkg: W.image.path });
    } catch (e) { errToast(e); W.step = 1; await enterImages(); refreshButtons(); return; }
    const display = `${W.device.name} API ${W.image.api}`;
    const base = display.replace(/[^A-Za-z0-9._-]+/g, '_').replace(/^_+|_+$/g, '');
    let id = base, n = 2;
    const used = new Set(state.avds.map((a) => a.name));
    while (used.has(id)) id = `${base}_${n++}`;
    if (!W.idTouched) W.avdId = id;
    const v = { ...W.preview.settings, displayName: W.displayTouched ? W.displayName : display };
    W.displayName = v.displayName;

    const idInput = h('input', { type: 'text', value: W.avdId, oninput: (e) => { W.avdId = e.target.value.trim(); W.idTouched = true; check(); } });
    const idErr = h('div', { class: 'small', style: { color: 'var(--danger)', minHeight: '16px' } });
    const check = () => {
      idErr.textContent = !W.avdId ? t('avd.name.required') : !/^[A-Za-z0-9._-]+$/.test(W.avdId) ? t('avd.name.invalid') : used.has(W.avdId) ? t('avd.name.exists') : '';
      refreshButtons();
    };
    W.form = avdForm(v, {
      advancedExtra: [h('div', { class: 'field' }, h('label', null, t('form.id')), idInput, h('div', { class: 'hint' }, t('form.id.hint')), idErr)],
      onName: (val) => {
        W.displayTouched = true; W.displayName = val;
        if (!W.idTouched) { const b = val.replace(/[^A-Za-z0-9._-]+/g, '_').replace(/^_+|_+$/g, ''); let i = b, k = 2; while (used.has(i)) i = `${b}_${k++}`; W.avdId = i; idInput.value = i; check(); }
      },
    });
    W.sc = shortcutFields({ avdName: W.avdId, displayName: 'Android Emulator ' + (W.displayName || W.avdId) });
    const scBox = h('div', { class: 'hidden', style: { marginTop: '14px' } }, W.sc.el);
    const scCb = h('input', { type: 'checkbox', checked: W.createSc || null, onchange: (e) => { W.createSc = e.target.checked; scBox.classList.toggle('hidden', !W.createSc); } });
    scBox.classList.toggle('hidden', !W.createSc);

    const summary = h('div', { class: 'sum' },
      h('div', null, h('span', { class: 'muted small' }, t('wiz.sum.device')), h('b', null, W.device.name), h('span', { class: 'small muted' }, `${W.device.w}×${W.device.h} · ${W.device.density} dpi`)),
      h('div', null, h('span', { class: 'muted small' }, t('wiz.sum.image')), h('b', null, `${W.image.apiName || 'API ' + W.image.api}`), h('span', { class: 'small muted' }, `${W.image.tagDisplay || W.image.tag} · ${W.image.abi}`)),
      h('div', { class: 'grow' }),
      h('div', { style: { alignSelf: 'center' } }, h('button', { class: 'btn outline sm', onclick: () => go(0) }, t('wiz.change.device')), ' ', h('button', { class: 'btn outline sm', onclick: () => go(1) }, t('wiz.change.image'))));

    mount(bodyEl, h('div', { style: { overflow: 'auto', flex: 1, paddingInlineEnd: '4px' } }, summary,
      h('div', { style: { marginTop: '18px' } }, W.form.el),
      state.info && state.info.shortcuts ? h('div', { class: 'card pad', style: { marginTop: '18px', boxShadow: 'none' } },
        h('label', { class: 'check' }, scCb, h('span', null, h('b', null, t('wiz.shortcut')), h('div', { class: 'muted small' }, t('wiz.shortcut.sub')))), scBox) : null));
    check();
    setTimeout(() => W.form.focusName && W.form.focusName(), 40);
  }

  async function doCreate() {
    finish.disabled = true;
    const body = { name: W.avdId, deviceId: W.device.id, imagePkg: W.image.path, ...W.form.value() };
    if (W.createSc) body.shortcut = W.sc.value();
    try {
      const r = await post('/api/avds', body);
      toast(t('wiz.created', { name: r.avd.displayName }), 'ok');
      if (r.shortcutError) toast(r.shortcutError, 'err');
      modal.close();
      await loadAvds();
      if (W.startAfter) startAvd(r.avd);
    } catch (e) { errToast(e, t('wiz.create.failed')); finish.disabled = false; }
  }

  renderHardware();
  refreshButtons();
}
