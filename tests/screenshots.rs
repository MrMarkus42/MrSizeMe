//! Pictures of the window for reviewing the look (not run by default):
//! `cargo test --test screenshots -- --ignored`; files in target/ui-screenshots.

mod common;

use std::ffi::OsStr;
use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mrsizeme::i18n::{self, Lang};
use mrsizeme::tree::{Kind, NO_PARENT, Node, NodeId, ROOT, Tree};
use mrsizeme::ui::App;

/// A made-up disk with typical folders (no real files are read).
fn sample_tree() -> Tree {
    let mut seed: u64 = 42;
    let mut rnd = move |max: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % max
    };
    let mut tree = Tree::new(PathBuf::from("/Datenträger"), Kind::Dir);
    let dir = |name: &str| Node::new(OsStr::new(name).into(), NO_PARENT, Kind::Dir);
    let top = ["Benutzer", "Programme", "System", "Spiele", "Projekte", "Temp"];
    let first = tree.add_children(ROOT, top.iter().map(|n| dir(n)));
    let exts =
        ["mp4", "jpg", "png", "pdf", "docx", "zip", "dll", "exe", "mkv", "mp3", "iso", "log", "rs", "json"];
    let mut pending: Vec<(NodeId, u32)> = (0..top.len() as NodeId).map(|i| (first + i, 0)).collect();
    while let Some((id, depth)) = pending.pop() {
        let mut kids = Vec::new();
        let subdirs = if depth < 2 { 2 + rnd(4) } else { 0 };
        for i in 0..subdirs {
            kids.push(dir(&format!("Ordner {}", i + 1)));
        }
        for i in 0..(3 + rnd(9)) {
            let ext = exts[rnd(exts.len() as u64) as usize];
            let mut f = Node::new(OsStr::new(&format!("datei_{i}.{ext}")).into(), NO_PARENT, Kind::File);
            let big = if rnd(6) == 0 { 4_000_000_000 } else { 300_000_000 };
            f.size = rnd(big) + 1;
            f.allocated = f.size.div_ceil(4096) * 4096;
            f.modified = 1_700_000_000 + rnd(90_000_000) as i64;
            kids.push(f);
        }
        let start = tree.add_children(id, kids);
        for i in 0..subdirs as NodeId {
            pending.push((start + i, depth + 1));
        }
    }
    tree.sum_up();
    tree
}

fn shot(dark: bool, lang: Lang, name: &str, clicks: &[&str]) {
    common::setup();
    i18n::set_lang(lang);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 820.0))
        .with_max_steps(64)
        .with_theme(theme)
        .wgpu()
        .build_eframe(move |cc| {
            cc.egui_ctx.set_theme(theme);
            App::with_tree(sample_tree())
        });
    h.run();
    for label in clicks {
        h.get_all_by_label(label).next().unwrap_or_else(|| panic!("no {label}")).click();
        h.run();
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    match h.render() {
        Ok(img) => img.save(dir.join(format!("{name}.png"))).unwrap(),
        Err(e) => panic!("rendering failed: {e}"),
    }
}

#[test]
#[ignore]
fn screenshots() {
    shot(true, Lang::De, "hauptfenster-dunkel", &[]);
    shot(true, Lang::De, "ring-kategorien-dunkel", &["Ringdiagramm", "Kategorien"]);
    shot(false, Lang::De, "ring-alter-hell", &["Ringdiagramm", "Alter"]);
    shot(false, Lang::De, "suche-hell", &["Suche"]);
    shot(false, Lang::De, "hauptfenster-hell", &[]);
    shot(false, Lang::En, "main-window-light-en", &["Ring chart", "Categories"]);
}
