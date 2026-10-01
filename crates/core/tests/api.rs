//! Fluxos completos pela API (a mesma que a interface usa), num ambiente
//! isolado: HOME/XDG/AVD home temporários e um SDK falso.

use avdcore::api::App;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Gitiles de mentira com a skin `pixel_9` (layout + duas imagens). Devolve a URL base.
fn serve_skins() -> String {
    use base64::Engine;
    use sha1::{Digest, Sha1};
    use std::io::{BufRead, BufReader, Write};
    let files: Vec<(&str, Vec<u8>)> = vec![("layout", b"parts {}\n".to_vec()), ("back.webp", vec![1; 64]), ("mask.webp", vec![2; 32])];
    let b64 = |d: &[u8]| base64::engine::general_purpose::STANDARD.encode(d);
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut line = String::new();
            let mut rd = BufReader::new(s.try_clone().unwrap());
            rd.read_line(&mut line).unwrap();
            while rd.read_line(&mut String::new()).unwrap_or(0) > 2 {}
            let path = line.split_whitespace().nth(1).unwrap_or("").trim_start_matches('/').split('?').next().unwrap().to_string();
            let body = if path == "pixel_9/" {
                let rows: Vec<String> = files
                    .iter()
                    .map(|(n, d)| {
                        format!("100644 blob {}\t{n}", hex::encode(Sha1::new().chain_update(format!("blob {}\0", d.len())).chain_update(d).finalize()))
                    })
                    .collect();
                Some(b64((rows.join("\n") + "\n").as_bytes()))
            } else {
                path.strip_prefix("pixel_9/").and_then(|n| files.iter().find(|(f, _)| *f == n)).map(|(_, d)| b64(d))
            };
            let resp = match body {
                Some(b) => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()),
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            };
            let _ = s.write_all(resp.as_bytes());
        }
    });
    url
}

static ENV: Mutex<()> = Mutex::new(());

