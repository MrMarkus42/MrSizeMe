//! Cloud folders (OneDrive, iCloud Drive, Dropbox …): what is only online
//! and what takes space on this computer, and "keep only online".

use std::io;
use std::path::{Path, PathBuf};

use crate::external::{self, Outcome};
use crate::tree::{Kind, NodeId, ROOT, Tree, flags};

/// A cloud folder found on this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub name: String,
    pub path: PathBuf,
}

/// The cloud folders of the user (only those that exist).
pub fn roots() -> Vec<Root> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut v = candidates(home.as_deref(), &env);
    v.retain(|r| r.path.is_dir());
    v.dedup_by(|a, b| a.path == b.path);
    v
}

/// Where cloud folders usually are (without checking that they exist).
pub fn candidates(home: Option<&Path>, env: &dyn Fn(&str) -> Option<PathBuf>) -> Vec<Root> {
    let mut v = Vec::new();
    let mut add = |name: &str, path: PathBuf| {
        if path.is_absolute() {
            v.push(Root { name: name.to_string(), path });
        }
    };
    if cfg!(windows) {
        for (var, name) in [
            ("OneDriveCommercial", "OneDrive (Arbeit/Work)"),
            ("OneDriveConsumer", "OneDrive"),
            ("OneDrive", "OneDrive"),
        ] {
            if let Some(p) = env(var) {
                add(name, p);
            }
        }
        if let Some(h) = home {
            add("iCloud Drive", h.join("iCloudDrive"));
            add("Dropbox", h.join("Dropbox"));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = home {
            add("iCloud Drive", h.join("Library/Mobile Documents"));
            // OneDrive, Dropbox, Google Drive and others since macOS 12.
            if let Ok(rd) = std::fs::read_dir(h.join("Library/CloudStorage")) {
                for e in rd.flatten() {
                    let name =
                        e.file_name().to_string_lossy().split('-').next().unwrap_or_default().to_string();
                    add(&name, e.path());
                }
            }
        }
    } else if let Some(h) = home {
        add("Dropbox", h.join("Dropbox"));
        add("OneDrive", h.join("OneDrive"));
    }
    v
}

/// Space of a part of a cloud folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Space used on this computer.
    pub local: u64,
    /// Size of the files that are only online.
    pub online: u64,
    pub online_files: u64,
}

/// Adds up a node: what is here and what is only online.
pub fn usage(tree: &Tree, id: NodeId) -> Usage {
    let mut u = Usage { local: tree.node(id).allocated, ..Usage::default() };
    let mut stack = vec![id];
    while let Some(d) = stack.pop() {
        let n = tree.node(d);
        if n.kind == Kind::File && n.has(flags::ONLINE_ONLY) {
            u.online += n.size;
            u.online_files += 1;
        }
        stack.extend_from_slice(tree.children(d));
    }
    u
}

/// The node of a cloud folder in the tree: the folder itself, or the whole
/// scan if the scan lies inside the cloud folder.
pub fn node_of(tree: &Tree, root: &Path) -> Option<NodeId> {
    if tree.root_path.starts_with(root) { Some(ROOT) } else { tree.find(root) }
}

/// True if "keep only online" works on this system.
pub fn can_free_up() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

/// Keeps a file or folder only online: the cloud service removes the local
/// copy and fetches it again when it is opened. Nothing is deleted.
pub fn free_up(path: &Path) -> io::Result<Outcome> {
    if external::is_blocked() {
        log::info!("outside action blocked (test mode): keep only online");
        return Ok(Outcome::Blocked);
    }
    imp::free_up(path)?;
    Ok(Outcome::Done)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::CloudFilters::{
        CF_PIN_STATE_UNPINNED, CF_SET_PIN_FLAG_NONE, CF_SET_PIN_FLAG_RECURSE, CfSetPinState,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING,
    };

    /// Marks the item as "unpinned"; the sync program (OneDrive) then frees its space.
    pub fn free_up(path: &Path) -> io::Result<()> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `name` ends with a zero; the other arguments are plain flags.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let flags = if path.is_dir() { CF_SET_PIN_FLAG_RECURSE } else { CF_SET_PIN_FLAG_NONE };
        // SAFETY: `h` is an open handle; no overlapped structure is used.
        let hr = unsafe { CfSetPinState(h, CF_PIN_STATE_UNPINNED, flags, std::ptr::null_mut()) };
        // SAFETY: `h` came from CreateFileW above.
        unsafe { CloseHandle(h) };
        if hr < 0 {
            return Err(io::Error::from_raw_os_error(hr));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::Path;
    use std::process::{Command, Stdio};

    /// iCloud Drive: `brctl evict`; other cloud services: `fileproviderctl evict`.
    /// The path is one plain argument, never passed through a shell.
    pub fn free_up(path: &Path) -> io::Result<()> {
        let icloud = path.components().any(|c| c.as_os_str() == "Mobile Documents");
        let (tool, verb) =
            if icloud { ("/usr/bin/brctl", "evict") } else { ("/usr/bin/fileproviderctl", "evict") };
        let out = Command::new(tool).arg(verb).arg(path).stdin(Stdio::null()).output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_string()))
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::io;
    use std::path::Path;

    pub fn free_up(_path: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_PARENT, Node};
    use std::ffi::OsStr;

    #[test]
    fn usage_counts_online_files_apart() {
        let mut t = Tree::new(PathBuf::from("/home/x/OneDrive"), Kind::Dir);
        let mut here = Node::new(OsStr::new("here.pdf").into(), NO_PARENT, Kind::File);
        here.size = 100;
        here.allocated = 4096;
        let mut away = Node::new(OsStr::new("away.mp4").into(), NO_PARENT, Kind::File);
        away.size = 5000;
        away.flags |= flags::ONLINE_ONLY;
        t.add_children(ROOT, [here, away]);
        t.sum_up();
        let u = usage(&t, ROOT);
        assert_eq!(u, Usage { local: 4096, online: 5000, online_files: 1 });
        assert_eq!(node_of(&t, Path::new("/home/x")), Some(ROOT));
        assert_eq!(node_of(&t, Path::new("/other")), None);
    }

    #[test]
    fn places_and_test_switch() {
        let home = if cfg!(windows) { PathBuf::from(r"C:\Users\x") } else { PathBuf::from("/home/x") };
        let env = |k: &str| (k == "OneDrive").then(|| home.join("OneDrive"));
        let c = candidates(Some(&home), &env);
        assert!(!c.is_empty());
        assert!(c.iter().all(|r| r.path.starts_with(&home)));
        // Never acts in tests.
        assert_eq!(free_up(&home).unwrap(), Outcome::Blocked);
    }
}
