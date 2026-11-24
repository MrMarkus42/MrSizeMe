//! Small drawings: icons, bars, colours.

use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::i18n::tr;
use crate::tree::Kind;

/// Colour of a file type: the same extension always gets the same colour.
pub fn type_color(ext: &str, dark: bool) -> Color32 {
    if ext.is_empty() {
        return if dark { Color32::from_gray(120) } else { Color32::from_gray(165) };
    }
    // FNV-1a: small and stable across versions and systems.
    let mut h: u32 = 0x811c_9dc5;
    for b in ext.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    let hue = (h % 360) as f32 / 360.0;
    let sat = 0.45 + ((h >> 9) % 30) as f32 / 100.0;
    let val = if dark { 0.70 } else { 0.86 };
    egui::ecolor::Hsva::new(hue, sat, val, 1.0).into()
}

/// Colour of the share bars by depth (soft blue tones).
pub fn bar_color(depth: usize, dark: bool) -> Color32 {
    const LIGHT: [Color32; 4] = [
        Color32::from_rgb(59, 130, 246),
        Color32::from_rgb(20, 184, 166),
        Color32::from_rgb(245, 158, 11),
        Color32::from_rgb(139, 92, 246),
    ];
    let c = LIGHT[depth % LIGHT.len()];
    if dark { c.gamma_multiply(0.85) } else { c }
}

/// Folder or file icon in a square of `size` pixels.
pub fn icon(ui: &mut egui::Ui, kind: Kind, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let dark = ui.visuals().dark_mode;
    let r = rect.shrink(size * 0.12);
    match kind {
        Kind::Dir => {
            let tab = Rect::from_min_size(r.min, vec2(r.width() * 0.45, r.height() * 0.25));
            let body = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.15), r.max);
            let back = Color32::from_rgb(217, 119, 6);
            let front = Color32::from_rgb(251, 191, 36);
            p.rect_filled(tab, 1.5, back);
            p.rect_filled(body, 2.0, front);
        }
        Kind::File | Kind::Other => {
            let paper = if dark { Color32::from_gray(200) } else { Color32::WHITE };
            let ink = if dark { Color32::from_gray(90) } else { Color32::from_gray(130) };
            let page = Rect::from_center_size(r.center(), vec2(r.width() * 0.72, r.height()));
            p.rect(page, 1.5, paper, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
            for i in 1..4 {
                let y = page.min.y + page.height() * i as f32 / 4.5 + 1.0;
                p.line_segment([pos2(page.min.x + 2.5, y), pos2(page.max.x - 2.5, y)], Stroke::new(1.0, ink));
            }
        }
        Kind::Link => {
            let c = ui.visuals().weak_text_color();
            p.circle_stroke(r.center(), r.width() * 0.35, Stroke::new(1.5, c));
            p.line_segment([r.center(), r.right_top()], Stroke::new(1.5, c));
        }
    }
}

/// Triangle to open or close a folder; returns true when clicked.
pub fn expander(ui: &mut egui::Ui, open: bool, size: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let color = if resp.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
    let c = rect.center();
    let s = size * 0.22;
    let pts: Vec<Pos2> = if open {
        vec![pos2(c.x - s, c.y - s * 0.6), pos2(c.x + s, c.y - s * 0.6), pos2(c.x, c.y + s * 0.8)]
    } else {
        vec![pos2(c.x - s * 0.6, c.y - s), pos2(c.x + s * 0.8, c.y), pos2(c.x - s * 0.6, c.y + s)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
    resp.clicked()
}

/// Share bar with the percentage on top.
pub fn share_bar(ui: &mut egui::Ui, share: f64, text: &str, color: Color32) {
    let size = vec2(ui.available_width().max(20.0), ui.available_height().min(18.0));
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter();
    let rect = rect.shrink2(vec2(2.0, 2.0));
    p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let mut fill = rect;
    fill.set_width(rect.width() * share as f32);
    p.rect_filled(fill, 3.0, color.gamma_multiply(0.75));
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(11.5),
        ui.visuals().strong_text_color(),
    );
}

/// The empty window before the first scan.
pub fn welcome(ui: &mut egui::Ui, busy: bool) {
    ui.centered_and_justified(|ui| {
        if busy {
            ui.spinner();
        } else {
            ui.label(egui::RichText::new(tr().welcome).size(16.0).weak());
        }
    });
}
