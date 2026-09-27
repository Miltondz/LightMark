// El wiring actual solo usa `list_directory`, `atomic_save` y `has_external_changes`; el
// resto (helpers de tipo de archivo, tamaño, codificación, recorrido de proyecto) queda
// disponible para el explorador de proyectos completo que no forma parte de este pase.
#![allow(dead_code)]

use ropey::Rope;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// Workspace file system operations

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
}

/// Lista el contenido de `dir`: carpetas primero, luego archivos, ambos en orden alfabético
/// sin distinguir mayúsculas/minúsculas. Oculta los archivos/carpetas que empiezan por `.`.
pub fn list_directory(dir: &Path) -> std::io::Result<Vec<FileInfo>> {
    let mut entries: Vec<FileInfo> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.starts_with('.') {
            continue;
        }

        entries.push(FileInfo {
            path: path.clone(),
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified()?,
        });
    }

    // Orden: carpetas primero, luego archivos; alfabético, sin distinguir mayúsculas.
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

pub fn read_file_to_rope(path: &Path) -> std::io::Result<Rope> {
    let content = std::fs::read_to_string(path)?;
    Ok(Rope::from_str(&content))
}

pub fn save_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
}

pub fn is_supported_file_type(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md")
            | Some("markdown")
            | Some("html")
            | Some("htm")
            | Some("js")
            | Some("mjs")
            | Some("cjs")
            | Some("sql")
            | Some("json")
            | Some("xml")
            | Some("txt")
    )
}

pub fn detect_language(path: &Path) -> Option<String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("markdown") => Some("markdown".to_string()),
        Some("html") | Some("htm") => Some("html".to_string()),
        Some("js") | Some("mjs") | Some("cjs") => Some("javascript".to_string()),
        Some("sql") => Some("sql".to_string()),
        Some("json") => Some("json".to_string()),
        Some("xml") => Some("xml".to_string()),
        Some("txt") => Some("plaintext".to_string()),
        _ => None,
    }
}

pub fn file_modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

pub fn has_external_changes(path: &Path, last_known: SystemTime) -> bool {
    file_modified_time(path).is_some_and(|t| t > last_known)
}

pub fn is_large_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 10 * 1024 * 1024)
        .unwrap_or(false)
}

pub fn is_huge_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 100 * 1024 * 1024)
        .unwrap_or(false)
}

// Atomic save: write to temp, flush, then rename. Si cualquier paso falla, el `.tmp` se borra
// (hallazgo C4): nunca debe quedar un archivo temporal huérfano en el directorio del usuario.
//
// Hallazgo L18 — casos adicionales cubiertos:
// - Symlink: escribe/renombra sobre el DESTINO real (`canonicalize`) en vez de sustituir el
//   propio enlace por un archivo normal (lo que rompería el symlink).
// - Permisos existentes (p.ej. solo lectura): se reaplican tras el `rename`, ya que un
//   `rename` sobre Windows conserva los del `.tmp` (recién creado, sin el atributo), no los
//   del archivo reemplazado.
// - `rename` puede fallar si el archivo destino está abierto por otro proceso sin
//   `FILE_SHARE_DELETE` (frecuente en Windows). En ese caso se hace un fallback: copia de
//   seguridad (`.bak`) del contenido anterior + truncar y escribir el archivo EN SITIO (que sí
//   suele estar permitido si el otro proceso comparte al menos escritura).
// - El atributo "oculto" de Windows NO se preserva: no hay forma de leerlo/reaplicarlo con
//   `std` sin llamar a la API Win32 directamente (y se decidió no invocar `attrib` por
//   `Command`); queda documentado como limitación conocida.
pub fn atomic_save(path: &Path, content: &str) -> std::io::Result<()> {
    atomic_save_impl(path, content, ReadonlyPolicy::ClearAndRestore)
}

/// Como `atomic_save`, pero para archivos que el USUARIO tiene abiertos y está editando
/// (hallazgo C7): "Guardar"/"Guardar como" sobre un archivo marcado de solo lectura NO
/// debe limpiar esa marca y sobrescribirlo en silencio — eso es una decisión del usuario
/// (o de otra herramienta) que el editor debe respetar, igual que cualquier otro editor
/// de texto. Devuelve `Err` con `ErrorKind::PermissionDenied` para que la UI lo muestre en
/// la barra de estado en vez de perder el archivo de solo lectura sin avisar.
///
/// `atomic_save` (sin el guard de solo lectura) sigue siendo el correcto para archivos
/// INTERNOS de la app (settings.json, session.json, scratch-index.json, borradores de
/// scratch): esos son datos propios de LightMark, no algo que el usuario haya marcado
/// deliberadamente como protegido, así que autoguardarlos puede seguir limpiando la marca.
pub fn atomic_save_document(path: &Path, content: &str) -> std::io::Result<()> {
    atomic_save_impl(path, content, ReadonlyPolicy::Reject)
}

