//! Scanning with administrator rights.
//!
//! The window itself never runs with raised rights. On Windows and Linux a
//! second copy of the program is started with admin rights in a helper
//! mode: it only scans and hands the result back (Windows: a file in a
//! fresh temp folder, Linux: through a pipe). On macOS admin rights do not
//! help; there the program checks for "Full Disk Access" and explains it.

use std::io;
use std::path::{Path, PathBuf};

use crate::scan;
use crate::snapshot;
use crate::tree::Tree;

/// Command line switch of the helper mode.
pub const HELPER_ARG: &str = "--scan-helper";
/// Prefix of the temp folder the helper may write into.
const OUT_DIR_PREFIX: &str = "mrsizeme-admin-";
const OUT_FILE: &str = "scan.bin";

/// What this system offers to see all folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSupport {
    /// A helper with admin rights can scan (Windows, Linux).
    Helper,
    /// Full Disk Access must be granted in the system settings (macOS).
    FullDiskAccess,
}

pub fn support() -> AdminSupport {
    if cfg!(target_os = "macos") { AdminSupport::FullDiskAccess } else { AdminSupport::Helper }
}

/// True if this process already has admin (root) rights.
pub fn is_elevated() -> bool {
    imp::is_elevated()
}

/// Checks the output target of the helper: standard output ("-") or a
/// new file `scan.bin` in a folder made by the window for this purpose.
/// This keeps the helper from being misused to overwrite other files.
pub fn checked_output(out: &Path) -> io::Result<Option<PathBuf>> {
    if out.as_os_str() == "-" {
        return Ok(None);
    }
    let ok_name = out.file_name().is_some_and(|n| n == OUT_FILE);
    let ok_dir = out
        .parent()
        .and_then(|d| d.file_name())
        .is_some_and(|n| n.to_string_lossy().starts_with(OUT_DIR_PREFIX));
    if !ok_name || !ok_dir || out.exists() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a valid helper output"));
    }
    Ok(Some(out.to_path_buf()))
}

/// The helper mode: scan `root`, write the result to `out`. Returns the exit code.
pub fn run_helper(root: &Path, out: &Path) -> i32 {
    let target = match checked_output(out) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    imp::enable_backup_privilege();
    let tree = match scan::scan_with_admin_rights(root, &scan::Options::default(), &scan::Progress::default())
    {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", root.display());
            return 1;
        }
    };
    let result = match target {
        None => {
            let stdout = io::stdout();
            let mut w = io::BufWriter::new(stdout.lock());
            snapshot::write(&tree, &mut w)
        }
        Some(path) => std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|f| snapshot::write(&tree, &mut io::BufWriter::new(f))),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// Starts the helper with admin rights (the system asks the user) and
/// waits for its result. Blocks; call it from a background thread.
pub fn scan_as_admin(root: &Path) -> io::Result<Tree> {
    if crate::external::is_blocked() {
        return Err(io::Error::other("blocked in test mode"));
    }
    let exe = std::env::current_exe()?;
    imp::run_elevated(&exe, root)
}

/// True if the user cancelled the admin request (no error message needed).
pub fn is_cancelled(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::Interrupted
}

/// macOS: whether the program may read folders protected by Full Disk Access.
/// `None` if this cannot be told (not macOS, or no protected folder found).
pub fn has_full_disk_access() -> Option<bool> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    for probe in ["Library/Safari", "Library/Mail", "Library/Messages"] {
        let p = home.join(probe);
        if !p.exists() {
            continue;
        }
        return Some(match std::fs::read_dir(&p) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => false,
            Err(_) => continue,
        });
    }
    None
}

