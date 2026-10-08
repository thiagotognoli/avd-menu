//! O que depende do sistema operacional: pastas do app, nomes de host usados
//! pelo repositório do SDK e utilitários pequenos.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_NAME: &str = "avd-menu";

/// Versão do app (CI define AVDMENU_VERSION; senão a do Cargo).
pub fn version() -> &'static str {
    option_env!("AVDMENU_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Nome do SO como o repositório do SDK escreve.
pub fn host_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macosx"
    } else {
        "linux"
    }
}

/// Arquitetura como o repositório do SDK escreve.
pub fn host_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        std::env::consts::ARCH
    }
}

pub fn is_mac() -> bool {
    cfg!(target_os = "macos")
}
pub fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("/"))
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

pub fn config_dir() -> PathBuf {
    if is_mac() {
        home().join("Library/Application Support").join(APP_NAME)
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config")).join(APP_NAME)
    }
}

pub fn cache_dir() -> PathBuf {
    if is_mac() {
        home().join("Library/Caches").join(APP_NAME)
    } else {
        env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache")).join(APP_NAME)
    }
}

pub fn state_dir() -> PathBuf {
    if is_mac() {
        home().join("Library/Application Support").join(APP_NAME)
    } else {
        env_dir("XDG_STATE_HOME").unwrap_or_else(|| home().join(".local/state")).join(APP_NAME)
    }
}

/// XDG_DATA_HOME (atalhos .desktop e ícones).
pub fn data_home() -> PathBuf {
    env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))
}

/// Caminho que o usuário iniciou. Dentro de um AppImage o executável real fica
/// num ponto de montagem temporário; o caminho útil é o do próprio .AppImage.
pub fn exe() -> PathBuf {
    if let Some(p) = env_dir("APPIMAGE") {
        if p.exists() {
            return p;
        }
    }
    let p = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(std::env::args().next().unwrap_or_default()));
    std::fs::canonicalize(&p).unwrap_or(p)
}

const APPIMAGE_VARS: [&str; 5] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "APPIMAGE_GTK_THEME"];

/// Prepara um comando filho: desfaz o que o AppImage muda no ambiente, para que
/// o emulador/navegador não herde o contexto do pacote.
pub fn clean_env(cmd: &mut Command) -> &mut Command {
    let vars: Vec<(String, String)> = std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
    for (k, v) in appimage_env_fixes(&vars, passwd_home().as_deref()) {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    cmd
}

/// O que mudar no ambiente de um filho (None = remover). Além das variáveis do
/// próprio AppImage, o AppRun põe caminhos de dentro do pacote (montado em
/// /tmp/.mount_*) em PATH, XDG_DATA_DIRS, QT_PLUGIN_PATH, GTK_*, PYTHONHOME...
/// e, se existir “<arquivo>.AppImage.config”, troca o XDG_CONFIG_HOME — com
/// isso o emulador gravava as configurações dele lá dentro, diferentes das de
/// quando é aberto pelo atalho.
pub fn appimage_env_fixes(vars: &[(String, String)], real_home: Option<&str>) -> Vec<(String, Option<String>)> {
    let get = |name: &str| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty());
    let mut out: Vec<(String, Option<String>)> = APPIMAGE_VARS.iter().filter(|k| get(k).is_some()).map(|k| (k.to_string(), None)).collect();
    let Some(appdir) = get("APPDIR").map(|d| d.trim_end_matches('/')).filter(|d| d.len() > 1) else { return out };
    let appimage = get("APPIMAGE").unwrap_or("");
    for (k, v) in vars {
        if APPIMAGE_VARS.contains(&k.as_str()) {
            continue;
        }
        if !appimage.is_empty() && k == "XDG_CONFIG_HOME" && v.trim_end_matches('/') == format!("{appimage}.config") {
            out.push((k.clone(), None));
        } else if !appimage.is_empty() && k == "HOME" && v.trim_end_matches('/') == format!("{appimage}.home") {
            if let Some(h) = real_home {
                out.push((k.clone(), Some(h.to_string())));
            }
        } else if k == "GTK_THEME" && get("APPIMAGE_GTK_THEME") == Some(v.as_str()) {
            out.push((k.clone(), None));
        } else if v.contains(appdir) {
            let kept: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !p.starts_with(appdir)).collect();
            out.push((k.clone(), if kept.is_empty() { None } else { Some(kept.join(":")) }));
        }
    }
    out
}

