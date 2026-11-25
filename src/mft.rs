//! Fast scan of an NTFS drive by reading its master file table (MFT)
//! directly (Windows, admin rights only).
//!
//! The parsing is plain and checked code that runs on every system, so it
//! is tested everywhere with built records; only reading the drive is
//! Windows-specific. Every number from the disk is checked before use: a
//! broken record is skipped, never trusted.

use crate::tree::NO_TIME;

/// The root folder of every NTFS volume has this record number.
pub const ROOT_RECORD: u64 = 5;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_REPARSE_POINT: u32 = 0xC0;
const ATTR_END: u32 = 0xFFFF_FFFF;
const NAME_SURROGATE: u32 = 0x2000_0000;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).and_then(|s| s.try_into().ok()).map(u64::from_le_bytes)
}

/// What the boot sector says about the volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boot {
    pub bytes_per_cluster: u64,
    pub mft_cluster: u64,
    pub record_size: u64,
}

pub fn parse_boot(b: &[u8]) -> Option<Boot> {
    if b.get(3..11)? != b"NTFS    " {
        return None;
    }
    let bytes_per_sector = u64::from(u16_at(b, 0x0B)?);
    let sectors_per_cluster = u64::from(*b.get(0x0D)?);
    let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;
    let mft_cluster = u64_at(b, 0x30)?;
    let raw = *b.get(0x40)? as i8;
    let record_size =
        if raw < 0 { 1u64 << (-i32::from(raw)) } else { u64::from(raw as u8) * bytes_per_cluster };
    let sane = (512..=4096).contains(&bytes_per_sector)
        && bytes_per_cluster.is_power_of_two()
        && (512..=2 * 1024 * 1024).contains(&bytes_per_cluster)
        && (1024..=65536).contains(&record_size);
    sane.then_some(Boot { bytes_per_cluster, mft_cluster, record_size })
}

/// Repairs the last two bytes of every 512-byte stride of a record
/// ("update sequence"). Returns false if the record is torn or broken.
pub fn apply_fixups(rec: &mut [u8]) -> bool {
    let (Some(off), Some(count)) = (u16_at(rec, 4), u16_at(rec, 6)) else { return false };
    let (off, count) = (off as usize, count as usize);
    if count < 2 || off + 2 * count > rec.len() || (count - 1) * 512 > rec.len() {
        return false;
    }
    let Some(usn) = u16_at(rec, off) else { return false };
    for i in 1..count {
        let end = i * 512;
        if u16_at(rec, end - 2) != Some(usn) {
            return false;
        }
        let Some(fix) = u16_at(rec, off + 2 * i) else { return false };
        rec[end - 2..end].copy_from_slice(&fix.to_le_bytes());
    }
    true
}

/// Decodes the run list of a non-resident attribute into (first cluster,
/// number of clusters); sparse runs have no cluster and are left out.
pub fn decode_runs(b: &[u8]) -> Option<Vec<(u64, u64)>> {
    let mut runs = Vec::new();
    let mut pos = 0usize;
    let mut lcn: i64 = 0;
    while let Some(&head) = b.get(pos) {
        if head == 0 {
            return Some(runs);
        }
        let (len_size, off_size) = ((head & 0x0F) as usize, (head >> 4) as usize);
        if len_size == 0 || len_size > 8 || off_size > 8 {
            return None;
        }
        let len_bytes = b.get(pos + 1..pos + 1 + len_size)?;
        let mut len = 0u64;
        for (i, &x) in len_bytes.iter().enumerate() {
            len |= u64::from(x) << (8 * i);
        }
        let off_bytes = b.get(pos + 1 + len_size..pos + 1 + len_size + off_size)?;
        if off_size > 0 {
            let mut off: i64 = 0;
            for (i, &x) in off_bytes.iter().enumerate() {
                off |= i64::from(x) << (8 * i);
            }
            // Sign-extend the relative offset.
            let bits = 8 * off_size as u32;
            if bits < 64 && off & (1 << (bits - 1)) != 0 {
                off -= 1 << bits;
            }
            lcn = lcn.checked_add(off)?;
            if lcn < 0 {
                return None;
            }
            runs.push((lcn as u64, len));
        }
        pos += 1 + len_size + off_size;
    }
    None
}

