//! Lê, cria e edita os Android Virtual Devices (os arquivos em ~/.android/avd),
//! sem depender do avdmanager.

pub mod create;
pub mod ini;
pub mod skin;

pub use create::{build_config, create, fetch_skin_for_create, fetch_skin_for_update, update, view_of, write_raw_config, CreateSpec, Settings, View};
pub use ini::Ini;

use crate::sdk::{self, Sdk};
use crate::{devices, platform, Error, Result};
use serde::Serialize;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Diretório de dados do Android do usuário (~/.android).
pub fn user_home() -> PathBuf {
    if let Some(v) = std::env::var_os("ANDROID_USER_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    if let Some(v) = std::env::var_os("ANDROID_SDK_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(v).join(".android");
    }
    platform::home().join(".android")
}

/// Pasta onde ficam os AVDs.
pub fn home() -> PathBuf {
    if let Some(v) = std::env::var_os("ANDROID_AVD_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    user_home().join("avd")
}

/// Um dispositivo virtual (nomes dos campos = os que a interface espera).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Avd {
    pub name: String,
    pub display_name: String,
    pub dir: String,
    pub ini_path: String,
    pub target: String,
    pub api: String,
    pub api_name: String,
    pub tag: String,
    pub tag_display: String,
    pub abi: String,
    pub play_store: bool,
    pub device_id: String,
    pub device_name: String,
    pub category: String,
    pub width: i32,
    pub height: i32,
    pub density: i32,
    #[serde(rename = "ramMB")]
    pub ram_mb: i32,
    pub image_pkg: String,
    #[serde(rename = "imageInstalled")]
    pub image_ok: bool,
    pub size_bytes: i64,
    /// epoch segundos
    pub last_used: i64,
    pub problem: String,
}

/// Aceita só `[A-Za-z0-9._-]+` sem ponto inicial.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Converte um nome livre (“Pixel 9 API 36”) em identificador.
pub fn sanitize_name(s: &str) -> String {
    let out: String = s
        .trim()
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') => Some(c),
            ' ' => Some('_'),
            _ => None,
        })
        .collect();
    out.trim_start_matches('.').to_string()
}

/// "system-images/android-36/google_apis/x86_64/" -> "system-images;android-36;google_apis;x86_64".
pub fn image_pkg_from_sysdir(sysdir: &str) -> String {
    sysdir.trim_matches('/').replace('/', ";")
}