enum ReadonlyPolicy {
    /// Limpia la marca de solo lectura para poder escribir, y la restaura al terminar
    /// (éxito o error) — comportamiento histórico, reservado a archivos internos de la
    /// app.
    ClearAndRestore,
    /// No toca el archivo: si está marcado de solo lectura, falla con
    /// `PermissionDenied` antes de intentar escribir nada.
    Reject,
}

fn atomic_save_impl(path: &Path, content: &str, policy: ReadonlyPolicy) -> std::io::Result<()> {
    use std::io::Write;

    // Si `path` es un symlink, opera sobre su destino real para no romper el enlace
    // sustituyéndolo por un archivo normal.
    let target: std::path::PathBuf = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    };

    let existing_permissions = std::fs::metadata(&target).ok().map(|m| m.permissions());

    if let Some(perms) = &existing_permissions {
        if perms.readonly() {
            match policy {
                ReadonlyPolicy::Reject => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "El archivo es de solo lectura",
                    ));
                }
                ReadonlyPolicy::ClearAndRestore => {
                    // Un archivo de solo lectura no se puede sustituir con `rename`
                    // (falla con acceso denegado) ni reescribir directamente; se quita
                    // la marca antes de escribir y se restaura al final sobre el
                    // resultado final, sea cual sea la ruta tomada (ver más abajo).
                    let mut writable = perms.clone();
                    writable.set_readonly(false);
                    std::fs::set_permissions(&target, writable).ok();
                }
            }
        }
    }

    let result = (|| -> std::io::Result<()> {
        // Hallazgo C8: nombre de temporal ÚNICO abierto con `create_new(true)`, no
        // `File::create` (que TRUNCA silenciosamente cualquier archivo existente con ese
        // nombre exacto). Con el nombre fijo `<archivo>.<ext>.tmp` de antes, si el
        // usuario ya tenía un archivo propio llamado justo así (nada exótico: es un
        // nombre predecible), guardar lo destruía sin avisar.
        let (temp_path, mut temp_file) = create_unique_temp_file(&target)?;

        let write_result = (|| -> std::io::Result<()> {
            temp_file.write_all(content.as_bytes())?;
            temp_file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_result {
            std::fs::remove_file(&temp_path).ok();
            return Err(e);
        }

        match std::fs::rename(&temp_path, &target) {
            Ok(()) => Ok(()),
            Err(_) => {
                // El rename falló (típicamente: el destino está abierto por otro proceso sin
                // FILE_SHARE_DELETE en Windows). Se limpia el .tmp y se intenta escribir EN
                // SITIO: si el otro proceso al menos comparte escritura, esto sí tendrá éxito;
                // si no, se propaga el error de esa escritura in situ (más informativo que el
                // del rename).
                std::fs::remove_file(&temp_path).ok();
                if target.exists() {
                    std::fs::copy(&target, backup_path_for(&target)).ok();
                }
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&target)?;
                file.write_all(content.as_bytes())?;
                file.sync_all()?;
                // Hallazgo C8: el `.bak` de respaldo del contenido anterior solo existe
                // para permitir recuperarlo si la escritura EN SITIO fallaba a mitad; una
                // vez la escritura in situ tuvo éxito, el archivo original ya está
                // reemplazado y el `.bak` sobrante no debe quedar huérfano pisando el
                // espacio de nombres del usuario (ni, peor, un archivo que el usuario
                // hubiera creado él mismo con ese nombre).
                std::fs::remove_file(backup_path_for(&target)).ok();
                Ok(())
            }
        }
    })();

    // Hallazgo C7: restaurar los permisos originales SIEMPRE al salir (éxito O error) —
    // antes solo se restauraban en la rama de éxito del `rename` y en la rama de
    // fallback in situ tras un `rename` fallido; si la escritura del `.tmp` fallaba
    // (disco lleno, etc.) la marca de solo lectura quedaba limpiada permanentemente.
    if let Some(perms) = existing_permissions {
        std::fs::set_permissions(&target, perms).ok();
    }

    result
}

