//! Reads a folder tree with several threads.
//!
//! Worker threads only list folders and read the metadata of the entries.
//! The calling thread collects their results and builds the [`Tree`]; it
//! also hands out new folders to the workers. Only metadata is read, never
//! the content of a file.

use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::platform::{self, Sizer};
use crate::tree::{Kind, Node, NodeId, ROOT, SortKey, Tree, flags};

#[derive(Debug, Clone)]
pub struct Options {
    /// Number of worker threads.
    pub threads: usize,
    /// Do not enter folders on another file system (Linux: /proc, /sys, other disks).
    pub stay_on_filesystem: bool,
    /// Folders that are never entered.
    pub skip: Vec<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        let cpus = thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            threads: cpus.clamp(2, 8),
            stay_on_filesystem: platform::default_stay_on_filesystem(),
            skip: platform::default_skip_list(),
        }
    }
}

/// Counters a running scan updates; can be read from another thread.
#[derive(Debug, Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    cancel: AtomicBool,
    current: Mutex<PathBuf>,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The folder a worker read last.
    pub fn current(&self) -> PathBuf {
        self.current.lock().map(|p| p.clone()).unwrap_or_default()
    }

    fn set_current(&self, p: &Path) {
        // Never wait for the lock: the shown path is only a hint.
        if let Ok(mut cur) = self.current.try_lock() {
            cur.clear();
            cur.push(p);
        }
    }
}

struct Job {
    node: NodeId,
    path: PathBuf,
}

struct Entry {
    name: Box<OsStr>,
    kind: Kind,
    size: u64,
    allocated: u64,
    modified: i64,
    link_id: Option<(u64, u64)>,
    device: Option<u64>,
}

struct Listing {
    job: Job,
    entries: Vec<Entry>,
    error: Option<io::Error>,
}

/// Folders waiting to be read. `pop` blocks until there is work or the queue is closed.
#[derive(Default)]
struct Queue {
    state: Mutex<(VecDeque<Job>, bool)>,
    ready: Condvar,
}

impl Queue {
    fn push_all(&self, jobs: impl IntoIterator<Item = Job>) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.0.extend(jobs);
        self.ready.notify_all();
    }

    fn pop(&self) -> Option<Job> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if s.1 {
                return None;
            }
            if let Some(job) = s.0.pop_front() {
                return Some(job);
            }
            s = self.ready.wait(s).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn close(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.1 = true;
        s.0.clear();
        self.ready.notify_all();
    }
}

/// Scans `root` and everything below it. Symbolic links and junctions are
/// never followed. Folders that cannot be read are noted in [`Tree::errors`].
/// After [`Progress::cancel`] the partial tree is returned with `cancelled` set.
pub fn scan(root: &Path, opts: &Options, progress: &Progress) -> io::Result<Tree> {
    let root = std::path::absolute(root)?;
    let md = fs::metadata(&root)?;
    let sizer = Sizer::new(&root);
    let kind = if md.is_dir() { Kind::Dir } else { Kind::File };
    let mut tree = Tree::new(root.clone(), kind);
    {
        let r = &mut tree.nodes[ROOT as usize];
        r.modified = platform::modified(&md);
        r.allocated = sizer.allocated(&|| root.clone(), &md);
        if kind == Kind::File {
            r.size = md.len();
            return Ok(tree);
        }
        r.flags |= flags::NOT_SCANNED;
    }
    let root_device = if opts.stay_on_filesystem { platform::device(&md) } else { None };

    let queue = Queue::default();
    queue.push_all([Job { node: ROOT, path: root }]);
    let (tx, rx) = mpsc::channel::<Listing>();
    let mut seen_links = HashSet::new();

    thread::scope(|s| {
        for _ in 0..opts.threads.max(1) {
            let tx = tx.clone();
            let (queue, sizer) = (&queue, &sizer);
            s.spawn(move || {
                while let Some(job) = queue.pop() {
                    let listing = read_folder(job, sizer, progress);
                    if tx.send(listing).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        let mut outstanding = 1usize;
        while outstanding > 0 {
            if progress.is_cancelled() {
                tree.cancelled = true;
                break;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(listing) => {
                    outstanding -= 1;
                    let jobs = collect(&mut tree, listing, &mut seen_links, root_device, opts);
                    outstanding += jobs.len();
                    queue.push_all(jobs);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        queue.close();
    });

    tree.sum_up();
    tree.sort(SortKey::Size, false);
    Ok(tree)
}

fn read_folder(job: Job, sizer: &Sizer, progress: &Progress) -> Listing {
    progress.set_current(&job.path);
    let mut entries = Vec::new();
    let mut error = None;
    if progress.is_cancelled() {
        return Listing { job, entries, error };
    }
    match fs::read_dir(&job.path) {
        Err(e) => error = Some(e),
        Ok(rd) => {
            for ent in rd {
                let ent = match ent {
                    Ok(ent) => ent,
                    Err(e) => {
                        error.get_or_insert(e);
                        continue;
                    }
                };
                // Does not follow links on any system.
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
                if kind == Kind::Dir {
                    progress.dirs.fetch_add(1, Ordering::Relaxed);
                } else {
                    progress.files.fetch_add(1, Ordering::Relaxed);
                    progress.bytes.fetch_add(size, Ordering::Relaxed);
                }
                entries.push(Entry {
                    name: ent.file_name().into_boxed_os_str(),
                    kind,
                    size,
                    allocated,
                    modified: platform::modified(&md),
                    link_id: if kind == Kind::File { platform::link_id(&md) } else { None },
                    device: if kind == Kind::Dir { platform::device(&md) } else { None },
                });
            }
        }
    }
    if error.is_some() {
        progress.errors.fetch_add(1, Ordering::Relaxed);
    }
    Listing { job, entries, error }
}

/// Puts one folder listing into the tree; returns the subfolders still to read.
fn collect(
    tree: &mut Tree,
    listing: Listing,
    seen_links: &mut HashSet<(u64, u64)>,
    root_device: Option<u64>,
    opts: &Options,
) -> Vec<Job> {
    let Listing { job, entries, error } = listing;
    let parent = job.node;
    tree.nodes[parent as usize].flags &= !flags::NOT_SCANNED;
    if let Some(e) = error {
        tree.nodes[parent as usize].flags |= flags::ERROR;
        tree.errors.insert(parent, crate::format::io_error(&e));
    }

    let mut subfolders = Vec::new();
    let nodes: Vec<Node> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let mut n = Node::new(e.name, parent, e.kind);
            n.size = e.size;
            n.allocated = e.allocated;
            n.modified = e.modified;
            if let Some(id) = e.link_id
                && !seen_links.insert(id)
            {
                n.flags |= flags::HARDLINK_DUP;
            }
            if e.kind == Kind::Dir {
                let path = job.path.join(&*n.name);
                let other_fs = root_device.is_some() && e.device.is_some() && e.device != root_device;
                if other_fs || platform::is_skipped(&path, &opts.skip) {
                    n.flags |= flags::SKIPPED;
                } else {
                    n.flags |= flags::NOT_SCANNED;
                    subfolders.push((i, path));
                }
            }
            n
        })
        .collect();
    let first = tree.add_children(parent, nodes);
    subfolders.into_iter().map(|(i, path)| Job { node: first + i as NodeId, path }).collect()
}
