#![allow(dead_code)]

use ropey::LineType;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Active,
    Parked,
    Inbox,
    Archived,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScratchMetadata {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub created_at: u64,
    pub modified_at: u64,
    pub language: Option<String>,
    pub status: DocumentStatus,
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
    pub status: DocumentStatus,
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
            status: DocumentStatus::Active,
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
                "Scratch".to_string()
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

    pub fn update_with_status(&mut self, id: &str, modified_at: u64, status: DocumentStatus) {
        if let Some(doc) = self.documents.iter_mut().find(|d| d.id == id) {
            doc.modified_at = modified_at;
            doc.status = status;
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

        index.add(ScratchMetadata {
            id: doc.id.clone(),
            title: doc.title.clone(),
            file_path: doc.file_path.to_string_lossy().to_string(),
            created_at: doc
                .created_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            modified_at: modified_secs,
            language: doc.language.clone(),
            status: doc.status.clone(),
        });
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
            status: meta.status.clone(),
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

    pub fn park_document(&self, doc: &mut ScratchDocument) -> std::io::Result<()> {
        doc.status = DocumentStatus::Parked;
        let modified_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut index = ScratchIndex::load(&self.base_dir);
        index.update_with_status(&doc.id, modified_secs, doc.status.clone());
        index.save(&self.base_dir)?;
        Ok(())
    }

    pub fn set_inbox(&self, doc: &mut ScratchDocument) -> std::io::Result<()> {
        doc.status = DocumentStatus::Inbox;
        let modified_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut index = ScratchIndex::load(&self.base_dir);
        index.update_with_status(&doc.id, modified_secs, doc.status.clone());
        index.save(&self.base_dir)?;
        Ok(())
    }

    pub fn archive_document(&self, doc: &mut ScratchDocument) -> std::io::Result<()> {
        doc.status = DocumentStatus::Archived;
        let modified_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut index = ScratchIndex::load(&self.base_dir);
        index.update_with_status(&doc.id, modified_secs, doc.status.clone());
        index.save(&self.base_dir)?;
        Ok(())
    }

    pub fn list_parked_documents(&self) -> std::io::Result<Vec<ScratchMetadata>> {
        let index = ScratchIndex::load(&self.base_dir);
        Ok(index
            .documents
            .into_iter()
            .filter(|d| d.status == DocumentStatus::Parked || d.status == DocumentStatus::Inbox)
            .collect())
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

pub fn detect_language(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if lower.contains("```sql") || lower.contains("select ") || lower.contains("create table") {
        return Some("sql".to_string());
    }
    if lower.contains("```json")
        || lower.starts_with('{')
        || lower.starts_with('[')
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

/// Generate an automatic name from document content
pub fn auto_name(document: &Rope) -> String {
    // Priority 1: First heading
    let first_line = document.line(0, LineType::Unicode).to_string();
    let heading = first_line.trim();
    if heading.starts_with('#') {
        let stripped = heading.trim_start_matches('#').trim();
        if !stripped.is_empty() {
            return sanitize_filename(stripped);
        }
    }

    // Priority 2: First non-empty line
    for lr in document.lines(LineType::Unicode) {
        let line = lr.to_string();
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            return sanitize_filename(trimmed);
        }
    }

    // Fallback
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("scratch-{}", timestamp)
}

fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .take(50)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Export archived documents to a folder structure
/// Creates: Archives/<date>/<time>-<name>/<files> + session.json
pub fn export_to_folder(
    archived_docs: &[ScratchDocument],
    dest_folder: &Path,
) -> std::io::Result<()> {
    let now = SystemTime::now();
    let date_str = now
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let date = time::OffsetDateTime::from_unix_timestamp(secs as i64)
                .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
            format!(
                "{}-{:02}-{:02}",
                date.year(),
                date.month() as u8,
                date.day()
            )
        })
        .unwrap_or_else(|_| "unknown-date".to_string());

    let archive_dir = dest_folder.join(&date_str);
    std::fs::create_dir_all(&archive_dir)?;

    let archive_subdir = archive_dir.join("archived");
    std::fs::create_dir_all(&archive_subdir)?;

    // Save each archived document
    for (i, doc) in archived_docs.iter().enumerate() {
        let prefix = format!("{:02}-", i + 1);
        let name = auto_name(&doc.document);
        let ext = doc
            .language
            .as_deref()
            .unwrap_or("md")
            .chars()
            .take_while(|c| *c != '+')
            .collect::<String>();

        let filename = format!("{}{}.{}", prefix, name, ext);
        let path = archive_subdir.join(&filename);

        // Handle conflicts
        let path = resolve_conflict(&path);

        std::fs::write(&path, doc.document.to_string())?;
    }

    Ok(())
}

fn resolve_conflict(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    let mut counter = 2;
    loop {
        let name = if ext.is_empty() {
            format!("{}-{}", stem, counter)
        } else {
            format!("{}-{}.{}", stem, counter, ext)
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Export archived documents as a ZIP file
pub fn export_to_zip(archived_docs: &[ScratchDocument], dest_path: &Path) -> std::io::Result<()> {
    use std::io::Write;

    // We create a simple ZIP without external deps using a minimal implementation
    // For a production app, we'd use the `zip` crate
    let mut zip_file = std::fs::File::create(dest_path)?;

    // Simple ZIP: We'll just write files as a flat list for now
    // This is a basic ZIP archive structure
    let mut central_directory = Vec::new();
    let mut offset = 0u32;

    let now = SystemTime::now();
    let date_time = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for (i, doc) in archived_docs.iter().enumerate() {
        let name = auto_name(&doc.document);
        let ext = doc
            .language
            .as_deref()
            .unwrap_or("md")
            .chars()
            .take_while(|c| *c != '+')
            .collect::<String>();

        let filename = format!("{:02}-{}.{}", i + 1, name, ext);
        let content = doc.document.to_string();

        // Local file header
        let local_header = create_zip_local_header(&filename, content.len(), date_time);
        zip_file.write_all(&local_header)?;
        zip_file.write_all(content.as_bytes())?;

        central_directory.push(create_zip_central_header(
            &filename,
            content.len(),
            offset,
            date_time,
        ));

        offset += local_header.len() as u32 + content.len() as u32;
    }

    // Write central directory
    let cd_start = offset;
    for cd in &central_directory {
        zip_file.write_all(cd)?;
        offset += cd.len() as u32;
    }

    // End of central directory
    let eocd = create_zip_eocd(central_directory.len() as u16, offset - cd_start, cd_start);
    zip_file.write_all(&eocd)?;

    Ok(())
}

fn create_zip_local_header(filename: &str, size: usize, date_time: u64) -> Vec<u8> {
    let name_bytes = filename.as_bytes();
    let (date, time) = dos_date_time(date_time);

    let mut header = Vec::new();
    // Local file header signature
    header.extend_from_slice(&0x04034b50u32.to_le_bytes());
    // Version needed to extract
    header.extend_from_slice(&20u16.to_le_bytes());
    // General purpose bit flag
    header.extend_from_slice(&0u16.to_le_bytes());
    // Compression method (stored)
    header.extend_from_slice(&0u16.to_le_bytes());
    // Last mod file time
    header.extend_from_slice(&time.to_le_bytes());
    // Last mod file date
    header.extend_from_slice(&date.to_le_bytes());
    // CRC-32 (we'll set it to 0 for now)
    header.extend_from_slice(&0u32.to_le_bytes());
    // Compressed size
    header.extend_from_slice(&(size as u32).to_le_bytes());
    // Uncompressed size
    header.extend_from_slice(&(size as u32).to_le_bytes());
    // Filename length
    header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    // Extra field length
    header.extend_from_slice(&0u16.to_le_bytes());
    // Filename
    header.extend_from_slice(name_bytes);

    header
}

fn create_zip_central_header(filename: &str, size: usize, offset: u32, date_time: u64) -> Vec<u8> {
    let name_bytes = filename.as_bytes();
    let (date, time) = dos_date_time(date_time);

    let mut header = Vec::new();
    // Central directory header signature
    header.extend_from_slice(&0x02014b50u32.to_le_bytes());
    // Version made by
    header.extend_from_slice(&20u16.to_le_bytes());
    // Version needed to extract
    header.extend_from_slice(&20u16.to_le_bytes());
    // General purpose bit flag
    header.extend_from_slice(&0u16.to_le_bytes());
    // Compression method
    header.extend_from_slice(&0u16.to_le_bytes());
    // Last mod file time
    header.extend_from_slice(&time.to_le_bytes());
    // Last mod file date
    header.extend_from_slice(&date.to_le_bytes());
    // CRC-32
    header.extend_from_slice(&0u32.to_le_bytes());
    // Compressed size
    header.extend_from_slice(&(size as u32).to_le_bytes());
    // Uncompressed size
    header.extend_from_slice(&(size as u32).to_le_bytes());
    // Filename length
    header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    // Extra field length
    header.extend_from_slice(&0u16.to_le_bytes());
    // File comment length
    header.extend_from_slice(&0u16.to_le_bytes());
    // Disk number start
    header.extend_from_slice(&0u16.to_le_bytes());
    // Internal file attributes
    header.extend_from_slice(&0u16.to_le_bytes());
    // External file attributes
    header.extend_from_slice(&0u32.to_le_bytes());
    // Relative offset of local header
    header.extend_from_slice(&offset.to_le_bytes());
    // Filename
    header.extend_from_slice(name_bytes);

    header
}

fn create_zip_eocd(num_entries: u16, cd_size: u32, cd_offset: u32) -> Vec<u8> {
    let mut eocd = Vec::new();
    // End of central directory signature
    eocd.extend_from_slice(&0x06054b50u32.to_le_bytes());
    // Number of this disk
    eocd.extend_from_slice(&0u16.to_le_bytes());
    // Disk where central directory starts
    eocd.extend_from_slice(&0u16.to_le_bytes());
    // Number of central directory records on this disk
    eocd.extend_from_slice(&num_entries.to_le_bytes());
    // Total number of central directory records
    eocd.extend_from_slice(&num_entries.to_le_bytes());
    // Size of central directory
    eocd.extend_from_slice(&cd_size.to_le_bytes());
    // Offset of start of central directory
    eocd.extend_from_slice(&cd_offset.to_le_bytes());
    // ZIP file comment length
    eocd.extend_from_slice(&0u16.to_le_bytes());

    eocd
}

fn dos_date_time(unix_time: u64) -> (u16, u16) {
    let secs = unix_time;
    // Convert unix timestamp to DOS date/time
    let date = time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());

    let dos_time =
        ((date.hour() as u16) << 11) | ((date.minute() as u16) << 5) | ((date.second() as u16) / 2);
    let dos_date =
        (((date.year() - 1980) as u16) << 9) | ((date.month() as u16) << 5) | (date.day() as u16);

    (dos_date, dos_time)
}
