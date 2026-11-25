//! Updates: look for a newer version on GitHub, download it, check its
//! minisign signature against the key built into the program, and start it.
//!
//! The program only looks for updates if the user allowed it (asked at the
//! first start), never in "fully offline" mode and never in tests. A file
//! without a valid signature is refused and deleted.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use minisign_verify::{PublicKey, Signature};

use crate::external::{self, Outcome};

pub const REPO: &str = "MrMarkus42/MrSizeMe";
/// The public key of the release signatures (made by the maintainer; the
/// secret key never leaves the maintainer's computer).
const KEY_FILE: &str = include_str!("../assets/update-key.pub");
/// Largest download accepted.
const MAX_DOWNLOAD: u64 = 300 << 20;

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The built-in key; `None` while no key was made yet (then nothing is installed).
pub fn public_key() -> Option<PublicKey> {
    PublicKey::decode(KEY_FILE.trim()).ok()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Without a leading "v".
    pub version: String,
    pub notes: String,
    /// The web page of the release.
    pub page: String,
    pub assets: Vec<Asset>,
}

/// Reads the answer of the GitHub API for a release.
pub fn parse_release(v: &serde_json::Value) -> Option<Release> {
    let tag = v["tag_name"].as_str()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let assets = v["assets"]
        .as_array()?
        .iter()
        .filter_map(|a| {
            let url = a["browser_download_url"].as_str()?;
            // Downloads only from GitHub itself, over https.
            if !url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) {
                return None;
            }
            Some(Asset {
                name: a["name"].as_str()?.to_string(),
                url: url.to_string(),
                size: a["size"].as_u64()?,
            })
        })
        .collect();
    let page = v["html_url"].as_str().filter(|u| u.starts_with("https://github.com/")).unwrap_or_default();
    Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        notes: v["body"].as_str().unwrap_or_default().chars().take(20_000).collect(),
        page: page.to_string(),
        assets,
    })
}

/// "1.2.10" > "1.2.9"; a version with a suffix ("1.0.0-rc1") is older than without.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> (Vec<u64>, bool) {
        let (main, pre) = match v.split_once('-') {
            Some((m, _)) => (m, true),
            None => (v, false),
        };
        (main.split('.').map(|p| p.parse().unwrap_or(0)).collect(), pre)
    }
    let ((a, a_pre), (b, b_pre)) = (parts(candidate), parts(current));
    let len = a.len().max(b.len());
    for i in 0..len {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    !a_pre && b_pre
}

/// How this copy of the program was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    /// Windows with the installer.
    Installer,
    /// Windows portable (a file `portable.txt` next to the program).
    Portable,
    /// macOS disk image.
    Dmg,
    /// Linux archive.
    Archive,
}

pub fn package() -> Package {
    if cfg!(windows) {
        if crate::settings::is_portable() { Package::Portable } else { Package::Installer }
    } else if cfg!(target_os = "macos") {
        Package::Dmg
    } else {
        Package::Archive
    }
}

/// File name of the download for a version and package.
pub fn asset_name(version: &str, p: Package) -> String {
    match p {
        Package::Installer => format!("MrSizeMe-{version}-windows-x64-setup.exe"),
        Package::Portable => format!("MrSizeMe-{version}-windows-x64.exe"),
        Package::Dmg => format!("MrSizeMe-{version}-macos.dmg"),
        Package::Archive => format!("MrSizeMe-{version}-linux-x86_64.tar.gz"),
    }
}

fn blocked(offline: bool) -> io::Result<()> {
    if external::is_blocked() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked in test mode"));
    }
    if offline {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "offline mode"));
    }
    Ok(())
}

fn get(url: &str) -> io::Result<ureq::http::Response<ureq::Body>> {
    let resp = crate::ai::agent()
        .get(url)
        .header("User-Agent", &format!("MrSizeMe/{}", current_version()))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let status = resp.status().as_u16();
    if status >= 400 {
        return Err(io::Error::other(format!("HTTP {status}")));
    }
    Ok(resp)
}

/// Asks GitHub for the newest release; `None` if this version is up to date.
pub fn check(offline: bool) -> io::Result<Option<Release>> {
    blocked(offline)?;
    let mut resp = get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))?;
    let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| io::Error::other(e.to_string()))?;
    let r = parse_release(&v).ok_or_else(|| io::Error::other("unexpected answer"))?;
    Ok(is_newer(&r.version, current_version()).then_some(r))
}

