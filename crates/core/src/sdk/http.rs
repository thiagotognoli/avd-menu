//! Clientes HTTP compartilhados (rustls, certificados do sistema, proxy do ambiente).

use crate::platform;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

pub fn user_agent() -> String {
    format!("avd-menu/{}", platform::version())
}

fn build(global: Option<Duration>) -> Agent {
    let cfg = Agent::config_builder()
        .user_agent(user_agent())
        .http_status_as_error(false)
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(global)
        .build();
    Agent::new_with_config(cfg)
}

/// Para manifestos e consultas curtas (limite de 90 s por chamada).
pub fn short() -> &'static Agent {
    static A: OnceLock<Agent> = OnceLock::new();
    A.get_or_init(|| build(Some(Duration::from_secs(90))))
}

/// Para downloads grandes (sem limite total de tempo).
pub fn long() -> &'static Agent {
    static A: OnceLock<Agent> = OnceLock::new();
    A.get_or_init(|| build(None))
}
