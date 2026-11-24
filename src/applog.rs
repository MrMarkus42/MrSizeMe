//! Log file. Messages never contain file contents, and the home folder is
//! written as "~" so the log does not show the user name.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;

/// Above this size the log is started anew (the old one is kept as .old).
const MAX_LOG: u64 = 1024 * 1024;

struct FileLogger {
    file: Mutex<Option<File>>,
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

/// Folder for the log file.
pub fn log_dir() -> Option<PathBuf> {
    let home = home_dir();
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("MrSizeMe"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Logs/MrSizeMe"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/state")))
            .map(|d| d.join("mrsizeme"))
    }
}

pub fn log_file() -> Option<PathBuf> {
    log_dir().map(|d| d.join("mrsizeme.log"))
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Replaces the home folder at the start of a path with "~".
pub fn private(path: &Path) -> String {
    private_with(path, home_dir().as_deref())
}

fn private_with(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        return Path::new("~").join(rest).display().to_string();
    }
    path.display().to_string()
}

/// Starts logging into the log file (and to stderr). Safe to call twice.
pub fn init() {
    let file = log_file().and_then(|path| {
        fs::create_dir_all(path.parent()?).ok()?;
        if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG) {
            let _ = fs::rename(&path, path.with_extension("log.old"));
        }
        OpenOptions::new().create(true).append(true).open(&path).ok()
    });
    if LOGGER.set(FileLogger { file: Mutex::new(file) }).is_ok()
        && let Some(logger) = LOGGER.get()
        && log::set_logger(logger).is_ok()
    {
        log::set_max_level(log::LevelFilter::Info);
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let time = format!(
            "{} {:02}:{:02}:{:02} UTC",
            crate::format::iso_date(secs),
            secs.rem_euclid(86_400) / 3600,
            secs.rem_euclid(3600) / 60,
            secs.rem_euclid(60)
        );
        let line = format!("{time} {:5} {}\n", record.level(), record.args());
        eprint!("{line}");
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_folder_is_hidden() {
        let home = Path::new("/home/someone");
        let shown = private_with(Path::new("/home/someone/Bilder/x"), Some(home));
        assert_eq!(shown, Path::new("~").join("Bilder/x").display().to_string());
        assert_eq!(private_with(Path::new("/srv/data"), Some(home)), "/srv/data");
        assert_eq!(private_with(Path::new("/home/someoneelse"), Some(home)), "/home/someoneelse");
    }
}
