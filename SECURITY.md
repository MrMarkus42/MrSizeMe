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
* **Tests stay inside.** One central switch blocks every outside action
  (opening programs or web pages) in all tests.
