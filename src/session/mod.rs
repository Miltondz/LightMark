// El wiring actual solo usa la sesión automática (`save_auto_session`/
// `load_auto_session`, "hot exit"); el resto de la API (sesiones nombradas,
// listar/borrar) queda para una futura función "Guardar sesión como...".
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TabState {
    pub document_id: String,
    pub path: Option<String>,
    pub title: String,
    pub is_scratch: bool,
    pub cursor: usize,
    pub scroll_line: usize,
    pub pinned: bool,
    /// Contenido no guardado de un archivo (para restaurar tras un cierre en caliente / "hot
    /// exit"). `None` si la pestaña estaba guardada o es un borrador (que se autoguarda solo).
    #[serde(default)]
    pub unsaved_content: Option<String>,
    /// mtime del archivo (segundos desde época Unix) en el momento de guardar la sesión
    /// (hallazgo C3): permite detectar en la restauración si el archivo cambió en disco desde
    /// entonces y avisar en vez de sobrescribir en silencio.
    #[serde(default)]
    pub mtime: Option<u64>,
    /// Nombre puesto por el usuario/IA (WS-B).
    #[serde(default)]
    pub custom_title: Option<String>,
    /// Creación del borrador, segundos Unix (para la fecha en el nombre).
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub untitled_n: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GroupState {
    pub id: usize,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub version: u32,
    pub name: String,
    pub created_at: u64,
    pub groups: Vec<GroupState>,
    pub active_group: usize,
    pub group_count: usize,
    pub workspace_root: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Default Session".to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            groups: vec![],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        }
    }
}

pub struct SessionManager {
    base_dir: PathBuf,
}

impl SessionManager {
    /// Usa `base_dir` directamente como directorio de sesiones (el llamador ya debe pasar el
    /// directorio final, p.ej. `<app_data>/sessions`; antes anidaba `sessions/sessions`).
    pub fn new(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self { base_dir }
    }

    pub fn sessions_dir(&self) -> &Path {
        &self.base_dir
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", session_id))
    }

    pub fn save_session(&self, session: &Session, session_id: &str) -> std::io::Result<()> {
        let path = self.session_path(session_id);
        let data = serde_json::to_string_pretty(session)?;
        // Escritura atómica real (hallazgo L10): `workspace::atomic_save` hace `sync_all()`
        // antes del rename (el `std::fs::write` + `rename` manual de antes no garantizaba que
        // los datos llegaran a disco antes del rename) y borra el `.tmp` si algo falla a mitad
        // de camino en vez de dejarlo huérfano.
        crate::workspace::atomic_save(&path, &data)
    }

    pub fn load_session(&self, session_id: &str) -> std::io::Result<Session> {
        let path = self.session_path(session_id);
        let data = std::fs::read_to_string(path)?;
        // Quita el BOM UTF-8 si lo hay (hallazgo M4): un session.json editado/guardado con
        // Notepad/PowerShell puede llevarlo y rompería el parseo JSON.
        let data = data.strip_prefix('\u{FEFF}').unwrap_or(&data);
        let session: Session = serde_json::from_str(data)?;
        Ok(session)
    }

    pub fn list_sessions(&self) -> std::io::Result<Vec<String>> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new("json"))
                && let Some(stem) = path.file_stem()
            {
                sessions.push(stem.to_string_lossy().to_string());
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    pub fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn auto_session_id() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("session-{}", timestamp)
    }

    pub fn save_auto_session(&self, session: &Session) -> std::io::Result<()> {
        // Save only to the fixed auto-restore session file to prevent disk flooding
        self.save_session(session, "auto-restore")?;
        Ok(())
    }

    pub fn load_auto_session(&self) -> std::io::Result<Option<Session>> {
        match self.load_session("auto-restore") {
            Ok(session) => Ok(Some(session)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

pub fn get_sessions_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("sessions")
}

pub fn ensure_sessions_dir(app_data_dir: &Path) -> PathBuf {
    let sessions_dir = get_sessions_dir(app_data_dir);
    std::fs::create_dir_all(&sessions_dir).ok();
    sessions_dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-session-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn new_does_not_nest_sessions_dir() {
        let dir = temp_dir("no-nest");
        let sessions_dir = ensure_sessions_dir(&dir);
        let manager = SessionManager::new(sessions_dir.clone());
        assert_eq!(manager.sessions_dir(), sessions_dir.as_path());
        assert!(!manager.sessions_dir().ends_with("sessions/sessions"));
        assert!(!manager.sessions_dir().ends_with("sessions\\sessions"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn roundtrip_session() {
        let dir = temp_dir("roundtrip");
        let manager = SessionManager::new(dir.clone());
        let mut session = Session::default();
        session.groups.push(GroupState {
            id: 0,
            tabs: vec![TabState {
                document_id: "scratch-1".to_string(),
                path: None,
                title: "Sin título 1".to_string(),
                is_scratch: true,
                cursor: 3,
                scroll_line: 0,
                pinned: false,
                unsaved_content: None,
                mtime: None,
                custom_title: None,
                created_at: None,
                untitled_n: None,
            }],
            active_tab: 0,
        });
        manager.save_session(&session, "test").unwrap();
        let loaded = manager.load_session("test").unwrap();
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].tabs[0].cursor, 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn old_json_without_unsaved_content_still_loads() {
        let dir = temp_dir("old-json");
        let manager = SessionManager::new(dir.clone());
        let old_json = r#"{
            "version": 1,
            "name": "Old",
            "created_at": 0,
            "groups": [
                {
                    "id": 0,
                    "tabs": [
                        {
                            "document_id": "tab-0",
                            "path": null,
                            "title": "Old title",
                            "is_scratch": false,
                            "cursor": 0,
                            "scroll_line": 0,
                            "pinned": false
                        }
                    ],
                    "active_tab": 0
                }
            ],
            "active_group": 0,
            "group_count": 1,
            "workspace_root": null
        }"#;
        std::fs::write(dir.join("legacy.json"), old_json).unwrap();
        let loaded = manager.load_session("legacy").unwrap();
        assert_eq!(loaded.groups[0].tabs[0].title, "Old title");
        assert_eq!(loaded.groups[0].tabs[0].unsaved_content, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_session_strips_utf8_bom() {
        let dir = temp_dir("bom");
        let manager = SessionManager::new(dir.clone());
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(serde_json::to_string(&Session::default()).unwrap().as_bytes());
        std::fs::write(dir.join("bomsession.json"), bytes).unwrap();
        let loaded = manager.load_session("bomsession").unwrap();
        assert_eq!(loaded.name, "Default Session");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_session_leaves_no_tmp_file() {
        let dir = temp_dir("no-tmp-leftover");
        let manager = SessionManager::new(dir.clone());
        manager.save_session(&Session::default(), "s1").unwrap();
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "tmp").unwrap_or(false))
            .collect();
        assert!(leftover.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_session_returns_not_found() {
        let dir = temp_dir("missing");
        let manager = SessionManager::new(dir.clone());
        let err = manager.load_session("nope").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        std::fs::remove_dir_all(&dir).ok();
    }
}
