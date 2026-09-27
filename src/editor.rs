//! Modelo de documentos/estado del editor: pila de deshacer, pestañas, grupos y el estado
//! global (`EditorState`), independiente de la UI. Sustituye a las copias que main.rs sigue
//! usando internamente hasta que se reescriba.
use crate::scratch::ScratchManager;
use crate::search;
use crate::session::{GroupState, Session, SessionManager, TabState};
use ropey::Rope;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

// ---------------------------------------------------------------------------
// UndoStack
// ---------------------------------------------------------------------------

/// Pila de deshacer/rehacer de snapshots `(documento, posición del cursor en bytes)`.
///
/// Invariante: `history[position] == documento en vivo` en todo momento (se mantiene
/// llamando a `record` después de CADA edición, nunca antes). Esto corrige el modelo previo
/// (hallazgo A), en el que se empujaba el estado PREVIO a la edición y `undo` sobrescribía
/// `history[pos]` con el doc vivo y retrocedía otra posición más, perdiendo un estado por cada
/// deshacer y fundiendo comandos discretos con el tecleo.
#[derive(Clone)]
pub struct UndoStack {
    history: Vec<(Rope, usize)>,
    position: usize,
}

const UNDO_CAP: usize = 500;

impl UndoStack {
    /// Crea una pila con un único estado inicial (el estado en vivo al abrir/crear la pestaña).
    pub fn new(document: Rope, cursor: usize) -> Self {
        Self {
            history: vec![(document, cursor)],
            position: 0,
        }
    }

    /// Registra el estado DESPUÉS de una edición.
    /// - `coalesce = true`: sustituye `history[position]` (funde con el paso anterior, p.ej.
    ///   tecleo en ráfaga) y descarta cualquier redo. Si `position == 0` (el estado base, el
    ///   que existía al abrir el documento) nunca se sobrescribe: en ese caso se comporta como
    ///   `coalesce = false` para no perder el estado original.
    /// - `coalesce = false`: trunca el redo y empuja un nuevo paso.
    ///
    /// Se limita a `UNDO_CAP` estados.
    pub fn record(&mut self, document: Rope, cursor: usize, coalesce: bool) {
        if coalesce && self.position > 0 {
            // L20: si se deshizo antes de este tecleo, `history` todavía conserva los
            // pasos de rehacer posteriores a `position`. Sin truncar aquí, ese redo
            // "fantasma" (que ya no corresponde al futuro real del documento, que acaba
            // de cambiar) seguía siendo alcanzable con Rehacer tras fundir esta edición.
            self.history.truncate(self.position + 1);
            self.history[self.position] = (document, cursor);
            return;
        }
        self.history.truncate(self.position + 1);
        self.history.push((document, cursor));
        self.position += 1;
        if self.history.len() > UNDO_CAP {
            self.history.remove(0);
            self.position -= 1;
        }
    }

    /// Deshace un paso. Devuelve `None` si ya se está en el estado más antiguo.
    pub fn undo(&mut self) -> Option<(Rope, usize)> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        self.history.get(self.position).cloned()
    }

    /// Rehace un paso. Devuelve `None` si ya se está en el estado más reciente.
    pub fn redo(&mut self) -> Option<(Rope, usize)> {
        if self.position + 1 >= self.history.len() {
            return None;
        }
        self.position += 1;
        self.history.get(self.position).cloned()
    }

    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.position + 1 < self.history.len()
    }

}

// ---------------------------------------------------------------------------
// Tab / Group
// ---------------------------------------------------------------------------

/// Una pestaña abierta: o bien un borrador (autoguardado) o bien un archivo real del disco.
pub struct Tab {
    pub title: String,
    pub document: Rope,
    pub scratch_id: Option<String>,
    pub file_path: Option<PathBuf>,
    pub last_known_mtime: Option<SystemTime>,
    /// Contenido tal como estaba la última vez que se guardó/cargó (para saber si está "sucia").
    pub saved_snapshot: Rope,
    /// Contenido tal como estaba en el último autoguardado de borrador (hallazgo C5): permite
    /// que `EditorState::save_changed_scratches` reescriba solo los borradores que cambiaron
    /// desde el autoguardado anterior en vez de reescribir todos en cada tick.
    pub autosaved_snapshot: Rope,
    pub undo: UndoStack,
    pub cursor: usize,
    /// Instante del último tecleo registrado (para el debounce de fusión de undo); `None` tras
    /// un comando discreto o un undo/redo, para que el siguiente tecleo no se funda con ellos.
    pub last_typing: Option<Instant>,
    /// Instante en que empezó la ráfaga de tecleo ACTUAL que se sigue fundiendo en un solo
    /// paso de undo (hallazgo C19): sin esto, escribir sin pausas de más de 600ms (el
    /// debounce de `record_typing`) fundía TODA la sesión de tecleo en un ÚNICO paso de
    /// undo sin límite de tiempo — un `Ctrl+Z` tras escribir varios párrafos sin parar
    /// deshacía todo de golpe. Se fuerza un paso nuevo tras ~2s de ráfaga continua, con
    /// independencia de que cada tecla individual llegara a tiempo del debounce.
    pub burst_started: Option<Instant>,
    /// Estilo de fin de línea del archivo tal como se cargó (hallazgo E11): el documento en
    /// memoria siempre usa `\n`; se restaura al guardar.
    pub line_ending: crate::textops::LineEnding,
    /// Si el archivo tenía BOM UTF-8 al cargarlo (hallazgo M4): se restaura al guardar.
    pub had_bom: bool,
    /// `false` si la última carga/restauración del borrador falló de forma no recuperable
    /// (hallazgo C1): en ese caso no se debe autoguardar para no sobrescribir el archivo
    /// original en disco con contenido vacío.
    pub autosave_enabled: bool,
}

impl Tab {
    /// Crea una pestaña de borrador (autoguardado, nunca "sucia").
    pub fn new_scratch(scratch_id: String, document: Rope, title: String) -> Self {
        Self {
            title,
            document: document.clone(),
            scratch_id: Some(scratch_id),
            file_path: None,
            last_known_mtime: None,
            saved_snapshot: document.clone(),
            autosaved_snapshot: document.clone(),
            undo: UndoStack::new(document, 0),
            cursor: 0,
            last_typing: None,
            burst_started: None,
            line_ending: crate::textops::LineEnding::Lf,
            had_bom: false,
            autosave_enabled: true,
        }
    }