/// The facts of one MFT record that the tree needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Record number of the base record (itself for base records).
    pub base: u64,
    pub in_use: bool,
    pub is_dir: bool,
    /// Parent folder record and name (preferring the long name over the 8.3 one).
    pub parent: Option<u64>,
    pub name: Vec<u16>,
    name_space: u8,
    pub size: u64,
    pub allocated: u64,
    /// Seconds since 1970.
    pub modified: i64,
    /// A link, junction or mount point: not followed.
    pub is_link: bool,
    /// A cloud file whose content is only online (attributes in $STANDARD_INFORMATION).
    pub online_only: bool,
    /// The run list of the unnamed data (only needed for the MFT itself).
    pub data_runs: Option<Vec<(u64, u64)>>,
}

/// Reads one MFT record (fixups already applied). `None` if it is not a
/// valid record.
pub fn parse_record(rec: &[u8], number: u64) -> Option<Record> {
    if rec.get(0..4)? != b"FILE" {
        return None;
    }
    let flags = u16_at(rec, 0x16)?;
    let base_ref = u64_at(rec, 0x20)? & 0x0000_FFFF_FFFF_FFFF;
    let mut r = Record {
        base: if base_ref == 0 { number } else { base_ref },
        in_use: flags & 1 != 0,
        is_dir: flags & 2 != 0,
        modified: NO_TIME,
        ..Record::default()
    };
    let mut pos = usize::from(u16_at(rec, 0x14)?);
    // At most this many attributes; guards against loops in broken records.
    for _ in 0..64 {
        let kind = u32_at(rec, pos)?;
        if kind == ATTR_END {
            break;
        }
        let len = u32_at(rec, pos + 4)? as usize;
        if len < 16 || pos + len > rec.len() {
            return None;
        }
        let attr = &rec[pos..pos + len];
        let non_resident = attr[8] != 0;
        let name_len = attr[9];
        match (kind, non_resident) {
            (ATTR_STANDARD_INFORMATION, false) => {
                let content = resident_content(attr)?;
                r.modified = crate::platform::filetime_to_unix(u64_at(content, 0x08)? as i64);
                // OFFLINE, RECALL_ON_OPEN, RECALL_ON_DATA_ACCESS: placeholders of cloud files.
                let attributes = u32_at(content, 0x20).unwrap_or(0);
                r.online_only = attributes & (0x1000 | 0x4_0000 | 0x40_0000) != 0;
            }
            (ATTR_FILE_NAME, false) => {
                let content = resident_content(attr)?;
                let space = *content.get(0x41)?;
                // 2 = only the short 8.3 name; any other name is better.
                if r.name.is_empty() || (r.name_space == 2 && space != 2) {
                    let n = usize::from(*content.get(0x40)?);
                    let bytes = content.get(0x42..0x42 + 2 * n)?;
                    r.name = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                    r.name_space = space;
                    r.parent = Some(u64_at(content, 0)? & 0x0000_FFFF_FFFF_FFFF);
                }
            }
            (ATTR_DATA, false) if name_len == 0 => {
                r.size = u64::from(u32_at(attr, 0x10)?);
            }
            (ATTR_DATA, true) if name_len == 0 => {
                // Only the first piece (starting at cluster 0) carries the sizes.
                if u64_at(attr, 0x10)? == 0 {
                    let compressed = u16_at(attr, 0x0C)? & 0x8001 != 0;
                    r.allocated = if compressed && u16_at(attr, 0x22)? != 0 {
                        u64_at(attr, 0x40)?
                    } else {
                        u64_at(attr, 0x28)?
                    };
                    r.size = u64_at(attr, 0x30)?;
                    let runs_at = usize::from(u16_at(attr, 0x20)?);
                    r.data_runs = attr.get(runs_at..).and_then(decode_runs);
                }
            }
            (ATTR_REPARSE_POINT, false) => {
                let content = resident_content(attr)?;
                r.is_link = u32_at(content, 0)? & NAME_SURROGATE != 0;
            }
            _ => {}
        }
        pos += len;
    }
    Some(r)
}

fn resident_content(attr: &[u8]) -> Option<&[u8]> {
    let len = u32_at(attr, 0x10)? as usize;
    let off = usize::from(u16_at(attr, 0x14)?);
    attr.get(off..off.checked_add(len)?)
}

#[cfg(windows)]
pub use win::scan;

#[cfg(windows)]
mod win {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs::File;
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::FileExt;
    use std::path::{Component, Path, Prefix};
    use std::sync::atomic::Ordering;

    use super::*;
    use crate::scan::Progress;
    use crate::tree::{Kind, Node, NodeId, ROOT, SortKey, Tree, flags};

