//! Fixar atalhos na dock do GNOME (org.gnome.shell favorite-apps).

use crate::emu::output_with_timeout;
use crate::platform;
use std::process::Command;
use std::time::Duration;

/// Roda o gsettings com o ambiente de fora do AppImage. O gsettings lê o banco
/// do dconf em $XDG_CONFIG_HOME/dconf/user, mas grava pelo serviço do dconf (o
/// banco de verdade). Com a configuração portátil do AppImage, o XDG_CONFIG_HOME
/// aponta para outra pasta: a leitura vinha com a lista padrão do sistema e,
/// ao fixar um atalho, essa lista substituía todos os favoritos do usuário.
fn gsettings(args: &[&str]) -> Option<String> {
    let bin = platform::which("gsettings")?;
    let mut cmd = Command::new(bin);
    platform::clean_env(cmd.args(args));
    let o = output_with_timeout(&mut cmd, Duration::from_secs(5))?;
    if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        None
    }
}

fn favorites() -> Option<Vec<String>> {
    let out = gsettings(&["get", "org.gnome.shell", "favorite-apps"])?;
    Some(out.split('\'').skip(1).step_by(2).map(str::to_string).collect())
}

fn set_favorites(fav: &[String]) -> bool {
    let quoted: Vec<String> = fav.iter().map(|f| format!("'{}'", f.replace('\'', ""))).collect();
    gsettings(&["set", "org.gnome.shell", "favorite-apps", &format!("[{}]", quoted.join(", "))]).is_some()
}

pub fn is_pinned(desktop_file: &str) -> bool {
    favorites().map(|f| f.iter().any(|x| x == desktop_file)).unwrap_or(false)
}

/// Fixa o atalho na dock do GNOME.
pub fn pin(desktop_file: &str) -> bool {
    let Some(mut fav) = favorites() else { return false };
    if fav.iter().any(|f| f == desktop_file) {
        return true;
    }
    fav.push(desktop_file.to_string());
    set_favorites(&fav)
}

pub fn unpin(desktop_file: &str) {
    let Some(fav) = favorites() else { return };
    let keep: Vec<String> = fav.iter().filter(|f| *f != desktop_file).cloned().collect();
    if keep.len() != fav.len() {
        set_favorites(&keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_come_from_the_real_dconf_inside_an_appimage() {
        let _g = crate::testutil::env_lock();
        // gsettings falso: a lista depende do XDG_CONFIG_HOME que ele recebe
        let bin = crate::testutil::tmp("fake-gsettings");
        let log = bin.join("set.log");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = set ]; then echo \"$XDG_CONFIG_HOME $4\" >> '{}'; exit 0; fi\ncase \"$XDG_CONFIG_HOME\" in\n  /home/u/.config) echo \"['meu.desktop', 'outro.desktop']\" ;;\n  *) echo \"['padrao-do-sistema.desktop']\" ;;\nesac\n",
            log.display()
        );
        std::fs::write(bin.join("gsettings"), script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bin.join("gsettings"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let saved: Vec<(&str, Option<std::ffi::OsString>)> =
            ["PATH", "APPDIR", "APPIMAGE", "XDG_CONFIG_HOME", "HOST_XDG_CONFIG_HOME"].iter().map(|k| (*k, std::env::var_os(k))).collect();
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
        std::env::set_var("APPDIR", "/tmp/.mount_AVDxyz");
        std::env::set_var("APPIMAGE", "/home/u/Apps/AVD Menu");
        std::env::set_var("XDG_CONFIG_HOME", "/home/u/Apps/AVD Menu.config");
        std::env::set_var("HOST_XDG_CONFIG_HOME", "/home/u/.config");

        assert!(is_pinned("meu.desktop"));
        assert!(pin("android-emulator-x.desktop"));
        let written = std::fs::read_to_string(&log).unwrap();
        assert_eq!(written.trim(), "/home/u/.config ['meu.desktop', 'outro.desktop', 'android-emulator-x.desktop']");

        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}
