//! Garante que toda chave t('...') usada na interface existe no dicionário,
//! em português e inglês.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui")
}

fn js_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            js_files(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("js") {
            out.push(p);
        }
    }
}

/// Chaves do dicionário: tudo que precede `: [` e parece um identificador/chave
/// (`'diag.sdk.ok': [` ou `ok: [`), em qualquer posição da linha.
fn dictionary(src: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let key_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    let mut from = 0;
    while let Some(i) = src[from..].find(": [") {
        let at = from + i;
        let before = &src[..at];
        let quoted = before.ends_with('\'');
        let body = if quoted { &before[..before.len() - 1] } else { before };
        let key: String = body.chars().rev().take_while(|c| key_char(*c)).collect::<Vec<_>>().into_iter().rev().collect();
        let prefix_ok = if quoted { body[..body.len() - key.len()].ends_with('\'') } else { true };
        if !key.is_empty() && prefix_ok {
            keys.insert(key);
        }
        from = at + 3;
    }
    keys
}

/// Usos `t('chave'` seguidos de `,` ou `)`; e prefixos dinâmicos `t('prefixo.' +`.
fn usages(code: &str) -> (Vec<String>, Vec<String>) {
    let (mut keys, mut prefixes) = (vec![], vec![]);
    let mut rest = code;
    while let Some(i) = rest.find("t('") {
        let prev = rest[..i].chars().last();
        let tail = &rest[i + 3..];
        if let Some(end) = tail.find('\'') {
            let key = &tail[..end];
            let after = tail[end + 1..].trim_start();
            let ident_before = prev.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
            if !ident_before && !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
                if after.starts_with(',') || after.starts_with(')') {
                    keys.push(key.to_string());
                } else if after.starts_with('+') {
                    prefixes.push(key.to_string());
                }
            }
        }
        rest = &rest[i + 3..];
    }
    (keys, prefixes)
}

#[test]
fn every_used_key_is_translated() {
    let ui = ui_dir();
    let i18n = std::fs::read_to_string(ui.join("js/i18n.js")).unwrap();
    let dict = dictionary(&i18n);
    assert!(dict.len() > 200, "dicionário pequeno demais ({} chaves) — o parser quebrou?", dict.len());
    let mut files = vec![];
    js_files(&ui.join("js"), &mut files);
    let mut prefixes = BTreeSet::new();
    let mut missing = vec![];
    for f in files.iter().filter(|f| !f.ends_with("i18n.js")) {
        let code = std::fs::read_to_string(f).unwrap();
        let (keys, pre) = usages(&code);
        for k in keys {
            if !dict.contains(&k) {
                missing.push(format!("{}: {k}", f.file_name().unwrap().to_string_lossy()));
            }
        }
        prefixes.extend(pre);
    }
    let need: &[(&str, &[&str])] = &[
        ("cat.", &["phone", "tablet", "wear", "tv", "automotive", "desktop", "xr"]),
        ("phase.", &["download", "verify", "extract", "finalize", "remove"]),
        ("sdk.group.", &["emulator", "platform-tools", "cmdline-tools", "build-tools", "ndk", "cmake", "extras", "sources", "other"]),
    ];
    for p in &prefixes {
        match need.iter().find(|(n, _)| n == p) {
            None => missing.push(format!("prefixo dinâmico desconhecido {p:?} — liste-o neste teste")),
            Some((_, sufs)) => missing.extend(sufs.iter().filter(|s| !dict.contains(&format!("{p}{s}"))).map(|s| format!("{p}{s}"))),
        }
    }
    // cada verificação do diagnóstico (emu::diag) precisa de título
    for k in [
        "diag.sdk.ok",
        "diag.sdk.warn",
        "diag.emulator.ok",
        "diag.emulator.warn",
        "diag.adb.ok",
        "diag.adb.warn",
        "diag.arch.error",
        "diag.accel.ok",
        "diag.accel.error",
        "diag.libs.ok",
        "diag.libs.error",
        "diag.gpu.info",
        "diag.render_host.info",
        "diag.render_auto.info",
        "diag.session.info",
        "diag.disk.ok",
        "diag.disk.warn",
    ] {
        if !dict.contains(k) {
            missing.push(k.to_string());
        }
    }
    assert!(missing.is_empty(), "chaves sem tradução:\n{}", missing.join("\n"));
}

