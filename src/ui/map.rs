//! The tile map: every file is a tile whose area matches its size;
//! folders frame the tiles of their contents.

use eframe::egui;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect, Stroke, pos2};

use super::Action;
use super::paint;
use crate::format;
use crate::stats::extension;
use crate::tree::{NodeId, Tree};
use crate::treemap::{self, Rect as TRect};

/// At most this many tiles are drawn (keeps big disks fluid).
const MAX_TILES: usize = 60_000;
/// Height of the name strip of a large folder.
const HEADER: f32 = 15.0;

pub struct Tile {
    pub id: NodeId,
    pub rect: Rect,
    pub depth: u16,
    pub is_dir: bool,
    pub header: bool,
}

pub struct Cache {
    key: (u64, NodeId, bool, [i32; 4]),
    tiles: Vec<Tile>,
}

fn to_egui(r: TRect) -> Rect {
    Rect::from_min_size(pos2(r.x, r.y), egui::vec2(r.w, r.h))
}

/// Lays out the tiles of `root` in `area`.
pub fn build(tree: &Tree, root: NodeId, area: Rect, allocated: bool) -> Vec<Tile> {
    let value = |id: NodeId| {
        let n = tree.node(id);
        if allocated { n.allocated } else { n.size }
    };
    let mut tiles = Vec::new();
    let mut stack = vec![(root, TRect::new(area.min.x, area.min.y, area.width(), area.height()), 0u16)];
    while let Some((id, r, depth)) = stack.pop() {
        if tiles.len() >= MAX_TILES {
            break;
        }
        let n = tree.node(id);
        let is_dir = n.is_dir();
        let header = is_dir && depth > 0 && r.h > 3.0 * HEADER && r.w > 70.0;
        tiles.push(Tile { id, rect: to_egui(r), depth, is_dir, header });
        if !is_dir || n.child_count == 0 {
            continue;
        }
        let mut inner = if depth == 0 { r } else { r.shrink(1.5) };
        if header {
            inner = inner.below(HEADER);
        }
        if inner.w < 3.0 || inner.h < 3.0 {
            continue;
        }
        let mut kids: Vec<NodeId> = tree.children(id).iter().copied().filter(|&c| value(c) > 0).collect();
        kids.sort_by_key(|&c| std::cmp::Reverse(value(c)));
        let sizes: Vec<u64> = kids.iter().map(|&c| value(c)).collect();
        let rects = treemap::squarify(&sizes, inner);
        // Pushed in reverse so the largest is laid out (and drawn) first.
        for (&c, &cr) in kids.iter().zip(&rects).rev() {
            if cr.w >= 1.0 && cr.h >= 1.0 {
                stack.push((c, cr, depth.saturating_add(1)));
            }
        }
    }
    tiles
}

