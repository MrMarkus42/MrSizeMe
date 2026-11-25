//! "Show with MrSizeMe" in the right-click menu of the file manager.
//!
//! Only on the user's request, only for the current user and always
//! removable again:
//! * Windows: entries below `HKEY_CURRENT_USER\Software\Classes` for folders,
//!   drives and the background of a folder.
//! * macOS: a Quick Action in `~/Library/Services`.
//! * Linux: a Nautilus script and a Dolphin service menu.

use std::io;
use std::path::{Path, PathBuf};

use crate::external::{self, Outcome};

const MENU_DE: &str = "Mit MrSizeMe anzeigen";
const MENU_EN: &str = "Show with MrSizeMe";

fn menu_text() -> &'static str {
    if crate::i18n::lang() == crate::i18n::Lang::De { MENU_DE } else { MENU_EN }
}

/// True if the entry is there.
pub fn is_installed() -> bool {
    imp::is_installed()
}

/// Adds the entry for this program file.
pub fn install() -> io::Result<Outcome> {
    if external::is_blocked() {
        log::info!("outside action blocked (test mode): context menu");
        return Ok(Outcome::Blocked);
    }
    let exe = std::env::current_exe()?;
    imp::install(&exe, menu_text())?;
    log::info!("context menu entry added");
    Ok(Outcome::Done)
}

/// Removes the entry again.
pub fn uninstall() -> io::Result<Outcome> {
    if external::is_blocked() {
        log::info!("outside action blocked (test mode): context menu");
        return Ok(Outcome::Blocked);
    }
    imp::uninstall()?;
    log::info!("context menu entry removed");
    Ok(Outcome::Done)
}

/// The text in single quotes for a POSIX shell script ('it'\''s').
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A program path for the `Exec=` line of a .desktop file.
pub fn desktop_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The command for the Windows registry: program and folder in quotes.
pub fn windows_command(exe: &Path, placeholder: &str) -> String {
    format!("\"{}\" \"{placeholder}\"", exe.display())
}

#[cfg(unix)]
fn home() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))
}

/// Linux: the two files (Nautilus script, Dolphin menu) below `home`.
pub fn linux_files(home: &Path, exe: &Path, text: &str) -> Vec<(PathBuf, String)> {
    let exe = exe.to_string_lossy();
    let script = format!(
        "#!/bin/sh\n# Added by MrSizeMe; remove it there (Start > {text}).\nexec {} \"$1\"\n",
        sh_quote(&exe)
    );
    let menu = format!(
        "[Desktop Entry]\nType=Service\nMimeType=inode/directory;\nActions=mrsizeme\nX-KDE-Priority=TopLevel\n\n\
         [Desktop Action mrsizeme]\nName={text}\nIcon=drive-harddisk\nExec={} %f\n",
        desktop_quote(&exe)
    );
    vec![
        (home.join(".local/share/nautilus/scripts").join(text), script),
        (home.join(".local/share/kio/servicemenus/mrsizeme.desktop"), menu),
    ]
}

