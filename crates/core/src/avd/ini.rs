//! Arquivos chave=valor do AVD (config.ini / <nome>.ini). Preserva a ordem e os
//! comentários ao editar — o emulador reescreve esses arquivos e não queremos
//! embaralhar o que o usuário ou o Android Studio escreveram.

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Ini {
    lines: Vec<String>,
    vals: BTreeMap<String, String>,
}

fn key_of(line: &str) -> Option<&str> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    t.find('=').filter(|&i| i > 0).map(|i| t[..i].trim())
}

impl Ini {
    pub fn new() -> Ini {
        Ini::default()
    }

    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            ini.lines.push(line.to_string());
            if let Some(k) = key_of(line) {
                let t = line.trim();
                let i = t.find('=').unwrap();
                ini.vals.insert(k.to_string(), t[i + 1..].trim().to_string());
            }
        }
        ini
    }

    pub fn read(path: &Path) -> std::io::Result<Ini> {
        Ok(Ini::parse(&std::fs::read_to_string(path)?))
    }

    /// Valor da chave ("" se ausente).
    pub fn get(&self, k: &str) -> &str {
        self.vals.get(k).map(String::as_str).unwrap_or("")
    }

    pub fn has(&self, k: &str) -> bool {
        self.vals.contains_key(k)
    }

    pub fn map(&self) -> &BTreeMap<String, String> {
        &self.vals
    }

    /// Define a chave, mantendo a posição se ela já existia.
    pub fn set(&mut self, k: &str, v: &str) {
        if self.vals.contains_key(k) {
            if let Some(line) = self.lines.iter_mut().find(|l| key_of(l) == Some(k)) {
                *line = format!("{k}={v}");
            }
        } else {
            self.lines.push(format!("{k}={v}"));
        }
        self.vals.insert(k.to_string(), v.to_string());
    }

    pub fn delete(&mut self, k: &str) {
        if self.vals.remove(k).is_some() {
            self.lines.retain(|l| key_of(l) != Some(k));
        }
    }

    /// Serializa mantendo a ordem original.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(l);
            s.push('\n');
        }
        s
    }

    /// Serializa com as chaves em ordem alfabética (como o avdmanager).
    pub fn to_sorted_text(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.vals {
            s.push_str(&format!("{k}={v}\n"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_order_and_comments() {
        let mut ini = Ini::parse("# comentário\nb=1\na=2\n\nc = 3\n");
        ini.set("a", "9");
        ini.set("novo", "x");
        ini.delete("b");
        assert_eq!(ini.to_text(), "# comentário\na=9\n\nc = 3\nnovo=x\n");
        assert_eq!(ini.get("c"), "3");
        assert!(!ini.has("b"));
        assert_eq!(ini.to_sorted_text(), "a=9\nc=3\nnovo=x\n");
    }
}
