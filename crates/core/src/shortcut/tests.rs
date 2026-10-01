use super::linux::exec_quote;
use super::*;
use crate::testutil::tmp;
use std::fs;

#[test]
fn quoting() {
    for (i, w) in
        [("simple", "simple"), ("/opt/My SDK/emu", "\"/opt/My SDK/emu\""), ("100%", "100%%"), ("a\"b", "\"a\\\"b\""), ("$HOME", "\"\\$HOME\""), ("", "\"\"")]
    {
        assert_eq!(exec_quote(i), w, "{i:?}");
    }
}

fn png_data(n: usize) -> Vec<u8> {
    icns::encode_png(&icns::Rgba { w: n, h: n, px: vec![10; n * n * 4] }).unwrap()
}

fn setup() -> (Sdk, PathBuf) {
    let data = tmp("data");
    std::env::set_var("XDG_DATA_HOME", &data);
    std::env::set_var("HOME", tmp("home"));
    std::env::set_var("ANDROID_AVD_HOME", tmp("avd"));
    std::env::set_var("PATH", "/usr/bin:/bin");
    let root = tmp("sdk");
    fs::create_dir_all(root.join("emulator")).unwrap();
    fs::write(root.join("emulator/emulator"), "#!/bin/sh\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(root.join("emulator/emulator"), fs::Permissions::from_mode(0o755)).unwrap();
    let img = root.join("system-images/android-36/google_apis/x86_64");
    fs::create_dir_all(&img).unwrap();
    fs::write(img.join("source.properties"), "SystemImage.TagId=google_apis\nSystemImage.Abi=x86_64\nAndroidVersion.ApiLevel=36\n").unwrap();
    let sdk = Sdk::new(root);
    avd::create(
        &sdk,
        &avd::CreateSpec {
            name: "My_AVD".into(),
            device_id: "pixel_9".into(),
            image_pkg: "system-images;android-36;google_apis;x86_64".into(),
            ..Default::default()
        },
    )
    .unwrap();
    (sdk, data)
}

#[test]
fn linux_create_and_remove() {
    if !platform::is_linux() {
        return;
    }
    let _g = crate::testutil::env_lock();
    let (sdk, data) = setup();
    let res = create(&sdk, &Options { avd: "My_AVD".into(), display_name: "Meu Emulador".into(), ..Default::default() }).unwrap();
    let txt = fs::read_to_string(&res.path).unwrap();
    let emu = sdk.emulator().to_string_lossy().into_owned();
    for want in [
        "Name=Meu Emulador".to_string(),
        "StartupWMClass=android-emulator-my-avd".into(),
        "X-AVDMenu-AVD=My_AVD".into(),
        "Exec=env RESOURCE_NAME=android-emulator-my-avd ".into(),
        format!("{emu} -avd My_AVD"),
        "[Desktop Action cold-boot]".into(),
        "-no-snapshot-load".into(),
        "[Desktop Action wipe-data]".into(),
        "-wipe-data".into(),
    ] {
        assert!(txt.contains(&want), "faltou {want:?} em:\n{txt}");
    }
    assert!(lookup("My_AVD").exists);
    assert!(data.join("icons/hicolor/scalable/apps/android-emulator-my-avd.svg").exists());

    // ícone PNG customizado substitui o SVG
    let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png_data(8)));
    create(&sdk, &Options { avd: "My_AVD".into(), icon_data: url, ..Default::default() }).unwrap();
    assert!(data.join("icons/hicolor/256x256/apps/android-emulator-my-avd.png").exists());
    assert!(!data.join("icons/hicolor/scalable/apps/android-emulator-my-avd.svg").exists());

    assert!(create(&sdk, &Options { avd: "Nao_Existe".into(), ..Default::default() }).is_err());
    assert!(create(&sdk, &Options { avd: "My_AVD".into(), icon_data: "data:image/png;base64,naoebase64!!".into(), ..Default::default() }).is_err());
    remove("My_AVD").unwrap();
    assert!(!lookup("My_AVD").exists);
    assert!(remove("My_AVD").is_err());
}

#[test]
fn self_install() {
    if !platform::is_linux() {
        return;
    }
    let _g = crate::testutil::env_lock();
    std::env::set_var("XDG_DATA_HOME", tmp("data2"));
    std::env::set_var("PATH", "/usr/bin:/bin");
    let res = install_self().unwrap();
    let b = fs::read_to_string(&res.path).unwrap();
    assert!(b.contains("StartupWMClass=avd-menu") && b.contains(" ui\n"), "{b}");
    assert!(res.path.ends_with("app.avdmenu.AVDMenu.desktop"), "{}", res.path);
    assert!(self_info().exists);
    uninstall_self().unwrap();
    assert!(!self_info().exists);
}

#[test]
fn mac_bundle() {
    let _g = crate::testutil::env_lock();
    let home = tmp("machome");
    std::env::set_var("HOME", &home);
    std::env::set_var("ANDROID_AVD_HOME", "");
    let root = tmp("macsdk");
    let sdk = Sdk::new(&root);
    let opt = Options { avd: "Pixel_9".into(), ..Default::default() };
    let png = IconSource { data: png_data(8), ext: "png" };
    let res = macos::create(&sdk, &opt, "Meu Pixel/9", &png).unwrap();
    assert_eq!(PathBuf::from(&res.path).file_name().unwrap(), "Meu Pixel-9.app");
    let launcher = PathBuf::from(&res.path).join("Contents/MacOS/launcher");
    let b = fs::read_to_string(&launcher).unwrap();
    assert!(b.contains(&format!("exec '{}' -avd 'Pixel_9'", root.join("emulator/emulator").display())), "{b}");
    use std::os::unix::fs::PermissionsExt;
    assert!(fs::metadata(&launcher).unwrap().permissions().mode() & 0o111 != 0);
    let plist = fs::read_to_string(PathBuf::from(&res.path).join("Contents/Info.plist")).unwrap();
    assert!(plist.contains("<key>AVDMenuAVD</key><string>Pixel_9</string>") && plist.contains("app.avdmenu.avd.pixel-9"));
    assert_eq!(&fs::read(PathBuf::from(&res.path).join("Contents/Resources/icon.icns")).unwrap()[..4], b"icns");
    assert_eq!(macos::lookup("Pixel_9").path, res.path);
    // recriar com outro nome apaga o bundle antigo
    let res2 = macos::create(&sdk, &opt, "Outro Nome", &IconSource { data: ICON_SVG.to_vec(), ext: "svg" }).unwrap();
    assert!(!PathBuf::from(&res.path).exists());
    macos::remove("Pixel_9").unwrap();
    assert!(!PathBuf::from(&res2.path).exists());
}

#[test]
fn desktop_entry_lines_are_localized() {
    use super::linux::localized;
    let name = localized("Name", "Iniciar com cold boot", "Cold boot", None);
    assert!(name.contains(&"Name[pt_BR]=Iniciar com cold boot".to_string()), "{name:?}");
    for prefix in ["Name[zh_CN]=", "Name[zh]=", "Name[ar]=", "Name[ur]="] {
        assert!(name.iter().any(|l| l.starts_with(prefix)), "{prefix} em {name:?}");
    }
    assert!(!name.iter().any(|l| l.starts_with("Name[en")), "o inglês é a chave sem sufixo");
    let comment = localized("Comment", "Inicia o emulador Android {}", "Starts the Android emulator {}", Some("Pixel_9"));
    assert!(comment.iter().any(|l| l.starts_with("Comment[id]=")), "{comment:?}");
    assert!(comment.iter().all(|l| l.contains("Pixel_9") && !l.contains("{}")), "{comment:?}");
}
