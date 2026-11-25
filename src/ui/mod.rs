//! The program window.

mod cleanup;
mod folders;
mod history_view;
mod icons;
mod lists;
mod map;
mod paint;
mod places;
mod ribbon;
mod ring;
mod tools;

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{CentralPanel, Panel};

use crate::category::CategoryStats;
use crate::i18n::{self, Lang, tr};
use crate::query::{AgeStats, Filter, Hits};
use crate::scan::{self, Progress};
use crate::stats::TypeStats;
use crate::tree::{Measure, NodeId, ROOT, SortKey, Tree};
use crate::{applog, export, external, format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Folders,
    Types,
    Categories,
    Largest,
    Oldest,
    Age,
    Search,
    Cleanup,
    History,
    Duplicates,
    Cloud,
    Apps,
}

/// The chart on the right side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chart {
    Map,
    Ring,
}

/// A computed list that stays valid while the tree and the start folder stay the same.
struct Cached<T> {
    generation: u64,
    start: NodeId,
    value: T,
}

fn refresh<T>(slot: &mut Option<Cached<T>>, generation: u64, start: NodeId, make: impl FnOnce() -> T) -> &T {
    if !slot.as_ref().is_some_and(|c| c.generation == generation && c.start == start) {
        *slot = Some(Cached { generation, start, value: make() });
    }
    &slot.as_ref().expect("filled above").value
}

/// How many files the file lists show at most.
const LIST_LIMIT: usize = 1000;

struct Running {
    progress: Arc<Progress>,
    handle: JoinHandle<io::Result<Tree>>,
    root: PathBuf,
    started: Instant,
    /// Scanned by the helper with admin rights (no live counters).
    admin: bool,
}

/// The tabs of the menu band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Start,
    View,
    Cleanup,
    History,
    Help,
}

/// Commands of the menu band and the keyboard; done after drawing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    Scan(PathBuf),
    ChooseFolder,
    Rescan,
    Cancel,
    AdminScan,
    FullDiskAccess,
    ExportCsv,
    ExportXlsx,
    ExportHtml,
    /// Ask whether to add (true) or remove the right-click menu entry.
    AskShell(bool),
    ExpandAll,
    CollapseAll,
    LogFolder,
    About,
    Shortcuts,
    SearchView,
    ToggleOffline,
    ToggleKeepHistory,
    AskClearHistory,
    ToggleLive,
    SetWarn(u8),
    CheckSpace,
    SetLang(Lang),
    Node(Action),
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
    /// Show the "What is this?" window.
    Info(NodeId),
    /// Ask whether to move the node to the recycle bin.
    AskTrash(NodeId),
    /// Select a node, open the folders above it and show it in the folder list.
    Reveal(NodeId),
    /// Open the move assistant for a folder.
    AskMove(NodeId),
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
    measure: Measure,
    chart: Chart,
    map_cache: Option<map::Cache>,
    ring_cache: Option<ring::Cache>,
    types_cache: Option<Cached<Vec<TypeStats>>>,
    categories_cache: Option<Cached<Vec<CategoryStats>>>,
    largest_cache: Option<Cached<Vec<NodeId>>>,
    oldest_cache: Option<Cached<Vec<NodeId>>>,
    age_cache: Option<Cached<[AgeStats; 7]>>,
    filter: Filter,
    min_mb: f64,
    /// Bumped when the search form changes, so the search runs again.
    filter_generation: u64,
    search_cache: Option<Cached<Hits>>,
    error: Option<String>,
    notice: Option<(String, Instant)>,
    scroll_to_selected: bool,
    last_duration: Option<Duration>,
    places: Vec<places::Place>,
    title: String,
    tab: Tab,
    show_about: bool,
    show_shortcuts: bool,
    show_full_access: bool,
    /// The shown tree was scanned with admin rights.
    admin_tree: bool,
    /// This process itself runs with admin rights.
    elevated: bool,
    cleanup: cleanup::State,
    settings: crate::settings::Settings,
    /// Settings are saved (not in tests).
    persist: bool,
    /// Where the summaries of earlier scans are kept (none in tests).
    history_store: Option<crate::history::Store>,
    hist: history_view::State,
    /// When the shown tree was scanned.
    scan_time: Option<i64>,
    /// Live update and the disk warning start by themselves (not in tests).
    auto: bool,
    live: Option<crate::live::Live>,
    /// The tree is too large to be watched on this system.
    live_too_large: bool,
    warn: Warn,
    dups: tools::Dups,
    /// Cloud folders of this computer (found when first shown).
    cloud_roots: Option<Vec<crate::cloud::Root>>,
    cloud_cache: Option<Cached<Vec<tools::CloudPart>>>,
    /// Installed programs (read when first shown).
    apps_list: Option<Vec<crate::apps::App>>,
    apps_cache: Option<Cached<Vec<crate::apps::Usage>>>,
    mover: tools::Move,
    /// Waiting for the answer to "keep only online".
    free_up_ask: Option<tools::FreeUpRequest>,
    /// Waiting for the answer to adding (true) or removing the right-click menu entry.
    shell_ask: Option<bool>,
    /// The right-click menu entry is there (checked when first shown).
    shell_installed: Option<bool>,
}

/// The check for nearly full disks.
#[derive(Default)]
struct Warn {
    low: Vec<crate::disk::Low>,
    /// Warnings the user hid (until the program starts again).
    hidden: HashSet<PathBuf>,
    job: Option<JoinHandle<Vec<crate::disk::Low>>>,
    last: Option<Instant>,
    /// Started with "Check now": report also when all is well.
    manual: bool,
}

/// Disks are checked again after this time.
const WARN_EVERY: Duration = Duration::from_secs(10 * 60);

/// The formats of the export buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportKind {
    Csv,
    Xlsx,
    Html,
}

/// True if no folder above the node (or the node itself) was taken out.
fn reachable(tree: &Tree, id: NodeId) -> bool {
    (id as usize) < tree.len()
        && tree.ancestors(id).iter().all(|&a| !tree.node(a).has(crate::tree::flags::REMOVED))
}

