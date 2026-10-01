//! Linha de comando (mesmo binário da interface): `avd-menu list`, `start`, `sdk install`...

use crate::lang::tr;
use crate::sdk::{self, human_bytes, Sdk};
use crate::{avd, config, emu, platform, shortcut, Cancel};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};

fn usage() -> String {
    let v = platform::version();
    if crate::lang::is_pt() {
        format!(
            r#"AVD Menu {v} — gerenciador de emuladores Android

USO:
  avd-menu [ui]                      Abre a interface gráfica (padrão)
  avd-menu list [--json]             Lista os AVDs
  avd-menu create NOME --device ID --image PACOTE [--ram MB] [--cores N]
                                     Cria um AVD (a imagem precisa estar instalada)
  avd-menu start NOME [opções]       Inicia um emulador
        --cold  cold boot   --wipe  apaga os dados   --dgpu  GPU dedicada   --headless  sem janela
        --arg X  argumento extra do emulador (repetível)
  avd-menu stop NOME                 Para um emulador
  avd-menu shortcut NOME [opções]    Cria o atalho no menu de aplicativos
        --name TEXTO  nome no menu   --icon ARQUIVO  ícone (png/svg)   --pin  fixa na dock (GNOME)
  avd-menu unshortcut NOME           Remove o atalho
  avd-menu install                   Adiciona o AVD Menu ao menu de aplicativos (Linux)
  avd-menu uninstall                 Remove o AVD Menu do menu de aplicativos
  avd-menu sdk list [--all]          Lista pacotes instalados (ou todos os disponíveis)
  avd-menu sdk install PACOTE...     Instala pacotes (ex.: "system-images;android-36;google_apis;x86_64")
  avd-menu sdk uninstall PACOTE...   Remove pacotes
  avd-menu doctor                    Verifica o ambiente (KVM, bibliotecas, espaço)
  avd-menu version                   Mostra a versão

OPÇÕES GLOBAIS:
  --sdk CAMINHO    Usa este Android SDK nesta execução (padrão: detecta ou ~/Android/Sdk)

Sem argumentos o programa abre a interface numa janela própria.
"#
        )
    } else {
        format!(
            r#"AVD Menu {v} — Android emulator manager

USAGE:
  avd-menu [ui]                      Open the graphical interface (default)
  avd-menu list [--json]             List AVDs
  avd-menu create NAME --device ID --image PACKAGE [--ram MB] [--cores N]
                                     Create an AVD (the image must be installed)
  avd-menu start NAME [options]      Start an emulator
        --cold  cold boot   --wipe  wipe data   --dgpu  dedicated GPU   --headless  no window
        --arg X  extra emulator argument (repeatable)
  avd-menu stop NAME                 Stop an emulator
  avd-menu shortcut NAME [options]   Create the app-menu shortcut
        --name TEXT  menu label   --icon FILE  icon (png/svg)   --pin  pin to dock (GNOME)
  avd-menu unshortcut NAME           Remove the shortcut
  avd-menu install                   Add AVD Menu to the application menu (Linux)
  avd-menu uninstall                 Remove AVD Menu from the application menu
  avd-menu sdk list [--all]          List installed packages (or everything available)
  avd-menu sdk install PACKAGE...    Install packages (e.g. "system-images;android-36;google_apis;x86_64")
  avd-menu sdk uninstall PACKAGE...  Remove packages
  avd-menu doctor                    Check the environment (KVM, libraries, disk space)
  avd-menu version                   Print the version

GLOBAL OPTIONS:
  --sdk PATH       Use this Android SDK for this run (default: auto-detect or ~/Android/Sdk)

Without arguments the program opens the interface in its own window.
"#
        )
    }
}

fn fail(msg: impl AsRef<str>) -> i32 {
    eprintln!("{}{}", tr("Erro: ", "Error: "), msg.as_ref());
    1
}

struct Parsed {
    pos: Vec<String>,
    vals: BTreeMap<String, Vec<String>>,
    bools: BTreeSet<String>,
}

impl Parsed {
    fn val(&self, k: &str) -> Option<&str> {
        self.vals.get(k).and_then(|v| v.last()).map(String::as_str)
    }
    fn has(&self, k: &str) -> bool {
        self.bools.contains(k)
    }
}

