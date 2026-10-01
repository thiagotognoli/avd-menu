//! Tarefas de manutenção do projeto (`cargo xtask <comando>`), em Rust.
//!
//!   cargo xtask gen-devices --sdk ~/Applications/AndroidSDK [--oracle-dir DIR] [--out ARQ]
//!
//! `gen-devices` gera crates/core/data/devices.json a partir do SDK:
//!   1. extrai do jar do cmdline-tools os XMLs de definição de aparelhos do Android
//!      (nexus.xml, wear.xml, tv.xml...);
//!   2. roda o avdmanager de verdade uma vez por aparelho (num AVD_HOME temporário)
//!      e guarda o config.ini resultante — o “oráculo” de quais chaves hw.* cada
//!      aparelho recebe (sensores, dobradiças, heap da VM...);
//!   3. grava um JSON compacto: metadados + só as chaves que diferem do padrão.
//!
//! Só precisa ser re-executado quando o Google lançar aparelhos novos. Requer JDK
//! (para o avdmanager) e ao menos uma imagem de sistema instalada. As definições de
//! aparelhos são do AOSP (Apache License 2.0).

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Chaves que dependem da imagem de sistema escolhida, não do aparelho.
const IMAGE_KEYS: &[&str] = &[
    "PlayStore.enabled",
    "abi.type",
    "hw.cpu.arch",
    "image.sysdir.1",
    "target",
    "tag.display",
    "tag.displaynames",
    "tag.id",
    "tag.ids",
    "avd.id",
    "avd.name",
    "avd.ini.encoding",
    "hw.device.name",
    "disk.dataPartition.path",
];

/// (arquivo, categoria forçada)
const FILES: &[(&str, Option<&str>)] = &[
    ("nexus.xml", None),
    ("devices.xml", None),
    ("wear.xml", Some("wear")),
    ("tv.xml", Some("tv")),
    ("automotive.xml", Some("automotive")),
    ("desktop.xml", Some("desktop")),
    ("xr.xml", Some("xr")),
];

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("xtask: {}", msg.as_ref());
    std::process::exit(1);
}

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("gen-devices") => gen_devices(args.collect()),
        _ => die("uso: cargo xtask gen-devices --sdk CAMINHO [--oracle-dir DIR] [--out ARQUIVO]"),
    }
}

fn find_jars(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            find_jars(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("jar") {
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            if n.starts_with("sdklib") || n == "tools.sdklib.jar" {
                out.push(p);
            }
        }
    }
}

