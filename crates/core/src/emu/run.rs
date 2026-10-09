use super::output_with_timeout;
use crate::sdk::Sdk;
use crate::{avd, platform, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Quando o usuário pediu para parar cada emulador (para não tratar o fim
/// esperado como falha).
static STOP_REQUESTS: std::sync::Mutex<BTreeMap<String, Instant>> = std::sync::Mutex::new(BTreeMap::new());

fn stop_was_requested(name: &str) -> bool {
    STOP_REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).get(name).map(|t| t.elapsed() < Duration::from_secs(90)).unwrap_or(false)
}

/// O emulador terminou por erro (e não porque alguém o fechou)?
fn is_failure(st: &std::process::ExitStatus, name: &str) -> bool {
    use std::os::unix::process::ExitStatusExt;
    const USER_SIGNALS: [i32; 4] = [libc::SIGHUP, libc::SIGINT, libc::SIGTERM, libc::SIGKILL];
    !st.success() && !st.signal().is_some_and(|s| USER_SIGNALS.contains(&s)) && !stop_was_requested(name)
}

/// Emulador em execução.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Instance {
    pub name: String,
    pub pids: Vec<i32>,
    /// porta do console (5554...)
    #[serde(skip_serializing_if = "is_zero")]
    pub port: u32,
    /// emulator-5554
    #[serde(skip_serializing_if = "String::is_empty")]
    pub serial: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// WM_CLASS/StartupWMClass que vincula a janela do emulador ao atalho do AVD no
/// GNOME (o atalho .desktop declara o mesmo valor).
pub fn wm_class(avd_name: &str) -> String {
    format!("android-emulator-{}", platform::slugify(avd_name))
}

/// Lista os emuladores ativos (inclusive os iniciados pelo Android Studio ou por
/// um terminal), agrupados por nome de AVD.
pub fn running() -> BTreeMap<String, Instance> {
    match output_with_timeout(Command::new("ps").args(["-axww", "-o", "pid=,args="]), Duration::from_secs(5)) {
        Some(o) => parse_ps(&String::from_utf8_lossy(&o.stdout), std::process::id() as i32),
        None => BTreeMap::new(),
    }
}

fn is_emulator_binary(token: &str) -> bool {
    let base = token.rsplit('/').next().unwrap_or(token);
    base == "emulator" || base.starts_with("emulator64-") || base.starts_with("qemu-system-")
}

/// Interpreta a saída de `ps -axww -o pid=,args=`.
pub fn parse_ps(out: &str, self_pid: i32) -> BTreeMap<String, Instance> {
    let mut res: BTreeMap<String, Instance> = BTreeMap::new();
    for line in out.lines() {
        let line = line.trim_start();
        let Some((pid_s, args)) = line.split_once(char::is_whitespace) else { continue };
        let Ok(pid) = pid_s.parse::<i32>() else { continue };
        let toks: Vec<&str> = args.split_whitespace().collect();
        if pid == self_pid || toks.is_empty() || !is_emulator_binary(toks[0]) {
            continue;
        }
        let mut name = None;
        let mut port = 0u32;
        let mut i = 1;
        while i < toks.len() {
            match toks[i] {
                "-avd" if i + 1 < toks.len() => {
                    name = Some(toks[i + 1].to_string());
                    i += 1;
                }
                "-ports" if i + 1 < toks.len() => {
                    port = toks[i + 1].split(',').next().and_then(|p| p.parse().ok()).unwrap_or(port);
                    i += 1;
                }
                "-port" if i + 1 < toks.len() => {
                    port = toks[i + 1].parse().unwrap_or(port);
                    i += 1;
                }
                t if t.starts_with('@') && t.len() > 1 && name.is_none() => name = Some(t[1..].to_string()),
                _ => {}
            }
            i += 1;
        }
        let Some(name) = name else { continue };
        let inst = res.entry(name.clone()).or_insert_with(|| Instance { name, pids: vec![], port: 0, serial: String::new() });
        inst.pids.push(pid);
        if port > 0 {
            inst.port = port;
        }
        if inst.port > 0 {
            inst.serial = format!("emulator-{}", inst.port);
        }
    }
    res
}

/// Como o emulador é iniciado.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StartOptions {
    /// "" / "normal" | "cold" | "wipe"
    pub mode: String,
    /// "" (a placa padrão do AVD) | "dedicated" | "integrated"
    pub gpu: String,
    /// sem janela
    pub headless: bool,
    /// argumentos livres
    pub extra_args: Vec<String>,
}

