# MrSizeMe

MrSizeMe shows which folders and files use the space on your disks – as a
folder tree with bars, as a tile map and by file type – and helps you clean
up safely. It runs on Windows, macOS and Linux and speaks German and English.

**Status:** in development, no release yet. The planned scope is described
(in German) in [docs/KONZEPT.md](docs/KONZEPT.md).

## What works today

* Fast scan with several threads; links and junctions are never followed,
  hard links are counted once.
* Size and space on disk (compressed, sparse and cloud files on Windows).
* Folder tree with share bars, sorting, keyboard navigation.
* Tile map with folder frames; double-click zooms into a folder.
* Ring chart; size, space on disk or number of files.
* File types, categories, largest and oldest files, age distribution.
* Search by name pattern, category, minimum size and age.
* CSV export (safe against formula injection in spreadsheets).
* German and English, light and dark mode.

## Building

```
cargo build --release --locked
```

Start with a folder: `mrsizeme /path/to/folder`. Summary on the command line:
`mrsizeme --scan /path/to/folder`.

## Privacy and safety

MrSizeMe reads only names, sizes and dates – never the content of your
files. Nothing is sent anywhere. See [SECURITY.md](SECURITY.md).

## Licence

MIT or Apache-2.0, at your choice.
