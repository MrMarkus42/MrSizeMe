//! The result of a scan: all folders and files in one flat array.
//!
//! The children of a folder are stored next to each other (the scanner adds
//! all entries of a folder in one go), so a node only needs the position of
//! its first child and the number of children. `order` holds the node ids in
//! display order; sorting a folder only reorders its slice of `order`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Index of a node in [`Tree::nodes`].
pub type NodeId = u32;

/// Marker for "no parent" (only the root node).
pub const NO_PARENT: NodeId = NodeId::MAX;

/// Marker for "time unknown" in [`Node::modified`].
pub const NO_TIME: i64 = i64::MIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    /// Symbolic link or (on Windows) junction; never followed.
    Link,
    /// Device, socket, pipe and the like.
    Other,
}

/// Bit flags in [`Node::flags`].
pub mod flags {
    /// The folder could not be read completely (see [`super::Tree::error`]).
    pub const ERROR: u8 = 1;
    /// A further hard link to a file that was already counted.
    pub const HARDLINK_DUP: u8 = 2;
    /// The folder was not entered (another file system or on the skip list).
    pub const SKIPPED: u8 = 4;
    /// The scan was cancelled before this folder was read.
    pub const NOT_SCANNED: u8 = 8;
    /// Moved to the recycle bin; no longer listed under its parent.
    pub const REMOVED: u8 = 16;
    /// A cloud file that is only online; its content is not on this computer.
    pub const ONLINE_ONLY: u8 = 32;
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: Box<OsStr>,
    pub parent: NodeId,
    pub kind: Kind,
    pub flags: u8,
    /// Logical size in bytes (sum of the file lengths; for folders the whole subtree).
    pub size: u64,
    /// Space used on the disk in bytes (for folders the whole subtree).
    pub allocated: u64,
    /// Number of files in the subtree (without the node itself).
    pub files: u64,
    /// Number of folders in the subtree (without the node itself).
    pub dirs: u64,
    /// Last change in seconds since 1970 (UTC); for folders the newest in the subtree.
    pub modified: i64,
    /// Position of the first child in [`Tree::order`].
    pub first_child: u32,
    pub child_count: u32,
}

impl Node {
    pub fn new(name: Box<OsStr>, parent: NodeId, kind: Kind) -> Self {
        Self {
            name,
            parent,
            kind,
            flags: 0,
            size: 0,
            allocated: 0,
            files: 0,
            dirs: 0,
            modified: NO_TIME,
            first_child: 0,
            child_count: 0,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn name_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.name.to_string_lossy()
    }
}

/// Columns a folder's children can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Allocated,
    Files,
    Modified,
}

/// What the charts measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// Logical size.
    Size,
    /// Space used on the disk.
    Allocated,
    /// Number of files.
    Files,
}

#[derive(Debug, Clone)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub order: Vec<NodeId>,
    /// Full path of the root node.
    pub root_path: PathBuf,
    /// Error messages of folders that could not be read.
    pub errors: HashMap<NodeId, String>,
    /// True if the scan was cancelled; sizes are then too small.
    pub cancelled: bool,
}

pub const ROOT: NodeId = 0;

impl Tree {
    /// A tree with only the root node.
    pub fn new(root_path: PathBuf, kind: Kind) -> Self {
        let name: Box<OsStr> = root_path.as_os_str().into();
        Self {
            nodes: vec![Node::new(name, NO_PARENT, kind)],
            order: vec![ROOT],
            root_path,
            errors: HashMap::new(),
            cancelled: false,
        }
    }

    pub fn root(&self) -> &Node {
        &self.nodes[ROOT as usize]
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The value of a node in the charts. A further hard link counts 0, as
    /// in the sums, so the parts always add up to the whole.
    pub fn value(&self, id: NodeId, m: Measure) -> u64 {
        let n = self.node(id);
        if n.has(flags::HARDLINK_DUP) {
            return 0;
        }
        match m {
            Measure::Size => n.size,
            Measure::Allocated => n.allocated,
            Measure::Files => {
                if n.is_dir() {
                    n.files
                } else {
                    1
                }
            }
        }
    }

    /// Children of a node in display order.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        let n = self.node(id);
        let start = n.first_child as usize;
        &self.order[start..start + n.child_count as usize]
    }

