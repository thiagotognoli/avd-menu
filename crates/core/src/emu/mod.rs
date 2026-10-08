//! Inicia, para e acompanha emuladores Android, além de diagnosticar o que
//! costuma impedi-los de funcionar (KVM, bibliotecas, GPU).

pub mod diag;
pub mod gpu;
pub mod run;

pub use diag::{diagnose, Check};
pub use gpu::{dedicated_gpu_env, gpu_env, list_gpus, prefer_host_gpu, Gpu};
pub use run::{log_path, parse_ps, quickboot_off, read_log_tail, running, start, start_watched, stop, wm_class, EarlyExit, Instance, StartOptions};

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Executa um comando com limite de tempo; devolve a saída (ou None se falhou /
/// estourou o tempo). A saída é lida em paralelo à espera: um `ps` de máquina
/// ocupada passa fácil dos 64 KB do pipe e travaria se só lêssemos no fim.
pub(crate) fn output_with_timeout(cmd: &mut Command, limit: Duration) -> Option<Output> {
    use std::io::Read;
    let mut child: Child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let mut out_pipe = child.stdout.take()?;
    let mut err_pipe = child.stderr.take()?;
    let t_out = std::thread::spawn(move || {
        let mut v = vec![];
        let _ = out_pipe.read_to_end(&mut v);
        v
    });
    let t_err = std::thread::spawn(move || {
        let mut v = vec![];
        let _ = err_pipe.read_to_end(&mut v);
        v
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() > limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break None,
        }
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    status.map(|status| Output { status, stdout, stderr })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_output_does_not_deadlock() {
        // 300 KB de saída: bem acima do buffer do pipe
        let t0 = Instant::now();
        let o = output_with_timeout(Command::new("sh").args(["-c", "head -c 300000 /dev/zero | tr '\\0' 'a'"]), Duration::from_secs(5)).unwrap();
        assert_eq!(o.stdout.len(), 300_000);
        assert!(t0.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn timeout_kills_the_child() {
        let t0 = Instant::now();
        assert!(output_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200)).is_none());
        assert!(t0.elapsed() < Duration::from_secs(2));
    }
}
