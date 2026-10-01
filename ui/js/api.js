// Ponte com o núcleo em Rust.
//
// No app (Tauri) as chamadas vão por `invoke`; os eventos em tempo real chegam
// pelo canal "avd-event". Num navegador comum (modo de desenvolvimento/testes,
// com o `devserver`) as mesmas rotas são chamadas por HTTP e os eventos por
// long-polling.

export class ApiError extends Error {
  constructor(message, status, data) {
    super(message);
    this.status = status;
    this.data = data;
  }
}

const tauri = window.__TAURI__;
export const inApp = !!(tauri && tauri.core);

function fromTauriError(e) {
  let o = null;
  try { o = typeof e === 'string' ? JSON.parse(e) : e; } catch (_) { /* texto puro */ }
  if (o && typeof o === 'object' && (o.error || o.message)) return new ApiError(o.error || o.message, o.status || 500, o.data);
  return new ApiError(String(e), 500);
}

export async function api(method, path, body) {
  if (inApp) {
    try {
      return await tauri.core.invoke('api', { method, path, body: body === undefined ? null : body });
    } catch (e) {
      throw fromTauriError(e);
    }
  }
  const opt = { method, headers: {} };
  if (body !== undefined) {
    opt.headers['Content-Type'] = 'application/json';
    opt.body = JSON.stringify(body);
  }
  let res;
  try {
    res = await fetch(path, opt);
  } catch (e) {
    throw new ApiError('offline', 0);
  }
  let data = null;
  const text = await res.text();
  if (text) { try { data = JSON.parse(text); } catch (_) { data = { error: text }; } }
  if (!res.ok) throw new ApiError((data && data.error) || res.statusText, res.status, data && data.data);
  return data;
}

export const get = (p) => api('GET', p);
export const post = (p, b) => api('POST', p, b ?? {});
export const put = (p, b) => api('PUT', p, b ?? {});
export const del = (p) => api('DELETE', p);

/** Recebe eventos do núcleo (running, task, refresh). Devolve uma função para parar. */
export function connectEvents(handlers) {
  const dispatch = (event, data) => { const fn = handlers[event]; if (fn) { try { fn(data); } catch (e) { console.error(e); } } };
  let stopped = false;
  let unlisten = null;

  // Estado inicial (tarefas em andamento e emuladores rodando).
  get('/api/tasks').then((r) => (r.tasks || []).forEach((t) => dispatch('task', t))).catch(() => {});
  get('/api/avds').then((r) => {
    const map = {};
    (r.avds || []).forEach((a) => { if (a.running) map[a.name] = a.running; });
    dispatch('running', map);
  }).catch(() => {});

  if (inApp) {
    tauri.event.listen('avd-event', (e) => dispatch(e.payload.event, e.payload.data)).then((u) => { unlisten = u; if (stopped) u(); });
    if (handlers._status) handlers._status(true);
    return () => { stopped = true; if (unlisten) unlisten(); };
  }
  // Navegador (devserver): long-polling.
  (async () => {
    let since = 0;
    while (!stopped) {
      try {
        const r = await (await fetch('/api/events?since=' + since)).json();
        if (handlers._status) handlers._status(true);
        since = r.seq;
        (r.events || []).forEach((ev) => dispatch(ev.event, ev.data));
      } catch (_) {
        if (handlers._status) handlers._status(false);
        await new Promise((res) => setTimeout(res, 1500));
      }
    }
  })();
  return () => { stopped = true; };
}
