//! speccoder's parse against OpenSpec's own answer (research R6): the JSON
//! of `openspec validate <id> --strict --json` and
//! `openspec instructions apply --change <id> --json`, which the runtime may
//! run. A disagreement is evidence of a parser gap: shown, never resolved
//! silently. This module only reads JSON; it runs nothing.

use serde_json::Value;
use speccoder_core::snapshot::{Diagnostic, ParsedTasks, Severity};

const ARCHIVE_WOULD_REFUSE: &str = "Archive would refuse this delta";

/// Diagnostics from comparing `tasks` (speccoder's parse of the change) with
/// OpenSpec's `validate` and `instructions apply` output (either may be
/// absent).
#[must_use]
pub fn compare(
    change_dir: &str,
    tasks: Option<&ParsedTasks>,
    validate: Option<&Value>,
    apply: Option<&Value>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for item in validate
        .and_then(|v| v["items"].as_array())
        .into_iter()
        .flatten()
    {
        for issue in item["issues"].as_array().into_iter().flatten() {
            let message = issue["message"].as_str().unwrap_or_default();
            let severity = match issue["level"].as_str().unwrap_or_default() {
                "ERROR" => Some(Severity::Error),
                "INFO" if message.starts_with(ARCHIVE_WOULD_REFUSE) => Some(Severity::Error),
                "WARNING" => Some(Severity::Warning),
                _ => None,
            };
            if let Some(severity) = severity {
                let path = issue["path"].as_str().map_or_else(
                    || change_dir.to_owned(),
                    |p| format!("{change_dir}/specs/{p}"),
                );
                out.push(
                    Diagnostic::new(
                        severity,
                        "crosscheck_error",
                        format!("openspec validate: {message}"),
                    )
                    .at(path),
                );
            }
        }
    }
    if let (Some(apply), Some(tasks)) = (apply, tasks) {
        let total = apply["progress"]["total"].as_u64();
        let complete = apply["progress"]["complete"].as_u64();
        let own_total = u64::try_from(tasks.totals.total).unwrap_or(u64::MAX);
        let own_done = u64::try_from(tasks.totals.done).unwrap_or(u64::MAX);
        if total.is_some_and(|t| t != own_total) || complete.is_some_and(|c| c != own_done) {
            out.push(
                Diagnostic::new(
                    Severity::Error,
                    "crosscheck_mismatch",
                    format!(
                        "OpenSpec counts {} task(s), {} done; speccoder counts {own_total}, {own_done} done",
                        total.unwrap_or_default(),
                        complete.unwrap_or_default()
                    ),
                )
                .at(&tasks.path)
                .hint("report this tasks.md to speccoder: its parser disagrees with OpenSpec's"),
            );
        }
        let theirs: std::collections::BTreeSet<u64> = apply["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["line"].as_u64())
            .collect();
        let ours: std::collections::BTreeSet<u64> =
            tasks.tasks.iter().map(|t| u64::from(t.line)).collect();
        if !theirs.is_empty() && theirs != ours {
            out.push(
                Diagnostic::new(
                    Severity::Warning,
                    "crosscheck_mismatch",
                    "OpenSpec and speccoder see tasks on different lines",
                )
                .at(&tasks.path),
            );
        }
    }
    out
}

/// Why the cross-check did not run, as an informational diagnostic.
#[must_use]
pub fn skipped(why: &str) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "crosscheck_skipped",
        format!("cross-check with OpenSpec's CLI skipped: {why}"),
    )
}
