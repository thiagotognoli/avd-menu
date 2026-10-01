//! Molduras dos aparelhos (o “Enable Device Frame” do Android Studio).
//!
//! O emulador só desenha a moldura quando o config.ini aponta uma *skin*: `skin.name` +
//! `skin.path` (pasta com o arquivo `layout` e as imagens). `showDeviceFrame` é só um
//! registro do Studio — o emulador o ignora; para esconder a moldura o Studio grava
//! `skin.path=_no_skin`. As skins dos Pixel & cia. não são pacote do SDK: vêm dentro do
//! Studio (`device-art-resources`). Aqui elas são baixadas do espelho oficial do AOSP, numa
//! revisão fixa e com cada arquivo conferido pelo hash do git, e guardadas em
//! `<sdk>/skins/<nome>` — onde o próprio Studio as põe.

use super::Ini;
use crate::sdk::{http, Sdk};
use crate::{devices, Error, Result};
use base64::Engine;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const REPO: &str = "https://android.googlesource.com/platform/tools/adt/idea/+";
/// Revisão do espelho do Studio de onde as molduras são baixadas.
const REV: &str = "7d76b0bf134f98ee4fc96f33542a9667dd403d4a";
const ART: &str = "artwork/resources/device-art-resources";
/// Valor de `skin.path` que o emulador entende como “sem moldura”.
const NO_SKIN: &str = "_no_skin";
/// Depois de uma falha de download, não insiste por este tempo ao iniciar emuladores.
const COOLDOWN: Duration = Duration::from_secs(300);

/// `AVD_MENU_SKINS_URL` troca a origem (um espelho próprio, ou os testes), com a mesma
/// estrutura de pastas do Gitiles.
fn base_url() -> String {
    match std::env::var("AVD_MENU_SKINS_URL") {
        Ok(u) if !u.trim().is_empty() => u.trim().trim_end_matches('/').to_string(),
        _ => format!("{REPO}/{REV}/{ART}"),
    }
}

