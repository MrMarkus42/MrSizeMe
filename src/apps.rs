//! Space per program: the program itself plus its data and caches.
//!
//! The list of programs comes from the system (Windows: the uninstall list
//! in the registry; macOS: the .app bundles; Linux: Flatpak, Snap and the
//! menu entries). Data folders are found by name in the usual places. Only
//! what lies inside the scan is counted.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::tree::{NodeId, Tree};

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub name: String,
    /// Folder of the program itself.
    pub location: Option<PathBuf>,
    /// Places where the program may keep data, settings and caches.
    pub data: Vec<PathBuf>,
    /// Size the system reports (Windows), if any.
    pub reported: Option<u64>,
}

/// One program with the space found in the scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub app: App,
    pub program: u64,
    pub data: u64,
    /// Node of the program folder, if it is in the scan.
    pub node: Option<NodeId>,
    /// The data folders that were found.
    pub data_found: Vec<PathBuf>,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.program + self.data
    }
}

/// The installed programs of this computer.
pub fn installed() -> Vec<App> {
    let mut v = imp::installed();
    v.sort_by_key(|a| a.name.to_lowercase());
    v.dedup_by(|a, b| a.name == b.name && a.location == b.location);
    v
}

/// Size of a path in the tree (0 if it is not part of the scan).
fn size_in(tree: &Tree, path: &Path) -> Option<(NodeId, u64)> {
    let id = tree.find(path)?;
    let n = tree.node(id);
    Some((id, n.allocated.max(n.size)))
}

/// Measures the programs with the scan; a folder is counted for one program only.
pub fn measure(tree: &Tree, apps: &[App]) -> Vec<Usage> {
    let mut used: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for app in apps {
        let mut u = Usage { app: app.clone(), program: 0, data: 0, node: None, data_found: Vec::new() };
        if let Some(loc) = &app.location
            && let Some((id, size)) = size_in(tree, loc)
            && used.insert(loc.clone())
        {
            u.program = size;
            u.node = Some(id);
        }
        for d in &app.data {
            // Not inside the program folder (already counted) and not twice.
            if app.location.as_ref().is_some_and(|l| d.starts_with(l)) || used.contains(d) {
                continue;
            }
            if let Some((_, size)) = size_in(tree, d) {
                used.insert(d.clone());
                u.data += size;
                u.data_found.push(d.clone());
            }
        }
        if u.total() > 0 {
            out.push(u);
        }
    }
    out.sort_by_key(|u| std::cmp::Reverse(u.total()));
    out
}