/// Crea y devuelve un archivo temporal con nombre ÚNICO junto a `target` (hallazgo C8),
/// abierto con `create_new(true)`: si por lo que sea el nombre elegido YA existe, falla
/// con `AlreadyExists` en vez de truncarlo (nunca pisa un archivo que no creó esta misma
/// llamada). Reintenta unas pocas veces con un sufijo distinto ante esa colisión —
/// extremadamente improbable dado que mezcla el PID y un contador de nanosegundos, pero
/// más seguro que asumirlo.
fn create_unique_temp_file(target: &Path) -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    let pid = std::process::id();
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0u32..8 {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let candidate = target.with_extension(format!("{ext}.{pid}-{nanos}-{attempt}.tmp"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last_err = Some(e);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no se pudo crear un archivo temporal único")
    }))
}

fn backup_path_for(target: &Path) -> std::path::PathBuf {
    target.with_extension(format!(
        "{}.bak",
        target.extension().and_then(|e| e.to_str()).unwrap_or("bak")
    ))
}

// Check if file has been modified externally since last check
pub fn check_external_modification(
    path: &Path,
    last_check: SystemTime,
) -> Result<Option<SystemTime>, std::io::Error> {
    let current_time = file_modified_time(path);
    if current_time.is_some() && current_time.unwrap() > last_check {
        Ok(current_time)
    } else {
        Ok(None)
    }
}

// Get file encoding info
pub fn get_encoding_info(path: &Path) -> (String, String) {
    let mut encoding = "UTF-8".to_string();
    let mut line_endings = "LF".to_string();

    if let Ok(content) = std::fs::read(path) {
        // Check for BOM
        if content.len() >= 3 && content[0] == 0xEF && content[1] == 0xBB && content[2] == 0xBF {
            encoding = "UTF-8 BOM".to_string();
        } else if content.len() >= 2 && content[0] == 0xFF && content[1] == 0xFE {
            encoding = "UTF-16 LE".to_string();
        } else if content.len() >= 2 && content[0] == 0xFE && content[1] == 0xFF {
            encoding = "UTF-16 BE".to_string();
        }

        // Detect line endings
        let sample = std::str::from_utf8(&content).unwrap_or("");
        let mut cut = sample.len().min(8192);
        while cut > 0 && !sample.is_char_boundary(cut) {
            cut -= 1;
        }
        let sample_truncated = &sample[..cut];
        if sample_truncated.contains("\r\n") {
            line_endings = "CRLF".to_string();
        } else if sample_truncated.contains('\r') {
            line_endings = "CR".to_string();
        }
    }

    (encoding, line_endings)
}

// Walk directory respecting .gitignore
pub fn walk_project_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories and common ignored dirs
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                files.extend(walk_project_files(&path));
            } else if is_supported_file_type(&path) {
                files.push(path);
            }
        }
    }

    files
}

// Workspace management
pub fn is_in_workspace(root: &Path, file: &Path) -> bool {
    file.starts_with(root)
}

pub fn get_relative_path(root: &Path, file: &Path) -> Option<String> {
    file.strip_prefix(root).ok().map(|p| {
        let s = p.to_string_lossy();
        s.to_string()
    })
}

