//! Perfis de hardware (Pixel, Nexus, Wear OS, TV, Automotive...) — os mesmos do
//! Android Studio. `data/devices.json` é gerado por `cargo xtask gen-devices` a
//! partir dos XMLs do sdklib (AOSP, Apache 2.0) e do próprio avdmanager, que
//! serve de referência para as chaves hw.* de cada aparelho.

use crate::{config, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub manufacturer: String,
    /// phone, tablet, wear, tv, automotive, desktop, xr
    pub category: String,
    pub deprecated: bool,
    pub playstore: bool,
    pub diag: f64,
    pub w: i32,
    pub h: i32,
    pub density: i32,
    #[serde(rename = "ramMB")]
    pub ram_mb: i32,
    #[serde(rename = "deviceRamMB")]
    pub device_ram_mb: i32,
    pub sensors: Vec<String>,
    pub front: bool,
    pub back: bool,
    pub foldable: bool,
    pub round: bool,
    pub skin: Option<String>,
    pub tags: Vec<String>,
    pub abis: Vec<String>,
    /// Diferenças do config.ini em relação ao padrão (null = chave ausente).
    #[serde(skip_serializing)]
    pub cfg: BTreeMap<String, Option<String>>,
    pub user: bool,
}

#[derive(Deserialize)]
struct File {
    defaults: BTreeMap<String, String>,
    devices: Vec<Device>,
}

fn data() -> &'static File {
    static D: OnceLock<File> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(include_str!("../data/devices.json")).expect("devices.json inválido"))
}

/// Dispositivos que acompanham o SDK.
pub fn builtin() -> Vec<Device> {
    data().devices.clone()
}

/// O config.ini base que o avdmanager gera.
pub fn defaults() -> BTreeMap<String, String> {
    data().defaults.clone()
}

/// Embutidos + criados pelo usuário.
pub fn all() -> Vec<Device> {
    let mut v = builtin();
    v.extend(users());
    v
}

pub fn get(id: &str) -> Option<Device> {
    if let Some(d) = data().devices.iter().find(|d| d.id == id) {
        return Some(d.clone());
    }
    users().into_iter().find(|d| d.id == id)
}

impl Device {
    /// Chaves do config.ini que o dispositivo define (padrão + diferenças do aparelho).
    pub fn hw_config(&self) -> BTreeMap<String, String> {
        let mut m = defaults();
        for (k, v) in &self.cfg {
            match v {
                None => {
                    m.remove(k);
                }
                Some(v) => {
                    m.insert(k.clone(), v.clone());
                }
            }
        }
        if self.user {
            m.insert("hw.lcd.width".into(), self.w.to_string());
            m.insert("hw.lcd.height".into(), self.h.to_string());
            m.insert("hw.lcd.density".into(), self.density.to_string());
            m.insert("hw.ramSize".into(), self.ram_mb.to_string());
            m.insert("hw.sdCard".into(), "yes".into());
        }
        m.insert("hw.device.name".into(), self.id.clone());
        if !m.contains_key("hw.device.manufacturer") || self.user {
            m.insert("hw.device.manufacturer".into(), self.manufacturer.clone());
        }
        // A câmera do avdmanager é sempre "none" na frente; o Studio usa a câmera
        // emulada quando o aparelho tem, e a cena virtual na traseira.
        m.insert("hw.camera.back".into(), if self.back { "virtualscene" } else { "none" }.into());
        m.insert("hw.camera.front".into(), if self.front { "emulated" } else { "none" }.into());
        m
    }

    /// Informa se uma imagem com esta tag (google_apis, android-wear...) serve
    /// para o dispositivo — o mesmo filtro da etapa “System Image” do Studio.
    pub fn tag_allowed(&self, tag: &str) -> bool {
        match self.category.as_str() {
            "wear" => return tag.starts_with("android-wear"),
            "tv" => return tag == "android-tv" || tag == "google-tv",
            "automotive" => {
                if !self.tags.is_empty() {
                    return self.tags.iter().any(|t| tag == t || tag.starts_with(t.as_str()));
                }
                return tag.starts_with("android-automotive");
            }
            "desktop" => return tag == "android-desktop",
            "xr" => return tag.contains("xr") || tag.contains("glasses"),
            _ => {}
        }
        // Celulares e tablets.
        matches!(
            tag,
            "default"
                | "google_apis"
                | "google_apis_playstore"
                | "google_atd"
                | "aosp_atd"
                | "google_apis_tablet"
                | "aosp_tablet"
                | "google_playstore_tablet"
                | "android-desktop-phone"
        ) || tag.starts_with("google_apis")
    }
}

// ---- dispositivos do usuário ------------------------------------------------------------

