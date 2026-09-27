//! Cableado de la UI: estado compartido (`AppState`) y las funciones `refresh_*` que
//! empujan el estado derivado del backend (`editor::EditorState`) a las propiedades de
//! la UI generada (`App`). Cada submódulo cablea un área de callbacks (archivo, edición,
//! búsqueda, vista, explorador, configuración) sobre el mismo `SharedState`.
//!
//! Filosofía (ver PLAN.md hallazgo #12): en vez de que cada callback actualice a mano
//! las propiedades que le "parecen relevantes" (fuente de los bugs de desincronización
//! del main.rs original), todo callback termina llamando a una de las `refresh_*` de
//! aquí, que son la única fuente de verdad de cómo se deriva el estado de la UI.

pub mod ai_ops;
pub mod edit_ops;
pub mod editor_view_ops;
pub mod explorer_ops;
pub mod file_ops;
pub mod naming_ops;
pub mod search_ops;
pub mod settings_ops;
pub mod view_ops;
pub mod windows;

use crate::editor::EditorState;
use crate::settings::Settings;
use crate::{App, TabInfo};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Estado compartido entre todos los cierres de callbacks de la UI.
pub struct AppState {
    pub editor: EditorState,
    pub settings: Settings,
    /// Último desplazamiento de "ancla" de selección conocido en la pestaña activa
    /// (el `cursor` en sí se guarda en `Tab::cursor`; ver `editor.rs`).
    pub anchor: usize,
    /// `true` cuando el documento activo cambió desde el último recálculo de
    /// estadísticas/vista previa (el timer de 150 ms lo consume y lo limpia).
    pub content_dirty: bool,
    pub last_line_count: usize,
    pub last_scratch_autosave: Instant,
    pub last_ext_check: Instant,
    /// Modo de vista ACTUAL (0/1/2), separado de `settings.view_mode` (que es solo la
    /// preferencia por defecto al abrir, editable en Configuración — hallazgo E4). Antes
    /// ambos conceptos compartían el mismo campo: cualquier `settings-changed()` (p.ej.
    /// alternar la barra de herramientas desde el menú Ver) releía
    /// `ui.get_default_view_mode()` y lo volvía a escribir en `settings.view_mode`,
    /// pisando silenciosamente el modo de vista en vivo y devolviendo al usuario a "solo
    /// editor" en mitad de la sesión. Se inicializa con la preferencia guardada y a
    /// partir de ahí vive solo en memoria; Ctrl+M/los botones de modo lo cambian sin
    /// tocar `settings.view_mode` ni el disco.
    pub current_view_mode: i32,
    /// `true` cuando la consulta de búsqueda cambió desde el último recómputo (hallazgo
    /// W5): en vez de recalcular `search_in_rope` de forma síncrona en CADA tecla del
    /// campo de búsqueda (perceptible con documentos grandes), el tecleo solo marca esta
    /// bandera y el timer de 150 ms (`on_tick`) es quien realmente recomputa, al mismo
    /// ritmo que las estadísticas/vista previa.
    pub search_dirty: bool,
    /// `true` cuando el DOCUMENTO cambió (con el panel de búsqueda abierto) desde el
    /// último recómputo (hallazgo C13): antes, cada tecla en el DOCUMENTO (no en el
    /// campo de búsqueda) recalculaba `search_in_rope` de forma síncrona vía
    /// `refresh_after_text_change` — el mismo problema de rendimiento que `search_dirty`
    /// ya resuelve para el campo de búsqueda (hallazgo W5), pero sin cubrir la edición
    /// del documento. Lo consume `search_tick` en el timer de 150 ms, SIN mover la
    /// selección (a diferencia de `search_dirty`, que sí selecciona la coincidencia más
    /// cercana): el cursor/selección recién tecleados por el usuario no deben perderse.
    pub search_stale: bool,
    /// `true` cuando el estado de la sesión (pestañas/grupos/cursor/contenido sin
    /// guardar) cambió desde el último "hot exit" de la sesión (hallazgo W9): sustituye
    /// a la huella `(ruta, longitud)` de archivos sucios que usaba `autosave_tick`, que
    /// no detectaba una edición que no cambiara la longitud del documento (p.ej.
    /// sustituir un carácter por otro, deshacer, o cualquier cambio puramente
    /// estructural como abrir/cerrar una pestaña sin pestañas de archivo sucias).
    pub session_dirty: bool,
    /// Último mensaje de error de autoguardado ya mostrado en la barra de estado
    /// (hallazgo C9): evita repetir/parpadear el MISMO error en cada tick de
    /// autoguardado mientras el problema persiste (p.ej. un archivo bloqueado); solo se
    /// vuelve a mostrar si el mensaje cambia (otro archivo, u otro tipo de error).
    pub last_autosave_error_shown: Option<String>,
    /// Rutas de archivo cuyo autoguardado falló, con el instante a partir del cual se
    /// puede reintentar (hallazgo C9): sin esto, un archivo bloqueado por otro proceso
    /// reintentaba escribirse (creando su `.bak` de respaldo) en CADA tick de
    /// autoguardado mientras siguiera bloqueado, en vez de esperar un poco antes de
    /// volver a intentarlo.
    pub autosave_retry_after: std::collections::HashMap<std::path::PathBuf, Instant>,
    /// Ruta del archivo sobre la que ya se avisó "cambió en disco; tienes cambios sin
    /// guardar" (hallazgo C20): `external_modification_tick` corre cada 2s y, sin esto,
    /// repetía el MISMO aviso en la barra de estado cada 2s mientras el conflicto
    /// siguiera sin resolverse — tapando cualquier otro mensaje más reciente (p.ej.
    /// "Reemplazado", "Documento formateado") casi de inmediato. Se limpia en cuanto el
    /// conflicto deja de aplicarse (se guarda, se recarga, o cambia de pestaña activa).
    pub external_conflict_warned_for: Option<std::path::PathBuf>,
    /// Estado de la vista del editor (resaltado + gutter); lo rellena WS-A.
    #[allow(dead_code)] // wave-1 fills (WS-A)
    pub view: editor_view_ops::EditorViewState,
    /// Id monótono de la última petición de IA DE ESTA VENTANA; las respuestas con id viejo
    /// se descartan (WS-D). Bug de la revisión post multi-ventana: antes esto era un
    /// `AtomicU64` GLOBAL en `ai_ops.rs`, así que una solicitud de la ventana B invalidaba el
    /// id que la ventana A estaba esperando, dejando `ai_busy` de A atascado en `true` para
    /// siempre. Ahora vive por ventana, en `AppState`, y `ai_ops::spawn` lo usa para decidir
    /// si una respuesta que llega sigue siendo la más reciente de ESTA ventana.
    pub ai_request_id: u64,
    /// Pestaña a la que se aplicará el candidato elegido en `NamePickerDialog` (grupo, índice
    /// en el momento de pedir las sugerencias, identidad estable) — bug de la revisión: antes
    /// era un `thread_local` GLOBAL (`NAME_TARGET` en `ai_ops.rs`) compartido por todas las
    /// ventanas, así que una solicitud manual de nombre en la ventana B pisaba el destino
    /// pendiente de la ventana A, descartando el resultado del diálogo que A esperaba. Ahora
    /// vive por ventana.
    pub ai_name_target: Option<(usize, usize, ai_ops::TabId)>,
    /// Timer de 400 ms que mantiene `can_ai` al día en esta ventana (ver `ai_ops::wire`) — bug
    /// de la revisión: antes era un `thread_local` GLOBAL (`CAN_AI_TIMER`), así que cablear una
    /// segunda ventana sobrescribía el slot y soltaba (parando, `slint::Timer` se detiene al
    /// dropearse) el timer de la PRIMERA ventana, que se quedaba con `can_ai`/el aviso de
    /// consentimiento congelados en lo que fuera que tuvieran en ese instante — incluyendo
    /// seguir pareciendo "activada" tras desactivar la IA en otra ventana. Ahora cada ventana
    /// guarda su propio `Timer` aquí, con vida igual a la de su `AppState`.
    pub ai_can_ai_timer: Option<slint::Timer>,
}

