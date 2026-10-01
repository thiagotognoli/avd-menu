use super::{de, ok, open_path, split_args, App};
use crate::avd::{self, CreateSpec, Settings};
use crate::config;
use crate::devices;
use crate::emu::{self, StartOptions};
use crate::shortcut;
use crate::{Error, Result};
use serde_json::{json, Value};
use std::sync::Arc;

fn name_ok(name: &str) -> Result<()> {
    if avd::valid_name(name) {
        Ok(())
    } else {
        Err(err!(400, "nome de AVD inválido", "invalid AVD name"))
    }
}

fn stop_first(name: &str, pt: &str, en: &str) -> Result<()> {
    if emu::running().contains_key(name) {
        return Err(Error::conflict(crate::lang::tr(pt, en)));
    }
    Ok(())
}

impl App {
    fn item(&self, a: &avd::Avd, running: &std::collections::BTreeMap<String, emu::Instance>, cfg: &config::Config) -> Value {
        let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
        let o = v.as_object_mut().unwrap();
        o.insert("shortcut".into(), serde_json::to_value(shortcut::lookup(&a.name)).unwrap());
        if let Some(inst) = running.get(&a.name) {
            o.insert("running".into(), serde_json::to_value(inst).unwrap());
        }
        if let Some(args) = cfg.avd_extra_args.get(&a.name) {
            o.insert("extraArgs".into(), json!(args));
        }
        v
    }

    pub(super) fn list_avds(&self) -> Result<Value> {
        let sd = self.sdk();
        let running = emu::running();
        let cfg = config::load();
        let items: Vec<Value> = avd::list(&sd).iter().map(|a| self.item(a, &running, &cfg)).collect();
        Ok(json!({"avds": items, "home": avd::home()}))
    }

    pub(super) fn get_avd(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        if !avd::exists(name) {
            return Err(err!(404, "o AVD {:?} não existe", "AVD {:?} does not exist", name));
        }
        let a = avd::load(&self.sdk(), name);
        let mut resp = json!({
            "avd": self.item(&a, &emu::running(), &config::load()),
            "pinned": shortcut::pinned(name),
            "log": emu::read_log_tail(name, 200),
        });
        if let Ok((cfg, _)) = avd::config(name) {
            resp["settings"] = serde_json::to_value(avd::view_of(&cfg, name))?;
            resp["raw"] = json!(cfg.to_text());
        }
        Ok(resp)
    }

    pub(super) fn create_avd(&self, body: Value) -> Result<Value> {
        let spec: CreateSpec = de(body.clone())?;
        let sd = self.sdk();
        // a moldura é enfeite: sem rede o AVD sai igual e a moldura é tentada de novo ao iniciar
        let skin_error = avd::fetch_skin_for_create(&sd, &spec).err();
        let a = avd::create(&sd, &spec)?;
        let mut resp = json!({"avd": a});
        if let Some(e) = skin_error {
            resp["skinError"] = json!(e.message);
        }
        if let Some(sc) = body.get("shortcut").filter(|v| v.is_object()) {
            let opt = shortcut::Options {
                avd: a.name.clone(),
                display_name: sc.get("displayName").and_then(Value::as_str).unwrap_or("").into(),
                icon_data: sc.get("iconData").and_then(Value::as_str).unwrap_or("").into(),
                pin: sc.get("pin").and_then(Value::as_bool).unwrap_or(false),
                ..Default::default()
            };
            match shortcut::create(&sd, &opt) {
                Ok(r) => resp["shortcut"] = serde_json::to_value(r)?,
                Err(e) => resp["shortcutError"] = json!(e.message),
            }
        }
        Ok(resp)
    }

    /// Valores padrão que um AVD novo teria (formulário da última etapa do assistente).
    pub(super) fn preview_avd(&self, body: Value) -> Result<Value> {
        let spec: CreateSpec = de(body)?;
        let dev = devices::get(&spec.device_id).ok_or_else(|| err!(400, "dispositivo desconhecido", "unknown device"))?;
        let img = self
            .sdk()
            .read_sys_image(&spec.image_pkg)
            .ok_or_else(|| err!(400, "a imagem de sistema não está instalada", "the system image is not installed"))?;
        let spec = CreateSpec { name: "preview".into(), ..spec };
        let cfg = avd::build_config(&dev, &img, &spec)?;
        Ok(json!({
            "settings": avd::view_of(&cfg, "preview"),
            "device": dev,
            "image": {"api": img.api, "tagId": img.tag_id, "tagDisplay": img.tag_display, "abi": img.abi, "playStore": img.play_store, "target": img.target, "dir": img.dir},
        }))
    }

