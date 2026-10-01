//! Manifestos de pacotes do Google (repository2-*.xml / sys-img2-*.xml).

use super::http;
use super::license::License;
use crate::{platform, Error, Result};
use sha1::{Digest, Sha1};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Raiz do repositório público de pacotes do Android SDK.
pub const REPO_BASE: &str = "https://dl.google.com/android/repository/";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Versão de um pacote (major.minor.micro[ rcN]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Revision {
    pub major: i32,
    pub minor: i32,
    pub micro: i32,
    pub preview: i32,
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.major)?;
        if self.minor != 0 || self.micro != 0 {
            write!(f, ".{}", self.minor)?;
        }
        if self.micro != 0 {
            write!(f, ".{}", self.micro)?;
        }
        if self.preview > 0 {
            write!(f, " rc{}", self.preview)?;
        }
        Ok(())
    }
}

impl Revision {
    /// Lê "37.1.11" (ou "37.1.11 rc2" / "37.1.11-rc2").
    pub fn parse(s: &str) -> Revision {
        let mut r = Revision::default();
        let s = s.trim();
        let mut main = s;
        if let Some(i) = s.find([' ', '-']) {
            let rest = &s[i..];
            if rest.contains("rc") {
                r.preview = rest.trim_start_matches(['-', ' ', 'r', 'c']).parse().unwrap_or(0);
            }
            main = &s[..i];
        }
        let mut it = main.split('.').map(|p| p.parse::<i32>().unwrap_or(0));
        r.major = it.next().unwrap_or(0);
        r.minor = it.next().unwrap_or(0);
        r.micro = it.next().unwrap_or(0);
        r
    }
}

impl Ord for Revision {
    /// Uma versão preview vale menos que a final.
    fn cmp(&self, o: &Self) -> Ordering {
        let pv = |p: i32| if p == 0 { i32::MAX } else { p };
        (self.major, self.minor, self.micro, pv(self.preview)).cmp(&(o.major, o.minor, o.micro, pv(o.preview)))
    }
}
impl PartialOrd for Revision {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Arquivo .zip de um pacote para um determinado host.
#[derive(Debug, Clone, Default)]
pub struct Archive {
    pub url: String,
    pub size: i64,
    pub sha1: String,
    pub host_os: String,
    pub arch: String,
}

/// Pacote exigido por outro (ex.: a imagem exige o emulador).
#[derive(Debug, Clone)]
pub struct Dependency {
    pub path: String,
    pub min: Revision,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Tag {
    pub id: String,
    pub display: String,
}

/// Dados específicos do tipo do pacote que nos interessam.
#[derive(Debug, Clone, Default)]
pub struct Details {
    pub api_level: String,
    pub codename: String,
    pub ext_level: String,
    pub base_ext: String,
    pub tags: Vec<Tag>,
    pub vendor_id: String,
    pub vendor_display: String,
    pub abi: String,
}

impl Details {
    /// Nome legível da tag principal (ex.: "Google APIs").
    pub fn tag_display(&self) -> String {
        self.tags.first().map(|t| t.display.clone()).unwrap_or_default()
    }
}

/// Pacote disponível para download.
#[derive(Debug, Clone, Default)]
pub struct Remote {
    pub path: String,
    pub obsolete: bool,
    pub name: String,
    pub revision: Revision,
    pub license_id: String,
    pub channel_id: String,
    pub channel: String,
    /// Valor de xsi:type, ex. "sys-img:sysImgDetailsType".
    pub details_type: String,
    /// Conteúdo bruto de <type-details>.
    pub details_inner: String,
    pub details: Details,
    pub deps: Vec<Dependency>,
    pub archives: Vec<Archive>,
    /// Diretório do manifesto, base das URLs relativas.
    pub base_url: String,
}

impl Remote {
    /// Escolhe o arquivo adequado ao host (None se o pacote não existe para esta plataforma).
    pub fn archive_for(&self, host_os: &str, host_arch: &str) -> Option<&Archive> {
        let mut best: Option<(&Archive, i32)> = None;
        for a in &self.archives {
            if !a.host_os.is_empty() && a.host_os != host_os {
                continue;
            }
            if !a.arch.is_empty() && a.arch != host_arch {
                continue;
            }
            let score = (!a.host_os.is_empty()) as i32 + (!a.arch.is_empty()) as i32;
            if best.map(|b| score > b.1).unwrap_or(true) {
                best = Some((a, score));
            }
        }
        best.map(|b| b.0)
    }

