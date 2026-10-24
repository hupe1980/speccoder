//! `speccoder spec archive`: an OpenSpec change merged into the main specs,
//! by the person, diff first (D-060, research R7). Never called by the
//! engine, the model or a hosted skill.
//!
//! OpenSpec's `archive --yes` also skips its incomplete-task check, so the
//! first call is always without `--yes`: OpenSpec answers whether tasks are
//! open or a confirmation is needed, and only then does a person's yes (or
//! `--yes` given to speccoder) archive.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use speccoder_core::{ArchiveReport, CommandData, CommandResult, CommandStatus};

use crate::git::Git;
use crate::verify::process::LocalRunner;

/// How the archive is confirmed.
pub struct Options<'a> {
    /// The change (else the only active one).
    pub change: Option<&'a str>,
    /// Confirmed in advance (`--yes`).
    pub yes: bool,
    /// Tasks ticked without a verdict that the person accepts by name.
    pub allow_unverified: &'a [String],
    /// Asks the person, after the diff is shown; `None`: nobody to ask.
    pub confirm: Option<&'a dyn Fn(&ArchiveReport) -> bool>,
    /// The `openspec` program (default: from PATH).
    pub program: Option<PathBuf>,
}

const COMMAND: &str = "spec";

fn result(status: CommandStatus, message: &str, report: ArchiveReport) -> CommandResult {
    CommandResult::new(COMMAND, status, message).with_data(CommandData::Archive(Box::new(report)))
}

/// The last JSON object OpenSpec printed.
fn json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    serde_json::from_str(&text[start..]).ok()
}

/// OpenSpec's first error status, if any.
fn error(answer: &Value) -> Option<&Value> {
    answer["status"]
        .as_array()
        .and_then(|s| s.iter().find(|e| e["severity"] == "error"))
}

