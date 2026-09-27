//! Cableado de las acciones de IA (WS-D): diálogo de configuración, claves, prueba de
//! conexión y lista de modelos. Las llamadas de red corren en un hilo de fondo (`spawn`) y
//! devuelven el resultado al hilo de la UI con `upgrade_in_event_loop`; jamás se bloquea la UI.
//! La clave API solo pasa por el campo `ai_key_input` (se limpia al guardar) y por
//! `ai::secrets`; nunca se escribe en settings, logs, barra de estado ni mensajes.

use super::{SharedState, refresh_tabs, set_status};
use crate::App;
use crate::ai::client::{self, AiConfig};
use crate::ai::error::AiError;
use crate::ai::naming;
use crate::ai::providers::{self, Auth, PROVIDERS};
use crate::ai::{cache, secrets};
use crate::editor::Tab;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Longitud mínima (en caracteres, tras recortar espacios) del documento para que valga la
/// pena pedirle un nombre a la IA (evita gastar una llamada en un borrador casi vacío).
const MIN_CHARS_FOR_SUGGESTION: usize = 20;
/// Caracteres del documento que se leen para el snippet (coincide con `ai::naming::SNIPPET_CHARS`
/// más un margen; el recorte exacto a 2000 lo hace `naming::build_prompt`).
const SNIPPET_READ_CHARS: usize = 2200;

/// Identidad estable de una pestaña (sobrevive a reordenar pestañas o cambiar de grupo, pero
/// no a que la pestaña se cierre): permite comprobar, cuando llega la respuesta de la IA, si
/// la pestaña de origen sigue existiendo antes de aplicarle nada.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TabId {
    Scratch(String),
    File(PathBuf),
}

pub(crate) fn tab_id(t: &Tab) -> Option<TabId> {
    if let Some(id) = &t.scratch_id {
        Some(TabId::Scratch(id.clone()))
    } else {
        t.file_path.clone().map(TabId::File)
    }
}

/// Busca la posición actual (grupo, índice) de una pestaña por identidad: primero comprueba
/// `hint` (rápido, normalmente sigue siendo válido), y si no coincide recorre todo el estado
/// (la pestaña puede haberse reordenado o cambiado de grupo mientras la IA respondía).
/// `None` si la pestaña ya no existe (se cerró) — el llamador debe descartar el resultado.
pub(crate) fn find_tab(state: &SharedState, hint: (usize, usize), id: &TabId) -> Option<(usize, usize)> {
    find_tab_in(&state.borrow().editor, hint, id)
}

/// Núcleo puro de `find_tab` (testeable sin `AppState`/`SharedState`).
fn find_tab_in(editor: &crate::editor::EditorState, hint: (usize, usize), id: &TabId) -> Option<(usize, usize)> {
    if let Some(t) = editor.groups.get(hint.0).and_then(|g| g.tabs.get(hint.1))
        && tab_id(t).as_ref() == Some(id)
    {
        return Some(hint);
    }
    for (gi, g) in editor.groups.iter().enumerate() {
        for (ti, t) in g.tabs.iter().enumerate() {
            if tab_id(t).as_ref() == Some(id) {
                return Some((gi, ti));
            }
        }
    }
    None
}

fn active_tab_pos(state: &SharedState) -> Option<(usize, usize)> {
    let st = state.borrow();
    let g = st.editor.active_group;
    let i = st.editor.groups.get(g)?.active;
    Some((g, i))
}

/// Nombre propuesto para "Guardar como" cuando la pestaña es un archivo real: conserva su
/// extensión (el usuario pidió un nombre, no un tipo de archivo distinto).
fn file_default_name(tab: &Tab, name: &str) -> String {
    match std::path::Path::new(&tab.title).extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{name}.{ext}"),
        _ => name.to_string(),
    }
}

thread_local! {
    /// Pestaña a la que se aplicará el candidato elegido en `NamePickerDialog` (grupo, índice
    /// en el momento de pedir las sugerencias, identidad estable). `ai-name-regenerate` y
    /// `ai-name-picked` la leen; se sobrescribe en cada nueva solicitud manual.
    static NAME_TARGET: RefCell<Option<(usize, usize, TabId)>> = const { RefCell::new(None) };
}

