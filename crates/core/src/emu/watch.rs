//! Acompanhamento de um emulador em execução: o que o AVD Menu faz enquanto
//! ele está aberto (o app faz isso pelo poller; o atalho, pelo `avd-menu launch`).

use super::{guest, running, xwin};
use crate::config;
use crate::sdk::Sdk;
use std::time::Duration;

/// Uma rodada: tira o _NET_WM_PING das janelas (ver `xwin`) e, na primeira vez
/// em que o serial aparece, aplica os ajustes do Android depois do boot.
/// `applied` guarda as execuções (serial + PIDs) já tratadas.
pub fn tick(sdk: &Sdk, name: &str, pids: &[i32], serial: &str, applied: &mut std::collections::BTreeSet<String>) {
    if !config::load().emulator_gnome_ping {
        let _ = xwin::disable_ping(pids);
    }
    if !serial.is_empty() && applied.insert(format!("{name}@{serial}@{pids:?}")) {
        let (sd, name, serial) = (sdk.clone(), name.to_string(), serial.to_string());
        std::thread::spawn(move || guest::apply_when_booted(&sd, &name, &serial));
    }
}

/// Acompanha o emulador do AVD até ele fechar.
pub fn until_closed(sdk: &Sdk, name: &str) {
    let mut applied = Default::default();
    while let Some(inst) = running().remove(name) {
        tick(sdk, name, &inst.pids, &inst.serial, &mut applied);
        std::thread::sleep(Duration::from_secs(1));
    }
}
