#![windows_subsystem = "windows"]

//! Punto de entrada. La UI vive por completo en `ui/*.slint` (compilada por `build.rs`
//! vía `slint-build`, expuesta aquí con `slint::include_modules!()`); toda la lógica
//! vive en módulos (`editor`, `textops`, `settings`, `scratch`, `session`, `search`,
//! `workspace`, `format`, `syntax`, `editor_view`, `ai`, `markdown`, `preview`) y el cableado de
//! callbacks en `app/*` (ver `app::wire_all`). Este archivo solo: carga configuración y
//! sesión, crea la ventana, cablea callbacks, aplica los argumentos de línea de
//! comandos, registra el temporizador de 150 ms y el cierre con "hot exit", y arranca
//! el bucle de eventos.

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

use app::{AppState, SharedState};
use editor::EditorState;
use scratch::ScratchManager;
use session::SessionManager;
use settings::Settings;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Instancia única (hallazgo R1_FIXES C.6)
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
/// (`File::try_lock`, estable desde Rust 1.89 — `rustc --version` confirmado en 1.94 al
/// escribir esto; no depende de la crate `windows`, eliminada en el hallazgo #1). El
/// `File` devuelto debe mantenerse vivo durante toda la ejecución de `main`: soltarlo
/// libera el lock. `None` significa que ya hay una instancia primaria activa.
fn try_acquire_single_instance_lock(app_data_dir: &Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(app_data_dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(instance_lock_path(app_data_dir))
        .ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        // Hallazgo W15: `WouldBlock` es la única señal fiable de que YA hay otra
        // instancia primaria con el lock — en ese caso, y SOLO en ese caso, esta
        // instancia debe convertirse en secundaria (reenviar argumentos y salir sin
        // ventana). Cualquier OTRO error (permisos, sistema de archivos sin soporte de
        // locking, antivirus interfiriendo...) no significa que haya otra instancia; lo
        // correcto es seguir como primaria SIN lock (mejor una instancia sin protección
        // contra duplicados que ninguna ventana en absoluto y sin ningún aviso).
        Err(std::fs::TryLockError::WouldBlock) => None,
        Err(std::fs::TryLockError::Error(_)) => Some(file),
    }
}

/// Instancia secundaria: anexa sus argumentos (rutas a abrir) al archivo de solicitudes
/// que la primaria sondea en su temporizador, para que abra esos archivos en su propia
/// ventana en vez de que se abra una segunda ventana.
fn forward_open_requests(app_data_dir: &Path, args: &[String]) {
    // Hallazgo W23: una segunda instancia SIN argumentos (p.ej. hacer doble clic en el
    // acceso directo de LightMark cuando ya hay una ventana abierta) debía poder traer
    // la ventana primaria al frente igual que cuando trae rutas que abrir. Antes, sin
    // argumentos, no se escribía nada en absoluto: la primaria nunca se enteraba de que
    // se había intentado abrir una segunda instancia. Se escribe siempre al menos una
    // línea vacía como marcador ("tráeme al frente"), que `poll_open_requests` ignora
    // como ruta pero sigue tratando como señal para mostrar/enfocar la ventana.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(open_requests_path(app_data_dir))
    {
        if args.is_empty() {
            let _ = writeln!(f);
        } else {
            for a in args {
                // Hallazgo W16: un argumento relativo (p.ej. `lightmark.exe nota.md`
                // desde una carpeta cualquiera) se interpretaba en la instancia
                // PRIMARIA con SU directorio de trabajo, casi nunca el mismo que el de
                // la secundaria que lo lanzó — casi siempre resolvía a un archivo que
                // no existe (`poll_open_requests` filtra con `p.is_file()` y lo
                // descarta en silencio). Se resuelve a absoluta aquí, con el cwd de la
                // instancia que realmente lo recibió como argumento.
                let abs = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let _ = writeln!(f, "{}", abs.to_string_lossy());
            }
        }
    }
}