    /// Adds the entries of folder `parent`. Must be called at most once per folder.
    /// Returns the id of the first new node; the others follow in sequence.
    pub fn add_children(&mut self, parent: NodeId, children: impl IntoIterator<Item = Node>) -> NodeId {
        let first = self.nodes.len() as NodeId;
        let start = self.order.len() as u32;
        for mut child in children {
            child.parent = parent;
            let id = self.nodes.len() as NodeId;
            self.nodes.push(child);
            self.order.push(id);
        }
        let p = &mut self.nodes[parent as usize];
        debug_assert_eq!(p.child_count, 0, "children added twice");
        p.first_child = start;
        p.child_count = (self.order.len() as u32) - start;
        first
    }

    /// Full path of a node.
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != ROOT && cur != NO_PARENT {
            let n = self.node(cur);
            parts.push(&*n.name);
            cur = n.parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    /// Ids from the root down to `id` (both included).
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut v = Vec::new();
        let mut cur = id;
        while cur != NO_PARENT {
            v.push(cur);
            cur = self.node(cur).parent;
        }
        v.reverse();
        v
    }

    pub fn depth(&self, id: NodeId) -> usize {
        self.ancestors(id).len() - 1
    }

    /// Adds up sizes, counts and dates from the leaves to the root.
    ///
    /// Works in one backwards pass because children always have higher ids
    /// than their parent. Folder values must start at their own values
    /// (size 0, own allocated size, time of the folder itself).
    pub fn sum_up(&mut self) {
        for id in (1..self.nodes.len()).rev() {
            let n = &self.nodes[id];
            let counted = !n.has(flags::HARDLINK_DUP);
            let (size, alloc) = if counted { (n.size, n.allocated) } else { (0, 0) };
            let files = n.files + u64::from(n.kind != Kind::Dir);
            let dirs = n.dirs + u64::from(n.kind == Kind::Dir);
            let modified = n.modified;
            let error = n.has(flags::ERROR | flags::NOT_SCANNED);
            let parent = n.parent as usize;
            let p = &mut self.nodes[parent];
            p.size += size;
            p.allocated += alloc;
            p.files += files;
            p.dirs += dirs;
            p.modified = p.modified.max(modified);
            if error {
                p.flags |= flags::ERROR;
            }
        }
    }

    /// Sorts the children of every folder. Size-like keys sort largest first,
    /// names A to Z; `reverse` turns the order around.
    pub fn sort(&mut self, key: SortKey, reverse: bool) {
        for id in 0..self.nodes.len() as NodeId {
            if self.nodes[id as usize].child_count > 1 {
                self.sort_children(id, key, reverse);
            }
        }
    }

    /// Sorts the children of one folder.
    pub fn sort_children(&mut self, id: NodeId, key: SortKey, reverse: bool) {
        let nodes = &self.nodes;
        let n = &nodes[id as usize];
        let start = n.first_child as usize;
        let slice = &mut self.order[start..start + n.child_count as usize];
        slice.sort_by(|&a, &b| {
            let (a, b) = (&nodes[a as usize], &nodes[b as usize]);
            let ord = match key {
                SortKey::Name => compare_names(&a.name, &b.name),
                SortKey::Size => b.size.cmp(&a.size),
                SortKey::Allocated => b.allocated.cmp(&a.allocated),
                SortKey::Files => (b.files + u64::from(!b.is_dir())).cmp(&(a.files + u64::from(!a.is_dir()))),
                SortKey::Modified => b.modified.cmp(&a.modified),
            }
            .then_with(|| compare_names(&a.name, &b.name));
            if reverse { ord.reverse() } else { ord }
        });
    }

    /// What a node adds to the sums of its parent: size, allocated, files, folders.
    fn share(&self, id: NodeId) -> [u64; 4] {
        let n = self.node(id);
        let counted = !n.has(flags::HARDLINK_DUP);
        let (size, alloc) = if counted { (n.size, n.allocated) } else { (0, 0) };
        [size, alloc, n.files + u64::from(n.kind != Kind::Dir), n.dirs + u64::from(n.kind == Kind::Dir)]
    }