    pub(super) fn update_avd(&self, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        let st: Settings = de(body)?;
        stop_first(name, "pare o emulador antes de editar o AVD", "stop the emulator before editing the AVD")?;
        let sd = self.sdk();
        // sem rede as outras opções são salvas do mesmo jeito; a moldura fica para a próxima
        let skin_error = avd::fetch_skin_for_update(&sd, name, &st).err();
        avd::update(&sd, name, &st)?;
        let mut resp = ok();
        if let Some(e) = skin_error {
            resp["skinError"] = json!(e.message);
        }
        Ok(resp)
    }

    pub(super) fn delete_avd(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        stop_first(name, "pare o emulador antes de apagar o AVD", "stop the emulator before deleting the AVD")?;
        if shortcut::lookup(name).exists {
            let _ = shortcut::remove(name);
        }
        avd::delete(name)?;
        let _ = config::update(|c| {
            c.avd_extra_args.remove(name);
            c.shortcuts.remove(name);
        });
        Ok(ok())
    }

    pub(super) fn start_avd(self: &Arc<Self>, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        let mut opt: StartOptions = de(body)?;
        let sd = self.sdk();
        if !avd::exists(name) {
            return Err(err!(404, "o AVD {:?} não existe", "AVD {:?} does not exist", name));
        }
        if let Some(extra) = config::load().avd_extra_args.get(name) {
            let mut args = split_args(extra);
            args.append(&mut opt.extra_args);
            opt.extra_args = args;
        }
        let app = self.clone();
        let pid = emu::start_watched(
            &sd,
            name,
            &opt,
            Some(Box::new(move |x| {
                app.emit("exited", json!({"name": x.name, "code": x.code, "log": x.log}));
                app.broadcast_running();
            })),
        )?;
        self.broadcast_running();
        Ok(json!({"pid": pid}))
    }

    pub(super) fn stop_avd(self: &Arc<Self>, name: &str) -> Result<Value> {
        name_ok(name)?;
        if !emu::running().contains_key(name) {
            return Err(Error::conflict(trf!("o emulador {:?} não está em execução", "emulator {:?} is not running", name)));
        }
        let (app, sd, name) = (self.clone(), self.sdk(), name.to_string());
        std::thread::spawn(move || {
            let _ = emu::stop(&sd, &name);
            app.broadcast_running();
        });
        Ok(ok())
    }

    pub(super) fn wipe_avd(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        stop_first(name, "pare o emulador antes de apagar os dados", "stop the emulator before wiping data")?;
        avd::wipe_data(name)?;
        Ok(ok())
    }

    pub(super) fn duplicate_avd(&self, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        stop_first(name, "pare o emulador antes de duplicar o AVD", "stop the emulator before duplicating the AVD")?;
        let new_name = body.get("name").and_then(Value::as_str).unwrap_or("");
        let display = body.get("displayName").and_then(Value::as_str).unwrap_or("");
        let a = avd::duplicate(&self.sdk(), name, new_name, display)?;
        Ok(json!({"avd": a}))
    }

    pub(super) fn show_avd(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        if !avd::exists(name) {
            return Err(err!(404, "o AVD {:?} não existe", "AVD {:?} does not exist", name));
        }
        let a = avd::load(&self.sdk(), name);
        open_path(std::path::Path::new(&a.dir))?;
        Ok(ok())
    }

    pub(super) fn put_raw(&self, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        stop_first(name, "pare o emulador antes de editar o config.ini", "stop the emulator before editing config.ini")?;
        avd::write_raw_config(name, body.get("content").and_then(Value::as_str).unwrap_or(""))?;
        Ok(ok())
    }

    pub(super) fn put_args(&self, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        let args = body.get("args").and_then(Value::as_str).unwrap_or("").trim().to_string();
        config::update(|c| {
            if args.is_empty() {
                c.avd_extra_args.remove(name);
            } else {
                c.avd_extra_args.insert(name.to_string(), args.clone());
            }
        })?;
        Ok(ok())
    }

    pub(super) fn get_log(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        Ok(json!({"text": emu::read_log_tail(name, 400)}))
    }

    pub(super) fn post_shortcut(&self, name: &str, body: Value) -> Result<Value> {
        name_ok(name)?;
        let mut opt: shortcut::Options = de(body)?;
        opt.avd = name.to_string();
        Ok(serde_json::to_value(shortcut::create(&self.sdk(), &opt)?)?)
    }

    pub(super) fn delete_shortcut(&self, name: &str) -> Result<Value> {
        name_ok(name)?;
        shortcut::remove(name)?;
        Ok(ok())
    }
}
