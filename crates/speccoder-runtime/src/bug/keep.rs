//! `speccoder bug keep <unit>/<task> --reason R`: a bug whose reproductions
//! passed on the untouched code on every try (`already_satisfied`) is kept
//! as a regression test — the reproductions the last try wrote are
//! committed, and the task is accepted as `attested`, the commit naming the
//! outcome. In a standing unit its task line is ticked; in the format's own
//! bug structure (`bug:<slug>`) nothing of the format's is touched and the
//! commit is the trailer-only task's evidence (FR-022, FR-002a).

use std::path::{Path, PathBuf};

use serde_json::json;
use speccoder_core::bug::{BugReport, Outcome, Stage};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::state;

fn refuse(status: CommandStatus, message: &str) -> CommandResult {
    CommandResult::new("bug", status, message)
}

/// [`keep_with`] with the environment's state directory.
#[must_use]
pub fn keep(path: &Path, key: &str, reason: &str, state: Option<&Path>) -> CommandResult {
    match state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
    {
        Some(s) => keep_with(path, key, reason, &s),
        None => refuse(
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR"),
    }
}

/// Keeps bug `key`'s reproductions as a regression test, against `state`.
#[must_use]
pub fn keep_with(path: &Path, key: &str, reason: &str, state: &Path) -> CommandResult {
    let reason = crate::controls::one_line(reason);
    if reason.is_empty() {
        return refuse(
            CommandStatus::UsageError,
            "a reason is needed: it is the acceptance's record",
        )
        .with_code("USAGE");
    }
    let opened = match crate::format::open(path, None) {
        Ok(o) => o,
        Err(r) => return refuse(r.status, &r.message),
    };
    let root = PathBuf::from(&opened.snapshot.root);
    let plane = crate::worktree::project_state_dir(state, &root);
    let Some(mut bug) = state::find(&plane, key) else {
        return refuse(CommandStatus::NotFound, &format!("no bug {key}"));
    };
    if bug.outcome != Outcome::AlreadySatisfied || bug.commit.is_some() {
        return refuse(
            CommandStatus::UsageError,
            &format!(
                "{key} is {}{}: only a bug whose reproductions already pass is kept as a regression test",
                bug.outcome.as_str(),
                if bug.commit.is_some() { " and kept" } else { "" }
            ),
        )
        .with_code("USAGE");
    }
    let paths: Vec<String> = bug.reproductions.iter().map(|r| r.file.clone()).collect();
    let files = state::repro(&plane, &bug.unit, &bug.task, &paths);
    if files.len() != paths.len() || !bug.reproductions.iter().any(|r| r.reproduces()) {
        return refuse(
            CommandStatus::NeedsHuman,
            &format!(
                "{key}'s reproductions are not all kept in the state directory: reproduce it again (speccoder bug run --task {key})"
            ),
        );
    }

    // The bug's worktree, clean.
    let wt = if bug.home.kind.native() {
        crate::commands::worktree_of(&opened.snapshot, &bug.unit, &bug.home.location, state)
    } else {
        crate::pipeline::workspace(path, Some(&bug.unit), state, &crate::RunOptions::default())
    };
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::carried("bug", &wt);
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::carried("bug", &wt);
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path");
    };
    let wt_root = PathBuf::from(&wt_path);
    let project = PathBuf::from(&project_dir);
    if crate::worktree::require_clean(&wt_root).is_err() {
        return refuse(
            CommandStatus::GitPrecondition,
            &format!(
                "the worktree {wt_path} has uncommitted changes; the next `speccoder run` sets them aside, or commit them yourself"
            ),
        )
        .with_code("WORKTREE-DIRTY");
    }
    let format = crate::format::at(&project);

    // A standing unit's task line, before anything is written.
    let task_line = if bug.home.kind.native() {
        None
    } else {
        let Some(snap) = format.inspect(&project, Some(&bug.unit)) else {
            return refuse(
                CommandStatus::NotAProject,
                format.messages().no_project_in_worktree,
            );
        };
        let Some(tasks) = snap.feature.as_ref().and_then(|f| f.tasks.clone()) else {
            return refuse(
                CommandStatus::NotFound,
                &format!("{} has no {}", bug.unit, format.messages().tasks_file),
            );
        };
        let Some(t) = tasks.tasks.iter().find(|t| t.id == bug.task).cloned() else {
            return refuse(
                CommandStatus::NotFound,
                &format!("no task {} in {}", bug.task, bug.unit),
            );
        };
        if t.done {
            return refuse(
                CommandStatus::TaskNotReady,
                &format!("{} is already done", bug.task),
            )
            .with_code("TASK-DONE");
        }
        let dir = snap
            .feature
            .as_ref()
            .map(|f| f.feature.dir.clone())
            .unwrap_or_default();
        Some((t, project.join(&tasks.path), dir))
    };

    // The reproductions, written as they were shown to pass.
    for (rel, content) in &files {
        let full = project.join(rel);
        match std::fs::read_to_string(&full) {
            Ok(now) if now == *content => continue,
            Ok(_) => {
                return refuse(
                    CommandStatus::GitPrecondition,
                    &format!(
                        "`{rel}` exists and differs from the reproduction; it is not overwritten"
                    ),
                );
            }
            Err(_) => {}
        }
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&full, content) {
            let _ = crate::git::Git::at(&wt_root).run(&["checkout", "--", "."]);
            return refuse(CommandStatus::Error, &format!("`{rel}`: {e}"));
        }
    }
    if let Some((t, file, _)) = &task_line
        && let Err(e) = format.tick(file, &bug.task, &t.line_hash)
    {
        let _ = crate::git::Git::at(&wt_root).run(&["clean", "-fdq"]);
        let _ = crate::git::Git::at(&wt_root).run(&["checkout", "--", "."]);
        return refuse(CommandStatus::Error, &format!("tick refused: {e}"));
    }
    let by = crate::controls::person(&project);
    let message = format!(
        "speccoder: {task} kept as a regression test by {by}\n\n{title}\n\n{reason}\n\nSpec-Task: {unit}/{task}\nVerdict: attested\nAccepted-By: {by}\nReason: {reason}\nBug: already_satisfied ({home}) triad={triad}\n",
        task = bug.task,
        title = bug.title,
        unit = bug.unit,
        home = bug.home.kind.as_str(),
        triad = bug.triad.digest,
    );
    let commit = match crate::commit_attempt(&wt_root, &message) {
        Ok(c) => c,
        Err(e) => return refuse(CommandStatus::Error, &format!("commit failed: {e}")),
    };
    let attested = match &task_line {
        Some((t, _, dir)) => crate::controls::attest_decision(
            &wt_root,
            &project,
            t,
            (&bug.unit, dir),
            (&commit, "attested"),
            (&by, &reason),
        ),
        None => attest_trailer_only(&wt_root, &project, &bug, &commit, (&by, &reason)),
    };
    bug.commit = Some(commit.clone());
    bug.stage = Stage::Done;
    bug.code = None;
    let saved = state::save(&plane, &bug);
    let mut said = format!(
        "{} kept as a regression test by {by} (attested, not verified): the reproductions are committed in {}{attested}",
        bug.key(),
        commit.chars().take(7).collect::<String>()
    );
    if let Err(e) = &saved {
        said.push_str(&format!("; the bug's state could not be updated: {e}"));
    }
    CommandResult::new(
        "bug",
        if saved.is_ok() {
            CommandStatus::Ok
        } else {
            CommandStatus::Error
        },
        &said,
    )
    .with_next(super::handoff::next(&bug, bug.outcome))
    .with_data(CommandData::Bug(Box::new(bug)))
}