/// Id de la solicitud vigente (0 = ninguna). Un resultado con otro id se descarta.
static ACTIVE: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CAN_AI_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    /// Copia de `SharedState` accesible desde el cuerpo de un `done` de `spawn` SIN capturarla
    /// (`SharedState` = `Rc<RefCell<_>>` no es `Send`, y `done` debe serlo para viajar dentro
    /// del cierre que `spawn` manda a `std::thread::spawn`). El cierre de `done` solo se
    /// EJECUTA en el hilo de la UI (lo garantiza `upgrade_in_event_loop`), así que leer esta
    /// TLS ahí siempre encuentra la copia poblada en `wire()` en ese mismo hilo.
    static STATE_TLS: RefCell<Option<SharedState>> = const { RefCell::new(None) };
}

/// Copia de `SharedState` para usar dentro de un cuerpo de `done` (hilo de la UI). Ver `STATE_TLS`.
pub(crate) fn current_state() -> Option<SharedState> {
    STATE_TLS.with(|c| c.borrow().clone())
}

fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// Prefijo "Error: " para que el diálogo pinte el estado en rojo.
pub fn error_text(e: &AiError) -> String {
    format!("Error: {}", e.message)
}

/// `can_ai`: IA activada y proveedor utilizable (clave si la exige, URL si es editable).
pub fn compute_can_ai(enabled: bool, needs_key: bool, key_stored: bool, url_editable: bool, url: &str) -> bool {
    enabled && (!needs_key || key_stored) && (!url_editable || !url.trim().is_empty())
}

/// Lanza `job` en un hilo de fondo; `done` se ejecuta en el hilo de la UI solo si la solicitud
/// sigue vigente. Devuelve `false` (sin hacer nada) si ya hay una en curso.
pub fn spawn<T: Send + 'static>(
    ui: &App,
    job: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&App, T) + Send + 'static,
) -> bool {
    if ui.get_ai_busy() {
        return false;
    }
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    ACTIVE.store(id, Ordering::SeqCst);
    ui.set_ai_busy(true);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let result = job();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            if ACTIVE.compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                ui.set_ai_busy(false);
                done(&ui, result);
            }
        });
    });
    true
}

/// Cancela la solicitud en curso: su resultado se ignorará al llegar.
pub fn cancel(ui: &App) {
    ACTIVE.store(0, Ordering::SeqCst);
    ui.set_ai_busy(false);
    ui.set_ai_status("Operación cancelada.".into());
}

/// Lista para el desplegable del proveedor: catálogo ∪ caché en vivo.
fn model_list_for(pid: &str) -> Vec<String> {
    let p = providers::find_or_default(pid);
    cache::merge_models(p.catalog, &cache::cached_models(pid))
}

fn selected_provider(ui: &App) -> &'static providers::Provider {
    PROVIDERS.get(ui.get_ai_provider_index().max(0) as usize).unwrap_or(&PROVIDERS[0])
}

/// Empuja a la UI el estado derivado del proveedor guardado (nunca la clave).
pub fn sync_provider_ui(ui: &App, state: &SharedState) {
    let (pid, model) = {
        let st = state.borrow();
        (st.settings.ai_provider.clone(), st.settings.ai_model.clone())
    };
    let p = providers::find_or_default(&pid);
    ui.set_ai_provider_index(providers::index_of(p.id) as i32);
    // La fila de clave se muestra también en "custom" (clave opcional).
    ui.set_ai_needs_key(p.auth != Auth::None);
    ui.set_ai_base_url_editable(p.base_url_editable);
    let models = model_list_for(p.id);
    if model.trim().is_empty() {
        let m = if p.cheap_model.is_empty() { models.first().cloned().unwrap_or_default() } else { p.cheap_model.to_string() };
        ui.set_ai_model(m.into());
    }
    ui.set_ai_model_names(strings(models));
    ui.set_ai_key_stored(secrets::has_key(p.id));
}

