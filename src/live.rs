//! Live update: after a scan the program watches the folder and corrects
//! the numbers when files appear, change or disappear – without a new scan.
//!
//! The system reports changed paths. For each one the nearest folder that is
//! known in the tree is read again (only that folder, not below it) in a
//! background thread; new subfolders are scanned on their own and put into
//! the tree. Nothing on the disk is ever changed.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecursiveMode, Watcher};

use crate::platform::{self, Entry};
use crate::scan::{self, Progress};
use crate::tree::{Kind, NO_PARENT, Node, NodeId, SortKey, Tree, flags};

/// On Linux every folder needs a watch of its own; larger trees are not watched.
pub const LINUX_MAX_FOLDERS: u64 = 100_000;

/// Waiting time after the last report before folders are read again.
const QUIET: Duration = Duration::from_millis(300);
/// Longest waiting time while reports keep coming.
const MAX_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Starting,
    Watching,
    Failed(String),
}

enum Msg {
    Path(PathBuf),
    /// The system lost reports (too many at once).
    Lost,
}

enum Job {
    List(PathBuf),
    Scan(PathBuf),
}

enum Done {
    Started(Result<(), String>),
    Listed(PathBuf, Vec<Entry>),
    Scanned(PathBuf, Tree),
    Failed,
}

pub struct Live {
    raw: Receiver<Msg>,
    jobs: Sender<Job>,
    done: Receiver<Done>,
    cancel: Arc<Progress>,
    pending: HashSet<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
    in_flight: usize,
    skip: Vec<PathBuf>,
    pub status: Status,
    /// Reports were lost; only a new scan gives exact numbers again.
    pub lost: bool,
    /// Number of corrections so far.
    pub updates: u64,
}

/// True if the tree can be watched on this system.
pub fn supported(tree: &Tree) -> bool {
    !(cfg!(target_os = "linux") && tree.root().dirs > LINUX_MAX_FOLDERS)
}

impl Live {
    /// Starts watching `root`. `wake` is called when something arrived
    /// (it redraws the window).
    pub fn start(root: &Path, wake: impl Fn() + Send + Sync + 'static) -> Live {
        let wake = Arc::new(wake);
        let (raw_tx, raw) = channel();
        let (jobs, job_rx) = channel::<Job>();
        let (done_tx, done) = channel();
        let cancel = Arc::new(Progress::default());
        let root = root.to_path_buf();
        // The system may report the real path (for example /private/var for /var).
        let real = std::fs::canonicalize(&root).ok().filter(|r| *r != root);
        let progress = Arc::clone(&cancel);
        let w = Arc::clone(&wake);
        let spawned = std::thread::Builder::new().name("live".into()).spawn(move || {
            let watch_root = root.clone();
            let w2 = Arc::clone(&w);
            let handler = move |res: notify::Result<notify::Event>| {
                let msgs: Vec<Msg> = match res {
                    Ok(ev) if ev.need_rescan() => vec![Msg::Lost],
                    // Reading a file changes nothing.
                    Ok(ev) if matches!(ev.kind, EventKind::Access(a) if a != AccessKind::Close(AccessMode::Write)) => {
                        Vec::new()
                    }
                    Ok(ev) => ev
                        .paths
                        .into_iter()
                        .map(|p| match &real {
                            Some(real) if !p.starts_with(&watch_root) => match p.strip_prefix(real) {
                                Ok(rest) => watch_root.join(rest),
                                Err(_) => p,
                            },
                            _ => p,
                        })
                        .map(Msg::Path)
                        .collect(),
                    Err(_) => vec![Msg::Lost],
                };
                if !msgs.is_empty() {
                    for m in msgs {
                        let _ = raw_tx.send(m);
                    }
                    w2();
                }
            };
            let mut watcher = match notify::recommended_watcher(handler) {
                Ok(w) => w,
                Err(e) => {
                    let _ = done_tx.send(Done::Started(Err(e.to_string())));
                    w();
                    return;
                }
            };
            if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
                let _ = done_tx.send(Done::Started(Err(e.to_string())));
                w();
                return;
            }
            let _ = done_tx.send(Done::Started(Ok(())));
            w();
            for job in job_rx {
                let done = match job {
                    Job::List(dir) => match platform::list(&dir) {
                        // A folder that cannot be read completely is left as it is.
                        (entries, None) => Done::Listed(dir, entries),
                        (_, Some(_)) => Done::Failed,
                    },
                    Job::Scan(dir) => match scan::scan(&dir, &scan::Options::default(), &progress) {
                        Ok(t) if !t.cancelled => Done::Scanned(dir, t),
                        _ => Done::Failed,
                    },
                };
                if done_tx.send(done).is_err() {
                    break;
                }
                w();
            }
            drop(watcher);
        });
        let status = match spawned {
            Ok(_) => Status::Starting,
            Err(e) => Status::Failed(e.to_string()),
        };
        Live {
            raw,
            jobs,
            done,
            cancel,
            pending: HashSet::new(),
            first: None,
            last: None,
            in_flight: 0,
            skip: scan::Options::default().skip,
            status,
            lost: false,
            updates: 0,
        }
    }

    /// True while reports wait or folders are being read.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || self.in_flight > 0
    }

    /// Takes over what arrived; returns true if the tree changed.
    pub fn poll(&mut self, tree: &mut Tree, sort: (SortKey, bool)) -> bool {
        let now = Instant::now();
        while let Ok(m) = self.raw.try_recv() {
            match m {
                Msg::Path(p) => {
                    self.pending.insert(p);
                    self.first.get_or_insert(now);
                    self.last = Some(now);
                }
                Msg::Lost => self.lost = true,
            }
        }
        let mut changed = false;
        while let Ok(d) = self.done.try_recv() {
            match d {
                Done::Started(Ok(())) => self.status = Status::Watching,
                Done::Started(Err(e)) => self.status = Status::Failed(e),
                Done::Listed(dir, entries) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    if let Some(id) = known_folder(tree, &dir) {
                        let (c, new_dirs) = apply_listing(tree, id, entries, &self.skip);
                        if c {
                            sort_up(tree, id, sort);
                            changed = true;
                        }
                        for nd in new_dirs {
                            if self.jobs.send(Job::Scan(tree.path(nd))).is_ok() {
                                self.in_flight += 1;
                            }
                        }
                    }
                }
                Done::Scanned(dir, sub) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    if let Some(id) = tree.find(&dir)
                        && tree.node(id).is_dir()
                        && tree.node(id).child_count == 0
                    {
                        tree.graft(id, sub);
                        sort_below(tree, id, sort);
                        changed = true;
                    }
                }
                Done::Failed => self.in_flight = self.in_flight.saturating_sub(1),
            }
        }
        let quiet = self.last.is_some_and(|l| now - l >= QUIET);
        let waited = self.first.is_some_and(|f| now - f >= MAX_WAIT);
        if !self.pending.is_empty() && self.in_flight == 0 && (quiet || waited) {
            let dirs: BTreeSet<PathBuf> = self.pending.drain().filter_map(|p| target(tree, &p)).collect();
            for d in dirs {
                if self.jobs.send(Job::List(d)).is_ok() {
                    self.in_flight += 1;
                }
            }
            self.first = None;
            self.last = None;
        }
        if changed {
            self.updates += 1;
        }
        changed
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // Stops a running scan of a new folder; the thread ends with the job list.
        self.cancel.cancel();
    }
}

