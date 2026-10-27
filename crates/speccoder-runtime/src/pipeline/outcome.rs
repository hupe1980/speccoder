//! How a task run ends: its scope finding, what a failed attempt is saved
//! as, the command's status and message, and the progress event.

use serde_json::Value;
use speccoder_core::scope::ScopeFinding;
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandStatus, RunReport};

use crate::progress;

use super::{add_usage, monitor_summary};

/// What the attempt changed beyond the paths the task line `title` names
/// (speccoder's own acceptance tests exempt).
pub(crate) fn scope_of(report: &RunReport, title: &str) -> ScopeFinding {
    let exempt: Vec<String> = report
        .acceptance
        .as_ref()
        .and_then(|a| a["files"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str().map(str::to_owned))
        .collect();
    let changed: Vec<String> = report.changes.iter().map(|(p, _)| p.clone()).collect();
    let named = speccoder_core::scope::named_paths(title);
    speccoder_core::scope::finding(&named, &changed, &exempt)
}

/// The outcome and reason an attempt that did not pass is saved with.
pub(crate) fn saved_outcome(
    report: &RunReport,
    attempt_status: &str,
    suspected: bool,
) -> (String, String) {
    let outcome = match attempt_status {
        _ if suspected => "suspected_gaming".to_owned(),
        "completed" | "integrity" => "verification_failed".to_owned(),
        other => other.to_owned(),
    };
    let reason = match &report.verdict {
        _ if suspected => monitor_summary(report.monitor.as_ref()),
        Some(Verdict::Failed { reasons }) => reasons.join("; "),
        _ => report
            .reason
            .clone()
            .unwrap_or_else(|| attempt_status.to_owned()),
    };
    (outcome, reason)
}

/// The command's status and message for a finished task run.
pub(crate) fn status_and_message(
    report: &RunReport,
    attempt_status: &str,
    suspected: bool,
) -> (CommandStatus, String) {
    match (&report.verdict, attempt_status) {
        (_, "over_strict") => (
            CommandStatus::NeedsHuman,
            format!(
                "the checks passed, but {}; the attempt is held, not ticked — rule on it (`speccoder task rule {task} --tests-right --reason …`), regenerate the tests (`speccoder run --task {task} --fresh-acceptance`) or accept it (`speccoder task accept {task}`)",
                report.reason.clone().unwrap_or_default(),
                task = report.task
            ),
        ),
        (_, "acceptance_weak") => (
            CommandStatus::NeedsHuman,
            format!(
                "the checks passed, but the {}; the attempt is saved, not ticked",
                report.reason.clone().unwrap_or_default()
            ),
        ),
        _ if suspected => (
            CommandStatus::NeedsHuman,
            format!(
                "the checks passed, but the gaming monitor suspects: {}; the attempt is saved, not ticked — review it, then `speccoder task accept` or run again",
                monitor_summary(report.monitor.as_ref())
            ),
        ),
        (_, "incomplete") => (
            CommandStatus::NeedsHuman,
            format!(
                "attempt incomplete by the model's own account{}",
                if report.changes.is_empty() {
                    "; no files changed"
                } else {
                    ""
                }
            ),
        ),
        (None, "collection_unknown") => (
            CommandStatus::NeedsHuman,
            format!(
                "{}; the attempt is saved, not ticked",
                report.reason.clone().unwrap_or_default()
            ),
        ),
        (Some(Verdict::Failed { reasons }), _) => (
            CommandStatus::VerificationFailed,
            format!("verdict: failed — {}", reasons.join("; ")),
        ),
        (Some(v), _) => (CommandStatus::Ok, format!("verdict: {}", verdict_word(v))),
        // A call nobody could ask about: named, with how to let it run.
        (None, "needs_approval") if report.pending.as_ref().is_some_and(Value::is_object) => (
            CommandStatus::NeedsHuman,
            format!(
                "attempt needs_approval: {}",
                crate::tools::policy::unasked(
                    report.pending.as_ref().unwrap_or(&Value::Null),
                    &format!(
                        "approve it (`speccoder task approve {}`), or run with prompts (without `--no-prompt`) to be asked",
                        report.task
                    ),
                )
            ),
        ),
        (
            None,
            "blocked" | "needs_approval" | "budget_exhausted" | "denied" | "interrupted"
            | "held_out" | "intent_changed" | "judge_changed" | "checkout_changed" | "git_changed",
        ) => (
            CommandStatus::NeedsHuman,
            format!(
                "attempt {attempt_status}: {}",
                report
                    .reason
                    .clone()
                    .or_else(|| report
                        .pending
                        .as_ref()
                        .map(|p| p["reason"].as_str().unwrap_or_default().to_owned()))
                    .unwrap_or_default()
            ),
        ),
        (None, _) => (
            CommandStatus::RunFailed,
            format!(
                "attempt failed: {}",
                report.reason.clone().unwrap_or_default()
            ),
        ),
    }
}

/// `verified`, `regression_only` or `failed`.
pub(crate) fn verdict_word(v: &Verdict) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v["verdict"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The progress event that ends a task run; `message` is already redacted.
pub(crate) fn finished_event(
    task: &str,
    report: &RunReport,
    attempt_status: &str,
    suspected: bool,
    message: String,
) -> progress::Event {
    let total = add_usage(
        report.usage.as_ref(),
        report.acceptance.as_ref().and_then(|a| a.get("usage")),
    );
    let tokens = |k: &str| total.as_ref().and_then(|u| u[k].as_u64()).unwrap_or(0);
    progress::Event::TaskFinished {
        task: task.to_owned(),
        // The verdict for a completed attempt; otherwise how the attempt ended
        // (an incomplete attempt with green checks is not a pass).
        outcome: if suspected {
            "suspected_gaming".to_owned()
        } else if attempt_status == "completed" {
            report
                .verdict
                .as_ref()
                .map_or_else(|| attempt_status.to_owned(), verdict_word)
        } else {
            attempt_status.to_owned()
        },
        message,
        run: (!report.run_id.is_empty()).then(|| report.run_id.clone()),
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        cache_read_tokens: tokens("cache_read_tokens"),
        cache_write_tokens: tokens("cache_write_tokens"),
        usd: None,
        regression: report.regression.clone(),
    }
}

/// A run report with nothing established yet.
pub(crate) fn bare_report(
    feature: &str,
    task: &str,
    worktree: &str,
    attempt_status: &str,
) -> RunReport {
    RunReport {
        run_id: String::new(),
        feature: feature.to_owned(),
        task: task.to_owned(),
        worktree: worktree.to_owned(),
        attempt_status: attempt_status.to_owned(),
        model_report: None,
        reason: None,
        pending: None,
        tool_calls: Vec::new(),
        changes: Vec::new(),
        verification: None,
        verdict: None,
        usage: None,
        integrity: Vec::new(),
        ticked: false,
        commit: None,
        acceptance: None,
        earlier: Vec::new(),
        retry_stop: None,
        saved_attempt: None,
        set_aside: None,
        notices: Vec::new(),
        monitor: None,
        mutation: None,
        samples: Vec::new(),
        scope: None,
        attestation: None,
        instructions: Vec::new(),
        strength: None,
        bug: None,
        collection: None,
        regression: None,
        context: None,
    }
}

/// Why a convergence task's gap is not demonstrated: no examples, no tests
/// written, or tests that do not fail on an assertion before any change.
pub(crate) fn undemonstrated_why(acceptance: Option<&super::PreparedAcceptance>) -> String {
    match acceptance {
        None => "the spec has no acceptance examples to test it against".to_owned(),
        Some(a) => match (a.report["skipped"].as_str(), a.report["note"].as_str()) {
            (Some(s), _) => format!("no acceptance tests ({s})"),
            (None, Some(note)) => match a.report["excerpt"].as_str().and_then(|e| e.lines().next())
            {
                Some(first) => format!("{note}: {first}"),
                None => note.to_owned(),
            },
            (None, None) => "its acceptance tests already pass before any change".to_owned(),
        },
    }
}