/// Pasta pessoal segundo o /etc/passwd (o HOME pode ter sido trocado).
fn passwd_home() -> Option<String> {
    let mut buf = vec![0 as libc::c_char; 16384];
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut res: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe { libc::getpwuid_r(libc::getuid(), &mut pw, buf.as_mut_ptr(), buf.len(), &mut res) };
    if rc != 0 || res.is_null() || pw.pw_dir.is_null() {
        return None;
    }
    let dir = unsafe { std::ffi::CStr::from_ptr(pw.pw_dir) }.to_string_lossy().into_owned();
    Some(dir).filter(|d| !d.is_empty())
}

/// Procura um executável no PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return if is_executable(&p) { Some(p) } else { None };
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| is_executable(p))
}

/// Identificador seguro para nomes de arquivo/ID de atalho (igual ao do avd-menu.sh).
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

pub fn file_exists(p: &Path) -> bool {
    p.metadata().map(|m| m.is_file()).unwrap_or(false)
}
pub fn dir_exists(p: &Path) -> bool {
    p.metadata().map(|m| m.is_dir()).unwrap_or(false)
}
pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Resolve links simbólicos quando possível.
pub fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_env_is_undone_for_children() {
        let m = "/tmp/.mount_AVD MeIHNMHl";
        let v = |k: &str, v: &str| (k.to_string(), v.to_string());
        let vars = vec![
            v("APPDIR", m),
            v("APPIMAGE", "/home/u/Apps/AVD Menu"),
            v("OWD", "/home/u"),
            v("APPIMAGE_GTK_THEME", "Adwaita:dark"),
            v("GTK_THEME", "Adwaita:dark"),
            v("PATH", &format!("{m}/usr/bin/:{m}/usr/sbin/:/home/u/bin:/usr/bin")),
            v("XDG_DATA_DIRS", &format!("{m}/usr/share/:/usr/share:/usr/local/share/")),
            v("PYTHONHOME", &format!("{m}/usr/")),
            v("GTK_EXE_PREFIX", &format!("{m}//usr")),
            v("QT_PLUGIN_PATH", &format!("{m}/usr/lib/qt5/plugins/:{m}/usr/lib64/qt5/plugins/:")),
            v("XDG_CONFIG_HOME", "/home/u/Apps/AVD Menu.config"),
            v("HOME", "/home/u/Apps/AVD Menu.home"),
            v("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            v("LANG", "pt_BR.UTF-8"),
        ];
        let fixes: std::collections::BTreeMap<String, Option<String>> = appimage_env_fixes(&vars, Some("/home/u")).into_iter().collect();
        let f = |k: &str| fixes.get(k).cloned();
        for gone in ["APPDIR", "APPIMAGE", "OWD", "APPIMAGE_GTK_THEME", "GTK_THEME", "PYTHONHOME", "GTK_EXE_PREFIX", "QT_PLUGIN_PATH", "XDG_CONFIG_HOME"] {
            assert_eq!(f(gone), Some(None), "{gone}");
        }
        assert_eq!(f("PATH"), Some(Some("/home/u/bin:/usr/bin".into())));
        assert_eq!(f("XDG_DATA_DIRS"), Some(Some("/usr/share:/usr/local/share/".into())));
        assert_eq!(f("HOME"), Some(Some("/home/u".into())));
        assert_eq!(f("DBUS_SESSION_BUS_ADDRESS"), None);
        assert_eq!(f("LANG"), None);
    }

    #[test]
    fn outside_an_appimage_nothing_changes() {
        let vars = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("XDG_CONFIG_HOME".to_string(), "/home/u/.config".to_string()),
            ("GTK_THEME".to_string(), "Adwaita:dark".to_string()),
        ];
        assert!(appimage_env_fixes(&vars, Some("/home/u")).is_empty());
    }
}
