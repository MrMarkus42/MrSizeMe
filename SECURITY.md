# Security

## Reporting a problem

Please report security problems privately through the "Report a
vulnerability" button of the GitHub repository (Security tab), not in a
public issue. You will get an answer within a few days.

## What MrSizeMe protects

* **Your files stay untouched.** A scan reads only names, sizes and dates
  (metadata). The content of a file is never read.
* **No hidden network access.** The program only goes online when you
  click “Send question” or “Open in browser” in the AI part. The exact text
  is shown before; it contains at most a folder name with up to two folders
  above it, its size and the kind of system – never a full path, your user
  name or file contents. “Fully offline” blocks all of it.
* **Your AI keys stay in the system key store** (Windows Credential
  Manager, macOS Keychain, on Linux a file readable only by you). They are
  never written to the settings, the log or the repository, and are only
  sent to the service they belong to.
* **Nothing is deleted for good.** Files and folders only go to the recycle
  bin, after a question. System folders, the standard user folders and
  everything marked red in the knowledge list are refused. An AI answer
  never triggers any action.
* **Links are not followed**, so a scan cannot loop or leave the chosen folder
  through a link or junction.
* **Safe export.** Names that start with `=`, `+`, `-` or `@` are written so
  that a spreadsheet does not run them as formulas.
* **Private log.** The log file contains no file contents, and your home
  folder is written as `~`.
* **No shell.** Paths are never passed through a shell; the file manager is
  opened with the system's own functions or one plain argument.
* **Admin rights only where needed.** The window never runs with raised
  rights. A scan with admin rights starts a second copy of the program in a
  helper mode that only scans. It writes its result only to standard output
  or to a new file `scan.bin` in a fresh temp folder made for it, and the
  window checks every number of that result before using it.
* **Raw disk data is checked.** The fast NTFS scan reads the master file
  table directly; every record is checked, broken records are skipped.
* **Tests stay inside.** One central switch blocks every outside action
  (opening programs or web pages) in all tests.
