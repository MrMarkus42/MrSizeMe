//! The history list: earlier scans, comparison and forecast.

use std::path::Path;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::Action;
use crate::format;
use crate::history::{self, Change, Outlook, Store, Summary};
use crate::i18n::{Texts, tr};
use crate::tree::{ROOT, Tree};

const ROW: f32 = 22.0;
const MAX_CHANGES: usize = 300;

/// A comparison that was worked out.
pub struct Compared {
    pub time: i64,
    pub min_size: u64,
    pub changes: Vec<Change>,
    /// Change of the whole scanned folder.
    pub total: i64,
}

#[derive(Default)]
pub struct State {
    /// Saved scans of the shown folder, oldest first.
    pub entries: Vec<Summary>,
    pub compared: Option<Compared>,
    /// The automatic comparison was tried for this tree.
    pub auto_done: bool,
    pub confirm_clear: bool,
    /// The user asked to compare with this entry (done after drawing).
    pub pending_compare: Option<usize>,
}

impl State {
    pub fn reset(&mut self) {
        *self = State { confirm_clear: self.confirm_clear, ..State::default() };
    }

    pub fn reload(&mut self, store: Option<&Store>, root: &Path) {
        self.entries = store.map(|s| s.list(root)).unwrap_or_default();
    }
}

/// Compares the tree with a saved scan.
pub fn compare(store: &Store, tree: &Tree, summary: &Summary) -> std::io::Result<Compared> {
    let old = store.load(summary, &tree.root_path)?;
    // Both sides with the same limits, so the cut-off does not look like a change.
    let min_size = summary.min_size.max(history::default_min_size(tree));
    let new = tree.copy(min_size, 8, 30_000, false);
    let changes = history::compare(&old, &new, min_size.max(1 << 20), MAX_CHANGES);
    Ok(Compared {
        time: summary.time,
        min_size,
        changes,
        total: tree.root().size as i64 - summary.size as i64,
    })
}

/// "in about 4 months"
pub fn duration(t: &Texts, days: f64) -> String {
    if days < 1.5 {
        return t.time_one_day.to_string();
    }
    let (n, text) = if days < 14.0 {
        (days.round(), t.time_days)
    } else if days < 60.0 {
        ((days / 7.0).round(), t.time_weeks)
    } else if days < 730.0 {
        ((days / 30.4).round(), t.time_months)
    } else {
        ((days / 365.0).round(), t.time_years)
    };
    text.replace("{n}", &format!("{n}"))
}

fn signed(delta: i64) -> String {
    let s = format::size(delta.unsigned_abs());
    match delta.signum() {
        1 => format!("+{s}"),
        -1 => format!("−{s}"),
        _ => s,
    }
}

fn delta_color(ui: &egui::Ui, delta: i64) -> egui::Color32 {
    let dark = ui.visuals().dark_mode;
    match delta.signum() {
        1 => {
            if dark {
                egui::Color32::from_rgb(251, 146, 60)
            } else {
                egui::Color32::from_rgb(194, 65, 12)
            }
        }
        -1 => {
            if dark {
                egui::Color32::from_rgb(74, 222, 128)
            } else {
                egui::Color32::from_rgb(21, 128, 61)
            }
        }
        _ => ui.visuals().weak_text_color(),
    }
}