    /// Changes the sums of all folders above `id` from `before` to `after`.
    fn adjust_above(&mut self, id: NodeId, before: [u64; 4], after: [u64; 4], modified: i64) {
        let mut cur = self.node(id).parent;
        while cur != NO_PARENT {
            let p = &mut self.nodes[cur as usize];
            let fields = [&mut p.size, &mut p.allocated, &mut p.files, &mut p.dirs];
            for (i, v) in fields.into_iter().enumerate() {
                *v = v.saturating_sub(before[i]).saturating_add(after[i]);
            }
            p.modified = p.modified.max(modified);
            cur = p.parent;
        }
    }

    /// New size, disk space and time of a file (live update); the folders above follow.
    pub fn set_file_values(&mut self, id: NodeId, size: u64, allocated: u64, modified: i64) {
        let before = self.share(id);
        let n = &mut self.nodes[id as usize];
        n.size = size;
        n.allocated = allocated;
        n.modified = modified;
        let after = self.share(id);
        self.adjust_above(id, before, after, modified);
    }

    /// Sets or clears a flag of one node.
    pub fn set_flag(&mut self, id: NodeId, flag: u8, on: bool) {
        let n = &mut self.nodes[id as usize];
        if on {
            n.flags |= flag;
        } else {
            n.flags &= !flag;
        }
    }

    /// Adds one more entry to a folder (live update) and returns its id.
    ///
    /// The child list moves to the end of `order`, so the children stay next
    /// to each other, and the new node gets the highest id, so a parent still
    /// comes before its children. The folders above take over its sums.
    pub fn add_child(&mut self, parent: NodeId, mut child: Node) -> NodeId {
        child.parent = parent;
        child.first_child = 0;
        child.child_count = 0;
        let id = self.nodes.len() as NodeId;
        let start = self.order.len();
        let p = self.node(parent);
        let old = p.first_child as usize..p.first_child as usize + p.child_count as usize;
        self.order.extend_from_within(old);
        self.order.push(id);
        let count = (self.order.len() - start) as u32;
        self.nodes.push(child);
        let p = &mut self.nodes[parent as usize];
        p.first_child = start as u32;
        p.child_count = count;
        let after = self.share(id);
        let modified = self.node(id).modified;
        self.adjust_above(id, [0; 4], after, modified);
        id
    }

    /// Puts a separately scanned folder in place of the empty folder `at`
    /// (live update) and adds its sums to the folders above.
    pub fn graft(&mut self, at: NodeId, sub: Tree) {
        if self.node(at).child_count != 0 || !self.node(at).is_dir() {
            return;
        }
        let before = self.share(at);
        let base = self.nodes.len() as NodeId;
        let order_base = self.order.len() as u32;
        // Node k of `sub` becomes base + k - 1; its root becomes `at`.
        let map = |k: NodeId| if k == ROOT { at } else { base + k - 1 };
        let mut nodes = sub.nodes.into_iter();
        let Some(root) = nodes.next() else { return };
        {
            let a = &mut self.nodes[at as usize];
            a.size = root.size;
            a.allocated = root.allocated;
            a.files = root.files;
            a.dirs = root.dirs;
            a.modified = a.modified.max(root.modified);
            a.flags = (a.flags & !(flags::NOT_SCANNED | flags::ERROR)) | (root.flags & flags::ERROR);
            a.child_count = root.child_count;
            // The order list of `sub` starts with its root, which is left out here.
            a.first_child = if root.child_count > 0 { order_base + root.first_child - 1 } else { 0 };
        }
        for mut n in nodes {
            n.parent = map(n.parent);
            if n.child_count > 0 {
                n.first_child = order_base + n.first_child - 1;
            }
            self.nodes.push(n);
        }
        self.order.extend(sub.order.iter().skip(1).map(|&k| map(k)));
        for (k, e) in sub.errors {
            self.errors.insert(map(k), e);
        }
        let after = self.share(at);
        let modified = self.node(at).modified;
        self.adjust_above(at, before, after, modified);
    }

    /// Number of entries that are no longer part of the tree (after live updates).
    pub fn unused(&self) -> usize {
        let used = 1 + self.root().files as usize + self.root().dirs as usize;
        self.nodes.len().saturating_sub(used) + self.order.len().saturating_sub(self.nodes.len())
    }

