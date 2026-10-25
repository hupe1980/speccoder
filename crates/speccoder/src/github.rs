//! GitHub Actions (F-056): under `GITHUB_ACTIONS=true`, `run` and
//! `report` append the Markdown report to `$GITHUB_STEP_SUMMARY` and
//! annotate tasks that need a look. GitHub shows at most 10 annotations
//! of a kind per step and caps a step summary at 1 MiB; speccoder stays
//! inside both and says what did not fit.

use std::io::Write;

use speccoder_core::{CommandData, CommandResult, StatusReport};

/// Bytes a step summary may take (GitHub's cap is 1 MiB per step).
pub const SUMMARY_BUDGET: usize = 1_000_000;

/// Annotations of one kind GitHub shows per step.
pub const ANNOTATIONS_PER_KIND: usize = 10;

/// Escapes workflow-command data.
#[must_use]
pub fn data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a workflow-command property.
#[must_use]
pub fn property(s: &str) -> String {
    data(s).replace(':', "%3A").replace(',', "%2C")
}

/// The annotation lines for a report: warnings for `regression_only` and
/// suspected gaming, errors for failed and blocked tasks; at most
/// [`ANNOTATIONS_PER_KIND`] of each, then a notice counting the rest.
#[must_use]
pub fn annotations(r: &StatusReport) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for t in &r.tasks {
        let last = t.last_attempt.as_ref().map(|a| a.outcome.as_str());
        let title = property(&format!(
            "{} {}",
            t.id,
            t.title.chars().take(60).collect::<String>()
        ));
        match (t.state.as_str(), last) {
            ("regression_only", _) => warnings.push(format!(
                "::warning title={title}::{}",
                data("ticked on the regression checks alone; nothing task-specific was shown")
            )),
            (_, Some("suspected_gaming")) => warnings.push(format!(
                "::warning title={title}::{}",
                data("the gaming monitor suspects a shortcut; not ticked")
            )),
            (_, Some("verification_failed" | "failed")) => errors.push(format!(
                "::error title={title}::{}",
                data("the checks or the acceptance tests did not pass")
            )),
            (_, Some("blocked")) => errors.push(format!(
                "::error title={title}::{}",
                data("the model says the task cannot be done as written")
            )),
            _ => {}
        }
    }
    let hidden = warnings.len().saturating_sub(ANNOTATIONS_PER_KIND)
        + errors.len().saturating_sub(ANNOTATIONS_PER_KIND);
    let mut out: Vec<String> = errors
        .into_iter()
        .take(ANNOTATIONS_PER_KIND)
        .chain(warnings.into_iter().take(ANNOTATIONS_PER_KIND))
        .collect();
    if hidden > 0 {
        out.push(format!(
            "::notice title=speccoder::{}",
            data(&format!(
                "{hidden} more task(s) need a look — see the job summary or `speccoder report`"
            ))
        ));
    }
    out
}

/// What [`report`] writes, given the environment (tests pass their own).
pub struct Env<'a> {
    /// `GITHUB_ACTIONS`.
    pub actions: bool,
    /// `GITHUB_STEP_SUMMARY`.
    pub summary: Option<&'a std::path::Path>,
}

/// Appends the summary and prints the annotations for `result` — a
/// `report`'s own, or the report taken after a `run` (`after`). Nothing
/// outside GitHub Actions; annotations only in human mode (`machine`
/// keeps stdout the contract).
pub fn report_with(
    result: &CommandResult,
    after: Option<&CommandResult>,
    machine: bool,
    env: &Env<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) {
    if !env.actions {
        return;
    }
    let source = match (result.data(), after.and_then(CommandResult::data)) {
        (Some(CommandData::Report(r)), _) | (_, Some(CommandData::Report(r))) => r,
        _ => return,
    };
    if let Some(path) = env.summary {
        let text = crate::markdown::within(source, result.next(), SUMMARY_BUDGET);
        let written = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .and_then(|mut f| writeln!(f, "{text}"));
        if let Err(e) = written {
            let _ = writeln!(
                err,
                "speccoder: the step summary was not written ({}): {e}",
                path.display()
            );
        }
    }
    if !machine {
        for line in annotations(source) {
            let _ = writeln!(out, "{line}");
        }
    }
}

/// [`report_with`] in this process's environment, for `main`.
pub fn report(result: &CommandResult, machine: bool, after: Option<&CommandResult>) {
    let actions = std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true");
    let summary = std::env::var_os("GITHUB_STEP_SUMMARY").map(std::path::PathBuf::from);
    report_with(
        result,
        after,
        machine,
        &Env {
            actions,
            summary: summary.as_deref(),
        },
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use speccoder_core::{LastAttempt, StatusTotals, TaskStatus};

    fn task(id: &str, state: &str, last: Option<&str>) -> TaskStatus {
        TaskStatus {
            id: id.to_owned(),
            title: "Parse: a, b\nc %".to_owned(),
            phase: "P".to_owned(),
            state: state.to_owned(),
            intent_changed: false,
            blocked_by: Vec::new(),
            commit: None,
            run_id: None,
            last_attempt: last.map(|o| LastAttempt {
                outcome: o.to_owned(),
                reason: "the model's words".to_owned(),
                run_id: None,
                reference: "refs/x".to_owned(),
            }),
            note: None,
            attestation: None,
            strength: None,
        }
    }

    fn report(tasks: Vec<TaskStatus>) -> CommandResult {
        CommandResult::ok("report", "x").with_data(CommandData::Report(Box::new(StatusReport {
            feature: "001-x".to_owned(),
            branch: None,
            source: "branch".to_owned(),
            selected_by: String::new(),
            unmerged_commits: 0,
            tasks,
            convergence: Vec::new(),
            trace: Vec::new(),
            totals: StatusTotals::default(),
        })))
    }

    #[test]
    fn annotations_are_escaped_limited_and_counted() {
        let mut tasks: Vec<TaskStatus> = (0..12)
            .map(|i| task(&format!("T{i}"), "regression_only", None))
            .collect();
        tasks.push(task("T99", "ready", Some("blocked")));
        let r = report(tasks);
        let Some(CommandData::Report(s)) = r.data() else {
            panic!()
        };
        let lines = annotations(s);
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("::warning")).count(),
            10
        );
        assert_eq!(lines.iter().filter(|l| l.starts_with("::error")).count(), 1);
        assert!(
            lines.last().unwrap().contains("2 more task(s)"),
            "{lines:?}"
        );
        assert!(
            lines[0].contains("title=T99 Parse%3A a%2C b%0Ac %25::"),
            "{}",
            lines[0]
        );
        assert!(!lines.iter().any(|l| l.contains("model's words")));
    }

    #[test]
    fn the_summary_is_appended_only_under_actions() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("summary.md");
        let r = report(vec![task("T1", "regression_only", None)]);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let off = Env {
            actions: false,
            summary: Some(&file),
        };
        report_with(&r, None, false, &off, &mut out, &mut err);
        assert!(!file.exists() && out.is_empty());
        let on = Env {
            actions: true,
            summary: Some(&file),
        };
        report_with(&r, None, false, &on, &mut out, &mut err);
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("## speccoder")
        );
        assert!(String::from_utf8(out).unwrap().starts_with("::warning"));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        report_with(&r, None, true, &on, &mut out, &mut err);
        assert!(out.is_empty(), "no annotations under --json/--events");
        let bad = Env {
            actions: true,
            summary: Some(dir.path()),
        };
        report_with(&r, None, false, &bad, &mut Vec::new(), &mut err);
        assert!(String::from_utf8(err).unwrap().contains("not written"));
    }
}
