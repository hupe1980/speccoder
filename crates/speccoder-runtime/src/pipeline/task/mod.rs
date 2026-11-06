//! `speccoder run --task`: one task from baseline to tick, one file per
//! phase: `open`, `ready`, `tests`, `attempt`, `judge`, `tick`.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{
    approvals, attempts, external, integrity, mcp, mutation, progress, resume, sandbox, substrate,
    tools, worktree,
};

/// `speccoder run`: one attempt at a ready task in its worktree, judged by
/// the verifier. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn run(path: &Path, feature: Option<&str>, task: &str, options: &RunOptions) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
        return CommandResult::new(
            "run",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR");
    };
    let model = match setup(path, feature, &state, "run") {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return substrate::model_config::refused("run", &e),
    };
    match driver {
        Ok(provider) => run_task(path, feature, task, &state, provider, Some(&key), options),
        Err(e) => CommandResult::new("run", CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// [`run`] with an explicit state directory and model driver.
pub fn run_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
) -> CommandResult {
    run_task(
        path,
        feature,
        task,
        state,
        provider,
        secret,
        &RunOptions::default(),
    )
}

/// [`run_with`] with explicit [`RunOptions`].
#[must_use]
pub fn run_with_options(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    run_task(path, feature, task, state, provider, secret, options)
}

/// [`run_with`] asking `approver` inline when a call needs approval.
#[must_use]
pub fn run_with_approver(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    approver: Arc<dyn approvals::Approver>,
) -> CommandResult {
    let options = RunOptions {
        approver: Some(approver),
        ..RunOptions::default()
    };
    run_task(path, feature, task, state, provider, secret, &options)
}

/// Runs a bug of the format's own bug structure: a unit no task list
/// names, its task `bug:<slug>`, ticked nowhere.
#[must_use]
pub fn run_native_bug(
    root: &Path,
    bug: &speccoder_core::bug::BugReport,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let options = RunOptions {
        bug_home: Some(Arc::new(super::bug::NativeBug::of(bug))),
        ..options.clone()
    };
    run_task(root, None, &bug.task, state, provider, secret, &options)
}

mod attempt;
mod judge;
/// One task: its attempt, and — when a planning attempt changed code —
/// the task again as code (the planning attempt saved and undone, its
/// saved outcome the reason it is code from now on).
mod open;
mod ready;
mod tests;
mod tick;

/// A phase's result, or the run's end with what a person sees.
macro_rules! ok {
    ($phase:expr) => {
        match $phase {
            Ok(x) => x,
            Err(end) => return *end,
        }
    };
}

/// What opening the task started that later phases carry on.
pub(super) struct Started {
    /// Removes the resume marker when the run ends, unless kept.
    pub(super) marker_guard: crate::resume::Guard,
    /// The worktree's head an interrupted attempt is resumed at.
    pub(super) head_now: String,
    /// Leftovers set aside before the run, named in its report.
    pub(super) set_aside: Option<String>,
}

/// What one phase sets and a later phase changes: handed on explicitly,
/// changed only by a phase that takes it mutably.
pub(super) struct Carried {
    /// The worktree's head an interrupted attempt is resumed at (moved by
    /// the story's oracle commit).
    pub(super) head_now: String,
    /// Removes the resume marker when the run ends, unless kept.
    pub(super) marker_guard: crate::resume::Guard,
    /// Leftovers set aside before the run, named in its report.
    pub(super) set_aside: Option<String>,
    /// The checks on the tree the attempt starts from (re-run after the
    /// story's oracle is committed).
    pub(super) baseline: speccoder_core::verify::VerifyReport,
    /// What the run tells a person, gathered by every phase.
    pub(super) notices: Vec<String>,
}

/// An early end recorded as the task run's end (`TaskFinished`), redacted.
pub(super) fn ended(
    live: &dyn progress::Progress,
    task: &str,
    secret: Option<&str>,
    status: CommandStatus,
    m: String,
) -> CommandResult {
    live.event(&progress::Event::TaskFinished {
        task: task.to_owned(),
        outcome: "error".to_owned(),
        message: redact(&m, secret),
        run: None,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        usd: None,
        regression: None,
    });
    CommandResult::new("run", status, &redact(&m, secret))
}

pub(crate) fn run_task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let first = run_task_once(
        path,
        feature,
        task,
        state,
        Arc::clone(&provider),
        secret,
        options,
    );
    match first.data() {
        Some(CommandData::Run(r)) if r.attempt_status == CHANGED_CODE => {
            if let Some(p) = &options.progress {
                p.event(&progress::Event::Notice {
                    text: format!("{task} changed code, so it runs as code"),
                });
            }
            run_task_once(path, feature, task, state, provider, secret, options)
        }
        _ => first,
    }
}

