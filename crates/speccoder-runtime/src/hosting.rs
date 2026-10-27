//! `speccoder spec`: list the installed Agent Skills, or run one.
//!
//! A hosted planning skill works in your checkout, because what it writes —
//! the feature's spec, plan and tasks — belongs there; the planning policy
//! lets it write only the format's artifacts and run the format's scripts,
//! and the changes are left uncommitted for you to review.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, SkillEntry, SkillRunReport, SkillsReport,
};

use crate::format::Planning;
use crate::{RunOptions, progress, redact, skills, substrate, tools, verify, worktree};

fn root_of(path: &Path) -> Result<PathBuf, Box<CommandResult>> {
    match crate::format::open(path, None) {
        Ok(opened) => Ok(PathBuf::from(&opened.snapshot.root)),
        Err(r) if r.unmarked => Err(Box::new(CommandResult::new(
            "skill",
            CommandStatus::NotAProject,
            crate::format::reference().messages().no_project,
        ))),
        Err(r) => Err(Box::new(CommandResult::new("skill", r.status, &r.message))),
    }
}

/// `speccoder spec`: the installed skills.
#[must_use]
pub fn list(path: &Path) -> CommandResult {
    let root = match root_of(path) {
        Ok(r) => r,
        Err(r) => return *r,
    };
    let format = crate::format::at(&root);
    let found = skills::discover_with(&root, format.planning());
    let message = if found.is_empty() {
        format.messages().no_skills.to_owned()
    } else {
        format!(
            "{} skill(s); run one with `speccoder spec <name> [arguments]`",
            found.len()
        )
    };
    CommandResult::new("skill", CommandStatus::Ok, &message).with_data(CommandData::Skills(
        Box::new(SkillsReport {
            skills: found
                .into_iter()
                .map(|s| SkillEntry {
                    name: s.name,
                    description: s.description,
                    path: s.path,
                    refused: s.refused,
                })
                .collect(),
        }),
    ))
}

/// `speccoder spec <name> [arguments]` against `state` with `provider`.
pub fn run_with(
    path: &Path,
    name: &str,
    arguments: &str,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    run_in(
        path, None, name, arguments, state, provider, secret, options,
    )
}

/// [`run_with`] for `feature` (`--feature`; else the format's own
/// selection): the skill's commands are told which unit is meant, and a
/// unit the creating skill makes becomes current.
///
/// The person's review records (`review.json`) are the person's: a skill
/// that changed one — through a command it was allowed to run — has the
/// change undone, and the result says so.
#[allow(clippy::too_many_arguments)]
pub fn run_in(
    path: &Path,
    feature: Option<&str>,
    name: &str,
    arguments: &str,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let reviews = crate::review::snapshot(path);
    let result = run_unguarded(
        path, feature, name, arguments, state, provider, secret, options, None,
    );
    let restored = crate::review::restore(&reviews);
    if restored.is_empty() {
        return result;
    }
    let message = format!(
        "{}; undone: the skill changed the person's review ({}) — only `speccoder review` writes it",
        result.message(),
        restored.join(", ")
    );
    let status = result.status();
    result.reworded(status, &message)
}

/// A format's own step (a bug's assess skill) under `rules` instead of the
/// format's planning rules: what it may write is narrowed to its own files.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scoped(
    path: &Path,
    name: &str,
    arguments: &str,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
    rules: &'static crate::format::Rules,
) -> CommandResult {
    let reviews = crate::review::snapshot(path);
    let result = run_unguarded(
        path,
        None,
        name,
        arguments,
        state,
        provider,
        secret,
        options,
        Some(rules),
    );
    let _ = crate::review::restore(&reviews);
    result
}

