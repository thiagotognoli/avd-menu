//! Operações demoradas (downloads, instalações) em segundo plano, com andamento
//! publicado para a interface.

use crate::cancel::{Cancel, CANCELED};
use crate::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Retrato (serializável) de uma tarefa.
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// pacotes do SDK envolvidos
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// queued | running | done | error | canceled
    pub state: String,
    /// 0..1
    pub progress: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub phase: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub done: i64,
    pub total: i64,
    #[serde(rename = "error", skip_serializing_if = "String::is_empty")]
    pub err: String,
    pub log: Vec<String>,
    pub started: u64,
    pub ended: u64,
}

struct Entry {
    task: Task,
}

struct Inner {
    tasks: Mutex<Vec<Entry>>,
    cancels: Mutex<HashMap<String, Cancel>>,
    notify: Box<dyn Fn(&Task) + Send + Sync>,
    serial: Mutex<()>,
    last_sent: Mutex<HashMap<String, Instant>>,
}

/// Guarda as tarefas e notifica mudanças.
#[derive(Clone)]
pub struct Manager {
    inner: Arc<Inner>,
}

/// O que a função da tarefa usa para reportar andamento.
pub struct Handle {
    inner: Arc<Inner>,
    id: String,
    pub cancel: Cancel,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    let mut b = [0u8; 6];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    if b == [0; 6] {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        b.copy_from_slice(&n.to_le_bytes()[..6]);
    }
    hex::encode(b)
}

impl Inner {
    fn update(&self, id: &str, force: bool, f: impl FnOnce(&mut Task)) {
        let snap = {
            let mut tasks = self.tasks.lock().unwrap();
            let Some(e) = tasks.iter_mut().find(|e| e.task.id == id) else { return };
            f(&mut e.task);
            e.task.clone()
        };
        self.publish(snap, force);
    }

    fn publish(&self, t: Task, force: bool) {
        {
            let mut last = self.last_sent.lock().unwrap();
            if !force && last.get(&t.id).map(|i| i.elapsed() < Duration::from_millis(200)).unwrap_or(false) {
                return;
            }
            last.insert(t.id.clone(), Instant::now());
        }
        (self.notify)(&t);
    }
}

fn push_log(log: &mut Vec<String>, s: String) {
    log.push(s);
    if log.len() > 300 {
        log.drain(..log.len() - 300);
    }
}

impl Handle {
    /// Atualiza o andamento geral (0..1) e os detalhes.
    pub fn progress(&self, frac: f64, phase: &str, detail: &str, done: i64, total: i64) {
        self.inner.update(&self.id, false, |t| {
            if frac >= 0.0 {
                t.progress = frac.min(1.0);
            }
            t.phase = phase.into();
            t.detail = detail.into();
            t.done = done;
            t.total = total;
        });
    }

    /// Acrescenta uma linha ao log da tarefa.
    pub fn logf(&self, line: String) {
        self.inner.update(&self.id, true, |t| push_log(&mut t.log, line));
    }
}