/// Config desde los ajustes + los campos actuales del diálogo (aún sin aplicar).
fn current_config(ui: &App, state: &SharedState) -> Result<AiConfig, AiError> {
    let mut s = state.borrow().settings.clone();
    s.ai_model = ui.get_ai_model().to_string();
    s.ai_base_url = ui.get_ai_base_url().to_string();
    AiConfig::from_settings(&s)
}

fn on_provider_changed(ui: &App, state: &SharedState, idx: i32) {
    let p = PROVIDERS.get(idx.max(0) as usize).unwrap_or(&PROVIDERS[0]);
    ACTIVE.store(0, Ordering::SeqCst);
    ui.set_ai_busy(false);
    let cached = cache::cached_models(p.id);
    let model = if p.cheap_model.is_empty() { cached.first().cloned().unwrap_or_default() } else { p.cheap_model.to_string() };
    let base = if p.base_url_editable { p.default_base_url.to_string() } else { String::new() };
    {
        let mut st = state.borrow_mut();
        st.settings.ai_provider = p.id.to_string();
        st.settings.ai_model = model.clone();
        st.settings.ai_base_url = base.clone();
        st.settings.save().ok();
    }
    ui.set_ai_model(model.into());
    ui.set_ai_base_url(base.into());
    ui.set_ai_key_input("".into());
    ui.set_ai_status("".into());
    sync_provider_ui(ui, state);
}

fn on_save_key(ui: &App, state: &SharedState) {
    let p = selected_provider(ui);
    let raw = ui.get_ai_key_input().to_string();
    match secrets::store_key(p.id, &raw) {
        Ok(()) => {
            ui.set_ai_key_input("".into());
            ui.set_ai_key_stored(true);
            ui.set_ai_status("Clave guardada en el Administrador de credenciales de Windows.".into());
        }
        Err(e) => ui.set_ai_status(format!("Error: {e}").into()),
    }
    let _ = state;
}

fn on_delete_key(ui: &App) {
    let p = selected_provider(ui);
    match secrets::delete_key(p.id) {
        Ok(()) => {
            ui.set_ai_key_input("".into());
            ui.set_ai_key_stored(false);
            ui.set_ai_status("Clave borrada del Administrador de credenciales.".into());
        }
        Err(e) => ui.set_ai_status(format!("Error: {e}").into()),
    }
}

fn on_test_connection(ui: &App, state: &SharedState) {
    if ui.get_ai_busy() {
        cancel(ui); // el botón se convierte en "Cancelar" mientras hay una solicitud en curso
        return;
    }
    let cfg = match current_config(ui, state) {
        Ok(c) => c,
        Err(e) => return ui.set_ai_status(error_text(&e).into()),
    };
    if let Err(e) = client::check_key(&cfg) {
        return ui.set_ai_status(error_text(&e).into());
    }
    ui.set_ai_status("Probando conexión…".into());
    spawn(
        ui,
        move || client::test_connection(&cfg),
        |ui, r| match r {
            Ok(msg) => ui.set_ai_status(msg.into()),
            Err(e) => ui.set_ai_status(error_text(&e).into()),
        },
    );
}

fn on_refresh_models(ui: &App, state: &SharedState) {
    if ui.get_ai_busy() {
        cancel(ui);
        return;
    }
    let cfg = match current_config(ui, state) {
        Ok(c) => c,
        Err(e) => return ui.set_ai_status(error_text(&e).into()),
    };
    let pid = cfg.provider.id;
    ui.set_ai_status("Actualizando lista de modelos…".into());
    spawn(
        ui,
        move || {
            let list = client::list_models(&cfg)?;
            cache::put(pid, list.clone()).ok(); // un fallo de caché no invalida la lista
            Ok::<Vec<String>, AiError>(list)
        },
        move |ui, r| match r {
            Ok(list) => {
                let n = list.len();
                if selected_provider(ui).id == pid {
                    let p = providers::find_or_default(pid);
                    let merged = cache::merge_models(p.catalog, &list);
                    if ui.get_ai_model().trim().is_empty() {
                        ui.set_ai_model(merged.first().cloned().unwrap_or_default().into());
                    }
                    ui.set_ai_model_names(strings(merged));
                }
                ui.set_ai_status(format!("Lista actualizada: {n} modelos disponibles.").into());
            }
            Err(e) => ui.set_ai_status(error_text(&e).into()),
        },
    );
}

