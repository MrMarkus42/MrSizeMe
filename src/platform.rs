//! The parts of the scan that differ between operating systems.

use std::fs::Metadata;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::tree::NO_TIME;

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

#[cfg(unix)]
mod imp {
    use std::fs::Metadata;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

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

    /// Identity of a file with more than one hard link, so it is counted once.
    pub fn link_id(md: &Metadata) -> Option<(u64, u64)> {
        (md.is_file() && md.nlink() > 1).then(|| (md.dev(), md.ino()))
    }

    pub fn device(md: &Metadata) -> Option<u64> {
        Some(md.dev())
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::Metadata;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::{GetLastError, NO_ERROR};
    use windows_sys::Win32::Storage::FileSystem::{
        GetCompressedFileSizeW, GetDiskFreeSpaceW, GetVolumePathNameW, INVALID_FILE_SIZE,
    };

    const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
    const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x800;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
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
        let name = wide(path);
        let mut high = 0u32;
        // SAFETY: `name` is NUL-terminated and `high` is a valid out pointer.
        let low = unsafe { GetCompressedFileSizeW(name.as_ptr(), &mut high) };
        // SAFETY: plain call without arguments.
        if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != NO_ERROR {
            return None;
        }
        Some((u64::from(high) << 32) | u64::from(low))
    }

    /// Hard links are not checked on Windows (it would need one open per file).
    pub fn link_id(_md: &Metadata) -> Option<(u64, u64)> {
        None
    }

    /// Mount points are reparse points and are never followed, so the
    /// device does not need to be compared.
    pub fn device(_md: &Metadata) -> Option<u64> {
        None
    }
}

pub use imp::{Sizer, device, link_id};

/// Whether `path` is on the skip list.
pub fn is_skipped(path: &Path, skip: &[std::path::PathBuf]) -> bool {
    skip.iter().any(|s| s == path)
}