/// Pasta das molduras do SDK.
pub fn root(sdk: &Sdk) -> PathBuf {
    sdk.root.join("skins")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `1080x2400`: o nome que o emulador usa para “tela sem moldura”.
fn is_size(s: &str) -> bool {
    s.split_once('x').is_some_and(|(w, h)| !w.is_empty() && !h.is_empty() && w.bytes().all(|b| b.is_ascii_digit()) && h.bytes().all(|b| b.is_ascii_digit()))
}

/// Uma skin completa tem `layout` (as dobráveis guardam um por postura, em `default/` e `closed/`).
fn has_layout(dir: &Path) -> bool {
    dir.join("layout").is_file() || dir.join("default/layout").is_file()
}

pub fn installed(sdk: &Sdk, id: &str) -> bool {
    valid_id(id) && has_layout(&root(sdk).join(id))
}

/// A skin que o config.ini já usa existe em disco? (`skin.path` pode ser relativo ao SDK.)
fn current_ok(cfg: &Ini, sdk: &Sdk) -> bool {
    let p = cfg.get("skin.path");
    if p.is_empty() || p == NO_SKIN {
        return false;
    }
    let p = Path::new(p);
    has_layout(&if p.is_absolute() { p.to_path_buf() } else { sdk.root.join(p) })
}

/// A moldura que o config.ini pede: só com “mostrar moldura” ligado, a skin que já está
/// nele ou, na falta dela, a do perfil de hardware do aparelho (`None` se ele não tem).
pub fn wanted(cfg: &Ini) -> Option<String> {
    if cfg.get("showDeviceFrame") != "yes" {
        return None;
    }
    let name = cfg.get("skin.name");
    if valid_id(name) && !is_size(name) {
        return Some(name.to_string());
    }
    devices::get(cfg.get("hw.device.name")).and_then(|d| d.skin).filter(|s| valid_id(s))
}

/// A moldura que falta baixar para o config.ini mostrar a moldura pedida.
pub fn missing(cfg: &Ini, sdk: &Sdk) -> Option<String> {
    let id = wanted(cfg)?;
    if current_ok(cfg, sdk) || installed(sdk, &id) {
        None
    } else {
        Some(id)
    }
}

/// Deixa `skin.name`/`skin.path` de acordo com “mostrar moldura”, só com o que já está em
/// disco (não baixa nada). Devolve `true` se mexeu no config.
pub fn sync(cfg: &mut Ini, sdk: &Sdk) -> bool {
    let before = (cfg.get("skin.name").to_string(), cfg.get("skin.path").to_string());
    match cfg.get("showDeviceFrame") {
        "no" => {
            let path = cfg.get("skin.path");
            if !path.is_empty() && path != NO_SKIN {
                let (w, h) = (cfg.get("hw.lcd.width"), cfg.get("hw.lcd.height"));
                if !w.is_empty() && !h.is_empty() {
                    let size = format!("{w}x{h}");
                    cfg.set("skin.name", &size);
                }
                cfg.set("skin.path", NO_SKIN);
            }
        }
        "yes" if !current_ok(cfg, sdk) => {
            if let Some(id) = wanted(cfg).filter(|id| installed(sdk, id)) {
                cfg.set("skin.name", &id);
                cfg.set("skin.path", &root(sdk).join(&id).to_string_lossy());
            }
        }
        _ => {}
    }
    before != (cfg.get("skin.name").to_string(), cfg.get("skin.path").to_string())
}

/// Baixa a moldura `id` para `<sdk>/skins/<id>` (não faz nada se já está lá).
pub fn download(sdk: &Sdk, id: &str) -> Result<()> {
    download_from(&base_url(), sdk, id)
}

pub(crate) fn download_from(base: &str, sdk: &Sdk, id: &str) -> Result<()> {
    if !valid_id(id) || is_size(id) {
        return Err(bad!("nome de moldura inválido: {:?}", "invalid device frame name: {:?}", id));
    }
    let dest = root(sdk).join(id);
    if has_layout(&dest) {
        return Ok(());
    }
    let wrap = |e: Error| Error::new(502, trf!("não foi possível baixar a moldura {:?}: {}", "could not download the device frame {:?}: {}", id, e.message));
    std::fs::create_dir_all(root(sdk)).map_err(|e| wrap(e.into()))?;
    let tmp = root(sdk).join(format!(".{id}.part-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let res = fetch_tree(base, id, &tmp, 0).and_then(|_| {
        if has_layout(&tmp) {
            Ok(())
        } else {
            Err(Error::new(502, crate::lang::tr("a moldura veio sem o arquivo layout", "the frame came without a layout file")))
        }
    });
    let res = res.and_then(|_| {
        // outra thread pode ter instalado enquanto baixávamos; uma pasta quebrada é trocada
        if has_layout(&dest) {
            return Ok(());
        }
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest).map_err(Error::from)
    });
    let _ = std::fs::remove_dir_all(&tmp);
    res.map_err(wrap)
}

struct Entry {
    tree: bool,
    sha: String,
    name: String,
}

fn safe_name(n: &str) -> bool {
    !n.is_empty() && n != "." && n != ".." && n.len() <= 100 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn fetch_tree(base: &str, rel: &str, dest: &Path, depth: u8) -> Result<()> {
    if depth > 3 {
        return Err(Error::new(502, "too deep"));
    }
    std::fs::create_dir_all(dest)?;
    let listing = get(base, &format!("{rel}/"))?;
    for e in parse_listing(&String::from_utf8_lossy(&listing))? {
        let path = format!("{rel}/{}", e.name);
        let target = dest.join(&e.name);
        if e.tree {
            fetch_tree(base, &path, &target, depth + 1)?;
            continue;
        }
        let data = get(base, &path)?;
        let sum = hex::encode(Sha1::new().chain_update(format!("blob {}\0", data.len())).chain_update(&data).finalize());
        if !sum.eq_ignore_ascii_case(&e.sha) {
            return Err(Error::new(502, format!("{path}: checksum")));
        }
        std::fs::write(&target, &data)?;
    }
    Ok(())
}

/// Listagem de uma pasta no Gitiles: `<modo> <blob|tree> <sha>\t<nome>` por linha.
fn parse_listing(text: &str) -> Result<Vec<Entry>> {
    let mut out = vec![];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let bad_line = || Error::new(502, format!("listing: {line:?}"));
        let (meta, name) = line.split_once('\t').ok_or_else(bad_line)?;
        let mut it = meta.split_whitespace();
        let (_mode, kind, sha) = (it.next(), it.next().ok_or_else(bad_line)?, it.next().ok_or_else(bad_line)?);
        if !safe_name(name) || !matches!(kind, "blob" | "tree") {
            return Err(bad_line());
        }
        out.push(Entry { tree: kind == "tree", sha: sha.to_string(), name: name.to_string() });
    }
    Ok(out)
}

