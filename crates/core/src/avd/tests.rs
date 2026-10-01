use super::*;
use crate::sdk::Sdk;
use crate::testutil::tmp;
use std::fs;
use std::path::PathBuf;

/// SDK mínimo com uma imagem de sistema instalada + AVD home isolado.
fn fake_sdk() -> (Sdk, PathBuf) {
    let root = tmp("sdk");
    let dir = root.join("system-images/android-36/google_apis_playstore/x86_64");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("source.properties"),
        "AndroidVersion.ApiLevel=36\nSystemImage.Abi=x86_64\nSystemImage.TagId=google_apis_playstore\nSystemImage.TagDisplay=Google Play\n",
    )
    .unwrap();
    let avd_home = tmp("avdhome");
    std::env::set_var("ANDROID_AVD_HOME", &avd_home);
    std::env::set_var("HOME", tmp("home"));
    (Sdk::new(root), avd_home)
}

const IMG: &str = "system-images;android-36;google_apis_playstore;x86_64";

#[test]
fn parse_mb_units() {
    for (i, w) in [("2G", 2048), ("2048", 2048), ("512 MB", 512), ("512M", 512), ("1.5G", 1536), ("", 0), ("x", 0), ("2048K", 2)] {
        assert_eq!(parse_mb(i), w, "{i:?}");
    }
}

#[test]
fn names() {
    for ok in ["Pixel_9_API_36", "a.b-c", "X"] {
        assert!(valid_name(ok), "{ok}");
    }
    for bad in ["", "a b", "../x", ".hidden", "a/b", "ação"] {
        assert!(!valid_name(bad), "{bad}");
    }
    assert_eq!(sanitize_name("Pixel 9 Pro (API 36)"), "Pixel_9_Pro_API_36");
}

