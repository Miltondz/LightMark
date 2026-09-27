//! Edición: deshacer/rehacer, formatear, edición de texto/cursor y operaciones de línea
//! (duplicar/borrar/mover/comentar/indentar/unir), todas conscientes de cursor/selección
//! (nunca del documento entero — hallazgo #2).

use super::{refresh_active_tab_dirty, refresh_tabs, refresh_undo_save_flags, set_status, SharedState};
use crate::App;
use ropey::Rope;
use slint::ComponentHandle;

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_text_edited(move |text| {
            if let Some(ui) = ui_weak.upgrade() {
                on_text_edited(&ui, &state, text.as_str());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_cursor_moved(move |cursor, anchor| {
            if let Some(ui) = ui_weak.upgrade() {
                on_cursor_moved(&ui, &state, cursor, anchor);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_undo(move || {
            if let Some(ui) = ui_weak.upgrade() {
                undo(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_redo(move || {
            if let Some(ui) = ui_weak.upgrade() {
                redo(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_format_document(move || {
            if let Some(ui) = ui_weak.upgrade() {
                format_document(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_duplicate_line(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_line_op(&ui, &state, "Línea duplicada", crate::textops::duplicate_lines);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_delete_line(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_line_op(&ui, &state, "Línea eliminada", crate::textops::delete_lines);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_move_line_up(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_line_op(&ui, &state, "Línea movida arriba", crate::textops::move_lines_up);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_move_line_down(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_line_op(&ui, &state, "Línea movida abajo", crate::textops::move_lines_down);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_toggle_comment(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let lang = {
                    let st = state.borrow();
                    st.editor.active_tab().map(|t| st.editor.language_of(t)).unwrap_or_default()
                };
                apply_line_op(&ui, &state, "Comentario alternado", |t, a, c| {
                    crate::textops::toggle_comment(t, a, c, &lang)
                });
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_indent_line(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let unit = indent_unit(&state);
                apply_line_op(&ui, &state, "Indentado", move |t, a, c| crate::textops::indent(t, a, c, &unit));
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_unindent_line(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let width = { state.borrow().settings.tab_size as usize };
                apply_line_op(&ui, &state, "Desindentado", move |t, a, c| crate::textops::unindent(t, a, c, width));
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_join_lines(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_line_op(&ui, &state, "Líneas unidas", crate::textops::join_lines);
            }
        });
    }
}

fn indent_unit(state: &SharedState) -> String {
    let st = state.borrow();
    if st.settings.insert_spaces {
        " ".repeat(st.settings.tab_size as usize)
    } else {
        "\t".to_string()
    }
}

/// Tamaño del cambio real aplicado por `apply_text_diff` (hallazgo W18: para decidir si
/// se funde con el tecleo anterior o se registra como comando discreto). Hallazgo C11:
/// contado en CARACTERES, no en bytes — un carácter multibyte ('ñ', 'á', '€'...) tecleado
/// de uno en uno tiene `len_utf8() > 1`, así que contar bytes lo marcaba como "cambio
/// masivo" (`> 1`) y abría un paso de undo propio en cada pulsación, partiendo la ráfaga
/// de tecleo normal en vez de fundirla como con un carácter ASCII.
struct TextDiff {
    /// Offset en bytes del primer byte cambiado (prefijo común).
    start: usize,
    removed_len: usize,
    inserted_len: usize,
    inserted_has_newline: bool,
}

/// Aplica `new_text` sobre `rope` mutando solo la porción que realmente cambió
/// (hallazgo W12), en vez de reconstruirlo entero con `Rope::from_str` en cada tecla:
/// ver `textops::common_prefix_suffix` para el porqué (comparición estructural de las
/// partes no tocadas entre snapshots de la pila de deshacer). Devuelve el tamaño del
/// cambio (hallazgo W18).
fn apply_text_diff(rope: &mut Rope, new_text: &str) -> TextDiff {
    let old_text = rope.to_string();
    if old_text == new_text {
        return TextDiff { start: 0, removed_len: 0, inserted_len: 0, inserted_has_newline: false };
    }
    let (prefix, old_suffix_start, new_suffix_start) =
        crate::textops::common_prefix_suffix(&old_text, new_text);
    if old_suffix_start > prefix {
        rope.remove(prefix..old_suffix_start);
    }
    let inserted = &new_text[prefix..new_suffix_start];
    if new_suffix_start > prefix {
        rope.insert(prefix, inserted);
    }
    // Hallazgo C11: contar CARACTERES, no bytes, del tramo realmente tocado.
    let removed_chars = old_text[prefix..old_suffix_start].chars().count();
    let inserted_chars = inserted.chars().count();
    TextDiff {
        start: prefix,
        removed_len: removed_chars,
        inserted_len: inserted_chars,
        inserted_has_newline: inserted.contains('\n'),
    }
}

/// Normaliza `\r\n`/`\r` sueltos a `\n` (hallazgo W11: pegar desde el portapapeles de
/// Windows) y recalcula la posición de `cursor` (offset en bytes dentro de `new_text`,
/// SIN normalizar) en el texto YA normalizado. Solo `"\r\n" -> "\n"` cambia la longitud
/// en bytes (se elimina el `\r`); un `\r` suelto se sustituye por `\n` sin cambiar la
/// longitud, así que basta con normalizar el mismo prefijo hasta `cursor` para saber
/// dónde cae en el texto nuevo.
/// Decide si `on_text_edited` debe pasar `new_text` por `normalize_pasted_text` (hallazgo
/// C2): nunca para una pestaña `Mixed` (su `\r` es contenido original del archivo, no
/// algo pegado, y normalizarlo en cada tecla le haría perder su estilo de fin de línea);
/// para las demás, solo si el texto trae algún `\r` (pegado desde el portapapeles de
/// Windows).
fn should_normalize_line_endings(is_mixed: bool, new_text: &str) -> bool {
    !is_mixed && new_text.contains('\r')
}

fn normalize_pasted_text(new_text: &str, cursor: usize) -> (String, usize) {
    let old_cursor = cursor.min(new_text.len());
    let normalized = new_text.replace("\r\n", "\n").replace('\r', "\n");
    let prefix_len = new_text[..old_cursor].replace("\r\n", "\n").replace('\r', "\n").len();
    (normalized, prefix_len)
}

fn on_text_edited(ui: &App, state: &SharedState, new_text: &str) {
    // Hallazgo W11: pegar desde el portapapeles de Windows (que usa CRLF) mete '\r' en
    // el documento; el editor interno siempre trabaja en LF (hallazgo E11) — un '\r'
    // suelto en el `Rope` rompe el conteo de líneas/columnas y el resaltado de
    // sintaxis. Si el texto nuevo trae '\r', se normaliza aquí mismo (antes de guardarlo
    // en el `Rope`) y se reescribe el `TextInput` con el texto normalizado + la
    // selección recalculada para el cursor equivalente.
    // Hallazgo C2: un archivo con fin de línea `Mixed` se guarda verbatim — su `Rope`
    // (y por tanto el `new_text` que llega aquí desde el `TextInput`, que refleja TODO
    // el documento en cada tecla) contiene `\r` como parte del contenido ORIGINAL del
    // archivo, no solo del fragmento recién pegado. Antes, ver `new_text.contains('\r')`
    // disparaba `normalize_pasted_text` sobre el documento COMPLETO en cada tecla — no
    // solo convertía a LF los `\r` del propio pegado, sino todos los `\r` preexistentes
    // del archivo Mixed, perdiendo su estilo de fin de línea original en cada pulsación
    // (y con coste O(n) por tecla). Un documento `Mixed` no se normaliza aquí: se deja
    // pasar verbatim, igual que hace `normalize_newlines` al cargarlo.
    let is_mixed = state
        .borrow()
        .editor
        .active_tab()
        .map(|t| t.line_ending == crate::textops::LineEnding::Mixed)
        .unwrap_or(false);
    let renormalized: Option<(String, usize)> = if should_normalize_line_endings(is_mixed, new_text) {
        let cursor_before = state.borrow().editor.active_tab().map(|t| t.cursor).unwrap_or(0);
        Some(normalize_pasted_text(new_text, cursor_before))
    } else {
        None
    };
    let text_for_document: &str = renormalized.as_ref().map(|(t, _)| t.as_str()).unwrap_or(new_text);
    let mut applied: Option<(usize, usize, usize)> = None;
    {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        if let Some(tab) = st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
            let diff = apply_text_diff(&mut tab.document, text_for_document);
            applied = Some((diff.start, diff.removed_len, diff.inserted_len));
            if let Some((_, new_cursor)) = &renormalized {
                tab.cursor = *new_cursor;
            }
            // Registrar DESPUÉS de aplicar la edición (invariante del modelo nuevo de
            // UndoStack — hallazgo A): funde con el tecleo anterior si llegó a tiempo,
            // o abre un paso nuevo.
            //
            // Hallazgo W18: un cambio "grande" (pegar, cortar una selección, un
            // reemplazo que borra varios caracteres a la vez) NO debe fundirse con la
            // ráfaga de tecleo anterior/siguiente en un solo paso de undo — un `Ctrl+Z`
            // justo después de pegar un párrafo debe deshacer SOLO el pegado, no
            // también las últimas teclas escritas antes. Un cambio de un solo carácter
            // (o su borrado) sigue tratándose como tecleo normal, para conservar la
            // fusión por ráfaga habitual.
            let is_bulk_change = diff.removed_len > 1 || diff.inserted_len > 1 || diff.inserted_has_newline;
            if is_bulk_change {
                tab.record_command(tab.cursor);
            } else {
                tab.record_typing(tab.cursor);
            }
        }
        st.content_dirty = true;
        // Hallazgo W9: cualquier tecleo debe poder disparar un "hot exit" de sesión en el
        // siguiente tick de autoguardado, no solo cuando cambia la LONGITUD del documento
        // sucio (ver `main.rs::autosave_tick`).
        st.session_dirty = true;
    }
    if let Some((normalized, new_cursor)) = &renormalized {
        ui.set_editor_text(normalized.clone().into());
        let c = *new_cursor as i32;
        ui.invoke_select_range(c, c);
    }
    // Hallazgo F (rendimiento): por cada tecla, antes se llamaba `refresh_tabs`
    // (reconstruía el `VecModel` de pestañas entero, re-instanciando el `TabBar`) y
    // `refresh_flags` (que además recalcula el lenguaje detectado con un `to_string()`
    // + heurísticas sobre todo el documento). Ahora solo se actualiza lo que realmente
    // puede cambiar con cada tecleo: el punto "●" de la fila activa y
    // deshacer/rehacer/guardar. El resto (lenguaje, formato, fin de línea, vista
    // previa, números de línea) se recalcula como mucho a ritmo del timer de 150 ms
    // (`content_dirty`, ver `main.rs::on_tick`) o en eventos poco frecuentes.
    refresh_active_tab_dirty(ui, state);
    refresh_undo_save_flags(ui, state);
    // Hallazgo W1: recalcula línea/columna sobre el `Rope` ya actualizado con el texto
    // nuevo, corrigiendo cualquier valor que `on_cursor_moved` hubiera calculado sobre el
    // `Rope` todavía viejo (ver `rope_line_col`).
    update_cursor_status(ui, state);
    super::search_ops::refresh_after_text_change(ui, state);
    // WS-A: el overlay de resaltado debe actualizarse de forma síncrona en el `edited`.
    if let Some((start, removed, inserted)) = applied {
        super::editor_view_ops::on_edit(ui, state, start, removed, inserted);
    }
}

fn on_cursor_moved(ui: &App, state: &SharedState, cursor: i32, anchor: i32) {
    let mut st = state.borrow_mut();
    st.anchor = anchor.max(0) as usize;
    let ag = st.editor.active_group;
    let mut line_col = None;
    if let Some(tab) = st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
        let c = cursor.max(0) as usize;
        tab.cursor = c;
        // Hallazgo F (rendimiento): antes se hacía `tab.document.to_string()` (copia
        // O(n) de todo el documento) en CADA movimiento de cursor/flecha solo para
        // contar líneas/columnas hasta ese punto. `rope_line_col` recorre el `Rope`
        // directamente (sin materializar un `String`) usando sus índices internos.
        line_col = Some(rope_line_col(&tab.document, c));
    }
    drop(st);
    if let Some((line, col)) = line_col {
        ui.set_cursor_line(line as i32);
        ui.set_cursor_col(col as i32);
    }
}

/// Línea/columna (1-based) de un desplazamiento en bytes dentro de un `Rope`, sin pasar
/// por `to_string()` del documento completo (hallazgo F: eso era O(n) en cada
/// movimiento de cursor/flecha). Solo se materializa como `String` la porción de la
/// línea actual hasta el cursor (típicamente unas pocas decenas de bytes), para contar
/// columnas en `char`s (no bytes) igual que `textops::line_col`, sin romper el recuento
/// con UTF-8 multi-byte.
///
/// Hallazgo W1 (CRÍTICO): Slint dispara `cursor-moved` (con la posición NUEVA del
/// cursor) ANTES de `edited` (que es cuando `tab.document` se actualiza con el texto
/// nuevo). Al teclear justo ANTES de un carácter multibyte ('ñ', 'á'...), el offset que
/// llega en `cursor-moved` corresponde al documento NUEVO, pero aquí todavía se lee el
/// `Rope` VIEJO — ese offset puede caer en mitad de un carácter multibyte del rope viejo
/// (p.ej. porque el nuevo carácter insertado tiene un ancho en bytes distinto), y
/// `byte_to_line_idx`/`slice` entran en pánico si no es un límite de carácter válido.
/// Se ajusta `clamped` hacia atrás al límite de carácter más cercano (nunca hacia
/// delante, para no saltarse el propio `byte_offset` cuando SÍ es válido) antes de usarlo.
fn rope_line_col(rope: &Rope, byte_offset: usize) -> (usize, usize) {
    let clamped = rope.floor_char_boundary(byte_offset.min(rope.len()));
    // Hallazgo C17: `LineType::Unicode` cuenta como salto de línea también separadores
    // Unicode que no son '\n' (p.ej. U+2028, U+0085, U+000C) — el número de línea de la
    // barra de estado podía así ir por delante del número real del gutter (que cuenta
    // líneas por '\n', ver `textops::line_count`) en un documento que contuviera alguno
    // de esos caracteres. `LineType::LF` cuenta únicamente '\n', igual que el resto del
    // editor, para que ambos números siempre coincidan.
    let line_type = ropey::LineType::LF;
    let line_idx = rope.byte_to_line_idx(clamped, line_type);
    let line_start = rope.line_to_byte_idx(line_idx, line_type);
    let col = rope.slice(line_start..clamped).chars().count();
    (line_idx + 1, col + 1)
}

/// Recalcula y publica línea/columna del cursor de la pestaña activa a partir del
/// `Rope` YA actualizado (hallazgo W1): se llama al final de `on_text_edited`, una vez
/// que `tab.document` refleja el texto nuevo, para corregir cualquier línea/columna que
/// `on_cursor_moved` hubiera calculado sobre el `Rope` todavía viejo (ver comentario de
/// `rope_line_col`).
fn update_cursor_status(ui: &App, state: &SharedState) {
    let line_col = {
        let st = state.borrow();
        let ag = st.editor.active_group;
        st.editor
            .groups
            .get(ag)
            .and_then(|g| g.active_tab())
            .map(|tab| rope_line_col(&tab.document, tab.cursor))
    };
    if let Some((line, col)) = line_col {
        ui.set_cursor_line(line as i32);
        ui.set_cursor_col(col as i32);
    }
}

fn undo(ui: &App, state: &SharedState) {
    let restored = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()).and_then(|tab| {
            if tab.apply_undo() {
                Some((tab.document.to_string(), tab.cursor))
            } else {
                None
            }
        })
    };
    if let Some((text, cursor)) = restored {
        ui.set_editor_text(text.into());
        let c = cursor as i32;
        ui.invoke_select_range(c, c);
        set_status(ui, "Deshecho");
        refresh_after_edit(ui, state);
    }
}

fn redo(ui: &App, state: &SharedState) {
    let restored = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()).and_then(|tab| {
            if tab.apply_redo() {
                Some((tab.document.to_string(), tab.cursor))
            } else {
                None
            }
        })
    };
    if let Some((text, cursor)) = restored {
        ui.set_editor_text(text.into());
        let c = cursor as i32;
        ui.invoke_select_range(c, c);
        set_status(ui, "Rehecho");
        refresh_after_edit(ui, state);
    }
}

fn format_document(ui: &App, state: &SharedState) {
    let (lang, text) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => (st.editor.language_of(t), t.document.to_string()),
            None => return,
        }
    };
    let fmt_lang = match lang.as_str() {
        "json" => crate::format::FormatLanguage::Json,
        "xml" => crate::format::FormatLanguage::Xml,
        "html" => crate::format::FormatLanguage::Html,
        "sql" => crate::format::FormatLanguage::Sql,
        _ => {
            set_status(ui, "El lenguaje actual no se puede formatear");
            return;
        }
    };
    match crate::format::format_text(&text, fmt_lang) {
        Ok(formatted) => {
            {
                let mut st = state.borrow_mut();
                let ag = st.editor.active_group;
                if let Some(tab) = st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
                    // Hallazgo C12: mutar el `Rope` in-place vía diff prefijo/sufijo (ver
                    // `apply_text_diff`) en vez de reconstruirlo entero con
                    // `Rope::from_str` — formatear suele dejar intactos el principio y/o
                    // final del documento (llaves, indentación exterior...), así que
                    // solo hace falta tocar el tramo que realmente cambió.
                    apply_text_diff(&mut tab.document, &formatted);
                    tab.cursor = 0;
                    // Comando discreto: paso de undo independiente (R1_FIXES B/A).
                    tab.record_command(0);
                }
            }
            ui.set_editor_text(formatted.into());
            ui.invoke_select_range(0, 0);
            set_status(ui, "Documento formateado");
            refresh_after_edit(ui, state);
        }
        Err(e) => {
            let msg = match e {
                crate::format::FormatError::ParseError(s) => s,
                crate::format::FormatError::SyntaxError(s) => s,
            };
            set_status(ui, &format!("Error al formatear: {}", msg));
        }
    }
}

/// Aplica una operación de `textops` (consciente de cursor/selección) sobre el
/// documento activo: empuja undo, aplica el nuevo texto/selección y refresca la UI.
fn apply_line_op<F>(ui: &App, state: &SharedState, status: &str, op: F)
where
    F: FnOnce(&str, usize, usize) -> crate::textops::Edit,
{
    let anchor = state.borrow().anchor;
    let result = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()).map(|tab| {
            let text = tab.document.to_string();
            let cursor = tab.cursor.min(text.len());
            let anchor = anchor.min(text.len());
            let edit = op(&text, anchor, cursor);
            // Hallazgo C12: mutar el `Rope` in-place vía diff prefijo/sufijo en vez de
            // reconstruirlo entero con `Rope::from_str` — una operación de línea
            // (duplicar/borrar/mover/comentar/indentar/unir) casi siempre deja intacto
            // el resto del documento fuera de la línea/selección tocada.
            apply_text_diff(&mut tab.document, &edit.text);
            tab.cursor = edit.cursor;
            // Comando discreto: paso de undo independiente (R1_MANUAL M3 / R1_FIXES A).
            tab.record_command(edit.cursor);
            edit
        })
    };
    if let Some(edit) = result {
        ui.set_editor_text(edit.text.into());
        ui.invoke_select_range(edit.anchor as i32, edit.cursor as i32);
        {
            let mut st = state.borrow_mut();
            st.anchor = edit.anchor;
        }
        set_status(ui, status);
        refresh_after_edit(ui, state);
    }
}

/// Hallazgo C12: una sola materialización del documento (`to_string()`) para banderas +
/// estadísticas + vista previa, en vez de que `refresh_flags`, `refresh_stats` y
/// `refresh_preview_if_visible` cada una volviera a copiar el documento entero por
/// separado (hasta 3-4 copias completas tras una sola operación de línea o Formatear).
/// Equivalente a `refresh_on_tick` (usado por el timer de 150 ms) pero fuerza siempre el
/// recómputo del gutter de números de línea, porque estas operaciones son poco
/// frecuentes (no por tecla) y casi siempre cambian el número de líneas.
fn refresh_after_edit(ui: &App, state: &SharedState) {
    refresh_tabs(ui, state);
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
    ui.set_can_save_all(super::any_tab_dirty(&state.borrow().editor));
    ui.set_can_format(can_format);
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
    ui.set_line_numbers_text(super::generate_line_numbers(lines).into());
    ui.set_goto_line_max(lines.max(1) as i32);
    ui.set_doc_stats(super::doc_stats_text(words, chars).into());

    {
        let mut st = state.borrow_mut();
        st.last_line_count = lines;
        st.content_dirty = true;
        st.session_dirty = true;
    }

    if effective != 0 {
        let theme = state.borrow().settings.theme;
        crate::preview::sync_preview(ui, &text, &lang, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rope_line_col_floors_to_char_boundary_no_panic() {
        // Hallazgo W1 (CRÍTICO): un offset que cae en mitad de un carácter multibyte
        // ('ñ' es 2 bytes en UTF-8: 0xC3 0xB1) no debe hacer panic; debe tratarse como si
        // apuntara al carácter anterior completo.
        let rope = Rope::from_str("a\u{f1}b"); // "a" + "ñ" (2 bytes) + "b" => bytes: a=0..1, ñ=1..3, b=3..4
        // byte_offset=2 cae en mitad de 'ñ' (que ocupa los bytes 1..3).
        let (line, col) = rope_line_col(&rope, 2);
        assert_eq!(line, 1);
        // Se comporta como floor_char_boundary(2) == 1 (justo tras la 'a').
        assert_eq!(col, 2);
    }

    #[test]
    fn rope_line_col_exact_boundary_unaffected() {
        let rope = Rope::from_str("line1\nlineñ2");
        // Offset justo después de la 'ñ' (límite válido): línea 2, columna 5 ("line" + "ñ").
        let byte_after_ntilde = "line1\nline".len() + 'ñ'.len_utf8();
        let (line, col) = rope_line_col(&rope, byte_after_ntilde);
        assert_eq!(line, 2);
        // "line" (4 chars) + "ñ" (1 char) = 5 chars antes del cursor -> columna 6 (1-based).
        assert_eq!(col, 6);
    }

    #[test]
    fn c17_rope_line_col_counts_only_lf_not_unicode_line_separators() {
        // Hallazgo C17: U+2028 (LINE SEPARATOR) es un salto de línea para
        // `LineType::Unicode` pero NO para `LineType::LF`. El resto del editor (gutter,
        // `textops::line_count`) solo cuenta '\n', así que `rope_line_col` debe coincidir
        // con eso: el cursor tras el separador Unicode debe seguir viéndose como línea 1,
        // no línea 2.
        let rope = Rope::from_str("a\u{2028}b\nc");
        let after_separator = "a\u{2028}".len();
        let (line, _col) = rope_line_col(&rope, after_separator);
        assert_eq!(line, 1, "U+2028 no es un salto de línea para LineType::LF");

        let after_real_newline = "a\u{2028}b\n".len();
        let (line2, _col2) = rope_line_col(&rope, after_real_newline);
        assert_eq!(line2, 2, "el '\\n' real sí debe avanzar de línea");
    }

    #[test]
    fn rope_line_col_clamps_past_end_of_document() {
        let rope = Rope::from_str("abc");
        let (line, col) = rope_line_col(&rope, 999);
        assert_eq!(line, 1);
        assert_eq!(col, 4);
    }

    #[test]
    fn normalize_pasted_text_strips_crlf() {
        // Hallazgo W11: "a\r\nb\r\nc" pegado desde Windows -> "a\nb\nc".
        let (normalized, _) = normalize_pasted_text("a\r\nb\r\nc", 0);
        assert_eq!(normalized, "a\nb\nc");
        assert!(!normalized.contains('\r'));
    }

    #[test]
    fn normalize_pasted_text_strips_lone_cr() {
        let (normalized, _) = normalize_pasted_text("a\rb", 0);
        assert_eq!(normalized, "a\nb");
    }

    #[test]
    fn normalize_pasted_text_adjusts_cursor_for_removed_bytes() {
        // "ab\r\ncd": el cursor tras pegar queda en 6 (justo después de la 'd', al
        // final). Tras normalizar a "ab\ncd" (5 bytes, se eliminó 1 '\r'), el cursor
        // equivalente debe ser 5, no 6 (que caería fuera del texto o desplazado).
        let (normalized, cursor) = normalize_pasted_text("ab\r\ncd", 6);
        assert_eq!(normalized, "ab\ncd");
        assert_eq!(cursor, 5);
        assert_eq!(cursor, normalized.len());
    }

    #[test]
    fn apply_text_diff_matches_full_rebuild() {
        // Hallazgo W12: mutar el `Rope` in-place vía diff prefijo/sufijo debe producir
        // exactamente el mismo texto que reconstruirlo entero con `Rope::from_str`.
        let cases: &[(&str, &str)] = &[
            ("", "hello"),
            ("hello", ""),
            ("abc", "abcd"),
            ("abcd", "abXcd"),
            ("año", "años"),
            ("hello", "goodbye"),
            ("same", "same"),
            ("line1\nline2\nline3", "line1\nlineX\nline3"),
        ];
        for (old, new) in cases {
            let mut rope = Rope::from_str(old);
            apply_text_diff(&mut rope, new);
            assert_eq!(rope.to_string(), *new, "old={:?} new={:?}", old, new);
            assert_eq!(rope, Rope::from_str(new));
        }
    }

    #[test]
    fn text_diff_flags_bulk_paste_vs_single_keystroke() {
        // Hallazgo W18: pegar/cortar un bloque (más de 1 carácter, o que incluya un
        // salto de línea) debe distinguirse de un tecleo normal de un solo carácter.
        let mut rope = Rope::from_str("hello world");
        let diff = apply_text_diff(&mut rope, "hello there world");
        assert!(diff.inserted_len > 1);
        assert!(diff.removed_len <= 1);

        let mut rope2 = Rope::from_str("ab");
        let single_char_diff = apply_text_diff(&mut rope2, "abc");
        assert_eq!(single_char_diff.inserted_len, 1);
        assert_eq!(single_char_diff.removed_len, 0);
        assert!(!single_char_diff.inserted_has_newline);

        let mut rope3 = Rope::from_str("a");
        let multiline_paste_diff = apply_text_diff(&mut rope3, "a\nb\nc");
        assert!(multiline_paste_diff.inserted_has_newline);
    }

    #[test]
    fn normalize_pasted_text_cursor_before_crlf_unaffected() {
        // Cursor en 2 (justo antes del "\r\n"): no hay ningún '\r' en el prefijo, así
        // que la posición no cambia.
        let (_, cursor) = normalize_pasted_text("ab\r\ncd", 2);
        assert_eq!(cursor, 2);
    }

    #[test]
    fn c2_mixed_line_ending_tab_is_never_normalized() {
        // Hallazgo C2: un documento `Mixed` guarda su `\r` verbatim como parte del
        // contenido original del archivo; normalizarlo en cada tecla (como si fuera un
        // pegado con CRLF) le haría perder su estilo de fin de línea real.
        let mixed_text = "a\r\nb\nc\rd"; // CRLF + LF + CR suelto: se detecta como Mixed.
        assert!(!should_normalize_line_endings(true, mixed_text));
        // Para una pestaña normal (LF/CRLF), un `\r` en el texto SÍ es un pegado y debe
        // normalizarse.
        assert!(should_normalize_line_endings(false, "a\r\nb"));
        // Sin ningún '\r' no hay nada que normalizar, sea o no Mixed.
        assert!(!should_normalize_line_endings(false, "abc"));
        assert!(!should_normalize_line_endings(true, "abc"));
    }

    #[test]
    fn c11_text_diff_counts_characters_not_bytes_for_multibyte_input() {
        // Hallazgo C11: 'ñ', 'á', '€'... ocupan más de 1 byte en UTF-8. Tecleados de uno
        // en uno deben seguir contando como UN solo carácter (para fundirse con la
        // ráfaga de tecleo normal), no como un "cambio masivo" solo porque su
        // codificación en bytes es más larga que 1.
        let mut rope = Rope::from_str("a");
        let diff_ntilde = apply_text_diff(&mut rope, "añ"); // 'ñ' son 2 bytes en UTF-8.
        assert_eq!(diff_ntilde.inserted_len, 1, "1 carácter insertado, no 2 bytes");
        assert_eq!(diff_ntilde.removed_len, 0);

        let mut rope2 = Rope::from_str("a");
        let diff_euro = apply_text_diff(&mut rope2, "a€"); // '€' son 3 bytes en UTF-8.
        assert_eq!(diff_euro.inserted_len, 1, "1 carácter insertado, no 3 bytes");

        // Borrar un solo carácter multibyte también cuenta como 1, no como sus bytes.
        let mut rope3 = Rope::from_str("añ");
        let diff_removed = apply_text_diff(&mut rope3, "a");
        assert_eq!(diff_removed.removed_len, 1);
        assert_eq!(diff_removed.inserted_len, 0);
    }

    #[test]
    fn c12_apply_text_diff_is_noop_on_identical_text() {
        // Hallazgo C12: si el texto no cambió (p.ej. Formatear un documento que ya
        // estaba bien formateado), `apply_text_diff` no debe hacer ninguna
        // remoción/inserción sobre el `Rope` — nada que mutar in-place.
        let mut rope = Rope::from_str("already formatted\ntext");
        let diff = apply_text_diff(&mut rope, "already formatted\ntext");
        assert_eq!(diff.removed_len, 0);
        assert_eq!(diff.inserted_len, 0);
        assert_eq!(rope.to_string(), "already formatted\ntext");
    }
}
