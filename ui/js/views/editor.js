// Editor de um AVD existente: configurações, config.ini bruto e opções de inicialização.
import { h, icon, mount } from '../util.js';
import { get, put } from '../api.js';
import { loadAvds } from '../store.js';
import { openModal, toast, errToast } from '../modal.js';
import { t } from '../i18n.js';
import { avdForm } from './avdform.js';

export async function openEditor(a) {
  let data;
  try { data = await get(`/api/avds/${encodeURIComponent(a.name)}`); } catch (e) { errToast(e); return; }
  if (!data.settings) { toast(t('edit.noconfig'), 'err'); return; }
  const name = encodeURIComponent(a.name);
  let tab = 'settings';
  const form = avdForm(data.settings, { advancedOpen: true });
  const raw = h('textarea', { rows: 22, spellcheck: 'false', style: { width: '100%' } });
  raw.value = data.raw || '';
  const args = h('input', { type: 'text', value: data.avd.extraArgs || '', placeholder: '-gpu host -no-audio' });

  const body = h('div');
  const tabsEl = h('div', { class: 'tabs' });
  const pane = h('div');
  body.append(tabsEl, pane);
  const info = h('div', { class: 'small muted', style: { marginBottom: '12px' } },
    h('span', null, `${a.deviceName} · API ${a.api} · ${a.abi} · `), h('code', null, a.dir));

  function show() {
    mount(tabsEl, [['settings', t('edit.tab.settings')], ['raw', 'config.ini'], ['launch', t('edit.tab.launch')]].map(([k, l]) =>
      h('button', { class: tab === k ? 'on' : '', onclick: () => { tab = k; show(); } }, l)));
    if (tab === 'settings') mount(pane, info, form.el);
    if (tab === 'raw') mount(pane, h('div', { class: 'banner warn' }, icon('warning', 20), h('div', null, t('edit.raw.warn'))), raw);
    if (tab === 'launch') mount(pane, h('div', { class: 'field' }, h('label', null, t('edit.args')), args, h('div', { class: 'hint' }, t('edit.args.hint'))),
      h('div', { class: 'small muted', style: { marginTop: '14px' } }, t('edit.args.ex')));
  }
  show();

  const m = openModal({
    title: t('edit.title', { name: a.displayName }), width: 760, tall: true, body,
    footer: [h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')),
      h('button', { class: 'btn primary', onclick: async () => {
        try {
          let skinError = '';
          if (tab === 'raw') await put(`/api/avds/${name}/raw`, { content: raw.value });
          else {
            skinError = (await put(`/api/avds/${name}`, form.value())).skinError;
            await put(`/api/avds/${name}/args`, { args: args.value });
          }
          toast(t('edit.saved'), 'ok');
          if (skinError) toast(skinError, 'err');
          m.close(); await loadAvds();
        } catch (e) { errToast(e); }
      } }, icon('check', 18), t('save'))],
  });
}