pub type SharedState = Rc<RefCell<AppState>>;

impl AppState {
    pub fn new(editor: EditorState, settings: Settings) -> Self {
        let current_view_mode = settings.view_mode.clamp(0, 2) as i32;
        Self {
            editor,
            settings,
            anchor: 0,
            content_dirty: true,
            last_line_count: 0,
            last_scratch_autosave: Instant::now(),
            last_ext_check: Instant::now(),
            current_view_mode,
            search_dirty: false,
            search_stale: false,
            session_dirty: true,
            last_autosave_error_shown: None,
            autosave_retry_after: std::collections::HashMap::new(),
            external_conflict_warned_for: None,
            view: editor_view_ops::EditorViewState::default(),
            ai_request_id: 0,
            ai_name_target: None,
            ai_can_ai_timer: None,
        }
    }
}

/// Cablea todos los callbacks de la UI sobre `state`. Llamar una sola vez tras crear `App`.
pub fn wire_all(ui: &App, state: &SharedState) {
    file_ops::wire(ui, state);
    edit_ops::wire(ui, state);
    search_ops::wire(ui, state);
    view_ops::wire(ui, state);
    explorer_ops::wire(ui, state);
    settings_ops::wire(ui, state);
    editor_view_ops::wire(ui, state);
    ai_ops::wire(ui, state);
}