/// Separa posicionais de opções. `value_flags` recebem um valor; `bool_flags` não.
fn parse(args: &[String], value_flags: &[&str], bool_flags: &[&str]) -> std::result::Result<Parsed, String> {
    let mut p = Parsed { pos: vec![], vals: BTreeMap::new(), bools: BTreeSet::new() };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(rest) = a.strip_prefix("--").or_else(|| a.strip_prefix('-').filter(|r| !r.is_empty())) {
            let (name, inline) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            };
            if value_flags.contains(&name) {
                let v = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        args.get(i).cloned().ok_or_else(|| format!("{a} {}", tr("exige um valor", "needs a value")))?
                    }
                };
                p.vals.entry(name.to_string()).or_default().push(v);
            } else if bool_flags.contains(&name) {
                p.bools.insert(name.to_string());
            } else {
                return Err(format!("{} {a}", tr("opção desconhecida:", "unknown option:")));
            }
        } else {
            p.pos.push(a.clone());
        }
        i += 1;
    }
    Ok(p)
}

fn current_sdk() -> Sdk {
    sdk::detect(&config::load().sdk_root)
}

/// Executa a linha de comando. `None` = abrir a interface gráfica.
pub fn run(args: &[String]) -> Option<i32> {
    // --sdk pode aparecer em qualquer posição
    let mut rest: Vec<String> = vec![];
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--sdk" && i + 1 < args.len() {
            sdk::set_override(&args[i + 1]);
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    let mut args = rest;
    // compatibilidade com o avd-menu.sh antigo
    if let Some(first) = args.first().cloned() {
        let mapped = match first.as_str() {
            "--ui" | "-u" => Some("ui"),
            "--list" | "-l" => Some("list"),
            "--install" => Some("install"),
            "--uninstall" => Some("uninstall"),
            "--remove" | "-r" => Some("unshortcut"),
            _ => None,
        };
        if let Some(m) = mapped {
            args[0] = m.to_string();
        }
    }
    let cmd = args.first().cloned()?;
    let rest = &args[1..];
    let code = match cmd.as_str() {
        "ui" | "gui" => return None,
        "list" | "ls" => cmd_list(rest),
        "start" | "run" => cmd_start(rest),
        "stop" => cmd_stop(rest),
        "create" => cmd_create(rest),
        "shortcut" => cmd_shortcut(rest),
        "unshortcut" => cmd_unshortcut(rest),
        "install" => match shortcut::install_self() {
            Ok(r) => {
                println!(
                    "{}\n  {} {}",
                    tr("\"AVD Menu\" adicionado ao menu de aplicativos.", "\"AVD Menu\" added to the application menu."),
                    tr("Arquivo:", "File:"),
                    r.path
                );
                0
            }
            Err(e) => fail(e.message),
        },
        "uninstall" => match shortcut::uninstall_self() {
            Ok(()) => {
                println!("{}", tr("Atalho removido.", "Shortcut removed."));
                0
            }
            Err(e) => fail(e.message),
        },
        "sdk" => cmd_sdk(rest),
        "doctor" => cmd_doctor(),
        "version" | "--version" | "-v" => {
            println!("avd-menu {}", platform::version());
            0
        }
        "help" | "--help" | "-h" => {
            print!("{}", usage());
            0
        }
        other if !other.starts_with('-') => cmd_shortcut(&args), // "avd-menu NOME" (como o script antigo)
        _ => {
            eprint!("{}", usage());
            2
        }
    };
    Some(code)
}

fn cmd_list(args: &[String]) -> i32 {
    let p = match parse(args, &[], &["json"]) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let sd = current_sdk();
    let list = avd::list(&sd);
    let running = emu::running();
    if p.has("json") {
        let items: Vec<_> = list
            .iter()
            .map(|a| serde_json::json!({"name": a.name, "displayName": a.display_name, "api": a.api, "abi": a.abi, "device": a.device_name, "running": running.contains_key(&a.name)}))
            .collect();
        println!("{}", serde_json::Value::Array(items));
        return 0;
    }
    if list.is_empty() {
        println!("{}", tr("Nenhum AVD encontrado. Abra a interface (avd-menu) para criar um.", "No AVDs found. Open the interface (avd-menu) to create one."));
        return 0;
    }
    let w = list.iter().map(|a| a.name.len()).max().unwrap_or(4).max(4);
    let wd = list.iter().map(|a| a.device_name.chars().count()).max().unwrap_or(6).max(6);
    println!("{:<w$}  {:<wd$}  {:<4}  {:<10}  {}", tr("NOME", "NAME"), tr("DISPOSITIVO", "DEVICE"), "API", "ABI", tr("ESTADO", "STATE"));
    for a in &list {
        let mut state = if running.contains_key(&a.name) { tr("rodando", "running") } else { String::new() };
        if !a.problem.is_empty() {
            state = format!("⚠ {}", a.problem);
        }
        println!("{:<w$}  {:<wd$}  {:<4}  {:<10}  {}", a.name, a.device_name, a.api, a.abi, state);
    }
    0
}

