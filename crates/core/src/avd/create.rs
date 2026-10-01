//! Criação e edição de AVDs: o config.ini é montado com as mesmas chaves que o
//! avdmanager gera para cada aparelho + as da imagem de sistema + os ajustes.

use super::ini::Ini;
use super::{avd_dir, home, load, parse_mb, skin, valid_name, write_avd_ini, Avd};
use crate::devices::{self, Device};
use crate::sdk::{Sdk, SysImageProps};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Opções editáveis de um AVD (etapa “Verify Configuration” / “Show Advanced
/// Settings” do Android Studio). Campos ausentes = não mexer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub display_name: Option<String>,
    /// portrait | landscape
    pub orientation: Option<String>,
    #[serde(rename = "ramMB")]
    pub ram_mb: Option<i32>,
    #[serde(rename = "vmHeapMB")]
    pub vm_heap_mb: Option<i32>,
    pub cores: Option<i32>,
    /// armazenamento interno
    #[serde(rename = "dataMB")]
    pub data_mb: Option<i32>,
    /// 0 = sem cartão SD
    #[serde(rename = "sdCardMB")]
    pub sd_card_mb: Option<i32>,
    /// none | emulated | webcam0
    pub camera_front: Option<String>,
    /// none | emulated | virtualscene | webcam0
    pub camera_back: Option<String>,
    pub net_speed: Option<String>,
    pub net_latency: Option<String>,
    /// auto | host | swiftshader_indirect | software ...
    pub gpu_mode: Option<String>,
    /// quick | cold
    pub boot_mode: Option<String>,
    pub keyboard: Option<bool>,
    pub show_frame: Option<bool>,
    /// Sobrescreve chaves arbitrárias do config.ini (null apaga).
    pub extra: BTreeMap<String, Option<String>>,
}

fn yesno(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

impl Settings {
    /// Grava as opções no config.ini em memória.
    pub fn apply(&self, cfg: &mut Ini) -> Result<()> {
        if let Some(n) = &self.display_name {
            let n = n.trim();
            if n.is_empty() {
                return Err(bad!("o nome do AVD não pode ficar vazio", "the AVD name cannot be empty"));
            }
            cfg.set("avd.ini.displayname", n);
        }
        if let Some(o) = &self.orientation {
            if o != "portrait" && o != "landscape" {
                return Err(bad!("orientação inválida: {:?}", "invalid orientation: {:?}", o));
            }
            cfg.set("hw.initialOrientation", o);
        }
        if let Some(v) = self.ram_mb {
            if !(256..=65536).contains(&v) {
                return Err(bad!("RAM deve ficar entre 256 MB e 64 GB", "RAM must be between 256 MB and 64 GB"));
            }
            cfg.set("hw.ramSize", &v.to_string());
        }
        if let Some(v) = self.vm_heap_mb {
            if !(16..=4096).contains(&v) {
                return Err(bad!("heap da VM deve ficar entre 16 MB e 4096 MB", "VM heap must be between 16 MB and 4096 MB"));
            }
            cfg.set("vm.heapSize", &v.to_string());
        }
        if let Some(v) = self.cores {
            if !(1..=64).contains(&v) {
                return Err(bad!("núcleos deve ficar entre 1 e 64", "cores must be between 1 and 64"));
            }
            cfg.set("hw.cpu.ncore", &v.to_string());
        }
        if let Some(v) = self.data_mb {
            if v < 512 {
                return Err(bad!("armazenamento interno mínimo: 512 MB", "minimum internal storage: 512 MB"));
            }
            cfg.set("disk.dataPartition.size", &format!("{v}M"));
        }
        if let Some(v) = self.sd_card_mb {
            if v <= 0 {
                cfg.set("hw.sdCard", "no");
                cfg.delete("sdcard.size");
            } else {
                cfg.set("hw.sdCard", "yes");
                cfg.set("sdcard.size", &format!("{v} MB"));
            }
        }
        if let Some(v) = &self.camera_front {
            cfg.set("hw.camera.front", v);
        }
        if let Some(v) = &self.camera_back {
            cfg.set("hw.camera.back", v);
        }
        if let Some(v) = &self.net_speed {
            cfg.set("runtime.network.speed", v);
        }
        if let Some(v) = &self.net_latency {
            cfg.set("runtime.network.latency", v);
        }
        if let Some(mode) = &self.gpu_mode {
            if !matches!(mode.as_str(), "auto" | "host" | "swiftshader_indirect" | "software" | "angle_indirect" | "guest" | "mesa") {
                return Err(bad!("modo gráfico inválido: {:?}", "invalid graphics mode: {:?}", mode));
            }
            cfg.set("hw.gpu.enabled", yesno(mode != "software" && mode != "guest"));
            cfg.set("hw.gpu.mode", mode);
        }
        if let Some(b) = &self.boot_mode {
            let cold = b == "cold";
            cfg.set("fastboot.forceColdBoot", yesno(cold));
            cfg.set("fastboot.forceFastBoot", yesno(!cold));
        }
        if let Some(v) = self.keyboard {
            cfg.set("hw.keyboard", yesno(v));
        }
        if let Some(v) = self.show_frame {
            cfg.set("showDeviceFrame", yesno(v));
        }
        for (k, v) in &self.extra {
            if k.is_empty() || k.contains(['=', '\n', '\r', ' ']) {
                return Err(bad!("chave inválida: {:?}", "invalid key: {:?}", k));
            }
            match v {
                None => cfg.delete(k),
                Some(v) => {
                    if v.contains(['\n', '\r']) {
                        return Err(bad!("valor inválido para {}", "invalid value for {}", k));
                    }
                    cfg.set(k, v);
                }
            }
        }
        Ok(())
    }
}

/// Valores atuais, no formato que o formulário de edição usa.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub display_name: String,
    pub orientation: String,
    #[serde(rename = "ramMB")]
    pub ram_mb: i32,
    #[serde(rename = "vmHeapMB")]
    pub vm_heap_mb: i32,
    pub cores: i32,
    #[serde(rename = "dataMB")]
    pub data_mb: i32,
    #[serde(rename = "sdCardMB")]
    pub sd_card_mb: i32,
    pub camera_front: String,
    pub camera_back: String,
    pub net_speed: String,
    pub net_latency: String,
    pub gpu_mode: String,
    pub boot_mode: String,
    pub keyboard: bool,
    pub show_frame: bool,
}

