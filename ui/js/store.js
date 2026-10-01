// Estado global e barramento de eventos da interface.
import { get } from './api.js';

export const state = {
  info: null,
  avds: [],
  avdsLoaded: false,
  running: {},
  devices: [],
  packages: null, // resposta de /api/packages
  tasks: new Map(),
  starting: new Set(),
  stopping: new Set(),
  connected: true,
};

const listeners = new Map();
export function on(evt, fn) {
  if (!listeners.has(evt)) listeners.set(evt, new Set());
  listeners.get(evt).add(fn);
  return () => listeners.get(evt).delete(fn);
}
export function emit(evt, data) {
  (listeners.get(evt) || []).forEach((fn) => { try { fn(data); } catch (e) { console.error(e); } });
}

export async function loadInfo() {
  state.info = await get('/api/state');
  emit('info');
  return state.info;
}

export async function loadAvds() {
  const r = await get('/api/avds');
  state.avds = r.avds || [];
  state.avdsLoaded = true;
  state.running = {};
  for (const a of state.avds) if (a.running) state.running[a.name] = a.running;
  emit('avds');
  return state.avds;
}

export async function loadDevices() {
  const r = await get('/api/devices');
  state.devices = r.devices || [];
  emit('devices');
  return state.devices;
}

export async function loadPackages(refresh = false, device) {
  if (device !== undefined) state.packagesDevice = device;
  device = state.packagesDevice || '';
  const q = [];
  if (refresh) q.push('refresh=1');
  if (device) q.push('device=' + encodeURIComponent(device));
  state.packages = await get('/api/packages' + (q.length ? '?' + q.join('&') : ''));
  emit('packages');
  return state.packages;
}

export function activeTasks() {
  return [...state.tasks.values()].filter((t) => t.state === 'queued' || t.state === 'running');
}

/** Tarefa de instalação em andamento para um pacote (pelo título/paths). */
export function taskFor(path) {
  for (const t of state.tasks.values()) {
    if ((t.state === 'queued' || t.state === 'running') && t.paths && t.paths.includes(path)) return t;
  }
  return null;
}
