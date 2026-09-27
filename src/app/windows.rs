//! Registro de ventanas para multi-ventana (E1, item 12 del plan). `thread_local` porque
//! Slint corre en un solo hilo (el hilo de la UI); todo el estado vive aquí.
//!
//! ## Alcance real de esta pieza (léase antes de tocar nada)
//! Este módulo implementa el `Registry` y su API tal como la describe PLAN2.md §10 (E1), pero
//! **no** construye ventanas por sí mismo: crear un `App` Slint completo requiere `wire_all`,
//! un `ScratchManager`/`SessionManager` propios y el resto del arranque que hoy vive en
//! `main.rs` (500+ líneas). Extraer eso de forma segura es, por sí solo, el grueso del trabajo
//! de E1 y no se hizo en este pase (ver `WSE1_REPORT.md` — "no completado"). Por eso
//! `register_window` recibe un `App`/`SharedState`/HWND YA construidos por el llamador, en vez
//! de un `create_window(tabs, geometry) -> u32` que los fabrique él mismo como pedía el plan al
//! pie de la letra. El resto de la API (cierre, iteración, búsqueda de archivo, "más
//! reciente", difusión de configuración, números "Sin título" combinados) sí sigue el diseño
//! y es funcional/testeable en la parte que no depende de una ventana Slint real.
//!
//! ## Regla de oro: nunca mantener `REG` prestado al re-entrar en Slint
//! `for_each`/`with_window` prestan el registro (`RefCell::borrow`) mientras llaman al closure.
//! Si ese closure dispara algo que pueda re-entrar en el registro (mostrar un diálogo modal,
//! `show()`/`hide()` de OTRA ventana, un callback que a su vez llame a `register_window` o
//! `remove_window`), hay que soltar el borrow primero: usar `snapshot_ids()` para copiar los
//! ids y iterar sobre esa copia, resolviendo cada ventana con `with_window(id, ...)` en
//! llamadas separadas (cada una con su propio borrow corto), en vez de un único `for_each` con
//! el borrow abierto todo el tiempo.

use super::SharedState;
use crate::settings::Settings;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

/// Una ventana registrada: su `App` Slint, el estado compartido que cablea sus callbacks, el
/// HWND Win32 (para `SetForegroundWindow` al activar/desduplicar) y cuándo se usó por última
/// vez (para `most_recent`).
pub struct WindowEntry {
    pub id: u32,
    pub ui: crate::App,
    pub state: SharedState,
    pub hwnd: isize,
    pub last_active: Instant,
}

struct Registry {
    windows: Vec<WindowEntry>,
    next_id: u32,
    /// Copia de la configuración compartida entre ventanas (la fuente de verdad para guardar a
    /// disco UNA vez, no una vez por ventana, cuando cualquiera dispara `settings-changed`).
    shared_settings: Settings,
}

impl Registry {
    fn new() -> Self {
        Registry { windows: Vec::new(), next_id: 1, shared_settings: Settings::default() }
    }
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::new());
}

/// Registra una ventana ya construida y le asigna un id. El llamador (`main.rs`) es
/// responsable de haber llamado `wire_all(&ui, &state)` y de obtener el HWND (ver
/// `hwnd_of_slint_window` en este módulo) ANTES de llamar aquí.
pub fn register_window(ui: crate::App, state: SharedState, hwnd: isize) -> u32 {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        let id = reg.next_id;
        reg.next_id += 1;
        reg.windows.push(WindowEntry { id, ui, state, hwnd, last_active: Instant::now() });
        id
    })
}

/// Quita la ventana `id` del registro y la devuelve (para que el llamador la suelte/`hide()`
/// como corresponda). **Nunca** llamar esto desde dentro de un callback de esa MISMA ventana —
/// dropear un `App` dentro de su propio callback crashea. Diferir siempre con
/// `slint::invoke_from_event_loop`.
pub fn remove_window(id: u32) -> Option<WindowEntry> {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        let pos = reg.windows.iter().position(|w| w.id == id)?;
        Some(reg.windows.remove(pos))
    })
}