fn cmd_start(args: &[String]) -> i32 {
    let p = match parse(args, &["arg"], &["cold", "wipe", "dgpu", "headless"]) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let Some(name) = p.pos.first() else { return fail(tr("informe o nome do AVD", "give the AVD name")) };
    let mut opt = emu::StartOptions { headless: p.has("headless"), extra_args: p.vals.get("arg").cloned().unwrap_or_default(), ..Default::default() };
    if p.has("wipe") {
        opt.mode = "wipe".into();
    } else if p.has("cold") {
        opt.mode = "cold".into();
    }
    if p.has("dgpu") {
        opt.gpu = "dedicated".into();
    }
    if let Some(extra) = config::load().avd_extra_args.get(name) {
        let mut a = crate::api::split_args(extra);
        a.append(&mut opt.extra_args);
        opt.extra_args = a;
    }
    match emu::start(&current_sdk(), name, &opt) {
        Ok(pid) => {
            println!("{} {name} ({}) PID {pid}. Log: {}", tr("Emulador", "Emulator"), tr("iniciado", "started"), emu::log_path(name).display());
            0
        }
        Err(e) => fail(e.message),
    }
}

fn cmd_stop(args: &[String]) -> i32 {
    let Some(name) = args.first() else { return fail(tr("informe o nome do AVD", "give the AVD name")) };
    match emu::stop(&current_sdk(), name) {
        Ok(()) => {
            println!("{}", tr("Emulador parado.", "Emulator stopped."));
            0
        }
        Err(e) => fail(e.message),
    }
}

fn cmd_create(args: &[String]) -> i32 {
    let p = match parse(args, &["device", "image", "display-name", "ram", "cores"], &[]) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let (Some(name), Some(image)) = (p.pos.first(), p.val("image")) else {
        return fail(tr("uso: avd-menu create NOME --device ID --image PACOTE", "usage: avd-menu create NAME --device ID --image PACKAGE"));
    };
    let mut spec =
        avd::CreateSpec { name: name.clone(), device_id: p.val("device").unwrap_or("pixel_9").to_string(), image_pkg: image.to_string(), ..Default::default() };
    spec.settings.display_name = p.val("display-name").map(str::to_string);
    spec.settings.ram_mb = p.val("ram").and_then(|v| v.parse().ok());
    spec.settings.cores = p.val("cores").and_then(|v| v.parse().ok());
    match avd::create(&current_sdk(), &spec) {
        Ok(a) => {
            println!("{}: {} ({}, API {})\n  {}", tr("AVD criado", "AVD created"), a.name, a.device_name, a.api, a.dir);
            0
        }
        Err(e) => fail(e.message),
    }
}

fn cmd_shortcut(args: &[String]) -> i32 {
    let p = match parse(args, &["name", "n", "icon", "i"], &["pin"]) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let sd = current_sdk();
    let Some(name) = p.pos.first() else {
        let names: Vec<String> = avd::list(&sd).into_iter().map(|a| a.name).collect();
        return fail(trf!("informe o nome do AVD. Disponíveis: {}", "give the AVD name. Available: {}", names.join(", ")));
    };
    let opt = shortcut::Options {
        avd: name.clone(),
        display_name: p.val("name").or(p.val("n")).unwrap_or("").to_string(),
        icon_path: p.val("icon").or(p.val("i")).unwrap_or("").to_string(),
        pin: p.has("pin"),
        ..Default::default()
    };
    match shortcut::create(&sd, &opt) {
        Ok(r) => {
            println!(
                "{}\n  AVD:   {name}\n  {}: {}\n  {}: {}",
                tr("Atalho criado.", "Shortcut created."),
                tr("Nome", "Label"),
                r.display_name,
                tr("Arquivo", "File"),
                r.path
            );
            if r.gpu_action {
                println!(
                    "  GPU:   {}",
                    tr(
                        "ação \"Iniciar com placa de vídeo dedicada\" disponível (clique direito no ícone)",
                        "\"Start with dedicated GPU\" action available (right-click the icon)"
                    )
                );
            }
            0
        }
        Err(e) => fail(e.message),
    }
}

fn cmd_unshortcut(args: &[String]) -> i32 {
    let Some(name) = args.first() else { return fail(tr("informe o nome do AVD", "give the AVD name")) };
    match shortcut::remove(name) {
        Ok(()) => {
            println!("{}", tr("Atalho removido.", "Shortcut removed."));
            0
        }
        Err(e) => fail(e.message),
    }
}