fn right(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

/// What the list asks for besides normal actions.
pub enum Request {
    /// Compare with this saved scan.
    Compare(usize),
}

/// The forecast sentences.
fn forecast(ui: &mut egui::Ui, t: &Texts, entries: &[Summary]) {
    ui.strong(t.forecast_heading);
    let sizes: Vec<(i64, f64)> = entries.iter().map(|e| (e.time, e.size as f64)).collect();
    match history::trend(&sizes) {
        None => {
            ui.label(t.forecast_unknown);
            return;
        }
        Some(per_day) => {
            let month = per_day * 30.4;
            // Less than 1 MB per month counts as standing still.
            let text = if month.abs() < 1e6 {
                t.forecast_folder_steady.to_string()
            } else if month > 0.0 {
                t.forecast_folder_grows.replace("{size}", &format::size(month as u64))
            } else {
                t.forecast_folder_shrinks.replace("{size}", &format::size((-month) as u64))
            };
            ui.label(text);
        }
    }
    let free: Vec<(i64, u64)> = entries.iter().filter_map(|e| e.space.map(|s| (e.time, s.free))).collect();
    match history::outlook(&free) {
        Outlook::FullIn(days) => {
            let text = t.forecast_full_in.replace("{time}", &duration(t, days));
            ui.label(egui::RichText::new(text).color(ui.visuals().warn_fg_color).strong());
        }
        Outlook::NotFilling => {
            ui.label(t.forecast_not_filling);
        }
        Outlook::Unknown => {}
    }
}

/// A small line chart of the size over time.
fn chart(ui: &mut egui::Ui, entries: &[Summary]) {
    if entries.len() < 2 {
        return;
    }
    let width = ui.available_width().min(640.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 90.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let v = ui.visuals();
    p.rect_filled(rect, 4.0, v.extreme_bg_color);
    let (t0, t1) = (entries[0].time, entries[entries.len() - 1].time);
    // The scale runs from a bit below the smallest to the largest size, so growth is visible.
    let hi = entries.iter().map(|e| e.size).max().unwrap_or(1).max(1);
    let lo = entries.iter().map(|e| e.size).min().unwrap_or(0);
    let pad = ((hi - lo) / 8).max(hi / 100).max(1);
    let (lo, hi) = (lo.saturating_sub(pad), hi + pad);
    let range = (hi - lo).max(1) as f32;
    let font = egui::FontId::proportional(10.0);
    let weak = v.weak_text_color();
    p.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        format::size(hi),
        font.clone(),
        weak,
    );
    p.text(
        rect.left_bottom() + egui::vec2(4.0, -2.0),
        egui::Align2::LEFT_BOTTOM,
        format::size(lo),
        font,
        weak,
    );
    let inner =
        egui::Rect::from_min_max(rect.min + egui::vec2(70.0, 10.0), rect.max - egui::vec2(12.0, 10.0));
    let span = (t1 - t0).max(1) as f32;
    let at = |e: &Summary| {
        egui::pos2(
            inner.left() + (e.time - t0) as f32 / span * inner.width(),
            inner.bottom() - (e.size.saturating_sub(lo)) as f32 / range * inner.height(),
        )
    };
    let color = egui::Color32::from_rgb(20, 184, 166);
    let pts: Vec<egui::Pos2> = entries.iter().map(at).collect();
    p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(2.0, color)));
    for q in &pts {
        p.circle_filled(*q, 3.0, color);
    }
    // The point under the mouse shows its date and size.
    if let Some(m) = resp.hover_pos()
        && let Some((i, _)) =
            pts.iter().enumerate().min_by(|a, b| (a.1.x - m.x).abs().total_cmp(&(b.1.x - m.x).abs()))
    {
        let e = &entries[i];
        resp.on_hover_text(format!("{}  ·  {}", format::date_time(e.time), format::size(e.size)));
    }
}