fn find_jar(sdk: &Path) -> PathBuf {
    let mut jars = vec![];
    if let Ok(rd) = std::fs::read_dir(sdk.join("cmdline-tools")) {
        for e in rd.flatten() {
            find_jars(&e.path().join("lib"), &mut jars);
        }
    }
    for j in jars {
        if let Ok(f) = std::fs::File::open(&j) {
            if let Ok(z) = zip::ZipArchive::new(f) {
                if z.file_names().any(|n| n.ends_with("devices/nexus.xml")) {
                    return j;
                }
            }
        }
    }
    die(format!("não achei o jar do sdklib com nexus.xml em {}", sdk.display()))
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}
fn text(n: roxmltree::Node, path: &[&str]) -> Option<String> {
    let mut cur = n;
    for p in path {
        cur = child(cur, p)?;
    }
    let t: String = cur.children().filter(|c| c.is_text()).filter_map(|c| c.text()).collect();
    let t = t.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

struct Dev {
    id: String,
    name: String,
    manufacturer: String,
    forced: Option<&'static str>,
    tags: Vec<String>,
    deprecated: bool,
    playstore: bool,
    diag: f64,
    w: i64,
    h: i64,
    sensors: Vec<String>,
    front: bool,
    back: bool,
    foldable: bool,
    skin: Option<String>,
    abis: Vec<String>,
    round: bool,
    device_ram: i64,
}

fn parse_devices(jar: &Path) -> Vec<Dev> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(jar).unwrap()).unwrap();
    let mut out = vec![];
    for (file, forced) in FILES {
        let Some(member) = z.file_names().find(|n| n.ends_with(&format!("sdklib/devices/{file}"))).map(str::to_string) else { continue };
        let mut xml = String::new();
        z.by_name(&member).unwrap().read_to_string(&mut xml).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        for dev in doc.root_element().children().filter(|c| c.is_element()) {
            let name = text(dev, &["name"]).unwrap_or_default();
            let id = text(dev, &["id"]).unwrap_or_else(|| name.clone());
            let Some(hw) = child(dev, "hardware") else { continue };
            let scr = child(hw, "screen");
            let tags: Vec<String> = dev
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "tag-id")
                .filter_map(|n| n.text().map(|t| t.trim().to_string()))
                .collect();
            let cams: Vec<String> =
                hw.children().filter(|c| c.is_element() && c.tag_name().name() == "camera").filter_map(|c| text(c, &["location"])).collect();
            let fold = scr.map(|s| child(s, "foldable-region").is_some()).unwrap_or(false) || child(hw, "hinge").is_some();
            let ram = child(hw, "ram");
            let mut ram_mib = ram.and_then(|r| r.text()).and_then(|t| t.trim().parse::<f64>().ok()).map(|f| f as i64).unwrap_or(0);
            if ram.and_then(|r| r.attribute("unit")) == Some("GiB") {
                ram_mib *= 1024;
            }
            out.push(Dev {
                round: id.contains("round") || name.contains("Round"),
                id,
                name,
                manufacturer: text(dev, &["manufacturer"]).unwrap_or_else(|| "Generic".into()),
                forced: *forced,
                tags,
                deprecated: dev.attribute("deprecated") == Some("true"),
                playstore: text(dev, &["playstore-enabled"]).as_deref() == Some("true"),
                diag: scr.and_then(|s| text(s, &["diagonal-length"])).and_then(|t| t.parse().ok()).unwrap_or(0.0),
                w: scr.and_then(|s| text(s, &["dimensions", "x-dimension"])).and_then(|t| t.parse().ok()).unwrap_or(0),
                h: scr.and_then(|s| text(s, &["dimensions", "y-dimension"])).and_then(|t| t.parse().ok()).unwrap_or(0),
                sensors: text(hw, &["sensors"]).unwrap_or_default().split_whitespace().map(str::to_string).collect(),
                front: cams.iter().any(|c| c == "front"),
                back: cams.iter().any(|c| c == "back"),
                foldable: fold,
                skin: text(hw, &["skin"]),
                abis: text(hw, &["abis"]).unwrap_or_default().split_whitespace().map(str::to_string).collect(),
                device_ram: ram_mib,
            });
        }
    }
    out
}

fn oracle_name(id: &str) -> String {
    format!("o_{}", id.replace([' ', '/', '(', ')'], "_"))
}

fn run_oracle(sdk: &Path, devs: &[Dev], dir: &Path) {
    let avdm = sdk.join("cmdline-tools/latest/bin/avdmanager");
    let mut pkg = None;
    if let Ok(rd) = std::fs::read_dir(sdk.join("system-images")) {
        'outer: for api in rd.flatten() {
            for tag in std::fs::read_dir(api.path()).into_iter().flatten().flatten() {
                for abi in std::fs::read_dir(tag.path()).into_iter().flatten().flatten() {
                    if abi.path().join("package.xml").exists() {
                        pkg = Some(format!(
                            "system-images;{};{};{}",
                            api.file_name().to_string_lossy(),
                            tag.file_name().to_string_lossy(),
                            abi.file_name().to_string_lossy()
                        ));
                        break 'outer;
                    }
                }
            }
        }
    }
    let Some(pkg) = pkg else { die("instale alguma imagem de sistema antes (ex.: system-images;android-36;google_apis;x86_64)") };
    std::fs::create_dir_all(dir).unwrap();
    for d in devs {
        if dir.join(format!("{}.avd/config.ini", oracle_name(&d.id))).exists() {
            continue;
        }
        eprintln!("avdmanager: {}", d.id);
        let _ = Command::new(&avdm)
            .args(["create", "avd", "-n", &oracle_name(&d.id), "-k", &pkg, "-d", &d.id, "--force"])
            .env("ANDROID_SDK_ROOT", sdk)
            .env("ANDROID_AVD_HOME", dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn read_ini(p: &Path) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for line in std::fs::read_to_string(p).unwrap_or_default().lines() {
        if let Some((k, v)) = line.split_once('=') {
            if !line.starts_with('#') {
                m.insert(k.to_string(), v.to_string());
            }
        }
    }
    m
}

fn category(d: &Dev) -> &'static str {
    if let Some(f) = d.forced {
        return f;
    }
    if d.tags.iter().any(|t| t.contains("wear")) {
        return "wear";
    }
    if d.foldable && d.diag < 14.0 {
        return "phone";
    }
    if d.id == "resizable" {
        return "phone";
    }
    if d.id == "13.5in Freeform" {
        return "desktop";
    }
    if d.diag >= 7.0 {
        "tablet"
    } else {
        "phone"
    }
}

