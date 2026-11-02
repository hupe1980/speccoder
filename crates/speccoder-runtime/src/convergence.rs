//! `speccoder converge`: assess a
//! finished feature against its artifacts and append what remains.
//!
//! The model reads and reports; speccoder decides what becomes a task.
//! Actionable findings (`missing`, `partial`, `contradicts`) are appended as
//! one `## Phase N: Convergence` section behind a prefix guard and committed
//! on `speccoder/<feature>`; `unrequested` findings go to a person; a finding
//! that repeats an earlier convergence task is no progress and stops the loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use serde_json::Value;
use speccoder_core::{CommandData, CommandResult, CommandStatus, ConvergeReport};

use crate::{redact, substrate, tools, verify, worktree};

/// Severity order: the appended section lists the gravest first.
const SEVERITY: [&str; 4] = ["CRITICAL", "HIGH", "MEDIUM", "LOW"];

/// Words too common to tell two tasks apart.
const STOP: [&str; 24] = [
    "a", "an", "the", "and", "or", "so", "to", "of", "for", "in", "on", "is", "are", "be", "it",
    "its", "every", "each", "must", "only", "per", "that", "this", "with",
];

/// A task's distinguishing words: lowercase alphanumeric runs, stop words out.
fn words(text: &str) -> std::collections::BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !STOP.contains(w))
        .map(str::to_owned)
        .collect()
}

