// Este módulo expone una API más amplia (parking/archivado de documentos, título
// derivado del contenido, `export_to_folder`, etc.) que la que la ventana actual del
// wiring ejerce: son capacidades ya implementadas y probadas, pensadas para un panel de
// "borradores archivados" que no forma parte de este pase de reescritura de main.rs.
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
    /// Carga el índice. Si el archivo existe pero está corrupto (no es JSON válido), se
    /// renombra a `.bak` en vez de dejarlo para que el siguiente `save()` lo sobrescriba en
    /// silencio (hallazgo C4): así el usuario conserva una copia para inspeccionar/recuperar.
    pub fn load(base_dir: &Path) -> Self {
        let index_path = base_dir.join("scratch-index.json");
        // Se lee como bytes en vez de `read_to_string` (hallazgo L11): un índice no-UTF-8
        // (corrupción de disco, escritura a medias con bytes inválidos) hacía que
        // `read_to_string` fallara con el mismo `Err` que "no existe", perdiendo el índice
        // completo en silencio sin dejar ninguna copia de seguridad.
        let bytes = match std::fs::read(&index_path) {
            Ok(b) => b,
            Err(_) => return Self { documents: Vec::new() },
        };
        let bytes = bytes
            .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
            .unwrap_or(&bytes);
        let parsed = std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<ScratchMetadata>>(s).ok());
        match parsed {
            Some(docs) => Self { documents: docs },
            None => {
                Self::quarantine_corrupt_index(base_dir, &index_path);
                Self { documents: Vec::new() }
            }
        }
    }

    /// Renombra un índice corrupto (JSON inválido o bytes no-UTF-8) a `.bak` para que el
    /// usuario conserve una copia inspeccionable, en vez de perderlo cuando el siguiente
    /// `save()` lo sobrescriba en silencio. Si ya existe un `.bak` de una corrupción anterior,
    /// usa un nombre con marca de tiempo para no perder tampoco esa copia previa (hallazgo L11:
    /// "no sobrescribir .bak previo").
    fn quarantine_corrupt_index(base_dir: &Path, index_path: &Path) {
        let plain_bak = base_dir.join("scratch-index.json.bak");
        let target = if plain_bak.exists() {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            base_dir.join(format!("scratch-index.json.bak-{}", ts))
        } else {
            plain_bak
        };
        std::fs::rename(index_path, target).ok();
    }

    /// Guarda el índice de forma atómica (hallazgo C4: nunca dejar `scratch-index.json` a
    /// medio escribir).
    pub fn save(&self, base_dir: &Path) -> std::io::Result<()> {
        let index_path = base_dir.join("scratch-index.json");
        let data = serde_json::to_string_pretty(&self.documents)?;
        crate::workspace::atomic_save(&index_path, &data)
    }

    /// Inserta o actualiza (por `id`) una entrada del índice; nunca genera duplicados.
    /// Conserva el `created_at` original si ya existía (hallazgo G: guardar repetidamente un
    /// borrador no debe resetear su fecha de creación).
    pub fn add(&mut self, meta: ScratchMetadata) {
        if let Some(existing) = self.documents.iter_mut().find(|d| d.id == meta.id) {
            let created_at = existing.created_at;
            *existing = meta;
            existing.created_at = created_at;
        } else {
            self.documents.push(meta);
        }
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

    /// Genera un id único para un nuevo borrador. Incluye un contador atómico monótono además
    /// del timestamp (hallazgo G): dos llamadas muy próximas en el tiempo podrían obtener el
    /// mismo timestamp de reloj (resolución/redondeo del sistema), lo que antes podía producir
    /// ids duplicados; el contador garantiza unicidad dentro del proceso.
    pub fn next_id_public(&self) -> String {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or_else(|_| rand_u64());
        format!("scratch-{}-{}", timestamp, n)
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
        // Escritura atómica (hallazgo L8): un corte de luz/proceso a mitad de un
        // `std::fs::write` dejaba el borrador truncado o vacío en disco.
        crate::workspace::atomic_save(&doc.file_path, &doc.document.to_string())?;
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

    /// Guarda varios borradores en un solo paso, cargando y guardando el índice una única vez
    /// (evita E/S redundante frente a llamar `save_scratch` en bucle). Escritura atómica por
    /// borrador (hallazgo L8). Si UNO falla (disco lleno, ruta inválida, etc.), los DEMÁS se
    /// siguen guardando en vez de abortar todo el lote con `?` (hallazgo L9): el índice se
    /// guarda solo con los que tuvieron éxito, y se devuelve el primer error encontrado para
    /// que el llamador lo pueda mostrar (pero ya con el resto de borradores a salvo en disco).
    pub fn save_scratch_batch(&self, docs: &mut [ScratchDocument]) -> std::io::Result<()> {
        let mut index = ScratchIndex::load(&self.base_dir);
        let mut first_error: Option<std::io::Error> = None;
        for doc in docs.iter_mut() {
            if let Err(e) = crate::workspace::atomic_save(&doc.file_path, &doc.document.to_string())
            {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                continue;
            }
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
            doc.mark_saved();
        }
        index.save(&self.base_dir)?;
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
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

/// Heurística SQL estricta (hallazgo L23): reimplementación local de
/// `editor::looks_like_sql` (no se puede depender de `editor.rs`, que pertenece al pase de
/// wiring en curso). Antes, `detect_language` usaba `contains("select ")`, que etiquetaba como
/// SQL cualquier prosa que contuviera la palabra "select" en cualquier contexto (p.ej. "please
/// select an option below"). Exige la forma de una instrucción real (`SELECT ... FROM`,
/// `INSERT INTO`, `UPDATE ... SET`, `DELETE FROM`, `CREATE TABLE/VIEW/...`, `WITH x AS (`).
fn looks_like_sql(sample: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::RegexBuilder::new(
            r"^\s*(select\b.+\bfrom\b|insert\s+into\b|update\s+\S+\s+set\b|delete\s+from\b|create\s+(table|view|index|database|procedure|function)\b|with\s+\w+\s+as\s*\()",
        )
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .expect("regex SQL heurística válida")
    });
    re.is_match(sample)
}

pub fn detect_language(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if lower.contains("```sql") || looks_like_sql(content) || lower.contains("create table") {
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

// ---------------------------------------------------------------------------
// Nombres de borradores (WS-B): metadatos cosméticos; los ids en disco no cambian.
// ---------------------------------------------------------------------------

/// Opciones de nombrado derivadas de `Settings` (ver `NamingOpts::from_settings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamingOpts {
    pub auto_name: bool,
    /// 0 ninguna, 1 dd-mm-aaaa, 2 aaaa-mm-dd
    pub date_format: u32,
    /// 0 prefijo, 1 sufijo
    pub date_position: u32,
    pub date_in_tab: bool,
}

impl Default for NamingOpts {
    fn default() -> Self {
        Self { auto_name: true, date_format: 0, date_position: 1, date_in_tab: false }
    }
}

impl NamingOpts {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Self {
            auto_name: s.draft_auto_name,
            date_format: s.draft_date_format.min(2),
            date_position: s.draft_date_position.min(1),
            date_in_tab: s.draft_date_in_tab,
        }
    }
}

const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Limpia un texto para usarlo como nombre de archivo (sin extensión). Vacío si no queda nada.
pub fn sanitize_file_stem(s: &str) -> String {
    let replaced: String = s
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = collapsed.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    const MAX: usize = 60;
    if out.chars().count() > MAX {
        let cut = out.char_indices().nth(MAX).map(|(i, _)| i).unwrap_or(out.len());
        let head = &out[..cut];
        // Preferir cortar en el último espacio si queda al menos 30 caracteres.
        let end = match head.rfind(' ') {
            Some(sp) if head[..sp].chars().count() >= 30 => sp,
            _ => cut,
        };
        out = out[..end].trim_matches(|c: char| c == '.' || c == ' ').to_string();
    }
    let first_seg = out.split('.').next().unwrap_or("").trim().to_ascii_uppercase();
    if RESERVED_NAMES.contains(&first_seg.as_str()) {
        out.push('_');
    }
    out
}

/// Título automático a partir del contenido: primer encabezado ATX (en las primeras 50 líneas) o
/// primera línea no vacía; máx. 6 palabras; saneado. Lee como mucho ~4 KB.
pub fn auto_title(doc: &Rope) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut bytes = 0usize;
    for lr in doc.lines(LineType::Unicode).take(50) {
        let l: String = lr.to_string().chars().take(300).collect();
        bytes += l.len();
        lines.push(l);
        if bytes >= 4096 {
            break;
        }
    }
    // 1) encabezado ATX: "# texto"
    for l in &lines {
        let t = l.trim();
        if t.starts_with('#') {
            let rest = t.trim_start_matches('#');
            if rest.starts_with(' ') || rest.starts_with('\t') {
                if let Some(x) = clean_title_line(rest) {
                    return Some(x);
                }
            }
        }
    }
    // 2) primera línea con texto útil
    for l in &lines {
        if l.trim().is_empty() {
            continue;
        }
        if let Some(x) = clean_title_line(l) {
            return Some(x);
        }
    }
    None
}

