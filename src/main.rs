#![windows_subsystem = "windows"]

//! Punto de entrada. La UI vive por completo en `ui/*.slint` (compilada por `build.rs`
//! vía `slint-build`, expuesta aquí con `slint::include_modules!()`); toda la lógica
//! vive en módulos (`editor`, `textops`, `settings`, `scratch`, `session`, `search`,
//! `workspace`, `format`, `syntax`, `editor_view`, `ai`, `markdown`, `preview`) y el cableado de
//! callbacks en `app/*` (ver `app::wire_all`). Este archivo, desde WSE1b (multi-ventana,
//! item 12 del plan), ya NO crea una sola ventana: `bring_up_window` construye/cablea/
//! registra UNA ventana (App + AppState + EditorState propios) y `main` la llama una vez
//! por cada ventana de la sesión v2 restaurada (o una sola, por defecto), antes de entrar
//! al bucle de eventos UNA sola vez con `slint::run_event_loop()` — nunca `App::run()`,
//! que asume una sola ventana (ver comentario en `main()`).

slint::include_modules!();

mod ai;
mod app;
mod editor;
mod editor_view;
mod format;
mod markdown;
mod preview;
mod scratch;
mod search;
mod session;
mod settings;
mod syntax;
mod textops;
mod workspace;

use app::windows;
use app::{AppState, SharedState};
use editor::EditorState;
use scratch::ScratchManager;
use session::{SessionManager, SessionV2, WindowState as SessionWindowState};
use settings::Settings;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Instancia única (hallazgo R1_FIXES C.6) — ahora "una instancia primaria con N
// ventanas", no "una ventana" (E1).
// ---------------------------------------------------------------------------

/// Ruta del lock-file exclusivo que identifica a la instancia "primaria".
fn instance_lock_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("instance.lock")
}

/// Ruta del archivo de solicitudes que una instancia secundaria deja para que la
/// primaria abra sus rutas (`lightmark.exe archivo.md` mientras ya hay una ventana).
fn open_requests_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("open-requests.txt")
}

/// Intenta convertirse en la instancia primaria bloqueando `instance.lock` en exclusiva
/// (`File::try_lock`, estable desde Rust 1.89). El `File` devuelto debe mantenerse vivo
/// durante toda la ejecución de `main`: soltarlo libera el lock. `None` significa que ya
/// hay una instancia primaria activa.
fn try_acquire_single_instance_lock(app_data_dir: &Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(app_data_dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(instance_lock_path(app_data_dir))
        .ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(std::fs::TryLockError::WouldBlock) => None,
        Err(std::fs::TryLockError::Error(_)) => Some(file),
    }
}

/// Instancia secundaria: anexa sus argumentos (rutas a abrir) al archivo de solicitudes
/// que la primaria sondea en su temporizador, para que abra esos archivos en su propia
/// ventana (o en una nueva, según `Settings.open_files_in_new_window` — E1) en vez de
/// lanzar un segundo proceso con su propia ventana.
fn forward_open_requests(app_data_dir: &Path, args: &[String]) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(open_requests_path(app_data_dir))
    {
        if args.is_empty() {
            let _ = writeln!(f);
        } else {
            for a in args {
                let abs = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let _ = writeln!(f, "{}", abs.to_string_lossy());
            }
        }
    }
}

/// Sondeado por el temporizador global (una vez por tick, no una vez por ventana): si una
/// instancia secundaria dejó rutas pendientes, las abre en la ventana destino (la más
/// reciente, o una nueva si `open_files_in_new_window` está activado — E1) y activa esa
/// ventana. Sin rutas (marcador vacío) simplemente activa la ventana más reciente.
fn poll_open_requests(app_data_dir: &Path) {
    let path = open_requests_path(app_data_dir);
    let claimed = path.with_extension("processing");
    if std::fs::rename(&path, &claimed).is_err() {
        return;
    }
    let Ok(content) = std::fs::read_to_string(&claimed) else {
        let _ = std::fs::remove_file(&claimed);
        return;
    };
    let _ = std::fs::remove_file(&claimed);
    let paths: Vec<PathBuf> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();

    let target_id = if paths.is_empty() {
        windows::most_recent()
    } else if windows::shared_settings().open_files_in_new_window {
        bring_up_window(app_data_dir, None).ok()
    } else {
        windows::most_recent()
    };
    let Some(id) = target_id.or_else(|| bring_up_window(app_data_dir, None).ok()) else {
        return;
    };
    windows::with_window(id, |ui, state| {
        for p in &paths {
            if p.is_file() {
                app::file_ops::open_path(ui, state, p.clone());
            }
        }
    });
    windows::activate(id);
}

