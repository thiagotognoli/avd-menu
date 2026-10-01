use super::views::package_views;
use super::{de, App};
use crate::{avd, config, devices, platform, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default, Deserialize)]
#[serde(default)]
struct PkgReq {
    paths: Vec<String>,
    accept: Vec<String>,
    force: bool,
}

impl App {
    pub(super) fn list_packages(&self, q: &BTreeMap<String, String>) -> Result<Value> {
        let force = q.get("refresh").map(String::as_str) == Some("1");
        let cfg = config::load();
        let sd = self.sdk();
        let mut resp = json!({"sdkRoot": sd.root});
        let cat = match self.catalog(force) {
            Ok(c) => c,
            Err(e) => {
                // Sem rede e sem cache: ainda mostra o que está instalado.
                resp["offline"] = json!(true);
                resp["error"] = json!(e.message);
                resp["packages"] = serde_json::to_value(package_views(&sd, None, cfg.show_preview))?;
                return Ok(resp);
            }
        };
        resp["offline"] = json!(cat.offline);
        resp["fetched"] = json!(cat.fetched);
        let mut views = package_views(&sd, Some(&cat), cfg.show_preview);
        if let Some(dev) = q.get("device").filter(|d| !d.is_empty()).and_then(|id| devices::get(id)) {
            for v in views.iter_mut().filter(|v| v.kind == "sysimg") {
                v.allowed = Some(dev.tag_allowed(&v.tag));
            }
        }
        resp["packages"] = serde_json::to_value(views)?;
        Ok(resp)
    }

    fn plan(&self, req: &PkgReq) -> Result<(crate::sdk::Sdk, Arc<crate::sdk::Catalog>, crate::sdk::Plan)> {
        let sd = self.sdk();
        let cat = self
            .catalog(false)
            .map_err(|e| Error::new(502, trf!("sem acesso à lista de pacotes do Google: {}", "cannot reach Google’s package list: {}", e)))?;
        if req.paths.is_empty() {
            return Err(err!(400, "nenhum pacote informado", "no package given"));
        }
        let plan = sd.plan_install(&cat, &req.paths, config::load().show_preview, req.force);
        if !plan.missing.is_empty() {
            return Err(err!(400, "pacote indisponível para este sistema: {}", "package unavailable for this system: {}", plan.missing.join(", ")));
        }
        Ok((sd, cat, plan))
    }

    pub(super) fn plan_packages(&self, body: Value) -> Result<Value> {
        let req: PkgReq = de(body)?;
        let (_, _, plan) = self.plan(&req)?;
        let items: Vec<Value> = plan
            .install
            .iter()
            .map(|p| {
                let size = p.archive_for(platform::host_os(), platform::host_arch()).map(|a| a.size).unwrap_or(0);
                json!({"path": p.path, "name": p.name, "revision": p.revision.to_string(), "size": size})
            })
            .collect();
        Ok(json!({"install": items, "licenses": plan.licenses, "size": plan.size}))
    }

    pub(super) fn install_packages(self: &Arc<Self>, body: Value) -> Result<Value> {
        let req: PkgReq = de(body)?;
        let (sd, cat, plan) = self.plan(&req)?;
        if plan.install.is_empty() {
            return Ok(json!({"nothing": true}));
        }
        for l in &plan.licenses {
            if !req.accept.contains(&l.id) {
                return Err(
                    Error::conflict(trf!("é preciso aceitar as licenças", "licenses must be accepted")).with_data(serde_json::to_value(&plan.licenses)?)
                );
            }
        }
        for l in &plan.licenses {
            sd.accept(l)
                .map_err(|e| Error::internal(trf!("não foi possível gravar o aceite da licença: {}", "could not record the license acceptance: {}", e)))?;
        }
        let names: Vec<&str> = plan.install.iter().map(|p| p.name.as_str()).collect();
        let title = if names.len() > 2 { format!("{} +{}", names[..2].join(", "), names.len() - 2) } else { names.join(", ") };
        let mut paths = req.paths.clone();
        for r in &plan.install {
            if !paths.contains(&r.path) {
                paths.push(r.path.clone());
            }
        }
        let (app, asked, force) = (self.clone(), req.paths.clone(), req.force);
        let t = self.tasks.start("install", &title, paths, true, move |h| {
            // Replaneja já com o lock da fila: tarefas anteriores podem ter
            // instalado parte das dependências.
            let sd = app.sdk();
            let p = sd.plan_install(&cat, &asked, config::load().show_preview, force);
            let res = sd.install(&cat, &p, &h.cancel, &mut |pr| h.progress(pr.overall, &pr.phase, &pr.name, pr.done, pr.total), &|s| h.logf(s));
            app.emit("refresh", json!("packages"));
            res
        });
        let _ = sd;
        Ok(json!({"task": t}))
    }

    pub(super) fn uninstall_packages(self: &Arc<Self>, body: Value) -> Result<Value> {
        let req: PkgReq = de(body)?;
        if req.paths.is_empty() {
            return Err(err!(400, "nenhum pacote informado", "no package given"));
        }
        let sd = self.sdk();
        let have = sd.installed_map();
        for p in &req.paths {
            if !have.contains_key(p) {
                return Err(err!(400, "pacote não instalado: {}", "package not installed: {}", p));
            }
        }
        if !req.force {
            let users: Vec<String> = avd::list(&sd).into_iter().filter(|a| req.paths.contains(&a.image_pkg)).map(|a| a.display_name).collect();
            if !users.is_empty() {
                return Err(Error::conflict(trf!("há AVDs usando esta imagem", "AVDs are using this image")).with_data(json!(users)));
            }
        }
        let names: Vec<String> = req.paths.iter().map(|p| have[p].name.clone()).collect();
        let (app, paths) = (self.clone(), req.paths.clone());
        let title = names.join(", ");
        let t = self.tasks.start("uninstall", &title, req.paths.clone(), true, move |h| {
            h.progress(0.3, "remove", &names.join(", "), 0, 0);
            app.sdk().uninstall(&paths)?;
            h.logf(trf!("removido: {}", "removed: {}", paths.join(", ")));
            app.emit("refresh", json!("packages"));
            Ok(())
        });
        Ok(json!({"task": t}))
    }
}