/// The outcome of a planning attempt that changed something outside its
/// unit: saved and undone, and the task is code from then on.
pub const CHANGED_CODE: &str = "planning_changed_code";

/// One task's run, phase by phase — each a file of `task/`: the task
/// opened (`open`), the worktree made ready (`ready`), the tests written
/// first (`tests`), the attempt (`attempt`), judged beyond its verdict
/// (`judge`), ticked or saved (`tick`). Results are read-only to later
/// phases; what one phase changes for another travels in [`Carried`].
fn run_task_once(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    // In place: the person's checkout, switched and switched back around the task.
    if options.in_place && options.session.is_none() {
        return in_session("run", path, feature, state, options, |o| {
            run_task(path, feature, task, state, Arc::clone(&provider), secret, o)
        });
    }
    let (opened, started) = ok!(open::task(path, feature, task, state, secret, options));
    let (ready, mut carried) = ok!(ready::make(
        &opened,
        started,
        task,
        state,
        secret,
        options,
        Arc::clone(&provider),
    ));
    let mut tests = ok!(tests::first(
        &opened,
        &ready,
        &mut carried,
        task,
        state,
        secret,
        options,
    ));
    let attempted = ok!(attempt::run(
        path,
        &opened,
        &ready,
        &mut tests,
        &mut carried,
        task,
        state,
        secret,
        options,
    ));
    let (attempted, judged) = judge::after(&opened, &ready, &tests, attempted, task, secret);
    tick::finish(
        path, feature, &opened, &ready, tests, attempted, carried, judged, task, state, secret,
        options,
    )
}

/// The code of a run the collection check decided: a file that never ran
/// (`TESTS-NOT-COLLECTED`), or one speccoder cannot tell about.
fn collection_code(
    report: &speccoder_core::RunReport,
    attempt_status: &str,
) -> Option<&'static str> {
    if attempt_status == "collection_unknown" {
        return Some("TESTS-COLLECTION-UNKNOWN");
    }
    match &report.verdict {
        Some(speccoder_core::verify::Verdict::Failed { reasons })
            if reasons
                .first()
                .is_some_and(|r| r.starts_with(speccoder_core::collection::REASON_PREFIX)) =>
        {
            Some("TESTS-NOT-COLLECTED")
        }
        _ => None,
    }
}

/// Judges the attempt's changed test files against what the run collected:
/// a file no test ran from fails the verdict (its reason
/// first); for an otherwise passing attempt, a file speccoder cannot tell
/// about returns the stop's message. Listings run only then.
/// What the collection check came to beyond the verdict's reasons.
enum Collected {
    /// A file speccoder cannot tell about: the stop's message.
    Unknown(String),
    /// A listing changed the worktree: the integrity finding.
    Changed(String),
}