fn main() -> Result<(), slint::PlatformError> {
    let app_data_dir = scratch::get_app_data_dir();
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    // Debe permanecer viva hasta el final de `main` (el bucle de eventos incluido):
    // soltarla liberaría el lock y dejaría de considerarse la instancia primaria.
    let _instance_lock = match try_acquire_single_instance_lock(&app_data_dir) {
        Some(lock) => lock,
        None => {
            forward_open_requests(&app_data_dir, &cli_args);
            return Ok(());
        }
    };

    let settings = Settings::load();
    windows::set_shared_settings(settings.clone());
    let session_manager = SessionManager::new(session::ensure_sessions_dir(&app_data_dir));

    // Sesión v2 (E1): una lista de ventanas (geometría + pestañas cada una), migrada
    // automáticamente desde v1 (una sola ventana implícita) si es lo que hay en disco.
    // Sin sesión guardada, o con "Restaurar sesión al iniciar" desactivado, se abre una
    // sola ventana con un borrador vacío (mismo comportamiento que antes de E1).
    let restored: Vec<SessionWindowState> = if settings.restore_session {
        session_manager.load_auto_session_v2().ok().flatten().map(|s| s.windows).unwrap_or_default()
    } else {
        Vec::new()
    };

    if restored.is_empty() {
        bring_up_window(&app_data_dir, None)?;
    } else {
        // Clamp/cascada de geometría (E1): una ventana guardada en un monitor que ya no
        // está conectado no debe abrirse fuera de la pantalla visible.
        let (vx, vy, vw, vh) = virtual_screen_rect();
        for (i, ws) in restored.iter().enumerate() {
            let (x, y) = session::clamp_window_position(ws.x, ws.y, ws.width, ws.height, vx, vy, vw, vh, i as u32);
            let mut ws = ws.clone();
            ws.x = x;
            ws.y = y;
            bring_up_window(&app_data_dir, Some(&ws))?;
        }
        // Renumeración "Sin título N" combinada (E1, item 8 del plan): varias ventanas
        // restauradas a la vez pueden traer números que colisionan entre sí (cada una se
        // guardó/restauró sin saber de las demás).
        for id in windows::snapshot_ids() {
            renumber_untitled_against_others(id);
        }
    }

    // Argumentos de línea de comandos (`lightmark.exe archivo1.md archivo2.txt`): se
    // abren en la ventana más reciente (la primera, en el arranque).
    if !cli_args.is_empty()
        && let Some(id) = windows::most_recent()
    {
        windows::with_window(id, |ui, state| {
            for arg in &cli_args {
                let path = PathBuf::from(arg);
                if path.is_file() {
                    app::file_ops::open_path(ui, state, path);
                }
            }
        });
    }

    // Temporizador único de 150 ms para TODO el proceso (E1): itera cada ventana
    // registrada (autoguardado, cambios externos, refresco de "otras ventanas") y
    // termina con el sondeo de solicitudes de una segunda instancia y el guardado de la
    // sesión v2 combinada — UNA sola vez por tick, no una vez por ventana.
    let timer = slint::Timer::default();
    {
        let app_data_dir = app_data_dir.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
            for id in windows::snapshot_ids() {
                windows::with_window(id, |ui, state| {
                    on_tick(ui, state);
                });
                refresh_other_windows_titles(id);
            }
            poll_open_requests(&app_data_dir);
            session_tick_multi(&app_data_dir);
        });
    }

    // Bucle de eventos único para TODAS las ventanas (E1): cada `App` ya se mostró
    // (`show()`, dentro de `bring_up_window`) ANTES de esta llamada — nunca usar
    // `App::run()` aquí, que asume una sola ventana (hace su propio `show()` +
    // `run_event_loop()` + `hide()`). `run_event_loop()` (no `run_event_loop_until_quit`)
    // porque queremos que salir de la última ventana visible termine el proceso salvo que
    // "Salir"/el cierre de la última ventana ya haya llamado a `quit_event_loop()` antes
    // (ver `handle_window_close_request`) — cualquiera de los dos caminos es correcto.
    slint::run_event_loop()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// E1 — construcción/registro de una ventana
