//! External executors: another agent makes
//! the attempt; speccoder judges it from the diff. A shell script stands in
//! for Claude Code or Codex.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::run_with;

const MODEL: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n";
const CHECK: &str = "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// The `graph` fixture with an executor script (kept outside the repository).
fn repo(script: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let agent = bin.path().join("agent.sh");
    fs::write(&agent, format!("#!/bin/sh\nset -e\n{script}\n")).unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let r = dir.path();
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::create_dir_all(r.join("tests")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("tests/existing.rs"), "#[test]\nfn existing() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "{MODEL}{CHECK}[budget]\nmax_attempts = 2\n[executor]\nkind = \"command\"\nargv = [\"sh\", \"{}\"]\n",
            agent.display()
        ),
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    (dir, bin)
}

fn run(repo: &Path, state: &Path) -> CommandResult {
    // No model call is scripted: an external attempt makes none.
    let d: Arc<dyn ModelProvider> = FakeProvider::new();
    run_with(repo, None, "T005", state, d, None)
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[test]
fn an_external_attempt_is_verified_ticked_and_committed() {
    // The agent gets the task on stdin, edits the worktree, says what it did.
    let (r, bin) = repo(
        "cat > \"$(dirname \"$0\")/prompt\"\nprintf 'pub fn ok() {}\\npub fn login() {}\\n' > src/lib.rs\necho 'Added login.'",
    );
    let state = tempfile::tempdir().unwrap();

    let res = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(
        rep.run_id.starts_with("external_command_"),
        "{}",
        rep.run_id
    );
    assert_eq!(rep.attempt_status, "completed");
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert_eq!(
        rep.model_report.as_ref().unwrap()["summary"],
        "Added login."
    );
    assert!(rep.ticked && rep.commit.is_some());

    let prompt = fs::read_to_string(bin.path().join("prompt")).unwrap();
    assert!(prompt.contains("# Task T005"), "{prompt}");
    assert!(prompt.contains("Test files that exist now are frozen"));
    assert!(prompt.contains("BLOCKED:"));
}

#[test]
fn a_frozen_test_the_agent_deletes_fails_the_audit() {
    let (r, _bin) = repo("rm tests/existing.rs\necho 'done'");
    let state = tempfile::tempdir().unwrap();
    let res = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::VerificationFailed);
    let rep = report(&res);
    assert!(
        rep.integrity
            .contains(&"integrity: frozen test deleted: tests/existing.rs".to_owned()),
        "{:?}",
        rep.integrity
    );
}

#[test]
fn blocked_and_crashed_agents_are_reported_honestly() {
    let (r, _bin) = repo("echo 'BLOCKED: the spec contradicts itself'");
    let state = tempfile::tempdir().unwrap();
    let res = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "blocked");
    assert_eq!(rep.reason.as_deref(), Some("the spec contradicts itself"));

    let (r, _bin) = repo("echo 'out of credits' >&2\nexit 3");
    let state = tempfile::tempdir().unwrap();
    let rep_res = run(r.path(), state.path());
    let rep = report(&rep_res);
    assert_eq!(rep.attempt_status, "failed");
    assert!(
        rep.reason
            .as_deref()
            .unwrap_or_default()
            .contains("exited 3"),
        "{:?}",
        rep.reason
    );
}

#[test]
fn a_failed_external_attempt_is_retried_with_the_digest() {
    // First attempt breaks the check; the second sees the digest and fixes it.
    let (r, _bin) = repo(
        "if grep -q 'Previous attempt' -; then\n  printf 'pub fn ok() {}\\npub fn login() {}\\n' > src/lib.rs\n  echo fixed\nelse\n  echo 'pub fn renamed() {}' > src/lib.rs\n  echo first\nfi",
    );
    let state = tempfile::tempdir().unwrap();
    let res = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.earlier.len(), 1);
    assert_eq!(rep.model_report.as_ref().unwrap()["summary"], "fixed");
}