// ---------------------------------------------------------------------------
// Refresh: la única fuente de verdad de cómo se deriva la UI del backend
// ---------------------------------------------------------------------------

/// Refresca la lista/estado de pestañas, grupo activo y título de ventana. Barato
/// (O(número de pestañas del grupo activo)): se puede llamar en cada pulsación.
pub fn refresh_tabs(ui: &App, state: &SharedState) {
    let st = state.borrow();
    let ed = &st.editor;
    let Some(group) = ed.groups.get(ed.active_group) else {
        return;
    };
    let tabs: Vec<TabInfo> = group
        .tabs
        .iter()
        .map(|t| TabInfo {
            title: t.title.clone().into(),
            dirty: t.is_dirty(),
            tooltip: t.tooltip().into(),
            untitled: t.is_scratch(),
        })
        .collect();
    ui.set_tabs(ModelRc::new(VecModel::from(tabs)));
    ui.set_active_tab(group.active as i32);
    ui.set_active_group(ed.active_group as i32);
    let used = ed.groups.iter().filter(|g| !g.tabs.is_empty()).count().max(1);
    ui.set_group_count_used(used as i32);

    let title = match group.active_tab() {
        Some(t) if t.is_dirty() => format!("{} ● — LightMark", t.title),
        Some(t) => format!("{} — LightMark", t.title),
        None => "LightMark".to_string(),
    };
    ui.set_window_title(title.into());
}

/// Hallazgo F (rendimiento): actualiza solo el punto "●" (sucio) de la fila de la
/// pestaña activa en el `VecModel` YA existente (`set_row_data`), en vez de reconstruir
/// la lista entera con `refresh_tabs` (que instancia un `VecModel` nuevo — y por tanto
/// recrea el `TabBar` — en cada pulsación). Se usa desde `on_text_edited`, que se llama
/// por cada tecla.
pub fn refresh_active_tab_dirty(ui: &App, state: &SharedState) {
    let st = state.borrow();
    let ed = &st.editor;
    let Some(group) = ed.groups.get(ed.active_group) else {
        return;
    };
    let idx = group.active;
    let Some(tab) = group.tabs.get(idx) else {
        return;
    };
    let dirty = tab.is_dirty();
    // Hallazgo W19: el título de la ventana ("nombre ● — LightMark") solo se
    // recalculaba en `refresh_tabs` (eventos poco frecuentes: abrir/cerrar/guardar...),
    // así que el punto "●" no aparecía al teclear la primera edición de un archivo
    // recién abierto hasta el siguiente evento de ese tipo. Se recalcula aquí también,
    // tan barato como el resto de esta función (sin reconstruir el `VecModel`).
    let title = if dirty {
        format!("{} ● — LightMark", tab.title)
    } else {
        format!("{} — LightMark", tab.title)
    };
    drop(st);
    ui.set_window_title(title.into());
    let model = ui.get_tabs();
    if let Some(vec_model) = model.as_any().downcast_ref::<VecModel<TabInfo>>()
        && let Some(mut row) = vec_model.row_data(idx)
        && row.dirty != dirty
    {
        row.dirty = dirty;
        vec_model.set_row_data(idx, row);
    }
}

