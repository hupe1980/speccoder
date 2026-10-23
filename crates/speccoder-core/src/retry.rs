//! Retries: the verifier's failure
//! digest for the next attempt, and the fingerprint that stops retries
//! which cannot help.
//!
//! Everything here comes from verifier output — never from the model's
//! transcript or its own account of what went wrong.

use serde::{Deserialize, Serialize};

use crate::verify::{CheckStatus, VerifyReport};

/// Lines of output kept per failing check.
const EXCERPT_LINES: usize = 40;
/// Bytes of output kept per failing check.
const EXCERPT_BYTES: usize = 4000;
/// Words that mark the first error line of a check's output.
const MARKERS: [&str; 5] = ["error", "panicked", "assert", "failed", "fail"];

/// One check that did not pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedCheck {
    /// Name from `speccoder.toml` (or `acceptance`).
    pub name: String,
    /// The command, as run.
    pub argv: Vec<String>,
    /// What happened.
    pub status: CheckStatus,
    /// Failing test IDs, when the check reports tests.
    pub failing_tests: Vec<String>,
    /// Output from the first error line on, capped.
    pub excerpt: String,
}

/// What the next attempt is told about the previous one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest {
    /// The attempt this digest describes (1-based).
    pub attempt: u32,
    /// The verdict's reasons.
    pub reasons: Vec<String>,
    /// Checks that did not pass.
    pub checks: Vec<FailedCheck>,
    /// Paths the previous attempt changed; its changes were discarded.
    pub discarded: Vec<String>,
}