/// The share of two wordings' distinguishing words they have in common.
fn overlap(a: &str, b: &str) -> f64 {
    let (a, b) = (words(a), words(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let share = a.intersection(&b).count() as f64 / union as f64;
    share
}

/// Two wordings of one task: at least half of their distinguishing words
/// are shared (Jaccard ≥ 0.5). Measured on a real run, rewordings scored
/// 0.65 and distinct tasks ≤ 0.24.
#[must_use]
pub fn same_task(a: &str, b: &str) -> bool {
    let (a, b) = (words(a), words(b));
    let union = a.union(&b).count();
    union > 0 && a.intersection(&b).count() * 2 >= union
}

/// A scenario id as Spec Kit's converge cites it: `US1-3` → `US1/AC3`.
fn cite(id: &str) -> String {
    match id.split_once('-') {
        Some((story, n))
            if story.starts_with("US")
                && !n.is_empty()
                && n.chars().all(|c| c.is_ascii_digit()) =>
        {
            format!("{story}/AC{n}")
        }
        _ => format!("(scenarios {id})"),
    }
}

/// The scenario ids a finding cites: `US1/AC3` → `US1-3`, and ids written
/// as they are (`US1-3`).
fn scenario_cites(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '/' || c == '-'))
        .filter_map(|w| {
            let w = w.trim_matches('-').to_ascii_uppercase();
            let (story, n) = w.split_once("/AC").or_else(|| w.split_once('-'))?;
            let num = story.strip_prefix("US")?;
            (!num.is_empty()
                && num.chars().all(|c| c.is_ascii_digit())
                && !n.is_empty()
                && n.chars().all(|c| c.is_ascii_digit()))
            .then(|| format!("{story}-{n}"))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The task line speccoder appends for a finding (Spec Kit's format).
#[must_use]
pub fn task_line(finding: &Value) -> String {
    let text = |k: &str| finding[k].as_str().unwrap_or_default().trim().to_owned();
    format!(
        "{} per {} ({})",
        text("task").trim_end_matches('.'),
        text("source_ref"),
        text("gap_type")
    )
}

/// Assesses the feature in its worktree with `provider` and appends the
/// actionable findings. Every task must be ticked first. Spec Kit's
/// `before_converge` / `after_converge` hooks run around it.
pub fn converge_with(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
) -> CommandResult {
    converge_with_options(
        path,
        feature,
        state,
        provider,
        secret,
        &crate::RunOptions::default(),
    )
}

/// [`converge_with`] with explicit options: `in_place` assesses in the
/// person's checkout on `speccoder/<feature>` (`converge --in-place`).
pub fn converge_with_options(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &crate::RunOptions,
) -> CommandResult {
    if options.in_place && options.session.is_none() {
        return crate::pipeline::in_session("converge", path, feature, state, options, |o| {
            converge_with_options(path, feature, state, Arc::clone(&provider), secret, o)
        });
    }
    let before = match crate::hosting::run_hooks(
        "before_converge",
        "",
        path,
        state,
        &provider,
        secret,
        options,
    ) {
        Ok(lines) => lines,
        Err(stop) => return CommandResult::carried("converge", &stop),
    };
    let result = assess(path, feature, state, Arc::clone(&provider), secret, options);
    let Some(CommandData::Converge(report)) = result.data() else {
        return result;
    };
    let mut report = (**report).clone();
    report.hooks = before;
    let (status, message) = (result.status(), result.message().to_owned());
    if status == CommandStatus::Ok {
        match crate::hosting::run_hooks(
            "after_converge",
            "",
            path,
            state,
            &provider,
            secret,
            options,
        ) {
            Ok(lines) => report.hooks.extend(lines),
            Err(stop) => {
                return CommandResult::carried("converge", &stop)
                    .with_data(CommandData::Converge(Box::new(report)));
            }
        }
    }
    CommandResult::new("converge", status, &message)
        .with_data(CommandData::Converge(Box::new(report)))
}

/// Each requirement reworded, added or removed since a tick that cites it,
/// as `T004 FR-001: was "…"; now "…"`: what the assessment must
/// look at again.
fn changed_requirements(project: &Path, feature: &str) -> Vec<String> {
    let report = crate::status::report(project, Some(feature));
    let Some(CommandData::Report(r)) = report.data() else {
        return Vec::new();
    };
    r.tasks
        .iter()
        .filter_map(|t| t.drift.as_ref().map(|d| (t, d)))
        .flat_map(|(t, d)| {
            d.requirements.iter().map(move |q| {
                let quote = |l: &Option<String>| {
                    l.as_deref()
                        .map_or_else(|| "(absent)".to_owned(), |l| format!("\"{}\"", l.trim()))
                };
                format!(
                    "- {} {}: was {}; now {}",
                    t.id,
                    q.id,
                    quote(&q.then),
                    quote(&q.now)
                )
            })
        })
        .collect()
}

/// The assessment itself.
fn assess(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &crate::RunOptions,
) -> CommandResult {
    let refuse = |status: CommandStatus, m: String| {
        CommandResult::new("converge", status, &redact(&m, secret))
    };
    let s = match crate::setup(path, feature, state, "converge") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let Some(f) = s.snapshot.feature.as_ref() else {
        return refuse(
            CommandStatus::SelectionNeeded,
            "no feature selected; pass --feature".to_owned(),
        );
    };
    let feature_name = f.feature.name.clone();

    // The worktree, where the feature's progress lives.
    let wt = crate::pipeline::workspace(path, Some(&feature_name), state, options);
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::carried("converge", &wt);
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::carried("converge", &wt).with_data(CommandData::Worktree(w.clone()));
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path".to_owned());
    };
    let wt_root = PathBuf::from(&wt_path);
    let project = PathBuf::from(&project_dir);
    if options.session.is_none()
        && let Err(r) = worktree::require_clean(&wt_root)
    {
        return refuse(
            CommandStatus::GitPrecondition,
            format!(
                "the worktree has uncommitted changes ({}); a failed attempt's work? Commit or undo it in {wt_path}",
                r.paths
                    .iter()
                    .map(|(p, _)| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .with_code("WORKTREE-DIRTY");
    }

    let format = crate::format::at(&project);
    let Some(snap) = format.inspect(&project, Some(&feature_name)) else {
        return refuse(
            CommandStatus::NotAProject,
            format.messages().no_project_in_worktree.to_owned(),
        );
    };
    let Some(tasks) = snap.feature.as_ref().and_then(|f| f.tasks.as_ref()) else {
        return refuse(
            CommandStatus::NotFound,
            format!(
                "{feature_name} has no {}; {}",
                format.messages().tasks_file,
                format.messages().make_tasks
            ),
        );
    };
    let open: Vec<&str> = tasks
        .tasks
        .iter()
        .filter(|t| !t.done)
        .map(|t| t.id.as_str())
        .collect();
    if !open.is_empty() {
        return refuse(
            CommandStatus::TaskNotReady,
            format!(
                "open tasks: {}; converge assesses a finished feature — run it first",
                open.join(", ")
            ),
        );
    }
    let tasks_path = project.join(&tasks.path);
    let feature_dir = tasks_path
        .parent()
        .map_or_else(|| project.clone(), Path::to_path_buf);
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    let tasks_text = read(&tasks_path);
    let tasks_hash = crate::integrity::sha256_hex(tasks_text.as_bytes());

    let Some(config_root) = verify::find_config(&project) else {
        return refuse(
            CommandStatus::ConfigError,
            "the worktree has no speccoder.toml (is it committed?)".to_owned(),
        )
        .with_code("CONFIG-MISSING");
    };
    let config_text =
        std::fs::read_to_string(config_root.join(verify::config::FILE)).unwrap_or_default();
    let run_config = match substrate::run_config::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };

    let is_convergence = |phase: &str| format.is_convergence(phase);
    let done_rounds = tasks
        .phases
        .iter()
        .filter(|p| is_convergence(&p.title))
        .count();
    let round = u32::try_from(done_rounds)
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let mut report = ConvergeReport {
        feature: feature_name.clone(),
        run_id: None,
        round,
        outcome: String::new(),
        findings: Vec::new(),
        appended: Vec::new(),
        unrequested: Vec::new(),
        repeated: Vec::new(),
        dropped: Vec::new(),
        stories: Vec::new(),
        commit: None,
        usage: None,
        tool_calls: Vec::new(),
        hooks: Vec::new(),
    };
    let done = |report: ConvergeReport, status: CommandStatus, message: String| {
        CommandResult::new("converge", status, &redact(&message, secret))
            .with_data(CommandData::Converge(Box::new(report)))
    };
    if round > run_config.max_converge_rounds {
        report.outcome = "round_limit".to_owned();
        return done(
            report,
            CommandStatus::NeedsHuman,
            format!(
                "{done_rounds} convergence rounds already ([converge] max_rounds = {}); a person decides what remains",
                run_config.max_converge_rounds
            ),
        );
    }

    // Evidence first: every finished story's oracle, written when it has
    // none, run on the branch. Its red scenarios are this round's gaps,
    // named without asking the model; what holds is no gap.
    // With acceptance tests off there are no oracles to read.
    let stories: Vec<String> = if !run_config.acceptance {
        Vec::new()
    } else if format.kind() == speccoder_core::spec::FormatKind::Openspec {
        vec![feature_name.clone()]
    } else {
        let mut keys: Vec<String> = tasks.tasks.iter().filter_map(|t| t.story.clone()).collect();
        keys.sort();
        keys.dedup();
        keys
    };
    let mut holds = std::collections::BTreeSet::new();
    let mut red = std::collections::BTreeSet::new();
    let mut evidence: Vec<Value> = Vec::new();
    for key in &stories {
        let checked = crate::pipeline::story::checked(
            path,
            Some(&feature_name),
            key,
            state,
            &provider,
            options,
        );
        let c = match checked {
            Ok(c) if c.finished => c,
            Ok(_) => continue,
            Err(r) => {
                report
                    .stories
                    .push(format!("{key}: no oracle — {}", r.message()));
                continue;
            }
        };
        let green = c
            .scenarios
            .iter()
            .filter(|s| c.states.get(&s.id) == Some(&speccoder_core::story::ScenarioState::Green))
            .count();
        report.usage = crate::pipeline::commit::add_usage(report.usage.as_ref(), c.usage.as_ref());
        report.stories.push(format!(
            "{key}: {green} of {} hold{}",
            c.scenarios.len(),
            if c.wrote { " (oracle written now)" } else { "" }
        ));
        for sc in &c.scenarios {
            match c.states.get(&sc.id) {
                Some(speccoder_core::story::ScenarioState::Green) => {
                    holds.insert(sc.id.clone());
                }
                Some(speccoder_core::story::ScenarioState::Red)
                    if !c.untestable.contains(&sc.id) =>
                {
                    red.insert(sc.id.clone());
                    evidence.push(serde_json::json!({
                        "task": format!("Make {} hold through the story's interface", sc.id),
                        "source_ref": cite(&sc.id),
                        "gap_type": "scenario",
                        "severity": "HIGH",
                        "source": "oracle",
                        "evidence": sc.text,
                    }));
                }
                _ => {}
            }
        }
    }

    // The assessment: read-only tools, everything it depends on journaled.
    let tmp = s.plane_dir.join("tmp");
    let files: Vec<String> = worktree::checkpoint(&wt_root, &tmp)
        .and_then(|c| worktree::files(&wt_root, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let [constitution, spec, plan, _] = format.convergence_texts(&project, &feature_dir);
    let changed = changed_requirements(&project, &feature_name);
    let input = substrate::converge::input(
        &feature_name,
        [&constitution, &spec, &plan, &tasks_text],
        &files,
        &changed,
        run_config.max_turns,
        run_config.max_tool_calls,
    );
    let rt = match crate::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return refuse(CommandStatus::RunFailed, e),
    };
    let runner = match crate::sandbox::runner_at(&wt_root) {
        Ok((r, _)) => r,
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };
    let policy = tools::policy::Policy::new(&[], &[]).rules(format.rules());
    // In place: the checkout as the session found it, and the model's view
    // of it confined as for an attempt.
    let (policy, runner) = match options.session.as_ref().and_then(|s| s.lock().ok()) {
        Some(s) => {
            if let Err(e) = s.check(&policy) {
                return e.result("converge");
            }
            let (policy, runner, _) = s.confine(policy, runner);
            (policy, runner)
        }
        None => (policy, runner),
    };
    let server = match tools::WorkspaceTools::new(&wt_root, policy) {
        Ok(t) => Arc::new(t.with_runner(runner)),
        Err(e) => return refuse(CommandStatus::Error, e.to_string()),
    };
    let outcome = rt.block_on(async {
        let plane = substrate::Plane::open_with(
            &s.plane_dir,
            &s.model,
            provider,
            Some(server),
            substrate::Hooks {
                format: Some(format),
                ..substrate::Hooks::default()
            },
        )?;
        plane.converge(&feature_name, input).await
    });
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return refuse(CommandStatus::RunFailed, e.0),
    };
    report.run_id = Some(outcome.run_id.to_string());
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or(Value::Null);
    report.usage = crate::pipeline::commit::add_usage(report.usage.as_ref(), output.get("usage"));
    report.tool_calls = output["tool_calls"].as_array().cloned().unwrap_or_default();
    if output["status"] != "completed" {
        report.outcome = "failed".to_owned();
        let why = output["reason"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{:?}", outcome.status));
        return done(
            report,
            CommandStatus::NeedsHuman,
            format!("the assessment did not finish: {why}"),
        );
    }
    report.findings = output["report"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // speccoder decides: unrequested → a person; repeats → no progress.
    // An earlier convergence task's text without its ` per <ref> (<gap>)` tail.
    let earlier: Vec<String> = tasks
        .tasks
        .iter()
        .filter(|t| t.phase.as_deref().is_some_and(is_convergence))
        .map(|t| {
            t.title
                .rsplit_once(" per ")
                .map_or(t.title.clone(), |(task, _)| task.to_owned())
        })
        .collect();
    // Done tasks of the plan; an earlier round's own repeat is "no progress".
    let ticked: Vec<&speccoder_core::snapshot::Task> = tasks
        .tasks
        .iter()
        .filter(|t| t.done && !t.phase.as_deref().is_some_and(is_convergence))
        .collect();
    let mut actionable: Vec<Value> = Vec::new();
    for finding in &evidence {
        if earlier
            .iter()
            .any(|e| same_task(e, finding["task"].as_str().unwrap_or_default()))
        {
            report.repeated.push(finding.clone());
        } else {
            actionable.push(finding.clone());
        }
    }
    for finding in &report.findings {
        let text = format!(
            "{} {}",
            finding["task"].as_str().unwrap_or_default(),
            finding["source_ref"].as_str().unwrap_or_default()
        );
        let cites = scenario_cites(&text);
        // A done task said again: its wording, or its id with much of it.
        let said = finding["task"].as_str().unwrap_or_default();
        let restates = ticked
            .iter()
            .find(|t| {
                same_task(said, &t.title)
                    || (text
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|w| w == t.id)
                        && overlap(said, &t.title) >= 0.3)
            })
            .map(|t| t.id.as_str());
        let why = if !cites.is_empty() && cites.iter().all(|c| holds.contains(c)) {
            Some(format!(
                "{} already hold{}",
                cites.join(", "),
                if cites.len() == 1 { "s" } else { "" }
            ))
        } else if cites.iter().any(|c| red.contains(c)) {
            Some("its scenario's gap is appended from the oracle".to_owned())
        } else {
            restates
                .filter(|_| !cites.iter().any(|c| red.contains(c)))
                .map(|id| format!("restates {id}, which is done, with no red scenario"))
        };
        if let Some(why) = why {
            let mut dropped = finding.clone();
            dropped["why"] = Value::String(why);
            report.dropped.push(dropped);
        } else if finding["gap_type"] == "unrequested" {
            report.unrequested.push(finding.clone());
        } else if earlier
            .iter()
            .any(|e| same_task(e, finding["task"].as_str().unwrap_or_default()))
        {
            report.repeated.push(finding.clone());
        } else {
            actionable.push(finding.clone());
        }
    }
    actionable.sort_by_key(|f| {
        SEVERITY
            .iter()
            .position(|s| f["severity"] == *s)
            .unwrap_or(SEVERITY.len())
    });
    let for_person = if report.unrequested.is_empty() {
        String::new()
    } else {
        format!(
            "; {} unrequested finding(s) for a person",
            report.unrequested.len()
        )
    };

    if actionable.is_empty() {
        return if report.repeated.is_empty() && !report.unrequested.is_empty() {
            // Not "converged" while a finding waits for a person: a model
            // can misfile a requirement gap as unrequested (real run).
            report.outcome = "needs_review".to_owned();
            let n = report.unrequested.len();
            done(
                report,
                CommandStatus::NeedsHuman,
                format!("no tasks appended; {n} unrequested finding(s) for a person to review"),
            )
        } else if report.repeated.is_empty() {
            report.outcome = "converged".to_owned();
            done(
                report,
                CommandStatus::Ok,
                format!(
                    "converged: the implementation satisfies the spec, plan and tasks{for_person}"
                ),
            )
        } else {
            report.outcome = "no_progress".to_owned();
            let n = report.repeated.len();
            done(
                report,
                CommandStatus::NeedsHuman,
                format!("no progress: {n} finding(s) repeat earlier convergence tasks{for_person}"),
            )
        };
    }

    let items: Vec<String> = actionable.iter().map(task_line).collect();
    let tasks_before = std::fs::read(&tasks_path).unwrap_or_default();
    let ids = match format.append(&tasks_path, &tasks_hash, &items) {
        Ok(ids) => ids,
        Err(e) => {
            report.outcome = "failed".to_owned();
            return done(report, CommandStatus::Error, e);
        }
    };
    report.appended = ids
        .iter()
        .zip(&items)
        .map(|(id, text)| format!("{id} {text}"))
        .collect();
    let message = format!(
        "speccoder: converge round {round} ({} task{})\n\n{}\n\nSpec-Feature: {feature_name}\nRun: {}\n{}",
        ids.len(),
        if ids.len() == 1 { "" } else { "s" },
        report.appended.join("\n"),
        report.run_id.clone().unwrap_or_default(),
        crate::attempts::usage_trailer(report.usage.as_ref())
    );
    let committed = match options.session.as_ref().and_then(|s| s.lock().ok()) {
        None => crate::commit_attempt(&wt_root, &message),
        Some(mut s) => {
            let policy = tools::policy::Policy::new(&[], &[]).rules(format.rules());
            let digest =
                crate::in_place::intent_digest(&project, &policy, format.messages().tasks_file);
            s.commit_round(
                &plane_dir_of(state, &project),
                &tasks_path,
                &tasks_before,
                &message,
                &digest,
            )
        }
    };
    match committed {
        Ok(id) => report.commit = Some(id),
        Err(e) => {
            report.outcome = "failed".to_owned();
            return done(report, CommandStatus::Error, format!("commit failed: {e}"));
        }
    }
    report.outcome = "tasks_appended".to_owned();
    let range = match (ids.first(), ids.last()) {
        (Some(a), Some(b)) if a != b => format!("{a}–{b}"),
        (Some(a), _) => a.clone(),
        _ => String::new(),
    };
    done(
        report,
        CommandStatus::Ok,
        format!(
            "appended {range} as Phase: Convergence (round {round}); run the feature again{for_person}"
        ),
    )
}

/// The state directory of the project at `project`.
fn plane_dir_of(state: &Path, project: &Path) -> PathBuf {
    worktree::project_state_dir(state, project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewordings_are_the_same_task_and_distinct_tasks_are_not() {
        // From a real run (2026-10-21): T007 restated T005.
        let t005 = "Add tests and tighten pre-release validation so every identifier is rejected unless it is non-empty ASCII letters, digits, or hyphen only";
        let t007 = "Tighten identifier validation so every pre-release and build identifier is non-empty and consists only of ASCII letters, digits, or hyphen";
        let t003 = "Tighten build metadata parsing so every dot-separated build identifier must be non-empty and valid, and reject inputs with empty build identifiers";
        let t006 = "Add unit tests for every invalid-input category named in the specification and ensure compare returns None for each of them";
        assert!(same_task(t005, t007));
        assert!(!same_task(t003, t007));
        assert!(!same_task(t006, t007));
        assert!(!same_task("", ""));
    }
}
