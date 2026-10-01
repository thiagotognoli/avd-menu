use super::{gpu, output_with_timeout};
use crate::platform;
use crate::sdk::{human_bytes, Sdk};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Resultado de uma verificação de ambiente. O texto exibido vem da interface
/// (chaves diag.<id>.*); aqui só vão o identificador e os dados.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    /// ok | warn | error | info
    pub level: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    /// comando sugerido
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fix: String,
}

fn chk(id: &str, level: &str, args: &[(&str, String)], fix: &str) -> Check {
    Check { id: id.into(), level: level.into(), args: args.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), fix: fix.into() }
}

/// Verifica o que o emulador precisa para rodar bem.
pub fn diagnose(sdk: &Sdk) -> Vec<Check> {
    let mut out = vec![];
    out.push(chk("sdk", if sdk.exists() { "ok" } else { "warn" }, &[("path", sdk.root.to_string_lossy().into_owned())], ""));
    let inst = sdk.installed_map();
    let em = inst.get("emulator");
    if em.is_some() || sdk.has_emulator() {
        out.push(chk("emulator", "ok", &[("version", em.map(|e| e.rev()).unwrap_or_default())], ""));
    } else {
        out.push(chk("emulator", "warn", &[], ""));
    }
    let pt = inst.get("platform-tools");
    if pt.is_some() || sdk.has_adb() {
        out.push(chk("adb", "ok", &[("version", pt.map(|e| e.rev()).unwrap_or_default())], ""));
    } else {
        out.push(chk("adb", "warn", &[], ""));
    }
    if platform::is_linux() && !cfg!(target_arch = "x86_64") {
        out.push(chk("arch", "error", &[("arch", std::env::consts::ARCH.to_string())], ""));
    }
    out.extend(check_accel(sdk));
    if platform::is_linux() && sdk.has_emulator() {
        let missing = missing_libs(sdk);
        if missing.is_empty() {
            out.push(chk("libs", "ok", &[], ""));
        } else {
            out.push(chk("libs", "error", &[("libs", missing.join(", "))], &install_hint(&missing)));
        }
    }
    if platform::is_linux() {
        let gpus = gpu::list_gpus();
        if !gpus.is_empty() {
            let names: Vec<_> = gpus.iter().map(|g| g.name.clone()).collect();
            out.push(chk("gpu", "info", &[("gpus", names.join(" · ")), ("count", gpus.len().to_string())], ""));
            out.push(chk(if gpu::prefer_host_gpu() { "render_host" } else { "render_auto" }, "info", &[], ""));
        }
        if let Ok(t) = std::env::var("XDG_SESSION_TYPE") {
            if !t.is_empty() {
                out.push(chk("session", "info", &[("type", t)], ""));
            }
        }
    }
    let probe = if sdk.exists() { sdk.root.clone() } else { platform::home() };
    if let Some(free) = crate::sdk::install::free_space(&probe) {
        out.push(chk(
            "disk",
            if free < 8 << 30 { "warn" } else { "ok" },
            &[("free", human_bytes(free as i64)), ("path", probe.to_string_lossy().into_owned())],
            "",
        ));
    }
    out
}

fn kvm_fix() -> String {
    if Path::new("/dev/kvm").exists() {
        "sudo usermod -aG kvm $USER   # depois saia da sessão e entre de novo".into()
    } else {
        "sudo apt install qemu-kvm   # e habilite VT-x/AMD-V na BIOS".into()
    }
}

fn check_accel(sdk: &Sdk) -> Vec<Check> {
    let chk_bin = sdk.root.join("emulator/emulator-check");
    if platform::is_executable(&chk_bin) {
        if let Some(o) = output_with_timeout(Command::new(&chk_bin).arg("accel"), Duration::from_secs(8)) {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            let lines: Vec<&str> = text.trim().lines().collect();
            // formato: accel: / <código> / <texto> / accel
            if lines.len() >= 3 {
                let (code, msg) = (lines[1].trim(), lines[2].trim().to_string());
                if code == "0" {
                    return vec![chk("accel", "ok", &[("info", msg)], "")];
                }
                let fix = if platform::is_linux() { kvm_fix() } else { String::new() };
                return vec![chk("accel", "error", &[("info", msg)], &fix)];
            }
        }
    }
    // Sem o emulator-check (emulador ainda não instalado): checagem manual.
    if platform::is_linux() {
        return match std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm") {
            Ok(_) => vec![chk("accel", "ok", &[("info", "KVM (/dev/kvm)".into())], "")],
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                vec![chk("accel", "error", &[("info", crate::lang::tr("/dev/kvm não existe", "/dev/kvm does not exist"))], &kvm_fix())]
            }
            Err(_) => vec![chk("accel", "error", &[("info", crate::lang::tr("sem permissão em /dev/kvm", "no permission on /dev/kvm"))], &kvm_fix())],
        };
    }
    if platform::is_mac() {
        let ok = output_with_timeout(Command::new("sysctl").args(["-n", "kern.hv_support"]), Duration::from_secs(3))
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
            .unwrap_or(false);
        return vec![if ok {
            chk("accel", "ok", &[("info", "Hypervisor.framework".into())], "")
        } else {
            chk("accel", "error", &[("info", crate::lang::tr("Hypervisor.framework indisponível", "Hypervisor.framework unavailable"))], "")
        }];
    }
    vec![]
}

