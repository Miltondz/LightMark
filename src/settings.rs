//! Configuración persistente del usuario (`settings.json` en el directorio de datos de la app).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuración persistente de la aplicación.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub font_size: u32,
    pub font_family: String,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub autosave_delay_ms: u32,
    pub autosave_files: bool,
    pub restore_session: bool,
    /// 0 = solo editor, 1 = dividido, 2 = solo vista previa.
    pub view_mode: u32,
    pub preview_selectable: bool,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub show_explorer: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub recent_files: Vec<String>,
    /// R3: 0 = "Oscuro" (paleta original), 1 = "Océano" (petróleo/teal, ver ui/theme.slint).
    pub theme: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_size: 14,
            font_family: "Consolas".to_string(),
            tab_size: 4,
            insert_spaces: true,
            word_wrap: false,
            show_line_numbers: true,
            autosave_delay_ms: 1000,
            autosave_files: false,
            restore_session: true,
            view_mode: 0,
            preview_selectable: false,
            show_toolbar: true,
            show_status_bar: true,
            show_explorer: false,
            window_width: 1100.0,
            window_height: 700.0,
            recent_files: Vec::new(),
            theme: 0,
        }
    }
}

/// Quita el BOM UTF-8 (`\u{FEFF}`) inicial si lo hay (hallazgo M4).
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

impl Settings {
    fn path() -> PathBuf {
        crate::scratch::get_app_data_dir().join("settings.json")
    }

