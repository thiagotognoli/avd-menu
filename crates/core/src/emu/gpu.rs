use super::output_with_timeout;
use crate::platform;
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Placa de vídeo detectada.
#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub node: String,
    /// nvidia | amd | intel | other
    pub vendor: String,
    pub driver: String,
    pub name: String,
    /// Endereço PCI (0000:12:00.0).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pci: String,
    /// IDs PCI em hexadecimal, sem “0x” (1002, 7480).
    #[serde(skip)]
    pub vendor_id: String,
    #[serde(skip)]
    pub device_id: String,
    /// Placa que o firmware usou na inicialização — a dos monitores, onde o
    /// OpenGL roda quando ninguém pede outra.
    pub boot_vga: bool,
    /// Integrada ao processador? (None = não deu para saber)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrated: Option<bool>,
}

/// Lista as GPUs com nó de renderização (Linux).
pub fn list_gpus() -> Vec<Gpu> {
    let mut gpus = list_gpus_in(Path::new("/sys/class/drm"));
    if platform::which("lspci").is_some() {
        for g in gpus.iter_mut().filter(|g| !g.pci.is_empty()) {
            if let Some(o) = output_with_timeout(Command::new("lspci").args(["-mm", "-s", &g.pci]), Duration::from_secs(3)) {
                if let Some(nm) = parse_lspci_name(&String::from_utf8_lossy(&o.stdout)) {
                    g.name = nm;
                }
            }
        }
    }
    gpus
}

/// Lê as GPUs de uma árvore no formato do /sys/class/drm (sem consultar o lspci).
pub fn list_gpus_in(drm: &Path) -> Vec<Gpu> {
    let Ok(rd) = std::fs::read_dir(drm) else { return vec![] };
    let mut nodes: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect();
    nodes.sort();
    let mut out = vec![];
    for n in nodes {
        let dev = drm.join(&n).join("device");
        let read = |f: &str| std::fs::read_to_string(dev.join(f)).unwrap_or_default().trim().to_string();
        let hex = |f: &str| read(f).trim_start_matches("0x").to_lowercase();
        let vendor_id = hex("vendor");
        let vendor = match vendor_id.as_str() {
            "10de" => "nvidia",
            "1002" => "amd",
            "8086" => "intel",
            _ => "other",
        };
        let (mut driver, mut pci) = (String::new(), String::new());
        for line in read("uevent").lines() {
            if let Some(v) = line.strip_prefix("DRIVER=") {
                driver = v.to_string();
            }
            if let Some(v) = line.strip_prefix("PCI_SLOT_NAME=") {
                pci = v.to_string();
            }
        }
        let integrated = match vendor {
            "nvidia" => Some(false),
            // a integrada da Intel fica sempre em 00:02.0; as Arc ficam em outro barramento
            "intel" => Some(pci.ends_with("00:02.0")),
            // o amdgpu só cria mem_info_vram_vendor para placas com memória própria
            "amd" if dev.join("mem_info_vram_vendor").exists() => Some(false),
            "amd" if dev.join("mem_info_vram_total").exists() => Some(true),
            _ => None,
        };
        let name = {
            let mut c = vendor.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        };
        out.push(Gpu { node: n, vendor: vendor.into(), driver, name, pci, vendor_id, device_id: hex("device"), boot_vga: read("boot_vga") == "1", integrated });
    }
    out
}

/// Extrai um nome curto (“AMD Radeon RX 7600”) de uma linha de `lspci -mm`:
/// os campos entre aspas são classe, fabricante, dispositivo...
pub fn parse_lspci_name(line: &str) -> Option<String> {
    let mut fields = vec![];
    let mut cur = String::new();
    let mut inq = false;
    for c in line.chars() {
        match c {
            '"' => {
                inq = !inq;
                if !inq {
                    fields.push(std::mem::take(&mut cur));
                }
            }
            c if inq => cur.push(c),
            _ => {}
        }
    }
    if fields.len() < 3 {
        return None;
    }
    let mut vendor = fields[1].clone();
    if vendor.contains("AMD") || vendor.contains("Advanced Micro") {
        vendor = "AMD".into();
    } else if vendor.contains("NVIDIA") {
        vendor = "NVIDIA".into();
    } else if vendor.contains("Intel") {
        vendor = "Intel".into();
    }
    let mut dev = fields[2].as_str();
    // "Navi 33 [Radeon RX 7600/7600 XT]" -> "Radeon RX 7600"
    if let (Some(a), Some(b)) = (dev.rfind('['), dev.rfind(']')) {
        if b > a {
            dev = &dev[a + 1..b];
        }
    }
    if let Some(k) = dev.find('/') {
        if k > 0 {
            dev = &dev[..k];
        }
    }
    Some(format!("{} {}", vendor, dev.trim()).trim().to_string())
}