/// Perfis criados pelo usuário.
pub fn users() -> Vec<Device> {
    config::load()
        .user_devices
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Device>(v).ok())
        .filter(|d| !d.id.is_empty())
        .map(|mut d| {
            d.user = true;
            d
        })
        .collect()
}

/// Valida e grava um perfil de hardware do usuário (novo ou existente).
pub fn save_user(mut d: Device) -> Result<Device> {
    d.name = d.name.trim().to_string();
    if d.name.is_empty() {
        return Err(bad!("informe um nome para o dispositivo", "enter a name for the device"));
    }
    if d.w < 100 || d.h < 100 || d.w > 10000 || d.h > 10000 {
        return Err(bad!("resolução inválida ({}x{})", "invalid resolution ({}x{})", d.w, d.h));
    }
    if d.density < 100 || d.density > 1000 {
        return Err(bad!("densidade inválida ({} dpi)", "invalid density ({} dpi)", d.density));
    }
    if d.ram_mb < 128 {
        d.ram_mb = 2048;
    }
    if d.id.is_empty() {
        let mut slug = String::new();
        let mut us = true;
        for c in d.name.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
                us = false;
            } else if !us {
                slug.push('_');
                us = true;
            }
        }
        d.id = format!("user_{}", slug.trim_matches('_'));
    }
    if data().devices.iter().any(|b| b.id == d.id) {
        return Err(bad!("já existe um dispositivo embutido com o ID {:?}", "a built-in device with ID {:?} already exists", d.id));
    }
    if d.category.is_empty() {
        d.category = "phone".into();
    }
    if d.manufacturer.is_empty() {
        d.manufacturer = "User".into();
    }
    if d.diag <= 0.0 {
        d.diag = (((d.w as f64).hypot(d.h as f64) / d.density as f64) * 100.0).round() / 100.0;
    }
    d.user = true;
    d.sensors.clear();
    let val = serde_json::to_value(&d)?;
    config::update(|c| {
        let mut replaced = false;
        for r in c.user_devices.iter_mut() {
            if r.get("id").and_then(|v| v.as_str()) == Some(d.id.as_str()) {
                *r = val.clone();
                replaced = true;
            }
        }
        if !replaced {
            c.user_devices.push(val.clone());
        }
    })
    .map_err(|e| Error::internal(e.to_string()))?;
    Ok(d)
}

/// Remove um perfil do usuário.
pub fn delete_user(id: &str) -> Result<()> {
    config::update(|c| c.user_devices.retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(id))).map_err(|e| Error::internal(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog() {
        let all = builtin();
        assert!(all.len() >= 80, "poucos dispositivos: {}", all.len());
        let mut seen = std::collections::HashSet::new();
        for d in &all {
            assert!(!d.id.is_empty() && !d.name.is_empty() && d.w > 0 && d.h > 0 && d.density > 0 && !d.category.is_empty(), "{d:?}");
            assert!(seen.insert(d.id.clone()), "ID repetido: {}", d.id);
        }
        let p9 = get("pixel_9").unwrap();
        assert_eq!((p9.w, p9.h, p9.density), (1080, 2424, 420));
        assert!(p9.playstore);
        assert_eq!(p9.category, "phone");
    }

    #[test]
    fn hw_config() {
        let d = get("pixel_9").unwrap();
        let c = d.hw_config();
        for (k, v) in [
            ("hw.lcd.width", "1080"),
            ("hw.lcd.height", "2424"),
            ("hw.lcd.density", "420"),
            ("hw.device.name", "pixel_9"),
            ("hw.camera.back", "virtualscene"),
            ("hw.camera.front", "emulated"),
        ] {
            assert_eq!(c[k], v, "{k}");
        }
        let fold = get("pixel_9_pro_fold").unwrap().hw_config();
        assert_eq!(fold["hw.sensor.hinge"], "yes");
        assert_eq!(fold["hw.sensor.hinge.count"], "1");
    }

    #[test]
    fn tag_allowed() {
        let phone = get("pixel_9").unwrap();
        let wear = get("wearos_small_round").unwrap();
        let tv = get("tv_1080p").unwrap();
        let auto = get("automotive_1024p_landscape").unwrap();
        for (d, tag, want) in [
            (&phone, "google_apis", true),
            (&phone, "google_apis_playstore", true),
            (&phone, "default", true),
            (&phone, "google_apis_ps16k", true),
            (&phone, "android-wear", false),
            (&phone, "android-tv", false),
            (&phone, "android-automotive", false),
            (&wear, "android-wear", true),
            (&wear, "android-wear-signed", true),
            (&wear, "google_apis", false),
            (&tv, "android-tv", true),
            (&tv, "google-tv", true),
            (&tv, "google_apis", false),
            (&auto, "android-automotive-playstore", true),
            (&auto, "google_apis", false),
        ] {
            assert_eq!(d.tag_allowed(tag), want, "{} com {}", d.id, tag);
        }
    }
}
