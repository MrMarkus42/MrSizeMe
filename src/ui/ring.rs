//! The ring chart: the chosen folder in the middle, its contents as rings.

use std::f32::consts::TAU;

use eframe::egui;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Stroke, Vec2};

use super::map::value_text;
use super::{Action, paint};
use crate::format;
use crate::i18n::tr;
use crate::sunburst::{self, Segment};
use crate::tree::{Measure, NO_PARENT, NodeId, Tree};

/// Number of rings around the middle.
const RINGS: u16 = 6;
/// Parts thinner than this (in turns) are not drawn.
const MIN_SPAN: f32 = 0.0015;

pub struct Cache {
    key: (u64, NodeId, Measure),
    segments: Vec<Segment>,
}

struct Geometry {
    center: Pos2,
    inner: f32,
    ring: f32,
}

impl Geometry {
    fn radii(&self, depth: u16) -> (f32, f32) {
        let r0 = self.inner + f32::from(depth - 1) * self.ring;
        (r0 + 0.5, r0 + self.ring - 0.5)
    }

    fn point(&self, turn: f32, r: f32) -> Pos2 {
        let a = turn * TAU;
        self.center + Vec2::new(a.sin(), -a.cos()) * r
    }

    /// The outline of a segment as points (outer arc, then inner arc back).
    fn outline(&self, s: &Segment) -> Vec<Pos2> {
        let (r0, r1) = self.radii(s.depth);
        let steps = ((s.span() * 360.0 / 3.0).ceil() as usize).max(1);
        let at = |i: usize| s.start + s.span() * i as f32 / steps as f32;
        let mut pts: Vec<Pos2> = (0..=steps).map(|i| self.point(at(i), r1)).collect();
        pts.extend((0..=steps).rev().map(|i| self.point(at(i), r0)));
        pts
    }
}

fn color(s: &Segment, segments: &[Segment], tree: &Tree, dark: bool) -> Color32 {
    let top = segments.get(s.top as usize).unwrap_or(s);
    let hue = (top.start + top.end) * 180.0;
    let d = f32::from(s.depth - 1);
    let is_file = !tree.node(s.id).is_dir();
    let sat = (0.62 - d * 0.06 - if is_file { 0.15 } else { 0.0 }).max(0.18);
    let val = if dark { 0.80 - d * 0.03 } else { 0.90 - d * 0.02 };
    paint::hsv(hue % 360.0, sat, val)
}

fn add_segment(mesh: &mut Mesh, g: &Geometry, s: &Segment, c: Color32) {
    let (r0, r1) = g.radii(s.depth);
    // A small gap between neighbours (about one pixel on the outer edge).
    let gap = (0.8 / (TAU * r1)).min(s.span() * 0.25);
    let (a0, a1) = (s.start + gap / 2.0, s.end - gap / 2.0);
    let steps = (((a1 - a0) * 360.0 / 3.0).ceil() as usize).max(1);
    let base = mesh.vertices.len() as u32;
    for i in 0..=steps {
        let t = a0 + (a1 - a0) * i as f32 / steps as f32;
        mesh.vertices.push(Vertex { pos: g.point(t, r0), uv: WHITE_UV, color: c });
        mesh.vertices.push(Vertex { pos: g.point(t, r1), uv: WHITE_UV, color: c });
    }
    for i in 0..steps as u32 {
        let k = base + 2 * i;
        mesh.indices.extend_from_slice(&[k, k + 1, k + 3, k, k + 3, k + 2]);
    }
}

