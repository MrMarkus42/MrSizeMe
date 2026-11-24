//! Searching and filtering files, oldest files, age distribution.

use crate::category::{Category, walk_files};
use crate::tree::{NO_TIME, NodeId, Tree, flags};

pub const DAY: i64 = 86_400;
pub const YEAR: i64 = 365 * DAY + DAY / 4;

/// Conditions a file must meet; empty fields mean "any".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Part of the name, or a pattern with `*` and `?` (case does not matter).
    pub name: String,
    pub category: Option<Category>,
    /// At least this many bytes.
    pub min_size: u64,
    /// Last change at least this many seconds ago.
    pub older_than: Option<i64>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty()
            && self.category.is_none()
            && self.min_size == 0
            && self.older_than.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// The largest matches (at most the requested number), largest first.
    pub files: Vec<NodeId>,
    /// Number and size of all matches.
    pub count: u64,
    pub size: u64,
}

/// Case-insensitive match of `name` against a pattern with `*` and `?`;
/// without wildcards the pattern only has to occur somewhere in the name.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.trim().chars().flat_map(char::to_lowercase).collect();
    let n: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    if p.is_empty() {
        return true;
    }
    if !p.iter().any(|&c| c == '*' || c == '?') {
        return n.windows(p.len()).any(|w| w == p.as_slice());
    }
    // Wildcard match with backtracking to the last '*'.
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Files below `start` that meet `filter`; `now` in seconds since 1970.
pub fn search(tree: &Tree, start: NodeId, filter: &Filter, now: i64, limit: usize) -> Hits {
    let mut hits = Hits::default();
    let mut all = Vec::new();
    walk_files(tree, start, |id, cat| {
        let n = tree.node(id);
        if n.has(flags::HARDLINK_DUP)
            || n.size < filter.min_size
            || filter.category.is_some_and(|c| c != cat)
            || filter.older_than.is_some_and(|age| n.modified == NO_TIME || n.modified > now - age)
            || !name_matches(&filter.name, &n.name_lossy())
        {
            return;
        }
        hits.count += 1;
        hits.size += n.size;
        all.push(id);
    });
    hits.files = largest_of(tree, all, limit, |n| std::cmp::Reverse(n.size));
    hits
}

/// The `limit` files with the oldest change date below `start` (unknown dates left out).
pub fn oldest_files(tree: &Tree, start: NodeId, limit: usize) -> Vec<NodeId> {
    let mut all = Vec::new();
    walk_files(tree, start, |id, _| {
        if tree.node(id).modified != NO_TIME {
            all.push(id);
        }
    });
    largest_of(tree, all, limit, |n| n.modified)
}

/// Sorts by `key` and keeps the first `limit` (partial sort for big lists).
fn largest_of<K: Ord>(
    tree: &Tree,
    mut ids: Vec<NodeId>,
    limit: usize,
    key: impl Fn(&crate::tree::Node) -> K,
) -> Vec<NodeId> {
    let k = |id: &NodeId| key(tree.node(*id));
    if ids.len() > limit && limit > 0 {
        ids.select_nth_unstable_by_key(limit - 1, k);
        ids.truncate(limit);
    }
    ids.sort_by_key(k);
    ids.truncate(limit);
    ids
}

/// Age groups of the age distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeGroup {
    Month,
    HalfYear,
    Year,
    TwoYears,
    FiveYears,
    Older,
    Unknown,
}

impl AgeGroup {
    pub const ALL: [AgeGroup; 7] = [
        AgeGroup::Month,
        AgeGroup::HalfYear,
        AgeGroup::Year,
        AgeGroup::TwoYears,
        AgeGroup::FiveYears,
        AgeGroup::Older,
        AgeGroup::Unknown,
    ];