fn cmd_sdk(args: &[String]) -> i32 {
    let Some(sub) = args.first() else {
        eprint!("{}", usage());
        return 2;
    };
    let rest = &args[1..];
    let sd = current_sdk();
    let cfg = config::load();
    match sub.as_str() {
        "list" => {
            let all = rest.iter().any(|a| a == "--all");
            println!("SDK: {}", sd.root.display());
            let inst = sd.installed_map();
            if !all {
                for (k, v) in &inst {
                    println!("  {:<55} {:<12} {}", k, v.rev(), v.name);
                }
                return 0;
            }
            match sdk::manifest::fetch_catalog(false) {
                Ok(cat) => {
                    for r in cat.latest(cfg.show_preview) {
                        println!("{} {:<60} {:<10} {}", if inst.contains_key(&r.path) { "✓" } else { " " }, r.path, r.revision.to_string(), r.name);
                    }
                    0
                }
                Err(e) => fail(e.message),
            }
        }
        "install" => {
            let p = match parse(rest, &[], &["accept-licenses", "y"]) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            if p.pos.is_empty() {
                return fail(tr("informe os pacotes", "give the packages"));
            }
            let cat = match sdk::manifest::fetch_catalog(false) {
                Ok(c) => c,
                Err(e) => return fail(e.message),
            };
            let plan = sd.plan_install(&cat, &p.pos, cfg.show_preview, false);
            if !plan.missing.is_empty() {
                return fail(trf!("pacote indisponível: {}", "package unavailable: {}", plan.missing.join(", ")));
            }
            if plan.install.is_empty() {
                println!("{}", tr("Nada a fazer: já instalado.", "Nothing to do: already installed."));
                return 0;
            }
            println!("{}", tr("Serão instalados:", "Will install:"));
            for r in &plan.install {
                let size = r.archive_for(platform::host_os(), platform::host_arch()).map(|a| a.size).unwrap_or(0);
                println!("  {} ({})", r.name, human_bytes(size));
            }
            let yes = p.has("accept-licenses") || p.has("y");
            for l in &plan.licenses {
                if !yes {
                    println!("\n{}\n", l.text);
                    print!("{}", trf!("Aceita a licença {:?}? [s/N] ", "Accept license {:?}? [y/N] ", l.id));
                    let _ = std::io::stdout().flush();
                    let mut ans = String::new();
                    let _ = std::io::stdin().lock().read_line(&mut ans);
                    if !matches!(ans.trim().to_lowercase().as_str(), "s" | "y" | "sim" | "yes") {
                        return fail(tr("licença não aceita", "license not accepted"));
                    }
                }
                if let Err(e) = sd.accept(l) {
                    return fail(e.to_string());
                }
            }
            let cancel = Cancel::new();
            let mut last = String::new();
            let res = sd.install(
                &cat,
                &plan,
                &cancel,
                &mut |p| {
                    let line = format!("\r[{}/{}] {:<8} {:5.1}%  {}", p.index, p.count, p.phase, p.overall * 100.0, p.name);
                    if line != last {
                        eprint!("{line:<100}");
                        last = line;
                    }
                },
                &|s| eprintln!("\n{s}"),
            );
            eprintln!();
            match res {
                Ok(()) => {
                    println!("{}", tr("Concluído.", "Done."));
                    0
                }
                Err(e) => fail(e.message),
            }
        }
        "uninstall" => {
            if rest.is_empty() {
                return fail(tr("informe os pacotes", "give the packages"));
            }
            match sd.uninstall(rest) {
                Ok(()) => {
                    println!("{}", tr("Removido.", "Removed."));
                    0
                }
                Err(e) => fail(e.message),
            }
        }
        _ => {
            eprint!("{}", usage());
            2
        }
    }
}

fn cmd_doctor() -> i32 {
    let sd = current_sdk();
    println!("SDK: {}\nAVD home: {}\n", sd.root.display(), avd::home().display());
    for c in emu::diagnose(&sd) {
        let icon = match c.level.as_str() {
            "ok" => "✓",
            "warn" => "!",
            "error" => "✗",
            _ => "i",
        };
        let args: Vec<String> = c.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
        print!(" {icon} {}", c.id);
        if !args.is_empty() {
            print!("  ({})", args.join(", "));
        }
        println!();
        if !c.fix.is_empty() {
            println!("     → {}", c.fix);
        }
    }
    0
}
