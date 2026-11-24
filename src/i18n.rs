//! Languages. Every text of the program is a field of [`Texts`], so a
//! missing translation is a compile error, not an empty label.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    /// Name of the language in that language.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let primary = code.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// The language of the operating system; English if it is not supported.
    pub fn from_system() -> Lang {
        sys_locale::get_locales().find_map(|l| Lang::from_code(&l)).unwrap_or(Lang::En)
    }

    pub fn texts(self) -> &'static Texts {
        match self {
            Lang::De => &DE,
            Lang::En => &EN,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        0 => Lang::De,
        _ => Lang::En,
    }
}

/// Texts of the current language.
pub fn tr() -> &'static Texts {
    lang().texts()
}

/// Number formatting of a language.
pub struct NumberStyle {
    pub decimal: char,
    pub group: char,
}

pub struct Texts {
    pub numbers: NumberStyle,
    pub bytes: &'static str,

    // Errors of the file system.
    pub err_access_denied: &'static str,
    pub err_not_found: &'static str,
    pub err_timeout: &'static str,
    pub err_invalid_name: &'static str,
    pub err_other: &'static str,
    pub err_system_code: &'static str,

    // Toolbar.
    pub choose_folder: &'static str,
    pub scan: &'static str,
    pub cancel: &'static str,
    pub rescan: &'static str,
    pub export_csv: &'static str,
    pub path_hint: &'static str,
    pub drives: &'static str,
    pub home_folder: &'static str,
    pub language: &'static str,
    pub theme_dark: &'static str,
    pub theme_light: &'static str,

    // Views.
    pub view_folders: &'static str,
    pub view_types: &'static str,
    pub view_largest: &'static str,
    pub map_title: &'static str,
    pub map_up: &'static str,
    pub show_size: &'static str,
    pub show_allocated: &'static str,

    // Columns.
    pub col_name: &'static str,
    pub col_size: &'static str,
    pub col_allocated: &'static str,
    pub col_share: &'static str,
    pub col_files: &'static str,
    pub col_folders: &'static str,
    pub col_modified: &'static str,
    pub col_type: &'static str,
    pub col_path: &'static str,
    pub no_extension: &'static str,
    pub kind_folder: &'static str,
    pub kind_file: &'static str,
    pub kind_link: &'static str,
    pub kind_other: &'static str,
    pub csv_size_bytes: &'static str,
    pub csv_allocated_bytes: &'static str,

    // Context menu.
    pub open_folder: &'static str,
    pub copy_path: &'static str,
    pub show_in_map: &'static str,

    // Status.
    pub welcome: &'static str,
    pub scanning: &'static str,
    pub files: &'static str,
    pub folders: &'static str,
    pub errors: &'static str,
    pub cancelled: &'static str,
    pub seconds: &'static str,
    pub not_scanned: &'static str,
    pub skipped: &'static str,
    pub hardlink: &'static str,
    pub link: &'static str,

    // Error window.
    pub error_title: &'static str,
    pub copy: &'static str,
    pub close: &'static str,
    pub export_done: &'static str,
}

