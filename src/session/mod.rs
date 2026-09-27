// El wiring actual solo usa la sesión automática (`save_auto_session`/
// `load_auto_session`, "hot exit"); el resto de la API (sesiones nombradas,
// listar/borrar) queda para una futura función "Guardar sesión como...".
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TabState {
    pub document_id: String,
    pub path: Option<String>,
    pub title: String,
    pub is_scratch: bool,
    pub cursor: usize,
    pub scroll_line: usize,
    pub pinned: bool,
    /// Contenido no guardado de un archivo (para restaurar tras un cierre en caliente / "hot
    /// exit"). `None` si la pestaña estaba guardada o es un borrador (que se autoguarda solo).
    #[serde(default)]
    pub unsaved_content: Option<String>,
    /// mtime del archivo (segundos desde época Unix) en el momento de guardar la sesión
    /// (hallazgo C3): permite detectar en la restauración si el archivo cambió en disco desde
    /// entonces y avisar en vez de sobrescribir en silencio.
    #[serde(default)]
    pub mtime: Option<u64>,
    /// Nombre puesto por el usuario/IA (WS-B).
    #[serde(default)]
    pub custom_title: Option<String>,
    /// Creación del borrador, segundos Unix (para la fecha en el nombre).
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub untitled_n: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GroupState {
    pub id: usize,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub version: u32,
    pub name: String,
    pub created_at: u64,
    pub groups: Vec<GroupState>,
    pub active_group: usize,
    pub group_count: usize,
    pub workspace_root: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Default Session".to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            groups: vec![],
            active_group: 0,
            group_count: 1,
            workspace_root: None,
        }
    }
}

pub struct SessionManager {
    base_dir: PathBuf,
}

impl SessionManager {
    /// Usa `base_dir` directamente como directorio de sesiones (el llamador ya debe pasar el
    /// directorio final, p.ej. `<app_data>/sessions`; antes anidaba `sessions/sessions`).
    pub fn new(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self { base_dir }
    }

    pub fn sessions_dir(&self) -> &Path {
        &self.base_dir
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", session_id))
    }

    pub fn save_session(&self, session: &Session, session_id: &str) -> std::io::Result<()> {
        let path = self.session_path(session_id);
        let data = serde_json::to_string_pretty(session)?;
        // Escritura atómica real (hallazgo L10): `workspace::atomic_save` hace `sync_all()`
        // antes del rename (el `std::fs::write` + `rename` manual de antes no garantizaba que
        // los datos llegaran a disco antes del rename) y borra el `.tmp` si algo falla a mitad
        // de camino en vez de dejarlo huérfano.
        crate::workspace::atomic_save(&path, &data)
    }

    pub fn load_session(&self, session_id: &str) -> std::io::Result<Session> {
        let path = self.session_path(session_id);
        let data = std::fs::read_to_string(path)?;
        // Quita el BOM UTF-8 si lo hay (hallazgo M4): un session.json editado/guardado con
        // Notepad/PowerShell puede llevarlo y rompería el parseo JSON.
        let data = data.strip_prefix('\u{FEFF}').unwrap_or(&data);
        let session: Session = serde_json::from_str(data)?;
        Ok(session)
    }

