//! Búsqueda y reemplazo: incremental, con mayúsculas/palabra completa/regex, selección
//! visual de la coincidencia, F3/Shift+F3, reemplazar uno/todos con undo, y recómputo
//! tras editar mientras el panel está abierto.

use super::{push_active_document_to_ui, refresh_ui, set_status, SharedState};
use crate::App;
use crate::search::{self, SearchOptions};
use slint::ComponentHandle;

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_search_changed(move |_query| {
            if ui_weak.upgrade().is_some() {
                // Hallazgo W5: no recomputar de forma síncrona en cada tecla — solo marcar
                // la búsqueda como pendiente; `search_tick` (llamado desde el timer de 150ms
                // en `main.rs`) es quien recalcula, al mismo ritmo que estadísticas/vista
                // previa, para no colgar la UI con documentos grandes.
                state.borrow_mut().search_dirty = true;
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_find_next(move || {
            if let Some(ui) = ui_weak.upgrade() {
                find_next(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_find_prev(move || {
            if let Some(ui) = ui_weak.upgrade() {
                find_prev(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_replace_one(move || {
            if let Some(ui) = ui_weak.upgrade() {
                replace_one(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_replace_all(move || {
            if let Some(ui) = ui_weak.upgrade() {
                replace_all(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_toggle_search_panel(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let visible = !ui.get_show_search();
                ui.set_show_search(visible);
                if visible {
                    let query = ui.get_search_query().to_string();
                    do_search(&ui, &state, &query);
                }
            }
        });
    }
}

fn options_from_ui(ui: &App) -> SearchOptions {
    SearchOptions {
        case_sensitive: ui.get_search_case_sensitive(),
        whole_word: ui.get_search_whole_word(),
        use_regex: ui.get_search_regex(),
        wrap: true,
    }
}

/// Recalcula los resultados de búsqueda para `query` (hallazgo D1: separado de la
/// selección) y actualiza `search_status`. No mueve el cursor ni la selección — lo
/// hace `do_search`, que llama a esto y luego a `select_current_result`.
fn compute_search(ui: &App, state: &SharedState, query: &str) {
    let opts = options_from_ui(ui);
    let status = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        let cursor = st
            .editor
            .groups
            .get(ag)
            .and_then(|g| g.active_tab())
            .map(|t| t.cursor)
            .unwrap_or(0);
        let rope = st.editor.groups.get(ag).and_then(|g| g.active_tab()).map(|t| t.document.clone());
        match rope {
            None => {
                st.editor.search_results.clear();
                "Sin resultados".to_string()
            }
            Some(_) if query.is_empty() => {
                st.editor.search_results.clear();
                st.editor.search_index = 0;
                String::new()
            }
            // Hallazgo W5: `search_in_rope` compila el regex una sola vez y devuelve
            // `Err` si no es válido, en vez de que el llamador lo compilara aparte solo
            // para validarlo (y `search_in_rope` lo volviera a compilar después).
            Some(rope) => match search::search_in_rope(&rope, query, opts) {
                Err(_) => {
                    st.editor.search_results.clear();
                    "Expresión regular no válida".to_string()
                }
                Ok(results) => {
                    let idx = search::nearest_index(&results, cursor);
                    let status = if results.is_empty() {
                        "Sin resultados".to_string()
                    } else if results.len() >= search::MAX_SEARCH_RESULTS {
                        format!("{}+ resultados", search::MAX_SEARCH_RESULTS)
                    } else {
                        format!("{} de {}", idx + 1, results.len())
                    };
                    st.editor.search_results = results;
                    st.editor.search_index = idx;
                    status
                }
            },
        }
    };
    ui.set_search_status(status.into());
}

/// Recalcula los resultados para `query` y selecciona la coincidencia más cercana al
/// cursor actual: usar solo cuando cambia la consulta/opciones o en F3/Enter (D1). Tras
/// una edición del documento con el panel abierto, usar `refresh_after_text_change`
/// (que solo recomputa, sin robar la selección/cursor recién tecleados).
pub fn do_search(ui: &App, state: &SharedState, query: &str) {
    compute_search(ui, state, query);
    if !state.borrow().editor.search_results.is_empty() {
        select_current_result(ui, state);
    }
}

fn select_current_result(ui: &App, state: &SharedState) {
    let selected = {
        let mut st = state.borrow_mut();
        let idx = st.editor.search_index;
        let ag = st.editor.active_group;
        match st.editor.search_results.get(idx).cloned() {
            Some(r) => {
                if let Some(tab) = st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
                    tab.cursor = r.end;
                }
                Some((r.start, r.end))
            }
            None => None,
        }
    };
    if let Some((start, end)) = selected {
        ui.invoke_select_range(start as i32, end as i32);
        state.borrow_mut().anchor = start;
    }
}

fn update_search_status(ui: &App, state: &SharedState) {
    let st = state.borrow();
    let status = if st.editor.search_results.is_empty() {
        "Sin resultados".to_string()
    } else {
        format!("{} de {}", st.editor.search_index + 1, st.editor.search_results.len())
    };
    drop(st);
    ui.set_search_status(status.into());
}

fn find_next(ui: &App, state: &SharedState) {
    // Hallazgo W10: recomputar sobre el documento actual antes de navegar. Los
    // resultados cacheados pueden quedar obsoletos (offsets de OTRA pestaña/documento)
    // tras cambiar de pestaña/grupo, deshacer/rehacer, una operación de línea, formatear
    // o una recarga externa, mientras el panel de búsqueda seguía abierto.
    {
        let query = ui.get_search_query().to_string();
        compute_search(ui, state, &query);
    }
    {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        let cursor = st
            .editor
            .groups
            .get(ag)
            .and_then(|g| g.active_tab())
            .map(|t| t.cursor)
            .unwrap_or(0);
        let anchor = st.anchor;
        let (sel_start, sel_end) = (anchor.min(cursor), anchor.max(cursor));
        let results = &st.editor.search_results;
        if !results.is_empty() {
            // D3: navegar desde el final de la selección actual (índice, no búsqueda
            // lineal repetida), no solo desde el cursor.
            st.editor.search_index = search::next_index(results, sel_start, sel_end);
        }
    }
    if !state.borrow().editor.search_results.is_empty() {
        select_current_result(ui, state);
        update_search_status(ui, state);
    }
}

fn find_prev(ui: &App, state: &SharedState) {
    // Hallazgo W10: ver comentario equivalente en `find_next`.
    {
        let query = ui.get_search_query().to_string();
        compute_search(ui, state, &query);
    }
    {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        let cursor = st
            .editor
            .groups
            .get(ag)
            .and_then(|g| g.active_tab())
            .map(|t| t.cursor)
            .unwrap_or(0);
        let anchor = st.anchor;
        let sel_start = anchor.min(cursor);
        let results = &st.editor.search_results;
        if !results.is_empty() {
            st.editor.search_index = search::prev_index(results, sel_start);
        }
    }
    if !state.borrow().editor.search_results.is_empty() {
        select_current_result(ui, state);
        update_search_status(ui, state);
    }
}

fn replace_one(ui: &App, state: &SharedState) {
    let query = ui.get_search_query().to_string();
    let replacement = ui.get_replace_query().to_string();
    let opts = options_from_ui(ui);
    // Recomputar sobre el documento actual antes de buscar la coincidencia seleccionada
    // (hallazgo D2): los resultados cacheados pueden estar obsoletos tras cambiar de
    // pestaña/grupo, deshacer o cualquier otra edición desde el último recómputo.
    compute_search(ui, state, &query);
    let (sel_lo, sel_hi) = {
        let st = state.borrow();
        let ag = st.editor.active_group;
        let cursor = st
            .editor
            .groups
            .get(ag)
            .and_then(|g| g.active_tab())
            .map(|t| t.cursor)
            .unwrap_or(0);
        let anchor = st.anchor;
        (anchor.min(cursor), anchor.max(cursor))
    };
    let did = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        // Hallazgo W4: reemplazar la coincidencia REALMENTE SELECCIONADA (anchor..cursor),
        // no `search_results[search_index]` — tras seleccionar una coincidencia el cursor
        // queda en `r.end`, así que `nearest_index(cursor)` apuntaba a la coincidencia
        // SIGUIENTE (k+1), no a la resaltada (k).
        let result = search::find_selected_result(&st.editor.search_results, sel_lo, sel_hi)
            .and_then(|idx| st.editor.search_results.get(idx).cloned());
        match result {
            Some(r) => match st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
                Some(tab) => {
                    let mut rope = tab.document.clone();
                    if r.start > rope.len() || r.end > rope.len() {
                        false
                    } else {
                        // D5/W4: en modo regex expande `$1`/`$2`... contra los grupos
                        // capturados del resultado; el nuevo cursor se calcula con la
                        // longitud del texto YA expandido, no la del patrón `$n` literal.
                        let expanded = search::expand_replacement(&rope.to_string(), &query, opts, &replacement, &r);
                        search::replace_in_rope_expand(&mut rope, std::slice::from_ref(&r), &replacement, false, &query, opts);
                        let new_cursor = r.start + expanded.len();
                        tab.document = rope;
                        tab.cursor = new_cursor;
                        tab.record_command(new_cursor);
                        true
                    }
                }
                None => false,
            },
            None => false,
        }
    };
    if did {
        push_active_document_to_ui(ui, state);
        set_status(ui, "Reemplazado");
        refresh_ui(ui, state);
        do_search(ui, state, &query);
    } else if !state.borrow().editor.search_results.is_empty() {
        // Sin selección exacta sobre ninguna coincidencia (p.ej. justo tras abrir el panel
        // o tras editar el documento): seleccionar la más cercana en vez de no hacer nada,
        // para que la siguiente pulsación de "Reemplazar" sí actúe sobre ella.
        select_current_result(ui, state);
        update_search_status(ui, state);
    }
}

fn replace_all(ui: &App, state: &SharedState) {
    let query = ui.get_search_query().to_string();
    let replacement = ui.get_replace_query().to_string();
    let opts = options_from_ui(ui);
    let count = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        // Recalcular sobre el documento actual, no sobre `search_results` cacheados
        // (hallazgo D2): evita `rope.remove` fuera de rango tras cambios previos.
        let rope = st.editor.groups.get(ag).and_then(|g| g.active_tab()).map(|t| t.document.clone());
        match rope {
            // Hallazgo C1: sin el tope de `MAX_SEARCH_RESULTS` — "Reemplazar todo" debe
            // afectar a TODAS las coincidencias, no solo a las primeras 10000. Hallazgo
            // C4: incluye también las coincidencias de ancho cero (p.ej. `^` para
            // insertar un prefijo en cada línea), que la búsqueda de navegación omite a
            // propósito porque no hay nada que resaltar, pero que un reemplazo sí debe
            // aplicar.
            Some(rope) => match search::search_in_rope_for_replace_all(&rope, &query, opts) {
                Ok(results) if !results.is_empty() => {
                    match st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
                        Some(tab) => {
                            let mut new_rope = tab.document.clone();
                            let n = search::replace_in_rope_expand(&mut new_rope, &results, &replacement, true, &query, opts);
                            tab.document = new_rope;
                            tab.cursor = 0;
                            tab.record_command(0);
                            n
                        }
                        None => 0,
                    }
                }
                _ => 0,
            },
            None => 0,
        }
    };
    if count > 0 {
        push_active_document_to_ui(ui, state);
        set_status(ui, &format!("{} reemplazos", count));
        refresh_ui(ui, state);
        do_search(ui, state, &query);
    } else {
        set_status(ui, "Sin resultados");
    }
}

/// Llamado desde `edit_ops` tras cada edición de texto: si el panel de búsqueda está
/// abierto, marca los resultados como obsoletos (hallazgo #15/C13: la búsqueda debe
/// seguir siendo válida tras editar, no quedarse con offsets obsoletos) para que
/// `search_tick` los recalcule en el siguiente tick del timer de 150 ms.
///
/// Hallazgo C13: antes esto llamaba a `compute_search` de forma SÍNCRONA en CADA tecla
/// del DOCUMENTO (no solo del campo de búsqueda) mientras el panel estuviera abierto —
/// el mismo coste de `search_in_rope` sobre todo el documento que el hallazgo W5 ya evita
/// para el campo de búsqueda, pero sin cubrir la edición del documento en sí. Ahora solo
/// marca `search_stale`; `search_tick` (llamado desde el timer, igual que con
/// `search_dirty`) es quien recomputa, y SIN mover la selección (D1): cada tecla en el
/// documento no debe robarle el cursor/selección recién movidos al propio tecleo.
pub fn refresh_after_text_change(ui: &App, state: &SharedState) {
    if ui.get_show_search() {
        state.borrow_mut().search_stale = true;
    }
}

/// Llamado desde el timer de 150ms en `main.rs` (hallazgo W5): si el campo de búsqueda
/// cambió desde el último tick (`search_dirty`), recalcula los resultados ahora, en vez
/// de en cada tecla. Selecciona la coincidencia más cercana, igual que `do_search`
/// (que sigue usándose para los eventos puntuales: abrir el panel, F3/Shift+F3, tras un
/// reemplazo).
pub fn search_tick(ui: &App, state: &SharedState) {
    let dirty = std::mem::replace(&mut state.borrow_mut().search_dirty, false);
    // Hallazgo C13: recómputo pendiente por edición del DOCUMENTO (no del campo de
    // búsqueda) — se consume por separado de `dirty` porque, a diferencia de este, NO
    // debe mover la selección (el cursor recién tecleado es el del propio documento).
    let stale = std::mem::replace(&mut state.borrow_mut().search_stale, false);
    if !ui.get_show_search() {
        return;
    }
    if dirty {
        let query = ui.get_search_query().to_string();
        do_search(ui, state, &query);
    } else if stale {
        let query = ui.get_search_query().to_string();
        compute_search(ui, state, &query);
    }
}
