//! Export of a scan as CSV (semicolon separated, as spreadsheet programs
//! with German settings expect it).

use std::io::{self, Write};

use crate::format;
use crate::tree::{Kind, NodeId, Tree};

/// Writes all folders down to `max_depth` levels below `start` (0 = only
/// `start`); with `with_files` also the files.
pub fn csv(
    tree: &Tree,
    start: NodeId,
    max_depth: usize,
    with_files: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    let t = crate::i18n::tr();
    out.write_all("\u{feff}".as_bytes())?; // BOM, so spreadsheet programs read UTF-8
    writeln!(
        out,
        "{};{};{};{};{};{};{}",
        t.col_path,
        t.col_type,
        t.csv_size_bytes,
        t.csv_allocated_bytes,
        t.col_files,
        t.col_folders,
        t.col_modified
    )?;
    let mut stack = vec![(start, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let n = tree.node(id);
        let kind = match n.kind {
            Kind::Dir => t.kind_folder,
            Kind::File => t.kind_file,
            Kind::Link => t.kind_link,
            Kind::Other => t.kind_other,
        };
        writeln!(
            out,
            "{};{kind};{};{};{};{};{}",
            field(&tree.path(id).to_string_lossy()),
            n.size,
            n.allocated,
            n.files,
            n.dirs,
            format::iso_date(n.modified),
        )?;
        if depth < max_depth {
            for &c in tree.children(id).iter().rev() {
                if with_files || tree.node(c).is_dir() {
                    stack.push((c, depth + 1));
                }
            }
        }
    }
    out.flush()
}

/// One CSV field: quoted when needed. A leading = + - @ (or tab/CR) is
/// neutralised with an apostrophe so a spreadsheet does not run it as a formula.
fn field(text: &str) -> String {
    let mut s = String::with_capacity(text.len() + 2);
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        s.push('\'');
    }
    s.push_str(text);
    if s.contains([';', '"', '\n', '\r', '\'']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn fields_are_quoted_and_formulas_neutralised() {
        assert_eq!(field("abc"), "abc");
        assert_eq!(field("a;b"), "\"a;b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("=SUM(A1)"), "\"'=SUM(A1)\"");
        assert_eq!(field("zeile\numbruch"), "\"zeile\numbruch\"");
    }

    #[test]
    fn writes_rows_in_tree_order() {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let mut f = Node::new(OsStr::new("f.txt").into(), NO_PARENT, Kind::File);
        f.size = 7;
        let first = t.add_children(ROOT, [Node::new(OsStr::new("sub").into(), NO_PARENT, Kind::Dir), f]);
        let mut g = Node::new(OsStr::new("g").into(), NO_PARENT, Kind::File);
        g.size = 3;
        t.add_children(first, [g]);
        t.sum_up();

        let mut buf = Vec::new();
        csv(&t, ROOT, 9, false, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[1].contains(";Ordner;10;"));
        assert!(lines[2].contains("sub;Ordner;3;"));

        let mut buf = Vec::new();
        csv(&t, ROOT, 9, true, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap().lines().count(), 5);
    }
}
