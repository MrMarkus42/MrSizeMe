//! Duplicates, cloud folders, programs and the move assistant.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Action;
use crate::i18n::tr;
use crate::tree::{Kind, NodeId, ROOT, Tree};
use crate::{apps, cloud, duplicates, format, mover};

const ROW: f32 = 22.0;

fn right(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

/// A file or folder name that can be clicked to show it in the folder list.
fn link(ui: &mut egui::Ui, tree: &Tree, path: &std::path::Path, text: String, actions: &mut Vec<Action>) {
    let resp = ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()));
    if resp.clicked()
        && let Some(id) = tree.find(path)
    {
        actions.push(Action::Reveal(id));
    }
}

// ---------------------------------------------------------------- duplicates

pub struct Dups {
    pub min_mb: f64,
    job: Option<(Arc<duplicates::Progress>, JoinHandle<Vec<duplicates::Group>>)>,
    pub groups: Option<Vec<duplicates::Group>>,
    /// Tree generation the groups belong to.
    generation: u64,
}

impl Default for Dups {
    fn default() -> Self {
        Dups { min_mb: 1.0, job: None, groups: None, generation: 0 }
    }
}

impl Dups {
    pub fn reset(&mut self) {
        if let Some((p, _)) = &self.job {
            p.cancel();
        }
        *self = Dups { min_mb: self.min_mb, ..Dups::default() };
    }