/// Converte "2G", "2048M", "2048 MB", "2048" em megabytes.
pub fn parse_mb(s: &str) -> i32 {
    let s = s.trim().to_uppercase();
    if s.is_empty() {
        return 0;
    }
    let (num, mult) = if let Some(n) = s.strip_suffix("GB").or_else(|| s.strip_suffix('G')) {
        (n, 1024.0)
    } else if let Some(n) = s.strip_suffix("MB").or_else(|| s.strip_suffix('M')) {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("KB").or_else(|| s.strip_suffix('K')) {
        (n, 1.0 / 1024.0)
    } else {
        (s.as_str(), 1.0)
    };
    num.trim().parse::<f64>().map(|f| (f * mult) as i32).unwrap_or(0)
}

fn ini_path(name: &str) -> PathBuf {
    home().join(format!("{name}.ini"))
}

/// Informa se existe um AVD com este nome.
pub fn exists(name: &str) -> bool {
    valid_name(name) && ini_path(name).is_file()
}

/// Lista todos os AVDs, ordenados pelo nome de exibição.
pub fn list(sdk: &Sdk) -> Vec<Avd> {
    let Ok(rd) = std::fs::read_dir(home()) else { return vec![] };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let is_file = e.file_type().map(|t| !t.is_dir()).unwrap_or(false);
            n.strip_suffix(".ini").filter(|_| is_file).map(str::to_string)
        })
        .collect();
    names.sort();
    let mut out: Vec<Avd> = std::thread::scope(|sc| {
        let hs: Vec<_> = names.iter().map(|n| sc.spawn(move || load(sdk, n))).collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    out.sort_by_key(|a| a.display_name.to_lowercase());
    out
}

/// Resolve o diretório .avd a partir do <nome>.ini.
fn dir_of(name: &str) -> Result<(PathBuf, Ini)> {
    let ini = Ini::read(&ini_path(name)).map_err(|_| err!(404, "o AVD {:?} não existe", "AVD {:?} does not exist", name))?;
    let mut dir = PathBuf::from(ini.get("path"));
    if ini.get("path").is_empty() || !platform::dir_exists(&dir) {
        let rel = ini.get("path.rel");
        if !rel.is_empty() {
            let alt = user_home().join(rel);
            if platform::dir_exists(&alt) {
                dir = alt;
            }
        }
    }
    if dir.as_os_str().is_empty() {
        dir = home().join(format!("{name}.avd"));
    }
    Ok((dir, ini))
}

/// Lê um AVD pelo nome. Nunca falha: problemas vão em `problem`.
pub fn load(sdk: &Sdk, name: &str) -> Avd {
    let mut a =
        Avd { name: name.to_string(), display_name: name.replace('_', " "), ini_path: ini_path(name).to_string_lossy().into_owned(), ..Default::default() };
    let (dir, ini) = match dir_of(name) {
        Ok(v) => v,
        Err(_) => {
            a.problem = tr_pt_en("arquivo .ini ilegível", "unreadable .ini file");
            return a;
        }
    };
    a.dir = dir.to_string_lossy().into_owned();
    a.target = ini.get("target").to_string();
    a.api = a.target.trim_start_matches("android-").to_string();
    let cfg = match Ini::read(&dir.join("config.ini")) {
        Ok(c) => c,
        Err(_) => {
            a.problem = tr_pt_en("config.ini não encontrado", "config.ini not found");
            return a;
        }
    };
    if !cfg.get("avd.ini.displayname").is_empty() {
        a.display_name = cfg.get("avd.ini.displayname").to_string();
    }
    let sysdir = cfg.get("image.sysdir.1");
    a.image_pkg = image_pkg_from_sysdir(sysdir);
    let parts: Vec<&str> = a.image_pkg.split(';').collect();
    if parts.len() == 4 {
        a.api = parts[1].trim_start_matches("android-").to_string();
        a.tag = parts[2].to_string();
        a.abi = parts[3].to_string();
    }
    a.api_name = sdk::api_name(&a.api, "");
    if !cfg.get("tag.id").is_empty() {
        a.tag = cfg.get("tag.id").to_string();
    }
    a.tag_display = if cfg.get("tag.display").is_empty() { a.tag.clone() } else { cfg.get("tag.display").to_string() };
    if !cfg.get("abi.type").is_empty() {
        a.abi = cfg.get("abi.type").to_string();
    }
    a.play_store = matches!(cfg.get("PlayStore.enabled").to_lowercase().as_str(), "true" | "yes");
    a.device_id = cfg.get("hw.device.name").to_string();
    a.device_name = a.device_id.clone();
    a.category = "phone".into();
    if let Some(d) = devices::get(&a.device_id) {
        a.device_name = d.name;
        a.category = d.category;
    } else if a.tag.contains("wear") {
        a.category = "wear".into();
    } else if a.tag.contains("tv") {
        a.category = "tv".into();
    } else if a.tag.contains("automotive") {
        a.category = "automotive".into();
    }
    a.width = cfg.get("hw.lcd.width").parse().unwrap_or(0);
    a.height = cfg.get("hw.lcd.height").parse().unwrap_or(0);
    a.density = cfg.get("hw.lcd.density").parse().unwrap_or(0);
    a.ram_mb = parse_mb(cfg.get("hw.ramSize"));
    if !sysdir.is_empty() {
        a.image_ok = platform::dir_exists(&sdk.root.join(sysdir));
        if !a.image_ok {
            a.problem = trf!("a imagem de sistema {} não está instalada", "system image {} is not installed", a.image_pkg);
        }
    } else {
        a.problem = tr_pt_en("o AVD não aponta para nenhuma imagem de sistema", "the AVD does not point to any system image");
    }
    a.size_bytes = disk_usage(&dir);
    let mtime = |p: PathBuf| std::fs::metadata(p).ok().map(|m| m.mtime());
    a.last_used = mtime(dir.join("userdata-qemu.img")).or_else(|| mtime(dir.join("config.ini"))).unwrap_or(0);
    a
}

fn tr_pt_en(pt: &str, en: &str) -> String {
    crate::lang::tr(pt, en)
}

/// Soma o espaço realmente ocupado em disco (arquivos esparsos como o
/// userdata-qemu.img contam só os blocos usados).
pub fn disk_usage(dir: &Path) -> i64 {
    fn walk(p: &Path, total: &mut i64) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue }; // não segue links
            if md.is_dir() {
                walk(&e.path(), total);
            } else {
                *total += md.blocks() as i64 * 512;
            }
        }
    }
    let mut total = 0;
    walk(&platform::canon(dir), &mut total);
    total
}

/// config.ini completo do AVD (+ caminho do arquivo).
pub fn config(name: &str) -> Result<(Ini, PathBuf)> {
    let (dir, _) = dir_of(name)?;
    let p = dir.join("config.ini");
    let c = Ini::read(&p).map_err(|e| Error::internal(e.to_string()))?;
    Ok((c, p))
}

