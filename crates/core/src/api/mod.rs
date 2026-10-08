//! API que a interface usa (mesmas rotas “REST” de antes, só que sem HTTP: o
//! shell do Tauri entrega método + caminho + corpo JSON e recebe JSON de volta).

mod avds;
mod packages;
pub mod views;

use crate::sdk::{self, Catalog, Sdk};
use crate::{avd, config, devices, emu, platform, shortcut, tasks, Error, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

/// Estado compartilhado da aplicação.
pub struct App {
    pub tasks: tasks::Manager,
    emit: Emit,
    cat: Mutex<Option<(Arc<Catalog>, Instant)>>,
    quit: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

fn de<T: DeserializeOwned + Default>(body: Value) -> Result<T> {
    if body.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(body).map_err(|e| Error::bad(trf!("JSON inválido: {}", "invalid JSON: {}", e)))
}

fn ok() -> Value {
    json!({"ok": true})
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> BTreeMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(p), String::new()),
        })
        .collect()
}

impl App {
    /// `emit(evento, dados)` entrega eventos em tempo real à interface
    /// (“running”, “task”, “refresh”).
    pub fn new(emit: impl Fn(&str, Value) + Send + Sync + 'static) -> Arc<App> {
        let emit: Emit = Arc::new(emit);
        let e2 = emit.clone();
        Arc::new(App {
            tasks: tasks::Manager::new(move |t| e2("task", serde_json::to_value(t).unwrap_or(Value::Null))),
            emit,
            cat: Mutex::new(None),
            quit: Mutex::new(None),
        })
    }

    /// Define o que “Encerrar o AVD Menu” faz.
    pub fn set_quit(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.quit.lock().unwrap() = Some(Arc::new(f));
    }

    pub fn emit(&self, event: &str, data: Value) {
        (self.emit)(event, data);
    }

    /// O SDK configurado/detectado.
    pub fn sdk(&self) -> Sdk {
        sdk::detect(&config::load().sdk_root)
    }

