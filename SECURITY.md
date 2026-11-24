# Security

## Reporting a problem

Please report security problems privately through the "Report a
vulnerability" button of the GitHub repository (Security tab), not in a
public issue. You will get an answer within a few days.

## What MrSizeMe protects

* **Your files stay untouched.** A scan reads only names, sizes and dates
  (metadata). The content of a file is never read.
* **No hidden network access.** The program sends nothing anywhere. Planned
  online functions (update check, asking an AI about a folder) only run after
  you agree, and send at most a folder name, its size and the operating
  system – never a full path or your user name.
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
