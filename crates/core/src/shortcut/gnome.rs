//! Fixar atalhos na dock do GNOME (org.gnome.shell favorite-apps).

use crate::emu::output_with_timeout;
use crate::platform;
use std::process::Command;
use std::time::Duration;

fn gsettings(args: &[&str]) -> Option<String> {
    let bin = platform::which("gsettings")?;
    let o = output_with_timeout(Command::new(bin).args(args), Duration::from_secs(5))?;
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
