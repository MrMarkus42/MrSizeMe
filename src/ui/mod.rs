//! The program window.

mod folders;
mod lists;
mod map;
mod paint;
mod places;

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{CentralPanel, Panel};

use crate::i18n::{self, Lang, tr};
use crate::scan::{self, Progress};
use crate::stats::TypeStats;
use crate::tree::{NodeId, ROOT, SortKey, Tree};
use crate::{applog, export, external, format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Folders,
    Types,
    Largest,
}

struct Running {
    progress: Arc<Progress>,
    handle: JoinHandle<io::Result<Tree>>,
    root: PathBuf,
    started: Instant,
}

/// Something the user asked for while a list was drawn; done afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Select(NodeId),
    Toggle(NodeId),
    MapRoot(NodeId),
    OpenFolder(NodeId),
    CopyPath(NodeId),
    Sort(SortKey),
}

pub struct App {
    path_text: String,
    running: Option<Running>,
    tree: Option<Tree>,
    /// Counts up whenever the tree changes, so caches know when to refresh.
    generation: u64,
    selected: Option<NodeId>,
    expanded: HashSet<NodeId>,
    sort: (SortKey, bool),
    view: View,
    map_root: NodeId,
    use_allocated: bool,
    map_cache: Option<map::Cache>,
    types_cache: Option<(u64, NodeId, Vec<TypeStats>)>,
    largest_cache: Option<(u64, NodeId, Vec<NodeId>)>,
    error: Option<String>,
    notice: Option<(String, Instant)>,
    scroll_to_selected: bool,
    last_duration: Option<Duration>,
    places: Vec<places::Place>,
    title: String,
}

impl App {
    /// The window as it starts; scans `start` right away if given.
    pub fn new(start: Option<PathBuf>) -> Self {
        let mut app = Self::empty();
        if let Some(path) = start {
            app.path_text = path.display().to_string();
            app.start_scan(path);
        }
        app
    }

    /// A window that shows an existing scan (for tests and saved scans).
    pub fn with_tree(tree: Tree) -> Self {
        let mut app = Self::empty();
        app.path_text = tree.root_path.display().to_string();
        app.set_tree(tree);
        app
    }

    fn empty() -> Self {
        Self {
            path_text: String::new(),
            running: None,
            tree: None,
            generation: 0,
            selected: None,
            expanded: HashSet::new(),
            sort: (SortKey::Size, false),
            view: View::Folders,
            map_root: ROOT,
            use_allocated: false,
            map_cache: None,
            types_cache: None,
            largest_cache: None,
            error: None,
            notice: None,
            scroll_to_selected: false,
            last_duration: None,
            places: places::list(),
            title: String::new(),
        }
    }

    fn set_tree(&mut self, mut tree: Tree) {
        tree.sort(self.sort.0, self.sort.1);
        self.tree = Some(tree);
        self.generation += 1;
        self.selected = Some(ROOT);
        self.expanded.clear();
        self.expanded.insert(ROOT);
        self.map_root = ROOT;
        self.map_cache = None;
        self.types_cache = None;
        self.largest_cache = None;
    }

    fn start_scan(&mut self, path: PathBuf) {
        if let Some(r) = &self.running {
            r.progress.cancel();
        }
        let progress = Arc::new(Progress::default());
        let p = Arc::clone(&progress);
        let root = path.clone();
        let handle = std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || scan::scan(&root, &scan::Options::default(), &p))
            .expect("start scan thread");
        log::info!("scan started: {}", applog::private(&path));
        self.running = Some(Running { progress, handle, root: path, started: Instant::now() });
        self.tree = None;
        self.generation += 1;
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        let Some(r) = &self.running else { return };
        if !r.handle.is_finished() {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let Some(r) = self.running.take() else { return };
        let elapsed = r.started.elapsed();
        match r.handle.join() {
            Ok(Ok(tree)) => {
                log::info!(
                    "scan finished: {} entries, {} folders with errors, {:.2} s{}",
                    tree.len(),
                    tree.errors.len(),
                    elapsed.as_secs_f64(),
                    if tree.cancelled { " (cancelled)" } else { "" }
                );
                self.last_duration = Some(elapsed);
                self.set_tree(tree);
            }
            Ok(Err(e)) => {
                self.show_error(format!("{}\n{}", r.root.display(), format::io_error(&e)));
            }
            Err(_) => self.show_error("Scan: interner Fehler / internal error".into()),
        }
    }

