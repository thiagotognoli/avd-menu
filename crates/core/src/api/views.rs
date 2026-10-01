//! Pacotes do SDK como a interface os vê (catálogo + instalados numa lista só).

use crate::platform;
use crate::sdk::{api_name, Catalog, Remote, Sdk};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgView {
    pub path: String,
    pub name: String,
    pub revision: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub channel: String,
    pub installed: bool,
    #[serde(rename = "installedRevision", skip_serializing_if = "String::is_empty")]
    pub installed_rev: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub update: bool,
    /// existe download para este sistema
    pub available: bool,
    pub size: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub obsolete: bool,
    // Imagens de sistema e plataformas
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ext: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tag_display: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub abi: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub play_store: bool,
    /// ok | slow | no
    #[serde(skip_serializing_if = "String::is_empty")]
    pub compat: String,
    /// a imagem serve para o dispositivo escolhido
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<bool>,
}

pub fn pkg_type(path: &str) -> &'static str {
    if path.starts_with("system-images;") {
        "sysimg"
    } else if path.starts_with("platforms;") {
        "platform"
    } else if path.starts_with("build-tools;") {
        "build-tools"
    } else if path.starts_with("cmdline-tools;") {
        "cmdline-tools"
    } else if path == "platform-tools" {
        "platform-tools"
    } else if path == "emulator" {
        "emulator"
    } else if path.starts_with("ndk") {
        "ndk"
    } else if path.starts_with("cmake;") {
        "cmake"
    } else if path.starts_with("sources;") {
        "sources"
    } else if path.starts_with("extras;") {
        "extras"
    } else {
        "other"
    }
}

/// A imagem (ABI) roda bem neste computador?
pub fn host_compat(abi: &str) -> &'static str {
    let arch = platform::host_arch();
    match abi {
        "x86_64" | "x86" => {
            if arch == "x64" {
                "ok"
            } else {
                "no"
            }
        }
        "arm64-v8a" => {
            if arch == "aarch64" {
                "ok"
            } else {
                "slow"
            }
        }
        "armeabi-v7a" => "slow",
        _ => "ok",
    }
}

/// "36.1" / "34x" / "37.2-beta1" -> algo ordenável.
fn api_sort_key(api: &str) -> f64 {
    let clean: String = api.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    clean.parse().unwrap_or(0.0)
}

/// Pacotes de prévia que o Google publica no canal estável (codinome de versão
/// futura, betas de plataforma...).
fn is_preview_pkg(r: &Remote) -> bool {
    if !r.details.codename.is_empty() {
        return true;
    }
    r.path.split(';').any(|part| {
        ["-alpha", "-beta", "-rc", "-dev"].iter().any(|m| part.rsplit_once(m).map(|(_, rest)| rest.chars().all(|c| c.is_ascii_digit())).unwrap_or(false))
    })
}

fn fill_api(v: &mut PkgView, path: &str, details_api: &str, codename: &str, ext: &str, tag_display: &str, vendor: &str) {
    if v.kind == "sysimg" || v.kind == "platform" {
        v.api = details_api.to_string();
        if v.kind == "sysimg" {
            let parts: Vec<&str> = path.split(';').collect();
            if parts.len() == 4 {
                v.api = parts[1].trim_start_matches("android-").to_string();
                v.tag = parts[2].to_string();
                v.abi = parts[3].to_string();
            }
            v.tag_display = tag_display.to_string();
            v.vendor = vendor.to_string();
            v.play_store = v.tag.contains("playstore");
            v.compat = host_compat(&v.abi).into();
        } else if let Some(a) = path.strip_prefix("platforms;android-") {
            v.api = a.to_string();
        }
        v.ext = ext.to_string();
        v.api_name = api_name(v.api.trim_end_matches('x'), codename);
    }
}

/// Junta catálogo e pacotes instalados numa lista só.
pub fn package_views(sdk: &Sdk, cat: Option<&Catalog>, preview: bool) -> Vec<PkgView> {
    let installed = sdk.installed_map();
    let mut out: Vec<PkgView> = vec![];
    let mut seen = std::collections::HashSet::new();
    let (os, arch) = (platform::host_os(), platform::host_arch());
    if let Some(cat) = cat {
        for r in cat.latest(preview) {
            if !preview && is_preview_pkg(r) {
                continue;
            }
            let mut v = PkgView {
                path: r.path.clone(),
                name: r.name.clone(),
                revision: r.revision.to_string(),
                kind: pkg_type(&r.path).into(),
                channel: r.channel.clone(),
                license: r.license_id.clone(),
                obsolete: r.obsolete,
                ..Default::default()
            };
            if let Some(a) = r.archive_for(os, arch) {
                v.available = true;
                v.size = a.size;
            }
            if let Some(inst) = installed.get(&r.path) {
                v.installed = true;
                v.installed_rev = inst.rev();
                v.update = inst.revision < r.revision;
            }
            let tag_display = if r.details.tags.len() > 1 && !r.details.tags[1].id.is_empty() {
                format!("{} · {}", r.details.tags[0].display, r.details.tags[1].display)
            } else {
                r.details.tag_display()
            };
            fill_api(&mut v, &r.path, &r.details.api_level, &r.details.codename, &r.details.ext_level, &tag_display, &r.details.vendor_display);
            seen.insert(r.path.clone());
            out.push(v);
        }
    }
    // Instalados que o catálogo não conhece (removidos do repositório, manuais...).
    for (p, inst) in &installed {
        if seen.contains(p) {
            continue;
        }
        let mut v = PkgView {
            path: p.clone(),
            name: inst.name.clone(),
            revision: inst.rev(),
            kind: pkg_type(p).into(),
            installed: true,
            installed_rev: inst.rev(),
            channel: "stable".into(),
            ..Default::default()
        };
        fill_api(&mut v, p, "", "", "", &inst.details.tag_display(), "");
        out.push(v);
    }
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| api_sort_key(&b.api).partial_cmp(&api_sort_key(&a.api)).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_detection() {
        let mk = |path: &str, code: &str| {
            let mut r = Remote { path: path.into(), ..Default::default() };
            r.details.codename = code.into();
            r
        };
        assert!(is_preview_pkg(&mk("system-images;android-37.2-beta3;google_apis_ps16k;x86_64", "")));
        assert!(is_preview_pkg(&mk("system-images;android-CinnamonBun;google_apis;x86_64", "CinnamonBun")));
        assert!(!is_preview_pkg(&mk("system-images;android-37.2;google_apis_ps16k;x86_64", "")));
        assert!(is_preview_pkg(&mk("cmdline-tools;19.0-alpha01", "")));
    }

    #[test]
    fn types_and_sorting() {
        assert_eq!(pkg_type("system-images;android-36;google_apis;x86_64"), "sysimg");
        assert_eq!(pkg_type("ndk-bundle"), "ndk");
        assert!(api_sort_key("36.1") > api_sort_key("36"));
        assert_eq!(api_sort_key("34x"), 34.0);
    }
}
