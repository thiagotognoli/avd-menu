use super::{apps_dir, desktop_id, gnome, icons_dir, CreateResult, IconSource, Options};
use crate::emu::{self, output_with_timeout};
use crate::sdk::Sdk;
use crate::{platform, Result};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Linhas `Chave[locale]=texto` do .desktop para todos os idiomas traduzidos (o texto sem
/// sufixo, em inglês, é o reserva). `arg` preenche o `{}` do modelo, se houver.
pub(crate) fn localized(key: &str, pt: &str, en: &str, arg: Option<&str>) -> Vec<String> {
    let mut out = vec![];
    for code in crate::lang::LANGS {
        if code == "en" {
            continue;
        }
        let text = crate::lang::tr_in(code, pt, en);
        if code != "pt" && text == en {
            continue; // sem tradução: o reserva em inglês já cobre
        }
        let text = match arg {
            Some(a) => crate::lang::format_dyn(&text, &[&a]),
            None => text,
        };
        // pt_BR e zh_CN são o que os ambientes costumam pedir; o código curto é o reserva.
        let locales: &[&str] = match code {
            "pt" => &["pt", "pt_BR"],
            "zh" => &["zh_CN", "zh"],
            c => &[c],
        };
        out.extend(locales.iter().map(|l| format!("{key}[{l}]={text}")));
    }
    out
}

/// Regras de aspas do campo Exec do .desktop.
pub(crate) fn exec_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".into();
    }
    let needs = arg.contains([' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')', '`']);
    let arg = arg.replace('%', "%%");
    if !needs {
        return arg;
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '\\' | '"' | '`' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

pub(crate) fn exec_line(envs: &[String], argv: &[&str]) -> String {
    let mut parts = vec!["env".to_string()];
    parts.extend(envs.iter().map(|e| exec_quote(e)));
    parts.extend(argv.iter().map(|a| exec_quote(a)));
    parts.join(" ")
}

pub(crate) fn install_icon(src: &IconSource, name: &str) -> Result<String> {
    let (dir, file, other) = if src.ext == "svg" {
        (icons_dir().join("scalable/apps"), format!("{name}.svg"), icons_dir().join("256x256/apps").join(format!("{name}.png")))
    } else {
        (icons_dir().join("256x256/apps"), format!("{name}.png"), icons_dir().join("scalable/apps").join(format!("{name}.svg")))
    };
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&file), &src.data)?;
    // Não deixa um ícone antigo do outro formato sobrescrevendo este.
    let _ = std::fs::remove_file(other);
    Ok(name.to_string())
}

pub(crate) fn create(sdk: &Sdk, opt: &Options, name: &str, icon: &IconSource) -> Result<CreateResult> {
    let id = desktop_id(&opt.avd);
    let icon_name = install_icon(icon, &id)?;
    let (b, gpu_action) = render(sdk, &opt.avd, name, &icon_name);
    std::fs::create_dir_all(apps_dir())?;
    let path = apps_dir().join(format!("{id}.desktop"));
    std::fs::write(&path, b)?;
    validate(&path);
    refresh_caches();
    let mut res = CreateResult { path: path.to_string_lossy().into_owned(), display_name: name.to_string(), icon: icon_name, pinned: false, gpu_action };
    if opt.pin {
        res.pinned = gnome::pin(&format!("{id}.desktop"));
    }
    Ok(res)
}

/// Regrava o .desktop que já existe (mesmo nome e ícone) se o conteúdo mudou —
/// opções de inicialização do AVD, placas de vídeo, ações novas. Devolve se regravou.
pub(crate) fn refresh(sdk: &Sdk, avd_name: &str) -> Result<bool> {
    let path = apps_dir().join(format!("{}.desktop", desktop_id(avd_name)));
    let Ok(old) = std::fs::read_to_string(&path) else { return Ok(false) };
    let (name, icon) = (desktop_value(&path, "Name"), desktop_value(&path, "Icon"));
    if name.is_empty() || icon.is_empty() {
        return Ok(false);
    }
    let (b, _) = render(sdk, avd_name, &name, &icon);
    if b == old {
        return Ok(false);
    }
    std::fs::write(&path, b)?;
    validate(&path);
    if let Some(p) = platform::which("update-desktop-database") {
        let _ = output_with_timeout(Command::new(p).arg(apps_dir()), Duration::from_secs(10));
    }
    Ok(true)
}