/// Hallazgo F (rendimiento): equivalente ligero de `refresh_flags` que NO recalcula el
/// lenguaje detectado (`language_of` hace `to_string()` + heurísticas sobre todo el
/// documento) ni el modo de vista efectivo — solo las tres banderas que dependen
/// directamente de cada tecleo (deshacer/rehacer/guardar). El lenguaje/formato/fin de
/// línea se siguen recalculando en `refresh_flags`, ahora solo desde el timer de 150 ms
/// (cuando `content_dirty`) o tras eventos poco frecuentes (abrir/guardar/cambiar
/// pestaña), nunca por tecla.
pub fn refresh_undo_save_flags(ui: &App, state: &SharedState) {
    let st = state.borrow();
    if let Some(t) = st.editor.active_tab() {
        ui.set_can_undo(t.undo.can_undo());
        ui.set_can_redo(t.undo.can_redo());
        ui.set_can_save(t.is_dirty() || t.file_path.is_none());
    }
    ui.set_can_save_all(any_tab_dirty(&st.editor));
}

/// V15: "Guardar todo" solo debe habilitarse si hay al menos una pestaña (en cualquier
/// grupo) con cambios sin guardar o sin ruta de archivo asignada todavía.
fn any_tab_dirty(editor: &crate::editor::EditorState) -> bool {
    editor
        .groups
        .iter()
        .any(|g| g.tabs.iter().any(|t| t.is_dirty() || t.file_path.is_none()))
}

/// Refresca banderas de habilitación, lenguaje detectado, fin de línea y disponibilidad
/// de vista previa; fuerza `view_mode` a 0 (solo editor) cuando el lenguaje activo no
/// soporta vista previa, sin descartar la preferencia guardada en `settings.view_mode`.
pub fn refresh_flags(ui: &App, state: &SharedState) {
    let (can_undo, can_redo, can_save, lang, line_ending, desired_view) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => {
                let lang = st.editor.language_of(t);
                (
                    t.undo.can_undo(),
                    t.undo.can_redo(),
                    t.is_dirty() || t.file_path.is_none(),
                    lang,
                    // Hallazgo (rendimiento + corrección): `detect_line_ending` sobre
                    // `t.document.to_string()` era a la vez caro (to_string del
                    // documento entero) y SIEMPRE devolvía "LF": el documento en
                    // memoria está normalizado a LF (hallazgo E11), así que nunca
                    // contiene "\r\n" sin importar el fin de línea real del archivo.
                    // `t.line_ending` es el que recuerda el formato original tal como
                    // se cargó (y el que se restaura al guardar).
                    t.line_ending.as_str().to_string(),
                    st.current_view_mode,
                )
            }
            None => (false, false, false, "plaintext".to_string(), "LF".to_string(), 0),
        }
    };
    let available = crate::preview::is_preview_available(&lang);
    let can_format = matches!(lang.as_str(), "json" | "xml" | "html" | "sql");

    ui.set_can_undo(can_undo);
    ui.set_can_redo(can_redo);
    ui.set_can_save(can_save);
    ui.set_can_save_all(any_tab_dirty(&state.borrow().editor));
    ui.set_can_format(can_format);
    ui.set_comment_marker(crate::textops::comment_marker_label(&lang).into());
    ui.set_language_label(lang.into());
    ui.set_line_ending(line_ending.into());
    ui.set_preview_available(available);

    let effective = if available { desired_view } else { 0 };
    if ui.get_view_mode() != effective {
        ui.set_view_mode(effective);
    }
}

