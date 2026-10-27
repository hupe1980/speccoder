//! The integrity audit of an attempt's diff.
//!
//! Line-based and deliberately suspicious: it errs on flagging. Any finding
//! fails the verdict with an `integrity:` reason; a person looks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::tools::paths::{Class, Protected};
use crate::tools::policy::Policy;
use crate::worktree::{Change, ChangeKind};

/// Directories a snapshot never descends into: build output and installed
/// dependencies, where no judge input lives.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
];

/// The judge, intent and agent context inputs of a worktree by content hash,
/// git-ignored files included (`.claude/settings.local.json` is ignored by
/// default, and an agent still loads it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JudgeSnapshot(BTreeMap<String, String>);

/// A judge input that changed between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Drift {
    /// Workspace-relative path.
    pub path: String,
    /// `judge`, `intent` or `context`.
    pub class: &'static str,
    /// `added`, `modified` or `deleted`.
    pub change: &'static str,
}

impl JudgeSnapshot {
    /// Hashes every judge, intent and context input under `root` as `policy`
    /// sees them (a planning skill may write Spec Kit's artifacts).
    #[must_use]
    pub fn take(root: &Path, policy: &Policy) -> Self {
        let mut files = BTreeMap::new();
        let walk = ignore::WalkBuilder::new(root)
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .parents(false)
            .follow_links(false)
            .filter_entry(|e| {
                !(e.depth() > 0
                    && e.file_type().is_some_and(|t| t.is_dir())
                    && SKIPPED_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
            })
            .build();
        for entry in walk.flatten() {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let Ok(rel) = entry.path().strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if class_of(policy, &rel).is_none() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(entry.path()) {
                files.insert(rel, sha256_hex(&bytes));
            }
        }
        Self(files)
    }

    /// Path → content hash.
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    /// A snapshot from recorded entries.
    #[must_use]
    pub fn from_entries(entries: BTreeMap<String, String>) -> Self {
        Self(entries)
    }

    /// Records `path`'s new content hash (the engine's own sanctioned write).
    pub fn set(&mut self, path: &str, hash: String) {
        self.0.insert(path.to_owned(), hash);
    }

    /// What changed from `self` to `later`, except `approved` paths (a
    /// person's grant to write them).
    #[must_use]
    pub fn drift(&self, later: &Self, policy: &Policy, approved: &BTreeSet<String>) -> Vec<Drift> {
        let mut out = Vec::new();
        let paths: BTreeSet<&String> = self.0.keys().chain(later.0.keys()).collect();
        for path in paths {
            let change = match (self.0.get(path), later.0.get(path)) {
                (None, Some(_)) => "added",
                (Some(_), None) => "deleted",
                (Some(a), Some(b)) if a != b => "modified",
                _ => continue,
            };
            if approved.contains(path.as_str()) {
                continue;
            }
            if let Some(class) = class_of(policy, path) {
                out.push(Drift {
                    path: path.clone(),
                    class,
                    change,
                });
            }
        }
        out
    }
}

fn class_of(policy: &Policy, rel: &str) -> Option<&'static str> {
    match policy.write_protected(rel) {
        Some(Class::Judge) => Some("judge"),
        Some(Class::Intent) => Some("intent"),
        Some(_) => None,
        None if policy.protected().is_context(rel) => Some("context"),
        None => None,
    }
}

/// Markers that declare a test.
const TEST_MARKERS: &str = r"#\[(tokio::)?test\b|^\s*def test_|^\s*(async\s+)?fn test_|\b(it|test)\s*\(\s*['\x22`]|@Test\b|^\s*func Test";
/// Markers that skip a test.
const SKIP_MARKERS: &str = r"#\[ignore\b|@pytest\.mark\.(skip|xfail)|\.skip\s*\(|\bx(it|describe|test)\s*\(|@Disabled\b|@Ignore\b|\bt\.Skip(Now)?\s*\(|@unittest\.skip|\.todo\s*\(";

/// Test definitions with their names: Rust `#[test] fn`, Python `def
/// test_`, Go `func Test`, JavaScript `it("…")` / `test("…")`.
const TEST_NAMES: &str = r#"(?m)#\[(?:tokio::)?test\b[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)|^\s*(?:async\s+)?def\s+(test_\w+)|^\s*func\s+(Test\w+)|\b(?:it|test)\s*\(\s*['"`]([^'"`]+)['"`]"#;