#[allow(clippy::too_many_arguments)]
fn run_unguarded(
    path: &Path,
    feature: Option<&str>,
    name: &str,
    arguments: &str,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
    scoped: Option<&'static crate::format::Rules>,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("skill", status, &redact(&m, secret));
    let s = match crate::setup(path, feature, state, "skill") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let root = PathBuf::from(&s.snapshot.root);
    let planning = s.format.planning();
    let installed = skills::discover_with(&root, planning);
    let Some(skill) = installed
        .iter()
        .find(|k| k.name == name || k.name == format!("{}{name}", planning.prefix))
    else {
        return refuse(
            CommandStatus::NotFound,
            format!("no skill `{name}` (see `speccoder spec`)"),
        );
    };
    if let Some(why) = &skill.refused {
        return refuse(
            CommandStatus::UsageError,
            format!("`{}` is not hosted: {why}", skill.name),
        );
    }
    if !skill.invocable {
        return refuse(
            CommandStatus::UsageError,
            format!("`{}` is marked user-invocable: false", skill.name),
        );
    }
    // Extension hooks around the format's core skills.
    let event = core_event(planning, &skill.name).map(str::to_owned);
    let mut hook_lines = Vec::new();
    if let Some(e) = &event {
        match run_hooks_in(
            &format!("before_{e}"),
            arguments,
            path,
            feature,
            state,
            &provider,
            secret,
            options,
        ) {
            Ok(lines) => hook_lines.extend(lines),
            Err(stop) => return *stop,
        }
    }
    let instruction = match skills::instruction(&root, skill, arguments) {
        Ok(i) => i,
        Err(e) => return refuse(CommandStatus::Error, e),
    };
    let config_text = verify::find_config(&root)
        .and_then(|d| std::fs::read_to_string(d.join(verify::config::FILE)).ok())
        .unwrap_or_default();
    let budget = match substrate::run_config::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };

    let tmp = s.plane_dir.join("tmp");
    let before = match worktree::checkpoint(&root, &tmp) {
        Ok(c) => c,
        Err(e) => {
            return refuse(
                CommandStatus::GitPrecondition,
                format!(
                    "a hosted skill needs a git repository with a commit: {}",
                    e.0
                ),
            );
        }
    };
    let live: Arc<dyn progress::Progress> = Arc::new(progress::Live::new(
        &s.plane_dir,
        s.model.prices,
        options.progress.clone(),
    ));
    live.event(&progress::Event::TaskStarted {
        feature: "skill".to_owned(),
        task: skill.name.clone(),
        title: skill.description.clone(),
    });
    let rt = match crate::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return refuse(CommandStatus::RunFailed, e),
    };
    // The format's scripts run confined to the checkout.
    let runner = match crate::sandbox::runner_at(&root) {
        Ok((r, _)) => r,
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };
    let canonical = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    // Which unit the skill works on. The format's scripts are told
    // (Spec Kit: SPECIFY_FEATURE_DIRECTORY), so they never depend on a
    // pointer file the model may not have written — except the skill that
    // creates a unit, which a variable would redirect into an old one.
    let short = skill
        .name
        .strip_prefix(planning.prefix)
        .unwrap_or(&skill.name);
    let creates = planning.creates.contains(&short);
    let selected = s.snapshot.feature.as_ref().map(|f| f.feature.dir.clone());
    let env: Vec<(String, String)> = match (planning.current_env, &selected) {
        (Some(var), Some(dir)) if !creates => {
            vec![(var.to_owned(), canonical.join(dir).display().to_string())]
        }
        _ => Vec::new(),
    };
    let server = match tools::WorkspaceTools::new(
        &root,
        tools::policy::Policy::new(&[], &[])
            .rules(scoped.unwrap_or_else(|| s.format.rules()))
            .authoring(),
    ) {
        Ok(t) => Arc::new(t.with_runner(runner).with_env(env)),
        Err(e) => return refuse(CommandStatus::Error, e.to_string()),
    };
    let mut input = substrate::host::input(
        &skill.name,
        &instruction,
        &canonical.display().to_string(),
        (budget.max_turns, budget.max_tool_calls),
        options.approver.is_some(),
    );
    if scoped.is_some() {
        input["settings"]["scope"] = serde_json::json!("bug");
    }
    let outcome = rt.block_on(async {
        let plane = substrate::Plane::open_with(
            &s.plane_dir,
            &s.model,
            Arc::clone(&provider),
            Some(server),
            substrate::Hooks {
                progress: Some(Arc::clone(&live)),
                format: Some(s.format),
                ..substrate::Hooks::default()
            },
        )?;
        // The installed skill and the person's arguments: trusted.
        let outcome = plane
            .skill(&skill.name, agentplane::core::Tainted::trusted(input))
            .await?;
        // Questions and calls are decided inline, recorded with who
        // answered; "later" ends the skill (a hosted run is not resumed).
        let context = crate::approvals::PresentContext {
            root: Some(canonical.clone()),
            ..crate::approvals::PresentContext::default()
        };
        plane
            .decide_inline(outcome, options.approver.as_deref(), &context)
            .await
    });
    // Always Allow answered in a hosted skill: its exact argv joins the
    // allow list, as anywhere else.
    let outcome = outcome.map(|i| {
        let _ = crate::actions::allow_always_inline(
            &root,
            &i.always.first().map_or_else(String::new, |a| {
                a.task.split('/').next().unwrap_or_default().to_owned()
            }),
            &i.always,
            state,
        );
        i.outcome
    });
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return refuse(CommandStatus::RunFailed, e.0),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or(serde_json::Value::Null);
    let succeeded = matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded);
    let status = if succeeded {
        output["status"].as_str().unwrap_or("failed").to_owned()
    } else {
        "failed".to_owned()
    };
    let changes: Vec<(String, String)> = worktree::checkpoint(&root, &tmp)
        .and_then(|after| worktree::diff(&root, &before, &after))
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            let kind = match c.kind {
                worktree::ChangeKind::Added => "added",
                worktree::ChangeKind::Modified => "modified",
                worktree::ChangeKind::Deleted => "deleted",
            };
            (c.path, kind.to_owned())
        })
        .collect();
    let report = SkillRunReport {
        skill: skill.name.clone(),
        run_id: outcome.run_id.to_string(),
        summary: output["report"]["summary"].as_str().map(str::to_owned),
        questions: output["report"]["questions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|q| q.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        changes,
        tool_calls: output["tool_calls"].as_array().cloned().unwrap_or_default(),
        pending: output.get("pending").cloned(),
        reason: output["reason"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| (!succeeded).then(|| format!("{:?}", outcome.status))),
        usage: output.get("usage").cloned(),
        status: status.clone(),
        hooks: Vec::new(),
        current: None,
    };
    // A unit the creating skill made becomes current, in the format's own
    // pointer, whether or not the model recorded it.
    let mut report = report;
    if creates && succeeded {
        let known: Vec<&str> = s.snapshot.features.iter().map(|f| f.dir.as_str()).collect();
        let made = s
            .format
            .inspect(&root, None)
            .map(|after| after.features)
            .unwrap_or_default()
            .into_iter()
            .rfind(|f| !known.contains(&f.dir.as_str()));
        if let Some(f) = made {
            match s.format.set_current(&root, Some(&f.dir)) {
                Ok(pointer) => {
                    report.current = Some(format!("{} (recorded in {pointer})", f.name));
                }
                Err(e) => report.current = Some(format!("{} (not recorded: {e})", f.name)),
            }
        }
    } else if let Some(f) = s.snapshot.feature.as_ref() {
        report.current = Some(format!(
            "{} ({})",
            f.feature.name,
            crate::current::reason_text(s.snapshot.selection.reason, s.format)
        ));
    }
    let tokens = |k: &str| {
        report
            .usage
            .as_ref()
            .and_then(|u| u[k].as_u64())
            .unwrap_or(0)
    };
    let (code, message) = match status.as_str() {
        "completed" => (
            CommandStatus::Ok,
            format!(
                "{} done: {} file(s) changed in your checkout, uncommitted — review them (git diff, git status)",
                skill.name,
                report.changes.len()
            ),
        ),
        "failed" => (
            CommandStatus::RunFailed,
            format!(
                "{} failed: {}",
                skill.name,
                report.reason.clone().unwrap_or_default()
            ),
        ),
        other => (
            CommandStatus::NeedsHuman,
            format!(
                "{} stopped ({other}): {}",
                skill.name,
                report
                    .reason
                    .clone()
                    .filter(|r| !r.is_empty())
                    .or_else(|| {
                        (!report.questions.is_empty())
                            .then(|| format!("open questions: {}", report.questions.join(" | ")))
                    })
                    .or_else(|| report
                        .pending
                        .as_ref()
                        .map(|p| p["reason"].as_str().unwrap_or_default().to_owned()))
                    .unwrap_or_default()
            ),
        ),
    };
    // What this skill's run cost, kept for the next time it is offered
    // (the New Feature sheet's cost line is read from these, never guessed).
    crate::controls::record_skill_usage(
        &s.plane_dir,
        &skill.name,
        [
            tokens("input_tokens"),
            tokens("output_tokens"),
            tokens("cache_read_tokens"),
            tokens("cache_write_tokens"),
        ],
        s.model.prices,
    );
    live.event(&progress::Event::TaskFinished {
        task: skill.name.clone(),
        outcome: status,
        message: redact(&message, secret),
        run: None,
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        cache_read_tokens: tokens("cache_read_tokens"),
        cache_write_tokens: tokens("cache_write_tokens"),
        usd: None,
        regression: None,
    });
    if code == CommandStatus::Ok
        && let Some(e) = &event
    {
        match run_hooks_in(
            &format!("after_{e}"),
            arguments,
            path,
            feature,
            state,
            &provider,
            secret,
            options,
        ) {
            Ok(lines) => hook_lines.extend(lines),
            Err(stop) => {
                report.hooks = hook_lines;
                return CommandResult::carried("skill", &stop)
                    .with_data(CommandData::Skill(Box::new(report)));
            }
        }
    }
    report.hooks = hook_lines;
    CommandResult::new("skill", code, &redact(&message, secret))
        .with_data(CommandData::Skill(Box::new(report)))
}

