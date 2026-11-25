//! Start of the program: the window, or with `--scan PATH` the command line
//! mode (see `--help`).

// No console window behind the program on Windows (release builds).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mrsizeme::i18n::{self, Lang};
use mrsizeme::{applog, cli};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    i18n::set_lang(Lang::from_system());
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("MrSizeMe {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    // Helper mode with admin rights: only scan and hand the result back.
    if args.first().is_some_and(|a| a == mrsizeme::elevate::HELPER_ARG) {
        let (Some(root), Some(out)) = (args.get(1), args.get(2)) else { return ExitCode::from(2) };
        let code = mrsizeme::elevate::run_helper(Path::new(root), Path::new(out));
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    if args.first().is_some_and(|a| a == "--scan" || a == "--help" || a == "-h") {
        attach_console();
        return match cli::Args::parse(&args) {
            Ok(cli::Command::Help) => {
                println!("{}", cli::HELP);
                ExitCode::SUCCESS
            }
            Ok(cli::Command::Scan(a)) => cli::run(&a),
            Err(e) => {
                eprintln!("{e}\n\n{}", cli::HELP);
                ExitCode::from(2)
            }
        };
    }

    applog::init();
    log::info!("MrSizeMe {} started", env!("CARGO_PKG_VERSION"));
    let start = args.first().map(PathBuf::from);
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("MrSizeMe")
        .with_app_id("mrsizeme")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([720.0, 420.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/mrsizeme-256.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("window icon could not be loaded: {e}"),
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
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

/// Windows: a program without a console window prints into the console it
/// was started from.
fn attach_console() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        // SAFETY: plain call; fails harmlessly if there is no console.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}
