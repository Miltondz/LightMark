//! Tabla de proveedores de IA (WS-D), dirigida por datos. Fuente: `AI_PROVIDERS.md`.
//! Los ids marcados "no verificados" del catálogo son solo un punto de partida: la lista
//! en vivo ("Actualizar lista de modelos") y el campo de ID personalizado siempre mandan.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiStyle {
    OpenAiChat,
    GeminiGenerate,
    AnthropicMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    Bearer,
    GoogApiKey,
    AnthropicKey,
    None,
}

#[derive(Debug)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub style: ApiStyle,
    pub auth: Auth,
    /// La clave es obligatoria (custom: opcional, `needs_key = false` pero se envía si existe).
    pub needs_key: bool,
    pub max_tokens_field: &'static str,
    /// El usuario puede cambiar la URL base (ollama / lmstudio / custom).
    pub base_url_editable: bool,
    /// Proveedor local (mensajes de error específicos).
    pub local: bool,
    /// Modelo barato/rápido por defecto ("★"). Vacío = el primero que devuelva /models.
    pub cheap_model: &'static str,
    pub catalog: &'static [&'static str],
}

pub static PROVIDERS: &[Provider] = &[
    Provider {
        id: "gemini",
        name: "Google Gemini",
        default_base_url: "https://generativelanguage.googleapis.com/v1beta",
        style: ApiStyle::GeminiGenerate,
        auth: Auth::GoogApiKey,
        needs_key: true,
        max_tokens_field: "maxOutputTokens",
        base_url_editable: false,
        local: false,
        cheap_model: "gemini-3.1-flash-lite",
        catalog: &[
            "gemini-3.1-flash-lite",
            "gemini-3.5-flash-lite",
            "gemini-3.5-flash",
            "gemini-3.6-flash",
            "gemini-3.7-flash",
            "gemini-3.8-flash",
            "gemini-3.1-pro-preview",
            "gemini-2.5-flash-lite",
            "gemini-2.5-flash",
            "gemini-2.5-pro",
        ],
    },
    Provider {
        id: "openrouter",
        name: "OpenRouter",
        default_base_url: "https://openrouter.ai/api/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "mistralai/mistral-nemo",
        catalog: &[
            "mistralai/mistral-nemo",
            "google/gemma-4-31b-it:free",
            "google/gemma-4-26b-a4b-it:free",
            "qwen/qwen3.8-27b:free",
            "nvidia/nemotron-3.5-lightning:free",
            "deepseek/deepseek-v4-flash",
            "openai/gpt-oss-20b",
            "meta-llama/llama-3.1-8b-instruct",
            "google/gemini-3.1-flash-lite",
            "openai/gpt-5.4-nano",
            "anthropic/claude-haiku-4.5",
        ],
    },
    Provider {
        id: "nvidia",
        name: "NVIDIA Build (NIM)",
        default_base_url: "https://integrate.api.nvidia.com/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "nv-mistralai/mistral-nemo-12b-instruct",
        catalog: &[
            "nv-mistralai/mistral-nemo-12b-instruct",
            "nvidia/nemotron-3.5-lightning-30b-a3b",
            "nvidia/nemotron-nano-3-30b-a3b",
            "nvidia/nemotron-3-super-120b-a12b",
            "nvidia/nemotron-3-ultra-550b-a55b",
            "deepseek-ai/deepseek-v4.1-flash",
            "moonshotai/kimi-k3",
            "moonshotai/kimi-k2.6",
            "google/gemma-4-31b-it",
            "openai/gpt-oss-20b",
            "mistralai/mistral-large",
            "mistralai/mistral-nemotron",
        ],
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        default_base_url: "https://api.openai.com/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_completion_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "gpt-5.4-nano",
        catalog: &[
            "gpt-5.4-nano",
            "gpt-5.4-mini",
            "gpt-5.4",
            "gpt-5.5",
            "gpt-6-luna",
            "gpt-6-sol",
            "gpt-6-astra",
        ],
    },
    Provider {
        id: "anthropic",
        name: "Anthropic (Claude)",
        default_base_url: "https://api.anthropic.com/v1",
        style: ApiStyle::AnthropicMessages,
        auth: Auth::AnthropicKey,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "claude-haiku-4-5-20251001",
        catalog: &[
            "claude-haiku-4-5-20251001",
            "claude-sonnet-5",
            "claude-opus-5-5",
            "claude-fable-5-1",
        ],
    },
    Provider {
        id: "groq",
        name: "Groq",
        default_base_url: "https://api.groq.com/openai/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "llama-3.1-8b-instant",
        catalog: &[
            "llama-3.1-8b-instant",
            "openai/gpt-oss-20b",
            "openai/gpt-oss-120b",
            "llama-3.3-70b-versatile",
        ],
    },
    Provider {
        id: "mistral",
        name: "Mistral AI",
        default_base_url: "https://api.mistral.ai/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        // IDs no verificados (docs con JS): usar "Actualizar lista de modelos".
        cheap_model: "mistral-small-latest",
        catalog: &[
            "mistral-small-latest",
            "ministral-8b-latest",
            "ministral-3b-latest",
            "mistral-medium-latest",
            "mistral-large-latest",
        ],
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
        default_base_url: "https://api.deepseek.com",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "deepseek-v4-flash",
        catalog: &["deepseek-v4-flash", "deepseek-v4-pro"],
    },
    Provider {
        id: "xai",
        name: "xAI (Grok)",
        default_base_url: "https://api.x.ai/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: true,
        max_tokens_field: "max_tokens",
        base_url_editable: false,
        local: false,
        cheap_model: "grok-4.20-non-reasoning",
        catalog: &[
            "grok-4.20-non-reasoning",
            "grok-4.3",
            "grok-4.5",
            "grok-4.6",
            "grok-4.7",
            "grok-code-fast-1",
        ],
    },
    Provider {
        id: "ollama",
        name: "Ollama (local)",
        default_base_url: "http://localhost:11434/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::None,
        needs_key: false,
        max_tokens_field: "max_tokens",
        base_url_editable: true,
        local: true,
        cheap_model: "",
        catalog: &[],
    },
    Provider {
        id: "lmstudio",
        name: "LM Studio (local)",
        default_base_url: "http://localhost:1234/v1",
        style: ApiStyle::OpenAiChat,
        auth: Auth::None,
        needs_key: false,
        max_tokens_field: "max_tokens",
        base_url_editable: true,
        local: true,
        cheap_model: "",
        catalog: &[],
    },
    Provider {
        id: "custom",
        name: "Personalizado (compatible OpenAI)",
        default_base_url: "",
        style: ApiStyle::OpenAiChat,
        auth: Auth::Bearer,
        needs_key: false,
        max_tokens_field: "max_tokens",
        base_url_editable: true,
        local: false,
        cheap_model: "",
        catalog: &[],
    },
];