fn or_default(v: &str, def: &str) -> String {
    if v.is_empty() { def } else { v }.to_string()
}

pub fn view_of(cfg: &Ini, name: &str) -> View {
    let cores: i32 = cfg.get("hw.cpu.ncore").parse().unwrap_or(0);
    View {
        display_name: or_default(cfg.get("avd.ini.displayname"), &name.replace('_', " ")),
        orientation: or_default(cfg.get("hw.initialOrientation"), "portrait"),
        ram_mb: parse_mb(cfg.get("hw.ramSize")),
        vm_heap_mb: parse_mb(cfg.get("vm.heapSize")),
        cores: if cores == 0 { 1 } else { cores },
        data_mb: parse_mb(cfg.get("disk.dataPartition.size")),
        sd_card_mb: if cfg.get("hw.sdCard") != "no" { parse_mb(cfg.get("sdcard.size")) } else { 0 },
        camera_front: or_default(cfg.get("hw.camera.front"), "none"),
        camera_back: or_default(cfg.get("hw.camera.back"), "none"),
        net_speed: or_default(cfg.get("runtime.network.speed"), "full"),
        net_latency: or_default(cfg.get("runtime.network.latency"), "none"),
        gpu_mode: or_default(cfg.get("hw.gpu.mode"), "auto"),
        boot_mode: if cfg.get("fastboot.forceColdBoot") == "yes" { "cold" } else { "quick" }.to_string(),
        keyboard: cfg.get("hw.keyboard") == "yes",
        show_frame: cfg.get("showDeviceFrame") == "yes",
    }
}

/// Altera as opções de um AVD existente. A moldura só é ligada se a skin já está no SDK
/// (`skin::download`, que usa a rede, fica a cargo de quem chama).
pub fn update(sdk: &Sdk, name: &str, st: &Settings) -> Result<()> {
    let dir = avd_dir(name)?;
    let p = dir.join("config.ini");
    let mut cfg = Ini::read(&p)?;
    st.apply(&mut cfg)?;
    skin::sync(&mut cfg, sdk);
    std::fs::write(&p, cfg.to_text())?;
    Ok(())
}

/// Baixa a moldura que o AVD vai precisar depois de aplicar `st` (se faltar).
pub fn fetch_skin_for_update(sdk: &Sdk, name: &str, st: &Settings) -> Result<()> {
    let (mut cfg, _) = super::config(name)?;
    st.apply(&mut cfg)?;
    match skin::missing(&cfg, sdk) {
        Some(id) => skin::download(sdk, &id),
        None => Ok(()),
    }
}

/// Baixa a moldura do aparelho de um AVD novo (se ele tem uma e a moldura está ligada).
pub fn fetch_skin_for_create(sdk: &Sdk, spec: &CreateSpec) -> Result<()> {
    if spec.settings.show_frame == Some(false) {
        return Ok(());
    }
    match devices::get(&spec.device_id).and_then(|d| d.skin) {
        Some(id) if !skin::installed(sdk, &id) => skin::download(sdk, &id),
        _ => Ok(()),
    }
}

/// Substitui o config.ini inteiro (editor avançado), guardando um backup em config.ini.bak.
pub fn write_raw_config(name: &str, content: &str) -> Result<()> {
    let dir = avd_dir(name)?;
    let p = dir.join("config.ini");
    if let Ok(old) = std::fs::read(&p) {
        let _ = std::fs::write(dir.join("config.ini.bak"), old);
    }
    if Ini::parse(content).get("image.sysdir.1").is_empty() {
        return Err(bad!("o config.ini precisa manter a chave image.sysdir.1", "config.ini must keep the image.sysdir.1 key"));
    }
    std::fs::write(&p, content.replace("\r\n", "\n"))?;
    Ok(())
}

