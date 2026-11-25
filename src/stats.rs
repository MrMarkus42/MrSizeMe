//! Space per file type (file name extension) below a folder.

use std::collections::HashMap;

use crate::tree::{Kind, NodeId, Tree, flags};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeStats {
    /// Extension in lower case without the dot; empty for files without one.
    pub extension: String,
    pub files: u64,
    pub size: u64,
    pub allocated: u64,
}

/// Extension of a file name in lower case ("" if there is none).
/// A leading dot (".bashrc") does not start an extension.
pub fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(pos) if pos > 0 && pos + 1 < name.len() => name[pos + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// Statistics for all files below `start`, largest first.
pub fn by_type(tree: &Tree, start: NodeId) -> Vec<TypeStats> {
    let mut map: HashMap<String, TypeStats> = HashMap::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        let n = tree.node(id);
        match n.kind {
            Kind::Dir => stack.extend_from_slice(tree.children(id)),
            Kind::File => {
                if n.has(flags::HARDLINK_DUP | flags::ONLINE_ONLY) {
                    continue;
                }
                let ext = extension(&n.name_lossy());
                let e = map.entry(ext.clone()).or_insert_with(|| TypeStats {
                    extension: ext,
                    files: 0,
                    size: 0,
                    allocated: 0,
                });
                e.files += 1;
                e.size += n.size;
                e.allocated += n.allocated;
            }
            Kind::Link | Kind::Other => {}
        }
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.extension.cmp(&b.extension)));
    v
}

/// Ids of the `limit` largest files below `start`, largest first.
pub fn largest_files(tree: &Tree, start: NodeId, limit: usize) -> Vec<NodeId> {
    let mut files = Vec::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        let n = tree.node(id);
        match n.kind {
            Kind::Dir => stack.extend_from_slice(tree.children(id)),
            // Cloud files that are only online take no space on this computer.
            Kind::File if !n.has(flags::ONLINE_ONLY) => files.push(id),
            _ => {}
        }
    }
    let key = |id: &NodeId| std::cmp::Reverse(tree.node(*id).size);
    if files.len() > limit {
        files.select_nth_unstable_by_key(limit, key);
        files.truncate(limit);
    }
    files.sort_by_key(key);
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn file(name: &str, size: u64) -> Node {
        let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
        n.size = size;
        n
    }

    #[test]
    fn extensions() {
        assert_eq!(extension("a.TXT"), "txt");
        assert_eq!(extension("archiv.tar.gz"), "gz");
        assert_eq!(extension(".bashrc"), "");
        assert_eq!(extension("README"), "");
        assert_eq!(extension("punkt."), "");
    }

    #[test]
    fn stats_and_largest() {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let first = t.add_children(
            ROOT,
            [
                file("a.jpg", 10),
                file("b.JPG", 30),
                file("c", 5),
                Node::new(OsStr::new("d").into(), NO_PARENT, Kind::Dir),
            ],
        );
        t.add_children(first + 3, [file("e.txt", 100)]);
        t.sum_up();
        let s = by_type(&t, ROOT);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].extension.as_str(), s[0].files, s[0].size), ("txt", 1, 100));
        assert_eq!((s[1].extension.as_str(), s[1].files, s[1].size), ("jpg", 2, 40));
        assert_eq!(s[2].extension, "");
        let big = largest_files(&t, ROOT, 2);
        let names: Vec<_> = big.iter().map(|&id| t.node(id).name_lossy().into_owned()).collect();
        assert_eq!(names, ["e.txt", "b.JPG"]);
    }
}
