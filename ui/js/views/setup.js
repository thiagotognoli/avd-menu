// Cartão de configuração inicial do SDK (local + componentes essenciais).
import { h, icon } from '../util.js';
import { state } from '../store.js';
import { t } from '../i18n.js';
import { installPackages } from '../sdkops.js';
import { sdkLocationDialog } from './settings.js';

export function setupNeeded() {
  const s = state.info && state.info.sdk;
  return !s || !s.exists || !s.hasEmulator || !s.hasAdb;
}

export function setupCard() {
  const s = state.info.sdk;
  const need = [];
  if (!s.hasEmulator) need.push('emulator');
  if (!s.hasAdb) need.push('platform-tools');
  const row = (ok, label, sub) => h('div', { class: 'row', style: { alignItems: 'flex-start' } },
    h('span', { style: { color: ok ? 'var(--ok)' : 'var(--warn)', marginTop: '1px' } }, icon(ok ? 'check' : 'warning', 20)),
    h('div', null, h('b', null, label), sub ? h('div', { class: 'muted small' }, sub) : null));
  return h('div', { class: 'card pad', style: { marginBottom: '18px' } },
    h('div', { class: 'row', style: { alignItems: 'flex-start', gap: '18px' } },
      h('div', { class: 'empty', style: { padding: 0, textAlign: 'start' } },
        h('div', { class: 'big', style: { width: '64px', height: '64px', borderRadius: '18px', margin: 0 } }, icon('sdk', 32))),
      h('div', { class: 'grow', style: { display: 'flex', flexDirection: 'column', gap: '12px' } },
        h('div', null, h('h2', null, t('setup.title')), h('p', { class: 'muted' }, t('setup.intro'))),
        row(s.exists, t('setup.sdk'), s.root),
        row(s.hasEmulator, t('setup.emulator'), t('setup.emulator.sub')),
        row(s.hasAdb, t('setup.adb'), t('setup.adb.sub')),
        h('div', { class: 'row wrap' },
          need.length ? h('button', { class: 'btn primary', onclick: () => installPackages(need) }, icon('download', 18), t('setup.install')) : null,
          h('button', { class: 'btn outline', onclick: () => sdkLocationDialog() }, icon('folder', 18), t('setup.change')),
        ),
      )));
}
