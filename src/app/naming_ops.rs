//! Nombres de borradores (WS-B; D2 extiende). Wave 0: hook sin comportamiento.
#![allow(dead_code)] // wave-1 fills (WS-B)

use super::SharedState;
use crate::App;

/// Llamado desde el temporizador de 150 ms tras `refresh_on_tick`.
pub fn on_tick(_ui: &App, _state: &SharedState) {
    // wave-1 fills (WS-B): recomputar título automático del borrador activo.
    // D2: auto AI naming hook
}