    /// The drive letter of a path on a local drive ("C:\..." → 'C').
    fn drive_letter(p: &Path) -> Option<char> {
        match p.components().next()? {
            Component::Prefix(pre) => match pre.kind() {
                Prefix::Disk(d) | Prefix::VerbatimDisk(d) => Some(char::from(d).to_ascii_uppercase()),
                _ => None,
            },
            _ => None,
        }
    }

    fn read_at(vol: &File, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut done = 0;
        while done < buf.len() {
            let n = vol.seek_read(&mut buf[done..], offset + done as u64)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of volume"));
            }
            done += n;
        }
        Ok(())
    }

    /// Scans `root` from the MFT. `None` if this is not possible here (no
    /// admin rights, not NTFS, network drive …); the caller then scans normally.
    pub fn scan(root: &Path, progress: &Progress) -> Option<io::Result<Tree>> {
        let root = std::path::absolute(root).ok()?;
        let letter = drive_letter(&root)?;
        let vol = File::open(format!(r"\\.\{letter}:")).ok()?;
        let mut boot = vec![0u8; 4096];
        read_at(&vol, 0, &mut boot).ok()?;
        let boot = parse_boot(&boot)?;
        // The long names are needed to find the folder (no 8.3 short names).
        let canon = std::fs::canonicalize(&root).ok()?;
        Some(read_all(&vol, &boot, &canon, progress).map(|mut t| {
            t.root_path = root.clone();
            t
        }))
    }

    fn read_all(vol: &File, boot: &Boot, root: &Path, progress: &Progress) -> io::Result<Tree> {
        let rs = boot.record_size as usize;
        let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("MFT: {what}"));

        // Record 0 describes the MFT itself, including where its pieces are.
        let mut first = vec![0u8; rs];
        read_at(vol, boot.mft_cluster * boot.bytes_per_cluster, &mut first)?;
        if !apply_fixups(&mut first) {
            return Err(bad("record 0"));
        }
        let mft = parse_record(&first, 0).ok_or_else(|| bad("record 0"))?;
        let runs = mft.data_runs.ok_or_else(|| bad("no data runs"))?;
        let total_records = mft.size / boot.record_size;

        let mut records: Vec<Option<Record>> = Vec::with_capacity(total_records as usize);
        // Extension records: their data sizes belong to the base record.
        let mut extensions: Vec<Record> = Vec::new();
        let chunk_clusters = (4 * 1024 * 1024 / boot.bytes_per_cluster).max(1);
        let mut buf = Vec::new();
        'runs: for (lcn, len) in runs {
            let mut done = 0;
            while done < len {
                if progress.is_cancelled() {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                }
                let n = chunk_clusters.min(len - done);
                buf.resize((n * boot.bytes_per_cluster) as usize, 0);
                read_at(vol, (lcn + done) * boot.bytes_per_cluster, &mut buf)?;
                for rec in buf.chunks_exact_mut(rs) {
                    let number = records.len() as u64;
                    if number >= total_records {
                        break 'runs;
                    }
                    let parsed = if apply_fixups(rec) { parse_record(rec, number) } else { None };
                    match parsed {
                        Some(r) if r.in_use && r.base != number => {
                            extensions.push(r);
                            records.push(None);
                        }
                        Some(r) if r.in_use => {
                            if r.is_dir {
                                progress.dirs.fetch_add(1, Ordering::Relaxed);
                            } else {
                                progress.files.fetch_add(1, Ordering::Relaxed);
                                if !r.online_only {
                                    progress.bytes.fetch_add(r.size, Ordering::Relaxed);
                                }
                            }
                            records.push(Some(r));
                        }
                        _ => records.push(None),
                    }
                }
                done += n;
            }
        }
        for ext in extensions {
            if let Some(Some(base)) = records.get_mut(ext.base as usize) {
                if base.size == 0 && base.allocated == 0 {
                    base.size = ext.size;
                    base.allocated = ext.allocated;
                }
                if base.name.is_empty() {
                    base.name = ext.name;
                    base.parent = ext.parent;
                }
            }
        }
        build_tree(&records, root).ok_or_else(|| bad("folder not found in the MFT"))
    }

    /// Builds the tree below the folder `root` from the records.
    fn build_tree(records: &[Option<Record>], root: &Path) -> Option<Tree> {
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        for (i, r) in records.iter().enumerate() {
            if let Some(r) = r
                && let Some(p) = r.parent
                && i as u64 != ROOT_RECORD
                && !r.name.is_empty()
            {
                children.entry(p).or_default().push(i as u64);
            }
        }
        // Walk down from the drive root to the chosen folder, name by name.
        let mut start = ROOT_RECORD;
        for comp in root.components() {
            if let Component::Normal(name) = comp {
                let want = name.to_string_lossy().to_lowercase();
                start = *children.get(&start)?.iter().find(|&&c| {
                    records[c as usize]
                        .as_ref()
                        .is_some_and(|r| String::from_utf16_lossy(&r.name).to_lowercase() == want)
                })?;
            }
        }
        let rec = records.get(start as usize)?.as_ref()?;
        let mut tree = Tree::new(root.to_path_buf(), if rec.is_dir { Kind::Dir } else { Kind::File });
        tree.nodes[ROOT as usize].modified = rec.modified;
        // Breadth first, so every folder's children are added in one go.
        let mut queue = std::collections::VecDeque::from([(ROOT, start)]);
        while let Some((node, record)) = queue.pop_front() {
            let Some(kids) = children.get(&record) else { continue };
            let mut nodes = Vec::with_capacity(kids.len());
            let mut dirs = Vec::new();
            for &k in kids {
                let Some(r) = records[k as usize].as_ref() else { continue };
                let kind = if r.is_link {
                    Kind::Link
                } else if r.is_dir {
                    Kind::Dir
                } else {
                    Kind::File
                };
                let mut n = Node::new(OsString::from_wide(&r.name).into_boxed_os_str(), node, kind);
                if kind == Kind::File {
                    n.size = r.size;
                    n.allocated = r.allocated;
                    if r.online_only {
                        n.flags |= crate::tree::flags::ONLINE_ONLY;
                    }
                }
                n.modified = r.modified;
                if kind == Kind::Dir {
                    dirs.push((nodes.len(), k));
                }
                nodes.push(n);
            }
            let first = tree.add_children(node, nodes);
            for (offset, k) in dirs {
                queue.push_back((first + offset as NodeId, k));
            }
        }
        // MFT data can lag behind for files changed a moment ago; nothing is "not scanned".
        for n in &mut tree.nodes {
            n.flags &= !flags::NOT_SCANNED;
        }
        tree.sum_up();
        tree.sort(SortKey::Size, false);
        Some(tree)
    }
}