impl App {
    /// The window as it starts; scans `start` right away if given.
    pub fn new(start: Option<PathBuf>) -> Self {
        let mut app = Self::empty();
        app.settings = crate::settings::Settings::load();
        app.persist = true;
        app.auto = true;
        app.history_store = crate::history::Store::default_location();
        if let Some(l) = app.settings.lang {
            i18n::set_lang(l);
        }
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

    /// A window with an existing scan and a history in the given folder (tests).
    pub fn with_history(tree: Tree, history_dir: PathBuf, scan_time: Option<i64>) -> Self {
        let mut app = Self::empty();
        app.path_text = tree.root_path.display().to_string();
        app.history_store = Some(crate::history::Store::at(history_dir));
        app.scan_time = scan_time;
        app.set_tree(tree);
        app
    }

    /// Shows the warning for these disks as if the check had found them (tests).
    #[doc(hidden)]
    pub fn with_low_disks(mut self, low: Vec<crate::disk::Low>) -> Self {
        self.warn.low = low;
        self
    }

    /// Uses these cloud folders instead of looking for them (tests).
    #[doc(hidden)]
    pub fn with_cloud_roots(mut self, roots: Vec<crate::cloud::Root>) -> Self {
        self.cloud_roots = Some(roots);
        self
    }

    /// Uses this list of programs instead of asking the system (tests).
    #[doc(hidden)]
    pub fn with_apps(mut self, apps: Vec<crate::apps::App>) -> Self {
        self.apps_list = Some(apps);
        self
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
            measure: Measure::Size,
            chart: Chart::Map,
            map_cache: None,
            ring_cache: None,
            types_cache: None,
            categories_cache: None,
            largest_cache: None,
            oldest_cache: None,
            age_cache: None,
            filter: Filter::default(),
            min_mb: 0.0,
            filter_generation: 0,
            search_cache: None,
            error: None,
            notice: None,
            scroll_to_selected: false,
            last_duration: None,
            places: places::list(),
            title: String::new(),
            tab: Tab::Start,
            show_about: false,
            show_shortcuts: false,
            show_full_access: false,
            admin_tree: false,
            elevated: crate::elevate::is_elevated(),
            cleanup: cleanup::State::default(),
            settings: crate::settings::Settings::default(),
            persist: false,
            history_store: None,
            hist: history_view::State::default(),
            scan_time: None,
            auto: false,
            live: None,
            live_too_large: false,
            warn: Warn::default(),
            dups: tools::Dups::default(),
            cloud_roots: None,
            cloud_cache: None,
            apps_list: None,
            apps_cache: None,
            mover: tools::Move::default(),
            free_up_ask: None,
            shell_ask: None,
            shell_installed: None,
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
        // The caches compare the generation, so they refresh by themselves.
        self.cleanup.reset();
        self.map_cache = None;
        self.ring_cache = None;
        self.dups.reset();
        self.hist.reset();
        if let Some(tree) = &self.tree {
            self.hist.reload(self.history_store.as_ref(), &tree.root_path);
        }
        // Watching starts again for the new tree (in `live_poll`).
        self.live = None;
        self.live_too_large = false;
    }

    /// Takes over what the live update found and starts it when needed.
    fn live_poll(&mut self, ctx: &egui::Context) {
        if self.live.is_none()
            && self.auto
            && self.settings.live
            && self.running.is_none()
            && !self.live_too_large
            && let Some(tree) = &self.tree
        {
            if crate::live::supported(tree) {
                let c = ctx.clone();
                self.live = Some(crate::live::Live::start(&tree.root_path, move || c.request_repaint()));
            } else {
                self.live_too_large = true;
            }
        }
        let (Some(live), Some(tree)) = (&mut self.live, &mut self.tree) else { return };
        if live.poll(tree, self.sort) {
            self.generation += 1;
            self.map_cache = None;
            self.ring_cache = None;
            let tree = self.tree.as_ref().expect("checked above");
            if self.selected.is_some_and(|s| !reachable(tree, s)) {
                self.selected = Some(ROOT);
            }
            if !reachable(tree, self.map_root) {
                self.map_root = ROOT;
            }
            self.expanded.retain(|&e| reachable(tree, e));
            self.compact_if_needed();
        }
        if self.live.as_ref().is_some_and(|l| l.busy()) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    /// After many live changes the tree holds many unused entries: copy it tidily.
    fn compact_if_needed(&mut self) {
        let Some(tree) = &self.tree else { return };
        let unused = tree.unused();
        if unused < 200_000 || unused < tree.len() / 2 {
            return;
        }
        let selected = self.selected.map(|s| tree.path(s));
        let map_root = tree.path(self.map_root);
        let expanded: Vec<PathBuf> = self.expanded.iter().map(|&e| tree.path(e)).collect();
        let tidy = tree.copy(0, usize::MAX, usize::MAX, true);
        self.selected = Some(selected.and_then(|p| tidy.find(&p)).unwrap_or(ROOT));
        self.map_root = tidy.find(&map_root).unwrap_or(ROOT);
        self.expanded = expanded.iter().filter_map(|p| tidy.find(p)).collect();
        self.expanded.insert(ROOT);
        self.tree = Some(tidy);
        self.generation += 1;
        self.cleanup.reset();
    }

    /// Checks the disks for free space now and then (in the background).
    fn warn_poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.warn.job {
            if job.is_finished() {
                let low = self.warn.job.take().and_then(|j| j.join().ok()).unwrap_or_default();
                if self.warn.manual && low.is_empty() {
                    self.notice = Some((tr().warn_all_ok.to_string(), Instant::now()));
                }
                self.warn.low = low;
                self.warn.manual = false;
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return;
        }
        if self.auto && self.settings.warn_percent > 0 {
            if self.warn.last.is_none_or(|l| l.elapsed() >= WARN_EVERY) {
                self.check_space(false);
            }
            ctx.request_repaint_after(WARN_EVERY);
        }
    }

    fn check_space(&mut self, manual: bool) {
        if self.warn.job.is_some() {
            return;
        }
        let disks: Vec<(String, PathBuf)> = self
            .places
            .iter()
            .filter(|p| p.drive && crate::disk::is_fixed(&p.path))
            .map(|p| (p.label.clone(), p.path.clone()))
            .collect();
        let percent = self.settings.warn_percent;
        self.warn.last = Some(Instant::now());
        self.warn.manual = manual;
        self.warn.job = std::thread::Builder::new()
            .name("disk space".into())
            .spawn(move || crate::disk::low_disks(&disks, percent))
            .ok();
    }

    /// The yellow bar for nearly full disks.
    fn warning_bar(&mut self, ui: &mut egui::Ui) -> Vec<Command> {
        let t = tr();
        let mut cmds = Vec::new();
        let mut hide = Vec::new();
        for low in self.warn.low.iter().filter(|l| !self.warn.hidden.contains(&l.path)) {
            ui.horizontal(|ui| {
                let text = t
                    .warn_text
                    .replace("{disk}", &low.label)
                    .replace("{pct}", &format!("{:.0} %", low.space.free_percent()))
                    .replace("{free}", &format::size(low.space.free))
                    .replace("{total}", &format::size(low.space.total));
                ui.label(egui::RichText::new(format!("⚠ {text}")).color(ui.visuals().warn_fg_color).strong());
                if ui.button(t.warn_scan).clicked() {
                    cmds.push(Command::Scan(low.path.clone()));
                }
                if ui.button(t.warn_hide).clicked() {
                    hide.push(low.path.clone());
                }
            });
        }
        self.warn.hidden.extend(hide);
        cmds
    }

    /// Compares the shown tree with a saved scan.
    fn compare_with(&mut self, index: usize) {
        let (Some(store), Some(tree)) = (&self.history_store, &self.tree) else { return };
        let Some(summary) = self.hist.entries.get(index) else { return };
        match history_view::compare(store, tree, summary) {
            Ok(c) => self.hist.compared = Some(c),
            Err(e) => {
                let text = format::io_error(&e);
                self.show_error(text);
            }
        }
    }

    fn start_scan(&mut self, path: PathBuf) {
        self.start(path, false);
    }

    /// Starts a scan in the background; with `admin` through the helper with admin rights.
    fn start(&mut self, path: PathBuf, admin: bool) {
        if let Some(r) = &self.running {
            r.progress.cancel();
        }
        let progress = Arc::new(Progress::default());
        let p = Arc::clone(&progress);
        let root = path.clone();
        let elevated = self.elevated;
        let handle = std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || {
                if admin {
                    crate::elevate::scan_as_admin(&root)
                } else if elevated {
                    scan::scan_with_admin_rights(&root, &scan::Options::default(), &p)
                } else {
                    scan::scan(&root, &scan::Options::default(), &p)
                }
            })
            .expect("start scan thread");
        log::info!("scan started{}: {}", if admin { " (admin helper)" } else { "" }, applog::private(&path));
        // The scan brings fresh numbers; watching starts again afterwards.
        self.live = None;
        self.running = Some(Running { progress, handle, root: path, started: Instant::now(), admin });
        // An admin scan keeps the old result visible until the new one is there.
        if !admin {
            self.tree = None;
            self.generation += 1;
        }
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
                self.admin_tree = r.admin || self.elevated;
                let time = crate::query::now();
                self.scan_time = Some(time);
                if self.settings.history
                    && let Some(store) = &self.history_store
                {
                    let space = crate::disk::space(&tree.root_path).ok();
                    if let Err(e) = store.save(&tree, time, space) {
                        log::warn!("history not saved: {}", format::io_error(&e));
                    }
                }
                self.set_tree(tree);
            }
            Ok(Err(e)) if r.admin && crate::elevate::is_cancelled(&e) => {
                self.notice = Some((tr().admin_cancelled.to_string(), Instant::now()));
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
            Action::Info(id) => {
                self.selected = Some(id);
                self.cleanup.open_info(id);
            }
            // The scanned folder itself is never offered for the recycle bin.
            Action::AskTrash(id) if id != ROOT => self.cleanup.confirm = Some(id),
            Action::AskTrash(_) => {}
            Action::Reveal(id) => {
                self.view = View::Folders;
                self.reveal(id);
            }
            Action::AskMove(id) => {
                if id != ROOT && tree.node(id).is_dir() {
                    self.mover.open(tree, id);
                }
            }
            Action::Sort(key) => {
                self.sort = if self.sort.0 == key { (key, !self.sort.1) } else { (key, false) };
                if let Some(tree) = &mut self.tree {
                    tree.sort(self.sort.0, self.sort.1);
                }
            }
        }
    }

