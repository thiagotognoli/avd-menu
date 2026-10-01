// Painel flutuante de tarefas (downloads/instalações).
import { h, icon, fmtBytes, clear } from './util.js';
import { post } from './api.js';
import { state, on, activeTasks } from './store.js';
import { t } from './i18n.js';

let expanded = false;
let openLogs = new Set();

export function initTasks() {
  on('tasks', render);
  on('lang', render);
  render();
}

function phaseLabel(task) {
  const map = { download: t('phase.download'), verify: t('phase.verify'), extract: t('phase.extract'), finalize: t('phase.finalize'), remove: t('phase.remove') };
  return map[task.phase] || '';
}

function taskEl(task) {
  const running = task.state === 'running' || task.state === 'queued';
  const pct = Math.round((task.progress || 0) * 100);
  const status =
    task.state === 'queued' ? t('task.queued') :
    task.state === 'running' ? [phaseLabel(task), task.total ? `${fmtBytes(task.done)} / ${fmtBytes(task.total)}` : '', task.detail].filter(Boolean).join(' · ') :
    task.state === 'done' ? t('task.done') :
    task.state === 'canceled' ? t('task.canceled') : (task.error || t('task.failed'));
  const kindIcon = task.kind === 'uninstall' ? 'trash' : 'download';
  return h('div', { class: 'task' },
    h('div', { class: 't' },
      icon(task.state === 'done' ? 'check' : task.state === 'error' ? 'error' : kindIcon, 18, task.state === 'error' ? '' : ''),
      h('b', { title: task.title }, (task.kind === 'uninstall' ? t('task.removing') : t('task.installing')) + ' ' + task.title),
      running ? h('span', { class: 'muted small' }, pct + '%') : null,
      running ? h('button', { class: 'btn icon ghost sm', title: t('cancel'), onclick: () => post(`/api/tasks/${task.id}/cancel`) }, icon('close', 16)) : null,
      h('button', { class: 'btn icon ghost sm', title: 'log', onclick: () => { openLogs.has(task.id) ? openLogs.delete(task.id) : openLogs.add(task.id); render(); } }, icon('log', 16)),
    ),
    running ? h('div', { class: 'bar' + (task.progress > 0 ? '' : ' indet') }, h('i', { style: { width: pct + '%' } })) : null,
    h('div', { class: 's', style: task.state === 'error' ? { color: 'var(--danger)' } : null }, status),
    openLogs.has(task.id) ? h('pre', null, (task.log || []).join('\n') || '—') : null,
  );
}

export function render() {
  const box = document.getElementById('tasks');
  clear(box);
  const all = [...state.tasks.values()];
  if (!all.length) return;
  const act = activeTasks();
  const overall = act.length ? act.reduce((s, x) => s + (x.progress || 0), 0) / act.length : 1;
  const head = h('div', { class: 'tasks-head', onclick: () => { expanded = !expanded; render(); } },
    act.length ? icon('refresh', 18, 'spin') : icon('check', 18),
    h('span', { class: 'grow' }, act.length ? t('tasks.active', { n: act.length }) : t('tasks.title')),
    act.length ? h('span', { class: 'muted small' }, Math.round(overall * 100) + '%') : null,
    !act.length ? h('button', { class: 'btn ghost sm', onclick: (e) => { e.stopPropagation(); post('/api/tasks/clear'); state.tasks.forEach((v, k) => { if (v.state !== 'running' && v.state !== 'queued') state.tasks.delete(k); }); render(); } }, t('tasks.clear')) : null,
    icon('expand', 18, ''),
  );
  const show = expanded || act.length > 0;
  box.append(h('div', { class: 'tasks-card' }, head,
    show && (expanded || act.length) ? h('div', { class: 'tasks-list' }, all.slice().reverse().map(taskEl)) : null));
}
