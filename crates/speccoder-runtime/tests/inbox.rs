//! The inbox (F-086): every decision waiting for a person, across features,
//! the most urgent first, each with the actions that decide it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use speccoder_core::actions::Runnable;
use speccoder_core::{CommandData, InboxReport};

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

fn write(r: &Path, path: &str, text: &str) {
    let p = r.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

const SPEC: &str = "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n";

/// A Spec Kit project with two features, each one task citing FR-001 and
/// ticked by a verdict commit on its `speccoder/` branch.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, ".specify/memory/constitution.md", "# Constitution\n");
    write(r, ".specify/init-options.json", "{}\n");
    for f in ["001-login", "002-logout"] {
        write(r, &format!("specs/{f}/spec.md"), SPEC);
        write(r, &format!("specs/{f}/plan.md"), "# Plan\n");
        write(
            r,
            &format!("specs/{f}/tasks.md"),
            "# Tasks\n\n## Phase 1: Core\n\n- [x] T001 Implement login per FR-001\n",
        );
    }
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    // The person approved both specs (F-087) and committed the review.
    for f in ["001-login", "002-logout"] {
        let res = speccoder_runtime::review::decide(r, Some(f), &[], "approve", None, "cli");
        assert_eq!(
            res.status(),
            speccoder_core::CommandStatus::Ok,
            "{}",
            res.message()
        );
    }
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "review"]);
    for f in ["001-login", "002-logout"] {
        git(r, &["checkout", "-q", "-b", &format!("speccoder/{f}")]);
        git(
            r,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!(
                    "speccoder: T001 x\n\nx\n\nSpec-Task: {f}/T001\nVerdict: verified\nRun: r1\n"
                ),
            ],
        );
        git(r, &["checkout", "-q", "main"]);
    }
    dir
}

fn inbox(r: &Path) -> (InboxReport, String) {
    let res = speccoder_runtime::inbox::inbox(r, None);
    let Some(CommandData::Inbox(i)) = res.data() else {
        panic!("{}", res.message())
    };
    ((**i).clone(), res.message().to_owned())
}

#[test]
fn finished_features_wait_to_be_handed_off() {
    let r = repo();
    let (i, message) = inbox(r.path());
    assert_eq!(i.features, 2);
    let kinds: Vec<(&str, &str)> = i
        .items
        .iter()
        .map(|x| (x.kind.as_str(), x.feature.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [("handoff", "001-login"), ("handoff", "002-logout")],
        "{message}"
    );
    let a = &i.items[0].actions[0];
    assert_eq!(a.run, Some(Runnable::PullRequest));
    assert!(
        a.confirm.is_some(),
        "it leaves the machine: confirmed first"
    );
    // Two features: each command says which.
    assert_eq!(a.command, "speccoder pr --yes --feature 001-login");
    assert!(
        message.starts_with("2 decisions waiting in 2 features"),
        "{message}"
    );
}

#[test]
fn a_changed_spec_comes_before_a_handoff_and_replaces_it() {
    let r = repo();
    write(
        r.path(),
        "specs/002-logout/spec.md",
        &SPEC.replace("log in", "log in with a passkey"),
    );
    git(r.path(), &["commit", "-q", "-am", "spec"]);
    let (i, _) = inbox(r.path());
    let kinds: Vec<(&str, &str)> = i
        .items
        .iter()
        .map(|x| (x.kind.as_str(), x.feature.as_str()))
        .collect();
    // A stale feature is not handed off: its evidence is for the old wording;
    // the reworded requirement waits for the person's review again.
    assert_eq!(
        kinds,
        [
            ("stale", "002-logout"),
            ("review", "002-logout"),
            ("handoff", "001-login")
        ]
    );
    let stale = &i.items[0];
    assert!(stale.detail.contains("T001 (FR-001)"), "{}", stale.detail);
    assert_eq!(stale.actions[0].run, Some(Runnable::Converge));
    assert_eq!(
        stale.actions[0].command,
        "speccoder run --converge --feature 002-logout"
    );
}

#[test]
fn uncommitted_plans_wait_to_be_committed() {
    let r = repo();
    write(r.path(), "specs/001-login/plan.md", "# Plan\n\nRust.\n");
    let (i, _) = inbox(r.path());
    let commit = i
        .items
        .iter()
        .find(|x| x.kind == "commit")
        .expect("commit item");
    assert_eq!(commit.feature, "001-login");
    assert!(matches!(
        &commit.actions[0].run,
        Some(Runnable::Commit { paths }) if paths.iter().any(|p| p.ends_with("plan.md"))
    ));
    // Commit before hand-off: a run reads committed files only.
    let at = |k: &str| i.items.iter().position(|x| x.kind == k).unwrap();
    assert!(at("commit") < at("handoff"));
}

#[test]
fn nothing_waiting_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, ".specify/memory/constitution.md", "# Constitution\n");
    write(r, ".specify/init-options.json", "{}\n");
    write(r, "specs/001-login/spec.md", SPEC);
    write(
        r,
        "specs/001-login/tasks.md",
        "# Tasks\n\n## Phase 1: Core\n\n- [ ] T001 Implement login per FR-001\n",
    );
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let (i, _) = inbox(r);
    assert_eq!(i.items.len(), 1, "the unreviewed spec: {:?}", i.items);
    assert_eq!(i.items[0].kind, "review");
    let res = speccoder_runtime::review::decide(r, None, &[], "approve", None, "cli");
    assert_eq!(
        res.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        res.message()
    );
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "review"]);
    let (i, message) = inbox(r);
    assert!(i.items.is_empty(), "{:?}", i.items);
    assert!(message.starts_with("nothing waits for you"), "{message}");
}