    fn save_settings(&mut self) {
        if self.persist
            && let Err(e) = self.settings.save()
        {
            log::warn!("settings not saved: {e}");
        }
    }

    /// Moves a node to the recycle bin and takes it out of the tree.
    fn move_to_trash(&mut self, id: NodeId) {
        let Some(tree) = &self.tree else { return };
        if id == ROOT {
            return;
        }
        let path = tree.path(id);
        let size = tree.node(id).size;
        match crate::trash::move_to_trash(&path) {
            Ok(external::Outcome::Done) => {
                self.take_out(id);
                let text = format!("{}: {}", tr().trash_done, format::size(size));
                self.notice = Some((text, Instant::now()));
            }
            Ok(external::Outcome::Blocked) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => self.show_error(format!(
                "{}
{}",
                path.display(),
                format::io_error(&e)
            )),
        }
    }

    /// Takes a node out of the tree after it left the disk (recycle bin, moved).
    fn take_out(&mut self, id: NodeId) {
        let Some(tree) = &mut self.tree else { return };
        if id == ROOT || (id as usize) >= tree.len() {
            return;
        }
        let parent = tree.node(id).parent;
        let map_inside = tree.ancestors(self.map_root).contains(&id);
        tree.remove(id);
        self.generation += 1;
        self.map_cache = None;
        self.ring_cache = None;
        if self.selected == Some(id) {
            self.selected = Some(parent);
        }
        if map_inside {
            self.map_root = parent;
        }
        self.cleanup.info = None;
    }

    fn export(&mut self, kind: ExportKind) {
        let Some(tree) = &self.tree else { return };
        let (name, filter, ext) = match kind {
            ExportKind::Csv => ("mrsizeme.csv", "CSV", "csv"),
            ExportKind::Xlsx => ("mrsizeme.xlsx", "Excel", "xlsx"),
            ExportKind::Html => ("mrsizeme.html", "HTML", "html"),
        };
        let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter(filter, &[ext]).save_file()
        else {
            return;
        };
        let when = self.scan_time.unwrap_or_else(crate::query::now);
        let result = std::fs::File::create(&path).and_then(|f| {
            let mut w = io::BufWriter::new(f);
            match kind {
                ExportKind::Csv => export::csv(tree, ROOT, usize::MAX, false, &mut w),
                ExportKind::Xlsx => crate::report::xlsx(tree, ROOT, usize::MAX, false, &mut w),
                ExportKind::Html => crate::report::html(tree, when, &mut w),
            }
        });
        match result {
            Ok(()) => self.notice = Some((tr().export_done.to_string(), Instant::now())),
            Err(e) => self.show_error(format!("{}\n{}", path.display(), format::io_error(&e))),
        }
    }

