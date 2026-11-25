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
    assert!(h.query_all_by_label_contains("4 Treffer").next().is_some());

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

/// A made-up project folder with a big node_modules (no real files).
fn cleanup_tree() -> mrsizeme::tree::Tree {
    use mrsizeme::tree::{Kind, NO_PARENT, Node, ROOT, Tree};
    use std::ffi::OsStr;
    let mut t = Tree::new(std::path::PathBuf::from("/projekte/web"), Kind::Dir);
    let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
    let file = |n: &str, size: u64| {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = size;
        f
    };
    let first = t.add_children(ROOT, [dir("node_modules"), file("package.json", 300)]);
    t.add_children(first, [file("bundle.js", 400 << 20)]);
    t.sum_up();
    t
}

#[test]
fn cleanup_info_trash_question_and_ai() {
    let _guard = lock();
    setup();
    i18n::set_lang(Lang::De);
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1300.0, 820.0))
        .with_max_steps(32)
        .build_eframe(|_cc| App::with_tree(cleanup_tree()));
    h.run();

    // "Aufräumen" is both a menu band tab and a list tab: click them all,
    // which opens the suggestion list and the clean-up tab of the band.
    let count = h.get_all_by_label("Aufräumen").count();
    for i in 0..count {
        if let Some(n) = h.get_all_by_label("Aufräumen").nth(i) {
            n.click();
        }
        h.run();
    }
    assert!(h.query_all_by_label_contains("ohne Risiko freigeben").next().is_some());
    // Nothing yellow in this folder: no sentence about tools.
    assert!(h.query_all_by_label_contains("passenden Werkzeugen").next().is_none());
    h.get_by_label("JavaScript-Pakete (node_modules)").click();
    h.run();

    // "What is this?" from the menu band.
    h.get_by_label("Was ist das?").click();
    h.run();
    assert!(h.query_by_label("So räumen Sie richtig auf").is_some());
    assert!(h.query_all_by_label_contains("node_modules").next().is_some());

    // The AI part: the browser is blocked in tests; Claude needs a key.
    h.get_by_label("Im Browser öffnen").click();
    h.run();
    h.get_by_label("Claude").click();
    h.run();
    assert!(h.query_all_by_label_contains("Kein Schlüssel hinterlegt").next().is_some());

    // The recycle bin question appears; confirming does nothing in tests.
    h.get_all_by_label("In den Papierkorb").last().unwrap().click();
    h.run();
    assert!(h.query_by_label("In den Papierkorb verschieben?").is_some());
    h.get_all_by_label("In den Papierkorb").last().unwrap().click();
    h.run();
    assert!(h.query_by_label("In den Papierkorb verschieben?").is_none());
    assert!(h.query_by_label("Fehler").is_none());

    // Offline switch.
    h.get_by_label("Komplett offline").click();
    h.run();
    h.get_by_label("Google KI-Modus").click();
    h.run();
    assert!(h.query_all_by_label_contains("ist eingeschaltet").next().is_some());
}

#[test]
fn low_space_warning_can_be_hidden() {
    use mrsizeme::disk::{Low, Space};
    let _guard = lock();
    setup();
    i18n::set_lang(Lang::De);
    let gib = 1u64 << 30;
    let low = vec![Low {
        label: "C:\\".into(),
        path: "C:\\".into(),
        space: Space { total: 500 * gib, free: 20 * gib },
    }];
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1200.0, 760.0))
        .with_max_steps(32)
        .build_eframe(move |_cc| App::with_tree(project(10)).with_low_disks(low.clone()));
    h.run();
    assert!(h.query_all_by_label_contains("Wenig Platz: Auf C:\\ sind nur noch 4 % frei").next().is_some());
    h.get_by_label("Ausblenden").click();
    h.run();
    assert!(h.query_all_by_label_contains("Wenig Platz").next().is_none());
}

/// The made-up project folder with a node_modules of the given size in MiB.
fn project(mib: u64) -> mrsizeme::tree::Tree {
    use mrsizeme::tree::{Kind, NO_PARENT, Node, ROOT, Tree};
    use std::ffi::OsStr;
    let mut t = Tree::new(std::path::PathBuf::from("/projekte/web"), Kind::Dir);
    let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
    let file = |n: &str, size: u64| {
        let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
        f.size = size;
        f
    };
    let first = t.add_children(ROOT, [dir("node_modules"), file("video.mp4", 50 << 20)]);
    t.add_children(first, [file("bundle.js", mib << 20)]);
    t.sum_up();
    t
}