/// The retry reason for failed held-out tests: the scenario IDs and the
/// spec's own words — which the implementer has already seen — and nothing
/// from the held-out run itself (no path, test name, literal or output).
#[must_use]
pub fn held_out_reason(failed: &[crate::spec::ScenarioRef]) -> String {
    let mut failed: Vec<&crate::spec::ScenarioRef> = failed.iter().collect();
    failed.sort_by(|a, b| a.id.cmp(&b.id));
    failed.dedup_by(|a, b| a.id == b.id);
    let parts: Vec<String> = failed
        .iter()
        .map(|s| {
            format!(
                "{} (\"{}\")",
                s.id,
                s.text.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect();
    format!("held-out acceptance failed: {}", parts.join(", "))
}

/// Why a failed attempt must not be retried, if it must not: a check that
/// could not run at all is an infrastructure problem, not the model's.
#[must_use]
pub fn infrastructure(report: &VerifyReport) -> Option<String> {
    report
        .checks
        .iter()
        .find(|c| c.status == CheckStatus::Error)
        .map(|c| {
            format!(
                "check {} could not run{}",
                c.name,
                c.note
                    .as_deref()
                    .map(|n| format!(": {n}"))
                    .unwrap_or_default()
            )
        })
}

/// The digest of a failed attempt.
#[must_use]
pub fn digest(
    attempt: u32,
    reasons: &[String],
    report: &VerifyReport,
    discarded: &[String],
) -> Digest {
    let checks = report
        .checks
        .iter()
        .filter(|c| !matches!(c.status, CheckStatus::Passed | CheckStatus::Skipped))
        .map(|c| FailedCheck {
            name: c.name.clone(),
            argv: c.argv.clone(),
            status: c.status,
            failing_tests: c
                .tests
                .iter()
                .flatten()
                .filter(|t| t.outcome.is_failing())
                .map(|t| t.id.clone())
                .collect(),
            excerpt: excerpt(&output(&c.stderr_tail, &c.stdout_tail)),
        })
        .collect();
    Digest {
        attempt,
        reasons: reasons.to_vec(),
        checks,
        discarded: discarded.to_vec(),
    }
}

/// The failure's identity: failing checks by name, each with its first
/// error line, numbers normalised. Equal fingerprints mean the same failure.
#[must_use]
pub fn fingerprint(digest: &Digest) -> String {
    let mut parts: Vec<String> = digest
        .checks
        .iter()
        .map(|c| format!("{}: {}", c.name, normalise(first_error(&c.excerpt))))
        .collect();
    parts.sort();
    if parts.is_empty() {
        let mut reasons: Vec<String> = digest.reasons.iter().map(|r| normalise(r)).collect();
        reasons.sort();
        return reasons.join(" | ");
    }
    parts.join(" | ")
}

fn output(stderr: &str, stdout: &str) -> String {
    match (stderr.trim().is_empty(), stdout.trim().is_empty()) {
        (false, false) => format!("{stderr}\n{stdout}"),
        (false, true) => stderr.to_owned(),
        _ => stdout.to_owned(),
    }
}

fn is_error_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

fn first_error(text: &str) -> &str {
    text.lines()
        .find(|l| is_error_line(l))
        .or_else(|| text.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or_default()
}

/// From the first error line on (or the last lines, if there is none),
/// at most [`EXCERPT_LINES`] lines and [`EXCERPT_BYTES`] bytes.
fn excerpt(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| is_error_line(l))
        .unwrap_or_else(|| lines.len().saturating_sub(EXCERPT_LINES));
    let mut out = lines
        .iter()
        .skip(start)
        .take(EXCERPT_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if out.len() > EXCERPT_BYTES {
        let mut end = EXCERPT_BYTES;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("\n…");
    }
    out
}

/// Digits become `#`, whitespace runs one space; at most 200 characters.
fn normalise(line: &str) -> String {
    let mapped: String = line
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect();
    mapped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_out_failure_names_the_requirement_only() {
        use crate::spec::ScenarioRef;
        let failed = [
            ScenarioRef {
                id: "US1-2".to_owned(),
                text: "**Given** a name,\n **When** greeted".to_owned(),
            },
            ScenarioRef {
                id: "US1-1".to_owned(),
                text: "x".to_owned(),
            },
        ];
        let reason = held_out_reason(&failed);
        assert_eq!(
            reason,
            "held-out acceptance failed: US1-1 (\"x\"), US1-2 (\"**Given** a name, **When** greeted\")"
        );
        let report = VerifyReport {
            root: String::new(),
            tier: "fast".to_owned(),
            checks: Vec::new(),
            passed: true,
            comparison: None,
        };
        let d = digest(2, std::slice::from_ref(&reason), &report, &[]);
        assert!(d.checks.is_empty(), "no check entry for the held-out run");
        assert_eq!(fingerprint(&d), normalise(&reason));
    }
    use crate::verify::{CheckResult, TestOutcome, TestResult};

    fn check(name: &str, status: CheckStatus, stderr: &str) -> CheckResult {
        CheckResult {
            name: name.to_owned(),
            argv: vec!["cargo".to_owned(), "test".to_owned()],
            status,
            exit_code: Some(101),
            duration_ms: 10,
            junit: false,
            tests: None,
            stdout_tail: String::new(),
            stderr_tail: stderr.to_owned(),
            truncated: false,
            note: None,
            flaky: false,
        }
    }

    fn report(checks: Vec<CheckResult>) -> VerifyReport {
        VerifyReport {
            root: ".".to_owned(),
            tier: "fast".to_owned(),
            checks,
            comparison: None,
            passed: false,
        }
    }

    #[test]
    fn the_digest_keeps_failing_checks_from_the_first_error() {
        let mut failing = check(
            "test",
            CheckStatus::Failed,
            "   Compiling x v0.1.0\nerror[E0425]: cannot find function `add`\n --> src/lib.rs:3:5\n",
        );
        failing.tests = Some(vec![
            TestResult {
                id: "adds".to_owned(),
                outcome: TestOutcome::Failed,
            },
            TestResult {
                id: "ok".to_owned(),
                outcome: TestOutcome::Passed,
            },
        ]);
        let r = report(vec![check("fmt", CheckStatus::Passed, ""), failing]);
        let d = digest(
            1,
            &["check test failed".to_owned()],
            &r,
            &["src/lib.rs".to_owned()],
        );
        assert_eq!(d.checks.len(), 1);
        assert_eq!(d.checks[0].failing_tests, ["adds"]);
        assert!(
            d.checks[0].excerpt.starts_with("error[E0425]"),
            "{}",
            d.checks[0].excerpt
        );
        assert!(!d.checks[0].excerpt.contains("Compiling"));
        assert_eq!(d.discarded, ["src/lib.rs"]);
    }

    #[test]
    fn fingerprints_ignore_numbers_and_order() {
        let a = report(vec![
            check("b", CheckStatus::Failed, "error at line 12"),
            check("a", CheckStatus::Failed, "assertion failed: 4 == 5"),
        ]);
        let b = report(vec![
            check("a", CheckStatus::Failed, "assertion failed: 3 == 5"),
            check("b", CheckStatus::Failed, "error at line 40"),
        ]);
        let c = report(vec![check(
            "a",
            CheckStatus::Failed,
            "error: something else",
        )]);
        let fp = |r: &VerifyReport| fingerprint(&digest(1, &[], r, &[]));
        assert_eq!(fp(&a), fp(&b));
        assert_ne!(fp(&a), fp(&c));
        // Without failing checks (e.g. a regression), the reasons decide.
        let none = report(vec![]);
        let x = fingerprint(&digest(1, &["regressed: t1".to_owned()], &none, &[]));
        assert_eq!(x, "regressed: t#");
    }

    #[test]
    fn a_check_that_could_not_run_is_infrastructure() {
        let mut broken = check("lint", CheckStatus::Error, "");
        broken.note = Some("command not found".to_owned());
        assert_eq!(
            infrastructure(&report(vec![broken])).as_deref(),
            Some("check lint could not run: command not found")
        );
        assert!(infrastructure(&report(vec![check("t", CheckStatus::Failed, "x")])).is_none());
    }

    #[test]
    fn long_output_is_capped_on_a_char_boundary() {
        let long = format!("error: {}", "é".repeat(5000));
        let e = excerpt(&long);
        assert!(e.len() <= EXCERPT_BYTES + "\n…".len());
        assert!(e.ends_with('…'));
    }
}