// ---------------------------------------------------------------------------

/// Rectángulo del área virtual (todos los monitores conectados), para clampar/cascadear
/// geometría restaurada de una ventana fuera de pantalla (E1, item 8 del plan).
fn virtual_screen_rect() -> (i32, i32, i32, i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// HWND Win32 real de `ui` (E1): solo disponible tras al menos una vuelta del bucle de
/// eventos siguiente a `show()` (ver `slint::Window::window_handle`, doc de la crate) —
/// por eso `bring_up_window` lo programa vía `slint::invoke_from_event_loop` en vez de
/// leerlo inmediatamente tras `show()`. `0` si el backend no lo expone (no debería pasar
/// en Windows con el backend por defecto, pero se trata como "sin HWND conocido" en vez
/// de entrar en pánico).
fn capture_hwnd(ui: &App) -> isize {
    use raw_window_handle::HasWindowHandle;
    match ui.window().window_handle().window_handle() {
        Ok(h) => match h.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(w) => w.hwnd.get(),
            _ => 0,
        },
        Err(_) => 0,
    }
}

/// Construye UNA ventana completa (App + AppState/EditorState propios, `wire_all`
/// cableado, geometría aplicada, mostrada) y la registra en `app::windows` (E1, item 12
/// del plan — la extracción que `WSE1_REPORT.md` deja pendiente). `ws`: `Some` para
/// restaurar contenido+geometría desde la sesión v2; `None` para una ventana nueva con un
/// borrador vacío, posicionada +32px respecto a la ventana activa (si ya hay alguna).
fn bring_up_window(app_data_dir: &Path, ws: Option<&SessionWindowState>) -> Result<u32, slint::PlatformError> {
    let settings = windows::shared_settings();
    let scratch_manager = ScratchManager::new(scratch::get_scratch_dir());
    let session_manager = SessionManager::new(session::ensure_sessions_dir(app_data_dir));
    let mut editor_state = EditorState::new(scratch_manager, session_manager);

    let mut restore_warnings: Vec<String> = Vec::new();
    if let Some(ws) = ws {
        restore_warnings = editor_state.restore_from_session(&ws.to_session());
    }

    let state: SharedState = Rc::new(RefCell::new(AppState::new(editor_state, settings)));

    let ui = App::new()?;
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    ui.set_shortcuts_list(ModelRc::new(VecModel::from(app::build_shortcuts_list())));

    app::settings_ops::push_settings_to_ui(&ui, &state);
    app::wire_all(&ui, &state);
    app::push_active_document_to_ui(&ui, &state);
    app::refresh_ui(&ui, &state);
    {
        let root = state.borrow().editor.workspace_root.clone();
        if let Some(root) = root {
            app::explorer_ops::refresh_explorer(&ui, &root);
        }
    }
    if !restore_warnings.is_empty() {
        app::set_status(&ui, &restore_warnings.join("; "));
    }

    // Geometría: restaurada (ya clampada/cascadeada por el llamador) o, para una ventana
    // nueva, +32px respecto a la ventana activa (si hay alguna).
    match ws {
        Some(ws) => {
            let _ = ui
                .window()
                .set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(ws.x, ws.y)));
            let _ = ui.window().set_size(slint::WindowSize::Physical(slint::PhysicalSize::new(
                ws.width.max(1.0) as u32,
                ws.height.max(1.0) as u32,
            )));
            if ws.maximized {
                ui.window().set_maximized(true);
            }
        }
        None => {
            if let Some(active_id) = windows::most_recent()
                && let Some((x, y)) = windows::with_window(active_id, |active_ui, _state| {
                    let p = active_ui.window().position();
                    (p.x + 32, p.y + 32)
                })
            {
                let _ = ui.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(x, y)));
            }
        }
    }

    ui.show()?;
    let id = windows::register_window(ui.clone_strong(), state.clone(), 0);

    // HWND: se pide en cuanto el bucle de eventos empiece a correr (ver `capture_hwnd`).
    {
        let ui_weak = ui.as_weak();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                windows::set_hwnd(id, capture_hwnd(&ui));
            }
        });
    }

    wire_window_specific_callbacks(&ui, &state, id, app_data_dir.to_path_buf());
    Ok(id)
}

