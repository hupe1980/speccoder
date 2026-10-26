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

/// Where a sealed set lives: `<project state>/held-out/<unit>/req-<key>/`
/// (run-time task sets stay at `<unit>/<task>/`).
#[must_use]
pub fn sealed_dir(project_state: &Path, unit: &str, key: &str) -> PathBuf {
    project_state
        .join("held-out")
        .join(unit)
        .join(format!("req-{}", key.replace([':', '/'], "-")))
}

/// Seals a held-out set keyed by `(unit, requirement)` — the entry point
/// for sets decided before a run (a bug's reproduction variants, a
/// review's oracle): the one store protocol, each manifest entry citing
/// its statements or scenarios.
pub fn seal(
    project_state: &Path,
    unit: &str,
    key: &str,
    manifest: &Manifest,
    contents: &[(String, String)],
) -> Result<speccoder_core::acceptance::SealedRef, String> {
    let sha = store(&sealed_dir(project_state, unit, key), manifest, contents)?;
    Ok(speccoder_core::acceptance::SealedRef {
        key: key.to_owned(),
        manifest_sha256: sha,
        count: manifest.files.len(),
    })
}

/// A sealed set, checked against its reference.
pub fn open_sealed(
    project_state: &Path,
    unit: &str,
    sealed: &speccoder_core::acceptance::SealedRef,
) -> Result<(Manifest, Vec<(String, String)>), Tampered> {
    load_verified(
        &sealed_dir(project_state, unit, &sealed.key),
        &sealed.manifest_sha256,
    )
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

/// Where a unit's published held-out sets live.
pub const REF_PREFIX: &str = "refs/speccoder/held-out";

/// The held-out sets a publication carries: exactly those whose manifest
/// digest a task closed on the pushed head names in its evidence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// `(store key, its directory)` of every set carried.
    pub sets: Vec<(String, PathBuf)>,
    /// The closed tasks whose sets are carried, in order.
    pub tasks: Vec<String>,
    /// Stored sets no closed task names (open tasks'): they stay local.
    pub withheld: usize,
    /// Named sets that no longer match their digest: left out, by key.
    pub tampered: Vec<String>,
}

/// The digests a task commit's evidence names: the signed `held_out`
/// phase, else the commit's `Held-Out:` trailer.
#[must_use]
pub fn named_digests(repo: &Path, commit: &str) -> Vec<String> {
    let mut out: Vec<String> = crate::evidence::statements(repo, commit)
        .iter()
        .filter(|s| s["predicate"]["configuration"][0]["name"] == "speccoder:phase/held_out")
        .filter_map(|s| {
            s["predicate"]["configuration"][0]["digest"]["sha256"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    let trailer = Git::at(repo)
        .line(&[
            "log",
            "-1",
            "--format=%(trailers:key=Held-Out,valueonly)",
            commit,
        ])
        .unwrap_or_default();
    out.extend(
        trailer
            .split_whitespace()
            .filter_map(|w| w.strip_prefix("manifest=sha256:"))
            .map(str::to_owned),
    );
    out.sort();
    out.dedup();
    out
}

/// Selects, from every set stored for `unit` (whatever its key: a task's
/// today, a sealed set's once the store holds them — one protocol), those
/// whose manifest digest a closed task names. `closed` is `(task, digests
/// its evidence names)`. Each selected set is checked against its digest
/// first; a tampered one is never published.
#[must_use]
pub fn select_closed(
    project_state: &Path,
    unit: &str,
    closed: &[(String, Vec<String>)],
) -> Selection {
    let base = project_state.join("held-out").join(unit);
    let mut stored: Vec<(String, PathBuf)> = std::fs::read_dir(&base)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("manifest.json").is_file())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    stored.sort();
    let mut out = Selection::default();
    for (key, dir) in stored {
        let digest = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Manifest>(&t).ok())
            .map(|m| m.sha256());
        let named_by = digest.as_ref().and_then(|d| {
            closed
                .iter()
                .find(|(_, digests)| digests.contains(d))
                .map(|(task, _)| task.clone())
        });
        match (named_by, digest) {
            (Some(task), Some(d)) => {
                if load_verified(&dir, &d).is_ok() {
                    if !out.tasks.contains(&task) {
                        out.tasks.push(task);
                    }
                    out.sets.push((key, dir));
                } else {
                    out.tampered.push(key);
                }
            }
            _ => out.withheld += 1,
        }
    }
    out
}

/// Publishes `selection` as one commit under
/// `refs/speccoder/held-out/<unit>`: `<key>/manifest.json` and
/// `<key>/files/…` of every set, parented on `tip` (the pushed branch) and
/// on the ref's previous commit, so the ref only grows and every push of
/// it is a fast-forward. `Ok(None)` when nothing is selected.
pub fn publish_closed(
    repo: &Path,
    unit: &str,
    tip: &str,
    selection: &Selection,
    tmp: &Path,
) -> Result<Option<String>, String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for (key, dir) in &selection.sets {
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(dir) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    files.push((format!("{key}/{rel}"), p.clone()));
                }
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
    let reference = format!("{REF_PREFIX}/{unit}");
    let previous = git
        .line(&[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{reference}^{{commit}}"),
        ])
        .ok()
        .filter(|c| !c.is_empty());
    // Nothing new since the last publication of this tip: keep it.
    if let Some(prev) = &previous
        && git
            .line(&["rev-parse", &format!("{prev}^{{tree}}")])
            .ok()
            .as_deref()
            == Some(&tree)
        && git
            .succeeds(&["merge-base", "--is-ancestor", tip, prev])
            .unwrap_or(false)
    {
        return Ok(Some(prev.clone()));
    }
    let message = format!(
        "speccoder: held-out tests of {unit}\n\nThe tests that judged the tasks closed on {tip}: {}.\nEach <key>/manifest.json hashes to the digest its task's evidence names.\n\nSpec-Feature: {unit}\n",
        selection.tasks.join(", ")
    );
    let mut args = crate::attempts::IDENTITY.to_vec();
    args.extend(["commit-tree", &tree, "-p", tip]);
    if let Some(prev) = &previous {
        args.extend(["-p", prev]);
    }
    args.extend(["-m", &message]);
    let commit = git.line(&args).map_err(|e| e.0)?;
    git.run(&["update-ref", &reference, &commit])
        .map_err(|e| e.0)?;
    Ok(Some(commit))
}

/// Publishes a finished feature's held-out sets locally (every task is
/// closed then): [`publish_closed`] over the tasks the branch tip closes.
/// Never pushed here; `speccoder pr` pushes the ref. `Ok(None)` when no
/// closed task names a stored set.
pub fn publish(
    repo: &Path,
    project_state: &Path,
    feature: &str,
    tmp: &Path,
) -> Result<Option<String>, String> {
    let branch = format!("speccoder/{feature}");
    let tip = Git::at(repo)
        .line(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")])
        .map_err(|e| e.0)?;
    let closed = closed_on(repo, &tip, feature);
    let selection = select_closed(project_state, feature, &closed);
    publish_closed(repo, feature, &tip, &selection, tmp)
}

/// The tasks of `unit` closed on `head` (ticked, with the commit carrying
/// their verdict — the verifier's discovery), each with the held-out
/// digests its evidence names.
#[must_use]
pub fn closed_on(repo: &Path, head: &str, unit: &str) -> Vec<(String, Vec<String>)> {
    crate::attest::branch::closed(repo, head, unit)
        .into_iter()
        .map(|(task, commit)| {
            let digests = named_digests(repo, &commit);
            (task, digests)
        })
        .collect()
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
