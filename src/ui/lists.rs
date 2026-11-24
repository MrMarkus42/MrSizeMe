//! File types and largest files.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Action;
use super::paint;
use crate::format;
use crate::i18n::tr;
use crate::stats::TypeStats;
use crate::tree::{NodeId, Tree};

const ROW: f32 = 22.0;

fn right(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

pub fn types(ui: &mut egui::Ui, tree: &Tree, start: NodeId, stats: &[TypeStats]) {
    let t = tr();
    let dark = ui.visuals().dark_mode;
    let total = tree.node(start).size;
    ui.label(egui::RichText::new(tree.path(start).display().to_string()).weak());
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::initial(72.0))
        .column(Column::initial(84.0))
        .column(Column::initial(84.0))
        .column(Column::initial(110.0))
        .header(ROW, |mut h| {
            for label in [t.col_type, t.col_files, t.col_size, t.col_allocated, t.col_share] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, stats.len(), |mut row| {
                let s = &stats[row.index()];
                row.col(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, paint::type_color(&s.extension, dark));
                    if s.extension.is_empty() {
                        ui.label(egui::RichText::new(t.no_extension).italics());
                    } else {
                        ui.label(format!(".{}", s.extension));
                    }
                });
                row.col(|ui| right(ui, format::count(s.files)));
                row.col(|ui| right(ui, format::size(s.size)));
                row.col(|ui| right(ui, egui::RichText::new(format::size(s.allocated)).weak()));
                row.col(|ui| {
                    let color = paint::type_color(&s.extension, dark);
                    paint::share_bar(
                        ui,
                        format::share(s.size, total),
                        &format::percent(s.size, total),
                        color,
                    );
                });
            });
        });
}

pub fn largest(ui: &mut egui::Ui, tree: &Tree, files: &[NodeId], selected: Option<NodeId>) -> Vec<Action> {
    let t = tr();
    let mut actions = Vec::new();
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(200.0).at_least(100.0).clip(true))
        .column(Column::initial(84.0))
        .column(Column::initial(84.0))
        .column(Column::remainder().at_least(100.0).clip(true))
        .header(ROW, |mut h| {
            for label in [t.col_name, t.col_size, t.col_modified, t.col_path] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, files.len(), |mut row| {
                let id = files[row.index()];
                let n = tree.node(id);
                row.set_selected(selected == Some(id));
                row.col(|ui| {
                    paint::icon(ui, n.kind, 16.0);
                    ui.add(egui::Label::new(n.name_lossy()).truncate().selectable(false));
                });
                row.col(|ui| right(ui, format::size(n.size)));
                row.col(|ui| {
                    ui.label(format::date(n.modified));
                });
                row.col(|ui| {
                    let parent = tree.path(n.parent).display().to_string();
                    ui.add(egui::Label::new(egui::RichText::new(parent).weak()).truncate().selectable(false));
                });
                let resp = row.response();
                if resp.clicked() {
                    actions.push(Action::Select(id));
                }
                if resp.double_clicked() {
                    actions.push(Action::MapRoot(id));
                }
                resp.context_menu(|ui| super::folders::context_menu(ui, id, &mut actions));
            });
        });
    actions
}
