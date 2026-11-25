//! The history: after each scan a short summary (names and sizes of the
//! larger folders and files, no file contents) is kept on this computer,
//! so later scans can be compared and the growth can be forecast.
//!
//! One file per scan: a fixed header with the totals (so the list of scans
//! is read quickly), then a reduced copy of the tree in the checked
//! snapshot format.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use crate::disk::Space;
use crate::snapshot;
use crate::tree::{NodeId, ROOT, Tree};

const MAGIC: &[u8; 8] = b"MRSZHIS1";
/// Magic, eight numbers, one flag.
const HEADER: usize = 8 + 8 * 8 + 1;
const EXTENSION: &str = "mszh";
/// Scans kept per folder; older ones are deleted.
pub const MAX_ENTRIES: usize = 100;
/// A new scan within this time replaces the last one (seconds).
const REPLACE_WITHIN: i64 = 10 * 60;
const MAX_FILE: u64 = 256 << 20;

/// The header of a saved scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub file: PathBuf,
    /// Seconds since 1970 (UTC).
    pub time: i64,
    pub size: u64,
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
    /// Entries below this size were left out.
    pub min_size: u64,
    /// Size and free space of the disk at that time.
    pub space: Option<Space>,
}

pub struct Store {
    dir: PathBuf,
}

/// The smallest entry kept: 1 MiB, more for very large scans.
pub fn default_min_size(tree: &Tree) -> u64 {
    (1u64 << 20).max(tree.root().size / 50_000)
}

/// A stable short name for a folder path (FNV-1a).
fn folder_key(root: &Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in snapshot::os_bytes(root.as_os_str()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged history file: {what}"))
}

impl Store {
    /// The history in the configuration folder; `None` in tests (nothing is written there).
    pub fn default_location() -> Option<Store> {
        if crate::external::is_blocked() {
            return None;
        }
        crate::settings::config_dir().map(|d| Store { dir: d.join("history") })
    }

    /// A history in a folder of one's own (tests).
    pub fn at(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn folder(&self, root: &Path) -> PathBuf {
        self.dir.join(folder_key(root))
    }

    /// Saves a finished scan. Cancelled scans are not saved.
    pub fn save(&self, tree: &Tree, time: i64, space: Option<Space>) -> io::Result<Option<Summary>> {
        self.save_with(tree, time, space, default_min_size(tree))
    }

    pub fn save_with(
        &self,
        tree: &Tree,
        time: i64,
        space: Option<Space>,
        min_size: u64,
    ) -> io::Result<Option<Summary>> {
        if tree.cancelled {
            return Ok(None);
        }
        let folder = self.folder(&tree.root_path);
        std::fs::create_dir_all(&folder)?;
        let old = self.list(&tree.root_path);
        // Several scans in a row count as one.
        if let Some(last) = old.last()
            && (time - last.time).abs() < REPLACE_WITHIN
        {
            let _ = std::fs::remove_file(&last.file);
        }
        let small = tree.copy(min_size, 8, 30_000, false);
        let r = tree.root();
        let mut data = Vec::with_capacity(HEADER + small.len() * 60);
        data.extend_from_slice(MAGIC);
        let (total, free) = space.map_or((0, 0), |s| (s.total, s.free));
        for v in [time as u64, r.size, r.allocated, r.files, r.dirs, min_size, total, free] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.push(u8::from(space.is_some()));
        snapshot::write(&small, &mut data)?;
        let file = folder.join(format!("{time}.{EXTENSION}"));
        let temp = folder.join(format!("{time}.tmp"));
        std::fs::write(&temp, &data)?;
        std::fs::rename(&temp, &file)?;
        // Keep the newest entries only.
        let all = self.list(&tree.root_path);
        if all.len() > MAX_ENTRIES {
            for s in &all[..all.len() - MAX_ENTRIES] {
                let _ = std::fs::remove_file(&s.file);
            }
        }
        Ok(parse_header(&data, file).ok())
    }

    /// The saved scans of a folder, oldest first. Damaged files are skipped.
    pub fn list(&self, root: &Path) -> Vec<Summary> {
        let Ok(rd) = std::fs::read_dir(self.folder(root)) else { return Vec::new() };
        let mut v: Vec<Summary> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension() == Some(OsStr::new(EXTENSION)))
            .filter_map(|p| {
                use std::io::Read;
                let mut head = [0u8; HEADER];
                std::fs::File::open(&p).and_then(|mut f| f.read_exact(&mut head)).ok()?;
                parse_header(&head, p).ok()
            })
            .collect();
        v.sort_by_key(|s| s.time);
        v
    }

    /// Reads the reduced tree of a saved scan.
    pub fn load(&self, summary: &Summary, root: &Path) -> io::Result<Tree> {
        let len = std::fs::metadata(&summary.file)?.len();
        if len > MAX_FILE {
            return Err(bad("too large"));
        }
        let data = std::fs::read(&summary.file)?;
        parse_header(&data, summary.file.clone())?;
        let tree = snapshot::from_bytes(&data[HEADER..])?;
        if tree.root_path != root {
            return Err(bad("other folder"));
        }
        Ok(tree)
    }

    /// Deletes the whole history.
    pub fn clear(&self) -> io::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}

fn parse_header(data: &[u8], file: PathBuf) -> io::Result<Summary> {
    if data.len() < HEADER || &data[..8] != MAGIC {
        return Err(bad("header"));
    }
    let n = |i: usize| {
        let at = 8 + i * 8;
        u64::from_le_bytes(data[at..at + 8].try_into().expect("8 bytes"))
    };
    let space = match data[HEADER - 1] {
        0 => None,
        1 => Some(Space { total: n(6), free: n(7) }),
        _ => return Err(bad("flag")),
    };
    Ok(Summary {
        file,
        time: n(0) as i64,
        size: n(1),
        allocated: n(2),
        files: n(3),
        dirs: n(4),
        min_size: n(5),
        space,
    })
}

/// A folder or file whose size changed between two scans.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Path below the scanned folder.
    pub path: PathBuf,
    pub is_dir: bool,
    /// `None`: not in the older scan (new, or too small to be kept).
    pub old: Option<u64>,
    /// `None`: not in the newer scan (deleted, or now too small).
    pub new: Option<u64>,
}

