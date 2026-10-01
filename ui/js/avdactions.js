// Ações sobre AVDs compartilhadas pelas telas.
import { get, post, put, del } from './api.js';
import { state, emit, loadAvds } from './store.js';
import { toast, errToast, confirmDialog, promptDialog, openModal } from './modal.js';
import { h, icon } from './util.js';
import { t } from './i18n.js';
import { installPackages } from './sdkops.js';

function touch() { emit('avds'); }

export async function startAvd(a, opt = {}) {
  if (state.starting.has(a.name)) return;
  state.starting.add(a.name); touch();
  try {
    await post(`/api/avds/${encodeURIComponent(a.name)}/start`, opt);
    toast(t('avd.started', { name: a.displayName }), 'ok');
  } catch (e) {
    errToast(e, t('avd.start.failed', { name: a.displayName }));
  } finally {
    state.starting.delete(a.name); touch();
    loadAvds().catch(() => {});
  }
}

export async function stopAvd(a) {
  state.stopping.add(a.name); touch();
  try {
    await post(`/api/avds/${encodeURIComponent(a.name)}/stop`);
  } catch (e) {
    errToast(e, t('avd.stop.failed'));
    state.stopping.delete(a.name); touch();
    return;
  }
  // a fila de eventos "running" limpa o estado; garante limpeza mesmo sem ela
  setTimeout(() => { state.stopping.delete(a.name); touch(); }, 25000);
}

export async function deleteAvd(a) {
  const ok = await confirmDialog({
    title: t('avd.delete.title'), message: t('avd.delete.msg', { name: a.displayName }), confirm: t('delete'), danger: true,
  });
  if (!ok) return;
  try { await del(`/api/avds/${encodeURIComponent(a.name)}`); toast(t('avd.deleted'), 'ok'); await loadAvds(); }
  catch (e) { errToast(e); }
}

export async function wipeAvd(a) {
  const ok = await confirmDialog({
    title: t('avd.wipe.title'), message: t('avd.wipe.msg', { name: a.displayName }), confirm: t('avd.wipe'), danger: true,
  });
  if (!ok) return;
  try { await post(`/api/avds/${encodeURIComponent(a.name)}/wipe`); toast(t('avd.wiped'), 'ok'); await loadAvds(); }
  catch (e) { errToast(e); }
}

export async function duplicateAvd(a) {
  const base = a.name.replace(/_copy\d*$/, '');
  let n = 2, cand = base + '_copy';
  const names = new Set(state.avds.map((x) => x.name));
  while (names.has(cand)) cand = base + '_copy' + n++;
  const name = await promptDialog({
    title: t('avd.duplicate.title'), label: t('avd.duplicate.label'), value: cand, confirm: t('avd.duplicate'),
    hint: t('avd.duplicate.hint'),
    validate: (v) => (!/^[A-Za-z0-9._-]+$/.test(v) ? t('avd.name.invalid') : names.has(v) ? t('avd.name.exists') : null),
  });
  if (!name) return;
  try {
    await post(`/api/avds/${encodeURIComponent(a.name)}/duplicate`, { name, displayName: name.replace(/_/g, ' ') });
    toast(t('avd.duplicated'), 'ok'); await loadAvds();
  } catch (e) { errToast(e); }
}

export async function showOnDisk(a) {
  try { await post(`/api/avds/${encodeURIComponent(a.name)}/show`); } catch (e) { errToast(e); }
}

export async function viewLog(a) {
  let text = '';
  const pre = h('pre', { style: { margin: 0, maxHeight: '460px', overflow: 'auto', background: 'var(--surface2)', padding: '12px', borderRadius: '10px', whiteSpace: 'pre-wrap', fontSize: '12px' } });
  const load = async () => {
    try { const r = await get(`/api/avds/${encodeURIComponent(a.name)}/log`); text = r.text || ''; } catch (_) {}
    pre.textContent = text || t('log.empty');
    pre.scrollTop = pre.scrollHeight;
  };
  const m = openModal({
    title: t('log.title', { name: a.displayName }), width: 820,
    body: pre,
    footer: [h('button', { class: 'btn ghost', onclick: load }, icon('refresh', 18), t('refresh')), h('button', { class: 'btn primary', onclick: () => m.close() }, t('close'))],
  });
  load();
}

export async function downloadImage(a) {
  if (!a.imagePkg) return;
  await installPackages([a.imagePkg]);
}

export { put };