/// The tests `before` defines that `after` no longer does, in order.
#[must_use]
pub fn removed_tests(before: &str, after: &str) -> Vec<String> {
    let Ok(re) = Regex::new(TEST_NAMES) else {
        return Vec::new();
    };
    let names = |text: &str| -> Vec<String> {
        re.captures_iter(text)
            .filter_map(|c| {
                c.iter()
                    .skip(1)
                    .flatten()
                    .next()
                    .map(|m| m.as_str().to_owned())
            })
            .collect()
    };
    let kept: BTreeSet<String> = names(after).into_iter().collect();
    let mut gone = names(before);
    gone.retain(|n| !kept.contains(n));
    gone.dedup();
    gone
}

/// The section `new` adds at the end of `old`: `None` when `new` changes,
/// removes or inserts among `old`'s lines, or adds nothing.
#[must_use]
pub fn appended_section<'n>(old: &str, new: &'n str) -> Option<&'n str> {
    // The old text whole, then new lines: never a line inserted among it.
    let added = new.strip_prefix(old).or_else(|| {
        (!old.ends_with('\n'))
            .then(|| new.strip_prefix(&format!("{old}\n")))
            .flatten()
    })?;
    (!added.trim().is_empty()).then_some(added)
}

/// Whether `text` holds a marker that skips a test.
#[must_use]
pub fn has_skip_marker(text: &str) -> bool {
    Regex::new(SKIP_MARKERS).is_ok_and(|r| r.is_match(text))
}

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
/// (removed tests and skip markers still are). An agent context file
/// (`AGENTS.md`, …) changed without a person's grant in `approved` is one.
#[must_use]
pub fn audit(
    changes: &[Change],
    patch: &str,
    frozen: &BTreeSet<String>,
    additions_ok: bool,
    approved: &BTreeSet<String>,
) -> Vec<String> {
    audit_in(
        &Protected::default(),
        changes,
        patch,
        frozen,
        additions_ok,
        approved,
    )
}