/// Tasks of the change ticked in `root`'s checkout without a speccoder
/// verdict commit (`Spec-Task: <change>/<id>` with a `Verdict:`) on HEAD.
fn unverified(root: &Path, change: &str, done: &[String]) -> Vec<String> {
    let log = Git::at(root)
        .run(&[
            "log",
            "--format=%(trailers:key=Spec-Task,valueonly)%x1f%(trailers:key=Verdict,valueonly)%x1e",
            "HEAD",
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let verdicts: BTreeSet<String> = log
        .split('\u{1e}')
        .filter_map(|rec| {
            let (task, verdict) = rec.split_once('\u{1f}')?;
            (!verdict.trim().is_empty()).then(|| task.trim().to_owned())
        })
        .collect();
    done.iter()
        .filter(|id| !verdicts.contains(&format!("{change}/{id}")))
        .cloned()
        .collect()
}

/// Archives the selected change of the OpenSpec project at `path`.
#[must_use]
pub fn archive(path: &Path, opts: &Options<'_>) -> CommandResult {
    let refuse = |status, message: &str| CommandResult::new(COMMAND, status, message);
    let opened = match crate::format::open_with(
        path,
        opts.change,
        Some(speccoder_core::spec::FormatKind::Openspec),
    ) {
        Ok(o) => o,
        Err(r) => return refuse(r.status, &r.message),
    };
    let snapshot = opened.snapshot;
    let Some(f) = snapshot.feature.as_ref() else {
        let why = snapshot
            .diagnostics
            .iter()
            .find(|d| d.code.starts_with("select."))
            .map_or("no change selected", |d| d.message.as_str());
        return refuse(
            CommandStatus::SelectionNeeded,
            &format!("{why}; name the change"),
        );
    };
    let id = f.feature.name.clone();
    let root = PathBuf::from(&snapshot.root);
    let mut report = ArchiveReport {
        change: id.clone(),
        stage: "probe".to_owned(),
        openspec_status: None,
        unverified_tasks: Vec::new(),
        diff: None,
        archived_as: None,
        totals: None,
        commit: None,
        confirmed_by: None,
    };
    let Some(program) = opts.program.clone().or_else(super::cli::find) else {
        return refuse(
            CommandStatus::NotFound,
            "archiving needs OpenSpec's CLI: install it with `npm i -g @fission-ai/openspec` (Node 20.19 or newer)",
        );
    };
    let git = Git::at(&root);
    let dirty = git
        .run(&["status", "--porcelain", "--", "openspec"])
        .map(|b| !b.is_empty())
        .unwrap_or(true);
    if dirty {
        return refuse(
            CommandStatus::GitPrecondition,
            "openspec/ has uncommitted changes; commit them first, so the archive commit holds only the archive",
        )
            .with_code("UNCOMMITTED-INTENT");
    }

    // Ticks without evidence are the person's to own, task by task (D-074).
    let done: Vec<String> = f
        .tasks
        .iter()
        .flat_map(|t| &t.tasks)
        .filter(|t| t.done)
        .map(|t| t.id.clone())
        .collect();
    let unproven = unverified(&root, &id, &done);
    let unaccepted: Vec<String> = unproven
        .iter()
        .filter(|t| !opts.allow_unverified.contains(t))
        .cloned()
        .collect();
    report.unverified_tasks = unproven.clone();
    if !unaccepted.is_empty() {
        report.stage = "unverified".to_owned();
        return result(
            CommandStatus::NeedsHuman,
            &format!(
                "{} ticked without a speccoder verdict: {}; run them, accept them (`speccoder task accept`), or name each with --allow-unverified",
                id,
                unaccepted.join(", ")
            ),
            report,
        );
    }

    // The probe: never `--yes`, so OpenSpec's own checks run.
    let runner = LocalRunner;
    let probe = match super::cli::run(&runner, &program, &root, &["archive", &id, "--json"]) {
        Ok(out) => json(&out),
        Err(e) => {
            report.stage = "failed".to_owned();
            return result(
                CommandStatus::RunFailed,
                &format!("openspec archive: {e}"),
                report,
            );
        }
    };
    let Some(probe) = probe else {
        report.stage = "failed".to_owned();
        return result(
            CommandStatus::RunFailed,
            "openspec archive printed no JSON",
            report,
        );
    };
    let finish = |mut report: ArchiveReport, answer: &Value| -> CommandResult {
        report.archived_as = answer["archive"]["archivedAs"].as_str().map(str::to_owned);
        report.totals = Some(answer["archive"]["totals"].clone()).filter(|t| !t.is_null());
        let who = crate::approvals::who();
        let mut message = format!(
            "archive {id}\n\nArchived-By: {who}\nSpeccoder-Archive: {}\n",
            report.archived_as.as_deref().unwrap_or(&id)
        );
        if !unproven.is_empty() {
            message.push_str(&format!("Archived-Unverified: {}\n", unproven.join(", ")));
        }
        let committed = git
            .run(&["add", "-A", "--", "openspec"])
            .and_then(|_| {
                git.run(&[
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-q",
                    "--no-verify",
                    "-m",
                    &message,
                ])
            })
            .and_then(|_| git.line(&["rev-parse", "HEAD"]));
        match committed {
            Ok(sha) => {
                report.commit = Some(sha);
                report.confirmed_by = Some(who);
                report.stage = "archived".to_owned();
                let m = format!(
                    "{id} archived as {} and committed",
                    report.archived_as.as_deref().unwrap_or(&id)
                );
                result(CommandStatus::Ok, &m, report)
            }
            Err(e) => {
                report.stage = "failed".to_owned();
                result(
                    CommandStatus::GitPrecondition,
                    &format!("OpenSpec archived {id}, but committing failed: {}", e.0),
                    report,
                )
            }
        }
    };
    if !probe["archive"].is_null() {
        // Nothing to confirm: OpenSpec archived on the probe.
        return finish(report, &probe);
    }
    let Some(status) = error(&probe) else {
        report.stage = "failed".to_owned();
        return result(
            CommandStatus::RunFailed,
            "openspec archive answered neither success nor an error",
            report,
        );
    };
    report.openspec_status = Some(status.clone());
    let code = status["code"].as_str().unwrap_or_default();
    let said = |s: &Value| {
        let fix = s["fix"]
            .as_str()
            .map(|f| format!(" — {f}"))
            .unwrap_or_default();
        format!("{}{fix}", s["message"].as_str().unwrap_or_default())
    };
    match code {
        "archive_tasks_incomplete" => {
            report.stage = "incomplete".to_owned();
            return result(
                CommandStatus::NeedsHuman,
                &format!("OpenSpec: {}", said(status)),
                report,
            );
        }
        "archive_confirmation_required" => {}
        _ => {
            report.stage = "failed".to_owned();
            return result(
                CommandStatus::RunFailed,
                &format!("OpenSpec refused ({code}): {}", said(status)),
                report,
            );
        }
    }

    // What archiving changes, then the person's yes.
    report.diff = super::cli::run(&runner, &program, &root, &["show", &id, "--diff"]).ok();
    report.stage = "awaiting_confirmation".to_owned();
    let yes = opts.yes || opts.confirm.is_some_and(|ask| ask(&report));
    if !yes {
        if opts.confirm.is_some() {
            report.stage = "declined".to_owned();
            return result(
                CommandStatus::Ok,
                &format!("{id} not archived; nothing changed"),
                report,
            );
        }
        return result(
            CommandStatus::NeedsHuman,
            &format!("archiving {id} rewrites the main specs and needs your yes: rerun with --yes"),
            report,
        );
    }
    let answer = match super::cli::run(
        &runner,
        &program,
        &root,
        &["archive", &id, "--yes", "--json"],
    )
    .ok()
    .and_then(|o| json(&o))
    {
        Some(a) => a,
        None => {
            report.stage = "failed".to_owned();
            return result(
                CommandStatus::RunFailed,
                "openspec archive --yes printed no JSON",
                report,
            );
        }
    };
    if answer["archive"].is_null() {
        report.openspec_status = error(&answer).cloned();
        report.stage = "failed".to_owned();
        let why = report
            .openspec_status
            .as_ref()
            .map(said)
            .unwrap_or_default();
        return result(
            CommandStatus::RunFailed,
            &format!("OpenSpec did not archive {id}: {why}"),
            report,
        );
    }
    finish(report, &answer)
}
