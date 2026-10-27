//! `speccoder verify`: run the project's declared checks and judge the result.

pub mod config;
pub mod junit;
pub mod process;
pub mod summary;

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::format::Workspace;
use speccoder_core::verify::{CheckResult, CheckStatus, VerifyReport, compare, passes};
use speccoder_core::{CommandData, CommandResult, CommandStatus, SCHEMA_VERSION};

use self::config::{Check, Tier};
use self::process::{CommandRunner, Invocation, OUTPUT_CAP, scrubbed_env};

/// Largest JUnit report read.
const MAX_REPORT_BYTES: u64 = 64 * 1024 * 1024;
/// Largest baseline file read.
const MAX_BASELINE_BYTES: u64 = 64 * 1024 * 1024;
/// Output kept in the report per stream.
const TAIL_BYTES: usize = 4 * 1024;

/// Runs the checks of the project containing `path` with `runner`.
///
/// `tier`: `None` runs every check; `Some(Fast)` only fast ones.
/// `against`: a file saved from `speccoder --json verify`.
pub fn verify(
    path: &Path,
    tier: Option<Tier>,
    against: Option<&Path>,
    runner: &dyn CommandRunner,
) -> CommandResult {
    let config_error = |msg: String| CommandResult::new("verify", CommandStatus::ConfigError, &msg);
    let baseline = match against
        .map(|p| {
            let root = path
                .canonicalize()
                .ok()
                .and_then(|p| find_config(&p))
                .unwrap_or_default();
            load_baseline(p, &root)
        })
        .transpose()
    {
        Ok(b) => b,
        Err(e) => return config_error(e),
    };
    let mut report = match run_checks(path, tier, runner) {
        Ok(r) => r,
        // No check at all: add one — `init` would only say the file exists.
        Err(e) if e.starts_with("no applicable checks") => {
            return config_error(e).with_code("NO-CHECKS");
        }
        Err(e) => return config_error(e),
    };
    report.comparison = baseline.as_ref().map(|b| compare(b, &report));
    report.passed = passes(&report.checks, report.comparison.as_ref());

    let status = if report.passed {
        CommandStatus::Ok
    } else {
        CommandStatus::VerificationFailed
    };
    let message = summary(&report);
    CommandResult::new("verify", status, &message).with_data(CommandData::Verify(Box::new(report)))
}

/// Runs the applicable checks of the project containing `path` and returns
/// the report (no comparison yet). `Err` is a configuration problem: no
/// `speccoder.toml`, an invalid one, or nothing applicable — never a pass.
pub fn run_checks(
    path: &Path,
    tier: Option<Tier>,
    runner: &dyn CommandRunner,
) -> Result<VerifyReport, String> {
    let root = path.canonicalize().ok().and_then(|p| find_config(&p)).ok_or_else(|| {
        format!(
            "no {} at or above {}; declare checks first, e.g. [checks.test] argv = [\"cargo\", \"test\"]",
            config::FILE,
            path.display()
        )
    })?;
    let ws = Workspace::at(&root).ok_or_else(|| format!("cannot open {}", root.display()))?;
    let checks = ws
        .read_text(config::FILE)
        .map_err(|e| format!("{e:?}"))
        .and_then(|t| config::parse(&t))
        .map_err(|e| format!("{}: {e}", config::FILE))?;

    let env = scrubbed_env(std::env::vars());
    let results: Vec<CheckResult> = checks
        .iter()
        .filter(|c| tier.is_none_or(|t| c.tier == t))
        .map(|c| {
            rerun_once(run_check(&ws, c, &env, runner), || {
                run_check(&ws, c, &env, runner)
            })
        })
        .collect();
    if results.iter().all(|r| r.status == CheckStatus::Skipped) {
        return Err(format!(
            "no applicable checks in {}{} — nothing would be verified",
            config::FILE,
            tier.map(|t| format!(" for tier {}", t.as_str()))
                .unwrap_or_default()
        ));
    }
    let passed = passes(&results, None);
    Ok(VerifyReport {
        root: ws.root().display().to_string(),
        tier: tier.map_or("all", Tier::as_str).to_owned(),
        checks: results,
        comparison: None,
        passed,
    })
}

/// The argv of every configured check (for the command policy).
pub fn check_argvs(root: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(root.join(config::FILE))
        .ok()
        .and_then(|t| config::parse(&t).ok())
        .map(|cs| cs.into_iter().map(|c| c.argv).collect())
        .unwrap_or_default()
}

