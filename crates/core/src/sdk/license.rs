//! Licenças do SDK. O aceite é gravado em `<sdk>/licenses/<id>` com o mesmo
//! hash que o sdkmanager usa, então vale também para o Android Studio.

use super::Sdk;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::io::Write;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct License {
    pub id: String,
    pub text: String,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

/// Reproduz o `TrimStringAdapter` do sdklib do Google, aplicado ao texto antes
/// do hash:
///   replaceAll("(?<=\s)[ \t]*", "")    remove espaços depois de espaço/indentação
///   replaceAll("(?<!\n)\n(?!\n)", " ")  junta as linhas de um mesmo parágrafo
///   replaceAll(" +", " ")
///   trim()
pub fn normalize(s: &str) -> String {
    let b = s.as_bytes();
    let mut s1 = Vec::with_capacity(b.len());
    for (i, &c) in b.iter().enumerate() {
        if (c == b' ' || c == b'\t') && i > 0 && is_ws(b[i - 1]) {
            continue;
        }
        s1.push(c);
    }
    let n = s1.len();
    let mut s2 = s1.clone();
    for i in 0..n {
        if s1[i] == b'\n' && (i == 0 || s1[i - 1] != b'\n') && (i == n - 1 || s1[i + 1] != b'\n') {
            s2[i] = b' ';
        }
    }
    let mut s3 = Vec::with_capacity(n);
    for &c in &s2 {
        if c == b' ' && s3.last() == Some(&b' ') {
            continue;
        }
        s3.push(c);
    }
    let start = s3.iter().position(|&c| c > b' ').unwrap_or(s3.len());
    let end = s3.iter().rposition(|&c| c > b' ').map(|i| i + 1).unwrap_or(start);
    String::from_utf8_lossy(&s3[start..end]).into_owned()
}

impl License {
    /// SHA-1 do texto normalizado — o valor que o sdkmanager grava em licenses/.
    pub fn hash(&self) -> String {
        let mut h = Sha1::new();
        h.update(normalize(&self.text).as_bytes());
        hex::encode(h.finalize())
    }
}

impl Sdk {
    fn license_file(&self, id: &str) -> std::path::PathBuf {
        let base = std::path::Path::new(id).file_name().map(|s| s.to_os_string()).unwrap_or_default();
        self.root.join("licenses").join(base)
    }

    pub fn accepted(&self, l: &License) -> bool {
        match std::fs::read_to_string(self.license_file(&l.id)) {
            Ok(s) => {
                let want = l.hash();
                s.lines().any(|line| line.trim() == want)
            }
            Err(_) => false,
        }
    }

    /// Grava o aceite (preserva aceites anteriores de outras versões do texto).
    pub fn accept(&self, l: &License) -> std::io::Result<()> {
        if self.accepted(l) {
            return Ok(());
        }
        std::fs::create_dir_all(self.root.join("licenses"))?;
        let mut f = std::fs::OpenOptions::new().append(true).create(true).open(self.license_file(&l.id))?;
        f.write_all(format!("\n{}", l.hash()).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_java_regexes() {
        assert_eq!(normalize("  A  b\n  c\n\n d \n\n\nE\tf  "), "A b c\n\nd \n\n\nE\tf");
    }

    #[test]
    fn hash_matches_sdkmanager() {
        // Os dois textos (antigo e atual, com quebras de linha diferentes) geram o
        // mesmo hash que o sdkmanager do Google grava em licenses/.
        const WANT: &str = "24333f8a63b6825ea9c5514f83c2829b004d1fee";
        for f in ["android-sdk-license.old.txt", "android-sdk-license.current.txt"] {
            let text = std::fs::read_to_string(format!("{}/tests/data/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            assert_eq!(License { id: "android-sdk-license".into(), text }.hash(), WANT, "{f}");
        }
    }

    #[test]
    fn accept_round_trip() {
        let dir = std::env::temp_dir().join(format!("avdcore-lic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Sdk::new(&dir);
        let l = License { id: "android-sdk-license".into(), text: "hello world".into() };
        assert!(!s.accepted(&l));
        s.accept(&l).unwrap();
        assert!(s.accepted(&l));
        let l2 = License { id: "android-sdk-license".into(), text: "hello world v2".into() };
        assert!(!s.accepted(&l2));
        s.accept(&l2).unwrap();
        assert!(s.accepted(&l) && s.accepted(&l2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