/// Cablea lo que "Cerrar ventana"/"Nueva ventana"/"Mover pestaña..." necesitan conocer el
/// `id` de ESTA ventana en el registro — por eso vive en `main.rs` (donde se conoce `id`
/// justo tras `register_window`) y no en `app::wire_all` (que se cablea ANTES de que la
/// ventana tenga un id, y es compartido con cualquier posible caso de un solo `App` sin
/// registro, como los tests de integración de `app/*` si algún día existieran).
fn wire_window_specific_callbacks(ui: &App, state: &SharedState, id: u32, app_data_dir: PathBuf) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.window().on_close_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return slint::CloseRequestResponse::HideWindow;
            };
            handle_window_close_request(&ui, &state, id)
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        let app_data_dir = app_data_dir.clone();
        ui.on_new_window(move || {
            if ui_weak.upgrade().is_none() {
                return;
            }
            match bring_up_window(&app_data_dir, None) {
                Ok(new_id) => {
                    renumber_untitled_against_others(new_id);
                    refresh_all_other_windows_titles();
                    windows::activate(new_id);
                }
                Err(e) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        app::set_status(&ui, &format!("No se pudo crear la ventana: {}", e));
                    }
                }
            }
            let _ = &state; // silencia "no usado" si el brazo Ok no la necesita
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        let app_data_dir = app_data_dir.clone();
        ui.on_move_tab_to_new_window(move || {
            if let Some(ui) = ui_weak.upgrade() {
                move_active_tab_to_new_window(&ui, &state, id, &app_data_dir);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_move_tab_to_window(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                move_active_tab_to_window(&ui, &state, id, index);
            }
        });
    }
    refresh_all_other_windows_titles();
}

/// Ids + título de las DEMÁS ventanas registradas (excluyendo `exclude_id`), en el mismo
/// orden en el que se empujan a `other_windows_titles` — el índice que recibe
/// `move-tab-to-window(i)` desde la UI es el índice en ESTA lista.
fn other_window_entries(exclude_id: u32) -> Vec<(u32, String)> {
    windows::snapshot_ids()
        .into_iter()
        .filter(|&id| id != exclude_id)
        .filter_map(|id| windows::with_window(id, |ui, _state| (id, ui.get_window_title().to_string())))
        .collect()
}

fn refresh_other_windows_titles(id: u32) {
    let entries = other_window_entries(id);
    let titles: Vec<slint::SharedString> = entries.iter().map(|(_, t)| t.clone().into()).collect();
    windows::with_window(id, |ui, _state| {
        ui.set_other_windows_titles(ModelRc::new(VecModel::from(titles)));
    });
}

fn refresh_all_other_windows_titles() {
    for id in windows::snapshot_ids() {
        refresh_other_windows_titles(id);
    }
}

/// Renumera "Sin título N" de la ventana `id` para que no colisione con las de las demás
/// ventanas registradas (E1, item 8 del plan): `EditorState::recompute_titles` solo
/// dedupe dentro de SU PROPIA ventana (ver su doc); esto cubre el resto combinando
/// `windows::used_untitled_numbers_except`, tanto al restaurar varias ventanas a la vez
/// como al crear una ventana nueva con su borrador "Sin título 1" por defecto.
fn renumber_untitled_against_others(id: u32) {
    let extra = windows::used_untitled_numbers_except(Some(id));
    windows::with_window(id, |ui, state| {
        let mut st = state.borrow_mut();
        let opts = crate::scratch::NamingOpts::from_settings(&st.settings);
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for g in st.editor.groups.iter_mut() {
            for t in g.tabs.iter_mut().filter(|t| t.is_scratch() && t.custom_title.is_none() && t.auto_title.is_none()) {
                if let Some(n) = t.untitled_n
                    && (extra.contains(&n) || !seen.insert(n))
                {
                    let mut next = 1u32;
                    while extra.contains(&next) || seen.contains(&next) {
                        next += 1;
                    }
                    seen.insert(next);
                    t.untitled_n = Some(next);
                }
            }
        }
        for g in st.editor.groups.iter_mut() {
            for t in g.tabs.iter_mut().filter(|t| t.is_scratch()) {
                t.title = t.naming_display_title(&opts);
            }
        }
        drop(st);
        app::refresh_tabs(ui, state);
    });
}

