//! The command line mode: scan a folder and print or save the result.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::{export, format, report, scan, tree};

pub const HELP: &str = "\
MrSizeMe – shows which folders and files use the space on your disks.

Usage:
  mrsizeme [FOLDER]                 open the window (and scan FOLDER)
  mrsizeme --scan FOLDER [OPTIONS]  scan on the command line

Options:
  --format text|json|csv|html|xlsx  kind of output (default: text)
  --out FILE                        write to FILE instead of the screen
                                    (needed for xlsx)
  --depth N                         levels below FOLDER (default: text 1,
                                    json/csv/xlsx all)
  --files                           list files too, not only folders
  --top N                           text: number of entries (default 20)
  --help                            this help
  --version                         program version

Exit codes: 0 done, 1 error, 2 wrong options.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
    Csv,
    Html,
    Xlsx,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub root: PathBuf,
    pub format: Format,
    pub out: Option<PathBuf>,
    pub depth: Option<usize>,
    pub files: bool,
    pub top: usize,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Scan(Args),
}

impl Args {
    /// Reads the options (the first argument is `--scan` or `--help`).
    pub fn parse(args: &[OsString]) -> Result<Command, String> {
        let mut it = args.iter();
        match it.next().and_then(|a| a.to_str()) {
            Some("--help" | "-h") => return Ok(Command::Help),
            Some("--scan") => {}
            _ => return Err("expected --scan".into()),
        }
        let mut a = Args {
            root: PathBuf::new(),
            format: Format::Text,
            out: None,
            depth: None,
            files: false,
            top: 20,
        };
        let mut root = None;
        let number = |v: Option<&OsString>, name: &str| -> Result<usize, String> {
            v.and_then(|v| v.to_str())
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("{name} needs a number"))
        };
        while let Some(arg) = it.next() {
            match arg.to_str() {
                Some("--help" | "-h") => return Ok(Command::Help),
                Some("--files") => a.files = true,
                Some("--format") => {
                    a.format = match it.next().and_then(|v| v.to_str()) {
                        Some("text") => Format::Text,
                        Some("json") => Format::Json,
                        Some("csv") => Format::Csv,
                        Some("html") => Format::Html,
                        Some("xlsx") => Format::Xlsx,
                        _ => return Err("--format needs text, json, csv, html or xlsx".into()),
                    }
                }
                Some("--out") => a.out = Some(it.next().map(PathBuf::from).ok_or("--out needs a file name")?),
                Some("--depth") => a.depth = Some(number(it.next(), "--depth")?),
                Some("--top") => a.top = number(it.next(), "--top")?,
                Some(s) if s.starts_with("--") => return Err(format!("unknown option {s}")),
                _ if root.is_none() => root = Some(PathBuf::from(arg)),
                _ => return Err("only one folder can be given".into()),
            }
        }
        a.root = root.unwrap_or_else(|| PathBuf::from("."));
        if a.format == Format::Xlsx && a.out.is_none() {
            return Err("xlsx needs --out FILE".into());
        }
        Ok(Command::Scan(a))
    }
}

/// Writes the result in the chosen form.
pub fn write(tree: &tree::Tree, a: &Args, secs: f64, out: &mut dyn Write) -> io::Result<()> {
    let all = usize::MAX;
    match a.format {
        Format::Json => {
            let v = report::json(tree, a.depth.unwrap_or(all), a.files);
            serde_json::to_writer_pretty(&mut *out, &v)?;
            writeln!(out)?;
        }
        Format::Csv => export::csv(tree, tree::ROOT, a.depth.unwrap_or(all), a.files, out)?,
        Format::Html => report::html(tree, crate::query::now(), out)?,
        Format::Xlsx => report::xlsx(tree, tree::ROOT, a.depth.unwrap_or(all), a.files, out)?,
        Format::Text => {
            let r = tree.root();
            writeln!(
                out,
                "{}: {} ({}), {} files, {} folders, {} errors, {:.2} s",
                tree.root_path.display(),
                format::size(r.size),
                format::size(r.allocated),
                format::count(r.files),
                format::count(r.dirs),
                tree.errors.len(),
                secs
            )?;
            let depth = a.depth.unwrap_or(1).max(1);
            let rows = report::rows(tree, tree::ROOT, depth, a.files, a.top.saturating_add(1));
            for row in rows.iter().skip(1) {
                let rel = std::path::Path::new(&row.path)
                    .strip_prefix(&tree.root_path)
                    .map_or_else(|_| row.path.clone(), |p| p.display().to_string());
                writeln!(
                    out,
                    "{:>12}  {:>7}  {rel}",
                    format::size(row.size),
                    format::percent(row.size, r.size)
                )?;
            }
        }
    }
    out.flush()
}

pub fn run(a: &Args) -> ExitCode {
    let started = std::time::Instant::now();
    let mut tree = match scan::scan(&a.root, &scan::Options::default(), &scan::Progress::default()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {}", a.root.display(), format::io_error(&e));
            return ExitCode::FAILURE;
        }
    };
    tree.sort(tree::SortKey::Size, false);
    let secs = started.elapsed().as_secs_f64();
    let result = match &a.out {
        Some(path) => {
            std::fs::File::create(path).and_then(|f| write(&tree, a, secs, &mut io::BufWriter::new(f)))
        }
        None => write(&tree, a, secs, &mut io::stdout().lock()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // The reader stopped early (for example `| head`): not an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            let target = a.out.as_ref().map_or_else(|| "stdout".to_string(), |p| p.display().to_string());
            eprintln!("{target}: {}", format::io_error(&e));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn options() {
        assert_eq!(Args::parse(&args(&["--help"])), Ok(Command::Help));
        let Ok(Command::Scan(a)) = Args::parse(&args(&[
            "--scan", "/x", "--format", "json", "--depth", "2", "--files", "--out", "o.json",
        ])) else {
            panic!()
        };
        assert_eq!(a.root, PathBuf::from("/x"));
        assert_eq!((a.format, a.depth, a.files), (Format::Json, Some(2), true));
        assert_eq!(a.out, Some(PathBuf::from("o.json")));
        let Ok(Command::Scan(d)) = Args::parse(&args(&["--scan"])) else { panic!() };
        assert_eq!((d.root, d.format, d.top), (PathBuf::from("."), Format::Text, 20));
        assert!(Args::parse(&args(&["--scan", "/x", "--format", "pdf"])).is_err());
        assert!(Args::parse(&args(&["--scan", "/x", "--depth", "zwei"])).is_err());
        assert!(Args::parse(&args(&["--scan", "/x", "/y"])).is_err());
        assert!(Args::parse(&args(&["--scan", "/x", "--format", "xlsx"])).is_err());
        assert!(Args::parse(&args(&["--scan", "--wat"])).is_err());
    }
}
