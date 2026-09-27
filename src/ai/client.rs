//! Cliente de IA (WS-D): constructores de petición y parsers PUROS por estilo de API
//! (OpenAI-compatible, Gemini generateContent, Anthropic messages) + capa de red delgada.
//! Todos los mensajes de error están en español y nunca incluyen la clave.

use super::error::{AiError, AiErrorKind};
use super::http;
use super::providers::{ApiStyle, Auth, Provider};
use serde_json::{Value, json};

/// Configuración efectiva de una llamada.
#[derive(Clone)]
pub struct AiConfig {
    pub provider: &'static Provider,
    /// URL base efectiva (sin `/` final).
    pub base_url: String,
    /// Modelo efectivo (ya resuelto al ★ si el usuario no eligió).
    pub model: String,
    pub key: Option<String>,
}

// La clave no debe salir jamás por `{:?}`.
impl std::fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiConfig")
            .field("provider", &self.provider.id)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "***"))
            .finish()
    }
}

impl AiConfig {
    /// Construye la config desde los ajustes; lee la clave del Administrador de credenciales
    /// (si el proveedor la usa). Una clave que falta se detecta al llamar (`MissingKey`).
    pub fn from_settings(s: &crate::settings::Settings) -> Result<Self, AiError> {
        let provider = super::providers::find_or_default(&s.ai_provider);
        let key = if provider.auth == Auth::None {
            None
        } else {
            super::secrets::load_key(provider.id)
                .map_err(|e| AiError::new(AiErrorKind::Other, e))?
        };
        Ok(Self {
            provider,
            base_url: provider.effective_base_url(&s.ai_base_url),
            model: provider.effective_model(&s.ai_model),
            key,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

const SNIPPET_MAX: usize = 160;

fn key_of(cfg: &AiConfig) -> &str {
    cfg.key.as_deref().map(str::trim).unwrap_or("")
}

/// Falta la clave obligatoria del proveedor.
pub fn check_key(cfg: &AiConfig) -> Result<(), AiError> {
    if cfg.provider.needs_key && key_of(cfg).is_empty() {
        return Err(AiError::new(
            AiErrorKind::MissingKey,
            format!(
                "Falta la clave API de {}. Configúrala en IA → Configuración.",
                cfg.provider.name
            ),
        ));
    }
    if cfg.base_url.is_empty() {
        return Err(AiError::new(
            AiErrorKind::InvalidUrl,
            format!("Falta la URL base de {}. Indícala en IA → Configuración.", cfg.provider.name),
        ));
    }
    Ok(())
}

fn auth_headers(cfg: &AiConfig) -> Vec<(String, String)> {
    let mut h: Vec<(String, String)> = Vec::new();
    let key = key_of(cfg);
    match cfg.provider.auth {
        Auth::Bearer if !key.is_empty() => h.push(("Authorization".into(), format!("Bearer {key}"))),
        Auth::GoogApiKey if !key.is_empty() => h.push(("x-goog-api-key".into(), key.to_string())),
        Auth::AnthropicKey => {
            if !key.is_empty() {
                h.push(("x-api-key".into(), key.to_string()));
            }
            h.push(("anthropic-version".into(), "2023-06-01".into()));
        }
        _ => {}
    }
    if cfg.provider.id == "openrouter" {
        h.push(("X-Title".into(), "LightMark".into()));
    }
    h
}

fn gemini_model_path(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

/// Petición de completado (sin red).
pub fn build_completion_request(cfg: &AiConfig, system: &str, user: &str, max_tokens: u32) -> Request {
    let p = cfg.provider;
    let headers = auth_headers(cfg);
    match p.style {
        ApiStyle::OpenAiChat => {
            let mut body = json!({
                "model": cfg.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user},
                ],
                "stream": false,
            });
            body[p.max_tokens_field] = json!(max_tokens);
            // Los modelos de razonamiento de OpenAI rechazan una temperatura distinta de la de fábrica.
            if p.id != "openai" {
                body["temperature"] = json!(0.4);
            }
            Request { url: format!("{}/chat/completions", cfg.base_url), headers, body: Some(body) }
        }
        ApiStyle::GeminiGenerate => {
            let mut gen_cfg = json!({ "temperature": 0.4, p.max_tokens_field: max_tokens });
            let m = gemini_model_path(&cfg.model);
            if m.starts_with("gemini-3") {
                gen_cfg["thinkingConfig"] = json!({ "thinkingLevel": "low" });
            } else if m.starts_with("gemini-2.5-flash") {
                gen_cfg["thinkingConfig"] = json!({ "thinkingBudget": 0 });
            }
            let body = json!({
                "systemInstruction": { "parts": [ { "text": system } ] },
                "contents": [ { "role": "user", "parts": [ { "text": user } ] } ],
                "generationConfig": gen_cfg,
            });
            Request {
                url: format!("{}/models/{}:generateContent", cfg.base_url, m),
                headers,
                body: Some(body),
            }
        }
        ApiStyle::AnthropicMessages => {
            let mut body = json!({
                "model": cfg.model,
                "system": system,
                "messages": [ { "role": "user", "content": user } ],
            });
            body[p.max_tokens_field] = json!(max_tokens);
            Request { url: format!("{}/messages", cfg.base_url), headers, body: Some(body) }
        }
    }
}

/// Petición de la lista de modelos (sin red).
pub fn build_models_request(cfg: &AiConfig) -> Request {
    let url = match cfg.provider.style {
        ApiStyle::GeminiGenerate => format!("{}/models?pageSize=1000", cfg.base_url),
        ApiStyle::AnthropicMessages => format!("{}/models?limit=1000", cfg.base_url),
        ApiStyle::OpenAiChat => format!("{}/models", cfg.base_url),
    };
    Request { url, headers: auth_headers(cfg), body: None }
}

fn truncate_chars(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

/// Mensaje del proveedor dentro de un cuerpo de error (`error.message`, `error`, `message`...).
fn provider_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let m = v
        .pointer("/error/message")
        .or_else(|| v.get("error").filter(|e| e.is_string()))
        .or_else(|| v.get("message"))
        .or_else(|| v.get("detail"))
        .and_then(Value::as_str)?;
    let m = m.trim();
    (!m.is_empty()).then(|| m.to_string())
}

fn scrub(msg: &str, cfg: &AiConfig) -> String {
    let k = key_of(cfg);
    if k.len() >= 4 { msg.replace(k, "***") } else { msg.to_string() }
}

/// Estado HTTP no exitoso -> mensaje en español (AI_PROVIDERS §5).
pub fn map_status(cfg: &AiConfig, status: u16, body: &str) -> AiError {
    let name = cfg.provider.name;
    let msg = provider_message(body).map(|m| truncate_chars(&scrub(&m, cfg), SNIPPET_MAX));
    let gemini_bad_key = cfg.provider.style == ApiStyle::GeminiGenerate
        && (body.contains("API_KEY_INVALID") || body.contains("API key not valid"));
    if status == 401 || status == 403 || gemini_bad_key {
        return AiError::new(
            AiErrorKind::Auth,
            format!("Clave API inválida o sin permisos para {name}."),
        );
    }
    match status {
        400 => AiError::new(
            AiErrorKind::BadRequest,
            match msg {
                Some(m) => format!("Solicitud rechazada por {name}: {m}"),
                None => format!("Solicitud rechazada por {name}."),
            },
        ),
        404 => AiError::new(
            AiErrorKind::NotFound,
            format!("Modelo o endpoint no encontrado: {}.", cfg.model),
        ),
        408 => AiError::new(
            AiErrorKind::Timeout,
            format!("Tiempo de espera agotado al contactar con {name}."),
        ),
        429 => AiError::new(
            AiErrorKind::RateLimit,
            format!("Límite de uso alcanzado en {name} (429). Espera un momento o cambia de modelo."),
        ),
        500..=599 => AiError::new(
            AiErrorKind::Server,
            format!("{name} tiene problemas ahora mismo ({status}). Inténtalo más tarde."),
        ),
        _ => AiError::new(
            AiErrorKind::Other,
            match msg {
                Some(m) => format!("Error {status} de {name}: {m}"),
                None => format!("Error {status} de {name}."),
            },
        ),
    }
}

/// Error de la capa de red -> mensaje contextual en español.
pub fn contextualize(cfg: &AiConfig, e: AiError) -> AiError {
    let name = cfg.provider.name;
    match e.kind {
        AiErrorKind::Timeout => AiError::new(
            AiErrorKind::Timeout,
            format!("Tiempo de espera agotado al contactar con {name}."),
        ),
        AiErrorKind::Network => {
            let hint = if cfg.provider.local { " ¿Está Ollama/LM Studio en ejecución?" } else { "" };
            AiError::new(
                AiErrorKind::Network,
                format!("No se pudo conectar con {name}: {}.{hint}", e.message),
            )
        }
        _ => e,
    }
}

fn no_text() -> AiError {
    AiError::new(
        AiErrorKind::NoText,
        "El modelo no devolvió texto (prueba otro modelo o sube el límite).",
    )
}

/// Un 200 con un objeto `error` dentro (p. ej. OpenRouter): lo trata como error HTTP.
fn embedded_error(cfg: &AiConfig, v: &Value) -> Option<AiError> {
    let e = v.get("error")?;
    if e.is_null() {
        return None;
    }
    let code = e.get("code").and_then(Value::as_u64).filter(|c| (400..600).contains(c));
    let raw = v.to_string();
    Some(match code {
        Some(c) => map_status(cfg, c as u16, &raw),
        None => map_status(cfg, 400, &raw),
    })
}

/// Parsea la respuesta de un completado según el estilo del proveedor.
pub fn parse_completion(cfg: &AiConfig, status: u16, body: &str) -> Result<String, AiError> {
    if !(200..300).contains(&status) {
        return Err(map_status(cfg, status, body));
    }
    let v: Value = serde_json::from_str(body).map_err(|_| {
        AiError::new(
            AiErrorKind::Parse,
            format!("Respuesta no válida de {}.", cfg.provider.name),
        )
    })?;
    let text = match cfg.provider.style {
        ApiStyle::OpenAiChat => {
            let Some(content) = v.pointer("/choices/0/message/content") else {
                return Err(embedded_error(cfg, &v).unwrap_or_else(no_text));
            };
            match content {
                Value::String(s) => s.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            }
        }
        ApiStyle::GeminiGenerate => {
            let cands = v.get("candidates").and_then(Value::as_array).filter(|c| !c.is_empty());
            let Some(cands) = cands else {
                if let Some(r) = v.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
                    return Err(AiError::new(
                        AiErrorKind::Blocked,
                        format!("Contenido bloqueado por el proveedor ({r})."),
                    ));
                }
                return Err(embedded_error(cfg, &v).unwrap_or_else(no_text));
            };
            cands[0]
                .pointer("/content/parts")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default()
        }
        ApiStyle::AnthropicMessages => {
            let Some(blocks) = v.get("content").and_then(Value::as_array) else {
                return Err(embedded_error(cfg, &v).unwrap_or_else(no_text));
            };
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() { Err(no_text()) } else { Ok(text) }
}

const NON_CHAT_MARKERS: &[&str] = &[
    "embed", "whisper", "tts", "dall-e", "moderation", "rerank", "transcribe", "reward",
    "-image", "-live", "computer-use", "realtime", "guard",
];

fn is_chat_model(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    !NON_CHAT_MARKERS.iter().any(|m| l.contains(m))
}

fn price(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_str().and_then(|s| s.trim().parse::<f64>().ok()).or_else(|| v.as_f64())
}

/// Parsea la lista de modelos y la ordena: OpenRouter = gratis primero y luego precio
/// ascendente; resto = ★ primero y luego alfabético.
pub fn parse_models(p: &Provider, body: &str) -> Result<Vec<String>, AiError> {
    let v: Value = serde_json::from_str(body).map_err(|_| {
        AiError::new(AiErrorKind::Parse, format!("Lista de modelos no válida de {}.", p.name))
    })?;
    let mut ids: Vec<String>;
    match p.style {
        ApiStyle::GeminiGenerate => {
            let arr = v.get("models").and_then(Value::as_array).cloned().unwrap_or_default();
            ids = arr
                .iter()
                .filter(|m| {
                    m.get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .is_some_and(|a| a.iter().any(|x| x.as_str() == Some("generateContent")))
                })
                .filter_map(|m| m.get("name").and_then(Value::as_str))
                .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                .filter(|id| is_chat_model(id))
                .collect();
        }
        _ => {
            let arr = v.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
            if p.id == "openrouter" {
                let mut rows: Vec<(String, bool, f64)> = Vec::new();
                for m in &arr {
                    let Some(id) = m.get("id").and_then(Value::as_str) else { continue };
                    if !is_chat_model(id) {
                        continue;
                    }
                    if let Some(mods) = m.pointer("/architecture/output_modalities").and_then(Value::as_array)
                        && !mods.iter().any(|x| x.as_str() == Some("text"))
                    {
                        continue;
                    }
                    let pp = price(m.pointer("/pricing/prompt"));
                    let pc = price(m.pointer("/pricing/completion"));
                    let free = id.ends_with(":free") || (pp == Some(0.0) && pc == Some(0.0));
                    let total = pp.unwrap_or(0.0) + pc.unwrap_or(0.0);
                    if !free && (pp.is_some_and(|x| x < 0.0) || pc.is_some_and(|x| x < 0.0)) {
                        continue; // `openrouter/auto` = -1
                    }
                    rows.push((id.to_string(), free, total));
                }
                rows.sort_by(|a, b| {
                    b.1.cmp(&a.1)
                        .then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
                        .then_with(|| a.0.cmp(&b.0))
                });
                ids = rows.into_iter().map(|r| r.0).collect();
                ids.dedup();
                return Ok(ids);
            }
            ids = arr
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .filter(|id| p.style != ApiStyle::OpenAiChat || is_chat_model(id))
                .map(str::to_string)
                .collect();
        }
    }
    ids.sort_by_key(|s| s.to_ascii_lowercase());
    ids.dedup();
    if !p.cheap_model.is_empty()
        && let Some(pos) = ids.iter().position(|i| i == p.cheap_model)
    {
        let star = ids.remove(pos);
        ids.insert(0, star);
    }
    Ok(ids)
}

// -------------------------------------------------------------------------------------
// Red

#[cfg(debug_assertions)]
fn fake_enabled() -> bool {
    std::env::var("LIGHTMARK_AI_FAKE").is_ok_and(|v| v == "1")
}
#[cfg(not(debug_assertions))]
fn fake_enabled() -> bool {
    false
}

/// Completado bloqueante (usar SIEMPRE desde un hilo de fondo).
pub fn complete(cfg: &AiConfig, system: &str, user: &str, max_tokens: u32) -> Result<String, AiError> {
    if fake_enabled() {
        return Ok(if user.contains("títulos") {
            "[\"Notas de la reunión semanal\", \"Plan del proyecto\", \"Resumen ejecutivo\"]".to_string()
        } else {
            "OK".to_string()
        });
    }
    check_key(cfg)?;
    let req = build_completion_request(cfg, system, user, max_tokens);
    let body = req.body.as_ref().cloned().unwrap_or(Value::Null);
    let (status, text) = http::post_json(&req.url, &req.headers, &body).map_err(|e| contextualize(cfg, e))?;
    parse_completion(cfg, status, &text)
}

/// API asíncrona para el resto de la app: ejecuta `complete` en un hilo y llama a `on_done`
/// EN ESE HILO (quien la use debe volver al hilo de la UI con `upgrade_in_event_loop`).
pub fn complete_async(
    cfg: AiConfig,
    system: String,
    user: String,
    max_tokens: u32,
    on_done: impl FnOnce(Result<String, AiError>) + Send + 'static,
) {
    std::thread::spawn(move || on_done(complete(&cfg, &system, &user, max_tokens)));
}

/// Lista de modelos en vivo (bloqueante).
pub fn list_models(cfg: &AiConfig) -> Result<Vec<String>, AiError> {
    if fake_enabled() {
        let mut v: Vec<String> = cfg.provider.catalog.iter().map(|s| s.to_string()).collect();
        v.push("fake-model-1".to_string());
        return Ok(v);
    }
    // Las listas de OpenRouter/NVIDIA son públicas: no exigimos clave para refrescar.
    if cfg.provider.needs_key && key_of(cfg).is_empty() && !matches!(cfg.provider.id, "openrouter" | "nvidia") {
        check_key(cfg)?;
    }
    if cfg.base_url.is_empty() {
        check_key(cfg)?;
    }
    let req = build_models_request(cfg);
    let (status, text) = http::get(&req.url, &req.headers).map_err(|e| contextualize(cfg, e))?;
    if !(200..300).contains(&status) {
        return Err(map_status(cfg, status, &text));
    }
    let models = parse_models(cfg.provider, &text)?;
    if models.is_empty() {
        return Err(AiError::new(
            AiErrorKind::Other,
            format!("{} no devolvió ningún modelo.", cfg.provider.name),
        ));
    }
    Ok(models)
}

/// "Probar conexión": un prompt diminuto real (valida clave, modelo y red) + latencia.
pub fn test_connection(cfg: &AiConfig) -> Result<String, AiError> {
    let t0 = std::time::Instant::now();
    let r = complete(cfg, "Responde solo con la palabra OK.", "Di OK.", 24);
    let ms = t0.elapsed().as_millis();
    let head = format!("Conexión correcta con {} · modelo {} · {} ms", cfg.provider.name, cfg.model, ms);
    match r {
        Ok(_) => Ok(head),
        // El servicio respondió pero el modelo gastó los tokens pensando: la conexión sí funciona.
        Err(e) if e.kind == AiErrorKind::NoText => Ok(format!("{head} (sin texto con el límite de prueba)")),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::find;

    fn cfg(id: &str, model: &str, key: Option<&str>) -> AiConfig {
        let p = find(id).unwrap();
        AiConfig {
            provider: p,
            base_url: p.default_base_url.to_string(),
            model: if model.is_empty() { p.cheap_model.to_string() } else { model.to_string() },
            key: key.map(str::to_string),
        }
    }

    // ---------- constructores ----------

    #[test]
    fn openai_style_request_golden() {
        let c = cfg("groq", "", Some("gsk-1"));
        let r = build_completion_request(&c, "SYS", "USR", 400);
        assert_eq!(r.url, "https://api.groq.com/openai/v1/chat/completions");
        assert_eq!(r.headers, vec![("Authorization".to_string(), "Bearer gsk-1".to_string())]);
        assert_eq!(
            r.body.unwrap(),
            json!({
                "model": "llama-3.1-8b-instant",
                "messages": [{"role":"system","content":"SYS"},{"role":"user","content":"USR"}],
                "stream": false,
                "max_tokens": 400,
                "temperature": 0.4
            })
        );
    }

    #[test]
    fn openai_provider_uses_completion_tokens_and_no_temperature() {
        let c = cfg("openai", "", Some("sk-x"));
        let b = build_completion_request(&c, "s", "u", 50).body.unwrap();
        assert_eq!(b["max_completion_tokens"], 50);
        assert!(b.get("max_tokens").is_none());
        assert!(b.get("temperature").is_none());
    }

    #[test]
    fn openrouter_adds_title_header() {
        let c = cfg("openrouter", "", Some("k"));
        let r = build_completion_request(&c, "s", "u", 5);
        assert!(r.headers.contains(&("X-Title".to_string(), "LightMark".to_string())));
    }

    #[test]
    fn local_provider_without_key_sends_no_auth() {
        let c = cfg("ollama", "llama3", None);
        let r = build_completion_request(&c, "s", "u", 5);
        assert!(r.headers.is_empty());
        assert_eq!(r.url, "http://localhost:11434/v1/chat/completions");
        assert!(check_key(&c).is_ok());
    }

    #[test]
    fn gemini_request_golden_and_thinking_variants() {
        let c = cfg("gemini", "", Some("AIza-1"));
        let r = build_completion_request(&c, "SYS", "USR", 400);
        assert_eq!(
            r.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:generateContent"
        );
        assert_eq!(r.headers, vec![("x-goog-api-key".to_string(), "AIza-1".to_string())]);
        assert_eq!(
            r.body.unwrap(),
            json!({
                "systemInstruction": {"parts":[{"text":"SYS"}]},
                "contents": [{"role":"user","parts":[{"text":"USR"}]}],
                "generationConfig": {
                    "temperature": 0.4,
                    "maxOutputTokens": 400,
                    "thinkingConfig": {"thinkingLevel": "low"}
                }
            })
        );
        let g25 = build_completion_request(&cfg("gemini", "gemini-2.5-flash-lite", Some("k")), "s", "u", 9);
        assert_eq!(g25.body.unwrap()["generationConfig"]["thinkingConfig"], json!({"thinkingBudget": 0}));
        let pro = build_completion_request(&cfg("gemini", "gemini-2.5-pro", Some("k")), "s", "u", 9);
        assert!(pro.body.unwrap()["generationConfig"].get("thinkingConfig").is_none());
        let pref = build_completion_request(&cfg("gemini", "models/gemini-3.5-flash", Some("k")), "s", "u", 9);
        assert!(pref.url.ends_with("/models/gemini-3.5-flash:generateContent"));
    }

    #[test]
    fn anthropic_request_golden() {
        let c = cfg("anthropic", "", Some("sk-ant-1"));
        let r = build_completion_request(&c, "SYS", "USR", 400);
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(
            r.headers,
            vec![
                ("x-api-key".to_string(), "sk-ant-1".to_string()),
                ("anthropic-version".to_string(), "2023-06-01".to_string())
            ]
        );
        assert_eq!(
            r.body.unwrap(),
            json!({
                "model": "claude-haiku-4-5-20251001",
                "system": "SYS",
                "messages": [{"role":"user","content":"USR"}],
                "max_tokens": 400
            })
        );
    }

    #[test]
    fn models_request_urls() {
        assert_eq!(build_models_request(&cfg("gemini", "", Some("k"))).url,
            "https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000");
        assert_eq!(build_models_request(&cfg("anthropic", "", Some("k"))).url,
            "https://api.anthropic.com/v1/models?limit=1000");
        assert_eq!(build_models_request(&cfg("nvidia", "", None)).url,
            "https://integrate.api.nvidia.com/v1/models");
        assert!(build_models_request(&cfg("nvidia", "", None)).body.is_none());
    }

    #[test]
    fn missing_key_message() {
        let e = check_key(&cfg("openai", "", None)).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::MissingKey);
        assert_eq!(e.message, "Falta la clave API de OpenAI. Configúrala en IA → Configuración.");
        assert!(check_key(&cfg("openai", "", Some("   "))).is_err());
        // custom sin URL
        let mut c = cfg("custom", "m", None);
        c.base_url = String::new();
        assert_eq!(check_key(&c).unwrap_err().kind, AiErrorKind::InvalidUrl);
    }

    #[test]
    fn debug_never_prints_key() {
        let c = cfg("openai", "", Some("sk-test-1234"));
        assert!(!format!("{c:?}").contains("sk-test-1234"));
    }

    // ---------- parsers ----------

    #[test]
    fn parse_openai_ok_string_and_parts() {
        let c = cfg("groq", "", Some("k"));
        let b = r#"{"choices":[{"message":{"role":"assistant","content":"  Hola  "}}]}"#;
        assert_eq!(parse_completion(&c, 200, b).unwrap(), "Hola");
        let b = r#"{"choices":[{"message":{"content":[{"type":"text","text":"A"},{"type":"text","text":"B"}]}}]}"#;
        assert_eq!(parse_completion(&c, 200, b).unwrap(), "AB");
    }

    #[test]
    fn parse_openai_null_or_empty_content_is_no_text() {
        let c = cfg("openai", "", Some("k"));
        for b in [
            r#"{"choices":[{"message":{"content":null}}]}"#,
            r#"{"choices":[{"message":{"content":""}}]}"#,
            r#"{"choices":[]}"#,
        ] {
            assert_eq!(parse_completion(&c, 200, b).unwrap_err().kind, AiErrorKind::NoText, "{b}");
        }
    }

    #[test]
    fn parse_openrouter_error_inside_200() {
        let c = cfg("openrouter", "", Some("k"));
        let b = r#"{"error":{"code":429,"message":"Rate limit"}}"#;
        assert_eq!(parse_completion(&c, 200, b).unwrap_err().kind, AiErrorKind::RateLimit);
    }

    #[test]
    fn parse_gemini_skips_thought_parts() {
        let c = cfg("gemini", "", Some("k"));
        let b = r#"{"candidates":[{"content":{"parts":[{"text":"pensando","thought":true},{"text":"Hola "},{"text":"mundo"}]},"finishReason":"STOP"}]}"#;
        assert_eq!(parse_completion(&c, 200, b).unwrap(), "Hola mundo");
    }

    #[test]
    fn parse_gemini_blocked_and_empty() {
        let c = cfg("gemini", "", Some("k"));
        let e = parse_completion(&c, 200, r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::Blocked);
        assert_eq!(e.message, "Contenido bloqueado por el proveedor (SAFETY).");
        let e = parse_completion(&c, 200, r#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[]}}]}"#)
            .unwrap_err();
        assert_eq!(e.kind, AiErrorKind::NoText);
    }

    #[test]
    fn parse_anthropic_blocks() {
        let c = cfg("anthropic", "", Some("k"));
        let b = r#"{"content":[{"type":"thinking","thinking":"x"},{"type":"text","text":"Uno"},{"type":"text","text":" dos"}],"stop_reason":"end_turn"}"#;
        assert_eq!(parse_completion(&c, 200, b).unwrap(), "Uno dos");
        assert_eq!(parse_completion(&c, 200, r#"{"content":[]}"#).unwrap_err().kind, AiErrorKind::NoText);
    }

    #[test]
    fn parse_garbage_body() {
        let c = cfg("groq", "", Some("k"));
        assert_eq!(parse_completion(&c, 200, "<html>").unwrap_err().kind, AiErrorKind::Parse);
    }

    // ---------- errores ----------

    #[test]
    fn error_mapping_per_status() {
        let c = cfg("openai", "gpt-x", Some("sk-secret-key-1"));
        let auth = r#"{"error":{"message":"Incorrect API key provided: sk-secret-key-1","code":"invalid_api_key"}}"#;
        let e = parse_completion(&c, 401, auth).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::Auth);
        assert_eq!(e.message, "Clave API inválida o sin permisos para OpenAI.");
        assert_eq!(parse_completion(&c, 403, "{}").unwrap_err().kind, AiErrorKind::Auth);

        let e = parse_completion(&c, 429, r#"{"error":{"message":"slow down"}}"#).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::RateLimit);
        assert!(e.message.contains("(429)") && e.message.contains("OpenAI"));

        let e = parse_completion(&c, 404, r#"{"error":{"message":"model not found"}}"#).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::NotFound);
        assert_eq!(e.message, "Modelo o endpoint no encontrado: gpt-x.");

        let e = parse_completion(&c, 503, "overloaded").unwrap_err();
        assert_eq!(e.kind, AiErrorKind::Server);
        assert!(e.message.contains("(503)"));

        assert_eq!(parse_completion(&c, 408, "").unwrap_err().kind, AiErrorKind::Timeout);
    }

    #[test]
    fn bad_request_message_is_trimmed_and_key_scrubbed() {
        let c = cfg("groq", "m", Some("gsk-topsecret"));
        let long = "x".repeat(400);
        let body = format!(r#"{{"error":{{"message":"bad gsk-topsecret {long}"}}}}"#);
        let e = map_status(&c, 400, &body);
        assert_eq!(e.kind, AiErrorKind::BadRequest);
        assert!(e.message.starts_with("Solicitud rechazada por Groq: bad ***"));
        assert!(!e.message.contains("topsecret"));
        assert!(e.message.chars().count() < 230);
        assert!(e.message.ends_with('…'));
    }

    #[test]
    fn gemini_invalid_key_is_400_but_maps_to_auth() {
        let c = cfg("gemini", "", Some("k"));
        let b = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;
        let e = parse_completion(&c, 400, b).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::Auth);
    }

    #[test]
    fn anthropic_error_shape() {
        let c = cfg("anthropic", "", Some("k"));
        let b = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: too big"}}"#;
        let e = parse_completion(&c, 400, b).unwrap_err();
        assert_eq!(e.message, "Solicitud rechazada por Anthropic (Claude): max_tokens: too big");
    }

    #[test]
    fn network_and_timeout_contextual_messages() {
        let c = cfg("openai", "", Some("k"));
        let e = contextualize(&c, AiError::new(AiErrorKind::Timeout, "x"));
        assert_eq!(e.message, "Tiempo de espera agotado al contactar con OpenAI.");
        let e = contextualize(&c, AiError::new(AiErrorKind::Network, "conexión rechazada"));
        assert_eq!(e.message, "No se pudo conectar con OpenAI: conexión rechazada.");
        let l = cfg("ollama", "m", None);
        let e = contextualize(&l, AiError::new(AiErrorKind::Network, "conexión rechazada"));
        assert!(e.message.contains("¿Está Ollama/LM Studio en ejecución?"));
        // otros tipos pasan intactos
        let e = contextualize(&c, AiError::new(AiErrorKind::InvalidUrl, "u"));
        assert_eq!(e.message, "u");
    }

    // ---------- modelos ----------

    #[test]
    fn parse_models_openai_style_filters_and_puts_star_first() {
        let p = find("groq").unwrap();
        let b = r#"{"data":[{"id":"whisper-large-v3"},{"id":"openai/gpt-oss-20b"},{"id":"llama-3.1-8b-instant"},{"id":"Zeta"},{"id":"llama-guard-4"},{"id":"alpha"}]}"#;
        assert_eq!(
            parse_models(p, b).unwrap(),
            vec!["llama-3.1-8b-instant", "alpha", "openai/gpt-oss-20b", "Zeta"]
        );
    }

    #[test]
    fn parse_models_gemini() {
        let p = find("gemini").unwrap();
        let b = r#"{"models":[
          {"name":"models/gemini-3.5-flash","supportedGenerationMethods":["generateContent","countTokens"]},
          {"name":"models/text-embedding-9","supportedGenerationMethods":["embedContent"]},
          {"name":"models/gemini-3.1-flash-lite","supportedGenerationMethods":["generateContent"]},
          {"name":"models/gemini-3-tts","supportedGenerationMethods":["generateContent"]},
          {"name":"models/gemini-3-flash-live","supportedGenerationMethods":["generateContent"]},
          {"name":"models/aqa","supportedGenerationMethods":["generateAnswer"]}
        ]}"#;
        assert_eq!(parse_models(p, b).unwrap(), vec!["gemini-3.1-flash-lite", "gemini-3.5-flash"]);
    }

    #[test]
    fn parse_models_anthropic() {
        let p = find("anthropic").unwrap();
        let b = r#"{"data":[{"id":"claude-sonnet-5","display_name":"S"},{"id":"claude-haiku-4-5-20251001"}],"has_more":false}"#;
        assert_eq!(parse_models(p, b).unwrap(), vec!["claude-haiku-4-5-20251001", "claude-sonnet-5"]);
    }

    #[test]
    fn parse_models_openrouter_free_first_then_price() {
        let p = find("openrouter").unwrap();
        let b = r#"{"data":[
          {"id":"paid/expensive","pricing":{"prompt":"0.000005","completion":"0.000015"},"architecture":{"output_modalities":["text"]}},
          {"id":"openrouter/auto","pricing":{"prompt":"-1","completion":"-1"}},
          {"id":"a/free:free","pricing":{"prompt":"0","completion":"0"},"architecture":{"output_modalities":["text"]}},
          {"id":"paid/cheap","pricing":{"prompt":"0.00000002","completion":"0.00000003"},"architecture":{"output_modalities":["text"]}},
          {"id":"img/only","pricing":{"prompt":"0.1","completion":"0.1"},"architecture":{"output_modalities":["image"]}},
          {"id":"b/free","pricing":{"prompt":"0","completion":"0"}}
        ]}"#;
        assert_eq!(
            parse_models(p, b).unwrap(),
            vec!["a/free:free", "b/free", "paid/cheap", "paid/expensive"]
        );
    }

    #[test]
    fn parse_models_bad_body() {
        assert_eq!(parse_models(find("groq").unwrap(), "no").unwrap_err().kind, AiErrorKind::Parse);
        assert_eq!(parse_models(find("groq").unwrap(), "{}").unwrap(), Vec::<String>::new());
    }

    // ---------- red contra un servidor local de mentira ----------

    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Sirve las respuestas dadas (una por conexión) y devuelve las peticiones crudas.
    fn mock(responses: Vec<(u16, String)>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in responses {
                let (mut s, _) = l.accept().unwrap();
                s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
                        let cl = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + cl {
                            break;
                        }
                    }
                }
                seen.push(String::from_utf8_lossy(&buf).to_string());
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                s.write_all(resp.as_bytes()).unwrap();
            }
            seen
        });
        (port, h)
    }