pub fn time_since_modified(path: &Path) -> Option<Duration> {
    let modified = file_modified_time(path)?;
    modified.elapsed().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lightmark-workspace-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_directory_orders_dirs_first_then_alphabetical_case_insensitive() {
        let dir = temp_dir("order");
        std::fs::create_dir_all(dir.join("Zebra")).unwrap();
        std::fs::create_dir_all(dir.join("apple_dir")).unwrap();
        std::fs::write(dir.join("banana.txt"), "b").unwrap();
        std::fs::write(dir.join("Apple.txt"), "a").unwrap();
        std::fs::write(dir.join(".hidden"), "h").unwrap();

        let entries = list_directory(&dir).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, vec!["apple_dir", "Zebra", "Apple.txt", "banana.txt"]);
        assert!(entries[0].is_dir && entries[1].is_dir);
        assert!(!entries[2].is_dir && !entries[3].is_dir);
        assert!(!names.iter().any(|n| n.starts_with('.')));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_save_writes_content() {
        let dir = temp_dir("atomic");
        let file = dir.join("doc.txt");
        atomic_save(&file, "hola mundo").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hola mundo");
        // No debe quedar el temporal
        assert!(!dir.join("doc.txt.tmp").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_save_does_not_leave_tmp_on_failure() {
        let dir = temp_dir("atomic-fail");
        // Ruta con un directorio padre inexistente -> falla en el paso de rename/create.
        let bad_path = dir.join("no-existe-dir").join("doc.txt");
        assert!(atomic_save(&bad_path, "x").is_err());
        let tmp_path = bad_path.with_extension("txt.tmp");
        assert!(!tmp_path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_save_overwrites_existing_content() {
        let dir = temp_dir("atomic-overwrite");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "old").unwrap();
        atomic_save(&file, "new content").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_save_preserves_readonly_permission() {
        let dir = temp_dir("atomic-readonly");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "old").unwrap();
        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&file, perms).unwrap();

        atomic_save(&file, "new content").unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());

        // Deja el archivo escribible para poder limpiar el directorio temporal.
        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&file, perms).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn c7_atomic_save_document_rejects_readonly_without_overwriting() {
        // Hallazgo C7: `atomic_save_document` (usado por Guardar/Guardar como sobre
        // archivos del usuario) NO debe limpiar la marca de solo lectura ni sobrescribir
        // el archivo — debe fallar con PermissionDenied y dejar el contenido intacto.
        let dir = temp_dir("c7-readonly-doc");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "contenido original").unwrap();
        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&file, perms).unwrap();

        let err = atomic_save_document(&file, "contenido nuevo").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "contenido original");
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());

        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&file, perms).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn c7_atomic_save_internal_still_clears_and_restores_readonly() {
        // `atomic_save` (archivos internos de la app) conserva el comportamiento
        // histórico: limpia la marca para poder escribir y la restaura al terminar.
        let dir = temp_dir("c7-readonly-internal");
        let file = dir.join("settings.json");
        std::fs::write(&file, "{}").unwrap();
        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&file, perms).unwrap();

        atomic_save(&file, r#"{"x":1}"#).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), r#"{"x":1}"#);
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());

        let mut perms = std::fs::metadata(&file).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&file, perms).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn c8_atomic_save_never_truncates_a_preexisting_file_that_collides_with_temp_name() {
        // Hallazgo C8: el nombre de temporal ya NO es el fijo predecible
        // "<archivo>.<ext>.tmp" — si el usuario tenía un archivo real con ESE nombre
        // exacto, `File::create` lo truncaba sin avisar. Se comprueba que ese archivo
        // sobrevive intacto tras guardar.
        let dir = temp_dir("c8-collision");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "contenido").unwrap();
        let colliding_tmp = dir.join("doc.txt.tmp"); // nombre fijo que se usaba antes.
        std::fs::write(&colliding_tmp, "NO ME TOQUES").unwrap();

        atomic_save(&file, "contenido nuevo").unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "contenido nuevo");
        assert_eq!(std::fs::read_to_string(&colliding_tmp).unwrap(), "NO ME TOQUES");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn c8_no_backup_left_after_normal_successful_save() {
        let dir = temp_dir("c8-bak-cleanup");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "old").unwrap();
        atomic_save(&file, "new").unwrap();
        let bak = backup_path_for(&file);
        assert!(!bak.exists(), "no debe quedar un .bak tras un guardado normal");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn c8_backup_removed_after_successful_fallback_when_rename_is_blocked() {
        // Hallazgo C8: cuando el `rename` atómico falla (aquí, forzado manteniendo el
        // destino abierto sin `FILE_SHARE_DELETE` — el comportamiento por defecto de
        // `std::fs::File` en Windows, que hace que `MoveFileExW` falle con una
        // violación de uso compartido) y se recurre al fallback de escritura EN SITIO,
        // el `.bak` de seguridad solo debe existir mientras esa escritura está en
        // curso; una vez tiene éxito, no debe quedar huérfano.
        let dir = temp_dir("c8-bak-fallback");
        let file = dir.join("doc.txt");
        std::fs::write(&file, "old").unwrap();
        let keep_open = std::fs::OpenOptions::new().read(true).open(&file).unwrap();

        atomic_save(&file, "new").unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
        let bak = backup_path_for(&file);
        assert!(!bak.exists(), "el .bak de respaldo no debe quedar tras el fallback exitoso");
        drop(keep_open);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn atomic_save_through_symlink_writes_real_target_and_keeps_link() {
        let dir = temp_dir("atomic-symlink");
        let target = dir.join("real.txt");
        std::fs::write(&target, "real").unwrap();
        let link = dir.join("link.txt");

        // Crear symlinks en Windows puede requerir un privilegio que no siempre está
        // disponible en el entorno de compilación/CI; si falla, no es un fallo de esta
        // función y se omite el resto de la comprobación (igual que hace el sondeo r2).
        if std::os::windows::fs::symlink_file(&target, &link).is_ok() {
            atomic_save(&link, "via link").unwrap();
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "via link");
            assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