/// The nearest ancestor of `start` (inclusive) holding `speccoder.toml`.
pub fn find_config(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|d| d.join(config::FILE).is_file())
        .map(Path::to_path_buf)
}

fn run_check(
    ws: &Workspace,
    check: &Check,
    env: &[(String, String)],
    runner: &dyn CommandRunner,
) -> CheckResult {
    let mut result = CheckResult {
        name: check.name.clone(),
        argv: check.argv.clone(),
        status: CheckStatus::Skipped,
        exit_code: None,
        duration_ms: 0,
        junit: check.junit.is_some(),
        tests: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        truncated: false,
        note: None,
        flaky: false,
    };
    if let Some(path) = &check.when_exists
        && ws.resolve(path).is_err()
    {
        result.note = Some(format!("not applicable: {path} does not exist"));
        return result;
    }
    let cwd = match ws.resolve(&check.cwd) {
        Ok(p) if p.is_dir() => p,
        _ => {
            result.status = CheckStatus::Error;
            result.note = Some(format!(
                "cwd `{}` is not a directory inside the project",
                check.cwd
            ));
            return result;
        }
    };

    // A report left from before (or planted) is removed: only what this
    // run writes is read.
    // A tracked file is never a run's report (and is never deleted).
    let mut tracked_report = false;
    if let Some(report) = &check.junit
        && let Ok(path) = ws.resolve(report)
        && path.is_file()
    {
        tracked_report = crate::git::Git::at(ws.root())
            .succeeds(&["ls-files", "--error-unmatch", "--", report])
            .unwrap_or(false);
        if !tracked_report {
            let _ = std::fs::remove_file(&path);
        }
    }
    let outcome = runner.run(&Invocation {
        argv: check.argv.clone(),
        cwd,
        env: env.to_vec(),
        timeout: check.timeout,
        output_cap: OUTPUT_CAP,
    });
    result.duration_ms = u64::try_from(outcome.duration.as_millis()).unwrap_or(u64::MAX);
    result.exit_code = outcome.exit_code;
    result.stdout_tail = tail(&outcome.stdout.kept);
    result.stderr_tail = tail(&outcome.stderr.kept);
    result.truncated = outcome.stdout.truncated() || outcome.stderr.truncated();
    result.status = if let Some(e) = &outcome.spawn_error {
        result.note = Some(e.clone());
        CheckStatus::Error
    } else if outcome.timed_out {
        result.note = Some(format!("timed out after {}s", check.timeout.as_secs()));
        CheckStatus::TimedOut
    } else if outcome.exit_code == Some(0) {
        CheckStatus::Passed
    } else if crate::init::no_tests(
        &check.argv,
        outcome.exit_code,
        &format!("{}\n{}", result.stdout_tail, result.stderr_tail),
    ) {
        // An empty suite (pytest's exit 5) is no tests yet, as `init` reads
        // it — nothing to regress — like nextest's `--no-tests=warn`.
        result.note = Some("no tests yet".to_owned());
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };

    if let Some(report) = check.junit.as_ref().filter(|_| tracked_report) {
        result
            .note
            .get_or_insert(format!("{report} is tracked by git: not a run's report"));
    } else if let Some(report) = &check.junit {
        let finished = outcome.started + outcome.duration;
        match fresh_report(ws, report, outcome.started, finished) {
            Ok(xml) => match junit::parse(&xml) {
                Ok(tests) => result.tests = Some(tests),
                Err(e) => result.note = Some(format!("{report}: {e}")),
            },
            Err(e) => {
                result.note.get_or_insert(e);
            }
        }
    }
    result
}

/// Reads a JUnit report only if it was written by this run (not a leftover).
fn fresh_report(
    ws: &Workspace,
    rel: &str,
    started: std::time::SystemTime,
    finished: std::time::SystemTime,
) -> Result<String, String> {
    let path = ws
        .resolve(rel)
        .map_err(|_| format!("no test report at {rel}"))?;
    let modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("{rel}: {e}"))?;
    // Allow one second of filesystem timestamp granularity.
    let since = |t: std::time::SystemTime| {
        t.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    };
    if since(modified) + 1 < since(started) {
        return Err(format!("{rel} predates this run; ignored"));
    }
    // Written after the run ended: not the run's report.
    if since(modified) > since(finished) + 1 {
        return Err(format!("{rel} is dated after this run ended; ignored"));
    }
    ws.read_text_limited(rel, MAX_REPORT_BYTES)
        .map_err(|e| format!("{rel}: {e:?}"))
}

