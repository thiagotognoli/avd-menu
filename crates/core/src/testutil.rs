//! Apoio aos testes: variáveis de ambiente são do processo inteiro, então os
//! testes que as alteram precisam rodar um de cada vez.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENV: Mutex<()> = Mutex::new(());

pub fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn tmp(name: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("avdcore-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
