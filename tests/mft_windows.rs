//! The fast MFT scan on a real NTFS drive. Needs admin rights, so it only
//! runs where the tests have them (e.g. the Windows build machines of the CI).

#![cfg(windows)]

mod common;

use common::{TempDir, setup};
use mrsizeme::scan::{Options, Progress, scan};

#[test]
fn mft_scan_matches_the_normal_scan() {
    setup();
    if !mrsizeme::elevate::is_elevated() {
        eprintln!("skipped: no admin rights");
        return;
    }
    let t = TempDir::new();
    t.file("a/b.bin", 5000);
    t.file("a/c/d.bin", 123_456);
    t.file("e.txt", 7);
    // The MFT on the disk lags behind the cache; write it out first.
    let letter = t.path().to_string_lossy().chars().next().unwrap();
    if let Ok(vol) = std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\.\{letter}:")) {
        let _ = vol.sync_all();
    }
    let direct = scan(t.path(), &Options::default(), &Progress::default()).unwrap();
    let fast = match mrsizeme::mft::scan(t.path(), &Progress::default()) {
        Some(result) => result.expect("MFT scan failed"),
        None => {
            eprintln!("skipped: drive is not NTFS");
            return;
        }
    };
    assert_eq!(fast.root().size, direct.root().size);
    assert_eq!(fast.root().files, direct.root().files);
    assert_eq!(fast.root().dirs, direct.root().dirs);
    assert_eq!(fast.root_path, direct.root_path);
    let d = fast.find(&t.path().join("a").join("c").join("d.bin")).expect("d.bin in MFT tree");
    assert_eq!(fast.node(d).size, 123_456);
}
