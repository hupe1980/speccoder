//! Attestations as git notes under `refs/notes/speccoder`: git's own place
//! for per-commit metadata, outside the commit it describes.

use std::path::Path;

use crate::git::Git;

/// The notes ref attestations live under.
pub const REF: &str = "refs/notes/speccoder";

const IDENTITY: [&str; 6] = [
    "-c",
    "user.name=speccoder",
    "-c",
    "user.email=speccoder@users.noreply.invalid",
    "-c",
    "commit.gpgsign=false",
];

/// Writes `bundle` as `object`'s note, replacing speccoder's earlier one.
pub fn write(repo: &Path, object: &str, bundle: &str) -> Result<(), String> {
    let mut args = IDENTITY.to_vec();
    args.extend(["notes", "--ref=speccoder", "add", "-f", "-F", "-", object]);
    Git::at(repo)
        .run_with_stdin(&args, bundle.as_bytes())
        .map(|_| ())
        .map_err(|e| e.0)
}

/// Removes `object`'s note, if it has one.
pub fn remove(repo: &Path, object: &str) {
    let mut args = IDENTITY.to_vec();
    args.extend([
        "notes",
        "--ref=speccoder",
        "remove",
        "--ignore-missing",
        object,
    ]);
    let _ = Git::at(repo).run(&args);
}

/// `object`'s note; `None` when it has none.
#[must_use]
pub fn read(repo: &Path, object: &str) -> Option<String> {
    Git::at(repo)
        .run(&["notes", "--ref=speccoder", "show", object])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Every object with a note.
#[must_use]
pub fn noted(repo: &Path) -> std::collections::BTreeSet<String> {
    Git::at(repo)
        .run(&["notes", "--ref=speccoder", "list"])
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