impl Change {
    pub fn delta(&self) -> i64 {
        self.new.unwrap_or(0) as i64 - self.old.unwrap_or(0) as i64
    }
}

/// The changes from `old` to `new`, largest first. A folder is left out if
/// one of its entries explains (almost) all of its change: the list then
/// points straight to where the space went.
pub fn compare(old: &Tree, new: &Tree, min_change: u64, limit: usize) -> Vec<Change> {
    let mut out = Vec::new();
    visit(old, Some(ROOT), new, Some(ROOT), PathBuf::new(), min_change, &mut out);
    out.sort_by_key(|c| std::cmp::Reverse(c.delta().unsigned_abs()));
    out.truncate(limit);
    out
}

/// Compares the children of one folder; returns the largest change found below.
fn visit(
    old: &Tree,
    o: Option<NodeId>,
    new: &Tree,
    n: Option<NodeId>,
    path: PathBuf,
    min_change: u64,
    out: &mut Vec<Change>,
) {
    let (Some(o), Some(n)) = (o, n) else { return };
    let old_kids: HashMap<&OsStr, NodeId> =
        old.children(o).iter().map(|&c| (&*old.node(c).name, c)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut pairs: Vec<(Option<NodeId>, Option<NodeId>, &OsStr)> = Vec::new();
    for &c in new.children(n) {
        let name = &*new.node(c).name;
        seen.insert(name);
        pairs.push((old_kids.get(name).copied(), Some(c), name));
    }
    for &c in old.children(o) {
        let name = &*old.node(c).name;
        if !seen.contains(name) {
            pairs.push((Some(c), None, name));
        }
    }
    for (oc, nc, name) in pairs {
        let before = oc.map(|c| old.node(c).size);
        let after = nc.map(|c| new.node(c).size);
        let change = Change {
            path: path.join(name),
            is_dir: match nc {
                Some(c) => new.node(c).is_dir(),
                None => oc.is_some_and(|c| old.node(c).is_dir()),
            },
            old: before,
            new: after,
        };
        let delta = change.delta();
        if delta.unsigned_abs() < min_change {
            continue;
        }
        let mark = out.len();
        let both_dirs = oc.is_some_and(|c| old.node(c).is_dir()) && nc.is_some_and(|c| new.node(c).is_dir());
        if both_dirs {
            visit(old, oc, new, nc, change.path.clone(), min_change, out);
        }
        // Left out when a single entry inside explains 90 % of the change.
        let explained = out[mark..].iter().any(|c| {
            c.path.parent() == Some(change.path.as_path())
                && c.delta().signum() == delta.signum()
                && c.delta().unsigned_abs() * 10 >= delta.unsigned_abs() * 9
        });
        if !explained {
            out.push(change);
        }
    }
}

/// Change per day of a value, from a straight line through the points
/// (time in seconds). Needs points that span at least one day.
pub fn trend(points: &[(i64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let (first, last) = (points.iter().map(|p| p.0).min()?, points.iter().map(|p| p.0).max()?);
    if last - first < 86_400 {
        return None;
    }
    let n = points.len() as f64;
    let days: Vec<f64> = points.iter().map(|p| (p.0 - first) as f64 / 86_400.0).collect();
    let mx = days.iter().sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (x, p) in days.iter().zip(points) {
        sxy += (x - mx) * (p.1 - my);
        sxx += (x - mx) * (x - mx);
    }
    (sxx > 0.0).then(|| sxy / sxx)
}

/// What the free space of the disk will do at the current pace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outlook {
    /// Not enough scans on different days.
    Unknown,
    /// Free space stays the same or grows.
    NotFilling,
    /// Full in about this many days.
    FullIn(f64),
}

/// Forecast from (time, free bytes) points.
pub fn outlook(points: &[(i64, u64)]) -> Outlook {
    let pts: Vec<(i64, f64)> = points.iter().map(|&(t, f)| (t, f as f64)).collect();
    let Some(per_day) = trend(&pts) else { return Outlook::Unknown };
    let Some(&(_, free)) = points.iter().max_by_key(|p| p.0) else { return Outlook::Unknown };
    // Less than 1 MB per day counts as standing still; more than 100 years too.
    if per_day > -1e6 {
        return Outlook::NotFilling;
    }
    let days = free as f64 / -per_day;
    if days > 36_500.0 { Outlook::NotFilling } else { Outlook::FullIn(days) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, NO_PARENT, Node};

    fn dir(n: &str) -> Node {
        Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir)
    }
    fn file(n: &str, size: u64) -> Node {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = size;
        f
    }

    /// /data/{Downloads/{iso 100, x 5}, Fotos/{a 50}, alt 30}
    fn sample(iso: u64, with_old: bool) -> Tree {
        let mut t = Tree::new(PathBuf::from("/data"), Kind::Dir);
        let mut kids = vec![dir("Downloads"), dir("Fotos")];
        if with_old {
            kids.push(file("alt", 30));
        }
        let first = t.add_children(ROOT, kids);
        t.add_children(first, [file("iso", iso), file("x", 5)]);
        t.add_children(first + 1, [file("a", 50)]);
        t.sum_up();
        t
    }

    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "mrsizeme-history-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn save_list_load_and_clear() {
        let dir = temp();
        let store = Store::at(dir.clone());
        let space = Some(Space { total: 1000, free: 400 });
        store.save_with(&sample(100, true), 1_000_000, space, 1).unwrap();
        // Within ten minutes: replaces the one before.
        store.save_with(&sample(110, true), 1_000_100, space, 1).unwrap();
        store.save_with(&sample(200, false), 2_000_000, None, 1).unwrap();
        let mut cancelled = sample(1, true);
        cancelled.cancelled = true;
        assert!(store.save_with(&cancelled, 3_000_000, None, 1).unwrap().is_none());

        let list = store.list(Path::new("/data"));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].time, 1_000_100);
        assert_eq!(list[0].size, 195);
        assert_eq!(list[0].space, space);
        assert_eq!(list[1].space, None);
        assert!(store.list(Path::new("/other")).is_empty());

        let old = store.load(&list[0], Path::new("/data")).unwrap();
        assert!(old.find(Path::new("/data/Downloads/iso")).is_some());
        assert!(store.load(&list[0], Path::new("/other")).is_err());

        // A damaged file is skipped or refused, never a crash.
        std::fs::write(&list[1].file, b"MRSZHIS1 broken").unwrap();
        assert_eq!(store.list(Path::new("/data")).len(), 1);

        store.clear().unwrap();
        assert!(!dir.exists());
        store.clear().unwrap();
    }

    #[test]
    fn only_the_newest_are_kept() {
        let dir = temp();
        let store = Store::at(dir.clone());
        for i in 0..MAX_ENTRIES as i64 + 3 {
            store.save_with(&sample(100, true), i * 3600, None, 1).unwrap();
        }
        let list = store.list(Path::new("/data"));
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0].time, 3 * 3600);
        store.clear().unwrap();
    }

    #[test]
    fn compare_points_to_the_cause() {
        let old = sample(100, true);
        let new = sample(1100, false);
        let changes = compare(&old, &new, 10, 50);
        // "Downloads" grew only because of "iso", so "iso" is named, not the folder.
        assert_eq!(changes[0].path, PathBuf::from("Downloads/iso"));
        assert_eq!(changes[0].delta(), 1000);
        assert!(!changes.iter().any(|c| c.path == Path::new("Downloads")));
        let gone = changes.iter().find(|c| c.path == Path::new("alt")).unwrap();
        assert_eq!((gone.old, gone.new, gone.delta()), (Some(30), None, -30));
        // Small changes are left out.
        assert!(compare(&old, &old, 1, 50).is_empty());
    }

    #[test]
    fn trend_and_outlook() {
        let day = 86_400;
        assert_eq!(trend(&[(0, 1.0)]), None);
        assert_eq!(trend(&[(0, 1.0), (3600, 2.0)]), None);
        let t = trend(&[(0, 100.0), (day, 110.0), (2 * day, 120.0)]).unwrap();
        assert!((t - 10.0).abs() < 1e-9);

        let gb = 1_000_000_000u64;
        // 1 GB less per day, 30 GB left: full in 30 days.
        match outlook(&[(0, 32 * gb), (day, 31 * gb), (2 * day, 30 * gb)]) {
            Outlook::FullIn(d) => assert!((d - 30.0).abs() < 0.01),
            o => panic!("{o:?}"),
        }
        assert_eq!(outlook(&[(0, gb), (2 * day, 2 * gb)]), Outlook::NotFilling);
        assert_eq!(outlook(&[(0, gb)]), Outlook::Unknown);
    }
}