/// Sondeado por el temporizador de la instancia primaria: si una instancia secundaria
/// dejó rutas pendientes, las abre y borra el archivo. Limitación conocida: Slint 1.18
/// no expone una API multiplataforma para forzar el foco de la ventana del sistema
/// (`SetForegroundWindow` en Win32) sin depender de la crate `windows` que se eliminó a
/// propósito (hallazgo #1); `Window::show()` sí deshace un `hide()`/minimizado propio.
fn poll_open_requests(ui: &App, state: &SharedState, app_data_dir: &Path) {
    let path = open_requests_path(app_data_dir);
    // Hallazgo W16 (segunda parte): leer y luego borrar por separado deja una ventana
    // en la que una instancia secundaria puede haber ABIERTO EN MODO APPEND y estar
    // escribiendo sus rutas justo entre el `read_to_string` y el `remove_file` de
    // aquí — ese contenido se perdería al borrarlo sin haberlo leído. Renombrar
    // primero (operación atómica en el mismo volumen) reclama el archivo entero para
    // esta lectura: cualquier escritura de una secundaria que llegue DESPUÉS del rename
    // crea un archivo nuevo con el nombre original, que se recogerá en el siguiente tick,
    // en vez de perderse.
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
    for p in paths {
        if p.is_file() {
            app::file_ops::open_path(ui, state, p);
        }
    }
    // Hallazgo W23: se muestra/enfoca la ventana primaria SIEMPRE que una segunda
    // instancia haya dejado un archivo de solicitudes (aunque no traiga ninguna ruta
    // válida — p.ej. el marcador vacío de `forward_open_requests` cuando la segunda
    // instancia se lanzó sin argumentos). Antes, sin rutas, se salía sin llamar a
    // `show()`/`focus_editor`, así que abrir una segunda instancia sin argumentos no
    // hacía absolutamente nada visible.
    let _ = ui.window().show();
    ui.invoke_focus_editor();
}

fn main() -> Result<(), slint::PlatformError> {
    let app_data_dir = scratch::get_app_data_dir();
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    // Debe permanecer viva hasta el final de `main` (`ui.run()` incluido): soltarla
    // liberaría el lock y dejaría de considerarse la instancia primaria.
    let _instance_lock = match try_acquire_single_instance_lock(&app_data_dir) {
        Some(lock) => lock,
        None => {
            // Ya hay una instancia primaria: reenviar los argumentos y salir sin crear
            // ventana ni tocar sesión/borradores compartidos (hallazgo C.6).
            forward_open_requests(&app_data_dir, &cli_args);
            return Ok(());
        }
    };

    let settings = Settings::load();
    let scratch_manager = ScratchManager::new(scratch::get_scratch_dir());
    let session_manager = SessionManager::new(session::ensure_sessions_dir(&app_data_dir));

    let mut editor_state = EditorState::new(scratch_manager, session_manager);
    // `restore_from_session` ahora devuelve avisos (archivo cambiado en disco, borrador
    // ilegible, etc. — ver LOGIC_API_CHANGES.md) que se muestran en la barra de estado
    // tras crear la ventana, en vez de perderse en silencio.
    let mut restore_warnings: Vec<String> = Vec::new();
    if settings.restore_session {
        if let Ok(Some(session)) = editor_state.session_manager.load_auto_session() {
            restore_warnings = editor_state.restore_from_session(&session);
        }
    }

    let state: SharedState = Rc::new(RefCell::new(AppState::new(editor_state, settings)));

    let ui = App::new()?;
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    ui.set_shortcuts_list(ModelRc::new(VecModel::from(app::build_shortcuts_list())));

    app::settings_ops::push_settings_to_ui(&ui, &state);
    app::wire_all(&ui, &state);
    app::push_active_document_to_ui(&ui, &state);
    app::refresh_ui(&ui, &state);
    // Hallazgo E7: si la sesión restaurada trae una carpeta de trabajo, el explorador
    // debe mostrarla ya al arrancar (antes quedaba vacío hasta la primera navegación).
    {
        let root = state.borrow().editor.workspace_root.clone();
        if let Some(root) = root {
            app::explorer_ops::refresh_explorer(&ui, &root);
        }
    }
    // Hallazgo W20: antes solo se mostraba el PRIMER aviso de restauración (con un
    // sufijo "(+N más)" que no decía qué eran esos otros N) — cualquier aviso a partir
    // del segundo se perdía en silencio. Se concatenan todos en la barra de estado, para
    // no perder ninguno.
    if !restore_warnings.is_empty() {
        app::set_status(&ui, &restore_warnings.join("; "));
    }

    // Argumentos de línea de comandos (`lightmark.exe archivo1.md archivo2.txt`): el
    // instalador asocia .md/.txt con `"%1"`; sin esto, abrir desde el Explorador de
    // Windows no hacía nada (hallazgo #20). Se aplican después de restaurar la sesión.
    for arg in &cli_args {
        let path = PathBuf::from(arg);
        if path.is_file() {
            app::file_ops::open_path(&ui, &state, path);
        }
    }

    // Cierre de ventana: "hot exit" (filosofía "zero data loss", hallazgo #9). Nunca se
    // pregunta: los borradores se autoguardan, y el contenido sin guardar de archivos
    // sucios queda embebido en la sesión (`EditorState::to_session`) para restaurarse
    // como "sucio" en el próximo inicio.
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.window().on_close_requested(move || {
            // Hallazgo C.7: con `restore_session = false` puede hacer falta confirmar
            // antes de cerrar (ver `confirm_close_if_needed`); con `restore_session =
            // true` (el caso normal) esto es un no-op y se sigue sin preguntar nunca.
            if let Some(ui) = ui_weak.upgrade() {
                if let Some(response) = app::file_ops::confirm_close_if_needed(&ui, &state) {
                    return response;
                }
            }
            app::file_ops::perform_hot_exit(&state);
            slint::CloseRequestResponse::HideWindow
        });
    }

    // Temporizador único de 150 ms: (a) recalcula estadísticas/vista previa solo si el
    // documento cambió desde el último tick, (b) autoguarda borradores (y archivos, si
    // el usuario activó esa opción) al ritmo de `settings.autosave_delay_ms`, (c)
    // comprueba cambios externos del archivo activo cada ~2 s. Sustituye por completo
    // al timer de 50 ms del main.rs original que reconvertía todo el documento a `Rope`
    // y comparaba en cada tick (hallazgo #4).
    let timer = slint::Timer::default();
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        let app_data_dir = app_data_dir.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_tick(&ui, &state, &app_data_dir);
            }
        });
    }

    ui.run()?;
    Ok(())
}