    fn start(&mut self, tree: &Tree, generation: u64) {
        let candidates = duplicates::candidates(tree, ROOT, (self.min_mb.max(0.0) * 1_048_576.0) as u64);
        let progress = Arc::new(duplicates::Progress::default());
        let p = Arc::clone(&progress);
        let handle = std::thread::Builder::new()
            .name("duplicates".into())
            .spawn(move || duplicates::find(candidates, &p))
            .expect("start duplicate search");
        self.job = Some((progress, handle));
        self.groups = None;
        self.generation = generation;
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some((_, h)) = &self.job {
            if h.is_finished() {
                let (_, h) = self.job.take().expect("checked");
                self.groups = Some(h.join().unwrap_or_default());
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
}

pub fn duplicates(ui: &mut egui::Ui, tree: &Tree, generation: u64, state: &mut Dups) -> Vec<Action> {
    let t = tr();
    let mut actions = Vec::new();
    ui.label(egui::RichText::new(t.dup_intro).weak());
    ui.horizontal(|ui| {
        let label = ui.label(t.dup_min_size);
        ui.add(egui::DragValue::new(&mut state.min_mb).range(0.0..=100_000.0).speed(1.0))
            .labelled_by(label.id);
        let running = state.job.is_some();
        if ui.add_enabled(!running, egui::Button::new(t.dup_start)).clicked() {
            state.start(tree, generation);
        }
        if running && ui.button(t.cancel).clicked() {
            state.reset();
        }
    });
    if let Some((p, _)) = &state.job {
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
        ui.add(egui::ProgressBar::new(done as f32 / total as f32).text(format!(
            "{}  {} / {}",
            t.dup_running,
            format::size(done),
            format::size(total)
        )));
        return actions;
    }
    let Some(groups) = &state.groups else { return actions };
    ui.separator();
    if groups.is_empty() {
        ui.label(t.dup_none);
        return actions;
    }
    if state.generation != generation {
        ui.colored_label(ui.visuals().warn_fg_color, t.dup_outdated);
    }
    let wasted: u64 = groups.iter().map(|g| g.wasted()).sum();
    ui.strong(
        t.dup_summary
            .replace("{groups}", &format::count(groups.len() as u64))
            .replace("{size}", &format::size(wasted)),
    );
    ui.label(egui::RichText::new(t.dup_hint).weak().small());
    egui::ScrollArea::vertical().id_salt("dups").auto_shrink([false; 2]).show(ui, |ui| {
        for (i, g) in groups.iter().take(500).enumerate() {
            ui.push_id(i, |ui| {
                ui.add_space(4.0);
                ui.strong(
                    t.dup_group
                        .replace("{n}", &g.paths.len().to_string())
                        .replace("{size}", &format::size(g.size)),
                );
                for path in &g.paths {
                    let id = tree.find(path);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(id.is_some(), egui::Button::new("🗑").small()).clicked()
                            && let Some(id) = id
                        {
                            actions.push(Action::AskTrash(id));
                        }
                        let shown = path.strip_prefix(&tree.root_path).unwrap_or(path).display().to_string();
                        if id.is_some() {
                            link(ui, tree, path, shown, &mut actions);
                        } else {
                            ui.label(egui::RichText::new(shown).strikethrough().weak());
                        }
                    });
                }
            });
        }
    });
    actions
}

// --------------------------------------------------------------------- cloud

/// A row of the cloud list.
pub struct CloudRow {
    pub name: String,
    pub path: PathBuf,
    pub usage: cloud::Usage,
    pub is_dir: bool,
}

/// One cloud folder with its biggest entries.
pub struct CloudPart {
    pub root: cloud::Root,
    pub usage: cloud::Usage,
    pub rows: Vec<CloudRow>,
}

pub fn cloud_parts(tree: &Tree, roots: &[cloud::Root]) -> Vec<CloudPart> {
    roots
        .iter()
        .filter_map(|r| {
            let id = cloud::node_of(tree, &r.path)?;
            let mut rows: Vec<CloudRow> = tree
                .children(id)
                .iter()
                .map(|&c| CloudRow {
                    name: tree.node(c).name_lossy().into_owned(),
                    path: tree.path(c),
                    usage: cloud::usage(tree, c),
                    is_dir: tree.node(c).is_dir(),
                })
                .collect();
            rows.sort_by_key(|r| std::cmp::Reverse(r.usage.local));
            rows.truncate(200);
            Some(CloudPart { root: r.clone(), usage: cloud::usage(tree, id), rows })
        })
        .collect()
}

/// Asks for "keep only online" of a path with this much local space.
pub type FreeUpRequest = (PathBuf, String, u64);

pub fn cloud(ui: &mut egui::Ui, tree: &Tree, parts: &[CloudPart]) -> (Vec<Action>, Option<FreeUpRequest>) {
    let t = tr();
    let mut actions = Vec::new();
    let mut request = None;
    if parts.is_empty() {
        ui.label(t.cloud_none);
        return (actions, request);
    }
    if !cloud::can_free_up() {
        ui.label(egui::RichText::new(t.cloud_unsupported).weak());
    }
    egui::ScrollArea::vertical().id_salt("cloud").auto_shrink([false; 2]).show(ui, |ui| {
        for (i, part) in parts.iter().enumerate() {
            ui.push_id(i, |ui| {
                ui.add_space(4.0);
                ui.heading(&part.root.name);
                ui.label(egui::RichText::new(part.root.path.display().to_string()).weak());
                ui.label(
                    t.cloud_summary
                        .replace("{local}", &format::size(part.usage.local))
                        .replace("{online}", &format::size(part.usage.online))
                        .replace("{n}", &format::count(part.usage.online_files)),
                );
                TableBuilder::new(ui)
                    .striped(true)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::remainder().at_least(100.0).clip(true))
                    .column(Column::initial(96.0).at_least(70.0).clip(true))
                    .column(Column::initial(80.0).at_least(60.0).clip(true))
                    .column(Column::auto())
                    .header(ROW, |mut h| {
                        for label in [t.col_name, t.col_local, t.col_online, ""] {
                            h.col(|ui| {
                                ui.strong(label);
                            });
                        }
                    })
                    .body(|mut body| {
                        for r in &part.rows {
                            body.row(ROW, |mut row| {
                                row.col(|ui| {
                                    let name = if r.is_dir { format!("{}/", r.name) } else { r.name.clone() };
                                    link(ui, tree, &r.path, name, &mut actions);
                                });
                                row.col(|ui| right(ui, format::size(r.usage.local)));
                                row.col(|ui| {
                                    right(ui, egui::RichText::new(format::size(r.usage.online)).weak())
                                });
                                row.col(|ui| {
                                    let can = cloud::can_free_up() && r.usage.local > 0;
                                    if ui
                                        .add_enabled(can, egui::Button::new(t.cloud_free_up).small())
                                        .clicked()
                                    {
                                        request = Some((r.path.clone(), r.name.clone(), r.usage.local));
                                    }
                                });
                            });
                        }
                    });
            });
        }
    });
    (actions, request)
}

