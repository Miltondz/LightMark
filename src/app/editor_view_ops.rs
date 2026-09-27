//! Estado y operaciones de la vista del editor (WS-A): resaltado por overlay y gutter
//! con filas visuales estilo Sublime (número solo en la primera fila de cada línea lógica).
#![allow(dead_code)]

use super::SharedState;
use crate::editor_view::{Metrics, RowLayout};
use crate::syntax::{self, Lang, LineState};
use crate::App;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

/// Límites de §1: por encima de esto el resaltado se desactiva (el gutter/wrap sigue activo).
const MAX_HIGHLIGHT_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINE_CHARS: usize = 10_000;
/// Filas de margen por encima/debajo de la ventana visible que también se materializan.
const ROW_MARGIN: i32 = 20;
/// Margen horizontal (px) fuera del viewport para el que aún se generan piezas del overlay.
const H_CULL_MARGIN: f32 = 256.0;

/// Estado de vista por ventana (`AppState.view`).
pub struct EditorViewState {
    pub metrics: Option<Metrics>,
    pub layout: RowLayout,
    pub lang: Lang,
    pub hl_enabled: bool,
    /// Estado del tokenizador al INICIO de cada línea (longitud = nº de líneas del documento).
    pub line_states: Vec<LineState>,
    /// Cuántas entradas de `line_states` son ya válidas (a partir del principio).
    pub states_valid_upto: usize,
    /// Identifica la pestaña activa actualmente representada (grupo, índice) para no
    /// mezclar el estado de una pestaña con el de otra al cambiar.
    pub doc_key: (usize, usize),
    last_render_ok: bool,
    disabled_reason: Option<&'static str>,
}

impl Default for EditorViewState {
    fn default() -> Self {
        EditorViewState {
            metrics: None,
            layout: RowLayout::default(),
            lang: Lang::Plain,
            hl_enabled: false,
            line_states: Vec::new(),
            states_valid_upto: 0,
            doc_key: (usize::MAX, usize::MAX),
            last_render_ok: false,
            disabled_reason: None,
        }
    }
}

pub fn wire(ui: &App, state: &SharedState) {
    // Disparado por `CodeEditor.viewport-changed` (scroll, redimensionado, cambio de fuente
    // o de ajuste de línea): solo necesita volver a materializar la ventana visible.
    let weak = ui.as_weak();
    let state = state.clone();
    ui.on_editor_viewport_changed(move || {
        if let Some(ui) = weak.upgrade() {
            on_viewport_changed(&ui, &state);
        }
    });
}

fn measure_closure(ui: &App) -> impl FnMut(char) -> f32 + '_ {
    move |c: char| {
        let s: String = std::iter::repeat_n(c, 8).collect();
        ui.set_editor_measure_text(s.into());
        ui.get_editor_measure_width() / 8.0
    }
}

/// Reconstruye `metrics` si cambió la fuente/tamaño; siempre re-mide `tab_adv` porque puede
/// cambiar independientemente si la familia cambia (glifo de tabulador ~ancho de espacio).
fn ensure_metrics(ui: &App, view: &mut EditorViewState) -> bool {
    let family = ui.get_font_family().to_string();
    let size_px = ui.get_font_size() as f32;
    let need_init = match &view.metrics {
        Some(m) => !m.key_matches(&family, size_px),
        None => true,
    };
    if need_init {
        let mut m = Metrics::default();
        {
            let mut meas = measure_closure(ui);
            m.init_ascii(&family, size_px, &mut meas);
            ui.set_editor_measure_text("a\ta".into());
            let a = ui.get_editor_measure_width();
            ui.set_editor_measure_text("aa".into());
            let b = ui.get_editor_measure_width();
            m.tab_adv = (a - b).max(1.0);
        }
        view.metrics = Some(m);
        true
    } else {
        false
    }
}

fn active_document(state: &SharedState) -> Option<(String, usize, usize)> {
    let st = state.borrow();
    let ag = st.editor.active_group;
    let idx = st.editor.groups.get(ag)?.active;
    let tab = st.editor.groups.get(ag)?.tabs.get(idx)?;
    Some((tab.document.to_string(), ag, idx))
}

