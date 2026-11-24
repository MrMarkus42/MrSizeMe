mod common;

use std::path::Path;

use common::{TempDir, setup};
use mrsizeme::external;
use mrsizeme::scan::{Options, Progress, scan};
use mrsizeme::tree::{Kind, ROOT, Tree, flags};

fn run(root: &Path) -> Tree {
    scan(root, &Options { threads: 4, ..Options::default() }, &Progress::default()).unwrap()
}

#[test]
fn outside_actions_are_blocked() {
    setup();
    assert!(external::is_blocked());
    assert_eq!(external::open_folder(Path::new(".")).unwrap(), external::Outcome::Blocked);
}

#[test]
fn sizes_counts_and_order() {
    setup();
    let t = TempDir::new();
    t.file("a.bin", 1000);
    t.file("big/b.bin", 50_000);
    t.file("big/deeper/c.bin", 20_000);
    t.file("small/d.txt", 10);
    t.dir("empty");

    let tree = run(t.path());
    let root = tree.root();
    assert_eq!(root.size, 71_010);
    assert_eq!(root.files, 4);
    assert_eq!(root.dirs, 4);
    assert!(root.allocated > 0);
    assert!(tree.errors.is_empty());
    assert!(!tree.cancelled);

    // Largest first.
    let names: Vec<_> = tree.children(ROOT).iter().map(|&c| tree.node(c).name_lossy().into_owned()).collect();
    assert_eq!(names, ["big", "a.bin", "small", "empty"]);

    let big = tree.find(&t.path().join("big")).unwrap();
    assert_eq!(tree.node(big).size, 70_000);
    assert_eq!(tree.node(big).files, 2);
    assert_eq!(tree.node(big).dirs, 1);
    assert_eq!(tree.path(big), t.path().join("big"));
    // No node is left marked as not scanned.
    assert!(tree.nodes.iter().all(|n| !n.has(flags::NOT_SCANNED)));
}

#[test]
fn a_single_file_as_root() {
    setup();
    let t = TempDir::new();
    let f = t.file("only.bin", 1234);
    let tree = run(&f);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.root().kind, Kind::File);
    assert_eq!(tree.root().size, 1234);
}

#[test]
fn missing_root_is_an_error() {
    setup();
    let t = TempDir::new();
    assert!(scan(&t.path().join("nope"), &Options::default(), &Progress::default()).is_err());
}

#[test]
fn cancel_returns_partial_tree() {
    setup();
    let t = TempDir::new();
    for i in 0..20 {
        t.file(&format!("d{i}/f.bin"), 10);
    }
    let progress = Progress::default();
    progress.cancel();
    let tree = scan(t.path(), &Options::default(), &progress).unwrap();
    assert!(tree.cancelled);
    assert!(tree.root().size <= 200);
}

#[test]
fn many_folders_with_many_threads() {
    setup();
    let t = TempDir::new();
    for i in 0..40 {
        for j in 0..5 {
            t.file(&format!("l{i}/m{j}/f{j}.dat"), i * 10 + j);
        }
    }
    let progress = Progress::default();
    let tree = scan(t.path(), &Options { threads: 8, ..Options::default() }, &progress).unwrap();
    let expected: usize = (0..40).flat_map(|i| (0..5).map(move |j| i * 10 + j)).sum();
    assert_eq!(tree.root().size, expected as u64);
    assert_eq!(tree.root().files, 200);
    assert_eq!(tree.root().dirs, 240);
    assert_eq!(progress.files.load(std::sync::atomic::Ordering::Relaxed), 200);
}

#[test]
fn skip_list_is_respected() {
    setup();
    let t = TempDir::new();
    t.file("keep/a", 100);
    t.file("skip/b", 1000);
    let opts = Options { skip: vec![t.path().join("skip")], ..Options::default() };
    let tree = scan(t.path(), &opts, &Progress::default()).unwrap();
    assert_eq!(tree.root().size, 100);
    let skip = tree.find(&t.path().join("skip")).unwrap();
    assert!(tree.node(skip).has(flags::SKIPPED));
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn symlinks_are_not_followed() {
        setup();
        let t = TempDir::new();
        t.file("real/data.bin", 5000);
        std::os::unix::fs::symlink(t.path().join("real"), t.path().join("link")).unwrap();
        // A loop must not hang the scan.
        std::os::unix::fs::symlink(t.path(), t.path().join("real/loop")).unwrap();
        let tree = run(t.path());
        assert_eq!(tree.root().size, 5000);
        let link = tree.find(&t.path().join("link")).unwrap();
        assert_eq!(tree.node(link).kind, Kind::Link);
    }

    #[test]
    fn hardlinks_count_once() {
        setup();
        let t = TempDir::new();
        let a = t.file("a.bin", 8000);
        fs::hard_link(&a, t.path().join("b.bin")).unwrap();
        let tree = run(t.path());
        assert_eq!(tree.root().size, 8000);
        assert_eq!(tree.root().files, 2);
        let dups = tree.nodes.iter().filter(|n| n.has(flags::HARDLINK_DUP)).count();
        assert_eq!(dups, 1);
    }

    #[test]
    fn unreadable_folder_is_reported() {
        setup();
        let t = TempDir::new();
        t.file("open/a", 10);
        let locked = t.dir("locked");
        fs::write(locked.join("secret"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read_dir(&locked).is_ok() {
            return; // running as root: the folder stays readable
        }
        let tree = run(t.path());
        let id = tree.find(&locked).unwrap();
        assert!(tree.node(id).has(flags::ERROR));
        assert!(tree.errors[&id].starts_with("Zugriff verweigert"), "{}", tree.errors[&id]);
        assert!(tree.root().has(flags::ERROR));
        assert_eq!(tree.root().size, 10);
    }
}
