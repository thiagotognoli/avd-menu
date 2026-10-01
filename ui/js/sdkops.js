// Operações de SDK compartilhadas (instalar com licenças, remover).
import { h, icon, fmtBytes } from './util.js';
import { post, ApiError } from './api.js';
import { openModal, toast, errToast, confirmDialog } from './modal.js';
import { t } from './i18n.js';
import { state, emit } from './store.js';

/** Mostra o plano de instalação + licenças; resolve true se o usuário aceitar. */
function reviewDialog(plan) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const checks = [];
    const licenseBlocks = plan.licenses.map((l) => {
      const cb = h('input', { type: 'checkbox' });
      checks.push(cb);
      cb.addEventListener('change', update);
      return h('details', { style: { marginTop: '10px' } },
        h('summary', { style: { cursor: 'pointer', fontWeight: 600 } }, t('license.title', { id: l.id })),
        h('div', { class: 'license-box', style: { marginTop: '8px' } }, l.text.trim()),
        h('label', { class: 'check', style: { marginTop: '10px' } }, cb, h('span', null, t('license.accept', { id: l.id }))));
    });
    const ok = h('button', { class: 'btn primary', onclick: () => { finish(true); m.close(); } }, icon('download', 18), t('install'));
    function update() { ok.disabled = checks.some((c) => !c.checked); }
    const m = openModal({
      title: t('review.title'), width: 620, onClose: () => finish(false),
      body: h('div', null,
        h('p', { class: 'muted', style: { marginBottom: '10px' } }, t('review.intro')),
        h('div', { class: 'table-wrap' }, h('table', { class: 'table' },
          h('tbody', null, plan.install.map((p) => h('tr', null,
            h('td', null, h('b', null, p.name), h('div', { class: 'small muted' }, p.path)),
            h('td', { class: 'num muted' }, fmtBytes(p.size)))))),
        ),
        h('div', { class: 'row', style: { marginTop: '10px', justifyContent: 'flex-end' } },
          h('span', { class: 'muted' }, t('review.total', { size: fmtBytes(plan.size) }))),
        ...licenseBlocks,
      ),
      footer: [h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')), ok],
    });
    update();
    // abre a primeira licença para o usuário ver que existe
    const d = m.el.querySelector('details'); if (d) d.open = plan.licenses.length === 1;
  });
}

/** Instala pacotes (resolvendo dependências e licenças). Devolve a tarefa ou null. */
export async function installPackages(paths, { silent = false } = {}) {
  try {
    const plan = await post('/api/packages/plan', { paths });
    plan.install = plan.install || [];
    plan.licenses = plan.licenses || [];
    if (!plan.install.length) {
      if (!silent) toast(t('already.installed'), 'ok');
      return null;
    }
    const ok = await reviewDialog(plan);
    if (!ok) return null;
    const res = await post('/api/packages/install', { paths, accept: plan.licenses.map((l) => l.id) });
    if (res.task) {
      state.tasks.set(res.task.id, res.task);
      emit('tasks');
    }
    return res.task || null;
  } catch (e) {
    errToast(e, t('install.failed'));
    return null;
  }
}

export async function uninstallPackages(paths, names) {
  try {
    let res;
    try {
      res = await post('/api/packages/uninstall', { paths });
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && Array.isArray(e.data)) {
        const yes = await confirmDialog({
          title: t('uninstall.inuse.title'),
          message: t('uninstall.inuse.msg', { avds: e.data.join(', ') }),
          confirm: t('uninstall.anyway'), danger: true,
        });
        if (!yes) return null;
        res = await post('/api/packages/uninstall', { paths, force: true });
      } else throw e;
    }
    if (res.task) { state.tasks.set(res.task.id, res.task); emit('tasks'); }
    return res.task;
  } catch (e) {
    errToast(e, t('uninstall.failed'));
    return null;
  }
}