    /// Thread que avisa a interface quando emuladores sobem ou descem.
    pub fn spawn_poller(self: &Arc<Self>) {
        let app = self.clone();
        std::thread::spawn(move || {
            let mut prev: Option<Value> = None;
            loop {
                let cur = serde_json::to_value(emu::running()).unwrap_or(Value::Null);
                if prev.as_ref() != Some(&cur) {
                    app.emit("running", cur.clone());
                    prev = Some(cur);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }

    /// Em segundo plano, atualiza os atalhos já criados (versões novas trazem
    /// ações e variáveis novas; a máquina pode ter ganhado uma placa de vídeo) e
    /// o tema do emulador — que também vale para quem abre pelo atalho.
    pub fn spawn_shortcut_refresh(self: &Arc<Self>) {
        let app = self.clone();
        std::thread::spawn(move || {
            let _ = emu::theme::sync(&config::load().emulator_theme);
            let sd = app.sdk();
            for a in avd::list(&sd) {
                let _ = shortcut::refresh(&sd, &a.name);
            }
        });
    }

    pub fn broadcast_running(&self) {
        self.emit("running", serde_json::to_value(emu::running()).unwrap_or(Value::Null));
    }

    pub(crate) fn catalog(&self, force: bool) -> Result<Arc<Catalog>> {
        let mut g = self.cat.lock().unwrap_or_else(|e| e.into_inner());
        if !force {
            if let Some((c, at)) = g.as_ref() {
                if at.elapsed() < Duration::from_secs(15 * 60) {
                    return Ok(c.clone());
                }
            }
        }
        match sdk::manifest::fetch_catalog(force) {
            Ok(c) => {
                let c = Arc::new(c);
                *g = Some((c.clone(), Instant::now()));
                Ok(c)
            }
            Err(e) => match g.as_ref() {
                Some((c, _)) => Ok(c.clone()),
                None => Err(e),
            },
        }
    }

    fn clear_catalog(&self) {
        *self.cat.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Despacha uma chamada da interface. `path` pode ter query string.
    pub fn handle(self: &Arc<Self>, method: &str, path: &str, body: Value) -> Result<Value> {
        let (p, q) = path.split_once('?').unwrap_or((path, ""));
        let query = parse_query(q);
        let segs: Vec<String> = p.trim_matches('/').split('/').map(percent_decode).collect();
        let s: Vec<&str> = segs.iter().map(String::as_str).collect();
        match (method, s.as_slice()) {
            ("GET", ["api", "ping"]) => Ok(json!({"ok": true, "version": platform::version()})),
            ("GET", ["api", "state"]) => self.state(),
            ("POST", ["api", "settings"]) => self.post_settings(body),
            ("POST", ["api", "lang"]) => {
                let lang = body.get("lang").and_then(Value::as_str).unwrap_or("");
                crate::lang::set_runtime_lang(lang);
                Ok(ok())
            }
            ("POST", ["api", "sdk", "setup"]) => self.sdk_setup(body),
            ("GET", ["api", "fs"]) => self.list_fs(query.get("path").map(String::as_str).unwrap_or("")),
            ("GET", ["api", "diag"]) => Ok(json!({"checks": emu::diagnose(&self.sdk())})),
            ("POST", ["api", "shutdown"]) => {
                if let Some(q) = self.quit.lock().unwrap().clone() {
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(200));
                        q();
                    });
                }
                Ok(ok())
            }
            ("POST", ["api", "self", "install"]) => Ok(serde_json::to_value(shortcut::install_self()?)?),
            ("POST", ["api", "self", "uninstall"]) => {
                shortcut::uninstall_self()?;
                Ok(ok())
            }
            // dispositivos (perfis de hardware)
            ("GET", ["api", "devices"]) => Ok(json!({"devices": devices::all()})),
            ("POST", ["api", "devices"]) => {
                let d: devices::Device = de(body)?;
                Ok(json!({"device": devices::save_user(d)?}))
            }
            ("DELETE", ["api", "devices", id]) => {
                devices::delete_user(id)?;
                Ok(ok())
            }
            // tarefas
            ("GET", ["api", "tasks"]) => Ok(json!({"tasks": self.tasks.list()})),
            ("POST", ["api", "tasks", "clear"]) => {
                self.tasks.clear();
                Ok(ok())
            }
            ("POST", ["api", "tasks", id, "cancel"]) => {
                if self.tasks.cancel(id) {
                    Ok(ok())
                } else {
                    Err(err!(404, "tarefa não encontrada", "task not found"))
                }
            }
            // pacotes do SDK
            ("GET", ["api", "packages"]) => self.list_packages(&query),
            ("POST", ["api", "packages", "plan"]) => self.plan_packages(body),
            ("POST", ["api", "packages", "install"]) => self.install_packages(body),
            ("POST", ["api", "packages", "uninstall"]) => self.uninstall_packages(body),
            // AVDs
            ("GET", ["api", "avds"]) => self.list_avds(),
            ("POST", ["api", "avds"]) => self.create_avd(body),
            ("POST", ["api", "avds", "preview"]) => self.preview_avd(body),
            ("GET", ["api", "avds", name]) => self.get_avd(name),
            ("PUT", ["api", "avds", name]) => self.update_avd(name, body),
            ("DELETE", ["api", "avds", name]) => self.delete_avd(name),
            ("POST", ["api", "avds", name, "start"]) => self.start_avd(name, body),
            ("POST", ["api", "avds", name, "stop"]) => self.stop_avd(name),
            ("POST", ["api", "avds", name, "wipe"]) => self.wipe_avd(name),
            ("POST", ["api", "avds", name, "duplicate"]) => self.duplicate_avd(name, body),
            ("POST", ["api", "avds", name, "show"]) => self.show_avd(name),
            ("PUT", ["api", "avds", name, "raw"]) => self.put_raw(name, body),
            ("PUT", ["api", "avds", name, "args"]) => self.put_args(name, body),
            ("GET", ["api", "avds", name, "log"]) => self.get_log(name),
            ("POST", ["api", "avds", name, "shortcut"]) => self.post_shortcut(name, body),
            ("DELETE", ["api", "avds", name, "shortcut"]) => self.delete_shortcut(name),
            _ => Err(Error::not_found(format!("{method} {path}"))),
        }
    }

    // ---- estado e configurações ----------------------------------------------------

    fn state(&self) -> Result<Value> {
        let cfg = config::load();
        let sd = self.sdk();
        let gpus = emu::list_gpus();
        let os = if platform::is_mac() { "darwin" } else { "linux" };
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
        Ok(json!({
            "version": platform::version(),
            "os": os,
            "arch": arch,
            "hostArch": platform::host_arch(),
            "sdk": {
                "root": sd.root,
                "exists": sd.exists(),
                "hasEmulator": sd.has_emulator(),
                "hasAdb": sd.has_adb(),
                "configured": !cfg.sdk_root.is_empty(),
                "defaultRoot": sdk::default_root(),
            },
            "candidates": sdk::candidates(&cfg.sdk_root),
            "config": {"showPreview": cfg.show_preview, "lang": cfg.lang, "theme": cfg.theme, "emulatorTheme": cfg.emulator_theme},
            "avdHome": avd::home(),
            "shortcuts": shortcut::supported(),
            "self": shortcut::self_info(),
            "canInstallSelf": platform::is_linux(),
            "dgpu": !emu::dedicated_gpu_env().is_empty(),
            // nomes das duas placas para o formulário do AVD
            "gpuCards": match emu::gpu::pick(&gpus) {
                (Some(i), Some(d)) => json!({"integrated": gpus[i].name, "dedicated": gpus[d].name}),
                _ => Value::Null,
            },
            "gpuSmart": emu::prefer_host_gpu(),
            "gpus": gpus,
            "exe": platform::exe(),
            "appImage": std::env::var_os("APPIMAGE").is_some(),
        }))
    }

    fn post_settings(&self, body: Value) -> Result<Value> {
        let show = body.get("showPreview").and_then(Value::as_bool);
        let lang = body.get("lang").and_then(Value::as_str).filter(|l| l.is_empty() || crate::lang::normalize(l).is_some()).map(str::to_string);
        let theme = body.get("theme").and_then(Value::as_str).map(str::to_string);
        let emu_theme = body.get("emulatorTheme").and_then(Value::as_str).filter(|t| matches!(*t, "" | "light" | "dark" | "keep")).map(str::to_string);
        config::update(|c| {
            if let Some(v) = show {
                c.show_preview = v;
            }
            if let Some(v) = lang {
                c.lang = v;
            }
            if let Some(v) = theme {
                c.theme = v;
            }
            if let Some(v) = emu_theme.clone() {
                c.emulator_theme = v;
            }
        })?;
        if let Some(t) = emu_theme {
            let _ = emu::theme::sync(&t);
        }
        if show.is_some() {
            self.clear_catalog();
        }
        Ok(ok())
    }

    fn sdk_setup(&self, body: Value) -> Result<Value> {
        let mut root = body.get("root").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let create = body.get("create").and_then(Value::as_bool).unwrap_or(false);
        if let Some(rest) = root.strip_prefix('~') {
            root = format!("{}{}", platform::home().display(), rest);
        }
        if !root.is_empty() && !root.starts_with('/') {
            return Err(err!(400, "informe um caminho absoluto", "enter an absolute path"));
        }
        if !root.is_empty() {
            let p = std::path::Path::new(&root);
            if !platform::dir_exists(p) {
                if !create {
                    return Err(err!(400, "a pasta {} não existe", "the folder {} does not exist", root));
                }
                std::fs::create_dir_all(p).map_err(|e| err!(400, "não foi possível criar {}: {}", "could not create {}: {}", root, e))?;
            }
        }
        config::update(|c| c.sdk_root = root.clone())?;
        self.clear_catalog();
        self.state()
    }

    /// Lista subpastas para o seletor de pasta da interface.
    fn list_fs(&self, path: &str) -> Result<Value> {
        let mut p = if path.is_empty() { platform::home() } else { std::path::PathBuf::from(path) };
        if let Some(rest) = path.strip_prefix('~') {
            p = platform::home().join(rest.trim_start_matches('/'));
        }
        let mut entries = std::fs::read_dir(&p);
        if entries.is_err() {
            // caminho digitado pela metade: lista o pai
            let parent = p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| "/".into());
            entries = std::fs::read_dir(&parent);
            if entries.is_err() {
                return Err(err!(400, "não foi possível abrir {}", "could not open {}", p.display()));
            }
            p = parent;
        }
        let mut dirs: Vec<String> = entries
            .unwrap()
            .flatten()
            .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .collect();
        dirs.sort();
        let sd = Sdk::new(&p);
        Ok(json!({
            "path": p,
            "parent": p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| "/".into()),
            "dirs": dirs,
            "isSdk": sd.has_emulator() || sd.has_adb() || platform::dir_exists(&p.join("licenses")),
        }))
    }
}

