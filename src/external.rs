//! Everything that reaches outside the program (opening the file manager).
//!
//! One central switch blocks all of it. Unit tests always block; the shared
//! setup of the integration tests calls [`block_for_tests`]; the variable
//! `MRSIZEME_NO_EXTERNAL` blocks it for test runs of the finished program.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static BLOCKED: AtomicBool = AtomicBool::new(false);

/// Blocks every outside action for the rest of the process.
pub fn block_for_tests() {
    BLOCKED.store(true, Ordering::SeqCst);
}

/// True if outside actions are blocked.
pub fn is_blocked() -> bool {
    cfg!(test) || BLOCKED.load(Ordering::SeqCst) || std::env::var_os("MRSIZEME_NO_EXTERNAL").is_some()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// Not done because of the test switch.
    Blocked,
}

/// Opens a folder in the system's file manager. The path is handed over as
/// one argument (never through a shell), so its characters have no meaning.
pub fn open_folder(path: &Path) -> std::io::Result<Outcome> {
    if is_blocked() {
        log::info!("outside action blocked (test mode): open folder");
        return Ok(Outcome::Blocked);
    }
    if !path.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Ordner nicht gefunden"));
    }
    imp::open_folder(path)?;
    Ok(Outcome::Done)
}

/// macOS: opens System Settings at "Privacy & Security → Full Disk Access".
pub fn open_full_disk_access_settings() -> std::io::Result<Outcome> {
    if is_blocked() {
        log::info!("outside action blocked (test mode): open system settings");
        return Ok(Outcome::Blocked);
    }
    if !cfg!(target_os = "macos") {
        return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "only on macOS"));
    }
    let mut child = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(Outcome::Done)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    pub fn open_folder(path: &Path) -> std::io::Result<()> {
        let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let verb: Vec<u16> = "explore".encode_utf16().chain(Some(0)).collect();
        // SAFETY: both strings are NUL-terminated; the other pointers may be null.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // Values above 32 mean success.
        if result as isize > 32 {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "Explorer konnte nicht geöffnet werden (Code {})",
                result as isize
            )))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    use std::process::{Command, Stdio};

    pub fn open_folder(path: &Path) -> std::io::Result<()> {
        let program = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        // "--" is not understood by these tools; a path starting with "-" is
        // made harmless by making it absolute (it then starts with "/").
        let path = std::path::absolute(path)?;
        let mut child = Command::new(program)
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // Collect the exit status so no zombie process is left behind.
        std::thread::spawn(move || child.wait());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_tests_never_reach_outside() {
        assert!(is_blocked());
        assert_eq!(open_folder(Path::new(".")).unwrap(), Outcome::Blocked);
    }
}