/// Conteúdo do .desktop do AVD (e se ele tem a ação da placa dedicada).
fn render(sdk: &Sdk, avd_name: &str, name: &str, icon_name: &str) -> (String, bool) {
    let opt = Options { avd: avd_name.to_string(), ..Default::default() };
    let emulator = sdk.emulator().to_string_lossy().into_owned();
    // O GNOME vincula a janela ao atalho comparando o WM_CLASS com
    // StartupWMClass. O emulador (Qt/xcb) usa o mesmo WM_CLASS para todos os AVDs;
    // RESOURCE_NAME troca a parte “instância”, dando um nome por AVD.
    let mut envs = vec![format!("RESOURCE_NAME={}", emu::wm_class(&opt.avd))];
    if let Ok(h) = std::env::var("ANDROID_AVD_HOME") {
        if !h.is_empty() {
            envs.push(format!("ANDROID_AVD_HOME={h}"));
        }
    }
    let dgpu = emu::dedicated_gpu_env();
    // OpenGL e Vulkan na mesma placa também no início normal (ver emu::gpu_env).
    envs.extend(emu::gpu_env(false).iter().map(|(k, v)| format!("{k}={v}")));
    // Modo gráfico “automático” resolvido (ver emu::prefer_host_gpu).
    let mut tail: Vec<&str> =
        if emu::prefer_host_gpu() && crate::avd::config(&opt.avd).map(|(c, _)| matches!(c.get("hw.gpu.mode"), "" | "auto")).unwrap_or(true) {
            vec!["-gpu", "host"]
        } else {
            vec![]
        };
    // Inicialização rápida desligada: nem carrega nem grava o snapshot (ver emu::build_args).
    if emu::quickboot_off(&opt.avd) {
        tail.push("-no-snapshot");
    }
    let argv = |extra: &[&str]| -> Vec<String> {
        let mut v = vec![emulator.clone(), "-avd".to_string(), opt.avd.clone()];
        v.extend(extra.iter().map(|s| s.to_string()));
        v.extend(tail.iter().map(|s| s.to_string()));
        v
    };
    let line = |envs: &[String], extra: &[&str]| -> String {
        let a = argv(extra);
        exec_line(envs, &a.iter().map(String::as_str).collect::<Vec<_>>())
    };

    let mut actions = vec!["cold-boot", "wipe-data"];
    if !dgpu.is_empty() {
        // o snapshot guarda o estado gráfico da placa em que foi salvo: ao trocar
        // de placa é preciso o cold boot, então ele existe para as duas
        actions = vec!["gpu-dedicada", "cold-boot", "cold-boot-dedicada", "wipe-data"];
    }
    let wm = emu::wm_class(&opt.avd);
    let mut b = String::new();
    let mut w = |s: String| {
        b.push_str(&s);
        b.push('\n');
    };
    w("[Desktop Entry]".into());
    w("Version=1.0".into());
    w("Type=Application".into());
    w(format!("Name={name}"));
    w("GenericName=Android Emulator".into());
    w(format!("Comment=Starts the Android emulator {}", opt.avd));
    for l in localized("Comment", "Inicia o emulador Android {}", "Starts the Android emulator {}", Some(&opt.avd)) {
        w(l);
    }
    w(format!("Exec={}", line(&envs, &[])));
    w(format!("Icon={icon_name}"));
    w("Terminal=false".into());
    w("Categories=Development;IDE;".into());
    w(format!("Keywords=android;emulator;avd;{};", opt.avd));
    w("StartupNotify=true".into());
    w(format!("StartupWMClass={wm}"));
    w(format!("X-AVDMenu-AVD={}", opt.avd));
    w(format!("Actions={};", actions.join(";")));
    w(String::new());
    w("[Desktop Action cold-boot]".into());
    w("Name=Cold boot".into());
    for l in localized("Name", "Iniciar com cold boot", "Cold boot", None) {
        w(l);
    }
    w(format!("Exec={}", line(&envs, &["-no-snapshot-load"])));
    w(String::new());
    w("[Desktop Action wipe-data]".into());
    w("Name=Wipe data and start".into());
    for l in localized("Name", "Iniciar apagando os dados", "Wipe data and start", None) {
        w(l);
    }
    w(format!("Exec={}", line(&envs, &["-wipe-data"])));
    if !dgpu.is_empty() {
        // O GNOME oferece “Iniciar usando placa de vídeo dedicada” sozinho, mas só
        // enquanto considera o app parado — com o atalho vinculado à janela o item
        // some quando o emulador sobe. Esta ação fica sempre disponível.
        let mut denvs: Vec<String> = envs.iter().filter(|e| e.starts_with("RESOURCE_NAME=") || e.starts_with("ANDROID_AVD_HOME=")).cloned().collect();
        denvs.extend(dgpu.iter().map(|(k, v)| format!("{k}={v}")));
        w(String::new());
        w("[Desktop Action gpu-dedicada]".into());
        w("Name=Start with dedicated GPU".into());
        for l in localized("Name", "Iniciar com placa de vídeo dedicada", "Start with dedicated GPU", None) {
            w(l);
        }
        w(format!("Exec={}", line(&denvs, &[])));
        w(String::new());
        w("[Desktop Action cold-boot-dedicada]".into());
        w("Name=Cold boot with dedicated GPU".into());
        for l in localized("Name", "Iniciar com cold boot na placa de vídeo dedicada", "Cold boot with dedicated GPU", None) {
            w(l);
        }
        w(format!("Exec={}", line(&denvs, &["-no-snapshot-load"])));
    }
    (b, !dgpu.is_empty())
}

/// Roda o desktop-file-validate, se existir (só para avisar no stderr).
pub(crate) fn validate(path: &Path) {
    if let Some(p) = platform::which("desktop-file-validate") {
        let _ = output_with_timeout(Command::new(p).arg(path), Duration::from_secs(5));
    }
}

pub(crate) fn refresh_caches() {
    if let Some(p) = platform::which("update-desktop-database") {
        let _ = output_with_timeout(Command::new(p).arg(apps_dir()), Duration::from_secs(10));
    }
    if let Some(p) = platform::which("gtk-update-icon-cache") {
        let _ = output_with_timeout(Command::new(p).args(["-f", "-t"]).arg(icons_dir()), Duration::from_secs(10));
    }
}

/// Lê uma chave da seção [Desktop Entry] de um .desktop.
pub(crate) fn desktop_value(path: &Path, key: &str) -> String {
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    let mut in_main = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if in_main {
            if let Some(v) = line.strip_prefix(&format!("{key}=")) {
                return v.to_string();
            }
        }
    }
    String::new()
}
