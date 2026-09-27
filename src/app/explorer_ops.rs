//! Explorador de archivos: abrir carpeta, navegar a subcarpeta/padre, abrir archivo.

use super::{set_status, SharedState};
use crate::app::file_ops::open_path;
use crate::{App, ExplorerEntry};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::path::{Path, PathBuf};

pub fn wire(ui: &App, state: &SharedState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_open_folder(move || {
            if let Some(ui) = ui_weak.upgrade() {
                // Hallazgo W17: sin `set_parent`, el diálogo nativo no queda asociado a la
                // ventana de LightMark (puede abrirse detrás, o no bloquear correctamente
                // la interacción con la ventana principal mientras está abierto).
                let picker = rfd::FileDialog::new().set_parent(&ui.window().window_handle());
                if let Some(dir) = picker.pick_folder() {
                    set_root(&ui, &state, dir);
                }
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_explorer_navigate(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                navigate(&ui, &state, PathBuf::from(path.as_str()));
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_explorer_navigate_parent(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let parent = {
                    let st = state.borrow();
                    st.editor.workspace_root.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf()))
                };
                if let Some(p) = parent {
                    navigate(&ui, &state, p);
                }
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_explorer_open(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                open_path(&ui, &state, PathBuf::from(path.as_str()));
            }
        });
    }
}

fn set_root(ui: &App, state: &SharedState, dir: PathBuf) {
    {
        let mut st = state.borrow_mut();
        st.editor.workspace_root = Some(dir.clone());
    }
    refresh_explorer(ui, &dir);
}

fn navigate(ui: &App, state: &SharedState, dir: PathBuf) {
    if !dir.is_dir() {
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.editor.workspace_root = Some(dir.clone());
    }
    refresh_explorer(ui, &dir);
}

/// Hallazgo E7: expuesto para que `main.rs` pueda rellenar el explorador al iniciar
/// cuando la sesión restaurada trae un `workspace_root` (antes solo se rellenaba tras
/// "Abrir carpeta…"/navegar, así que reabrir la app con una carpeta ya abierta mostraba
/// el panel vacío hasta la primera interacción).
pub fn refresh_explorer(ui: &App, dir: &Path) {
    match crate::workspace::list_directory(dir) {
        Ok(entries) => {
            let items: Vec<ExplorerEntry> = entries
                .into_iter()
                .map(|e| ExplorerEntry {
                    name: e.name.into(),
                    is_dir: e.is_dir,
                    path: e.path.to_string_lossy().to_string().into(),
                })
                .collect();
            ui.set_explorer_entries(ModelRc::new(VecModel::from(items)));
            ui.set_explorer_has_folder(true);
            let folder_name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.to_string_lossy().to_string());
            ui.set_explorer_folder(folder_name.into());
            ui.set_explorer_folder_path(dir.to_string_lossy().to_string().into());
        }
        Err(e) => set_status(ui, &format!("No se pudo leer la carpeta: {}", e)),
    }
}