/// Aplica un nombre ya elegido (del picker o de una sugerencia única) a la pestaña `hint`/`id`
/// si sigue existiendo. Borrador → `custom_title`; archivo real → abre "Guardar como" con ese
/// nombre (conservando la extensión), tal como pide el plan §9.3.
fn apply_name_choice(ui: &App, state: &SharedState, hint: (usize, usize), id: &TabId, raw: &str) {
    let Some((g, i)) = find_tab(state, hint, id) else {
        set_status(ui, "IA: la pestaña ya no existe; se descarta la sugerencia.");
        return;
    };
    let name = naming::sanitize_name(raw);
    if name.is_empty() {
        set_status(ui, "IA: el nombre no es válido.");
        return;
    }
    let is_scratch = match state.borrow().editor.groups.get(g).and_then(|gr| gr.tabs.get(i)) {
        Some(t) => t.is_scratch(),
        None => return,
    };
    if is_scratch {
        {
            let mut st = state.borrow_mut();
            let opts = crate::scratch::NamingOpts::from_settings(&st.settings);
            if let Some(t) = st.editor.groups.get_mut(g).and_then(|gr| gr.tabs.get_mut(i)) {
                t.custom_title = Some(name.clone());
            }
            st.editor.recompute_titles(&opts);
        }
        refresh_tabs(ui, state);
        set_status(ui, &format!("Nombre aplicado: «{name}»"));
    } else {
        let default_name = state
            .borrow()
            .editor
            .groups
            .get(g)
            .and_then(|gr| gr.tabs.get(i))
            .map(|t| file_default_name(t, &name));
        if let Some(default_name) = default_name {
            super::file_ops::save_as_tab_with_name(ui, state, g, i, Some(default_name));
        }
    }
}

/// Pide a la IA `ai_name_candidates` sugerencias de nombre para la pestaña `(g, i)`. Guarda la
/// identidad en `NAME_TARGET` para que el picker/"Regenerar" sepan a qué pestaña aplicar el
/// resultado. Descarta el resultado si la pestaña ya no existe cuando llega la respuesta.
fn request_name_suggestions(ui: &App, state: &SharedState, g: usize, i: usize) {
    let built = {
        let st = state.borrow();
        let Some(t) = st.editor.groups.get(g).and_then(|gr| gr.tabs.get(i)) else {
            set_status(ui, "IA: no hay ninguna pestaña activa.");
            return;
        };
        let Some(id) = tab_id(t) else {
            set_status(ui, "IA: no se pudo identificar la pestaña.");
            return;
        };
        // Ropey: recorrer solo los primeros caracteres, sin convertir todo el documento
        // (puede pesar varios MB) a String.
        let snippet: String = t.document.chars().take(SNIPPET_READ_CHARS).collect();
        if snippet.trim().chars().count() < MIN_CHARS_FOR_SUGGESTION {
            set_status(ui, "IA: el documento es demasiado corto para sugerir un nombre.");
            return;
        }
        let lang = st.editor.language_of(t);
        let n = st.settings.ai_name_candidates.clamp(1, 5);
        (id, lang, snippet, n)
    };
    let (id, lang, snippet, n) = built;

    let cfg = match AiConfig::from_settings(&state.borrow().settings) {
        Ok(c) => c,
        Err(e) => return ui.set_ai_status(error_text(&e).into()),
    };
    if let Err(e) = client::check_key(&cfg) {
        return ui.set_ai_status(error_text(&e).into());
    }
    let (system, user) = naming::build_prompt(n, &lang, &snippet);
    NAME_TARGET.with(|c| *c.borrow_mut() = Some((g, i, id.clone())));
    ui.set_ai_status("IA: generando sugerencias de nombre…".into());

    let hint = (g, i);
    let single = n == 1;
    let started = spawn(
        ui,
        move || client::complete(&cfg, &system, &user, 300),
        move |ui, r| match r {
            Ok(raw) => {
                let cands = naming::parse_candidates(&raw, n as usize);
                if cands.is_empty() {
                    ui.set_ai_status("IA: el proveedor no devolvió sugerencias válidas.".into());
                    return;
                }
                if single {
                    // `state` no se puede capturar en `done` (no es `Send`); se recupera de la
                    // TLS poblada en `wire()` — este cuerpo solo corre en el hilo de la UI.
                    if let Some(state) = current_state() {
                        apply_name_choice(ui, &state, hint, &id, &cands[0]);
                    }
                    return;
                }
                ui.set_ai_name_suggestions(strings(cands.clone()));
                ui.set_ai_name_choice(cands[0].clone().into());
                ui.set_ai_status("".into());
                ui.set_show_name_picker(true);
            }
            Err(e) => ui.set_ai_status(error_text(&e).into()),
        },
    );
    if !started {
        // No debería pasar (ya comprobamos ai_busy antes de llegar aquí), pero por si dos
        // disparadores (manual + automático) coinciden exactamente: no perder el estado.
        ui.set_ai_status("IA: ya hay una solicitud en curso.".into());
    }
}