/// Versión combinada de `refresh_flags` + `refresh_stats` + `refresh_preview_if_visible`
/// para el timer de 150 ms (hallazgo W13): obtiene el texto y el lenguaje del documento
/// activo UNA sola vez. Antes, cada una de esas tres funciones hacía su propio
/// `document.to_string()` (`refresh_flags` además llamaba a `language_of`, que vuelve a
/// hacer `to_string()` internamente) — hasta 3-4 copias completas del documento y 2
/// detecciones de lenguaje por tick mientras se escribe un documento grande. Solo se usa
/// desde `main.rs::on_tick`; el resto de llamadores (eventos poco frecuentes: abrir,
/// guardar, cambiar de pestaña...) siguen usando las funciones por separado, más simples
/// de razonar y sin impacto de rendimiento apreciable a esa frecuencia.
pub fn refresh_on_tick(ui: &App, state: &SharedState) {
    let (text, lang, can_undo, can_redo, can_save, line_ending) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => {
                let text = t.document.to_string();
                let lang = crate::editor::detect_language(t.file_path.as_deref(), &text);
                (
                    text,
                    lang,
                    t.undo.can_undo(),
                    t.undo.can_redo(),
                    t.is_dirty() || t.file_path.is_none(),
                    t.line_ending.as_str().to_string(),
                )
            }
            None => (String::new(), "plaintext".to_string(), false, false, false, "LF".to_string()),
        }
    };
    let available = crate::preview::is_preview_available(&lang);
    let can_format = matches!(lang.as_str(), "json" | "xml" | "html" | "sql");

    ui.set_can_undo(can_undo);
    ui.set_can_redo(can_redo);
    ui.set_can_save(can_save);
    ui.set_can_save_all(any_tab_dirty(&state.borrow().editor));
    ui.set_can_format(can_format);
    ui.set_comment_marker(crate::textops::comment_marker_label(&lang).into());
    ui.set_language_label(lang.clone().into());
    ui.set_line_ending(line_ending.into());
    ui.set_preview_available(available);

    let desired_view = state.borrow().current_view_mode;
    let effective = if available { desired_view } else { 0 };
    if ui.get_view_mode() != effective {
        ui.set_view_mode(effective);
    }

    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let lines = crate::textops::line_count(&text);
    {
        let mut st = state.borrow_mut();
        if lines != st.last_line_count {
            ui.set_line_numbers_text(generate_line_numbers(lines).into());
            ui.set_goto_line_max(lines.max(1) as i32);
            st.last_line_count = lines;
        }
    }
    ui.set_doc_stats(doc_stats_text(words, chars).into());

    if ui.get_view_mode() != 0 {
        let theme = state.borrow().settings.theme;
        crate::preview::sync_preview(ui, &text, &lang, theme);
    }
}

/// V26: "1 palabras" / "1 caracteres" se veía mal en singular; construye el texto de
/// estadísticas del documento respetando el número (singular cuando es exactamente 1).
fn doc_stats_text(words: usize, chars: usize) -> String {
    let words_label = if words == 1 { "palabra" } else { "palabras" };
    let chars_label = if chars == 1 { "carácter" } else { "caracteres" };
    format!("{} {} · {} {}", words, words_label, chars, chars_label)
}

fn generate_line_numbers(line_count: usize) -> String {
    if line_count == 0 {
        "1".to_string()
    } else {
        (1..=line_count).map(|n| n.to_string()).collect::<Vec<_>>().join("\n")
    }
}

