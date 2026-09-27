//! Nombres de borradores (WS-B; D2 extiende): recalcula títulos automáticos y números
//! "Sin título N" de todos los borradores en el temporizador de 150 ms. Es barato (lee como
//! mucho ~4 KB por borrador) y solo refresca la UI cuando algún título cambia.

use super::SharedState;
use crate::App;

/// Llamado desde el temporizador de 150 ms tras `refresh_on_tick`.
pub fn on_tick(ui: &App, state: &SharedState) {
    let changed = {
        let mut st = state.borrow_mut();
        let opts = crate::scratch::NamingOpts::from_settings(&st.settings);
        st.editor.recompute_titles(&opts)
    };
    if changed {
        super::refresh_tabs(ui, state);
    }
    // D2: auto AI naming hook
}