    /// The menu band with its tabs.
    fn ribbon(&mut self, ui: &mut egui::Ui) -> Vec<Command> {
        use icons::Icon;
        use ribbon::{big, column, group, small};
        let t = tr();
        let mut cmds = Vec::new();
        let has_tree = self.tree.is_some();
        let busy = self.running.is_some();
        let sel = self.tree.as_ref().and(self.selected);

        ui.add_space(3.0);
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Start, t.tab_start),
                (Tab::View, t.tab_view),
                (Tab::Cleanup, t.tab_cleanup),
                (Tab::History, t.tab_history),
                (Tab::Help, t.tab_help),
            ] {
                ui.selectable_value(&mut self.tab, tab, egui::RichText::new(label).strong());
            }
        });
        ui.add_space(2.0);
        egui::ScrollArea::horizontal().id_salt("ribbon_scroll").show(ui, |ui| {
            ui.horizontal(|ui| match self.tab {
                Tab::Start => {
                    group(ui, t.grp_scan, |ui| {
                        let drives = big(ui, Icon::Drive, t.drives, !busy, false);
                        egui::Popup::menu(&drives).show(|ui| {
                            for place in &self.places {
                                if ui.button(&place.label).clicked() {
                                    cmds.push(Command::Scan(place.path.clone()));
                                }
                            }
                        });
                        if big(ui, Icon::FolderOpen, t.btn_folder, !busy, false).clicked() {
                            cmds.push(Command::ChooseFolder);
                        }
                        if busy {
                            if big(ui, Icon::Stop, t.cancel, true, false).clicked() {
                                cmds.push(Command::Cancel);
                            }
                        } else if big(
                            ui,
                            Icon::Rescan,
                            t.rescan,
                            has_tree || !self.path_text.is_empty(),
                            false,
                        )
                        .clicked()
                        {
                            cmds.push(Command::Rescan);
                        }
                    });
                    group(ui, t.grp_all_folders, |ui| match crate::elevate::support() {
                        crate::elevate::AdminSupport::Helper => {
                            let can = !busy && !self.elevated && !self.path_text.trim().is_empty();
                            let tip = if self.elevated { t.tip_already_admin } else { t.tip_admin_scan };
                            if big(ui, Icon::Shield, t.btn_admin_scan, can, self.admin_tree)
                                .on_hover_text(tip)
                                .clicked()
                            {
                                cmds.push(Command::AdminScan);
                            }
                        }
                        crate::elevate::AdminSupport::FullDiskAccess => {
                            if big(ui, Icon::Key, t.btn_full_access, true, false).clicked() {
                                cmds.push(Command::FullDiskAccess);
                            }
                        }
                    });
                    group(ui, t.grp_selection, |ui| {
                        column(ui, |ui| {
                            if let Some(id) = sel {
                                if small(ui, Icon::OpenExternal, t.open_folder, true, false).clicked() {
                                    cmds.push(Command::Node(Action::OpenFolder(id)));
                                }
                                if small(ui, Icon::Copy, t.copy_path, true, false).clicked() {
                                    cmds.push(Command::Node(Action::CopyPath(id)));
                                }
                                if small(ui, Icon::Target, t.btn_show_in_chart, true, false).clicked() {
                                    cmds.push(Command::Node(Action::MapRoot(id)));
                                }
                            } else {
                                small(ui, Icon::OpenExternal, t.open_folder, false, false);
                                small(ui, Icon::Copy, t.copy_path, false, false);
                                small(ui, Icon::Target, t.btn_show_in_chart, false, false);
                            }
                        });
                    });
                    group(ui, t.grp_export, |ui| {
                        if big(ui, Icon::Export, t.btn_csv, has_tree && !busy, false).clicked() {
                            cmds.push(Command::ExportCsv);
                        }
                        if big(ui, Icon::Sheet, t.btn_xlsx, has_tree && !busy, false).clicked() {
                            cmds.push(Command::ExportXlsx);
                        }
                        if big(ui, Icon::Report, t.btn_html, has_tree && !busy, false).clicked() {
                            cmds.push(Command::ExportHtml);
                        }
                    });
                    group(ui, t.grp_system, |ui| {
                        let on = *self.shell_installed.get_or_insert_with(crate::shell::is_installed);
                        if big(ui, Icon::Menu, t.btn_context_menu, true, on)
                            .on_hover_text(t.tip_context_menu)
                            .clicked()
                        {
                            cmds.push(Command::AskShell(!on));
                        }
                    });
                }
                Tab::View => {
                    group(ui, t.grp_lists, |ui| {
                        let mut list = |ui: &mut egui::Ui, view: View, icon: Icon, label: &str| {
                            if small(ui, icon, label, has_tree, self.view == view).clicked() {
                                self.view = view;
                            }
                        };
                        column(ui, |ui| {
                            list(ui, View::Folders, Icon::TreeList, t.view_folders);
                            list(ui, View::Types, Icon::Types, t.view_types);
                            list(ui, View::Categories, Icon::Categories, t.view_categories);
                        });
                        column(ui, |ui| {
                            list(ui, View::Largest, Icon::Largest, t.view_largest);
                            list(ui, View::Oldest, Icon::Oldest, t.view_oldest);
                            list(ui, View::Age, Icon::Age, t.view_age);
                        });
                        column(ui, |ui| {
                            list(ui, View::Search, Icon::Search, t.view_search);
                            list(ui, View::Duplicates, Icon::Duplicates, t.view_duplicates);
                            list(ui, View::Apps, Icon::Apps, t.view_apps);
                        });
                        column(ui, |ui| {
                            list(ui, View::Cloud, Icon::Cloud, t.view_cloud);
                        });
                    });
                    group(ui, t.grp_chart, |ui| {
                        if big(ui, Icon::TileMap, t.map_title, has_tree, self.chart == Chart::Map).clicked() {
                            self.chart = Chart::Map;
                        }
                        if big(ui, Icon::Ring, t.ring_title, has_tree, self.chart == Chart::Ring).clicked() {
                            self.chart = Chart::Ring;
                        }
                        let parent = self.tree.as_ref().map(|tr| tr.node(self.map_root).parent);
                        let up = parent.filter(|&p| p != crate::tree::NO_PARENT);
                        if big(ui, Icon::Up, t.map_up, up.is_some(), false).clicked()
                            && let Some(p) = up
                        {
                            cmds.push(Command::Node(Action::MapRoot(p)));
                        }
                    });
                    group(ui, t.grp_measure, |ui| {
                        column(ui, |ui| {
                            for (m, icon, label) in [
                                (Measure::Size, Icon::Size, t.show_size),
                                (Measure::Allocated, Icon::Disk, t.show_allocated),
                                (Measure::Files, Icon::Count, t.show_count),
                            ] {
                                if small(ui, icon, label, has_tree, self.measure == m).clicked() {
                                    self.measure = m;
                                }
                            }
                        });
                    });
                    group(ui, t.grp_tree, |ui| {
                        column(ui, |ui| {
                            if small(ui, Icon::Expand, t.expand_all, has_tree, false).clicked() {
                                cmds.push(Command::ExpandAll);
                            }
                            if small(ui, Icon::Collapse, t.collapse_all, has_tree, false).clicked() {
                                cmds.push(Command::CollapseAll);
                            }
                        });
                    });
                    group(ui, t.grp_look, |ui| {
                        let pref = ui.ctx().options(|o| o.theme_preference);
                        column(ui, |ui| {
                            for (p, icon, label) in [
                                (egui::ThemePreference::Light, Icon::Sun, t.theme_light),
                                (egui::ThemePreference::Dark, Icon::Moon, t.theme_dark),
                                (egui::ThemePreference::System, Icon::Disk, t.theme_system),
                            ] {
                                if small(ui, icon, label, true, pref == p).clicked() {
                                    ui.ctx().set_theme(p);
                                }
                            }
                        });
                        column(ui, |ui| {
                            for l in Lang::ALL {
                                if small(ui, Icon::Globe, l.native_name(), true, i18n::lang() == l).clicked()
                                {
                                    cmds.push(Command::SetLang(l));
                                }
                            }
                        });
                    });
                }
                Tab::Cleanup => {
                    group(ui, t.grp_selection, |ui| {
                        let id = sel;
                        if big(ui, Icon::Question, t.btn_what_is, id.is_some(), false).clicked()
                            && let Some(id) = id
                        {
                            cmds.push(Command::Node(Action::Info(id)));
                        }
                        let can_trash = id.is_some_and(|i| i != ROOT);
                        if big(ui, Icon::Trash, t.btn_trash, can_trash, false).clicked()
                            && let Some(id) = id
                        {
                            cmds.push(Command::Node(Action::AskTrash(id)));
                        }
                    });
                    group(ui, t.grp_suggestions, |ui| {
                        if big(ui, Icon::Broom, t.view_cleanup, has_tree, self.view == View::Cleanup)
                            .clicked()
                        {
                            self.view = View::Cleanup;
                        }
                    });
                    group(ui, t.grp_find, |ui| {
                        if big(
                            ui,
                            Icon::Duplicates,
                            t.btn_duplicates,
                            has_tree,
                            self.view == View::Duplicates,
                        )
                        .clicked()
                        {
                            self.view = View::Duplicates;
                        }
                    });
                    group(ui, t.grp_move, |ui| {
                        let dir = sel.filter(|&i| {
                            i != ROOT && self.tree.as_ref().is_some_and(|tr| tr.node(i).is_dir())
                        });
                        if big(ui, Icon::Move, t.btn_move, dir.is_some() && !busy, false).clicked()
                            && let Some(id) = dir
                        {
                            cmds.push(Command::Node(Action::AskMove(id)));
                        }
                    });
                    group(ui, t.grp_ai, |ui| {
                        if big(ui, Icon::Sparkle, t.ai_heading, sel.is_some(), false).clicked()
                            && let Some(id) = sel
                        {
                            cmds.push(Command::Node(Action::Info(id)));
                        }
                        if big(ui, Icon::Gear, t.btn_ai_settings, true, self.cleanup.show_settings).clicked()
                        {
                            self.cleanup.show_settings = true;
                        }
                    });
                    group(ui, t.grp_privacy, |ui| {
                        if big(ui, Icon::Offline, t.btn_offline, true, self.settings.offline)
                            .on_hover_text(t.tip_offline)
                            .clicked()
                        {
                            cmds.push(Command::ToggleOffline);
                        }
                    });
                }
                Tab::History => {
                    group(ui, t.grp_history, |ui| {
                        if big(ui, Icon::History, t.btn_history_view, has_tree, self.view == View::History)
                            .clicked()
                        {
                            self.view = View::History;
                        }
                        column(ui, |ui| {
                            if small(ui, Icon::Bookmark, t.btn_keep_history, true, self.settings.history)
                                .on_hover_text(t.tip_keep_history)
                                .clicked()
                            {
                                cmds.push(Command::ToggleKeepHistory);
                            }
                            if small(
                                ui,
                                Icon::Trash,
                                t.btn_clear_history,
                                self.history_store.is_some(),
                                false,
                            )
                            .clicked()
                            {
                                cmds.push(Command::AskClearHistory);
                            }
                        });
                    });
                    group(ui, t.grp_live, |ui| {
                        if big(ui, Icon::Live, t.btn_live, true, self.settings.live)
                            .on_hover_text(t.tip_live)
                            .clicked()
                        {
                            cmds.push(Command::ToggleLive);
                        }
                    });
                    group(ui, t.grp_warning, |ui| {
                        let current = self.settings.warn_percent;
                        let resp = big(ui, Icon::Bell, t.btn_warning, true, current > 0);
                        egui::Popup::menu(&resp).show(|ui| {
                            for p in [0u8, 5, 10, 15, 20] {
                                let label = if p == 0 {
                                    t.warn_off.to_string()
                                } else {
                                    t.warn_below.replace("{n}", &p.to_string())
                                };
                                if ui.selectable_label(current == p, label).clicked() {
                                    cmds.push(Command::SetWarn(p));
                                }
                            }
                            ui.separator();
                            if ui.add_enabled(current > 0, egui::Button::new(t.warn_check_now)).clicked() {
                                cmds.push(Command::CheckSpace);
                            }
                        });
                    });
                }
                Tab::Help => {
                    group(ui, t.grp_help, |ui| {
                        if big(ui, Icon::Info, t.about, true, false).clicked() {
                            cmds.push(Command::About);
                        }
                        if big(ui, Icon::Keyboard, t.shortcuts, true, false).clicked() {
                            cmds.push(Command::Shortcuts);
                        }
                        if big(ui, Icon::Log, t.log_folder, true, false).clicked() {
                            cmds.push(Command::LogFolder);
                        }
                    });
                }
            });
        });
        ui.add_space(2.0);
        cmds
    }

    /// The line with the path field below the menu band.
    fn address_bar(&mut self, ui: &mut egui::Ui) -> Vec<Command> {
        let t = tr();
        let mut cmds = Vec::new();
        ui.horizontal(|ui| {
            let field = egui::TextEdit::singleline(&mut self.path_text)
                .hint_text(t.path_hint)
                .desired_width((ui.available_width() - 260.0).max(200.0));
            let resp = ui.add(field);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let busy = self.running.is_some();
            if !busy && (ui.button(t.scan).clicked() || enter) && !self.path_text.trim().is_empty() {
                cmds.push(Command::Scan(PathBuf::from(self.path_text.trim())));
            }
            if self.admin_tree && self.tree.is_some() {
                ui.label(
                    egui::RichText::new(format!("🛡 {}", t.admin_badge))
                        .color(egui::Color32::from_rgb(139, 92, 246)),
                );
            }
        });
        ui.add_space(3.0);
        cmds
    }

    fn command(&mut self, cmd: Command, ctx: &egui::Context) {
        match cmd {
            Command::Scan(path) => {
                self.path_text = path.display().to_string();
                self.start_scan(path);
            }
            Command::ChooseFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.command(Command::Scan(dir), ctx);
                }
            }
            Command::Rescan => {
                let path = self.tree.as_ref().map(|t| t.root_path.clone());
                let path = path.or_else(|| {
                    (!self.path_text.trim().is_empty()).then(|| PathBuf::from(self.path_text.trim()))
                });
                if let Some(p) = path {
                    self.command(Command::Scan(p), ctx);
                }
            }
            Command::Cancel => {
                if let Some(r) = self.running.take() {
                    r.progress.cancel();
                    if r.admin {
                        // The helper cannot be stopped from here; its result is ignored.
                        self.notice = Some((tr().admin_cancelled.to_string(), Instant::now()));
                    } else {
                        self.running = Some(r);
                    }
                }
            }
            Command::AdminScan => {
                let path = self.tree.as_ref().map(|t| t.root_path.clone());
                let path = path.or_else(|| {
                    (!self.path_text.trim().is_empty()).then(|| PathBuf::from(self.path_text.trim()))
                });
                if let Some(p) = path {
                    self.start(p, true);
                }
            }
            Command::FullDiskAccess => self.show_full_access = true,
            Command::ExportCsv => self.export(ExportKind::Csv),
            Command::ExportXlsx => self.export(ExportKind::Xlsx),
            Command::ExportHtml => self.export(ExportKind::Html),
            Command::AskShell(add) => self.shell_ask = Some(add),
            Command::ExpandAll => {
                if let Some(tree) = &self.tree {
                    let start = self.selected.unwrap_or(ROOT);
                    let mut stack = vec![start];
                    // A limit keeps huge trees usable.
                    while let Some(id) = stack.pop()
                        && self.expanded.len() < 20_000
                    {
                        if tree.node(id).is_dir() {
                            self.expanded.insert(id);
                            stack.extend_from_slice(tree.children(id));
                        }
                    }
                }
            }
            Command::CollapseAll => {
                self.expanded.clear();
                self.expanded.insert(ROOT);
                self.selected = Some(ROOT);
            }
            Command::LogFolder => {
                if let Some(dir) = applog::log_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    if let Err(e) = external::open_folder(&dir) {
                        self.show_error(format!("{}\n{}", applog::private(&dir), format::io_error(&e)));
                    }
                }
            }
            Command::About => self.show_about = true,
            Command::Shortcuts => self.show_shortcuts = true,
            Command::SearchView => self.view = View::Search,
            Command::ToggleOffline => {
                self.settings.offline = !self.settings.offline;
                self.save_settings();
            }
            Command::ToggleKeepHistory => {
                self.settings.history = !self.settings.history;
                self.save_settings();
            }
            Command::AskClearHistory => self.hist.confirm_clear = true,
            Command::ToggleLive => {
                self.settings.live = !self.settings.live;
                self.save_settings();
                if !self.settings.live {
                    self.live = None;
                }
                self.live_too_large = false;
            }
            Command::SetWarn(p) => {
                self.settings.warn_percent = p;
                self.save_settings();
                self.warn.low.clear();
                if p > 0 {
                    self.check_space(false);
                }
            }
            Command::CheckSpace => {
                self.warn.hidden.clear();
                self.check_space(true);
            }
            Command::SetLang(l) => {
                i18n::set_lang(l);
                self.settings.lang = Some(l);
                self.save_settings();
            }
            Command::Node(action) => self.apply(action, ctx),
        }
    }

    /// The small windows: about, shortcuts, Full Disk Access. All can be moved.
    fn dialogs(&mut self, ctx: &egui::Context) {
        let t = tr();
        let center = ctx.content_rect().center();
        if self.show_about {
            egui::Window::new(t.about)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut self.show_about)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading(format!("MrSizeMe {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(4.0);
                    ui.label(t.about_text);
                });
        }
        if self.show_shortcuts {
            egui::Window::new(t.shortcuts)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut self.show_shortcuts)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("shortcut_grid").num_columns(2).striped(true).show(ui, |ui| {
                        for (keys, what) in t.shortcut_list {
                            let keys = if cfg!(target_os = "macos") {
                                keys.replace("Strg", "⌘").replace("Ctrl", "⌘")
                            } else {
                                keys.to_string()
                            };
                            ui.strong(keys);
                            ui.label(what);
                            ui.end_row();
                        }
                    });
                });
        }
        if self.show_full_access {
            let mut open = true;
            let mut error = None;
            egui::Window::new(t.fda_title)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut open)
                .collapsible(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label(t.fda_intro);
                    ui.add_space(6.0);
                    ui.label(t.fda_steps);
                    ui.add_space(6.0);
                    let status = match crate::elevate::has_full_disk_access() {
                        Some(true) => t.fda_status_yes,
                        Some(false) => t.fda_status_no,
                        None => t.fda_status_unknown,
                    };
                    ui.strong(status);
                    if ui.button(t.fda_open_settings).clicked()
                        && let Err(e) = external::open_full_disk_access_settings()
                    {
                        error = Some(format::io_error(&e));
                    }
                });
            self.show_full_access = open;
            if let Some(e) = error {
                self.show_error(e);
            }
        }
        if let Some(add) = self.shell_ask {
            let mut open = true;
            let mut decided = None;
            egui::Window::new(t.btn_context_menu)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(440.0)
                .show(ctx, |ui| {
                    ui.label(if add { t.shell_add_question } else { t.shell_remove_question });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(if add { t.btn_add } else { t.btn_remove }).clicked() {
                            decided = Some(true);
                        }
                        if ui.button(t.cancel).clicked() {
                            decided = Some(false);
                        }
                    });
                });
            if !open || decided.is_some() {
                self.shell_ask = None;
            }
            if decided == Some(true) {
                let result = if add { crate::shell::install() } else { crate::shell::uninstall() };
                match result {
                    Ok(external::Outcome::Done) => {
                        self.shell_installed = Some(add);
                        let text = if add { t.shell_added } else { t.shell_removed };
                        self.notice = Some((text.to_string(), Instant::now()));
                    }
                    Ok(external::Outcome::Blocked) => {}
                    Err(e) => self.show_error(format::io_error(&e)),
                }
            }
        }
        if let Some((path, name, size)) = self.free_up_ask.clone() {
            let mut open = true;
            let mut decided = None;
            egui::Window::new(t.cloud_free_up)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    let q = t
                        .cloud_free_up_question
                        .replace("{name}", &name)
                        .replace("{size}", &format::size(size));
                    ui.label(q);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(t.cloud_free_up).clicked() {
                            decided = Some(true);
                        }
                        if ui.button(t.cancel).clicked() {
                            decided = Some(false);
                        }
                    });
                });
            if !open || decided.is_some() {
                self.free_up_ask = None;
            }
            if decided == Some(true) {
                match crate::cloud::free_up(&path) {
                    Ok(_) => self.notice = Some((t.cloud_free_up_done.to_string(), Instant::now())),
                    Err(e) => self.show_error(format!(
                        "{}
{}",
                        path.display(),
                        format::io_error(&e)
                    )),
                }
            }
        }
        if self.hist.confirm_clear {
            let mut open = true;
            let mut decided = None;
            egui::Window::new(t.btn_clear_history)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(400.0)
                .show(ctx, |ui| {
                    ui.label(t.clear_history_question);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(t.btn_delete).clicked() {
                            decided = Some(true);
                        }
                        if ui.button(t.cancel).clicked() {
                            decided = Some(false);
                        }
                    });
                });
            if !open || decided.is_some() {
                self.hist.confirm_clear = false;
            }
            if decided == Some(true)
                && let Some(store) = &self.history_store
            {
                match store.clear() {
                    Ok(()) => {
                        self.hist.reset();
                        self.hist.auto_done = true;
                        self.notice = Some((t.clear_history_done.to_string(), Instant::now()));
                    }
                    Err(e) => self.show_error(format::io_error(&e)),
                }
            }
        }
    }

    /// Shortcuts that work everywhere (unless a text field has the keyboard).
    fn global_keys(&self, ctx: &egui::Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        let typing = ctx.egui_wants_keyboard_input();
        ctx.input_mut(|i| {
            use egui::{Key, KeyboardShortcut, Modifiers};
            let cmd = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
            if i.consume_shortcut(&cmd(Key::O)) {
                cmds.push(Command::ChooseFolder);
            }
            if i.consume_shortcut(&cmd(Key::E)) {
                cmds.push(Command::ExportCsv);
            }
            if i.consume_shortcut(&cmd(Key::F)) {
                cmds.push(Command::SearchView);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::C))
                && let Some(id) = self.selected
            {
                cmds.push(Command::Node(Action::CopyPath(id)));
            }
            if i.key_pressed(Key::F5) && self.running.is_none() {
                cmds.push(Command::Rescan);
            }
            if i.key_pressed(Key::Escape) && self.running.is_some() {
                cmds.push(Command::Cancel);
            }
            if !typing
                && i.key_pressed(Key::Backspace)
                && let Some(tree) = &self.tree
            {
                let parent = tree.node(self.map_root).parent;
                if parent != crate::tree::NO_PARENT {
                    cmds.push(Command::Node(Action::MapRoot(parent)));
                }
            }
        });
        cmds
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        let t = tr();
        ui.horizontal(|ui| {
            if let Some(r) = &self.running
                && r.admin
            {
                ui.spinner();
                ui.label(format!("{}  {}", t.admin_scanning, r.root.display()));
            } else if let Some(r) = &self.running {
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
                if let Some(live) = &self.live {
                    use crate::live::Status;
                    if live.lost {
                        ui.colored_label(ui.visuals().warn_fg_color, t.live_lost);
                    } else {
                        match &live.status {
                            Status::Watching => {
                                let green = egui::Color32::from_rgb(34, 197, 94);
                                ui.label(egui::RichText::new(format!("● {}", t.live_on)).color(green))
                                    .on_hover_text(t.tip_live);
                            }
                            Status::Starting => {
                                ui.label(egui::RichText::new(t.live_starting).weak());
                            }
                            Status::Failed(e) => {
                                ui.label(egui::RichText::new(t.live_failed).weak()).on_hover_text(e.as_str());
                            }
                        }
                    }
                } else if self.live_too_large {
                    ui.label(egui::RichText::new(t.live_too_large).weak());
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
        ui.horizontal_wrapped(|ui| {
            for (view, label) in [
                (View::Folders, t.view_folders),
                (View::Types, t.view_types),
                (View::Categories, t.view_categories),
                (View::Largest, t.view_largest),
                (View::Oldest, t.view_oldest),
                (View::Age, t.view_age),
                (View::Search, t.view_search),
                (View::Cleanup, t.view_cleanup),
                (View::History, t.view_history),
                (View::Duplicates, t.view_duplicates),
                (View::Cloud, t.view_cloud),
                (View::Apps, t.view_apps),
            ] {
                ui.selectable_value(&mut self.view, view, label);
            }
        });
        ui.separator();
        let Some(tree) = &self.tree else { return Vec::new() };
        let start = self.selected.filter(|&s| tree.node(s).is_dir()).unwrap_or(ROOT);
        let generation = self.generation;
        match self.view {
            View::Folders => {
                let scroll = std::mem::take(&mut self.scroll_to_selected);
                folders::show(ui, tree, &self.expanded, self.selected, self.sort, scroll)
            }
            View::Types => {
                let stats =
                    refresh(&mut self.types_cache, generation, start, || crate::stats::by_type(tree, start));
                lists::types(ui, tree, start, stats);
                Vec::new()
            }
            View::Categories => {
                let stats = refresh(&mut self.categories_cache, generation, start, || {
                    crate::category::by_category(tree, start)
                });
                lists::categories(ui, tree, start, stats);
                Vec::new()
            }
            View::Largest => {
                let files = refresh(&mut self.largest_cache, generation, start, || {
                    crate::stats::largest_files(tree, start, LIST_LIMIT)
                });
                lists::largest(ui, tree, files, self.selected)
            }
            View::Oldest => {
                let files = refresh(&mut self.oldest_cache, generation, start, || {
                    crate::query::oldest_files(tree, start, LIST_LIMIT)
                });
                lists::largest(ui, tree, files, self.selected)
            }
            View::Age => {
                let stats = refresh(&mut self.age_cache, generation, start, || {
                    crate::query::age_distribution(tree, start, crate::query::now())
                });
                lists::ages(ui, tree, start, stats);
                Vec::new()
            }
            // Suggestions always cover the whole scan.
            View::Cleanup => cleanup::list(ui, tree, ROOT, generation, &mut self.cleanup),
            View::Duplicates => tools::duplicates(ui, tree, generation, &mut self.dups),
            View::Cloud => {
                let roots = self.cloud_roots.get_or_insert_with(crate::cloud::roots);
                let parts =
                    refresh(&mut self.cloud_cache, generation, ROOT, || tools::cloud_parts(tree, roots));
                let (acts, request) = tools::cloud(ui, tree, parts);
                if request.is_some() {
                    self.free_up_ask = request;
                }
                acts
            }
            View::Apps => {
                let list = self.apps_list.get_or_insert_with(crate::apps::installed);
                let usage =
                    refresh(&mut self.apps_cache, generation, ROOT, || crate::apps::measure(tree, list));
                tools::programs(ui, tree, usage)
            }
            View::History => {
                // At first the newest earlier scan is compared.
                if !self.hist.auto_done {
                    self.hist.auto_done = true;
                    let earlier = self.hist.entries.iter().rposition(|e| Some(e.time) != self.scan_time);
                    if let (Some(store), Some(i)) = (&self.history_store, earlier) {
                        match history_view::compare(store, tree, &self.hist.entries[i]) {
                            Ok(c) => self.hist.compared = Some(c),
                            Err(e) => log::warn!("history not read: {}", format::io_error(&e)),
                        }
                    }
                }
                let (acts, request) = history_view::show(
                    ui,
                    tree,
                    self.scan_time,
                    &self.hist,
                    self.history_store.is_some(),
                    self.settings.history,
                );
                if let Some(history_view::Request::Compare(i)) = request {
                    self.hist.pending_compare = Some(i);
                }
                acts
            }
            View::Search => {
                ui.label(
                    egui::RichText::new(format!("{} {}", t.in_folder, tree.path(start).display())).weak(),
                );
                if lists::search_form(ui, &mut self.filter, &mut self.min_mb) {
                    self.filter_generation += 1;
                }
                // The search runs again when the tree or the form changes.
                let key = generation.wrapping_mul(1_000_003).wrapping_add(self.filter_generation);
                let filter = &self.filter;
                let hits = refresh(&mut self.search_cache, key, start, || {
                    crate::query::search(tree, start, filter, crate::query::now(), LIST_LIMIT)
                });
                ui.separator();
                let mut line =
                    format!("{} {}  ·  {}", format::count(hits.count), t.hits, format::size(hits.size));
                if hits.count > hits.files.len() as u64 {
                    line +=
                        &format!("  ·  {}", t.shown_first.replace("{n}", &format::count(LIST_LIMIT as u64)));
                }
                ui.strong(line);
                lists::largest(ui, tree, &hits.files, self.selected)
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
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
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
        self.live_poll(&ctx);
        self.warn_poll(&ctx);
        self.update_title(&ctx);
        let mut actions = self.keyboard(&ctx);
        let mut cmds = self.global_keys(&ctx);

        cmds.extend(Panel::top("ribbon").show(ui, |ui| self.ribbon(ui)).inner);
        cmds.extend(Panel::top("address").show(ui, |ui| self.address_bar(ui)).inner);
        if self.warn.low.iter().any(|l| !self.warn.hidden.contains(&l.path)) {
            cmds.extend(Panel::top("warning").show(ui, |ui| self.warning_bar(ui)).inner);
        }
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
        if let Some(i) = self.hist.pending_compare.take() {
            self.compare_with(i);
        }
        self.dups.poll(&ctx);
        match tools::move_window(&ctx, &mut self.mover) {
            Some(tools::MoveEvent::Moved(id)) => self.take_out(id),
            Some(tools::MoveEvent::Failed(e)) => self.show_error(e),
            None => {}
        }
        self.dialogs(&ctx);
        self.cleanup.poll(&ctx);
        let mut trash_now = None;
        if let Some(tree) = &self.tree {
            actions.extend(cleanup::info_window(&ctx, tree, &mut self.cleanup, &self.settings));
            trash_now = cleanup::confirm_window(&ctx, tree, &mut self.cleanup);
        }
        let (changed, settings_error) = cleanup::settings_window(&ctx, &mut self.cleanup, &mut self.settings);
        if changed {
            self.save_settings();
        }
        if let Some(e) = settings_error {
            self.show_error(e);
        }
        if let Some(id) = trash_now {
            self.move_to_trash(id);
        }
        self.error_window(&ctx);
        for a in actions {
            self.apply(a, &ctx);
        }
        for c in cmds {
            self.command(c, &ctx);
        }
    }

    fn map(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let t = tr();
        let Some(tree) = &self.tree else { return Vec::new() };
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.chart, Chart::Map, t.map_title);
            ui.selectable_value(&mut self.chart, Chart::Ring, t.ring_title);
            ui.separator();
            ui.selectable_value(&mut self.measure, Measure::Size, t.show_size);
            ui.selectable_value(&mut self.measure, Measure::Allocated, t.show_allocated);
            ui.selectable_value(&mut self.measure, Measure::Files, t.show_count);
            ui.separator();
            let parent = tree.node(self.map_root).parent;
            if ui.add_enabled(parent != crate::tree::NO_PARENT, egui::Button::new(t.map_up)).clicked() {
                actions.push(Action::MapRoot(parent));
            }
            ui.add(
                egui::Label::new(egui::RichText::new(tree.path(self.map_root).display().to_string()).weak())
                    .truncate(),
            );
        });
        let (root, selected, measure, generation) =
            (self.map_root, self.selected, self.measure, self.generation);
        actions.extend(match self.chart {
            Chart::Map => map::show(ui, tree, root, selected, measure, generation, &mut self.map_cache),
            Chart::Ring => ring::show(ui, tree, root, selected, measure, generation, &mut self.ring_cache),
        });
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
