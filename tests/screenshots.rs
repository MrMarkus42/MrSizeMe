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
    shot_with(sample_tree, dark, lang, name, clicks);
}

/// A made-up home folder with typical clean-up candidates.
fn cleanup_tree() -> Tree {
    let mut t = Tree::new(PathBuf::from("/home/beispiel"), Kind::Dir);
    let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
    let file = |n: &str, mb: u64| {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = mb << 20;
        f.allocated = f.size;
        f.modified = 1_760_000_000;
        f
    };
    let top = t.add_children(ROOT, [dir("Projekte"), dir(".cache"), dir("Downloads"), dir("Videos")]);
    let proj = t.add_children(top, [dir("web"), dir("rust")]);
    let web = t.add_children(proj, [dir("node_modules"), file("package.json", 0)]);
    t.add_children(web, [file("bundle.js", 900), file("vendor.js", 400)]);
    let rust = t.add_children(proj + 1, [dir("target"), file("Cargo.toml", 0)]);
    t.add_children(rust, [file("app", 2300)]);
    t.add_children(top + 1, [file("browser-cache.bin", 1700), file("thumbnails.db", 300)]);
    t.add_children(top + 2, [file("setup-alt.exe", 800), file("urlaub.zip", 3100)]);
    t.add_children(top + 3, [file("film.mkv", 6400)]);
    t.sum_up();
    t
}

fn shot_with(tree: fn() -> Tree, dark: bool, lang: Lang, name: &str, clicks: &[&str]) {
    shot_sized(tree, (1400.0, 820.0), dark, lang, name, clicks);
}

