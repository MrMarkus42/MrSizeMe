//! Kinds of content (videos, pictures, games, caches …).
//!
//! A file gets the category of the nearest folder above it that marks a
//! kind of content (e.g. a game library, a cache, a developer folder);
//! otherwise its extension decides.

use std::path::Component;

use crate::stats::extension;
use crate::tree::{Kind, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Video,
    Pictures,
    Music,
    Documents,
    Archives,
    Programs,
    Games,
    DiskImages,
    Developer,
    Caches,
    System,
    Other,
}

impl Category {
    pub const ALL: [Category; 12] = [
        Category::Video,
        Category::Pictures,
        Category::Music,
        Category::Documents,
        Category::Archives,
        Category::Programs,
        Category::Games,
        Category::DiskImages,
        Category::Developer,
        Category::Caches,
        Category::System,
        Category::Other,
    ];

    /// Fixed colour (sRGB) for charts and legends.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Category::Video => [239, 68, 68],
            Category::Pictures => [236, 72, 153],
            Category::Music => [168, 85, 247],
            Category::Documents => [59, 130, 246],
            Category::Archives => [234, 179, 8],
            Category::Programs => [20, 184, 166],
            Category::Games => [34, 197, 94],
            Category::DiskImages => [249, 115, 22],
            Category::Developer => [99, 102, 241],
            Category::Caches => [148, 163, 184],
            Category::System => [100, 116, 139],
            Category::Other => [163, 163, 163],
        }
    }

    pub fn by_extension(ext: &str) -> Category {
        match ext {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "ts"
            | "m2ts" | "mts" | "3gp" | "vob" => Category::Video,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif"
            | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf" | "psd" | "svg"
            | "ico" => Category::Pictures,
            "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" | "aif" | "alac"
            | "mid" | "midi" => Category::Music,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "txt"
            | "rtf" | "md" | "csv" | "epub" | "pages" | "numbers" | "key" => Category::Documents,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "cab" | "lz4" => {
                Category::Archives
            }
            "exe" | "dll" | "msi" | "msix" | "appx" | "sys" | "so" | "dylib" | "dmg" | "pkg" | "deb"
            | "rpm" | "appimage" | "jar" | "ocx" | "drv" | "mui" => Category::Programs,
            "iso" | "img" | "vmdk" | "vdi" | "vhd" | "vhdx" | "qcow2" | "vmem" | "vmsn" | "hdd" | "wim"
            | "esd" | "sparseimage" | "sparsebundle" => Category::DiskImages,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "py" | "js" | "go" | "rb" | "php"
            | "kt" | "swift" | "o" | "obj" | "a" | "lib" | "pdb" | "rlib" | "rmeta" | "class" | "pyc"
            | "whl" | "ilk" | "pch" => Category::Developer,
            _ => Category::Other,
        }
    }

    /// Category of everything below a folder with this name, if it marks one.
    pub fn of_folder(name: &str) -> Option<Category> {
        let n = name.to_lowercase();
        Some(match n.as_str() {
            "steamapps"
            | "steamlibrary"
            | "epic games"
            | "gog games"
            | "gog galaxy"
            | "origin games"
            | "ea games"
            | "ubisoft game launcher"
            | "battle.net"
            | "xboxgames"
            | "riot games" => Category::Games,
            "cache"
            | "caches"
            | ".cache"
            | "cache2"
            | "code cache"
            | "gpucache"
            | "shadercache"
            | "inetcache"
            | "temp"
            | "tmp"
            | "__pycache__"
            | ".npm"
            | "deliveryoptimization" => Category::Caches,
            "node_modules" | ".git" | ".venv" | "venv" | ".m2" | ".gradle" | ".rustup" | ".cargo"
            | "deriveddata" | ".nuget" | ".android" => Category::Developer,
            "winsxs"
            | "system32"
            | "syswow64"
            | "windows.old"
            | "$recycle.bin"
            | ".trash"
            | ".trashes"
            | "installer"
            | "softwaredistribution" => Category::System,
            "virtual machines" | "virtualbox vms" | "parallels" | ".docker" => Category::DiskImages,
            _ => return None,
        })
    }

    /// Category of single well-known files.
    pub fn of_file(name: &str) -> Option<Category> {
        let n = name.to_lowercase();
        matches!(n.as_str(), "pagefile.sys" | "hiberfil.sys" | "swapfile.sys" | "swapfile" | "sleepimage")
            .then_some(Category::System)
    }
}