/// Checks a minisign signature (text of the .minisig file) against the built-in key.
pub fn verify(data: &[u8], signature: &str, key: &PublicKey) -> Result<(), String> {
    let sig = Signature::decode(signature).map_err(|e| e.to_string())?;
    key.verify(data, &sig, false).map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    cancel: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn download_bytes(asset: &Asset, p: Option<&Progress>) -> io::Result<Vec<u8>> {
    if asset.size > MAX_DOWNLOAD {
        return Err(io::Error::other("download too large"));
    }
    let mut resp = get(&asset.url)?;
    let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut data = Vec::with_capacity(asset.size as usize);
    let mut buf = vec![0u8; 256 * 1024];
    if let Some(p) = p {
        p.total.store(asset.size, Ordering::Relaxed);
    }
    loop {
        if p.is_some_and(|p| p.cancel.load(Ordering::Relaxed)) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(p) = p {
            p.done.fetch_add(n as u64, Ordering::Relaxed);
        }
    }
    Ok(data)
}

/// Downloads the file for this computer and its signature, checks it and
/// saves it in `dir`. Returns the saved file.
pub fn download(release: &Release, offline: bool, dir: &Path, p: &Progress) -> io::Result<PathBuf> {
    blocked(offline)?;
    let key = public_key().ok_or_else(|| io::Error::other("this version has no update key"))?;
    let name = asset_name(&release.version, package());
    let find = |n: &str| release.assets.iter().find(|a| a.name == n).cloned();
    let file = find(&name).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.clone()))?;
    let sig = find(&format!("{name}.minisig"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name}.minisig")))?;
    let sig_text =
        String::from_utf8(download_bytes(&sig, None)?).map_err(|_| io::Error::other("bad signature file"))?;
    let data = download_bytes(&file, Some(p))?;
    verify(&data, &sig_text, &key).map_err(|e| io::Error::other(format!("signature check failed: {e}")))?;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(&name);
    let mut f = std::fs::File::create(&path)?;
    f.write_all(&data)?;
    f.sync_all()?;
    Ok(path)
}

/// Starts a checked download: the installer (Windows), replaces the program
/// file (portable), opens the disk image (macOS) or shows the archive (Linux).
pub fn install(path: &Path, p: Package) -> io::Result<Outcome> {
    if external::is_blocked() {
        log::info!("outside action blocked (test mode): install update");
        return Ok(Outcome::Blocked);
    }
    match p {
        Package::Installer => {
            std::process::Command::new(path).spawn()?;
        }
        Package::Portable => {
            let exe = std::env::current_exe()?;
            let old = exe.with_extension("old.exe");
            let _ = std::fs::remove_file(&old);
            // A running program file may be renamed, not overwritten.
            std::fs::rename(&exe, &old)?;
            if let Err(e) = std::fs::copy(path, &exe) {
                let _ = std::fs::rename(&old, &exe);
                return Err(e);
            }
            std::process::Command::new(&exe).spawn()?;
        }
        Package::Dmg => {
            std::process::Command::new("/usr/bin/open").arg(path).spawn()?;
        }
        Package::Archive => {
            if let Some(dir) = path.parent() {
                external::open_folder(dir)?;
            }
        }
    }
    Ok(Outcome::Done)
}

/// Removes what an earlier update left behind (the old portable program file).
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("2.0", "1.99.99"));
        assert!(is_newer("1.0.0", "1.0.0-rc1"));
        assert!(!is_newer("1.0.0-rc1", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn release_answer() {
        let v = serde_json::json!({
            "tag_name": "v1.2.0",
            "html_url": "https://github.com/MrMarkus42/MrSizeMe/releases/tag/v1.2.0",
            "body": "Neu: alles",
            "assets": [
                {"name": "MrSizeMe-1.2.0-macos.dmg", "size": 10,
                 "browser_download_url": "https://github.com/MrMarkus42/MrSizeMe/releases/download/v1.2.0/MrSizeMe-1.2.0-macos.dmg"},
                {"name": "evil.exe", "size": 10, "browser_download_url": "https://example.com/evil.exe"}
            ]
        });
        let r = parse_release(&v).unwrap();
        assert_eq!(r.version, "1.2.0");
        // Only downloads from the project's own release page are taken.
        assert_eq!(r.assets.len(), 1);
        let mut draft = v.clone();
        draft["draft"] = true.into();
        assert!(parse_release(&draft).is_none());
        assert_eq!(asset_name("1.2.0", Package::Dmg), "MrSizeMe-1.2.0-macos.dmg");
    }

    #[test]
    fn signatures() {
        // Test data of minisign: the current (prehashed) form is accepted …
        let key = PublicKey::from_base64("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3").unwrap();
        let sig = "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1556193335\tfile:test\n\
y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";
        // … the old form (not prehashed) is refused.
        let legacy = "untrusted comment: signature from minisign secret key\n\
RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\n\
trusted comment: timestamp:1555779966\tfile:test\n\
QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
        assert!(verify(b"test", legacy, &key).is_err());
        assert!(verify(b"test", sig, &key).is_ok());
        assert!(verify(b"Test", sig, &key).is_err());
        assert!(verify(b"test", "rubbish", &key).is_err());
    }

    #[test]
    fn the_update_key_is_built_in() {
        // A release without a key could never install updates.
        assert!(public_key().is_some());
    }

    #[test]
    fn nothing_happens_in_tests() {
        assert!(check(false).is_err());
        assert_eq!(install(Path::new("x"), Package::Archive).unwrap(), Outcome::Blocked);
    }
}