    /// Crea una pestaña de archivo real. `document` se considera el contenido "guardado" hasta
    /// que se edite.
    pub fn new_file(path: PathBuf, document: Rope, mtime: Option<SystemTime>) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Sin título".to_string());
        Self {
            title,
            document: document.clone(),
            scratch_id: None,
            file_path: Some(path),
            last_known_mtime: mtime,
            saved_snapshot: document.clone(),
            autosaved_snapshot: document.clone(),
            undo: UndoStack::new(document, 0),
            cursor: 0,
            last_typing: None,
            burst_started: None,
            line_ending: crate::textops::LineEnding::Lf,
            had_bom: false,
            autosave_enabled: true,
        }
    }

    /// `true` si es un borrador (autoguardado en el directorio de scratch).
    pub fn is_scratch(&self) -> bool {
        self.scratch_id.is_some()
    }

    /// Registra el estado tras un tecleo: funde con el registro anterior si el último tecleo
    /// fue hace menos de 600ms (ráfaga) Y la ráfaga actual lleva menos de ~2s en marcha;
    /// empuja un paso nuevo en caso contrario (hallazgo C19: sin el límite de ráfaga, escribir
    /// sin ninguna pausa de más de 600ms fundía TODO el tecleo en un solo paso de undo, sin
    /// límite de tiempo). Nunca se funde con un comando discreto anterior porque éste pone
    /// `last_typing = None`.
    pub fn record_typing(&mut self, cursor: usize) {
        const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(600);
        const MAX_BURST: std::time::Duration = std::time::Duration::from_secs(2);
        let now = Instant::now();
        let coalesce = match (self.last_typing, self.burst_started) {
            (Some(last), Some(start)) => last.elapsed() < DEBOUNCE && start.elapsed() < MAX_BURST,
            _ => false,
        };
        self.undo.record(self.document.clone(), cursor, coalesce);
        self.last_typing = Some(now);
        if !coalesce {
            self.burst_started = Some(now);
        }
    }

    /// Registra el estado tras un comando discreto (ops de línea, formatear, reemplazar,
    /// pegar por menú, recarga externa...): siempre un paso de undo independiente, y resetea
    /// el debounce de tecleo para que el siguiente tecleo no se funda con este comando.
    pub fn record_command(&mut self, cursor: usize) {
        self.undo.record(self.document.clone(), cursor, false);
        self.last_typing = None;
        self.burst_started = None;
    }

    /// Aplica un deshacer: actualiza `document`/`cursor` desde la pila si hay algo que deshacer.
    /// Devuelve `true` si se aplicó.
    pub fn apply_undo(&mut self) -> bool {
        match self.undo.undo() {
            Some((doc, cursor)) => {
                self.document = doc;
                self.cursor = cursor;
                self.last_typing = None;
                self.burst_started = None;
                true
            }
            None => false,
        }
    }

    /// Aplica un rehacer: actualiza `document`/`cursor` desde la pila si hay algo que rehacer.
    /// Devuelve `true` si se aplicó.
    pub fn apply_redo(&mut self) -> bool {
        match self.undo.redo() {
            Some((doc, cursor)) => {
                self.document = doc;
                self.cursor = cursor;
                self.last_typing = None;
                self.burst_started = None;
                true
            }
            None => false,
        }
    }

    /// `true` si el contenido del borrador cambió desde el último autoguardado (hallazgo C5).
    pub fn needs_autosave(&self) -> bool {
        self.is_scratch() && self.autosave_enabled && self.document != self.autosaved_snapshot
    }

    /// Marca el contenido actual como autoguardado.
    pub fn mark_autosaved(&mut self) {
        self.autosaved_snapshot = self.document.clone();
    }

    /// Contenido listo para escribir en disco (hallazgo E11/M4): restaura el estilo de fin de
    /// línea original (`\r\n` si el archivo era CRLF) y reintroduce el BOM UTF-8 si el archivo
    /// lo tenía al abrirlo. El wiring debe usar esto en vez de `tab.document.to_string()` en
    /// todo punto donde se escriba el archivo a disco (guardar, guardar como, guardar todo).
    pub fn content_for_disk(&self) -> String {
        let restored = crate::textops::restore_newlines(&self.document.to_string(), self.line_ending);
        if self.had_bom {
            format!("\u{FEFF}{}", restored)
        } else {
            restored
        }
    }

    /// Un archivo está "sucio" si su contenido difiere del último guardado; un borrador nunca
    /// está sucio (se autoguarda).
    pub fn is_dirty(&self) -> bool {
        if self.is_scratch() {
            false
        } else {
            self.document != self.saved_snapshot
        }
    }

    /// Marca el contenido actual como guardado.
    pub fn mark_saved(&mut self) {
        self.saved_snapshot = self.document.clone();
    }

    /// Título a mostrar en la pestaña, con el indicador "●" si está sucia. El wiring
    /// actual pasa `title` y `dirty` como campos separados a `TabInfo` (la UI dibuja el
    /// punto ella misma), así que este helper de conveniencia queda sin uso por ahora.
    #[allow(dead_code)]
    pub fn display_title(&self) -> String {
        if self.is_dirty() {
            format!("{} ●", self.title)
        } else {
            self.title.clone()
        }
    }

    /// Texto de la tooltip: la ruta completa, o una nota para los borradores.
    pub fn tooltip(&self) -> String {
        match &self.file_path {
            Some(p) => p.to_string_lossy().to_string(),
            None => "Borrador (se guarda automáticamente)".to_string(),
        }
    }
}

/// Un grupo de pestañas (equivalente a "Alt+1..4" en la UI).
pub struct Group {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Group {
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }
}

// ---------------------------------------------------------------------------
// Detección de lenguaje
// ---------------------------------------------------------------------------

/// Detecta el lenguaje de un documento por extensión de archivo, o por heurística de
/// contenido (primeros ~4 KB) cuando no hay ruta (borradores).
pub fn detect_language(path: Option<&Path>, content: &str) -> String {
    if let Some(p) = path {
        let lang = match p.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ext.to_lowercase().as_str() {
                "md" | "markdown" => "markdown",
                "html" | "htm" => "html",
                "xml" | "svg" => "xml",
                "json" => "json",
                "sql" => "sql",
                "js" | "mjs" | "cjs" => "javascript",
                "ts" => "typescript",
                "rs" => "rust",
                "py" => "python",
                "toml" => "toml",
                "yaml" | "yml" => "yaml",
                "css" => "css",
                "sh" => "shell",
                "ps1" => "powershell",
                "ini" => "ini",
                "c" | "h" => "c",
                "cpp" => "cpp",
                "cs" => "csharp",
                "java" => "java",
                "go" => "go",
                "lua" => "lua",
                "txt" => "plaintext",
                _ => "plaintext",
            },
            None => "plaintext",
        };
        return lang.to_string();
    }

    // Sin ruta (borrador): heurística de contenido sobre los primeros ~4 KB (salvo JSON, que se
    // parsea completo hasta un tope razonable, para no fallar en documentos JSON > 4KB).
    let mut limit = content.len().min(4096);
    while limit > 0 && !content.is_char_boundary(limit) {
        limit -= 1;
    }
    let sample = content[..limit].trim();

    let trimmed_full = content.trim_start();
    let starts_like_json = trimmed_full.starts_with('{') || trimmed_full.starts_with('[');
    if starts_like_json {
        if content.len() <= 5 * 1024 * 1024 {
            if serde_json::from_str::<serde_json::Value>(trimmed_full.trim_end()).is_ok() {
                return "json".to_string();
            }
        } else {
            // L22: por encima de 5MB, parsear el documento completo para confirmarlo es
            // demasiado caro para una simple detección de lenguaje (podía notarse al
            // escribir/cambiar de pestaña). Si empieza por `{`/`[`, se asume JSON sin
            // validar el resto — es la heurística más simple que sigue clasificando
            // correctamente cualquier documento JSON grande en vez de caer a "markdown".
            return "json".to_string();
        }
    }
    let lower = sample.to_lowercase();
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return "html".to_string();
    }
    if sample.starts_with("<?xml") {
        return "xml".to_string();
    }
    if looks_like_sql(sample) {
        return "sql".to_string();
    }
    "markdown".to_string()
}