// ---------------------------------------------------------------------------
// E1 — mover pestañas entre ventanas
// ---------------------------------------------------------------------------

/// Tras mover UNA pestaña fuera de `id` (a una ventana nueva o existente): si la ventana
/// de origen quedó sin ninguna pestaña en ningún grupo y hay otras ventanas registradas,
/// se cierra automáticamente (no queda nada útil en ella); si es la ÚLTIMA ventana, se le
/// inserta un borrador vacío (una ventana nunca debe quedar sin ninguna pestaña). En
/// cualquier otro caso, solo refresca su UI.
fn finish_source_after_tab_moved(ui: &App, state: &SharedState, id: u32) {
    let all_empty = {
        let st = state.borrow();
        st.editor.groups.iter().all(|g| g.tabs.is_empty())
    };
    if all_empty {
        let other_exists = windows::snapshot_ids().into_iter().any(|i| i != id);
        if other_exists {
            close_window_silently(id);
            return;
        }
        state.borrow_mut().editor.new_scratch_tab();
    }
    app::push_active_document_to_ui(ui, state);
    app::refresh_ui(ui, state);
}

fn move_active_tab_to_new_window(ui: &App, state: &SharedState, id: u32, app_data_dir: &Path) {
    let tab = {
        let mut st = state.borrow_mut();
        let g = st.editor.active_group;
        let idx = st.editor.groups.get(g).map(|gr| gr.active).unwrap_or(0);
        st.editor.take_tab(g, idx)
    };
    let Some(tab) = tab else {
        app::set_status(ui, "No hay pestaña activa para mover");
        return;
    };
    match bring_up_window(app_data_dir, None) {
        Ok(new_id) => {
            windows::with_window(new_id, |new_ui, new_state| {
                // La ventana nueva ya trae su propio borrador "Sin título 1" por defecto
                // (`EditorState::new`) — la pestaña movida se añade además de ese, no en
                // su lugar; el usuario puede cerrar el que no quiera.
                new_state.borrow_mut().editor.insert_tab(tab);
                app::push_active_document_to_ui(new_ui, new_state);
                app::refresh_ui(new_ui, new_state);
            });
            renumber_untitled_against_others(new_id);
            refresh_all_other_windows_titles();
            windows::activate(new_id);
        }
        Err(e) => {
            // No se pudo crear la ventana destino: devolver la pestaña a su sitio en vez
            // de perderla.
            state.borrow_mut().editor.insert_tab(tab);
            app::set_status(ui, &format!("No se pudo crear la ventana: {}", e));
            return;
        }
    }
    finish_source_after_tab_moved(ui, state, id);
}

fn move_active_tab_to_window(ui: &App, state: &SharedState, id: u32, target_index: i32) {
    if target_index < 0 {
        return;
    }
    let entries = other_window_entries(id);
    let Some(&(target_id, _)) = entries.get(target_index as usize) else {
        app::set_status(ui, "Esa ventana ya no existe");
        return;
    };
    let tab = {
        let mut st = state.borrow_mut();
        let g = st.editor.active_group;
        let idx = st.editor.groups.get(g).map(|gr| gr.active).unwrap_or(0);
        st.editor.take_tab(g, idx)
    };
    let Some(tab) = tab else {
        app::set_status(ui, "No hay pestaña activa para mover");
        return;
    };
    windows::with_window(target_id, |target_ui, target_state| {
        target_state.borrow_mut().editor.insert_tab(tab);
        app::push_active_document_to_ui(target_ui, target_state);
        app::refresh_ui(target_ui, target_state);
    });
    windows::activate(target_id);
    finish_source_after_tab_moved(ui, state, id);
}