/// Divide `text` en sus líneas lógicas SIN el separador, igual que `textops::line_count`
/// (un `\n` final cuenta como una línea vacía adicional).
fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn decide_hl_enabled(text: &str, lang: Lang) -> (bool, Option<&'static str>) {
    if lang == Lang::Plain {
        return (false, None);
    }
    if text.len() > MAX_HIGHLIGHT_BYTES {
        return (false, Some("Resaltado desactivado: archivo grande"));
    }
    if text.lines().any(|l| l.len() > MAX_LINE_CHARS) {
        return (false, Some("Resaltado desactivado: línea demasiado larga"));
    }
    (true, None)
}

/// Tras sustituir el documento activo (cambio de pestaña, abrir, recarga...).
pub fn on_document_replaced(ui: &App, state: &SharedState) {
    let Some((text, ag, idx)) = active_document(state) else {
        return;
    };
    let path_lang = {
        let st = state.borrow();
        st.editor
            .groups
            .get(ag)
            .and_then(|g| g.tabs.get(idx))
            .map(|t| st.editor.language_of(t))
            .unwrap_or_else(|| "plaintext".to_string())
    };
    let setting_on = state.borrow().settings.syntax_highlighting;
    let lang = syntax::lang_from_id(&path_lang);
    let mut view = std::mem::take(&mut state.borrow_mut().view);
    view.doc_key = (ag, idx);
    view.lang = lang;
    let (mut enabled, reason) = decide_hl_enabled(&text, lang);
    enabled = enabled && setting_on;
    view.hl_enabled = enabled;
    view.disabled_reason = if setting_on { reason } else { None };
    ensure_metrics(ui, &mut view);
    let wrap = ui.get_word_wrap();
    let wrap_width = ui.get_editor_wrap_width() as f32;
    view.layout.wrap = wrap;
    view.layout.wrap_width = wrap_width;
    {
        let lines = split_lines(&text);
        let mut meas = measure_closure(ui);
        let m = view.metrics.as_mut().unwrap();
        view.layout.rebuild(&lines, m, &mut meas);
        view.line_states = vec![0; lines.len()];
    }
    view.states_valid_upto = 0;
    state.borrow_mut().view = view;
    render(ui, state);
}

/// Índice (0-based) de la línea que contiene `start_byte` en el texto ACTUAL (tras la
/// edición). `start_byte` es el prefijo común entre el texto viejo y el nuevo (ver
/// `apply_text_diff`/`TextDiff::start`), así que es un offset válido en AMBOS — de ahí que
/// contar los `\n` hasta ahí sea correcto sin necesitar el texto anterior. Todo lo que venga
/// después de esta línea se invalida (estado del tokenizador y layout de filas se recalculan
/// de todos modos para toda la ventana visible en `render`).
fn first_changed_line(text_after: &str, start_byte: usize) -> usize {
    text_after[..start_byte.min(text_after.len())].matches('\n').count()
}

/// Tras aplicar una edición al `Rope`: `start` = offset en bytes del primer cambio.
/// Debe ejecutarse de forma síncrona (mismo frame) para que el overlay no quede desincronizado.
pub fn on_edit(ui: &App, state: &SharedState, start_byte: usize, removed: usize, inserted: usize) {
    let Some((text, ag, idx)) = active_document(state) else {
        return;
    };
    let mut view = std::mem::take(&mut state.borrow_mut().view);
    if view.doc_key != (ag, idx) {
        // Cambio de pestaña sin pasar por `on_document_replaced` (no debería ocurrir, pero
        // por seguridad recae en una reconstrucción completa).
        state.borrow_mut().view = view;
        on_document_replaced(ui, state);
        return;
    }
    let lines = split_lines(&text);
    let first_line = first_changed_line(&text, start_byte).min(lines.len().saturating_sub(1));
    let _ = (removed, inserted); // el estado se invalida desde `first_line`; no se necesita el recuento exacto

    ensure_layout_current(ui, &mut view, &text, &lines);

    // Recalcular si el resaltado debe estar activo (el tamaño puede haber cruzado el límite).
    let setting_on = state.borrow().settings.syntax_highlighting;
    let (mut enabled, reason) = decide_hl_enabled(&text, view.lang);
    enabled = enabled && setting_on;
    view.hl_enabled = enabled;
    view.disabled_reason = if setting_on { reason } else { None };

    // El estado de tokenizador puede haber cambiado a partir de la línea editada: se invalida
    // desde ahí (se re-tokeniza perezosamente en `render`, solo para la ventana visible).
    view.states_valid_upto = view.states_valid_upto.min(first_line);
    if view.line_states.len() != lines.len() {
        view.line_states.resize(lines.len(), 0);
    }
    state.borrow_mut().view = view;
    render(ui, state);
}

