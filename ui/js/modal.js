// Modais, menus, toasts e diálogos de confirmação.
import { h, icon, clear } from './util.js';
import { t } from './i18n.js';

const layer = () => document.getElementById('layer');
const stack = [];

export function closeAllModals() { while (stack.length) stack[stack.length - 1].close(); }

/**
 * Abre um modal. `body` e `footer` são nós (ou funções que recebem o modal).
 * Devolve { el, body, footer, close, setTitle }.
 */
export function openModal({ title, body, footer, width = 560, tall = false, onClose, closable = true, steps }) {
  const titleEl = h('h2', null, title || '');
  const closeBtn = closable ? h('button', { class: 'btn icon ghost sm', title: t('close'), onclick: () => api.close() }, icon('close', 18)) : null;
  const bodyEl = h('div', { class: 'body' });
  const footerEl = h('footer');
  const stepsEl = h('div', { class: 'steps hidden' });
  const modal = h('div', { class: 'modal' + (tall ? ' tall' : ''), style: { '--w': width + 'px' }, role: 'dialog', 'aria-modal': 'true' },
    h('header', null, titleEl, closeBtn), stepsEl, bodyEl, footerEl);
  const overlay = h('div', { class: 'overlay', onmousedown: (e) => { if (e.target === overlay && closable && !overlay._drag) api.close(); } }, modal);
  const keyHandler = (e) => {
    if (e.key === 'Escape' && closable && stack[stack.length - 1] === api) { e.stopPropagation(); api.close(); }
  };
  let closed = false;
  const api = {
    el: modal, body: bodyEl, footer: footerEl,
    setTitle(s) { titleEl.textContent = s; },
    setBody(...n) { clear(bodyEl); n.flat().forEach((c) => c && bodyEl.append(c.nodeType ? c : document.createTextNode(c))); },
    setFooter(...n) { clear(footerEl); n.flat().forEach((c) => c && footerEl.append(c)); footerEl.classList.toggle('hidden', !n.flat().filter(Boolean).length); },
    setSteps(list, current) {
      clear(stepsEl);
      stepsEl.classList.toggle('hidden', !list);
      if (!list) return;
      list.forEach((s, i) => {
        if (i) stepsEl.append(h('span', { class: 'sep' }));
        stepsEl.append(h('span', { class: 's' + (i === current ? ' on' : i < current ? ' done' : '') }, h('span', { class: 'n' }, i < current ? icon('check', 14) : String(i + 1)), s));
      });
    },
    close(result) {
      if (closed) return; closed = true;
      document.removeEventListener('keydown', keyHandler, true);
      overlay.remove();
      const i = stack.indexOf(api); if (i >= 0) stack.splice(i, 1);
      if (onClose) onClose(result);
    },
  };
  if (body) api.setBody(typeof body === 'function' ? body(api) : body);
  if (footer) api.setFooter(typeof footer === 'function' ? footer(api) : footer);
  else footerEl.classList.add('hidden');
  if (steps) api.setSteps(steps.list, steps.current);
  layer().append(overlay);
  stack.push(api);
  document.addEventListener('keydown', keyHandler, true);
  const first = modal.querySelector('input:not([type=checkbox]),select,textarea');
  if (first && !('ontouchstart' in window)) setTimeout(() => first.focus(), 30);
  return api;
}

export function confirmDialog({ title, message, confirm, danger = false, details }) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const m = openModal({
      title, width: 480, onClose: () => finish(false),
      body: h('div', null, h('p', null, message), details || null),
      footer: [
        h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')),
        h('button', { class: 'btn ' + (danger ? 'danger' : 'primary'), onclick: () => { finish(true); m.close(); } }, confirm || t('ok')),
      ],
    });
  });
}

export function promptDialog({ title, label, value = '', confirm, validate, hint }) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const input = h('input', { type: 'text', value });
    const err = h('div', { class: 'small', style: { color: 'var(--danger)', minHeight: '18px' } });
    const submit = () => {
      const v = input.value.trim();
      const e = validate ? validate(v) : null;
      if (e) { err.textContent = e; return; }
      finish(v); m.close();
    };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') submit(); });
    const m = openModal({
      title, width: 480, onClose: () => finish(null),
      body: h('div', { class: 'field' }, h('label', null, label), input, hint ? h('div', { class: 'hint' }, hint) : null, err),
      footer: [
        h('button', { class: 'btn ghost', onclick: () => m.close() }, t('cancel')),
        h('button', { class: 'btn primary', onclick: submit }, confirm || t('ok')),
      ],
    });
    setTimeout(() => { input.focus(); input.select(); }, 40);
  });
}

/** Popup de menu ancorado num elemento. items: [{label, icon, onClick, danger, disabled}|'-'] */
export function popMenu(anchor, items) {
  document.querySelectorAll('.menu').forEach((m) => m.remove());
  const menu = h('div', { class: 'menu', role: 'menu' });
  const close = () => { menu.remove(); document.removeEventListener('mousedown', outside, true); document.removeEventListener('keydown', esc, true); window.removeEventListener('blur', close); };
  const outside = (e) => { if (!menu.contains(e.target)) close(); };
  const esc = (e) => { if (e.key === 'Escape') close(); };
  for (const it of items) {
    if (it === '-') { menu.append(h('hr')); continue; }
    if (!it) continue;
    menu.append(h('button', {
      class: it.danger ? 'danger' : '', disabled: it.disabled || null, role: 'menuitem',
      onclick: () => { close(); it.onClick && it.onClick(); },
    }, it.icon ? icon(it.icon, 18) : h('span', { style: { width: '18px' } }), h('span', null, it.label)));
  }
  layer().append(menu);
  const r = anchor.getBoundingClientRect();
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  let left = Math.min(r.right - mw, window.innerWidth - mw - 8);
  let top = r.bottom + 6;
  if (top + mh > window.innerHeight - 8) top = Math.max(8, r.top - mh - 6);
  menu.style.left = Math.max(8, left) + 'px';
  menu.style.top = top + 'px';
  setTimeout(() => {
    document.addEventListener('mousedown', outside, true);
    document.addEventListener('keydown', esc, true);
    window.addEventListener('blur', close);
  });
  return close;
}

export function toast(message, kind = 'info', detail) {
  const box = document.getElementById('toasts');
  const el = h('div', { class: 'toast ' + kind },
    icon(kind === 'err' ? 'error' : kind === 'ok' ? 'check' : 'info', 20),
    h('div', null, h('div', null, message), detail ? h('pre', null, detail) : null));
  box.append(el);
  const ttl = kind === 'err' ? 9000 : 4200;
  const timer = setTimeout(() => el.remove(), ttl);
  el.addEventListener('click', () => { clearTimeout(timer); el.remove(); });
}

export function errToast(e, prefix) {
  const msg = e && e.message ? e.message : String(e);
  const [first, ...rest] = msg.split('\n');
  toast((prefix ? prefix + ': ' : '') + first, 'err', rest.join('\n').trim() || undefined);
}