/// Heurística de detección de SQL que exige estructura real (hallazgo G): frases sueltas como
/// "With love", "Update: meeting" o "Create a shopping list" ya no se confunden con SQL.
pub fn looks_like_sql(sample: &str) -> bool {
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

// ---------------------------------------------------------------------------
// Normalización de rutas (hallazgo C9)
// ---------------------------------------------------------------------------

/// Canonicaliza `p` y, si el resultado sigue siendo una ruta válida sin él, retira el prefijo
/// `\\?\` ("verbatim") que `std::fs::canonicalize` añade en Windows. Ese prefijo hace que dos
/// rutas al mismo archivo (una obtenida por diálogo, otra por argv/reciente) no se comparen
/// igual y confunde a la UI (rutas larguísimas en tooltips/título). Si `p` no existe o no se
/// puede canonicalizar, se devuelve tal cual (sin fallar).
/// Ajusta `pos` (offset en bytes) al límite de carácter válido más cercano hacia atrás dentro
/// de `rope`, sin exceder su longitud en bytes (hallazgo G: "restore cursor: floor a char
/// boundary" — una posición de cursor persistida de una sesión anterior podría no coincidir
/// con un límite de carácter si el contenido cambió).
fn clamp_cursor_to_document(rope: &Rope, pos: usize) -> usize {
    let text = rope.to_string();
    let mut p = pos.min(text.len());
    while p > 0 && !text.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Retira el prefijo "verbatim" `\\?\` que `std::fs::canonicalize` añade en Windows de una
/// ruta ya canonicalizada, incluyendo el caso UNC (hallazgo L16): `\\?\UNC\server\share`
/// se convierte en `\\server\share` (la forma UNC normal, sin el prefijo verbatim), no se
/// deja como estaba antes.
fn strip_verbatim_prefix(canon: &Path) -> PathBuf {
    let s = canon.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        if let Some(unc_rest) = stripped.strip_prefix(r"UNC\").or_else(|| stripped.strip_prefix(r"unc\")) {
            return PathBuf::from(format!(r"\\{}", unc_rest));
        }
        return PathBuf::from(stripped);
    }
    canon.to_path_buf()
}

/// Normaliza `p` a una ruta absoluta, canónica y sin el prefijo "verbatim" `\\?\` de
/// Windows (hallazgo C9/L16), para que dos rutas al mismo archivo (una obtenida por
/// diálogo, otra por argv/reciente) se comparen iguales y no se vean como rutas
/// larguísimas en tooltips/título.
///
/// - Si `p` existe: se canonicaliza directamente y se retira el prefijo verbatim.
/// - Si `p` NO existe todavía (p.ej. "Guardar como…" con un nombre de archivo nuevo en
///   una carpeta existente): se canonicaliza el directorio PADRE y se le añade el nombre
///   de archivo, así la ruta queda igualmente absoluta/canónica salvo en el componente
///   final que aún no existe.
/// - Si tampoco el padre existe (o `p` no tiene padre/nombre de archivo): se cae a
///   `std::path::absolute` (resuelve `.`/`..` léxicamente contra el directorio de trabajo
///   actual, sin tocar el disco) en vez de devolver la ruta tal cual sin normalizar.
pub fn normalize_path(p: &Path) -> PathBuf {
    if let Ok(canon) = std::fs::canonicalize(p) {
        return strip_verbatim_prefix(&canon);
    }
    if let (Some(parent), Some(file_name)) = (p.parent(), p.file_name())
        && let Ok(canon_parent) = std::fs::canonicalize(parent)
    {
        return strip_verbatim_prefix(&canon_parent).join(file_name);
    }
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

// ---------------------------------------------------------------------------
// EditorState
// ---------------------------------------------------------------------------

/// Estado global del editor: grupos de pestañas, borradores, sesiones y búsqueda.
pub struct EditorState {
    pub groups: Vec<Group>,
    pub active_group: usize,
    pub scratch_manager: ScratchManager,
    pub session_manager: SessionManager,
    pub workspace_root: Option<PathBuf>,
    pub search_results: Vec<search::SearchResult>,
    pub search_index: usize,
}

impl EditorState {
    /// Crea el estado inicial con un único borrador "Sin título 1".
    pub fn new(scratch_manager: ScratchManager, session_manager: SessionManager) -> Self {
        let id = scratch_manager.next_id_public();
        let tab = Tab::new_scratch(id, Rope::new(), "Sin título 1".to_string());
        Self {
            groups: vec![Group {
                tabs: vec![tab],
                active: 0,
            }],
            active_group: 0,
            scratch_manager,
            session_manager,
            workspace_root: None,
            search_results: Vec::new(),
            search_index: 0,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.groups.get(self.active_group)?.active_tab()
    }

    /// El wiring en `app/*.rs` casi siempre necesita el grupo activo explícito junto con
    /// el índice de pestaña (para pasarlos a `close_tab`/undo/etc.), así que accede a
    /// `groups[ag].active_tab_mut()` directamente en vez de por aquí; se conserva por
    /// completitud simétrica con `active_tab()`.
    #[allow(dead_code)]
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.groups.get_mut(self.active_group)?.active_tab_mut()
    }

    /// El lenguaje detectado de una pestaña (por ruta, o por contenido si es un borrador).
    pub fn language_of(&self, tab: &Tab) -> String {
        detect_language(tab.file_path.as_deref(), &tab.document.to_string())
    }

    /// Hallazgo E9: expuesto para que el wiring de grupos (`app::view_ops::switch_group`)
    /// pueda titular la primera pestaña de un grupo nuevo con un número realmente libre,
    /// en vez de "Sin título 1" fijo (que podía duplicar el título de un borrador ya
    /// abierto en otro grupo).
    pub(crate) fn smallest_unused_scratch_number(&self) -> usize {
        let mut used = std::collections::HashSet::new();
        for g in &self.groups {
            for t in &g.tabs {
                if let Some(rest) = t.title.strip_prefix("Sin título ")
                    && let Ok(n) = rest.trim().parse::<usize>()
                {
                    used.insert(n);
                }
            }
        }
        let mut n = 1usize;
        while used.contains(&n) {
            n += 1;
        }
        n
    }

    /// Crea una nueva pestaña de borrador en el grupo activo, titulada "Sin título N" con la N
    /// más pequeña no usada entre todas las pestañas abiertas. Devuelve `(grupo, índice)`.
    pub fn new_scratch_tab(&mut self) -> (usize, usize) {
        let n = self.smallest_unused_scratch_number();
        let title = format!("Sin título {}", n);
        let id = self.scratch_manager.next_id_public();
        let tab = Tab::new_scratch(id, Rope::new(), title);
        let g = self.active_group;
        self.groups[g].tabs.push(tab);
        let idx = self.groups[g].tabs.len() - 1;
        self.groups[g].active = idx;
        (g, idx)
    }

    /// Busca una pestaña de archivo ya abierta por su ruta, en todos los grupos. `path` se
    /// normaliza (hallazgo C9) antes de comparar, igual que en `open_file`, para que una ruta
    /// obtenida por diálogo y otra por "recientes"/argv que apunten al mismo archivo se
    /// reconozcan como la misma pestaña.
    pub fn find_tab_by_path(&self, path: &Path) -> Option<(usize, usize)> {
        let norm = normalize_path(path);
        for (gi, g) in self.groups.iter().enumerate() {
            for (ti, t) in g.tabs.iter().enumerate() {
                if t.file_path.as_deref() == Some(norm.as_path()) {
                    return Some((gi, ti));
                }
            }
        }
        None
    }

    /// Abre un archivo: si ya está abierto en algún grupo, simplemente lo activa. Si no, lo lee
    /// del disco (debe ser UTF-8) y crea una nueva pestaña en el grupo activo. Quita el BOM
    /// UTF-8 si lo hay (hallazgo M4, recordándolo en `had_bom` para restaurarlo al guardar) y
    /// normaliza los finales de línea a `\n` (hallazgo E11, recordando el estilo original en
    /// `line_ending`).
    pub fn open_file(&mut self, path: PathBuf) -> Result<(usize, usize), String> {
        let canon = normalize_path(&path);
        if let Some((g, i)) = self.find_tab_by_path(&canon) {
            self.active_group = g;
            self.groups[g].active = i;
            return Ok((g, i));
        }

        let bytes = std::fs::read(&path).map_err(|e| format!("No se puede abrir: {}", e))?;
        let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
        let bytes = if had_bom { &bytes[3..] } else { &bytes[..] };
        let content = String::from_utf8(bytes.to_vec())
            .map_err(|_| "No se puede abrir (no es texto UTF-8)".to_string())?;
        let (normalized, line_ending) = crate::textops::normalize_newlines(&content);
        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        let rope = Rope::from_str(&normalized);
        let mut tab = Tab::new_file(canon, rope, mtime);
        tab.had_bom = had_bom;
        tab.line_ending = line_ending;

        let g = self.active_group;
        self.groups[g].tabs.push(tab);
        let idx = self.groups[g].tabs.len() - 1;
        self.groups[g].active = idx;
        Ok((g, idx))
    }

    /// Cierra una pestaña. Si era la última del grupo, la sustituye por un borrador nuevo (el
    /// grupo nunca queda vacío).
    pub fn close_tab(&mut self, group: usize, index: usize) {
        if group >= self.groups.len() || index >= self.groups[group].tabs.len() {
            return;
        }
        self.groups[group].tabs.remove(index);
        if self.groups[group].tabs.is_empty() {
            let n = self.smallest_unused_scratch_number();
            let id = self.scratch_manager.next_id_public();
            self.groups[group]
                .tabs
                .push(Tab::new_scratch(id, Rope::new(), format!("Sin título {}", n)));
            self.groups[group].active = 0;
        } else {
            // Hallazgo E5: cerrar una pestaña ANTERIOR a la activa desplaza todos los
            // índices posteriores una posición hacia atrás, así que `active` debe
            // decrementarse para seguir apuntando al mismo documento (antes solo se
            // acotaba con `.min(len-1)`, que en [A,B,C,D] con C activo y A cerrada
            // dejaba `active` en el mismo índice numérico, ahora ocupado por D, en vez
            // de seguir en C).
            if index < self.groups[group].active {
                self.groups[group].active -= 1;
            }
            let len = self.groups[group].tabs.len();
            self.groups[group].active = self.groups[group].active.min(len - 1);
        }
    }

    /// Serializa el estado actual a una `Session` persistible. Para archivos sucios, guarda el
    /// contenido no guardado (para "hot exit").
    pub fn to_session(&self) -> Session {
        let mut groups = Vec::new();
        for (i, g) in self.groups.iter().enumerate() {
            let tabs: Vec<TabState> = g
                .tabs
                .iter()
                .map(|t| TabState {
                    document_id: t
                        .scratch_id
                        .clone()
                        .unwrap_or_else(|| format!("tab-{}", i)),
                    path: t.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    title: t.title.clone(),
                    is_scratch: t.is_scratch(),
                    cursor: t.cursor,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: if !t.is_scratch() && t.is_dirty() {
                        Some(t.document.to_string())
                    } else {
                        None
                    },
                    mtime: t.last_known_mtime.and_then(|m| {
                        m.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
                    }),
                })
                .collect();
            groups.push(GroupState {
                id: i,
                tabs,
                active_tab: g.active,
            });
        }
        Session {
            version: 1,
            name: "Restored Session".to_string(),
            created_at: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            groups,
            active_group: self.active_group,
            group_count: self.groups.len(),
            workspace_root: self
                .workspace_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        }
    }

    /// Restaura el estado a partir de una `Session` guardada. Recupera el contenido de los
    /// borradores desde el directorio de scratch y normaliza BOM/CRLF de los archivos. Nunca
    /// pierde contenido no guardado en silencio (hallazgo C1/C2/C3): si una lectura falla o el
    /// archivo cambió en disco desde que se guardó la sesión, se conserva la pestaña con el
    /// contenido disponible y se devuelve un aviso en el `Vec<String>` resultante (para que el
    /// wiring lo muestre en la barra de estado).
    pub fn restore_from_session(&mut self, session: &Session) -> Vec<String> {
        let scratch_dir = self.scratch_manager.base_dir().to_path_buf();
        let mut groups: Vec<Group> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        for gs in &session.groups {
            let mut tabs: Vec<Tab> = Vec::new();
            for ts in &gs.tabs {
                if ts.is_scratch {
                    let scratch_path = scratch_dir.join(format!("{}.md", ts.document_id));
                    let title = if ts.title.trim().is_empty() {
                        "Sin título".to_string()
                    } else {
                        ts.title.clone()
                    };
                    let (rope, autosave_enabled) = match std::fs::read_to_string(&scratch_path) {
                        Ok(content) => (Rope::from_str(&content), true),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Rope::new(), true),
                        Err(e) => {
                            warnings.push(format!(
                                "No se pudo leer el borrador '{}': {} (no se autoguardará hasta reabrirlo)",
                                title, e
                            ));
                            (Rope::new(), false)
                        }
                    };
                    let mut tab = Tab::new_scratch(ts.document_id.clone(), rope, title);
                    tab.autosave_enabled = autosave_enabled;
                    tab.cursor = clamp_cursor_to_document(&tab.document, ts.cursor);
                    tabs.push(tab);
                } else if let Some(path_str) = &ts.path {
                    let path = PathBuf::from(path_str);
                    let disk_read = std::fs::read(&path);
                    let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

                    if let Some(saved_mtime_secs) = ts.mtime {
                        if let Some(current) = mtime {
                            let current_secs = current
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            if current_secs != saved_mtime_secs {
                                warnings.push(format!(
                                    "'{}' cambió en disco desde el último cierre; revisa el contenido antes de guardar",
                                    ts.title
                                ));
                            }
                        }
                    }

                    // Hallazgo W6: recordar si el archivo en disco tenía BOM UTF-8, para
                    // restaurarlo en `tab.had_bom` (antes se descartaba aquí y la pestaña
                    // restaurada perdía el BOM original al guardar).
                    let mut had_bom = false;
                    let disk_text: Option<String> = match disk_read {
                        Ok(bytes) => {
                            had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
                            let bytes = if had_bom { &bytes[3..] } else { &bytes[..] };
                            String::from_utf8(bytes.to_vec()).ok()
                        }
                        Err(_) => None,
                    };

                    let (rope, saved_snapshot, keep) = match (&ts.unsaved_content, &disk_text) {
                        (Some(unsaved), _) => {
                            let (norm, _) = crate::textops::normalize_newlines(unsaved);
                            // Si no hay contenido en disco con el que comparar (archivo borrado
                            // o ilegible), se considera sucio a propósito: es contenido "sin
                            // guardar" por definición, nunca debe aparentar estar limpio.
                            let saved = disk_text
                                .as_ref()
                                .map(|d| crate::textops::normalize_newlines(d).0)
                                .unwrap_or_default();
                            (Rope::from_str(&norm), Rope::from_str(&saved), true)
                        }
                        (None, Some(disk)) => {
                            let (norm, _) = crate::textops::normalize_newlines(disk);
                            (Rope::from_str(&norm), Rope::from_str(&norm), true)
                        }
                        (None, None) => {
                            if !path.exists() {
                                // Archivo eliminado y sin contenido sin guardar: nada que
                                // conservar. Se omite (comportamiento previo).
                                (Rope::new(), Rope::new(), false)
                            } else {
                                warnings.push(format!(
                                    "No se pudo leer '{}' (bloqueado o no es UTF-8); revísalo manualmente",
                                    ts.title
                                ));
                                (Rope::new(), Rope::new(), false)
                            }
                        }
                    };

                    if !keep {
                        continue;
                    }

                    let (_, line_ending) = disk_text
                        .as_deref()
                        .map(crate::textops::normalize_newlines)
                        .unwrap_or((String::new(), crate::textops::LineEnding::Lf));
                    let mut tab = Tab::new_file(path, rope, mtime);
                    tab.saved_snapshot = saved_snapshot;
                    tab.line_ending = line_ending;
                    tab.had_bom = had_bom;
                    if !ts.title.trim().is_empty() {
                        tab.title = ts.title.clone();
                    }
                    tab.cursor = clamp_cursor_to_document(&tab.document, ts.cursor);
                    tabs.push(tab);
                }
            }
            if !tabs.is_empty() {
                let active = gs.active_tab.min(tabs.len() - 1);
                groups.push(Group { tabs, active });
            }
        }

        if groups.is_empty() {
            let id = self.scratch_manager.next_id_public();
            groups.push(Group {
                tabs: vec![Tab::new_scratch(id, Rope::new(), "Sin título 1".to_string())],
                active: 0,
            });
        }

        self.active_group = session.active_group.min(groups.len() - 1);
        self.groups = groups;
        if let Some(root) = &session.workspace_root {
            self.workspace_root = Some(PathBuf::from(root));
        }
        warnings
    }

    /// Guarda todos los borradores abiertos en un único paso de E/S (una sola carga/guardado
    /// del índice). Devuelve el número de borradores guardados.
    pub fn save_all_scratches(&mut self) -> Result<usize, String> {
        let mut docs = Vec::new();
        for g in &self.groups {
            for t in &g.tabs {
                // Hallazgo W3: si la última carga/restauración del borrador falló de forma no
                // recuperable (`autosave_enabled == false`), no se debe escribir sobre el
                // archivo de scratch original en disco con el contenido vacío en memoria.
                if let Some(id) = &t.scratch_id {
                    if !t.autosave_enabled {
                        continue;
                    }
                    let mut doc = self
                        .scratch_manager
                        .create_scratch_doc_for_save(id.clone(), t.document.clone())
                        .ok_or_else(|| "No se pudo preparar el borrador".to_string())?;
                    doc.title = t.title.clone();
                    docs.push(doc);
                }
            }
        }
        let count = docs.len();
        self.scratch_manager
            .save_scratch_batch(&mut docs)
            .map_err(|e| e.to_string())?;
        for g in &mut self.groups {
            for t in &mut g.tabs {
                if t.is_scratch() && t.autosave_enabled {
                    t.mark_autosaved();
                }
            }
        }
        Ok(count)
    }

    /// Autoguardado incremental (hallazgo C5): solo reescribe los borradores cuyo contenido
    /// cambió desde el último autoguardado (`Tab::needs_autosave`), en vez de reescribir todos
    /// en cada tick del temporizador.
    ///
    /// Hallazgo C9: devuelve el número de borradores REALMENTE escritos y, si algo falló, un
    /// mensaje de error — en vez de `Result<usize, String>`, que con el `?` original abortaba
    /// ANTES de marcar como autoguardado cualquier borrador, incluidos los que
    /// `save_scratch_batch` sí había escrito con éxito en disco (escribe cada uno por
    /// separado y sigue tras un fallo individual, pero antes esta función tiraba todo el
    /// progreso igualmente): esos borradores ya guardados volvían a reescribirse en cada
    /// tick siguiente sin necesidad, indefinidamente, mientras UN solo borrador siguiera
    /// fallando.
    pub fn save_changed_scratches(&mut self) -> (usize, Option<String>) {
        let mut docs = Vec::new();
        let mut touched: Vec<(usize, usize)> = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for (ti, t) in g.tabs.iter().enumerate() {
                if let Some(id) = &t.scratch_id {
                    if t.needs_autosave() {
                        match self.scratch_manager.create_scratch_doc_for_save(id.clone(), t.document.clone()) {
                            Some(mut doc) => {
                                doc.title = t.title.clone();
                                docs.push(doc);
                                touched.push((gi, ti));
                            }
                            None => {
                                return (0, Some("No se pudo preparar el borrador para autoguardar".to_string()));
                            }
                        }
                    }
                }
            }
        }
        if docs.is_empty() {
            return (0, None);
        }
        // `save_scratch_batch` escribe cada borrador por separado y marca `doc.dirty =
        // false` (vía `mark_saved`) solo en los que tuvo éxito, aunque devuelva `Err` con
        // el primer error encontrado — así que, tras la llamada, `!doc.dirty` es la señal
        // fiable de "este SÍ se guardó", con independencia de si el lote en conjunto
        // devolvió error.
        let batch_result = self.scratch_manager.save_scratch_batch(&mut docs);
        let mut saved_count = 0usize;
        for ((gi, ti), doc) in touched.into_iter().zip(docs.iter()) {
            if !doc.dirty {
                if let Some(t) = self.groups.get_mut(gi).and_then(|g| g.tabs.get_mut(ti)) {
                    t.mark_autosaved();
                }
                saved_count += 1;
            }
        }
        (saved_count, batch_result.err().map(|e| e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l16_strip_verbatim_unc_prefix() {
        let p = Path::new(r"\\?\UNC\server\share\file.txt");
        assert_eq!(strip_verbatim_prefix(p), PathBuf::from(r"\\server\share\file.txt"));
    }

    #[test]
    fn l16_strip_verbatim_plain_prefix() {
        let p = Path::new(r"\\?\C:\Users\me\file.txt");
        assert_eq!(strip_verbatim_prefix(p), PathBuf::from(r"C:\Users\me\file.txt"));
    }

    #[test]
    fn l16_normalize_nonexistent_path_under_existing_parent() {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-normalize-test-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("does-not-exist.txt");
        assert!(!missing.exists());
        let normalized = normalize_path(&missing);
        // El resultado debe ser absoluto y terminar con el mismo nombre de archivo (el
        // directorio padre, que sí existe, queda canonicalizado).
        assert!(normalized.is_absolute());
        assert_eq!(normalized.file_name(), missing.file_name());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn l16_normalize_path_with_missing_parent_falls_back_to_absolute() {
        // Ni la ruta ni su padre existen: debe caer a `std::path::absolute` (resolución
        // léxica) en vez de devolver la ruta relativa tal cual sin normalizar.
        let p = Path::new("no/such/parent/either/file.txt");
        let normalized = normalize_path(p);
        assert!(normalized.is_absolute(), "normalized: {:?}", normalized);
    }

    fn temp_manager(name: &str) -> (ScratchManager, SessionManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-editor-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let scratch_dir = dir.join("scratch");
        let sessions_dir = dir.join("sessions");
        (
            ScratchManager::new(scratch_dir),
            SessionManager::new(sessions_dir),
            dir,
        )
    }

    // --- UndoStack ---

    #[test]
    fn undo_redo_basic_sequence() {
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        stack.record(Rope::from_str("AB"), 2, false);
        stack.record(Rope::from_str("ABC"), 3, false);

        let prev = stack.undo().unwrap();
        assert_eq!(prev.0.to_string(), "AB");
        let prev2 = stack.undo().unwrap();
        assert_eq!(prev2.0.to_string(), "A");
        assert!(stack.undo().is_none());

        let next = stack.redo().unwrap();
        assert_eq!(next.0.to_string(), "AB");
        let next2 = stack.redo().unwrap();
        assert_eq!(next2.0.to_string(), "ABC");
        assert!(stack.redo().is_none());
    }

    #[test]
    fn record_after_undo_truncates_redo() {
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        stack.record(Rope::from_str("AB"), 2, false);
        stack.record(Rope::from_str("ABC"), 3, false);
        let prev = stack.undo().unwrap();
        assert_eq!(prev.0.to_string(), "AB");

        stack.record(Rope::from_str("ABX"), 3, false);
        assert!(!stack.can_redo());
    }

    #[test]
    fn l20_coalesce_after_undo_truncates_redo() {
        // Hallazgo L20: a diferencia de `record_after_undo_truncates_redo` (que prueba
        // con coalesce=false), este cubre el caso con coalesce=true: deshacer y luego
        // teclear en ráfaga (coalesce) también debe descartar el redo "fantasma" que
        // quedaba de antes de deshacer.
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        stack.record(Rope::from_str("AB"), 2, false);
        stack.record(Rope::from_str("ABC"), 3, false);
        let prev = stack.undo().unwrap();
        assert_eq!(prev.0.to_string(), "AB");
        assert!(stack.can_redo());

        // Se sigue tecleando desde "AB" (fundido, coalesce=true): el redo hacia "ABC"
        // ya no es válido y no debe seguir siendo alcanzable.
        stack.record(Rope::from_str("ABX"), 3, true);
        assert!(!stack.can_redo(), "el redo hacia \"ABC\" no debería sobrevivir");
        assert_eq!(stack.history.len(), 2);
    }

    #[test]
    fn coalesce_replaces_top_of_stack() {
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        stack.record(Rope::from_str("AB"), 2, false);
        // Ráfaga de tecleo: cada pulsación funde con la anterior, no crea pasos nuevos.
        stack.record(Rope::from_str("ABC"), 3, true);
        stack.record(Rope::from_str("ABCD"), 4, true);
        assert_eq!(stack.history.len(), 2);
        let prev = stack.undo().unwrap();
        assert_eq!(prev.0.to_string(), "A");
    }

    #[test]
    fn coalesce_at_base_state_does_not_overwrite_it() {
        // El estado base (position == 0, el documento tal como se abrió) nunca se sobrescribe
        // aunque el primer registro llegue con coalesce=true.
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        stack.record(Rope::from_str("AB"), 2, true);
        assert!(stack.can_undo());
        let prev = stack.undo().unwrap();
        assert_eq!(prev.0.to_string(), "A");
    }

    #[test]
    fn record_caps_history_at_500() {
        let mut stack = UndoStack::new(Rope::from_str(""), 0);
        for i in 0..600 {
            stack.record(Rope::from_str(&i.to_string()), 0, false);
        }
        assert_eq!(stack.history.len(), 500);
    }

    #[test]
    fn can_undo_redo_flags() {
        let mut stack = UndoStack::new(Rope::from_str("A"), 0);
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
        stack.record(Rope::from_str("AB"), 2, false);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    // --- Flujo realista: Tab::record_typing / record_command / apply_undo / apply_redo ---

    #[test]
    fn realistic_flow_typing_burst_then_command_then_undo() {
        let mut tab = Tab::new_scratch("s1".to_string(), Rope::from_str(""), "t".to_string());
        // Primera edición tras cargar: debe ser deshacible (hallazgo A: "primera edición no
        // deshacible" era el bug original).
        tab.document = Rope::from_str("a");
        tab.record_typing(1);
        assert!(tab.undo.can_undo());

        // Ráfaga de tecleo (funde en un solo paso).
        tab.document = Rope::from_str("ab");
        tab.record_typing(2);
        tab.document = Rope::from_str("abc");
        tab.record_typing(3);

        // Comando discreto (p.ej. duplicar línea): paso independiente.
        let before_command = tab.document.clone();
        tab.document = Rope::from_str("abc\nabc");
        tab.record_command(7);

        // Un solo undo debe deshacer SOLO el comando, no la ráfaga de tecleo completa.
        assert!(tab.apply_undo());
        assert_eq!(tab.document, before_command);
        assert_eq!(tab.document.to_string(), "abc");

        // Otro undo deshace la ráfaga completa (fue un solo paso fundido, ya que cada tecleo
        // se fundió con el anterior) y vuelve al estado base (documento vacío al crear la
        // pestaña).
        assert!(tab.apply_undo());
        assert_eq!(tab.document.to_string(), "");
        assert!(!tab.undo.can_undo());

        // Redo.
        assert!(tab.apply_redo());
        assert_eq!(tab.document.to_string(), "abc");
        assert!(tab.apply_redo());
        assert_eq!(tab.document.to_string(), "abc\nabc");
        assert!(!tab.undo.can_redo());
    }

    #[test]
    fn typing_after_undo_drops_redo() {
        let mut tab = Tab::new_scratch("s1".to_string(), Rope::from_str(""), "t".to_string());
        tab.document = Rope::from_str("a");
        tab.record_typing(1);
        tab.document = Rope::from_str("ab");
        tab.record_command(2);
        assert!(tab.apply_undo());
        assert_eq!(tab.document.to_string(), "a");

        // Teclear tras un undo debe truncar el redo (comportamiento estándar de editor).
        tab.document = Rope::from_str("ax");
        tab.record_typing(2);
        assert!(!tab.undo.can_redo());
    }

    // --- Tab ---

    #[test]
    fn scratch_tab_never_dirty() {
        let mut tab = Tab::new_scratch("s1".to_string(), Rope::from_str("hi"), "Sin título 1".to_string());
        tab.document = Rope::from_str("hi changed");
        assert!(!tab.is_dirty());
        assert_eq!(tab.tooltip(), "Borrador (se guarda automáticamente)");
    }

    #[test]
    fn file_tab_dirty_tracking() {
        let mut tab = Tab::new_file(PathBuf::from("a.txt"), Rope::from_str("hi"), None);
        assert!(!tab.is_dirty());
        tab.document = Rope::from_str("hi!");
        assert!(tab.is_dirty());
        assert_eq!(tab.display_title(), "a.txt ●");
        tab.mark_saved();
        assert!(!tab.is_dirty());
        assert_eq!(tab.display_title(), "a.txt");
    }

    // --- detect_language ---

    #[test]
    fn detect_language_by_extension() {
        assert_eq!(detect_language(Some(Path::new("a.rs")), ""), "rust");
        assert_eq!(detect_language(Some(Path::new("a.md")), ""), "markdown");
        assert_eq!(detect_language(Some(Path::new("a.yml")), ""), "yaml");
        assert_eq!(detect_language(Some(Path::new("a.unknownext")), ""), "plaintext");
    }

    #[test]
    fn detect_language_content_heuristic_json() {
        assert_eq!(detect_language(None, r#"{"a": 1}"#), "json");
    }

    #[test]
    fn l22_detect_language_large_json_heuristic() {
        // Hallazgo L22: por encima de 5MB no se valida el JSON completo (demasiado
        // caro); si empieza por '{'/'[' se asume "json" en vez de caer a "markdown".
        let mut big = String::from("[");
        while big.len() <= 5 * 1024 * 1024 {
            big.push_str(r#"{"x":1},"#);
        }
        big.push(']');
        assert!(big.len() > 5 * 1024 * 1024);
        assert_eq!(detect_language(None, &big), "json");
    }

    #[test]
    fn detect_language_content_heuristic_html() {
        assert_eq!(detect_language(None, "<!DOCTYPE html><html></html>"), "html");
    }

    #[test]
    fn detect_language_content_heuristic_sql() {
        assert_eq!(detect_language(None, "SELECT * FROM t"), "sql");
    }

    #[test]
    fn detect_language_content_heuristic_defaults_markdown() {
        assert_eq!(detect_language(None, "# hello\nworld"), "markdown");
        assert_eq!(detect_language(None, ""), "markdown");
    }

    // --- EditorState ---

    #[test]
    fn new_scratch_tab_uses_smallest_unused_number() {
        let (sm, sesm, dir) = temp_manager("scratch-num");
        let mut state = EditorState::new(sm, sesm);
        // Ya existe "Sin título 1"
        let (g, _) = state.new_scratch_tab();
        assert_eq!(state.groups[g].tabs[state.groups[g].active].title, "Sin título 2");
        state.new_scratch_tab();
        assert_eq!(
            state.groups[g].tabs[state.groups[g].active].title,
            "Sin título 3"
        );
        state.close_tab(g, 1); // cierra "Sin título 2"
        let (g2, _) = state.new_scratch_tab();
        assert_eq!(
            state.groups[g2].tabs[state.groups[g2].active].title,
            "Sin título 2"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn close_tab_keeps_at_least_one_tab() {
        let (sm, sesm, dir) = temp_manager("close-last");
        let mut state = EditorState::new(sm, sesm);
        assert_eq!(state.groups[0].tabs.len(), 1);
        state.close_tab(0, 0);
        assert_eq!(state.groups[0].tabs.len(), 1); // reemplazada por un borrador nuevo
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_file_dedupes_already_open() {
        let (sm, sesm, dir) = temp_manager("open-dedupe");
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "contenido").unwrap();
        let mut state = EditorState::new(sm, sesm);
        let (g1, i1) = state.open_file(file_path.clone()).unwrap();
        let count_after_first = state.groups[g1].tabs.len();
        let (g2, i2) = state.open_file(file_path.clone()).unwrap();
        assert_eq!((g1, i1), (g2, i2));
        assert_eq!(state.groups[g2].tabs.len(), count_after_first);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_file_rejects_non_utf8() {
        let (sm, sesm, dir) = temp_manager("open-non-utf8");
        let file_path = dir.join("bin.dat");
        std::fs::write(&file_path, [0xFF, 0xFE, 0x00, 0xD8]).unwrap();
        let mut state = EditorState::new(sm, sesm);
        let err = state.open_file(file_path).unwrap_err();
        assert_eq!(err, "No se puede abrir (no es texto UTF-8)");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn to_session_and_restore_roundtrip_scratch() {
        let (sm, sesm, dir) = temp_manager("session-roundtrip");
        let mut state = EditorState::new(sm, sesm);
        state.active_tab_mut().unwrap().document = Rope::from_str("# Hola");
        state.save_all_scratches().unwrap();

        let session = state.to_session();
        let (sm2, sesm2, _) = (
            ScratchManager::new(state.scratch_manager.base_dir().to_path_buf()),
            SessionManager::new(state.session_manager.sessions_dir().to_path_buf()),
            (),
        );
        let mut state2 = EditorState::new(sm2, sesm2);
        state2.restore_from_session(&session);
        assert_eq!(
            state2.active_tab().unwrap().document.to_string(),
            "# Hola"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_from_session_skips_missing_files() {
        let (sm, sesm, dir) = temp_manager("session-missing-file");
        let mut state = EditorState::new(sm, sesm);
        let session = Session {
            version: 1,
            name: "t".to_string(),
            created_at: 0,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "tab-0".to_string(),
                    path: Some(dir.join("no-existe.txt").to_string_lossy().to_string()),
                    title: "no-existe.txt".to_string(),
                    is_scratch: false,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: None,
                    mtime: None,
                }],
                active_tab: 0,
            }],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        };
        state.restore_from_session(&session);
        // No debe haber crasheado; y como no había pestañas válidas, cae al borrador por defecto.
        assert_eq!(state.groups.len(), 1);
        assert!(state.active_tab().unwrap().is_scratch());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_from_session_clamps_active_group() {
        let (sm, sesm, dir) = temp_manager("session-clamp");
        let mut state = EditorState::new(sm, sesm);
        let session = Session {
            version: 1,
            name: "t".to_string(),
            created_at: 0,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "s1".to_string(),
                    path: None,
                    title: "Sin título 1".to_string(),
                    is_scratch: true,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: None,
                    mtime: None,
                }],
                active_tab: 99, // fuera de rango a propósito
            }],
            active_group: 99, // fuera de rango a propósito
            group_count: 1,
            workspace_root: None,
        };
        state.restore_from_session(&session);
        assert_eq!(state.active_group, 0);
        assert_eq!(state.groups[0].active, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_all_scratches_writes_files_once() {
        let (sm, sesm, dir) = temp_manager("save-all");
        let mut state = EditorState::new(sm, sesm);
        state.new_scratch_tab();
        state.new_scratch_tab();
        let count = state.save_all_scratches().unwrap();
        assert_eq!(count, 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_changed_scratches_only_writes_dirty_ones() {
        let (sm, sesm, dir) = temp_manager("save-changed");
        let mut state = EditorState::new(sm, sesm);
        state.new_scratch_tab(); // "Sin título 2", activo
        // Nada cambió todavía desde la creación (autosaved_snapshot == document).
        assert_eq!(state.save_changed_scratches().0, 0);

        state.groups[0].tabs[1].document = Rope::from_str("cambiado");
        assert_eq!(state.save_changed_scratches().0, 1);
        // Tras guardar, ya no hay nada pendiente.
        assert_eq!(state.save_changed_scratches().0, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn c9_save_changed_scratches_reports_count_and_no_error_on_success() {
        // Hallazgo C9: `save_changed_scratches` ya no es `Result<usize, String>` (cuyo
        // `?` interno abortaba ANTES de marcar como autoguardado cualquier borrador que
        // `save_scratch_batch` sí hubiera escrito con éxito, con independencia de si el
        // lote en conjunto acababa en error) — ahora devuelve siempre `(nº guardados,
        // error opcional)`, así que un guardado exitoso reporta el conteo Y `None`.
        let (sm, sesm, dir) = temp_manager("save-changed-partial-fail");
        let mut state = EditorState::new(sm, sesm);
        state.new_scratch_tab();
        state.groups[0].tabs[1].document = Rope::from_str("cambiado");

        let (count, err) = state.save_changed_scratches();
        assert_eq!(count, 1);
        assert!(err.is_none());
        // Ya no queda pendiente: quedó marcado como autoguardado.
        assert_eq!(state.save_changed_scratches().0, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_from_session_preserves_unsaved_content_when_file_missing() {
        let (sm, sesm, dir) = temp_manager("session-missing-with-unsaved");
        let mut state = EditorState::new(sm, sesm);
        let session = Session {
            version: 1,
            name: "t".to_string(),
            created_at: 0,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "tab-0".to_string(),
                    path: Some(dir.join("no-existe.txt").to_string_lossy().to_string()),
                    title: "no-existe.txt".to_string(),
                    is_scratch: false,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: Some("contenido sin guardar".to_string()),
                    mtime: None,
                }],
                active_tab: 0,
            }],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        };
        state.restore_from_session(&session);
        assert_eq!(state.active_tab().unwrap().document.to_string(), "contenido sin guardar");
        assert!(state.active_tab().unwrap().is_dirty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_from_session_warns_when_disk_changed_since_save() {
        let (sm, sesm, dir) = temp_manager("session-mtime-changed");
        let file_path = dir.join("a.txt");
        std::fs::write(&file_path, "en disco").unwrap();
        let mut state = EditorState::new(sm, sesm);
        let session = Session {
            version: 1,
            name: "t".to_string(),
            created_at: 0,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "tab-0".to_string(),
                    path: Some(file_path.to_string_lossy().to_string()),
                    title: "a.txt".to_string(),
                    is_scratch: false,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: None,
                    mtime: Some(0), // Deliberadamente distinto al mtime real del archivo.
                }],
                active_tab: 0,
            }],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        };
        let warnings = state.restore_from_session(&session);
        assert!(warnings.iter().any(|w| w.contains("cambió en disco")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_file_normalizes_path_for_dedupe() {
        let (sm, sesm, dir) = temp_manager("normalize-path-dedupe");
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "hola").unwrap();
        let mut state = EditorState::new(sm, sesm);
        let (g1, i1) = state.open_file(file_path.clone()).unwrap();
        // Reabrir con una ruta con componentes ./ redundantes debe seguir deduplicando.
        let weird = dir.join(".").join("test.txt");
        let (g2, i2) = state.open_file(weird).unwrap();
        assert_eq!((g1, i1), (g2, i2));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_language_sql_heuristic_rejects_prose_false_positives() {
        assert_eq!(detect_language(None, "With love, always."), "markdown");
        assert_eq!(detect_language(None, "Update: meeting moved to 3pm"), "markdown");
        assert_eq!(detect_language(None, "Create a shopping list:\n- milk"), "markdown");
    }

    #[test]
    fn detect_language_sql_heuristic_accepts_real_sql() {
        assert_eq!(detect_language(None, "SELECT * FROM users WHERE id = 1"), "sql");
        assert_eq!(detect_language(None, "insert into t (a) values (1)"), "sql");
        assert_eq!(detect_language(None, "CREATE TABLE t (id INT)"), "sql");
        assert_eq!(detect_language(None, "with x as (select 1) select * from x"), "sql");
    }

    #[test]
    fn restore_from_session_preserves_bom_for_file_tabs() {
        // Hallazgo W6: `had_bom` no se fijaba al restaurar una pestaña de archivo desde
        // la sesión, así que el BOM se perdía al volver a guardar tras restaurar.
        let (sm, sesm, dir) = temp_manager("restore-bom");
        let file_path = dir.join("bom.txt");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hola".as_bytes());
        std::fs::write(&file_path, &bytes).unwrap();
        let mut state = EditorState::new(sm, sesm);
        let session = Session {
            version: 1,
            name: "t".to_string(),
            created_at: 0,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "tab-0".to_string(),
                    path: Some(file_path.to_string_lossy().to_string()),
                    title: "bom.txt".to_string(),
                    is_scratch: false,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: None,
                    mtime: None,
                }],
                active_tab: 0,
            }],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        };
        state.restore_from_session(&session);
        assert!(state.active_tab().unwrap().had_bom);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_file_strips_bom_and_normalizes_crlf() {
        let (sm, sesm, dir) = temp_manager("open-bom-crlf");
        let file_path = dir.join("bom.txt");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("A\r\nB".as_bytes());
        std::fs::write(&file_path, bytes).unwrap();
        let mut state = EditorState::new(sm, sesm);
        let (g, i) = state.open_file(file_path).unwrap();
        let tab = &state.groups[g].tabs[i];
        assert_eq!(tab.document.to_string(), "A\nB");
        assert!(tab.had_bom);
        assert_eq!(tab.line_ending, crate::textops::LineEnding::Crlf);
        std::fs::remove_dir_all(&dir).ok();
    }
}
