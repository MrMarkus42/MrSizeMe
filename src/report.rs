//! Further exports: Excel workbook (.xlsx), HTML report and JSON.
//!
//! All names come from the disk and are treated as hostile text: they are
//! escaped for XML and HTML, control characters are removed, and in the
//! workbook they are plain text cells (never formulas).

use std::io::{self, Write};

use crate::format;
use crate::i18n::tr;
use crate::tree::{Kind, NodeId, ROOT, Tree};

/// Most rows a worksheet can hold.
const MAX_ROWS: usize = 1_048_575;

/// One line of the table exports.
pub struct Row {
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
    pub modified: i64,
}

/// Folders (and files if wanted) down to `max_depth` levels, in tree order.
pub fn rows(tree: &Tree, start: NodeId, max_depth: usize, with_files: bool, limit: usize) -> Vec<Row> {
    let mut out = Vec::new();
    let mut stack = vec![(start, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let n = tree.node(id);
        out.push(Row {
            path: tree.path(id).to_string_lossy().into_owned(),
            kind: n.kind,
            size: n.size,
            allocated: n.allocated,
            files: n.files,
            dirs: n.dirs,
            modified: n.modified,
        });
        if depth < max_depth {
            for &c in tree.children(id).iter().rev() {
                if with_files || tree.node(c).is_dir() {
                    stack.push((c, depth + 1));
                }
            }
        }
    }
    out
}

fn kind_name(k: Kind) -> &'static str {
    let t = tr();
    match k {
        Kind::Dir => t.kind_folder,
        Kind::File => t.kind_file,
        Kind::Link => t.kind_link,
        Kind::Other => t.kind_other,
    }
}

/// Text for XML and HTML: special characters escaped, control characters removed.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\t' | '\n' => out.push(' '),
            c if c.is_control() || c == '\u{fffe}' || c == '\u{ffff}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

// ------------------------------------------------------------------- xlsx

/// A small ZIP writer (files stored without compression), enough for .xlsx.
struct Zip<'a> {
    out: &'a mut dyn Write,
    offset: u32,
    central: Vec<u8>,
    count: u16,
}

impl<'a> Zip<'a> {
    fn new(out: &'a mut dyn Write) -> Self {
        Zip { out, offset: 0, central: Vec::new(), count: 0 }
    }

    fn file(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let too_big = || io::Error::other("export too large");
        let crc = crc32fast::hash(data);
        let len = u32::try_from(data.len()).map_err(|_| too_big())?;
        let name_len = name.len() as u16;
        // Fixed date 1980-01-01, so the same scan gives the same file.
        let (time, date) = (0u16, 0x21u16);
        let mut head = Vec::with_capacity(30 + name.len());
        head.extend(0x0403_4b50u32.to_le_bytes());
        head.extend(20u16.to_le_bytes()); // version needed
        head.extend(0x0800u16.to_le_bytes()); // UTF-8 names
        head.extend(0u16.to_le_bytes()); // stored
        head.extend(time.to_le_bytes());
        head.extend(date.to_le_bytes());
        head.extend(crc.to_le_bytes());
        head.extend(len.to_le_bytes());
        head.extend(len.to_le_bytes());
        head.extend(name_len.to_le_bytes());
        head.extend(0u16.to_le_bytes());
        head.extend(name.as_bytes());
        self.out.write_all(&head)?;
        self.out.write_all(data)?;

        let c = &mut self.central;
        c.extend(0x0201_4b50u32.to_le_bytes());
        c.extend(20u16.to_le_bytes()); // made by
        c.extend(20u16.to_le_bytes()); // needed
        c.extend(0x0800u16.to_le_bytes());
        c.extend(0u16.to_le_bytes());
        c.extend(time.to_le_bytes());
        c.extend(date.to_le_bytes());
        c.extend(crc.to_le_bytes());
        c.extend(len.to_le_bytes());
        c.extend(len.to_le_bytes());
        c.extend(name_len.to_le_bytes());
        c.extend([0u8; 12]); // extra, comment, disk, internal and external attributes
        c.extend(self.offset.to_le_bytes());
        c.extend(name.as_bytes());
        self.offset = self
            .offset
            .checked_add(head.len() as u32)
            .and_then(|o| o.checked_add(len))
            .ok_or_else(too_big)?;
        self.count += 1;
        Ok(())
    }