    /// Carga la configuración desde disco. Si el archivo no existe o está corrupto, devuelve
    /// los valores por defecto (nunca falla). Quita el BOM UTF-8 si lo hay (hallazgo M4:
    /// PowerShell/Notepad lo añaden al guardar con "UTF-8", lo que rompía el parseo y
    /// sobrescribía la configuración del usuario con los valores por defecto). Si un campo
    /// tiene un tipo incorrecto, solo ese campo cae al valor por defecto en vez de perderse
    /// toda la configuración (hallazgo G: fusión campo a campo sobre `serde_json::Value`).
    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(data) => {
                let data = strip_bom(&data);
                // Si el archivo ni siquiera parsea como un objeto JSON, está totalmente
                // corrupto: se respalda ANTES de que el próximo `save()` lo sobrescriba en
                // silencio con los valores por defecto (hallazgo L17), para que el usuario
                // pueda recuperarlo o inspeccionarlo manualmente.
                if !matches!(
                    serde_json::from_str::<serde_json::Value>(data),
                    Ok(serde_json::Value::Object(_))
                ) {
                    let path = Self::path();
                    std::fs::copy(&path, path.with_extension("json.bak")).ok();
                }
                Self::from_json_str_merging_defaults(data).validated()
            }
            Err(_) => Settings::default(),
        }
    }

    /// Parsea `data` como configuración; si falla por completo, usa los valores por defecto.
    /// Si se puede parsear como `serde_json::Value` pero algún campo individual es inválido
    /// (tipo incorrecto, o de tipo correcto pero un valor que no encaja, p.ej. un elemento no
    /// cadena en `recent_files`), SOLO ese campo cae a su valor por defecto y el resto se
    /// conserva (hallazgo L17: antes, comparar solo la "forma" JSON del valor, en vez de
    /// intentar de verdad construir la configuración con él, dejaba pasar campos con la forma
    /// correcta pero contenido inválido — p.ej. `recent_files: [1]` es un array, la misma forma
    /// que el array de cadenas por defecto, pero un elemento numérico dentro rompía la
    /// deserialización completa de `Settings` y perdía TODA la configuración, no solo esa
    /// clave; igual para `font_size: 14.5`, un número con la forma correcta pero que no cabe en
    /// `u32`). Se comprueba cada campo intentando construir una `Settings` completa (el resto
    /// en sus valores por defecto) con solo ese campo puesto a su valor del archivo: si esa
    /// prueba deserializa con éxito, el campo es válido y se conserva; si no, se descarta en
    /// silencio y se queda en su valor por defecto.
    fn from_json_str_merging_defaults(data: &str) -> Self {
        if let Ok(s) = serde_json::from_str::<Settings>(data) {
            return s;
        }
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(data)
        else {
            return Settings::default();
        };
        let default_value = serde_json::to_value(Settings::default()).unwrap_or_default();
        let serde_json::Value::Object(default_map) = default_value else {
            return Settings::default();
        };
        let mut result_map = default_map.clone();
        for (key, val) in map {
            if !default_map.contains_key(&key) {
                continue; // Clave desconocida (versión futura, campo eliminado, etc.): se ignora.
            }
            let mut candidate = default_map.clone();
            candidate.insert(key.clone(), val.clone());
            if serde_json::from_value::<Settings>(serde_json::Value::Object(candidate)).is_ok() {
                result_map.insert(key, val);
            }
            // Si falla, el campo se queda en su valor por defecto (ya está en `result_map`
            // porque partió de `default_map.clone()`).
        }
        serde_json::from_value(serde_json::Value::Object(result_map)).unwrap_or_default()
    }

    /// Guarda la configuración en disco, creando el directorio si es necesario, de forma
    /// atómica (escribe a un `.tmp` y renombra, hallazgo C4) para no dejar un `settings.json`
    /// corrupto o vacío si el proceso se interrumpe a mitad de escritura.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let data = serde_json::to_string_pretty(self)?;
        crate::workspace::atomic_save(&path, &data)
    }

    /// Añade `path` al principio de "recientes", deduplicando y limitando a 10 entradas.
    pub fn add_recent(&mut self, path: String) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(10);
    }

    /// Devuelve una copia con los valores saturados a rangos razonables.
    pub fn validated(mut self) -> Self {
        self.font_size = self.font_size.clamp(8, 40);
        self.tab_size = self.tab_size.clamp(1, 8);
        self.autosave_delay_ms = self.autosave_delay_ms.clamp(200, 10000);
        // 0 = solo editor, 1 = dividido, 2 = solo vista previa; cualquier otro valor (un
        // settings.json de una versión futura, o corrupción parcial) se acota al máximo válido
        // en vez de dejar un `view_mode` que la UI no sepa interpretar (hallazgo L17).
        self.view_mode = self.view_mode.min(2);
        // 0 = Oscuro, 1 = Océano; cualquier otro valor (versión futura, corrupción
        // parcial) se acota al máximo válido, igual que `view_mode` arriba.
        self.theme = self.theme.min(1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // LOCALAPPDATA es global al proceso; serializamos los tests que lo tocan.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_appdata<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "lightmark-settings-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var("LOCALAPPDATA").ok();
        unsafe {
            std::env::set_var("LOCALAPPDATA", &dir);
        }
        f();
        unsafe {
            match &prev {
                Some(v) => std::env::set_var("LOCALAPPDATA", v),
                None => std::env::remove_var("LOCALAPPDATA"),
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.font_size, 14);
        assert_eq!(s.font_family, "Consolas");
        assert_eq!(s.tab_size, 4);
        assert!(s.insert_spaces);
        assert!(!s.word_wrap);
        assert!(s.show_line_numbers);
        assert!(!s.autosave_files);
        assert!(s.restore_session);
        assert_eq!(s.view_mode, 0);
    }

    #[test]
    fn roundtrip_save_load() {
        with_temp_appdata(|| {
            let mut s = Settings::default();
            s.font_size = 20;
            s.add_recent("C:/a.md".to_string());
            s.save().unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.font_size, 20);
            assert_eq!(loaded.recent_files, vec!["C:/a.md".to_string()]);
        });
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{ not valid json ").unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded, Settings::default());
        });
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        with_temp_appdata(|| {
            let loaded = Settings::load();
            assert_eq!(loaded, Settings::default());
        });
    }

    #[test]
    fn add_recent_dedupes_and_caps() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.add_recent(format!("file{}.md", i));
        }
        assert_eq!(s.recent_files.len(), 10);
        assert_eq!(s.recent_files[0], "file14.md");

        s.add_recent("file10.md".to_string());
        assert_eq!(s.recent_files[0], "file10.md");
        assert_eq!(
            s.recent_files.iter().filter(|f| *f == "file10.md").count(),
            1
        );
    }

    #[test]
    fn load_strips_utf8_bom() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(b"{\"font_size\": 22}");
            std::fs::write(&path, bytes).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.font_size, 22);
        });
    }

    #[test]
    fn field_with_wrong_type_falls_back_only_for_that_field() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // font_size con un tipo erróneo (string en vez de número) no debe tirar el resto.
            std::fs::write(&path, r#"{"font_size": "grande", "tab_size": 2}"#).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.font_size, Settings::default().font_size);
            assert_eq!(loaded.tab_size, 2);
        });
    }

    #[test]
    fn save_does_not_leave_tmp_file() {
        with_temp_appdata(|| {
            Settings::default().save().unwrap();
            let dir = crate::scratch::get_app_data_dir();
            let leftover: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "tmp").unwrap_or(false))
                .collect();
            assert!(leftover.is_empty());
        });
    }

    #[test]
    fn float_font_size_falls_back_only_for_that_field() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // font_size con forma correcta (Number) pero que no cabe en u32 (float): antes esto
            // hacía fallar la deserialización completa de `Settings` y perdía TODA la
            // configuración, no solo esa clave.
            std::fs::write(&path, r#"{"font_size": 14.5, "tab_size": 2}"#).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.font_size, Settings::default().font_size);
            assert_eq!(loaded.tab_size, 2);
        });
    }

    #[test]
    fn recent_files_with_wrong_element_type_falls_back_only_for_that_field() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"recent_files": [1], "tab_size": 2}"#).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.recent_files, Settings::default().recent_files);
            assert_eq!(loaded.tab_size, 2);
        });
    }

    #[test]
    fn view_mode_out_of_range_is_clamped_not_defaulted() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"view_mode": 7, "tab_size": 2}"#).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.view_mode, 2);
            assert_eq!(loaded.tab_size, 2);
        });
    }

    #[test]
    fn totally_corrupt_file_is_backed_up_before_being_overwritten() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{ not valid json at all").unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded, Settings::default());
            let backup = path.with_extension("json.bak");
            assert!(backup.exists());
            assert_eq!(
                std::fs::read_to_string(&backup).unwrap(),
                "{ not valid json at all"
            );
        });
    }

    #[test]
    fn validated_clamps_ranges() {
        let mut s = Settings::default();
        s.font_size = 200;
        s.tab_size = 0;
        s.autosave_delay_ms = 50;
        let v = s.validated();
        assert_eq!(v.font_size, 40);
        assert_eq!(v.tab_size, 1);
        assert_eq!(v.autosave_delay_ms, 200);
    }

    #[test]
    fn theme_out_of_range_is_clamped_not_defaulted() {
        with_temp_appdata(|| {
            let path = crate::scratch::get_app_data_dir().join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"theme": 7, "tab_size": 2}"#).unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.theme, 1);
            assert_eq!(loaded.tab_size, 2);
        });
    }

    #[test]
    fn theme_default_is_oscuro() {
        assert_eq!(Settings::default().theme, 0);
    }
}
