// Configurações: local do SDK, idioma, tema, atalho do AVD Menu.
import { h, icon, mount } from '../util.js';
import { get, post } from '../api.js';
import { state, loadInfo, loadAvds, loadPackages, emit } from '../store.js';
import { openModal, toast, errToast } from '../modal.js';
import { t, setLang, currentLang, LANGUAGES } from '../i18n.js';

/** Seleção do local do Android SDK, com navegador de pastas simples. */
export function sdkLocationDialog() {
  const s = state.info.sdk;
  let path = s.root;
  const input = h('input', { type: 'text', value: path, oninput: () => { path = input.value; browse(path, true); } });
  const tree = h('div', { class: 'tree' });
  const flag = h('div', { class: 'small muted', style: { minHeight: '20px' } });
  let lastBrowse = '';

  async function browse(p, soft) {
    if (!p) return;
    try {
      const r = await get('/api/fs?path=' + encodeURIComponent(p));
      if (!soft) { input.value = r.path; path = r.path; }
      lastBrowse = r.path;
      flag.textContent = r.isSdk ? '✓ ' + t('sdkloc.detected') : '';
      mount(tree,
        h('button', { onclick: () => browse(r.parent) }, icon('folder', 18), '..'),
        (r.dirs || []).map((d) => h('button', { onclick: () => browse(r.path.replace(/\/$/, '') + '/' + d) }, icon('folder', 18), d)));
    } catch (_) { /* caminho ainda incompleto */ }
  }

  const cands = (state.info.candidates || []).filter((c) => c.path !== s.root);
  const m = openModal({
    title: t('sdkloc.title'), width: 620,
    body: h('div', { style: { display: 'flex', flexDirection: 'column', gap: '14px' } },
      h('p', { class: 'muted' }, t('sdkloc.intro')),
      h('div', { class: 'field' }, h('label', null, t('sdkloc.path')), input, flag),
      cands.length ? h('div', null, h('div', { class: 'label', style: { marginBottom: '6px' } }, t('sdkloc.found')),
        h('div', { class: 'tree', style: { maxHeight: '140px' } }, cands.map((c) => h('button', { onclick: () => { input.value = c.path; path = c.path; browse(c.path); } }, icon('sdk', 18), h('span', { class: 'grow' }, c.path), c.emulator ? h('span', { class: 'pill ok' }, 'emulator') : null)))) : null,
      h('div', null, h('div', { class: 'label', style: { marginBottom: '6px' } }, t('sdkloc.browse')), tree),
      h('button', { class: 'btn outline sm', style: { alignSelf: 'flex-start' }, onclick: () => { input.value = state.info.sdk.defaultRoot; path = input.value; browse(path); } }, t('sdkloc.default', { path: state.info.sdk.defaultRoot }))),
    footer: [h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')),
      h('button', { class: 'btn primary', onclick: async () => {
        try {
          await post('/api/sdk/setup', { root: input.value.trim(), create: true });
          state.packages = null;
          await loadInfo(); await loadAvds(); emit('packages');
          toast(t('sdkloc.saved'), 'ok'); m.close();
        } catch (e) { errToast(e); }
      } }, t('sdkloc.use'))],
  });
  browse(s.root);
}

export function settingsDialog() {
  const info = state.info;
  const langSel = h('select', { onchange: (e) => setLang(e.target.value) },
    [['', t('settings.auto')], ...LANGUAGES.map((l) => [l.code, l.name])].map(([v, l]) => h('option', { value: v, selected: v === (info.config.lang || '') || null }, l)));
  const themeSel = h('select', { onchange: async (e) => { await post('/api/settings', { theme: e.target.value }); applyTheme(e.target.value); info.config.theme = e.target.value; } },
    [['', t('settings.auto')], ['light', t('settings.light')], ['dark', t('settings.dark')]].map(([v, l]) => h('option', { value: v, selected: v === (info.config.theme || '') || null }, l)));
  const self = info.self && info.self.exists;
  const selfBtn = h('button', { class: 'btn outline sm', onclick: async () => {
    try {
      if (self) await post('/api/self/uninstall'); else await post('/api/self/install');
      await loadInfo(); m.close(); toast(self ? t('settings.self.removed') : t('settings.self.added'), 'ok');
    } catch (e) { errToast(e); }
  } }, self ? t('settings.self.remove') : t('settings.self.add'));
  const m = openModal({
    title: t('settings.title'), width: 580,
    body: h('div', { style: { display: 'flex', flexDirection: 'column', gap: '18px' } },
      h('div', { class: 'field' }, h('label', null, t('settings.sdk')), h('div', { class: 'row' }, h('code', { class: 'grow' }, info.sdk.root), h('button', { class: 'btn outline sm', onclick: () => { m.close(); sdkLocationDialog(); } }, t('sdk.change')))),
      h('div', { class: 'field' }, h('label', null, t('settings.avdhome')), h('code', null, info.avdHome)),
      h('div', { class: 'cols' }, h('div', { class: 'field' }, h('label', null, t('settings.lang')), langSel), h('div', { class: 'field' }, h('label', null, t('settings.theme')), themeSel)),
      info.canInstallSelf ? h('div', { class: 'field' }, h('label', null, t('settings.self')), h('div', { class: 'row' }, h('div', { class: 'grow muted small' }, self ? t('settings.self.on') : t('settings.self.off')), selfBtn),
        info.appImage ? h('div', { class: 'hint' }, t('settings.self.appimage')) : null) : null,
      h('div', { class: 'small muted' }, `AVD Menu ${info.version} · ${info.os}/${info.arch}`),
      h('div', { class: 'row', style: { justifyContent: 'flex-end' } }, h('button', { class: 'btn danger sm', onclick: async () => { await post('/api/shutdown'); document.body.innerHTML = '<p style="padding:40px;font:16px system-ui">AVD Menu encerrado. Pode fechar esta janela.</p>'; setTimeout(() => window.close(), 400); } }, t('settings.quit')))),
    footer: [h('button', { class: 'btn primary', onclick: () => m.close() }, t('close'))],
  });
}

export function applyTheme(theme) {
  if (theme) document.documentElement.setAttribute('data-theme', theme);
  else document.documentElement.removeAttribute('data-theme');
}
