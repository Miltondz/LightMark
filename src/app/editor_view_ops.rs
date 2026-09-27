//! Estado y operaciones de la vista del editor (WS-A): resaltado por overlay y gutter
//! con filas visuales. Wave 0: estado vacío y hooks sin comportamiento.
#![allow(dead_code)] // wave-1 fills (WS-A)

use super::SharedState;
use crate::App;

/// Estado de vista por ventana (`AppState.view`).
#[derive(Default)]
pub struct EditorViewState {
    pub layout: crate::editor_view::RowLayout,
    pub metrics: Option<crate::editor_view::Metrics>,
}

pub fn wire(_ui: &App, _state: &SharedState) {
    // wave-1 fills: `editor-viewport-changed` -> render.
}

/// Tras sustituir el documento activo (cambio de pestaña, abrir, recarga...).
pub fn on_document_replaced(_ui: &App, _state: &SharedState) {}

/// Tras aplicar una edición al `Rope`: `start` = offset en bytes del primer cambio.
pub fn on_edit(_ui: &App, _state: &SharedState, _start_byte: usize, _removed: usize, _inserted: usize) {}

/// Tras aplicar la configuración (fuente, ajuste de línea, resaltado...).
pub fn on_settings_changed(_ui: &App, _state: &SharedState) {}
