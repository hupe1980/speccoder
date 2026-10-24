//! `speccoder skill`: list the installed Agent Skills, or run one.
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

/// `speccoder skill`: the installed skills.
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
            "{} skill(s); run one with `speccoder skill <name> [arguments]`",
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

/// `speccoder skill <name> [arguments]` against `state` with `provider`.
pub fn run_with(
    path: &Path,
    name: &str,
    arguments: &str,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("skill", status, &redact(&m, secret));
    let s = match crate::setup(path, None, state, "skill") {
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
            format!("no skill `{name}` (see `speccoder skill`)"),
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
        match run_hooks(
            &format!("before_{e}"),
            arguments,
            path,
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
        Err(e) => return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
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
    let server = match tools::WorkspaceTools::new(
        &root,
        tools::policy::Policy::new(&[], &[])
            .rules(s.format.rules())
            .authoring(),
    ) {
        Ok(t) => Arc::new(t.with_runner(runner)),
        Err(e) => return refuse(CommandStatus::Error, e.to_string()),
    };
    let canonical = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let input = substrate::host::input(
        &skill.name,
        &instruction,
        &canonical.display().to_string(),
        (budget.max_turns, budget.max_tool_calls),
        options.approver.is_some(),
    );
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
            .map(|(o, _)| o)
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
    };
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
    live.event(&progress::Event::TaskFinished {
        task: skill.name.clone(),
        outcome: status,
        message: redact(&message, secret),
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        cache_read_tokens: tokens("cache_read_tokens"),
        cache_write_tokens: tokens("cache_write_tokens"),
        usd: None,
    });
    let mut report = report;
    if code == CommandStatus::Ok
        && let Some(e) = &event
    {
        match run_hooks(
            &format!("after_{e}"),
            arguments,
            path,
            state,
            &provider,
            secret,
            options,
        ) {
            Ok(lines) => hook_lines.extend(lines),
            Err(stop) => {
                report.hooks = hook_lines;
                return CommandResult::new("skill", stop.status(), stop.message())
                    .with_data(CommandData::Skill(Box::new(report)));
            }
        }
    }
    report.hooks = hook_lines;
    CommandResult::new("skill", code, &redact(&message, secret))
        .with_data(CommandData::Skill(Box::new(report)))
}

/// `speccoder skill <name>` with the configured model.
#[must_use]
pub fn run(path: &Path, name: &str, arguments: &str, options: &RunOptions) -> CommandResult {
    crate::with_driver("skill", path, None, |state, provider, key| {
        run_with(
            path,
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
        let res = run_with(
            path,
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