#[test]
fn create_list_update_duplicate_delete() {
    let _g = crate::testutil::env_lock();
    let (sdk, _) = fake_sdk();
    let spec = CreateSpec {
        name: "Meu_Pixel".into(),
        device_id: "pixel_9".into(),
        image_pkg: IMG.into(),
        settings: Settings { display_name: Some("Meu Pixel".into()), ram_mb: Some(4096), cores: Some(6), ..Default::default() },
    };
    let a = create(&sdk, &spec).unwrap();
    assert_eq!((a.display_name.as_str(), a.api.as_str(), a.abi.as_str(), a.tag.as_str()), ("Meu Pixel", "36", "x86_64", "google_apis_playstore"));
    assert!(a.play_store && a.image_ok);
    assert_eq!((a.width, a.height, a.density, a.ram_mb), (1080, 2424, 420, 4096));
    assert_eq!((a.device_name.as_str(), a.api_name.as_str()), ("Pixel 9", "Android 16"));

    let (cfg, _) = config("Meu_Pixel").unwrap();
    for (k, w) in [
        ("image.sysdir.1", "system-images/android-36/google_apis_playstore/x86_64/"),
        ("hw.cpu.ncore", "6"),
        ("hw.ramSize", "4096"),
        ("abi.type", "x86_64"),
        ("hw.cpu.arch", "x86_64"),
        ("hw.device.name", "pixel_9"),
        ("tag.id", "google_apis_playstore"),
        ("hw.camera.back", "virtualscene"),
        ("hw.camera.front", "emulated"),
        ("AvdId", "Meu_Pixel"),
        ("target", "android-36"),
    ] {
        assert_eq!(cfg.get(k), w, "{k}");
    }
    let ini = fs::read_to_string(home().join("Meu_Pixel.ini")).unwrap();
    assert!(ini.contains("target=android-36") && ini.contains(&format!("path={}", a.dir)), "{ini}");

    assert!(create(&sdk, &spec).is_err(), "não deve recriar");
    assert!(create(
        &sdk,
        &CreateSpec { name: "x".into(), device_id: "pixel_9".into(), image_pkg: "system-images;android-1;default;x86".into(), ..Default::default() }
    )
    .is_err());
    assert!(
        create(&sdk, &CreateSpec { name: "w".into(), device_id: "wearos_small_round".into(), image_pkg: IMG.into(), ..Default::default() }).is_err(),
        "imagem de celular não serve para Wear OS"
    );
    assert!(create(&sdk, &CreateSpec { name: "bad name".into(), device_id: "pixel_9".into(), image_pkg: IMG.into(), ..Default::default() }).is_err());

    // edição
    update(
        &sdk,
        "Meu_Pixel",
        &Settings { gpu_mode: Some("host".into()), boot_mode: Some("cold".into()), keyboard: Some(true), sd_card_mb: Some(1024), ..Default::default() },
    )
    .unwrap();
    let v = view_of(&config("Meu_Pixel").unwrap().0, "Meu_Pixel");
    assert_eq!((v.gpu_mode.as_str(), v.boot_mode.as_str(), v.keyboard, v.sd_card_mb, v.ram_mb, v.cores), ("host", "cold", true, 1024, 4096, 6));
    assert!(update(&sdk, "Meu_Pixel", &Settings { gpu_mode: Some("banana".into()), ..Default::default() }).is_err());
    update(&sdk, "Meu_Pixel", &Settings { sd_card_mb: Some(0), ..Default::default() }).unwrap();
    let c = config("Meu_Pixel").unwrap().0;
    assert_eq!(c.get("hw.sdCard"), "no");
    assert!(!c.has("sdcard.size"));

    // dados do usuário + snapshots
    fs::write(format!("{}/userdata-qemu.img", a.dir), "dados").unwrap();
    fs::create_dir_all(format!("{}/snapshots/default_boot", a.dir)).unwrap();
    fs::write(format!("{}/snapshots/default_boot/ram.bin", a.dir), "x").unwrap();
    fs::write(format!("{}/multiinstance.lock", a.dir), "1").unwrap();

    let dup = duplicate(&sdk, "Meu_Pixel", "Copia", "Cópia").unwrap();
    assert_eq!((dup.display_name.as_str(), dup.width), ("Cópia", 1080));
    assert!(PathBuf::from(&dup.dir).join("userdata-qemu.img").exists(), "a cópia deve levar os dados");
    for junk in ["snapshots", "multiinstance.lock"] {
        assert!(!PathBuf::from(&dup.dir).join(junk).exists(), "{junk}");
    }
    assert_eq!(config("Copia").unwrap().0.get("AvdId"), "Copia");
    assert!(duplicate(&sdk, "Meu_Pixel", "Copia", "").is_err());
    assert_eq!(list(&sdk).len(), 2);

    wipe_data("Meu_Pixel").unwrap();
    assert!(!PathBuf::from(&a.dir).join("userdata-qemu.img").exists());
    assert!(PathBuf::from(&a.dir).join("config.ini").exists());

    delete("Copia").unwrap();
    assert!(!PathBuf::from(&dup.dir).exists());
    assert_eq!(list(&sdk).len(), 1);
    assert!(delete("../Meu_Pixel").is_err());
}

#[test]
fn missing_image_is_reported() {
    let _g = crate::testutil::env_lock();
    let (sdk, _) = fake_sdk();
    let a = create(&sdk, &CreateSpec { name: "A".into(), device_id: "pixel_9".into(), image_pkg: IMG.into(), ..Default::default() }).unwrap();
    fs::remove_dir_all(sdk.root.join("system-images")).unwrap();
    let got = load(&sdk, &a.name);
    assert!(!got.image_ok && !got.problem.is_empty(), "{got:?}");
}