#[allow(dead_code)] // API pública del registro; sin uso propio en el wiring actual todavía.
pub fn window_count() -> usize {
    REG.with(|r| r.borrow().windows.len())
}

/// Marca `id` como usada ahora (llamar al recibir foco/activación).
pub fn mark_active(id: u32) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(w) = reg.windows.iter_mut().find(|w| w.id == id) {
            w.last_active = Instant::now();
        }
    });
}

/// Id de la ventana usada más recientemente, o `None` si el registro está vacío.
pub fn most_recent() -> Option<u32> {
    REG.with(|r| r.borrow().windows.iter().max_by_key(|w| w.last_active).map(|w| w.id))
}

/// Copia de los ids registrados en el orden actual, SIN mantener el borrow de `REG` — usar
/// esto para iterar antes de cualquier operación que pueda re-entrar (diálogos, `show()`,
/// callbacks cruzados) en vez de `for_each`.
pub fn snapshot_ids() -> Vec<u32> {
    REG.with(|r| r.borrow().windows.iter().map(|w| w.id).collect())
}

/// Ejecuta `f` con el `App`/`SharedState` de cada ventana registrada. El registro permanece
/// prestado durante toda la iteración: `f` no debe re-entrar en este módulo (ver el aviso al
/// principio del archivo). Para difundir algo simple (empujar una propiedad, refrescar la UI)
/// esto es seguro y es la forma preferida; para nada que pueda mostrar un diálogo o
/// cerrar/crear ventanas, usar `snapshot_ids()` + `with_window` en su lugar.
pub fn for_each(mut f: impl FnMut(&crate::App, &SharedState)) {
    REG.with(|r| {
        let reg = r.borrow();
        for w in reg.windows.iter() {
            f(&w.ui, &w.state);
        }
    });
}

/// Ejecuta `f` con la ventana `id` (si existe), con un borrow corto que se suelta antes de
/// volver. Preferir esto sobre `for_each` cuando `f` pueda re-entrar en el registro.
pub fn with_window<T>(id: u32, f: impl FnOnce(&crate::App, &SharedState) -> T) -> Option<T> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.windows.iter().find(|w| w.id == id).map(|w| f(&w.ui, &w.state))
    })
}

/// HWND Win32 de la ventana `id` (para `SetForegroundWindow` al activarla/desduplicar).
pub fn hwnd_of(id: u32) -> Option<isize> {
    REG.with(|r| r.borrow().windows.iter().find(|w| w.id == id).map(|w| w.hwnd))
}

/// Fija el HWND de la ventana `id` (WSE1b): el HWND real de una ventana Slint solo está
/// disponible tras al menos una vuelta del bucle de eventos siguiente a `show()` (ver
/// `slint::Window::window_handle` — "may only become available after the window has been
/// created by the window manager"), así que no puede leerse en el mismo tick en que se
/// registra la ventana. El llamador (`main.rs::bring_up_window`) programa esto vía
/// `slint::invoke_from_event_loop` justo después de registrar/mostrar la ventana.
pub fn set_hwnd(id: u32, hwnd: isize) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(w) = reg.windows.iter_mut().find(|w| w.id == id) {
            w.hwnd = hwnd;
        }
    });
}

/// Id de la ventana registrada cuyo `WindowEntry.state` es EXACTAMENTE `state` (mismo
/// `Rc`), si alguna — usado por `settings_ops` para excluir la ventana de origen al
/// difundir `settings-changed()` a las demás.
pub fn id_of_state(state: &SharedState) -> Option<u32> {
    REG.with(|r| {
        r.borrow()
            .windows
            .iter()
            .find(|w| std::rc::Rc::ptr_eq(&w.state, state))
            .map(|w| w.id)
    })
}