fn shade(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| (f32::from(v) * f).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Adds a tile with a soft light from the top left (cushion look).
fn cushion(mesh: &mut Mesh, r: Rect, c: Color32) {
    let i = mesh.vertices.len() as u32;
    let v = |pos: Pos2, color| Vertex { pos, uv: WHITE_UV, color };
    mesh.vertices.push(v(r.left_top(), shade(c, 1.22)));
    mesh.vertices.push(v(r.right_top(), shade(c, 1.0)));
    mesh.vertices.push(v(r.right_bottom(), shade(c, 0.72)));
    mesh.vertices.push(v(r.left_bottom(), shade(c, 1.0)));
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
}

pub fn show(
    ui: &mut egui::Ui,
    tree: &Tree,
    root: NodeId,
    selected: Option<NodeId>,
    allocated: bool,
    generation: u64,
    cache: &mut Option<Cache>,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let (area, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
    if area.width() < 10.0 || area.height() < 10.0 {
        return actions;
    }
    let key = (
        generation,
        root,
        allocated,
        [area.min.x as i32, area.min.y as i32, area.width() as i32, area.height() as i32],
    );
    if cache.as_ref().is_none_or(|c| c.key != key) {
        *cache = Some(Cache { key, tiles: build(tree, root, area, allocated) });
    }
    let Some(cache) = cache.as_ref() else { return actions };
    let tiles = &cache.tiles;
    let dark = ui.visuals().dark_mode;
    let painter = ui.painter_at(area);

    painter.rect_filled(area, 4.0, ui.visuals().extreme_bg_color);
    let mut mesh = Mesh::default();
    let frame = if dark { Color32::from_gray(28) } else { Color32::from_gray(90) };
    for tile in tiles {
        if tile.is_dir {
            if tile.depth > 0 {
                let fill = if dark { Color32::from_gray(48) } else { Color32::from_gray(215) };
                painter.rect(tile.rect, 0.0, fill, Stroke::new(1.0, frame), egui::StrokeKind::Inside);
            }
        } else {
            let ext = extension(&tree.node(tile.id).name_lossy());
            cushion(&mut mesh, tile.rect.shrink(0.5), paint::type_color(&ext, dark));
        }
    }
    painter.add(egui::Shape::mesh(mesh));

    // Names: in the strip of large folders and on large files.
    let text_color = ui.visuals().strong_text_color();
    for tile in tiles {
        let n = tree.node(tile.id);
        if tile.header {
            let pos = tile.rect.left_top() + egui::vec2(4.0, 1.5);
            let text =
                format!("{}  {}", n.name_lossy(), format::size(if allocated { n.allocated } else { n.size }));
            let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), text_color);
            let clip = Rect::from_min_size(tile.rect.min, egui::vec2(tile.rect.width() - 4.0, HEADER));
            painter.with_clip_rect(clip.intersect(area)).galley(pos, galley, text_color);
        } else if !tile.is_dir && tile.rect.width() > 60.0 && tile.rect.height() > 18.0 {
            let galley = painter.layout_no_wrap(
                n.name_lossy().into_owned(),
                egui::FontId::proportional(11.0),
                Color32::BLACK,
            );
            if galley.size().x < tile.rect.width() - 6.0 {
                painter.galley(tile.rect.center() - galley.size() / 2.0, galley, Color32::BLACK);
            }
        }
    }

    // The tile under the mouse: the deepest one, i.e. the last drawn.
    let hover = resp.hover_pos().and_then(|p| tiles.iter().rev().find(|t| t.rect.contains(p)));
    if let Some(sel) = selected
        && let Some(t) = tiles.iter().find(|t| t.id == sel)
    {
        painter.rect_stroke(t.rect, 0.0, Stroke::new(2.5, Color32::WHITE), egui::StrokeKind::Inside);
        painter.rect_stroke(
            t.rect.shrink(2.5),
            0.0,
            Stroke::new(1.0, Color32::BLACK),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(t) = hover {
        painter.rect_stroke(
            t.rect,
            0.0,
            Stroke::new(1.5, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Inside,
        );
        let n = tree.node(t.id);
        let id = t.id;
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
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("map_menu"), id));
        }
    }
    let menu_id: Option<NodeId> = ui.ctx().data(|d| d.get_temp(egui::Id::unique("map_menu")));
    if let Some(id) = menu_id {
        resp.context_menu(|ui| super::folders::context_menu(ui, id, &mut actions));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn tiles_cover_files_inside_the_area() {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let file = |name: &str, size| {
            let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
            n.size = size;
            n
        };
        let first =
            t.add_children(ROOT, [Node::new(OsStr::new("d").into(), NO_PARENT, Kind::Dir), file("a", 300)]);
        t.add_children(first, [file("b", 500), file("c", 200), file("empty", 0)]);
        t.sum_up();
        let area = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let tiles = build(&t, ROOT, area, false);
        // root, d, b, c, a – the empty file gets no tile.
        assert_eq!(tiles.len(), 5);
        assert_eq!(tiles[0].id, ROOT);
        for tile in &tiles {
            assert!(area.expand(0.1).contains_rect(tile.rect), "{:?}", tile.rect);
        }
        let a = tiles.iter().find(|x| t.node(x.id).name_lossy() == "a").unwrap();
        assert!((a.rect.area() - 0.3 * area.area()).abs() < 50.0);
    }
}
