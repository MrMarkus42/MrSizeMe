//! The folder tree as a table.

use std::collections::HashSet;

use eframe::egui;
use egui::RichText;
use egui_extras::{Column, TableBuilder};

use super::Action;
use super::paint;
use crate::format;
use crate::i18n::tr;
use crate::tree::{Kind, NO_PARENT, NodeId, ROOT, SortKey, Tree, flags};

const ROW: f32 = 22.0;
const INDENT: f32 = 14.0;

/// The rows that are visible with the given folders opened: (node, depth).
pub fn visible_rows(tree: &Tree, expanded: &HashSet<NodeId>) -> Vec<(NodeId, u16)> {
    let mut out = Vec::new();
    let mut stack = vec![(ROOT, 0u16)];
    while let Some((id, depth)) = stack.pop() {
        out.push((id, depth));
        if expanded.contains(&id) {
            for &c in tree.children(id).iter().rev() {
                stack.push((c, depth.saturating_add(1)));
            }
        }
    }
    out
}

fn header(ui: &mut egui::Ui, label: &str, key: SortKey, sort: (SortKey, bool), actions: &mut Vec<Action>) {
    let arrow = match (sort.0 == key, sort.1) {
        (false, _) => "",
        (true, false) => " ⏷",
        (true, true) => " ⏶",
    };
    let resp =
        ui.add(egui::Button::new(egui::RichText::new(format!("{label}{arrow}")).strong()).frame(false));
    if resp.clicked() {
        actions.push(Action::Sort(key));
    }
}

pub fn show(
    ui: &mut egui::Ui,
    tree: &Tree,
    expanded: &HashSet<NodeId>,
    selected: Option<NodeId>,
    sort: (SortKey, bool),
    scroll_to_selected: bool,
) -> Vec<Action> {
    let t = tr();
    let rows = visible_rows(tree, expanded);
    let dark = ui.visuals().dark_mode;
    let mut actions = Vec::new();

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(180.0).clip(true))
        .column(Column::initial(84.0).at_least(60.0))
        .column(Column::initial(84.0).at_least(60.0))
        .column(Column::initial(110.0).at_least(60.0))
        .column(Column::initial(72.0).at_least(50.0))
        .column(Column::initial(64.0).at_least(50.0))
        .column(Column::initial(84.0).at_least(60.0));
    if scroll_to_selected && let Some(pos) = selected.and_then(|s| rows.iter().position(|r| r.0 == s)) {
        table = table.scroll_to_row(pos, Some(egui::Align::Center));
    }

    table
        .header(ROW, |mut h| {
            h.col(|ui| header(ui, t.col_name, SortKey::Name, sort, &mut actions));
            h.col(|ui| header(ui, t.col_size, SortKey::Size, sort, &mut actions));
            h.col(|ui| header(ui, t.col_allocated, SortKey::Allocated, sort, &mut actions));
            h.col(|ui| {
                ui.strong(t.col_share);
            });
            h.col(|ui| header(ui, t.col_files, SortKey::Files, sort, &mut actions));
            h.col(|ui| {
                ui.strong(t.col_folders);
            });
            h.col(|ui| header(ui, t.col_modified, SortKey::Modified, sort, &mut actions));
        })
        .body(|body| {
            body.rows(ROW, rows.len(), |mut row| {
                let (id, depth) = rows[row.index()];
                let n = tree.node(id);
                row.set_selected(selected == Some(id));
                let parent_size = if n.parent == NO_PARENT { n.size } else { tree.node(n.parent).size };

                row.col(|ui| {
                    ui.add_space(f32::from(depth) * INDENT);
                    if n.is_dir() && n.child_count > 0 {
                        if paint::expander(ui, expanded.contains(&id), 16.0) {
                            actions.push(Action::Toggle(id));
                        }
                    } else {
                        ui.add_space(16.0 + ui.spacing().item_spacing.x);
                    }
                    paint::icon(ui, n.kind, 16.0);
                    let name = if id == ROOT {
                        tree.root_path.display().to_string()
                    } else {
                        n.name_lossy().into_owned()
                    };
                    let mut label = egui::RichText::new(name);
                    if n.has(flags::HARDLINK_DUP) || n.kind == Kind::Link || n.has(flags::SKIPPED) {
                        label = label.weak();
                    }
                    let resp = ui.add(egui::Label::new(label).truncate().selectable(false));
                    if n.has(flags::ERROR) {
                        let warn = ui.colored_label(ui.visuals().warn_fg_color, "⚠");
                        if let Some(e) = tree.errors.get(&id) {
                            warn.on_hover_text(e);
                        }
                    }
                    let hint = if n.has(flags::NOT_SCANNED) {
                        Some(t.not_scanned)
                    } else if n.has(flags::SKIPPED) {
                        Some(t.skipped)
                    } else if n.has(flags::HARDLINK_DUP) {
                        Some(t.hardlink)
                    } else if n.kind == Kind::Link {
                        Some(t.link)
                    } else {
                        None
                    };
                    if let Some(h) = hint {
                        resp.on_hover_text(h);
                    }
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format::size(n.size));
                    });
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format::size(n.allocated)).weak());
                    });
                });
                row.col(|ui| {
                    let share = format::share(n.size, parent_size);
                    paint::share_bar(
                        ui,
                        share,
                        &format::percent(n.size, parent_size),
                        paint::bar_color(depth.into(), dark),
                    );
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let files = if n.is_dir() { n.files } else { 1 };
                        ui.label(format::count(files));
                    });
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if n.is_dir() {
                            ui.label(format::count(n.dirs));
                        }
                    });
                });
                row.col(|ui| {
                    ui.label(format::date(n.modified));
                });

                let resp = row.response();
                if resp.clicked() {
                    actions.push(Action::Select(id));
                }
                if resp.double_clicked() {
                    if n.is_dir() && n.child_count > 0 {
                        actions.push(Action::Toggle(id));
                    }
                    actions.push(Action::MapRoot(id));
                }
                resp.context_menu(|ui| context_menu(ui, id, &mut actions));
            });
        });
    actions
}

/// The right-click menu for a folder or file (shared with the tile map).
pub(crate) fn context_menu(ui: &mut egui::Ui, id: NodeId, actions: &mut Vec<Action>) {
    let t = tr();
    actions.push(Action::Select(id));
    if ui.button(RichText::new(format!("{} – {}", t.btn_what_is, t.ai_heading)).strong()).clicked() {
        actions.push(Action::Info(id));
        ui.close();
    }
    ui.separator();
    if ui.button(t.open_folder).clicked() {
        actions.push(Action::OpenFolder(id));
        ui.close();
    }
    if ui.button(t.copy_path).clicked() {
        actions.push(Action::CopyPath(id));
        ui.close();
    }
    if ui.button(t.show_in_map).clicked() {
        actions.push(Action::MapRoot(id));
        ui.close();
    }
    ui.separator();
    if ui.add_enabled(id != ROOT, egui::Button::new(format!("{} …", t.btn_move))).clicked() {
        actions.push(Action::AskMove(id));
        ui.close();
    }
    if ui.add_enabled(id != ROOT, egui::Button::new(format!("{} …", t.btn_trash))).clicked() {
        actions.push(Action::AskTrash(id));
        ui.close();
    }
}