    pub fn abs_url(&self, a: &Archive) -> String {
        if a.url.starts_with("http://") || a.url.starts_with("https://") {
            a.url.clone()
        } else {
            format!("{}{}", self.base_url, a.url)
        }
    }
}

// ---- parser --------------------------------------------------------------------

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text_of(n: roxmltree::Node) -> String {
    n.children().filter(|c| c.is_text()).filter_map(|c| c.text()).collect::<String>()
}

fn child_text(n: roxmltree::Node, name: &str) -> String {
    child(n, name).map(|c| text_of(c).trim().to_string()).unwrap_or_default()
}

fn int_of(n: roxmltree::Node, name: &str) -> i32 {
    child_text(n, name).parse().unwrap_or(0)
}

fn revision_of(n: Option<roxmltree::Node>) -> Revision {
    let Some(n) = n else { return Revision::default() };
    Revision { major: int_of(n, "major"), minor: int_of(n, "minor"), micro: int_of(n, "micro"), preview: int_of(n, "preview") }
}

/// Conteúdo de um elemento como texto bruto XML (sem a tag de abertura/fechamento).
fn inner_xml(src: &str, n: roxmltree::Node) -> String {
    let s = &src[n.range()];
    let Some(open_end) = s.find('>') else { return String::new() };
    if s[..open_end].ends_with('/') {
        return String::new();
    }
    let Some(close) = s.rfind("</") else { return String::new() };
    if close <= open_end {
        return String::new();
    }
    s[open_end + 1..close].to_string()
}

/// Lê um manifesto (repository2 ou sys-img2) já baixado.
pub fn parse_repo(src: &str, base_url: &str) -> Result<(Vec<Remote>, BTreeMap<String, License>)> {
    let doc = roxmltree::Document::parse(src).map_err(|e| Error::internal(e.to_string()))?;
    let root = doc.root_element();
    let mut licenses = BTreeMap::new();
    let mut channels = BTreeMap::new();
    let mut out = vec![];
    for n in root.children().filter(|c| c.is_element()) {
        match n.tag_name().name() {
            "license" => {
                if let Some(id) = n.attribute("id") {
                    licenses.insert(id.to_string(), License { id: id.to_string(), text: text_of(n) });
                }
            }
            "channel" => {
                if let Some(id) = n.attribute("id") {
                    channels.insert(id.to_string(), text_of(n).trim().to_string());
                }
            }
            _ => {}
        }
    }
    for p in root.children().filter(|c| c.is_element() && c.tag_name().name() == "remotePackage") {
        let mut r = Remote {
            path: p.attribute("path").unwrap_or("").to_string(),
            obsolete: p.attribute("obsolete") == Some("true"),
            name: child_text(p, "display-name"),
            revision: revision_of(child(p, "revision")),
            license_id: child(p, "uses-license").and_then(|n| n.attribute("ref")).unwrap_or("").to_string(),
            channel_id: child(p, "channelRef").and_then(|n| n.attribute("ref")).unwrap_or("").to_string(),
            base_url: base_url.to_string(),
            ..Default::default()
        };
        r.channel = channels.get(&r.channel_id).cloned().filter(|c| !c.is_empty()).unwrap_or_else(|| "stable".to_string());
        if let Some(td) = child(p, "type-details") {
            r.details_type = td.attribute((XSI, "type")).unwrap_or("").to_string();
            r.details_inner = inner_xml(src, td);
            r.details = Details {
                api_level: child_text(td, "api-level"),
                codename: child_text(td, "codename"),
                ext_level: child_text(td, "extension-level"),
                base_ext: child_text(td, "base-extension"),
                abi: child_text(td, "abi"),
                vendor_id: child(td, "vendor").map(|v| child_text(v, "id")).unwrap_or_default(),
                vendor_display: child(td, "vendor").map(|v| child_text(v, "display")).unwrap_or_default(),
                tags: td
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "tag")
                    .map(|t| Tag { id: child_text(t, "id"), display: child_text(t, "display") })
                    .collect(),
            };
        }
        if let Some(deps) = child(p, "dependencies") {
            for d in deps.children().filter(|c| c.is_element() && c.tag_name().name() == "dependency") {
                r.deps.push(Dependency { path: d.attribute("path").unwrap_or("").to_string(), min: revision_of(child(d, "min-revision")) });
            }
        }
        if let Some(archs) = child(p, "archives") {
            for a in archs.children().filter(|c| c.is_element() && c.tag_name().name() == "archive") {
                let Some(c) = child(a, "complete") else { continue };
                r.archives.push(Archive {
                    url: child_text(c, "url"),
                    size: child_text(c, "size").parse().unwrap_or(0),
                    sha1: child_text(c, "checksum").to_lowercase(),
                    host_os: child_text(a, "host-os"),
                    arch: child_text(a, "host-arch"),
                });
            }
        }
        out.push(r);
    }
    Ok((out, licenses))
}

// ---- catálogo --------------------------------------------------------------------

/// Conjunto de pacotes publicados pelo Google.
#[derive(Debug, Default)]
pub struct Catalog {
    pub packages: Vec<Remote>,
    pub licenses: BTreeMap<String, License>,
    /// Segundos desde a época Unix em que foi montado.
    pub fetched: u64,
    /// Verdadeiro quando só havia cópia antiga em cache.
    pub offline: bool,
}

fn channel_allowed(p: &Remote, preview: bool) -> bool {
    preview || p.channel_id.is_empty() || p.channel_id == "channel-0"
}

impl Catalog {
    /// Melhor revisão permitida do pacote com o path dado.
    pub fn find(&self, path: &str, preview: bool) -> Option<&Remote> {
        self.packages.iter().filter(|p| p.path == path && channel_allowed(p, preview)).max_by(|a, b| a.revision.cmp(&b.revision))
    }

