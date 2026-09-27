//! Caché de listas de modelos en vivo: `%LOCALAPPDATA%\LightMark\ai-models-cache.json`
//! con la forma `{ "<provider_id>": { "fetched_at": <unix>, "models": ["id", ...] } }`.
//! Independiente de settings.json; nunca contiene claves.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub fetched_at: u64,
    pub models: Vec<String>,
}

pub type Cache = BTreeMap<String, Entry>;

pub fn default_path() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string()));
    PathBuf::from(local).join("LightMark").join("ai-models-cache.json")
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Lee la caché; cualquier fallo (falta, JSON roto) = caché vacía.
pub fn load_from(path: &Path) -> Cache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, cache: &Cache) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(cache).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Guarda la lista de un proveedor (fusionando con el resto de la caché).
pub fn put_at(path: &Path, provider_id: &str, models: Vec<String>) -> std::io::Result<()> {
    let mut c = load_from(path);
    c.insert(provider_id.to_string(), Entry { fetched_at: now_unix(), models });
    save_to(path, &c)
}

pub fn put(provider_id: &str, models: Vec<String>) -> std::io::Result<()> {
    put_at(&default_path(), provider_id, models)
}

pub fn cached_models(provider_id: &str) -> Vec<String> {
    load_from(&default_path()).remove(provider_id).map(|e| e.models).unwrap_or_default()
}

/// Lista para el desplegable: catálogo estático (★ primero) ∪ caché en vivo, sin duplicados.
pub fn merge_models(catalog: &[&str], live: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(catalog.len() + live.len());
    let mut seen = std::collections::HashSet::new();
    for m in catalog.iter().map(|s| s.to_string()).chain(live.iter().cloned()) {
        if !m.trim().is_empty() && seen.insert(m.clone()) {
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lm_wsd_cache_{}_{name}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join("ai-models-cache.json")
    }

    #[test]
    fn roundtrip_and_merge_between_providers() {
        let p = tmp("rt");
        put_at(&p, "gemini", vec!["a".into(), "b".into()]).unwrap();
        put_at(&p, "groq", vec!["c".into()]).unwrap();
        let c = load_from(&p);
        assert_eq!(c["gemini"].models, vec!["a", "b"]);
        assert_eq!(c["groq"].models, vec!["c"]);
        assert!(c["groq"].fetched_at > 1_600_000_000);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn missing_or_corrupt_is_empty() {
        let p = tmp("bad");
        assert!(load_from(&p).is_empty());
        std::fs::write(&p, "{ nope").unwrap();
        assert!(load_from(&p).is_empty());
        put_at(&p, "x", vec!["m".into()]).unwrap();
        assert_eq!(load_from(&p)["x"].models, vec!["m"]);
        std::fs::remove_dir_all(p.parent().unwrap()).ok();
    }

    #[test]
    fn merge_keeps_catalog_first_and_dedups() {
        let m = merge_models(&["star", "b"], &["b".into(), "z".into(), "a".into(), "".into()]);
        assert_eq!(m, vec!["star", "b", "z", "a"]);
    }
}