#[test]
fn raw_config_keeps_backup() {
    let _g = crate::testutil::env_lock();
    let (sdk, _) = fake_sdk();
    let a = create(&sdk, &CreateSpec { name: "A".into(), device_id: "pixel_9".into(), image_pkg: IMG.into(), ..Default::default() }).unwrap();
    assert!(write_raw_config("A", "hw.ramSize=1024\n").is_err(), "sem image.sysdir.1 deve falhar");
    let (mut cfg, _) = config("A").unwrap();
    cfg.set("hw.ramSize", "1234");
    write_raw_config("A", &cfg.to_text()).unwrap();
    assert!(PathBuf::from(&a.dir).join("config.ini.bak").exists());
    assert_eq!(config("A").unwrap().0.get("hw.ramSize"), "1234");
}

#[test]
fn device_frame_follows_the_checkbox() {
    let _g = crate::testutil::env_lock();
    let (sdk, _) = fake_sdk();
    let spec = |frame: Option<bool>| CreateSpec {
        name: "Moldura".into(),
        device_id: "pixel_8".into(),
        image_pkg: IMG.into(),
        settings: Settings { show_frame: frame, ..Default::default() },
    };
    let skin_keys = || {
        let c = config("Moldura").unwrap().0;
        (c.get("showDeviceFrame").to_string(), c.get("skin.name").to_string(), c.get("skin.path").to_string())
    };

    // moldura ligada, mas a skin ainda não foi baixada: o AVD sai sem skin.* (o emulador
    // desenharia uma tela lisa), e `missing` diz o que falta
    create(&sdk, &spec(None)).unwrap();
    assert_eq!(skin_keys(), ("yes".into(), "".into(), "".into()));
    assert!(view_of(&config("Moldura").unwrap().0, "Moldura").show_frame);
    assert_eq!(skin::missing(&config("Moldura").unwrap().0, &sdk).as_deref(), Some("pixel_8"));

    // com a skin no SDK, salvar o formulário passa a apontar para ela
    let dir = skin::root(&sdk).join("pixel_8");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("layout"), "parts {}").unwrap();
    update(&sdk, "Moldura", &Settings { show_frame: Some(true), ..Default::default() }).unwrap();
    assert_eq!(skin_keys(), ("yes".into(), "pixel_8".into(), dir.to_string_lossy().into()));

    // desmarcar de fato esconde (o emulador só obedece skin.path)
    update(&sdk, "Moldura", &Settings { show_frame: Some(false), ..Default::default() }).unwrap();
    assert_eq!(skin_keys(), ("no".into(), "1080x2400".into(), "_no_skin".into()));
    assert!(!view_of(&config("Moldura").unwrap().0, "Moldura").show_frame);
    // outras edições não mexem na moldura
    update(&sdk, "Moldura", &Settings { cores: Some(2), ..Default::default() }).unwrap();
    assert_eq!(skin_keys().2, "_no_skin");
    // e marcar de novo volta
    update(&sdk, "Moldura", &Settings { show_frame: Some(true), ..Default::default() }).unwrap();
    assert_eq!(skin_keys(), ("yes".into(), "pixel_8".into(), dir.to_string_lossy().into()));
    delete("Moldura").unwrap();

    // AVD novo com a skin já instalada nasce com a moldura; sem moldura nasce sem skin
    create(&sdk, &spec(None)).unwrap();
    assert_eq!(skin_keys(), ("yes".into(), "pixel_8".into(), dir.to_string_lossy().into()));
    delete("Moldura").unwrap();
    create(&sdk, &spec(Some(false))).unwrap();
    assert_eq!(skin_keys(), ("no".into(), "".into(), "".into()));
    assert!(!view_of(&config("Moldura").unwrap().0, "Moldura").show_frame);
    delete("Moldura").unwrap();

    // o que falta baixar: só quando a moldura está ligada e o aparelho tem skin
    fs::remove_dir_all(&dir).unwrap();
    assert!(fetch_skin_for_create(&sdk, &spec(Some(false))).is_ok(), "moldura desligada não baixa nada");
    let generic = CreateSpec { device_id: "medium_phone".into(), ..spec(None) };
    assert!(fetch_skin_for_create(&sdk, &generic).is_ok(), "aparelho sem skin não baixa nada");
}
