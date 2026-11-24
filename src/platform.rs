//! The parts of the scan that differ between operating systems.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::tree::{Kind, NO_TIME};

/// One entry of a folder, as the scanner needs it.
pub struct Entry {
    pub name: Box<OsStr>,
    pub kind: Kind,
    pub size: u64,
    pub allocated: u64,
    pub modified: i64,
    /// Identity of a file with several hard links (counted once).
    pub link_id: Option<(u64, u64)>,
    /// Device of a folder (to stay on one file system).
    pub device: Option<u64>,
}

/// Last change of an entry in seconds since 1970, or [`NO_TIME`].
pub fn modified(md: &Metadata) -> i64 {
    match md.modified() {
        Ok(t) => match t.duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
            Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
        },
        Err(_) => NO_TIME,
    }
}

/// Folders that are not entered by default when a scan passes them.
pub fn default_skip_list() -> Vec<std::path::PathBuf> {
    if cfg!(target_os = "macos") {
        // The data volume is reachable a second time through firm links;
        // "/Volumes" holds other disks.
        ["/System/Volumes", "/Volumes", "/dev"].iter().map(Into::into).collect()
    } else {
        Vec::new()
    }
}

/// Whether folders on another file system are entered by default.
pub fn default_stay_on_filesystem() -> bool {
    // On macOS the system and data volumes are joined by firm links, so the
    // device number changes inside one logical tree; the skip list is used there.
    cfg!(all(unix, not(target_os = "macos")))
}

/// Windows time (100 ns steps since 1601) to seconds since 1970.
pub fn filetime_to_unix(t: i64) -> i64 {
    if t <= 0 { NO_TIME } else { t / 10_000_000 - 11_644_473_600 }
}

/// One record of a Windows `FILE_FULL_DIR_INFO` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinDirRecord {
    pub name: Vec<u16>,
    pub last_write: i64,
    pub end_of_file: u64,
    pub allocation: u64,
    pub attributes: u32,
    /// The reparse tag when the entry is a reparse point.
    pub ea_size: u32,
}

/// Reads the records of a `FILE_FULL_DIR_INFO` buffer. Pure and checked,
/// so it can be tested anywhere; a broken record ends the list.
pub fn parse_full_dir_info(buf: &[u8]) -> Vec<WinDirRecord> {
    const HEADER: usize = 68;
    let u32_at = |b: &[u8], o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let i64_at =
        |b: &[u8], o: usize| b.get(o..o + 8).map(|s| i64::from_le_bytes(s.try_into().unwrap_or([0; 8])));
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(rec) = buf.get(pos..) {
        let (Some(next), Some(write), Some(eof), Some(alloc), Some(attr), Some(name_len), Some(ea)) = (
            u32_at(rec, 0),
            i64_at(rec, 24),
            i64_at(rec, 40),
            i64_at(rec, 48),
            u32_at(rec, 56),
            u32_at(rec, 60),
            u32_at(rec, 64),
        ) else {
            break;
        };
        let Some(name_bytes) = rec.get(HEADER..HEADER + name_len as usize) else { break };
        let name = name_bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        out.push(WinDirRecord {
            name,
            last_write: write,
            end_of_file: eof.max(0) as u64,
            allocation: alloc.max(0) as u64,
            attributes: attr,
            ea_size: ea,
        });
        if next == 0 {
            break;
        }
        pos += next as usize;
    }
    out
}

#[cfg(unix)]
mod imp {
    use std::fs::{self, Metadata};
    use std::io;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use super::Entry;
    use crate::tree::Kind;

    pub struct Sizer;

    impl Sizer {
        pub fn new(_root: &Path) -> Self {
            Sizer
        }

        /// Space used on the disk.
        pub fn allocated(&self, _path: &dyn Fn() -> std::path::PathBuf, md: &Metadata) -> u64 {
            md.blocks().saturating_mul(512)
        }
    }

    pub fn read_entries(path: &Path, sizer: &Sizer) -> (Vec<Entry>, Option<io::Error>) {
        let mut entries = Vec::new();
        let mut error = None;
        let rd = match fs::read_dir(path) {
            Ok(rd) => rd,
            Err(e) => return (entries, Some(e)),
        };
        for ent in rd {
            let ent = match ent {
                Ok(ent) => ent,
                Err(e) => {
                    error.get_or_insert(e);
                    continue;
                }
            };
            // Does not follow links.
            let md = match ent.metadata() {
                Ok(md) => md,
                Err(e) => {
                    error.get_or_insert(e);
                    continue;
                }
            };
            let ft = md.file_type();
            let kind = if ft.is_symlink() {
                Kind::Link
            } else if ft.is_dir() {
                Kind::Dir
            } else if ft.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            let (size, allocated) = match kind {
                Kind::File => (md.len(), sizer.allocated(&|| ent.path(), &md)),
                Kind::Dir => (0, sizer.allocated(&|| ent.path(), &md)),
                Kind::Link | Kind::Other => (0, 0),
            };
            entries.push(Entry {
                name: ent.file_name().into_boxed_os_str(),
                kind,
                size,
                allocated,
                modified: super::modified(&md),
                link_id: (kind == Kind::File && md.nlink() > 1).then(|| (md.dev(), md.ino())),
                device: (kind == Kind::Dir).then(|| md.dev()),
            });
        }
        (entries, error)
    }