/// The category given by the folders above `start` (including the root path).
fn context_of(tree: &Tree, start: NodeId) -> Option<Category> {
    let mut ctx = None;
    for c in tree.root_path.components() {
        if let Component::Normal(name) = c
            && let Some(cat) = Category::of_folder(&name.to_string_lossy())
        {
            ctx = Some(cat);
        }
    }
    for a in tree.ancestors(start).into_iter().skip(1) {
        if let Some(cat) = Category::of_folder(&tree.node(a).name_lossy()) {
            ctx = Some(cat);
        }
    }
    ctx
}

/// Calls `visit(file, category)` for every file below `start`.
pub fn walk_files(tree: &Tree, start: NodeId, mut visit: impl FnMut(NodeId, Category)) {
    let mut stack = vec![(start, context_of(tree, start))];
    while let Some((id, ctx)) = stack.pop() {
        let n = tree.node(id);
        match n.kind {
            Kind::Dir => {
                let here = if id == start { ctx } else { Category::of_folder(&n.name_lossy()).or(ctx) };
                for &c in tree.children(id) {
                    stack.push((c, here));
                }
            }
            Kind::File => {
                let name = n.name_lossy();
                let cat = Category::of_file(&name)
                    .or(ctx)
                    .unwrap_or_else(|| Category::by_extension(&extension(&name)));
                visit(id, cat);
            }
            Kind::Link | Kind::Other => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryStats {
    pub category: Category,
    pub files: u64,
    pub size: u64,
    pub allocated: u64,
}

/// Space per category below `start`, largest first (empty categories left out).
pub fn by_category(tree: &Tree, start: NodeId) -> Vec<CategoryStats> {
    let mut v: Vec<CategoryStats> = Category::ALL
        .iter()
        .map(|&category| CategoryStats { category, files: 0, size: 0, allocated: 0 })
        .collect();
    walk_files(tree, start, |id, cat| {
        let n = tree.node(id);
        if n.has(crate::tree::flags::HARDLINK_DUP) {
            return;
        }
        let s = &mut v[cat as usize];
        s.files += 1;
        s.size += n.size;
        s.allocated += n.allocated;
    });
    v.retain(|s| s.files > 0);
    v.sort_by(|a, b| b.size.cmp(&a.size).then(a.category.cmp(&b.category)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn file(name: &str, size: u64) -> Node {
        let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
        n.size = size;
        n
    }

    fn dir(name: &str) -> Node {
        Node::new(OsStr::new(name).into(), NO_PARENT, Kind::Dir)
    }

    #[test]
    fn categories_by_extension_and_folder() {
        assert_eq!(Category::by_extension("mkv"), Category::Video);
        assert_eq!(Category::by_extension("heic"), Category::Pictures);
        assert_eq!(Category::by_extension("unbekannt"), Category::Other);
        assert_eq!(Category::of_folder("SteamApps"), Some(Category::Games));
        assert_eq!(Category::of_folder("WinSxS"), Some(Category::System));
        assert_eq!(Category::of_folder("Urlaub"), None);
        assert_eq!(Category::of_file("Hiberfil.sys"), Some(Category::System));
        // The table index of a category must match its position in ALL.
        for (i, c) in Category::ALL.iter().enumerate() {
            assert_eq!(*c as usize, i);
        }
    }

    #[test]
    fn folder_marks_win_over_extension() {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let first = t.add_children(ROOT, [dir("steamapps"), file("film.mp4", 100), file("pagefile.sys", 7)]);
        let inner = t.add_children(first, [dir("Cache"), file("intro.mp4", 50)]);
        t.add_children(inner, [file("x.jpg", 3)]);
        t.sum_up();
        let stats = by_category(&t, ROOT);
        let get = |c| stats.iter().find(|s| s.category == c).map(|s| (s.files, s.size));
        assert_eq!(get(Category::Video), Some((1, 100)));
        assert_eq!(get(Category::Games), Some((1, 50)));
        assert_eq!(get(Category::Caches), Some((1, 3)));
        assert_eq!(get(Category::System), Some((1, 7)));
        assert_eq!(get(Category::Pictures), None);
        // Starting inside the game folder keeps its category.
        let games = by_category(&t, first);
        assert_eq!(games[0].category, Category::Games);
    }

    #[test]
    fn root_path_counts_as_context() {
        let mut t = Tree::new(PathBuf::from("/home/a/.cache"), Kind::Dir);
        t.add_children(ROOT, [file("data.bin", 10)]);
        t.sum_up();
        assert_eq!(by_category(&t, ROOT)[0].category, Category::Caches);
    }
}
