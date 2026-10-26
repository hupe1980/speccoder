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

/// Whether `unit` and `task` name a place under the state's `bugs/`: each
/// part of the unit a plain name (no `..`, no root, no backslash), the task
/// one too — a name from an interface or a commit never becomes a path out.
#[must_use]
pub fn names_a_bug(unit: &str, task: &str) -> bool {
    let plain =
        |p: &str| !p.is_empty() && p != "." && p != ".." && !p.contains('\\') && !p.contains('\0');
    !unit.starts_with('/') && unit.split('/').all(plain) && plain(&key(task))
}

/// A bug's directory.
#[must_use]
pub fn dir(plane: &Path, unit: &str, task: &str) -> PathBuf {
    plane.join("bugs").join(unit).join(key(task))
}

/// [`dir`], refused for a name that is not one.
fn dir_checked(plane: &Path, unit: &str, task: &str) -> Result<PathBuf, String> {
    if names_a_bug(unit, task) {
        Ok(dir(plane, unit, task))
    } else {
        Err(format!("`{unit}/{task}` does not name a bug"))
    }
}

/// The bug of `unit`'s `task`, if speccoder recorded one.
#[must_use]
pub fn load(plane: &Path, unit: &str, task: &str) -> Option<BugReport> {
    let text =
        std::fs::read_to_string(dir_checked(plane, unit, task).ok()?.join("bug.json")).ok()?;
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
    let d = dir_checked(plane, &bug.unit, &bug.task)?;
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    let text = serde_json::to_string_pretty(bug).map_err(|e| e.to_string())?;
    let tmp = d.join("bug.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, d.join("bug.json")).map_err(|e| e.to_string())
}

/// What the bug was recorded with.
#[must_use]
pub fn extra(plane: &Path, unit: &str, task: &str) -> Extra {
    dir_checked(plane, unit, task)
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("extra.json")).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Saves what the bug was recorded with.
pub fn save_extra(plane: &Path, unit: &str, task: &str, extra: &Extra) -> Result<(), String> {
    let d = dir_checked(plane, unit, task)?;
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
    let d = dir_checked(plane, unit, task)?.join("repro");
    let _ = std::fs::remove_dir_all(&d);
    for (path, content) in files {
        if crate::tools::paths::normalise(path).ok().as_deref() != Some(path.as_str()) {
            return Err(format!("`{path}` is not a project-relative path"));
        }
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
    let Ok(d) = dir_checked(plane, unit, task).map(|d| d.join("repro")) else {
        return Vec::new();
    };
    paths
        .iter()
        .filter(|p| crate::tools::paths::normalise(p).ok().as_deref() == Some(p.as_str()))
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
            held_out_reason: None,
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
        assert_eq!(all(plane.path()), vec![bug.clone()]);
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
        // A name that would leave the state's `bugs/` is no bug.
        assert_eq!(find(plane.path(), "bugs/../../x/bug:x"), None);
        assert_eq!(find(plane.path(), "../T001"), None);
        let mut out = bug;
        out.unit = "bugs/../../outside".to_owned();
        assert!(save(plane.path(), &out).is_err());
        assert!(!plane.path().join("outside").exists());
        assert!(names_a_bug("007-bugfixes", "T001") && names_a_bug("bugs/x", "bug:x"));
        assert!(!names_a_bug("/etc", "T001") && !names_a_bug("a", ".."));
    }
}