fn judge_collection(
    dirs: (&Path, &Path, &Path),
    runner: &sandbox::Runner,
    otherwise_passing: bool,
    report: &mut speccoder_core::RunReport,
    live: &dyn progress::Progress,
    task: &str,
) -> Option<Collected> {
    let (project, wt_root, tmp) = dirs;
    // Listings run the project's code: the tree they leave must be the one judged.
    let before = worktree::checkpoint(wt_root, tmp).ok();
    let checks = std::fs::read_to_string(project.join(crate::verify::config::FILE))
        .ok()
        .and_then(|t| crate::verify::config::parse(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.tier == crate::verify::config::Tier::Fast)
        .collect::<Vec<_>>();
    let results = report
        .verification
        .as_ref()
        .map(|v| v.checks.clone())
        .unwrap_or_default();
    let c = crate::collection::judge(&crate::collection::Input {
        root: project,
        changes: &report.changes,
        checks: &checks,
        results: &results,
        runner,
        listings: otherwise_passing,
    })?;
    if !c.listing.is_empty()
        && let Some(before) = &before
    {
        let changed: Vec<String> = worktree::checkpoint(wt_root, tmp)
            .and_then(|after| worktree::diff(wt_root, before, &after))
            .map(|d| {
                d.into_iter()
                    .map(|c| c.path)
                    .filter(|p| !speccoder_core::collection::is_build_or_dependency(p))
                    .collect()
            })
            .unwrap_or_else(|e| vec![format!("(the tree could not be read: {})", e.0)]);
        if !changed.is_empty() {
            report.collection = Some(c);
            return Some(Collected::Changed(format!(
                "integrity: a test listing changed the worktree: {}",
                changed.join(", ")
            )));
        }
    }
    live.event(&progress::Event::CollectionChecked {
        task: task.to_owned(),
        files: c
            .files
            .iter()
            .map(|f| progress::CollectionFile {
                path: f.path.clone(),
                state: f.state,
                decided_by: f.decided_by,
            })
            .collect(),
    });
    let never = c.reasons();
    let unknown: Vec<String> = c
        .unknown()
        .map(|f| {
            format!(
                "speccoder cannot tell whether the runner ran `{}` ({})",
                f.path,
                f.why.as_deref().unwrap_or("no report or listing names it")
            )
        })
        .collect();
    report.collection = Some(c);
    if !never.is_empty() {
        let mut reasons = never;
        if let Some(speccoder_core::verify::Verdict::Failed { reasons: more }) =
            report.verdict.take()
        {
            reasons.extend(more);
        }
        report.verdict = Some(speccoder_core::verify::Verdict::Failed { reasons });
        return None;
    }
    (otherwise_passing && !unknown.is_empty()).then(|| Collected::Unknown(unknown.join("; ")))
}

/// `note`, with what the check itself said last (its own reason) and, for
/// the causes that are known, the fix.
fn with_cause(note: &str, c: &speccoder_core::verify::CheckResult) -> String {
    let said = c
        .stderr_tail
        .lines()
        .chain(c.stdout_tail.lines())
        .map(str::trim)
        .find(|l| l.starts_with("error"))
        .map(|l| l.chars().take(200).collect::<String>());
    let fix = match said.as_deref() {
        Some(l) if l.contains("profile `speccoder` not found") => Some(
            "`.config/nextest.toml` is not committed — `git add .config/nextest.toml && git commit -m \"speccoder: nextest profile\"`",
        ),
        Some(l) if l.contains("no tests to run") => Some(
            "the project has no tests yet; if the first task writes them, run with --allow-failing-baseline, or add `--no-tests=pass` to the check's argv",
        ),
        Some(l) if l.contains("must be present") || l.contains("no targets") => Some(
            "the project has nothing to build yet; if the first task creates it, run with --allow-failing-baseline",
        ),
        _ => None,
    };
    match (said, fix) {
        (Some(s), Some(f)) => format!("{note} ({s}); {f}"),
        (Some(s), None) => format!("{note} ({s})"),
        _ => note.to_owned(),
    }
}

/// Why a failing baseline check is broken rather than failing tests: it
/// could not start, or it failed without any test result to show for it.
/// `None`: it passed, or its tests ran and some failed.
/// What the sandbox denied it comes last: often the cause itself.
#[must_use]
pub fn broken(c: &speccoder_core::verify::CheckResult) -> Option<String> {
    let why = broken_why(c)?;
    Some(match c.sandbox.denied.first() {
        Some(first) => {
            let others = c.sandbox.denied.len() - 1 + c.sandbox.more;
            let more = if others > 0 {
                format!(" (and {others} more)")
            } else {
                String::new()
            };
            format!("{why} — the sandbox denied {first}{more}")
        }
        None => why,
    })
}

fn broken_why(c: &speccoder_core::verify::CheckResult) -> Option<String> {
    use speccoder_core::verify::CheckStatus;
    if !c.status.is_failure() || c.status == CheckStatus::TimedOut {
        return None;
    }
    if let Some(note) = &c.note {
        return Some(with_cause(note, c));
    }
    if c.tests.as_ref().is_some_and(|t| !t.is_empty()) {
        return None;
    }
    let summary = crate::verify::summary::parse(&c.stdout_tail, &c.stderr_tail, c.exit_code);
    // Only what is unambiguous: a command that cannot start, a build that
    // fails, or a test runner that ran no test. A script or `grep` that
    // fails until the task is done is a failing check, not a broken one.
    let runner = c
        .argv
        .first()
        .map(|a| a.rsplit('/').next().unwrap_or(a))
        .is_some_and(|a| {
            matches!(
                a,
                "cargo"
                    | "pytest"
                    | "python"
                    | "python3"
                    | "uv"
                    | "npm"
                    | "npx"
                    | "pnpm"
                    | "yarn"
                    | "bun"
                    | "go"
                    | "jest"
                    | "vitest"
                    | "mvn"
                    | "gradle"
                    | "dotnet"
                    | "mix"
                    | "swift"
            )
        });
    let cannot_start = matches!(c.exit_code, Some(126 | 127));
    let is_broken = match &summary {
        _ if cannot_start => true,
        Some(s) if s.build_failed => true,
        Some(s) => s.passed + s.failed + s.errors == 0,
        None => runner,
    };
    match is_broken {
        false => None,
        true => {
            let first = c
                .stderr_tail
                .lines()
                .chain(c.stdout_tail.lines())
                .map(str::trim)
                .find(|l| l.to_lowercase().contains("error"))
                .or_else(|| c.stderr_tail.lines().map(str::trim).find(|l| !l.is_empty()))
                .unwrap_or("it failed without running a test")
                .chars()
                .take(160)
                .collect::<String>();
            Some(first)
        }
    }
}

/// The task stops for the agent's sign-in: its own ways, a command one
/// names as a step of the person's own (a terminal of theirs: it reads the
/// keyboard). Nothing was attempted.
fn login(
    fail: impl FnOnce(CommandStatus, String) -> CommandResult,
    shown: &str,
    agent: &crate::acp::agent::Agent,
    ways: &[crate::acp::session::SignIn],
) -> CommandResult {
    let words: Vec<String> = ways
        .iter()
        .filter(|w| w.command.is_none())
        .map(|w| {
            format!(
                "→ {}",
                w.description.clone().unwrap_or_else(|| w.name.clone())
            )
        })
        .collect();
    let mut commands: Vec<Vec<String>> = ways
        .iter()
        .filter_map(|w| w.command.as_deref())
        .map(|c| c.split_whitespace().map(str::to_owned).collect())
        .collect();
    if words.is_empty()
        && commands.is_empty()
        && let Some(p) = agent.preset()
    {
        commands.push(p.login.split_whitespace().map(str::to_owned).collect());
    }
    commands.dedup();
    let argvs: Vec<speccoder_core::step::Argvs> = commands.into_iter().map(|c| vec![c]).collect();
    let mut fix: Vec<String> = argvs
        .iter()
        .map(|a| speccoder_core::step::render(a))
        .collect();
    fix.extend(words);
    fix.push("speccoder doctor".to_owned());
    fail(
        CommandStatus::NeedsHuman,
        format!("{shown} needs signing in before it can make the attempt; nothing was attempted"),
    )
    .with_code("EXECUTOR-LOGIN")
    .with_fix(fix)
    .with_commands(argvs)
}

/// The red class of a story task's turned scenarios, from the evidence
/// the oracle gave it.
fn story_red(report: &speccoder_core::RunReport) -> Option<speccoder_core::verify::RedClass> {
    let a = report
        .acceptance
        .as_ref()
        .filter(|a| a["oracle"].is_string())?;
    serde_json::from_value::<speccoder_core::verify::RedClass>(a["red_class"].clone())
        .ok()
        .filter(|c| c.is_red())
}

/// The bundle the implementer is given: with the story's oracle, when the
/// task is judged by one.
fn with_oracle(bundle: &serde_json::Value, oracle: &serde_json::Value) -> serde_json::Value {
    let mut b = bundle.clone();
    if !oracle.is_null() {
        b["oracle"] = oracle.clone();
    }
    b
}

/// The tick's record of a person's rulings on the task's tests:
/// `Ruling: tests-wrong by <who> — <why>`.
fn ruling_trailers(ruled: &[crate::acceptance_at::Ruling]) -> String {
    ruled
        .iter()
        .map(|r| {
            format!(
                "Ruling: tests-wrong by {} — {}\n",
                r.by,
                r.reason.replace('\n', " ")
            )
        })
        .collect()
}

/// The frozen test a `wrong_test` stop names (speccoder's own words:
/// "frozen test `tests/x.py`").
fn frozen_test_named(reason: &str) -> Option<String> {
    Some(
        reason
            .split("frozen test `")
            .nth(1)?
            .split('`')
            .next()?
            .trim_start_matches("./")
            .to_owned(),
    )
}
