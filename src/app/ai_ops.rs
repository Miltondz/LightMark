//! Cableado de las acciones de IA (WS-D). Wave 0: todos los callbacks muestran
//! "IA: no implementado".

use super::{set_status, SharedState};
use crate::App;
use slint::ComponentHandle;

const NOT_IMPLEMENTED: &str = "IA: no implementado";

pub fn wire(ui: &App, _state: &SharedState) {
    // wave-1 fills (WS-D): reemplazar cada stub por su implementación real.
    ui.on_ai_open_settings({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_show_ai_settings(true);
            }
        }
    });
    ui.on_ai_provider_changed({
        let ui_weak = ui.as_weak();
        move |_i| stub(&ui_weak)
    });
    ui.on_ai_refresh_models({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
    ui.on_ai_save_key({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
    ui.on_ai_delete_key({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
    ui.on_ai_test_connection({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
    ui.on_ai_suggest_name({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
    ui.on_ai_name_picked({
        let ui_weak = ui.as_weak();
        move |_s| stub(&ui_weak)
    });
    ui.on_ai_name_regenerate({
        let ui_weak = ui.as_weak();
        move || stub(&ui_weak)
    });
}

fn stub(ui_weak: &slint::Weak<App>) {
    if let Some(ui) = ui_weak.upgrade() {
        set_status(&ui, NOT_IMPLEMENTED);
    }
}
