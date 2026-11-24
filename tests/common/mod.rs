//! Shared setup for all integration tests.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Must be called first in every test: blocks every action outside the test.
pub fn setup() {
    mrsizeme::external::block_for_tests();
}

/// A fresh folder in the system's temp folder, removed again on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("mrsizeme-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes a file of `len` bytes (folders are created as needed).
    pub fn file(&self, rel: &str, len: usize) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![b'x'; len]).unwrap();
        p
    }

    pub fn dir(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Make locked test folders removable again.
            use std::os::unix::fs::PermissionsExt;
            for entry in fs::read_dir(&self.0).into_iter().flatten().flatten() {
                let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o755));
            }
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}
