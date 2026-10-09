//! Preferências persistidas do aplicativo (~/.config/avd-menu/config.json).

use crate::platform;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutMeta {
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Android SDK escolhido ("" = detectar).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sdk_root: String,
    /// Inclui canais beta/dev/canary na lista de pacotes.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub show_preview: bool,
    /// "pt", "en" ou "" (automático).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lang: String,
    /// "light", "dark" ou "".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub theme: String,
    /// Tema das janelas do emulador: "" (segue o sistema), "light", "dark" ou
    /// "keep" (não mexe no que foi escolhido nas configurações do emulador).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub emulator_theme: String,
    /// Deixar o GNOME verificar se o emulador responde (_NET_WM_PING). Por
    /// padrão o AVD Menu desliga essa verificação só nas janelas do emulador
    /// (ver `emu::xwin`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub emulator_gnome_ping: bool,
    /// Argumentos extras do emulador, por AVD.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub avd_extra_args: BTreeMap<String, String>,
    /// Metadados dos atalhos criados.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub shortcuts: BTreeMap<String, ShortcutMeta>,
    /// Perfis de hardware do usuário (JSON livre; ver devices).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub user_devices: Vec<Value>,
}

static LOCK: Mutex<()> = Mutex::new(());

fn path() -> std::path::PathBuf {
    platform::config_dir().join("config.json")
}

fn load_unlocked() -> Config {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Lê as preferências (padrão se não existirem).
pub fn load() -> Config {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked()
}

/// Aplica `f` sobre as preferências atuais e grava o resultado.
pub fn update(f: impl FnOnce(&mut Config)) -> std::io::Result<Config> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut c = load_unlocked();
    f(&mut c);
    std::fs::create_dir_all(platform::config_dir())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&c).unwrap())?;
    std::fs::rename(&tmp, path())?;
    Ok(c)
}
