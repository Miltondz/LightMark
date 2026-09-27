//! Vista: modos de vista (editor/dividido/preview), zoom, pantalla completa, navegador,
//! copiar vista previa, "ir a línea", navegación de pestañas/grupos, y el cableado de
//! `activate-tab` (ver hallazgo #19 / requisito de arquitectura #1: un clic de pestaña
//! debe pasar siempre por Rust para no desincronizar `editor_text`).

use super::{activate_tab, refresh_flags, refresh_preview_if_visible, set_status, SharedState};
use crate::App;
use slint::ComponentHandle;

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_activate_tab(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                let group = state.borrow().editor.active_group;
                activate_tab(&ui, &state, group, index.max(0) as usize);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_set_view_mode(move |mode| {
            if let Some(ui) = ui_weak.upgrade() {
                set_view_mode(&ui, &state, mode);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_toggle_preview(move || {
            if let Some(ui) = ui_weak.upgrade() {
                // Hallazgo W26: con un lenguaje que no soporta vista previa, `Ctrl+M`
                // igualmente cambiaba `current_view_mode` a 1 (aunque `refresh_flags`
                // lo forzara de vuelta a 0 en el siguiente refresco) — un estado oculto
                // que no se veía pero podía quedar inconsistente. Ahora es un no-op con
                // aviso, igual que el resto de controles de vista previa cuando no está
                // disponible (p.ej. el botón "Abrir en navegador").
                if !ui.get_preview_available() {
                    set_status(&ui, "La vista previa no está disponible para este lenguaje");
                    return;
                }
                let current = state.borrow().current_view_mode;
                let next = match current {
                    0 => 1,
                    _ => 0,
                };
                set_view_mode(&ui, &state, next);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_open_browser(move || {
            if let Some(ui) = ui_weak.upgrade() {
                open_browser(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_copy_preview(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let text = ui.get_preview_text().to_string();
                match arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                    Ok(_) => set_status(&ui, "Vista previa copiada al portapapeles"),
                    Err(_) => set_status(&ui, "Error al copiar al portapapeles"),
                }
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_zoom_in(move || {
            if let Some(ui) = ui_weak.upgrade() {
                zoom(&ui, &state, 2);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_zoom_out(move || {
            if let Some(ui) = ui_weak.upgrade() {
                zoom(&ui, &state, -2);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_zoom_reset(move || {
            if let Some(ui) = ui_weak.upgrade() {
                set_font_size(&ui, &state, 14);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_fullscreen(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_fullscreen(!ui.get_is_fullscreen());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_goto_line(move |s| {
            if let Some(ui) = ui_weak.upgrade() {
                goto_line(&ui, &state, s.as_str());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_next_tab(move || {
            if let Some(ui) = ui_weak.upgrade() {
                step_tab(&ui, &state, 1);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_prev_tab(move || {
            if let Some(ui) = ui_weak.upgrade() {
                step_tab(&ui, &state, -1);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_switch_group(move |g| {
            if let Some(ui) = ui_weak.upgrade() {
                let g = (g.max(0) as usize).min(3);
                let index = {
                    let mut st = state.borrow_mut();
                    if g >= st.editor.groups.len() {
                        // Crea el grupo bajo demanda (hasta 4, ver Alt+1..4). Hallazgo
                        // E9: el título usa el próximo número "Sin título N" realmente
                        // libre en toda la app, no un "Sin título 1" fijo que podía
                        // duplicar el de un borrador ya abierto en otro grupo.
                        while st.editor.groups.len() <= g {
                            let n = st.editor.smallest_unused_scratch_number();
                            let id = st.editor.scratch_manager.next_id_public();
                            st.editor.groups.push(crate::editor::Group {
                                tabs: vec![crate::editor::Tab::new_scratch(id, ropey::Rope::new(), format!("Sin título {}", n))],
                                active: 0,
                            });
                        }
                    }
                    st.editor.groups[g].active
                };
                activate_tab(&ui, &state, g, index);
            }
        });
    }
}

/// Cambia el modo de vista EN VIVO (Ctrl+M, botones segmentados, menú Ver). A propósito
/// no toca `settings.view_mode` ni persiste nada (hallazgo E4): ese campo es solo la
/// preferencia de arranque, que el usuario cambia explícitamente desde "Configuración"
/// (ver `settings_ops::apply_settings_from_ui`). Si se guardara aquí, cualquier
/// `settings-changed()` posterior (p.ej. alternar la barra de herramientas) releería
/// `default_view_mode` desde la UI y devolvería silenciosamente el modo de vista actual
/// al que estuviera puesto por defecto.
fn set_view_mode(ui: &App, state: &SharedState, mode: i32) {
    {
        let mut st = state.borrow_mut();
        st.current_view_mode = mode.clamp(0, 2);
    }
    refresh_flags(ui, state);
    refresh_preview_if_visible(ui, state);
}

fn open_browser(ui: &App, state: &SharedState) {
    let (text, lang) = {
        let st = state.borrow();
        match st.editor.active_tab() {
            Some(t) => (t.document.to_string(), st.editor.language_of(t)),
            None => return,
        }
    };
    if !crate::preview::is_preview_available(&lang) {
        set_status(ui, "La vista previa no está disponible para este lenguaje");
        return;
    }
    let html = crate::markdown::generate_standalone_html(&text, &lang);
    let path = std::env::temp_dir().join("lightmark-preview.html");
    match std::fs::write(&path, html) {
        Ok(_) => {
            let result = std::process::Command::new("cmd")
                .args(["/c", "start", "", &path.to_string_lossy()])
                .spawn();
            match result {
                Ok(_) => set_status(ui, "Vista previa abierta en el navegador"),
                Err(e) => set_status(ui, &format!("No se pudo abrir el navegador: {}", e)),
            }
        }
        Err(e) => set_status(ui, &format!("No se pudo escribir la vista previa temporal: {}", e)),
    }
}

fn zoom(ui: &App, state: &SharedState, delta: i32) {
    let current = state.borrow().settings.font_size as i32;
    set_font_size(ui, state, (current + delta).clamp(8, 40) as u32);
}

fn set_font_size(ui: &App, state: &SharedState, size: u32) {
    {
        let mut st = state.borrow_mut();
        st.settings.font_size = size;
    }
    // Bug W3: guardar directamente con `Settings::save()` aquí dejaba la copia
    // compartida del registro (`windows::shared_settings`) y las demás ventanas
    // desincronizadas del zoom aplicado en esta — usar el punto único de escritura.
    super::settings_ops::commit_settings(state);
    ui.set_font_size(size as i32);
    ui.set_zoom_level(size as f32 / 14.0);
    // Hallazgo W21: la vista previa no se refrescaba al hacer zoom (Ctrl+=/Ctrl+-/
    // Ctrl+0), así que el tamaño de fuente ahí quedaba desincronizado del editor hasta
    // el siguiente cambio de documento.
    refresh_preview_if_visible(ui, state);
}

fn goto_line(ui: &App, state: &SharedState, input: &str) {
    let Ok(target) = input.trim().parse::<usize>() else {
        set_status(ui, "Número de línea no válido");
        return;
    };
    let offset = {
        let mut st = state.borrow_mut();
        let ag = st.editor.active_group;
        match st.editor.groups.get_mut(ag).and_then(|g| g.active_tab_mut()) {
            Some(tab) => {
                let text = tab.document.to_string();
                let offset = crate::textops::line_start_offset(&text, target);
                tab.cursor = offset;
                Some(offset)
            }
            None => None,
        }
    };
    if let Some(offset) = offset {
        let o = offset as i32;
        ui.invoke_select_range(o, o);
        ui.invoke_focus_editor();
        state.borrow_mut().anchor = offset;
        set_status(ui, &format!("Línea {}", target));
    }
}

fn step_tab(ui: &App, state: &SharedState, delta: i32) {
    let (group, next) = {
        let st = state.borrow();
        let ag = st.editor.active_group;
        let Some(g) = st.editor.groups.get(ag) else { return };
        let len = g.tabs.len() as i32;
        if len == 0 {
            return;
        }
        let next = ((g.active as i32 + delta) % len + len) % len;
        (ag, next as usize)
    };
    activate_tab(ui, state, group, next);
}