/// Roda o ldd nos binários do emulador com o mesmo LD_LIBRARY_PATH que o lançador
/// usa, e devolve as bibliotecas ausentes.
fn missing_libs(sdk: &Sdk) -> Vec<String> {
    let root = sdk.root.join("emulator");
    let ldpath: Vec<String> = ["lib64", "lib64/qt/lib", "lib64/gles_angle", "lib64/gles_swiftshader", "lib64/vulkan"]
        .iter()
        .map(|d| root.join(d).to_string_lossy().into_owned())
        .collect();
    let mut targets = vec![root.join("emulator"), root.join("lib64/qt/plugins/platforms/libqxcb.so"), root.join("qemu/linux-x86_64/qemu-system-x86_64")];
    targets.retain(|t| platform::file_exists(t));
    let mut set = BTreeSet::new();
    for t in targets {
        let o = output_with_timeout(Command::new("ldd").arg(&t).env("LD_LIBRARY_PATH", ldpath.join(":")), Duration::from_secs(10));
        let Some(o) = o else { continue };
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if line.contains("not found") {
                if let Some(first) = line.split_whitespace().next() {
                    set.insert(first.to_string());
                }
            }
        }
    }
    set.into_iter().collect()
}

// pacotes por família de distribuição para as bibliotecas mais comuns: (apt, dnf, pacman)
const LIB_PACKAGES: &[(&str, [&str; 3])] = &[
    ("libxcb-cursor.so.0", ["libxcb-cursor0", "xcb-util-cursor", "xcb-util-cursor"]),
    ("libxcb-xinerama.so.0", ["libxcb-xinerama0", "libxcb", "libxcb"]),
    ("libxkbfile.so.1", ["libxkbfile1", "libxkbfile", "libxkbfile"]),
    ("libpulse.so.0", ["libpulse0", "pulseaudio-libs", "libpulse"]),
    ("libnss3.so", ["libnss3", "nss", "nss"]),
    ("libasound.so.2", ["libasound2t64", "alsa-lib", "alsa-lib"]),
    ("libGL.so.1", ["libgl1", "mesa-libGL", "mesa"]),
    ("libX11-xcb.so.1", ["libx11-xcb1", "libX11-xcb", "libx11"]),
    ("libxcb-xkb.so.1", ["libxcb-xkb1", "libxcb", "libxcb"]),
    ("libxkbcommon-x11.so.0", ["libxkbcommon-x11-0", "libxkbcommon-x11", "libxkbcommon-x11"]),
    ("libtinfo.so.5", ["libtinfo5", "ncurses-compat-libs", "ncurses5-compat-libs"]),
    ("libbsd.so.0", ["libbsd0", "libbsd", "libbsd"]),
];

pub fn install_hint(libs: &[String]) -> String {
    let family = distro_family();
    let idx = match family {
        "fedora" => 1,
        "arch" => 2,
        _ => 0,
    };
    let pkgs: Vec<&str> = libs.iter().filter_map(|l| LIB_PACKAGES.iter().find(|(n, _)| n == l).map(|(_, p)| p[idx])).collect();
    if pkgs.is_empty() {
        return String::new();
    }
    let cmd = match family {
        "fedora" => "sudo dnf install",
        "arch" => "sudo pacman -S",
        _ => "sudo apt install",
    };
    format!("{cmd} {}", pkgs.join(" "))
}

fn distro_family() -> &'static str {
    let s = std::fs::read_to_string("/etc/os-release").unwrap_or_default().to_lowercase();
    if ["fedora", "rhel", "centos", "suse"].iter().any(|k| s.contains(k)) {
        "fedora"
    } else if s.contains("arch") || s.contains("manjaro") {
        "arch"
    } else {
        "debian"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints() {
        assert!(!install_hint(&["libxcb-cursor.so.0".into(), "libdesconhecida.so".into()]).is_empty());
        assert!(install_hint(&["libdesconhecida.so".into()]).is_empty());
    }
}
