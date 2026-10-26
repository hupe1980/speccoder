//! The judge's steps within one attempt: which writes a person approved,
//! what the integrity audit finds, the verdict from the checks, and whether
//! a failed attempt is worth retrying.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use speccoder_core::RunReport;
use speccoder_core::verify::{CheckStatus, Verdict, VerifyReport, compare, passes, verdict};

use crate::tools::policy::{Object, Policy};
use crate::{integrity, sandbox, verify, worktree};

use super::{PreparedAcceptance, run_acceptance};

/// Paths a person granted writing, for the task or inline in this attempt.
/// A frozen test so approved is not a violation; the audit still flags
/// removed tests and skip markers in it — approval permits editing a test,
/// not weakening it.
pub(crate) fn approved_writes(granted: &[Object], tool_calls: &Value) -> BTreeSet<String> {
    granted
        .iter()
        .filter_map(|g| g.path().map(str::to_owned))
        .chain(
            tool_calls
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["approved_by"] == "person")
                .filter_map(|c| {
                    serde_json::from_value::<Object>(c["object"].clone())
                        .ok()?
                        .path()
                        .map(str::to_owned)
                }),
        )
        .collect()
}

/// What the integrity audit of one attempt reads.
pub(crate) struct Audited<'a> {
    pub(crate) wt_root: &'a Path,
    pub(crate) policy: &'a Policy,
    /// Judge inputs as the attempt found them.
    pub(crate) judged_before: &'a integrity::JudgeSnapshot,
    pub(crate) raw_changes: &'a [worktree::Change],
    pub(crate) patch: &'a str,
    pub(crate) frozen: &'a [String],
    pub(crate) frozen_additions: bool,
    pub(crate) approved_writes: &'a BTreeSet<String>,
    /// Judge drift a command caused, reported by the tool (it ended the attempt).
    pub(crate) command_drift: &'a Value,
}

/// Every integrity finding of an attempt, each `integrity: …`: the diff
/// audit, judge inputs the diff cannot show (git-ignored files, whoever
/// changed them — any executor), and drift a command caused.
pub(crate) fn attempt_findings(a: &Audited<'_>) -> Vec<String> {
    let unseen = a
        .judged_before
        .drift(
            &integrity::JudgeSnapshot::take(a.wt_root, a.policy),
            a.policy,
            a.approved_writes,
        )
        .into_iter()
        .filter(|d| !a.raw_changes.iter().any(|c| c.path == d.path))
        .map(|d| format!("integrity: {} input {}: {}", d.class, d.change, d.path));
    let frozen: BTreeSet<String> = a
        .frozen
        .iter()
        .filter(|p| !a.approved_writes.contains(*p))
        .cloned()
        .collect();
    let mut findings: Vec<String> = integrity::audit_in(
        a.policy.protected(),
        a.raw_changes,
        a.patch,
        &frozen,
        a.frozen_additions,
        a.approved_writes,
    )
    .into_iter()
    .map(|f| format!("integrity: {f}"))
    .chain(unseen)
    .collect();
    for d in a.command_drift.as_array().into_iter().flatten() {
        let finding = format!(
            "integrity: judge input changed by a command: {} ({})",
            d["path"].as_str().unwrap_or_default(),
            d["change"].as_str().unwrap_or_default()
        );
        if !findings.contains(&finding) {
            findings.push(finding);
        }
    }
    findings
}

/// `(path, added|modified|deleted)` for the report.
pub(crate) fn changes_of(raw: Vec<worktree::Change>) -> Vec<(String, String)> {
    raw.into_iter()
        .map(|c| {
            let kind = match c.kind {
                worktree::ChangeKind::Added => "added",
                worktree::ChangeKind::Modified => "modified",
                worktree::ChangeKind::Deleted => "deleted",
            };
            (c.path, kind.to_owned())
        })
        .collect()
}

