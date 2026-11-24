//! Saving and loading a scan in a compact binary form.
//!
//! Used by the admin helper to hand its result to the window, and later
//! for saved scans. Reading checks every number against the data, so a
//! damaged or forged file gives an error, never a crash or a huge allocation.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::tree::{Kind, NO_PARENT, Node, NodeId, Tree};

const MAGIC: &[u8; 8] = b"MRSZSNP1";
/// Names are stored as the system writes them; a file is only read on the same kind of system.
const SYSTEM: u8 = if cfg!(windows) { 1 } else { 0 };
const MAX_NAME: usize = 1 << 16;
const MAX_NODES: usize = 1 << 28;

fn os_bytes(s: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        s.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().flat_map(u16::to_le_bytes).collect()
    }
}

fn os_string(b: &[u8]) -> io::Result<OsString> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(OsStr::from_bytes(b).to_os_string())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if !b.len().is_multiple_of(2) {
            return Err(bad("odd name length"));
        }
        let wide: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        Ok(OsString::from_wide(&wide))
    }
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged scan file: {what}"))
}

struct Out<'a>(&'a mut dyn Write);

impl Out<'_> {
    fn bytes(&mut self, b: &[u8]) -> io::Result<()> {
        self.u32(b.len() as u32)?;
        self.0.write_all(b)
    }
    fn u8(&mut self, v: u8) -> io::Result<()> {
        self.0.write_all(&[v])
    }
    fn u32(&mut self, v: u32) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
}

struct In<'a> {
    data: &'a [u8],
    pos: usize,
}

impl In<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end =
            self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| bad("too short"))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| bad("u32"))?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| bad("u64"))?))
    }
    fn bytes(&mut self, max: usize) -> io::Result<&[u8]> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(bad("entry too long"));
        }
        self.take(n)
    }
    /// A count of items that each need at least `min_size` bytes.
    fn count(&mut self, min_size: usize, max: usize) -> io::Result<usize> {
        let n = self.u32()? as usize;
        if n > max || n.saturating_mul(min_size) > self.data.len() - self.pos {
            return Err(bad("count"));
        }
        Ok(n)
    }
}

pub fn write(tree: &Tree, w: &mut dyn Write) -> io::Result<()> {
    let mut o = Out(w);
    o.0.write_all(MAGIC)?;
    o.u8(SYSTEM)?;
    o.bytes(&os_bytes(tree.root_path.as_os_str()))?;
    o.u8(u8::from(tree.cancelled))?;
    o.u32(tree.nodes.len() as u32)?;
    for n in &tree.nodes {
        o.bytes(&os_bytes(&n.name))?;
        o.u32(n.parent)?;
        o.u8(match n.kind {
            Kind::Dir => 0,
            Kind::File => 1,
            Kind::Link => 2,
            Kind::Other => 3,
        })?;
        o.u8(n.flags)?;
        o.u64(n.size)?;
        o.u64(n.allocated)?;
        o.u64(n.files)?;
        o.u64(n.dirs)?;
        o.u64(n.modified as u64)?;
        o.u32(n.first_child)?;
        o.u32(n.child_count)?;
    }
    o.u32(tree.order.len() as u32)?;
    for &id in &tree.order {
        o.u32(id)?;
    }
    let mut errors: Vec<_> = tree.errors.iter().collect();
    errors.sort();
    o.u32(errors.len() as u32)?;
    for (id, text) in errors {
        o.u32(*id)?;
        o.bytes(text.as_bytes())?;
    }
    o.0.flush()
}

pub fn read(r: &mut dyn Read) -> io::Result<Tree> {
    let mut data = Vec::new();
    r.read_to_end(&mut data)?;
    from_bytes(&data)
}

