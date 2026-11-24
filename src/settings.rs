//! Settings that stay between starts, in a small text file in the user's
//! configuration folder. Keys (passwords) are never stored here – see
//! [`crate::secrets`].

use std::io;
use std::path::PathBuf;

use crate::i18n::Lang;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `None`: follow the system language.
    pub lang: Option<Lang>,
    /// Blocks everything that would go to the internet (AI, web search, updates).
    pub offline: bool,
    pub ollama_url: String,
    pub ollama_model: String,
    pub gemini_model: String,
    pub claude_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: None,
            offline: false,
            ollama_url: "http://localhost:11434".into(),
            ollama_model: String::new(),
            gemini_model: "gemini-3.5-flash-lite".into(),
            claude_model: "claude-opus-5-5".into(),
        }
    }
}

/// Folder for settings (and, on Linux, the key file).
pub fn config_dir() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("MrSizeMe"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/MrSizeMe"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")))
            .map(|d| d.join("mrsizeme"))
    }
}

fn file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.txt"))
}

impl Settings {
    /// Reads the settings; unknown or broken lines are ignored.
    pub fn load() -> Settings {
        file().and_then(|f| std::fs::read_to_string(f).ok()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().to_string();
            match key.trim() {
                "lang" => s.lang = Lang::from_code(&value),
                "offline" => s.offline = value == "1",
                "ollama_url" if !value.is_empty() => s.ollama_url = value,
                "ollama_model" => s.ollama_model = value,
                "gemini_model" if !value.is_empty() => s.gemini_model = value,
                "claude_model" if !value.is_empty() => s.claude_model = value,
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        // Values never contain line breaks (they come from single-line fields).
        let clean = |v: &str| v.replace(['\n', '\r'], " ");
        format!(
            "# MrSizeMe settings\nlang={}\noffline={}\nollama_url={}\nollama_model={}\ngemini_model={}\nclaude_model={}\n",
            self.lang.map_or("", Lang::code),
            u8::from(self.offline),
            clean(&self.ollama_url),
            clean(&self.ollama_model),
            clean(&self.gemini_model),
            clean(&self.claude_model),
        )
    }

    /// Writes the settings. In tests nothing is written.
    pub fn save(&self) -> io::Result<()> {
        if crate::external::is_blocked() {
            return Ok(());
        }
        let f = file().ok_or_else(|| io::Error::other("no configuration folder"))?;
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(f, self.to_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bad_lines() {
        let s = Settings {
            lang: Some(Lang::En),
            offline: true,
            ollama_model: "llama".into(),
            claude_model: "x\ny".into(),
            ..Settings::default()
        };
        let back = Settings::parse(&s.to_text());
        assert_eq!(back.lang, Some(Lang::En));
        assert!(back.offline);
        assert_eq!(back.ollama_model, "llama");
        assert_eq!(back.claude_model, "x y");
        let junk = Settings::parse("no equals\nlang=fr\noffline=yes\ngemini_model=\n");
        assert_eq!(junk, Settings::default());
    }
}
