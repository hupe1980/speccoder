//! A person's controls over single tasks: `accept`, `skip`,
//! `nudge` — and `cost`, what the work used.
//!
//! What a person decides is recorded where speccoder's other durable facts
//! are: an acceptance is a commit on `speccoder/<feature>` (`Verdict:
//! attested` — a person's word, never shown as evidence); a skip is a ref
//! under `refs/speccoder/skipped/`; a nudge is a task note in the state
//! directory that rides in the attempt's journaled input.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ControlReport, CostReport, TaskCost, Usage,
};

use crate::git::Git;

pub use crate::attest::notes::SKIPPED;

const FS: char = '\u{1f}';
const RS: char = '\u{1e}';

/// One line: trailers and subjects never span lines.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The person, named as every approval names them: `<name> (asserted)`.
pub(crate) fn person(root: &Path) -> String {
    format!("{} (asserted)", crate::approvals::who_in(root))
}

/// A person's decision on a task as an attestation: one
/// `task-verification`, verdict `attested` or `skipped`, no summary. A
/// line for the message; empty when `[attest]` is off.
pub(crate) fn attest_decision(
    repo: &Path,
    project: &Path,
    task: &speccoder_core::snapshot::Task,
    unit: (&str, &str),
    record: (&str, &str),
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
    let adapter = crate::format::at(project);
    let format = format!("{:?}", adapter.kind()).to_lowercase();
    let requirements = adapter.requirement_ids(project, unit.1);
    let tick = crate::attest::Tick {
        repo,
        config: &config,
        attest: &run_config.attest,
        format: &format,
        feature: unit.0,
        feature_dir: unit.1,
        task: &task.id,
        title: &task.title,
        line_hash: &task.line_hash,
        section: task.story.as_deref(),
        section_text: "",
        requirements: &requirements,
        commit: record.0,
        verdict: record.1,
        sandbox: serde_json::Value::Null,
        executor: serde_json::Value::Null,
        model: serde_json::Value::Null,
        bug: None,
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

fn done(command: &str, report: ControlReport, message: &str) -> CommandResult {
    CommandResult::new(command, CommandStatus::Ok, message)
        .with_data(CommandData::Control(Box::new(report)))
}

/// The outcome a run's set-aside changes are saved with.
pub const SET_ASIDE: &str = "set_aside";

/// The files `commit` changes against its parent, repository-relative.
#[must_use]
pub fn changed_files(root: &Path, commit: &str) -> Vec<String> {
    Git::at(root)
        .run(&["diff", "--name-only", &format!("{commit}^"), commit])
        .map(|o| {
            String::from_utf8_lossy(&o)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a saved attempt holds changes a run set aside (0.1.0 saved
/// them as `interrupted`, with this reason).
#[must_use]
pub fn is_set_aside(a: &crate::attempts::Saved) -> bool {
    a.outcome == SET_ASIDE
        || (a.outcome == "interrupted"
            && a.reason
                .starts_with("the worktree had uncommitted changes when a run started"))
}

/// The newest changes set aside before `task`'s run.
fn set_aside_of(root: &Path, feature: &str, task: &str) -> Option<crate::attempts::Saved> {
    crate::attempts::list(root, feature)
        .into_iter()
        .find(|a| a.task == task && is_set_aside(a))
}

/// `speccoder task restore TASK`: the changes set aside before the task's
/// run, committed onto `speccoder/<feature>` as the person's own — merged
/// three ways when the branch moved since, refused when they conflict.
#[must_use]
pub fn restore(path: &Path, feature: Option<&str>, task: &str) -> CommandResult {
    let (feature, root) = match locate("restore", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let refuse = |status, m: String| CommandResult::new("restore", status, &m);
    let Some(saved) = set_aside_of(&root, &feature, task) else {
        return refuse(
            CommandStatus::NotFound,
            format!("nothing was set aside before {task}'s run"),
        );
    };
    let git = Git::at(&root);
    let branch = format!("refs/heads/speccoder/{feature}");
    let (Ok(tip), Ok(parent)) = (
        git.line(&["rev-parse", "--verify", &branch]),
        git.line(&["rev-parse", &format!("{}^", saved.commit)]),
    ) else {
        return refuse(
            CommandStatus::NotFound,
            format!("speccoder/{feature} or the saved changes are gone"),
        );
    };
    let tree = if tip == parent {
        git.line(&["rev-parse", &format!("{}^{{tree}}", saved.commit)])
    } else {
        git.line(&[
            "merge-tree",
            "--write-tree",
            &format!("--merge-base={parent}"),
            &tip,
            &saved.commit,
        ])
    };
    let Ok(tree) = tree else {
        return refuse(
            CommandStatus::NeedsHuman,
            format!(
                "the set-aside changes no longer apply cleanly to speccoder/{feature}; nothing was changed — see them with `git show {}`",
                saved.reference
            ),
        );
    };
    let files = changed_files(&root, &saved.commit);
    let message = format!(
        "keep the changes set aside before {task}\n\n{}\n\nKept-From: {}",
        files.join("\n"),
        saved.reference
    );
    let kept = match git.line(&[
        "-c",
        "commit.gpgsign=false",
        "commit-tree",
        tree.lines().next().unwrap_or_default(),
        "-p",
        &tip,
        "-m",
        &message,
    ]) {
        Ok(kept) => kept,
        // git says why (no identity configured, most often).
        Err(e) => {
            return refuse(
                CommandStatus::Error,
                format!("the commit could not be made: {e}"),
            );
        }
    };
    // The worktree follows its branch: clean, it fast-forwards; with
    // changes, nothing moves.
    let moved = match crate::worktree::worktree_for(&git, &format!("speccoder/{feature}")) {
        Ok(Some(wt)) => {
            // Untracked build output does not stand in the way; edits to
            // tracked files do, and are named.
            let dirty: Vec<String> = Git::at(&wt)
                .run(&["status", "--porcelain=v1", "--untracked-files=no"])
                .map(|o| {
                    String::from_utf8_lossy(&o)
                        .lines()
                        .map(|l| l.get(3..).unwrap_or(l).to_owned())
                        .collect()
                })
                .unwrap_or_default();
            if !dirty.is_empty() {
                return refuse(
                    CommandStatus::NeedsHuman,
                    format!(
                        "the worktree {} has uncommitted changes to {}; nothing was changed",
                        wt.display(),
                        dirty.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                            + if dirty.len() > 5 { ", …" } else { "" }
                    ),
                );
            }
            // An untracked copy identical to what is kept (a lockfile the
            // checks wrote again) gives way; a different one stops the merge.
            let wt_git = Git::at(&wt);
            for f in &files {
                let p = wt.join(f);
                let same = p.is_file()
                    && wt_git
                        .line(&["ls-files", "--error-unmatch", "--", f])
                        .is_err()
                    && wt_git.line(&["hash-object", "--", f]).ok()
                        == git.line(&["rev-parse", &format!("{kept}:{f}")]).ok();
                if same {
                    let _ = std::fs::remove_file(&p);
                }
            }
            wt_git.run(&["merge", "--ff-only", "-q", &kept]).map(|_| ())
        }
        _ => git.run(&["update-ref", &branch, &kept, &tip]).map(|_| ()),
    };
    if let Err(e) = moved {
        return refuse(
            CommandStatus::Error,
            format!("speccoder/{feature} could not move: {e}"),
        );
    }
    let _ = git.run(&["update-ref", "-d", &saved.reference]);
    done(
        "restore",
        ControlReport {
            feature: feature.clone(),
            task: task.to_owned(),
            action: "restored".to_owned(),
            text: Some(kept.chars().take(12).collect()),
            record: Some(saved.reference.clone()),
        },
        &format!(
            "kept the changes set aside before {task} ({} file(s)) as {} on speccoder/{feature}",
            files.len(),
            kept.chars().take(7).collect::<String>()
        ),
    )
}

/// `speccoder task drop TASK`: every change set aside before the task's
/// runs, discarded.
#[must_use]
pub fn drop_set_aside(path: &Path, feature: Option<&str>, task: &str) -> CommandResult {
    let (feature, root) = match locate("drop", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let saved: Vec<crate::attempts::Saved> = crate::attempts::list(&root, &feature)
        .into_iter()
        .filter(|a| a.task == task && is_set_aside(a))
        .collect();
    if saved.is_empty() {
        return CommandResult::new(
            "drop",
            CommandStatus::NotFound,
            &format!("nothing was set aside before {task}'s run"),
        );
    }
    let git = Git::at(&root);
    for a in &saved {
        if let Err(e) = git.run(&["update-ref", "-d", &a.reference]) {
            return CommandResult::new("drop", CommandStatus::Error, &e.to_string());
        }
    }
    let n = saved.len();
    done(
        "drop",
        ControlReport {
            feature,
            task: task.to_owned(),
            action: "dropped".to_owned(),
            text: Some(n.to_string()),
            record: saved.first().map(|a| a.reference.clone()),
        },
        &if n == 1 {
            format!("discarded the changes set aside before {task}")
        } else {
            format!("discarded the {n} sets of changes set aside before {task}")
        },
    )
}

/// The unit of the bug `slug` in the format's own bug structure, and the
/// project root: the bug must be there (its directory), and a `feature`
/// given must name that unit.
fn bug_unit_of(
    path: &Path,
    feature: Option<&str>,
    slug: &str,
) -> Result<(String, PathBuf), Box<CommandResult>> {
    let not_found = |m: String| Box::new(CommandResult::new("skip", CommandStatus::NotFound, &m));
    if !speccoder_core::bug::valid_slug(slug) {
        return Err(Box::new(
            CommandResult::new(
                "skip",
                CommandStatus::UsageError,
                &format!("`{slug}` is not a bug slug"),
            )
            .with_code("USAGE"),
        ));
    }
    let opened = crate::format::open(path, None)
        .map_err(|r| Box::new(CommandResult::new("skip", r.status, &r.message)))?;
    let root = PathBuf::from(&opened.snapshot.root);
    let format = crate::format::at(&root);
    let home = format
        .bug_unit(&root)
        .ok()
        .filter(|h| h.kind.native())
        .ok_or_else(|| {
            not_found(format!(
                "bug:{slug} names a bug of the format's own bug structure, which this project does not have"
            ))
        })?;
    let (dir, unit) = format
        .bug_dir(&home, slug)
        .filter(|(dir, _)| root.join(dir).is_dir())
        .ok_or_else(|| not_found(format!("no bug {slug} in {}", home.location)))?;
    if let Some(f) = feature
        && f != unit
        && f != dir
    {
        return Err(not_found(format!(
            "bug:{slug} is a task of {unit}, not of {f}"
        )));
    }
    Ok((unit, root))
}

/// The selected feature and the project root of the user's checkout.
fn locate(
    command: &str,
    path: &Path,
    feature: Option<&str>,
) -> Result<(String, PathBuf), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(command, &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    Ok((f.feature.name.clone(), PathBuf::from(&snapshot.root)))
}

/// The feature's open tasks as `report` sees them (branch first).
fn open_tasks(root: &Path, feature: &str) -> Vec<String> {
    match crate::status::report(root, Some(feature)).data() {
        Some(CommandData::Report(r)) => r
            .tasks
            .iter()
            .filter(|t| matches!(t.state.as_str(), "ready" | "blocked" | "skipped"))
            .map(|t| t.id.clone())
            .collect(),
        _ => Vec::new(),
    }
}

// ── accept ─────────────────────────────────────────────────────────────────

/// `speccoder task accept T --reason R`: a person attests the task is
/// done. speccoder ticks it and commits `Verdict: attested`.
pub fn accept_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    reason: &str,
    state: &Path,
) -> CommandResult {
    accept_in(path, feature, task, reason, state, false)
}

/// [`accept_with`], in the person's checkout when `in_place` (`accept
/// --in-place`): committed with the in-place trailers, then ticked.
pub fn accept_in(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    reason: &str,
    state: &Path,
    in_place: bool,
) -> CommandResult {
    let options = crate::RunOptions {
        in_place,
        ..crate::RunOptions::default()
    };
    crate::pipeline::in_session("accept", path, feature, state, &options, |o| {
        accept_task(path, feature, task, reason, state, o)
    })
}

fn accept_task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    reason: &str,
    state: &Path,
    options: &crate::RunOptions,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("accept", status, &m);
    let wt = crate::pipeline::workspace(path, feature, state, options);
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::carried("accept", &wt);
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::carried("accept", &wt);
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path".to_owned());
    };
    let wt_root = PathBuf::from(&wt_path);
    if options.session.is_none() && crate::worktree::require_clean(&wt_root).is_err() {
        return refuse(
            CommandStatus::GitPrecondition,
            format!(
                "the worktree {wt_path} has uncommitted changes; the next `speccoder run` sets them aside, or commit them yourself"
            ),
        )
        .with_code("WORKTREE-DIRTY");
    }
    let project = PathBuf::from(&project_dir);
    let format = crate::format::at(&project);
    let Some(snap) = format.inspect(&project, Some(&w.feature)) else {
        return refuse(
            CommandStatus::NotAProject,
            format.messages().no_project_in_worktree.to_owned(),
        );
    };
    let Some(tasks) = snap.feature.as_ref().and_then(|f| f.tasks.as_ref()) else {
        return refuse(
            CommandStatus::NotFound,
            format!("{} has no {}", w.feature, format.messages().tasks_file),
        );
    };
    let Some(t) = tasks.tasks.iter().find(|t| t.id == task) else {
        return refuse(
            CommandStatus::NotFound,
            format!("no task {task} in {}", w.feature),
        );
    };
    if t.done {
        return refuse(
            CommandStatus::TaskNotReady,
            format!("{task} is already done"),
        )
        .with_code("TASK-DONE");
    }
    // The attempt that stopped because speccoder could not tell whether
    // its tests ran was saved and undone: accepting it accepts that code,
    // so it is put back first — never a tick on the code without it.
    let mut restored = String::new();
    if let Some(saved) = crate::attempts::list(&wt_root, &w.feature)
        .into_iter()
        .find(|a| a.task == task && !is_set_aside(a))
        .filter(|a| a.outcome == "collection_unknown")
    {
        if options.session.is_some() {
            return refuse(
                CommandStatus::GitPrecondition,
                format!(
                    "{task}'s attempt ({}) was saved and undone; in place it cannot be put back for you: restore it with `git checkout {} -- .`, then accept",
                    saved.reference, saved.reference
                ),
            );
        }
        let tree = crate::git::Git::at(&wt_root)
            .line(&["rev-parse", &format!("{}^{{tree}}", saved.commit)])
            .map(|t| t.trim().to_owned());
        let put_back = tree.map_err(|e| e.0).and_then(|tree| {
            let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
            crate::worktree::restore(&wt_root, tmp.path(), &crate::worktree::Checkpoint(tree))
                .map_err(|e| e.0)
        });
        if let Err(e) = put_back {
            return refuse(
                CommandStatus::Error,
                format!(
                    "{task}'s saved attempt ({}) could not be put back: {e}",
                    saved.reference
                ),
            );
        }
        restored = format!(" with its saved attempt ({})", saved.reference);
    }
    let reason = one_line(reason);
    let by = person(&project);
    let message = format!(
        "speccoder: {task} accepted by {by}\n\n{}\n\n{reason}\n\nSpec-Task: {}/{task}\nVerdict: attested\nAccepted-By: {by}\nReason: {reason}\n",
        t.title, w.feature
    );
    let file = project.join(&tasks.path);
    let committed = match &options.session {
        None => format.tick(&file, task, &t.line_hash).and_then(|()| {
            crate::commit_attempt(&wt_root, &message).map_err(|e| format!("commit failed: {e}"))
        }),
        // In place: the checkout and the intent as the session found them.
        Some(session) => {
            let policy = crate::tools::policy::Policy::new(&[], &[]).rules(format.rules());
            let Ok(mut s) = session.lock() else {
                return refuse(
                    CommandStatus::Error,
                    "the in-place session is unavailable".to_owned(),
                );
            };
            if let Err(e) = s.check(&policy) {
                return e.result("accept");
            }
            let digest =
                crate::in_place::intent_digest(&project, &policy, format.messages().tasks_file);
            let done = s.commit_and_tick(
                &plane_dir_of(state, &project),
                &file,
                &t.line_hash,
                &message,
                &digest,
                || format.tick(&file, task, &t.line_hash),
            );
            match (done.commit, done.ticked) {
                (Some(c), true) => Ok(c),
                _ => Err(done.problems.join("; ")),
            }
        }
    };
    match committed {
        Ok(commit) => {
            let attested = attest_decision(
                &wt_root,
                &project,
                t,
                (
                    &w.feature,
                    &snap
                        .feature
                        .as_ref()
                        .map(|f| f.feature.dir.clone())
                        .unwrap_or_default(),
                ),
                (&commit, "attested"),
                (&by, &reason),
            );
            done(
                "accept",
                ControlReport {
                    feature: w.feature.clone(),
                    task: task.to_owned(),
                    action: "accepted".to_owned(),
                    text: Some(reason),
                    record: Some(commit),
                },
                &format!(
                    "{task} ticked as attested by {by}{restored} (not verified); `report` shows it as such{attested}"
                ),
            )
        }
        Err(e) => refuse(CommandStatus::Error, e),
    }
}

/// The state directory of the project at `project`.
fn plane_dir_of(state: &Path, project: &Path) -> PathBuf {
    crate::worktree::project_state_dir(state, project)
}

// ── skip ───────────────────────────────────────────────────────────────────

/// A task id as a ref name component: `bug:<slug>` (a trailer-only task
/// of the format's own bug structure) has a `:`, which git refuses in a ref.
fn ref_task(task: &str) -> String {
    task.replace('%', "%25").replace(':', "%3A")
}

/// [`ref_task`] read back.
fn task_of_ref(name: &str) -> String {
    name.replace("%3A", ":").replace("%25", "%")
}

/// Skipped tasks of `feature` with their reasons.
#[must_use]
pub fn skipped(root: &Path, feature: &str) -> BTreeMap<String, String> {
    let format = format!("--format=%(refname){FS}%(trailers:key=Reason,valueonly){RS}");
    let prefix = format!("{SKIPPED}/{feature}/");
    let out = Git::at(root)
        .run(&["for-each-ref", &format, &prefix])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    out.split(RS)
        .filter_map(|r| {
            let (name, reason) = r.trim_start_matches('\n').split_once(FS)?;
            Some((
                task_of_ref(name.strip_prefix(&prefix)?),
                reason.trim().to_owned(),
            ))
        })
        .collect()
}

/// `speccoder task skip T --reason R` (or `--undo`): the task is neither
/// done nor attempted; the feature loop passes it by, and what depends on it
/// stays blocked.
pub fn skip(path: &Path, feature: Option<&str>, task: &str, reason: Option<&str>) -> CommandResult {
    // A bug of the format's own bug structure (`bug:<slug>`): no feature
    // lists it as a task — its unit is found from its slug.
    let located = match task.strip_prefix("bug:") {
        Some(slug) => bug_unit_of(path, feature, slug),
        None => locate("skip", path, feature),
    };
    let (feature, root) = match located {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let git = Git::at(&root);
    let reference = format!("{SKIPPED}/{feature}/{}", ref_task(task));
    let report = |action: &str, text: Option<String>| ControlReport {
        feature: feature.clone(),
        task: task.to_owned(),
        action: action.to_owned(),
        text,
        record: Some(reference.clone()),
    };
    let Some(reason) = reason else {
        let _ = git.run(&["update-ref", "-d", &reference]);
        return done(
            "skip",
            report("unskipped", None),
            &format!("{task} is no longer skipped"),
        );
    };
    if !task.starts_with("bug:") && !open_tasks(&root, &feature).iter().any(|t| t == task) {
        return CommandResult::new(
            "skip",
            CommandStatus::NotFound,
            &format!("{task} is not an open task of {feature}"),
        );
    }
    if task.starts_with("bug:")
        && let Some(state) = crate::worktree::state_dir()
        && let Some(b) = crate::bug::state::load(
            &crate::worktree::project_state_dir(&state, &root),
            &feature,
            task,
        )
        && b.commit.is_some()
    {
        return CommandResult::new(
            "skip",
            CommandStatus::TaskNotReady,
            &format!("{task} is done already: its commit is recorded"),
        )
        .with_code("TASK-DONE");
    }
    let reason = one_line(reason);
    let by = person(&root);
    let made = git
        .line(&["rev-parse", "HEAD^{tree}"])
        .and_then(|tree| {
            git.line(&[
                "-c",
                "user.name=speccoder",
                "-c",
                "user.email=speccoder@users.noreply.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit-tree",
                &tree,
                "-m",
                &format!("speccoder: skip {task}\n\n{reason}\n\nSpec-Task: {feature}/{task}\nSkipped-By: {by}\nReason: {reason}\n"),
            ])
        })
        .and_then(|commit| git.run(&["update-ref", &reference, &commit]).map(|_| commit));
    match made {
        Ok(commit) => {
            let snap = crate::format::at(&root).inspect(&root, Some(&feature));
            let unit = snap.as_ref().and_then(|s| s.feature.as_ref());
            let attested = unit
                .and_then(|u| {
                    u.tasks
                        .as_ref()
                        .and_then(|t| t.tasks.iter().find(|x| x.id == task))
                        .map(|t| (u, t))
                })
                .map(|(u, t)| {
                    attest_decision(
                        &root,
                        &root,
                        t,
                        (&feature, &u.feature.dir),
                        (&commit, "skipped"),
                        (&by, &reason),
                    )
                })
                .unwrap_or_default();
            done(
                "skip",
                report("skipped", Some(reason)),
                &format!(
                    "{task} skipped: runs pass it by, and tasks depending on it stay blocked (`skip --task {task} --undo` to take it back){attested}"
                ),
            )
        }
        Err(e) => CommandResult::new("skip", CommandStatus::GitPrecondition, &e.0),
    }
}

// ── nudge ──────────────────────────────────────────────────────────────────

/// `speccoder task rule <task> --tests-right --reason …`: a
/// person rules that the tests of a held attempt are right — the flagged
/// alternative is not what the spec allows. The ruling is bound to the
/// alternative's digest; the next `run` of the task judges the held attempt
/// again from the journal and ticks it with the ruling recorded.
pub fn rule(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    reason: &str,
    variant: Option<&str>,
    state: &Path,
) -> CommandResult {
    let (feature, root) = match locate("rule", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    if reason.trim().is_empty() {
        return CommandResult::new("rule", CommandStatus::UsageError, "a ruling needs a reason");
    }
    let keep = crate::strength::dir(
        &crate::worktree::project_state_dir(state, &root),
        &feature,
        task,
    );
    let Some(held) = crate::strength::held(&keep) else {
        return CommandResult::new(
            "rule",
            CommandStatus::NotFound,
            &format!("{task} has no attempt held for a ruling on its tests"),
        )
        .with_code("NOT-FOUND");
    };
    let chosen: Vec<_> = held
        .findings
        .iter()
        .filter(|p| variant.is_none_or(|v| p.id == v))
        .collect();
    if chosen.is_empty() {
        return CommandResult::new(
            "rule",
            CommandStatus::NotFound,
            &format!(
                "{task} has no finding `{}` (held: {})",
                variant.unwrap_or_default(),
                held.findings
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .with_code("NOT-FOUND");
    }
    let by = crate::approvals::who_in(&root);
    for p in &chosen {
        let ruling = speccoder_core::strength::Ruling {
            digest: p.digest.clone(),
            scenario: p.scenario.clone(),
            by: by.clone(),
            reason: reason.trim().to_owned(),
        };
        if let Err(e) = crate::strength::add_ruling(&keep, ruling) {
            return CommandResult::new("rule", CommandStatus::Error, &e.to_string());
        }
    }
    done(
        "rule",
        ControlReport {
            feature,
            task: task.to_owned(),
            action: "ruled_tests_right".to_owned(),
            text: Some(reason.trim().to_owned()),
            record: None,
        },
        &format!(
            "ruled by {by}: {task}'s tests are right about {}; `speccoder run --task {task}` judges the held attempt again and ticks it",
            chosen
                .iter()
                .map(|p| format!("{} ({})", p.id, p.scenario))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
}

/// `speccoder task nudge T "note"` (or `--clear`): a person's note for the
/// task's next attempts, journaled with them.
pub fn nudge_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    note: Option<&str>,
    state: &Path,
) -> CommandResult {
    let (feature, root) = match locate("nudge", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let dir = crate::worktree::project_state_dir(state, &root);
    let mut decisions = match crate::approvals::Decisions::load(&dir) {
        Ok(d) => d,
        Err(e) => return CommandResult::new("nudge", CommandStatus::ConfigError, &e),
    };
    let entry = decisions
        .tasks
        .entry(format!("{feature}/{task}"))
        .or_default();
    let (action, message) = match note {
        Some(n) => {
            entry.nudges.push(n.trim().to_owned());
            (
                "nudged",
                format!("{task}'s next attempts will read: {}", n.trim()),
            )
        }
        None => {
            entry.nudges.clear();
            ("nudges_cleared", format!("{task} has no notes any more"))
        }
    };
    if let Err(e) = decisions.save(&dir) {
        return CommandResult::new("nudge", CommandStatus::Error, &e);
    }
    done(
        "nudge",
        ControlReport {
            feature,
            task: task.to_owned(),
            action: action.to_owned(),
            text: note.map(|n| n.trim().to_owned()),
            record: None,
        },
        &message,
    )
}

// ── cost ───────────────────────────────────────────────────────────────────

fn add(total: &mut Usage, u: &serde_json::Value) {
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    total.input_tokens += n("input_tokens");
    total.output_tokens += n("output_tokens");
    total.cache_read_tokens += n("cache_read_tokens");
    total.cache_write_tokens += n("cache_write_tokens");
    if let Some(c) = u["cost_usd"].as_f64() {
        total.usd = Some(total.usd.unwrap_or(0.0) + c);
    }
}

/// Prices `u` from `[model]`, unless its cost was reported (an external
/// agent's own figure, which the configured prices do not describe).
fn price(u: &mut Usage, prices: Option<crate::substrate::model_config::Prices>) {
    if let Some(p) = prices.filter(|_| u.usd.is_none()) {
        u.usd = Some(p.usd(
            u.input_tokens,
            u.output_tokens,
            u.cache_read_tokens,
            u.cache_write_tokens,
        ));
    }
}

/// `speccoder report --cost`: tokens per task — every attempt recorded in git, the
/// passing one and the saved ones — plus convergence, priced when `[model]`
/// states prices.
pub fn cost(path: &Path, feature: Option<&str>) -> CommandResult {
    let (feature, root) = match locate("cost", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let model = crate::verify::find_config(&root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| crate::substrate::model_config::parse(&t).ok().flatten());
    let prices = model.as_ref().and_then(|m| m.prices);

    let branch = format!("speccoder/{feature}");
    let format = format!(
        "--format=%s{FS}%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Usage,valueonly){RS}"
    );
    let log = Git::at(&root)
        .run(&["log", &format, &branch])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut per_task: BTreeMap<String, (usize, Usage)> = BTreeMap::new();
    let mut convergence = Usage::default();
    for record in log.split(RS) {
        let f: Vec<&str> = record.trim_start_matches('\n').split(FS).collect();
        let [subject, task, usage] = f[..] else {
            continue;
        };
        let Some(u) = crate::attempts::parse_usage(usage.trim()) else {
            continue;
        };
        if let Some(t) = task.trim().strip_prefix(&format!("{feature}/")) {
            let e = per_task.entry(t.to_owned()).or_default();
            e.0 += 1;
            add(&mut e.1, &u);
        } else if subject.starts_with("speccoder: converge round") {
            add(&mut convergence, &u);
        }
    }
    for saved in crate::attempts::list(&root, &feature) {
        if let Some(u) = saved.usage.as_ref() {
            let e = per_task.entry(saved.task.clone()).or_default();
            e.0 += 1;
            add(&mut e.1, u);
        }
    }

    let mut total = Usage::default();
    let mut tasks = Vec::new();
    for (task, (attempts, mut usage)) in per_task {
        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        total.cache_read_tokens += usage.cache_read_tokens;
        total.cache_write_tokens += usage.cache_write_tokens;
        price(&mut usage, prices);
        tasks.push(TaskCost {
            task,
            attempts,
            usage,
        });
    }
    total.input_tokens += convergence.input_tokens;
    total.output_tokens += convergence.output_tokens;
    total.cache_read_tokens += convergence.cache_read_tokens;
    total.cache_write_tokens += convergence.cache_write_tokens;
    price(&mut convergence, prices);
    let priced: Vec<f64> = tasks
        .iter()
        .map(|t| &t.usage)
        .chain([&convergence])
        .filter_map(|u| u.usd)
        .collect();
    total.usd = (!priced.is_empty()).then(|| priced.iter().sum());
    let message = match total.usd {
        Some(usd) => format!(
            "{} tokens in, {} out ({} from the cache): ${usd:.4}",
            total.input_tokens, total.output_tokens, total.cache_read_tokens
        ),
        None => format!(
            "{} tokens in, {} out ({} from the cache); add price_input_per_mtok and price_output_per_mtok to [model] for money",
            total.input_tokens, total.output_tokens, total.cache_read_tokens
        ),
    };
    CommandResult::new("cost", CommandStatus::Ok, &message).with_data(CommandData::Cost(Box::new(
        CostReport {
            feature,
            tasks,
            convergence,
            total,
            priced_model: prices.and(model.map(|m| format!("{}/{}", m.provider, m.model))),
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_input_is_priced_at_the_cache_rate() {
        let mut u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 600_000,
            cache_write_tokens: 0,
            usd: None,
        };
        price(
            &mut u,
            Some(crate::substrate::model_config::Prices {
                input: 1.0,
                output: 4.0,
                cache_read: 0.1,
                cache_write: 1.25,
                cache_write_1h: 2.0,
                one_hour: false,
            }),
        );
        // 0.4M fresh × $1 + 0.6M cached × $0.1 + 0.1M out × $4
        assert!(
            (u.usd.unwrap_or_default() - 0.86).abs() < 1e-9,
            "{:?}",
            u.usd
        );
    }
}

/// The record of hosted skills' runs (one JSON line each): tokens and,
/// when the model is priced, money.
const SKILL_USAGE: &str = "skills.jsonl";

/// Appends one hosted skill run's usage to the project's record.
pub(crate) fn record_skill_usage(
    plane: &Path,
    skill: &str,
    [input, output, cache_read, cache_write]: [u64; 4],
    prices: Option<crate::substrate::model_config::Prices>,
    calls: u64,
) {
    use std::io::Write as _;
    if input + output == 0 {
        return;
    }
    let usd = crate::progress::usd(prices, input, output, cache_read, cache_write);
    let line = serde_json::json!({
        "skill": skill,
        "input_tokens": input,
        "output_tokens": output,
        "usd": usd,
        "calls": calls,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(plane.join(SKILL_USAGE))
    {
        let _ = writeln!(f, "{line}");
    }
}

/// What running `skills` once each cost before, from the project's record
/// of hosted skill runs: per skill the median of its runs, summed. `None`
/// when a skill has never run here — no estimate is made up.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SkillCost {
    /// Tokens (input and output), the medians summed.
    pub tokens: u64,
    /// Model calls, the medians summed (`None`: a run on record did not count them).
    pub calls: Option<u64>,
    /// USD, when every run was priced.
    pub usd: Option<f64>,
    /// The fewest runs any of the skills has on record.
    pub runs: usize,
}

/// [`SkillCost`] of `skills` in the project at `root`, and the model.
#[must_use]
pub fn skill_cost(root: &Path, skills: &[String]) -> (Option<SkillCost>, Option<String>) {
    let model = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| crate::substrate::model_config::parse(&t).ok().flatten())
        .map(|m| m.model);
    let Some(state) = crate::worktree::state_dir() else {
        return (None, model);
    };
    let plane = crate::worktree::project_state_dir(&state, root);
    let text = std::fs::read_to_string(plane.join(SKILL_USAGE)).unwrap_or_default();
    (cost_from(&text, skills), model)
}

/// [`SkillCost`] of `skills` from the record's lines.
fn cost_from(text: &str, skills: &[String]) -> Option<SkillCost> {
    let rows: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let median = |mut v: Vec<f64>| -> f64 {
        v.sort_by(f64::total_cmp);
        let n = v.len();
        if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        }
    };
    let mut tokens = 0.0;
    let mut calls = Some(0.0);
    let mut usd = Some(0.0);
    let mut runs = usize::MAX;
    for skill in skills {
        let mine: Vec<&serde_json::Value> = rows
            .iter()
            .filter(|r| r["skill"] == skill.as_str())
            .collect();
        if mine.is_empty() {
            return None;
        }
        runs = runs.min(mine.len());
        #[allow(clippy::cast_precision_loss)]
        let t: Vec<f64> = mine
            .iter()
            .map(|r| {
                (r["input_tokens"].as_u64().unwrap_or(0) + r["output_tokens"].as_u64().unwrap_or(0))
                    as f64
            })
            .collect();
        tokens += median(t);
        #[allow(clippy::cast_precision_loss)]
        let counted: Vec<f64> = mine
            .iter()
            .filter_map(|r| r["calls"].as_u64())
            .map(|c| c as f64)
            .collect();
        calls = match calls {
            Some(sum) if counted.len() == mine.len() => Some(sum + median(counted)),
            _ => None,
        };
        let priced: Vec<f64> = mine.iter().filter_map(|r| r["usd"].as_f64()).collect();
        usd = match usd {
            Some(sum) if priced.len() == mine.len() => Some(sum + median(priced)),
            _ => None,
        };
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cost = SkillCost {
        tokens: tokens.round() as u64,
        calls: calls.map(|c| c.round() as u64),
        usd,
        runs,
    };
    Some(cost)
}

#[cfg(test)]
mod skill_cost_tests {
    use super::*;

    #[test]
    fn the_cost_line_is_the_median_of_runs_on_record_or_nothing() {
        let rows = [
            r#"{"skill":"speckit-specify","input_tokens":100,"output_tokens":10,"usd":0.01,"calls":3}"#,
            r#"{"skill":"speckit-specify","input_tokens":300,"output_tokens":30,"usd":0.03,"calls":5}"#,
            r#"{"skill":"speckit-specify","input_tokens":200,"output_tokens":20,"usd":0.02,"calls":4}"#,
            r#"{"skill":"speckit-plan","input_tokens":1000,"output_tokens":100,"usd":null}"#,
        ]
        .join("\n");
        let one =
            cost_from(&rows, &["speckit-specify".to_owned()]).unwrap_or_else(|| panic!("none"));
        assert_eq!((one.tokens, one.runs, one.calls), (220, 3, Some(4)));
        assert!((one.usd.unwrap_or_default() - 0.02).abs() < 1e-9);
        let two = cost_from(
            &rows,
            &["speckit-specify".to_owned(), "speckit-plan".to_owned()],
        )
        .unwrap_or_else(|| panic!("none"));
        // A run on record without its count: no count is made up.
        assert_eq!(
            (two.tokens, two.runs, two.usd, two.calls),
            (1320, 1, None, None)
        );
        assert_eq!(cost_from(&rows, &["speckit-tasks".to_owned()]), None);
    }
}