#[test]
fn history_compare_forecast_and_clear() {
    use mrsizeme::disk::Space;
    use mrsizeme::history::Store;
    let _guard = lock();
    setup();
    i18n::set_lang(Lang::De);
    let dir = TempDir::new();
    let store = Store::at(dir.path().join("history"));
    let now = mrsizeme::query::now();
    let day = 86_400;
    let gib = 1u64 << 30;
    // Two earlier scans: the folder grows, the disk fills by 1 GiB a day.
    for (days_ago, mib, free) in [(10, 100, 40 * gib), (3, 250, 33 * gib)] {
        let space = Some(Space { total: 500 * gib, free });
        store.save(&project(mib), now - days_ago * day, space).unwrap();
    }
    let dir_path = dir.path().join("history");
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1300.0, 860.0))
        .with_max_steps(32)
        .build_eframe(move |_cc| App::with_history(project(400), dir_path.clone(), None));
    h.run();

    // The menu band tab comes first, then its large button opens the list.
    h.get_all_by_label("Verlauf").next().unwrap().click();
    h.run();
    h.get_by_label("Verlauf & Vergleich").click();
    h.run();
    // Compared with the newest earlier scan: bundle.js grew by 150 MiB.
    assert!(h.query_all_by_label_contains("gewachsen").next().is_some());
    assert!(h.query_all_by_label_contains("bundle.js").next().is_some());
    assert!(h.query_all_by_label_contains("pro Monat").next().is_some());
    assert!(h.query_all_by_label_contains("voll").next().is_some());

    // Compare with the oldest scan instead.
    h.get_all_by_label("Vergleichen").last().unwrap().click();
    h.run();
    assert!(h.query_all_by_label_contains("+300").next().is_some());

    // A click on a change shows it in the folder list.
    h.query_all_by_label_contains("bundle.js").last().unwrap().click();
    h.run();
    assert!(h.query_by_label("Größe ⏷").is_some() || h.query_by_label("node_modules").is_some());

    // Keep scans off and on, then delete the history (only the test folder).
    h.get_by_label("Scans merken").click();
    h.run();
    h.get_by_label("Verlauf löschen").click();
    h.run();
    assert!(h.query_all_by_label_contains("Den ganzen Verlauf löschen").next().is_some());
    h.get_by_label("Löschen").click();
    h.run();
    assert!(!dir.path().join("history").exists());
    assert!(h.query_by_label("Fehler").is_none());
}

