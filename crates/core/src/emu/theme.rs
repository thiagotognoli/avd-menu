//! Tema (claro/escuro) das janelas do emulador: barra de ferramentas e
//! controles estendidos. O emulador não segue o tema do sistema; ele lê
//! `theme` da seção `[set]` do Emulator.conf (0 = claro, 1 = escuro), que é o
//! que a opção “Theme” das configurações dele grava.

use super::output_with_timeout;
use crate::platform;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Arquivo de configurações do emulador (QSettings, formato INI). Só no Linux:
/// no macOS o Qt usa um .plist.
pub fn conf_path() -> Option<PathBuf> {
    platform::is_linux().then(|| platform::child_config_home().join("Android Open Source Project/Emulator.conf"))
}

/// O sistema está no modo escuro? (None = não deu para saber)
pub fn system_prefers_dark() -> Option<bool> {
    if !platform::is_linux() || platform::which("gsettings").is_none() {
        return None;
    }
    // Com o ambiente de fora do AppImage: o gsettings lê o banco do dconf em
    // $XDG_CONFIG_HOME, que dentro do AppImage com config portátil é outro.
    let get = |key: &str| {
        let mut cmd = Command::new("gsettings");
        platform::clean_env(cmd.args(["get", "org.gnome.desktop.interface", key]));
        output_with_timeout(&mut cmd, Duration::from_secs(3))
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_lowercase())
    };
    match get("color-scheme").as_deref() {
        Some("prefer-dark") => Some(true),
        Some("prefer-light") => Some(false),
        // “padrão”: vale o tema GTK (Adwaita-dark, Yaru-dark...)
        _ => get("gtk-theme").map(|t| t.contains("dark")),
    }
}

/// Valor de `theme` para a preferência (`config::Config::emulator_theme`).
pub fn wanted(pref: &str, system_dark: Option<bool>) -> Option<&'static str> {
    match pref {
        "light" => Some("0"),
        "dark" => Some("1"),
        "" | "system" => system_dark.map(|d| if d { "1" } else { "0" }),
        _ => None, // keep
    }
}

/// Aplica a preferência no Emulator.conf (vale para os emuladores iniciados
/// depois). Devolve se o arquivo mudou.
pub fn sync(pref: &str) -> std::io::Result<bool> {
    let Some(path) = conf_path() else { return Ok(false) };
    let Some(value) = wanted(pref, if matches!(pref, "" | "system") { system_prefers_dark() } else { None }) else { return Ok(false) };
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let new = set_ini_value(&old, "set", "theme", value);
    if new == old {
        return Ok(false);
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, new)?;
    Ok(true)
}

/// Troca (ou acrescenta) `key=value` na seção `[section]` de um INI, sem mexer
/// no resto.
pub fn set_ini_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let mut out: Vec<String> = vec![];
    let (mut in_sec, mut seen_sec, mut done) = (false, false, false);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_sec && !done {
                insert_before_blanks(&mut out, format!("{key}={value}"));
                done = true;
            }
            in_sec = t == header;
            seen_sec |= in_sec;
        } else if in_sec && !done && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !done {
        if in_sec {
            insert_before_blanks(&mut out, format!("{key}={value}"));
        } else {
            if !seen_sec {
                if out.last().is_some_and(|l| !l.trim().is_empty()) {
                    out.push(String::new());
                }
                out.push(header);
            }
            out.push(format!("{key}={value}"));
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn insert_before_blanks(out: &mut Vec<String>, line: String) {
    let mut at = out.len();
    while at > 0 && out[at - 1].trim().is_empty() {
        at -= 1;
    }
    out.insert(at, line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_edit_keeps_the_rest() {
        let conf = "[set]\nautoFindAdb=true\ntheme=0\nsavePath=/home/u/Área de trabalho\n\n[perAvd]\nx=1\n";
        assert_eq!(set_ini_value(conf, "set", "theme", "1"), conf.replace("theme=0", "theme=1"));
        // chave nova vai para o fim da própria seção
        let no_theme = "[set]\nautoFindAdb=true\n\n[perAvd]\nx=1\n";
        assert_eq!(set_ini_value(no_theme, "set", "theme", "1"), "[set]\nautoFindAdb=true\ntheme=1\n\n[perAvd]\nx=1\n");
        assert_eq!(set_ini_value("[perAvd]\nx=1\n", "set", "theme", "1"), "[perAvd]\nx=1\n\n[set]\ntheme=1\n");
        assert_eq!(set_ini_value("", "set", "theme", "0"), "[set]\ntheme=0\n");
        // não confunde com chaves parecidas nem com outras seções
        assert_eq!(set_ini_value("[a]\ntheme=9\n[set]\nthemeX=2\n", "set", "theme", "1"), "[a]\ntheme=9\n[set]\nthemeX=2\ntheme=1\n");
    }

    #[test]
    fn preference_to_value() {
        assert_eq!(wanted("light", Some(true)), Some("0"));
        assert_eq!(wanted("dark", None), Some("1"));
        assert_eq!(wanted("", Some(true)), Some("1"));
        assert_eq!(wanted("", Some(false)), Some("0"));
        assert_eq!(wanted("", None), None);
        assert_eq!(wanted("keep", Some(true)), None);
    }
}