/// Arquivo de log da última execução do AVD.
pub fn log_path(name: &str) -> PathBuf {
    platform::state_dir().join("logs").join(format!("{name}.log"))
}

/// Últimas `n` linhas do log do AVD.
pub fn read_log_tail(name: &str, n: usize) -> String {
    let Ok(mut b) = std::fs::read(log_path(name)) else { return String::new() };
    if b.len() > 256 << 10 {
        b.drain(..b.len() - (256 << 10));
    }
    let s = String::from_utf8_lossy(&b);
    let lines: Vec<&str> = s.trim_end_matches('\n').split('\n').collect();
    let from = lines.len().saturating_sub(n);
    lines[from..].join("\n")
}

/// Esta execução usa a placa dedicada? `requested` vem de `StartOptions::gpu`;
/// vazio = a placa padrão escolhida no AVD.
pub fn wants_dedicated(name: &str, requested: &str) -> bool {
    match requested {
        "dedicated" => true,
        "integrated" | "default" => false,
        _ => avd::config(name).map(|(c, _)| avd::gpu_card(&c) == "dedicated").unwrap_or(false),
    }
}

/// O AVD está em “cold boot” (inicialização rápida desligada)?
pub fn quickboot_off(name: &str) -> bool {
    avd::config(name).map(|(c, _)| c.get("fastboot.forceColdBoot") == "yes").unwrap_or(false)
}

/// Argumentos do emulador para o AVD, com o modo gráfico “automático” resolvido
/// (ver `prefer_host_gpu`).
pub fn build_args(name: &str, opt: &StartOptions) -> Vec<String> {
    let mut args: Vec<String> = vec!["-avd".into(), name.into()];
    match opt.mode.as_str() {
        "cold" => args.push("-no-snapshot-load".into()),
        "wipe" => args.push("-wipe-data".into()),
        _ => {}
    }
    // Com fastboot.forceColdBoot o emulador não carrega o snapshot, mas continua
    // gravando a RAM inteira (GBs) ao fechar — lento, e se for morto no meio o
    // snapshot fica corrompido. -no-snapshot desliga as duas pontas.
    if quickboot_off(name) && !opt.extra_args.iter().any(|a| a.starts_with("-no-snapshot") || a == "-snapshot") {
        args.push("-no-snapshot".into());
    }
    if opt.headless {
        args.push("-no-window".into());
        args.push("-no-audio".into());
    }
    args.extend(opt.extra_args.iter().cloned());
    let user_chose_gpu = args.iter().any(|a| a == "-gpu");
    if !user_chose_gpu {
        let mode = avd::config(name).map(|(c, _)| c.get("hw.gpu.mode").to_string()).unwrap_or_default();
        if (mode.is_empty() || mode == "auto") && super::gpu::prefer_host_gpu() {
            args.push("-gpu".into());
            args.push("host".into());
        }
    }
    args
}

/// Inicia o emulador do AVD, desacoplado do nosso processo (continua rodando se
/// fecharmos o AVD Menu). Devolve o PID do lançador.
pub fn start(sdk: &Sdk, name: &str, opt: &StartOptions) -> Result<u32> {
    start_watched(sdk, name, opt, None)
}

/// Como terminou um emulador que durou pouco (provavelmente falhou ao iniciar).
#[derive(Debug, Clone)]
pub struct EarlyExit {
    pub name: String,
    pub code: Option<i32>,
    /// Últimas linhas do log.
    pub log: String,
}