fn clean_title_line(line: &str) -> Option<String> {
    let noise_lead = |c: char| c.is_whitespace() || matches!(c, '#' | '>' | '-' | '*' | '_' | '`' | '{' | '[' | '"' | '\'' | '=' | '~');
    let noise_trail = |c: char| c.is_whitespace() || matches!(c, '}' | ']' | '"' | '\'' | ',' | ':' | ';' | '*' | '_' | '`' | '{' | '[');
    let t = line.trim_start_matches(noise_lead).trim_end_matches(noise_trail);
    if !t.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }
    let words: Vec<&str> = t.split_whitespace().take(6).collect();
    let s = sanitize_file_stem(&words.join(" "));
    if s.chars().any(|c| c.is_alphanumeric()) { Some(s) } else { None }
}

/// Fecha `unix` (segundos) en el formato pedido con un desfase explícito (testeable).
pub fn format_date_with_offset(unix: u64, fmt: u32, offset: time::UtcOffset) -> Option<String> {
    if fmt == 0 {
        return None;
    }
    let dt = time::OffsetDateTime::from_unix_timestamp(unix as i64).ok()?.to_offset(offset);
    let (y, m, d) = (dt.year(), dt.month() as u8, dt.day());
    Some(match fmt {
        1 => format!("{:02}-{:02}-{:04}", d, m, y),
        _ => format!("{:04}-{:02}-{:02}", y, m, d),
    })
}

