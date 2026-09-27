//! Errores de IA con mensaje en español ya listo para mostrar (WS-D).
//! Ningún mensaje incluye la clave API ni el cuerpo de la petición.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiErrorKind {
    MissingKey,
    Auth,
    NotFound,
    RateLimit,
    Timeout,
    Network,
    Server,
    BadRequest,
    NoText,
    Blocked,
    InvalidUrl,
    Parse,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiError {
    pub kind: AiErrorKind,
    pub message: String,
}

impl AiError {
    pub fn new(kind: AiErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AiError {}
