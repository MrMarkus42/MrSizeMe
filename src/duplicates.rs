//! Finds files with the same content.
//!
//! Step 1 groups the files of a scan by size (no disk access). Step 2 reads
//! only the first 64 KiB of files of equal size, step 3 the whole file, and
//! compares SHA-256 checksums. Cloud files that are only online are never
//! read (that would download them), nor are further hard links of a file.
//! This is the only part of the program that reads file contents; nothing
//! of it is kept or sent anywhere.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::tree::{Kind, NodeId, Tree, flags};

const HEAD: u64 = 64 * 1024;

/// Files with the same content.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Size of each file.
    pub size: u64,
    pub paths: Vec<PathBuf>,
}

impl Group {
    /// Space the copies take beyond the first file.
    pub fn wasted(&self) -> u64 {
        self.size.saturating_mul(self.paths.len().saturating_sub(1) as u64)
    }
}

#[derive(Default)]
pub struct Progress {
    /// Bytes read so far and bytes to read at most.
    pub done: AtomicU64,
    pub total: AtomicU64,
    cancel: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Files of equal size below `start` (at least `min_size` bytes), largest first.
pub fn candidates(tree: &Tree, start: NodeId, min_size: u64) -> Vec<(u64, Vec<PathBuf>)> {
    let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        for &c in tree.children(id) {
            let n = tree.node(c);
            match n.kind {
                Kind::Dir => stack.push(c),
                Kind::File => {
                    let skip = n.has(flags::HARDLINK_DUP | flags::ONLINE_ONLY)
                        // Content not on the disk (cloud or sparse placeholder).
                        || (n.allocated == 0 && n.size > 1024 && cfg!(windows));
                    if n.size >= min_size.max(1) && !skip {
                        by_size.entry(n.size).or_default().push(c);
                    }
                }
                _ => {}
            }
        }
    }
    let mut groups: Vec<(u64, Vec<PathBuf>)> = by_size
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(size, ids)| (size, ids.into_iter().map(|id| tree.path(id)).collect()))
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.0 * g.1.len() as u64));
    groups
}

/// SHA-256 of the first `limit` bytes of a file.
fn checksum(path: &Path, limit: u64, progress: &Progress) -> io::Result<[u8; 32]> {
    let mut f = File::open(path)?.take(limit);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(h.finalize().into())
}

/// Splits each group by the checksum of the first `limit` bytes; keeps groups of two or more.
fn split(groups: Vec<(u64, Vec<PathBuf>)>, limit: u64, progress: &Progress) -> Vec<(u64, Vec<PathBuf>)> {
    let mut out = Vec::new();
    for (size, paths) in groups {
        let mut by_sum: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
        for p in paths {
            if progress.is_cancelled() {
                return Vec::new();
            }
            // Unreadable files (locked, no rights, gone) are left out.
            if let Ok(sum) = checksum(&p, limit, progress) {
                by_sum.entry(sum).or_default().push(p);
            }
        }
        out.extend(by_sum.into_values().filter(|v| v.len() > 1).map(|v| (size, v)));
    }
    out
}

/// Finds the groups of equal files among the candidates, largest waste first.
pub fn find(candidates: Vec<(u64, Vec<PathBuf>)>, progress: &Progress) -> Vec<Group> {
    let head: u64 = candidates.iter().map(|(s, p)| (*s).min(HEAD) * p.len() as u64).sum();
    let full: u64 = candidates.iter().filter(|(s, _)| *s > HEAD).map(|(s, p)| s * p.len() as u64).sum();
    progress.total.store(head + full, Ordering::Relaxed);
    let after_head = split(candidates, HEAD, progress);
    // Small files were read completely already.
    let (small, big): (Vec<_>, Vec<_>) = after_head.into_iter().partition(|(s, _)| *s <= HEAD);
    // Only files that still match need the full read; correct the total.
    let left: u64 = big.iter().map(|(s, p)| s * p.len() as u64).sum();
    progress.total.store(progress.done.load(Ordering::Relaxed) + left, Ordering::Relaxed);
    let mut groups: Vec<Group> = small
        .into_iter()
        .chain(split(big, u64::MAX, progress))
        .map(|(size, mut paths)| {
            paths.sort();
            Group { size, paths }
        })
        .collect();
    if progress.is_cancelled() {
        return Vec::new();
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_equal_files_only() {
        let dir = std::env::temp_dir().join(format!("mrsizeme-dups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("b")).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut other = big.clone();
        *other.last_mut().unwrap() ^= 1; // same size and start, different end
        std::fs::write(dir.join("eins.bin"), &big).unwrap();
        std::fs::write(dir.join("b/zwei.bin"), &big).unwrap();
        std::fs::write(dir.join("anders.bin"), &other).unwrap();
        std::fs::write(dir.join("klein1.txt"), b"hallo").unwrap();
        std::fs::write(dir.join("klein2.txt"), b"hallo").unwrap();
        std::fs::write(dir.join("klein3.txt"), b"hallO").unwrap();

        let tree = crate::scan::scan(&dir, &Default::default(), &Default::default()).unwrap();
        let c = candidates(&tree, crate::tree::ROOT, 1);
        assert_eq!(c.len(), 2);
        let p = Progress::default();
        let groups = find(c, &p);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].size, 200_000);
        assert_eq!(groups[0].paths, vec![dir.join("b/zwei.bin"), dir.join("eins.bin")]);
        assert_eq!(groups[0].wasted(), 200_000);
        assert_eq!(groups[1].paths.len(), 2);
        assert!(p.done.load(Ordering::Relaxed) <= p.total.load(Ordering::Relaxed));
        // A minimum size leaves the small ones out; cancelling gives nothing.
        assert_eq!(candidates(&tree, crate::tree::ROOT, 100).len(), 1);
        let stop = Progress::default();
        stop.cancel();
        assert!(find(candidates(&tree, crate::tree::ROOT, 1), &stop).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
