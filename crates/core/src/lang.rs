//! Idioma das mensagens geradas aqui no núcleo (a interface tem o dela).
//!
//! Português e inglês vêm no próprio código (`tr`, `trf!`). Os outros idiomas ficam em
//! catálogos JSON (`data/i18n/<código>.json`: texto em inglês → tradução), embutidos no
//! binário. Sem entrada no catálogo a mensagem sai em inglês.

use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::{OnceLock, RwLock};

/// Idiomas suportados (código ISO 639-1). O primeiro é a língua “de casa”; “en” é o reserva.
pub const LANGS: [&str; 11] = ["pt", "en", "zh", "hi", "es", "fr", "ar", "bn", "ru", "ur", "id"];

static RUNTIME: RwLock<Option<&'static str>> = RwLock::new(None);

/// Código suportado a partir de um valor como `pt-BR`, `zh_CN.UTF-8` ou `en`.
pub fn normalize(code: &str) -> Option<&'static str> {
    let primary = code.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    LANGS.iter().copied().find(|l| *l == primary)
}

/// Registra o idioma efetivo da interface.
pub fn set_runtime_lang(l: &str) {
    if let Some(l) = normalize(l) {
        *RUNTIME.write().unwrap() = Some(l);
    }
}

/// Idioma da interface; senão o das configurações; senão o do sistema; senão inglês.
pub fn lang() -> &'static str {
    if let Some(l) = *RUNTIME.read().unwrap() {
        return l;
    }
    if let Some(l) = normalize(&crate::config::load().lang) {
        return l;
    }
    for k in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return normalize(&v).unwrap_or("en");
            }
        }
    }
    "en"
}

pub fn is_pt() -> bool {
    lang() == "pt"
}

fn raw_catalog(code: &str) -> Option<&'static str> {
    Some(match code {
        "zh" => include_str!("../data/i18n/zh.json"),
        "hi" => include_str!("../data/i18n/hi.json"),
        "es" => include_str!("../data/i18n/es.json"),
        "fr" => include_str!("../data/i18n/fr.json"),
        "ar" => include_str!("../data/i18n/ar.json"),
        "bn" => include_str!("../data/i18n/bn.json"),
        "ru" => include_str!("../data/i18n/ru.json"),
        "ur" => include_str!("../data/i18n/ur.json"),
        "id" => include_str!("../data/i18n/id.json"),
        _ => return None,
    })
}

/// Catálogo (inglês → tradução) de um idioma; `None` para pt/en.
pub fn catalog(code: &str) -> Option<&'static HashMap<String, String>> {
    static ALL: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    ALL.get_or_init(|| {
        LANGS
            .iter()
            .filter_map(|l| {
                let raw = raw_catalog(l)?;
                let map = serde_json::from_str(raw).unwrap_or_else(|e| panic!("catálogo {l}.json inválido: {e}"));
                Some((*l, map))
            })
            .collect()
    })
    .get(code)
}

/// Tradução do texto em inglês para `code` (ou `None` se não houver).
pub fn translated(code: &str, en: &str) -> Option<&'static str> {
    catalog(code)?.get(en).map(String::as_str)
}

/// Escolhe o texto pelo idioma `code`.
pub fn tr_in(code: &str, pt: &str, en: &str) -> String {
    match code {
        "pt" => pt,
        "en" => en,
        c => translated(c, en).unwrap_or(en),
    }
    .to_string()
}

/// Escolhe o texto pelo idioma atual.
pub fn tr(pt: &str, en: &str) -> String {
    tr_in(lang(), pt, en)
}

/// Argumento de `trf!` (precisa funcionar com `{}` e com `{:?}`).
pub trait Arg: Display + Debug {}
impl<T: Display + Debug + ?Sized> Arg for T {}

/// Marcadores de um modelo de mensagem: `(índice do argumento, é {:?})`.
/// `{}` consome os argumentos em ordem; `{0}`/`{1:?}` escolhem o argumento (para reordenar).
pub fn placeholders(template: &str) -> Vec<(usize, bool)> {
    let mut out = vec![];
    walk(template, &mut |_| {}, &mut |idx, dbg| out.push((idx, dbg)));
    out
}

