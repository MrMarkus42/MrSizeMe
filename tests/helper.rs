//! The admin helper mode, run without admin rights: it must scan like the
//! window does and only write where it is allowed to.

mod common;

use std::process::Command;

use common::{TempDir, setup};
use mrsizeme::scan::{Options, Progress, scan};
use mrsizeme::snapshot;

fn helper() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_mrsizeme"));
    c.env("MRSIZEME_NO_EXTERNAL", "1");
    c
}

#[test]
fn helper_writes_the_same_scan() {
    setup();
    let t = TempDir::new();
    t.file("a/b.bin", 5000);
    t.file("c.txt", 12);
    let out_dir = std::env::temp_dir().join(format!("mrsizeme-admin-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();
    let out = out_dir.join("scan.bin");

    let status = helper().arg("--scan-helper").arg(t.path()).arg(&out).status().unwrap();
    assert!(status.success());
    let from_helper = snapshot::from_bytes(&std::fs::read(&out).unwrap()).unwrap();
    let direct = scan(t.path(), &Options::default(), &Progress::default()).unwrap();
    assert_eq!(from_helper.root().size, direct.root().size);
    assert_eq!(from_helper.root().files, 2);
    assert_eq!(from_helper.len(), direct.len());

    // It never overwrites an existing file.
    let again = helper().arg("--scan-helper").arg(t.path()).arg(&out).status().unwrap();
    assert_eq!(again.code(), Some(2));
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn helper_refuses_other_targets() {
    setup();
    let t = TempDir::new();
    let target = t.path().join("victim.txt");
    let status = helper().arg("--scan-helper").arg(t.path()).arg(&target).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(!target.exists());
}

#[test]
fn helper_can_write_to_standard_output() {
    setup();
    let t = TempDir::new();
    t.file("x.dat", 777);
    let out = helper().arg("--scan-helper").arg(t.path()).arg("-").output().unwrap();
    assert!(out.status.success());
    let tree = snapshot::from_bytes(&out.stdout).unwrap();
    assert_eq!(tree.root().size, 777);
}
