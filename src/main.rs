//! Start of the program: the window, or with `--scan PATH` a short
//! summary on the command line.

// No console window behind the program on Windows (release builds).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use mrsizeme::i18n::{self, Lang};
use mrsizeme::{applog, format, scan, tree};

fn main() -> ExitCode {
    i18n::set_lang(Lang::from_system());
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("MrSizeMe {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|a| a == "--scan") {
        let root = args.get(1).map_or_else(|| PathBuf::from("."), PathBuf::from);
        return cli_scan(root);
    }

    applog::init();
    log::info!("MrSizeMe {} started", env!("CARGO_PKG_VERSION"));
    let start = args.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MrSizeMe")
            .with_app_id("mrsizeme")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 420.0]),
        ..Default::default()
    };
    let result =
        eframe::run_native("MrSizeMe", options, Box::new(|_cc| Ok(Box::new(mrsizeme::ui::App::new(start)))));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("window could not be opened: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cli_scan(root: PathBuf) -> ExitCode {
    let progress = scan::Progress::default();
    let started = Instant::now();
    let tree = match scan::scan(&root, &scan::Options::default(), &progress) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {}", root.display(), format::io_error(&e));
            return ExitCode::FAILURE;
        }
    };
    let r = tree.root();
    println!(
        "{}: {} ({}), {} / {} / {} / {:.2} s",
        tree.root_path.display(),
        format::size(r.size),
        format::size(r.allocated),
        format::count(r.files),
        format::count(r.dirs),
        tree.errors.len(),
        started.elapsed().as_secs_f64()
    );
    for &c in tree.children(tree::ROOT).iter().take(20) {
        let n = tree.node(c);
        println!("{:>12}  {:>7}  {}", format::size(n.size), format::percent(n.size, r.size), n.name_lossy());
    }
    ExitCode::SUCCESS
}