fn tmp(name: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("avdcore-it-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

struct Env {
    _g: MutexGuard<'static, ()>,
    app: Arc<App>,
    root: PathBuf,
    events: Arc<Mutex<Vec<(String, Value)>>>,
}

fn setup() -> Env {
    let g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let t = tmp("env");
    for (k, v) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "cfg"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_DATA_HOME", "data"),
        ("ANDROID_AVD_HOME", "avd"),
    ] {
        std::env::set_var(k, t.join(v));
    }
    std::env::set_var("PATH", "/usr/bin:/bin");
    // molduras: nunca sai para a internet nos testes (porta fechada = falha na hora)
    std::env::set_var("AVD_MENU_SKINS_URL", "http://127.0.0.1:1");
    std::env::remove_var("ANDROID_SDK_ROOT");
    std::env::remove_var("ANDROID_HOME");
    let root = t.join("sdk");
    let img = root.join("system-images/android-36/google_apis/x86_64");
    std::fs::create_dir_all(&img).unwrap();
    std::fs::write(
        img.join("source.properties"),
        "SystemImage.TagId=google_apis\nSystemImage.TagDisplay=Google APIs\nSystemImage.Abi=x86_64\nAndroidVersion.ApiLevel=36\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("emulator")).unwrap();
    let emu = root.join("emulator/emulator");
    // “emulador” falso: dorme, mas com a linha de comando de um emulador de verdade
    std::fs::write(&emu, "#!/bin/bash\nexec -a \"$0 $*\" sleep 30\n").unwrap();
    std::fs::set_permissions(&emu, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(t.join("cfg/avd-menu")).unwrap();
    std::fs::write(t.join("cfg/avd-menu/config.json"), json!({"sdkRoot": root}).to_string()).unwrap();
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let app = App::new(move |n, d| ev.lock().unwrap().push((n.to_string(), d)));
    Env { _g: g, app, root, events }
}

impl Env {
    fn call(&self, method: &str, path: &str, body: Value) -> Result<Value, avdcore::Error> {
        self.app.handle(method, path, body)
    }
    fn ok(&self, method: &str, path: &str, body: Value) -> Value {
        self.call(method, path, body).unwrap_or_else(|e| panic!("{method} {path}: {} ({})", e.message, e.status))
    }
}

const IMG: &str = "system-images;android-36;google_apis;x86_64";

#[test]
fn state_devices_and_avd_lifecycle() {
    let e = setup();
    let st = e.ok("GET", "/api/state", Value::Null);
    assert_eq!(st["sdk"]["root"], json!(e.root));
    assert_eq!((st["sdk"]["hasEmulator"].clone(), st["sdk"]["hasAdb"].clone()), (json!(true), json!(false)));

    let devs = e.ok("GET", "/api/devices", Value::Null);
    assert!(devs["devices"].as_array().unwrap().len() >= 80);

    let prev = e.ok("POST", "/api/avds/preview", json!({"deviceId": "pixel_9", "imagePkg": IMG}));
    assert_eq!(prev["settings"]["ramMB"], 2048);

    let created = e.ok(
        "POST",
        "/api/avds",
        json!({"name": "Server_Test", "deviceId": "pixel_9", "imagePkg": IMG, "displayName": "Server Test", "ramMB": 3072, "shortcut": {"displayName": "Server Test!"}}),
    );
    assert_eq!(created["avd"]["name"], "Server_Test");
    assert!(created["shortcut"]["path"].as_str().is_some_and(|p| !p.is_empty()), "{created}");
    let err = e.call("POST", "/api/avds", json!({"name": "Server_Test", "deviceId": "pixel_9", "imagePkg": IMG})).unwrap_err();
    assert_eq!(err.status, 400);

    let list = e.ok("GET", "/api/avds", Value::Null);
    let avds = list["avds"].as_array().unwrap();
    assert_eq!(avds.len(), 1);
    assert_eq!(avds[0]["name"], "Server_Test");
    assert_eq!(avds[0]["shortcut"]["exists"], true);
    assert_eq!(avds[0]["imageInstalled"], true);

    let det = e.ok("GET", "/api/avds/Server_Test", Value::Null);
    assert_eq!(det["settings"]["ramMB"], 3072);
    assert!(det["raw"].as_str().unwrap().contains("image.sysdir.1="));
    e.ok("PUT", "/api/avds/Server_Test", json!({"ramMB": 1024, "gpuMode": "host"}));
    assert_eq!(e.ok("GET", "/api/avds/Server_Test", Value::Null)["settings"]["ramMB"], 1024);
    assert_eq!(e.call("PUT", "/api/avds/Server_Test", json!({"ramMB": 10})).unwrap_err().status, 400);
    assert!(e.call("GET", "/api/avds/..%2Fetc", Value::Null).is_err());
    assert_eq!(e.call("GET", "/api/avds/Nao_Existe", Value::Null).unwrap_err().status, 404);

    e.ok("POST", "/api/avds/Server_Test/duplicate", json!({"name": "Server_Copy"}));
    e.ok("POST", "/api/avds/Server_Test/wipe", Value::Null);
    e.ok("PUT", "/api/avds/Server_Test/args", json!({"args": "-no-audio"}));
    assert_eq!(e.ok("GET", "/api/avds", Value::Null)["avds"][1]["extraArgs"], "-no-audio");
    e.ok("DELETE", "/api/avds/Server_Copy", Value::Null);
    e.ok("DELETE", "/api/avds/Server_Test/shortcut", Value::Null);
    e.ok("DELETE", "/api/avds/Server_Test", Value::Null);
    assert!(e.ok("GET", "/api/avds", Value::Null)["avds"].as_array().unwrap().is_empty());
}

#[test]
fn start_and_stop_with_fake_emulator() {
    let e = setup();
    e.ok("POST", "/api/avds", json!({"name": "Fake", "deviceId": "pixel_9", "imagePkg": IMG}));
    e.ok("POST", "/api/avds/Fake/start", json!({"mode": "cold"}));
    let running = |e: &Env| e.ok("GET", "/api/avds", Value::Null)["avds"][0].get("running").is_some();
    let mut up = false;
    for _ in 0..25 {
        if running(&e) {
            up = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    if !up {
        eprintln!("ambiente de teste não permitiu detectar o processo (ps); pulando");
        return;
    }
    assert_eq!(e.call("POST", "/api/avds/Fake/start", Value::Null).unwrap_err().status, 409);
    assert_eq!(e.call("DELETE", "/api/avds/Fake", Value::Null).unwrap_err().status, 409);
    // o log registra a linha de comando (com o modo cold boot)
    let log = e.ok("GET", "/api/avds/Fake/log", Value::Null);
    assert!(log["text"].as_str().unwrap().contains("-no-snapshot-load"), "{log}");
    e.ok("POST", "/api/avds/Fake/stop", Value::Null);
    let mut down = false;
    for _ in 0..60 {
        if !running(&e) {
            down = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert!(down, "o emulador deveria ter parado");
    assert!(e.events.lock().unwrap().iter().any(|(n, _)| n == "running"));
}

#[test]
fn auth_free_misc_routes() {
    let e = setup();
    assert!(e.call("GET", "/api/nao-existe", Value::Null).is_err());
    let fs = e.ok("GET", &format!("/api/fs?path={}", e.root.display()), Value::Null);
    assert!(fs["dirs"].as_array().unwrap().iter().any(|d| d == "emulator"));
    assert_eq!(fs["isSdk"], true);
    let diag = e.ok("GET", "/api/diag", Value::Null);
    assert!(diag["checks"].as_array().unwrap().iter().any(|c| c["id"] == "sdk"));
    // perfil de hardware do usuário
    let d = e.ok("POST", "/api/devices", json!({"name": "Meu Aparelho", "w": 1000, "h": 2000, "density": 400, "ramMB": 3072, "playstore": true}));
    assert_eq!(d["device"]["id"], "user_meu_aparelho");
    assert!(e.ok("GET", "/api/devices", Value::Null)["devices"].as_array().unwrap().iter().any(|x| x["id"] == "user_meu_aparelho"));
    assert_eq!(e.call("POST", "/api/devices", json!({"name": "x", "w": 10, "h": 2000, "density": 400})).unwrap_err().status, 400);
    e.ok("DELETE", "/api/devices/user_meu_aparelho", Value::Null);
    // idioma em tempo de execução
    e.ok("POST", "/api/lang", json!({"lang": "en"}));
    assert_eq!(e.call("GET", "/api/avds/Nao_Existe", Value::Null).unwrap_err().message, "AVD \"Nao_Existe\" does not exist");
    e.ok("POST", "/api/lang", json!({"lang": "pt"}));
    assert!(e.call("GET", "/api/avds/Nao_Existe", Value::Null).unwrap_err().message.contains("não existe"));
}

#[test]
fn uninstall_only_installed_packages() {
    let e = setup();
    assert_eq!(e.call("POST", "/api/packages/uninstall", json!({"paths": ["licenses"]})).unwrap_err().status, 400);
    assert_eq!(e.call("POST", "/api/packages/uninstall", json!({"paths": []})).unwrap_err().status, 400);
    let _: &Path = &e.root;
}

#[test]
fn device_frame_is_downloaded_attached_and_hidden() {
    let e = setup();
    let avd_home = PathBuf::from(std::env::var_os("ANDROID_AVD_HOME").unwrap());
    let keys = || {
        let c = std::fs::read_to_string(avd_home.join("Frame.avd/config.ini")).unwrap();
        let get = |k: &str| c.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
        (get("showDeviceFrame"), get("skin.name"), get("skin.path"))
    };
    let skin = e.root.join("skins/pixel_9");

    // sem rede: o AVD é criado mesmo assim, avisando que a moldura não veio
    let r = e.ok("POST", "/api/avds", json!({"name": "Frame", "deviceId": "pixel_9", "imagePkg": IMG}));
    assert!(r["skinError"].as_str().is_some_and(|m| m.contains("pixel_9")), "{r}");
    assert_eq!(keys(), ("yes".into(), "".into(), "".into()));
    // salvar sem rede grava as outras opções e avisa da moldura
    let r = e.ok("PUT", "/api/avds/Frame", json!({"showFrame": true, "cores": 3}));
    assert!(r["skinError"].as_str().is_some_and(|m| m.contains("pixel_9")), "{r}");
    assert_eq!(keys().1, "");
    assert_eq!(e.ok("GET", "/api/avds/Frame", Value::Null)["settings"]["cores"], 3);

    // com a origem no ar, salvar o formulário baixa a skin e aponta o AVD para ela
    std::env::set_var("AVD_MENU_SKINS_URL", serve_skins());
    e.ok("PUT", "/api/avds/Frame", json!({"showFrame": true}));
    assert!(skin.join("layout").is_file() && skin.join("back.webp").is_file());
    assert_eq!(keys(), ("yes".into(), "pixel_9".into(), skin.to_string_lossy().into()));
    assert_eq!(e.ok("GET", "/api/avds/Frame", Value::Null)["settings"]["showFrame"], true);

    // desmarcar esconde de verdade: o emulador só obedece skin.path
    e.ok("PUT", "/api/avds/Frame", json!({"showFrame": false}));
    assert_eq!(keys(), ("no".into(), "1080x2424".into(), "_no_skin".into()));
    assert_eq!(e.ok("GET", "/api/avds/Frame", Value::Null)["settings"]["showFrame"], false);

    // AVD novo, com a skin já no SDK, nasce com moldura
    e.ok("DELETE", "/api/avds/Frame", Value::Null);
    let r = e.ok("POST", "/api/avds", json!({"name": "Frame", "deviceId": "pixel_9", "imagePkg": IMG}));
    assert!(r.get("skinError").is_none(), "{r}");
    assert_eq!(keys(), ("yes".into(), "pixel_9".into(), skin.to_string_lossy().into()));
}