/// The node of a folder that can be read again (known, scanned, not skipped).
fn known_folder(tree: &Tree, dir: &Path) -> Option<NodeId> {
    let id = tree.find(dir)?;
    let n = tree.node(id);
    (n.is_dir() && !n.has(flags::SKIPPED | flags::NOT_SCANNED | flags::REMOVED)).then_some(id)
}

/// The folder to read again for a reported path: the nearest folder above
/// (or the path itself) that is in the tree and still exists.
fn target(tree: &Tree, path: &Path) -> Option<PathBuf> {
    let mut cur = path;
    loop {
        if !cur.starts_with(&tree.root_path) {
            return None;
        }
        if let Some(id) = tree.find(cur) {
            let n = tree.node(id);
            // Changes inside skipped folders are not shown.
            if n.is_dir() && n.has(flags::SKIPPED | flags::NOT_SCANNED) {
                return None;
            }
            if n.is_dir() && cur.symlink_metadata().is_ok_and(|m| m.is_dir()) {
                return Some(cur.to_path_buf());
            }
        }
        if cur == tree.root_path {
            return None;
        }
        cur = cur.parent()?;
    }
}

/// Brings the children of folder `dir` in line with a fresh listing.
/// Returns whether something changed and the new subfolders to scan.
pub fn apply_listing(
    tree: &mut Tree,
    dir: NodeId,
    entries: Vec<Entry>,
    skip: &[PathBuf],
) -> (bool, Vec<NodeId>) {
    let kids: HashMap<Box<OsStr>, NodeId> =
        tree.children(dir).iter().map(|&c| (tree.node(c).name.clone(), c)).collect();
    let listed: HashSet<&OsStr> = entries.iter().map(|e| &*e.name).collect();
    let mut changed = false;
    for (name, &id) in &kids {
        if !listed.contains(&**name) {
            tree.remove(id);
            changed = true;
        }
    }
    let base = tree.path(dir);
    let mut new_dirs = Vec::new();
    for e in entries {
        match kids.get(&e.name) {
            Some(&id) if tree.node(id).kind == e.kind => {
                let n = tree.node(id);
                if e.kind != Kind::Dir
                    && (n.size, n.allocated, n.modified) != (e.size, e.allocated, e.modified)
                {
                    tree.set_file_values(id, e.size, e.allocated, e.modified);
                    changed = true;
                }
                if tree.node(id).has(flags::ONLINE_ONLY) != e.online_only {
                    tree.set_flag(id, flags::ONLINE_ONLY, e.online_only);
                    changed = true;
                }
            }
            old => {
                if let Some(&id) = old {
                    tree.remove(id);
                }
                let path = base.join(&*e.name);
                let mut n = Node::new(e.name, NO_PARENT, e.kind);
                n.size = e.size;
                n.allocated = e.allocated;
                n.modified = e.modified;
                if e.online_only {
                    n.flags |= flags::ONLINE_ONLY;
                }
                let scan_it = e.kind == Kind::Dir && !platform::is_skipped(&path, skip);
                if e.kind == Kind::Dir {
                    n.flags |= if scan_it { flags::NOT_SCANNED } else { flags::SKIPPED };
                }
                let id = tree.add_child(dir, n);
                if scan_it {
                    new_dirs.push(id);
                }
                changed = true;
            }
        }
    }
    (changed, new_dirs)
}

fn sort_below(tree: &mut Tree, id: NodeId, sort: (SortKey, bool)) {
    let mut stack = vec![id];
    while let Some(d) = stack.pop() {
        if tree.node(d).child_count > 1 {
            tree.sort_children(d, sort.0, sort.1);
        }
        stack.extend(tree.children(d).iter().copied().filter(|&c| tree.node(c).is_dir()));
    }
    sort_up(tree, id, sort);
}

/// Sorts the folders above again: their sizes changed.
fn sort_up(tree: &mut Tree, id: NodeId, sort: (SortKey, bool)) {
    for a in tree.ancestors(id) {
        tree.sort_children(a, sort.0, sort.1);
    }
}