/// Tras aplicar la configuración (fuente, ajuste de línea, resaltado...).
pub fn on_settings_changed(ui: &App, state: &SharedState) {
    on_document_replaced(ui, state);
}

/// Reconstruye `view.layout` si cambió el ajuste de línea, el ancho de ajuste o la fuente
/// (métricas) desde la última vez. Común a `on_edit` y `on_viewport_changed`: un
/// redimensionado/maximizado de ventana, o el primer layout tras abrir un archivo (cuando el
/// ancho real del `TextInput` aún no se conocía), deben re-envolver el texto exactamente igual
/// que hace `on_edit` tras una edición — si no, el overlay (que usa `view.layout`/`wrap_line`
/// para posicionar sus `Text`) queda desalineado del `TextInput` real, que sí se re-envuelve
/// solo automáticamente al cambiar de ancho.
fn ensure_layout_current(ui: &App, view: &mut EditorViewState, text: &str, lines: &[&str]) {
    let wrap = ui.get_word_wrap();
    let wrap_width = ui.get_editor_wrap_width() as f32;
    let metrics_changed = ensure_metrics(ui, view);
    let wrap_changed = view.layout.wrap != wrap || (view.layout.wrap_width - wrap_width).abs() > 0.5;
    let count_changed = view.layout.rows_per_line.len() != lines.len();
    if wrap_changed || metrics_changed || count_changed {
        view.layout.wrap = wrap;
        view.layout.wrap_width = wrap_width;
        if view.layout.wrap || text.len() <= MAX_HIGHLIGHT_BYTES {
            let mut meas = measure_closure(ui);
            let m = view.metrics.as_mut().unwrap();
            view.layout.rebuild(lines, m, &mut meas);
        } else {
            // Documento grande sin ajuste de línea: una fila por línea, sin medir carácter a
            // carácter (el resaltado también está desactivado por tamaño en este caso).
            view.layout.rows_per_line = vec![1; lines.len()];
            view.layout.row_prefix = (0..=lines.len() as u32).collect();
        }
    }
}

/// Llamado por `editor-viewport-changed` (scroll/redimensionado/cambio de fuente/ajuste de
/// línea en Slint).
pub fn on_viewport_changed(ui: &App, state: &SharedState) {
    let Some((text, ag, idx)) = active_document(state) else {
        return;
    };
    let mut view = std::mem::take(&mut state.borrow_mut().view);
    if view.doc_key != (ag, idx) {
        state.borrow_mut().view = view;
        on_document_replaced(ui, state);
        return;
    }
    let lines = split_lines(&text);
    ensure_layout_current(ui, &mut view, &text, &lines);
    state.borrow_mut().view = view;
    render(ui, state);
}

fn status_message_for(view: &EditorViewState) -> Option<&'static str> {
    view.disabled_reason
}

