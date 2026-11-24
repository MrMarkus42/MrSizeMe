//! Drives and other good starting points for a scan.

use std::path::PathBuf;

use crate::i18n::tr;

pub struct Place {
    pub label: String,
    pub path: PathBuf,
}

pub fn list() -> Vec<Place> {
    let mut v = Vec::new();
    imp::drives(&mut v);
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(home) = home.map(PathBuf::from).filter(|p| p.is_absolute()) {
        v.push(Place { label: tr().home_folder.to_string(), path: home });
    }
    v
}

#[cfg(windows)]
mod imp {
    use super::Place;
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    pub fn drives(v: &mut Vec<Place>) {
        // A bit mask of the drive letters; no disk is touched (no waiting
        // for empty card readers or lost network drives).
        // SAFETY: plain call without arguments.
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u8 {
            if mask & (1 << i) != 0 {
                let label = format!("{}:\\", char::from(b'A' + i));
                v.push(Place { path: label.clone().into(), label });
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Place;

    pub fn drives(v: &mut Vec<Place>) {
        v.push(Place { label: "/".into(), path: "/".into() });
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                // The system disk appears here once more as a link to "/".
                if e.file_type().is_ok_and(|t| t.is_symlink()) {
                    continue;
                }
                v.push(Place { label: e.file_name().to_string_lossy().into_owned(), path: e.path() });
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::Place;

    pub fn drives(v: &mut Vec<Place>) {
        v.push(Place { label: "/".into(), path: "/".into() });
        let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else { return };
        for point in real_mount_points(&mounts) {
            if point != "/" && !v.iter().any(|p| p.path.as_os_str() == point.as_str()) {
                v.push(Place { label: point.clone(), path: point.into() });
            }
        }
    }

    /// Mount points of real disks (device under /dev), without system parts.
    pub fn real_mount_points(mounts: &str) -> Vec<String> {
        mounts
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let (dev, point) = (f.next()?, f.next()?);
                let point = unescape(point);
                let system = ["/boot", "/proc", "/sys", "/dev", "/run", "/snap"]
                    .iter()
                    .any(|s| point == *s || point.starts_with(&format!("{s}/")));
                (dev.starts_with("/dev/") && !system).then_some(point)
            })
            .collect()
    }

    /// Undoes the octal escapes of /proc/self/mounts ("\040" = space).
    fn unescape(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
                out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
                i += 4;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mount_points() {
            let m = "/dev/sda2 / ext4 rw 0 0\nproc /proc proc rw 0 0\n/dev/sdb1 /mnt/meine\\040Daten ext4 rw 0 0\n\
                     /dev/sda1 /boot/efi vfat rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n";
            assert_eq!(real_mount_points(m), ["/", "/mnt/meine Daten"]);
        }
    }
}
