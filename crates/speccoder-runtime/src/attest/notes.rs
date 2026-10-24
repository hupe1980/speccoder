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

/// One writer of the notes ref at a time in this process.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Holds the notes ref for one writer across processes. `git notes add`
/// does not guard its ref update: concurrent writers silently lose each
/// other's notes (parallel lanes, two speccoder runs on one repository).
/// The lock is a file in git's common directory, created exclusively;
/// one older than a minute is a dead writer's and is taken over.
struct Held(Option<std::path::PathBuf>);

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn hold(repo: &Path) -> Held {
    let dir = Git::at(repo)
        .line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .map(std::path::PathBuf::from);
    let Some(file) = dir.map(|d| d.join("speccoder-notes.lock")) else {
        return Held(None);
    };
    for _ in 0..200 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(_) => return Held(Some(file)),
            Err(_) => {
                let stale = std::fs::metadata(&file)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > std::time::Duration::from_secs(60));
                if stale {
                    let _ = std::fs::remove_file(&file);
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
        }
    }
    // Five seconds without the lock: write anyway (the reader verifies).
    Held(None)
}

/// Writes `bundle` as `object`'s note, replacing speccoder's earlier one.
pub fn write(repo: &Path, object: &str, bundle: &str) -> Result<(), String> {
    let _one = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _held = hold(repo);
    let mut args = IDENTITY.to_vec();
    args.extend(["notes", "--ref=speccoder", "add", "-f", "-F", "-", object]);
    Git::at(repo)
        .run_with_stdin(&args, bundle.as_bytes())
        .map(|_| ())
        .map_err(|e| e.0)
}

/// Removes `object`'s note, if it has one.
pub fn remove(repo: &Path, object: &str) {
    let _one = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _held = hold(repo);
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
