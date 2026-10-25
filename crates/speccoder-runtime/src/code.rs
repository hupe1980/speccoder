//! A feature's code as a person reads it (F-081): what its branch changed
//! since it forked — by commit, by file — and its files. The revision is
//! never the caller's: it is the feature's branch, or `HEAD` before
//! speccoder worked on it; a file is read only when the tree lists it.

use std::path::Path;

use serde::Serialize;

use crate::git::Git;

/// One commit of the feature's branch.
#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    /// Full id.
    pub id: String,
    /// Subject line.
    pub subject: String,
    /// `Spec-Task` trailer (`feature/T005`), when a tick.
    pub task: Option<String>,
    /// `Verdict` trailer, when a tick.
    pub verdict: Option<String>,
    /// Author date, ISO 8601.
    pub date: String,
}

/// One changed file.
#[derive(Debug, Clone, Serialize)]
pub struct File {
    /// Path in the repository.
    pub path: String,
    /// Lines added (none for a binary file).
    pub add: Option<u64>,
    /// Lines removed.
    pub del: Option<u64>,
}

/// What the feature's branch changed since it forked.
#[derive(Debug, Clone, Serialize)]
pub struct Changes {
    /// The branch, when speccoder has worked on the feature.
    pub branch: Option<String>,
    /// Where it forked from the person's `HEAD`.
    pub base: Option<String>,
    /// Its commits, newest first.
    pub commits: Vec<Commit>,
    /// The files it changed.
    pub files: Vec<File>,
    /// The whole change as a patch (cut at `PATCH_LIMIT`).
    pub patch: String,
    /// Whether the patch was cut.
    pub cut: bool,
}

/// The largest patch sent whole.
pub const PATCH_LIMIT: usize = 2 * 1024 * 1024;
/// The largest file shown.
pub const FILE_LIMIT: usize = 512 * 1024;

/// The feature's branch, when it exists.
fn branch_of(root: &Path, feature: Option<&str>) -> Option<String> {
    let report = crate::status::report(root, feature);
    match report.data() {
        Some(speccoder_core::CommandData::Report(r)) => r.branch.clone(),
        _ => None,
    }
}

/// What the feature's branch changed since it forked from `HEAD`.
#[must_use]
pub fn changes(root: &Path, feature: Option<&str>) -> Changes {
    let empty = |branch| Changes {
        branch,
        base: None,
        commits: Vec::new(),
        files: Vec::new(),
        patch: String::new(),
        cut: false,
    };
    let Some(branch) = branch_of(root, feature) else {
        return empty(None);
    };
    let g = Git::at(root);
    let Ok(head) = g.line(&["rev-parse", "HEAD"]) else {
        return empty(Some(branch));
    };
    let Ok(base) = g.line(&["merge-base", &branch, &head]) else {
        return empty(Some(branch));
    };
    let range = format!("{base}..{branch}");
    let commits = g
        .run(&[
            "log",
            "--no-color",
            "--format=%H%x1f%s%x1f%(trailers:key=Spec-Task,valueonly,separator=%x2c)%x1f%(trailers:key=Verdict,valueonly,separator=%x2c)%x1f%aI%x1e",
            &range,
        ])
        .map(|b| {
            String::from_utf8_lossy(&b)
                .split('\x1e')
                .filter_map(|rec| {
                    let f: Vec<&str> = rec.trim().split('\x1f').collect();
                    let [id, subject, task, verdict, date] = f.as_slice() else { return None };
                    let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
                    Some(Commit {
                        id: (*id).to_owned(),
                        subject: (*subject).to_owned(),
                        task: some(task),
                        verdict: some(verdict),
                        date: (*date).to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let files = g
        .run(&["diff", "--numstat", "-M", &base, &branch])
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .filter_map(|l| {
                    let mut it = l.splitn(3, '\t');
                    let (a, d, p) = (it.next()?, it.next()?, it.next()?);
                    Some(File {
                        path: p.to_owned(),
                        add: a.parse().ok(),
                        del: d.parse().ok(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut patch = g
        .run(&["diff", "--no-color", "-M", &base, &branch])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let cut = patch.len() > PATCH_LIMIT;
    if cut {
        let mut at = PATCH_LIMIT;
        while !patch.is_char_boundary(at) {
            at -= 1;
        }
        patch.truncate(at);
    }
    Changes {
        branch: Some(branch),
        base: Some(base),
        commits,
        files,
        patch,
        cut,
    }
}

/// The revision whose files are read: the feature's branch, else `HEAD`.
fn revision(root: &Path, feature: Option<&str>) -> String {
    branch_of(root, feature).unwrap_or_else(|| "HEAD".to_owned())
}

/// The files at the feature's revision (at most 20 000).
#[must_use]
pub fn tree(root: &Path, feature: Option<&str>) -> (String, Vec<String>) {
    let rev = revision(root, feature);
    let files = Git::at(root)
        .run(&["ls-tree", "-r", "--name-only", "-z", &rev])
        .map(|b| {
            crate::git::nul_fields(&b)
                .into_iter()
                .take(20_000)
                .collect()
        })
        .unwrap_or_default();
    (rev, files)
}

/// One file at the feature's revision: its text, or why not (not in the
/// tree, binary, too large).
///
/// # Errors
///
/// Why the file is not shown.
pub fn file(root: &Path, feature: Option<&str>, path: &str) -> Result<String, String> {
    let (rev, files) = tree(root, feature);
    if !files.iter().any(|f| f == path) {
        return Err(format!("{path} is not in {rev}"));
    }
    let bytes = Git::at(root)
        .run(&["show", &format!("{rev}:{path}")])
        .map_err(|e| e.0)?;
    if bytes.len() > FILE_LIMIT {
        return Err(format!("{path} is larger than {} KB", FILE_LIMIT / 1024));
    }
    if bytes.contains(&0) {
        return Err(format!("{path} is binary"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
