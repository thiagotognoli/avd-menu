//! Atalhos de emuladores no menu de aplicativos: um .desktop no Linux (GNOME,
//! KDE...) e um pacote .app no macOS.

pub mod gnome;
pub mod icns;
mod linux;
mod macos;
mod selfapp;

pub use selfapp::{install_self, self_info, uninstall_self, SELF_WM_CLASS};

use crate::sdk::Sdk;
use crate::{avd, platform, Error, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// O que o atalho deve ter.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub avd: String,
    /// nome no menu; vazio = “Android Emulator <AVD>”
    pub display_name: String,
    /// Arquivo de ícone (CLI)...
    pub icon_path: String,
    /// ...ou data URL PNG/SVG vindo da interface.
    pub icon_data: String,
    /// fixar na dock do GNOME
    pub pin: bool,
}

/// O que foi criado.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    pub path: String,
    pub display_name: String,
    pub icon: String,
    pub pinned: bool,
    pub gpu_action: bool,
}

/// Um atalho existente.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub exists: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
}

/// Criamos atalhos neste sistema?
pub fn supported() -> bool {
    platform::is_linux() || platform::is_mac()
}

/// Identificador do atalho de um AVD (compatível com o avd-menu.sh).
pub fn desktop_id(avd_name: &str) -> String {
    format!("android-emulator-{}", platform::slugify(avd_name))
}

pub(crate) fn apps_dir() -> PathBuf {
    platform::data_home().join("applications")
}
pub(crate) fn icons_dir() -> PathBuf {
    platform::data_home().join("icons/hicolor")
}

/// Procura o atalho de um AVD.
pub fn lookup(avd_name: &str) -> Info {
    if platform::is_mac() {
        return macos::lookup(avd_name);
    }
    let p = apps_dir().join(format!("{}.desktop", desktop_id(avd_name)));
    if !platform::file_exists(&p) {
        return Info::default();
    }
    Info { exists: true, display_name: linux::desktop_value(&p, "Name"), path: p.to_string_lossy().into_owned() }
}

/// O atalho do AVD está fixo na dock do GNOME?
pub fn pinned(avd_name: &str) -> bool {
    platform::is_linux() && gnome::is_pinned(&format!("{}.desktop", desktop_id(avd_name)))
}

/// Ícone resolvido: bytes + extensão (“svg” ou “png”).
pub(crate) struct IconSource {
    pub data: Vec<u8>,
    pub ext: &'static str,
}

pub(crate) const ICON_SVG: &[u8] = include_bytes!("../../assets/android-icon.svg");
pub(crate) const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-1024.png");

fn resolve_icon(opt: &Options) -> Result<IconSource> {
    if !opt.icon_data.is_empty() {
        let bad = || Error::bad(crate::lang::tr("ícone inválido", "invalid icon"));
        let rest = opt.icon_data.strip_prefix("data:").ok_or_else(bad)?;
        let (_, b64) = rest.split_once(";base64,").ok_or_else(bad)?;
        let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| bad())?;
        if raw.is_empty() || raw.len() > 8 << 20 {
            return Err(Error::bad(crate::lang::tr("ícone inválido ou grande demais", "invalid or too large icon")));
        }
        return classify_icon(raw);
    }
    if !opt.icon_path.is_empty() {
        let raw = std::fs::read(&opt.icon_path).map_err(|e| Error::bad(trf!("não foi possível ler o ícone: {}", "could not read the icon: {}", e)))?;
        return classify_icon(raw);
    }
    Ok(IconSource { data: ICON_SVG.to_vec(), ext: "svg" })
}

fn classify_icon(raw: Vec<u8>) -> Result<IconSource> {
    let head = String::from_utf8_lossy(&raw[..raw.len().min(512)]).trim().to_string();
    if head.contains("<svg") || (head.starts_with("<?xml") && String::from_utf8_lossy(&raw).contains("<svg")) {
        return Ok(IconSource { data: raw, ext: "svg" });
    }
    // PNG direto; outros formatos a interface já converteu para PNG.
    if raw.starts_with(b"\x89PNG") {
        icns::decode_png(&raw)?; // valida
        return Ok(IconSource { data: raw, ext: "png" });
    }
    Err(Error::bad(crate::lang::tr("formato de imagem não suportado (use PNG, JPG, GIF ou SVG)", "unsupported image format (use PNG, JPG, GIF or SVG)")))
}

/// Cria (ou recria) o atalho do AVD.
pub fn create(sdk: &Sdk, opt: &Options) -> Result<CreateResult> {
    if !supported() {
        return Err(Error::bad(crate::lang::tr("atalhos não são suportados neste sistema", "shortcuts are not supported on this system")));
    }
    if !avd::valid_name(&opt.avd) {
        return Err(bad!("nome de AVD inválido: {:?}", "invalid AVD name: {:?}", opt.avd));
    }
    if !sdk.has_emulator() {
        return Err(Error::bad(crate::lang::tr(
            "instale o pacote \"Android Emulator\" antes de criar o atalho",
            "install the \"Android Emulator\" package before creating the shortcut",
        )));
    }
    if !avd::exists(&opt.avd) {
        return Err(err!(404, "o AVD {:?} não existe", "AVD {:?} does not exist", opt.avd));
    }
    let mut name = opt.display_name.trim().to_string();
    if name.is_empty() {
        name = format!("Android Emulator {}", opt.avd);
    }
    if name.contains(['\n', '\r', '\0']) {
        return Err(Error::bad(crate::lang::tr("nome inválido", "invalid name")));
    }
    let icon = resolve_icon(opt)?;
    if platform::is_mac() {
        macos::create(sdk, opt, &name, &icon)
    } else {
        linux::create(sdk, opt, &name, &icon)
    }
}

/// Regrava o atalho que já existe (mesmo nome, ícone e lugar na dock) se ele
/// não reflete mais o AVD e esta máquina: opções de inicialização, placas de
/// vídeo, ações novas de uma versão mais nova. Devolve se regravou.
pub fn refresh(sdk: &Sdk, avd_name: &str) -> Result<bool> {
    if !platform::is_linux() || !avd::valid_name(avd_name) {
        return Ok(false);
    }
    linux::refresh(sdk, avd_name)
}

/// Remove o atalho de um AVD (e o ícone instalado).
pub fn remove(avd_name: &str) -> Result<()> {
    if platform::is_mac() {
        return macos::remove(avd_name);
    }
    let id = desktop_id(avd_name);
    let p = apps_dir().join(format!("{id}.desktop"));
    if !platform::file_exists(&p) {
        return Err(err!(404, "nenhum atalho encontrado para o AVD {:?}", "no shortcut found for AVD {:?}", avd_name));
    }
    gnome::unpin(&format!("{id}.desktop"));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(icons_dir().join("scalable/apps").join(format!("{id}.svg")));
    let _ = std::fs::remove_file(icons_dir().join("256x256/apps").join(format!("{id}.png")));
    linux::refresh_caches();
    Ok(())
}

#[cfg(test)]
mod tests;