/// Qual placa é a padrão (a dos monitores) e qual seria a “dedicada” (índices em `gpus`).
pub fn pick(gpus: &[Gpu]) -> (Option<usize>, Option<usize>) {
    if gpus.is_empty() {
        return (None, None);
    }
    let display = gpus.iter().position(|g| g.boot_vga).unwrap_or(0);
    if gpus.len() < 2 || gpus[display].integrated == Some(false) {
        // uma placa só, ou os monitores já estão na dedicada: nada a trocar
        return (Some(display), None);
    }
    let others = || gpus.iter().enumerate().filter(|(i, _)| *i != display);
    let dedicated = others().find(|(_, g)| g.integrated == Some(false)).or_else(|| others().find(|(_, g)| g.integrated.is_none())).map(|(i, _)| i);
    (Some(display), dedicated)
}

/// Variáveis que põem o OpenGL e o Vulkan do emulador na mesma placa (vazio se
/// há uma só, ou fora do Linux). `dedicated` escolhe a placa dedicada; senão, a
/// dos monitores.
///
/// Sem isso o emulador divide o trabalho: o OpenGL ES fica na placa padrão (a
/// integrada, que desenha a tela) e o Vulkan vai para a dedicada. As imagens
/// passam de uma placa para a outra pela CPU e a dedicada fica desligando e
/// religando — travadas e “o aplicativo não está respondendo”. Também não dá
/// para trocar de placa entre uma execução e outra sem cold boot: o snapshot
/// guarda o estado gráfico da placa em que foi salvo.
pub fn gpu_env(dedicated: bool) -> Vec<(String, String)> {
    if !platform::is_linux() {
        return vec![];
    }
    gpu_env_for(&list_gpus(), dedicated)
}

/// `gpu_env` para uma lista já conhecida.
pub fn gpu_env_for(gpus: &[Gpu], dedicated: bool) -> Vec<(String, String)> {
    if gpus.len() < 2 {
        return vec![];
    }
    let (display, ded) = pick(gpus);
    let Some(target) = (if dedicated { ded } else { display }) else { return vec![] };
    let g = &gpus[target];
    let kv = |k: &str, v: &str| (k.to_string(), v.to_string());
    if g.driver == "nvidia" {
        if target == display.unwrap_or(target) {
            return vec![]; // a NVIDIA já é a padrão e o emulador escolhe a dedicada para o Vulkan
        }
        // Não dá para juntar com as variáveis do Mesa: em máquina AMD/Intel,
        // __GLX_VENDOR_LIBRARY_NAME=nvidia faz o libglvnd procurar libGLX_nvidia.so.0.
        return vec![kv("__NV_PRIME_RENDER_OFFLOAD", "1"), kv("__GLX_VENDOR_LIBRARY_NAME", "nvidia"), kv("__VK_LAYER_NV_optimus", "NVIDIA_only")];
    }
    let mut env = vec![];
    // OpenGL (Mesa): o mesmo formato do ID_PATH_TAG do udev
    if !g.pci.is_empty() {
        env.push(kv("DRI_PRIME", &format!("pci-{}", g.pci.replace([':', '.'], "_"))));
    } else if dedicated {
        env.push(kv("DRI_PRIME", "1"));
    }
    // Vulkan: a camada device-select do Mesa mostra só esta placa ao emulador
    if !g.vendor_id.is_empty() && !g.device_id.is_empty() {
        env.push(kv("MESA_VK_DEVICE_SELECT", &format!("{}:{}", g.vendor_id, g.device_id)));
        env.push(kv("MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE", "1"));
    }
    if gpus.iter().any(|o| o.driver == "nvidia") {
        env.push(kv("__VK_LAYER_NV_optimus", "non_NVIDIA_only"));
    }
    env
}