// ------------------------------------------------------------------ programs

pub fn programs(ui: &mut egui::Ui, tree: &Tree, list: &[apps::Usage]) -> Vec<Action> {
    let t = tr();
    let mut actions = Vec::new();
    ui.label(egui::RichText::new(t.apps_intro).weak());
    if list.is_empty() {
        ui.label(t.apps_none);
        return actions;
    }
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(160.0).clip(true))
        .column(Column::initial(90.0))
        .column(Column::initial(110.0))
        .column(Column::initial(90.0))
        .header(ROW, |mut h| {
            for label in [t.col_name, t.col_program, t.col_data, t.col_total] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, list.len(), |mut row| {
                let u = &list[row.index()];
                row.col(|ui| {
                    let resp = ui.add(egui::Label::new(&u.app.name).truncate().sense(egui::Sense::click()));
                    let mut tip: Vec<String> =
                        u.app.location.iter().map(|p| p.display().to_string()).collect();
                    tip.extend(u.data_found.iter().map(|p| p.display().to_string()));
                    if let Some(r) = u.app.reported {
                        tip.push(t.apps_reported.replace("{size}", &format::size(r)));
                    }
                    let resp = resp.on_hover_text(tip.join("\n"));
                    let target = u.node.or_else(|| u.data_found.first().and_then(|p| tree.find(p)));
                    if resp.clicked()
                        && let Some(id) = target
                    {
                        actions.push(Action::Reveal(id));
                    }
                });
                row.col(|ui| right(ui, format::size(u.program)));
                row.col(|ui| right(ui, format::size(u.data)));
                row.col(|ui| right(ui, egui::RichText::new(format::size(u.total())).strong()));
            });
        });
    actions
}

// ------------------------------------------------------------------- moving

#[derive(Default)]
pub struct Move {
    /// The folder to move (path and size, links inside).
    pub source: Option<(NodeId, PathBuf, u64, bool)>,
    pub target: String,
    pub link: bool,
    pub delete: bool,
    job: Option<(Arc<mover::Progress>, JoinHandle<std::io::Result<mover::Report>>)>,
    pub result: Option<String>,
}

/// What the move window asks for after drawing.
pub enum MoveEvent {
    /// Moved: take the node out of the tree.
    Moved(NodeId),
    Failed(String),
}

impl Move {
    pub fn open(&mut self, tree: &Tree, id: NodeId) {
        if self.job.is_some() {
            return;
        }
        // Links inside the folder (Windows cannot copy them safely).
        let mut links = false;
        let mut stack = vec![id];
        while let Some(d) = stack.pop() {
            for &c in tree.children(d) {
                match tree.node(c).kind {
                    Kind::Dir => stack.push(c),
                    Kind::Link => links = true,
                    _ => {}
                }
            }
        }
        *self = Move {
            source: Some((id, tree.path(id), tree.node(id).size, links)),
            target: String::new(),
            link: true,
            delete: false,
            job: None,
            result: None,
        };
    }

    fn plan(&self) -> Option<mover::Plan> {
        let (_, path, _, _) = self.source.as_ref()?;
        Some(mover::Plan {
            source: path.clone(),
            target_dir: PathBuf::from(self.target.trim()),
            link: self.link,
            original: if self.delete { mover::Original::Delete } else { mover::Original::Trash },
        })
    }
}