/// [`audit`] against the protected set of the project's format.
#[must_use]
pub fn audit_in(
    protected: &Protected,
    changes: &[Change],
    patch: &str,
    frozen: &BTreeSet<String>,
    additions_ok: bool,
    approved: &BTreeSet<String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    let removed_lines = removed_lines(patch);

    for c in changes {
        if frozen
            .iter()
            .any(|f| f.to_lowercase() == c.path.to_lowercase())
        {
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
            None if protected.is_context(&c.path) && !approved.contains(&c.path) => {
                findings.push(format!("agent context file changed: {}", c.path));
            }
            _ => {}
        }
    }

    let (Ok(test), Ok(skip)) = (Regex::new(TEST_MARKERS), Regex::new(SKIP_MARKERS)) else {
        return findings;
    };
    let mut tests: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (added, removed)
    let mut text: BTreeMap<String, (String, String)> = BTreeMap::new(); // (added, removed)
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
            let t = text.entry(file.clone()).or_default();
            t.0.push_str(added);
            t.0.push('\n');
            if test.is_match(added) {
                tests.entry(file.clone()).or_default().0 += 1;
            }
            if skip.is_match(added) {
                skips.insert(file.clone());
            }
        } else if let Some(removed) = line.strip_prefix('-') {
            let t = text.entry(file.clone()).or_default();
            t.1.push_str(removed);
            t.1.push('\n');
            if test.is_match(removed) {
                tests.entry(file.clone()).or_default().1 += 1;
            }
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
            let names = text
                .get(&path)
                .map(|(a, r)| removed_tests(r, a))
                .unwrap_or_default();
            let named = if names.is_empty() {
                String::new()
            } else {
                format!(": {}", names.join(", "))
            };
            findings.push(format!(
                "test removed in {path} ({removed} removed, {added} added){named}"
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

/// Lowercase hex SHA-256 of `bytes`: the content hash of a judge input or a
/// task list, as the format adapters' write guards compare it.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The bytes with every `[x]`/`[X]` checkbox read as `[ ]`.
pub(crate) fn untick(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for i in 1..out.len().saturating_sub(1) {
        if out[i - 1] == b'[' && out[i + 1] == b']' && matches!(out[i], b'x' | b'X') {
            out[i] = b' ';
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
    fn removed_tests_are_named_across_languages() {
        let before = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {}\n    #[tokio::test]\n    async fn later() {}\n}\ndef test_py(): pass\nfunc TestGo(t *testing.T) {}\nit('greets', () => {})\n";
        assert_eq!(
            removed_tests(before, "#[test]\nfn later() {}\n"),
            ["it_works", "test_py", "TestGo", "greets"]
        );
        assert!(removed_tests(before, before).is_empty());
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
            &BTreeSet::new(),
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
            audit(&[], removed, &BTreeSet::new(), false, &BTreeSet::new()),
            ["test removed in src/lib.rs (1 removed, 0 added): a"]
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
                false,
                &BTreeSet::new()
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
        assert!(audit(&modified, added, &frozen, true, &BTreeSet::new()).is_empty());
        assert_eq!(
            audit(&modified, added, &frozen, false, &BTreeSet::new()),
            ["frozen test modified: tests/api.rs"]
        );
        assert_eq!(
            audit(&modified, changed, &frozen, true, &BTreeSet::new()),
            ["frozen test modified: tests/api.rs"]
        );

        assert!(only_additions("a\nb\n", "a\nnew\nb\nmore\n"));
        assert!(!only_additions("a\nb\n", "a\nB\n"));
        assert!(!only_additions("a\nb\n", "b\na\n"));
    }

    #[test]
    fn a_snapshot_sees_ignored_judge_inputs_and_reports_drift() {
        use crate::tools::policy::Policy;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join(".claude/settings.json"), "{}").unwrap();
        std::fs::write(root.join("AGENTS.md"), "a").unwrap();
        std::fs::write(root.join("speccoder.toml"), "x").unwrap();
        std::fs::write(root.join("src.rs"), "free").unwrap();
        std::fs::write(root.join("target/debug/speccoder.toml"), "skipped").unwrap();
        let policy = Policy::new(&[], &[]);
        let before = JudgeSnapshot::take(root, &policy);
        assert_eq!(
            before.0.len(),
            3,
            "{:?}",
            before.0.keys().collect::<Vec<_>>()
        );

        std::fs::write(root.join(".claude/settings.local.json"), "{}").unwrap();
        std::fs::write(root.join("AGENTS.md"), "b").unwrap();
        std::fs::remove_file(root.join("speccoder.toml")).unwrap();
        std::fs::write(root.join("src.rs"), "changed").unwrap();
        let after = JudgeSnapshot::take(root, &policy);
        let drift = before.drift(&after, &policy, &BTreeSet::new());
        let seen: Vec<(&str, &str, &str)> = drift
            .iter()
            .map(|d| (d.path.as_str(), d.class, d.change))
            .collect();
        assert_eq!(
            seen,
            [
                (".claude/settings.local.json", "judge", "added"),
                ("AGENTS.md", "context", "modified"),
                ("speccoder.toml", "judge", "deleted"),
            ]
        );
        // A person's grant for the context file exempts it.
        let approved: BTreeSet<String> = ["AGENTS.md".to_owned()].into();
        assert_eq!(before.drift(&after, &policy, &approved).len(), 2);
        // Restored content is no drift.
        std::fs::write(root.join("AGENTS.md"), "a").unwrap();
        std::fs::write(root.join("speccoder.toml"), "x").unwrap();
        std::fs::remove_file(root.join(".claude/settings.local.json")).unwrap();
        assert!(
            before
                .drift(
                    &JudgeSnapshot::take(root, &policy),
                    &policy,
                    &BTreeSet::new()
                )
                .is_empty()
        );
    }

    /// An agent configuration created under another letter case — the
    /// directory absent when the attempt began — is a judge input: on a
    /// case-insensitive volume the agent loads it as `.claude/…`.
    #[test]
    fn a_judge_input_in_another_letter_case_is_drift() {
        use crate::tools::policy::Policy;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let policy = Policy::new(&[], &[]);
        let before = JudgeSnapshot::take(root, &policy);
        std::fs::create_dir_all(root.join(".CLAUDE")).unwrap();
        std::fs::write(root.join(".CLAUDE/settings.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join(".Cursor")).unwrap();
        std::fs::write(root.join(".Cursor/rules.json"), "{}").unwrap();
        std::fs::write(root.join("Makefile.x"), "free").unwrap();
        std::fs::write(root.join("MAKEFILE"), "all:").unwrap();
        let after = JudgeSnapshot::take(root, &policy);
        let drift: Vec<&str> = before
            .drift(&after, &policy, &BTreeSet::new())
            .iter()
            .map(|d| d.class)
            .collect();
        assert_eq!(drift, ["judge", "judge", "judge"], "{:?}", after.0.keys());
        // A frozen test is frozen in any letter case.
        let frozen: BTreeSet<String> = ["tests/api.rs".to_owned()].into();
        assert_eq!(
            audit(
                &[change("Tests/API.rs", ChangeKind::Deleted)],
                "",
                &frozen,
                false,
                &BTreeSet::new()
            ),
            ["frozen test deleted: Tests/API.rs"]
        );
    }
}
