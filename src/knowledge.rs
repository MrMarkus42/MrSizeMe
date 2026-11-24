//! Built-in knowledge about well-known big folders and files: what they
//! are, whether they may go, and the safe way to clean them up.
//!
//! Works offline. The traffic light: green = may go, yellow = clean up
//! with the right tool, red = do not touch.

use crate::i18n::{Lang, lang};
use crate::tree::{Kind, NO_PARENT, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Light {
    Green,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Any,
    Windows,
    Mac,
    Linux,
}

impl Os {
    fn matches_current(self) -> bool {
        match self {
            Os::Any => true,
            Os::Windows => cfg!(windows),
            Os::Mac => cfg!(target_os = "macos"),
            Os::Linux => cfg!(all(unix, not(target_os = "macos"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Dir,
    File,
    Any,
}

/// Title, what it is, how to clean it up.
pub type Text = (&'static str, &'static str, &'static str);

pub struct Entry {
    pub id: &'static str,
    pub os: Os,
    /// Names in lower case (exact match).
    pub names: &'static [&'static str],
    /// Name ending in lower case (e.g. ".photoslibrary"), alternative to `names`.
    pub suffix: Option<&'static str>,
    /// The direct parent must have this name (lower case).
    pub parent: Option<&'static str>,
    /// A neighbour with this name must exist (e.g. "cargo.toml" next to "target").
    pub sibling: Option<&'static str>,
    pub want: Want,
    pub light: Light,
    /// MrSizeMe may move the whole folder or file to the recycle bin itself.
    pub can_trash: bool,
    pub de: Text,
    pub en: Text,
}

impl Entry {
    pub fn text(&self) -> Text {
        match lang() {
            Lang::De => self.de,
            Lang::En => self.en,
        }
    }
}

macro_rules! entry {
    ($id:expr, $os:ident, [$($n:expr),*], $suffix:expr, $parent:expr, $sibling:expr, $want:ident, $light:ident, $trash:expr,
     de: ($dt:expr, $dw:expr, $dh:expr), en: ($et:expr, $ew:expr, $eh:expr)) => {
        Entry {
            id: $id,
            os: Os::$os,
            names: &[$($n),*],
            suffix: $suffix,
            parent: $parent,
            sibling: $sibling,
            want: Want::$want,
            light: Light::$light,
            can_trash: $trash,
            de: ($dt, $dw, $dh),
            en: ($et, $ew, $eh),
        }
    };
}

pub static ENTRIES: &[Entry] = &[
    // ---------------- Windows ----------------
    entry!("winsxs", Windows, ["winsxs"], None, Some("windows"), None, Dir, Yellow, false,
        de: ("Windows-Komponentenspeicher (WinSxS)",
             "Systemdateien von Windows und ältere Versionen nach Updates. Vieles davon sind Hardlinks auf Dateien im System – der Ordner wirkt größer, als er wirklich ist.",
             "Nie von Hand löschen – Windows startet sonst evtl. nicht mehr. Aufräumen: Datenträgerbereinigung → „Systemdateien bereinigen“ → „Windows Update-Bereinigung“. Oder in einer Eingabeaufforderung als Administrator: DISM /Online /Cleanup-Image /StartComponentCleanup"),
        en: ("Windows component store (WinSxS)",
             "Windows system files and older versions kept after updates. Much of it is hard links to files elsewhere – the folder looks bigger than it really is.",
             "Never delete it by hand – Windows may no longer start. Clean up: Disk Cleanup → “Clean up system files” → “Windows Update Cleanup”. Or in an admin command prompt: DISM /Online /Cleanup-Image /StartComponentCleanup")),
    entry!("windows-old", Windows, ["windows.old"], None, None, None, Dir, Yellow, false,
        de: ("Vorherige Windows-Installation",
             "Die alte Windows-Version nach einem großen Upgrade. Damit kann man ca. 10 Tage lang zurückwechseln.",
             "Einstellungen → System → Speicher → Temporäre Dateien → „Vorherige Windows-Installation(en)“ auswählen und entfernen. Danach ist kein Zurückwechseln mehr möglich."),
        en: ("Previous Windows installation",
             "The old Windows version after a major upgrade. It allows going back for about 10 days.",
             "Settings → System → Storage → Temporary files → select “Previous Windows installation(s)” and remove. Going back is no longer possible afterwards.")),
    entry!("recycle-bin", Windows, ["$recycle.bin"], None, None, None, Dir, Green, false,
        de: ("Papierkorb", "Gelöschte Dateien, die noch wiederhergestellt werden können.",
             "Papierkorb auf dem Desktop mit der rechten Maustaste anklicken → „Papierkorb leeren“. Vorher prüfen, ob nichts Wichtiges darin liegt."),
        en: ("Recycle bin", "Deleted files that can still be restored.",
             "Right-click the recycle bin on the desktop → “Empty Recycle Bin”. Check first that nothing important is in it.")),
    entry!("hiberfil", Windows, ["hiberfil.sys"], None, None, None, File, Yellow, false,
        de: ("Ruhezustandsdatei", "Speichert den Arbeitsspeicher beim Ruhezustand und für den Schnellstart. Etwa so groß wie ein Teil des Arbeitsspeichers.",
             "Nicht löschen. Wer Ruhezustand und Schnellstart nicht braucht: Eingabeaufforderung als Administrator → powercfg /hibernate off. Windows entfernt die Datei dann selbst."),
        en: ("Hibernation file", "Holds the memory contents for hibernation and fast startup. Roughly the size of part of the RAM.",
             "Do not delete it. If you need neither hibernation nor fast startup: admin command prompt → powercfg /hibernate off. Windows then removes the file itself.")),
    entry!("pagefile", Windows, ["pagefile.sys", "swapfile.sys"], None, None, None, File, Red, false,
        de: ("Auslagerungsdatei", "Wird von Windows als zusätzlicher Arbeitsspeicher benutzt.",
             "Nicht anfassen. Windows verwaltet die Größe selbst. Nur wer genau weiß, was er tut: Systemeigenschaften → Erweitert → Leistung → Virtueller Arbeitsspeicher."),
        en: ("Page file", "Used by Windows as extra memory.",
             "Do not touch it. Windows manages its size. Only if you know what you are doing: System Properties → Advanced → Performance → Virtual memory.")),
    entry!("softwaredistribution", Windows, ["softwaredistribution"], None, Some("windows"), None, Dir, Yellow, false,
        de: ("Windows-Update-Dateien", "Heruntergeladene Updates und ihr Verlauf.",
             "Datenträgerbereinigung → „Systemdateien bereinigen“ → „Windows Update-Bereinigung“ und „Temporäre Dateien“. Nicht von Hand löschen, während Updates laufen."),
        en: ("Windows Update files", "Downloaded updates and their history.",
             "Disk Cleanup → “Clean up system files” → “Windows Update Cleanup” and “Temporary files”. Do not delete by hand while updates are running.")),
    entry!("win-temp", Windows, ["temp"], None, None, None, Dir, Green, false,
        de: ("Temporäre Dateien", "Zwischendateien von Windows und Programmen.",
             "Einstellungen → System → Speicher → Temporäre Dateien → „Temporäre Dateien“ entfernen. Dateien, die gerade benutzt werden, bleiben einfach liegen."),
        en: ("Temporary files", "Scratch files of Windows and programs.",
             "Settings → System → Storage → Temporary files → remove “Temporary files”. Files in use are simply left alone.")),
    entry!("installer", Windows, ["installer"], None, Some("windows"), None, Dir, Red, false,
        de: ("Installations-Zwischenspeicher", "Wird zum Reparieren und Deinstallieren von Programmen gebraucht.",
             "Nicht löschen – sonst lassen sich Programme nicht mehr reparieren oder entfernen. Platz gewinnen: nicht mehr benutzte Programme deinstallieren."),
        en: ("Installer cache", "Needed to repair and uninstall programs.",
             "Do not delete – programs could no longer be repaired or removed. To gain space, uninstall programs you no longer use.")),
    entry!("system-volume-information", Windows, ["system volume information"], None, None, None, Dir, Red, false,
        de: ("Wiederherstellungspunkte", "Systemwiederherstellung und Schattenkopien.",
             "Nicht von Hand löschen. Platz begrenzen: Systemsteuerung → System → Computerschutz → Konfigurieren → maximale Belegung verringern oder alte Punkte löschen."),
        en: ("Restore points", "System restore and shadow copies.",
             "Do not delete by hand. Limit the space: Control Panel → System → System protection → Configure → lower the maximum usage or delete old points.")),
    entry!("driverstore", Windows, ["filerepository"], None, Some("driverstore"), None, Dir, Red, false,
        de: ("Treiberspeicher", "Alle installierten Gerätetreiber.",
             "Nicht von Hand löschen. Alte Treiberpakete entfernt die Datenträgerbereinigung (Systemdateien → „Gerätetreiberpakete“)."),
        en: ("Driver store", "All installed device drivers.",
             "Do not delete by hand. Old driver packages are removed by Disk Cleanup (system files → “Device driver packages”).")),
    entry!("delivery-optimization", Windows, ["deliveryoptimization"], None, None, None, Dir, Yellow, false,
        de: ("Übermittlungsoptimierung", "Update-Teile, die Windows mit anderen PCs teilt.",
             "Datenträgerbereinigung → „Systemdateien bereinigen“ → „Übermittlungsoptimierungsdateien“."),
        en: ("Delivery Optimization", "Update pieces Windows shares with other PCs.",
             "Disk Cleanup → “Clean up system files” → “Delivery Optimization Files”.")),
    entry!("crash-dumps", Windows, ["crashdumps", "minidump", "memory.dmp", "livekernelreports"], None, None, None, Any, Green, true,
        de: ("Absturzberichte", "Speicherabbilder nach Programm- oder Systemabstürzen.",
             "Können in den Papierkorb, wenn kein Fehler mehr untersucht wird."),
        en: ("Crash dumps", "Memory dumps written after program or system crashes.",
             "Can go to the recycle bin unless an error is still being investigated.")),
    entry!("inetcache", Windows, ["inetcache"], None, None, None, Dir, Green, false,
        de: ("Internet-Zwischenspeicher", "Zwischengespeicherte Webseiten.",
             "Datenträgerbereinigung → „Temporäre Internetdateien“, oder im Browser den Cache leeren."),
        en: ("Internet cache", "Cached web pages.",
             "Disk Cleanup → “Temporary Internet Files”, or clear the cache in the browser.")),
    entry!("windows-bt", Windows, ["$windows.~bt", "$windows.~ws"], None, None, None, Dir, Yellow, false,
        de: ("Reste eines Windows-Upgrades", "Installationsdateien eines (abgeschlossenen oder abgebrochenen) Upgrades.",
             "Datenträgerbereinigung → „Systemdateien bereinigen“ → „Temporäre Windows-Installationsdateien“."),
        en: ("Windows upgrade leftovers", "Setup files of a finished or cancelled upgrade.",
             "Disk Cleanup → “Clean up system files” → “Temporary Windows installation files”.")),
    entry!("package-cache", Windows, ["package cache"], None, None, None, Dir, Red, false,
        de: ("Installationspakete von Programmen", "Wird von Programmen (z. B. Visual Studio) zum Reparieren und Aktualisieren gebraucht.",
             "Nicht löschen. Platz gewinnen: nicht benutzte Programme deinstallieren."),
        en: ("Program setup packages", "Needed by programs (e.g. Visual Studio) to repair and update.",
             "Do not delete. To gain space, uninstall programs you no longer use.")),
    entry!("onedrive", Windows, ["onedrive"], None, None, None, Dir, Yellow, false,
        de: ("OneDrive", "Mit der Cloud abgeglichene Dateien. Ein Teil davon liegt auch lokal auf der Platte.",
             "Nicht löschen (das löscht auch in der Cloud!). Platz sparen: Ordner mit der rechten Maustaste → „Speicherplatz freigeben“ – die Dateien bleiben online erreichbar."),
        en: ("OneDrive", "Files synced with the cloud. Some of them are also stored locally.",
             "Do not delete (that deletes in the cloud too!). To save space: right-click a folder → “Free up space” – the files stay available online.")),
    // ---------------- macOS ----------------
    entry!("mac-caches", Mac, ["caches"], None, Some("library"), None, Dir, Green, false,
        de: ("Zwischenspeicher (Caches)", "Daten, die Programme zur Beschleunigung ablegen. Sie werden bei Bedarf neu angelegt.",
             "Den Ordner selbst stehen lassen. Programme beenden und große Unterordner einzelner Programme in den Papierkorb legen."),
        en: ("Caches", "Data programs keep to work faster. It is created again when needed.",
             "Keep the folder itself. Quit the programs and move large subfolders of single programs to the trash.")),
    entry!("deriveddata", Mac, ["deriveddata"], None, Some("xcode"), None, Dir, Green, true,
        de: ("Xcode-Zwischendaten", "Build-Ergebnisse und Indizes von Xcode.",
             "Kann in den Papierkorb; Xcode baut alles beim nächsten Mal neu."),
        en: ("Xcode derived data", "Build results and indexes of Xcode.",
             "Can go to the trash; Xcode rebuilds everything next time.")),
    entry!("xcode-archives", Mac, ["archives"], None, Some("xcode"), None, Dir, Yellow, false,
        de: ("Xcode-Archive", "Archivierte App-Versionen (für Veröffentlichungen und Absturzanalysen).",
             "In Xcode → Window → Organizer alte Archive gezielt löschen."),
        en: ("Xcode archives", "Archived app builds (for releases and crash analysis).",
             "In Xcode → Window → Organizer, delete old archives selectively.")),
    entry!("coresimulator", Mac, ["coresimulator"], None, None, None, Dir, Yellow, false,
        de: ("iOS-Simulatoren", "Simulierte Geräte von Xcode.",
             "Im Terminal: xcrun simctl delete unavailable – entfernt Simulatoren alter Systemversionen."),
        en: ("iOS simulators", "Simulated devices of Xcode.",
             "In Terminal: xcrun simctl delete unavailable – removes simulators of old system versions.")),
    entry!("ios-backups", Mac, ["backup"], None, Some("mobilesync"), None, Dir, Yellow, false,
        de: ("iPhone- und iPad-Backups", "Lokale Sicherungen Ihrer Geräte.",
             "Finder → Gerät auswählen → „Backups verwalten“ → alte Backups löschen. Das neueste Backup behalten!"),
        en: ("iPhone and iPad backups", "Local backups of your devices.",
             "Finder → select the device → “Manage Backups” → delete old backups. Keep the newest one!")),
    entry!("mac-trash", Mac, [".trash"], None, None, None, Dir, Green, false,
        de: ("Papierkorb", "Gelöschte Dateien, die noch wiederhergestellt werden können.",
             "Papierkorb im Dock → „Papierkorb entleeren“."),
        en: ("Trash", "Deleted files that can still be restored.",
             "Trash in the Dock → “Empty Trash”.")),
    entry!("mac-vm", Mac, ["vm"], None, Some("var"), None, Dir, Red, false,
        de: ("Auslagerung und Ruhezustand", "Auslagerungsdateien und das Speicherabbild für den Ruhezustand.",
             "Nicht anfassen – macOS verwaltet diesen Ordner selbst."),
        en: ("Swap and sleep image", "Swap files and the memory image for sleep.",
             "Do not touch – macOS manages this folder itself.")),
    entry!("docker-raw", Any, ["docker.raw", "docker_data.vhdx", "ext4.vhdx"], None, None, None, File, Yellow, false,
        de: ("Docker-Speicher", "Virtuelle Platte mit allen Docker-Images und -Containern.",
             "Nicht löschen. Aufräumen: docker system prune (entfernt Ungenutztes); in Docker Desktop unter Troubleshoot auch „Clean / Purge data“."),
        en: ("Docker storage", "Virtual disk with all Docker images and containers.",
             "Do not delete. Clean up: docker system prune (removes unused data); Docker Desktop also offers “Clean / Purge data” under Troubleshoot.")),
    entry!("photos-library", Mac, [], Some(".photoslibrary"), None, None, Dir, Red, false,
        de: ("Fotos-Mediathek", "Alle Bilder und Videos der App „Fotos“.",
             "Nie im Finder verändern. Platz sparen: Fotos → Einstellungen → iCloud → „Mac-Speicher optimieren“, oder in Fotos aufräumen."),
        en: ("Photos library", "All pictures and videos of the Photos app.",
             "Never change it in the Finder. To save space: Photos → Settings → iCloud → “Optimize Mac Storage”, or tidy up inside Photos.")),
    // ---------------- Linux ----------------
    entry!("linux-cache", Linux, [".cache"], None, None, None, Dir, Green, false,
        de: ("Zwischenspeicher (~/.cache)", "Daten, die Programme zur Beschleunigung ablegen.",
             "Den Ordner selbst stehen lassen. Programme beenden und große Unterordner in den Papierkorb legen; sie werden bei Bedarf neu angelegt."),
        en: ("Cache (~/.cache)", "Data programs keep to work faster.",
             "Keep the folder itself. Quit the programs and move large subfolders to the trash; they are created again when needed.")),
    entry!("apt-archives", Linux, ["archives"], None, Some("apt"), None, Dir, Green, false,
        de: ("Heruntergeladene Pakete", "Installationspakete, die schon installiert sind.",
             "Im Terminal: sudo apt clean"),
        en: ("Downloaded packages", "Package files that are already installed.",
             "In a terminal: sudo apt clean")),
    entry!("journal", Linux, ["journal"], None, Some("log"), None, Dir, Yellow, false,
        de: ("Systemprotokolle", "Das Protokoll von systemd.",
             "Nicht von Hand löschen. Im Terminal verkleinern: sudo journalctl --vacuum-size=200M"),
        en: ("System logs", "The systemd journal.",
             "Do not delete by hand. Shrink it in a terminal: sudo journalctl --vacuum-size=200M")),
    entry!("linux-trash", Linux, ["trash"], None, Some("share"), None, Dir, Green, false,
        de: ("Papierkorb", "Gelöschte Dateien, die noch wiederhergestellt werden können.",
             "Im Dateimanager den Papierkorb leeren."),
        en: ("Trash", "Deleted files that can still be restored.",
             "Empty the trash in the file manager.")),
    entry!("snaps", Linux, ["snaps"], None, Some("snapd"), None, Dir, Yellow, false,
        de: ("Snap-Pakete", "Installierte Snap-Programme, oft mit mehreren alten Versionen.",
             "Weniger alte Versionen behalten: sudo snap set system refresh.retain=2"),
        en: ("Snap packages", "Installed snap programs, often with several old versions.",
             "Keep fewer old versions: sudo snap set system refresh.retain=2")),
    entry!("flatpak", Linux, ["flatpak"], None, None, None, Dir, Yellow, false,
        de: ("Flatpak-Programme", "Installierte Flatpak-Programme und Laufzeitumgebungen.",
             "Ungenutzte Laufzeitumgebungen entfernen: flatpak uninstall --unused"),
        en: ("Flatpak programs", "Installed Flatpak programs and runtimes.",
             "Remove unused runtimes: flatpak uninstall --unused")),
    // ---------------- Development and games (all systems) ----------------
    entry!("node-modules", Any, ["node_modules"], None, None, None, Dir, Green, true,
        de: ("JavaScript-Pakete (node_modules)", "Heruntergeladene Pakete eines JavaScript-Projekts.",
             "Kann in den Papierkorb; „npm install“ im Projekt holt alles wieder."),
        en: ("JavaScript packages (node_modules)", "Downloaded packages of a JavaScript project.",
             "Can go to the trash; “npm install” in the project fetches everything again.")),
    entry!("rust-target", Any, ["target"], None, None, Some("cargo.toml"), Dir, Green, true,
        de: ("Rust-Build-Ordner (target)", "Übersetzte Zwischen- und Endergebnisse eines Rust-Projekts.",
             "Kann in den Papierkorb (oder „cargo clean“); der nächste Build legt alles neu an."),
        en: ("Rust build folder (target)", "Compiled intermediate and final results of a Rust project.",
             "Can go to the trash (or “cargo clean”); the next build creates everything again.")),
    entry!("pycache", Any, ["__pycache__"], None, None, None, Dir, Green, true,
        de: ("Python-Zwischendateien", "Vorübersetzte Python-Dateien.",
             "Kann in den Papierkorb; Python legt sie bei Bedarf neu an."),
        en: ("Python cache", "Precompiled Python files.",
             "Can go to the trash; Python creates them again when needed.")),
    entry!("venv", Any, [".venv", "venv"], None, None, None, Dir, Yellow, true,
        de: ("Python-Umgebung", "Installierte Pakete eines Python-Projekts.",
             "Kann weg, wenn das Projekt eine requirements.txt o. Ä. hat – dann neu anlegen mit „python -m venv .venv“ und „pip install -r requirements.txt“."),
        en: ("Python environment", "Installed packages of a Python project.",
             "Can go if the project has a requirements.txt or similar – recreate with “python -m venv .venv” and “pip install -r requirements.txt”.")),
    entry!("gradle", Any, [".gradle"], None, None, None, Dir, Yellow, false,
        de: ("Gradle-Zwischenspeicher", "Heruntergeladene Bibliotheken und Build-Daten von Gradle (Java/Android).",
             "Im Benutzerordner: „caches“ darin kann weg, Gradle lädt bei Bedarf neu. Bei laufender Entwicklung erst Android Studio/IDE schließen."),
        en: ("Gradle cache", "Downloaded libraries and build data of Gradle (Java/Android).",
             "In the home folder: the “caches” folder inside can go; Gradle downloads again when needed. Close Android Studio/IDE first.")),
    entry!("maven", Any, ["repository"], None, Some(".m2"), None, Dir, Yellow, true,
        de: ("Maven-Bibliotheken", "Heruntergeladene Java-Bibliotheken.",
             "Kann weg; Maven lädt benötigte Bibliotheken beim nächsten Build neu (dauert dann länger)."),
        en: ("Maven libraries", "Downloaded Java libraries.",
             "Can go; Maven downloads the needed libraries again on the next build (which then takes longer).")),
    entry!("npm-cache", Any, [".npm"], None, None, None, Dir, Green, false,
        de: ("npm-Zwischenspeicher", "Zwischengespeicherte JavaScript-Pakete.",
             "Im Terminal: npm cache clean --force"),
        en: ("npm cache", "Cached JavaScript packages.",
             "In a terminal: npm cache clean --force")),
    entry!("cargo-registry", Any, ["registry"], None, Some(".cargo"), None, Dir, Yellow, false,
        de: ("Rust-Paketspeicher", "Heruntergeladene Rust-Pakete und ihre Quellen.",
             "Kann geleert werden; Cargo lädt benötigte Pakete neu. Vorher alle Builds beenden."),
        en: ("Rust package store", "Downloaded Rust packages and their sources.",
             "Can be emptied; Cargo downloads the needed packages again. Stop all builds first.")),
    entry!("shadercache", Any, ["shadercache"], None, None, None, Dir, Green, true,
        de: ("Shader-Zwischenspeicher (Spiele)", "Vorübersetzte Grafikprogramme von Spielen.",
             "Kann in den Papierkorb; Spiele legen ihn neu an (der erste Start dauert dann etwas länger)."),
        en: ("Shader cache (games)", "Precompiled graphics programs of games.",
             "Can go to the trash; games build it again (the first start then takes a bit longer).")),
    entry!("downloads", Any, ["downloads"], None, None, None, Dir, Yellow, false,
        de: ("Download-Ordner", "Heruntergeladene Dateien – oft alte Installationsprogramme, ZIP-Archive und Videos.",
             "Den Ordner selbst behalten. Inhalt durchsehen (z. B. mit „Größte Dateien“) und Unnötiges in den Papierkorb legen."),
        en: ("Downloads folder", "Downloaded files – often old installers, ZIP archives and videos.",
             "Keep the folder itself. Look through the content (e.g. with “Largest files”) and move what you do not need to the trash.")),
];

/// The entry that describes node `id`, if any (only entries for this system).
pub fn lookup(tree: &Tree, id: NodeId) -> Option<&'static Entry> {
    let n = tree.node(id);
    let name = if n.parent == NO_PARENT {
        tree.root_path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
    } else {
        n.name_lossy().to_lowercase()
    };
    let parent = if n.parent == NO_PARENT {
        tree.root_path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_lowercase())
    } else if tree.node(n.parent).parent == NO_PARENT {
        tree.root_path.file_name().map(|s| s.to_string_lossy().to_lowercase())
    } else {
        Some(tree.node(n.parent).name_lossy().to_lowercase())
    };
    ENTRIES.iter().find(|e| {
        e.os.matches_current()
            && match e.want {
                Want::Dir => n.kind == Kind::Dir,
                Want::File => n.kind == Kind::File,
                Want::Any => true,
            }
            && (e.names.contains(&name.as_str()) || e.suffix.is_some_and(|s| name.ends_with(s)))
            && e.parent.is_none_or(|p| parent.as_deref() == Some(p))
            && e.sibling.is_none_or(|s| {
                n.parent != NO_PARENT
                    && tree.children(n.parent).iter().any(|&c| tree.node(c).name_lossy().to_lowercase() == s)
            })
    })
}

/// A cleanup suggestion: a node the knowledge list knows, big enough to matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub id: NodeId,
    pub entry: &'static str,
    pub light: Light,
    pub size: u64,
}

/// Suggestions below `start` for entries that are green or yellow and at
/// least `min_size` bytes; largest first. Below a suggested folder nothing
/// else is suggested (it would be counted twice).
pub fn suggestions(tree: &Tree, start: NodeId, min_size: u64) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        let n = tree.node(id);
        if n.size < min_size {
            continue;
        }
        if let Some(e) = lookup(tree, id)
            && e.light != Light::Red
        {
            out.push(Suggestion { id, entry: e.id, light: e.light, size: n.size });
            continue;
        }
        if n.is_dir() {
            stack.extend_from_slice(tree.children(id));
        }
    }
    out.sort_by(|a, b| a.light.cmp(&b.light).then(b.size.cmp(&a.size)));
    out
}

pub fn by_id(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Node, ROOT};
    use std::collections::HashSet;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn entries_are_well_formed() {
        let mut ids = HashSet::new();
        for e in ENTRIES {
            assert!(ids.insert(e.id), "duplicate id {}", e.id);
            assert!(!e.names.is_empty() || e.suffix.is_some(), "{} matches nothing", e.id);
            for n in e.names {
                assert_eq!(*n, n.to_lowercase(), "{}: names must be lower case", e.id);
            }
            for t in [e.de, e.en] {
                assert!(!t.0.is_empty() && !t.1.is_empty() && !t.2.is_empty(), "{}: empty text", e.id);
            }
            // Something red must never be offered for the recycle bin.
            assert!(!(e.light == Light::Red && e.can_trash), "{}", e.id);
        }
    }

    fn dir(name: &str) -> Node {
        Node::new(OsStr::new(name).into(), NO_PARENT, Kind::Dir)
    }

    fn file(name: &str, size: u64) -> Node {
        let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
        n.size = size;
        n
    }

    #[test]
    fn development_folders_are_found_and_suggested() {
        let mut t = Tree::new(PathBuf::from("/projekte"), Kind::Dir);
        let first = t.add_children(ROOT, [dir("web"), dir("rust"), dir("target")]);
        let web = t.add_children(first, [dir("node_modules"), file("package.json", 1)]);
        t.add_children(web, [file("big.js", 300 << 20)]);
        let rust = t.add_children(first + 1, [dir("target"), file("Cargo.toml", 1)]);
        t.add_children(rust, [file("app", 200 << 20)]);
        // A "target" folder without Cargo.toml next to it is not a Rust build folder.
        t.add_children(first + 2, [file("x", 500 << 20)]);
        t.sum_up();

        assert_eq!(lookup(&t, web).map(|e| e.id), Some("node-modules"));
        assert_eq!(lookup(&t, rust).map(|e| e.id), Some("rust-target"));
        assert!(lookup(&t, first + 2).is_none());

        let s = suggestions(&t, ROOT, 100 << 20);
        let ids: Vec<_> = s.iter().map(|s| s.entry).collect();
        assert_eq!(ids, ["node-modules", "rust-target"]);
        assert!(suggestions(&t, ROOT, 1 << 40).is_empty());
    }

    #[test]
    fn parent_rule_and_root_names() {
        let mut t = Tree::new(PathBuf::from("/home/a/.m2"), Kind::Dir);
        t.add_children(ROOT, [dir("repository")]);
        t.sum_up();
        // The parent of a top-level node is the last part of the root path.
        assert_eq!(lookup(&t, 1).map(|e| e.id), Some("maven"));
        let t2 = Tree::new(PathBuf::from("/x/node_modules"), Kind::Dir);
        assert_eq!(lookup(&t2, ROOT).map(|e| e.id), Some("node-modules"));
    }
}
