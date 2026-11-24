//! Moving files and folders to the recycle bin – never deleting for good –
//! and the list of places that are protected from it.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::external::{self, Outcome};

/// Why a path must not be moved to the recycle bin, or `None` if it may.
pub fn protection(path: &Path) -> Option<&'static str> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    protection_with(path, home.as_deref())
}

fn lower_parts(p: &Path) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// The protection rules, with the home folder passed in (testable).
pub fn protection_with(path: &Path, home: Option<&Path>) -> Option<&'static str> {
    let parts = lower_parts(path);
    if parts.is_empty() {
        return Some("drive");
    }
    if let Some(home) = home {
        let h = lower_parts(home);
        if parts == h {
            return Some("home");
        }
        // The standard folders directly in the home folder stay; their content may go.
        if parts.len() == h.len() + 1 && parts.starts_with(&h) {
            const STANDARD: [&str; 12] = [
                "desktop",
                "documents",
                "downloads",
                "pictures",
                "music",
                "videos",
                "movies",
                "library",
                "appdata",
                "onedrive",
                "dokumente",
                "bilder",
            ];
            if STANDARD.contains(&parts[h.len()].as_str()) {
                return Some("user folder");
            }
        }
    }
    let first = parts[0].as_str();
    let windows_system = [
        "windows",
        "program files",
        "program files (x86)",
        "programdata",
        "system volume information",
        "$recycle.bin",
        "recovery",
        "boot",
    ];
    let windows_files = ["pagefile.sys", "hiberfil.sys", "swapfile.sys", "bootmgr"];
    let unix_system = [
        "bin",
        "boot",
        "dev",
        "etc",
        "lib",
        "lib32",
        "lib64",
        "libx32",
        "proc",
        "run",
        "sbin",
        "snap",
        "sys",
        "usr",
        "var",
        "opt",
        "root",
        "srv",
        "system",
        "private",
        "cores",
        "applications",
        "library",
    ];
    if cfg!(windows) {
        if windows_system.contains(&first) || (parts.len() == 1 && windows_files.contains(&first)) {
            return Some("system");
        }
    } else {
        // "/Applications/X.app" may go (that is how apps are removed on a Mac),
        // "/Applications" itself and everything else listed may not.
        let app_inside = first == "applications" && parts.len() >= 2 && cfg!(target_os = "macos");
        if unix_system.contains(&first) && !app_inside {
            return Some("system");
        }
        // Mount folders themselves stay; the disks inside them may be cleaned.
        if parts.len() == 1 && ["volumes", "mnt", "media", "home", "users"].contains(&first) {
            return Some("system");
        }
    }
    None
}

/// Moves `path` to the recycle bin of the system. Protected places are refused.
pub fn move_to_trash(path: &Path) -> io::Result<Outcome> {
    if let Some(why) = protection(path) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("protected ({why})")));
    }
    if external::is_blocked() {
        log::info!("outside action blocked (test mode): recycle bin");
        return Ok(Outcome::Blocked);
    }
    if std::fs::symlink_metadata(path).is_err() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
    }
    imp::trash(path)?;
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::other("the recycle bin did not take it"));
    }
    log::info!("moved to the recycle bin: {}", crate::applog::private(path));
    Ok(Outcome::Done)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
        SHFILEOPSTRUCTW, SHFileOperationW,
    };

    pub fn trash(path: &Path) -> io::Result<()> {
        // The list of paths ends with two zeros.
        let from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
        // SAFETY: plain C struct; all fields may be zero.
        let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        op.wFunc = FO_DELETE;
        op.pFrom = from.as_ptr();
        // Into the recycle bin; where there is none (e.g. a network drive)
        // Windows asks instead of deleting for good.
        op.fFlags =
            (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING) as u16;
        // SAFETY: `op` and the path it points to live until the call returns.
        let code = unsafe { SHFileOperationW(&mut op) };
        if op.fAnyOperationsAborted != 0 {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        if code != 0 {
            return Err(io::Error::other(format!("recycle bin refused it (code {code:#x})")));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::Path;

    pub fn trash(path: &Path) -> io::Result<()> {
        // The path is passed as a separate argument, never put into the script text.
        let out = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Finder\" to delete (POSIX file (item 1 of argv))",
            ])
            .args(["-e", "end run"])
            .arg(path)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(
                String::from_utf8_lossy(&out.stderr).trim().chars().take(300).collect::<String>(),
            ))
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};

    pub fn trash(path: &Path) -> io::Result<()> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok_or_else(|| io::Error::other("no home folder"))?;
        super::freedesktop_trash(path, &data.join("Trash"))
    }
}