    pub fn device(md: &Metadata) -> Option<u64> {
        Some(md.dev())
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::fs::Metadata;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::MetadataExt;
    use std::path::{Component, Path, PathBuf, Prefix};

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE, NO_ERROR,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileFullDirectoryInfo, GetCompressedFileSizeW,
        GetDiskFreeSpaceW, GetFileInformationByHandleEx, GetVolumePathNameW, INVALID_FILE_SIZE,
        OPEN_EXISTING,
    };

    use super::{Entry, filetime_to_unix, parse_full_dir_info};
    use crate::tree::Kind;

    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_DEVICE: u32 = 0x40;
    const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x800;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    /// Reparse points that stand for another place (links, junctions, mount points).
    const NAME_SURROGATE: u32 = 0x2000_0000;
    /// Files whose space on the disk differs from their length rounded up
    /// to whole clusters: compressed, sparse and cloud files.
    const ASK_SYSTEM: u32 = FILE_ATTRIBUTE_SPARSE_FILE
        | FILE_ATTRIBUTE_COMPRESSED
        | FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    /// The path with the `\\?\` prefix, so long paths work too.
    fn long_path(p: &Path) -> PathBuf {
        match p.components().next() {
            Some(Component::Prefix(pre)) => match pre.kind() {
                Prefix::Verbatim(_)
                | Prefix::VerbatimDisk(_)
                | Prefix::VerbatimUNC(..)
                | Prefix::DeviceNS(_) => p.to_path_buf(),
                Prefix::UNC(..) => {
                    let s = p.as_os_str().to_string_lossy();
                    PathBuf::from(format!(r"\\?\UNC\{}", s.trim_start_matches('\\')))
                }
                Prefix::Disk(_) => {
                    let mut s = OsString::from(r"\\?\");
                    s.push(p.as_os_str());
                    PathBuf::from(s)
                }
            },
            _ => p.to_path_buf(),
        }
    }

    pub struct Sizer {
        cluster: u64,
    }

    impl Sizer {
        pub fn new(root: &Path) -> Self {
            Sizer { cluster: cluster_size(root).unwrap_or(4096) }
        }

        pub fn allocated(&self, path: &dyn Fn() -> PathBuf, md: &Metadata) -> u64 {
            if md.is_dir() {
                return 0;
            }
            let len = if md.file_attributes() & ASK_SYSTEM != 0 {
                compressed_size(&path()).unwrap_or(md.len())
            } else {
                md.len()
            };
            len.div_ceil(self.cluster).saturating_mul(self.cluster)
        }
    }

    fn cluster_size(root: &Path) -> Option<u64> {
        let name = wide(root);
        let mut volume = [0u16; 1024];
        // SAFETY: `name` is NUL-terminated; the buffer length is passed along.
        let ok = unsafe { GetVolumePathNameW(name.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) };
        if ok == 0 {
            return None;
        }
        let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
        // SAFETY: `volume` was NUL-terminated by the call above; all out pointers are valid.
        let ok = unsafe { GetDiskFreeSpaceW(volume.as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
        let size = u64::from(spc) * u64::from(bps);
        (ok != 0 && size > 0).then_some(size)
    }

    fn compressed_size(path: &Path) -> Option<u64> {
        let name = wide(&long_path(path));
        let mut high = 0u32;
        // SAFETY: `name` is NUL-terminated and `high` is a valid out pointer.
        let low = unsafe { GetCompressedFileSizeW(name.as_ptr(), &mut high) };
        // SAFETY: plain call without arguments.
        if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != NO_ERROR {
            return None;
        }
        Some((u64::from(high) << 32) | u64::from(low))
    }

    /// Lists a folder with one handle: names, sizes, space on disk and
    /// dates come in large blocks, without one call per file. With the
    /// backup right (admin helper) protected folders can be read as well.
    pub fn read_entries(path: &Path, _sizer: &Sizer) -> (Vec<Entry>, Option<io::Error>) {
        let name = wide(&long_path(path));
        // SAFETY: `name` is NUL-terminated; the other arguments are plain flags.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return (Vec::new(), Some(io::Error::last_os_error()));
        }
        let mut entries = Vec::new();
        let mut error = None;
        // u64 elements keep the buffer 8-byte aligned, as the records need.
        let mut buf = vec![0u64; 64 * 1024 / 8];
        loop {
            // SAFETY: `h` is an open folder handle; the buffer length is passed along.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    h,
                    FileFullDirectoryInfo,
                    buf.as_mut_ptr().cast(),
                    (buf.len() * 8) as u32,
                )
            };
            if ok == 0 {
                // SAFETY: plain call without arguments.
                let code = unsafe { GetLastError() };
                if code != ERROR_NO_MORE_FILES {
                    error = Some(io::Error::from_raw_os_error(code as i32));
                }
                break;
            }
            // SAFETY: the buffer is plain memory of exactly this many bytes.
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len() * 8) };
            for r in parse_full_dir_info(bytes) {
                if r.name == [b'.' as u16] || r.name == [b'.' as u16, b'.' as u16] {
                    continue;
                }
                let link =
                    r.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && r.ea_size & NAME_SURROGATE != 0;
                let kind = if link {
                    Kind::Link
                } else if r.attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                    Kind::Dir
                } else if r.attributes & FILE_ATTRIBUTE_DEVICE != 0 {
                    Kind::Other
                } else {
                    Kind::File
                };
                let name = OsString::from_wide(&r.name);
                let (size, allocated) = match kind {
                    Kind::File => {
                        let alloc =
                            if r.attributes & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0 {
                                compressed_size(&path.join(&name)).unwrap_or(r.allocation)
                            } else {
                                r.allocation
                            };
                        (r.end_of_file, alloc)
                    }
                    Kind::Dir => (0, r.allocation),
                    Kind::Link | Kind::Other => (0, 0),
                };
                entries.push(Entry {
                    name: name.into_boxed_os_str(),
                    kind,
                    size,
                    allocated,
                    modified: filetime_to_unix(r.last_write),
                    link_id: None,
                    device: None,
                });
            }
        }
        // SAFETY: `h` came from CreateFileW above.
        unsafe { CloseHandle(h) };
        (entries, error)
    }

    /// Mount points are reparse points and are never followed, so the
    /// device does not need to be compared.
    pub fn device(_md: &Metadata) -> Option<u64> {
        None
    }
}

