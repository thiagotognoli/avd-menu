// Formulário de configuração de um AVD (usado ao criar e ao editar).
import { h, icon } from '../util.js';
import { t } from '../i18n.js';
import { state } from '../store.js';

const opt = (value, label) => ({ value, label });

function select(options, current, onchange) {
  const s = h('select', { onchange: onchange ? () => onchange(s.value) : null },
    options.map((o) => h('option', { value: o.value, selected: o.value === current || null }, o.label)));
  if (!options.some((o) => o.value === current)) s.prepend(h('option', { value: current, selected: true }, current));
  s.value = current;
  return s;
}

function field(label, control, hint) {
  return h('div', { class: 'field' }, h('label', null, label), control, hint ? h('div', { class: 'hint' }, hint) : null);
}

function num(v, { min, max, step = 1 } = {}) {
  return h('input', { type: 'number', value: v, min, max, step });
}

function unit(input, u) { return h('div', { class: 'unit' }, input, h('span', null, u)); }

function seg(options, current, onchange) {
  const wrap = h('div', { class: 'seg' });
  const render = (cur) => {
    wrap.replaceChildren(...options.map((o) => h('button', { type: 'button', class: o.value === cur ? 'on' : '', onclick: () => { render(o.value); onchange(o.value); } }, o.label)));
  };
  render(current);
  return wrap;
}

/**
 * Cria o formulário. `v` é o objeto "settings" da API.
 * opts: { showName, showId:{value,onchange}, extra: Node[] }
 */
export function avdForm(v, opts = {}) {
  const s = { ...v };
  const name = h('input', { type: 'text', value: v.displayName || '', oninput: () => { s.displayName = name.value; opts.onName && opts.onName(name.value); } });
  const orient = seg([opt('portrait', t('form.portrait')), opt('landscape', t('form.landscape'))], v.orientation, (x) => { s.orientation = x; });
  const gpu = seg([opt('auto', t('form.gpu.auto')), opt('host', t('form.gpu.host')), opt('swiftshader_indirect', t('form.gpu.soft'))],
    ['auto', 'host', 'swiftshader_indirect'].includes(v.gpuMode) ? v.gpuMode : 'auto', (x) => { s.gpuMode = x; });
  const boot = seg([opt('quick', t('form.boot.quick')), opt('cold', t('form.boot.cold'))], v.bootMode, (x) => { s.bootMode = x; });
  const cards = state.info && state.info.gpuCards;
  const card = cards ? seg([opt('integrated', t('form.gpucard.integrated')), opt('dedicated', t('form.gpucard.dedicated'))], v.gpuCard || 'integrated', (x) => { s.gpuCard = x; }) : null;

  const ram = num(v.ramMB, { min: 256, max: 65536, step: 128 });
  const heap = num(v.vmHeapMB, { min: 16, max: 4096, step: 16 });
  const cores = num(v.cores, { min: 1, max: 64 });
  const data = num(v.dataMB, { min: 512, step: 512 });
  const sd = num(v.sdCardMB, { min: 0, step: 128 });
  const camF = select([opt('none', t('form.cam.none')), opt('emulated', t('form.cam.emulated')), opt('webcam0', t('form.cam.webcam'))], v.cameraFront, (x) => { s.cameraFront = x; });
  const camB = select([opt('none', t('form.cam.none')), opt('emulated', t('form.cam.emulated')), opt('virtualscene', t('form.cam.virtual')), opt('webcam0', t('form.cam.webcam'))], v.cameraBack, (x) => { s.cameraBack = x; });
  const speed = select(['full', 'lte', 'hsdpa', 'hsupa', 'umts', 'edge', 'gprs', 'hscsd', 'gsm'].map((x) => opt(x, x === 'full' ? t('form.net.full') : x.toUpperCase())), v.netSpeed, (x) => { s.netSpeed = x; });
  const latency = select(['none', 'umts', 'edge', 'gprs'].map((x) => opt(x, x === 'none' ? t('form.lat.none') : x.toUpperCase())), v.netLatency, (x) => { s.netLatency = x; });
  const kb = h('input', { type: 'checkbox', checked: v.keyboard || null });
  const frame = h('input', { type: 'checkbox', checked: v.showFrame || null });

  const adv = h('details', { open: opts.advancedOpen || null, style: { marginTop: '18px' } },
    h('summary', { style: { cursor: 'pointer', fontWeight: 650, padding: '6px 0' } }, t('form.advanced')),
    h('div', { style: { display: 'flex', flexDirection: 'column', gap: '18px', marginTop: '12px' } },
      h('div', { class: 'cols' },
        field(t('form.cam.front'), camF), field(t('form.cam.back'), camB),
        field(t('form.net.speed'), speed), field(t('form.net.latency'), latency)),
      h('div', { class: 'cols three' },
        field(t('form.ram'), unit(ram, 'MB')), field(t('form.heap'), unit(heap, 'MB')), field(t('form.cores'), cores)),
      h('div', { class: 'cols' },
        field(t('form.storage'), unit(data, 'MB'), t('form.storage.hint')),
        field(t('form.sd'), unit(sd, 'MB'), t('form.sd.hint'))),
      h('div', { class: 'row wrap', style: { gap: '28px' } },
        h('label', { class: 'check' }, kb, h('span', null, t('form.keyboard'))),
        h('label', { class: 'check' }, frame, h('span', null, t('form.frame')))),
      ...(opts.advancedExtra || []),
    ));

  const el = h('div', { style: { display: 'flex', flexDirection: 'column', gap: '18px' } },
    opts.showName === false ? null : field(t('form.name'), name),
    h('div', { class: 'cols' },
      field(t('form.orientation'), orient),
      field(t('form.boot'), boot, t('form.boot.hint'))),
    field(t('form.gpu'), gpu, state.info && state.info.gpuSmart ? t('form.gpu.hint.smart') : t('form.gpu.hint')),
    card ? field(t('form.gpucard'), card, t('form.gpucard.hint', { i: cards.integrated, d: cards.dedicated })) : null,
    adv);

  const posInt = (input) => { const n = parseInt(input.value, 10); return Number.isFinite(n) && n > 0 ? n : undefined; };
  return {
    el,
    focusName: () => name.focus(),
    /** Corpo para a API (campos vazios/zerados são omitidos). */
    value() {
      const out = {
        displayName: s.displayName?.trim() || undefined,
        orientation: s.orientation, gpuMode: s.gpuMode, bootMode: s.bootMode, gpuCard: cards ? s.gpuCard : undefined,
        cameraFront: s.cameraFront, cameraBack: s.cameraBack, netSpeed: s.netSpeed, netLatency: s.netLatency,
        ramMB: posInt(ram), vmHeapMB: posInt(heap), cores: posInt(cores), dataMB: posInt(data),
        sdCardMB: Math.max(0, parseInt(sd.value, 10) || 0),
        keyboard: kb.checked, showFrame: frame.checked,
      };
      return out;
    },
  };
}
