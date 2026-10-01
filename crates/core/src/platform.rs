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

const APPIMAGE_VARS: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];

/// Prepara um comando filho: tira as variáveis que o AppImage injeta, para que
/// o emulador/navegador não herde o contexto do pacote.
pub fn clean_env(cmd: &mut Command) -> &mut Command {
    for v in APPIMAGE_VARS {
        cmd.env_remove(v);
    }
    cmd
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