fn walk(t: &str, lit: &mut dyn FnMut(&str), hole: &mut dyn FnMut(usize, bool)) {
    let b = t.as_bytes();
    let (mut i, mut start, mut next) = (0, 0, 0);
    while i < b.len() {
        match b[i] {
            b'{' if b.get(i + 1) == Some(&b'{') => {
                lit(&t[start..=i]);
                i += 2;
                start = i;
            }
            b'}' if b.get(i + 1) == Some(&b'}') => {
                lit(&t[start..=i]);
                i += 2;
                start = i;
            }
            b'{' => {
                let Some(end) = t[i..].find('}') else { break };
                let body = &t[i + 1..i + end];
                let (num, dbg) = match body.strip_suffix(":?") {
                    Some(n) => (n, true),
                    None => (body, false),
                };
                if num.is_empty() || num.bytes().all(|c| c.is_ascii_digit()) {
                    lit(&t[start..i]);
                    let idx = num.parse().unwrap_or_else(|_| {
                        next += 1;
                        next - 1
                    });
                    hole(idx, dbg);
                    i += end + 1;
                    start = i;
                } else {
                    i += end + 1; // `{v}` e afins: ficam como texto
                }
            }
            _ => i += 1,
        }
    }
    lit(&t[start..]);
}

/// Formata um modelo escolhido em tempo de execução (as traduções), no estilo de `format!`.
pub fn format_dyn(template: &str, args: &[&dyn Arg]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    // `lit` e `hole` escrevem no mesmo buffer, então passam por uma célula.
    let buf = std::cell::RefCell::new(&mut out);
    walk(template, &mut |s| buf.borrow_mut().push_str(s), &mut |idx, dbg| {
        if let Some(a) = args.get(idx) {
            let s = if dbg { format!("{a:?}") } else { format!("{a}") };
            buf.borrow_mut().push_str(&s);
        }
    });
    out
}

/// `trf!("olá {}", "hello {}", nome)` — formata no idioma atual.
#[macro_export]
macro_rules! trf {
    ($pt:literal, $en:literal $(, $a:expr)* $(,)?) => {
        match $crate::lang::lang() {
            "pt" => format!($pt $(, $a)*),
            "en" => format!($en $(, $a)*),
            l => match $crate::lang::translated(l, $en) {
                Some(t) => $crate::lang::format_dyn(t, &[$(&$a as &dyn $crate::lang::Arg),*]),
                None => format!($en $(, $a)*),
            },
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_locale_names() {
        assert_eq!(normalize("pt-BR"), Some("pt"));
        assert_eq!(normalize("zh_CN.UTF-8"), Some("zh"));
        assert_eq!(normalize("AR"), Some("ar"));
        assert_eq!(normalize("en_US@euro"), Some("en"));
        assert_eq!(normalize("de"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn formats_like_format_macro() {
        let (a, b): (&dyn Arg, &dyn Arg) = (&"x y", &42);
        assert_eq!(format_dyn("{} / {:?} / {{}}", &[a, a]), "x y / \"x y\" / {}");
        assert_eq!(format_dyn("{1} antes de {0:?}", &[a, b]), "42 antes de \"x y\"");
        assert_eq!(format_dyn("sem marcadores", &[]), "sem marcadores");
        assert_eq!(format_dyn("falta {} e {}", &[a]), "falta x y e ");
        assert_eq!(format_dyn("versão {v}: {}", &[b]), "versão {v}: 42");
    }

    #[test]
    fn placeholders_normalize_implicit_and_explicit() {
        assert_eq!(placeholders("{} e {:?}"), vec![(0, false), (1, true)]);
        assert_eq!(placeholders("{1:?} e {0} {{x}}"), vec![(1, true), (0, false)]);
        assert!(placeholders("AVD Menu {v}").is_empty());
    }

    #[test]
    fn translations_apply_to_other_languages_and_fall_back_to_english() {
        let _g = crate::testutil::env_lock();
        for code in LANGS {
            set_runtime_lang(code);
            assert_eq!(lang(), code);
            assert_eq!(tr("sim", "yes-unknown-in-catalog"), if code == "pt" { "sim" } else { "yes-unknown-in-catalog" });
            let msg = crate::trf!("o emulador {:?} já está em execução", "emulator {:?} is already running", "Pixel");
            assert!(msg.contains("Pixel"), "{code}: {msg}");
            if code == "en" {
                assert_eq!(msg, "emulator \"Pixel\" is already running");
            } else if code != "pt" {
                assert_ne!(msg, "emulator \"Pixel\" is already running", "{code} deveria estar traduzido");
            }
        }
        set_runtime_lang("en");
    }
}