/// Quotes one argument for a Windows command line so that the program
/// receives it unchanged (rules of `CommandLineToArgvW`).
pub fn quote_windows_arg(arg: &[u16]) -> Vec<u16> {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    let needs = arg.is_empty() || arg.iter().any(|&c| c == b' ' as u16 || c == b'\t' as u16 || c == QUOTE);
    if !needs {
        return arg.to_vec();
    }
    let mut out = vec![QUOTE];
    let mut backslashes = 0;
    for &c in arg {
        if c == BACKSLASH {
            backslashes += 1;
        } else if c == QUOTE {
            out.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2 + 1));
            out.push(QUOTE);
            backslashes = 0;
        } else {
            out.extend(std::iter::repeat_n(BACKSLASH, backslashes));
            out.push(c);
            backslashes = 0;
        }
    }
    out.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
    out.push(QUOTE);
    out
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, GetTokenInformation, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
        SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
        TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    use super::{HELPER_ARG, OUT_DIR_PREFIX, OUT_FILE, quote_windows_arg};
    use crate::tree::Tree;

    const ERROR_CANCELLED: u32 = 1223;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    fn token(access: u32) -> Option<HANDLE> {
        let mut h: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo handle of the own process; `h` is a valid out pointer.
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut h) };
        (ok != 0).then_some(h)
    }

    pub fn is_elevated() -> bool {
        let Some(h) = token(TOKEN_QUERY) else { return false };
        let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        // SAFETY: `e` is large enough for TokenElevation; the handle is closed below.
        let ok = unsafe {
            GetTokenInformation(
                h,
                TokenElevation,
                (&mut e as *mut TOKEN_ELEVATION).cast(),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
        };
        // SAFETY: `h` came from OpenProcessToken.
        unsafe { CloseHandle(h) };
        ok != 0 && e.TokenIsElevated != 0
    }

    /// Turns on the backup right (only works for admins): folders can then
    /// be listed even where their permissions would forbid it.
    pub fn enable_backup_privilege() {
        let Some(h) = token(TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY) else { return };
        let name = wide(OsStr::new("SeBackupPrivilege"));
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        // SAFETY: `name` is NUL-terminated; `luid` is a valid out pointer.
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } != 0 {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            // SAFETY: `tp` is a complete TOKEN_PRIVILEGES; no previous state is requested.
            unsafe { AdjustTokenPrivileges(h, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) };
        }
        // SAFETY: `h` came from OpenProcessToken.
        unsafe { CloseHandle(h) };
    }

    pub fn run_elevated(exe: &Path, root: &Path) -> io::Result<Tree> {
        // A fresh folder in the user's temp folder; the helper may only create scan.bin in it.
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("{OUT_DIR_PREFIX}{}-{stamp}", std::process::id()));
        std::fs::create_dir(&dir)?;
        let out = dir.join(OUT_FILE);
        let result = launch_and_read(exe, root, &out);
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn launch_and_read(exe: &Path, root: &Path, out: &Path) -> io::Result<Tree> {
        let mut params: Vec<u16> = Vec::new();
        for (i, arg) in [OsStr::new(HELPER_ARG), root.as_os_str(), out.as_os_str()].iter().enumerate() {
            if i > 0 {
                params.push(b' ' as u16);
            }
            params.extend(quote_windows_arg(&arg.encode_wide().collect::<Vec<_>>()));
        }
        params.push(0);
        let file = wide(exe.as_os_str());
        let verb = wide(OsStr::new("runas"));
        // SAFETY: an all-zero SHELLEXECUTEINFOW is valid (null pointers, no flags).
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        // SAFETY: all strings are NUL-terminated and outlive the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            // SAFETY: plain call without arguments.
            let code = unsafe { GetLastError() };
            if code == ERROR_CANCELLED {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled by the user"));
            }
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let process = info.hProcess;
        if process.is_null() {
            return Err(io::Error::other("admin helper did not start"));
        }
        let mut code = 1u32;
        // SAFETY: `process` is the handle returned by ShellExecuteExW; closed below.
        unsafe {
            WaitForSingleObject(process, INFINITE);
            GetExitCodeProcess(process, &mut code);
            CloseHandle(process);
        }
        if code != 0 {
            return Err(io::Error::other(format!("admin helper failed (code {code})")));
        }
        let data = std::fs::read(out)?;
        crate::snapshot::from_bytes(&data)
    }
}

#[cfg(unix)]
mod imp {
    use std::io;
    use std::path::Path;

    use crate::tree::Tree;

    pub fn is_elevated() -> bool {
        use std::os::unix::fs::MetadataExt;
        // /proc/self belongs to the effective user of this process (Linux).
        std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
    }

    pub fn enable_backup_privilege() {}

    #[cfg(target_os = "macos")]
    pub fn run_elevated(_exe: &Path, _root: &Path) -> io::Result<Tree> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "not used on macOS"))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn run_elevated(exe: &Path, root: &Path) -> io::Result<Tree> {
        use std::io::Read;
        use std::process::{Command, Stdio};

        // pkexec asks for the password in a system dialog and starts the helper as root.
        let mut child = Command::new("pkexec")
            .arg(exe)
            .arg(super::HELPER_ARG)
            .arg(root)
            .arg("-")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    io::Error::new(io::ErrorKind::NotFound, "pkexec (polkit) is not installed")
                } else {
                    e
                }
            })?;
        let mut data = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            out.read_to_end(&mut data)?;
        }
        let status = child.wait()?;
        match status.code() {
            Some(0) => crate::snapshot::from_bytes(&data),
            // 126: the user closed the password dialog or is not allowed.
            Some(126) => Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled by the user")),
            other => Err(io::Error::other(format!("admin helper failed (code {other:?})"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> String {
        String::from_utf16(&quote_windows_arg(&s.encode_utf16().collect::<Vec<_>>())).unwrap()
    }

    #[test]
    fn windows_quoting() {
        assert_eq!(q(r"C:\Daten"), r"C:\Daten");
        assert_eq!(q(r"C:\Program Files"), r#""C:\Program Files""#);
        assert_eq!(q(r"C:\Mit Leer\"), r#""C:\Mit Leer\\""#);
        assert_eq!(q(r#"a"b"#), r#""a\"b""#);
        assert_eq!(q(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(q(""), r#""""#);
    }

    #[test]
    fn helper_output_is_restricted() {
        assert_eq!(checked_output(Path::new("-")).unwrap(), None);
        let dir = std::env::temp_dir().join(format!("{OUT_DIR_PREFIX}test-{}", std::process::id()));
        let good = dir.join(OUT_FILE);
        assert_eq!(checked_output(&good).unwrap(), Some(good.clone()));
        assert!(checked_output(&dir.join("other.bin")).is_err());
        assert!(checked_output(&std::env::temp_dir().join(OUT_FILE)).is_err());
        assert!(checked_output(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn admin_scan_is_blocked_in_tests() {
        assert!(scan_as_admin(Path::new(".")).is_err());
    }
}
