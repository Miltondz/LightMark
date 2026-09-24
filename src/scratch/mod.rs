#![allow(dead_code)]

use ropey::LineType;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScratchMetadata {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub created_at: u64,
    pub modified_at: u64,
    pub language: Option<String>,
}

pub struct ScratchDocument {
    pub id: String,
    pub title: String,
    pub document: Rope,
    pub file_path: PathBuf,
    pub dirty: bool,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub language: Option<String>,
    pub last_saved: SystemTime,
}

impl ScratchDocument {
    pub fn new(id: String, document: Rope, base_dir: &Path) -> Self {
        let file_path = base_dir.join(format!("{}.md", id));
        let now = SystemTime::now();
        Self {
            id,
            title: String::new(),
            document,
            file_path,
            dirty: true,
            created_at: now,
            modified_at: now,
            language: None,
            last_saved: now,
        }
    }

    fn title_from_content(&self) -> String {
        let first_line = self.document.line(0, LineType::Unicode).to_string();
        first_line.trim().trim_start_matches('#').trim().to_string()
    }

    pub fn effective_title(&self) -> String {
        if self.title.is_empty() {
            let from_content = self.title_from_content();
            if !from_content.is_empty() {
                from_content
            } else {
                format!("Scratch {}", self.id)
            }
        } else {
            self.title.clone()
        }
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.last_saved = SystemTime::now();
        self.modified_at = self.last_saved;
    }
}

pub struct ScratchIndex {
    pub documents: Vec<ScratchMetadata>,
}

impl ScratchIndex {
    pub fn load(base_dir: &Path) -> Self {
        let index_path = base_dir.join("scratch-index.json");
        if let Ok(data) = std::fs::read_to_string(index_path)
            && let Ok(docs) = serde_json::from_str::<Vec<ScratchMetadata>>(&data)
        {
            return Self { documents: docs };
        }
        Self {
            documents: Vec::new(),
        }
    }

    pub fn save(&self, base_dir: &Path) -> std::io::Result<()> {
        let index_path = base_dir.join("scratch-index.json");
        let data = serde_json::to_string_pretty(&self.documents)?;
        std::fs::write(index_path, data)
    }

    pub fn add(&mut self, meta: ScratchMetadata) {
        self.documents.push(meta);
    }

    pub fn update(&mut self, id: &str, modified_at: u64) {
        if let Some(doc) = self.documents.iter_mut().find(|d| d.id == id) {
            doc.modified_at = modified_at;
        }
    }
}

pub struct ScratchManager {
    base_dir: PathBuf,
    autosave_interval: Duration,
}

impl ScratchManager {
    pub fn new(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        let interval = std::env::var("LIGHTMARK_AUTOSAVE_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(500);
        Self {
            base_dir,
            autosave_interval: Duration::from_millis(interval),
        }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn autosave_interval(&self) -> Duration {
        self.autosave_interval
    }

    fn next_id(&self) -> String {
        self.next_id_public()
    }

    pub fn next_id_public(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or_else(|_| rand_u64());
        format!("scratch-{}", timestamp)
    }

    pub fn create_scratch(&self, document: Rope) -> ScratchDocument {
        let id = self.next_id();
        ScratchDocument::new(id.clone(), document, &self.base_dir)
    }

    pub fn create_scratch_with_id(&self, id: String, document: Rope) -> ScratchDocument {
        ScratchDocument::new(id, document, &self.base_dir)
    }

    pub fn create_scratch_doc_for_save(
        &self,
        id: String,
        document: Rope,
    ) -> Option<ScratchDocument> {
        let mut doc = ScratchDocument::new(id, document, &self.base_dir);
        // Determine language from content
        let content = doc.document.to_string();
        doc.language = detect_language(&content);
        doc.dirty = true;
        Some(doc)
    }

    pub fn save_scratch(
        &self,
        doc: &mut ScratchDocument,
        index: &mut ScratchIndex,
    ) -> std::io::Result<()> {
        std::fs::write(&doc.file_path, doc.document.to_string())?;
        let modified_secs = doc
            .modified_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        index.update(&doc.id, modified_secs);
        index.save(&self.base_dir)?;
        doc.mark_saved();
        Ok(())
    }

    pub fn load_scratch(&self, meta: &ScratchMetadata) -> std::io::Result<ScratchDocument> {
        let content = std::fs::read_to_string(&meta.file_path)?;
        let document = Rope::from_str(&content);
        let created_secs = meta.created_at;
        let modified_secs = meta.modified_at;
        Ok(ScratchDocument {
            id: meta.id.clone(),
            title: String::new(),
            document,
            file_path: PathBuf::from(&meta.file_path),
            dirty: false,
            created_at: SystemTime::UNIX_EPOCH + Duration::from_secs(created_secs),
            modified_at: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
            language: meta.language.clone(),
            last_saved: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
        })
    }

    pub fn list_scratch_documents(&self) -> std::io::Result<Vec<ScratchMetadata>> {
        let index = ScratchIndex::load(&self.base_dir);
        Ok(index.documents)
    }

    pub fn delete_scratch(&self, doc: &ScratchDocument) -> std::io::Result<()> {
        std::fs::remove_file(&doc.file_path)?;
        let mut index = ScratchIndex::load(&self.base_dir);
        index.documents.retain(|d| d.id != doc.id);
        index.save(&self.base_dir)?;
        Ok(())
    }
}

fn rand_u64() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

// Helper function to generate a random ID if needed
fn random_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("scratch-{}", nanos)
}

pub fn get_app_data_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::var("APPDATA").unwrap_or(".".to_string()));
    PathBuf::from(local).join("LightMark")
}

pub fn get_scratch_dir() -> PathBuf {
    get_app_data_dir().join("scratch")
}

pub fn ensure_scratch_dirs() -> PathBuf {
    let scratch_dir = get_scratch_dir();
    std::fs::create_dir_all(&scratch_dir).ok();
    scratch_dir
}

fn detect_language(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if lower.contains("```sql") || lower.contains("select ") || lower.contains("create table") {
        return Some("sql".to_string());
    }
    if lower.contains("```json")
        || (lower.starts_with('{') && lower.starts_with('['))
        || lower.contains("\"key\"")
    {
        return Some("json".to_string());
    }
    if lower.contains("```html") || lower.contains("<html") || lower.contains("<!doctype") {
        return Some("html".to_string());
    }
    if lower.contains("# ") || lower.contains("## ") {
        return Some("markdown".to_string());
    }
    None
}
