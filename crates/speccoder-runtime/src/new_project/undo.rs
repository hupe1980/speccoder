//! Undo a project speccoder created — only while nothing but speccoder's
//! commit exists there. A new folder goes to the system Trash (never
//! deleted); in an existing repository speccoder's commit is reverted as
//! the person, history kept.

use std::path::Path;

use speccoder_core::actions::Via;
use speccoder_core::new_project::{ProjectMode, UndoOffer};
use speccoder_core::{CommandResult, CommandStatus};

use super::{COMMAND, Creation, creation, now, path_digest, shown, write_creation};
use crate::git::Git;

/// Paths a check's run leaves that do not count as a change.
fn build_output(path: &str) -> bool {
    speccoder_core::collection::is_build_or_dependency(path)
        || path
            .split('/')
            .any(|c| matches!(c, ".pytest_cache" | ".ruff_cache" | ".mypy_cache"))
}

/// What changed since speccoder's commit, at most 10 paths and a count;
/// empty when nothing did.
fn changes(root: &Path, c: &Creation) -> Result<Vec<String>, String> {
    let g = Git::at(root);
    let mut out = Vec::new();
    let head = g.head().map_err(|e| e.0)?;
    if head.as_deref() != c.commit.as_deref() {
        out.push(format!(
            "HEAD is {} now, not speccoder's commit",
            head.map_or_else(|| "unborn".to_owned(), |h| h.chars().take(7).collect())
        ));
    }
    // Untrimmed: a status line starts with a space (` M path`).
    let porcelain = String::from_utf8_lossy(
        &g.run(&[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
        ])
        .map_err(|e| e.0)?,
    )
    .into_owned();
    for line in porcelain.lines() {
        let path = line.get(3..).unwrap_or_default();
        // Build output, and what git ignored before speccoder ran, is no change.
        if line.starts_with("!!")
            && (build_output(path)
                || c.before
                    .ignored
                    .iter()
                    .any(|i| i == path || path.starts_with(i.as_str())))
        {
            continue;
        }
        out.push(path.to_owned());
    }
    if c.mode == ProjectMode::NewFolder {
        let refs = g
            .line(&["for-each-ref", "--format=%(refname)"])
            .map_err(|e| e.0)?;
        let refs: Vec<&str> = refs.lines().filter(|r| !r.is_empty()).collect();
        let heads = refs.iter().filter(|r| r.starts_with("refs/heads/")).count();
        if heads > 1 {
            out.push(format!("{heads} branches"));
        }
        for r in refs.iter().filter(|r| !r.starts_with("refs/heads/")) {
            out.push((*r).to_owned());
        }
    }
    Ok(out)
}

/// What Undo would do for the project at `root`, now.
#[must_use]
pub fn undo_offer(state: &Path, root: &Path) -> UndoOffer {
    let none = |reason: &str| UndoOffer {
        available: false,
        kind: "none".to_owned(),
        reason: Some(reason.to_owned()),
    };
    let Some(c) = creation(state, root) else {
        return none("speccoder did not create this project");
    };
    if c.undone_at.is_some() {
        return none("it was undone already");
    }
    if c.commit.is_none() {
        return none("speccoder has not committed it yet");
    }
    let kind = match c.mode {
        // Only a folder speccoder made goes to the Trash.
        ProjectMode::NewFolder if !c.before.made => {
            return none("speccoder did not make this folder: remove what it added yourself");
        }
        ProjectMode::NewFolder => "trash",
        ProjectMode::ExistingRepository => "revert",
    };
    match changes(root, &c) {
        Ok(ch) if ch.is_empty() => UndoOffer {
            available: true,
            kind: kind.to_owned(),
            reason: None,
        },
        Ok(ch) => UndoOffer {
            available: false,
            kind: kind.to_owned(),
            reason: Some(format!("changed since: {}", listed(&ch))),
        },
        Err(e) => none(&e),
    }
}

fn listed(paths: &[String]) -> String {
    let shown: Vec<&str> = paths.iter().take(10).map(String::as_str).collect();
    let more = paths.len().saturating_sub(10);
    if more > 0 {
        format!("{} and {more} more", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

/// Moves `path` to the system Trash.
fn to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

/// `speccoder new --undo`: the project at `root`, undone — its folder to
/// the Trash, or speccoder's commit reverted — when `commit` is still the
/// only thing that happened there.
#[must_use]
pub fn undo(
    root: &Path,
    commit: &str,
    via: Via,
    state: &Path,
    options: &crate::RunOptions,
) -> CommandResult {
    undo_with(root, commit, via, state, options, &to_trash)
}

/// [`undo`] with the Trash given (tests: a folder standing in for it).
#[must_use]
pub fn undo_with(
    root: &Path,
    commit: &str,
    via: Via,
    state: &Path,
    options: &crate::RunOptions,
    trash: &dyn Fn(&Path) -> Result<(), String>,
) -> CommandResult {
    let refused = |m: &str| {
        CommandResult::new(COMMAND, CommandStatus::GitPrecondition, m).with_code("NEW-UNDO-REFUSED")
    };
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let Some(mut c) = creation(state, &root) else {
        return refused(&format!(
            "speccoder did not create {}; nothing was moved",
            shown(&root)
        ));
    };
    let Some(made) = c.commit.clone() else {
        return refused("speccoder has not committed this project; nothing was moved");
    };
    if c.undone_at.is_some() {
        return refused("this project was undone already");
    }
    if !commit.is_empty() && !made.starts_with(commit) && !commit.starts_with(&made) {
        return refused(&format!(
            "{commit} is not speccoder's commit ({}); nothing was moved",
            made.chars().take(7).collect::<String>()
        ));
    }
    match changes(&root, &c) {
        Ok(ch) if ch.is_empty() => {}
        Ok(ch) => {
            return refused(&format!(
                "{} changed since speccoder's commit: {}; nothing was moved",
                shown(&root),
                listed(&ch)
            ));
        }
        Err(e) => return refused(&e),
    }
    let (how, message) = match c.mode {
        ProjectMode::NewFolder if !c.before.made => {
            return refused(&format!(
                "speccoder did not make {}: it moves to the Trash only a folder it made; nothing was moved",
                shown(&root)
            ));
        }
        ProjectMode::NewFolder => match trash(&root) {
            Ok(()) => ("trash", format!("moved {} to the Trash", shown(&root))),
            Err(e) => {
                return refused(&format!(
                    "{} cannot go to the Trash on this system ({e}); remove it yourself — speccoder never deletes it",
                    shown(&root)
                ));
            }
        },
        ProjectMode::ExistingRepository => {
            match Git::at(&root).run(&["revert", "--no-edit", &made]) {
                Ok(_) => (
                    "revert",
                    format!(
                        "reverted {} \"new project: {}\"",
                        made.chars().take(7).collect::<String>(),
                        c.name
                    ),
                ),
                Err(e) => return CommandResult::new(COMMAND, CommandStatus::GitPrecondition, &e.0),
            }
        }
    };
    c.undone_at = Some(now());
    let _ = write_creation(state, &c);
    crate::welcome::forget(state, &root);
    if let Some(p) = &options.progress {
        p.event(&crate::progress::Event::Project {
            step: crate::progress::ProjectEvent::Undone {
                path_digest: path_digest(&root),
                how: how.to_owned(),
                via: via.as_str().to_owned(),
            },
        });
    }
    CommandResult::new(COMMAND, CommandStatus::Ok, &message)
}
