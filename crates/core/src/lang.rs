//! Idioma das mensagens geradas aqui no núcleo (a interface tem o dela).

use std::sync::RwLock;

static RUNTIME: RwLock<Option<String>> = RwLock::new(None);

/// Registra o idioma efetivo da interface ("pt" ou "en").
pub fn set_runtime_lang(l: &str) {
    if l == "pt" || l == "en" {
        *RUNTIME.write().unwrap() = Some(l.to_string());
    }
}

/// "pt" ou "en": idioma da interface; senão o das configurações; senão o do sistema.
pub fn lang() -> &'static str {
    if let Some(l) = RUNTIME.read().unwrap().as_deref() {
        return if l == "pt" { "pt" } else { "en" };
    }
    match crate::config::load().lang.as_str() {
        "pt" => return "pt",
        "en" => return "en",
        _ => {}
    }
    for k in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return if v.to_lowercase().starts_with("pt") { "pt" } else { "en" };
            }
        }
    }
    "en"
}

pub fn is_pt() -> bool {
    lang() == "pt"
}

/// Escolhe o texto pelo idioma atual.
pub fn tr(pt: &str, en: &str) -> String {
    if is_pt() { pt } else { en }.to_string()
}

/// `trf!("olá {}", "hello {}", nome)` — formata no idioma atual.
#[macro_export]
macro_rules! trf {
    ($pt:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::lang::is_pt() { format!($pt $(, $a)*) } else { format!($en $(, $a)*) }
    };
}