/// Todos los ids registrados salvo `exclude_id`, en orden de uso más reciente primero (lo
/// bastante para elegir un destino razonable al mover/cerrar sin que el llamador tenga que
/// conocer el orden interno del registro).
fn other_id_most_recent_first(exclude_id: u32) -> Vec<u32> {
    let mut ids: Vec<u32> = windows::snapshot_ids().into_iter().filter(|&i| i != exclude_id).collect();
    // `most_recent()` no filtra por exclusión; se usa solo para desempatar poniéndolo
    // primero si aparece en la lista (normalmente sí, salvo que la más reciente sea la
    // que se está cerrando/vaciando, que es justo el caso que queremos excluir).
    if let Some(mr) = windows::most_recent()
        && let Some(pos) = ids.iter().position(|&i| i == mr)
    {
        ids.swap(0, pos);
    }
    ids
}

fn move_all_tabs_to_window(source_state: &SharedState, target_id: u32) {
    let tabs: Vec<crate::editor::Tab> = {
        let mut st = source_state.borrow_mut();
        let mut out = Vec::new();
        let group_count = st.editor.groups.len();
        for g in 0..group_count {
            while let Some(t) = st.editor.take_tab(g, 0) {
                out.push(t);
            }
        }
        out
    };
    windows::with_window(target_id, |ui, target_state| {
        for t in tabs {
            target_state.borrow_mut().editor.insert_tab(t);
        }
        app::push_active_document_to_ui(ui, target_state);
        app::refresh_ui(ui, target_state);
    });
    renumber_untitled_against_others(target_id);
}

// ---------------------------------------------------------------------------
// E1 — cierre de ventana
// ---------------------------------------------------------------------------

/// Quita `id` del registro y libera su `App` — SIEMPRE diferido con
/// `slint::invoke_from_event_loop` (nunca dentro de su propio `on_close_requested`: dropear
/// un `App` en ese punto crashea, ver doc de `app::windows`). Seguro llamarlo también
/// fuera de un `close_requested` (p.ej. tras mover la última pestaña a otra ventana):
/// oculta explícitamente antes de diferir la baja del registro.
fn close_window_silently(id: u32) {
    windows::with_window(id, |ui, _state| {
        let _ = ui.window().hide();
    });
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(entry) = windows::remove_window(id) {
            drop(entry);
        }
        refresh_all_other_windows_titles();
    });
}

/// `on_close_requested` de una ventana concreta (E1, item 7 del plan):
/// - Última ventana registrada → camino de siempre (hot exit + `quit_event_loop`).
/// - Ventana no-última SIN nada que perder (ningún borrador con contenido, ningún archivo
///   sucio) → se cierra en silencio.
/// - Ventana no-última CON algo que perder → diálogo de 3 botones ("Mover a otra
///   ventana" / "Cerrar sin guardar" / "Cancelar").
fn handle_window_close_request(ui: &App, state: &SharedState, id: u32) -> slint::CloseRequestResponse {
    if let Some(response) = app::file_ops::confirm_close_if_needed(ui, state) {
        return response;
    }

    let others = other_id_most_recent_first(id);
    if others.is_empty() {
        app::file_ops::perform_hot_exit(state);
        let _ = slint::quit_event_loop();
        return slint::CloseRequestResponse::HideWindow;
    }

    let has_something_to_lose = {
        let st = state.borrow();
        st.editor.groups.iter().any(|g| {
            g.tabs.iter().any(|t| {
                if t.is_scratch() {
                    !t.document.to_string().is_empty()
                } else {
                    t.is_dirty()
                }
            })
        })
    };
    if !has_something_to_lose {
        close_window_silently(id);
        return slint::CloseRequestResponse::HideWindow;
    }

    let result = rfd::MessageDialog::new()
        .set_title("LightMark")
        .set_description("Esta ventana tiene pestañas sin guardar. ¿Qué quieres hacer?")
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            "Mover a otra ventana".to_string(),
            "Cerrar sin guardar".to_string(),
            "Cancelar".to_string(),
        ))
        .set_level(rfd::MessageLevel::Warning)
        .set_parent(&ui.window().window_handle())
        .show();
    match result {
        rfd::MessageDialogResult::Custom(label) if label == "Mover a otra ventana" => {
            move_all_tabs_to_window(state, others[0]);
            close_window_silently(id);
            slint::CloseRequestResponse::HideWindow
        }
        rfd::MessageDialogResult::Custom(label) if label == "Cerrar sin guardar" => {
            close_window_silently(id);
            slint::CloseRequestResponse::HideWindow
        }
        _ => slint::CloseRequestResponse::KeepWindowShown,
    }
}