fn on_tick(ui: &App, state: &SharedState, app_data_dir: &Path) {
    let dirty = {
        let mut st = state.borrow_mut();
        std::mem::replace(&mut st.content_dirty, false)
    };
    if dirty {
        // Hallazgo F: el lenguaje detectado (implica `to_string()` + heurísticas sobre
        // todo el documento), el fin de línea, el formato disponible y el modo de vista
        // efectivo ya no se recalculan por tecla (ver `edit_ops::on_text_edited`); como
        // mucho, aquí, a ritmo de ~6-7 veces por segundo mientras se escribe.
        // Hallazgo W13: `refresh_on_tick` hace el trabajo de `refresh_flags` +
        // `refresh_stats` + `refresh_preview_if_visible` reutilizando un solo
        // `document.to_string()`/detección de lenguaje, en vez de que cada una haga el
        // suyo por separado.
        app::refresh_on_tick(ui, state);
    }
    app::naming_ops::on_tick(ui, state);
    // Hallazgo W5: recomputar la búsqueda al ritmo del timer, no en cada tecla.
    app::search_ops::search_tick(ui, state);
    autosave_tick(ui, state);
    external_modification_tick(ui, state);
    // Hallazgo C.6: atender rutas que una segunda instancia haya dejado pendientes.
    poll_open_requests(ui, state, app_data_dir);
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
    // Hallazgo C.5: solo reescribir los borradores que cambiaron desde el último
    // autoguardado (`save_changed_scratches`), no todos en cada tick.
    // Hallazgo C9: la función ya no traga el error en silencio (antes `unwrap_or(0)`).
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
                            // Hallazgo C9: nunca autoguardar por encima de un cambio
                            // EXTERNO detectado en el archivo — se perdería en silencio
                            // esa versión de disco, que el usuario (u otra herramienta)
                            // puede haber escrito después de que LightMark cargara el
                            // archivo. `external_modification_tick` es quien se encarga
                            // de avisar/recargar ese caso; el autoguardado simplemente lo
                            // deja en paz hasta que se resuelva.
                            if crate::workspace::has_external_changes(&path, t.last_known_mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH)) {
                                return None;
                            }
                            Some((gi, ti, path))
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        // Hallazgo W14: usar el guardado "silencioso" (sin `set_status`/`refresh_ui`
        // completo POR archivo) y hacer un único refresco ligero al final — antes,
        // autoguardar N archivos sucios en el mismo tick disparaba N refrescos
        // completos (pestañas + banderas + estadísticas + vista previa + recientes),
        // visiblemente costoso con varios archivos grandes abiertos a la vez.
        for (g, i, path) in dirty {
            // Hallazgo C9: si este archivo falló recientemente, esperar el cooldown
            // antes de reintentar — sin esto, un archivo bloqueado por otro proceso
            // reintentaba (y recreaba su `.bak` de respaldo) en CADA tick mientras
            // seguía bloqueado.
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

    // Hallazgo C9: reportar los errores de autoguardado en la barra de estado UNA vez
    // (no en cada tick mientras el problema persiste) — solo si el mensaje combinado
    // cambió desde la última vez que se mostró.
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

    // Hallazgo W9 (antes C.5, segunda parte): guardar la sesión ("hot exit") cuando algo
    // cambió desde el último autoguardado de sesión, para que un cierre inesperado
    // (crash/corte de luz) no pierda contenido sin guardar. Antes se comparaba una
    // huella `(ruta, longitud)` de las pestañas de archivo sucias, que no detectaba una
    // edición que no cambiara la longitud (p.ej. sustituir un carácter por otro, o
    // deshacer) ni ningún cambio puramente estructural (abrir/cerrar pestañas) cuando no
    // había ninguna pestaña de archivo sucia en ese momento. `session_dirty` se marca en
    // cualquier tecleo, comando discreto, o refresco tras una operación estructural (ver
    // `app::mod::refresh_ui`/`push_active_document_to_ui`).
    let changed = std::mem::replace(&mut state.borrow_mut().session_dirty, false);
    if changed {
        let st = state.borrow();
        let session = st.editor.to_session();
        st.editor.session_manager.save_auto_session(&session).ok();
    }
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
        // Hallazgo C20: sin conflicto (ya sea porque nunca lo hubo o porque se acaba de
        // resolver — guardado, recargado, o el archivo volvió a su mtime conocido) — se
        // limpia el recordatorio de "ya avisado" para esta ruta, si era la que estaba
        // pendiente.
        if state.borrow().external_conflict_warned_for.as_deref() == Some(path.as_path()) {
            state.borrow_mut().external_conflict_warned_for = None;
        }
        return;
    }
    if is_dirty {
        // Hallazgo C20: avisar UNA vez por conflicto, no en cada tick de 2s mientras
        // siga sin resolverse — repetirlo tapaba cualquier otro mensaje más reciente en
        // la barra de estado casi de inmediato.
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
                // Hallazgo W22 (segunda parte): el mtime puede cambiar sin que el
                // CONTENIDO cambie (p.ej. otro proceso reescribe el archivo con bytes
                // idénticos, o solo toca sus metadatos). Comparar el texto evita
                // recargar innecesariamente: no hay nada que perder en la posición del
                // cursor ni un paso de undo espurio que reescribiría el documento por
                // uno idéntico.
                if tab.document == normalized {
                    tab.last_known_mtime = new_mtime;
                    false
                } else {
                    tab.document = ropey::Rope::from_str(&normalized);
                    tab.saved_snapshot = tab.document.clone();
                    tab.last_known_mtime = new_mtime;
                    tab.had_bom = had_bom;
                    tab.line_ending = line_ending;
                    // Hallazgo W22 (primera parte): acotar al límite de carácter más
                    // cercano, no solo a la longitud en bytes — un cursor a mitad de un
                    // carácter multibyte del contenido NUEVO haría panic más adelante
                    // (mismo tipo de problema que W1, aquí con el contenido recargado
                    // en vez de con el tecleo).
                    let clamped = tab.cursor.min(normalized.len());
                    tab.cursor = tab.document.floor_char_boundary(clamped);
                    // Recarga externa = comando discreto deshacible (hallazgo A/R1_FIXES
                    // C.10): un Ctrl+Z tras esto debe poder volver al contenido que
                    // había en memoria.
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
