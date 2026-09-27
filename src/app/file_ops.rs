//! Callbacks de archivo: nuevo, abrir, abrir reciente, guardar, guardar como, guardar
//! todo, exportar a ZIP, cerrar pestaña/todas, salir. También expone `perform_hot_exit`
//! (usado tanto por `exit-app()` como por el `close_requested` de la ventana en
//! `main.rs`) y `open_path` (compartido con `explorer_ops` y con los argumentos de línea
//! de comandos en `main.rs`).

use super::{activate_tab, push_active_document_to_ui, refresh_recent, refresh_ui, set_status, SharedState};
use crate::App;
use slint::ComponentHandle;
use std::path::PathBuf;

fn text_filters(dialog: rfd::FileDialog) -> rfd::FileDialog {
    dialog
        .add_filter(
            "Todos los archivos de texto",
            &[
                "md", "txt", "json", "xml", "html", "htm", "sql", "js", "ts", "rs", "py", "toml",
                "yaml", "yml", "css", "ini", "log", "csv",
            ],
        )
        .add_filter("Markdown", &["md", "markdown"])
        .add_filter("Texto", &["txt"])
        .add_filter("Todos", &["*"])
}

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_new_tab(move || {
            if let Some(ui) = ui_weak.upgrade() {
                new_tab(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_open_file(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let picker = text_filters(rfd::FileDialog::new()).set_parent(&ui.window().window_handle());
                if let Some(path) = picker.pick_file() {
                    open_path(&ui, &state, path);
                }
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_open_recent(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                open_recent(&ui, &state, index);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_save(move || {
            if let Some(ui) = ui_weak.upgrade() {
                save_active(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_save_as(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let (g, i) = {
                    let st = state.borrow();
                    (st.editor.active_group, st.editor.groups[st.editor.active_group].active)
                };
                save_as_tab(&ui, &state, g, i);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_save_all(move || {
            if let Some(ui) = ui_weak.upgrade() {
                save_all(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_export_zip(move || {
            if let Some(ui) = ui_weak.upgrade() {
                export_zip(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_close_tab(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                let group = state.borrow().editor.active_group;
                close_tab_at(&ui, &state, group, index.max(0) as usize);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_close_all(move || {
            if let Some(ui) = ui_weak.upgrade() {
                close_all(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_exit_app(move || {
            if let Some(ui) = ui_weak.upgrade() {
                // Hallazgo W2: el menú "Salir" saltaba directamente a `perform_hot_exit` +
                // cerrar, sin pasar por `confirm_close_if_needed` (que sí protege el cierre de
                // ventana vía `on_close_requested`, hallazgo C.7). Con "Restaurar sesión al
                // iniciar" desactivado esto perdía en silencio los cambios sin guardar de
                // archivos sucios al salir por el menú.
                if confirm_close_if_needed(&ui, &state).is_some() {
                    return;
                }
                perform_hot_exit(&state);
                let _ = ui.window().hide();
            }
            let _ = slint::quit_event_loop();
        });
    }
}

pub fn new_tab(ui: &App, state: &SharedState) {
    let (g, i) = {
        let mut st = state.borrow_mut();
        st.editor.new_scratch_tab()
    };
    activate_tab(ui, state, g, i);
    set_status(ui, "Nueva pestaña");
}

/// Abre `path`: si ya está abierto lo activa (deduplicación, hallazgo #12); si no, lo
/// carga del disco y lo activa. Comparte esta lógica `open-file`, `open-recent`, el
/// explorador y los argumentos de línea de comandos.
pub fn open_path(ui: &App, state: &SharedState, path: PathBuf) {
    let result = { state.borrow_mut().editor.open_file(path.clone()) };
    match result {
        Ok((g, i)) => {
            activate_tab(ui, state, g, i);
            {
                let mut st = state.borrow_mut();
                st.settings.add_recent(path.to_string_lossy().to_string());
                st.settings.save().ok();
            }
            refresh_recent(ui, state);
            set_status(ui, &format!("Abierto: {}", path.display()));
        }
        Err(e) => set_status(ui, &e),
    }
}

fn open_recent(ui: &App, state: &SharedState, index: i32) {
    if index < 0 {
        return;
    }
    let path = { state.borrow().settings.recent_files.get(index as usize).cloned() };
    let Some(p) = path else { return };
    let pb = PathBuf::from(&p);
    if !pb.exists() {
        {
            let mut st = state.borrow_mut();
            st.settings.recent_files.retain(|x| x != &p);
            st.settings.save().ok();
        }
        refresh_recent(ui, state);
        set_status(ui, "El archivo ya no existe; se quitó de recientes");
        return;
    }
    open_path(ui, state, pb);
}

fn save_active(ui: &App, state: &SharedState) -> bool {
    let (g, i) = {
        let st = state.borrow();
        (st.editor.active_group, st.editor.groups[st.editor.active_group].active)
    };
    save_tab(ui, state, g, i)
}

/// Núcleo de "Guardar" para una pestaña que YA tiene ruta en disco (hallazgo W14): solo
/// la escritura (`atomic_save`) y actualizar `saved_snapshot`/`last_known_mtime` en el
/// modelo — SIN tocar la UI (ni `set_status` ni `refresh_ui`). Lo usa tanto `save_tab`
/// (que sí refresca la UI, para el caso interactivo Ctrl+S de una sola pestaña) como
/// `autosave_tick` en `main.rs` (que guarda potencialmente varias pestañas de archivo
/// por tick y quiere hacer UN solo refresco al final, no uno completo POR archivo).
pub fn save_tab_to_disk(state: &SharedState, group: usize, index: usize) -> Result<Option<PathBuf>, String> {
    let file_path = {
        let st = state.borrow();
        st.editor
            .groups
            .get(group)
            .and_then(|g| g.tabs.get(index))
            .and_then(|t| t.file_path.clone())
    };
    let Some(path) = file_path else {
        return Ok(None);
    };
    // `content_for_disk` restaura CRLF/BOM originales del archivo (hallazgo E11/M4);
    // usarlo aquí en vez de `document.to_string()` evita mezclar finales de línea.
    let content = { state.borrow().editor.groups[group].tabs[index].content_for_disk() };
    // Hallazgo C7: documento del usuario — respeta la marca de solo lectura en vez de
    // limpiarla y sobrescribir en silencio.
    crate::workspace::atomic_save_document(&path, &content).map_err(|e| e.to_string())?;
    {
        let mut st = state.borrow_mut();
        let tab = &mut st.editor.groups[group].tabs[index];
        tab.mark_saved();
        tab.last_known_mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
    }
    Ok(Some(path))
}

/// Guarda la pestaña `(group, index)`: si tiene ruta en disco, escribe directamente
/// (`atomic_save`); si es un borrador, delega en "Guardar como". Devuelve `true` si se
/// guardó (o el usuario canceló "Guardar como" cuenta como `false`, no como error).
pub fn save_tab(ui: &App, state: &SharedState, group: usize, index: usize) -> bool {
    let has_path = {
        let st = state.borrow();
        st.editor
            .groups
            .get(group)
            .and_then(|g| g.tabs.get(index))
            .map(|t| t.file_path.is_some())
            .unwrap_or(false)
    };
    if !has_path {
        return save_as_tab(ui, state, group, index);
    }
    // Hallazgo C20: si el archivo cambió en disco desde que se cargó/guardó por última
    // vez (otra herramienta, otra instancia, control de versiones...), Ctrl+S lo
    // sobrescribía sin preguntar, perdiendo esa versión externa en silencio. Se pregunta
    // primero, igual que cualquier otro editor.
    let external_conflict = {
        let st = state.borrow();
        st.editor
            .groups
            .get(group)
            .and_then(|g| g.tabs.get(index))
            .and_then(|t| Some((t.file_path.clone()?, t.last_known_mtime?)))
            .map(|(path, mtime)| crate::workspace::has_external_changes(&path, mtime))
            .unwrap_or(false)
    };
    if external_conflict {
        let result = rfd::MessageDialog::new()
            .set_title("LightMark")
            .set_description("El archivo cambió en disco. ¿Sobrescribir?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_level(rfd::MessageLevel::Warning)
            .set_parent(&ui.window().window_handle())
            .show();
        if result != rfd::MessageDialogResult::Yes {
            set_status(ui, "Guardado cancelado: el archivo cambió en disco");
            return false;
        }
    }
    match save_tab_to_disk(state, group, index) {
        Ok(Some(path)) => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            set_status(ui, &format!("Guardado: {}", name));
            refresh_ui(ui, state);
            true
        }
        Ok(None) => save_as_tab(ui, state, group, index),
        Err(e) => {
            set_status(ui, &format!("Error al guardar: {}", e));
            false
        }
    }
}

pub fn save_as_tab(ui: &App, state: &SharedState, group: usize, index: usize) -> bool {
    let default_name = {
        let st = state.borrow();
        let title = st
            .editor
            .groups
            .get(group)
            .and_then(|g| g.tabs.get(index))
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Sin título".to_string());
        // No duplicar la extensión (hallazgo C.9: ".md.md") si el título ya trae una
        // (p.ej. un archivo reabierto y guardado con otro nombre); solo añadir ".md" a
        // los borradores sin extensión ("Sin título N").
        if std::path::Path::new(&title).extension().is_some() {
            title
        } else {
            format!("{}.md", title)
        }
    };
    let picker = text_filters(rfd::FileDialog::new().set_file_name(&default_name))
        .set_parent(&ui.window().window_handle());
    let Some(raw_path) = picker.save_file() else {
        return false;
    };
    // `content_for_disk` restaura CRLF/BOM (hallazgo E11/M4); los borradores nunca los
    // tuvieron, así que para ellos equivale a `document.to_string()`.
    let content = { state.borrow().editor.groups[group].tabs[index].content_for_disk() };
    // Hallazgo C7: documento del usuario — ver comentario en `save_tab_to_disk`.
    match crate::workspace::atomic_save_document(&raw_path, &content) {
        Ok(_) => {
            // Normalizar (hallazgo C.9) para que `find_tab_by_path`/recientes reconozcan
            // este archivo de forma consistente con rutas obtenidas por otras vías.
            let path = crate::editor::normalize_path(&raw_path);
            // Título = nombre de archivo CON extensión, igual que al abrir (hallazgo C.9).
            let title = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sin título".to_string());
            let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            // Hallazgo C16: si YA había otra pestaña abierta con esta misma ruta (p.ej.
            // "Guardar como" sobre la ruta de un archivo que el usuario tenía abierto en
            // otra pestaña), evitar quedarse con dos pestañas del mismo archivo en disco
            // tras guardar — se calcula ANTES de tocar `file_path` de esta pestaña, para
            // no encontrarse a sí misma.
            let duplicate = state.borrow().editor.find_tab_by_path(&path).filter(|&(dg, di)| (dg, di) != (group, index));
            {
                let mut st = state.borrow_mut();
                if let Some(old_id) = st.editor.groups[group].tabs[index].scratch_id.clone() {
                    let doc = st.editor.scratch_manager.create_scratch_with_id(old_id, ropey::Rope::new());
                    st.editor.scratch_manager.delete_scratch(&doc).ok();
                }
                let tab = &mut st.editor.groups[group].tabs[index];
                tab.title = title;
                tab.scratch_id = None;
                tab.file_path = Some(path.clone());
                tab.last_known_mtime = mtime;
                tab.mark_saved();
                st.settings.add_recent(path.to_string_lossy().to_string());
                st.settings.save().ok();
            }
            let duplicate_warning = match duplicate {
                Some((dg, di)) => {
                    let other_dirty = state
                        .borrow()
                        .editor
                        .groups
                        .get(dg)
                        .and_then(|g| g.tabs.get(di))
                        .map(|t| t.is_dirty())
                        .unwrap_or(false);
                    if other_dirty {
                        Some(" (aviso: ya había otra pestaña abierta con cambios sin guardar para este mismo archivo)".to_string())
                    } else {
                        // La otra pestaña no tenía cambios pendientes: cerrarla, para no
                        // dejar dos pestañas del mismo archivo en disco.
                        close_tab_at(ui, state, dg, di);
                        None
                    }
                }
                None => None,
            };
            set_status(
                ui,
                &format!(
                    "Guardado como: {}{}",
                    path.display(),
                    duplicate_warning.unwrap_or_default()
                ),
            );
            refresh_ui(ui, state);
            refresh_recent(ui, state);
            true
        }
        Err(e) => {
            set_status(ui, &format!("Error al guardar: {}", e));
            false
        }
    }
}

fn save_all(ui: &App, state: &SharedState) {
    let scratch_count = { state.borrow_mut().editor.save_all_scratches().unwrap_or(0) };
    let dirty: Vec<(usize, usize)> = {
        let st = state.borrow();
        st.editor
            .groups
            .iter()
            .enumerate()
            .flat_map(|(gi, g)| {
                g.tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| !t.is_scratch() && t.is_dirty())
                    .map(move |(ti, _)| (gi, ti))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let mut saved = 0usize;
    for (g, i) in dirty {
        if save_tab(ui, state, g, i) {
            saved += 1;
        }
    }
    set_status(ui, &format!("Guardados {} borrador(es) y {} archivo(s)", scratch_count, saved));
    refresh_ui(ui, state);
}

fn export_zip(ui: &App, state: &SharedState) {
    let docs: Vec<crate::scratch::ScratchDocument> = {
        let st = state.borrow();
        st.editor
            .groups
            .iter()
            .flat_map(|g| g.tabs.iter())
            .filter(|t| t.is_scratch() && !t.document.to_string().is_empty())
            .map(|t| {
                let mut doc = st
                    .editor
                    .scratch_manager
                    .create_scratch_with_id(t.scratch_id.clone().unwrap(), t.document.clone());
                doc.title = t.title.clone();
                doc.language = crate::scratch::detect_language(&t.document.to_string());
                doc
            })
            .collect()
    };
    if docs.is_empty() {
        set_status(ui, "No hay borradores con contenido para exportar");
        return;
    }
    let now = time::OffsetDateTime::now_utc();
    let default_name = format!("borradores-{:04}-{:02}-{:02}.zip", now.year(), now.month() as u8, now.day());
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter("ZIP", &["zip"])
        .set_parent(&ui.window().window_handle())
        .save_file()
    else {
        return;
    };
    let count = docs.len();
    match crate::scratch::export_to_zip(&docs, &path) {
        Ok(_) => set_status(ui, &format!("Exportados {} borrador(es) a {}", count, path.display())),
        Err(e) => set_status(ui, &format!("Error al exportar: {}", e)),
    }
}

pub fn close_tab_at(ui: &App, state: &SharedState, group: usize, index: usize) {
    let (is_scratch, is_dirty, is_empty, scratch_id, title) = {
        let st = state.borrow();
        match st.editor.groups.get(group).and_then(|g| g.tabs.get(index)) {
            Some(t) => (
                t.is_scratch(),
                t.is_dirty(),
                t.document.to_string().is_empty(),
                t.scratch_id.clone(),
                t.title.clone(),
            ),
            None => return,
        }
    };

    if is_scratch {
        if is_empty {
            if let Some(id) = scratch_id {
                let st = state.borrow();
                let doc = st.editor.scratch_manager.create_scratch_with_id(id, ropey::Rope::new());
                st.editor.scratch_manager.delete_scratch(&doc).ok();
            }
            finish_close(ui, state, group, index);
            return;
        }
        // Borrador NO vacío (hallazgo C.8): preguntar en vez de cerrar en silencio.
        // Sí → "Guardar como" (si se completa, se cierra); No → descartar (borra el
        // archivo de scratch en disco); Cancelar → no hacer nada.
        let result = rfd::MessageDialog::new()
            .set_title("LightMark")
            .set_description(format!("¿Guardar el borrador '{}' antes de cerrarlo?", title))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .set_level(rfd::MessageLevel::Warning)
            .set_parent(&ui.window().window_handle())
            .show();
        match result {
            rfd::MessageDialogResult::Yes => {
                if save_as_tab(ui, state, group, index) {
                    finish_close(ui, state, group, index);
                }
            }
            rfd::MessageDialogResult::No => {
                if let Some(id) = scratch_id {
                    let st = state.borrow();
                    let doc = st.editor.scratch_manager.create_scratch_with_id(id, ropey::Rope::new());
                    st.editor.scratch_manager.delete_scratch(&doc).ok();
                }
                finish_close(ui, state, group, index);
            }
            _ => {}
        }
        return;
    }

    if is_dirty {
        let result = rfd::MessageDialog::new()
            .set_title("LightMark")
            .set_description(format!("¿Guardar cambios en {}?", title))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .set_level(rfd::MessageLevel::Warning)
            .set_parent(&ui.window().window_handle())
            .show();
        match result {
            rfd::MessageDialogResult::Yes => {
                if save_tab(ui, state, group, index) {
                    finish_close(ui, state, group, index);
                }
            }
            rfd::MessageDialogResult::No => finish_close(ui, state, group, index),
            _ => {}
        }
    } else {
        finish_close(ui, state, group, index);
    }
}

fn finish_close(ui: &App, state: &SharedState, group: usize, index: usize) {
    // Hallazgo E5 (segunda parte): cerrar una pestaña de FONDO (`index` distinto de la
    // pestaña activa del grupo, o un grupo distinto del activo) no debe reescribir
    // `editor_text`/la selección del editor — antes `push_active_document_to_ui` se
    // llamaba siempre, interrumpiendo la escritura/selección en curso en la pestaña
    // realmente visible aunque el documento activo no hubiera cambiado en absoluto.
    let was_active_group = state.borrow().editor.active_group == group;
    let was_active_tab = was_active_group
        && state
            .borrow()
            .editor
            .groups
            .get(group)
            .map(|g| g.active == index)
            .unwrap_or(false);
    {
        let mut st = state.borrow_mut();
        st.editor.close_tab(group, index);
    }
    if was_active_tab {
        push_active_document_to_ui(ui, state);
    }
    refresh_ui(ui, state);
}

fn close_all(ui: &App, state: &SharedState) {
    let group = state.borrow().editor.active_group;
    loop {
        let (index, len) = {
            let st = state.borrow();
            (st.editor.groups[group].active, st.editor.groups[group].tabs.len())
        };
        let only_default_left = len == 1
            && state
                .borrow()
                .editor
                .groups[group]
                .tabs
                .first()
                .map(|t| t.is_scratch() && t.document.to_string().is_empty())
                .unwrap_or(false);
        if only_default_left {
            break;
        }
        close_tab_at(ui, state, group, index);
        let after_len = state.borrow().editor.groups[group].tabs.len();
        if after_len == len {
            // El usuario canceló el diálogo de guardado: abortar "cerrar todas".
            break;
        }
    }
    set_status(ui, "Pestañas cerradas");
}

/// Persiste borradores + sesión ("hot exit": el contenido no guardado de archivos
/// sucios queda dentro de la sesión, ver `EditorState::to_session`) + configuración.
/// Se usa tanto al pulsar "Salir" como al cerrar la ventana: nunca se pregunta, nunca
/// se pierde nada (filosofía "zero data loss", hallazgo #9) — SIEMPRE que
/// `restore_session` esté activado, que es lo que hace que esa sesión se vuelva a leer
/// en el siguiente arranque (ver `confirm_close_if_needed`, hallazgo C.7, para el caso
/// contrario).
pub fn perform_hot_exit(state: &SharedState) {
    let mut st = state.borrow_mut();
    st.editor.save_all_scratches().ok();
    let session = st.editor.to_session();
    st.editor.session_manager.save_auto_session(&session).ok();
    st.settings.save().ok();
}

/// Hallazgo C.7: `perform_hot_exit` guarda el contenido sin guardar de las pestañas de
/// archivo sucias DENTRO de la sesión ("hot exit"), pero esa sesión solo se vuelve a
/// leer al arrancar si `settings.restore_session` está activado (ver `main.rs`). Con
/// `restore_session = false` esos cambios se guardarían en un archivo que nadie va a
/// leer nunca: para el usuario equivale a perderlos en silencio al cerrar. Se pregunta
/// antes de cerrar si hay alguna pestaña de archivo sucia en ese caso; devuelve
/// `Some(KeepWindowShown)` si el usuario cancela el cierre, o `None` si se puede
/// continuar con el cierre normal (`perform_hot_exit` + ocultar ventana).
pub fn confirm_close_if_needed(ui: &App, state: &SharedState) -> Option<slint::CloseRequestResponse> {
    let restore_session = state.borrow().settings.restore_session;
    if restore_session {
        return None;
    }
    let dirty: Vec<(usize, usize)> = {
        let st = state.borrow();
        st.editor
            .groups
            .iter()
            .enumerate()
            .flat_map(|(gi, g)| {
                g.tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| !t.is_scratch() && t.is_dirty())
                    .map(move |(ti, _)| (gi, ti))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    if dirty.is_empty() {
        return None;
    }
    let result = rfd::MessageDialog::new()
        .set_title("LightMark")
        .set_description(format!(
            "Hay {} archivo(s) con cambios sin guardar y \"Restaurar sesión al iniciar\" está desactivado en Configuración: esos cambios se perderían al cerrar. ¿Guardarlos ahora?",
            dirty.len()
        ))
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .set_level(rfd::MessageLevel::Warning)
        .set_parent(&ui.window().window_handle())
        .show();
    match result {
        rfd::MessageDialogResult::Yes => {
            // Hallazgo C6: si ALGUNO de los guardados falla (p.ej. archivo de solo
            // lectura, disco lleno, ruta ya no accesible), no se debe cerrar la ventana
            // de todos modos — eso perdería exactamente los cambios que el usuario
            // acaba de pedir explícitamente guardar. `save_tab` ya informa el error en
            // la barra de estado; aquí solo se decide si el cierre puede continuar.
            let mut ok = true;
            for (g, i) in dirty {
                ok &= save_tab(ui, state, g, i);
            }
            if ok {
                None
            } else {
                Some(slint::CloseRequestResponse::KeepWindowShown)
            }
        }
        rfd::MessageDialogResult::No => None,
        _ => Some(slint::CloseRequestResponse::KeepWindowShown),
    }
}