/// Fecha en hora local (fallback UTC si el SO no da el desfase).
pub fn format_date(unix: u64, fmt: u32) -> Option<String> {
    let off = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    format_date_with_offset(unix, fmt, off)
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `position`: 0 prefijo ("26-09-2026 Nota"), otro sufijo ("Nota 26-09-2026").
pub fn compose_name(base: &str, date: Option<String>, position: u32) -> String {
    match date {
        None => base.to_string(),
        Some(d) if position == 0 => format!("{} {}", d, base),
        Some(d) => format!("{} {}", base, d),
    }
}

/// Extensión de archivo para un id de lenguaje de `editor::detect_language`.
pub fn extension_for_language(lang: &str) -> &'static str {
    match lang {
        "json" => "json",
        "sql" => "sql",
        "html" => "html",
        "xml" => "xml",
        "javascript" => "js",
        "typescript" => "ts",
        "rust" => "rs",
        "python" => "py",
        "toml" => "toml",
        "yaml" => "yaml",
        "css" => "css",
        "shell" => "sh",
        "powershell" => "ps1",
        "ini" => "ini",
        "c" => "c",
        "cpp" => "cpp",
        "csharp" => "cs",
        "java" => "java",
        "go" => "go",
        "lua" => "lua",
        _ => "md",
    }
}

