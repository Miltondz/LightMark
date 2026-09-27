//! Cliente HTTP de IA (WS-D). Wave 0: solo construye el `Agent` (smoke de que
//! `ureq` + SChannel/`native-tls` compila y se configura); WS-D añade `post_json`/`get`.
#![allow(dead_code)] // wave-1 fills (WS-D)

use std::time::Duration;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Construye el agente HTTP: TLS nativo de Windows (SChannel), almacén de raíces del
/// sistema, timeouts, y los códigos de estado HTTP no se convierten en error.
pub fn agent() -> Agent {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(45)))
        .http_status_as_error(false)
        .user_agent(concat!("LightMark/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_builds() {
        let _ = super::agent();
    }
}