/// Recalcula palabras/caracteres y (si `force` o cambió el nº de líneas) el gutter de
/// números de línea. Es la única operación potencialmente O(n) por pulsación; el timer
/// de 150 ms la limita a como mucho ~6-7 veces por segundo mientras se escribe.
pub fn refresh_stats(ui: &App, state: &SharedState, force: bool) {
    let mut st = state.borrow_mut();
    let text = match st.editor.active_tab() {
        Some(t) => t.document.to_string(),
        None => String::new(),
    };
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let lines = crate::textops::line_count(&text);
    if force || lines != st.last_line_count {
        ui.set_line_numbers_text(generate_line_numbers(lines).into());
        // Hallazgo E6: `goto_line_max` nunca se asignaba, así que el diálogo "Ir a
        // línea" siempre mostraba "(1 - 1)" sin importar el tamaño real del documento.
        ui.set_goto_line_max(lines.max(1) as i32);
        st.last_line_count = lines;
    }
    ui.set_doc_stats(doc_stats_text(words, chars).into());
}

/// Reconstruye la vista previa solo si está realmente visible (`view_mode != 0`).
pub fn refresh_preview_if_visible(ui: &App, state: &SharedState) {
    if ui.get_view_mode() == 0 {
        return;
    }
    let (text, lang, theme) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => (t.document.to_string(), st.editor.language_of(t), st.settings.theme),
            None => (String::new(), "plaintext".to_string(), st.settings.theme),
        }
    };
    crate::preview::sync_preview(ui, &text, &lang, theme);
}

/// Escribe un mensaje en la barra de estado (reemplaza todos los `eprintln!` del
/// main.rs original: la app es `windows_subsystem = "windows"`, invisible sin esto).
pub fn set_status(ui: &App, msg: &str) {
    ui.set_status_text(msg.into());
}

pub fn refresh_recent(ui: &App, state: &SharedState) {
    let st = state.borrow();
    let items: Vec<SharedString> = st.settings.recent_files.iter().map(|s| s.clone().into()).collect();
    ui.set_recent_files(ModelRc::new(VecModel::from(items)));
}

/// Empuja el texto/cursor de la pestaña activa a la UI. Necesario tras cualquier cambio
/// de pestaña activa (abrir, cerrar, cambiar de grupo, clic en pestaña): `editor_text`
/// está enlazado 1:1 al `TextInput` interno y no se sincroniza solo con el backend.
pub fn push_active_document_to_ui(ui: &App, state: &SharedState) {
    let (text, cursor) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => {
                let text = t.document.to_string();
                let len = text.len();
                (text, t.cursor.min(len))
            }
            None => (String::new(), 0),
        }
    };
    ui.set_editor_text(text.into());
    let c = cursor as i32;
    ui.invoke_select_range(c, c);
    {
        let mut st = state.borrow_mut();
        st.content_dirty = true;
        st.session_dirty = true;
        st.last_line_count = 0; // fuerza recomputar el gutter en el siguiente refresh
    }
    editor_view_ops::on_document_replaced(ui, state);
}

/// El refresco "completo": pestañas + banderas + estadísticas (forzando el gutter) +
/// vista previa (si visible) + recientes. Se usa tras operaciones poco frecuentes
/// (abrir/cerrar/guardar/deshacer/rehacer/cambio de pestaña), nunca por pulsación.
pub fn refresh_ui(ui: &App, state: &SharedState) {
    refresh_tabs(ui, state);
    refresh_flags(ui, state);
    refresh_stats(ui, state, true);
    refresh_preview_if_visible(ui, state);
    refresh_recent(ui, state);
    // Hallazgo W9: `refresh_ui` se llama tras cualquier operación estructural poco
    // frecuente (abrir, cerrar, guardar, deshacer/rehacer, reemplazar, formatear...);
    // marcar la sesión como pendiente aquí también cubre cambios que no pasan por
    // `push_active_document_to_ui` (p.ej. cerrar una pestaña de FONDO).
    state.borrow_mut().session_dirty = true;
}

/// Cambia la pestaña/grupo activo y sincroniza todo lo que dependa de ello.
pub fn activate_tab(ui: &App, state: &SharedState, group: usize, index: usize) {
    {
        let mut st = state.borrow_mut();
        if group < st.editor.groups.len() {
            st.editor.active_group = group;
            if let Some(g) = st.editor.groups.get_mut(group)
                && index < g.tabs.len()
            {
                g.active = index;
            }
        }
    }
    push_active_document_to_ui(ui, state);
    refresh_ui(ui, state);
}

