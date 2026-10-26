//! What speccoder keeps of a bug: `<project state>/bugs/<unit>/<task>/` —
//! the bug (`bug.json`, the `bug` payload), its reproductions' contents
//! until they are confirmed (`repro/`), and the issue text and candidate
//! diff it was recorded with (`extra.json`). Nothing here is in the
//! format's files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::bug::BugReport;

/// The issue a bug came from, as fetched: context for the reproduction
/// author only (untrusted), never the judge's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueText {
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
}

/// What a bug was recorded with besides its triad.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extra {
    /// The issue's text.
    #[serde(default)]
    pub issue: Option<IssueText>,
    /// The candidate's diff.
    #[serde(default)]
    pub candidate_patch: Option<String>,
    /// Who recorded the bug.
    #[serde(default)]
    pub by: Option<String>,
    /// Through what.
    #[serde(default)]
    pub via: Option<speccoder_core::actions::Via>,
}

fn key(task: &str) -> String {
    task.replace([':', '/'], "-")
}

/// A bug's directory.
#[must_use]
pub fn dir(plane: &Path, unit: &str, task: &str) -> PathBuf {
    plane.join("bugs").join(unit).join(key(task))
}

/// The bug of `unit`'s `task`, if speccoder recorded one.
#[must_use]
pub fn load(plane: &Path, unit: &str, task: &str) -> Option<BugReport> {
    let text = std::fs::read_to_string(dir(plane, unit, task).join("bug.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The bug named `<unit>/<task>` (a unit may hold a `/`).
#[must_use]
pub fn find(plane: &Path, name: &str) -> Option<BugReport> {
    let (unit, task) = name.rsplit_once('/')?;
    load(plane, unit, task)
}

/// Every recorded bug.
#[must_use]
pub fn all(plane: &Path) -> Vec<BugReport> {
    fn walk(d: &Path, out: &mut Vec<BugReport>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            let file = p.join("bug.json");
            if let Some(b) = std::fs::read_to_string(&file)
                .ok()
                .and_then(|t| serde_json::from_str::<BugReport>(&t).ok())
            {
                out.push(b);
            } else if p.is_dir() {
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&plane.join("bugs"), &mut out);
    out.sort_by_key(BugReport::key);
    out
}

/// Saves the bug.
pub fn save(plane: &Path, bug: &BugReport) -> Result<(), String> {
    let d = dir(plane, &bug.unit, &bug.task);
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    let text = serde_json::to_string_pretty(bug).map_err(|e| e.to_string())?;
    let tmp = d.join("bug.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, d.join("bug.json")).map_err(|e| e.to_string())
}

/// What the bug was recorded with.
#[must_use]
pub fn extra(plane: &Path, unit: &str, task: &str) -> Extra {
    std::fs::read_to_string(dir(plane, unit, task).join("extra.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Saves what the bug was recorded with.
pub fn save_extra(plane: &Path, unit: &str, task: &str, extra: &Extra) -> Result<(), String> {
    let d = dir(plane, unit, task);
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    let text = serde_json::to_string_pretty(extra).map_err(|e| e.to_string())?;
    std::fs::write(d.join("extra.json"), text).map_err(|e| e.to_string())
}

/// Keeps the reproductions' contents (replacing earlier ones).
pub fn save_repro(
    plane: &Path,
    unit: &str,
    task: &str,
    files: &[(String, String)],
) -> Result<(), String> {
    let d = dir(plane, unit, task).join("repro");
    let _ = std::fs::remove_dir_all(&d);
    for (path, content) in files {
        let file = d.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

/// The reproductions' contents, for `paths`.
#[must_use]
pub fn repro(plane: &Path, unit: &str, task: &str, paths: &[String]) -> Vec<(String, String)> {
    let d = dir(plane, unit, task).join("repro");
    paths
        .iter()
        .filter_map(|p| {
            std::fs::read_to_string(d.join(p))
                .ok()
                .map(|c| (p.clone(), c))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use speccoder_core::bug::{BugHome, HomeKind, Outcome, Stage, Triad};

    use super::*;

    #[test]
    fn a_bug_reads_back_by_its_name() {
        let plane = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let triad = Triad::new("x", &["y".to_owned()], &[], crate::integrity::sha256_hex)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let bug = BugReport {
            home: BugHome {
                kind: HomeKind::NativeSpeckitBug,
                location: "b".to_owned(),
                reason: "r".to_owned(),
            },
            unit: "bugs/x".to_owned(),
            requirement: "bug:x".to_owned(),
            task: "bug:x".to_owned(),
            title: "x".to_owned(),
            triad,
            stage: Stage::Record,
            outcome: Outcome::Recorded,
            verdict: None,
            code: None,
            reproductions: Vec::new(),
            held_out: Vec::new(),
            held_out_sealed: None,
            confirm_digest: None,
            confirmation: None,
            rejections: Vec::new(),
            acceptance: None,
            pass_to_pass: None,
            revert: None,
            special_case: Vec::new(),
            tries: Vec::new(),
            max_tries: 3,
            handoff: None,
            issue: None,
            candidate: None,
            recorded_commit: None,
            commit: None,
            run_id: None,
        };
        save(plane.path(), &bug).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(find(plane.path(), "bugs/x/bug:x"), Some(bug.clone()));
        assert_eq!(all(plane.path()), vec![bug]);
        save_repro(
            plane.path(),
            "bugs/x",
            "bug:x",
            &[("tests/a.rs".to_owned(), "t".to_owned())],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            repro(plane.path(), "bugs/x", "bug:x", &["tests/a.rs".to_owned()]),
            [("tests/a.rs".to_owned(), "t".to_owned())]
        );
    }
}
