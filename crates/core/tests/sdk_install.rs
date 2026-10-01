//! Instalação de pacotes do SDK de ponta a ponta contra um servidor HTTP local
//! (com suporte a Range, como o do Google).

use avdcore::sdk::install::{download_file, extract_zip, package_xml, verify_sha1};
use avdcore::sdk::{manifest, Sdk};
use avdcore::Cancel;
use sha1::{Digest, Sha1};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn tmp(name: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("avdcore-sdk-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Servidor mínimo: GET com Range opcional. Devolve (url, log de Ranges).
fn serve(data: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let ranges = Arc::new(Mutex::new(vec![]));
    let rg = ranges.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let mut rd = BufReader::new(s.try_clone().unwrap());
            let mut range = String::new();
            loop {
                let mut line = String::new();
                if rd.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                    range = v.trim().trim_end_matches('-').to_string();
                }
            }
            rg.lock().unwrap().push(if range.is_empty() { String::new() } else { format!("bytes={range}-") });
            let start: usize = range.parse().unwrap_or(0);
            let body = &data[start.min(data.len())..];
            let head = if range.is_empty() {
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
            } else {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                    body.len(),
                    start,
                    data.len() - 1,
                    data.len()
                )
            };
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(body);
        }
    });
    (format!("http://127.0.0.1:{port}/pt.zip"), ranges)
}

fn make_zip(files: &[(&str, &str, u32)], links: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(vec![]);
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        for (name, content, mode) in files {
            let o = zip::write::SimpleFileOptions::default().unix_permissions(*mode);
            zw.start_file(*name, o).unwrap();
            zw.write_all(content.as_bytes()).unwrap();
        }
        for (name, target) in links {
            zw.add_symlink(*name, *target, zip::write::SimpleFileOptions::default()).unwrap();
        }
        zw.finish().unwrap();
    }
    buf.into_inner()
}

fn fixture() -> manifest::Catalog {
    let src = std::fs::read_to_string(format!("{}/tests/data/repo.xml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (pkgs, lic) = manifest::parse_repo(&src, "https://example.test/repo/").unwrap();
    manifest::Catalog { packages: pkgs, licenses: lic, ..Default::default() }
}

fn point_platform_tools_at(cat: &mut manifest::Catalog, url: &str, zip: &[u8], sha: &str) {
    for p in cat.packages.iter_mut().filter(|p| p.path == "platform-tools") {
        p.base_url = String::new();
        for a in p.archives.iter_mut() {
            a.url = url.to_string();
            a.size = zip.len() as i64;
            a.sha1 = sha.to_string();
        }
    }
}

#[test]
fn install_end_to_end() {
    let zip = make_zip(
        &[("platform-tools/adb", "#!/bin/sh\necho adb\n", 0o755), ("platform-tools/source.properties", "Pkg.Revision=37.0.1\n", 0o644)],
        &[("platform-tools/adb-link", "adb")],
    );
    let sha = hex::encode(Sha1::digest(&zip));
    let (url, _) = serve(zip.clone());
    let mut cat = fixture();
    point_platform_tools_at(&mut cat, &url, &zip, &sha);

    let sdk = Sdk::new(tmp("sdk").join("sdk"));
    let plan = sdk.plan_install(&cat, &["platform-tools".to_string()], false, false);
    assert_eq!(plan.install.len(), 1);
    let cancel = Cancel::new();
    // sem aceitar a licença deve falhar
    assert!(sdk.install(&cat, &plan, &cancel, &mut |_| {}, &|_| {}).is_err());
    for l in &plan.licenses {
        sdk.accept(l).unwrap();
    }
    let mut last = None;
    sdk.install(&cat, &plan, &cancel, &mut |p| last = Some(p), &|_| {}).unwrap();
    let p = last.unwrap();
    assert!((p.overall - 1.0).abs() < 0.001, "progresso final = {}", p.overall);

    let adb = sdk.root.join("platform-tools/adb");
    use std::os::unix::fs::PermissionsExt;
    assert!(std::fs::metadata(&adb).unwrap().permissions().mode() & 0o111 != 0, "adb deve ser executável");
    assert_eq!(std::fs::read_link(sdk.root.join("platform-tools/adb-link")).unwrap(), Path::new("adb"));
    let inst = sdk.installed_map();
    assert_eq!(inst["platform-tools"].rev(), "37.0.1");
    // já instalado => nada a fazer; força => reinstala
    assert!(sdk.plan_install(&cat, &["platform-tools".to_string()], false, false).install.is_empty());
    assert_eq!(sdk.plan_install(&cat, &["platform-tools".to_string()], false, true).install.len(), 1);
    assert!(!sdk.root.join(".downloadIntermediates").exists());
    let leftovers: Vec<_> = std::fs::read_dir(&sdk.root).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".installing")).collect();
    assert!(leftovers.is_empty());

    sdk.uninstall(&["platform-tools".to_string()]).unwrap();
    assert!(!sdk.root.join("platform-tools").exists());
}

