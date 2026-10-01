//! Resolver dependências, baixar, verificar, extrair e registrar pacotes do SDK.

use super::license::License;
use super::manifest::{Archive, Catalog, Remote};
use super::{http, Sdk};
use crate::cancel::{Cancel, CANCELED};
use crate::{platform, Error, Result};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

/// Andamento de uma instalação.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// "download", "verify", "extract", "finalize"
    pub phase: String,
    pub package: String,
    pub name: String,
    pub done: i64,
    pub total: i64,
    /// 0..1 do plano inteiro
    pub overall: f64,
    /// pacote atual (1-based) e total de pacotes
    pub index: usize,
    pub count: usize,
}

/// Resultado de resolver o que precisa ser instalado.
#[derive(Debug, Default)]
pub struct Plan {
    pub install: Vec<Remote>,
    /// Licenças ainda não aceitas exigidas pelo plano.
    pub licenses: Vec<License>,
    /// Bytes a baixar.
    pub size: i64,
    /// Pacotes pedidos que não existem / não servem para este host.
    pub missing: Vec<String>,
}

struct Planner<'a> {
    sdk: &'a Sdk,
    cat: &'a Catalog,
    preview: bool,
    force: bool,
    have: BTreeMap<String, super::Installed>,
    added: HashSet<String>,
    plan: Plan,
}

impl Planner<'_> {
    fn add(&mut self, path: &str, min: Option<super::Revision>, requested: bool) {
        if self.added.contains(path) {
            return;
        }
        let Some(r) = self.cat.find(path, self.preview) else {
            if requested {
                self.plan.missing.push(path.to_string());
            }
            return;
        };
        if r.archive_for(platform::host_os(), platform::host_arch()).is_none() {
            if requested {
                self.plan.missing.push(path.to_string());
            }
            return;
        }
        let needed = match self.have.get(path) {
            None => true,
            Some(cur) => (self.force && requested) || min.map(|m| cur.revision < m).unwrap_or(false) || (requested && cur.revision < r.revision),
        };
        if !needed {
            return;
        }
        self.added.insert(path.to_string());
        for d in r.deps.clone() {
            self.add(&d.path, Some(d.min), false);
        }
        self.plan.install.push(r.clone());
    }
}

impl Sdk {
    /// Resolve dependências (emulador para imagens de sistema, etc.) e informa quais
    /// licenças faltam. Pacotes já instalados na mesma revisão (ou mais nova) são
    /// ignorados, a menos que `force`.
    pub fn plan_install(&self, cat: &Catalog, paths: &[String], preview: bool, force: bool) -> Plan {
        let mut pl = Planner { sdk: self, cat, preview, force, have: self.installed_map(), added: HashSet::new(), plan: Plan::default() };
        for p in paths {
            pl.add(p, None, true);
        }
        // Quem instala imagens ou o emulador quase sempre quer o adb junto.
        if pl.plan.install.iter().any(|r| r.path.starts_with("system-images;") || r.path == "emulator") {
            pl.add("platform-tools", None, false);
        }
        let _ = pl.sdk;
        let mut plan = pl.plan;
        let mut seen = HashSet::new();
        for r in &plan.install {
            if let Some(a) = r.archive_for(platform::host_os(), platform::host_arch()) {
                plan.size += a.size;
            }
            if r.license_id.is_empty() || !seen.insert(r.license_id.clone()) {
                continue;
            }
            if let Some(l) = cat.licenses.get(&r.license_id) {
                if !self.accepted(l) {
                    plan.licenses.push(l.clone());
                }
            }
        }
        plan.licenses.sort_by(|a, b| a.id.cmp(&b.id));
        plan
    }