/// Abre uma pasta no gerenciador de arquivos.
pub fn open_path(p: &std::path::Path) -> Result<()> {
    let name = if platform::is_mac() { "open" } else { "xdg-open" };
    let bin = platform::which(name).ok_or_else(|| Error::bad(trf!("{} não encontrado", "{} not found", name)))?;
    let mut cmd = std::process::Command::new(bin);
    cmd.arg(p).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    platform::clean_env(&mut cmd);
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Divide uma linha de argumentos respeitando aspas simples/duplas.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    has = true;
                }
                ' ' | '\t' | '\n' => {
                    if has || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        has = false;
                    }
                }
                c => cur.push(c),
            },
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_respects_quotes() {
        assert_eq!(split_args(r#"-gpu host -dns-server "8.8.8.8" -prop 'a b'  c"#), vec!["-gpu", "host", "-dns-server", "8.8.8.8", "-prop", "a b", "c"]);
    }

    #[test]
    fn percent_and_query() {
        assert_eq!(percent_decode("a%20b%2Fc+d%"), "a b/c d%");
        let q = parse_query("refresh=1&device=pixel%209&x");
        assert_eq!(q["refresh"], "1");
        assert_eq!(q["device"], "pixel 9");
        assert_eq!(q["x"], "");
    }
}