/// Conteúdo de um arquivo (ou listagem de pasta, se `rel` termina em `/`) do Gitiles, que
/// serve tudo em base64 com `?format=TEXT`. O servidor limita o ritmo (429/503): tenta de novo.
fn get(base: &str, rel: &str) -> Result<Vec<u8>> {
    let url = format!("{base}/{rel}?format=TEXT");
    let mut last = String::new();
    for attempt in 0..4u32 {
        if attempt > 0 {
            std::thread::sleep(retry_delay(attempt));
        }
        let mut resp = match http::short().get(&url).call() {
            Ok(r) => r,
            Err(e) => return Err(Error::new(502, format!("{url}: {e}"))),
        };
        let st = resp.status().as_u16();
        if matches!(st, 429 | 500 | 502 | 503 | 504) {
            last = format!("{url}: HTTP {st}");
            continue;
        }
        if st != 200 {
            return Err(Error::new(502, format!("{url}: HTTP {st}")));
        }
        let body = resp.body_mut().with_config().limit(48 << 20).read_to_vec().map_err(|e| Error::new(502, format!("{url}: {e}")))?;
        let clean: Vec<u8> = body.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
        return base64::engine::general_purpose::STANDARD.decode(clean).map_err(|e| Error::new(502, format!("{url}: {e}")));
    }
    Err(Error::new(502, last))
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_millis(if cfg!(test) { 5 } else { 1500 * u64::from(attempt) })
}

static LAST_FAILURE: Mutex<Option<Instant>> = Mutex::new(None);