fn sort_key(d: &Value) -> (usize, bool, usize, i64, String) {
    let cat = ["phone", "tablet", "wear", "tv", "automotive", "desktop", "xr"].iter().position(|c| Some(*c) == d["category"].as_str()).unwrap_or(9);
    let id = d["id"].as_str().unwrap_or("");
    let pix = id
        .strip_prefix("pixel_")
        .map(|r| r.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| -n)
        .unwrap_or(0);
    (cat, d["deprecated"].as_bool().unwrap_or(false), if d["manufacturer"] == "Google" { 0 } else { 1 }, pix, id.to_string())
}

fn gen_devices(args: Vec<String>) {
    let (mut sdk, mut oracle, mut out) = (None, None, PathBuf::from("crates/core/data/devices.json"));
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sdk" => sdk = it.next().map(PathBuf::from),
            "--oracle-dir" => oracle = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from).unwrap_or(out),
            o => die(format!("opção desconhecida: {o}")),
        }
    }
    let sdk = sdk.unwrap_or_else(|| die("--sdk é obrigatório"));
    let devs = parse_devices(&find_jar(&sdk));
    let oracle = oracle.unwrap_or_else(|| std::env::temp_dir().join(format!("avd-oracle-{}", std::process::id())));
    run_oracle(&sdk, &devs, &oracle);

    // config.ini de cada aparelho (sem as chaves que dependem da imagem)
    let mut cfgs: Vec<(usize, BTreeMap<String, String>)> = vec![];
    for (i, d) in devs.iter().enumerate() {
        let p = oracle.join(format!("{}.avd/config.ini", oracle_name(&d.id)));
        if !p.exists() {
            eprintln!("sem config para {}", d.id);
            continue;
        }
        cfgs.push((i, read_ini(&p).into_iter().filter(|(k, _)| !IMAGE_KEYS.contains(&k.as_str())).collect()));
    }
    let mut keys: Vec<String> = cfgs.iter().flat_map(|(_, c)| c.keys().cloned()).collect();
    keys.sort();
    keys.dedup();
    // padrão = valor mais comum de cada chave (ausente conta como valor; empate = o primeiro visto)
    let mut defaults: BTreeMap<String, String> = BTreeMap::new();
    for k in &keys {
        let mut counts: Vec<(Option<&String>, usize)> = vec![];
        for (_, c) in &cfgs {
            let v = c.get(k);
            match counts.iter_mut().find(|(x, _)| *x == v) {
                Some(e) => e.1 += 1,
                None => counts.push((v, 1)),
            }
        }
        let best = counts.iter().fold(None::<&(Option<&String>, usize)>, |b, e| if b.map(|b| e.1 > b.1).unwrap_or(true) { Some(e) } else { b });
        if let Some((Some(v), _)) = best {
            defaults.insert(k.clone(), (*v).clone());
        }
    }

    let mut list: Vec<Value> = vec![];
    for (i, c) in &cfgs {
        let d = &devs[*i];
        let mut diff = Map::new();
        for k in &keys {
            let v = c.get(k);
            if v != defaults.get(k) {
                diff.insert(k.clone(), v.map(|s| json!(s)).unwrap_or(Value::Null));
            }
        }
        let ramv = c.get("hw.ramSize").cloned().unwrap_or_else(|| "0".into());
        let ram_mb: i64 = match ramv.strip_suffix('G') {
            Some(g) => g.parse::<i64>().unwrap_or(0) * 1024,
            None => ramv.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0),
        };
        let num = |k: &str, dflt: i64| c.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(dflt);
        list.push(json!({
            "id": d.id, "name": d.name, "manufacturer": d.manufacturer, "tags": d.tags, "deprecated": d.deprecated, "playstore": d.playstore,
            "diag": d.diag, "w": num("hw.lcd.width", d.w), "h": num("hw.lcd.height", d.h), "density": num("hw.lcd.density", 0),
            "sensors": d.sensors, "front": d.front, "back": d.back, "foldable": d.foldable, "skin": d.skin, "abis": d.abis, "round": d.round,
            "category": category(d), "ramMB": ram_mb, "deviceRamMB": d.device_ram, "cfg": Value::Object(diff),
        }));
    }
    list.sort_by_key(sort_key);
    let doc = json!({"defaults": defaults, "devices": list});
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(&out, serde_json::to_string(&doc).unwrap()).unwrap();
    eprintln!("gravado {}: {} dispositivos, {} chaves padrão", out.display(), doc["devices"].as_array().unwrap().len(), defaults.len());
}