    fn finish(self) -> io::Result<()> {
        self.out.write_all(&self.central)?;
        let mut end = Vec::with_capacity(22);
        end.extend(0x0605_4b50u32.to_le_bytes());
        end.extend([0u8; 4]);
        end.extend(self.count.to_le_bytes());
        end.extend(self.count.to_le_bytes());
        end.extend((self.central.len() as u32).to_le_bytes());
        end.extend(self.offset.to_le_bytes());
        end.extend(0u16.to_le_bytes());
        self.out.write_all(&end)?;
        self.out.flush()
    }
}

fn cell_text(s: &str) -> String {
    format!("<c t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", escape(s))
}

fn cell_num(v: u64) -> String {
    format!("<c><v>{v}</v></c>")
}

/// Writes an Excel workbook with one sheet.
pub fn xlsx(
    tree: &Tree,
    start: NodeId,
    max_depth: usize,
    with_files: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    let t = tr();
    let rows = rows(tree, start, max_depth, with_files, MAX_ROWS);
    let mut sheet = String::with_capacity(200 + rows.len() * 260);
    sheet.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet \
         xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><cols><col min=\"1\" max=\"1\" \
         width=\"80\" customWidth=\"1\"/><col min=\"2\" max=\"8\" width=\"16\" customWidth=\"1\"/></cols><sheetData>",
    );
    let header = [
        t.col_path,
        t.col_type,
        t.csv_size_bytes,
        t.csv_allocated_bytes,
        t.col_files,
        t.col_folders,
        t.col_modified,
    ];
    sheet.push_str("<row>");
    for h in header {
        sheet.push_str(&cell_text(h));
    }
    sheet.push_str("</row>");
    for r in &rows {
        sheet.push_str("<row>");
        sheet.push_str(&cell_text(&r.path));
        sheet.push_str(&cell_text(kind_name(r.kind)));
        for v in [r.size, r.allocated, r.files, r.dirs] {
            sheet.push_str(&cell_num(v));
        }
        sheet.push_str(&cell_text(&format::iso_date(r.modified)));
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");

    let mut zip = Zip::new(out);
    zip.file(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
    )?;
    zip.file(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
    )?;
    zip.file(
        "xl/workbook.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MrSizeMe" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
    )?;
    zip.file(
        "xl/_rels/workbook.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
    )?;
    zip.file("xl/worksheets/sheet1.xml", sheet.as_bytes())?;
    zip.finish()
}

// ------------------------------------------------------------------- html

fn bar(part: u64, whole: u64) -> String {
    let pct = format::share(part, whole) * 100.0;
    format!("<span class=\"bar\"><span style=\"width:{pct:.1}%\"></span></span>")
}