/// Moves `path` into a freedesktop.org trash folder (`files/` and `info/`).
/// Only within one file system; across drives it refuses instead of copying.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub fn freedesktop_trash(path: &Path, trash: &Path) -> io::Result<()> {
    let files = trash.join("files");
    let info = trash.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
    let path = std::path::absolute(path)?;
    let encoded: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let now = crate::query::now();
    let date = format!(
        "{}T{:02}:{:02}:{:02}",
        crate::format::iso_date(now),
        now.rem_euclid(86_400) / 3600,
        now.rem_euclid(3600) / 60,
        now.rem_euclid(60)
    );
    for n in 1..10_000 {
        let candidate = if n == 1 { name.clone() } else { format!("{name}.{n}") };
        let info_file = info.join(format!("{candidate}.trashinfo"));
        // `create_new` reserves the name atomically.
        let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).open(&info_file) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        if files.join(&candidate).exists() {
            drop(f);
            let _ = std::fs::remove_file(&info_file);
            continue;
        }
        use std::io::Write;
        f.write_all(format!("[Trash Info]\nPath={encoded}\nDeletionDate={date}\n").as_bytes())?;
        if let Err(e) = std::fs::rename(&path, files.join(&candidate)) {
            let _ = std::fs::remove_file(&info_file);
            return Err(e);
        }
        return Ok(());
    }
    Err(io::Error::other("no free name in the trash"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_places() {
        let home = Path::new(if cfg!(windows) { r"C:\Users\anna" } else { "/home/anna" });
        let p = |s: &str| protection_with(Path::new(s), Some(home));
        if cfg!(windows) {
            assert!(p(r"C:\").is_some());
            assert!(p(r"C:\Windows\WinSxS").is_some());
            assert!(p(r"C:\Program Files\App").is_some());
            assert!(p(r"C:\pagefile.sys").is_some());
            assert!(p(r"C:\Users\anna").is_some());
            assert!(p(r"C:\Users\anna\Downloads").is_some());
            assert!(p(r"C:\Users\anna\Downloads\setup.exe").is_none());
            assert!(p(r"D:\Projekte\node_modules").is_none());
        } else {
            assert!(p("/").is_some());
            assert!(p("/usr/lib").is_some());
            assert!(p("/etc").is_some());
            assert!(p("/home/anna").is_some());
            assert!(p("/home/anna/Downloads").is_some());
            assert!(p("/home/anna/Downloads/film.mkv").is_none());
            assert!(p("/home/anna/projekt/node_modules").is_none());
            assert!(p("/mnt/daten/alt").is_none());
        }
    }

    #[test]
    fn trash_is_blocked_in_tests_and_protects() {
        assert_eq!(move_to_trash(Path::new("does-not-matter")).unwrap(), Outcome::Blocked);
        assert!(move_to_trash(Path::new(if cfg!(windows) { r"C:\Windows" } else { "/usr" })).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn freedesktop_layout() {
        let base = std::env::temp_dir().join(format!("mrsizeme-trash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join("ordner mit leer")).unwrap();
        std::fs::write(src.join("ordner mit leer/a.txt"), b"x").unwrap();
        std::fs::write(src.join("b.txt"), b"y").unwrap();
        std::fs::write(src.join("b2.txt"), b"z").unwrap();
        let trash = base.join("Trash");

        freedesktop_trash(&src.join("ordner mit leer"), &trash).unwrap();
        assert!(!src.join("ordner mit leer").exists());
        assert!(trash.join("files/ordner mit leer/a.txt").exists());
        let info = std::fs::read_to_string(trash.join("info/ordner mit leer.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(info.contains("ordner%20mit%20leer"));

        // The same name twice gets a number.
        freedesktop_trash(&src.join("b.txt"), &trash).unwrap();
        std::fs::rename(src.join("b2.txt"), src.join("b.txt")).unwrap();
        freedesktop_trash(&src.join("b.txt"), &trash).unwrap();
        assert!(trash.join("files/b.txt").exists() && trash.join("files/b.txt.2").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