/// Vuelve a tokenizar las líneas `[from, upto)` encadenando el estado, y guarda el estado de
/// FIN de cada línea en `line_states[i+1]` (position i = estado de INICIO de la línea i;
/// `line_states.len() == num_lines`, así que el estado final de la última línea no se guarda
/// en ningún sitio adicional — no se necesita).
fn ensure_states_upto(view: &mut EditorViewState, lines: &[&str], upto: usize) {
    let upto = upto.min(lines.len());
    if syntax::is_stateless(view.lang) {
        view.states_valid_upto = lines.len();
        return;
    }
    let mut i = view.states_valid_upto.min(lines.len());
    let mut st = if i == 0 { 0 } else { view.line_states.get(i).copied().unwrap_or(0) };
    // Nota: `line_states[i]` es el estado de INICIO de la línea i; para reanudar necesitamos el
    // estado de inicio de `states_valid_upto`, que ya está guardado ahí mismo.
    let mut scratch = Vec::new();
    while i < upto {
        scratch.clear();
        let end_state = syntax::tokenize_line(view.lang, lines[i], st, &mut scratch);
        st = end_state;
        i += 1;
        if i < view.line_states.len() {
            view.line_states[i] = st;
        }
    }
    view.states_valid_upto = view.states_valid_upto.max(i);
}

/// Renderiza el gutter + overlay de resaltado para la ventana visible actual. No hace nada si
/// no hay ninguna pestaña activa.
pub fn render(ui: &App, state: &SharedState) {
    let Some((text, ag, idx)) = active_document(state) else {
        return;
    };
    let mut view = std::mem::take(&mut state.borrow_mut().view);
    if view.doc_key != (ag, idx) {
        state.borrow_mut().view = view;
        on_document_replaced(ui, state);
        return;
    }
    let lines = split_lines(&text);
    let total_lines = lines.len().max(1);
    let digits = total_lines.to_string().len().max(1);
    ui.set_editor_gutter_width_sample("9".repeat(digits).into());

    let total_rows = view.layout.total_rows().max(1) as i32;
    let first_visible = ui.get_editor_first_visible_row().max(0);
    let visible_rows = ui.get_editor_visible_rows().max(1);
    let win_start = (first_visible - ROW_MARGIN).max(0);
    let win_end = (first_visible + visible_rows + ROW_MARGIN).min(total_rows);

    // --- Gutter: una entrada de texto por fila visual de la ventana, número solo en la
    // primera fila de cada línea lógica. ---
    let mut gutter = String::new();
    {
        let mut row = win_start;
        let mut first = true;
        while row < win_end {
            if !first {
                gutter.push('\n');
            }
            first = false;
            let (line, sub) = view.layout.line_of_row(row as u32);
            if sub == 0 {
                gutter.push_str(&(line + 1).to_string());
            }
            row += 1;
        }
    }
    ui.set_editor_gutter_text(gutter.into());
    ui.set_editor_gutter_first_row(win_start);

    // --- Overlay de resaltado ---
    if !view.hl_enabled {
        ui.set_editor_hl_active(false);
        if ui.get_editor_hl_spans().row_count() != 0 {
            ui.set_editor_hl_spans(ModelRc::new(VecModel::from(Vec::<crate::EditorSpan>::new())));
        }
    } else {
        ensure_metrics(ui, &mut view);
        let (win_start_line, _) = view.layout.line_of_row(win_start as u32);
        let (win_end_line, _) = view.layout.line_of_row((win_end.max(win_start + 1) - 1) as u32);
        let needed_upto = (win_end_line + 1).min(lines.len());
        ensure_states_upto(&mut view, &lines, needed_upto);

        let theme = state.borrow().settings.theme;
        let palette = syntax::palette(theme);
        let line_h = ui.get_editor_line_height();
        let scroll_x = if view.layout.wrap { 0.0 } else { ui.get_editor_scroll_x() as f32 };
        let view_w = ui.get_editor_view_width() as f32;

        let mut spans: Vec<crate::EditorSpan> = Vec::new();
        let mut toks = Vec::new();
        let mut meas = measure_closure(ui);
        let metrics = view.metrics.as_mut().unwrap();
        for line_idx in win_start_line..=win_end_line.min(lines.len().saturating_sub(1)) {
            let line = lines[line_idx];
            let start_state = view.line_states.get(line_idx).copied().unwrap_or(0);
            toks.clear();
            syntax::tokenize_line(view.lang, line, start_state, &mut toks);
            let line_first_row = view.layout.row_prefix.get(line_idx).copied().unwrap_or(0) as i32;
            let rows = crate::editor_view::wrap_line(
                line,
                if view.layout.wrap { view.layout.wrap_width } else { f32::INFINITY },
                metrics,
                &mut meas,
            );
            for (row_i, &(row_start, row_end)) in rows.iter().enumerate() {
                let visual_row = line_first_row + row_i as i32;
                if visual_row < win_start || visual_row >= win_end {
                    continue;
                }
                let y = (visual_row as f32) * line_h as f32;
                if !view.layout.wrap && view_w > 0.0 {
                    // Poda horizontal barata: solo materializar filas que puedan solaparse con
                    // el viewport ampliado en X (aproximación por nº de caracteres * avance
                    // ascii típico, sin medir); la poda fina por pieza ocurre dentro de la fila.
                    let approx_w = (row_end - row_start) as f32 * metrics.size_px.max(6.0) * 0.7;
                    if approx_w < scroll_x - H_CULL_MARGIN {
                        continue;
                    }
                }
                emit_row_spans(
                    line,
                    row_start,
                    row_end,
                    &toks,
                    &palette,
                    y,
                    metrics,
                    &mut meas,
                    &mut spans,
                    if view.layout.wrap { None } else { Some((scroll_x, view_w)) },
                );
            }
        }
        ui.set_editor_hl_spans(ModelRc::new(VecModel::from(spans)));
        ui.set_editor_hl_active(true);
    }

    if let Some(msg) = status_message_for(&view) {
        if !view.last_render_ok {
            super::set_status(ui, msg);
        }
        view.last_render_ok = true;
    } else {
        view.last_render_ok = false;
    }

    state.borrow_mut().view = view;
}

