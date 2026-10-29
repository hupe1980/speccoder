//! `verify` against small temp projects.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use speccoder_core::verify::{CheckStatus, VerifyReport};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::verify::config::Tier;
use speccoder_runtime::verify::process::{
    CommandRunner, Invocation, LocalRunner, OUTPUT_CAP, scrubbed_env,
};
use speccoder_runtime::verify::verify;

const PASSING_JUNIT: &str = r#"<testsuites><testsuite name="s">
<testcase classname="c" name="a"/><testcase classname="c" name="b"/>
</testsuite></testsuites>"#;
const FAILING_JUNIT: &str = r#"<testsuites><testsuite name="s">
<testcase classname="c" name="a"/><testcase classname="c" name="b"><failure/></testcase>
</testsuite></testsuites>"#;

fn project(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("speccoder.toml"), config).unwrap();
    fs::write(dir.path().join("pass.xml"), PASSING_JUNIT).unwrap();
    fs::write(dir.path().join("fail.xml"), FAILING_JUNIT).unwrap();
    fs::create_dir(dir.path().join("out")).unwrap();
    dir
}

fn report(r: &CommandResult) -> &VerifyReport {
    match r.data() {
        Some(CommandData::Verify(v)) => v,
        other => panic!("no report: {other:?} ({})", r.message()),
    }
}

fn check<'r>(r: &'r VerifyReport, name: &str) -> &'r speccoder_core::verify::CheckResult {
    r.checks.iter().find(|c| c.name == name).unwrap()
}

#[test]
fn reports_passing_failing_and_skipped_checks_with_tests() {
    let dir = project(
        r#"
[checks.unit]
argv = ["cp", "fail.xml", "out/junit.xml"]
junit = "out/junit.xml"

[checks.lint]
argv = ["true"]

[checks.broken]
argv = ["false"]

[checks.node]
argv = ["npm", "test"]
when_exists = "package.json"
"#,
    );
    let r = verify(dir.path(), None, None, &LocalRunner);
    assert_eq!(r.status(), CommandStatus::VerificationFailed);
    assert_eq!(r.exit_code().code(), 20);
    let rep = report(&r);
    assert_eq!(check(rep, "unit").status, CheckStatus::Passed);
    assert_eq!(check(rep, "unit").test_counts(), Some((1, 1, 0)));
    assert_eq!(check(rep, "lint").status, CheckStatus::Passed);
    assert_eq!(check(rep, "broken").status, CheckStatus::Failed);
    assert_eq!(check(rep, "broken").exit_code, Some(1));
    let node = check(rep, "node");
    assert_eq!(node.status, CheckStatus::Skipped);
    assert!(node.note.as_deref().unwrap().contains("package.json"));
}

#[test]
fn passes_when_every_applicable_check_passes() {
    let dir = project(
        "[checks.lint]\nargv = [\"true\"]\n[checks.slow]\nargv = [\"false\"]\ntier = \"full\"\n",
    );
    let r = verify(dir.path(), Some(Tier::Fast), None, &LocalRunner);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(report(&r).checks.len(), 1, "the full-tier check is not run");
}

#[test]
fn missing_config_or_nothing_applicable_is_a_refusal_not_a_pass() {
    let empty = tempfile::tempdir().unwrap();
    let r = verify(empty.path(), None, None, &LocalRunner);
    assert_eq!(r.status(), CommandStatus::ConfigError);
    assert_eq!(r.exit_code().code(), 13);

    let none =
        project("[checks.node]\nargv = [\"npm\", \"test\"]\nwhen_exists = \"package.json\"\n");
    let r = verify(none.path(), None, None, &LocalRunner);
    assert_eq!(r.status(), CommandStatus::ConfigError);
    assert!(r.message().contains("nothing would be verified"));

    let bad = project("[checks.x]\nargv = [\"true\"]\nshell = true\n");
    assert_eq!(
        verify(bad.path(), None, None, &LocalRunner).status(),
        CommandStatus::ConfigError
    );
}

#[test]
fn command_not_found_is_an_error_check() {
    let dir = project("[checks.ghost]\nargv = [\"speccoder-no-such-program\"]\n");
    let r = verify(dir.path(), None, None, &LocalRunner);
    let ghost = check(report(&r), "ghost");
    assert_eq!(ghost.status, CheckStatus::Error);
    assert!(ghost.note.as_deref().unwrap().contains("could not start"));
}

