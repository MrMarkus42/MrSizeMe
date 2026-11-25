//! The move assistant: moves a large folder to another disk.
//!
//! Order of work, so that nothing is lost: copy everything, check every
//! copied file, only then remove the original (recycle bin or delete, as
//! the user chose), and if wanted put a link at the old place so programs
//! still find the folder. On any problem before the original is removed,
//! the copy is deleted again and the original stays as it was.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Original {
    /// Into the recycle bin (the space is free once it is emptied).
    Trash,
    /// Deleted for good after the copy was checked.
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub source: PathBuf,
    /// The folder is moved into this folder (keeping its name).
    pub target_dir: PathBuf,
    /// Put a link at the old place.
    pub link: bool,
    pub original: Original,
}

impl Plan {
    pub fn target(&self) -> PathBuf {
        self.target_dir.join(self.source.file_name().unwrap_or_default())
    }
}

/// Why a plan cannot run (or a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    /// The folder is protected (system folder, user folder …).
    Protected,
    NotAFolder,
    TargetMissing,
    TargetInsideSource,
    TargetExists,
    /// Not enough free space at the target (needed, free).
    NoSpace(u64, u64),
    /// The folder lies in a cloud folder (it would be removed from the cloud).
    InCloud,
    /// Windows: the folder contains links, which cannot be copied safely.
    HasLinks,
    /// Windows: a link to a network place is not possible.
    LinkToNetwork,
    /// Warning only: the target is on the same disk, no space becomes free.
    SameDisk,
}

impl Problem {
    pub fn blocks(&self) -> bool {
        !matches!(self, Problem::SameDisk)
    }
}

#[derive(Default)]
pub struct Progress {
    pub copied: AtomicU64,
    pub total: AtomicU64,
    cancel: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// What happened.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub files: u64,
    pub bytes: u64,
    /// The original went to the recycle bin (false: deleted, or blocked in tests).
    pub trashed: bool,
    pub linked: bool,
}

/// Checks a plan. `size` is the size of the folder, `has_links` whether it
/// contains links, `free` the free space at the target (if known).
pub fn check(plan: &Plan, size: u64, has_links: bool, free: Option<u64>) -> Vec<Problem> {
    let mut v = Vec::new();
    if crate::trash::protection(&plan.source).is_some() {
        v.push(Problem::Protected);
    }
    if !fs::symlink_metadata(&plan.source).is_ok_and(|m| m.is_dir()) {
        v.push(Problem::NotAFolder);
    }
    if !plan.target_dir.is_dir() {
        v.push(Problem::TargetMissing);
    }
    let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if real(&plan.target_dir).starts_with(real(&plan.source)) {
        v.push(Problem::TargetInsideSource);
    }
    if fs::symlink_metadata(plan.target()).is_ok() {
        v.push(Problem::TargetExists);
    }
    if crate::cloud::roots().iter().any(|r| plan.source.starts_with(&r.path)) {
        v.push(Problem::InCloud);
    }
    if cfg!(windows) && has_links {
        v.push(Problem::HasLinks);
    }
    if cfg!(windows) && plan.link && plan.target_dir.to_string_lossy().starts_with(r"\\") {
        v.push(Problem::LinkToNetwork);
    }
    // Some room to spare for the file system's own data.
    let needed = size + size / 50;
    if let Some(free) = free
        && free < needed
    {
        v.push(Problem::NoSpace(needed, free));
    }
    if same_disk(&plan.source, &plan.target_dir) {
        v.push(Problem::SameDisk);
    }
    v
}

fn same_disk(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!((fs::metadata(a), fs::metadata(b)), (Ok(x), Ok(y)) if x.dev() == y.dev())
    }
    #[cfg(windows)]
    {
        let root = |p: &Path| p.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        root(a).is_some() && root(a) == root(b)
    }
}

/// Size of all files below a folder (for the progress bar).
fn total_size(dir: &Path) -> u64 {
    let mut sum = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => sum += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    sum
}

fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// Copies `src` to the new folder `dst`; links are copied as links (Unix).
fn copy_tree(src: &Path, dst: &Path, p: &Progress, files: &mut u64) -> io::Result<()> {
    fs::create_dir(dst)?;
    for e in fs::read_dir(src)? {
        if p.cancelled() {
            return Err(interrupted());
        }
        let e = e?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        let t = e.file_type()?;
        if t.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
            #[cfg(windows)]
            return Err(io::Error::other("link inside the folder"));
        } else if t.is_dir() {
            copy_tree(&from, &to, p, files)?;
        } else if t.is_file() {
            let n = fs::copy(&from, &to)?;
            p.copied.fetch_add(n, Ordering::Relaxed);
            *files += 1;
        } else {
            return Err(io::Error::other("special file inside the folder"));
        }
    }
    Ok(())
}

/// Checks that every file of `src` exists in `dst` with the same size.
fn verify(src: &Path, dst: &Path) -> io::Result<()> {
    for e in fs::read_dir(src)? {
        let e = e?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        let t = e.file_type()?;
        let other = fs::symlink_metadata(&to)?;
        if t.is_dir() {
            if !other.is_dir() {
                return Err(io::Error::other("copy differs (folder)"));
            }
            verify(&from, &to)?;
        } else if t.is_file() && (!other.is_file() || other.len() != e.metadata()?.len()) {
            return Err(io::Error::other(format!("copy differs: {}", e.file_name().to_string_lossy())));
        }
    }
    Ok(())
}

/// Runs a checked plan.
pub fn run(plan: &Plan, p: &Progress) -> io::Result<Report> {
    let target = plan.target();
    if fs::symlink_metadata(&target).is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
    }
    p.total.store(total_size(&plan.source), Ordering::Relaxed);
    let mut files = 0;
    let copied = copy_tree(&plan.source, &target, p, &mut files).and_then(|()| verify(&plan.source, &target));
    if let Err(e) = copied {
        // The original is untouched; the incomplete copy goes away again.
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    let bytes = p.copied.load(Ordering::Relaxed);
    let trashed = match plan.original {
        Original::Trash => match crate::trash::move_to_trash(&plan.source)? {
            crate::external::Outcome::Done => true,
            // Tests: the recycle bin is never used, so nothing more happens.
            crate::external::Outcome::Blocked => {
                return Ok(Report { files, bytes, trashed: false, linked: false });
            }
        },
        Original::Delete => {
            fs::remove_dir_all(&plan.source)?;
            false
        }
    };
    let linked = plan.link && make_link(&target, &plan.source).is_ok();
    Ok(Report { files, bytes, trashed, linked })
}

/// A link at `at` that points to `to`: a symbolic link on Unix, a junction
/// on Windows (needs no admin rights).
pub fn make_link(to: &Path, at: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(to, at)
    }
    #[cfg(windows)]
    {
        junction(to, at)
    }
}