/// `ai-suggest-name`: acción manual del menú/barra de herramientas sobre la pestaña activa.
fn on_suggest_name(ui: &App, state: &SharedState) {
    if ui.get_ai_busy() {
        // Segundo clic mientras ya hay una solicitud en curso: se ignora (a lo sumo una
        // solicitud de IA a la vez por ventana, como pide el plan). El botón de "Probar
        // conexión"/"Actualizar lista" sí se reconvierte en Cancelar; aquí no hay un botón
        // visible que lo pida, así que solo avisamos por la barra de estado.
        set_status(ui, "IA: ya hay una solicitud en curso.");
        return;
    }
    if !ui.get_can_ai() {
        sync_provider_ui(ui, state);
        ui.set_ai_status("Configura la IA (proveedor y clave) para poder sugerir nombres.".into());
        ui.set_show_ai_settings(true);
        return;
    }
    let Some((g, i)) = active_tab_pos(state) else {
        set_status(ui, "IA: no hay ninguna pestaña activa.");
        return;
    };
    request_name_suggestions(ui, state, g, i);
}

pub fn wire(ui: &App, state: &SharedState) {
    STATE_TLS.with(|c| *c.borrow_mut() = Some(state.clone()));
    ui.set_ai_provider_names(strings(PROVIDERS.iter().map(|p| p.name.to_string()).collect()));
    sync_provider_ui(ui, state);

    ui.on_ai_open_settings({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_ai_key_input("".into());
                ui.set_ai_status("".into());
                sync_provider_ui(&ui, &state);
                ui.set_show_ai_settings(true);
            }
        }
    });
    ui.on_ai_provider_changed({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move |i| {
            if let Some(ui) = ui_weak.upgrade() {
                on_provider_changed(&ui, &state, i);
            }
        }
    });
    ui.on_ai_refresh_models({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_refresh_models(&ui, &state);
            }
        }
    });
    ui.on_ai_save_key({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_save_key(&ui, &state);
            }
        }
    });
    ui.on_ai_delete_key({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_delete_key(&ui);
            }
        }
    });
    ui.on_ai_test_connection({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_test_connection(&ui, &state);
            }
        }
    });
    // WS-D2: sugerencia manual de nombre + picker (§9.3).
    ui.on_ai_suggest_name({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                on_suggest_name(&ui, &state);
            }
        }
    });
    ui.on_ai_name_picked({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move |s| {
            if let Some(ui) = ui_weak.upgrade() {
                match NAME_TARGET.with(|c| c.borrow().clone()) {
                    Some((g, i, id)) => apply_name_choice(&ui, &state, (g, i), &id, &s),
                    None => set_status(&ui, "IA: no hay una sugerencia pendiente."),
                }
            }
        }
    });
    ui.on_ai_name_regenerate({
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                if ui.get_ai_busy() {
                    return;
                }
                let Some((g, i, id)) = NAME_TARGET.with(|c| c.borrow().clone()) else { return };
                match find_tab(&state, (g, i), &id) {
                    Some((gg, ii)) => request_name_suggestions(&ui, &state, gg, ii),
                    None => {
                        ui.set_ai_status("IA: la pestaña ya no existe.".into());
                        ui.set_show_name_picker(false);
                    }
                }
            }
        }
    });

    // Mantiene `can_ai` al día (ai_enabled/clave/URL cambian desde el diálogo) y registra el
    // consentimiento la primera vez que se activan las funciones de IA.
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(400), {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let p = selected_provider(&ui);
            let ok = compute_can_ai(
                ui.get_ai_enabled(),
                p.needs_key,
                ui.get_ai_key_stored(),
                p.base_url_editable,
                &ui.get_ai_base_url(),
            );
            if ui.get_can_ai() != ok {
                ui.set_can_ai(ok);
            }
            if ui.get_ai_enabled() {
                if let Ok(mut st) = state.try_borrow_mut() {
                    if !st.settings.ai_consent {
                        st.settings.ai_consent = true;
                        st.settings.save().ok();
                    }
                }
            }
        }
    });
    CAN_AI_TIMER.with(|t| *t.borrow_mut() = Some(timer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn can_ai_rules() {
        assert!(!compute_can_ai(false, true, true, false, ""));
        assert!(!compute_can_ai(true, true, false, false, ""));
        assert!(compute_can_ai(true, true, true, false, ""));
        assert!(compute_can_ai(true, false, false, true, "http://localhost:11434/v1"));
        assert!(!compute_can_ai(true, false, false, true, "  "));
    }

    #[test]
    fn error_text_has_red_prefix() {
        let e = AiError::new(crate::ai::error::AiErrorKind::Auth, "x");
        assert_eq!(error_text(&e), "Error: x");
    }

    #[test]
    fn model_list_includes_star_first() {
        let l = model_list_for("gemini");
        assert_eq!(l[0], "gemini-3.1-flash-lite");
    }

    // -- Identidad de pestaña / "descartar si la pestaña ya no existe" (WS-D2) --------------

    fn temp_editor(name: &str) -> (crate::editor::EditorState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-ai-ops-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(dir.join("scratch")).unwrap();
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        let sm = crate::scratch::ScratchManager::new(dir.join("scratch"));
        let sesm = crate::session::SessionManager::new(dir.join("sessions"));
        (crate::editor::EditorState::new(sm, sesm), dir)
    }

    #[test]
    fn tab_id_uses_scratch_id_then_file_path() {
        let (mut state, dir) = temp_editor("tabid");
        // El borrador inicial de `new` ya tiene scratch_id.
        let id0 = tab_id(&state.groups[0].tabs[0]).unwrap();
        assert!(matches!(id0, TabId::Scratch(_)));
        state.groups[0].tabs[0].file_path = Some(PathBuf::from("C:/x/nota.md"));
        state.groups[0].tabs[0].scratch_id = None;
        let id1 = tab_id(&state.groups[0].tabs[0]).unwrap();
        assert_eq!(id1, TabId::File(PathBuf::from("C:/x/nota.md")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_tab_follows_a_moved_tab_and_none_if_closed() {
        let (mut state, dir) = temp_editor("findtab");
        state.new_scratch_tab(); // ahora hay 2 pestañas: [0]=Sin título1, [1]=Sin título2
        let id = tab_id(&state.groups[0].tabs[1]).unwrap();
        // Simular que se reordenó: intercambiar posiciones 0 y 1 directamente.
        state.groups[0].tabs.swap(0, 1);
        assert_eq!(find_tab_in(&state, (1, 0), &id), Some((0, 0)));
        // Cerrar la pestaña (quitarla del vector): ya no se encuentra.
        state.groups[0].tabs.remove(0);
        assert_eq!(find_tab_in(&state, (0, 0), &id), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_default_name_keeps_extension_or_none() {
        let mut t = Tab::new_file(PathBuf::from("informe.txt"), Rope::new(), None);
        assert_eq!(file_default_name(&t, "Nuevo nombre"), "Nuevo nombre.txt");
        t.file_path = Some(PathBuf::from("README"));
        t.title = "README".to_string();
        assert_eq!(file_default_name(&t, "Léame"), "Léame");
    }
}