/// macOS: Info.plist and document.wflow of the Quick Action.
pub fn mac_workflow(exe: &Path, text: &str) -> (String, String) {
    let info = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict><key>default</key><string>{0}</string></dict>
			<key>NSMessage</key><string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>
			<key>NSSendFileTypes</key>
			<array><string>public.folder</string></array>
		</dict>
	</array>
</dict>
</plist>
"#,
        crate::report::escape(text)
    );
    // The path is passed as argument "$1"; the script itself is fixed text.
    let command = format!(
        "{} \"$1\" >/dev/null 2>&amp;1 &amp;",
        crate::report::escape(&sh_quote(&exe.to_string_lossy()))
    );
    let wflow = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key><string>523</string>
	<key>AMApplicationVersion</key><string>2.10</string>
	<key>AMDocumentVersion</key><string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Optional</key><true/>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>AMActionVersion</key><string>2.0.3</string>
				<key>AMApplication</key><array><string>Automator</string></array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key><dict/>
					<key>CheckedForUserDefaultShell</key><dict/>
					<key>inputMethod</key><dict/>
					<key>shell</key><dict/>
					<key>source</key><dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key><string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key><string>{command}</string>
					<key>CheckedForUserDefaultShell</key><true/>
					<key>inputMethod</key><integer>1</integer>
					<key>shell</key><string>/bin/sh</string>
					<key>source</key><string></string>
				</dict>
				<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key><string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key><false/>
				<key>CanShowWhenRun</key><true/>
				<key>Category</key><array><string>AMCategoryUtilities</string></array>
				<key>Class Name</key><string>RunShellScriptAction</string>
				<key>InputUUID</key><string>5C6F3E0A-1D7B-4B0E-9F1A-6A3B2C1D0E01</string>
				<key>Keywords</key><array><string>Shell</string></array>
				<key>OutputUUID</key><string>5C6F3E0A-1D7B-4B0E-9F1A-6A3B2C1D0E02</string>
				<key>UUID</key><string>5C6F3E0A-1D7B-4B0E-9F1A-6A3B2C1D0E03</string>
				<key>UnlocalizedApplications</key><array><string>Automator</string></array>
				<key>arguments</key><dict/>
				<key>isViewVisible</key><integer>1</integer>
				<key>location</key><string>309.000000:316.000000</string>
				<key>nibPath</key><string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key><integer>1</integer>
		</dict>
	</array>
	<key>connectors</key><dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key><dict/>
		<key>applicationPaths</key><array/>
		<key>inputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject.folder</string>
		<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>presentationMode</key><integer>15</integer>
		<key>processesInput</key><false/>
		<key>serviceApplicationBundleID</key><string>com.apple.finder</string>
		<key>serviceApplicationPath</key><string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject.folder</string>
		<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key><false/>
		<key>systemImageName</key><string>NSActionTemplate</string>
		<key>useAutomaticInputType</key><false/>
		<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#
    );
    (info, wflow)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;

    pub fn is_installed() -> bool {
        home().is_ok_and(|h| h.join(".local/share/kio/servicemenus/mrsizeme.desktop").exists())
    }

    pub fn install(exe: &Path, text: &str) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        for (path, content) in linux_files(&home()?, exe, text) {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, content)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }

    pub fn uninstall() -> io::Result<()> {
        let h = home()?;
        for text in [MENU_DE, MENU_EN] {
            for (path, _) in linux_files(&h, Path::new(""), text) {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    fn bundle() -> io::Result<PathBuf> {
        Ok(home()?.join("Library/Services/MrSizeMe.workflow"))
    }

    pub fn is_installed() -> bool {
        bundle().is_ok_and(|b| b.exists())
    }

    pub fn install(exe: &Path, text: &str) -> io::Result<()> {
        let dir = bundle()?.join("Contents");
        std::fs::create_dir_all(&dir)?;
        let (info, wflow) = mac_workflow(exe, text);
        std::fs::write(dir.join("Info.plist"), info)?;
        std::fs::write(dir.join("document.wflow"), wflow)?;
        Ok(())
    }

    pub fn uninstall() -> io::Result<()> {
        // Only the folder this program made, with its two known files.
        let b = bundle()?;
        for f in ["Contents/Info.plist", "Contents/document.wflow"] {
            match std::fs::remove_file(b.join(f)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        let _ = std::fs::remove_dir(b.join("Contents"));
        let _ = std::fs::remove_dir(&b);
        Ok(())
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW,
    };

    /// (key below HKCU, placeholder for the folder)
    const PLACES: [(&str, &str); 3] = [
        (r"Software\Classes\Directory\shell\MrSizeMe", "%1"),
        (r"Software\Classes\Drive\shell\MrSizeMe", "%1"),
        (r"Software\Classes\Directory\Background\shell\MrSizeMe", "%V"),
    ];

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn set(path: &str, name: Option<&str>, value: &str) -> io::Result<()> {
        let p = wide(path);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `p` ends with a zero; `key` receives the key.
        let r = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                p.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if r != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(r as i32));
        }
        let n = name.map(wide);
        let v = wide(value);
        // SAFETY: the value is a zero-terminated UTF-16 string of the given byte length.
        let r = unsafe {
            RegSetValueExW(
                key,
                n.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
                0,
                REG_SZ,
                v.as_ptr().cast(),
                (v.len() * 2) as u32,
            )
        };
        // SAFETY: `key` was opened above.
        unsafe { RegCloseKey(key) };
        if r != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(r as i32));
        }
        Ok(())
    }

    pub fn is_installed() -> bool {
        let p = wide(PLACES[0].0);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: as above.
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, p.as_ptr(), 0, KEY_READ, &mut key) };
        if r == ERROR_SUCCESS {
            // SAFETY: opened above.
            unsafe { RegCloseKey(key) };
        }
        r == ERROR_SUCCESS
    }

    pub fn install(exe: &Path, text: &str) -> io::Result<()> {
        for (key, placeholder) in PLACES {
            set(key, None, text)?;
            set(key, Some("Icon"), &format!("\"{}\",0", exe.display()))?;
            set(&format!(r"{key}\command"), None, &windows_command(exe, placeholder))?;
        }
        Ok(())
    }

    pub fn uninstall() -> io::Result<()> {
        for (key, _) in PLACES {
            let p = wide(key);
            // SAFETY: `p` ends with a zero; only our own key is removed.
            let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, p.as_ptr()) };
            if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
                return Err(io::Error::from_raw_os_error(r as i32));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("/opt/it's here/mrsizeme"), r"'/opt/it'\''s here/mrsizeme'");
        assert_eq!(desktop_quote(r#"/a "b" $c\d"#), r#""/a \"b\" \$c\\d""#);
        assert_eq!(
            windows_command(Path::new(r"C:\Program Files\MrSizeMe\mrsizeme.exe"), "%1"),
            r#""C:\Program Files\MrSizeMe\mrsizeme.exe" "%1""#
        );
    }

    #[test]
    fn files_contain_the_quoted_program() {
        let files = linux_files(Path::new("/home/x"), Path::new("/opt/my apps/mrsizeme"), MENU_DE);
        assert_eq!(files[0].0, Path::new("/home/x/.local/share/nautilus/scripts/Mit MrSizeMe anzeigen"));
        assert!(files[0].1.contains("exec '/opt/my apps/mrsizeme' \"$1\""));
        assert!(files[1].1.contains("Exec=\"/opt/my apps/mrsizeme\" %f"));
        let (info, wflow) =
            mac_workflow(Path::new("/Applications/Mr <Size>.app/Contents/MacOS/mrsizeme"), MENU_EN);
        assert!(info.contains("<string>Show with MrSizeMe</string>"));
        assert!(wflow.contains("&#39;/Applications/Mr &lt;Size&gt;.app/Contents/MacOS/mrsizeme&#39; \"$1\""));
        // Never changes anything in tests.
        assert_eq!(install().unwrap(), Outcome::Blocked);
        assert_eq!(uninstall().unwrap(), Outcome::Blocked);
    }
}