/// Descrição de um AVD novo.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateSpec {
    /// identificador (sem espaços)
    pub name: String,
    /// id do perfil de hardware
    pub device_id: String,
    /// system-images;android-36;google_apis;x86_64
    pub image_pkg: String,
    #[serde(flatten)]
    pub settings: Settings,
}

fn cpu_arch(abi: &str) -> &str {
    match abi {
        "arm64-v8a" => "arm64",
        "armeabi-v7a" => "arm",
        o => o, // x86, x86_64
    }
}

/// Monta o config.ini de um AVD novo.
pub fn build_config(dev: &Device, img: &SysImageProps, spec: &CreateSpec) -> Result<Ini> {
    let mut cfg = Ini::new();
    for (k, v) in dev.hw_config() {
        cfg.set(&k, &v);
    }
    let mut display = spec.name.replace('_', " ");
    if let Some(d) = &spec.settings.display_name {
        if !d.trim().is_empty() {
            display = d.trim().to_string();
        }
    }
    cfg.set("AvdId", &spec.name);
    cfg.set("avd.ini.displayname", &display);
    cfg.set("avd.ini.encoding", "UTF-8");
    cfg.set("abi.type", &img.abi);
    cfg.set("hw.cpu.arch", cpu_arch(&img.abi));
    cfg.set("image.sysdir.1", &img.dir);
    cfg.set("tag.id", &img.tag_id);
    cfg.set("tag.ids", &img.tag_id);
    let disp = if img.tag_display.is_empty() { &img.tag_id } else { &img.tag_display };
    cfg.set("tag.display", disp);
    cfg.set("tag.displaynames", disp);
    cfg.set("PlayStore.enabled", &img.play_store.to_string());
    cfg.set("hw.gpu.enabled", "yes");
    cfg.set("hw.gpu.mode", "auto");
    cfg.set("showDeviceFrame", "yes");
    // Armazenamento interno padrão do Studio (6 GB, esparso no disco). O avdmanager usa
    // 10 GB, e o emulador recusa iniciar sem 1,2x esse valor livre.
    cfg.set("disk.dataPartition.size", "6G");
    cfg.set("fastboot.forceColdBoot", "no");
    cfg.set("fastboot.forceFastBoot", "yes");
    if matches!(dev.category.as_str(), "wear" | "tv" | "automotive") {
        // aparelhos sem tela de toque de celular usam o teclado do PC
        cfg.set("hw.keyboard", "yes");
    }
    // target é a pasta da API (android-36); o avdmanager repete no config.ini.
    cfg.set("target", &img.target);
    spec.settings.apply(&mut cfg)?;
    // O emulador prefere a RAM sem unidade ambígua.
    let ram = parse_mb(cfg.get("hw.ramSize"));
    if ram > 0 {
        cfg.set("hw.ramSize", &ram.to_string());
    }
    let heap = parse_mb(cfg.get("vm.heapSize"));
    if heap > 0 {
        cfg.set("vm.heapSize", &heap.to_string());
    }
    Ok(cfg)
}

/// Cria o AVD em disco e devolve o resultado lido de volta.
pub fn create(sdk: &Sdk, spec: &CreateSpec) -> Result<Avd> {
    if !valid_name(&spec.name) {
        return Err(bad!(
            "nome inválido {:?}: use letras, números, ponto, hífen e sublinhado (sem espaços)",
            "invalid name {:?}: use letters, digits, dot, dash and underscore (no spaces)",
            spec.name
        ));
    }
    let dev = devices::get(&spec.device_id).ok_or_else(|| bad!("dispositivo desconhecido: {:?}", "unknown device: {:?}", spec.device_id))?;
    let img = sdk
        .read_sys_image(&spec.image_pkg)
        .ok_or_else(|| bad!("a imagem de sistema {:?} não está instalada", "system image {:?} is not installed", spec.image_pkg))?;
    if !dev.tag_allowed(&img.tag_id) {
        return Err(bad!("a imagem {} não combina com o dispositivo {}", "image {} does not fit device {}", img.tag_id, dev.name));
    }
    let h = home();
    std::fs::create_dir_all(&h)?;
    let ini = h.join(format!("{}.ini", spec.name));
    let dir = h.join(format!("{}.avd", spec.name));
    if ini.exists() {
        return Err(bad!("já existe um AVD chamado {:?}", "an AVD named {:?} already exists", spec.name));
    }
    if dir.exists() {
        return Err(bad!("a pasta {} já existe", "the folder {} already exists", dir.display()));
    }
    let mut cfg = build_config(&dev, &img, spec)?;
    skin::sync(&mut cfg, sdk);
    std::fs::create_dir_all(&dir)?;
    if let Err(e) = std::fs::write(dir.join("config.ini"), cfg.to_sorted_text()) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e.into());
    }
    if let Err(e) = write_avd_ini(&spec.name, &dir, &img.target) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(load(sdk, &spec.name))
}

#[allow(dead_code)]
fn _assert_error_is_send(_: Error) {}
