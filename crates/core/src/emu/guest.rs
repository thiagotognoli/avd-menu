//! Ajustes feitos dentro do Android depois que ele termina de iniciar (pelo
//! adb), para opções que o emulador não tem.

use super::output_with_timeout;
use crate::sdk::Sdk;
use crate::{avd, platform};
use std::process::Command;
use std::time::{Duration, Instant};

/// O que aplicar no Android do AVD: (ajuste global, valor).
pub fn wanted(name: &str) -> Vec<(&'static str, &'static str)> {
    let Ok((cfg, _)) = avd::config(name) else { return vec![] };
    match cfg.get(avd::BLUR_KEY) {
        "yes" => vec![("disable_window_blurs", "1")],
        "no" => vec![("disable_window_blurs", "0")],
        _ => vec![],
    }
}

/// Espera o Android do emulador `serial` terminar de iniciar e aplica os ajustes
/// do AVD. Bloqueia (até ~6 min): chame numa thread. Devolve se aplicou.
pub fn apply_when_booted(sdk: &Sdk, name: &str, serial: &str) -> bool {
    let todo = wanted(name);
    if todo.is_empty() || serial.is_empty() || !sdk.has_adb() {
        return false;
    }
    let adb = |args: &[&str]| {
        let mut cmd = Command::new(sdk.adb());
        platform::clean_env(cmd.arg("-s").arg(serial).args(args));
        output_with_timeout(&mut cmd, Duration::from_secs(15)).filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let t0 = Instant::now();
    loop {
        if adb(&["shell", "getprop", "sys.boot_completed"]).as_deref() == Some("1") {
            break;
        }
        if t0.elapsed() > Duration::from_secs(360) || !super::running().get(name).is_some_and(|i| i.serial == serial) {
            return false;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut ok = true;
    for (key, value) in todo {
        ok &= adb(&["shell", "settings", "put", "global", key, value]).is_some();
    }
    ok
}