#[test]
fn bad_checksum_installs_nothing() {
    let zip = make_zip(&[("platform-tools/adb", "x", 0o755)], &[]);
    let (url, _) = serve(zip.clone());
    let mut cat = fixture();
    point_platform_tools_at(&mut cat, &url, &zip, &"0".repeat(40));
    let sdk = Sdk::new(tmp("sdk2"));
    let plan = sdk.plan_install(&cat, &["platform-tools".to_string()], false, false);
    for l in &plan.licenses {
        sdk.accept(l).unwrap();
    }
    let err = sdk.install(&cat, &plan, &Cancel::new(), &mut |_| {}, &|_| {}).unwrap_err();
    assert!(err.message.contains("checksum"), "{}", err.message);
    assert!(!sdk.root.join("platform-tools").exists());
}

#[test]
fn download_resumes_partial_file() {
    let data: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    let (url, ranges) = serve(data.clone());
    let dir = tmp("dl");
    let dest = dir.join("f.bin");
    std::fs::write(dir.join("f.bin.part"), &data[..12_345]).unwrap(); // download interrompido antes
    download_file(&url, &dest, data.len() as i64, &Cancel::new(), &mut |_, _| {}).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert_eq!(ranges.lock().unwrap().as_slice(), ["bytes=12345-"]);
    verify_sha1(&dest, &hex::encode(Sha1::digest(&data))).unwrap();
    assert!(verify_sha1(&dest, &"1".repeat(40)).is_err());
}

#[test]
fn zip_slip_and_absolute_links_are_refused() {
    let dir = tmp("zs");
    let zp = dir.join("bad.zip");
    // “..” no nome
    let mut buf = std::io::Cursor::new(vec![]);
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        zw.start_file("../evil.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(b"x").unwrap();
        zw.finish().unwrap();
    }
    std::fs::write(&zp, buf.into_inner()).unwrap();
    assert!(extract_zip(&zp, &tmp("dest1"), &Cancel::new(), &mut |_, _| {}).is_err());
    assert!(!dir.join("evil.txt").exists());
    // link simbólico absoluto / que foge da pasta
    for target in ["/etc/passwd", "../../outside"] {
        std::fs::write(&zp, make_zip(&[("ok/a.txt", "x", 0o644)], &[("ok/link", target)])).unwrap();
        assert!(extract_zip(&zp, &tmp("dest2"), &Cancel::new(), &mut |_, _| {}).is_err(), "{target}");
    }
}

#[test]
fn installed_list_follows_symlinks() {
    // SDK com system-images num “outro disco” (link) e raiz que também é link
    let real = tmp("sym");
    let img = real.join("big-disk-images/android-36/google_apis/x86_64");
    std::fs::create_dir_all(&img).unwrap();
    let cat = fixture();
    let mut r = cat.find("system-images;android-36;google_apis_playstore;x86_64", false).unwrap().clone();
    r.path = "system-images;android-36;google_apis;x86_64".into();
    std::fs::write(img.join("package.xml"), package_xml(&r, &avdcore::sdk::License { id: String::new(), text: String::new() })).unwrap();
    let sdk_dir = real.join("sdk");
    std::fs::create_dir_all(&sdk_dir).unwrap();
    std::os::unix::fs::symlink(real.join("big-disk-images"), sdk_dir.join("system-images")).unwrap();
    let link = tmp("linkroot").join("Sdk");
    std::os::unix::fs::symlink(&sdk_dir, &link).unwrap();
    let got = Sdk::new(&link).list_installed();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].path, "system-images;android-36;google_apis;x86_64");
    assert_eq!(got[0].details.abi, "x86_64");
}

#[test]
fn package_xml_round_trip_matches_sdkmanager_shape() {
    let cat = fixture();
    let r = cat.find("system-images;android-36;google_apis_playstore;x86_64", false).unwrap();
    let x = package_xml(r, &cat.licenses["android-sdk-license"]);
    assert!(x.contains(r#"xsi:type="ns14:sysImgDetailsType""#) && x.contains(r#"<dependency path="emulator">"#), "{x}");
    assert!(
        x.contains(r#"<uses-license ref="android-sdk-license"/>"#)
            && x.contains("<localPackage path=\"system-images;android-36;google_apis_playstore;x86_64\" obsolete=\"false\">")
    );
    let root = tmp("px");
    let dir = root.join("system-images/android-36/google_apis_playstore/x86_64");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.xml"), &x).unwrap();
    let inst = Sdk::new(&root).list_installed();
    assert_eq!(inst.len(), 1);
    assert_eq!((inst[0].rev(), inst[0].license_id.as_str(), inst[0].details.abi.as_str()), ("7".to_string(), "android-sdk-license", "x86_64"));
    assert_eq!(inst[0].details.tags[0].display, "Google Play");
    let _ = Read::bytes(std::io::empty());
}
