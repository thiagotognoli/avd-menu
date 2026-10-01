//! Nome amigável (“Android 16”) para cada nível de API.

const NAMES: &[(&str, &str)] = &[
    ("37", "Android 17"),
    ("36", "Android 16"),
    ("35", "Android 15"),
    ("34", "Android 14"),
    ("33", "Android 13"),
    ("32", "Android 12L"),
    ("31", "Android 12"),
    ("30", "Android 11"),
    ("29", "Android 10"),
    ("28", "Android 9 (Pie)"),
    ("27", "Android 8.1 (Oreo)"),
    ("26", "Android 8.0 (Oreo)"),
    ("25", "Android 7.1 (Nougat)"),
    ("24", "Android 7.0 (Nougat)"),
    ("23", "Android 6.0 (Marshmallow)"),
    ("22", "Android 5.1 (Lollipop)"),
    ("21", "Android 5.0 (Lollipop)"),
    ("20", "Android 4.4W"),
    ("19", "Android 4.4 (KitKat)"),
    ("18", "Android 4.3"),
    ("17", "Android 4.2"),
    ("16", "Android 4.1"),
    ("15", "Android 4.0.3"),
    ("14", "Android 4.0"),
];

fn lookup(api: &str) -> Option<&'static str> {
    NAMES.iter().find(|(k, _)| *k == api).map(|(_, v)| *v)
}

/// Nome da versão do Android para um nível de API ("" se desconhecido); usa o
/// codinome da prévia quando houver.
pub fn api_name(api: &str, codename: &str) -> String {
    let api = api.trim_end_matches('x');
    if let Some(n) = lookup(api) {
        return n.to_string();
    }
    // 36.1, 37.2... são versões menores do mesmo Android
    if let Some(i) = api.find('.') {
        if let Some(n) = lookup(&api[..i]) {
            return n.to_string();
        }
    }
    if let Some(i) = api.find("-ext") {
        if let Some(n) = lookup(&api[..i]) {
            return n.to_string();
        }
    }
    if !codename.is_empty() {
        return format!("{codename} (preview)");
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        for (i, w) in [("36", "Android 16"), ("36.1", "Android 16"), ("34x", "Android 14"), ("34-ext10", "Android 14"), ("99", "")] {
            assert_eq!(api_name(i, ""), w, "{i}");
        }
    }
}
