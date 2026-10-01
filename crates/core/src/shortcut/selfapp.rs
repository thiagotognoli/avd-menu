//! Atalho do próprio AVD Menu no menu de aplicativos (Linux).

use super::linux::{desktop_value, exec_quote, install_icon, refresh_caches, validate};
use super::{apps_dir, icons_dir, CreateResult, IconSource, Info, ICON_SVG};
use crate::{platform, Error, Result};

/// ID do .desktop do app: igual ao identificador do Tauri, que é o app_id da
/// janela no Wayland (assim o GNOME vincula janela e atalho sozinho).
const SELF_ID: &str = "app.avdmenu.AVDMenu";
/// Nome do arquivo usado pelo avd-menu.sh antigo.
const LEGACY_ID: &str = "avd-menu";
/// WM_CLASS (instância) da janela no X11: o nome do executável.
pub const SELF_WM_CLASS: &str = "avd-menu";

/// O AVD Menu está no menu de aplicativos?
pub fn self_info() -> Info {
    if !platform::is_linux() {
        return Info::default();
    }
    let p = apps_dir().join(format!("{SELF_ID}.desktop"));
    if !platform::file_exists(&p) {
        return Info::default();
    }
    Info { exists: true, display_name: desktop_value(&p, "Name"), path: p.to_string_lossy().into_owned() }
}

/// Adiciona o AVD Menu ao menu de aplicativos (Linux). No macOS o usuário arrasta
/// o .app para a pasta Aplicativos.
pub fn install_self() -> Result<CreateResult> {
    if !platform::is_linux() {
        return Err(Error::bad(crate::lang::tr("no macOS, arraste o AVD Menu para a pasta Aplicativos", "on macOS, drag AVD Menu to the Applications folder")));
    }
    let icon = install_icon(&IconSource { data: ICON_SVG.to_vec(), ext: "svg" }, SELF_ID)?;
    let exe = platform::exe();
    let content = format!(
        "[Desktop Entry]\nVersion=1.0\nType=Application\nName=AVD Menu\nGenericName=Android Virtual Devices\n\
Comment=Create, start and manage Android emulators\nComment[pt]=Crie, inicie e gerencie emuladores Android\n\
Comment[pt_BR]=Crie, inicie e gerencie emuladores Android\nExec={} ui\nIcon={icon}\nTerminal=false\n\
Categories=Development;IDE;\nKeywords=android;emulator;avd;sdk;device manager;emulador;\nStartupNotify=false\n\
StartupWMClass={SELF_WM_CLASS}\n",
        exec_quote(&exe.to_string_lossy())
    );
    std::fs::create_dir_all(apps_dir())?;
    remove_legacy();
    let p = apps_dir().join(format!("{SELF_ID}.desktop"));
    std::fs::write(&p, content)?;
    validate(&p);
    refresh_caches();
    Ok(CreateResult { path: p.to_string_lossy().into_owned(), display_name: "AVD Menu".into(), icon, pinned: false, gpu_action: false })
}

/// O avd-menu.sh antigo instalava `avd-menu.desktop` apontando para o script; ao
/// instalar a versão nova esse arquivo é removido para não ficar um item duplicado.
fn remove_legacy() {
    let p = apps_dir().join(format!("{LEGACY_ID}.desktop"));
    if desktop_value(&p, "Exec").contains("avd-menu.sh") {
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(icons_dir().join("scalable/apps").join(format!("{LEGACY_ID}.svg")));
        let _ = std::fs::remove_file(icons_dir().join("256x256/apps").join(format!("{LEGACY_ID}.png")));
    }
}

/// Remove o AVD Menu do menu de aplicativos.
pub fn uninstall_self() -> Result<()> {
    let p = apps_dir().join(format!("{SELF_ID}.desktop"));
    if !platform::file_exists(&p) {
        return Err(Error::bad(crate::lang::tr("o AVD Menu não está instalado no menu", "AVD Menu is not installed in the menu")));
    }
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(icons_dir().join("scalable/apps").join(format!("{SELF_ID}.svg")));
    refresh_caches();
    Ok(())
}