/// `speccoder spec <name>` with the configured model, for `feature`.
#[must_use]
pub fn run(
    path: &Path,
    feature: Option<&str>,
    name: &str,
    arguments: &str,
    options: &RunOptions,
) -> CommandResult {
    crate::with_driver("skill", path, None, |state, provider, key| {
        run_in(
            path,
            feature,
            name,
            arguments,
            state,
            Arc::clone(provider),
            Some(key),
            options,
        )
    })
}

/// Runs the mandatory hooks of `event` as hosted skills and lists the
/// optional ones. `Ok(lines)` to show; `Err` when a mandatory hook
/// did not complete — the caller stops with it.
pub fn run_hooks(
    event: &str,
    arguments: &str,
    path: &Path,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> Result<Vec<String>, Box<CommandResult>> {
    run_hooks_in(
        event, arguments, path, None, state, provider, secret, options,
    )
}

/// [`run_hooks`] for `feature`, which the hook skills work on.
#[allow(clippy::too_many_arguments)]
pub fn run_hooks_in(
    event: &str,
    arguments: &str,
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> Result<Vec<String>, Box<CommandResult>> {
    let root = root_of(path)?;
    let plan = crate::hooks::plan(&root, event)
        .map_err(|e| Box::new(CommandResult::new("hooks", CommandStatus::ConfigError, &e)))?;
    let mut lines: Vec<String> = Vec::new();
    for hook in &plan.run {
        let note = |text: String| {
            if let Some(p) = &options.progress {
                p.event(&progress::Event::Notice { text });
            }
        };
        note(format!(
            "{event} hook ({}): {}",
            hook.extension, hook.command
        ));
        let res = run_in(
            path,
            feature,
            &hook.skill(),
            arguments,
            state,
            Arc::clone(provider),
            secret,
            options,
        );
        if res.status() != CommandStatus::Ok {
            let status = if res.status() == CommandStatus::NotFound {
                CommandStatus::ConfigError
            } else {
                res.status()
            };
            return Err(Box::new(CommandResult::new(
                "hooks",
                status,
                &format!(
                    "mandatory {event} hook `{}` ({}) did not complete: {}",
                    hook.command,
                    hook.extension,
                    res.message()
                ),
            )));
        }
        lines.push(format!(
            "ran {event} hook {} ({})",
            hook.command, hook.extension
        ));
    }
    for hook in &plan.offer {
        let line = crate::hooks::offer_line(event, hook);
        if let Some(p) = &options.progress {
            p.event(&progress::Event::Notice { text: line.clone() });
        }
        lines.push(line);
    }
    for (hook, why) in &plan.skipped {
        lines.push(format!(
            "skipped {event} hook {} ({}): {why}",
            hook.command, hook.extension
        ));
    }
    Ok(lines)
}

/// The `before_` / `after_` event name for a hosted core skill.
fn core_event<'a>(planning: &Planning, skill: &'a str) -> Option<&'a str> {
    let short = skill.strip_prefix(planning.prefix).unwrap_or(skill);
    planning.core.contains(&short).then_some(short)
}