fn shot_sized(tree: fn() -> Tree, size: (f32, f32), dark: bool, lang: Lang, name: &str, clicks: &[&str]) {
    common::setup();
    i18n::set_lang(lang);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    let mut h = Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_max_steps(64)
        .with_theme(theme)
        .wgpu()
        .build_eframe(move |cc| {
            cc.egui_ctx.set_theme(theme);
            App::with_tree(tree())
        });
    h.run();
    for label in clicks {
        // "Text@last" clicks the last element with that text (e.g. a list tab
        // that has the same name as a menu band tab).
        let (text, last) = match label.strip_suffix("@last") {
            Some(t) => (t, true),
            None => (*label, false),
        };
        let node = if last { h.get_all_by_label(text).last() } else { h.get_all_by_label(text).next() };
        node.unwrap_or_else(|| panic!("no {label}")).click();
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
    shot(false, Lang::De, "menueband-ansicht-hell", &["Ansicht"]);
    shot(true, Lang::De, "menueband-hilfe-dunkel", &["Hilfe", "Tastenkürzel"]);
    shot(false, Lang::De, "hauptfenster-hell", &[]);
    shot_with(cleanup_tree, false, Lang::De, "aufraeumen-hell", &["Aufräumen", "Aufräumen@last"]);
    shot_with(
        cleanup_tree,
        true,
        Lang::De,
        "was-ist-das-dunkel",
        &["Aufräumen", "Aufräumen@last", "JavaScript-Pakete (node_modules)", "Was ist das?"],
    );
    shot(false, Lang::En, "main-window-light-en", &["Ring chart", "Categories"]);
    shot(false, Lang::De, "einstellungen-hell", &["Einstellungen"]);
    shot(true, Lang::De, "einstellungen-ki-dunkel", &["Einstellungen", "KI"]);
    // A narrow window: the menu band shows only icons.
    shot_sized(cleanup_tree, (820.0, 640.0), false, Lang::De, "schmal-hell", &[]);
}

/// The home folder at step `s` of a month: downloads and caches grow.
fn grown(s: u64) -> Tree {
    let mut t = Tree::new(PathBuf::from("/home/beispiel"), Kind::Dir);
    let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
    let file = |n: &str, mb: u64| {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = mb << 20;
        f.allocated = f.size;
        f
    };
    let top = t.add_children(ROOT, [dir("Downloads"), dir(".cache"), dir("Videos")]);
    t.add_children(top, [file("urlaub.zip", 600 * s + 100), file("setup-alt.exe", 800)]);
    t.add_children(top + 1, [file("browser-cache.bin", 300 + 280 * s)]);
    t.add_children(top + 2, [file("film.mkv", 6400)]);
    t.sum_up();
    t
}

#[test]
#[ignore]
fn history_screenshots() {
    use mrsizeme::disk::Space;
    use mrsizeme::history::Store;
    for (dark, name) in [(false, "verlauf-hell"), (true, "verlauf-dunkel")] {
        common::setup();
        i18n::set_lang(Lang::De);
        let dir = common::TempDir::new();
        let store = Store::at(dir.path().join("history"));
        let now = mrsizeme::query::now();
        let gib = 1u64 << 30;
        for s in 0..5u64 {
            let space = Some(Space { total: 500 * gib, free: 60 * gib - s * 4 * gib });
            store.save(&grown(s), now - (5 - s as i64) * 6 * 86_400, space).unwrap();
        }
        store.save(&grown(5), now, Some(Space { total: 500 * gib, free: 40 * gib })).unwrap();
        let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
        let path = dir.path().join("history");
        let mut h = Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .with_max_steps(64)
            .with_theme(theme)
            .wgpu()
            .build_eframe(move |cc| {
                cc.egui_ctx.set_theme(theme);
                App::with_history(grown(5), path.clone(), Some(now))
            });
        h.run();
        h.get_all_by_label("Verlauf").next().unwrap().click();
        h.run();
        h.get_by_label("Verlauf & Vergleich").click();
        h.run();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-screenshots");
        std::fs::create_dir_all(&out).unwrap();
        h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
    }
}

/// Cloud folder and programs with made-up data.
fn cloud_app() -> App {
    use mrsizeme::apps::App as Program;
    use mrsizeme::cloud::Root;
    use mrsizeme::tree::flags;
    let mut t = Tree::new(PathBuf::from("/home/beispiel"), Kind::Dir);
    let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
    let file = |n: &str, mb: u64, online: bool| {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = mb << 20;
        f.allocated = if online { 0 } else { f.size };
        if online {
            f.flags |= flags::ONLINE_ONLY;
        }
        f
    };
    let top = t.add_children(ROOT, [dir("OneDrive"), dir("Spiele"), dir(".config")]);
    let od = t.add_children(top, [dir("Fotos"), dir("Dokumente"), dir("Videos")]);
    t.add_children(od, [file("urlaub-2025.zip", 2400, false), file("hochzeit.zip", 5100, true)]);
    t.add_children(od + 1, [file("steuer.pdf", 12, false), file("archiv.zip", 800, true)]);
    t.add_children(od + 2, [file("geburtstag.mp4", 3900, false)]);
    let games = t.add_children(top + 1, [dir("Weltraum"), dir("Rennen")]);
    t.add_children(games, [file("daten.pak", 18_000, false)]);
    t.add_children(games + 1, [file("strecken.pak", 7_500, false)]);
    let cfg = t.add_children(top + 2, [dir("Weltraum")]);
    t.add_children(cfg, [file("shader-cache.bin", 900, false)]);
    t.sum_up();
    let roots = vec![Root { name: "OneDrive".into(), path: PathBuf::from("/home/beispiel/OneDrive") }];
    let program = |name: &str, dir: &str, data: &[&str]| Program {
        name: name.into(),
        location: Some(PathBuf::from(format!("/home/beispiel/Spiele/{dir}"))),
        data: data.iter().map(|d| PathBuf::from(format!("/home/beispiel/{d}"))).collect(),
        reported: None,
    };
    let programs = vec![
        program("Weltraum-Abenteuer", "Weltraum", &[".config/Weltraum"]),
        program("Rennspiel", "Rennen", &[]),
    ];
    App::with_tree(t).with_cloud_roots(roots).with_apps(programs)
}

#[test]
#[ignore]
fn stage7_screenshots() {
    for (dark, name, click) in [(false, "cloud-hell", "Cloud"), (true, "programme-dunkel", "Programme")] {
        common::setup();
        i18n::set_lang(Lang::De);
        let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
        let mut h = Harness::builder()
            .with_size(egui::vec2(1400.0, 820.0))
            .with_max_steps(64)
            .with_theme(theme)
            .wgpu()
            .build_eframe(move |cc| {
                cc.egui_ctx.set_theme(theme);
                cloud_app()
            });
        h.run();
        h.get_by_label(click).click();
        h.run();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-screenshots");
        std::fs::create_dir_all(&out).unwrap();
        h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
    }
}
