//! Configuración: lee `settings-changed()` desde la UI (el diálogo ya escribió los
//! valores en las propiedades two-way de `App`), los valida y persiste, y los vuelve a
//! aplicar (los valores saturados pueden diferir de lo que tecleó el usuario). También
//! "Abrir carpeta de datos", "Restablecer valores" y la ayuda de autoguardado.

use super::{refresh_flags, refresh_preview_if_visible, refresh_stats, set_status, SharedState};
use crate::settings::Settings;
use crate::App;
use slint::ComponentHandle;

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_settings_changed(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_settings_from_ui(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_open_data_folder(move || {
            if let Some(ui) = ui_weak.upgrade() {
                open_data_folder(&ui);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_open_app_folder(move || {
            if let Some(ui) = ui_weak.upgrade() {
                open_data_folder(&ui);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_reset_settings(move || {
            if let Some(ui) = ui_weak.upgrade() {
                reset_settings(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_show_scratch_help(move || {
            if let Some(ui) = ui_weak.upgrade() {
                set_status(
                    &ui,
                    "Los borradores se autoguardan solos en %LOCALAPPDATA%\\LightMark\\scratch. Los archivos reales solo se guardan con Ctrl+S, salvo que actives \"Guardar archivos automáticamente\" en Configuración.",
                );
            }
        });
    }
}

fn apply_settings_from_ui(ui: &App, state: &SharedState) {
    {
        let mut st = state.borrow_mut();
        let s = &mut st.settings;
        s.font_family = ui.get_font_family().to_string();
        s.font_size = ui.get_font_size().max(0) as u32;
        s.tab_size = ui.get_tab_size().max(0) as u32;
        s.insert_spaces = ui.get_insert_spaces();
        s.word_wrap = ui.get_word_wrap();
        s.show_line_numbers = ui.get_show_line_numbers();
        s.autosave_delay_ms = ui.get_autosave_delay_ms().max(0) as u32;
        s.autosave_files = ui.get_autosave_files();
        s.restore_session = ui.get_restore_session();
        s.view_mode = ui.get_default_view_mode().clamp(0, 2) as u32;
        s.show_toolbar = ui.get_show_toolbar();
        s.show_status_bar = ui.get_show_status_bar();
        s.show_explorer = ui.get_show_explorer();
        // M5: antes `preview_selectable` no se leía/escribía nunca aquí, así que el
        // campo persistido en `settings.json` quedaba muerto y la UI arrancaba siempre
        // en `true` sin importar lo guardado.
        s.preview_selectable = ui.get_preview_selectable();
        s.theme = ui.get_theme().max(0) as u32;
        st.settings = st.settings.clone().validated();
        st.settings.save().ok();
    }
    push_settings_to_ui(ui, state);
    refresh_flags(ui, state);
    refresh_stats(ui, state, true);
    // Hallazgo W21: si el tamaño de fuente cambió desde el diálogo de Configuración (no
    // solo con los botones de zoom), la vista previa debe reflejarlo también.
    refresh_preview_if_visible(ui, state);
    // V26: `settings-changed()` también lo disparan los toggles rápidos de la barra de
    // herramientas / menú Ver (Explorador, ajuste de línea, checkables...) que persisten
    // en segundo plano sin abrir el diálogo; mostrar "Configuración guardada" en CADA
    // uno de esos parpadeaba en la barra de estado. Solo se muestra cuando el cambio
    // viene realmente del diálogo de Configuración (abierto en ese momento).
    if ui.get_show_settings() {
        set_status(ui, "Configuración guardada");
    }
}

/// Empuja `state.settings` a las propiedades de la UI (usado tras cargar/restablecer
/// configuración, y para reflejar el valor ya saturado tras `settings-changed`).
pub fn push_settings_to_ui(ui: &App, state: &SharedState) {
    let s = state.borrow().settings.clone();
    ui.set_font_family(s.font_family.into());
    ui.set_font_size(s.font_size as i32);
    ui.set_tab_size(s.tab_size as i32);
    ui.set_insert_spaces(s.insert_spaces);
    ui.set_word_wrap(s.word_wrap);
    ui.set_show_line_numbers(s.show_line_numbers);
    ui.set_autosave_delay_ms(s.autosave_delay_ms as i32);
    ui.set_autosave_files(s.autosave_files);
    ui.set_restore_session(s.restore_session);
    ui.set_default_view_mode(s.view_mode as i32);
    ui.set_show_toolbar(s.show_toolbar);
    ui.set_show_status_bar(s.show_status_bar);
    ui.set_show_explorer(s.show_explorer);
    ui.set_preview_selectable(s.preview_selectable);
    ui.set_theme(s.theme as i32);
    ui.set_zoom_level(s.font_size as f32 / 14.0);
}

fn reset_settings(ui: &App, state: &SharedState) {
    {
        let mut st = state.borrow_mut();
        let recent = st.settings.recent_files.clone();
        st.settings = Settings::default();
        st.settings.recent_files = recent;
        st.settings.save().ok();
        // También aplica el modo de vista por defecto restablecido al modo EN VIVO
        // (ver E4): "Restablecer valores" es una acción explícita del usuario, a
        // diferencia de un `settings-changed()` cualquiera.
        st.current_view_mode = st.settings.view_mode as i32;
    }
    push_settings_to_ui(ui, state);
    refresh_flags(ui, state);
    refresh_stats(ui, state, true);
    set_status(ui, "Configuración restablecida");
}

fn open_data_folder(ui: &App) {
    let dir = crate::scratch::get_app_data_dir();
    std::fs::create_dir_all(&dir).ok();
    match std::process::Command::new("explorer").arg(&dir).spawn() {
        Ok(_) => set_status(ui, "Carpeta de datos abierta"),
        Err(e) => set_status(ui, &format!("No se pudo abrir la carpeta de datos: {}", e)),
    }
}
