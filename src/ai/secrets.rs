//! Almacén de claves de IA en el Administrador de credenciales de Windows (WS-D).
//! Destino: `LightMark/ai/<id>`. La clave NUNCA se escribe en settings.json, logs,
//! barra de estado ni mensajes de error: ninguna cadena de error incluye su valor.
#![allow(dead_code)]

/// Tamaño máximo del blob de una credencial genérica (CRED_MAX_CREDENTIAL_BLOB_SIZE).
const MAX_BLOB: usize = 2560;

/// Nombre de destino en el Administrador de credenciales.
pub fn target_name(provider_id: &str) -> String {
    format!("LightMark/ai/{provider_id}")
}

/// Valida y normaliza una clave: recorta, no vacía, sin espacios/control, <= 2560 bytes.
pub fn validate_key(raw: &str) -> Result<String, String> {
    let k = raw.trim();
    if k.is_empty() {
        return Err("La clave está vacía.".to_string());
    }
    if k.len() > MAX_BLOB {
        return Err("La clave es demasiado larga.".to_string());
    }
    if k.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("La clave no puede contener espacios ni saltos de línea.".to_string());
    }
    Ok(k.to_string())
}

#[cfg(windows)]
mod imp {
    use super::{MAX_BLOB, target_name, validate_key};
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn store_key(provider_id: &str, key: &str) -> Result<(), String> {
        let key = validate_key(key)?;
        debug_assert!(key.len() <= MAX_BLOB);
        let mut target = wide(&target_name(provider_id));
        let mut user = wide("LightMark");
        let mut blob: Vec<u8> = key.into_bytes();
        // SAFETY: estructura inicializada a cero + campos válidos durante la llamada.
        let ok = unsafe {
            let mut cred: CREDENTIALW = std::mem::zeroed();
            cred.Type = CRED_TYPE_GENERIC;
            cred.TargetName = target.as_mut_ptr();
            cred.UserName = user.as_mut_ptr();
            cred.CredentialBlobSize = blob.len() as u32;
            cred.CredentialBlob = blob.as_mut_ptr();
            cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
            CredWriteW(&cred, 0)
        };
        let err = unsafe { GetLastError() };
        // Sobrescribe la copia local antes de liberarla.
        blob.iter_mut().for_each(|b| *b = 0);
        if ok != 0 {
            Ok(())
        } else {
            Err(format!("No se pudo guardar la clave (código de Windows {err})."))
        }
    }

    pub fn load_key(provider_id: &str) -> Result<Option<String>, String> {
        let target = wide(&target_name(provider_id));
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` termina en NUL; `p` recibe un buffer que liberamos con CredFree.
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(format!("No se pudo leer la clave (código de Windows {err})."));
        }
        let out = unsafe {
            let c = &*p;
            let bytes = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                &[][..]
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
            };
            let s = String::from_utf8(bytes.to_vec()).ok();
            CredFree(p as *const core::ffi::c_void);
            s
        };
        match out {
            Some(s) if !s.trim().is_empty() => Ok(Some(s)),
            Some(_) => Ok(None),
            None => Err("La clave guardada está dañada; bórrala y vuelve a guardarla.".to_string()),
        }
    }

    pub fn delete_key(provider_id: &str) -> Result<(), String> {
        let target = wide(&target_name(provider_id));
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != 0 {
            return Ok(());
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("No se pudo borrar la clave (código de Windows {err})."))
        }
    }
}

#[cfg(windows)]
pub use imp::{delete_key, load_key, store_key};

#[cfg(not(windows))]
pub fn store_key(_provider_id: &str, key: &str) -> Result<(), String> {
    validate_key(key)?;
    Err("no soportado".to_string())
}
#[cfg(not(windows))]
pub fn load_key(_provider_id: &str) -> Result<Option<String>, String> {
    Err("no soportado".to_string())
}
#[cfg(not(windows))]
pub fn delete_key(_provider_id: &str) -> Result<(), String> {
    Err("no soportado".to_string())
}

/// `true` si hay una clave guardada (nunca devuelve su texto).
pub fn has_key(provider_id: &str) -> bool {
    matches!(load_key(provider_id), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_naming() {
        assert_eq!(target_name("gemini"), "LightMark/ai/gemini");
    }

    #[test]
    fn validate_rejects_bad_keys() {
        assert!(validate_key("").is_err());
        assert!(validate_key("   ").is_err());
        assert!(validate_key("ab cd").is_err());
        assert!(validate_key("ab\ncd").is_err());
        assert!(validate_key(&"a".repeat(2561)).is_err());
        assert_eq!(validate_key("  sk-test-1234 \n").unwrap(), "sk-test-1234");
        assert!(validate_key(&"a".repeat(2560)).is_ok());
    }

    #[test]
    fn errors_never_contain_key() {
        let e = validate_key("secret value").unwrap_err();
        assert!(!e.contains("secret"));
    }

    #[cfg(windows)]
    #[test]
    fn roundtrip_store_load_delete() {
        let id = "__wsd_test__";
        let _ = delete_key(id);
        assert_eq!(load_key(id).unwrap(), None);
        store_key(id, "  sk-test-roundtrip-ñ  ").unwrap();
        assert_eq!(load_key(id).unwrap().as_deref(), Some("sk-test-roundtrip-ñ"));
        assert!(has_key(id));
        // sobrescribe
        store_key(id, "second").unwrap();
        assert_eq!(load_key(id).unwrap().as_deref(), Some("second"));
        delete_key(id).unwrap();
        assert_eq!(load_key(id).unwrap(), None);
        // borrar de nuevo no es error
        delete_key(id).unwrap();
        assert!(!has_key(id));
    }
}