pub static DE: Texts = Texts {
    numbers: NumberStyle { decimal: ',', group: '.' },
    bytes: "Bytes",

    err_access_denied: "Zugriff verweigert",
    err_not_found: "Nicht gefunden (inzwischen gelöscht?)",
    err_timeout: "Zeitüberschreitung",
    err_invalid_name: "Ungültiger Name",
    err_other: "Fehler",
    err_system_code: "Systemfehler",

    choose_folder: "Ordner wählen …",
    scan: "Scannen",
    cancel: "Abbrechen",
    rescan: "Neu scannen",
    export_csv: "Als CSV exportieren …",
    path_hint: "Ordner oder Laufwerk",
    drives: "Laufwerke",
    home_folder: "Benutzerordner",
    language: "Sprache",
    theme_dark: "Dunkel",
    theme_light: "Hell",

    view_folders: "Ordner",
    view_types: "Dateitypen",
    view_largest: "Größte Dateien",
    map_title: "Kachelkarte",
    map_up: "Eine Ebene höher",
    show_size: "Größe",
    show_allocated: "Belegt",

    col_name: "Name",
    col_size: "Größe",
    col_allocated: "Belegt",
    col_share: "Anteil",
    col_files: "Dateien",
    col_folders: "Ordner",
    col_modified: "Geändert",
    col_type: "Typ",
    col_path: "Pfad",
    no_extension: "(ohne Endung)",
    kind_folder: "Ordner",
    kind_file: "Datei",
    kind_link: "Verknüpfung",
    kind_other: "Sonstiges",
    csv_size_bytes: "Größe (Bytes)",
    csv_allocated_bytes: "Belegt (Bytes)",

    open_folder: "Ordner öffnen",
    copy_path: "Pfad kopieren",
    show_in_map: "In der Kachelkarte zeigen",

    welcome: "Wählen Sie einen Ordner oder ein Laufwerk und klicken Sie auf „Scannen“.",
    scanning: "Scanne",
    files: "Dateien",
    folders: "Ordner",
    errors: "Fehler",
    cancelled: "abgebrochen – Zahlen unvollständig",
    seconds: "s",
    not_scanned: "Nicht gescannt (abgebrochen)",
    skipped: "Nicht betreten (anderes Laufwerk oder ausgeschlossen)",
    hardlink: "Weitere Verknüpfung (Hardlink) einer schon gezählten Datei",
    link: "Verknüpfung – wird nicht verfolgt",

    error_title: "Fehler",
    copy: "Kopieren",
    close: "Schließen",
    export_done: "Export gespeichert",
};

pub static EN: Texts = Texts {
    numbers: NumberStyle { decimal: '.', group: ',' },
    bytes: "bytes",

    err_access_denied: "Access denied",
    err_not_found: "Not found (deleted meanwhile?)",
    err_timeout: "Timed out",
    err_invalid_name: "Invalid name",
    err_other: "Error",
    err_system_code: "system error",

    choose_folder: "Choose folder …",
    scan: "Scan",
    cancel: "Cancel",
    rescan: "Scan again",
    export_csv: "Export as CSV …",
    path_hint: "Folder or drive",
    drives: "Drives",
    home_folder: "Home folder",
    language: "Language",
    theme_dark: "Dark",
    theme_light: "Light",

    view_folders: "Folders",
    view_types: "File types",
    view_largest: "Largest files",
    map_title: "Tile map",
    map_up: "One level up",
    show_size: "Size",
    show_allocated: "On disk",

    col_name: "Name",
    col_size: "Size",
    col_allocated: "On disk",
    col_share: "Share",
    col_files: "Files",
    col_folders: "Folders",
    col_modified: "Modified",
    col_type: "Type",
    col_path: "Path",
    no_extension: "(no extension)",
    kind_folder: "Folder",
    kind_file: "File",
    kind_link: "Link",
    kind_other: "Other",
    csv_size_bytes: "Size (bytes)",
    csv_allocated_bytes: "On disk (bytes)",

    open_folder: "Open folder",
    copy_path: "Copy path",
    show_in_map: "Show in tile map",

    welcome: "Choose a folder or drive and click “Scan”.",
    scanning: "Scanning",
    files: "files",
    folders: "folders",
    errors: "errors",
    cancelled: "cancelled – numbers incomplete",
    seconds: "s",
    not_scanned: "Not scanned (cancelled)",
    skipped: "Not entered (other drive or excluded)",
    hardlink: "Further hard link of a file already counted",
    link: "Link – not followed",

    error_title: "Error",
    copy: "Copy",
    close: "Close",
    export_done: "Export saved",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        assert_eq!(Lang::from_code("de-DE"), Some(Lang::De));
        assert_eq!(Lang::from_code("de_AT.UTF-8"), Some(Lang::De));
        assert_eq!(Lang::from_code("EN-us"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr-FR"), None);
        assert_eq!(Lang::from_code(""), None);
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
    }
}
