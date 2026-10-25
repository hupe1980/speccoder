//! Drift: a tick judged against an earlier spec says so, and says
//! which of its requirements changed.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_core::{CommandData, StatusReport};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
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
}

const SPEC: &str = "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in with a password.\n- **FR-002**: Users MUST log out.\n";

/// The graph fixture with a spec, T004 citing FR-001, committed on the
/// default branch; `speccoder/001-graph` holds T004's verdict commit.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
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
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    let tasks = r.join("specs/001-graph/tasks.md");
    let text = fs::read_to_string(&tasks).unwrap().replace(
        "T004 [P] [US1] Contract test for login",
        "T004 [P] [US1] Contract test for login per FR-001",
    );
    fs::write(&tasks, text).unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    git(r, &["checkout", "-q", "-b", "speccoder/001-graph"]);
    git(
        r,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "speccoder: T004 Contract test\n\nContract test\n\nSpec-Task: 001-graph/T004\nVerdict: verified\nRun: r1\n",
        ],
    );
    git(r, &["checkout", "-q", "main"]);
    dir
}

fn report(r: &Path) -> (StatusReport, String) {
    let res = speccoder_runtime::status::report(r, Some("001-graph"));
    let Some(CommandData::Report(rep)) = res.data() else {
        panic!("{}", res.message())
    };
    ((**rep).clone(), res.message().to_owned())
}

fn commit(r: &Path, path: &str, text: &str) {
    fs::write(r.join(path), text).unwrap();
    git(r, &["commit", "-q", "-am", "edit"]);
}

#[test]
fn an_unchanged_spec_is_no_drift() {
    let r = repo();
    let (rep, message) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    assert_eq!(t4.state, "verified", "{message}");
    assert!(t4.drift.is_none(), "{:?}", t4.drift);
    assert_eq!(rep.totals.stale, 0);
    assert!(!message.contains("spec changed"), "{message}");
}

#[test]
fn a_reworded_requirement_makes_the_tick_that_cites_it_stale() {
    let r = repo();
    commit(
        r.path(),
        "specs/001-graph/spec.md",
        &SPEC.replace("with a password", "with a passkey"),
    );
    let (rep, message) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    let d = t4.drift.as_ref().expect("drift");
    assert!(d.stale());
    assert_eq!(d.files, ["specs/001-graph/spec.md"]);
    assert_eq!(d.requirements.len(), 1, "only what T004 cites: {d:?}");
    let fr = &d.requirements[0];
    assert_eq!(fr.id, "FR-001");
    assert!(fr.then.as_deref().unwrap().contains("password"));
    assert!(fr.now.as_deref().unwrap().contains("passkey"));
    // The verdict stays what the evidence showed; the report says for what.
    assert_eq!(t4.state, "verified");
    assert_eq!(rep.totals.stale, 1);
    assert!(
        message.contains("1 tick(s) proven against an earlier wording"),
        "{message}"
    );
    let row = rep
        .trace
        .iter()
        .find(|row| row.requirement == "FR-001")
        .unwrap();
    assert!(row.changed);
    assert!(
        !rep.trace
            .iter()
            .any(|row| row.requirement == "FR-002" && row.changed)
    );
}

#[test]
fn a_removed_requirement_is_drift_with_nothing_now() {
    let r = repo();
    commit(
        r.path(),
        "specs/001-graph/spec.md",
        "# Spec\n\n## Requirements\n\n- **FR-002**: Users MUST log out.\n",
    );
    let (rep, _) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    let fr = &t4.drift.as_ref().unwrap().requirements[0];
    assert_eq!((fr.id.as_str(), fr.now.as_deref()), ("FR-001", None));
}

#[test]
fn a_change_elsewhere_in_the_intent_is_drift_but_not_stale() {
    let r = repo();
    commit(
        r.path(),
        "specs/001-graph/plan.md",
        "# Plan\n\nRust and SQLite.\n",
    );
    let (rep, message) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    let d = t4.drift.as_ref().expect("drift");
    assert!(!d.stale());
    assert_eq!(d.files, ["specs/001-graph/plan.md"]);
    assert_eq!(rep.totals.stale, 0);
    assert!(
        message.contains("the spec changed since 1 tick(s), not in what they cite"),
        "{message}"
    );
    // The constitution speaks for every feature.
    commit(
        r.path(),
        ".specify/memory/constitution.md",
        "# Constitution\n\nv2\n",
    );
    let (rep, _) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    assert!(
        t4.drift
            .as_ref()
            .unwrap()
            .files
            .contains(&".specify/memory/constitution.md".to_owned())
    );
}

#[test]
fn what_does_not_speak_for_the_feature_is_no_drift() {
    let r = repo();
    // Uncommitted: no run reads it yet.
    fs::write(
        r.path().join("specs/001-graph/spec.md"),
        SPEC.replace("password", "passkey"),
    )
    .unwrap();
    // Another feature, the current-feature pointer, the task list itself.
    fs::create_dir_all(r.path().join("specs/002-other")).unwrap();
    fs::write(r.path().join("specs/002-other/spec.md"), "# Other\n").unwrap();
    fs::write(
        r.path().join(".specify/feature.json"),
        "{\"feature_directory\":\"specs/002-other\"}\n",
    )
    .unwrap();
    git(
        r.path(),
        &["add", "specs/002-other", ".specify/feature.json"],
    );
    git(r.path(), &["commit", "-q", "-m", "other"]);
    let (rep, message) = report(r.path());
    let t4 = rep.tasks.iter().find(|t| t.id == "T004").unwrap();
    assert!(t4.drift.is_none(), "{:?} {message}", t4.drift);
}

#[test]
fn a_stale_tick_offers_the_re_check() {
    let r = repo();
    let none = speccoder_runtime::actions::available(r.path(), Some("001-graph"), None);
    assert!(
        !none
            .iter()
            .any(|a| a.run == Some(speccoder_core::actions::Runnable::Converge))
    );
    commit(
        r.path(),
        "specs/001-graph/spec.md",
        &SPEC.replace("with a password", "with a passkey"),
    );
    let actions = speccoder_runtime::actions::available(r.path(), Some("001-graph"), None);
    let a = actions
        .iter()
        .find(|a| a.run == Some(speccoder_core::actions::Runnable::Converge))
        .expect("re-check");
    assert_eq!(
        (a.group.as_str(), a.command.as_str()),
        ("next", "speccoder run --converge")
    );
    assert!(a.why.contains("T004"), "{}", a.why);
}