/// The verdict: the fast checks against the baseline, and the acceptance
/// tests green — task evidence only when they were red first. Any integrity
/// finding fails it, whatever the checks said.
pub(crate) fn judge_checks(
    project: &Path,
    runner: &sandbox::Runner,
    baseline: &VerifyReport,
    acceptance: Option<&PreparedAcceptance>,
    findings: &[String],
    report: &mut RunReport,
) -> Result<(), String> {
    let mut current = verify::run_checks(project, Some(verify::config::Tier::Fast), runner)?;
    let mut task_checks: Vec<&str> = Vec::new();
    if let Some(a) = acceptance.filter(|a| a.argv.is_some()) {
        let argv = a.argv.as_deref().unwrap_or_default();
        let all: Vec<&[String]> = std::iter::once(argv)
            .chain(a.more_argv.iter().map(Vec::as_slice))
            .collect();
        let green = verify::rerun_once(run_all_acceptance(project, &all, runner), || {
            run_all_acceptance(project, &all, runner)
        });
        if let Some(r) = report.acceptance.as_mut() {
            r["green"] = serde_json::json!(green.status == CheckStatus::Passed);
            if green.flaky {
                r["green_flaky"] = serde_json::json!(true);
                r["note"] = serde_json::json!("acceptance passed only on re-run");
            }
        }
        if a.red {
            task_checks.push("acceptance");
        }
        current.checks.push(green);
    }
    let cmp = compare(baseline, &current);
    current.passed = passes(&current.checks, Some(&cmp));
    let judged = verdict(&task_checks, &current.checks, Some(&cmp));
    report.verdict = Some(if findings.is_empty() {
        judged
    } else {
        let mut reasons = findings.to_vec();
        if let Verdict::Failed { reasons: more } = judged {
            reasons.extend(more);
        }
        Verdict::Failed { reasons }
    });
    current.comparison = Some(cmp);
    report.verification = Some(current);
    Ok(())
}

/// Every command of an acceptance set, as one result: passed only when
/// each passed (the first failure's output is kept).
fn run_all_acceptance(
    project: &Path,
    all: &[&[String]],
    runner: &sandbox::Runner,
) -> speccoder_core::verify::CheckResult {
    let mut first = None;
    for argv in all {
        let r = run_acceptance(project, argv, runner);
        if r.status != CheckStatus::Passed {
            return r;
        }
        first.get_or_insert(r);
    }
    first.unwrap_or_else(|| run_acceptance(project, &[], runner))
}

/// Why a failed, completed attempt is not retried, if it is not.
pub(crate) fn retry_stop(
    findings: &[String],
    verification: &VerifyReport,
    last_fingerprint: Option<&str>,
    fingerprint: &str,
    attempt_no: u32,
    max_attempts: u32,
) -> Option<String> {
    if !findings.is_empty() {
        Some("integrity findings are not retried".to_owned())
    } else if let Some(infra) = speccoder_core::retry::infrastructure(verification) {
        Some(format!("not retried: {infra}"))
    } else if last_fingerprint == Some(fingerprint) {
        Some(format!("same failure twice: {fingerprint}"))
    } else if attempt_no >= max_attempts {
        Some(format!("{attempt_no} of {max_attempts} attempts used"))
    } else {
        None
    }
}

/// The gaming monitor's view of a passing attempt, and whether it suspects
/// gaming. A monitor that fails blocks nothing: it can only add doubt.
pub(crate) fn monitor_attempt(
    rt: &tokio::runtime::Runtime,
    plane: &crate::substrate::Plane,
    feature: &str,
    task: &str,
    input: agentplane::core::Tainted<Value>,
    secret: Option<&str>,
) -> (Value, bool) {
    match rt.block_on(plane.monitor(feature, task, input)) {
        Ok(o) => {
            let out = o
                .output
                .as_ref()
                .map(|v| v.peek().clone())
                .unwrap_or(Value::Null);
            let suspected = out["answer"]["suspected"] == true;
            (
                serde_json::json!({
                    "suspected": suspected,
                    "findings": out["answer"]["findings"],
                    "usage": out["usage"],
                    "run_id": o.run_id.to_string(),
                }),
                suspected,
            )
        }
        Err(e) => (
            serde_json::json!({ "error": super::redact(&e.0, secret) }),
            false,
        ),
    }
}
