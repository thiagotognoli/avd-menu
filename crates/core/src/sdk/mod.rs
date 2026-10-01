//! Faz o que o `sdkmanager` do Google faz — localizar o SDK, ler os manifestos de
//! pacotes, baixar/instalar/remover e gerenciar licenças — sem precisar de Java.

pub mod apinames;
pub(crate) mod http;
pub mod install;
pub mod installed;
pub mod license;
pub mod manifest;

pub use apinames::api_name;
pub use install::{human_bytes, Plan, Progress};
pub use installed::{Installed, SysImageProps};
pub use license::License;
pub use manifest::{Archive, Catalog, Remote, Revision};

use crate::platform;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Uma instalação do Android SDK.
#[derive(Debug, Clone)]
pub struct Sdk {
    pub root: PathBuf,
}

/// Um local possível de SDK.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub path: String,
    pub source: String,
    pub exists: bool,
    pub emulator: bool,
    pub adb: bool,
}

/// Local padrão para criar um SDK novo (Linux e macOS). SDKs que já existem em outros
/// lugares — inclusive o padrão do Android Studio — continuam sendo detectados.
pub fn default_root() -> PathBuf {
    platform::home().join("Applications/AndroidSDK")
}

impl Sdk {
    pub fn new(root: impl Into<PathBuf>) -> Sdk {
        Sdk { root: root.into() }
    }
    pub fn emulator(&self) -> PathBuf {
        self.root.join("emulator/emulator")
    }
    pub fn adb(&self) -> PathBuf {
        self.root.join("platform-tools/adb")
    }
    pub fn has_emulator(&self) -> bool {
        platform::is_executable(&self.emulator())
    }
    pub fn has_adb(&self) -> bool {
        platform::is_executable(&self.adb())
    }
    pub fn exists(&self) -> bool {
        !self.root.as_os_str().is_empty() && platform::dir_exists(&self.root)
    }
    /// Converte um path de pacote ("system-images;android-36;x;y") na pasta de instalação.
    pub fn package_dir(&self, pkg_path: &str) -> PathBuf {
        let mut p = self.root.clone();
        for part in pkg_path.split(';') {
            p.push(part);
        }
        p
    }
}

/// Lista os locais onde um SDK pode estar, na ordem de preferência.
pub fn candidates(configured: &str) -> Vec<Candidate> {
    let home = platform::home();
    let mut list: Vec<(PathBuf, &str)> = vec![];
    let mut push = |p: PathBuf, src: &'static str| {
        if !p.as_os_str().is_empty() {
            list.push((p, src));
        }
    };
    push(PathBuf::from(configured), "config");
    push(std::env::var_os("ANDROID_SDK_ROOT").map(PathBuf::from).unwrap_or_default(), "ANDROID_SDK_ROOT");
    push(std::env::var_os("ANDROID_HOME").map(PathBuf::from).unwrap_or_default(), "ANDROID_HOME");
    // Binários no PATH apontam para a raiz do SDK.
    for (bin, src) in [("emulator", "emulator on PATH"), ("adb", "adb on PATH")] {
        if let Some(p) = platform::which(bin) {
            let p = platform::canon(&p);
            if let Some(root) = p.parent().and_then(Path::parent) {
                push(root.to_path_buf(), src);
            }
        }
    }
    push(default_root(), "default");
    push(home.join("Android/Sdk"), "Android Studio default");
    push(home.join("Library/Android/sdk"), "Android Studio default (macOS)");
    push(home.join("Applications/Android/Sdk"), "known location");
    push(home.join("Applications/android-sdk"), "known location");
    push(home.join(".local/share/Android/Sdk"), "known location");
    push(home.join("android-sdk"), "known location");
    push(home.join("Android/sdk"), "known location");
    push(PathBuf::from("/opt/android-sdk"), "known location");
    push(PathBuf::from("/usr/lib/android-sdk"), "known location");
    push(PathBuf::from("/usr/local/share/android-sdk"), "known location");
    push(PathBuf::from("/opt/homebrew/share/android-commandlinetools"), "Homebrew");
    push(PathBuf::from("/usr/local/share/android-commandlinetools"), "Homebrew");

    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for (p, src) in list {
        let p = clean(&p);
        if !seen.insert(p.clone()) {
            continue;
        }
        let sd = Sdk::new(&p);
        let exists = sd.exists();
        // Só interessam locais que existem ou foram pedidos explicitamente.
        if exists || src == "config" {
            out.push(Candidate { path: p.to_string_lossy().into_owned(), source: src.to_string(), exists, emulator: sd.has_emulator(), adb: sd.has_adb() });
        }
    }
    out
}

/// Remove "." e barras sobrando sem tocar no disco.
fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

static OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Força o SDK desta execução (`--sdk CAMINHO`), sem gravar nas configurações.
pub fn set_override(path: &str) {
    *OVERRIDE.write().unwrap() = Some(clean(Path::new(path)));
}

/// Escolhe o SDK a usar: o forçado por `--sdk`; o configurado; senão o primeiro
/// com emulador; senão o primeiro que exista; senão o padrão (ainda não criado).
pub fn detect(configured: &str) -> Sdk {
    if let Some(p) = OVERRIDE.read().unwrap().clone() {
        return Sdk::new(p);
    }
    if !configured.is_empty() {
        return Sdk::new(clean(Path::new(configured)));
    }
    let cands = candidates("");
    if let Some(c) = cands.iter().find(|c| c.emulator) {
        return Sdk::new(&c.path);
    }
    if let Some(c) = cands.iter().find(|c| c.exists && (c.adb || looks_like_sdk(Path::new(&c.path)))) {
        return Sdk::new(&c.path);
    }
    Sdk::new(default_root())
}

fn looks_like_sdk(root: &Path) -> bool {
    ["platforms", "platform-tools", "system-images", "cmdline-tools", "build-tools", "licenses"].iter().any(|d| platform::dir_exists(&root.join(d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sdk_goes_to_applications_android_sdk_and_is_found_afterwards() {
        let _g = crate::testutil::env_lock();
        let home = crate::testutil::tmp("sdk-default");
        // sem SDK no PATH nem nas variáveis (a máquina de quem roda o teste pode ter um)
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &home);
        std::env::set_var("HOME", &home);
        for v in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
            std::env::remove_var(v);
        }
        assert_eq!(default_root(), home.join("Applications/AndroidSDK"));
        // ainda não existe: o padrão é devolvido como destino da instalação
        assert_eq!(detect("").root, home.join("Applications/AndroidSDK"));
        // depois de criado com o conteúdo de um SDK, é detectado sozinho
        std::fs::create_dir_all(default_root().join("platform-tools")).unwrap();
        assert!(candidates("").iter().any(|c| c.path == default_root().to_string_lossy() && c.exists));
        assert_eq!(detect("").root, default_root());
        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
    }
}