    /// A copy with only the entries of at least `min_size` bytes, down to
    /// `max_depth` levels and with at most `max_nodes` nodes; the largest
    /// entries are kept first. Sums stay as they were. Without limits it is a
    /// tidy copy of the tree without removed entries.
    pub fn copy(&self, min_size: u64, max_depth: usize, max_nodes: usize, keep_errors: bool) -> Tree {
        let mut root = self.root().clone();
        root.first_child = 0;
        root.child_count = 0;
        let mut out = Tree {
            nodes: vec![root],
            order: vec![ROOT],
            root_path: self.root_path.clone(),
            errors: HashMap::new(),
            cancelled: self.cancelled,
        };
        let mut queue = std::collections::VecDeque::from([(ROOT, ROOT, 0usize)]);
        while let Some((old, new, depth)) = queue.pop_front() {
            if keep_errors && let Some(e) = self.errors.get(&old) {
                out.errors.insert(new, e.clone());
            }
            if depth >= max_depth {
                continue;
            }
            let mut kids: Vec<NodeId> =
                self.children(old).iter().copied().filter(|&c| self.node(c).size >= min_size).collect();
            let room = max_nodes.saturating_sub(out.nodes.len());
            if kids.len() > room {
                kids.sort_by_key(|&c| std::cmp::Reverse(self.node(c).size));
                kids.truncate(room);
            }
            if kids.is_empty() {
                continue;
            }
            let copies: Vec<Node> = kids
                .iter()
                .map(|&c| {
                    let mut n = self.node(c).clone();
                    n.first_child = 0;
                    n.child_count = 0;
                    n
                })
                .collect();
            let first = out.add_children(new, copies);
            for (i, &c) in kids.iter().enumerate() {
                if self.node(c).child_count > 0 {
                    queue.push_back((c, first + i as NodeId, depth + 1));
                }
            }
        }
        out
    }

    /// Takes a node out after it was moved away (recycle bin, live update):
    /// the sums above it shrink and it is no longer listed under its parent.
    pub fn remove(&mut self, id: NodeId) {
        let n = &self.nodes[id as usize];
        if id == ROOT || n.has(flags::REMOVED) {
            return;
        }
        let parent = n.parent;
        let before = self.share(id);
        self.adjust_above(id, before, [0; 4], NO_TIME);
        let p = &mut self.nodes[parent as usize];
        let start = p.first_child as usize;
        let slice = &mut self.order[start..start + p.child_count as usize];
        if let Some(pos) = slice.iter().position(|&c| c == id) {
            slice[pos..].rotate_left(1);
            p.child_count -= 1;
        }
        self.nodes[id as usize].flags |= flags::REMOVED;
    }

    /// Finds the node with this path, if it is part of the tree.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let rest = path.strip_prefix(&self.root_path).ok()?;
        let mut cur = ROOT;
        for part in rest.components() {
            let name = part.as_os_str();
            cur = *self.children(cur).iter().find(|&&c| &*self.node(c).name == name)?;
        }
        Some(cur)
    }
}