impl Manager {
    pub fn new(notify: impl Fn(&Task) + Send + Sync + 'static) -> Manager {
        Manager {
            inner: Arc::new(Inner {
                tasks: Mutex::new(vec![]),
                cancels: Mutex::new(HashMap::new()),
                notify: Box::new(notify),
                serial: Mutex::new(()),
                last_sent: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Dispara a tarefa em segundo plano. Com `serial` ela espera as outras
    /// tarefas seriais terminarem (instalações no SDK não podem se misturar).
    pub fn start<F>(&self, kind: &str, title: &str, paths: Vec<String>, serial: bool, f: F) -> Task
    where
        F: FnOnce(&Handle) -> Result<()> + Send + 'static,
    {
        let id = new_id();
        let cancel = Cancel::new();
        let task = Task {
            id: id.clone(),
            kind: kind.into(),
            title: title.into(),
            paths,
            state: "queued".into(),
            progress: 0.0,
            phase: String::new(),
            detail: String::new(),
            done: 0,
            total: 0,
            err: String::new(),
            log: vec![],
            started: now(),
            ended: 0,
        };
        self.inner.tasks.lock().unwrap().push(Entry { task: task.clone() });
        self.inner.cancels.lock().unwrap().insert(id.clone(), cancel.clone());
        self.inner.publish(task.clone(), true);

        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let _guard = if serial { Some(inner.serial.lock().unwrap_or_else(|e| e.into_inner())) } else { None };
            let finish = |res: Result<()>, cancel: &Cancel| {
                inner.update(&id, true, |t| {
                    t.ended = now();
                    match &res {
                        Ok(()) => {
                            t.state = "done".into();
                            t.progress = 1.0;
                        }
                        Err(e) if e.status == CANCELED || cancel.is_canceled() => t.state = "canceled".into(),
                        Err(e) => {
                            t.state = "error".into();
                            t.err = e.message.clone();
                            push_log(&mut t.log, format!("ERRO: {}", e.message));
                        }
                    }
                });
            };
            if cancel.is_canceled() {
                finish(Err(crate::Error::new(CANCELED, "canceled")), &cancel);
                return;
            }
            inner.update(&id, true, |t| t.state = "running".into());
            let h = Handle { inner: inner.clone(), id: id.clone(), cancel: cancel.clone() };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&h)))
                .unwrap_or_else(|_| Err(crate::Error::internal(trf!("erro interno", "internal error"))));
            finish(res, &cancel);
        });
        task
    }

    /// Pede o cancelamento de uma tarefa.
    pub fn cancel(&self, id: &str) -> bool {
        match self.inner.cancels.lock().unwrap().get(id) {
            Some(c) => {
                c.cancel();
                true
            }
            None => false,
        }
    }

    /// As 50 tarefas mais recentes, em ordem de criação.
    pub fn list(&self) -> Vec<Task> {
        let tasks = self.inner.tasks.lock().unwrap();
        let from = tasks.len().saturating_sub(50);
        tasks[from..].iter().map(|e| e.task.clone()).collect()
    }

    /// Remove as tarefas já terminadas.
    pub fn clear(&self) {
        let mut tasks = self.inner.tasks.lock().unwrap();
        let mut cancels = self.inner.cancels.lock().unwrap();
        let mut last = self.inner.last_sent.lock().unwrap();
        tasks.retain(|e| {
            let live = e.task.state == "queued" || e.task.state == "running";
            if !live {
                cancels.remove(&e.task.id);
                last.remove(&e.task.id);
            }
            live
        });
    }

    /// Há tarefas em andamento?
    pub fn active(&self) -> bool {
        self.inner.tasks.lock().unwrap().iter().any(|e| e.task.state == "queued" || e.task.state == "running")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn wait(m: &Manager, id: &str, want: &str) -> Task {
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(3) {
            if let Some(t) = m.list().into_iter().find(|t| t.id == id && t.state == want) {
                return t;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("tarefa {id} não chegou a {want}: {:?}", m.list());
    }

    #[test]
    fn success_failure_panic() {
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let m = Manager::new(move |t| ev.lock().unwrap().push(t.state.clone()));
        let ok = m.start("install", "ok", vec!["a".into()], false, |h| {
            h.progress(0.5, "download", "x", 5, 10);
            h.logf("linha 1".into());
            Ok(())
        });
        let done = wait(&m, &ok.id, "done");
        assert_eq!((done.progress, done.log.clone(), done.paths.clone()), (1.0, vec!["linha 1".to_string()], vec!["a".to_string()]));
        let bad = m.start("install", "bad", vec![], false, |_| Err(Error::internal("falhou")));
        assert_eq!(wait(&m, &bad.id, "error").err, "falhou");
        let pn = m.start("install", "panic", vec![], false, |_| panic!("boom"));
        assert!(!wait(&m, &pn.id, "error").err.is_empty());
        assert!(events.lock().unwrap().len() >= 6);
    }

    #[test]
    fn serial_queue_and_cancel() {
        let m = Manager::new(|_| {});
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let first = m.start("install", "primeira", vec![], true, move |_| {
            rx.lock().unwrap().recv().ok();
            Ok(())
        });
        wait(&m, &first.id, "running");
        let second = m.start("install", "segunda", vec![], true, |_| Ok(()));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(m.list().iter().find(|t| t.id == second.id).unwrap().state, "queued");
        assert!(m.active());
        tx.send(()).unwrap();
        wait(&m, &first.id, "done");
        wait(&m, &second.id, "done");

        let third = m.start("install", "terceira", vec![], false, |h| {
            while !h.cancel.is_canceled() {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(Error::new(CANCELED, "x"))
        });
        wait(&m, &third.id, "running");
        assert!(m.cancel(&third.id) && !m.cancel("nao-existe"));
        wait(&m, &third.id, "canceled");
        m.clear();
        assert!(m.list().is_empty());
    }
}