/// Writes a self-contained HTML report (no scripts, no outside files).
pub fn html(tree: &Tree, scanned_at: i64, out: &mut dyn Write) -> io::Result<()> {
    let t = tr();
    let r = tree.root();
    let total = r.size.max(1);
    let mut h = String::new();
    let lang = if t.numbers.decimal == ',' { "de" } else { "en" };
    h.push_str(&format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>MrSizeMe – {}</title><style>\
         body{{font-family:system-ui,sans-serif;margin:2rem auto;max-width:1000px;padding:0 1rem;color:#1f2937;background:#fff}}\
         h1{{font-size:1.5rem}}h2{{font-size:1.15rem;margin-top:2rem;border-bottom:1px solid #e5e7eb}}\
         table{{border-collapse:collapse;width:100%}}td,th{{padding:.25rem .5rem;text-align:left;border-bottom:1px solid #f3f4f6}}\
         td.n,th.n{{text-align:right;white-space:nowrap}}.bar{{display:inline-block;width:120px;height:.7rem;background:#e5e7eb;border-radius:3px}}\
         .bar span{{display:block;height:100%;background:#14b8a6;border-radius:3px}}.muted{{color:#6b7280}}\
         @media (prefers-color-scheme:dark){{body{{background:#111827;color:#e5e7eb}}td,th{{border-color:#1f2937}}.bar{{background:#374151}}}}\
         </style></head><body>",
        escape(&tree.root_path.display().to_string())
    ));
    h.push_str(&format!("<h1>{}</h1>", escape(&tree.root_path.display().to_string())));
    h.push_str(&format!(
        "<p>{} ({} {}) · {} {} · {} {} · <span class=\"muted\">{} {}</span></p>",
        format::size(r.size),
        format::size(r.allocated),
        escape(&t.col_allocated.to_lowercase()),
        format::count(r.files),
        escape(t.files),
        format::count(r.dirs),
        escape(t.folders),
        escape(t.report_created),
        format::date_time(scanned_at),
    ));

    // Largest entries of the scanned folder.
    h.push_str(&format!("<h2>{}</h2><table>", escape(t.view_folders)));
    h.push_str(&format!(
        "<tr><th>{}</th><th class=\"n\">{}</th><th class=\"n\">{}</th><th></th></tr>",
        escape(t.col_name),
        escape(t.col_size),
        escape(t.col_share)
    ));
    let mut kids: Vec<NodeId> = tree.children(ROOT).to_vec();
    kids.sort_by_key(|&c| std::cmp::Reverse(tree.node(c).size));
    for &c in kids.iter().take(40) {
        let n = tree.node(c);
        let slash = if n.is_dir() { "/" } else { "" };
        h.push_str(&format!(
            "<tr><td>{}{slash}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
            escape(&n.name_lossy()),
            format::size(n.size),
            format::percent(n.size, total),
            bar(n.size, total)
        ));
    }
    h.push_str("</table>");

    // Categories and file types.
    h.push_str(&format!("<h2>{}</h2><table>", escape(t.view_categories)));
    for s in crate::category::by_category(tree, ROOT) {
        h.push_str(&format!(
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
            escape(t.categories[s.category as usize]),
            format::count(s.files),
            format::size(s.size),
            bar(s.size, total)
        ));
    }
    h.push_str("</table>");
    h.push_str(&format!("<h2>{}</h2><table>", escape(t.view_types)));
    for s in crate::stats::by_type(tree, ROOT).iter().take(25) {
        let ext =
            if s.extension.is_empty() { t.no_extension.to_string() } else { format!(".{}", s.extension) };
        h.push_str(&format!(
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
            escape(&ext),
            format::count(s.files),
            format::size(s.size),
            bar(s.size, total)
        ));
    }
    h.push_str("</table>");

    // Largest files.
    h.push_str(&format!("<h2>{}</h2><table>", escape(t.view_largest)));
    for id in crate::stats::largest_files(tree, ROOT, 50) {
        let p = tree.path(id);
        let shown = p.strip_prefix(&tree.root_path).unwrap_or(&p).display().to_string();
        h.push_str(&format!(
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n muted\">{}</td></tr>",
            escape(&shown),
            format::size(tree.node(id).size),
            format::date(tree.node(id).modified)
        ));
    }
    h.push_str("</table>");

    // Clean-up suggestions.
    let tips = crate::knowledge::suggestions(tree, ROOT, 50 << 20);
    if !tips.is_empty() {
        h.push_str(&format!("<h2>{}</h2><table>", escape(t.view_cleanup)));
        for s in tips.iter().take(40) {
            let title = crate::knowledge::by_id(s.entry).map_or("", |e| e.text().0);
            let p = tree.path(s.id);
            let shown = p.strip_prefix(&tree.root_path).unwrap_or(&p).display().to_string();
            h.push_str(&format!(
                "<tr><td>{}</td><td class=\"muted\">{}</td><td class=\"n\">{}</td></tr>",
                escape(title),
                escape(&shown),
                format::size(s.size)
            ));
        }
        h.push_str("</table>");
    }
    h.push_str(&format!("<p class=\"muted\">MrSizeMe {}</p></body></html>\n", env!("CARGO_PKG_VERSION")));
    out.write_all(h.as_bytes())?;
    out.flush()
}

// ------------------------------------------------------------------- json

/// The tree as JSON, down to `max_depth` levels (files only if wanted).
pub fn json(tree: &Tree, max_depth: usize, with_files: bool) -> serde_json::Value {
    fn node(tree: &Tree, id: NodeId, depth: usize, max_depth: usize, with_files: bool) -> serde_json::Value {
        let n = tree.node(id);
        let kind = match n.kind {
            Kind::Dir => "folder",
            Kind::File => "file",
            Kind::Link => "link",
            Kind::Other => "other",
        };
        let mut v = serde_json::json!({
            "name": n.name_lossy(),
            "kind": kind,
            "size": n.size,
            "allocated": n.allocated,
        });
        if n.is_dir() {
            v["files"] = n.files.into();
            v["folders"] = n.dirs.into();
        }
        if n.modified != crate::tree::NO_TIME {
            v["modified"] = format::iso_date(n.modified).into();
        }
        if n.is_dir() && depth < max_depth {
            let kids: Vec<serde_json::Value> = tree
                .children(id)
                .iter()
                .filter(|&&c| with_files || tree.node(c).is_dir())
                .map(|&c| node(tree, c, depth + 1, max_depth, with_files))
                .collect();
            v["children"] = kids.into();
        }
        v
    }
    let mut v = node(tree, ROOT, 0, max_depth, with_files);
    v["path"] = tree.root_path.to_string_lossy().into();
    v["errors"] = tree.errors.len().into();
    v["cancelled"] = tree.cancelled.into();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_PARENT, Node};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn sample() -> Tree {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let mut f = Node::new(OsStr::new("<b>&\"böse\".exe").into(), NO_PARENT, Kind::File);
        f.size = 1000;
        let mut g = Node::new(OsStr::new("=SUMME(A1)").into(), NO_PARENT, Kind::File);
        g.size = 10;
        let first = t.add_children(ROOT, [Node::new(OsStr::new("sub").into(), NO_PARENT, Kind::Dir), g]);
        t.add_children(first, [f]);
        t.sum_up();
        t
    }

    #[test]
    fn escaping() {
        assert_eq!(escape("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
        assert_eq!(escape("a\u{1}b\tc"), "a\u{fffd}b c");
    }

    #[test]
    fn workbook_is_a_valid_zip() {
        let mut buf = Vec::new();
        xlsx(&sample(), ROOT, usize::MAX, true, &mut buf).unwrap();
        // Local header first, end record last, five files.
        assert_eq!(&buf[..4], b"PK\x03\x04");
        let end = &buf[buf.len() - 22..];
        assert_eq!(&end[..4], b"PK\x05\x06");
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 5);
        let text = String::from_utf8_lossy(&buf);
        // Names are escaped and stay text, never formulas.
        assert!(text.contains("&lt;b&gt;&amp;&quot;böse&quot;.exe"));
        assert!(text.contains("<t xml:space=\"preserve\">/x/=SUMME(A1)</t>"));
        assert!(!text.contains("<f>"));
        // The central directory points to where the files really are.
        let cd_offset = u32::from_le_bytes(end[16..20].try_into().unwrap()) as usize;
        assert_eq!(&buf[cd_offset..cd_offset + 4], b"PK\x01\x02");
    }

    #[test]
    fn html_report_is_escaped_and_without_scripts() {
        let mut buf = Vec::new();
        html(&sample(), 0, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("&lt;b&gt;&amp;&quot;böse&quot;.exe"));
        assert!(!text.contains("<b>&"));
        assert!(!text.to_lowercase().contains("<script"));
        assert!(text.contains("default-src 'none'"));
    }

    #[test]
    fn json_tree() {
        let v = json(&sample(), 1, false);
        assert_eq!(v["size"], 1010);
        assert_eq!(v["children"].as_array().unwrap().len(), 1);
        assert!(v["children"][0].get("children").is_none());
        let all = json(&sample(), usize::MAX, true);
        assert_eq!(all["children"][0]["children"][0]["name"], "<b>&\"böse\".exe");
    }
}