pub fn from_bytes(data: &[u8]) -> io::Result<Tree> {
    let mut i = In { data, pos: 0 };
    if i.take(MAGIC.len())? != MAGIC {
        return Err(bad("not a scan file"));
    }
    if i.u8()? != SYSTEM {
        return Err(bad("made on another kind of system"));
    }
    let root_path = PathBuf::from(os_string(i.bytes(MAX_NAME)?)?);
    let cancelled = i.u8()? != 0;
    // Smallest node: 4 (name length) + 4 + 1 + 1 + 5 * 8 + 4 + 4 bytes.
    let n = i.count(58, MAX_NODES)?;
    if n == 0 {
        return Err(bad("no root"));
    }
    let mut nodes = Vec::with_capacity(n);
    for idx in 0..n {
        let name: Box<OsStr> = os_string(i.bytes(MAX_NAME)?)?.into_boxed_os_str();
        let parent = i.u32()?;
        let kind = match i.u8()? {
            0 => Kind::Dir,
            1 => Kind::File,
            2 => Kind::Link,
            3 => Kind::Other,
            _ => return Err(bad("kind")),
        };
        // Parents always come before their children; only the root has none.
        if (idx == 0) != (parent == NO_PARENT) || (idx > 0 && parent as usize >= idx) {
            return Err(bad("parent"));
        }
        let mut node = Node::new(name, parent, kind);
        node.flags = i.u8()?;
        node.size = i.u64()?;
        node.allocated = i.u64()?;
        node.files = i.u64()?;
        node.dirs = i.u64()?;
        node.modified = i.u64()? as i64;
        node.first_child = i.u32()?;
        node.child_count = i.u32()?;
        nodes.push(node);
    }
    let m = i.count(4, MAX_NODES)?;
    if m != n {
        return Err(bad("order"));
    }
    let mut order = Vec::with_capacity(m);
    for _ in 0..m {
        let id = i.u32()?;
        if id as usize >= n {
            return Err(bad("order entry"));
        }
        order.push(id);
    }
    for (idx, node) in nodes.iter().enumerate() {
        let end = node.first_child as usize + node.child_count as usize;
        if end > m {
            return Err(bad("children"));
        }
        // Every listed child must really belong to this node.
        if order[node.first_child as usize..end].iter().any(|&c| nodes[c as usize].parent as usize != idx) {
            return Err(bad("child parent"));
        }
    }
    let e = i.count(8, MAX_NODES)?;
    let mut errors = HashMap::with_capacity(e);
    for _ in 0..e {
        let id: NodeId = i.u32()?;
        if id as usize >= n {
            return Err(bad("error entry"));
        }
        let text = String::from_utf8_lossy(i.bytes(MAX_NAME)?).into_owned();
        errors.insert(id, text);
    }
    if i.pos != data.len() {
        return Err(bad("trailing data"));
    }
    Ok(Tree { nodes, order, root_path, errors, cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{ROOT, flags};

    fn sample() -> Tree {
        let mut t = Tree::new(PathBuf::from("/x/ü ä"), Kind::Dir);
        let mut f = Node::new(OsStr::new("a.txt").into(), NO_PARENT, Kind::File);
        f.size = 10;
        f.modified = -5;
        let first = t.add_children(ROOT, [Node::new(OsStr::new("sub").into(), NO_PARENT, Kind::Dir), f]);
        let mut g = Node::new(OsStr::new("g").into(), NO_PARENT, Kind::File);
        g.size = 3;
        g.flags = flags::HARDLINK_DUP;
        t.add_children(first, [g]);
        t.errors.insert(first, "Zugriff verweigert".into());
        t.sum_up();
        t
    }

    #[test]
    fn round_trip() {
        let t = sample();
        let mut buf = Vec::new();
        write(&t, &mut buf).unwrap();
        let back = from_bytes(&buf).unwrap();
        assert_eq!(back.root_path, t.root_path);
        assert_eq!(back.len(), t.len());
        assert_eq!(back.order, t.order);
        assert_eq!(back.errors, t.errors);
        for (a, b) in back.nodes.iter().zip(&t.nodes) {
            assert_eq!((&a.name, a.parent, a.kind, a.flags), (&b.name, b.parent, b.kind, b.flags));
            assert_eq!(
                (a.size, a.allocated, a.files, a.dirs, a.modified),
                (b.size, b.allocated, b.files, b.dirs, b.modified)
            );
        }
    }

    #[test]
    fn damaged_files_are_rejected() {
        let t = sample();
        let mut buf = Vec::new();
        write(&t, &mut buf).unwrap();
        // Every shortened file and every single flipped byte must give an error or a valid tree,
        // never a panic.
        for len in 0..buf.len() {
            assert!(from_bytes(&buf[..len]).is_err(), "length {len}");
        }
        for pos in 0..buf.len() {
            let mut b = buf.clone();
            b[pos] ^= 0xff;
            let _ = from_bytes(&b);
        }
        let mut extra = buf.clone();
        extra.push(0);
        assert!(from_bytes(&extra).is_err());
        assert!(from_bytes(b"MRSZSNP1").is_err());
        assert!(from_bytes(b"something else").is_err());
    }
}