/// The current unit's planning files that are not committed — new or
/// changed files under the unit's directory. `speccoder run` reads committed
/// files only, so these are what stands between planning and a run.
#[must_use]
pub fn uncommitted(path: &Path, feature: Option<&str>) -> Vec<String> {
    let format = crate::format::at(path);
    let Some(snap) = format.open(path, feature) else {
        return Vec::new();
    };
    let Some(f) = snap.feature.as_ref() else {
        return Vec::new();
    };
    let (top, dir) = repo_dir(Path::new(&snap.root), &f.feature.dir);
    crate::git::Git::at(&top)
        .run(&[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--",
            &dir,
        ])
        .map(|out| {
            String::from_utf8_lossy(&out)
                .split('\0')
                .filter(|e| e.len() > 3)
                .map(|e| e[3..].to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Commits exactly `paths` (repository-root relative, inside the current
/// unit's directory) as the unit's planning: `spec(<unit>): <files>`.
/// Nothing else that is staged or changed goes into the commit.
#[must_use]
pub fn commit_planning(path: &Path, feature: Option<&str>, paths: &[String]) -> CommandResult {
    commit_planning_as(path, feature, paths, None)
}

/// [`commit_planning`] with the commit's subject (default: the files).
#[must_use]
pub fn commit_planning_as(
    path: &Path,
    feature: Option<&str>,
    paths: &[String],
    subject: Option<&str>,
) -> CommandResult {
    let refuse = |status, m: &str| CommandResult::new("commit", status, m);
    let format = crate::format::at(path);
    let Some(snap) = format.open(path, feature) else {
        return refuse(
            CommandStatus::NotAProject,
            &(format.messages().not_a_project)(path),
        );
    };
    let Some(f) = snap.feature.as_ref() else {
        return refuse(
            CommandStatus::SelectionNeeded,
            "no current feature to commit for",
        );
    };
    let (top, unit) = repo_dir(Path::new(&snap.root), &f.feature.dir);
    let dir = format!("{unit}/");
    if paths.is_empty() {
        return refuse(CommandStatus::NotFound, "nothing to commit");
    }
    // Inside the unit by its components: `<unit>/../../src/x` starts with
    // the unit's name and is not in it.
    let inside = |p: &str| {
        p.strip_prefix(&dir).is_some_and(|rest| {
            rest.split('/')
                .all(|c| !c.is_empty() && c != "." && c != "..")
        })
    };
    if let Some(p) = paths.iter().find(|p| !inside(p)) {
        return refuse(
            CommandStatus::UsageError,
            &format!(
                "`{p}` is not in {}; only planning files are committed here",
                f.feature.dir
            ),
        );
    }
    let git = crate::git::Git::at(&top);
    let names: Vec<&str> = paths
        .iter()
        .map(|p| p.rsplit('/').next().unwrap_or(p))
        .collect();
    let message = subject.map_or_else(
        || format!("spec({}): {}", f.feature.name, names.join(", ")),
        |t| format!("spec({}): {t}", f.feature.name),
    );
    let mut add = vec!["add", "--"];
    add.extend(paths.iter().map(String::as_str));
    let mut commit = vec!["commit", "-q", "-m", &message, "--"];
    commit.extend(paths.iter().map(String::as_str));
    match git.run(&add).and_then(|_| git.run(&commit)) {
        Ok(_) => {
            let id = git
                .line(&["rev-parse", "--short", "HEAD"])
                .unwrap_or_default();
            CommandResult::new(
                "commit",
                CommandStatus::Ok,
                &format!("committed {} file(s) as {id} \"{message}\"", paths.len()),
            )
        }
        Err(e) => refuse(
            CommandStatus::GitPrecondition,
            &format!("commit failed: {}", e.0),
        ),
    }
}

/// Commits exactly `paths` (repository-relative) in the repository of
/// `root` with `message` as given: a person-commanded write's own commit.
/// Returns the commit id as the message.
#[must_use]
pub fn commit_exactly(root: &Path, paths: &[String], message: &str) -> CommandResult {
    let refuse = |status, m: &str| CommandResult::new("commit", status, m);
    if paths.is_empty() {
        return refuse(CommandStatus::NotFound, "nothing to commit");
    }
    let (top, _) = repo_dir(root, "");
    let git = crate::git::Git::at(&top);
    let mut add = vec!["add", "--"];
    add.extend(paths.iter().map(String::as_str));
    let mut commit = vec!["commit", "-q", "-m", message, "--"];
    commit.extend(paths.iter().map(String::as_str));
    match git.run(&add).and_then(|_| git.run(&commit)) {
        Ok(_) => {
            let id = git.line(&["rev-parse", "HEAD"]).unwrap_or_default();
            CommandResult::new("commit", CommandStatus::Ok, &id)
        }
        Err(e) => refuse(
            CommandStatus::GitPrecondition,
            &format!("commit failed: {}", e.0),
        ),
    }
}

/// The repository's top level and `unit_dir` (relative to `root`) relative
/// to it: git's porcelain paths are repository-relative.
fn repo_dir(root: &Path, unit_dir: &str) -> (std::path::PathBuf, String) {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let top = crate::git::Git::at(root)
        .toplevel()
        .ok()
        .flatten()
        .map_or_else(|| root.to_path_buf(), |t| canonical(&t));
    let prefix = canonical(root)
        .strip_prefix(&top)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let dir = if prefix.is_empty() {
        unit_dir.to_owned()
    } else {
        format!("{prefix}/{unit_dir}")
    };
    (top, dir)
}
