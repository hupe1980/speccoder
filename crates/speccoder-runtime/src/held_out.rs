//! Held-out acceptance tests: written by a second author call
//! after the visible ones, kept in the state directory — never in the
//! worktree, never in an implementer's input — red-checked on the
//! checkpoint, and run over the final tree after an attempt that passed
//! visibly. Only the scenario IDs of a failure reach a retry.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::verify::{CheckResult, RunnerSummary};

use crate::git::Git;
use crate::integrity::{JudgeSnapshot, sha256_hex};
use crate::tools::policy::Policy;
use crate::{sandbox, worktree};

/// Held-out files per task, composition tests included.
pub const LIMIT: usize = 4;

/// One kept held-out test file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldOutFile {
    /// Workspace-relative path it is written to for a run.
    pub path: String,
    /// SHA-256 of its content.
    pub sha256: String,
    /// The scenarios it covers.
    pub scenarios: Vec<String>,
    /// The command that runs only it.
    pub argv: Vec<String>,
    /// It exercises several scenarios together.
    pub composition: bool,
}

/// A file the author wrote that was not kept, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discarded {
    /// Its path as given.
    pub path: String,
    /// `unknown_scenario`, `not_covered_by_visible`, `path`, `command`,
    /// `not_red`, `collision` or `limit`.
    pub reason: String,
}

/// What judged a task, stored beside the files' contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version.
    pub version: u32,
    /// The feature (or change).
    pub feature: String,
    /// The task.
    pub task: String,
    /// The author run.
    pub author_run: String,
    /// The tree the red runs used.
    pub checkpoint: String,
    /// Scenarios this task's acceptance tests cover.
    pub visible_scenarios: Vec<String>,
    /// Kept files.
    pub files: Vec<HeldOutFile>,
    /// Files not kept.
    pub discarded: Vec<Discarded>,
}

impl Manifest {
    /// SHA-256 of the canonical JSON: what the ticking commit names.
    #[must_use]
    pub fn sha256(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).unwrap_or_default())
    }
}

/// Where a task's held-out set lives: `<project state>/held-out/<feature>/<task>`.
#[must_use]
pub fn dir(project_state: &Path, feature: &str, task: &str) -> PathBuf {
    project_state.join("held-out").join(feature).join(task)
}

/// Stores the manifest and the kept files' contents; returns the
/// manifest's hash. A previous set for the task is replaced.
pub fn store(
    dir: &Path,
    manifest: &Manifest,
    contents: &[(String, String)],
) -> Result<String, String> {
    let _ = std::fs::remove_dir_all(dir);
    for (path, content) in contents {
        let file = dir.join("files").join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), text).map_err(|e| e.to_string())?;
    Ok(manifest.sha256())
}

/// A stored set that no longer matches what was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tampered(pub String);

/// The stored set, checked: the manifest must hash to `expected` and every
/// file to its recorded hash.
pub fn load_verified(
    dir: &Path,
    expected: &str,
) -> Result<(Manifest, Vec<(String, String)>), Tampered> {
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| Tampered(format!("the held-out manifest is unreadable: {e}")))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| Tampered(format!("the held-out manifest is not valid: {e}")))?;
    if manifest.sha256() != expected {
        return Err(Tampered(
            "the held-out manifest changed since it was written".to_owned(),
        ));
    }
    let mut contents = Vec::new();
    for f in &manifest.files {
        let content = std::fs::read_to_string(dir.join("files").join(&f.path))
            .map_err(|_| Tampered(format!("held-out file {} is missing", f.path)))?;
        if sha256_hex(content.as_bytes()) != f.sha256 {
            return Err(Tampered(format!("held-out file {} changed", f.path)));
        }
        contents.push((f.path.clone(), content));
    }
    Ok((manifest, contents))
}

/// One held-out file's run.
#[derive(Debug, Clone)]
pub struct Ran {
    /// Its path.
    pub path: String,
    /// The run, or `None` when its path already existed (not overwritten).
    pub result: Option<(CheckResult, Option<RunnerSummary>)>,
}

