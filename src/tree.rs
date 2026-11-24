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
        let nodes = &self.nodes;
        for n in nodes.iter().filter(|n| n.child_count > 1) {
            let start = n.first_child as usize;
            let slice = &mut self.order[start..start + n.child_count as usize];
            slice.sort_by(|&a, &b| {
                let (a, b) = (&nodes[a as usize], &nodes[b as usize]);
                let ord = match key {
                    SortKey::Name => compare_names(&a.name, &b.name),
                    SortKey::Size => b.size.cmp(&a.size),
                    SortKey::Allocated => b.allocated.cmp(&a.allocated),
                    SortKey::Files => {
                        (b.files + u64::from(!b.is_dir())).cmp(&(a.files + u64::from(!a.is_dir())))
                    }
                    SortKey::Modified => b.modified.cmp(&a.modified),
                }
                .then_with(|| compare_names(&a.name, &b.name));
                if reverse { ord.reverse() } else { ord }
            });
        }
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
    fn ancestors_and_depth() {
        let t = sample();
        let c = t.find(Path::new("/x/sub/c")).unwrap();
        assert_eq!(t.depth(c), 2);
        assert_eq!(t.ancestors(c)[0], ROOT);
        assert!(t.find(Path::new("/y")).is_none());
        assert!(t.find(Path::new("/x/nope")).is_none());
    }
}
