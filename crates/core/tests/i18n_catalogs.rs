//! Catálogos das mensagens do núcleo (crates/core/data/i18n/<código>.json):
//! mesmas entradas em todos os idiomas e mesmos marcadores (`{}`, `{:?}`, `{0}`…) do inglês.

use avdcore::lang::{catalog, placeholders, LANGS};
use std::collections::BTreeSet;

fn sorted(t: &str) -> Vec<(usize, bool)> {
    let mut p = placeholders(t);
    p.sort();
    p
}

#[test]
fn catalogs_have_the_same_entries_and_placeholders() {
    let others: Vec<&str> = LANGS.iter().copied().filter(|c| !matches!(*c, "pt" | "en")).collect();
    let reference: BTreeSet<&String> = catalog(others[0]).expect("catálogo").keys().collect();
    assert!(reference.len() > 100, "catálogo pequeno demais: {}", reference.len());
    let mut problems = vec![];
    for code in others {
        let cat = catalog(code).unwrap_or_else(|| panic!("falta o catálogo {code}"));
        let keys: BTreeSet<&String> = cat.keys().collect();
        for k in reference.difference(&keys) {
            problems.push(format!("{code}: falta {k:?}"));
        }
        for k in keys.difference(&reference) {
            problems.push(format!("{code}: sobra {k:?}"));
        }
        for (en, t) in cat {
            if t.trim().is_empty() {
                problems.push(format!("{code}: vazio {en:?}"));
            } else if sorted(t) != sorted(en) {
                problems.push(format!("{code}: marcadores de {en:?}: {:?} != {:?}", sorted(t), sorted(en)));
            }
            // `{v}` (versão) e os `{{`/`}}` literais precisam sobreviver
            if en.contains("{v}") != t.contains("{v}") || en.matches("{{").count() != t.matches("{{").count() {
                problems.push(format!("{code}: {{v}}/chaves literais em {en:?}"));
            }
        }
    }
    assert!(problems.is_empty(), "{} problema(s):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn desktop_entries_are_localized() {
    // as quatro frases do atalho (.desktop) têm tradução em todos os idiomas
    for code in LANGS.iter().filter(|c| !matches!(**c, "pt" | "en")) {
        for en in ["Cold boot", "Wipe data and start", "Start with dedicated GPU", "Starts the Android emulator {}"] {
            assert!(avdcore::lang::translated(code, en).is_some(), "{code}: {en}");
        }
    }
}
