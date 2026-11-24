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

fn swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

pub fn categories(ui: &mut egui::Ui, tree: &Tree, start: NodeId, stats: &[crate::category::CategoryStats]) {
    let t = tr();
    let total = tree.node(start).size;
    ui.label(egui::RichText::new(tree.path(start).display().to_string()).weak());
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(140.0).clip(true))
        .column(Column::initial(72.0))
        .column(Column::initial(84.0))
        .column(Column::initial(84.0))
        .column(Column::initial(110.0))
        .header(ROW, |mut h| {
            for label in [t.col_category, t.col_files, t.col_size, t.col_allocated, t.col_share] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, stats.len(), |mut row| {
                let s = &stats[row.index()];
                let [r, g, b] = s.category.rgb();
                let color = egui::Color32::from_rgb(r, g, b);
                row.col(|ui| {
                    swatch(ui, color);
                    ui.label(t.categories[s.category as usize]);
                });
                row.col(|ui| right(ui, format::count(s.files)));
                row.col(|ui| right(ui, format::size(s.size)));
                row.col(|ui| right(ui, egui::RichText::new(format::size(s.allocated)).weak()));
                row.col(|ui| {
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

pub fn ages(ui: &mut egui::Ui, tree: &Tree, start: NodeId, stats: &[crate::query::AgeStats; 7]) {
    let t = tr();
    let total: u64 = stats.iter().map(|s| s.size).sum();
    ui.label(egui::RichText::new(tree.path(start).display().to_string()).weak());
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(140.0).clip(true))
        .column(Column::initial(72.0))
        .column(Column::initial(84.0))
        .column(Column::initial(140.0))
        .header(ROW, |mut h| {
            for label in [t.col_age, t.col_files, t.col_size, t.col_share] {
                h.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(ROW, stats.len(), |mut row| {
                let i = row.index();
                let s = &stats[i];
                // From fresh green over yellow to old red; unknown in grey.
                let color = if i == 6 {
                    egui::Color32::from_gray(150)
                } else {
                    paint::hsv(140.0 - i as f32 * 28.0, 0.6, 0.85)
                };
                row.col(|ui| {
                    swatch(ui, color);
                    ui.label(t.age_groups[i]);
                });
                row.col(|ui| right(ui, format::count(s.files)));
                row.col(|ui| right(ui, format::size(s.size)));
                row.col(|ui| {
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

/// Seconds for the "changed more than … ago" choices (index 0 = any time).
pub const AGE_CHOICES: [Option<i64>; 6] = [
    None,
    Some(30 * crate::query::DAY),
    Some(crate::query::YEAR / 2),
    Some(crate::query::YEAR),
    Some(2 * crate::query::YEAR),
    Some(5 * crate::query::YEAR),
];

/// The search form; returns true when a condition changed.
pub fn search_form(ui: &mut egui::Ui, filter: &mut crate::query::Filter, min_mb: &mut f64) -> bool {
    use crate::category::Category;
    let t = tr();
    let before = (filter.clone(), *min_mb);
    egui::Grid::new("search_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        // Every field is linked to its label, so screen readers name it.
        let label = ui.label(t.search_name);
        ui.add(egui::TextEdit::singleline(&mut filter.name).hint_text(t.search_hint).desired_width(220.0))
            .labelled_by(label.id);
        ui.end_row();

        let label = ui.label(t.search_category);
        let current = filter.category.map_or(t.search_any, |c| t.categories[c as usize]);
        egui::ComboBox::from_id_salt("search_category")
            .selected_text(current)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut filter.category, None, t.search_any);
                for c in Category::ALL {
                    ui.selectable_value(&mut filter.category, Some(c), t.categories[c as usize]);
                }
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let label = ui.label(t.search_min_size);
        ui.add(egui::DragValue::new(min_mb).range(0.0..=10_000_000.0).speed(10.0).max_decimals(1))
            .labelled_by(label.id);
        ui.end_row();

        let label = ui.label(t.search_older);
        let pos = AGE_CHOICES.iter().position(|a| *a == filter.older_than).unwrap_or(0);
        egui::ComboBox::from_id_salt("search_age")
            .selected_text(t.search_ages[pos])
            .show_ui(ui, |ui| {
                for (i, a) in AGE_CHOICES.iter().enumerate() {
                    ui.selectable_value(&mut filter.older_than, *a, t.search_ages[i]);
                }
            })
            .response
            .labelled_by(label.id);
        ui.end_row();
    });
    if ui.button(t.search_reset).clicked() {
        *filter = crate::query::Filter::default();
        *min_mb = 0.0;
    }
    filter.min_size = (*min_mb * 1024.0 * 1024.0) as u64;
    before != (filter.clone(), *min_mb)
}
