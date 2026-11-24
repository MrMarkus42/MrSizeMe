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
