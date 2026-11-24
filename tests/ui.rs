//! Runs the window without a screen and clicks through it.

mod common;

use common::{TempDir, setup};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mrsizeme::i18n::{self, Lang};
use mrsizeme::scan::{Options, Progress, scan};
use mrsizeme::ui::App;

/// The language is global, so the window tests run one after another.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn harness(t: &TempDir) -> Harness<'static, App> {
    setup();
    i18n::set_lang(Lang::De);
    let tree = scan(t.path(), &Options::default(), &Progress::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1200.0, 760.0))
        .with_max_steps(32)
        .build_eframe(move |_cc| App::with_tree(tree));
    h.run();
    h
}

fn sample() -> TempDir {
    let t = TempDir::new();
    t.file("Videos/film.mkv", 90_000);
    t.file("Videos/clip.mp4", 30_000);
    t.file("Fotos/bild.jpg", 20_000);
    t.file("notiz.txt", 500);
    t
}

#[test]
fn shows_folders_and_switches_views() {
    let _guard = lock();
    let t = sample();
    let mut h = harness(&t);
    // Folder list with the largest folder first.
    assert!(h.query_by_label("Videos").is_some());
    assert!(h.query_by_label("Fotos").is_some());

    h.get_by_label("Dateitypen").click();
    h.run();
    assert!(h.query_by_label(".mkv").is_some());
    assert!(h.query_by_label(".jpg").is_some());

    h.get_by_label("Größte Dateien").click();
    h.run();
    assert!(h.query_by_label("film.mkv").is_some());

    // "Ordner" is both the tab and a column heading; the tab comes first.
    h.get_all_by_label("Ordner").next().unwrap().click();
    h.run();
    assert!(h.query_by_label("Videos").is_some());
}

#[test]
fn sort_by_name_and_language_switch() {
    let _guard = lock();
    let t = sample();
    let mut h = harness(&t);
    h.get_by_label("Name").click();
    h.run();
    assert!(h.query_by_label("Name ⏷").is_some());

    i18n::set_lang(Lang::En);
    h.run();
    assert!(h.query_by_label("File types").is_some());
    i18n::set_lang(Lang::De);
}

#[test]
fn categories_age_search_and_ring_chart() {
    let _guard = lock();
    let t = sample();
    let mut h = harness(&t);

    h.get_by_label("Kategorien").click();
    h.run();
    // Below a temp folder ("tmp", "Temp") everything counts as temporary.
    let temp_root = t.path().components().any(|c| {
        mrsizeme::category::Category::of_folder(&c.as_os_str().to_string_lossy())
            == Some(mrsizeme::category::Category::Caches)
    });
    if temp_root {
        assert!(h.query_by_label("Caches & Temporäres").is_some());
    } else {
        assert!(h.query_by_label("Bilder").is_some());
        assert!(h.query_by_label("Dokumente").is_some());
    }

    h.get_by_label("Alter").click();
    h.run();
    // The sample files were just written.
    assert!(h.query_by_label("Jünger als 1 Monat").is_some());

    h.get_by_label("Älteste").click();
    h.run();
    assert!(h.query_by_label("notiz.txt").is_some());

    h.get_by_label("Suche").click();
    h.run();
    assert!(h.query_by_label_contains("4 Treffer").is_some());

    // Both charts, with every measure.
    h.get_by_label("Ringdiagramm").click();
    h.run();
    for measure in ["Anzahl", "Belegt", "Größe"] {
        // Column headings share these names; the chart buttons come last.
        h.get_all_by_label(measure).last().unwrap().click();
        h.run();
    }
    h.get_by_label("Kachelkarte").click();
    h.run();
}