fn avd_dir(name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        return Err(bad!("nome de AVD inválido: {:?}", "invalid AVD name: {:?}", name));
    }
    Ok(dir_of(name)?.0)
}

/// Apaga o AVD (pasta e .ini).
pub fn delete(name: &str) -> Result<()> {
    let dir = avd_dir(name)?;
    if dir.extension().and_then(|e| e.to_str()) != Some("avd") || !platform::file_exists(&dir.join("config.ini")) {
        return Err(bad!("recusando apagar {}: não parece uma pasta de AVD", "refusing to delete {}: it does not look like an AVD folder", dir.display()));
    }
    std::fs::remove_dir_all(&dir)?;
    std::fs::remove_file(ini_path(name))?;
    Ok(())
}

/// Apaga os dados do usuário (como “Wipe Data” do Device Manager): o próximo boot
/// parte de uma instalação limpa. Preserva o cartão SD.
pub fn wipe_data(name: &str) -> Result<()> {
    let dir = avd_dir(name)?;
    for f in [
        "userdata-qemu.img",
        "userdata-qemu.img.qcow2",
        "userdata.img",
        "cache.img",
        "cache.img.qcow2",
        "encryptionkey.img",
        "encryptionkey.img.qcow2",
        "multiinstance.lock",
        "hardware-qemu.ini",
    ] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    let _ = std::fs::remove_dir_all(dir.join("snapshots"));
    Ok(())
}

/// Cria uma cópia do AVD com outro nome.
pub fn duplicate(sdk: &Sdk, name: &str, new_name: &str, new_display: &str) -> Result<Avd> {
    if !valid_name(new_name) {
        return Err(bad!("nome inválido: use letras, números, ponto, hífen e sublinhado", "invalid name: use letters, digits, dot, dash and underscore"));
    }
    let src = avd_dir(name)?;
    if ini_path(new_name).exists() {
        return Err(bad!("já existe um AVD chamado {:?}", "an AVD named {:?} already exists", new_name));
    }
    let dst = home().join(format!("{new_name}.avd"));
    if dst.exists() {
        return Err(bad!("a pasta {} já existe", "the folder {} already exists", dst.display()));
    }
    if let Err(e) = copy_tree(&src, &dst) {
        let _ = std::fs::remove_dir_all(&dst);
        return Err(e);
    }
    for junk in ["snapshots", "multiinstance.lock", "hardware-qemu.ini", "hardware-qemu.ini.lock", "emu-launch-params.txt", "tmpAdbCmds"] {
        let p = dst.join(junk);
        let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
    }
    let cfg_path = dst.join("config.ini");
    let mut cfg = Ini::read(&cfg_path).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&dst);
    })?;
    let display = if new_display.is_empty() { new_name.replace('_', " ") } else { new_display.to_string() };
    cfg.set("avd.ini.displayname", &display);
    if cfg.has("AvdId") {
        cfg.set("AvdId", new_name);
    }
    std::fs::write(&cfg_path, cfg.to_text())?;
    let target = Ini::read(&ini_path(name)).map(|i| i.get("target").to_string()).unwrap_or_default();
    if let Err(e) = write_avd_ini(new_name, &dst, &target) {
        let _ = std::fs::remove_dir_all(&dst);
        return Err(e);
    }
    Ok(load(sdk, new_name))
}

pub(crate) fn write_avd_ini(name: &str, dir: &Path, target: &str) -> Result<()> {
    let mut ini = Ini::new();
    ini.set("avd.ini.encoding", "UTF-8");
    ini.set("path", &dir.to_string_lossy());
    if let Ok(rel) = dir.strip_prefix(user_home()) {
        ini.set("path.rel", &rel.to_string_lossy());
    }
    if !target.is_empty() {
        ini.set("target", target);
    }
    std::fs::write(ini_path(name), ini.to_text())?;
    Ok(())
}

/// Copia um diretório preservando arquivos esparsos (imagens de disco).
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    let run = |args: &[&str]| -> std::result::Result<(), String> {
        let out = Command::new("cp").args(args).arg(src).arg(dst).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    };
    if platform::is_mac() {
        // clone APFS; se falhar (volume sem suporte) copia normal
        if run(&["-Rc"]).is_err() {
            let _ = std::fs::remove_dir_all(dst);
            run(&["-R"]).map_err(|e| Error::internal(format!("cp: {e}")))?;
        }
        Ok(())
    } else {
        run(&["-a", "--reflink=auto", "--sparse=always"]).map_err(|e| Error::internal(format!("cp: {e}")))
    }
}

#[cfg(test)]
mod tests;