/// Hace únicos (sin distinguir mayúsculas) los nombres de una lista: "a.md", "a (2).md", ...
pub fn dedupe_file_names(names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    names
        .into_iter()
        .map(|n| {
            if seen.insert(n.to_lowercase()) {
                return n;
            }
            let (stem, ext) = match n.rfind('.') {
                Some(i) if i > 0 => (n[..i].to_string(), n[i..].to_string()),
                _ => (n.clone(), String::new()),
            };
            let mut k = 2;
            loop {
                let cand = format!("{} ({}){}", stem, k, ext);
                if seen.insert(cand.to_lowercase()) {
                    return cand;
                }
                k += 1;
            }
        })
        .collect()
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
    export_to_zip_named(archived_docs, None, dest_path)
}

/// Como `export_to_zip`; si `names` (uno por documento, ya saneados/únicos) se da, se usan como
/// nombres de entrada tal cual en vez de `NN-<auto_name>.<ext>`.
pub fn export_to_zip_named(
    archived_docs: &[ScratchDocument],
    names: Option<&[String]>,
    dest_path: &Path,
) -> std::io::Result<()> {
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

        let filename = match names.and_then(|n| n.get(i)) {
            Some(n) => n.clone(),
            None => format!("{:02}-{}.{}", i + 1, name, ext),
        };
        let content = doc.document.to_string();
        let crc = crc32(content.as_bytes());

        // Local file header
        let local_header = create_zip_local_header(&filename, content.len(), date_time, crc);
        zip_file.write_all(&local_header)?;
        zip_file.write_all(content.as_bytes())?;

        central_directory.push(create_zip_central_header(
            &filename,
            content.len(),
            offset,
            date_time,
            crc,
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

fn create_zip_local_header(filename: &str, size: usize, date_time: u64, crc: u32) -> Vec<u8> {
    let name_bytes = filename.as_bytes();
    let (date, time) = dos_date_time(date_time);

    let mut header = Vec::new();
    // Local file header signature
    header.extend_from_slice(&0x04034b50u32.to_le_bytes());
    // Version needed to extract
    header.extend_from_slice(&20u16.to_le_bytes());
    // General purpose bit flag: bit 11 (0x0800) = nombre de archivo/comentario en UTF-8
    // (hallazgo G), para que extractores no asuman CP437/latin1 con nombres no-ASCII.
    header.extend_from_slice(&0x0800u16.to_le_bytes());
    // Compression method (stored)
    header.extend_from_slice(&0u16.to_le_bytes());
    // Last mod file time
    header.extend_from_slice(&time.to_le_bytes());
    // Last mod file date
    header.extend_from_slice(&date.to_le_bytes());
    // CRC-32
    header.extend_from_slice(&crc.to_le_bytes());
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

fn create_zip_central_header(
    filename: &str,
    size: usize,
    offset: u32,
    date_time: u64,
    crc: u32,
) -> Vec<u8> {
    let name_bytes = filename.as_bytes();
    let (date, time) = dos_date_time(date_time);

    let mut header = Vec::new();
    // Central directory header signature
    header.extend_from_slice(&0x02014b50u32.to_le_bytes());
    // Version made by
    header.extend_from_slice(&20u16.to_le_bytes());
    // Version needed to extract
    header.extend_from_slice(&20u16.to_le_bytes());
    // General purpose bit flag: bit 11 (0x0800) = UTF-8, igual que en el header local.
    header.extend_from_slice(&0x0800u16.to_le_bytes());
    // Compression method
    header.extend_from_slice(&0u16.to_le_bytes());
    // Last mod file time
    header.extend_from_slice(&time.to_le_bytes());
    // Last mod file date
    header.extend_from_slice(&date.to_le_bytes());
    // CRC-32
    header.extend_from_slice(&crc.to_le_bytes());
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

/// Genera la tabla de CRC-32 (polinomio IEEE 802.3, 0xEDB88320) usada por el formato ZIP.
fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut c = i;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
}

/// Calcula el CRC-32 (variante ZIP/PNG) de `data`.
fn crc32(data: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        let idx = ((crc ^ b as u32) & 0xFF) as usize;
        crc = table[idx] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_known_vector() {
        assert_eq!(crc32(b"hello"), 0x3610a686);
    }

    #[test]
    fn crc32_empty_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn scratch_index_add_upserts_by_id() {
        let mut index = ScratchIndex { documents: Vec::new() };
        index.add(ScratchMetadata {
            id: "a".to_string(),
            title: "First".to_string(),
            file_path: "a.md".to_string(),
            created_at: 1,
            modified_at: 1,
            language: None,
            status: DocumentStatus::Active,
        });
        index.add(ScratchMetadata {
            id: "a".to_string(),
            title: "Updated".to_string(),
            file_path: "a.md".to_string(),
            created_at: 1,
            modified_at: 2,
            language: None,
            status: DocumentStatus::Active,
        });
        assert_eq!(index.documents.len(), 1);
        assert_eq!(index.documents[0].title, "Updated");
    }

    #[test]
    fn next_id_public_is_unique_even_when_called_rapidly() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-ids-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let manager = ScratchManager::new(dir.clone());
        let ids: Vec<String> = (0..50).map(|_| manager.next_id_public()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_index_is_renamed_to_bak_not_overwritten_silently() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-corrupt-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scratch-index.json"), "{ not valid json").unwrap();
        let index = ScratchIndex::load(&dir);
        assert!(index.documents.is_empty());
        assert!(dir.join("scratch-index.json.bak").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_scratch_batch_preserves_created_at_across_saves() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-created-at-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let manager = ScratchManager::new(dir.clone());
        let mut doc = manager.create_scratch_with_id("t1".to_string(), Rope::from_str("v1"));
        doc.created_at = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        manager.save_scratch_batch(&mut [doc]).unwrap();

        let mut doc2 = manager.create_scratch_with_id("t1".to_string(), Rope::from_str("v2"));
        doc2.created_at = SystemTime::UNIX_EPOCH + Duration::from_secs(999); // distinto a propósito
        manager.save_scratch_batch(&mut [doc2]).unwrap();

        let index = ScratchIndex::load(&dir);
        let entry = index.documents.iter().find(|d| d.id == "t1").unwrap();
        assert_eq!(entry.created_at, 100);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_to_zip_produces_valid_signature_and_crc() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-zip-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = ScratchManager::new(dir.clone());
        let mut doc = manager.create_scratch_with_id("t1".to_string(), Rope::from_str("hello"));
        doc.language = Some("txt".to_string());

        let zip_path = dir.join("out.zip");
        export_to_zip(&[doc], &zip_path).unwrap();

        let bytes = std::fs::read(&zip_path).unwrap();
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]); // "PK\x03\x04"

        // El CRC-32 del contenido local debe coincidir con crc32("hello").
        let crc_bytes = &bytes[14..18];
        let crc = u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
        assert_eq!(crc, 0x3610a686);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_scratch_batch_leaves_no_tmp_file_atomic_write() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-atomic-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let manager = ScratchManager::new(dir.clone());
        let mut doc = manager.create_scratch_with_id("a".to_string(), Rope::from_str("hola"));
        manager.save_scratch_batch(std::slice::from_mut(&mut doc)).unwrap();
        assert_eq!(std::fs::read_to_string(&doc.file_path).unwrap(), "hola");
        assert!(!doc.file_path.with_extension("md.tmp").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_non_utf8_index_is_quarantined_not_lost_silently() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-nonutf8-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scratch-index.json"), [b'[', 0xFF, b']']).unwrap();
        let idx = ScratchIndex::load(&dir);
        assert!(idx.documents.is_empty());
        assert!(dir.join("scratch-index.json.bak").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn second_corruption_does_not_overwrite_previous_bak() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-doublecorrupt-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scratch-index.json"), "{corrupt-1").unwrap();
        let _ = ScratchIndex::load(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("scratch-index.json.bak")).unwrap(),
            "{corrupt-1"
        );
        std::fs::write(dir.join("scratch-index.json"), "{corrupt-2").unwrap();
        let _ = ScratchIndex::load(&dir);
        // El .bak original (corrupt-1) debe seguir intacto; la segunda corrupción se guarda
        // aparte con marca de tiempo, nunca pisando la copia anterior.
        assert_eq!(
            std::fs::read_to_string(dir.join("scratch-index.json.bak")).unwrap(),
            "{corrupt-1"
        );
        let has_timestamped_bak = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("scratch-index.json.bak-")
            });
        assert!(has_timestamped_bak, "esperaba un .bak-<timestamp> para la segunda corrupción");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_scratch_batch_continues_after_one_failure() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-scratch-partial-fail-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let manager = ScratchManager::new(dir.clone());
        let mut good1 = manager.create_scratch_with_id("ok1".to_string(), Rope::from_str("uno"));
        // Ruta con un directorio padre inexistente: falla al escribir.
        let mut bad = ScratchDocument::new(
            "bad".to_string(),
            Rope::from_str("x"),
            &dir.join("no-existe-dir-alguno"),
        );
        let mut good2 = manager.create_scratch_with_id("ok2".to_string(), Rope::from_str("dos"));

        let result = manager.save_scratch_batch(std::slice::from_mut(&mut good1));
        assert!(result.is_ok());
        let result_bad = manager.save_scratch_batch(std::slice::from_mut(&mut bad));
        assert!(result_bad.is_err());
        let result2 = manager.save_scratch_batch(std::slice::from_mut(&mut good2));
        assert!(result2.is_ok());

        // Ambos borradores "buenos" deben haberse guardado (y estar en el índice) a pesar del
        // fallo del malo en su propio lote.
        let index = ScratchIndex::load(&dir);
        assert!(index.documents.iter().any(|d| d.id == "ok1"));
        assert!(index.documents.iter().any(|d| d.id == "ok2"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_language_requires_real_sql_statement_shape() {
        assert_eq!(
            detect_language("please select an option from the menu below"),
            None
        );
        assert_eq!(
            detect_language("SELECT id, name FROM users WHERE active = 1"),
            Some("sql".to_string())
        );
        assert_eq!(
            detect_language("INSERT INTO users (id) VALUES (1)"),
            Some("sql".to_string())
        );
    }

    // --- WS-B: nombres ---

    fn rope(s: &str) -> Rope {
        Rope::from_str(s)
    }

    #[test]
    fn ws_b_sanitize_file_stem_table() {
        assert_eq!(sanitize_file_stem("a/b:c"), "a b c");
        assert_eq!(sanitize_file_stem("  hola   mundo.. "), "hola mundo");
        assert_eq!(sanitize_file_stem("Año nuevo ñandú"), "Año nuevo ñandú");
        assert_eq!(sanitize_file_stem("fiesta 🎉 hoy"), "fiesta 🎉 hoy");
        assert_eq!(sanitize_file_stem("con"), "con_");
        assert_eq!(sanitize_file_stem("NUL"), "NUL_");
        assert_eq!(sanitize_file_stem("com1"), "com1_");
        assert_eq!(sanitize_file_stem("lpt9.txt"), "lpt9.txt_");
        assert_eq!(sanitize_file_stem("console"), "console");
        assert_eq!(sanitize_file_stem("<>|?*\\\""), "");
        assert_eq!(sanitize_file_stem("a\u{7}b\tc"), "a b c");
        let long = "palabra ".repeat(20);
        let s = sanitize_file_stem(&long);
        assert!(s.chars().count() <= 60 && !s.ends_with(' '), "{s}");
        let nosp = "ñ".repeat(100);
        assert_eq!(sanitize_file_stem(&nosp).chars().count(), 60);
    }

    #[test]
    fn ws_b_auto_title_cases() {
        assert_eq!(auto_title(&rope("# Plan de viaje\ncuerpo")), Some("Plan de viaje".into()));
        assert_eq!(auto_title(&rope("intro\n\n## Segundo título\n")), Some("Segundo título".into()));
        assert_eq!(auto_title(&rope("\n\n  Primera línea útil  \nsegunda")), Some("Primera línea útil".into()));
        assert_eq!(auto_title(&rope("uno dos tres cuatro cinco seis siete ocho")), Some("uno dos tres cuatro cinco seis".into()));
        assert_eq!(auto_title(&rope("")), None);
        assert_eq!(auto_title(&rope("   \n\n")), None);
        assert_eq!(auto_title(&rope("---\n***\n")), None);
        assert_eq!(auto_title(&rope("{\n  \"nombre\": \"Ñu\"\n}")), Some("nombre Ñu".into()));
        assert_eq!(auto_title(&rope("> - **Lista** importante")), Some("Lista importante".into()));
        assert_eq!(auto_title(&rope("con/aux:?")), Some("con aux".into()));
        assert_eq!(auto_title(&rope("CON")), Some("CON_".into()));
        assert_eq!(auto_title(&rope("😀 fiesta")), Some("😀 fiesta".into()));
        let long = format!("{}\nx", "a".repeat(5000));
        assert_eq!(auto_title(&rope(&long)).unwrap().chars().count(), 60);
        assert_eq!(auto_title(&rope("#hashtag sin espacio")), Some("hashtag sin espacio".into()));
    }

    #[test]
    fn ws_b_format_date_and_compose() {
        // 2026-09-05 12:00:00 UTC
        let ts = 1_788_609_600u64;
        let utc = time::UtcOffset::UTC;
        assert_eq!(format_date_with_offset(ts, 1, utc).as_deref(), Some("05-09-2026"));
        assert_eq!(format_date_with_offset(ts, 2, utc).as_deref(), Some("2026-09-05"));
        assert_eq!(format_date_with_offset(ts, 0, utc), None);
        // desfase: 23:00 UTC + 2h cruza de día
        let off = time::UtcOffset::from_hms(2, 0, 0).unwrap();
        assert_eq!(format_date_with_offset(ts + 11 * 3600, 2, off).as_deref(), Some("2026-09-06"));
        // fallback local: nunca falla
        assert!(format_date(ts, 1).is_some());
        assert_eq!(compose_name("Nota", Some("05-09-2026".into()), 0), "05-09-2026 Nota");
        assert_eq!(compose_name("Nota", Some("05-09-2026".into()), 1), "Nota 05-09-2026");
        assert_eq!(compose_name("Nota", None, 0), "Nota");
    }

    #[test]
    fn ws_b_extension_and_zip_dedupe() {
        assert_eq!(extension_for_language("json"), "json");
        assert_eq!(extension_for_language("plaintext"), "md");
        assert_eq!(extension_for_language("markdown"), "md");
        assert_eq!(extension_for_language("csharp"), "cs");
        let v = dedupe_file_names(vec!["a.md".into(), "A.md".into(), "b.md".into(), "a.md".into(), "x".into(), "x".into()]);
        assert_eq!(v, ["a.md", "A (2).md", "b.md", "a (3).md", "x", "x (2)"]);
    }
}
