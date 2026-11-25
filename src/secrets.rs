//! The user's own API keys for cloud AI services.
//!
//! Windows: Credential Manager; macOS: Keychain; Linux: a file only the
//! user can read (`~/.config/mrsizeme/keys`, mode 0600). Keys never go into
//! settings, logs or the repository. In tests nothing is read or written.

use std::io;

/// The services a key can be stored for.
pub const GEMINI: &str = "gemini";
pub const CLAUDE: &str = "claude";
pub const OPENROUTER: &str = "openrouter";

fn blocked() -> io::Result<()> {
    if crate::external::is_blocked() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked in test mode"));
    }
    Ok(())
}

pub fn get(service: &str) -> Option<String> {
    blocked().ok()?;
    imp::get(service).filter(|k| !k.trim().is_empty())
}

pub fn set(service: &str, key: &str) -> io::Result<()> {
    blocked()?;
    imp::set(service, key.trim())
}

pub fn delete(service: &str) -> io::Result<()> {
    blocked()?;
    imp::delete(service)
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
        CredWriteW,
    };

    fn target(service: &str) -> Vec<u16> {
        OsStr::new(&format!("MrSizeMe/{service}")).encode_wide().chain(Some(0)).collect()
    }

    pub fn get(service: &str) -> Option<String> {
        let name = target(service);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; `cred` receives a buffer freed below.
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) } == 0 || cred.is_null() {
            return None;
        }
        // SAFETY: CredReadW returned a valid credential; the blob has the given size.
        let key = unsafe {
            let c = &*cred;
            let blob = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            String::from_utf8(blob.to_vec()).ok()
        };
        // SAFETY: the buffer came from CredReadW.
        unsafe { CredFree(cred.cast()) };
        key
    }

    pub fn set(service: &str, key: &str) -> io::Result<()> {
        let mut name = target(service);
        let mut user: Vec<u16> = OsStr::new("MrSizeMe").encode_wide().chain(Some(0)).collect();
        let mut blob = key.as_bytes().to_vec();
        // SAFETY: an all-zero CREDENTIALW is valid; the fields set below point to live buffers.
        let mut c: CREDENTIALW = unsafe { std::mem::zeroed() };
        c.Type = CRED_TYPE_GENERIC;
        c.TargetName = name.as_mut_ptr();
        c.UserName = user.as_mut_ptr();
        c.CredentialBlobSize = blob.len() as u32;
        c.CredentialBlob = blob.as_mut_ptr();
        c.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: `c` and its buffers live until the call returns.
        if unsafe { CredWriteW(&c, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn delete(service: &str) -> io::Result<()> {
        let name = target(service);
        // SAFETY: `name` is NUL-terminated.
        unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) };
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;

    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    const SERVICE: &str = "MrSizeMe";

    pub fn get(service: &str) -> Option<String> {
        get_generic_password(SERVICE, service).ok().and_then(|b| String::from_utf8(b).ok())
    }

    pub fn set(service: &str, key: &str) -> io::Result<()> {
        set_generic_password(SERVICE, service, key.as_bytes()).map_err(|e| io::Error::other(e.to_string()))
    }

    pub fn delete(service: &str) -> io::Result<()> {
        let _ = delete_generic_password(SERVICE, service);
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::io::{self, Write};
    use std::os::unix::fs::OpenOptionsExt;

    fn path() -> Option<std::path::PathBuf> {
        crate::settings::config_dir().map(|d| d.join("keys"))
    }

    fn read_all() -> Vec<(String, String)> {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| {
                t.lines()
                    .filter_map(|l| l.split_once('=').map(|(a, b)| (a.to_string(), b.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn write_all(entries: &[(String, String)]) -> io::Result<()> {
        let p = path().ok_or_else(|| io::Error::other("no configuration folder"))?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("tmp");
        let mut f =
            std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        for (k, v) in entries {
            writeln!(f, "{k}={v}")?;
        }
        f.sync_all()?;
        std::fs::rename(tmp, p)
    }

    pub fn get(service: &str) -> Option<String> {
        read_all().into_iter().find(|(k, _)| k == service).map(|(_, v)| v)
    }

    pub fn set(service: &str, key: &str) -> io::Result<()> {
        let mut all: Vec<_> = read_all().into_iter().filter(|(k, _)| k != service).collect();
        all.push((service.to_string(), key.replace(['\n', '\r'], "")));
        write_all(&all)
    }

    pub fn delete(service: &str) -> io::Result<()> {
        let all: Vec<_> = read_all().into_iter().filter(|(k, _)| k != service).collect();
        write_all(&all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_touch_real_key_stores() {
        assert!(get(CLAUDE).is_none());
        assert!(set(CLAUDE, "x").is_err());
        assert!(delete(GEMINI).is_err());
    }
}