/// Chamado antes de iniciar o emulador: baixa a moldura que falta (sem insistir logo depois
/// de uma falha, para não atrasar a abertura sem rede) e acerta o config.ini do AVD.
/// O config é acertado mesmo quando o download falha; o erro é devolvido no fim.
pub fn prepare(sdk: &Sdk, name: &str) -> Result<()> {
    // sem config legível o próprio emulador reclama
    let Ok((mut cfg, path)) = super::config(name) else { return Ok(()) };
    let mut failure = None;
    if let Some(id) = missing(&cfg, sdk) {
        let cooling = LAST_FAILURE.lock().ok().and_then(|g| *g).is_some_and(|t| t.elapsed() < COOLDOWN);
        if !cooling {
            if let Err(e) = download(sdk, &id) {
                if let Ok(mut g) = LAST_FAILURE.lock() {
                    *g = Some(Instant::now());
                }
                failure = Some(e);
            }
        }
    }
    if sync(&mut cfg, sdk) {
        std::fs::write(&path, cfg.to_text())?;
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tmp;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn git_sha(data: &[u8]) -> String {
        hex::encode(Sha1::new().chain_update(format!("blob {}\0", data.len())).chain_update(data).finalize())
    }

    /// Gitiles de mentira: `files` = (caminho, conteúdo); o primeiro pedido de cada URL
    /// pode falhar com 503 (`flaky`) para exercitar a nova tentativa.
    fn serve(files: Vec<(&'static str, Vec<u8>)>, flaky: bool, bad_hash: bool) -> (String, Arc<AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            let mut seen = std::collections::HashSet::new();
            for mut s in l.incoming().flatten() {
                let mut line = String::new();
                let mut rd = BufReader::new(s.try_clone().unwrap());
                rd.read_line(&mut line).unwrap();
                loop {
                    let mut x = String::new();
                    if rd.read_line(&mut x).unwrap_or(0) == 0 || x == "\r\n" {
                        break;
                    }
                }
                h.fetch_add(1, Ordering::SeqCst);
                let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let path = target.trim_start_matches('/').split('?').next().unwrap().to_string();
                if flaky && seen.insert(target.clone()) {
                    let _ = s.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let body: Option<String> = if path.ends_with('/') {
                    // listagem da pasta
                    let dir = path.trim_end_matches('/');
                    let mut rows: Vec<String> = vec![];
                    let mut subdirs = std::collections::BTreeSet::new();
                    for (p, data) in &files {
                        let Some(rest) = p.strip_prefix(&format!("{dir}/")) else { continue };
                        match rest.split_once('/') {
                            None => {
                                let sha = if bad_hash { "0".repeat(40) } else { git_sha(data) };
                                rows.push(format!("100644 blob {sha}\t{rest}"));
                            }
                            Some((d, _)) => {
                                subdirs.insert(d.to_string());
                            }
                        }
                    }
                    for d in subdirs {
                        rows.push(format!("040000 tree {}\t{d}", "1".repeat(40)));
                    }
                    if rows.is_empty() {
                        None
                    } else {
                        Some(base64::engine::general_purpose::STANDARD.encode(rows.join("\n") + "\n"))
                    }
                } else {
                    files.iter().find(|(p, _)| *p == path).map(|(_, d)| base64::engine::general_purpose::STANDARD.encode(d))
                };
                let resp = match body {
                    Some(b) => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (base, hits)
    }

    fn fake_files() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("pixel_8/layout", b"parts {\n}\n".to_vec()),
            ("pixel_8/back.webp", vec![1, 2, 3, 4, 5]),
            ("pixel_8/mask.webp", vec![9; 3000]),
            ("pixel_fold/default/layout", b"parts {}\n".to_vec()),
            ("pixel_fold/default/back.webp", vec![7; 10]),
            ("pixel_fold/closed/layout", b"parts {}\n".to_vec()),
            ("empty_skin/back.webp", vec![1]),
        ]
    }

    fn cfg_of(text: &str) -> Ini {
        Ini::parse(text)
    }

    #[test]
    fn ids_and_sizes() {
        assert!(valid_id("pixel_8") && valid_id("wearos_small_round"));
        assert!(!valid_id("") && !valid_id("../x") && !valid_id("Pixel") && !valid_id("a b"));
        assert!(is_size("1080x2400") && !is_size("pixel_8") && !is_size("x2") && !is_size("10x"));
    }

    #[test]
    fn wanted_follows_the_flag_and_the_device() {
        // sem a chave, ou com "no": nenhuma moldura
        assert_eq!(wanted(&cfg_of("hw.device.name=pixel_8\n")), None);
        assert_eq!(wanted(&cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=no\n")), None);
        // moldura do perfil de hardware
        assert_eq!(wanted(&cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=yes\n")).as_deref(), Some("pixel_8"));
        // a skin que já está no config (Studio) vale mais; WxH não é skin
        assert_eq!(wanted(&cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=yes\nskin.name=pixel_7\n")).as_deref(), Some("pixel_7"));
        assert_eq!(wanted(&cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=yes\nskin.name=1080x2400\n")).as_deref(), Some("pixel_8"));
        // aparelho sem skin
        assert_eq!(wanted(&cfg_of("hw.device.name=medium_phone\nshowDeviceFrame=yes\n")), None);
    }

    #[test]
    fn sync_attaches_and_hides() {
        let sdk = Sdk::new(tmp("skin-sdk"));
        let mut c = cfg_of("hw.device.name=pixel_8\nhw.lcd.width=1080\nhw.lcd.height=2400\nshowDeviceFrame=yes\n");
        // skin ainda não instalada: nada a fazer, e o que falta é a do pixel_8
        assert!(!sync(&mut c, &sdk));
        assert_eq!(missing(&c, &sdk).as_deref(), Some("pixel_8"));
        // instalada: o config passa a apontar para ela
        let dir = root(&sdk).join("pixel_8");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("layout"), "parts {}").unwrap();
        assert!(sync(&mut c, &sdk));
        assert_eq!(c.get("skin.name"), "pixel_8");
        assert_eq!(c.get("skin.path"), dir.to_string_lossy());
        assert_eq!(missing(&c, &sdk), None);
        assert!(!sync(&mut c, &sdk), "idempotente");
        // desligar esconde: o emulador ignora showDeviceFrame, só obedece skin.path
        c.set("showDeviceFrame", "no");
        assert!(sync(&mut c, &sdk));
        assert_eq!((c.get("skin.name"), c.get("skin.path")), ("1080x2400", "_no_skin"));
        assert!(!sync(&mut c, &sdk));
        // sem nenhuma skin no config e com a moldura desligada, não inventa chaves
        let mut d = cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=no\n");
        assert!(!sync(&mut d, &sdk));
        assert!(!d.has("skin.path"));
        // ligar de novo volta a skin do aparelho
        c.set("showDeviceFrame", "yes");
        assert!(sync(&mut c, &sdk));
        assert_eq!((c.get("skin.name"), c.get("skin.path")), ("pixel_8", dir.to_string_lossy().as_ref()));
    }

    #[test]
    fn existing_studio_skin_is_left_alone() {
        let sdk = Sdk::new(tmp("skin-sdk"));
        let studio = tmp("studio-skin");
        std::fs::write(studio.join("layout"), "parts {}").unwrap();
        let mut c = cfg_of(&format!("hw.device.name=pixel_8\nshowDeviceFrame=yes\nskin.name=pixel_8\nskin.path={}\n", studio.display()));
        assert!(!sync(&mut c, &sdk));
        assert_eq!(missing(&c, &sdk), None);
        // skin.path relativo é relativo ao SDK
        std::fs::create_dir_all(sdk.root.join("skins/pixel_8")).unwrap();
        std::fs::write(sdk.root.join("skins/pixel_8/layout"), "x").unwrap();
        let c = cfg_of("hw.device.name=pixel_8\nshowDeviceFrame=yes\nskin.name=pixel_8\nskin.path=skins/pixel_8\n");
        assert_eq!(missing(&c, &sdk), None);
    }

    #[test]
    fn downloads_a_skin_and_verifies_it() {
        let (base, hits) = serve(fake_files(), true, false);
        let sdk = Sdk::new(tmp("skin-dl"));
        download_from(&base, &sdk, "pixel_8").unwrap();
        let d = root(&sdk).join("pixel_8");
        assert_eq!(std::fs::read(d.join("back.webp")).unwrap(), vec![1, 2, 3, 4, 5]);
        assert_eq!(std::fs::read(d.join("mask.webp")).unwrap().len(), 3000);
        assert!(installed(&sdk, "pixel_8"));
        assert!(std::fs::read_dir(root(&sdk)).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')), "sem sobras");
        // já instalada: não volta à rede
        let n = hits.load(Ordering::SeqCst);
        download_from(&base, &sdk, "pixel_8").unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), n);
        // dobrável: subpastas default/ e closed/
        download_from(&base, &sdk, "pixel_fold").unwrap();
        assert!(root(&sdk).join("pixel_fold/default/layout").is_file() && root(&sdk).join("pixel_fold/closed/layout").is_file());
        assert!(installed(&sdk, "pixel_fold"));
    }

    #[test]
    fn download_failures_leave_nothing_behind() {
        let sdk = Sdk::new(tmp("skin-fail"));
        let list_leftovers = |sdk: &Sdk| std::fs::read_dir(root(sdk)).map(|r| r.count()).unwrap_or(0);
        // hash do git não bate
        let (base, _) = serve(fake_files(), false, true);
        let e = download_from(&base, &sdk, "pixel_8").unwrap_err();
        assert!(e.message.contains("pixel_8") && e.message.contains("checksum"), "{e}");
        assert_eq!(list_leftovers(&sdk), 0);
        // inexistente
        let (base, _) = serve(fake_files(), false, false);
        assert!(download_from(&base, &sdk, "pixel_99").is_err());
        // skin sem layout
        let e = download_from(&base, &sdk, "empty_skin").unwrap_err();
        assert!(e.message.contains("layout"), "{e}");
        assert_eq!(list_leftovers(&sdk), 0);
        // nomes inválidos nunca viram caminho
        for bad in ["", "../x", "a/b", "1080x2400", "Pixel_8"] {
            assert!(download_from(&base, &sdk, bad).is_err(), "{bad}");
        }
        // sem rede
        assert!(download_from("http://127.0.0.1:1", &sdk, "pixel_8").is_err());
    }

    #[test]
    fn rejects_unsafe_listings() {
        for line in ["100644 blob abc\t../evil", "100644 blob abc\ta/b", "100644 commit abc\tx", "sem tabulação", "100644 blob\tx"] {
            assert!(parse_listing(line).is_err(), "{line}");
        }
        let ok = parse_listing("100644 blob 6c19\tback.webp\n040000 tree 7e0c\tclosed\n").unwrap();
        assert_eq!((ok[0].name.as_str(), ok[0].tree, ok[1].tree), ("back.webp", false, true));
    }
}