/// The last part of an output stream, without terminal escape sequences
/// (reports are read in logs and JSON, not terminals).
fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(TAIL_BYTES);
    strip_ansi(&String::from_utf8_lossy(&bytes[start..]))
}

/// Removes ESC sequences: CSI (`ESC [ … final`) and two-byte escapes (`ESC (B`).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // Parameters and intermediates, then one final byte in @..~.
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // Charset designations like `ESC ( B` carry one more byte.
            Some(_) if chars.peek().is_some_and(char::is_ascii_alphanumeric) => {
                chars.next();
            }
            Some(_) | None => {}
        }
    }
    out
}

fn load_baseline(path: &Path, root: &Path) -> Result<VerifyReport, String> {
    let bad = |why: &str| format!("baseline {}: {why}", path.display());
    let meta = std::fs::metadata(path).map_err(|e| bad(&e.to_string()))?;
    if meta.len() > MAX_BASELINE_BYTES {
        return Err(bad("too large"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| bad(&e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| bad(&e.to_string()))?;
    if value["schema_version"] != SCHEMA_VERSION || value["command"] != "verify" {
        return Err(bad(
            "not a `speccoder --json verify` result of schema version 1",
        ));
    }
    let report: VerifyReport =
        serde_json::from_value(value["data"].clone()).map_err(|e| bad(&e.to_string()))?;
    if Path::new(&report.root) != root {
        return Err(bad(&format!(
            "recorded for {}, not this project",
            report.root
        )));
    }
    Ok(report)
}

fn summary(r: &VerifyReport) -> String {
    let count = |s: CheckStatus| r.checks.iter().filter(|c| c.status == s).count();
    let failed =
        count(CheckStatus::Failed) + count(CheckStatus::TimedOut) + count(CheckStatus::Error);
    let mut s = format!(
        "{} passed, {failed} failed, {} skipped",
        count(CheckStatus::Passed),
        count(CheckStatus::Skipped)
    );
    if let Some(c) = &r.comparison {
        s.push_str(&format!(
            "; {} regressions vs baseline",
            c.regressions.len()
        ));
    }
    s
}

/// A failed check runs once more: failing again, the second run stands;
/// passing, it is `flaky` — named, and never task evidence. Timeouts and
/// errors are not re-run.
pub fn rerun_once(first: CheckResult, again: impl FnOnce() -> CheckResult) -> CheckResult {
    if first.status != CheckStatus::Failed {
        return first;
    }
    let mut second = again();
    if second.status == CheckStatus::Passed {
        let last = format!("{}\n{}", first.stdout_tail, first.stderr_tail)
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_owned();
        second.flaky = true;
        second.note = Some(if last.is_empty() {
            "failed once, passed when run again".to_owned()
        } else {
            format!("failed once ({last}), passed when run again")
        });
    }
    second
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_terminal_escapes() {
        let colored = "\u{1b}(B\u{1b}[m\u{1b}[32m+    ] {\u{1b}[0m done";
        assert_eq!(super::strip_ansi(colored), "+    ] { done");
    }

    #[test]
    fn a_failed_check_runs_once_more() {
        let result = |status: CheckStatus, out: &str| CheckResult {
            name: "test".to_owned(),
            argv: vec!["t".to_owned()],
            status,
            exit_code: None,
            duration_ms: 0,
            junit: false,
            tests: None,
            stdout_tail: out.to_owned(),
            stderr_tail: String::new(),
            truncated: false,
            note: None,
            flaky: false,
        };
        let passed = result(CheckStatus::Passed, "");
        assert!(!rerun_once(passed.clone(), || unreachable!()).flaky);
        let timed_out = result(CheckStatus::TimedOut, "");
        assert_eq!(
            rerun_once(timed_out, || unreachable!()).status,
            CheckStatus::TimedOut
        );
        let flaky = rerun_once(result(CheckStatus::Failed, "boom\n"), || passed.clone());
        assert!(flaky.flaky);
        assert!(flaky.note.as_deref().unwrap().contains("boom"));
        let twice = rerun_once(result(CheckStatus::Failed, "a"), || {
            result(CheckStatus::Failed, "b")
        });
        assert_eq!((twice.flaky, twice.stdout_tail.as_str()), (false, "b"));
    }
}