// ---- demais idiomas (ui/js/lang/<código>.js) -------------------------------------------------

/// Literal JS entre aspas simples ou duplas a partir de `s[i]`; devolve (texto, índice seguinte).
fn js_string(s: &str, i: usize) -> Option<(String, usize)> {
    let b = s.as_bytes();
    let q = *b.get(i)?;
    if q != b'\'' && q != b'"' {
        return None;
    }
    let (mut out, mut j) = (String::new(), i + 1);
    let rest = &s[j..];
    let mut it = rest.char_indices();
    while let Some((k, c)) = it.next() {
        if c as u32 == q as u32 {
            j += k + 1;
            return Some((out, j));
        }
        if c == '\\' {
            let (_, n) = it.next()?;
            out.push(match n {
                'n' => '\n',
                other => other,
            });
        } else {
            out.push(c);
        }
    }
    None
}

/// Chave → inglês, lidos de `i18n.js` (`'chave': ['pt', 'en']`).
fn english(src: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut from = 0;
    let key_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    while let Some(i) = src[from..].find(": [") {
        let at = from + i;
        let before = src[..at].trim_end_matches('\'');
        let key: String = before.chars().rev().take_while(|c| key_char(*c)).collect::<Vec<_>>().into_iter().rev().collect();
        from = at + 3;
        let Some((_, after_pt)) = js_string(src, from) else { continue };
        let rest = src[after_pt..].trim_start_matches([',', ' ']);
        let start = src.len() - rest.len();
        if let Some((en, _)) = js_string(src, start) {
            if !key.is_empty() {
                map.insert(key, en);
            }
        }
    }
    map
}

fn vars(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        if let Some(j) = rest[i..].find('}') {
            out.insert(rest[i + 1..i + j].to_string());
            rest = &rest[i + j + 1..];
        } else {
            break;
        }
    }
    out
}

/// Uma entrada por linha: `  'chave': "texto",`.
fn pack(src: &str) -> std::collections::BTreeMap<String, String> {
    src.lines()
        .filter_map(|l| {
            let l = l.strip_prefix("  '")?;
            let (key, rest) = l.split_once("': ")?;
            let value = rest.strip_suffix(',')?;
            Some((key.to_string(), serde_json::from_str::<String>(value).unwrap_or_else(|e| panic!("{key}: {e}"))))
        })
        .collect()
}

#[test]
fn every_language_pack_is_complete() {
    let ui = ui_dir();
    let i18n = std::fs::read_to_string(ui.join("js/i18n.js")).unwrap();
    let en = english(&i18n);
    assert!(en.len() > 200, "o parser não achou o dicionário ({} chaves)", en.len());
    let mut problems = vec![];
    // a lista de idiomas da interface é a mesma do núcleo
    let langs_block = &i18n[i18n.find("export const LANGUAGES").unwrap()..];
    let langs_block = &langs_block[..langs_block.find("];").unwrap()];
    let ui_codes: Vec<String> = langs_block.split("code: '").skip(1).map(|s| s[..s.find('\'').unwrap()].to_string()).collect();
    assert_eq!(ui_codes, avdcore::lang::LANGS.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "LANGUAGES (i18n.js) != lang::LANGS");
    for code in avdcore::lang::LANGS.iter().filter(|c| !matches!(**c, "pt" | "en")) {
        let path = ui.join(format!("js/lang/{code}.js"));
        let Ok(src) = std::fs::read_to_string(&path) else {
            problems.push(format!("{code}: falta {}", path.display()));
            continue;
        };
        assert!(i18n.contains(&format!("import {code} from './lang/{code}.js'")), "i18n.js não importa lang/{code}.js");
        let tr = pack(&src);
        for (k, e) in &en {
            match tr.get(k) {
                None => problems.push(format!("{code}: sem tradução de {k}")),
                Some(t) if t.trim().is_empty() => problems.push(format!("{code}: {k} vazio")),
                Some(t) if vars(t) != vars(e) => problems.push(format!("{code}: {k}: variáveis {:?} != {:?}", vars(t), vars(e))),
                _ => {}
            }
        }
        for k in tr.keys().filter(|k| !en.contains_key(*k)) {
            problems.push(format!("{code}: chave que não existe em i18n.js: {k}"));
        }
    }
    assert!(problems.is_empty(), "{} problema(s) nos idiomas:\n{}", problems.len(), problems.join("\n"));
}
