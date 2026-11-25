//! The update windows: first question, check, download and install.

use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

use eframe::egui;

use crate::external::Outcome;
use crate::i18n::tr;
use crate::settings::Settings;
use crate::update::{self, Package, Release};

#[derive(Default)]
pub struct State {
    check: Option<(bool, JoinHandle<io::Result<Option<Release>>>)>,
    pub found: Option<Release>,
    download: Option<(Arc<update::Progress>, JoinHandle<io::Result<std::path::PathBuf>>)>,
    /// A note for the status line.
    pub notice: Option<String>,
    pub error: Option<String>,
    /// The program should close (the installer or the new program file runs).
    pub quit: bool,
}

impl State {
    /// Starts a check in the background. `manual`: report also "up to date" and errors.
    pub fn start_check(&mut self, settings: &Settings, manual: bool) {
        if self.check.is_some() || self.download.is_some() {
            return;
        }
        if settings.offline {
            if manual {
                self.notice = Some(tr().updates_offline.to_string());
            }
            return;
        }
        let offline = settings.offline;
        if let Ok(h) =
            std::thread::Builder::new().name("update check".into()).spawn(move || update::check(offline))
        {
            self.check = Some((manual, h));
        }
    }

    /// True while a check or download runs.
    pub fn busy(&self) -> bool {
        self.check.is_some() || self.download.is_some()
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some((manual, h)) = &self.check {
            if !h.is_finished() {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            let manual = *manual;
            let (_, h) = self.check.take().expect("checked");
            match h.join() {
                Ok(Ok(Some(r))) => self.found = Some(r),
                Ok(Ok(None)) if manual => {
                    self.notice = Some(tr().updates_none.replace("{v}", update::current_version()));
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    log::warn!("update check failed: {e}");
                    if manual {
                        self.error = Some(crate::format::io_error(&e));
                    }
                }
                Err(_) => log::warn!("update check: internal error"),
            }
        }
        if let Some((_, h)) = &self.download {
            if !h.is_finished() {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            let (_, h) = self.download.take().expect("checked");
            match h.join() {
                Ok(Ok(path)) => {
                    let p = update::package();
                    match update::install(&path, p) {
                        Ok(Outcome::Done) => {
                            self.found = None;
                            match p {
                                Package::Installer | Package::Portable => self.quit = true,
                                Package::Dmg => self.notice = Some(tr().updates_dmg_opened.to_string()),
                                Package::Archive => {
                                    self.notice = Some(tr().updates_archive_shown.to_string())
                                }
                            }
                        }
                        Ok(Outcome::Blocked) => {}
                        Err(e) => self.error = Some(crate::format::io_error(&e)),
                    }
                }
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Ok(Err(e)) => self.error = Some(crate::format::io_error(&e)),
                Err(_) => self.error = Some("update: internal error".into()),
            }
        }
    }
}

/// The question at the first start. Returns the answer once given.
pub fn ask_window(ctx: &egui::Context) -> Option<bool> {
    let t = tr();
    let mut answer = None;
    egui::Window::new(t.grp_updates)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .collapsible(false)
        .resizable(false)
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.label(t.updates_ask_text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(t.updates_ask_yes).clicked() {
                    answer = Some(true);
                }
                if ui.button(t.updates_ask_no).clicked() {
                    answer = Some(false);
                }
            });
        });
    answer
}

/// The window for a found update.
pub fn found_window(ctx: &egui::Context, state: &mut State, settings: &Settings) {
    let t = tr();
    let Some(release) = state.found.clone() else { return };
    let mut open = true;
    let mut later = false;
    egui::Window::new(t.grp_updates)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .default_width(480.0)
        .show(ctx, |ui| {
            ui.strong(
                t.updates_found
                    .replace("{new}", &release.version)
                    .replace("{cur}", update::current_version()),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.label(&release.notes);
            });
            ui.separator();
            if let Some((p, _)) = &state.download {
                let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).text(format!(
                    "{}  {} / {}",
                    t.updates_downloading,
                    crate::format::size(done),
                    crate::format::size(total)
                )));
                if ui.button(t.cancel).clicked() {
                    p.cancel();
                }
                return;
            }
            let has_key = update::public_key().is_some();
            if !has_key {
                ui.colored_label(ui.visuals().warn_fg_color, t.updates_no_key);
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(has_key, egui::Button::new(t.updates_install)).clicked()
                    && let Some(dir) = crate::settings::config_dir().map(|d| d.join("updates"))
                {
                    let progress = Arc::new(update::Progress::default());
                    let p = Arc::clone(&progress);
                    let r = release.clone();
                    let offline = settings.offline;
                    if let Ok(h) = std::thread::Builder::new()
                        .name("update download".into())
                        .spawn(move || update::download(&r, offline, &dir, &p))
                    {
                        state.download = Some((progress, h));
                    }
                }
                if !release.page.is_empty()
                    && ui.button(t.updates_page).clicked()
                    && let Err(e) = crate::external::open_url(&release.page)
                {
                    state.error = Some(crate::format::io_error(&e));
                }
                if ui.button(t.updates_later).clicked() {
                    later = true;
                }
            });
        });
    if (!open || later) && state.download.is_none() {
        state.found = None;
    }
}
