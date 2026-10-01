//! Pacotes já presentes no disco (via package.xml, como o sdkmanager).

use super::manifest::{Details, Revision, Tag};
use super::Sdk;
use crate::platform;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Installed {
    pub path: String,
    pub name: String,
    pub revision: Revision,
    pub license_id: String,
    pub dir: PathBuf,
    pub details: Details,
}

impl Installed {
    pub fn rev(&self) -> String {
        self.revision.to_string()
    }
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}
fn child_text(n: roxmltree::Node, name: &str) -> String {
    child(n, name).map(|c| c.text().unwrap_or("").trim().to_string()).unwrap_or_default()
}
fn int_of(n: roxmltree::Node, name: &str) -> i32 {
    child_text(n, name).parse().unwrap_or(0)
}

/// Lê um package.xml local.
pub fn read_package_xml(file: &Path) -> Option<Installed> {
    let text = std::fs::read_to_string(file).ok()?;
    let doc = roxmltree::Document::parse(&text).ok()?;
    let lp = doc.root_element().children().find(|c| c.is_element() && c.tag_name().name() == "localPackage")?;
    let path = lp.attribute("path")?.to_string();
    let rv = child(lp, "revision");
    let details = child(lp, "type-details").map(|td| Details {
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
    });
    Some(Installed {
        path,
        name: child_text(lp, "display-name"),
        revision: Revision {
            major: rv.map(|r| int_of(r, "major")).unwrap_or(0),
            minor: rv.map(|r| int_of(r, "minor")).unwrap_or(0),
            micro: rv.map(|r| int_of(r, "micro")).unwrap_or(0),
            preview: rv.map(|r| int_of(r, "preview")).unwrap_or(0),
        },
        license_id: child(lp, "uses-license").and_then(|n| n.attribute("ref")).unwrap_or("").to_string(),
        dir: file.parent().map(Path::to_path_buf).unwrap_or_default(),
        details: details.unwrap_or_default(),
    })
}

/// Lê um source.properties (chave=valor).
pub fn read_source_props(file: &Path) -> Option<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut m = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(i) = line.find('=') {
            if i > 0 {
                m.insert(line[..i].trim().to_string(), line[i + 1..].trim().to_string());
            }
        }
    }
    Some(m)
}

impl Sdk {
    /// Varre o SDK atrás de pacotes instalados. Pacotes descompactados à mão, só com
    /// source.properties, são reconhecidos para emulator, platform-tools e
    /// cmdline-tools. Links simbólicos para pastas são seguidos (é comum mover
    /// system-images para outro disco).
    pub fn list_installed(&self) -> Vec<Installed> {
        let mut out = vec![];
        if !self.exists() {
            return out;
        }
        let root = platform::canon(&self.root);
        walk(&root, "", 1, &mut out);
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    pub fn installed_map(&self) -> BTreeMap<String, Installed> {
        self.list_installed().into_iter().map(|p| (p.path.clone(), p)).collect()
    }

    /// Propriedades de uma imagem de sistema instalada.
    pub fn read_sys_image(&self, pkg_path: &str) -> Option<SysImageProps> {
        let dir = self.package_dir(pkg_path);
        if !platform::dir_exists(&dir) {
            return None;
        }
        let parts: Vec<&str> = pkg_path.split(';').collect();
        if parts.len() != 4 || parts[0] != "system-images" {
            return None;
        }
        let mut p = SysImageProps {
            dir: format!("{}/{}/{}/{}/", parts[0], parts[1], parts[2], parts[3]),
            api: parts[1].trim_start_matches("android-").to_string(),
            target: parts[1].to_string(),
            tag_id: parts[2].to_string(),
            abi: parts[3].to_string(),
            ..Default::default()
        };
        if let Some(sp) = read_source_props(&dir.join("source.properties")) {
            if let Some(v) = sp.get("AndroidVersion.ApiLevel").filter(|v| !v.is_empty()) {
                p.api = v.clone();
            }
            if let Some(v) = sp.get("SystemImage.TagId").filter(|v| !v.is_empty()) {
                p.tag_id = v.clone();
            }
            p.tag_display = sp.get("SystemImage.TagDisplay").cloned().unwrap_or_default();
            if let Some(v) = sp.get("SystemImage.Abi").filter(|v| !v.is_empty()) {
                p.abi = v.clone();
            }
            p.vendor = sp.get("Addon.VendorDisplay").cloned().unwrap_or_default();
        }
        p.play_store = p.tag_id.contains("playstore") || p.tag_display.to_lowercase().contains("play");
        Some(p)
    }
}

fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<Installed>) {
    const MAX_DEPTH: usize = 4;
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if depth == 1 && matches!(name.as_str(), "licenses" | ".temp" | ".downloadIntermediates" | "patcher") {
            continue;
        }
        if name.ends_with(".installing") || name.contains(".old-") {
            continue;
        }
        let p = e.path();
        if !p.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            continue; // metadata() segue links
        }
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if let Some(mut pk) = read_package_xml(&p.join("package.xml")) {
            pk.dir = p;
            out.push(pk);
            continue;
        }
        // Pacotes descompactados à mão, sem package.xml.
        let pkg_path = if depth == 1 && (r == "emulator" || r == "platform-tools") {
            Some(r.clone())
        } else if depth == 2 && r.starts_with("cmdline-tools/") {
            Some(format!("cmdline-tools;{name}"))
        } else {
            None
        };
        if let Some(pkg_path) = pkg_path {
            if let Some(sp) = read_source_props(&p.join("source.properties")) {
                let rev = Revision::parse(sp.get("Pkg.Revision").map(String::as_str).unwrap_or(""));
                let desc = sp.get("Pkg.Desc").filter(|d| !d.is_empty()).cloned().unwrap_or_else(|| pkg_path.clone());
                out.push(Installed { path: pkg_path, name: desc, revision: rev, license_id: String::new(), dir: p, details: Details::default() });
                continue;
            }
        }
        if depth < MAX_DEPTH {
            walk(&p, &r, depth + 1, out);
        }
    }
}

/// Propriedades de uma imagem de sistema instalada (lidas do source.properties).
#[derive(Debug, Clone, Default)]
pub struct SysImageProps {
    pub api: String,
    pub tag_id: String,
    pub tag_display: String,
    pub abi: String,
    pub vendor: String,
    pub play_store: bool,
    /// android-36 / android-36.1 (nome da pasta da API)
    pub target: String,
    /// relativo ao SDK, com barra final (vai em image.sysdir.1)
    pub dir: String,
}
