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
        Some(Verdict::Failed { reasons }) => failed_because(report, reasons),
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
                "attempt incomplete — {}{}",
                incomplete_because(report.model_report.as_ref()),
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
            format!("verdict: failed — {}", failed_because(report, reasons)),
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

/// The verdict's reasons, a failed check's with what it said first
/// (``check `acceptance` failed: No module named pytest``).
fn failed_because(report: &RunReport, reasons: &[String]) -> String {
    let checks = report
        .verification
        .as_ref()
        .map(|v| v.checks.as_slice())
        .unwrap_or_default();
    reasons
        .iter()
        .map(|r| {
            let named = r
                .strip_prefix("check `")
                .and_then(|n| n.split_once('`'))
                .map(|(n, _)| n);
            match named
                .and_then(|n| checks.iter().find(|c| c.name == n))
                .and_then(speccoder_core::retry::first_cause)
            {
                Some(cause) => format!("{r}: {cause}"),
                None => r.clone(),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// What the model's closing report says is missing — its notes, else its
/// summary — quoted as its claim, cut at 400 characters.
fn incomplete_because(model_report: Option<&Value>) -> String {
    let report = model_report.unwrap_or(&Value::Null);
    let notes: Vec<&str> = report["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    let said = if notes.is_empty() {
        report["summary"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned()
    } else {
        notes.join(" ")
    };
    if said.is_empty() {
        return "the model says it did not finish".to_owned();
    }
    let said = crate::approvals::present::escape(&said.replace('\n', " "));
    let cut: String = said.chars().take(400).collect();
    let more = if cut.len() < said.len() { "…" } else { "" };
    format!("the model says: “{cut}{more}”")
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
        agent: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_failed_check_says_what_it_said_first() {
        use speccoder_core::verify::{CheckResult, CheckStatus, VerifyReport};
        let mut report = bare_report("f", "T007", "w", "completed");
        report.verification = Some(VerifyReport {
            root: String::new(),
            tier: "fast".to_owned(),
            checks: vec![CheckResult {
                name: "acceptance".to_owned(),
                argv: vec!["python3".to_owned()],
                status: CheckStatus::Failed,
                exit_code: Some(1),
                duration_ms: 0,
                junit: false,
                tests: None,
                stdout_tail: String::new(),
                stderr_tail: "/usr/bin/python3: No module named pytest\n".to_owned(),
                truncated: false,
                note: None,
                flaky: false,
            }],
            comparison: None,
            passed: false,
        });
        let reasons = [
            "check `acceptance` failed".to_owned(),
            "regression: t is failed".to_owned(),
        ];
        assert_eq!(
            failed_because(&report, &reasons),
            "check `acceptance` failed: /usr/bin/python3: No module named pytest; regression: t is failed"
        );
    }

    #[test]
    fn an_incomplete_attempt_quotes_what_the_model_says_is_missing() {
        let notes =
            json!({ "summary": "s", "notes": ["", "pytest is \u{1b}[31mmissing", "so I stopped"] });
        assert_eq!(
            incomplete_because(Some(&notes)),
            "the model says: “pytest is ␛[31mmissing so I stopped”"
        );
        let summary = json!({ "summary": "only the plumbing\nso far", "notes": [] });
        assert_eq!(
            incomplete_because(Some(&summary)),
            "the model says: “only the plumbing so far”"
        );
        assert_eq!(incomplete_because(None), "the model says it did not finish");
        let long = json!({ "notes": ["é".repeat(500)] });
        assert!(incomplete_because(Some(&long)).ends_with("é…”"));
    }
}