#[test]
fn stale_junit_reports_are_ignored() {
    let dir = project("[checks.unit]\nargv = [\"true\"]\njunit = \"pass.xml\"\n");
    let status = Command::new("touch")
        .args(["-t", "202001010000", "pass.xml"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    let r = verify(dir.path(), None, None, &LocalRunner);
    let unit = check(report(&r), "unit");
    assert!(unit.tests.is_none());
    // Removed before the run (it was not the run's): no report was written.
    assert!(
        unit.note.as_deref().unwrap().contains("no test report"),
        "{:?}",
        unit.note
    );
    assert!(
        !unit.is_evidence(),
        "a passing check without its report is no evidence"
    );
}

#[test]
fn baseline_comparison_finds_regressions_and_pre_existing_failures() {
    let dir = project(
        "[checks.unit]\nargv = [\"cp\", \"pass.xml\", \"out/junit.xml\"]\njunit = \"out/junit.xml\"\n",
    );
    let base = verify(dir.path(), None, None, &LocalRunner);
    assert_eq!(base.status(), CommandStatus::Ok);
    let saved = dir.path().join("base.json");
    fs::write(&saved, serde_json::to_string(&base).unwrap()).unwrap();

    // Test `b` now fails: a regression.
    fs::write(
        dir.path().join("speccoder.toml"),
        "[checks.unit]\nargv = [\"cp\", \"fail.xml\", \"out/junit.xml\"]\njunit = \"out/junit.xml\"\n",
    )
    .unwrap();
    let now = verify(dir.path(), None, Some(&saved), &LocalRunner);
    assert_eq!(now.status(), CommandStatus::VerificationFailed);
    let cmp = report(&now).comparison.as_ref().unwrap();
    assert_eq!(cmp.regressions.len(), 1);
    assert_eq!(cmp.regressions[0].id, "unit/c::b");

    // Against a failing baseline, the same failure is pre-existing, not blamed.
    let failing_base = dir.path().join("failing.json");
    fs::write(&failing_base, serde_json::to_string(&now).unwrap()).unwrap();
    let again = verify(dir.path(), None, Some(&failing_base), &LocalRunner);
    let cmp = report(&again).comparison.as_ref().unwrap();
    assert_eq!(cmp.pre_existing, ["unit/c::b"]);
    assert_eq!(again.status(), CommandStatus::Ok, "{}", again.message());
}

#[test]
fn baseline_from_another_project_is_rejected() {
    let a = project("[checks.lint]\nargv = [\"true\"]\n");
    let b = project("[checks.lint]\nargv = [\"true\"]\n");
    let saved = a.path().join("base.json");
    fs::write(
        &saved,
        serde_json::to_string(&verify(a.path(), None, None, &LocalRunner)).unwrap(),
    )
    .unwrap();
    let r = verify(b.path(), None, Some(&saved), &LocalRunner);
    assert_eq!(r.status(), CommandStatus::ConfigError);
    assert!(r.message().contains("not this project"), "{}", r.message());
}

// ── US4: the child process environment ─────────────────────────────────────

fn run(
    argv: &[&str],
    env: Vec<(String, String)>,
    timeout: Duration,
) -> speccoder_runtime::verify::process::Outcome {
    LocalRunner.run(&Invocation {
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        cwd: std::env::temp_dir(),
        env,
        timeout,
        output_cap: OUTPUT_CAP,
    })
}

#[test]
fn children_get_no_secrets_but_keep_the_toolchain() {
    let mut vars: Vec<(String, String)> = std::env::vars().collect();
    vars.push(("ANTHROPIC_API_KEY".to_owned(), "sk-secret".to_owned()));
    vars.push(("GITHUB_TOKEN".to_owned(), "ghp-secret".to_owned()));
    vars.push(("SSH_AUTH_SOCK".to_owned(), "/tmp/agent".to_owned()));
    let out = run(&["env"], scrubbed_env(vars), Duration::from_secs(10));
    let env = String::from_utf8(out.stdout.kept).unwrap();
    for planted in [
        "sk-secret",
        "ghp-secret",
        "ANTHROPIC_API_KEY",
        "GITHUB_TOKEN",
    ] {
        assert!(!env.contains(planted), "{planted} leaked: {env}");
    }
    assert!(!env.contains("SSH_AUTH_SOCK"), "{env}");
    assert!(env.lines().any(|l| l == "CI=1"));
    assert!(env.lines().any(|l| l == "GIT_TERMINAL_PROMPT=0"));
    assert!(env.lines().any(|l| l.starts_with("PATH=")));
}

#[test]
fn stdin_is_closed() {
    let path = std::env::var("PATH").unwrap_or_default();
    let started = Instant::now();
    let out = run(
        &["cat"],
        vec![("PATH".to_owned(), path)],
        Duration::from_secs(10),
    );
    assert_eq!(out.exit_code, Some(0));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn timeout_kills_the_whole_process_group() {
    // A unique duration so pgrep finds only this test's grandchild.
    let marker = "sleep 31.4159";
    let path = std::env::var("PATH").unwrap_or_default();
    let out = run(
        &["sh", "-c", &format!("{marker} & {marker}")],
        vec![("PATH".to_owned(), path)],
        Duration::from_millis(300),
    );
    assert!(out.timed_out);
    assert_eq!(out.exit_code, None);
    std::thread::sleep(Duration::from_millis(200));
    let left = Command::new("pgrep").args(["-f", marker]).output().unwrap();
    assert!(
        left.stdout.is_empty(),
        "left running: {}",
        String::from_utf8_lossy(&left.stdout)
    );
}

#[test]
fn output_is_capped_and_marked() {
    let path = std::env::var("PATH").unwrap_or_default();
    let out = LocalRunner.run(&Invocation {
        argv: ["head", "-c", "5000", "/dev/zero"]
            .map(String::from)
            .to_vec(),
        cwd: Path::new("/").to_path_buf(),
        env: vec![("PATH".to_owned(), path)],
        timeout: Duration::from_secs(10),
        output_cap: 1000,
    });
    assert_eq!(out.stdout.kept.len(), 1000);
    assert_eq!(out.stdout.total, 5000);
    assert!(out.stdout.truncated());
}

/// A report planted before the check — or dated after
/// it ended — is never read as the run's.
#[test]
fn a_planted_or_future_dated_report_is_not_the_run_s() {
    let dir = project(
        r#"
[checks.planted]
argv = ["true"]
junit = "out/planted.xml"

[checks.future]
argv = ["sh", "-c", "cp pass.xml out/future.xml && touch -t 209901010000 out/future.xml"]
junit = "out/future.xml"
"#,
    );
    fs::write(dir.path().join("out/planted.xml"), PASSING_JUNIT).unwrap();
    // A future mtime would pass a lower bound alone.
    assert!(
        Command::new("touch")
            .args(["-t", "209901010000"])
            .arg(dir.path().join("out/planted.xml"))
            .status()
            .unwrap()
            .success()
    );
    let r = verify(dir.path(), None, None, &LocalRunner);
    let r = report(&r);
    assert_eq!(
        check(r, "planted").tests,
        None,
        "{:?}",
        check(r, "planted").note
    );
    assert!(
        !dir.path().join("out/planted.xml").exists(),
        "removed before the check ran"
    );
    assert_eq!(
        check(r, "future").tests,
        None,
        "{:?}",
        check(r, "future").note
    );
}

#[test]
fn an_empty_pytest_suite_is_no_tests_yet_and_another_failure_stays_one() {
    // A stand-in `pytest` with pytest's exit codes: 5 is "no tests
    // collected", 1 is a failing test.
    let dir = project(
        r#"
[checks.empty]
argv = ["bin/pytest", "5"]

[checks.failing]
argv = ["bin/pytest", "1"]
"#,
    );
    fs::create_dir(dir.path().join("bin")).unwrap();
    fs::write(dir.path().join("bin/pytest"), "#!/bin/sh\nexit \"$1\"\n").unwrap();
    Command::new("chmod")
        .args(["+x", "bin/pytest"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    let r = verify(dir.path(), None, None, &LocalRunner);
    let rep = report(&r);
    let empty = check(rep, "empty");
    assert_eq!(empty.status, CheckStatus::Passed);
    assert_eq!(empty.note.as_deref(), Some("no tests yet"));
    assert_eq!(check(rep, "failing").status, CheckStatus::Failed);
}