/// Genera las piezas de `EditorSpan` para una sola fila visual `[row_start, row_end)` (rango
/// de bytes dentro de `line`), fusionando caracteres consecutivos del mismo color en una sola
/// pieza y omitiendo el glifo de los tabuladores (solo se cuenta su avance en `x`), igual que
/// `TextInput` (que dibuja el tabulador como un único glifo del ancho de un espacio y no lo
/// pinta el overlay). `hcull`, si se da, es `(scroll_x, view_width)`: las piezas totalmente
/// fuera de `[scroll_x - margen, scroll_x + view_width + margen]` no se emiten.
fn emit_row_spans(
    line: &str,
    row_start: usize,
    row_end: usize,
    toks: &[(u32, u32, syntax::TokKind)],
    palette: &[slint::Color; syntax::TokKind::COUNT],
    y: f32,
    metrics: &mut Metrics,
    meas: &mut dyn FnMut(char) -> f32,
    spans: &mut Vec<crate::EditorSpan>,
    hcull: Option<(f32, f32)>,
) {
    let mut x = 0f32;
    let mut piece = String::new();
    let mut piece_x = 0f32;
    let mut piece_color: Option<slint::Color> = None;
    macro_rules! flush {
        () => {
            if !piece.is_empty() {
                let keep = match hcull {
                    Some((scroll_x, view_w)) => {
                        let lo = scroll_x - H_CULL_MARGIN;
                        let hi = scroll_x + view_w + H_CULL_MARGIN;
                        x >= lo && piece_x <= hi
                    }
                    None => true,
                };
                if keep {
                    spans.push(crate::EditorSpan {
                        x: piece_x,
                        y,
                        text: SharedString::from(piece.as_str()),
                        color: piece_color.unwrap_or(palette[syntax::TokKind::Text as usize]),
                    });
                }
                piece.clear();
            }
        };
    }
    for &(ts, te, kind) in toks {
        let (ts, te) = (ts as usize, te as usize);
        if te <= row_start || ts >= row_end {
            continue;
        }
        let s = ts.max(row_start);
        let e = te.min(row_end);
        let color = palette[kind as usize];
        for ch in line[s..e].chars() {
            if ch == '\t' {
                flush!();
                x += metrics.measure_char(ch, meas);
                continue;
            }
            if piece.is_empty() {
                piece_color = Some(color);
                piece_x = x;
            } else if piece_color != Some(color) {
                flush!();
                piece_color = Some(color);
                piece_x = x;
            }
            piece.push(ch);
            x += metrics.measure_char(ch, meas);
        }
    }
    flush!();
}