    /// Baixa e instala os pacotes do plano. As licenças precisam ter sido aceitas antes.
    pub fn install(&self, cat: &Catalog, plan: &Plan, cancel: &Cancel, on: &mut dyn FnMut(Progress), logf: &dyn Fn(String)) -> Result<()> {
        for r in &plan.install {
            if let Some(l) = cat.licenses.get(&r.license_id) {
                if !self.accepted(l) {
                    return Err(Error::bad(trf!("a licença {:?} ainda não foi aceita", "license {:?} has not been accepted yet", r.license_id)));
                }
            }
        }
        fs::create_dir_all(&self.root)
            .map_err(|e| Error::internal(trf!("não foi possível criar {}: {}", "could not create {}: {}", self.root.display(), e)))?;
        if let Some(free) = free_space(&self.root) {
            // Precisa do zip + o conteúdo extraído (~2x); usamos 2,2x como folga.
            let need = (plan.size as f64 * 2.2) as u64;
            if free < need {
                return Err(Error::bad(trf!(
                    "espaço insuficiente em {}: livre {}, necessário cerca de {}",
                    "not enough space in {}: {} free, about {} needed",
                    self.root.display(),
                    human_bytes(free as i64),
                    human_bytes(need as i64)
                )));
            }
        }
        let (os, arch) = (platform::host_os(), platform::host_arch());
        let total_work: f64 = plan.install.iter().filter_map(|r| r.archive_for(os, arch)).map(|a| a.size as f64 + 1.0).sum();
        let mut done_work = 0.0;
        let count = plan.install.len();
        for (i, r) in plan.install.iter().enumerate() {
            let a = r
                .archive_for(os, arch)
                .ok_or_else(|| Error::bad(trf!("{} não está disponível para este sistema", "{} is not available for this system", r.path)))?;
            let weight = a.size as f64 + 1.0;
            let report = |on: &mut dyn FnMut(Progress), phase: &str, done: i64, total: i64, frac: f64| {
                on(Progress {
                    phase: phase.into(),
                    package: r.path.clone(),
                    name: r.name.clone(),
                    done,
                    total,
                    overall: (done_work + weight * frac) / total_work.max(1.0),
                    index: i + 1,
                    count,
                })
            };
            logf(format!("→ {} ({}, {})", r.name, r.path, trf!("revisão {}", "revision {}", r.revision)));

            let dl_dir = self.root.join(".downloadIntermediates");
            fs::create_dir_all(&dl_dir)?;
            let base = a.url.rsplit('/').next().unwrap_or("pkg.zip");
            let zip_path = dl_dir.join(format!("{}_{}", r.path.replace(';', "_"), base));
            download_file(&r.abs_url(a), &zip_path, a.size, cancel, &mut |done, total| {
                let total = if total <= 0 { a.size } else { total };
                report(on, "download", done, total, 0.9 * done as f64 / (total.max(1)) as f64);
            })
            .map_err(|e| prefix_err(e, trf!("download de {}", "download of {}", r.path)))?;
            report(on, "verify", 0, a.size, 0.9);
            if !a.sha1.is_empty() {
                if let Err(e) = verify_sha1(&zip_path, &a.sha1) {
                    let _ = fs::remove_file(&zip_path);
                    return Err(prefix_err(e, r.path.clone()));
                }
            }
            // Extrai ao lado do destino final: assim o rename é sempre dentro do
            // mesmo sistema de arquivos (pastas do SDK podem ser links para outro disco).
            let dest = self.package_dir(&r.path);
            let tmp = PathBuf::from(format!("{}.installing", dest.display()));
            let _ = fs::remove_dir_all(&tmp);
            fs::create_dir_all(&tmp)?;
            if let Err(e) = extract_zip(&zip_path, &tmp, cancel, &mut |done, total| {
                report(on, "extract", done, total, 0.92 + 0.07 * done as f64 / (total.max(1)) as f64);
            }) {
                let _ = fs::remove_dir_all(&tmp);
                return Err(prefix_err(e, trf!("extração de {}", "extraction of {}", r.path)));
            }
            report(on, "finalize", 0, 0, 0.99);
            let lic = cat.licenses.get(&r.license_id).cloned().unwrap_or(License { id: String::new(), text: String::new() });
            if let Err(e) = self.commit(r, &lic, &tmp) {
                let _ = fs::remove_dir_all(&tmp);
                return Err(prefix_err(e, trf!("instalação de {}", "installation of {}", r.path)));
            }
            let _ = fs::remove_file(&zip_path);
            report(on, "finalize", 0, 0, 1.0);
            done_work += weight;
            logf(format!("✓ {}", trf!("{} instalado", "{} installed", r.path)));
        }
        let _ = fs::remove_dir(self.root.join(".downloadIntermediates")); // só remove se vazia
        Ok(())
    }