    pub fn of(modified: i64, now: i64) -> AgeGroup {
        if modified == NO_TIME {
            return AgeGroup::Unknown;
        }
        let age = now - modified;
        if age < 30 * DAY {
            AgeGroup::Month
        } else if age < YEAR / 2 {
            AgeGroup::HalfYear
        } else if age < YEAR {
            AgeGroup::Year
        } else if age < 2 * YEAR {
            AgeGroup::TwoYears
        } else if age < 5 * YEAR {
            AgeGroup::FiveYears
        } else {
            AgeGroup::Older
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgeStats {
    pub files: u64,
    pub size: u64,
}

/// Number and size of the files per age group (in the order of [`AgeGroup::ALL`]).
pub fn age_distribution(tree: &Tree, start: NodeId, now: i64) -> [AgeStats; 7] {
    let mut out = [AgeStats::default(); 7];
    walk_files(tree, start, |id, _| {
        let n = tree.node(id);
        if n.has(flags::HARDLINK_DUP) {
            return;
        }
        let s = &mut out[AgeGroup::of(n.modified, now) as usize];
        s.files += 1;
        s.size += n.size;
    });
    out
}

/// Current time in seconds since 1970.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    const NOW: i64 = 2_000_000_000;

    fn file(name: &str, size: u64, age_days: i64) -> Node {
        let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
        n.size = size;
        n.modified = NOW - age_days * DAY;
        n
    }

    fn sample() -> Tree {
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let first = t.add_children(
            ROOT,
            [
                Node::new(OsStr::new("Videos").into(), NO_PARENT, Kind::Dir),
                file("neu.mp4", 5_000_000_000, 3),
                file("Bericht.pdf", 2_000, 500),
            ],
        );
        t.add_children(first, [file("urlaub.MKV", 3_000_000_000, 1000), file("kurz.mp4", 10_000, 2000)]);
        t.sum_up();
        t
    }

    #[test]
    fn patterns() {
        assert!(name_matches("", "egal"));
        assert!(name_matches("laub", "Urlaub.mkv"));
        assert!(name_matches("*.MKV", "urlaub.mkv"));
        assert!(name_matches("u?laub*", "Urlaub.mkv"));
        assert!(!name_matches("*.mp4", "urlaub.mkv"));
        assert!(name_matches("a*b*c", "aXXbYYc"));
        assert!(!name_matches("a*b*c", "aXXbYY"));
    }

    #[test]
    fn videos_over_1_gb_older_than_2_years() {
        let t = sample();
        let f = Filter {
            category: Some(Category::Video),
            min_size: 1 << 30,
            older_than: Some(2 * YEAR),
            ..Filter::default()
        };
        let hits = search(&t, ROOT, &f, NOW, 100);
        assert_eq!(hits.count, 1);
        assert_eq!(t.node(hits.files[0]).name_lossy(), "urlaub.MKV");

        let all = search(&t, ROOT, &Filter::default(), NOW, 2);
        assert_eq!(all.count, 4);
        assert_eq!(all.files.len(), 2);
        assert_eq!(t.node(all.files[0]).name_lossy(), "neu.mp4");
        assert!(Filter::default().is_empty() && !f.is_empty());
    }

    #[test]
    fn oldest_and_ages() {
        let t = sample();
        let old = oldest_files(&t, ROOT, 2);
        let names: Vec<_> = old.iter().map(|&i| t.node(i).name_lossy().into_owned()).collect();
        assert_eq!(names, ["kurz.mp4", "urlaub.MKV"]);
        let ages = age_distribution(&t, ROOT, NOW);
        assert_eq!(ages[AgeGroup::Month as usize].files, 1);
        assert_eq!(ages[AgeGroup::TwoYears as usize].files, 1); // 500 days
        assert_eq!(ages[AgeGroup::FiveYears as usize].files, 1); // 1000 days
        assert_eq!(ages[AgeGroup::Older as usize].files, 1); // 2000 days
        assert_eq!(AgeGroup::of(NO_TIME, NOW), AgeGroup::Unknown);
    }
}