pub use imp::{Sizer, device, read_entries};

/// Reads a folder; for tests and callers outside the scanner.
pub fn list(path: &Path) -> (Vec<Entry>, Option<io::Error>) {
    read_entries(path, &Sizer::new(path))
}

/// Whether `path` is on the skip list.
pub fn is_skipped(path: &Path, skip: &[std::path::PathBuf]) -> bool {
    skip.iter().any(|s| s == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, attr: u32, eof: i64, alloc: i64, write: i64, last: bool) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let len = (68 + name.len()).div_ceil(8) * 8;
        let mut r = vec![0u8; len];
        let next = if last { 0 } else { len as u32 };
        r[0..4].copy_from_slice(&next.to_le_bytes());
        r[24..32].copy_from_slice(&write.to_le_bytes());
        r[40..48].copy_from_slice(&eof.to_le_bytes());
        r[48..56].copy_from_slice(&alloc.to_le_bytes());
        r[56..60].copy_from_slice(&attr.to_le_bytes());
        r[60..64].copy_from_slice(&(name.len() as u32).to_le_bytes());
        r[68..68 + name.len()].copy_from_slice(&name);
        r
    }

    #[test]
    fn windows_folder_records() {
        // 2026-11-29 00:00 UTC as Windows time.
        let t = (1_795_910_400 + 11_644_473_600) * 10_000_000;
        let mut buf = record("Größe.txt", 0x20, 1234, 4096, t, false);
        buf.extend(record("Ordner", 0x10, 0, 0, t, true));
        buf.extend(record("ignored after last", 0x20, 1, 1, t, true));
        let recs = parse_full_dir_info(&buf);
        assert_eq!(recs.len(), 2);
        assert_eq!(String::from_utf16(&recs[0].name).unwrap(), "Größe.txt");
        assert_eq!((recs[0].end_of_file, recs[0].allocation), (1234, 4096));
        assert_eq!(filetime_to_unix(recs[0].last_write), 1_795_910_400);
        assert_eq!(recs[1].attributes, 0x10);
    }

    #[test]
    fn broken_windows_records_stop_cleanly() {
        let full = record("abc", 0x20, 1, 1, 1, false);
        for len in 0..full.len() {
            let _ = parse_full_dir_info(&full[..len]);
        }
        // A name length beyond the buffer ends the list.
        let mut bad = record("abc", 0x20, 1, 1, 1, true);
        bad[60..64].copy_from_slice(&1000u32.to_le_bytes());
        assert!(parse_full_dir_info(&bad).is_empty());
        assert_eq!(filetime_to_unix(0), NO_TIME);
    }
}