/// Not Windows: never possible.
#[cfg(not(windows))]
pub fn scan(
    _root: &std::path::Path,
    _progress: &crate::scan::Progress,
) -> Option<std::io::Result<crate::tree::Tree>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a 1024-byte record with fixups and the given attributes.
    fn record(flags: u16, base: u64, attrs: &[Vec<u8>]) -> Vec<u8> {
        let mut r = vec![0u8; 1024];
        r[0..4].copy_from_slice(b"FILE");
        r[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // update sequence at 0x30
        r[6..8].copy_from_slice(&3u16.to_le_bytes()); // 1 + 2 strides
        r[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
        r[0x20..0x28].copy_from_slice(&base.to_le_bytes());
        let mut pos = 0x38;
        for a in attrs {
            r[pos..pos + a.len()].copy_from_slice(a);
            pos += a.len();
        }
        r[pos..pos + 4].copy_from_slice(&ATTR_END.to_le_bytes());
        // Protect the stride ends: store the originals, put the USN there.
        let usn = 0xABCDu16;
        r[0x30..0x32].copy_from_slice(&usn.to_le_bytes());
        for i in 1..3 {
            let end = i * 512;
            let orig = [r[end - 2], r[end - 1]];
            r[0x30 + 2 * i..0x32 + 2 * i].copy_from_slice(&orig);
            r[end - 2..end].copy_from_slice(&usn.to_le_bytes());
        }
        r
    }

    fn resident(kind: u32, content: &[u8]) -> Vec<u8> {
        let len = (0x18 + content.len()).div_ceil(8) * 8;
        let mut a = vec![0u8; len];
        a[0..4].copy_from_slice(&kind.to_le_bytes());
        a[4..8].copy_from_slice(&(len as u32).to_le_bytes());
        a[0x10..0x14].copy_from_slice(&(content.len() as u32).to_le_bytes());
        a[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
        a[0x18..0x18 + content.len()].copy_from_slice(content);
        a
    }

    fn file_name(parent: u64, name: &str, space: u8) -> Vec<u8> {
        let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut c = vec![0u8; 0x42 + wide.len()];
        c[0..8].copy_from_slice(&parent.to_le_bytes());
        c[0x40] = (wide.len() / 2) as u8;
        c[0x41] = space;
        c[0x42..].copy_from_slice(&wide);
        resident(ATTR_FILE_NAME, &c)
    }

    fn std_info(filetime: i64) -> Vec<u8> {
        let mut c = vec![0u8; 0x48];
        c[0x08..0x10].copy_from_slice(&filetime.to_le_bytes());
        resident(ATTR_STANDARD_INFORMATION, &c)
    }

    fn non_resident_data(alloc: u64, size: u64, runs: &[u8]) -> Vec<u8> {
        let len = (0x40 + runs.len()).div_ceil(8) * 8;
        let mut a = vec![0u8; len];
        a[0..4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        a[4..8].copy_from_slice(&(len as u32).to_le_bytes());
        a[8] = 1;
        a[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        a[0x28..0x30].copy_from_slice(&alloc.to_le_bytes());
        a[0x30..0x38].copy_from_slice(&size.to_le_bytes());
        a[0x40..0x40 + runs.len()].copy_from_slice(runs);
        a
    }

    #[test]
    fn boot_sector() {
        let mut b = vec![0u8; 512];
        b[3..11].copy_from_slice(b"NTFS    ");
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 8;
        b[0x30..0x38].copy_from_slice(&786_432u64.to_le_bytes());
        b[0x40] = 0xF6; // -10: 2^10 = 1024 bytes per record
        assert_eq!(
            parse_boot(&b),
            Some(Boot { bytes_per_cluster: 4096, mft_cluster: 786_432, record_size: 1024 })
        );
        b[3] = b'X';
        assert_eq!(parse_boot(&b), None);
        assert_eq!(parse_boot(&[]), None);
    }

    #[test]
    fn data_runs() {
        // 0x21: 1-byte length, 2-byte offset. Runs: 0x18 clusters at 0x5634,
        // then 0x10 clusters at -0x10 relative, then a sparse run, end.
        let runs = [0x21, 0x18, 0x34, 0x56, 0x21, 0x10, 0xF0, 0xFF, 0x01, 0x05, 0x00];
        assert_eq!(decode_runs(&runs), Some(vec![(0x5634, 0x18), (0x5624, 0x10)]));
        assert_eq!(decode_runs(&[0x21, 0x18]), None); // cut off
        assert_eq!(decode_runs(&[0x09, 0x00]), None); // length size 9
    }

    #[test]
    fn records_with_fixups() {
        let t = (1_795_910_400i64 + 11_644_473_600) * 10_000_000;
        let mut rec = record(
            1,
            0,
            &[
                std_info(t),
                file_name(5, "FOTO~1.JPG", 2),
                file_name(5, "Foto vom Urlaub.jpg", 1),
                non_resident_data(8192, 5000, &[0x11, 0x02, 0x20, 0x00]),
            ],
        );
        assert!(apply_fixups(&mut rec));
        let r = parse_record(&rec, 42).unwrap();
        assert!(r.in_use && !r.is_dir);
        assert_eq!(r.base, 42);
        assert_eq!(String::from_utf16(&r.name).unwrap(), "Foto vom Urlaub.jpg");
        assert_eq!(r.parent, Some(5));
        assert_eq!((r.size, r.allocated), (5000, 8192));
        assert_eq!(r.modified, 1_795_910_400);
        assert_eq!(r.data_runs, Some(vec![(0x20, 2)]));
        assert!(!r.online_only);

        // A OneDrive placeholder (RECALL_ON_DATA_ACCESS) is only online.
        let mut info = std_info(t);
        let content_at = usize::from(u16::from_le_bytes([info[0x14], info[0x15]]));
        info[content_at + 0x20..content_at + 0x24].copy_from_slice(&0x40_0000u32.to_le_bytes());
        let mut cloud = record(1, 0, &[info, file_name(5, "Video.mp4", 1)]);
        assert!(apply_fixups(&mut cloud));
        assert!(parse_record(&cloud, 43).unwrap().online_only);

        // A torn record (stride end does not match) is refused.
        let mut torn = record(1, 0, &[]);
        torn[510] ^= 1;
        assert!(!apply_fixups(&mut torn));

        // An extension record points to its base.
        let mut ext = record(1, 77, &[]);
        assert!(apply_fixups(&mut ext));
        assert_eq!(parse_record(&ext, 900).unwrap().base, 77);
    }

    #[test]
    fn broken_records_never_panic() {
        let mut good = record(3, 0, &[std_info(1), file_name(5, "Ordner", 1)]);
        assert!(apply_fixups(&mut good));
        for len in 0..good.len() {
            let _ = parse_record(&good[..len], 1);
        }
        for pos in 0..200 {
            let mut b = good.clone();
            b[pos] ^= 0xFF;
            let _ = parse_record(&b, 1);
            let _ = apply_fixups(&mut b);
        }
    }
}
