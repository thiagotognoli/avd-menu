// Campos e diálogo de atalho no menu de aplicativos.
import { h, icon, readIconFile } from '../util.js';
import { post, del } from '../api.js';
import { state, loadAvds } from '../store.js';
import { openModal, toast, errToast } from '../modal.js';
import { t } from '../i18n.js';

/** Campos do atalho (nome, ícone, fixar na dock). value() devolve o corpo da API. */
export function shortcutFields({ avdName, displayName, pin = false }) {
  let iconData = '';
  const name = h('input', { type: 'text', value: displayName || 'Android Emulator ' + avdName });
  const img = h('img', { src: '/logo.svg', alt: '' });
  const file = h('input', { type: 'file', accept: 'image/*', class: 'hidden' });
  const pinCb = h('input', { type: 'checkbox' });
  pinCb.checked = pin;
  file.addEventListener('change', async () => {
    const f = file.files[0]; if (!f) return;
    try { iconData = await readIconFile(f); img.src = iconData; } catch (e) { errToast(e); }
  });
  const reset = h('button', { class: 'btn ghost sm', type: 'button', onclick: () => { iconData = ''; img.src = '/logo.svg'; file.value = ''; } }, t('shortcut.icon.default'));
  const isGnome = state.info && state.info.os === 'linux';
  const el = h('div', { class: 'checklist', style: { gap: '14px' } },
    h('div', { class: 'field' }, h('label', null, t('shortcut.name')), name),
    h('div', { class: 'field' }, h('label', null, t('shortcut.icon')),
      h('div', { class: 'icon-pick' }, img,
        h('button', { class: 'btn outline sm', type: 'button', onclick: () => file.click() }, icon('folder', 16), t('shortcut.icon.choose')), reset, file)),
    isGnome ? h('label', { class: 'check' }, pinCb, h('span', null, t('shortcut.pin'))) : null,
  );
  return {
    el,
    value: () => ({ displayName: name.value.trim(), iconData, pin: pinCb.checked }),
    setIcon(dataUrl) { iconData = dataUrl; img.src = dataUrl || '/logo.svg'; },
  };
}

/** Cria/recria o atalho de um AVD existente. */
export function shortcutDialog(a) {
  const fields = shortcutFields({ avdName: a.name, displayName: a.shortcut && a.shortcut.displayName, pin: false });
  const m = openModal({
    title: a.shortcut && a.shortcut.exists ? t('shortcut.edit') : t('shortcut.create'), width: 520,
    body: h('div', null, h('p', { class: 'muted', style: { marginBottom: '14px' } }, state.info && state.info.os === 'darwin' ? t('shortcut.intro.mac') : t('shortcut.intro'), ' ', h('b', null, a.displayName)), fields.el),
    footer: [
      h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')),
      h('button', { class: 'btn primary', onclick: async () => {
        try {
          const r = await post(`/api/avds/${encodeURIComponent(a.name)}/shortcut`, fields.value());
          toast(t('shortcut.created'), 'ok', r.path);
          m.close(); await loadAvds();
        } catch (e) { errToast(e); }
      } }, icon('shortcut', 18), a.shortcut && a.shortcut.exists ? t('save') : t('shortcut.create.btn')),
    ],
  });
}

export async function removeShortcut(a) {
  try { await del(`/api/avds/${encodeURIComponent(a.name)}/shortcut`); toast(t('shortcut.removed'), 'ok'); await loadAvds(); }
  catch (e) { errToast(e); }
}
