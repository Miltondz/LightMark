//! Cableado de las acciones de IA (WS-D): diálogo de configuración, claves, prueba de
//! conexión y lista de modelos. Las llamadas de red corren en un hilo de fondo (`spawn`) y
//! devuelven el resultado al hilo de la UI con `upgrade_in_event_loop`; jamás se bloquea la UI.
//! La clave API solo pasa por el campo `ai_key_input` (se limpia al guardar) y por
//! `ai::secrets`; nunca se escribe en settings, logs, barra de estado ni mensajes.

use super::{SharedState, set_status};
use crate::App;
use crate::ai::client::{self, AiConfig};
use crate::ai::error::AiError;
use crate::ai::providers::{self, Auth, PROVIDERS};
use crate::ai::{cache, secrets};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Id de la solicitud vigente (0 = ninguna). Un resultado con otro id se descarta.
static ACTIVE: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CAN_AI_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
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

pub fn wire(ui: &App, state: &SharedState) {
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
    // WS-D2 (Guardar como / sugerencias): todavía sin implementar.
    ui.on_ai_suggest_name({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                set_status(&ui, "IA: no implementado");
            }
        }
    });
    ui.on_ai_name_picked({
        let ui_weak = ui.as_weak();
        move |_s| {
            if let Some(ui) = ui_weak.upgrade() {
                set_status(&ui, "IA: no implementado");
            }
        }
    });
    ui.on_ai_name_regenerate({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                set_status(&ui, "IA: no implementado");
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
}