/// Writes each held-out file over the worktree, runs its command, and puts
/// the tree back exactly as it was: no file of the set, nothing a test wrote,
/// no changed judge input survives — a later attempt can read none of it.
pub fn overlay_run(
    wt_root: &Path,
    project: &Path,
    tmp: &Path,
    files: &[(HeldOutFile, String)],
    runner: &sandbox::Runner,
    policy: &Policy,
) -> Result<Vec<Ran>, String> {
    let before = worktree::checkpoint(wt_root, tmp).map_err(|e| e.0)?;
    let judged = JudgeSnapshot::take(wt_root, policy);
    let mut out = Vec::new();
    for (f, content) in files {
        let full = project.join(&f.path);
        if full.exists() {
            out.push(Ran {
                path: f.path.clone(),
                result: None,
            });
            continue;
        }
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&full, content).map_err(|e| format!("{}: {e}", f.path))?;
        let ran = crate::pipeline::acceptance_run(project, &f.argv, runner);
        let _ = std::fs::remove_file(&full);
        out.push(Ran {
            path: f.path.clone(),
            result: Some(ran),
        });
    }
    worktree::restore(wt_root, tmp, &before).map_err(|e| e.0)?;
    // What a test wrote to its private scratch would outlive the run.
    if let Some(scratch) = runner.scratch() {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    let after = worktree::checkpoint(wt_root, tmp).map_err(|e| e.0)?;
    if after != before {
        return Err("the held-out run left changes in the worktree".to_owned());
    }
    let drift = judged.drift(
        &JudgeSnapshot::take(wt_root, policy),
        policy,
        &BTreeSet::new(),
    );
    if let Some(d) = drift.first() {
        return Err(format!("the held-out run changed judge input {}", d.path));
    }
    Ok(out)
}

/// Literals a copied test would carry: quoted strings of 6+ characters and
/// numbers with 4+ significant digits.
#[must_use]
pub fn literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for quote in ['"', '\''] {
        let mut parts = text.split(quote);
        parts.next();
        while let (Some(inside), Some(_)) = (parts.next(), parts.next().or(Some(""))) {
            if inside.chars().count() >= 6 && !inside.contains('\n') {
                out.insert(inside.to_owned());
            }
        }
    }
    let mut number = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || (c == '.' && !number.is_empty()) {
            number.push(c);
        } else {
            let n = number.trim_end_matches('.');
            let significant = n
                .trim_start_matches(['0', '.'])
                .chars()
                .filter(char::is_ascii_digit)
                .count();
            if significant >= 4 {
                out.insert(n.to_owned());
            }
            number.clear();
        }
    }
    out
}

/// Held-out literals in the attempt's added lines that it could only have
/// copied: not in the spec, the visible tests or the tree it started from
///. For a native attempt this never fires; for an external
/// agent, which can read the state directory, it is the mechanical check.
#[must_use]
pub fn leaked(
    held_out: &[(HeldOutFile, String)],
    known: &str,
    wt_root: &Path,
    start_tree: &worktree::Checkpoint,
    patch: &str,
) -> Vec<String> {
    let added: String = patch
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .collect::<Vec<_>>()
        .join("\n");
    let git = Git::at(wt_root);
    held_out
        .iter()
        .flat_map(|(_, content)| literals(content))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|lit| added.contains(lit.as_str()) && !known.contains(lit.as_str()))
        .filter(|lit| {
            !git.succeeds(&["grep", "-q", "-F", "-e", lit, &start_tree.0])
                .unwrap_or(true)
        })
        .collect()
}

/// Where a finished feature's held-out sets are published.
pub const REF_PREFIX: &str = "refs/speccoder/held-out";

