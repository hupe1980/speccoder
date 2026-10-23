//! The integrity audit of an attempt's diff.
//!
//! Line-based and deliberately suspicious: it errs on flagging. Any finding
//! fails the verdict with an `integrity:` reason; a person looks.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use crate::tools::paths::{Class, Protected};
use crate::worktree::{Change, ChangeKind};

/// Markers that declare a test.
const TEST_MARKERS: &str = r"#\[(tokio::)?test\b|^\s*def test_|^\s*(async\s+)?fn test_|\b(it|test)\s*\(\s*['\x22`]|@Test\b|^\s*func Test";
/// Markers that skip a test.
const SKIP_MARKERS: &str = r"#\[ignore\b|@pytest\.mark\.(skip|xfail)|\.skip\s*\(|\bx(it|describe|test)\s*\(|@Disabled\b|@Ignore\b|\bt\.Skip(Now)?\s*\(|@unittest\.skip|\.todo\s*\(";

/// Whether every line of `old` survives, unchanged and in order, in `new`:
/// the change only adds lines (`[tests] frozen = "additions"`).
#[must_use]
pub fn only_additions(old: &str, new: &str) -> bool {
    let mut new_lines = new.lines();
    old.lines().all(|line| new_lines.any(|n| n == line))
}

/// Findings for an attempt: `changes` and `patch` between its checkpoints,
/// `frozen` the test files that existed when it started. With
/// `additions_ok`, a frozen test whose diff only adds lines is no finding
/// (removed tests and skip markers still are).
#[must_use]
pub fn audit(
    changes: &[Change],
    patch: &str,
    frozen: &BTreeSet<String>,
    additions_ok: bool,
) -> Vec<String> {
    let protected = Protected::default();
    let mut findings = Vec::new();
    let removed_lines = removed_lines(patch);

    for c in changes {
        if frozen.contains(&c.path) {
            match c.kind {
                ChangeKind::Deleted => findings.push(format!("frozen test deleted: {}", c.path)),
                ChangeKind::Modified
                    if additions_ok && removed_lines.get(&c.path).copied().unwrap_or(0) == 0 => {}
                ChangeKind::Modified => findings.push(format!("frozen test modified: {}", c.path)),
                ChangeKind::Added => {}
            }
        }
        match protected.write_class(&c.path) {
            Some(Class::Judge) => findings.push(format!("verification input changed: {}", c.path)),
            Some(Class::Intent) => findings.push(format!("spec changed: {}", c.path)),
            _ => {}
        }
    }

    let (Ok(test), Ok(skip)) = (Regex::new(TEST_MARKERS), Regex::new(SKIP_MARKERS)) else {
        return findings;
    };
    let mut tests: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (added, removed)
    let mut skips: BTreeSet<String> = BTreeSet::new();
    let mut file = String::new();
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("diff --git a/") {
            // `diff --git a/<path> b/<path>` (no renames): the path is the
            // same on both sides, also for additions and deletions.
            file = header
                .split_once(" b/")
                .map_or(header, |(a, _)| a)
                .to_owned();
        } else if line.starts_with("+++ ") || line.starts_with("--- ") {
            // File headers; the name came from `diff --git`.
        } else if let Some(added) = line.strip_prefix('+') {
            if test.is_match(added) {
                tests.entry(file.clone()).or_default().0 += 1;
            }
            if skip.is_match(added) {
                skips.insert(file.clone());
            }
        } else if let Some(removed) = line.strip_prefix('-')
            && test.is_match(removed)
        {
            tests.entry(file.clone()).or_default().1 += 1;
        }
    }
    let deleted: BTreeSet<&str> = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Deleted)
        .map(|c| c.path.as_str())
        .collect();
    for (path, (added, removed)) in tests {
        // A deleted file is reported as such (frozen) or is new work undone.
        if removed > added && !deleted.contains(path.as_str()) {
            findings.push(format!(
                "test removed in {path} ({removed} removed, {added} added)"
            ));
        }
    }
    for path in skips {
        findings.push(format!("skip marker added in {path}"));
    }
    findings
}

/// Removed lines per file in a unified diff.
fn removed_lines(patch: &str) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    let mut file = String::new();
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("diff --git a/") {
            file = header
                .split_once(" b/")
                .map_or(header, |(a, _)| a)
                .to_owned();
        } else if line.starts_with('-') && !line.starts_with("--- ") {
            *out.entry(file.clone()).or_default() += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind) -> Change {
        Change {
            path: path.to_owned(),
            kind,
        }
    }

    #[test]
    fn flags_tampering_and_passes_clean_diffs() {
        let frozen: BTreeSet<String> = ["tests/api.rs".to_owned()].into();
        let patch = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-    #[test]\n+    #[ignore]\n+    #[test]\n";
        let f = audit(
            &[
                change("tests/api.rs", ChangeKind::Deleted),
                change("speccoder.toml", ChangeKind::Modified),
            ],
            patch,
            &frozen,
            false,
        );
        assert_eq!(
            f,
            [
                "frozen test deleted: tests/api.rs",
                "verification input changed: speccoder.toml",
                "skip marker added in src/lib.rs",
            ]
        );

        let removed = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,4 +4,0 @@\n-    #[test]\n-    fn a() {}\n";
        assert_eq!(
            audit(&[], removed, &BTreeSet::new(), false),
            ["test removed in src/lib.rs (1 removed, 0 added)"]
        );

        let clean = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,4 @@\n+pub fn add() {}\n+    #[test]\n+    fn adds() {}\n";
        assert!(
            audit(
                &[
                    change("src/lib.rs", ChangeKind::Modified),
                    change("tests/new.rs", ChangeKind::Added)
                ],
                clean,
                &frozen,
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn additions_to_a_frozen_test_pass_only_in_additions_mode() {
        let frozen: BTreeSet<String> = ["tests/api.rs".to_owned()].into();
        let added = "diff --git a/tests/api.rs b/tests/api.rs\n--- a/tests/api.rs\n+++ b/tests/api.rs\n@@ -1,2 +1,4 @@\n #[test]\n fn a() {}\n+#[test]\n+fn b() {}\n";
        let changed = "diff --git a/tests/api.rs b/tests/api.rs\n--- a/tests/api.rs\n+++ b/tests/api.rs\n@@ -1,2 +1,2 @@\n #[test]\n-fn a() { assert!(x()) }\n+fn a() {}\n";
        let modified = [change("tests/api.rs", ChangeKind::Modified)];
        assert!(audit(&modified, added, &frozen, true).is_empty());
        assert_eq!(
            audit(&modified, added, &frozen, false),
            ["frozen test modified: tests/api.rs"]
        );
        assert_eq!(
            audit(&modified, changed, &frozen, true),
            ["frozen test modified: tests/api.rs"]
        );

        assert!(only_additions("a\nb\n", "a\nnew\nb\nmore\n"));
        assert!(!only_additions("a\nb\n", "a\nB\n"));
        assert!(!only_additions("a\nb\n", "b\na\n"));
    }
}