fn problem_text(p: &mover::Problem) -> String {
    let t = tr();
    use mover::Problem as P;
    match p {
        P::Protected => t.move_p_protected.into(),
        P::NotAFolder => t.move_p_not_folder.into(),
        P::TargetMissing => t.move_p_target_missing.into(),
        P::TargetInsideSource => t.move_p_inside.into(),
        P::TargetExists => t.move_p_exists.into(),
        P::NoSpace(need, free) => {
            t.move_p_space.replace("{need}", &format::size(*need)).replace("{free}", &format::size(*free))
        }
        P::InCloud => t.move_p_cloud.into(),
        P::HasLinks => t.move_p_links.into(),
        P::LinkToNetwork => t.move_p_network.into(),
        P::SameDisk => t.move_p_same_disk.into(),
    }
}

pub fn move_window(ctx: &egui::Context, state: &mut Move) -> Option<MoveEvent> {
    let t = tr();
    let (id, path, size, links) = state.source.clone()?;
    let mut event = None;
    // A finished job.
    if let Some((_, h)) = &state.job {
        if h.is_finished() {
            let (_, h) = state.job.take().expect("checked");
            match h.join() {
                Ok(Ok(r)) => {
                    let mut text = t
                        .move_done
                        .replace("{files}", &format::count(r.files))
                        .replace("{size}", &format::size(r.bytes));
                    if r.linked {
                        text = format!("{text} {}", t.move_done_link);
                    }
                    if !r.trashed && !state.delete {
                        text = format!("{text} {}", t.move_done_kept);
                    } else {
                        event = Some(MoveEvent::Moved(id));
                    }
                    state.result = Some(text);
                }
                Ok(Err(e)) => {
                    event = Some(MoveEvent::Failed(format!("{}\n{}", path.display(), format::io_error(&e))))
                }
                Err(_) => event = Some(MoveEvent::Failed("move: internal error".into())),
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    let mut open = true;
    egui::Window::new(t.move_title)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(t.move_source);
                ui.label(format!("{}  ({})", path.display(), format::size(size)));
            });
            let busy = state.job.is_some();
            ui.horizontal(|ui| {
                let label = ui.strong(t.move_target);
                ui.add_enabled(!busy, egui::TextEdit::singleline(&mut state.target)).labelled_by(label.id);
                if ui.add_enabled(!busy, egui::Button::new(t.move_choose)).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    state.target = dir.display().to_string();
                }
            });
            ui.add_enabled(!busy, egui::Checkbox::new(&mut state.link, t.move_link));
            ui.add_enabled_ui(!busy, |ui| {
                ui.radio_value(&mut state.delete, false, t.move_original_trash);
                ui.radio_value(&mut state.delete, true, t.move_original_delete);
            });
            ui.label(egui::RichText::new(t.move_explain).weak().small());
            ui.separator();
            if let Some((p, _)) = &state.job {
                let (done, total) =
                    (p.copied.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).text(format!(
                    "{}  {} / {}",
                    t.move_running,
                    format::size(done),
                    format::size(total)
                )));
                if ui.button(t.cancel).clicked() {
                    p.cancel();
                }
                return;
            }
            if let Some(r) = &state.result {
                ui.strong(r);
                return;
            }
            let Some(plan) = state.plan() else { return };
            let ready = !state.target.trim().is_empty();
            let problems = if ready {
                let free = crate::disk::space(&plan.target_dir).ok().map(|s| s.free);
                mover::check(&plan, size, links, free)
            } else {
                Vec::new()
            };
            for p in &problems {
                let color = if p.blocks() { ui.visuals().error_fg_color } else { ui.visuals().warn_fg_color };
                ui.colored_label(color, problem_text(p));
            }
            let can = ready && problems.iter().all(|p| !p.blocks());
            if ui.add_enabled(can, egui::Button::new(t.move_start)).clicked() {
                let progress = Arc::new(mover::Progress::default());
                let pr = Arc::clone(&progress);
                let handle = std::thread::Builder::new()
                    .name("move".into())
                    .spawn(move || mover::run(&plan, &pr))
                    .expect("start move");
                log::info!("move started");
                state.job = Some((progress, handle));
            }
        });
    // The window can only be closed while nothing runs.
    if !open && state.job.is_none() {
        state.source = None;
    }
    event
}
