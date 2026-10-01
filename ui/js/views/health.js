// Diagnóstico do ambiente (KVM, bibliotecas, espaço, GPU...).
import { h, icon, mount, copyText } from '../util.js';
import { get } from '../api.js';
import { toast, errToast } from '../modal.js';
import { t } from '../i18n.js';

const LEVEL_ICON = { ok: 'check', warn: 'warning', error: 'error', info: 'info' };

export function mountHealth(root) {
  const list = h('div', { class: 'checklist' });
  const btn = h('button', { class: 'btn primary', onclick: load }, icon('refresh', 18), t('health.recheck'));
  mount(root, h('div', { class: 'page' },
    h('div', { class: 'page-head' }, h('h1', null, t('health.title')), h('span', { class: 'grow' }), btn),
    h('p', { class: 'muted', style: { marginBottom: '16px' } }, t('health.intro')), list));

  async function load() {
    mount(list, [1, 2, 3].map(() => h('div', { class: 'skeleton', style: { height: '64px' } })));
    try {
      const r = await get('/api/diag');
      mount(list, r.checks.map(item));
    } catch (e) { errToast(e); }
  }
  load();
  return () => {};
}

function item(c) {
  const title = t(`diag.${c.id}.${c.level}`, c.args || {});
  const detail = t(`diag.${c.id}.${c.level}.d`, c.args || {});
  return h('div', { class: 'check-item ' + c.level },
    icon(LEVEL_ICON[c.level] || 'info', 22),
    h('div', { class: 'grow' },
      h('b', null, title),
      detail && !detail.startsWith('diag.') ? h('div', { class: 'muted', style: { marginTop: '2px' } }, detail) : null,
      c.fix ? h('div', { class: 'cmd' }, h('code', null, c.fix), h('button', { class: 'btn icon ghost sm', title: t('copy'), onclick: async () => { if (await copyText(c.fix)) toast(t('copied'), 'ok'); } }, icon('copy', 16))) : null));
}
