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
}

/// Lista as GPUs com nó de renderização (Linux).
pub fn list_gpus() -> Vec<Gpu> {
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return vec![] };
    let mut nodes: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect();
    nodes.sort();
    let mut out = vec![];
    for n in nodes {
        let dev = Path::new("/sys/class/drm").join(&n).join("device");
        let vendor = match std::fs::read_to_string(dev.join("vendor")).unwrap_or_default().trim() {
            "0x10de" => "nvidia",
            "0x1002" => "amd",
            "0x8086" => "intel",
            _ => "other",
        };
        let (mut driver, mut slot) = (String::new(), String::new());
        for line in std::fs::read_to_string(dev.join("uevent")).unwrap_or_default().lines() {
            if let Some(v) = line.strip_prefix("DRIVER=") {
                driver = v.to_string();
            }
            if let Some(v) = line.strip_prefix("PCI_SLOT_NAME=") {
                slot = v.to_string();
            }
        }
        let mut name = {
            let mut c = vendor.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        };
        if !slot.is_empty() && platform::which("lspci").is_some() {
            if let Some(o) = output_with_timeout(Command::new("lspci").args(["-mm", "-s", &slot]), Duration::from_secs(3)) {
                if let Some(nm) = parse_lspci_name(&String::from_utf8_lossy(&o.stdout)) {
                    name = nm;
                }
            }
        }
        out.push(Gpu { node: n, vendor: vendor.into(), driver, name });
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

/// Variáveis que fazem o OpenGL/Vulkan usar a GPU dedicada em notebooks híbridos
/// (vazio se só há uma GPU, ou fora do Linux).
///
/// O emulador já escolhe a GPU discreta para o Vulkan, mas o caminho OpenGL ES
/// continua na GPU padrão (a integrada) — daí a diferença de desempenho. São as
/// mesmas variáveis que o switcheroo-control usa. Não dá para juntar as duas
/// famílias: em máquina AMD/Intel, definir __GLX_VENDOR_LIBRARY_NAME=nvidia faz
/// o libglvnd procurar libGLX_nvidia.so.0 e quebrar o GLX.
pub fn dedicated_gpu_env() -> Vec<(String, String)> {
    if !platform::is_linux() {
        return vec![];
    }
    let gpus = list_gpus();
    if gpus.len() < 2 {
        return vec![];
    }
    let kv = |k: &str, v: &str| (k.to_string(), v.to_string());
    if gpus.iter().any(|g| g.vendor == "nvidia") {
        return vec![kv("__NV_PRIME_RENDER_OFFLOAD", "1"), kv("__GLX_VENDOR_LIBRARY_NAME", "nvidia"), kv("__VK_LAYER_NV_optimus", "NVIDIA_only")];
    }
    vec![kv("DRI_PRIME", "1")]
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
}
