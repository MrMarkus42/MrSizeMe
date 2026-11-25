//! Live update with a real folder: files appear, grow and disappear.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{TempDir, setup};
use mrsizeme::live::{Live, Status};
use mrsizeme::scan::{Options, Progress, scan};
use mrsizeme::tree::{SortKey, Tree};

fn wait_until(live: &mut Live, tree: &mut Tree, what: &str, done: impl Fn(&Live, &Tree) -> bool) {
    let start = Instant::now();
    while !done(live, tree) {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}");
        live.poll(tree, (SortKey::Size, false));
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn changes_show_up_without_a_new_scan() {
    setup();
    let t = TempDir::new();
    t.file("a/eins.bin", 1000);
    t.file("zwei.bin", 500);
    let mut tree = scan(t.path(), &Options::default(), &Progress::default()).unwrap();
    assert_eq!(tree.root().size, 1500);

    let mut live = Live::start(t.path(), || {});
    wait_until(&mut live, &mut tree, "the watcher", |l, _| l.status != Status::Starting);
    assert_eq!(live.status, Status::Watching);

    t.file("a/neu.bin", 3000);
    t.file("b/c/tief.bin", 8000);
    std::fs::remove_file(t.path().join("zwei.bin")).unwrap();
    std::fs::write(t.path().join("a/eins.bin"), vec![b'y'; 4000]).unwrap();

    wait_until(&mut live, &mut tree, "the new numbers", |l, tr| tr.root().size == 15_000 && !l.busy());
    let has = |rel: &str| tree.find(&t.path().join(rel)).is_some();
    assert!(has("a/neu.bin") && has("b/c/tief.bin") && has("a/eins.bin"));
    assert!(!has("zwei.bin"));
    assert_eq!(tree.root().files, 3);
    assert_eq!(tree.root().dirs, 3);
    // The largest folder comes first again.
    let first = tree.children(mrsizeme::tree::ROOT)[0];
    assert_eq!(&*tree.node(first).name, std::ffi::OsStr::new("b"));
    assert!(live.updates > 0);
    assert!(!live.lost);

    // A folder that is deleted disappears with everything below it.
    std::fs::remove_dir_all(t.path().join("b")).unwrap();
    wait_until(&mut live, &mut tree, "the removal", |l, tr| tr.root().size == 7000 && !l.busy());
    assert!(tree.find(&t.path().join("b")).is_none());
    assert_eq!(tree.root().dirs, 1);
    drop(live);
    assert!(Path::new(t.path()).exists());
}
