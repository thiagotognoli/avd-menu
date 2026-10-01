use super::icns;
use super::{CreateResult, IconSource, Info, Options, ICON_PNG};
use crate::sdk::Sdk;
use crate::{platform, Error, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn apps_dir() -> PathBuf {
    platform::home().join("Applications")
}

fn plist_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub(crate) fn bundle_name(display: &str) -> String {
    let cleaned: String = display.chars().map(|c| if matches!(c, '/' | ':') { '-' } else { c }).filter(|c| *c != '\0').collect();
    format!("{}.app", cleaned.trim())
}

pub(crate) fn lookup(avd_name: &str) -> Info {
    let marker = format!("<key>AVDMenuAVD</key><string>{}</string>", plist_esc(avd_name));
    let Ok(rd) = std::fs::read_dir(apps_dir()) else { return Info::default() };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".app") else { continue };
        if let Ok(s) = std::fs::read_to_string(e.path().join("Contents/Info.plist")) {
            if s.contains(&marker) {
                return Info { exists: true, path: e.path().to_string_lossy().into_owned(), display_name: stem.to_string() };
            }
        }
    }
    Info::default()
}

pub(crate) fn create(sdk: &Sdk, opt: &Options, name: &str, icon: &IconSource) -> Result<CreateResult> {
    // Recria: remove um atalho anterior do mesmo AVD (o nome pode ter mudado).
    let old = lookup(&opt.avd);
    if old.exists {
        let _ = std::fs::remove_dir_all(&old.path);
    }
    let app = apps_dir().join(bundle_name(name));
    let macos = app.join("Contents/MacOS");
    let res = app.join("Contents/Resources");
    std::fs::create_dir_all(&macos)?;
    std::fs::create_dir_all(&res)?;
    // O macOS não lê SVG em .icns: usa o ícone padrão.
    let png: &[u8] = if icon.ext == "png" { &icon.data } else { ICON_PNG };
    std::fs::write(res.join("icon.icns"), icns::icns(png)?)?;
    let mut env = format!("export ANDROID_SDK_ROOT={}\n", sh_quote(&sdk.root.to_string_lossy()));
    if let Ok(h) = std::env::var("ANDROID_AVD_HOME") {
        if !h.is_empty() {
            env.push_str(&format!("export ANDROID_AVD_HOME={}\n", sh_quote(&h)));
        }
    }
    let script = format!("#!/bin/bash\n{env}exec {} -avd {}\n", sh_quote(&sdk.emulator().to_string_lossy()), sh_quote(&opt.avd));
    let launcher = macos.join("launcher");
    std::fs::write(&launcher, script)?;
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>{n}</string>
<key>CFBundleDisplayName</key><string>{n}</string>
<key>CFBundleIdentifier</key><string>app.avdmenu.avd.{slug}</string>
<key>CFBundleExecutable</key><string>launcher</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>AVDMenuAVD</key><string>{avd}</string>
</dict></plist>
"#,
        n = plist_esc(name),
        slug = platform::slugify(&opt.avd),
        avd = plist_esc(&opt.avd)
    );
    std::fs::write(app.join("Contents/Info.plist"), plist)?;
    Ok(CreateResult { path: app.to_string_lossy().into_owned(), display_name: name.to_string(), icon: "icon.icns".into(), pinned: false, gpu_action: false })
}

pub(crate) fn remove(avd_name: &str) -> Result<()> {
    let info = lookup(avd_name);
    if !info.exists {
        return Err(err!(404, "nenhum atalho encontrado para o AVD {:?}", "no shortcut found for AVD {:?}", avd_name));
    }
    std::fs::remove_dir_all(info.path).map_err(Error::from)
}