/// Publishes a finished feature's held-out sets: one commit,
/// parented on the feature branch's tip, with `<task>/manifest.json` and
/// `<task>/files/…` of every task, under `refs/speccoder/held-out/<feature>`.
/// Only once the feature is done — earlier, a later task could read them.
/// Never pushed by speccoder itself. `Ok(None)` when no task has a set.
pub fn publish(
    repo: &Path,
    project_state: &Path,
    feature: &str,
    tmp: &Path,
) -> Result<Option<String>, String> {
    let base = project_state.join("held-out").join(feature);
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut stack = vec![base.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(&base) {
                files.push((rel.to_string_lossy().replace('\\', "/"), p.clone()));
            }
        }
    }
    if files.is_empty() {
        return Ok(None);
    }
    files.sort();
    let git = Git::at(repo);
    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    let index = tmp.join(format!("held-out-index-{}", std::process::id()));
    let _ = std::fs::remove_file(&index);
    for (rel, path) in &files {
        let blob = git
            .line(&["hash-object", "-w", "--", &path.to_string_lossy()])
            .map_err(|e| e.0)?;
        git.run_with_index(
            &index,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{blob},{rel}"),
            ],
            None,
        )
        .map_err(|e| e.0)?;
    }
    let tree = git
        .run_with_index(&index, &["write-tree"], None)
        .map_err(|e| e.0)?;
    let _ = std::fs::remove_file(&index);
    let tree = String::from_utf8_lossy(&tree).trim().to_owned();
    let branch = format!("speccoder/{feature}");
    let parent = git
        .line(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")])
        .map_err(|e| e.0)?;
    let message = format!(
        "speccoder: held-out tests of {feature}\n\nThe tests that judged each task, published once the feature is done.\nEach <task>/manifest.json hashes to its task's Held-Out trailer.\n\nSpec-Feature: {feature}\n"
    );
    let mut args = crate::attempts::IDENTITY.to_vec();
    args.extend(["commit-tree", &tree, "-p", &parent, "-m", &message]);
    let commit = git.line(&args).map_err(|e| e.0)?;
    git.run(&["update-ref", &format!("{REF_PREFIX}/{feature}"), &commit])
        .map_err(|e| e.0)?;
    Ok(Some(commit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_long_strings_and_precise_numbers() {
        let l = literals(
            "assert_eq!(slug(\"Hello, World\"), \"hello-world\"); x(3.14159); y(42); z('ab')",
        );
        assert!(l.contains("Hello, World") && l.contains("hello-world"));
        assert!(l.contains("3.14159"));
        assert!(!l.contains("42") && !l.contains("ab"));
    }

    #[test]
    fn a_changed_file_or_manifest_is_tampering() {
        let d = tempfile::tempdir().unwrap();
        let content = "fn t() {}\n".to_owned();
        let m = Manifest {
            version: 1,
            feature: "f".to_owned(),
            task: "T1".to_owned(),
            author_run: "r".to_owned(),
            checkpoint: "c".to_owned(),
            visible_scenarios: vec!["US1-1".to_owned()],
            files: vec![HeldOutFile {
                path: "tests/h.rs".to_owned(),
                sha256: sha256_hex(content.as_bytes()),
                scenarios: vec!["US1-1".to_owned()],
                argv: vec!["true".to_owned()],
                composition: false,
            }],
            discarded: Vec::new(),
        };
        let sha = store(d.path(), &m, &[("tests/h.rs".to_owned(), content)]).unwrap();
        assert_eq!(load_verified(d.path(), &sha).unwrap().1.len(), 1);
        assert!(load_verified(d.path(), "other").is_err());
        std::fs::write(d.path().join("files/tests/h.rs"), "fn t() { changed }").unwrap();
        assert!(
            load_verified(d.path(), &sha)
                .unwrap_err()
                .0
                .contains("changed")
        );
        std::fs::remove_file(d.path().join("files/tests/h.rs")).unwrap();
        assert!(
            load_verified(d.path(), &sha)
                .unwrap_err()
                .0
                .contains("missing")
        );
    }
}