#[cfg(windows)]
fn junction(to: &Path, at: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

    let to = fs::canonicalize(to)?;
    // canonicalize gives \\?\C:\…; the junction wants \??\C:\… and C:\… as shown name.
    let plain = to.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    if plain.starts_with(r"\\") || plain.starts_with("UNC") {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let subst: Vec<u16> = format!(r"\??\{plain}").encode_utf16().collect();
    let print: Vec<u16> = plain.encode_utf16().collect();
    let mut path_buf: Vec<u16> = Vec::new();
    path_buf.extend(&subst);
    path_buf.push(0);
    path_buf.extend(&print);
    path_buf.push(0);
    let data_len = 8 + path_buf.len() * 2;
    let mut buf: Vec<u8> = Vec::with_capacity(8 + data_len);
    buf.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend((data_len as u16).to_le_bytes());
    buf.extend(0u16.to_le_bytes());
    buf.extend(0u16.to_le_bytes());
    buf.extend(((subst.len() * 2) as u16).to_le_bytes());
    buf.extend((((subst.len() + 1) * 2) as u16).to_le_bytes());
    buf.extend(((print.len() * 2) as u16).to_le_bytes());
    for w in path_buf {
        buf.extend(w.to_le_bytes());
    }

    fs::create_dir(at)?;
    let name: Vec<u16> = at.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `name` ends with a zero; the other arguments are plain flags.
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        let e = io::Error::last_os_error();
        let _ = fs::remove_dir(at);
        return Err(e);
    }
    let mut returned = 0u32;
    // SAFETY: `h` is open; the input buffer length is passed along; no output buffer.
    let ok = unsafe {
        DeviceIoControl(
            h,
            FSCTL_SET_REPARSE_POINT,
            buf.as_ptr().cast(),
            buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    let e = io::Error::last_os_error();
    // SAFETY: `h` came from CreateFileW above.
    unsafe { CloseHandle(h) };
    if ok == 0 {
        let _ = fs::remove_dir(at);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        // Not in the system temp folder: on macOS it lies below the protected /private/var.
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-tmp")
            .join(format!("move-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn moves_checks_and_links() {
        let base = temp("ok");
        let src = base.join("Spiele");
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("eins.dat"), vec![1u8; 5000]).unwrap();
        fs::write(src.join("a/b/zwei.dat"), vec![2u8; 300]).unwrap();
        let dst = base.join("andere Platte");
        fs::create_dir(&dst).unwrap();
        let plan =
            Plan { source: src.clone(), target_dir: dst.clone(), link: true, original: Original::Delete };
        let problems = check(&plan, 5300, false, Some(1 << 30));
        assert!(problems.iter().all(|p| !p.blocks()), "{problems:?}");

        let p = Progress::default();
        let r = run(&plan, &p).unwrap();
        assert_eq!((r.files, r.bytes, r.trashed), (2, 5300, false));
        assert_eq!(fs::read(dst.join("Spiele/a/b/zwei.dat")).unwrap(), vec![2u8; 300]);
        // The old place is a link now and still shows the files.
        assert!(r.linked);
        assert!(fs::symlink_metadata(&src).unwrap().file_type().is_symlink() || cfg!(windows));
        assert_eq!(fs::read(src.join("eins.dat")).unwrap().len(), 5000);
        // Moving again: the target exists.
        assert!(check(&plan, 0, false, None).contains(&Problem::TargetExists));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn refuses_bad_plans() {
        let base = temp("bad");
        let src = base.join("x");
        fs::create_dir_all(src.join("inner")).unwrap();
        let inside = Plan {
            source: src.clone(),
            target_dir: src.join("inner"),
            link: false,
            original: Original::Trash,
        };
        assert!(check(&inside, 10, false, None).contains(&Problem::TargetInsideSource));
        let full =
            Plan { source: src.clone(), target_dir: base.clone(), link: false, original: Original::Trash };
        assert!(check(&full, 1000, false, Some(10)).iter().any(|p| matches!(p, Problem::NoSpace(..))));
        let missing = Plan {
            source: src.clone(),
            target_dir: base.join("gibt es nicht"),
            link: false,
            original: Original::Trash,
        };
        assert!(check(&missing, 1, false, None).contains(&Problem::TargetMissing));
        let root = Plan {
            source: PathBuf::from("/"),
            target_dir: base.clone(),
            link: false,
            original: Original::Trash,
        };
        assert!(check(&root, 1, false, None).contains(&Problem::Protected));
        assert!(Problem::NoSpace(1, 0).blocks() && !Problem::SameDisk.blocks());

        // Trash in tests: copied and checked, but the original stays (recycle bin blocked).
        fs::write(src.join("f"), b"abc").unwrap();
        let dst = base.join("ziel");
        fs::create_dir(&dst).unwrap();
        let plan =
            Plan { source: src.clone(), target_dir: dst.clone(), link: true, original: Original::Trash };
        let r = run(&plan, &Progress::default()).unwrap();
        assert!(!r.trashed && !r.linked);
        assert!(src.join("f").exists() && dst.join("x/f").exists());

        // Cancelled: the copy is removed again, the original stays.
        fs::remove_dir_all(dst.join("x")).unwrap();
        let p = Progress::default();
        p.cancel();
        assert!(run(&plan, &p).is_err());
        assert!(!dst.join("x").exists() && src.join("f").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