/// Variáveis para iniciar na placa dedicada (vazio se não há uma que valha trocar).
pub fn dedicated_gpu_env() -> Vec<(String, String)> {
    gpu_env(true)
}

/// O modo “automático” do emulador (-gpu auto) cai para renderização por
/// software (SwiftShader/lavapipe — lento, a janela “não responde”) em muitos
/// sistemas Linux com GPU perfeitamente boa: ele imprime “Your GPU drivers may
/// have a bug. Switching to software rendering.” e só usa a GPU com -gpu host.
/// Esta função diz se vale forçar `-gpu host`: há nó de renderização e um driver
/// Vulkan/GL de hardware instalado (Mesa radv/anv, NVIDIA proprietário...).
pub fn prefer_host_gpu() -> bool {
    if !platform::is_linux() {
        return false;
    }
    let has_render_node = std::fs::read_dir("/dev/dri").map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("renderD"))).unwrap_or(false);
    if !has_render_node {
        return false;
    }
    if Path::new("/proc/driver/nvidia").exists() {
        return true;
    }
    for dir in ["/usr/share/vulkan/icd.d", "/etc/vulkan/icd.d", "/usr/local/share/vulkan/icd.d"] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if (n.starts_with("radeon_icd") || n.starts_with("amd_icd") || n.starts_with("intel_icd") || n.starts_with("nvidia_icd"))
                    && n.ends_with(".json")
                {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lspci_names() {
        let cases = [
            (
                r#"0000:12:00.0 "VGA compatible controller" "Advanced Micro Devices, Inc. [AMD/ATI]" "Navi 33 [Radeon RX 7600/7600 XT/7600M XT]" -rc1 "Device 1eae" "Device 7605""#,
                Some("AMD Radeon RX 7600"),
            ),
            (r#"01:00.0 "VGA compatible controller" "NVIDIA Corporation" "AD104 [GeForce RTX 4070]" -ra1"#, Some("NVIDIA GeForce RTX 4070")),
            (r#"00:02.0 "VGA compatible controller" "Intel Corporation" "Alder Lake-P GT2 [Iris Xe Graphics]" -r0c"#, Some("Intel Iris Xe Graphics")),
            ("lixo", None),
        ];
        for (i, w) in cases {
            assert_eq!(parse_lspci_name(i).as_deref(), w, "{i}");
        }
    }

    /// (renderD, vendor, device, slot, boot_vga, driver, arquivos extras)
    type Card<'a> = (&'a str, &'a str, &'a str, &'a str, bool, &'a str, &'a [&'a str]);

    /// Monta um /sys/class/drm falso.
    fn fake_drm(name: &str, cards: &[Card]) -> std::path::PathBuf {
        let root = crate::testutil::tmp(name);
        for (node, vendor, device, slot, boot, driver, extra) in cards {
            let dev = root.join(format!("pci/{slot}"));
            std::fs::create_dir_all(&dev).unwrap();
            std::fs::write(dev.join("vendor"), format!("0x{vendor}\n")).unwrap();
            std::fs::write(dev.join("device"), format!("0x{device}\n")).unwrap();
            std::fs::write(dev.join("boot_vga"), if *boot { "1\n" } else { "0\n" }).unwrap();
            std::fs::write(dev.join("uevent"), format!("DRIVER={driver}\nPCI_SLOT_NAME={slot}\n")).unwrap();
            for f in *extra {
                std::fs::write(dev.join(f), "1\n").unwrap();
            }
            std::fs::create_dir_all(root.join(node)).unwrap();
            std::os::unix::fs::symlink(&dev, root.join(node).join("device")).unwrap();
        }
        root
    }

    fn env_map(v: Vec<(String, String)>) -> std::collections::BTreeMap<String, String> {
        v.into_iter().collect()
    }

    #[test]
    fn amd_desktop_with_monitors_on_the_apu() {
        // a máquina que motivou isto: Ryzen 5700G (monitores) + RX 7600 XT
        let root = fake_drm(
            "gpu-amd-apu",
            &[
                ("renderD128", "1002", "7480", "0000:12:00.0", false, "amdgpu", &["mem_info_vram_total", "mem_info_vram_vendor"]),
                ("renderD129", "1002", "1638", "0000:4c:00.0", true, "amdgpu", &["mem_info_vram_total"]),
            ],
        );
        let gpus = list_gpus_in(&root);
        assert_eq!(gpus.len(), 2);
        assert_eq!((gpus[0].integrated, gpus[1].integrated), (Some(false), Some(true)));
        assert_eq!(pick(&gpus), (Some(1), Some(0)));

        let def = env_map(gpu_env_for(&gpus, false));
        assert_eq!(def.get("DRI_PRIME").map(String::as_str), Some("pci-0000_4c_00_0"));
        assert_eq!(def.get("MESA_VK_DEVICE_SELECT").map(String::as_str), Some("1002:1638"));
        assert_eq!(def.get("MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE").map(String::as_str), Some("1"));

        let ded = env_map(gpu_env_for(&gpus, true));
        assert_eq!(ded.get("DRI_PRIME").map(String::as_str), Some("pci-0000_12_00_0"));
        assert_eq!(ded.get("MESA_VK_DEVICE_SELECT").map(String::as_str), Some("1002:7480"));
        assert!(!ded.contains_key("__VK_LAYER_NV_optimus"));
    }

    #[test]
    fn monitors_on_the_dedicated_card() {
        // os monitores já estão na dedicada: não há “placa dedicada” para oferecer
        let root = fake_drm(
            "gpu-amd-dgpu-display",
            &[
                ("renderD128", "1002", "7480", "0000:12:00.0", true, "amdgpu", &["mem_info_vram_total", "mem_info_vram_vendor"]),
                ("renderD129", "1002", "1638", "0000:4c:00.0", false, "amdgpu", &["mem_info_vram_total"]),
            ],
        );
        let gpus = list_gpus_in(&root);
        assert_eq!(pick(&gpus), (Some(0), None));
        assert!(gpu_env_for(&gpus, true).is_empty());
        // mas o Vulkan continua preso à placa dos monitores
        assert_eq!(env_map(gpu_env_for(&gpus, false)).get("MESA_VK_DEVICE_SELECT").map(String::as_str), Some("1002:7480"));
    }

    #[test]
    fn intel_laptop_with_nvidia() {
        let root = fake_drm(
            "gpu-intel-nvidia",
            &[("renderD128", "8086", "a7a0", "0000:00:02.0", true, "i915", &[]), ("renderD129", "10de", "28e0", "0000:01:00.0", false, "nvidia", &[])],
        );
        let gpus = list_gpus_in(&root);
        assert_eq!((gpus[0].integrated, gpus[1].integrated), (Some(true), Some(false)));
        let ded = env_map(gpu_env_for(&gpus, true));
        assert_eq!(ded.get("__NV_PRIME_RENDER_OFFLOAD").map(String::as_str), Some("1"));
        assert_eq!(ded.get("__VK_LAYER_NV_optimus").map(String::as_str), Some("NVIDIA_only"));
        assert!(!ded.contains_key("DRI_PRIME"));
        // na integrada, o Vulkan da NVIDIA fica de fora
        let def = env_map(gpu_env_for(&gpus, false));
        assert_eq!(def.get("MESA_VK_DEVICE_SELECT").map(String::as_str), Some("8086:a7a0"));
        assert_eq!(def.get("__VK_LAYER_NV_optimus").map(String::as_str), Some("non_NVIDIA_only"));
        assert!(!def.contains_key("__GLX_VENDOR_LIBRARY_NAME"));
    }

    #[test]
    fn single_gpu_needs_nothing() {
        let root = fake_drm("gpu-single", &[("renderD128", "1002", "7480", "0000:12:00.0", true, "amdgpu", &["mem_info_vram_vendor"])]);
        let gpus = list_gpus_in(&root);
        assert!(gpu_env_for(&gpus, false).is_empty());
        assert!(gpu_env_for(&gpus, true).is_empty());
    }
}
