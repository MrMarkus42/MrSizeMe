# Changelog

## Unreleased

### Added
- Scanner: parallel reading of folders, size and space on disk, hard links
  counted once, links and junctions never followed, skip list, cancel.
- Window: folder tree with share bars and sorting, tile map, file types,
  largest files, status bar with progress, copyable error messages.
- German and English; follows the system language.
- CSV export with protection against spreadsheet formulas.
- Log file without private data (home folder shown as "~").
- Program icon (tile map with magnifier) as PNG, .ico and .icns, with a
  simplified variant for small sizes; shown as window icon.
- Ring chart next to the tile map; both can show size, space on disk or
  number of files.
- Categories (videos, pictures, music, documents, archives, programs,
  games, disk images, development, caches, system, other); folders such as
  game libraries or caches decide the category of everything below them.
- Oldest files and age distribution.
- Search with name pattern (`*`, `?`), category, minimum size and age.
- Menu band with tabs (Home, View, Help) and the program's own icons;
  path line below it; movable windows for About, keyboard shortcuts and
  Full Disk Access; keyboard shortcuts (F5, Esc, Ctrl+O/F/E, Ctrl+Shift+C,
  Backspace).
- Scan with admin rights (Windows, Linux): a helper copy of the program
  scans with raised rights and hands the result back; the window itself
  never runs with raised rights. The helper only writes to a fresh temp
  folder or to its standard output.
- Windows: folders are listed with one handle per folder (faster, exact
  space on disk); with admin rights the backup right opens protected
  folders, and local NTFS drives are read through the master file table.
- macOS: check and step-by-step guide for Full Disk Access.
- Scans can be saved in a checked binary format (used by the helper).
- Clean-up help: built-in knowledge list of about 40 well-known big folders
  and files (Windows, macOS, Linux, development, games) with a traffic
  light (may go / clean up with a tool / do not touch) and the safe way to
  clean up; "Clean up" list with the possible savings of the whole scan.
- "What is this? May it go?" window from the right-click menu and the new
  "Clean up" tab of the menu band.
- Move to the recycle bin (Windows, macOS, Linux), always with a question
  first; system folders, user standard folders and red entries are
  protected. The tree is updated without a new scan.
- Ask an AI about a folder: Google AI mode or Google search in the browser,
  a local AI through Ollama, Gemini or Claude with your own key. The exact
  question is shown before anything is sent; it contains only the folder
  name, its size and the kind of system.
- Settings file; "Fully offline" switch; keys kept in the system key store.
- History: after each scan a short summary (names and sizes of the larger
  folders and files, no contents) is kept on this computer; up to 100 per
  folder, can be switched off and deleted.
- Comparison with an earlier scan that names the exact place where the
  space went ("Since 19.11., Downloads/urlaub.zip has grown by 600 MB"),
  a chart of the size over time and a forecast ("At this pace the disk
  will be full in about 2 months").
- Low space warning for the fixed disks (off, below 5, 10, 15 or 20 % free),
  checked in the background every ten minutes, with "Scan" and "Hide".
- Live update: after a scan the folder is watched and the numbers follow
  new, changed and deleted files without a new scan (read-only; on Linux
  up to 100,000 subfolders).
- New "History" tab in the menu band; date and time in local time.
- Duplicate files: files of equal size are compared by SHA-256 checksum
  (first 64 KiB, then the whole file); cloud files that are only online are
  never read. Each copy can go to the recycle bin.
- Cloud folders (OneDrive, iCloud Drive, Dropbox and other cloud services):
  space on this computer versus only online, and "Keep only online"
  (Windows: cloud file API; macOS: brctl / fileproviderctl).
- Programs view: space per program = program folder + data and caches
  (Windows uninstall list, macOS .app bundles, Linux Flatpak/Snap/menu).
- Move assistant: moves a folder to another disk; copies, checks every
  file, only then removes the original (recycle bin or delete) and can put
  a link at the old place (junction on Windows, symbolic link elsewhere).
- Cloud files that are only online are marked during the scan.
- Export as Excel workbook (.xlsx, written without extra libraries; names
  are always text, never formulas) and as a self-contained HTML report
  (no scripts; overview, categories, types, largest files, clean-up hints).
- Command line: `--scan FOLDER` with `--format text|json|csv|html|xlsx`,
  `--out`, `--depth`, `--files`, `--top`; `--help`. On Windows the output
  appears in the console the program was started from.
- "Show with MrSizeMe" in the right-click menu of folders, only on request
  and removable: Windows (registry below HKEY_CURRENT_USER), macOS (Quick
  Action), Linux (Nautilus script, Dolphin service menu).
- Updates: asked at the first start whether to look for updates at all; never,
  daily, weekly or monthly; manual check in the Help tab. Downloads only from
  the project's GitHub releases, checked with the minisign key built into the
  program; unsigned or wrongly signed files are refused. Installer, portable
  version (replaces its own program file), macOS disk image, Linux archive.
- Portable mode: a file `portable.txt` next to the program keeps settings,
  history and log in the folder `MrSizeMe-Daten` next to it.
- Windows program file with icon and version information.
- Packaging: Inno Setup installer (per user, or for all users after a
  question), portable ZIP, universal macOS app in a .dmg, Linux archive with
  menu entry; release workflow that builds everything into a DRAFT release
  with SHA-256 checksums; scripts to make the key, sign and verify.
- Website (German and English) in `website/`, published only by hand.