/// The whole history list. `scan_time` is the time of the shown scan.
pub fn show(
    ui: &mut egui::Ui,
    tree: &Tree,
    scan_time: Option<i64>,
    state: &State,
    available: bool,
    keep: bool,
) -> (Vec<Action>, Option<Request>) {
    let t = tr();
    let mut actions = Vec::new();
    let mut request = None;
    ui.label(egui::RichText::new(format!("{} {}", t.history_title, tree.root_path.display())).weak());
    if !available {
        ui.label(t.history_unavailable);
        return (actions, request);
    }
    if !keep {
        ui.colored_label(ui.visuals().warn_fg_color, t.history_off);
    }
    let others = state.entries.iter().filter(|e| Some(e.time) != scan_time).count();
    if others == 0 {
        ui.label(t.history_none);
        return (actions, request);
    }
    egui::ScrollArea::vertical().id_salt("history_scroll").show(ui, |ui| {
        forecast(ui, t, &state.entries);
        ui.add_space(6.0);
        ui.label(egui::RichText::new(t.chart_size_over_time).weak());
        chart(ui, &state.entries);
        ui.add_space(6.0);

        // Saved scans, newest first.
        let rows: Vec<usize> = (0..state.entries.len()).rev().collect();
        let height = (rows.len() as f32 + 1.0) * ROW + 6.0;
        ui.push_id("history_table", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .vscroll(false)
                .min_scrolled_height(height.min(260.0))
                .max_scroll_height(260.0)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(130.0))
                .column(Column::initial(84.0))
                .column(Column::initial(90.0))
                .column(Column::initial(84.0))
                .column(Column::remainder().at_least(90.0))
                .header(ROW, |mut h| {
                    for label in [t.col_date, t.col_size, t.col_change, t.col_free, ""] {
                        h.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|mut body| {
                    for &i in &rows {
                        let e = &state.entries[i];
                        body.row(ROW, |mut row| {
                            row.col(|ui| {
                                ui.label(format::date_time(e.time));
                            });
                            row.col(|ui| right(ui, format::size(e.size)));
                            row.col(|ui| {
                                if i > 0 {
                                    let d = e.size as i64 - state.entries[i - 1].size as i64;
                                    let color = delta_color(ui, d);
                                    right(ui, egui::RichText::new(signed(d)).color(color));
                                }
                            });
                            row.col(|ui| {
                                if let Some(s) = e.space {
                                    right(ui, egui::RichText::new(format::size(s.free)).weak());
                                }
                            });
                            row.col(|ui| {
                                if Some(e.time) == scan_time {
                                    ui.label(egui::RichText::new(t.this_scan).italics().weak());
                                } else {
                                    let active = state.compared.as_ref().is_some_and(|c| c.time == e.time);
                                    if ui.selectable_label(active, t.btn_compare).clicked() {
                                        request = Some(Request::Compare(i));
                                    }
                                }
                            });
                        });
                    }
                });
        });

        if let Some(c) = &state.compared {
            ui.add_space(8.0);
            ui.separator();
            let date = format::date(c.time);
            ui.strong(t.compare_with.replace("{date}", &format::date_time(c.time)));
            // The headline: the biggest single change.
            let headline = match c.changes.first() {
                Some(top) if top.delta() > 0 => t
                    .compare_grown
                    .replace("{date}", &date)
                    .replace("{name}", &top.path.display().to_string())
                    .replace("{size}", &format::size(top.delta().unsigned_abs())),
                Some(top) => t
                    .compare_shrunk
                    .replace("{date}", &date)
                    .replace("{name}", &top.path.display().to_string())
                    .replace("{size}", &format::size(top.delta().unsigned_abs())),
                None => t.compare_same.replace("{date}", &date),
            };
            ui.label(egui::RichText::new(headline).size(15.0));
            let root_name = tree.root_path.display().to_string();
            ui.horizontal(|ui| {
                ui.label(format!("{root_name}:"));
                ui.label(egui::RichText::new(signed(c.total)).color(delta_color(ui, c.total)).strong());
            });
            if !c.changes.is_empty() {
                ui.label(egui::RichText::new(t.compare_hint).weak().small());
                changes_table(ui, t, tree, c, &mut actions);
            }
        }
    });
    (actions, request)
}

fn changes_table(ui: &mut egui::Ui, t: &Texts, tree: &Tree, c: &Compared, actions: &mut Vec<Action>) {
    let limit = format::size(c.min_size);
    ui.push_id("changes_table", |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(140.0).clip(true))
            .column(Column::initial(84.0))
            .column(Column::initial(84.0))
            .column(Column::initial(96.0))
            .header(ROW, |mut h| {
                for label in [t.col_name, t.col_before, t.col_now, t.col_change] {
                    h.col(|ui| {
                        ui.strong(label);
                    });
                }
            })
            .body(|mut body| {
                for ch in &c.changes {
                    body.row(ROW, |mut row| {
                        row.col(|ui| {
                            let icon = if ch.is_dir { "📁" } else { "📄" };
                            let text = format!("{icon} {}", ch.path.display());
                            let found = tree.find(&tree.root_path.join(&ch.path)).filter(|&id| id != ROOT);
                            let resp = ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()));
                            if let Some(id) = found
                                && resp.clicked()
                            {
                                actions.push(Action::Reveal(id));
                            }
                        });
                        row.col(|ui| match ch.old {
                            Some(v) => right(ui, format::size(v)),
                            None => {
                                ui.label(egui::RichText::new(t.change_new).italics())
                                    .on_hover_text(t.tip_change_new.replace("{size}", &limit));
                            }
                        });
                        row.col(|ui| match ch.new {
                            Some(v) => right(ui, format::size(v)),
                            None => {
                                ui.label(egui::RichText::new(t.change_gone).italics())
                                    .on_hover_text(t.tip_change_gone.replace("{size}", &limit));
                            }
                        });
                        row.col(|ui| {
                            let d = ch.delta();
                            let color = delta_color(ui, d);
                            right(ui, egui::RichText::new(signed(d)).color(color).strong());
                        });
                    });
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        let t = &crate::i18n::DE;
        assert_eq!(duration(t, 3.2), "3 Tagen");
        assert_eq!(duration(t, 0.2), "einem Tag");
        assert_eq!(duration(t, 21.0), "3 Wochen");
        assert_eq!(duration(t, 122.0), "4 Monaten");
        assert_eq!(duration(t, 1000.0), "3 Jahren");
    }
}