    pub fn list_sessions(&self) -> std::io::Result<Vec<String>> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new("json"))
                && let Some(stem) = path.file_stem()
            {
                sessions.push(stem.to_string_lossy().to_string());
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    pub fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn auto_session_id() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("session-{}", timestamp)
    }

    pub fn save_auto_session(&self, session: &Session) -> std::io::Result<()> {
        // Save only to the fixed auto-restore session file to prevent disk flooding
        self.save_session(session, "auto-restore")?;
        Ok(())
    }

    pub fn load_auto_session(&self) -> std::io::Result<Option<Session>> {
        match self.load_session("auto-restore") {
            Ok(session) => Ok(Some(session)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Estado de una ventana individual en la sesión v2 (E1 — multi-ventana). Sustituye a los
/// campos "planos" de `Session` (que ahora describen una sola ventana implícita).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    pub groups: Vec<GroupState>,
    pub active_group: usize,
    pub workspace_root: Option<String>,
}

/// Formato de sesión v2: una lista de ventanas en vez de un único conjunto de grupos.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionV2 {
    pub version: u32,
    pub windows: Vec<WindowState>,
    pub active_window: usize,
}

/// Geometría por defecto para la ventana única que resulta de migrar una sesión v1 (que no
/// guardaba posición/tamaño de ventana).
pub const DEFAULT_WINDOW_X: i32 = 100;
pub const DEFAULT_WINDOW_Y: i32 = 100;
pub const DEFAULT_WINDOW_WIDTH: f32 = 1024.0;
pub const DEFAULT_WINDOW_HEIGHT: f32 = 768.0;

impl SessionV2 {
    /// Migra una `Session` v1 (grupos "sueltos") a v2 (una sola ventana con esos grupos).
    pub fn from_v1(s: Session) -> Self {
        SessionV2 {
            version: 2,
            windows: vec![WindowState {
                x: DEFAULT_WINDOW_X,
                y: DEFAULT_WINDOW_Y,
                width: DEFAULT_WINDOW_WIDTH,
                height: DEFAULT_WINDOW_HEIGHT,
                maximized: false,
                groups: s.groups,
                active_group: s.active_group,
                workspace_root: s.workspace_root,
            }],
            active_window: 0,
        }
    }
}

impl Default for SessionV2 {
    fn default() -> Self {
        SessionV2::from_v1(Session::default())
    }
}

impl WindowState {
    /// Extrae los campos "de contenido" (grupos/pestañas/carpeta) como una `Session` v1
    /// suelta, reutilizando `EditorState::to_session`/`restore_from_session` (que ya
    /// conocen el formato de `GroupState`/`TabState`) también para una sola ventana de la
    /// sesión v2, en vez de duplicar esa lógica aquí.
    pub fn to_session(&self) -> Session {
        Session {
            version: 1,
            name: "window".to_string(),
            created_at: 0,
            groups: self.groups.clone(),
            active_group: self.active_group,
            group_count: self.groups.len(),
            workspace_root: self.workspace_root.clone(),
        }
    }

    /// Inversa de `to_session`: combina el contenido de una `Session` (tal como la
    /// produce `EditorState::to_session`) con la geometría/maximizado de esta ventana.
    pub fn from_session(session: Session, x: i32, y: i32, width: f32, height: f32, maximized: bool) -> Self {
        WindowState {
            x,
            y,
            width,
            height,
            maximized,
            groups: session.groups,
            active_group: session.active_group,
            workspace_root: session.workspace_root,
        }
    }
}

/// Clampa/cascada la posición guardada de una ventana (E1): si el rectángulo
/// `(x, y, width, height)` no solapa en absoluto el área virtual conectada actualmente
/// (`vx, vy, vw, vh` — de `GetSystemMetrics(SM_XVIRTUALSCREEN/...)`, el llamador es quien
/// consulta Win32), la posición guardada quedó fuera de pantalla (p.ej. se desconectó un
/// monitor secundario donde vivía esa ventana) y se sustituye por una posición visible
/// cerca de la esquina superior izquierda del área virtual, desplazada `index * 32px`
/// para que varias ventanas restauradas a la vez no queden exactamente superpuestas.
/// Si el rectángulo SÍ solapa el área virtual (aunque sea parcialmente, o esté
/// completamente contenido en un monitor que ya no es el principal pero sigue
/// conectado), se devuelve sin tocar.
pub fn clamp_window_position(
    x: i32,
    y: i32,
    width: f32,
    height: f32,
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
    index: u32,
) -> (i32, i32) {
    let w = width.max(1.0) as i32;
    let h = height.max(1.0) as i32;
    let visible = x < vx + vw && x + w > vx && y < vy + vh && y + h > vy;
    if visible {
        (x, y)
    } else {
        let offset = (index as i32) * 32;
        (vx + 60 + offset, vy + 60 + offset)
    }
}

impl SessionManager {
    /// Análogo a `save_session` pero para el formato v2 (multi-ventana).
    pub fn save_session_v2(&self, session: &SessionV2, session_id: &str) -> std::io::Result<()> {
        let path = self.session_path(session_id);
        let data = serde_json::to_string_pretty(session)?;
        crate::workspace::atomic_save(&path, &data)
    }

    /// Carga una sesión aceptando tanto v1 (grupos sueltos, migrados a una sola ventana) como
    /// v2 (multi-ventana nativa). El `"version"` del JSON decide el camino; ausente == 1.
    pub fn load_session_v2(&self, session_id: &str) -> std::io::Result<SessionV2> {
        let path = self.session_path(session_id);
        let data = std::fs::read_to_string(path)?;
        let data = data.strip_prefix('\u{FEFF}').unwrap_or(&data);
        let value: serde_json::Value = serde_json::from_str(data)?;
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
        if version >= 2 {
            serde_json::from_value::<SessionV2>(value)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        } else {
            let v1: Session = serde_json::from_value(value)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(SessionV2::from_v1(v1))
        }
    }

    pub fn save_auto_session_v2(&self, session: &SessionV2) -> std::io::Result<()> {
        self.save_session_v2(session, "auto-restore")
    }

    pub fn load_auto_session_v2(&self) -> std::io::Result<Option<SessionV2>> {
        match self.load_session_v2("auto-restore") {
            Ok(session) => Ok(Some(session)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

pub fn get_sessions_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("sessions")
}

pub fn ensure_sessions_dir(app_data_dir: &Path) -> PathBuf {
    let sessions_dir = get_sessions_dir(app_data_dir);
    std::fs::create_dir_all(&sessions_dir).ok();
    sessions_dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-session-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn new_does_not_nest_sessions_dir() {
        let dir = temp_dir("no-nest");
        let sessions_dir = ensure_sessions_dir(&dir);
        let manager = SessionManager::new(sessions_dir.clone());
        assert_eq!(manager.sessions_dir(), sessions_dir.as_path());
        assert!(!manager.sessions_dir().ends_with("sessions/sessions"));
        assert!(!manager.sessions_dir().ends_with("sessions\\sessions"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn roundtrip_session() {
        let dir = temp_dir("roundtrip");
        let manager = SessionManager::new(dir.clone());
        let mut session = Session::default();
        session.groups.push(GroupState {
            id: 0,
            tabs: vec![TabState {
                document_id: "scratch-1".to_string(),
                path: None,
                title: "Sin título 1".to_string(),
                is_scratch: true,
                cursor: 3,
                scroll_line: 0,
                pinned: false,
                unsaved_content: None,
                mtime: None,
                custom_title: None,
                created_at: None,
                untitled_n: None,
            }],
            active_tab: 0,
        });
        manager.save_session(&session, "test").unwrap();
        let loaded = manager.load_session("test").unwrap();
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].tabs[0].cursor, 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn old_json_without_unsaved_content_still_loads() {
        let dir = temp_dir("old-json");
        let manager = SessionManager::new(dir.clone());
        let old_json = r#"{
            "version": 1,
            "name": "Old",
            "created_at": 0,
            "groups": [
                {
                    "id": 0,
                    "tabs": [
                        {
                            "document_id": "tab-0",
                            "path": null,
                            "title": "Old title",
                            "is_scratch": false,
                            "cursor": 0,
                            "scroll_line": 0,
                            "pinned": false
                        }
                    ],
                    "active_tab": 0
                }
            ],
            "active_group": 0,
            "group_count": 1,
            "workspace_root": null
        }"#;
        std::fs::write(dir.join("legacy.json"), old_json).unwrap();
        let loaded = manager.load_session("legacy").unwrap();
        assert_eq!(loaded.groups[0].tabs[0].title, "Old title");
        assert_eq!(loaded.groups[0].tabs[0].unsaved_content, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_session_strips_utf8_bom() {
        let dir = temp_dir("bom");
        let manager = SessionManager::new(dir.clone());
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(serde_json::to_string(&Session::default()).unwrap().as_bytes());
        std::fs::write(dir.join("bomsession.json"), bytes).unwrap();
        let loaded = manager.load_session("bomsession").unwrap();
        assert_eq!(loaded.name, "Default Session");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_session_leaves_no_tmp_file() {
        let dir = temp_dir("no-tmp-leftover");
        let manager = SessionManager::new(dir.clone());
        manager.save_session(&Session::default(), "s1").unwrap();
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "tmp").unwrap_or(false))
            .collect();
        assert!(leftover.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_session_returns_not_found() {
        let dir = temp_dir("missing");
        let manager = SessionManager::new(dir.clone());
        let err = manager.load_session("nope").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ws_e1_v1_session_migrates_to_single_window_v2() {
        let dir = temp_dir("v1-to-v2");
        let manager = SessionManager::new(dir.clone());
        let mut v1 = Session::default();
        v1.workspace_root = Some("C:\\proyecto".to_string());
        v1.groups.push(GroupState {
            id: 0,
            tabs: vec![TabState {
                document_id: "scratch-1".to_string(),
                path: None,
                title: "Sin título 1".to_string(),
                is_scratch: true,
                cursor: 5,
                scroll_line: 0,
                pinned: false,
                unsaved_content: None,
                mtime: None,
                custom_title: None,
                created_at: None,
                untitled_n: Some(1),
            }],
            active_tab: 0,
        });
        manager.save_session(&v1, "legacy-v1").unwrap();

        let v2 = manager.load_session_v2("legacy-v1").unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.windows.len(), 1, "v1 debe migrar a exactamente una ventana");
        assert_eq!(v2.active_window, 0);
        let win = &v2.windows[0];
        assert_eq!(win.workspace_root, Some("C:\\proyecto".to_string()));
        assert_eq!(win.groups.len(), 1);
        assert_eq!(win.groups[0].tabs[0].cursor, 5);
        assert_eq!(win.groups[0].tabs[0].untitled_n, Some(1));
        // geometría por defecto para lo que v1 nunca guardó
        assert_eq!(win.x, DEFAULT_WINDOW_X);
        assert_eq!(win.y, DEFAULT_WINDOW_Y);
        assert!(!win.maximized);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ws_e1_v2_round_trip_is_identical() {
        let dir = temp_dir("v2-roundtrip");
        let manager = SessionManager::new(dir.clone());
        let session = SessionV2 {
            version: 2,
            active_window: 1,
            windows: vec![
                WindowState {
                    x: 10,
                    y: 20,
                    width: 800.0,
                    height: 600.0,
                    maximized: false,
                    groups: vec![GroupState { id: 0, tabs: vec![], active_tab: 0 }],
                    active_group: 0,
                    workspace_root: None,
                },
                WindowState {
                    x: 900,
                    y: 20,
                    width: 640.0,
                    height: 480.0,
                    maximized: true,
                    groups: vec![GroupState {
                        id: 0,
                        tabs: vec![TabState {
                            document_id: "tab-a".to_string(),
                            path: Some("C:\\a.md".to_string()),
                            title: "a.md".to_string(),
                            is_scratch: false,
                            cursor: 3,
                            scroll_line: 2,
                            pinned: true,
                            unsaved_content: Some("hola".to_string()),
                            mtime: Some(123),
                            custom_title: Some("A".to_string()),
                            created_at: Some(456),
                            untitled_n: None,
                        }],
                        active_tab: 0,
                    }],
                    active_group: 0,
                    workspace_root: Some("C:\\otro".to_string()),
                },
            ],
        };
        manager.save_session_v2(&session, "v2-test").unwrap();
        let loaded = manager.load_session_v2("v2-test").unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.active_window, 1);
        assert_eq!(loaded.windows.len(), 2);
        assert_eq!(loaded.windows[1].maximized, true);
        assert_eq!(loaded.windows[1].groups[0].tabs[0].unsaved_content, Some("hola".to_string()));
        assert_eq!(loaded.windows[1].workspace_root, Some("C:\\otro".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ws_e1_load_auto_session_v2_missing_returns_none() {
        let dir = temp_dir("v2-auto-missing");
        let manager = SessionManager::new(dir.clone());
        assert!(manager.load_auto_session_v2().unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ws_e1_load_session_v2_strips_bom() {
        let dir = temp_dir("v2-bom");
        let manager = SessionManager::new(dir.clone());
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(serde_json::to_string(&SessionV2::default()).unwrap().as_bytes());
        std::fs::write(dir.join("v2bom.json"), bytes).unwrap();
        let loaded = manager.load_session_v2("v2bom").unwrap();
        assert_eq!(loaded.version, 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ws_e1b_window_state_to_session_roundtrip_preserves_content() {
        let ws = WindowState {
            x: 10,
            y: 20,
            width: 800.0,
            height: 600.0,
            maximized: true,
            groups: vec![GroupState {
                id: 0,
                tabs: vec![TabState {
                    document_id: "scratch-1".to_string(),
                    path: None,
                    title: "Sin título 1".to_string(),
                    is_scratch: true,
                    cursor: 5,
                    scroll_line: 0,
                    pinned: false,
                    unsaved_content: None,
                    mtime: None,
                    custom_title: None,
                    created_at: Some(42),
                    untitled_n: Some(1),
                }],
                active_tab: 0,
            }],
            active_group: 0,
            workspace_root: Some("C:\\proyecto".to_string()),
        };
        let session = ws.to_session();
        assert_eq!(session.groups[0].tabs[0].cursor, 5);
        assert_eq!(session.workspace_root, Some("C:\\proyecto".to_string()));
        // La geometría NO viaja por `Session` (formato v1, sin ella): `from_session` la
        // vuelve a combinar con el contenido restaurado.
        let ws2 = WindowState::from_session(session, ws.x, ws.y, ws.width, ws.height, ws.maximized);
        assert_eq!(ws2.x, 10);
        assert_eq!(ws2.maximized, true);
        assert_eq!(ws2.groups[0].tabs[0].untitled_n, Some(1));
    }

    #[test]
    fn ws_e1b_clamp_window_position_keeps_onscreen_position_unchanged() {
        // Ventana totalmente dentro del área virtual (1920x1080 en el origen): sin cambios.
        let (x, y) = clamp_window_position(100, 100, 800.0, 600.0, 0, 0, 1920, 1080, 0);
        assert_eq!((x, y), (100, 100));
    }

    #[test]
    fn ws_e1b_clamp_window_position_keeps_partially_onscreen_position_unchanged() {
        // Solo una esquina solapa el área virtual: sigue "visible" (parcialmente), no se toca.
        let (x, y) = clamp_window_position(-700, -500, 800.0, 600.0, 0, 0, 1920, 1080, 0);
        assert_eq!((x, y), (-700, -500));
    }

    #[test]
    fn ws_e1b_clamp_window_position_cascades_fully_offscreen_position() {
        // Completamente a la derecha del área virtual (monitor desconectado): se cascada
        // cerca del origen del área virtual, con un offset distinto por índice.
        let (x0, y0) = clamp_window_position(5000, 5000, 800.0, 600.0, 0, 0, 1920, 1080, 0);
        assert_eq!((x0, y0), (60, 60));
        let (x1, y1) = clamp_window_position(5000, 5000, 800.0, 600.0, 0, 0, 1920, 1080, 1);
        assert_eq!((x1, y1), (92, 92));
        assert_ne!((x0, y0), (x1, y1));
    }

    #[test]
    fn ws_e1b_clamp_window_position_respects_nonzero_virtual_origin() {
        // Área virtual que no empieza en (0,0) (monitor secundario a la izquierda del
        // principal, área virtual total x en [-1920, 1920)): una posición REALMENTE fuera
        // de ese rango se cascada relativa a ESE origen, no a (0,0).
        let (x, y) = clamp_window_position(5000, 5000, 800.0, 600.0, -1920, 0, 3840, 1080, 0);
        assert_eq!((x, y), (-1860, 60));
    }
}