    fn show_error(&mut self, text: String) {
        log::error!("{}", privatise(&text).replace('\n', " "));
        self.error = Some(text);
    }

    /// Selects a node, opens all folders above it and scrolls to it.
    fn reveal(&mut self, id: NodeId) {
        let Some(tree) = &self.tree else { return };
        for a in tree.ancestors(id) {
            if a != id {
                self.expanded.insert(a);
            }
        }
        self.selected = Some(id);
        self.scroll_to_selected = true;
    }

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        let Some(tree) = &self.tree else { return };
        match action {
            Action::Select(id) => self.selected = Some(id),
            Action::Toggle(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
            }
            Action::MapRoot(id) => {
                let n = tree.node(id);
                self.map_root = if n.is_dir() || n.parent == crate::tree::NO_PARENT { id } else { n.parent };
                self.reveal(id);
            }
            Action::OpenFolder(id) => {
                let n = tree.node(id);
                let dir = if n.is_dir() || n.parent == crate::tree::NO_PARENT { id } else { n.parent };
                let path = tree.path(dir);
                if let Err(e) = external::open_folder(&path) {
                    self.show_error(format!("{}\n{}", path.display(), format::io_error(&e)));
                }
            }
            Action::CopyPath(id) => ctx.copy_text(tree.path(id).display().to_string()),
            Action::Sort(key) => {
                self.sort = if self.sort.0 == key { (key, !self.sort.1) } else { (key, false) };
                if let Some(tree) = &mut self.tree {
                    tree.sort(self.sort.0, self.sort.1);
                }
            }
        }
    }

    fn export_csv(&mut self) {
        let Some(tree) = &self.tree else { return };
        let Some(path) =
            rfd::FileDialog::new().set_file_name("mrsizeme.csv").add_filter("CSV", &["csv"]).save_file()
        else {
            return;
        };
        let result = std::fs::File::create(&path).and_then(|f| {
            let mut w = io::BufWriter::new(f);
            export::csv(tree, ROOT, usize::MAX, false, &mut w)
        });
        match result {
            Ok(()) => self.notice = Some((tr().export_done.to_string(), Instant::now())),
            Err(e) => self.show_error(format!("{}\n{}", path.display(), format::io_error(&e))),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let t = tr();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut go = None;
            ui.menu_button(t.drives, |ui| {
                for place in &self.places {
                    if ui.button(&place.label).clicked() {
                        go = Some(place.path.clone());
                        ui.close();
                    }
                }
            });
            if ui.button(t.choose_folder).clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                go = Some(dir);
            }
            let field =
                egui::TextEdit::singleline(&mut self.path_text).hint_text(t.path_hint).desired_width(360.0);
            let resp = ui.add(field);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let busy = self.running.is_some();
            if busy {
                if ui.button(t.cancel).clicked()
                    && let Some(r) = &self.running
                {
                    r.progress.cancel();
                }
            } else {
                let label = if self.tree.is_some() { t.rescan } else { t.scan };
                if (ui.button(label).clicked() || enter) && !self.path_text.trim().is_empty() {
                    go = Some(PathBuf::from(self.path_text.trim()));
                }
            }
            if self.tree.is_some() && !busy && ui.button(t.export_csv).clicked() {
                self.export_csv();
            }
            if let Some(path) = go {
                self.path_text = path.display().to_string();
                self.start_scan(path);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                let mut lang = i18n::lang();
                egui::ComboBox::from_id_salt("lang")
                    .selected_text(lang.native_name())
                    .show_ui(ui, |ui| {
                        for l in Lang::ALL {
                            ui.selectable_value(&mut lang, l, l.native_name());
                        }
                    })
                    .response
                    .on_hover_text(t.language);
                if lang != i18n::lang() {
                    i18n::set_lang(lang);
                }
            });
        });
        ui.add_space(2.0);
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        let t = tr();
        ui.horizontal(|ui| {
            if let Some(r) = &self.running {
                ui.spinner();
                let p = &r.progress;
                ui.label(format!(
                    "{} … {} {}, {} {}, {}",
                    t.scanning,
                    format::count(p.files.load(Ordering::Relaxed)),
                    t.files,
                    format::count(p.dirs.load(Ordering::Relaxed)),
                    t.folders,
                    format::size(p.bytes.load(Ordering::Relaxed)),
                ));
                ui.add(
                    egui::Label::new(egui::RichText::new(applog::private(&p.current())).weak()).truncate(),
                );
            } else if let Some(tree) = &self.tree {
                let r = tree.root();
                let mut text = format!(
                    "{}  ·  {} ({} {})  ·  {} {}  ·  {} {}",
                    tree.root_path.display(),
                    format::size(r.size),
                    format::size(r.allocated),
                    t.col_allocated.to_lowercase(),
                    format::count(r.files),
                    t.files,
                    format::count(r.dirs),
                    t.folders
                );
                if !tree.errors.is_empty() {
                    text += &format!("  ·  {} {}", format::count(tree.errors.len() as u64), t.errors);
                }
                if let Some(d) = self.last_duration {
                    text += &format!("  ·  {:.1} {}", d.as_secs_f64(), t.seconds);
                }
                ui.add(egui::Label::new(text).truncate());
                if tree.cancelled {
                    ui.colored_label(ui.visuals().warn_fg_color, t.cancelled);
                }
            } else {
                ui.label(t.welcome);
            }
            if let Some((text, at)) = &self.notice {
                if at.elapsed() < Duration::from_secs(4) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(text));
                    ui.ctx().request_repaint_after(Duration::from_millis(500));
                } else {
                    self.notice = None;
                }
            }
        });
    }

    fn left(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let t = tr();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Folders, t.view_folders);
            ui.selectable_value(&mut self.view, View::Types, t.view_types);
            ui.selectable_value(&mut self.view, View::Largest, t.view_largest);
        });
        ui.separator();
        let Some(tree) = &self.tree else { return Vec::new() };
        let start = self.selected.filter(|&s| tree.node(s).is_dir()).unwrap_or(ROOT);
        match self.view {
            View::Folders => {
                let scroll = std::mem::take(&mut self.scroll_to_selected);
                folders::show(ui, tree, &self.expanded, self.selected, self.sort, scroll)
            }
            View::Types => {
                if self.types_cache.as_ref().is_none_or(|c| c.0 != self.generation || c.1 != start) {
                    self.types_cache = Some((self.generation, start, crate::stats::by_type(tree, start)));
                }
                let stats = &self.types_cache.as_ref().map(|c| &c.2);
                lists::types(ui, tree, start, stats.map_or(&[][..], |s| s.as_slice()));
                Vec::new()
            }
            View::Largest => {
                if self.largest_cache.as_ref().is_none_or(|c| c.0 != self.generation || c.1 != start) {
                    self.largest_cache =
                        Some((self.generation, start, crate::stats::largest_files(tree, start, 500)));
                }
                let files = self.largest_cache.as_ref().map_or(&[][..], |c| c.2.as_slice());
                lists::largest(ui, tree, files, self.selected)
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let Some(tree) = &self.tree else { return Vec::new() };
        if ctx.egui_wants_keyboard_input() || self.view != View::Folders {
            return Vec::new();
        }
        let rows = folders::visible_rows(tree, &self.expanded);
        let Some(sel) = self.selected else { return Vec::new() };
        let pos = rows.iter().position(|r| r.0 == sel).unwrap_or(0);
        let mut actions = Vec::new();
        ctx.input(|i| {
            let n = tree.node(sel);
            if i.key_pressed(egui::Key::ArrowDown) && pos + 1 < rows.len() {
                actions.push(Action::Select(rows[pos + 1].0));
            } else if i.key_pressed(egui::Key::ArrowUp) && pos > 0 {
                actions.push(Action::Select(rows[pos - 1].0));
            } else if i.key_pressed(egui::Key::ArrowRight)
                && n.child_count > 0
                && !self.expanded.contains(&sel)
            {
                actions.push(Action::Toggle(sel));
            } else if i.key_pressed(egui::Key::ArrowLeft) {
                if self.expanded.contains(&sel) && sel != ROOT {
                    actions.push(Action::Toggle(sel));
                } else if n.parent != crate::tree::NO_PARENT {
                    actions.push(Action::Select(n.parent));
                }
            } else if i.key_pressed(egui::Key::Enter) {
                actions.push(Action::MapRoot(sel));
            }
        });
        if !actions.is_empty() {
            self.scroll_to_selected = true;
        }
        actions
    }

    fn error_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.error else { return };
        let t = tr();
        let mut open = true;
        let mut close = false;
        egui::Window::new(t.error_title)
            .id(egui::Id::unique("error_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                // Read-only but selectable, so the text can be marked and copied.
                let mut shown = text.as_str();
                ui.add(egui::TextEdit::multiline(&mut shown).desired_width(f32::INFINITY).desired_rows(3));
                if let Some(log) = applog::log_file() {
                    ui.label(egui::RichText::new(applog::private(&log)).weak().small());
                }
                ui.horizontal(|ui| {
                    if ui.button(t.copy).clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if ui.button(t.close).clicked() {
                        close = true;
                    }
                });
            });
        if !open || close {
            self.error = None;
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.tree {
            Some(t) => format!("MrSizeMe – {}", t.root_path.display()),
            None => "MrSizeMe".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// The whole window; also used by the tests.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll_scan(&ctx);
        self.update_title(&ctx);
        let mut actions = self.keyboard(&ctx);

        Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        Panel::bottom("status").show(ui, |ui| self.status(ui));
        if self.tree.is_some() {
            let width = ui.available_width() * 0.5;
            let left = Panel::left("lists")
                .resizable(true)
                .default_size(width)
                .min_size(280.0)
                .show(ui, |ui| self.left(ui));
            actions.extend(left.inner);
        }
        CentralPanel::default_margins().show(ui, |ui| {
            if self.tree.is_some() {
                actions.extend(self.map(ui));
            } else {
                paint::welcome(ui, self.running.is_some());
            }
        });
        self.error_window(&ctx);
        for a in actions {
            self.apply(a, &ctx);
        }
    }

    fn map(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let t = tr();
        let Some(tree) = &self.tree else { return Vec::new() };
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.strong(t.map_title);
            let parent = tree.node(self.map_root).parent;
            if ui.add_enabled(parent != crate::tree::NO_PARENT, egui::Button::new(t.map_up)).clicked() {
                actions.push(Action::MapRoot(parent));
            }
            ui.selectable_value(&mut self.use_allocated, false, t.show_size);
            ui.selectable_value(&mut self.use_allocated, true, t.show_allocated);
            ui.add(
                egui::Label::new(egui::RichText::new(tree.path(self.map_root).display().to_string()).weak())
                    .truncate(),
            );
        });
        actions.extend(map::show(
            ui,
            tree,
            self.map_root,
            self.selected,
            self.use_allocated,
            self.generation,
            &mut self.map_cache,
        ));
        actions
    }
}

/// Replaces the home folder in a message with "~" (for the log).
fn privatise(text: &str) -> String {
    let home = home_or_empty();
    if home.as_os_str().len() < 2 { text.to_string() } else { text.replace(&home.display().to_string(), "~") }
}

fn home_or_empty() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).unwrap_or_default()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
