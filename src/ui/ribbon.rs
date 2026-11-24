//! The menu band at the top: tabs with groups of large and small buttons.

use eframe::egui;
use egui::{Color32, FontId, Rect, Response, Role, Sense, Ui, WidgetInfo, vec2};

use super::icons::{self, Icon};

/// Height of the button area of a group (without its caption).
const BAND: f32 = 72.0;

fn colors(ui: &Ui) -> (Color32, Color32) {
    let v = ui.visuals();
    let paper = if v.dark_mode { Color32::from_gray(70) } else { Color32::WHITE };
    (v.text_color(), paper)
}

fn background(ui: &Ui, rect: Rect, resp: &Response, enabled: bool, selected: bool) {
    let v = ui.visuals();
    if selected {
        ui.painter().rect(
            rect,
            4.0,
            v.selection.bg_fill.gamma_multiply(0.35),
            egui::Stroke::new(1.0, v.selection.bg_fill),
            egui::StrokeKind::Inside,
        );
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, v.widgets.hovered.weak_bg_fill);
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 4.0, v.widgets.active.weak_bg_fill);
    }
}

/// A large button: icon above, label below (up to two lines).
pub fn big(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool) -> Response {
    let (ink, paper) = colors(ui);
    let text_color = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    let galley = ui.painter().layout(label.to_owned(), FontId::proportional(12.0), text_color, 96.0);
    let w = galley.size().x.max(40.0) + 12.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BAND), sense);
    background(ui, rect, &resp, enabled, selected);
    let icon_rect = Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 21.0), vec2(30.0, 30.0));
    icons::draw(ui.painter(), icon_rect, icon, ink, paper, enabled);
    let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 40.0);
    ui.painter().galley(text_pos, galley, text_color);
    resp.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, label));
    resp
}

/// A small button: icon left, label right.
pub fn small(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool) -> Response {
    let (ink, paper) = colors(ui);
    let text_color = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(12.0), text_color);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 32.0, 21.0), sense);
    background(ui, rect, &resp, enabled, selected);
    let icon_rect = Rect::from_min_size(rect.min + vec2(4.0, 2.5), vec2(16.0, 16.0));
    icons::draw(ui.painter(), icon_rect, icon, ink, paper, enabled);
    let text_pos = egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, text_color);
    resp.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, label));
    resp
}

/// A group of buttons with its caption centred below and a line after it.
pub fn group(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    let inner = ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(BAND);
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui);
        });
        let width = ui.min_rect().width();
        ui.allocate_ui_with_layout(vec2(width, 14.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(caption).small().weak());
        });
    });
    let r = inner.response.rect;
    ui.add_space(4.0);
    let x = ui.cursor().left();
    ui.painter().vline(x, r.y_range().shrink(4.0), ui.visuals().widgets.noninteractive.bg_stroke);
    ui.add_space(6.0);
}

/// A column of up to three small buttons inside a group.
pub fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.5;
        add(ui);
    });
}
