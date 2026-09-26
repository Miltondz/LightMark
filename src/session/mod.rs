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
    pub fn new(base_dir: PathBuf) -> Self {
        let sessions_dir = base_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).ok();
        Self {
            base_dir: sessions_dir,
        }
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
        std::fs::write(path, data)
    }

    pub fn load_session(&self, session_id: &str) -> std::io::Result<Session> {
        let path = self.session_path(session_id);
        let data = std::fs::read_to_string(path)?;
        let session: Session = serde_json::from_str(&data)?;
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