// ---------------------------------------------------------------------------
// Timer de 150 ms: trabajo por-ventana (sin cambios de comportamiento respecto a antes de
// E1, salvo que ahora se llama una vez por cada ventana registrada) + agregados de
// una-vez-por-tick (sesión v2 combinada, sondeo de solicitudes).
// ---------------------------------------------------------------------------

fn on_tick(ui: &App, state: &SharedState) {
    let dirty = {
        let mut st = state.borrow_mut();
        std::mem::replace(&mut st.content_dirty, false)
    };
    if dirty {
        app::refresh_on_tick(ui, state);
    }
    app::naming_ops::on_tick(ui, state);
    app::search_ops::search_tick(ui, state);
    autosave_tick(ui, state);
    external_modification_tick(ui, state);
}

fn autosave_tick(ui: &App, state: &SharedState) {
    let (due, autosave_files) = {
        let st = state.borrow();
        let delay = Duration::from_millis(st.settings.autosave_delay_ms as u64);
        (st.last_scratch_autosave.elapsed() >= delay, st.settings.autosave_files)
    };
    if !due {
        return;
    }
    let (scratch_count, scratch_error) = {
        let mut st = state.borrow_mut();
        st.last_scratch_autosave = Instant::now();
        st.editor.save_changed_scratches()
    };

    let mut saved_files = 0usize;
    let mut file_errors: Vec<String> = Vec::new();
    if autosave_files {
        let now = Instant::now();
        let dirty: Vec<(usize, usize, std::path::PathBuf)> = {
            let st = state.borrow();
            st.editor
                .groups
                .iter()
                .enumerate()
                .flat_map(|(gi, g)| {
                    g.tabs
                        .iter()
                        .enumerate()
                        .filter_map(|(ti, t)| {
                            if t.is_scratch() || !t.is_dirty() {
                                return None;
                            }
                            let path = t.file_path.clone()?;
                            if crate::workspace::has_external_changes(&path, t.last_known_mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH)) {
                                return None;
                            }
                            Some((gi, ti, path))
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        for (g, i, path) in dirty {
            let retry_ok = state
                .borrow()
                .autosave_retry_after
                .get(&path)
                .map(|until| now >= *until)
                .unwrap_or(true);
            if !retry_ok {
                continue;
            }
            match app::file_ops::save_tab_to_disk(state, g, i) {
                Ok(Some(_)) => {
                    saved_files += 1;
                    state.borrow_mut().autosave_retry_after.remove(&path);
                }
                Ok(None) => {}
                Err(e) => {
                    state.borrow_mut().autosave_retry_after.insert(path.clone(), now + Duration::from_secs(30));
                    file_errors.push(format!("{}: {}", path.display(), e));
                }
            }
        }
    }

    let mut all_errors: Vec<String> = Vec::new();
    if let Some(e) = &scratch_error {
        all_errors.push(format!("borrador: {}", e));
    }
    all_errors.extend(file_errors);
    if !all_errors.is_empty() {
        let combined = format!("Error de autoguardado — {}", all_errors.join("; "));
        let mut st = state.borrow_mut();
        if st.last_autosave_error_shown.as_deref() != Some(combined.as_str()) {
            st.last_autosave_error_shown = Some(combined.clone());
            drop(st);
            app::set_status(ui, &combined);
        }
    } else {
        state.borrow_mut().last_autosave_error_shown = None;
    }

    if scratch_count > 0 || saved_files > 0 {
        app::refresh_tabs(ui, state);
    }
    // El guardado de sesión ("hot exit") ya NO ocurre aquí (E1): `session_dirty` se deja
    // sin consumir para que `session_tick_multi` lo agregue de TODAS las ventanas y
    // guarde la sesión v2 completa una sola vez por tick, no una vez por ventana.
}

fn external_modification_tick(ui: &App, state: &SharedState) {
    let due = { state.borrow().last_ext_check.elapsed() >= Duration::from_secs(2) };
    if !due {
        return;
    }
    state.borrow_mut().last_ext_check = Instant::now();

    let (path, last_known, is_dirty, group, index) = {
        let st = state.borrow();
        let ag = st.editor.active_group;
        let idx = st.editor.groups.get(ag).map(|g| g.active).unwrap_or(0);
        match st.editor.groups.get(ag).and_then(|g| g.active_tab()) {
            Some(t) => (t.file_path.clone(), t.last_known_mtime, t.is_dirty(), ag, idx),
            None => (None, None, false, ag, idx),
        }
    };
    let (Some(path), Some(last_known)) = (path, last_known) else {
        return;
    };
    if !workspace::has_external_changes(&path, last_known) {
        if state.borrow().external_conflict_warned_for.as_deref() == Some(path.as_path()) {
            state.borrow_mut().external_conflict_warned_for = None;
        }
        return;
    }
    if is_dirty {
        let already_warned = state.borrow().external_conflict_warned_for.as_deref() == Some(path.as_path());
        if !already_warned {
            state.borrow_mut().external_conflict_warned_for = Some(path.clone());
            app::set_status(ui, "El archivo cambió en disco; tienes cambios sin guardar");
        }
        return;
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let raw = if had_bom { &bytes[3..] } else { &bytes[..] };
    let Ok(content) = String::from_utf8(raw.to_vec()) else {
        return;
    };
    let (normalized, line_ending) = textops::normalize_newlines(&content);
    let new_mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
    let reloaded = {
        let mut st = state.borrow_mut();
        match st.editor.groups.get_mut(group).and_then(|g| g.tabs.get_mut(index)) {
            Some(tab) => {
                if tab.document == normalized {
                    tab.last_known_mtime = new_mtime;
                    false
                } else {
                    tab.document = ropey::Rope::from_str(&normalized);
                    tab.saved_snapshot = tab.document.clone();
                    tab.last_known_mtime = new_mtime;
                    tab.had_bom = had_bom;
                    tab.line_ending = line_ending;
                    let clamped = tab.cursor.min(normalized.len());
                    tab.cursor = tab.document.floor_char_boundary(clamped);
                    tab.record_command(tab.cursor);
                    true
                }
            }
            None => false,
        }
    };
    if reloaded {
        app::push_active_document_to_ui(ui, state);
        app::refresh_ui(ui, state);
        app::set_status(ui, "Archivo recargado (cambió en disco)");
    }
}

/// Agregado de sesión v2 (E1, item 8 del plan): consume `session_dirty` de TODAS las
/// ventanas registradas (así que cualquiera que lo haya marcado dispara un guardado
/// completo) y, si alguna lo estaba, guarda una `SessionV2` con la geometría+contenido
/// ACTUAL de cada ventana registrada — una sola escritura por tick, no una por ventana.
fn session_tick_multi(app_data_dir: &Path) {
    let mut any_dirty = false;
    let mut windows_out: Vec<SessionWindowState> = Vec::new();
    let ids = windows::snapshot_ids();
    let mut active_window = 0usize;
    let most_recent = windows::most_recent();
    for (i, id) in ids.iter().enumerate() {
        if most_recent == Some(*id) {
            active_window = i;
        }
        windows::with_window(*id, |ui, state| {
            let mut st = state.borrow_mut();
            if std::mem::replace(&mut st.session_dirty, false) {
                any_dirty = true;
            }
            let pos = ui.window().position();
            let size = ui.window().size();
            let maximized = ui.window().is_maximized();
            let session = st.editor.to_session();
            windows_out.push(SessionWindowState::from_session(session, pos.x, pos.y, size.width as f32, size.height as f32, maximized));
        });
    }
    if any_dirty && !windows_out.is_empty() {
        let sm = SessionManager::new(session::ensure_sessions_dir(app_data_dir));
        let sv2 = SessionV2 { version: 2, windows: windows_out, active_window };
        sm.save_auto_session_v2(&sv2).ok();
    }
}
