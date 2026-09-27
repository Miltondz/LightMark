//! Nombres de borradores (WS-B; D2 extiende): recalcula títulos automáticos y números
//! "Sin título N" de todos los borradores en el temporizador de 150 ms. Es barato (lee como
//! mucho ~4 KB por borrador) y solo refresca la UI cuando algún título cambia.
//!
//! D2 añade el nombrado automático por IA (§9.3 del plan): cuando
//! `ai_auto_name_drafts && ai_consent` están activos, un borrador SIN nombre propio que lleve
//! ≥100 caracteres y esté "quieto" (sin cambios) unos segundos recibe, como mucho UNA vez, una
//! sugerencia silenciosa que se aplica directamente como `custom_title` (sin diálogo). No hay
//! límite de reintentos: si falla o no vuelve a cumplirse la condición, no se reintenta jamás
//! para ese borrador en esta sesión (evita gastar la cuota de la API reintentando sin parar un
//! borrador que, por ejemplo, el proveedor no supo nombrar).

use super::ai_ops::{self, TabId};
use super::{SharedState, refresh_tabs, set_status};
use crate::App;
use crate::ai::client::{self, AiConfig};
use crate::ai::naming;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Debajo de este umbral no vale la pena pedirle un nombre a la IA (plan §9.3: "≥200
/// caracteres"; usamos el mismo umbral mínimo pactado con WS-D2 de 100 para no ser más
/// restrictivos que el disparador manual en documentos cortos pero ya legibles).
const AUTO_MIN_CHARS: usize = 100;
/// Cuántos caracteres del borrador se leen para decidir si está "quieto" y para el snippet.
const AUTO_READ_CHARS: usize = 2200;
/// Tiempo sin cambios exigido antes de disparar (evita pedir un nombre a cada tecleo).
const AUTO_STABLE_AFTER: Duration = Duration::from_secs(5);

thread_local! {
    /// `scratch_id -> (hash del contenido leído, instante en que se vio ese contenido por
    /// primera vez)`: si el hash no cambia durante `AUTO_STABLE_AFTER`, el borrador se
    /// considera "quieto". Se limpia su entrada al disparar (con éxito o no) para no crecer
    /// sin límite mientras el borrador sigue abierto.
    static STABLE: RefCell<HashMap<String, (u64, Instant)>> = RefCell::new(HashMap::new());
    /// `scratch_id`s para los que YA se intentó (con éxito o no): como mucho un intento por
    /// borrador y por sesión de la app.
    static ATTEMPTED: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
}

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Llamado desde el temporizador de 150 ms tras `refresh_on_tick`.
pub fn on_tick(ui: &App, state: &SharedState) {
    let changed = {
        let mut st = state.borrow_mut();
        let opts = crate::scratch::NamingOpts::from_settings(&st.settings);
        st.editor.recompute_titles(&opts)
    };
    if changed {
        refresh_tabs(ui, state);
    }
    // D2: auto AI naming hook
    maybe_auto_name_draft(ui, state);
}

/// Decide, de forma pura, si un borrador es candidato a nombrado automático AHORA (sin
/// consultar el reloj ni la red): sin nombre propio, sin intento previo, y con suficiente
/// contenido. Separado de `maybe_auto_name_draft` para poder testearlo sin `SharedState`/`App`.
fn is_eligible(has_custom_title: bool, already_attempted: bool, snippet_trimmed_len: usize) -> bool {
    !has_custom_title && !already_attempted && snippet_trimmed_len >= AUTO_MIN_CHARS
}

/// Busca, entre todos los borradores abiertos, el primero que cumpla las condiciones de
/// nombrado automático y ya lleve `AUTO_STABLE_AFTER` sin cambios; actualiza `STABLE` de paso
/// (registra el instante en que se vio cada contenido nuevo). Como mucho una llamada de IA por
/// tick, y respeta `ai_busy` (una sola solicitud de IA en curso por ventana).
fn maybe_auto_name_draft(ui: &App, state: &SharedState) {
    if ui.get_ai_busy() || !ui.get_can_ai() {
        return;
    }
    let (ai_enabled, auto_on, consent, n) = {
        let st = state.borrow();
        (st.settings.ai_enabled, st.settings.ai_auto_name_drafts, st.settings.ai_consent, 1u32)
    };
    // Bug de la revisión (privacidad): comprobar `settings.ai_enabled` explícitamente además
    // de `ui.get_can_ai()` — este último depende del timer de 400ms de `ai_ops::wire`
    // (`ai_can_ai_timer`), que puede tardar hasta 400ms en reflejar que la IA se acaba de
    // desactivar. Sin esto, un borrador podía dispararse hacia el proveedor en esa ventana.
    if !ai_enabled || !auto_on || !consent {
        return;
    }
    let now = Instant::now();
    let target = {
        let st = state.borrow();
        let mut found = None;
        'outer: for (gi, g) in st.editor.groups.iter().enumerate() {
            for (ti, t) in g.tabs.iter().enumerate() {
                if !t.is_scratch() {
                    continue;
                }
                let Some(id) = &t.scratch_id else { continue };
                let already = ATTEMPTED.with(|a| a.borrow().contains(id));
                // Recortar antes de convertir a String: un borrador puede ser grande y no hay
                // que copiarlo entero en cada tick de 150ms solo para decidir si está "quieto".
                let snippet: String = t.document.chars().take(AUTO_READ_CHARS).collect();
                let trimmed_len = snippet.trim().chars().count();
                if !is_eligible(t.custom_title.is_some(), already, trimmed_len) {
                    continue;
                }
                let hash = hash_str(&snippet);
                let stable_enough = STABLE.with(|m| {
                    let mut map = m.borrow_mut();
                    match map.get(id.as_str()) {
                        Some(&(h, seen_at)) if h == hash => now.duration_since(seen_at) >= AUTO_STABLE_AFTER,
                        _ => {
                            map.insert(id.clone(), (hash, now));
                            false
                        }
                    }
                });
                if !stable_enough {
                    continue;
                }
                let lang = st.editor.language_of(t);
                found = Some((gi, ti, id.clone(), snippet, lang));
                break 'outer;
            }
        }
        found
    };
    let Some((g, i, scratch_id, snippet, lang)) = target else { return };

    let cfg = match AiConfig::from_settings(&state.borrow().settings) {
        Ok(c) => c,
        Err(_) => {
            // Sin clave configurada pese a `can_ai` (no debería pasar, pero por seguridad):
            // no reintentar este borrador para no repetir el mismo fallo cada tick.
            ATTEMPTED.with(|a| a.borrow_mut().insert(scratch_id.clone()));
            return;
        }
    };
    // Marca "ya intentado" ANTES de la respuesta: un solo intento por borrador y por sesión,
    // tanto si tiene éxito como si falla (pedido explícito del plan: "avoid to retry every
    // tick if it failed").
    ATTEMPTED.with(|a| a.borrow_mut().insert(scratch_id.clone()));
    STABLE.with(|m| m.borrow_mut().remove(&scratch_id));

    let (system, user) = naming::build_prompt(n, &lang, &snippet);
    let hint = (g, i);
    let id = TabId::Scratch(scratch_id);
    // `spawn` ya localiza la ventana de origen por su id en el registro y vuelve a resolver
    // su `SharedState` correcto al recibir la respuesta (ver `ai_ops::spawn`) — no hace falta
    // capturar `state` aquí (`Rc<RefCell<_>>` no es `Send` y `done` debe serlo).
    ai_ops::spawn(
        ui,
        state,
        move || client::complete(&cfg, &system, &user, 200),
        move |ui, state, r| {
            let raw = match r {
                Ok(raw) => raw,
                Err(_) => return, // fallo silencioso: ya no se reintentará (marcado arriba)
            };
            let Some(name) = naming::parse_candidates(&raw, 1).into_iter().next() else { return };
            let Some((g, i)) = ai_ops::find_tab(state, hint, &id) else { return }; // pestaña cerrada mientras tanto
            {
                let mut st = state.borrow_mut();
                let Some(t) = st.editor.groups.get_mut(g).and_then(|gr| gr.tabs.get_mut(i)) else { return };
                if t.custom_title.is_some() {
                    return; // el usuario ya le puso nombre mientras la IA respondía: no pisarlo
                }
                t.custom_title = Some(name.clone());
            }
            refresh_tabs(ui, state);
            set_status(ui, &format!("IA: borrador renombrado a «{name}»"));
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligible_rules() {
        assert!(is_eligible(false, false, AUTO_MIN_CHARS));
        assert!(!is_eligible(true, false, AUTO_MIN_CHARS)); // ya tiene nombre propio
        assert!(!is_eligible(false, true, AUTO_MIN_CHARS)); // ya se intentó antes
        assert!(!is_eligible(false, false, AUTO_MIN_CHARS - 1)); // muy corto
    }

    #[test]
    fn hash_changes_with_content() {
        assert_ne!(hash_str("hola"), hash_str("adios"));
        assert_eq!(hash_str("hola"), hash_str("hola"));
    }
}