/// Case-insensitive name order, ties broken by the exact name.
pub fn compare_names(a: &OsStr, b: &OsStr) -> std::cmp::Ordering {
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase)).then_with(|| a.cmp(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> Node {
        let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
        n.size = size;
        n.allocated = size.div_ceil(4096) * 4096;
        n.modified = size as i64;
        n
    }

    fn dir(name: &str) -> Node {
        Node::new(OsStr::new(name).into(), NO_PARENT, Kind::Dir)
    }

    /// root/{a.txt 10, sub/{b 100, c 1}, z 5}
    fn sample() -> Tree {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let first = t.add_children(ROOT, [file("a.txt", 10), dir("sub"), file("Z", 5)]);
        let sub = first + 1;
        t.add_children(sub, [file("b", 100), file("c", 1)]);
        t.sum_up();
        t
    }

    #[test]
    fn sums_and_counts() {
        let t = sample();
        assert_eq!(t.root().size, 116);
        assert_eq!(t.root().files, 4);
        assert_eq!(t.root().dirs, 1);
        assert_eq!(t.root().modified, 100);
        let sub = t.find(Path::new("/x/sub")).unwrap();
        assert_eq!(t.node(sub).size, 101);
        assert_eq!(t.node(sub).files, 2);
        assert_eq!(t.path(sub + 3), PathBuf::from("/x/sub/c"));
    }

    #[test]
    fn hardlink_duplicates_are_not_counted() {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let mut dup = file("b", 50);
        dup.flags |= flags::HARDLINK_DUP;
        t.add_children(ROOT, [file("a", 50), dup]);
        t.sum_up();
        assert_eq!(t.root().size, 50);
        assert_eq!(t.root().files, 2);
    }

    #[test]
    fn sorting() {
        let mut t = sample();
        let names = |t: &Tree| -> Vec<String> {
            t.children(ROOT).iter().map(|&c| t.node(c).name_lossy().into_owned()).collect()
        };
        t.sort(SortKey::Size, false);
        assert_eq!(names(&t), ["sub", "a.txt", "Z"]);
        t.sort(SortKey::Name, false);
        assert_eq!(names(&t), ["a.txt", "sub", "Z"]);
        t.sort(SortKey::Name, true);
        assert_eq!(names(&t), ["Z", "sub", "a.txt"]);
        // Sorting must not break the lookup by path.
        assert!(t.find(Path::new("/x/sub/b")).is_some());
    }

    #[test]
    fn removing_updates_sums_and_lists() {
        let mut t = sample();
        let sub = t.find(Path::new("/x/sub")).unwrap();
        t.remove(sub);
        assert_eq!(t.root().size, 15);
        assert_eq!(t.root().files, 2);
        assert_eq!(t.root().dirs, 0);
        assert_eq!(t.children(ROOT).len(), 2);
        assert!(t.find(Path::new("/x/sub")).is_none());
        t.remove(sub); // twice changes nothing
        assert_eq!(t.root().size, 15);
        t.remove(ROOT);
        assert_eq!(t.root().size, 15);
    }

    #[test]
    fn ancestors_and_depth() {
        let t = sample();
        let c = t.find(Path::new("/x/sub/c")).unwrap();
        assert_eq!(t.depth(c), 2);
        assert_eq!(t.ancestors(c)[0], ROOT);
        assert!(t.find(Path::new("/y")).is_none());
        assert!(t.find(Path::new("/x/nope")).is_none());
    }

    #[test]
    fn live_changes_keep_sums_right() {
        let mut t = sample();
        let sub = t.find(Path::new("/x/sub")).unwrap();
        // A new file in "sub".
        let id = t.add_child(sub, file("d", 1000));
        assert_eq!(t.root().size, 1116);
        assert_eq!(t.root().files, 5);
        assert_eq!(t.path(id), PathBuf::from("/x/sub/d"));
        assert_eq!(t.children(sub).len(), 3);
        // A file grows.
        t.set_file_values(id, 2000, 4096, 7);
        assert_eq!(t.node(sub).size, 2101);
        assert_eq!(t.root().size, 2116);
        // A new folder that was scanned on its own.
        let new_dir = t.add_child(ROOT, dir("neu"));
        assert_eq!(t.root().dirs, 2);
        let mut sub_tree = Tree::new(PathBuf::from("/x/neu"), Kind::Dir);
        let first = sub_tree.add_children(ROOT, [dir("tief"), file("e", 50)]);
        sub_tree.add_children(first, [file("f", 7)]);
        sub_tree.sum_up();
        t.graft(new_dir, sub_tree);
        assert_eq!(t.node(new_dir).size, 57);
        assert_eq!(t.root().size, 2173);
        assert_eq!(t.root().files, 7);
        assert_eq!(t.root().dirs, 3);
        assert!(t.find(Path::new("/x/neu/tief/f")).is_some());
        // Removing again gives the old numbers; the copy drops the unused nodes.
        t.remove(new_dir);
        t.remove(id);
        assert_eq!(t.root().size, 116);
        assert!(t.unused() > 0);
        let tidy = t.copy(0, usize::MAX, usize::MAX, true);
        assert_eq!(tidy.len(), 6);
        assert_eq!(tidy.unused(), 0);
        assert_eq!(tidy.root().size, 116);
        assert!(tidy.find(Path::new("/x/sub/b")).is_some());
    }

    #[test]
    fn small_copy_keeps_the_largest() {
        let t = sample();
        let c = t.copy(5, 1, usize::MAX, false);
        // "c" (1 byte) is too small, and level 2 is too deep.
        assert_eq!(c.len(), 4);
        assert!(c.find(Path::new("/x/sub/b")).is_none());
        assert_eq!(c.node(c.find(Path::new("/x/sub")).unwrap()).size, 101);
        let c = t.copy(0, usize::MAX, 2, false);
        assert_eq!(c.len(), 2);
        assert!(c.find(Path::new("/x/sub")).is_some());
    }
}