/// Busca un proveedor por id; desconocido -> `None`.
pub fn find(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Proveedor por id con respaldo al primero (Gemini), como `Settings::validated`.
pub fn find_or_default(id: &str) -> &'static Provider {
    find(id).unwrap_or(&PROVIDERS[0])
}

pub fn index_of(id: &str) -> usize {
    PROVIDERS.iter().position(|p| p.id == id).unwrap_or(0)
}

impl Provider {
    /// Modelo efectivo: el elegido (recortado) o el "★" barato por defecto.
    pub fn effective_model(&self, chosen: &str) -> String {
        let m = chosen.trim();
        if m.is_empty() { self.cheap_model.to_string() } else { m.to_string() }
    }

    /// URL base efectiva: la del usuario solo si el proveedor la permite editar.
    pub fn effective_base_url(&self, user_url: &str) -> String {
        let u = user_url.trim();
        let url = if self.base_url_editable && !u.is_empty() { u } else { self.default_base_url };
        url.trim_end_matches('/').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_match_settings_list() {
        let ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        assert_eq!(ids, crate::settings::KNOWN_AI_PROVIDERS);
    }

    #[test]
    fn cheap_model_is_in_catalog_or_provider_is_local_or_custom() {
        for p in PROVIDERS {
            if p.local || p.id == "custom" {
                assert!(p.cheap_model.is_empty(), "{}", p.id);
            } else {
                assert!(p.catalog.contains(&p.cheap_model), "{}", p.id);
                assert_eq!(p.catalog[0], p.cheap_model, "{} ★ debe ir primero", p.id);
            }
        }
    }

    #[test]
    fn urls_are_https_except_local() {
        for p in PROVIDERS {
            if p.default_base_url.is_empty() {
                assert_eq!(p.id, "custom");
                continue;
            }
            if p.local {
                assert!(p.default_base_url.starts_with("http://localhost"));
            } else {
                assert!(p.default_base_url.starts_with("https://"), "{}", p.id);
            }
            assert!(!p.default_base_url.ends_with('/'));
        }
    }

    #[test]
    fn only_local_and_custom_have_editable_url() {
        for p in PROVIDERS {
            assert_eq!(p.base_url_editable, p.local || p.id == "custom", "{}", p.id);
        }
    }

    #[test]
    fn styles_and_auth_match_table() {
        assert_eq!(find("gemini").unwrap().style, ApiStyle::GeminiGenerate);
        assert_eq!(find("gemini").unwrap().auth, Auth::GoogApiKey);
        assert_eq!(find("anthropic").unwrap().style, ApiStyle::AnthropicMessages);
        assert_eq!(find("anthropic").unwrap().auth, Auth::AnthropicKey);
        assert_eq!(find("openai").unwrap().max_tokens_field, "max_completion_tokens");
        assert_eq!(find("ollama").unwrap().auth, Auth::None);
        assert!(find("nope").is_none());
        assert_eq!(find_or_default("nope").id, "gemini");
    }

    #[test]
    fn effective_model_and_url() {
        let g = find("gemini").unwrap();
        assert_eq!(g.effective_model("  "), "gemini-3.1-flash-lite");
        assert_eq!(g.effective_model(" x-1 "), "x-1");
        // URL de usuario ignorada en proveedores no editables
        assert_eq!(g.effective_base_url("http://evil"), g.default_base_url);
        let o = find("ollama").unwrap();
        assert_eq!(o.effective_base_url(""), "http://localhost:11434/v1");
        assert_eq!(o.effective_base_url("http://localhost:9/v1/"), "http://localhost:9/v1");
    }
}
