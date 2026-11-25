//! The program's own icons, drawn as vector shapes (sharp at every size,
//! light and dark mode). One stroke width, at most one topic colour each:
//! blue = scanning and drives, teal = views and charts, amber = selection
//! and files, purple = admin rights, green = export, grey = settings.

use std::f32::consts::{PI, TAU};

use eframe::egui;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Drive,
    FolderOpen,
    Rescan,
    Stop,
    Shield,
    Key,
    OpenExternal,
    Copy,
    Target,
    Export,
    TreeList,
    Types,
    Categories,
    Largest,
    Oldest,
    Age,
    Search,
    TileMap,
    Ring,
    Size,
    Disk,
    Count,
    Up,
    Expand,
    Collapse,
    Sun,
    Moon,
    Globe,
    Info,
    Log,
    Keyboard,
    Question,
    Trash,
    Sparkle,
    Gear,
    Offline,
    Broom,
    History,
    Live,
    Bell,
    Bookmark,
    Duplicates,
    Apps,
    Cloud,
    Move,
    Sheet,
    Report,
    Menu,
}

const BLUE: Color32 = Color32::from_rgb(59, 130, 246);
const TEAL: Color32 = Color32::from_rgb(20, 184, 166);
const AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const PURPLE: Color32 = Color32::from_rgb(139, 92, 246);
const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const RED: Color32 = Color32::from_rgb(239, 68, 68);
const PINK: Color32 = Color32::from_rgb(236, 72, 153);

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    ink: Color32,
    w: f32,
}

impl Pen<'_> {
    /// A point in icon units (0..=24 on both axes).
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + vec2(x, y) * (self.r.width() / 24.0)
    }

    fn u(&self, v: f32) -> f32 {
        v * self.r.width() / 24.0
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_max(self.at(x, y), self.at(x + w, y + h))
    }

    fn stroke(&self) -> Stroke {
        Stroke::new(self.w, self.ink)
    }

    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::line(pts, self.stroke()));
    }

    fn closed(&self, pts: &[(f32, f32)], fill: Color32) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::convex_polygon(pts, fill, self.stroke()));
    }

    fn boxed(&self, x: f32, y: f32, w: f32, h: f32, fill: Color32) {
        self.p.rect(self.rect(x, y, w, h), self.u(1.5), fill, self.stroke(), egui::StrokeKind::Middle);
    }

    fn filled(&self, x: f32, y: f32, w: f32, h: f32, fill: Color32) {
        self.p.rect_filled(self.rect(x, y, w, h), self.u(1.0), fill);
    }

    fn circle(&self, x: f32, y: f32, r: f32, fill: Color32) {
        self.p.circle(self.at(x, y), self.u(r), fill, self.stroke());
    }

    /// Arc around (x, y) from angle a0 to a1 (radians, 0 = right, clockwise on screen).
    fn arc(&self, (x, y): (f32, f32), r: f32, (a0, a1): (f32, f32), color: Color32, width: f32) {
        let n = ((a1 - a0).abs() * 8.0).ceil().max(2.0) as usize;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.at(x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, Stroke::new(width, color)));
    }

    fn arrow_head(&self, tip: (f32, f32), dir: Vec2, color: Color32) {
        let d = dir.normalized();
        let n = vec2(-d.y, d.x);
        let t = self.at(tip.0, tip.1);
        let s = self.u(3.2);
        let pts = vec![t, t - d * s + n * s * 0.8, t - d * s - n * s * 0.8];
        self.p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }

    fn folder(&self, fill: Color32) {
        self.closed(&[(2.5, 6.0), (9.0, 6.0), (11.0, 8.0), (11.0, 9.0), (2.5, 9.0)], fill);
        self.boxed(2.5, 8.5, 19.0, 11.5, fill);
    }

    fn page(&self, fill: Color32) {
        self.closed(&[(5.0, 2.5), (14.5, 2.5), (19.0, 7.0), (19.0, 21.5), (5.0, 21.5)], fill);
        self.line(&[(14.5, 2.5), (14.5, 7.0), (19.0, 7.0)]);
    }
}