/// `repo()` with T002 and T003 open in 001-login, on its branch (where the
/// report reads the task list).
fn with_open_task() -> tempfile::TempDir {
    let r = repo();
    git(r.path(), &["checkout", "-q", "speccoder/001-login"]);
    write(
        r.path(),
        "specs/001-login/tasks.md",
        "# Tasks\n\n## Phase 1: Core\n\n- [x] T001 Implement login per FR-001\n- [ ] T002 Lock out after five failures\n- [ ] T003 Remember the device\n",
    );
    git(r.path(), &["commit", "-q", "-am", "tasks"]);
    git(r.path(), &["checkout", "-q", "main"]);
    r
}

#[test]
fn a_held_ruling_and_a_suspected_shortcut_are_their_own_decisions() {
    let r = with_open_task();
    let state = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // T003: the gaming monitor stopped it.
    speccoder_runtime::attempts::save(
        r.path(),
        tmp.path(),
        "001-login",
        "T003",
        &speccoder_runtime::attempts::Record {
            run: Some("r2"),
            outcome: "suspected_gaming",
            reason: "the test was rewritten to assert nothing",
            pending: None,
            usage: None,
        },
    )
    .unwrap();
    // T002: verified, held — its tests reject an alternative the spec allows.
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let permitted = speccoder_core::strength::Permitted {
        id: "permitted-1".to_owned(),
        scenario: "US1.2".to_owned(),
        rationale: "lock out after five or more".to_owned(),
        digest: "sha256:0".to_owned(),
        outcome: speccoder_core::strength::PermittedOutcome::Rejected,
        failing: None,
        patch: None,
    };
    speccoder_runtime::strength::hold(
        &speccoder_runtime::strength::dir(&plane, "001-login", "T002"),
        &[&permitted],
    )
    .unwrap();
    let res = speccoder_runtime::inbox::inbox_with(r.path(), Some("001-login"), Some(state.path()));
    let Some(CommandData::Inbox(i)) = res.data() else {
        panic!("{}", res.message())
    };
    let kinds: Vec<(&str, Option<&str>)> = i
        .items
        .iter()
        .map(|x| (x.kind.as_str(), x.task.as_deref()))
        .collect();
    // `--feature`: 002-logout's hand-off is not listed; ruling before gaming.
    assert_eq!(
        kinds,
        [("ruling", Some("T002")), ("gaming", Some("T003"))],
        "{}",
        res.message()
    );
    assert_eq!(i.features, 1);
    let ruling = &i.items[0];
    assert!(ruling.detail.contains("five or more"), "{}", ruling.detail);
    assert_eq!(
        ruling.actions[0].run,
        Some(Runnable::Rule {
            task: "T002".to_owned()
        })
    );
    assert!(ruling.actions[0].input.is_some(), "a ruling needs a reason");
    let gaming = &i.items[1];
    assert!(
        gaming.detail.contains("assert nothing"),
        "{}",
        gaming.detail
    );
    // One feature: commands carry no --feature.
    assert_eq!(
        gaming.actions[0].command,
        "speccoder task nudge T003 --note \"…\""
    );
}

#[test]
fn an_unknown_feature_is_refused_not_ignored() {
    let r = repo();
    let res = speccoder_runtime::inbox::inbox(r.path(), Some("009-nope"));
    assert_ne!(
        res.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        res.message()
    );
}

#[test]
fn a_change_request_waits_to_be_revised() {
    let r = repo();
    let res = speccoder_runtime::review::decide(
        r.path(),
        Some("001-login"),
        &["FR-001".to_owned()],
        "request",
        Some("name the login methods"),
        "cli",
    );
    assert_eq!(
        res.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        res.message()
    );
    let (i, _) = inbox(r.path());
    let revise = i
        .items
        .iter()
        .find(|x| x.kind == "revise")
        .expect("revise item");
    assert_eq!(revise.feature, "001-login");
    assert!(
        revise.detail.contains("FR-001: name the login methods"),
        "{}",
        revise.detail
    );
    // No revising skill installed here: editing by hand is what is offered.
    assert_eq!(revise.actions.len(), 1);
    assert_eq!(
        revise.actions[0].command,
        "speccoder review --feature 001-login"
    );
    // Revise ranks before the hand-off of the other feature.
    let at = |k: &str| i.items.iter().position(|x| x.kind == k).unwrap();
    assert!(at("revise") < at("handoff"));
}
