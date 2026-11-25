//! Size and free space of a disk, and the warning when a disk is nearly full.

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    /// Space the user can still use.
    pub free: u64,
}

impl Space {
    /// Free space in percent of the whole disk.
    pub fn free_percent(&self) -> f64 {
        if self.total == 0 { 100.0 } else { self.free as f64 * 100.0 / self.total as f64 }
    }
}

/// Size and free space of the disk that holds `path`.
pub fn space(path: &Path) -> io::Result<Space> {
    imp::space(path)
}

/// A disk with less free space than the user wants.
#[derive(Debug, Clone, PartialEq)]
pub struct Low {
    pub label: String,
    pub path: PathBuf,
    pub space: Space,
}

/// Checks the given disks; `percent` 0 switches the warning off.
pub fn low_disks(disks: &[(String, PathBuf)], percent: u8) -> Vec<Low> {
    if percent == 0 {
        return Vec::new();
    }
    disks
        .iter()
        .filter_map(|(label, path)| {
            let space = space(path).ok()?;
            (space.total > 0 && space.free_percent() < f64::from(percent)).then(|| Low {
                label: label.clone(),
                path: path.clone(),
                space,
            })
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::Space;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    pub fn space(path: &Path) -> io::Result<Space> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
        // SAFETY: the path ends with a zero; the three pointers point to u64 values.
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Space { total, free })
    }

    /// Only fixed disks: no waiting for card readers, CD drives or lost network drives.
    pub fn is_fixed(path: &Path) -> bool {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        const DRIVE_FIXED: u32 = 3;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: the path ends with a zero.
        unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
    }
}

#[cfg(unix)]
mod imp {
    use super::Space;
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn space(path: &Path) -> io::Result<Space> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
        // SAFETY: statvfs only writes into the given struct; the path ends with a zero.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let unit = s.f_frsize as u64;
        Ok(Space {
            total: (s.f_blocks as u64).saturating_mul(unit),
            free: (s.f_bavail as u64).saturating_mul(unit),
        })
    }

    pub fn is_fixed(_path: &Path) -> bool {
        true
    }
}

/// True for disks that are checked by the warning (on Windows only fixed disks).
pub fn is_fixed(path: &Path) -> bool {
    imp::is_fixed(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_of_the_temp_folder() {
        let s = space(&std::env::temp_dir()).unwrap();
        assert!(s.total > 0 && s.free <= s.total);
        assert!(space(Path::new("/does/not/exist/at/all")).is_err());
    }

    #[test]
    fn warning_threshold() {
        let disks = vec![("tmp".to_string(), std::env::temp_dir())];
        assert!(low_disks(&disks, 0).is_empty());
        // Every disk has less than 101 % free.
        assert_eq!(low_disks(&disks, 101).len(), 1);
        let s = Space { total: 200, free: 10 };
        assert_eq!(s.free_percent(), 5.0);
    }
}