/// Draws `icon` into `rect`. `ink` is the outline colour (theme text colour),
/// `paper` the fill of white parts; `enabled` false greys everything out.
pub fn draw(p: &Painter, rect: Rect, icon: Icon, ink: Color32, paper: Color32, enabled: bool) {
    let grey = |c: Color32| if enabled { c } else { c.gamma_multiply(0.35) };
    let ink = grey(ink);
    let paper = grey(paper);
    let (blue, teal, amber, purple, green, red, pink) =
        (grey(BLUE), grey(TEAL), grey(AMBER), grey(PURPLE), grey(GREEN), grey(RED), grey(PINK));
    let pen = Pen { p, r: rect, ink, w: (rect.width() / 13.0).max(1.0) };
    let w = pen.w;
    match icon {
        Icon::Drive | Icon::Disk => {
            pen.boxed(2.5, 7.0, 19.0, 10.0, paper);
            pen.line(&[(2.5, 13.0), (21.5, 13.0)]);
            pen.p.circle_filled(
                pen.at(18.0, 15.2),
                pen.u(1.1),
                if icon == Icon::Drive { blue } else { green },
            );
            pen.filled(5.0, 14.6, 7.0, 1.4, blue);
        }
        Icon::FolderOpen => {
            pen.folder(amber);
            pen.closed(&[(5.5, 12.0), (23.0, 12.0), (20.5, 20.0), (2.5, 20.0)], amber.gamma_multiply(0.85));
        }
        Icon::Rescan => {
            pen.arc((12.0, 12.0), 7.5, (-PI * 0.15, PI * 1.35), blue, w * 1.2);
            pen.arrow_head((19.2, 8.2), vec2(0.35, -1.0), blue);
        }
        Icon::Stop => {
            pen.closed(
                &[
                    (8.0, 2.5),
                    (16.0, 2.5),
                    (21.5, 8.0),
                    (21.5, 16.0),
                    (16.0, 21.5),
                    (8.0, 21.5),
                    (2.5, 16.0),
                    (2.5, 8.0),
                ],
                red,
            );
            pen.filled(8.0, 10.5, 8.0, 3.0, Color32::WHITE);
        }
        Icon::Shield => {
            pen.closed(
                &[(12.0, 2.0), (20.5, 5.5), (20.5, 12.0), (12.0, 22.0), (3.5, 12.0), (3.5, 5.5)],
                purple,
            );
            pen.p.add(Shape::line(
                vec![pen.at(8.0, 12.0), pen.at(11.0, 15.0), pen.at(16.5, 8.5)],
                Stroke::new(w * 1.2, Color32::WHITE),
            ));
        }
        Icon::Key => {
            pen.circle(8.0, 12.0, 4.5, purple);
            pen.p.circle_filled(pen.at(8.0, 12.0), pen.u(1.4), paper);
            pen.line(&[(12.5, 12.0), (21.5, 12.0), (21.5, 15.5)]);
            pen.line(&[(18.0, 12.0), (18.0, 15.0)]);
        }
        Icon::OpenExternal => {
            pen.folder(amber);
            pen.p.add(Shape::line(vec![pen.at(10.0, 17.0), pen.at(16.0, 11.0)], Stroke::new(w * 1.2, ink)));
            pen.arrow_head((17.5, 9.5), vec2(1.0, -1.0), ink);
        }
        Icon::Copy => {
            pen.boxed(8.5, 2.5, 12.0, 14.0, paper);
            pen.boxed(3.5, 7.5, 12.0, 14.0, paper);
            for y in [11.5, 14.5, 17.5] {
                pen.line(&[(6.0, y), (13.0, y)]);
            }
        }
        Icon::Target => {
            pen.circle(12.0, 12.0, 8.5, Color32::TRANSPARENT);
            pen.circle(12.0, 12.0, 4.5, amber.gamma_multiply(0.5));
            pen.p.circle_filled(pen.at(12.0, 12.0), pen.u(1.6), amber);
            pen.line(&[(12.0, 0.8), (12.0, 4.5)]);
            pen.line(&[(12.0, 19.5), (12.0, 23.2)]);
            pen.line(&[(0.8, 12.0), (4.5, 12.0)]);
            pen.line(&[(19.5, 12.0), (23.2, 12.0)]);
        }
        Icon::Export => {
            pen.page(paper);
            pen.filled(7.5, 10.0, 9.0, 1.6, green);
            pen.filled(7.5, 13.2, 9.0, 1.6, green);
            pen.p.add(Shape::line(vec![pen.at(12.0, 22.5), pen.at(21.0, 22.5)], Stroke::new(w * 1.2, green)));
            pen.arrow_head((23.0, 22.5), vec2(1.0, 0.0), green);
        }
        Icon::TreeList => {
            for (i, y) in [4.0f32, 10.0, 16.0].iter().enumerate() {
                let x = 3.0 + i as f32 * 3.5;
                pen.filled(x, *y, 3.5, 3.5, if i == 0 { amber } else { teal });
                pen.filled(x + 5.0, y + 1.0, 14.0 - x, 1.6, ink);
            }
            pen.line(&[(4.7, 7.5), (4.7, 18.0), (6.5, 18.0)]);
        }
        Icon::Types => {
            pen.closed(&[(3.0, 3.0), (12.0, 3.0), (21.0, 12.0), (12.0, 21.0), (3.0, 12.0)], teal);
            pen.p.circle_filled(pen.at(7.5, 7.5), pen.u(1.6), paper);
        }
        Icon::Categories => {
            let colors = [blue, pink, amber, teal];
            for (i, c) in colors.iter().enumerate() {
                let (x, y) = (3.0 + (i % 2) as f32 * 9.5, 3.0 + (i / 2) as f32 * 9.5);
                pen.p.rect(pen.rect(x, y, 8.5, 8.5), pen.u(2.0), *c, pen.stroke(), egui::StrokeKind::Middle);
            }
        }
        Icon::Largest => {
            for (i, len) in [17.0f32, 12.0, 7.0].iter().enumerate() {
                pen.boxed(3.0, 4.0 + i as f32 * 6.0, *len, 4.0, if i == 0 { amber } else { paper });
            }
        }
        Icon::Oldest => {
            pen.circle(12.0, 12.0, 9.0, paper);
            pen.p.add(Shape::line(
                vec![pen.at(12.0, 6.5), pen.at(12.0, 12.0), pen.at(16.0, 14.5)],
                Stroke::new(w * 1.1, amber),
            ));
        }
        Icon::Age => {
            let hues = [green, teal, amber, red];
            for (i, h) in [6.0f32, 10.0, 14.0, 8.0].iter().enumerate() {
                let x = 3.0 + i as f32 * 4.8;
                pen.boxed(x, 21.0 - h, 3.6, *h, hues[i]);
            }
            pen.line(&[(2.0, 21.5), (22.0, 21.5)]);
        }
        Icon::Search => {
            pen.circle(10.0, 10.0, 6.5, teal.gamma_multiply(0.35));
            pen.p.add(Shape::line(vec![pen.at(15.0, 15.0), pen.at(21.0, 21.0)], Stroke::new(w * 1.8, ink)));
        }
        Icon::TileMap => {
            pen.boxed(2.5, 2.5, 10.0, 19.0, blue);
            pen.boxed(12.5, 2.5, 9.0, 11.0, teal);
            pen.boxed(12.5, 13.5, 5.0, 8.0, amber);
            pen.boxed(17.5, 13.5, 4.0, 8.0, pink);
        }
        Icon::Ring => {
            let seg = |a0: f32, a1: f32, c: Color32| pen.arc((12.0, 12.0), 6.8, (a0, a1), c, pen.u(5.0));
            seg(-PI / 2.0, PI * 0.45, teal);
            seg(PI * 0.5, PI * 1.05, blue);
            seg(PI * 1.1, PI * 1.45, amber);
            pen.circle(12.0, 12.0, 9.6, Color32::TRANSPARENT);
        }
        Icon::Size => {
            pen.boxed(2.5, 8.0, 19.0, 8.0, amber.gamma_multiply(0.6));
            for (i, x) in [6.0f32, 9.5, 13.0, 16.5].iter().enumerate() {
                pen.line(&[(*x, 8.0), (*x, if i % 2 == 0 { 12.5 } else { 11.0 })]);
            }
        }
        Icon::Count => {
            pen.p.add(Shape::line(vec![pen.at(9.0, 3.0), pen.at(7.0, 21.0)], Stroke::new(w * 1.2, teal)));
            pen.p.add(Shape::line(vec![pen.at(17.0, 3.0), pen.at(15.0, 21.0)], Stroke::new(w * 1.2, teal)));
            pen.line(&[(3.5, 8.5), (21.0, 8.5)]);
            pen.line(&[(3.0, 15.5), (20.5, 15.5)]);
        }
        Icon::Up => {
            pen.folder(amber.gamma_multiply(0.6));
            pen.p.add(Shape::line(vec![pen.at(12.0, 19.0), pen.at(12.0, 11.0)], Stroke::new(w * 1.4, ink)));
            pen.arrow_head((12.0, 9.0), vec2(0.0, -1.0), ink);
        }
        Icon::Expand | Icon::Collapse => {
            for (i, y) in [4.0f32, 10.5, 17.0].iter().enumerate() {
                pen.filled(10.0, y + 0.8, 11.0, 1.6, ink);
                pen.boxed(3.0, *y, 4.0, 3.4, if i == 0 || icon == Icon::Expand { teal } else { paper });
            }
        }
        Icon::Sun => {
            pen.circle(12.0, 12.0, 4.5, amber);
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let (c, s) = (a.cos(), a.sin());
                pen.line(&[(12.0 + 7.0 * c, 12.0 + 7.0 * s), (12.0 + 10.0 * c, 12.0 + 10.0 * s)]);
            }
        }
        Icon::Moon => {
            pen.circle(12.0, 12.0, 8.5, blue);
            pen.p.circle_filled(pen.at(16.5, 8.5), pen.u(7.0), paper);
        }
        Icon::Globe => {
            pen.circle(12.0, 12.0, 9.0, teal.gamma_multiply(0.35));
            pen.p.add(Shape::ellipse_stroke(pen.at(12.0, 12.0), vec2(pen.u(4.0), pen.u(9.0)), pen.stroke()));
            pen.line(&[(3.0, 12.0), (21.0, 12.0)]);
        }
        Icon::Info => {
            pen.circle(12.0, 12.0, 9.5, blue);
            pen.filled(10.8, 10.5, 2.4, 7.0, Color32::WHITE);
            pen.p.circle_filled(pen.at(12.0, 7.2), pen.u(1.4), Color32::WHITE);
        }
        Icon::Log => {
            pen.page(paper);
            for y in [10.0, 13.5, 17.0] {
                pen.line(&[(8.0, y), (16.0, y)]);
            }
            pen.p.circle_filled(pen.at(18.0, 19.0), pen.u(3.0), amber);
        }
        Icon::Question => {
            pen.circle(12.0, 12.0, 9.5, amber);
            pen.p.add(Shape::line(
                vec![
                    pen.at(9.0, 9.5),
                    pen.at(9.6, 7.4),
                    pen.at(12.0, 6.5),
                    pen.at(14.4, 7.4),
                    pen.at(15.0, 9.5),
                    pen.at(12.0, 12.0),
                    pen.at(12.0, 14.0),
                ],
                Stroke::new(w * 1.2, Color32::WHITE),
            ));
            pen.p.circle_filled(pen.at(12.0, 17.3), pen.u(1.4), Color32::WHITE);
        }
        Icon::Trash => {
            pen.closed(&[(5.5, 7.0), (18.5, 7.0), (17.0, 21.5), (7.0, 21.5)], paper);
            pen.line(&[(3.0, 7.0), (21.0, 7.0)]);
            pen.line(&[(9.5, 7.0), (10.0, 3.5), (14.0, 3.5), (14.5, 7.0)]);
            for x in [9.8, 12.0, 14.2] {
                pen.p.add(Shape::line(vec![pen.at(x, 10.0), pen.at(x, 18.5)], Stroke::new(w * 0.9, green)));
            }
        }
        Icon::Sparkle => {
            let star = |cx: f32, cy: f32, r: f32, c: Color32| {
                let pts: Vec<Pos2> = (0..8)
                    .map(|k| {
                        let a = k as f32 * PI / 4.0 - PI / 2.0;
                        let rr = if k % 2 == 0 { r } else { r * 0.32 };
                        pen.at(cx + rr * a.cos(), cy + rr * a.sin())
                    })
                    .collect();
                pen.p.add(Shape::convex_polygon(pts.clone(), c, Stroke::NONE));
                pen.p.add(Shape::closed_line(pts, pen.stroke()));
            };
            star(10.0, 13.0, 8.5, purple);
            star(18.5, 5.5, 4.0, teal);
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                pen.p.add(Shape::line(
                    vec![
                        pen.at(12.0 + 6.5 * a.cos(), 12.0 + 6.5 * a.sin()),
                        pen.at(12.0 + 10.0 * a.cos(), 12.0 + 10.0 * a.sin()),
                    ],
                    Stroke::new(w * 2.0, ink),
                ));
            }
            pen.circle(12.0, 12.0, 7.0, paper);
            pen.circle(12.0, 12.0, 2.8, teal);
        }
        Icon::Offline => {
            pen.closed(
                &[
                    (6.0, 17.5),
                    (3.0, 14.5),
                    (5.0, 10.5),
                    (9.0, 9.5),
                    (12.0, 5.5),
                    (17.0, 6.0),
                    (19.5, 10.0),
                    (21.5, 13.5),
                    (19.0, 17.5),
                ],
                blue.gamma_multiply(0.5),
            );
            pen.p.add(Shape::line(vec![pen.at(3.5, 21.0), pen.at(20.5, 3.0)], Stroke::new(w * 1.5, red)));
        }
        Icon::Broom => {
            pen.p.add(Shape::line(vec![pen.at(19.5, 2.5), pen.at(11.5, 12.5)], Stroke::new(w * 1.5, amber)));
            pen.closed(&[(9.0, 11.0), (14.0, 15.0), (9.5, 21.5), (3.0, 16.5)], amber.gamma_multiply(0.75));
            for (x0, y0) in [(5.0, 18.0), (7.0, 19.5)] {
                pen.line(&[(x0, y0), (x0 + 3.0, y0 - 4.0)]);
            }
        }
        Icon::Duplicates => {
            pen.boxed(3.0, 3.0, 11.0, 14.0, paper);
            pen.boxed(10.0, 7.0, 11.0, 14.0, amber.gamma_multiply(0.5));
            pen.line(&[(12.5, 11.5), (18.5, 11.5)]);
            pen.line(&[(12.5, 15.0), (18.5, 15.0)]);
        }
        Icon::Apps => {
            let hues = [blue, teal, amber, purple];
            for (i, h) in hues.iter().enumerate() {
                let (x, y) = (3.0 + (i % 2) as f32 * 10.0, 3.0 + (i / 2) as f32 * 10.0);
                pen.boxed(x, y, 8.0, 8.0, *h);
            }
        }
        Icon::Cloud => {
            pen.circle(8.5, 14.0, 4.5, blue.gamma_multiply(0.6));
            pen.circle(14.0, 10.5, 6.0, blue.gamma_multiply(0.6));
            pen.circle(18.0, 15.0, 3.8, blue.gamma_multiply(0.6));
            pen.filled(8.5, 14.0, 9.5, 4.8, blue.gamma_multiply(0.6));
            pen.line(&[(8.5, 18.8), (18.0, 18.8)]);
        }
        Icon::Move => {
            pen.boxed(2.0, 8.0, 7.0, 9.0, paper);
            pen.boxed(15.0, 8.0, 7.0, 9.0, green.gamma_multiply(0.5));
            pen.p.add(Shape::line(vec![pen.at(9.5, 12.5), pen.at(13.0, 12.5)], Stroke::new(w * 1.3, green)));
            pen.arrow_head((15.0, 12.5), vec2(1.0, 0.0), green);
        }
        Icon::Sheet => {
            pen.boxed(3.0, 3.0, 18.0, 18.0, paper);
            pen.filled(3.5, 3.5, 17.0, 4.0, green);
            for x in [9.0, 15.0] {
                pen.line(&[(x, 7.5), (x, 21.0)]);
            }
            for y in [12.0, 16.5] {
                pen.line(&[(3.0, y), (21.0, y)]);
            }
        }
        Icon::Report => {
            pen.closed(&[(5.0, 2.5), (15.0, 2.5), (19.5, 7.0), (19.5, 21.5), (5.0, 21.5)], paper);
            for (i, len) in [9.0f32, 6.0, 11.0].iter().enumerate() {
                pen.filled(7.5, 9.0 + i as f32 * 3.8, *len, 2.2, teal);
            }
        }
        Icon::Menu => {
            pen.boxed(5.0, 3.0, 16.0, 18.0, paper);
            for (i, y) in [7.0f32, 11.5, 16.0].iter().enumerate() {
                let c = if i == 1 { blue } else { ink };
                pen.filled(7.5, *y, 11.0, 2.0, c);
            }
            pen.closed(&[(2.0, 9.0), (2.0, 18.0), (7.5, 14.5)], ink);
        }
        Icon::History => {
            pen.circle(12.0, 12.0, 9.0, teal.gamma_multiply(0.3));
            pen.line(&[(12.0, 6.5), (12.0, 12.0), (16.0, 14.5)]);
            pen.arrow_head((4.2, 8.0), vec2(-0.4, 1.0), teal);
        }
        Icon::Live => {
            let pts =
                [(2.0, 13.0), (7.0, 13.0), (9.0, 6.0), (12.5, 19.0), (15.0, 9.5), (17.0, 13.0), (22.0, 13.0)];
            let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| pen.at(x, y)).collect();
            pen.p.add(Shape::line(pts, Stroke::new(w * 1.4, green)));
        }
        Icon::Bell => {
            pen.closed(
                &[(12.0, 4.0), (16.0, 5.5), (17.5, 9.0), (18.5, 17.5), (5.5, 17.5), (6.5, 9.0), (8.0, 5.5)],
                amber,
            );
            pen.line(&[(3.5, 17.5), (20.5, 17.5)]);
            pen.circle(12.0, 20.3, 1.7, ink);
        }
        Icon::Bookmark => {
            pen.closed(&[(7.0, 3.0), (17.0, 3.0), (17.0, 21.0), (12.0, 16.5), (7.0, 21.0)], blue);
        }
        Icon::Keyboard => {
            pen.boxed(2.0, 6.5, 20.0, 11.0, paper);
            for row in 0..2 {
                for k in 0..5 {
                    pen.filled(4.2 + k as f32 * 3.4, 8.7 + row as f32 * 3.0, 2.2, 1.8, ink);
                }
            }
            pen.filled(7.0, 14.8, 10.0, 1.6, teal);
        }
    }
}