#[test]
fn duplicates_are_found_and_shown() {
    let _guard = lock();
    let t = TempDir::new();
    let data = vec![7u8; 2 << 20];
    t.file("a/kopie1.bin", 0);
    std::fs::write(t.path().join("a/kopie1.bin"), &data).unwrap();
    std::fs::create_dir_all(t.path().join("b")).unwrap();
    std::fs::write(t.path().join("b/kopie2.bin"), &data).unwrap();
    t.file("einzeln.bin", 3 << 20);
    let mut h = harness(&t);
    h.get_all_by_label("Aufräumen").next().unwrap().click();
    h.run();
    h.get_by_label("Doppelte Dateien").click();
    h.run();
    h.get_by_label("Suche starten").click();
    for _ in 0..100 {
        h.run();
        if h.query_all_by_label_contains("doppelt belegt").next().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(h.query_all_by_label_contains("1 Gruppen gleicher Dateien").next().is_some());
    assert!(h.query_all_by_label_contains("kopie2.bin").next().is_some());
    assert!(h.query_all_by_label_contains("einzeln.bin").next().is_none());
}

#[test]
fn cloud_and_programs() {
    use mrsizeme::apps::App as Program;
    use mrsizeme::cloud::Root;
    use mrsizeme::tree::{Kind, NO_PARENT, Node, ROOT, Tree, flags};
    use std::ffi::OsStr;
    use std::path::PathBuf;
    let _guard = lock();
    setup();
    i18n::set_lang(Lang::De);
    let make = || {
        let mut t = Tree::new(PathBuf::from("/home/x"), Kind::Dir);
        let dir = |n: &str| Node::new(OsStr::new(n).into(), NO_PARENT, Kind::Dir);
        let file = |n: &str, size: u64, online: bool| {
            let mut f = Node::new(OsStr::new(n).into(), NO_PARENT, Kind::File);
            f.size = size;
            f.allocated = if online { 0 } else { size };
            if online {
                f.flags |= flags::ONLINE_ONLY;
            }
            f
        };
        let top = t.add_children(ROOT, [dir("OneDrive"), dir("Spiele")]);
        let od = t.add_children(top, [dir("Fotos")]);
        t.add_children(od, [file("urlaub.jpg", 30 << 20, false), file("alt.mp4", 900 << 20, true)]);
        t.add_children(top + 1, [file("spiel.exe", 200 << 20, false)]);
        t.sum_up();
        t
    };
    let roots = vec![Root { name: "OneDrive".into(), path: PathBuf::from("/home/x/OneDrive") }];
    let programs = vec![Program {
        name: "Spiel".into(),
        location: Some(PathBuf::from("/home/x/Spiele")),
        data: vec![],
        reported: None,
    }];
    let mut h =
        Harness::builder().with_size(eframe::egui::vec2(1300.0, 820.0)).with_max_steps(32).build_eframe(
            move |_cc| App::with_tree(make()).with_cloud_roots(roots.clone()).with_apps(programs.clone()),
        );
    h.run();
    h.get_by_label("Cloud").click();
    h.run();
    assert!(h.query_all_by_label_contains("Nur online: 900 MB (1 Dateien)").next().is_some());
    if mrsizeme::cloud::can_free_up() {
        h.get_all_by_label("Nur online behalten").next().unwrap().click();
        h.run();
        // The question appears; confirming does nothing in tests.
        assert!(h.query_all_by_label_contains("nur online behalten?").next().is_some());
        h.get_all_by_label("Nur online behalten").last().unwrap().click();
        h.run();
    }
    assert!(h.query_by_label("Fehler").is_none());

    h.get_by_label("Programme").click();
    h.run();
    assert!(h.query_by_label("Spiel").is_some());
    assert!(h.query_all_by_label("200 MB").next().is_some());
}

#[test]
fn move_assistant_moves_and_links() {
    let _guard = lock();
    setup();
    i18n::set_lang(Lang::De);
    // Not in the system temp folder: on macOS that lies below a protected folder.
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-tmp")
        .join(format!("ui-move-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("quelle/Spiele/level")).unwrap();
    std::fs::write(base.join("quelle/Spiele/level/karte.dat"), vec![1u8; 70_000]).unwrap();
    std::fs::create_dir_all(base.join("ziel")).unwrap();
    let tree = scan(&base.join("quelle"), &Options::default(), &Progress::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1300.0, 820.0))
        .with_max_steps(32)
        .build_eframe(move |_cc| App::with_tree(tree));
    h.run();
    h.get_by_label("Spiele").click();
    h.run();
    h.get_all_by_label("Aufräumen").next().unwrap().click();
    h.run();
    h.get_by_label("Auf andere Platte verschieben").click();
    h.run();
    let field = h.get_all_by_label("Ziel:").last().unwrap();
    field.focus();
    field.type_text(&base.join("ziel").display().to_string());
    h.run();
    h.get_by_label("Original danach löschen (erst wenn die Kopie geprüft ist)").click();
    h.run();
    h.get_all_by_label("Verschieben").last().unwrap().click();
    for _ in 0..100 {
        h.run();
        if h.query_all_by_label_contains("Fertig:").next().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(h.query_all_by_label_contains("1 Dateien").next().is_some());
    assert!(base.join("ziel/Spiele/level/karte.dat").exists());
    assert!(
        std::fs::symlink_metadata(base.join("quelle/Spiele")).is_ok_and(|m| !m.is_dir() || cfg!(windows))
    );
    assert!(h.query_by_label("Fehler").is_none());
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn context_menu_entry_asks_first() {
    let _guard = lock();
    let t = sample();
    let mut h = harness(&t);
    // The button says "add" or "remove", depending on the system; both ask first.
    h.get_by_label("Im Rechtsklick-Menü").click();
    h.run();
    let add = h.query_all_by_label_contains("hinzufügen?").next().is_some();
    let remove = h.query_all_by_label_contains("entfernen?").next().is_some();
    assert!(add || remove);
    let button = if add { "Hinzufügen" } else { "Entfernen" };
    h.get_by_label(button).click();
    h.run();
    // Blocked in tests: nothing changed, no error.
    assert!(h.query_by_label("Fehler").is_none());
    assert!(h.query_all_by_label_contains("hinzufügen?").next().is_none());
}