/// A display name without version numbers and brackets ("Firefox (x64 de)" → "Firefox").
pub fn short_name(name: &str) -> String {
    let base = name.split(['(', '[']).next().unwrap_or(name);
    let is_version = |w: &str| {
        let w = w.strip_prefix(['v', 'V']).unwrap_or(w);
        w.chars().any(|c| c.is_ascii_digit()) && w.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    let words: Vec<&str> = base.split_whitespace().take_while(|w| !is_version(w)).collect();
    words.join(" ").trim_end_matches(['-', ' ']).to_string()
}

/// Data folders named like the program below the given places.
fn data_places(bases: &[PathBuf], names: &[String]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for b in bases {
        for n in names.iter().filter(|n| !n.is_empty() && !n.contains(['/', '\\']) && *n != "." && *n != "..")
        {
            v.push(b.join(n));
        }
    }
    v
}

#[cfg(windows)]
mod imp {
    use super::{App, data_places, short_name};
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
        RegOpenKeyExW,
    };

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn string(key: HKEY, sub: &[u16], value: &str) -> Option<String> {
        let v = wide(value);
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: all strings end with a zero; the buffer length in bytes is passed along.
        let r = unsafe {
            RegGetValueW(
                key,
                sub.as_ptr(),
                v.as_ptr(),
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn dword(key: HKEY, sub: &[u16], value: &str) -> Option<u32> {
        let v = wide(value);
        let mut out = 0u32;
        let mut len = 4u32;
        // SAFETY: as above; the out value is a u32.
        let r = unsafe {
            RegGetValueW(
                key,
                sub.as_ptr(),
                v.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut out as *mut u32).cast(),
                &mut len,
            )
        };
        (r == ERROR_SUCCESS).then_some(out)
    }

    fn read(root: HKEY, view: u32, out: &mut Vec<App>) {
        let path = wide(UNINSTALL);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` ends with a zero; `key` receives the opened key.
        if unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ | view, &mut key) } != ERROR_SUCCESS {
            return;
        }
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_absolute());
        let bases: Vec<PathBuf> =
            ["APPDATA", "LOCALAPPDATA", "ProgramData"].iter().filter_map(|k| env(k)).collect();
        for i in 0.. {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            // SAFETY: `key` is open; the buffer length is passed along.
            let r = unsafe {
                RegEnumKeyExW(
                    key,
                    i,
                    name.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if r != ERROR_SUCCESS {
                break;
            }
            let sub: Vec<u16> = name[..len as usize].iter().copied().chain(Some(0)).collect();
            let Some(display) = string(key, &sub, "DisplayName") else { continue };
            // Parts of the system and updates are not programs of their own.
            if dword(key, &sub, "SystemComponent") == Some(1) || string(key, &sub, "ParentKeyName").is_some()
            {
                continue;
            }
            let location = string(key, &sub, "InstallLocation")
                .map(|s| PathBuf::from(s.trim_matches('"').trim_end_matches('\\')))
                .filter(|p| p.is_absolute() && p.components().count() > 2);
            let short = short_name(&display);
            let mut names = vec![short.clone()];
            if let Some(dir) = location.as_ref().and_then(|l| l.file_name()) {
                names.push(dir.to_string_lossy().into_owned());
            }
            if let Some(publisher) = string(key, &sub, "Publisher") {
                let p = short_name(&publisher);
                names.push(format!("{p}\\{short}"));
            }
            let mut data = data_places(&bases, &names[..names.len().min(2)]);
            if let Some(n) = names.get(2) {
                // "Publisher\Name" below the data folders.
                for b in &bases {
                    data.push(b.join(n));
                }
            }
            let reported = dword(key, &sub, "EstimatedSize").map(|kb| u64::from(kb) * 1024);
            out.push(App { name: display, location, data, reported });
        }
        // SAFETY: `key` was opened above.
        unsafe { RegCloseKey(key) };
    }

    pub fn installed() -> Vec<App> {
        let mut v = Vec::new();
        read(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, &mut v);
        read(HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, &mut v);
        read(HKEY_CURRENT_USER, 0, &mut v);
        v
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{App, data_places};
    use std::path::{Path, PathBuf};

    /// The bundle id from an XML Info.plist (binary ones are left out).
    fn bundle_id(app: &Path) -> Option<String> {
        use std::io::Read;
        let mut text = String::new();
        std::fs::File::open(app.join("Contents/Info.plist"))
            .ok()?
            .take(1 << 20)
            .read_to_string(&mut text)
            .ok()?;
        let after = text.split("<key>CFBundleIdentifier</key>").nth(1)?;
        let value = after.trim_start().strip_prefix("<string>")?.split("</string>").next()?.trim();
        let ok = !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c));
        ok.then(|| value.to_string())
    }

    pub fn installed() -> Vec<App> {
        let home = std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute());
        let mut dirs = vec![PathBuf::from("/Applications"), PathBuf::from("/Applications/Utilities")];
        if let Some(h) = &home {
            dirs.push(h.join("Applications"));
        }
        let lib = home.map(|h| h.join("Library"));
        let mut v = Vec::new();
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().is_none_or(|x| x != "app") {
                    continue;
                }
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let mut data = Vec::new();
                if let Some(lib) = &lib {
                    let mut names = vec![name.clone()];
                    if let Some(id) = bundle_id(&path) {
                        data.push(lib.join("Containers").join(&id));
                        data.push(lib.join("HTTPStorages").join(&id));
                        data.push(lib.join("WebKit").join(&id));
                        data.push(lib.join("Saved Application State").join(format!("{id}.savedState")));
                        names.push(id);
                    }
                    let bases = ["Application Support", "Caches", "Logs"].map(|b| lib.join(b));
                    data.extend(data_places(&bases, &names));
                }
                v.push(App { name, location: Some(path), data, reported: None });
            }
        }
        v
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{App, data_places};
    use std::path::PathBuf;

    /// Name and program name from the menu entries (.desktop files).
    fn desktop_entries(dir: &std::path::Path) -> Vec<(String, String)> {
        let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut v = Vec::new();
        for e in rd.flatten() {
            if e.path().extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let (mut name, mut exec) = (None, None);
            for line in text.lines().take(400) {
                if let Some(n) = line.strip_prefix("Name=") {
                    name.get_or_insert(n.trim().to_string());
                } else if let Some(x) = line.strip_prefix("Exec=") {
                    let first = x.split_whitespace().next().unwrap_or_default();
                    let base = first.rsplit('/').next().unwrap_or_default().to_string();
                    exec.get_or_insert(base);
                }
            }
            if let (Some(n), Some(x)) = (name, exec)
                && !x.is_empty()
            {
                v.push((n, x));
            }
        }
        v
    }

    pub fn installed() -> Vec<App> {
        let home = std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute());
        let mut v = Vec::new();
        // Flatpak and Snap: program and data folders by id.
        for (dir, user) in [("/var/lib/flatpak/app", ".var/app"), ("/snap", "snap")] {
            let Ok(rd) = std::fs::read_dir(dir) else { continue };
            for e in rd.flatten() {
                let id = e.file_name().to_string_lossy().into_owned();
                if id == "bin" || id == "core" || id.starts_with("core") || id == "snapd" {
                    continue;
                }
                let data = home.iter().map(|h| h.join(user).join(&id)).collect();
                v.push(App { name: id, location: Some(e.path()), data, reported: None });
            }
        }
        if let Some(h) = &home {
            let bases = [h.join(".config"), h.join(".cache"), h.join(".local/share")];
            let mut entries = desktop_entries(std::path::Path::new("/usr/share/applications"));
            entries.extend(desktop_entries(&h.join(".local/share/applications")));
            for (name, exec) in entries {
                if exec == "flatpak" || exec == "env" || exec.starts_with("snap") {
                    continue;
                }
                v.push(App { name, location: None, data: data_places(&bases, &[exec]), reported: None });
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;

    #[test]
    fn names_without_versions() {
        assert_eq!(short_name("Mozilla Firefox (x64 de)"), "Mozilla Firefox");
        assert_eq!(short_name("7-Zip 24.08 (x64)"), "7-Zip");
        assert_eq!(short_name("Visual Studio Code"), "Visual Studio Code");
        assert_eq!(short_name("Steam"), "Steam");
    }

    #[test]
    fn program_and_data_are_counted_once() {
        let mut t = Tree::new(PathBuf::from("/c"), Kind::Dir);
        let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
        let file = |n: &str, size: u64| {
            let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
            f.size = size;
            f
        };
        let top = t.add_children(ROOT, [dir("Programme"), dir("Daten")]);
        let p = t.add_children(top, [dir("Editor")]);
        t.add_children(p, [file("editor.exe", 300)]);
        let d = t.add_children(top + 1, [dir("Editor")]);
        t.add_children(d, [file("cache.bin", 700)]);
        t.sum_up();
        let app = |name: &str| App {
            name: name.into(),
            location: Some(PathBuf::from("/c/Programme/Editor")),
            data: vec![PathBuf::from("/c/Daten/Editor"), PathBuf::from("/c/Daten/Fehlt")],
            reported: None,
        };
        let other = App { name: "Leer".into(), location: None, data: vec![], reported: Some(5) };
        // The same folders listed twice are counted for the first program only.
        let u = measure(&t, &[app("Editor"), app("Editor 2"), other]);
        assert_eq!(u.len(), 1);
        assert_eq!((u[0].program, u[0].data, u[0].total()), (300, 700, 1000));
        assert_eq!(u[0].data_found, vec![PathBuf::from("/c/Daten/Editor")]);
        assert!(u[0].node.is_some());
    }

    #[test]
    fn data_places_skip_odd_names() {
        let v = data_places(&[PathBuf::from("/b")], &["ok".into(), "".into(), "..".into(), "a/b".into()]);
        assert_eq!(v, vec![PathBuf::from("/b/ok")]);
    }
}