    /// Para cada path, a melhor revisão permitida (ordenado por path).
    pub fn latest(&self, preview: bool) -> Vec<&Remote> {
        let mut best: BTreeMap<&str, &Remote> = BTreeMap::new();
        for p in &self.packages {
            if !channel_allowed(p, preview) || p.obsolete {
                continue;
            }
            match best.get(p.path.as_str()) {
                Some(cur) if p.revision.cmp(&cur.revision) != Ordering::Greater => {}
                _ => {
                    best.insert(&p.path, p);
                }
            }
        }
        best.into_values().collect()
    }
}

fn cache_path(url: &str) -> PathBuf {
    let sum = hex::encode(Sha1::digest(url.as_bytes()));
    let base = url.rsplit('/').next().unwrap_or("manifest.xml");
    platform::cache_dir().join("manifests").join(format!("{}-{}", &sum[..16], base))
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let mut resp = http::short().get(url).call().map_err(|e| Error::new(502, format!("{url}: {e}")))?;
    let st = resp.status().as_u16();
    if st != 200 {
        return Err(Error::new(502, format!("{url}: HTTP {st}")));
    }
    resp.body_mut().with_config().limit(64 << 20).read_to_vec().map_err(|e| Error::new(502, format!("{url}: {e}")))
}

/// Baixa `url`, usando o cache em disco enquanto estiver “fresco”. Se a rede
/// falhar devolve a cópia antiga (stale = true) quando houver.
fn cached_get(url: &str, ttl: Duration, force: bool) -> Result<(Vec<u8>, bool)> {
    let cp = cache_path(url);
    let age = cp.metadata().and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    if !force {
        if let Some(a) = age {
            if a < ttl {
                if let Ok(b) = std::fs::read(&cp) {
                    return Ok((b, false));
                }
            }
        }
    }
    match http_get(url) {
        Ok(b) => {
            if let Some(dir) = cp.parent() {
                if std::fs::create_dir_all(dir).is_ok() {
                    let _ = std::fs::write(&cp, &b);
                }
            }
            Ok((b, false))
        }
        Err(e) => {
            if age.is_some() {
                if let Ok(old) = std::fs::read(&cp) {
                    return Ok((old, true));
                }
            }
            Err(e)
        }
    }
}

/// Tenta cada URL em ordem e devolve a primeira que funcionar: (dados, url, stale).
fn first_of(urls: &[String], ttl: Duration, force: bool) -> Result<(Vec<u8>, String, bool)> {
    let mut last = Error::internal("no URL");
    for u in urls {
        match cached_get(u, ttl, force) {
            Ok((b, stale)) => return Ok((b, u.clone(), stale)),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// URL do manifesto com versões de esquema mais novas primeiro (sys-img2-5.xml, -4, -3).
fn schema_variants(rel: &str) -> Vec<String> {
    let (dir, base) = match rel.rfind('/') {
        Some(i) => (&rel[..i + 1], &rel[i + 1..]),
        None => ("", rel),
    };
    let Some(num) = base.strip_prefix("sys-img2-").and_then(|r| r.strip_suffix(".xml")) else { return vec![rel.to_string()] };
    let cur: u32 = num.parse().unwrap_or(0);
    let mut out = vec![];
    for v in [cur, 5, 4, 3] {
        if v != 0 && !out.iter().any(|u: &String| u == &format!("{dir}sys-img2-{v}.xml")) {
            out.push(format!("{dir}sys-img2-{v}.xml"));
        }
    }
    out
}

fn dir_of(url: &str) -> String {
    match url.rfind('/') {
        Some(i) => url[..i + 1].to_string(),
        None => String::new(),
    }
}

type SiteResult = (Vec<Remote>, BTreeMap<String, License>, bool);

/// Baixa (ou lê do cache) os manifestos de pacotes e imagens de sistema.
pub fn fetch_catalog(force: bool) -> Result<Catalog> {
    let ttl = Duration::from_secs(3 * 3600);
    let mut cat = Catalog { fetched: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), ..Default::default() };

    let (data, url, stale) = first_of(&[format!("{REPO_BASE}repository2-4.xml"), format!("{REPO_BASE}repository2-3.xml")], ttl, force).map_err(|e| {
        Error::new(502, trf!("não foi possível obter a lista de pacotes do Android SDK: {}", "could not fetch the Android SDK package list: {}", e))
    })?;
    cat.offline = stale;
    let (pkgs, lic) =
        parse_repo(&String::from_utf8_lossy(&data), &dir_of(&url)).map_err(|e| Error::new(502, trf!("manifesto inválido: {}", "invalid manifest: {}", e)))?;
    cat.packages.extend(pkgs);
    cat.licenses.extend(lic);

    // Lista de sites de imagens de sistema.
    let Ok((list, _, stale2)) = first_of(&["7", "6", "5"].map(|v| format!("{REPO_BASE}addons_list-{v}.xml")), ttl, force) else {
        return Ok(cat); // sem imagens, mas o resto funciona
    };
    cat.offline |= stale2;
    let list = String::from_utf8_lossy(&list).into_owned();
    let mut urls = vec![];
    if let Ok(doc) = roxmltree::Document::parse(&list) {
        for s in doc.root_element().children().filter(|c| c.is_element() && c.tag_name().name() == "site") {
            let ty = s.attribute((XSI, "type")).unwrap_or("").to_lowercase();
            let u = child_text(s, "url");
            if ty.contains("sysimg") && !u.is_empty() {
                urls.push(u);
            }
        }
    }
    let results: Vec<Option<SiteResult>> = std::thread::scope(|sc| {
        let mut handles = vec![];
        for chunk in urls.chunks(urls.len().div_ceil(8).max(1)) {
            handles.push(sc.spawn(move || {
                chunk
                    .iter()
                    .map(|rel| {
                        let cands: Vec<String> = schema_variants(rel).into_iter().map(|c| format!("{REPO_BASE}{c}")).collect();
                        let (data, used, st) = first_of(&cands, ttl, force).ok()?;
                        let (ps, ls) = parse_repo(&String::from_utf8_lossy(&data), &dir_of(&used)).ok()?;
                        Some((ps, ls, st))
                    })
                    .collect::<Vec<_>>()
            }));
        }
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    for (ps, ls, st) in results.into_iter().flatten() {
        cat.packages.extend(ps);
        for (k, v) in ls {
            cat.licenses.entry(k).or_insert(v);
        }
        cat.offline |= st;
    }
    Ok(cat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Catalog, String) {
        let src = std::fs::read_to_string(format!("{}/tests/data/repo.xml", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (pkgs, lic) = parse_repo(&src, "https://example.test/repo/").unwrap();
        (Catalog { packages: pkgs, licenses: lic, ..Default::default() }, src)
    }

    #[test]
    fn revision() {
        let cases = [
            ("37.1.11", "37.1.11", Ordering::Equal),
            ("37.1.11", "37.2", Ordering::Less),
            ("38", "37.9.9", Ordering::Greater),
            ("19.0 rc1", "19.0", Ordering::Less),
            ("19.0 rc2", "19.0 rc1", Ordering::Greater),
        ];
        for (a, b, w) in cases {
            assert_eq!(Revision::parse(a).cmp(&Revision::parse(b)), w, "{a} vs {b}");
        }
        assert_eq!(Revision { major: 37, minor: 1, micro: 11, preview: 0 }.to_string(), "37.1.11");
        assert_eq!(Revision::parse("19.0 rc1").to_string(), "19 rc1");
    }

    #[test]
    fn parse_and_channels() {
        let (cat, _) = fixture();
        assert_eq!(cat.packages.len(), 4);
        assert_eq!(cat.find("emulator", false).unwrap().revision.to_string(), "37.1.11");
        assert_eq!(cat.find("emulator", true).unwrap().revision.to_string(), "37.3.2");
        let img = cat.find("system-images;android-36;google_apis_playstore;x86_64", false).unwrap();
        assert_eq!(img.details.abi, "x86_64");
        assert_eq!(img.details.tags[0].id, "google_apis_playstore");
        assert_eq!(img.details.vendor_display, "Google Inc.");
        assert_eq!(img.details_type, "sys-img:sysImgDetailsType");
        assert!(img.details_inner.contains("<api-level>36</api-level>"));
        assert_eq!(img.deps.len(), 1);
        assert_eq!(img.deps[0].path, "emulator");
        assert_eq!(img.deps[0].min.to_string(), "35.4.9");
        let em = cat.find("emulator", false).unwrap();
        assert_eq!(em.archive_for("linux", "x64").unwrap().sha1, "aa");
        assert!(em.archive_for("linux", "aarch64").is_none());
        assert_eq!(em.archive_for("macosx", "aarch64").unwrap().url, "emulator-mac-arm.zip");
        let pt = cat.find("platform-tools", false).unwrap();
        let a = pt.archive_for("macosx", "aarch64").unwrap();
        assert_eq!(pt.abs_url(a), "https://example.test/repo/pt-mac.zip");
        assert_eq!(cat.latest(false).len(), 3);
    }

    #[test]
    fn schema_variants_prefer_newest() {
        assert_eq!(
            schema_variants("sys-img/android/sys-img2-3.xml"),
            vec!["sys-img/android/sys-img2-3.xml", "sys-img/android/sys-img2-5.xml", "sys-img/android/sys-img2-4.xml"]
        );
        assert_eq!(schema_variants("addon2-4.xml"), vec!["addon2-4.xml"]);
    }
}