    /// Move o conteúdo extraído para o destino final e grava o package.xml.
    fn commit(&self, r: &Remote, lic: &License, tmp: &Path) -> Result<()> {
        // O zip costuma ter uma única pasta raiz; o conteúdo dela é o pacote.
        let entries: Vec<_> = fs::read_dir(tmp)?.flatten().collect();
        let src = if entries.len() == 1 && entries[0].path().is_dir() { entries[0].path() } else { tmp.to_path_buf() };
        let dest = self.package_dir(&r.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut backup = None;
        if platform::dir_exists(&dest) {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let b = PathBuf::from(format!("{}.old-{}", dest.display(), nanos));
            fs::rename(&dest, &b)?;
            backup = Some(b);
        }
        if let Err(e) = fs::rename(&src, &dest) {
            if let Some(b) = &backup {
                let _ = fs::rename(b, &dest);
            }
            return Err(e.into());
        }
        fs::write(dest.join("package.xml"), package_xml(r, lic))?;
        if let Some(b) = backup {
            let _ = fs::remove_dir_all(b);
        }
        let _ = fs::remove_dir_all(tmp);
        Ok(())
    }

    /// Remove pacotes instalados.
    pub fn uninstall(&self, paths: &[String]) -> Result<()> {
        for p in paths {
            if p.is_empty() || p.contains("..") {
                return Err(Error::bad(trf!("pacote inválido: {:?}", "invalid package: {:?}", p)));
            }
            let dir = self.package_dir(p);
            let rel = dir.strip_prefix(&self.root).unwrap_or(&dir);
            if rel.as_os_str().is_empty() || rel.is_absolute() {
                return Err(Error::bad(trf!("pacote inválido: {:?}", "invalid package: {:?}", p)));
            }
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
            // Remove pastas-pai vazias (system-images/android-36/google_apis...).
            let mut parent = dir.parent().map(Path::to_path_buf);
            while let Some(pp) = parent {
                if pp == self.root || fs::remove_dir(&pp).is_err() {
                    break; // só remove se estiver vazia
                }
                parent = pp.parent().map(Path::to_path_buf);
            }
        }
        Ok(())
    }
}

fn prefix_err(e: Error, prefix: String) -> Error {
    if e.status == CANCELED {
        return e;
    }
    Error { message: format!("{prefix}: {}", e.message), ..e }
}

// ---- package.xml -------------------------------------------------------------------

const LOCAL_NAMESPACES: &str = concat!(
    r#"xmlns:ns2="http://schemas.android.com/repository/android/common/02" "#,
    r#"xmlns:ns3="http://schemas.android.com/repository/android/common/01" "#,
    r#"xmlns:ns4="http://schemas.android.com/repository/android/generic/01" "#,
    r#"xmlns:ns5="http://schemas.android.com/repository/android/generic/02" "#,
    r#"xmlns:ns6="http://schemas.android.com/sdk/android/repo/repository2/04" "#,
    r#"xmlns:ns7="http://schemas.android.com/sdk/android/repo/repository2/01" "#,
    r#"xmlns:ns8="http://schemas.android.com/sdk/android/repo/repository2/02" "#,
    r#"xmlns:ns9="http://schemas.android.com/sdk/android/repo/repository2/03" "#,
    r#"xmlns:ns10="http://schemas.android.com/sdk/android/repo/addon2/01" "#,
    r#"xmlns:ns11="http://schemas.android.com/sdk/android/repo/addon2/02" "#,
    r#"xmlns:ns12="http://schemas.android.com/sdk/android/repo/addon2/03" "#,
    r#"xmlns:ns13="http://schemas.android.com/sdk/android/repo/addon2/04" "#,
    r#"xmlns:ns14="http://schemas.android.com/sdk/android/repo/sys-img2/05" "#,
    r#"xmlns:ns15="http://schemas.android.com/sdk/android/repo/sys-img2/04" "#,
    r#"xmlns:ns16="http://schemas.android.com/sdk/android/repo/sys-img2/03" "#,
    r#"xmlns:ns17="http://schemas.android.com/sdk/android/repo/sys-img2/02" "#,
    r#"xmlns:ns18="http://schemas.android.com/sdk/android/repo/sys-img2/01""#,
);

/// Converte o xsi:type do manifesto ("sys-img:sysImgDetailsType") no prefixo usado
/// nos package.xml locais ("ns14:sysImgDetailsType").
fn details_prefix(t: &str) -> String {
    let local = t.rsplit(':').next().unwrap_or(t);
    let ns = match local {
        "genericDetailsType" => "ns5",
        "sysImgDetailsType" => "ns14",
        "addonDetailsType" => "ns13",
        _ => "ns6", // platformDetailsType, sourceDetailsType, extraDetailsType...
    };
    format!("{ns}:{local}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

/// Produz o mesmo package.xml que o sdkmanager grava ao instalar, o que faz o
/// Android Studio e o sdkmanager reconhecerem o pacote como instalado.
pub fn package_xml(r: &Remote, lic: &License) -> String {
    let mut b = String::new();
    b.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    b.push_str(&format!("<ns2:repository {LOCAL_NAMESPACES}>"));
    if !lic.id.is_empty() {
        b.push_str(&format!(r#"<license id="{}" type="text">{}</license>"#, esc_attr(&lic.id), esc(&lic.text)));
    }
    b.push_str(&format!(r#"<localPackage path="{}" obsolete="false">"#, esc_attr(&r.path)));
    let typ = if r.details_type.is_empty() { "generic:genericDetailsType" } else { r.details_type.as_str() };
    let inner = r.details_inner.trim();
    if inner.is_empty() {
        b.push_str(&format!(r#"<type-details xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="{}"/>"#, details_prefix(typ)));
    } else {
        b.push_str(&format!(
            r#"<type-details xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="{}">{}</type-details>"#,
            details_prefix(typ),
            inner
        ));
    }
    b.push_str(&format!("<revision><major>{}</major><minor>{}</minor><micro>{}</micro>", r.revision.major, r.revision.minor, r.revision.micro));
    if r.revision.preview != 0 {
        b.push_str(&format!("<preview>{}</preview>", r.revision.preview));
    }
    b.push_str("</revision>");
    b.push_str(&format!("<display-name>{}</display-name>", esc(&r.name)));
    if !r.license_id.is_empty() {
        b.push_str(&format!(r#"<uses-license ref="{}"/>"#, esc_attr(&r.license_id)));
    }
    if !r.deps.is_empty() {
        b.push_str("<dependencies>");
        for d in &r.deps {
            b.push_str(&format!(
                r#"<dependency path="{}"><min-revision><major>{}</major><minor>{}</minor><micro>{}</micro></min-revision></dependency>"#,
                esc_attr(&d.path),
                d.min.major,
                d.min.minor,
                d.min.micro
            ));
        }
        b.push_str("</dependencies>");
    }
    b.push_str("</localPackage></ns2:repository>");
    b
}

// ---- download ------------------------------------------------------------------------

/// Formata um tamanho em bytes.
pub fn human_bytes(n: i64) -> String {
    const UNIT: f64 = 1024.0;
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / UNIT;
    let mut exp = 0;
    while v >= UNIT && exp < 5 {
        v /= UNIT;
        exp += 1;
    }
    format!("{:.1} {}iB", v, ['K', 'M', 'G', 'T', 'P', 'E'][exp])
}

/// Espaço livre (bytes) no volume onde `dir` está (ou o pai existente mais próximo).
pub fn free_space(dir: &Path) -> Option<u64> {
    let mut probe = dir.to_path_buf();
    while !platform::dir_exists(&probe) {
        match probe.parent() {
            Some(p) if p != probe => probe = p.to_path_buf(),
            _ => break,
        }
    }
    let c = std::ffi::CString::new(probe.to_string_lossy().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}

enum DlErr {
    Status(u16),
    Net(String),
    Err(Error),
}

fn canceled() -> Error {
    Error::new(CANCELED, trf!("cancelado", "canceled"))
}

/// Baixa `url` para `dest`, retomando de `dest.part` quando possível.
pub fn download_file(url: &str, dest: &Path, expected: i64, cancel: &Cancel, progress: &mut dyn FnMut(i64, i64)) -> Result<()> {
    if let Ok(m) = fs::metadata(dest) {
        if expected <= 0 || m.len() as i64 == expected {
            progress(m.len() as i64, m.len() as i64); // já baixado antes
            return Ok(());
        }
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut offset = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut attempt = 0u32;
    loop {
        if cancel.is_canceled() {
            return Err(canceled());
        }
        match download_once(url, &part, &mut offset, expected, cancel, progress) {
            Ok(()) => break,
            Err(DlErr::Err(e)) => return Err(e),
            Err(DlErr::Status(416)) => {
                // o .part está inválido: recomeça
                let _ = fs::remove_file(&part);
                offset = 0;
            }
            Err(DlErr::Status(code)) => return Err(Error::new(502, format!("{url}: HTTP {code}"))),
            Err(DlErr::Net(msg)) => {
                if attempt >= 5 {
                    return Err(Error::new(502, msg));
                }
            }
        }
        attempt += 1;
        // espera com cancelamento
        let until = Instant::now() + Duration::from_secs(2 * attempt as u64);
        while Instant::now() < until {
            if cancel.is_canceled() {
                return Err(canceled());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fs::rename(&part, dest)?;
    Ok(())
}

fn download_once(
    url: &str,
    part: &Path,
    offset: &mut u64,
    expected: i64,
    cancel: &Cancel,
    progress: &mut dyn FnMut(i64, i64),
) -> std::result::Result<(), DlErr> {
    let mut req = http::long().get(url);
    if *offset > 0 {
        req = req.header("Range", format!("bytes={}-", *offset));
    }
    let resp = req.call().map_err(|e| DlErr::Net(format!("{url}: {e}")))?;
    let status = resp.status().as_u16();
    let truncate = match status {
        200 => {
            *offset = 0; // servidor ignorou o Range: recomeça do zero
            true
        }
        206 => false,
        s => return Err(DlErr::Status(s)),
    };
    let content_len = resp.body().content_length().unwrap_or(0) as i64;
    let total = if content_len > 0 { *offset as i64 + content_len } else { expected };
    let mut opts = OpenOptions::new();
    opts.create(true).write(true);
    if truncate {
        opts.truncate(true);
    } else {
        opts.append(true);
    }
    let mut f = opts.open(part).map_err(|e| DlErr::Err(e.into()))?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = *offset;
    let mut last = Instant::now();
    loop {
        if cancel.is_canceled() {
            return Err(DlErr::Err(canceled()));
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                *offset = done;
                return Err(DlErr::Net(format!("{url}: {e}")));
            }
        };
        f.write_all(&buf[..n]).map_err(|e| DlErr::Err(e.into()))?;
        done += n as u64;
        if last.elapsed() > Duration::from_millis(150) {
            last = Instant::now();
            *offset = done;
            progress(done as i64, total);
        }
    }
    f.flush().map_err(|e| DlErr::Err(e.into()))?;
    *offset = done;
    progress(done as i64, total);
    if expected > 0 && done as i64 != expected {
        return Err(DlErr::Net(trf!("download incompleto ({} de {} bytes)", "incomplete download ({} of {} bytes)", done, expected)));
    }
    Ok(())
}

pub fn verify_sha1(file: &Path, want: &str) -> Result<()> {
    let mut f = File::open(file)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let got = hex::encode(h.finalize());
    if !got.eq_ignore_ascii_case(want) {
        return Err(Error::bad(trf!(
            "checksum inválido (esperado {}, obtido {}) — o download foi descartado",
            "invalid checksum (expected {}, got {}) — the download was discarded",
            want,
            got
        )));
    }
    Ok(())
}

/// Junta `name` a `base` recusando `..`, caminhos absolutos e prefixos (zip slip).
fn safe_join(base: &Path, name: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    for c in Path::new(name).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Normaliza `.` e `..` sem tocar no disco.
fn lexical_clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// Extrai o zip em `dest`, protegendo contra "zip slip" e preservando permissões
/// (bit de execução) e links simbólicos.
pub fn extract_zip(zip_path: &Path, dest: &Path, cancel: &Cancel, progress: &mut dyn FnMut(i64, i64)) -> Result<()> {
    use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
    let file = File::open(zip_path)?;
    let mut ar = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| Error::internal(e.to_string()))?;
    let mut total: i64 = 0;
    for i in 0..ar.len() {
        total += ar.by_index_raw(i).map(|f| f.size() as i64).unwrap_or(0);
    }
    let mut done: i64 = 0;
    let mut last = Instant::now();
    let dest_clean = lexical_clean(dest);
    let mut buf = vec![0u8; 256 * 1024];
    for i in 0..ar.len() {
        if cancel.is_canceled() {
            return Err(canceled());
        }
        let mut f = ar.by_index(i).map_err(|e| Error::internal(e.to_string()))?;
        let name = f.name().to_string();
        let target = safe_join(dest, &name).ok_or_else(|| Error::bad(trf!("entrada suspeita no zip: {}", "suspicious entry in zip: {}", name)))?;
        let mode = f.unix_mode().unwrap_or(0);
        let size = f.size() as i64;
        if f.is_dir() || name.ends_with('/') {
            fs::create_dir_all(&target)?;
        } else if mode & 0o170000 == 0o120000 {
            let mut link = String::new();
            f.read_to_string(&mut link)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if Path::new(&link).is_absolute() {
                return Err(Error::bad(trf!("link simbólico absoluto no zip: {} -> {}", "absolute symlink in zip: {} -> {}", name, link)));
            }
            let resolved = lexical_clean(&target.parent().unwrap_or(dest).join(&link));
            if !resolved.starts_with(&dest_clean) {
                return Err(Error::bad(trf!("link simbólico suspeito no zip: {} -> {}", "suspicious symlink in zip: {} -> {}", name, link)));
            }
            let _ = fs::remove_file(&target);
            symlink(&link, &target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut perm = mode & 0o777;
            if perm == 0 {
                perm = 0o644;
            }
            perm |= 0o600;
            let mut out = OpenOptions::new().create(true).write(true).truncate(true).mode(perm).open(&target)?;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
            }
            drop(out);
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(perm));
            // ignora o umask
        }
        done += size;
        if last.elapsed() > Duration::from_millis(150) {
            last = Instant::now();
            progress(done, total);
        }
    }
    progress(total, total);
    Ok(())
}

/// Atalho usado em testes e na API.
pub fn archive_size(a: &Archive) -> i64 {
    a.size
}