/// Construye la tabla de atajos mostrada en el diálogo de Ayuda (debe coincidir
/// exactamente con los `shortcut: @keys(...)` declarados en `ui/app.slint`).
pub fn build_shortcuts_list() -> Vec<crate::ShortcutEntry> {
    let rows: &[(&str, &str, &str)] = &[
        ("Archivo", "Nuevo", "Ctrl+N"),
        ("Archivo", "Abrir…", "Ctrl+O"),
        ("Archivo", "Abrir carpeta…", "Ctrl+Shift+O"),
        ("Archivo", "Guardar", "Ctrl+S"),
        ("Archivo", "Guardar como…", "Ctrl+Shift+S"),
        ("Archivo", "Guardar todo", "Ctrl+Alt+S"),
        ("Archivo", "Cerrar pestaña", "Ctrl+W"),
        ("Archivo", "Configuración…", "Ctrl+,"),
        ("Editar", "Deshacer", "Ctrl+Z"),
        ("Editar", "Rehacer", "Ctrl+Y"),
        ("Editar", "Cortar", "Ctrl+X"),
        ("Editar", "Copiar", "Ctrl+C"),
        ("Editar", "Pegar", "Ctrl+V"),
        ("Editar", "Seleccionar todo", "Ctrl+A"),
        ("Editar", "Buscar", "Ctrl+F"),
        ("Editar", "Reemplazar", "Ctrl+H"),
        ("Editar", "Buscar siguiente", "F3"),
        ("Editar", "Buscar anterior", "Shift+F3"),
        ("Editar", "Formatear documento", "Ctrl+Shift+F"),
        ("Selección", "Duplicar línea", "Ctrl+Shift+D"),
        ("Selección", "Eliminar línea", "Ctrl+Shift+K"),
        ("Selección", "Mover línea arriba", "Alt+↑"),
        ("Selección", "Mover línea abajo", "Alt+↓"),
        ("Selección", "Comentar / descomentar", "Ctrl+/"),
        ("Selección", "Indentar", "Tab"),
        ("Selección", "Desindentar", "Shift+Tab"),
        ("Selección", "Unir líneas", "Ctrl+J"),
        ("Ver", "Explorador", "Ctrl+B"),
        ("Ver", "Ajuste de línea", "Alt+Z"),
        ("Ver", "Alternar vista previa", "Ctrl+M"),
        ("Ver", "Acercar", "Ctrl+="),
        ("Ver", "Alejar", "Ctrl+-"),
        ("Ver", "Restablecer zoom", "Ctrl+0"),
        ("Ver", "Pantalla completa", "F11"),
        ("Ir", "Ir a línea…", "Ctrl+G"),
        ("Ir", "Pestaña siguiente", "Ctrl+Tab"),
        ("Ir", "Pestaña anterior", "Ctrl+Shift+Tab"),
        ("Ir", "Grupo 1..4", "Alt+1..4"),
        ("Ayuda", "Atajos de teclado", "F1"),
    ];
    // V16: en vez de repetir la categoría en cada fila (que además el scrollbar tapaba
    // junto con la columna de atajos), se emite una fila de cabecera por categoría
    // (action == category, shortcut == "" la marca como cabecera para la UI) y las
    // filas normales dejan `category` vacío: la UI ya no dibuja columna de categoría.
    let mut out = Vec::with_capacity(rows.len() + 8);
    let mut last_category = "";
    for (category, action, shortcut) in rows.iter().copied() {
        if category != last_category {
            out.push(crate::ShortcutEntry {
                category: category.into(),
                action: category.into(),
                shortcut: "".into(),
            });
            last_category = category;
        }
        out.push(crate::ShortcutEntry {
            category: category.into(),
            action: action.into(),
            shortcut: shortcut.into(),
        });
    }
    out
}