    fn local_cfg(id: &str, port: u16, key: Option<&str>) -> AiConfig {
        let mut c = cfg(id, "test-model", key);
        c.base_url = format!("http://127.0.0.1:{port}/v1");
        c
    }

    #[test]
    fn e2e_openai_compatible_complete_sends_bearer_and_parses() {
        let (port, h) = mock(vec![(200, r#"{"choices":[{"message":{"content":"Título"}}]}"#.into())]);
        let c = local_cfg("custom", port, Some("sk-test-1234"));
        assert_eq!(complete(&c, "sys", "usr", 20).unwrap(), "Título");
        let req = h.join().unwrap().remove(0);
        assert!(req.starts_with("POST /v1/chat/completions "), "{req}");
        assert!(req.to_ascii_lowercase().contains("authorization: bearer sk-test-1234"));
        assert!(req.contains(r#""model":"test-model""#));
        assert!(req.to_ascii_lowercase().contains("content-type: application/json"));
    }

    #[test]
    fn e2e_gemini_and_anthropic_paths_and_headers() {
        let (port, h) = mock(vec![
            (200, r#"{"candidates":[{"content":{"parts":[{"text":"G"}]}}]}"#.into()),
            (200, r#"{"content":[{"type":"text","text":"A"}]}"#.into()),
        ]);
        let g = local_cfg("gemini", port, Some("AIza-test"));
        assert_eq!(complete(&g, "s", "u", 9).unwrap(), "G");
        let a = local_cfg("anthropic", port, Some("sk-ant-test"));
        assert_eq!(complete(&a, "s", "u", 9).unwrap(), "A");
        let seen = h.join().unwrap();
        assert!(seen[0].starts_with("POST /v1/models/test-model:generateContent "));
        assert!(seen[0].to_ascii_lowercase().contains("x-goog-api-key: aiza-test"));
        assert!(seen[1].starts_with("POST /v1/messages "));
        let l = seen[1].to_ascii_lowercase();
        assert!(l.contains("x-api-key: sk-ant-test") && l.contains("anthropic-version: 2023-06-01"));
    }

    #[test]
    fn e2e_error_statuses_become_spanish_messages() {
        let (port, h) = mock(vec![
            (401, r#"{"error":{"message":"nope"}}"#.into()),
            (429, "{}".into()),
            (404, "{}".into()),
            (500, "boom".into()),
        ]);
        let c = local_cfg("custom", port, Some("k"));
        let kinds: Vec<AiErrorKind> = (0..4).map(|_| complete(&c, "s", "u", 5).unwrap_err().kind).collect();
        assert_eq!(
            kinds,
            vec![AiErrorKind::Auth, AiErrorKind::RateLimit, AiErrorKind::NotFound, AiErrorKind::Server]
        );
        h.join().unwrap();
    }

    #[test]
    fn e2e_list_models_and_test_connection() {
        let (port, h) = mock(vec![
            (200, r#"{"data":[{"id":"b"},{"id":"a"}]}"#.into()),
            (200, r#"{"choices":[{"message":{"content":"OK"}}]}"#.into()),
            (200, r#"{"choices":[{"message":{"content":null}}]}"#.into()),
        ]);
        let c = local_cfg("custom", port, None);
        assert_eq!(list_models(&c).unwrap(), vec!["a", "b"]);
        let msg = test_connection(&c).unwrap();
        assert!(msg.starts_with("Conexión correcta con Personalizado") && msg.contains(" ms"), "{msg}");
        let msg = test_connection(&c).unwrap();
        assert!(msg.contains("sin texto"), "{msg}");
        let seen = h.join().unwrap();
        assert!(seen[0].starts_with("GET /v1/models "));
        assert!(seen[1].contains(r#""max_tokens":24"#));
    }

    #[test]
    fn e2e_connection_refused_is_network_error() {
        // Puerto libre y cerrado.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let c = local_cfg("ollama", port, None);
        let e = complete(&c, "s", "u", 5).unwrap_err();
        assert_eq!(e.kind, AiErrorKind::Network);
        assert!(e.message.starts_with("No se pudo conectar con Ollama (local)"), "{}", e.message);
        assert!(e.message.contains("¿Está Ollama"));
    }

    #[test]
    fn plain_http_to_remote_host_is_refused_before_any_io() {
        let mut c = cfg("custom", "m", Some("k"));
        c.base_url = "http://example.com/v1".into();
        assert_eq!(complete(&c, "s", "u", 5).unwrap_err().kind, AiErrorKind::InvalidUrl);
    }

    #[test]
    fn complete_async_delivers_result() {
        let (port, h) = mock(vec![(200, r#"{"choices":[{"message":{"content":"async"}}]}"#.into())]);
        let c = local_cfg("custom", port, Some("k"));
        let (tx, rx) = std::sync::mpsc::channel();
        complete_async(c, "s".into(), "u".into(), 5, move |r| tx.send(r).unwrap());
        let r = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(r.unwrap(), "async");
        h.join().unwrap();
    }

    #[test]
    #[ignore = "usa la red real (endpoint público de OpenRouter)"]
    fn live_openrouter_models() {
        let c = cfg("openrouter", "", None);
        let m = list_models(&c).unwrap();
        assert!(m.len() > 50);
    }
}
