//! Cliente HTTP de IA (WS-D): `ureq` 3 + SChannel (native-tls). Bloqueante: se usa siempre
//! desde un hilo de fondo (ver `app::ai_ops::spawn`), jamás desde el hilo de la UI.

use super::error::{AiError, AiErrorKind};
use std::sync::OnceLock;
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
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(40)))
        .timeout_global(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .user_agent(concat!("LightMark/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn shared() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(agent)
}

/// Extrae el host de una URL `scheme://host[:port]/...` (soporta `[::1]`).
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let auth = rest.split(['/', '?', '#']).next()?;
    let auth = auth.rsplit_once('@').map_or(auth, |(_, h)| h);
    if let Some(stripped) = auth.strip_prefix('[') {
        let end = stripped.find(']')?;
        return Some(&auth[..end + 2]);
    }
    Some(auth.split(':').next().unwrap_or(auth))
}

/// Solo `https://`; `http://` únicamente hacia localhost / 127.0.0.1 / [::1].
pub fn check_url(url: &str) -> Result<(), AiError> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if lower.starts_with("http://") {
        if matches!(host_of(&lower), Some("localhost" | "127.0.0.1" | "[::1]")) {
            return Ok(());
        }
        return Err(AiError::new(
            AiErrorKind::InvalidUrl,
            "Por seguridad solo se permite https:// (http:// solo para localhost).",
        ));
    }
    Err(AiError::new(AiErrorKind::InvalidUrl, "La URL base no es válida (debe empezar por https://)."))
}

fn map_ureq(e: ureq::Error) -> AiError {
    // Mensajes propios, sin volcar el texto de `ureq` (podría citar cabeceras/URL).
    match e {
        ureq::Error::Timeout(_) => AiError::new(AiErrorKind::Timeout, "Tiempo de espera agotado."),
        ureq::Error::HostNotFound => AiError::new(AiErrorKind::Network, "no se encontró el servidor"),
        ureq::Error::ConnectionFailed => AiError::new(AiErrorKind::Network, "conexión rechazada"),
        ureq::Error::BadUri(_) => AiError::new(AiErrorKind::InvalidUrl, "La URL no es válida."),
        ureq::Error::Tls(_) => AiError::new(AiErrorKind::Network, "error TLS o de certificado"),
        _ => AiError::new(AiErrorKind::Network, "error de red"),
    }
}

fn finish(resp: ureq::http::Response<ureq::Body>) -> Result<(u16, String), AiError> {
    let status = resp.status().as_u16();
    let mut body = resp.into_body();
    let text = body
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_string()
        .map_err(map_ureq)?;
    Ok((status, text))
}

/// POST JSON. Devuelve `(estado HTTP, cuerpo)`; los 4xx/5xx NO son `Err`.
pub fn post_json(
    url: &str,
    headers: &[(String, String)],
    body: &serde_json::Value,
) -> Result<(u16, String), AiError> {
    check_url(url)?;
    let payload = serde_json::to_string(body)
        .map_err(|_| AiError::new(AiErrorKind::Other, "No se pudo preparar la petición."))?;
    let mut req = shared().post(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.content_type("application/json").send(payload.as_str()).map_err(map_ureq)?;
    finish(resp)
}

/// GET. Devuelve `(estado HTTP, cuerpo)`.
pub fn get(url: &str, headers: &[(String, String)]) -> Result<(u16, String), AiError> {
    check_url(url)?;
    let mut req = shared().get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.call().map_err(map_ureq)?;
    finish(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_builds() {
        let _ = super::agent();
    }

    #[test]
    fn https_always_allowed() {
        assert!(check_url("https://api.openai.com/v1").is_ok());
        assert!(check_url("HTTPS://example.com").is_ok());
    }

    #[test]
    fn http_only_for_loopback() {
        assert!(check_url("http://localhost:11434/v1").is_ok());
        assert!(check_url("http://127.0.0.1:1234/v1").is_ok());
        assert!(check_url("http://[::1]:8080/v1").is_ok());
        assert!(check_url("http://LOCALHOST/v1").is_ok());
        assert!(check_url("http://example.com/v1").is_err());
        assert!(check_url("http://localhost.evil.com/v1").is_err());
        assert!(check_url("http://localhost@evil.com/v1").is_err());
        assert!(check_url("http://192.168.1.5:1234/v1").is_err());
    }

    #[test]
    fn junk_urls_rejected() {
        assert_eq!(check_url("").unwrap_err().kind, AiErrorKind::InvalidUrl);
        assert!(check_url("ftp://x").is_err());
        assert!(check_url("localhost:11434").is_err());
    }

    #[test]
    fn ureq_errors_have_no_raw_detail() {
        let e = map_ureq(ureq::Error::Timeout(ureq::Timeout::Global));
        assert_eq!(e.kind, AiErrorKind::Timeout);
        let e = map_ureq(ureq::Error::HostNotFound);
        assert_eq!(e.kind, AiErrorKind::Network);
    }
}