/// Igual a `start`, mas `on_early_exit` é chamado se o emulador terminar com erro
/// nos primeiros 2 minutos — o caso típico de “abriu e fechou”: disco cheio,
/// biblioteca faltando, imagem corrompida...
pub fn start_watched(sdk: &Sdk, name: &str, opt: &StartOptions, on_early_exit: Option<Box<dyn FnOnce(EarlyExit) + Send>>) -> Result<u32> {
    let bin = sdk.emulator();
    if !platform::is_executable(&bin) {
        return Err(Error::bad(trf!(
            "o emulador não está instalado em {} — instale o pacote \"Android Emulator\" no SDK Manager",
            "the emulator is not installed at {} — install the \"Android Emulator\" package in the SDK Manager",
            bin.display()
        )));
    }
    if running().contains_key(name) {
        return Err(Error::conflict(trf!("o emulador {:?} já está em execução", "emulator {:?} is already running", name)));
    }
    let args = build_args(name, opt);
    let log = log_path(name);
    std::fs::create_dir_all(log.parent().unwrap())?;
    let mut logf = std::fs::File::create(&log)?;
    use std::io::Write;
    let _ = writeln!(logf, "$ {} {}", bin.display(), args.join(" "));
    // Tema das janelas do emulador (segue o sistema, a menos que o usuário escolha).
    if let Err(e) = super::theme::sync(&crate::config::load().emulator_theme) {
        let _ = writeln!(logf, "avd-menu: {e}");
    }
    // Moldura do aparelho: baixa a skin que falta e acerta skin.path no config.ini.
    if let Err(e) = avd::skin::prepare(sdk, name) {
        let _ = writeln!(logf, "avd-menu: {e}");
    }
    let mut cmd = Command::new(&bin);
    cmd.args(&args)
        .current_dir(bin.parent().unwrap())
        .env("ANDROID_SDK_ROOT", &sdk.root)
        .env("RESOURCE_NAME", wm_class(name))
        .stdin(Stdio::null())
        .stdout(logf.try_clone()?)
        .stderr(logf);
    platform::clean_env(&mut cmd);
    // OpenGL e Vulkan na mesma placa (a dedicada, se pedida; senão a dos monitores)
    for (k, v) in super::gpu::gpu_env(wants_dedicated(name, &opt.gpu)) {
        cmd.env(k, v);
    }
    // Nova sessão: o emulador sobrevive ao fechamento do AVD Menu / do terminal.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    // Se morrer logo de cara, quase sempre é erro de configuração: mostra o log.
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(2500) {
        if let Ok(Some(st)) = child.try_wait() {
            let tail = read_log_tail(name, 12);
            let head = trf!("o emulador terminou imediatamente", "the emulator exited immediately");
            return Err(Error::bad(format!("{head} ({st})\n{}", tail.trim())));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Recolhe o processo quando ele terminar (sem zumbi) e avisa se foi um fim precoce com erro.
    let (name, t_start) = (name.to_string(), Instant::now());
    std::thread::spawn(move || {
        let st = child.wait();
        if let (Some(cb), Ok(st)) = (on_early_exit, st) {
            if is_failure(&st, &name) && t_start.elapsed() < Duration::from_secs(120) {
                cb(EarlyExit { code: st.code(), log: read_log_tail(&name, 14), name });
            }
        }
    });
    Ok(pid)
}

fn wait_gone(name: &str, d: Duration) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < d {
        if !running().contains_key(name) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    false
}

/// Pede ao emulador para desligar. Usa o console (adb emu kill) quando possível —
/// o mesmo que o Android Studio faz — e cai para SIGTERM/SIGKILL.
pub fn stop(sdk: &Sdk, name: &str) -> Result<()> {
    let Some(inst) = running().remove(name) else {
        return Err(Error::conflict(trf!("o emulador {:?} não está em execução", "emulator {:?} is not running", name)));
    };
    STOP_REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).insert(name.to_string(), Instant::now());
    if !inst.serial.is_empty() && sdk.has_adb() {
        let ok = output_with_timeout(platform::clean_env(Command::new(sdk.adb()).args(["-s", &inst.serial, "emu", "kill"])), Duration::from_secs(8))
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok && wait_gone(name, Duration::from_secs(12)) {
            return Ok(());
        }
    }
    for p in &inst.pids {
        unsafe { libc::kill(*p, libc::SIGTERM) };
    }
    if wait_gone(name, Duration::from_secs(10)) {
        return Ok(());
    }
    for p in &inst.pids {
        unsafe { libc::kill(*p, libc::SIGKILL) };
    }
    if wait_gone(name, Duration::from_secs(5)) {
        return Ok(());
    }
    Err(Error::internal(trf!("não consegui encerrar o emulador {:?}", "could not stop emulator {:?}", name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ps_output() {
        let out = "  101 /usr/lib/systemd/systemd --user
 2001 /home/u/Android/Sdk/emulator/qemu/linux-x86_64/qemu-system-x86_64-headless -avd Pixel_9_API_36 -netdelay none -netspeed full -ports 5554,5555
 2002 /home/u/Android/Sdk/emulator/crashpad_handler --database=/tmp/x
 3001 /home/u/Android/Sdk/emulator/emulator @Tablet_API_34 -no-snapshot
 3002 /home/u/Android/Sdk/emulator/qemu/darwin-aarch64/qemu-system-aarch64 -avd Mac_AVD -port 5580
 4001 grep -avd Fake
 5001 /usr/bin/emulator-check accel
 9999 /home/u/bin/avd-menu start Foo -avd Bar";
        let got = parse_ps(out, 9999);
        assert_eq!(got.len(), 3, "{got:?}");
        let p = &got["Pixel_9_API_36"];
        assert_eq!((p.port, p.serial.as_str(), p.pids.clone()), (5554, "emulator-5554", vec![2001]));
        let t = &got["Tablet_API_34"];
        assert_eq!((t.name.as_str(), t.port, t.serial.as_str()), ("Tablet_API_34", 0, ""));
        assert_eq!(got["Mac_AVD"].port, 5580);
    }

    #[test]
    fn early_exit_is_reported_only_for_real_failures() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::mpsc;
        let _g = crate::testutil::env_lock();
        let root = crate::testutil::tmp("fake-emulator");
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::set_var("HOME", &root);
        let bin = root.join("emulator/emulator");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        // $2 = nome do AVD: FakeOk sai com 0, os outros com 3, depois da janela de 2,5 s.
        std::fs::write(&bin, "#!/bin/sh\nsleep 3\n[ \"$2\" = FakeOk ] && exit 0\nexit 3\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        STOP_REQUESTS.lock().unwrap().insert("FakeStopped".into(), Instant::now());

        let sdk = Sdk::new(&root);
        let (tx, rx) = mpsc::channel();
        for name in ["FakeFail", "FakeOk", "FakeStopped"] {
            let tx = tx.clone();
            start_watched(&sdk, name, &StartOptions::default(), Some(Box::new(move |x| tx.send(x).unwrap()))).unwrap();
        }
        drop(tx);
        // só FakeFail avisa; depois que todos os callbacks (e o canal) se encerram, não vem mais nada
        let first = rx.recv_timeout(Duration::from_secs(10)).expect("deveria avisar a falha");
        assert_eq!((first.name.as_str(), first.code), ("FakeFail", Some(3)));
        assert!(first.log.contains("-avd FakeFail"), "{}", first.log);
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_err(), "saída normal/pedida não é falha");
    }

    #[test]
    fn wm_class_slug() {
        assert_eq!(wm_class("Pixel_9 API.36"), "android-emulator-pixel-9-api-36");
    }
}