pub fn show(
    ui: &mut egui::Ui,
    tree: &Tree,
    root: NodeId,
    selected: Option<NodeId>,
    measure: Measure,
    generation: u64,
    cache: &mut Option<Cache>,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let (area, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
    if area.width() < 40.0 || area.height() < 40.0 {
        return actions;
    }
    let key = (generation, root, measure);
    if cache.as_ref().is_none_or(|c| c.key != key) {
        *cache = Some(Cache { key, segments: sunburst::layout(tree, root, RINGS, measure, MIN_SPAN) });
    }
    let Some(cache) = cache.as_ref() else { return actions };
    let segs = &cache.segments;
    let dark = ui.visuals().dark_mode;
    let radius = (area.width().min(area.height()) / 2.0 - 6.0).max(20.0);
    let inner = radius * 0.22;
    let g = Geometry { center: area.center(), inner, ring: (radius - inner) / f32::from(RINGS) };
    let painter = ui.painter_at(area);

    let mut mesh = Mesh::default();
    for s in segs {
        add_segment(&mut mesh, &g, s, color(s, segs, tree, dark));
    }
    painter.add(egui::Shape::mesh(mesh));

    // What is under the mouse: the middle or a segment.
    let pointer = resp.hover_pos();
    let (mut in_middle, mut hover) = (false, None);
    if let Some(p) = pointer {
        let d = p - g.center;
        let dist = d.length();
        if dist < inner {
            in_middle = true;
        } else if dist < radius {
            let depth = ((dist - inner) / g.ring) as u16 + 1;
            let turn = sunburst::turn_of(d.x, d.y);
            hover = segs.iter().find(|s| s.depth == depth && s.contains_angle(turn));
        }
    }

    if let Some(sel) = selected
        && let Some(s) = segs.iter().find(|s| s.id == sel)
    {
        let pts = g.outline(s);
        painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(3.0, Color32::WHITE)));
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, Color32::BLACK)));
    }
    if let Some(s) = hover {
        painter.add(egui::Shape::closed_line(
            g.outline(s),
            Stroke::new(1.5, ui.visuals().selection.stroke.color),
        ));
    }

    // The middle: name and value of the hovered part, or of the folder itself.
    let middle_fill =
        if in_middle { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().faint_bg_color };
    painter.circle_filled(g.center, inner - 2.0, middle_fill);
    let shown = hover.map_or(root, |s| s.id);
    let n = tree.node(shown);
    let name = if shown == crate::tree::ROOT {
        tree.root_path.display().to_string()
    } else {
        n.name_lossy().into_owned()
    };
    let total = tree.value(root, measure);
    let value = tree.value(shown, measure);
    let text_color = ui.visuals().strong_text_color();
    let max_w = inner * 1.7;
    let galley = painter.layout(name, egui::FontId::proportional(13.0), text_color, max_w);
    let galley_h = galley.size().y.min(inner);
    painter.galley(g.center - Vec2::new(galley.size().x / 2.0, galley_h + 2.0), galley, text_color);
    painter.text(
        g.center + Vec2::new(0.0, 2.0),
        egui::Align2::CENTER_TOP,
        format!("{}\n{}", value_text(value, measure), format::percent(value, total)),
        egui::FontId::proportional(12.0),
        ui.visuals().text_color(),
    );

    let parent = tree.node(root).parent;
    if in_middle && parent != NO_PARENT {
        resp.clone().on_hover_text_at_pointer(tr().ring_up_hint);
        if resp.clicked() {
            actions.push(Action::MapRoot(parent));
        }
    }
    if let Some(s) = hover {
        let id = s.id;
        let n = tree.node(id);
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.strong(n.name_lossy());
            ui.label(format!("{}  ({})", format::size(n.size), format::size(n.allocated)));
            ui.label(egui::RichText::new(tree.path(id).display().to_string()).weak());
        });
        if resp.clicked() {
            actions.push(Action::Select(id));
        }
        if resp.double_clicked() && n.is_dir() {
            actions.push(Action::MapRoot(id));
        }
        if resp.secondary_clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("ring_menu"), id));
        }
    }
    let menu_id: Option<NodeId> = ui.ctx().data(|d| d.get_temp(egui::Id::unique("ring_menu")));
    if let Some(id) = menu_id {
        resp.context_menu(|ui| super::folders::context_menu(ui, id, &mut actions));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn outline_and_radii() {
        let g = Geometry { center: pos2(100.0, 100.0), inner: 20.0, ring: 10.0 };
        assert_eq!(g.radii(1), (20.5, 29.5));
        assert_eq!(g.radii(3), (40.5, 49.5));
        let top = g.point(0.0, 10.0);
        assert!((top.x - 100.0).abs() < 1e-4 && (top.y - 90.0).abs() < 1e-4);
        let s = Segment { id: 1, depth: 1, start: 0.0, end: 0.25, top: 0 };
        let pts = g.outline(&s);
        assert!(pts.len() >= 4);
        assert!(pts.iter().all(
            |p| (p.distance(g.center) - 20.5).abs() < 0.01 || (p.distance(g.center) - 29.5).abs() < 0.01
        ));
    }
}