/// The decision's attestation for a trailer-only task: anchored on the
/// triad, as the verifier reads such a task. A line for the message.
fn attest_trailer_only(
    repo: &Path,
    project: &Path,
    bug: &BugReport,
    commit: &str,
    decider: (&str, &str),
) -> String {
    let config = crate::verify::find_config(project)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let Ok(run_config) = crate::substrate::run_config::parse(&config) else {
        return String::new();
    };
    if !run_config.attest.enabled {
        return String::new();
    }
    let format = format!("{:?}", crate::format::at(project).kind()).to_lowercase();
    let story = format!(
        "A bug, in the reporter's words.\n\n{}",
        bug.triad.labelled()
    );
    let tick = crate::attest::Tick {
        repo,
        config: &config,
        attest: &run_config.attest,
        format: &format,
        feature: &bug.unit,
        feature_dir: &bug.home.location,
        task: &bug.task,
        title: &bug.title,
        line_hash: "",
        section: None,
        section_text: &story,
        requirements: &[],
        commit,
        verdict: "attested",
        sandbox: serde_json::Value::Null,
        executor: serde_json::Value::Null,
        model: serde_json::Value::Null,
        bug: Some((
            json!({
                "home": bug.home.kind.as_str(),
                "triad_sha256": bug.triad.digest,
                "outcome": bug.outcome.as_str(),
                "reproductions": bug.reproductions.iter().map(|r| json!({
                    "statement": r.statement, "baseline": r.baseline, "final": r.final_, "revert": r.revert,
                })).collect::<Vec<_>>(),
            }),
            json!({ "source": "kept_regression_test" }),
        )),
    };
    let a = crate::attest::record_decision(&tick, decider.0, decider.1);
    match (a["state"].as_str(), a["reason"].as_str()) {
        (Some("signed"), _) => format!(
            "; attested: signed ({})",
            a["signer"].as_str().unwrap_or("?")
        ),
        (Some(state), Some(why)) => format!("; attestation {state}: {why}"),
        (Some(state), None) => format!("; attestation {state}"),
        _ => String::new(),
    }
}
