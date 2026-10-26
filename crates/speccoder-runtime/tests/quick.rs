//! Quick changes: a small change becomes a story, an approved
//! requirement and a ready task in the standing unit, committed — then it
//! runs like any other task.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use speccoder_core::{CommandData, CommandStatus};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=ada@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for (p, t) in [
        (".specify/memory/constitution.md", "# Constitution\n"),
        (".specify/init-options.json", "{}\n"),
        (
            "specs/001-login/spec.md",
            "# Spec\n\n- **FR-001**: Users MUST log in.\n",
        ),
        (
            "specs/001-login/tasks.md",
            "# Tasks\n\n## Phase 1: Core\n\n- [ ] T001 Log in per FR-001\n",
        ),
        (
            "specs/001-login/notes.txt",
            "uncommitted, and not ours to commit\n",
        ),
    ] {
        fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
        fs::write(r.join(p), t).unwrap();
    }
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(
        r,
        &[
            "add",
            ".specify",
            "specs/001-login/spec.md",
            "specs/001-login/tasks.md",
        ],
    );
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn record(r: &Path, request: &str, checks: &[&str]) -> speccoder_core::CommandResult {
    let checks: Vec<String> = checks.iter().map(|c| (*c).to_owned()).collect();
    speccoder_runtime::quick::change(
        r,
        request,
        &checks,
        false,
        speccoder_core::actions::Via::Terminal,
        &speccoder_runtime::RunOptions::default(),
    )
}

#[test]
fn a_quick_change_is_a_story_an_approved_requirement_and_a_ready_task() {
    let r = repo();
    let res = record(
        r.path(),
        "Show the version in the footer. It reads v1.2.",
        &["every page's footer shows the version"],
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(
        res.message()
            .contains("as FR-001 and T001 in 002-quick-changes"),
        "{}",
        res.message()
    );
    let spec = fs::read_to_string(r.path().join("specs/002-quick-changes/spec.md")).unwrap();
    assert!(
        spec.contains("### User Story 1 - Show the version in the footer (Priority: P1)"),
        "{spec}"
    );
    assert!(
        spec.contains("1. every page's footer shows the version"),
        "{spec}"
    );
    assert!(
        spec.contains("- **FR-001**: Show the version in the footer. It reads v1.2."),
        "{spec}"
    );
    // A second one appends: its own story, requirement and task.
    let res = record(r.path(), "Make the login button blue", &[]);
    assert!(
        res.message().contains("FR-002 and T002"),
        "{}",
        res.message()
    );
    let tasks = fs::read_to_string(r.path().join("specs/002-quick-changes/tasks.md")).unwrap();
    assert!(
        tasks.contains("- [ ] T002 [US2] Make the login button blue per FR-002"),
        "{tasks}"
    );
    // Approved, committed — exactly its files — and ready to run.
    let rv = speccoder_runtime::review::report(r.path(), Some("002-quick-changes"));
    let Some(CommandData::Review(rv)) = rv.data() else {
        panic!()
    };
    assert!(rv.totals.complete(), "{:?}", rv.totals);
    assert_eq!(
        git(r.path(), &["log", "-1", "--format=%s"]).trim(),
        "spec(002-quick-changes): quick change: Make the login button blue"
    );
    assert!(
        git(
            r.path(),
            &["status", "--porcelain", "specs/002-quick-changes"]
        )
        .trim()
        .is_empty()
    );
    assert!(
        git(r.path(), &["status", "--porcelain"]).contains("notes.txt"),
        "someone else's file stays uncommitted"
    );
    let report = speccoder_runtime::status::report(r.path(), Some("002-quick-changes"));
    let Some(CommandData::Report(rep)) = report.data() else {
        panic!("{}", report.message())
    };
    let states: Vec<&str> = rep.tasks.iter().map(|t| t.state.as_str()).collect();
    assert_eq!(
        states,
        ["ready", "ready"],
        "independent: neither waits for the other"
    );
}

#[test]
fn an_empty_request_is_refused() {
    let r = repo();
    let res = record(r.path(), "   ", &[]);
    assert_eq!(res.status(), CommandStatus::UsageError);
    assert!(!r.path().join("specs/002-quick-changes").exists());
}

#[test]
fn the_acceptance_author_reads_the_change_and_its_conditions() {
    let r = repo();
    let _ = record(
        r.path(),
        "Show the version in the footer",
        &["every page's footer shows the version"],
    );
    let _ = record(r.path(), "Make the login button blue", &[]);
    let inspected = speccoder_runtime::inspect(r.path(), Some("002-quick-changes"));
    let Some(CommandData::Inspect(snap)) = inspected.data() else {
        panic!()
    };
    let format = speccoder_runtime::format::of(snap);
    let first = format.bundle(snap, "T001").unwrap();
    assert!(
        first.story.contains("Show the version in the footer"),
        "{}",
        first.story
    );
    assert!(
        first
            .story
            .contains("every page's footer shows the version"),
        "{}",
        first.story
    );
    // Each task gets its own story, not its neighbour's.
    assert!(!first.story.contains("login button"), "{}", first.story);
    let second = format.bundle(snap, "T002").unwrap();
    assert!(
        second.story.contains("Make the login button blue") && !second.story.contains("footer"),
        "{}",
        second.story
    );
}
