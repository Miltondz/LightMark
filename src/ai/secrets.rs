//! Almacén de claves de IA en el Administrador de credenciales de Windows (WS-D).
//! Wave 0: solo comprueba que las features de `windows-sys` necesarias enlazan;
//! WS-D implementa `store_key`/`load_key`/`delete_key`.
#![allow(dead_code)] // wave-1 fills (WS-D)

#[cfg(windows)]
mod smoke {
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    /// Referencia (sin llamar) todas las APIs que usará WS-D, para fallar en compilación
    /// si falta alguna feature de `windows-sys`.
    pub fn apis() -> (usize, u32, u32, usize) {
        let _w: unsafe extern "system" fn(*const CREDENTIALW, u32) -> i32 = CredWriteW;
        let _r: unsafe extern "system" fn(*const u16, u32, u32, *mut *mut CREDENTIALW) -> i32 =
            CredReadW;
        let _d: unsafe extern "system" fn(*const u16, u32, u32) -> i32 = CredDeleteW;
        let _f: unsafe extern "system" fn(*const core::ffi::c_void) = CredFree;
        (
            std::mem::size_of::<CREDENTIALW>(),
            CRED_TYPE_GENERIC,
            CRED_PERSIST_LOCAL_MACHINE,
            0,
        )
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn credential_apis_link() {
        let (size, ty, persist, _) = super::smoke::apis();
        assert!(size > 0);
        assert_eq!(ty, 1);
        assert_eq!(persist, 2);
    }
}