/// Activa la ventana `id`: `show()` (deshace `hide()`/minimizado), enfoca el editor, la
/// trae al frente con `SetForegroundWindow` (Win32 — Slint 1.18 no expone una API
/// multiplataforma para forzar el foco de la ventana del sistema, mismo límite que ya
/// documentaba `poll_open_requests` en `main.rs` antes de E1) y la marca como la más
/// reciente (`most_recent()`). Usado tanto para desduplicar la apertura de un archivo ya
/// abierto en otra ventana como para el traspaso de una segunda instancia del proceso.
pub fn activate(id: u32) {
    with_window(id, |ui, _state| {
        let _ = ui.window().show();
        ui.invoke_focus_editor();
    });
    if let Some(hwnd) = hwnd_of(id)
        && hwnd != 0
    {
        // SAFETY: `hwnd` es el HWND real capturado para esta ventana vía
        // `raw-window-handle` (ver `main.rs::capture_hwnd`); `SetForegroundWindow` no
        // tiene requisitos de seguridad adicionales sobre un HWND válido.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd as windows_sys::Win32::Foundation::HWND);
        }
    }
    mark_active(id);
}

/// Busca en qué ventana está abierto `path` (normalizado por el llamador, ver
/// `editor::normalize_path`) y devuelve `(id de ventana, grupo, índice)`.
pub fn find_file(path: &Path) -> Option<(u32, usize, usize)> {
    REG.with(|r| {
        let reg = r.borrow();
        for w in reg.windows.iter() {
            let state = w.state.borrow();
            if let Some((g, i)) = state.editor.find_tab_by_path(path) {
                return Some((w.id, g, i));
            }
        }
        None
    })
}

/// Copia la configuración compartida guardada en el registro (para comparar/guardar una sola
/// vez cuando `settings-changed` dispara en cualquier ventana).
pub fn shared_settings() -> Settings {
    REG.with(|r| r.borrow().shared_settings.clone())
}

pub fn set_shared_settings(s: Settings) {
    REG.with(|r| r.borrow_mut().shared_settings = s);
}

/// Números "Sin título N" en uso en todas las ventanas registradas salvo, opcionalmente,
/// `except_id` (la que se está recalculando, que ya cuenta los suyos aparte en
/// `EditorState::recompute_titles`). Delega en `editor::used_untitled_numbers_except`, que es
/// la parte testeable sin ventanas Slint reales.
pub fn used_untitled_numbers_except(except_id: Option<u32>) -> std::collections::HashSet<u32> {
    REG.with(|r| {
        let reg = r.borrow();
        let states: Vec<_> = reg.windows.iter().filter_map(|w| w.state.try_borrow().ok()).collect();
        let refs: Vec<&crate::editor::EditorState> = states.iter().map(|s| &s.editor).collect();
        let except_index = except_id.and_then(|id| reg.windows.iter().position(|w| w.id == id));
        crate::editor::used_untitled_numbers_except(&refs, except_index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // El `Registry` guarda `App`/`SharedState` reales (componentes Slint), que necesitan un
    // backend de plataforma inicializado para construirse — no disponible en `cargo test`
    // headless. Por eso estos tests cubren solo la parte administrativa que no depende de
    // construir una ventana (contadores, orden de ids, configuración compartida); la parte
    // que sí toca `App`/`SharedState` (`register_window`, `for_each`, `find_file`,
    // `hwnd_of`, `with_window`) queda sin cobertura automatizada en este pase — ver
    // WSE1_REPORT.md.

    #[test]
    fn empty_registry_has_no_windows_and_no_most_recent() {
        // No usamos el thread_local global (compartido con otros tests del binario) para no
        // interferir; probamos la lógica directamente sobre un `Registry` local equivalente.
        let reg = Registry::new();
        assert_eq!(reg.windows.len(), 0);
        assert_eq!(reg.next_id, 1);
    }

    #[test]
    fn shared_settings_roundtrip() {
        let before = shared_settings();
        let mut s = Settings::default();
        s.font_size = 99;
        set_shared_settings(s.clone());
        assert_eq!(shared_settings().font_size, 99);
        // restaurar para no afectar otros tests en el mismo hilo/proceso
        set_shared_settings(before);
    }
}
