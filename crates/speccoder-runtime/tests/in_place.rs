//! In-place runs end to end with a scripted model: the person's
//! checkout, intent git-ignored, switched to `speccoder/<feature>` and back.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::RunOptions;

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The `graph` fixture with `specs/` and `.specify/` git-ignored: only the
/// code, the config and `.gitignore` are committed.
fn repo(extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    fs::write(r.join(".gitignore"), "/specs/\n/.specify/\n").unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn adds_login(f: &FakeProvider) {
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }),
    );
    f.will_structure(json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
}

fn in_place() -> RunOptions {
    RunOptions {
        in_place: true,
        ..RunOptions::default()
    }
}

fn run(r: &Path, state: &Path, d: Arc<dyn ModelProvider>, options: &RunOptions) -> CommandResult {
    speccoder_runtime::run_with_options(r, None, "T005", state, d, None, options)
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn tasks(r: &Path) -> String {
    fs::read_to_string(r.join("specs/001-graph/tasks.md")).unwrap()
}

#[test]
fn a_task_ticks_the_ignored_tasks_file_and_the_checkout_returns_to_main() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let main_tip = git(r.path(), &["rev-parse", "main"]);
    let (f, d) = fake();
    adds_login(&f);

    let res = run(r.path(), state.path(), d, &in_place());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked, "{rep:?}");
    assert!(res.message().contains("back on main"), "{}", res.message());

    // The tick is in the checkout's ignored tasks.md.
    assert!(
        tasks(r.path()).contains("- [X] T005"),
        "{}",
        tasks(r.path())
    );
    // Back on main, which did not move.
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(git(r.path(), &["rev-parse", "main"]), main_tip);
    assert_eq!(git(r.path(), &["status", "--porcelain"]), "");

    // The evidence is on the speccoder branch, and says the tick is local.
    let branch = git(
        r.path(),
        &[
            "branch",
            "--list",
            "speccoder/*",
            "--format=%(refname:short)",
        ],
    );
    assert_eq!(branch, "speccoder/001-graph");
    let message = git(r.path(), &["log", "-1", "--format=%B", &branch]);
    for trailer in [
        "Spec-Task:",
        "Verdict:",
        "Mode: in-place",
        "Intent-Digest: sha256:",
        "Task-Line:",
        "Tick: local specs/001-graph/tasks.md",
    ] {
        assert!(message.contains(trailer), "{trailer} missing in\n{message}");
    }
    let code = git(r.path(), &["show", &format!("{branch}:src/lib.rs")]);
    assert!(code.contains("fn login"));

    // `report` reads the local ticks against the commits' Task-Line.
    let state_of = |id: &str| {
        let res = speccoder_runtime::status::report(r.path(), None);
        let Some(CommandData::Report(rep)) = res.data() else {
            panic!("{}", res.message())
        };
        assert_eq!(rep.source, "checkout");
        assert!(
            res.message().contains("ticks are local"),
            "{}",
            res.message()
        );
        rep.tasks.iter().find(|t| t.id == id).unwrap().state.clone()
    };
    assert_eq!(state_of("T005"), "regression_only");
    // A hand tick has no evidence.
    let file = r.path().join("specs/001-graph/tasks.md");
    let text = tasks(r.path());
    fs::write(&file, text.replace("- [ ] T006", "- [x] T006")).unwrap();
    assert_eq!(state_of("T006"), "ticked_without_evidence");
    // A ticked line reworded after its commit is no longer the task judged.
    fs::write(
        &file,
        text.replace("Integration test for login", "Integration test for logout"),
    )
    .unwrap();
    assert_eq!(state_of("T005"), "ticked_without_evidence");
}

#[test]
fn a_second_run_reuses_the_branch() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    adds_login(&f);
    let first = run(r.path(), state.path(), d, &in_place());
    assert_eq!(first.status(), CommandStatus::Ok, "{}", first.message());
    let tip = git(r.path(), &["rev-parse", "speccoder/001-graph"]);

    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/user.rs", "content": "pub struct User;\n" }),
    );
    f.will_structure(json!({ "summary": "user", "status": "done", "files_changed": ["src/user.rs"], "checks_run": [], "notes": [] }));
    let second = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T006",
        state.path(),
        d,
        None,
        &in_place(),
    );
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    assert_eq!(
        git(r.path(), &["rev-parse", "speccoder/001-graph~1"]),
        tip,
        "the second commit builds on the first"
    );
    assert!(tasks(r.path()).contains("- [X] T006"));
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
}

#[test]
fn a_dirty_checkout_is_refused_and_nothing_switches() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    fs::write(r.path().join("src/lib.rs"), "pub fn ok() {}\n// mine\n").unwrap();
    let (_f, d) = fake();

    let res = run(r.path(), state.path(), d, &in_place());
    assert_eq!(
        res.status(),
        CommandStatus::GitPrecondition,
        "{}",
        res.message()
    );
    assert!(
        res.code().map(|c| c.name) == Some("DIRTY-CHECKOUT"),
        "{}",
        res.message()
    );
    let Some(CommandData::InPlace(p)) = res.data() else {
        panic!("{}", res.message())
    };
    assert!(p.paths.iter().any(|r| r.path == "src/lib.rs"), "{p:?}");
    assert_eq!(res.fix().first().map(String::as_str), Some("git stash -u"));
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(git(r.path(), &["branch", "--list", "speccoder/*"]), "");
    assert!(
        fs::read_to_string(r.path().join("src/lib.rs"))
            .unwrap()
            .contains("// mine")
    );
}

#[test]
fn an_intent_edit_before_the_tick_stops_the_task_unticked() {
    // The check rewrites the ignored spec: the judge ran against an intent
    // that is no longer the one the run started with.
    let r = repo(
        "\n[checks.edits-spec]\nargv = [\"sh\", \"-c\", \"echo more >> specs/001-graph/spec.md\"]\n",
    );
    let state = tempfile::tempdir().unwrap();
    let main_tip = git(r.path(), &["rev-parse", "main"]);
    let (f, d) = fake();
    adds_login(&f);

    let res = run(r.path(), state.path(), d, &in_place());
    let rep = report(&res);
    assert!(!rep.ticked, "{rep:?}");
    let reason = rep.reason.clone().unwrap_or_default();
    assert!(reason.contains("INTENT-CHANGED"), "{reason}");
    assert!(reason.contains("specs/001-graph/spec.md"), "{reason}");
    assert_eq!(rep.attempt_status, "intent_changed");
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert!(tasks(r.path()).contains("- [ ] T005"));
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(git(r.path(), &["rev-parse", "main"]), main_tip);
}

#[test]
fn parallel_lanes_are_refused_in_place() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (_f, d) = fake();
    let options = RunOptions {
        parallel: 2,
        ..in_place()
    };
    let res = speccoder_runtime::run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::UsageError, "{}", res.message());
    assert!(
        res.code().map(|c| c.name) == Some("IN-PLACE-PARALLEL"),
        "{}",
        res.message()
    );
    assert_eq!(git(r.path(), &["branch", "--list", "speccoder/*"]), "");
}

#[test]
fn an_interrupted_session_blocks_worktree_runs_until_return() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(r.path()).unwrap();
    let plane_dir = speccoder_runtime::worktree::project_state_dir(state.path(), &root);
    let policy = speccoder_runtime::tools::policy::Policy::new(&[], &[]);
    // A session left by a run that died with work in the checkout.
    speccoder_runtime::in_place::Session::begin(&root, &plane_dir, "001-graph", &policy).unwrap();
    assert_eq!(
        git(r.path(), &["symbolic-ref", "--short", "HEAD"]),
        "speccoder/001-graph"
    );
    fs::write(
        r.path().join("src/lib.rs"),
        "pub fn ok() {}\npub fn half() {}\n",
    )
    .unwrap();
    // The run died: its pid is gone.
    let mut session = speccoder_runtime::in_place::Session::load(&plane_dir).unwrap();
    session.pid = 0x7fff_fff0;
    session.save(&plane_dir).unwrap();
    let status = speccoder_runtime::progress::status_with(r.path(), state.path());
    assert!(
        status
            .message()
            .contains("interrupted in-place run of 001-graph"),
        "{}",
        status.message()
    );
    assert!(
        status.message().contains("returns to main"),
        "{}",
        status.message()
    );
    let next: Vec<&str> = status.next().iter().map(|n| n.command.as_str()).collect();
    assert_eq!(
        next,
        [
            "speccoder run --in-place",
            "speccoder run --in-place --return"
        ]
    );

    let (_f, d) = fake();
    let res = run(r.path(), state.path(), d, &RunOptions::default());
    assert!(
        res.code().map(|c| c.name) == Some("IN-PLACE-SESSION"),
        "{}",
        res.message()
    );
    assert!(
        res.fix().iter().any(|f| f.contains("--return")),
        "{:?}",
        res.fix()
    );

    let back = speccoder_runtime::in_place::return_in_place(r.path(), state.path());
    assert_eq!(back.status(), CommandStatus::Ok, "{}", back.message());
    assert!(
        back.message().contains("back on main"),
        "{}",
        back.message()
    );
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(git(r.path(), &["status", "--porcelain"]), "");
    let refs = git(
        r.path(),
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/speccoder/attempts",
        ],
    );
    assert!(
        !refs.is_empty(),
        "the interrupted work is saved under an attempt ref"
    );
}

#[test]
fn a_run_after_a_kill_between_tasks_returns_to_the_recorded_branch() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(r.path()).unwrap();
    let plane_dir = speccoder_runtime::worktree::project_state_dir(state.path(), &root);
    let policy = speccoder_runtime::tools::policy::Policy::new(&[], &[]);
    speccoder_runtime::in_place::Session::begin(&root, &plane_dir, "001-graph", &policy).unwrap();
    let mut session = speccoder_runtime::in_place::Session::load(&plane_dir).unwrap();
    session.pid = 0x7fff_fff0;
    session.save(&plane_dir).unwrap();

    let (f, d) = fake();
    adds_login(&f);
    let res = run(r.path(), state.path(), d, &in_place());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(report(&res).ticked);
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert!(speccoder_runtime::in_place::Session::load(&plane_dir).is_none());
}

#[test]
fn a_tick_lost_after_its_commit_is_restored_from_the_commit() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    adds_login(&f);
    let first = run(r.path(), state.path(), d, &in_place());
    assert_eq!(first.status(), CommandStatus::Ok, "{}", first.message());
    // Killed between the verdict commit and the local tick.
    let file = r.path().join("specs/001-graph/tasks.md");
    fs::write(&file, tasks(r.path()).replace("- [X] T005", "- [ ] T005")).unwrap();
    let task_of = |id: &str| {
        let res = speccoder_runtime::status::report(r.path(), None);
        let Some(CommandData::Report(rep)) = res.data() else {
            panic!("{}", res.message())
        };
        (
            rep.tasks.iter().find(|t| t.id == id).unwrap().clone(),
            res.message().to_owned(),
        )
    };
    let (t5, message) = task_of("T005");
    assert_eq!(t5.state, "evidence_without_tick", "{message}");
    assert!(t5.commit.is_some() && t5.drift.is_none());
    assert!(
        message.contains("1 committed without their local tick"),
        "{message}"
    );

    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/user.rs", "content": "pub struct User;\n" }),
    );
    f.will_structure(json!({ "summary": "user", "status": "done", "files_changed": ["src/user.rs"], "checks_run": [], "notes": [] }));
    let second = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T006",
        state.path(),
        d,
        None,
        &in_place(),
    );
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    assert!(
        second
            .message()
            .contains("ticked T005 from 1 verdict commit"),
        "{}",
        second.message()
    );
    let now = tasks(r.path());
    assert!(
        now.contains("- [X] T005") && now.contains("- [X] T006"),
        "{now}"
    );
    // The spec changes after the evidence: the report says so.
    let spec = r.path().join("specs/001-graph/spec.md");
    fs::write(
        &spec,
        format!("{}\nUsers log out.\n", fs::read_to_string(&spec).unwrap()),
    )
    .unwrap();
    // Ignored, the intent is not in git: the commits' `Intent-Digest` tells
    // that it changed, not which file.
    let (t5, message) = task_of("T005");
    assert_eq!(t5.state, "regression_only");
    let d = t5.drift.as_ref().expect(&message);
    assert!(d.files.is_empty() && !d.stale(), "{message}");
    assert!(
        message.contains("the spec changed since 2 tick(s)"),
        "{message}"
    );

    // A reworded line is not the task that commit judged: no tick.
    fs::write(
        &file,
        now.replace(
            "- [X] T005 [P] [US1] Integration test",
            "- [ ] T005 [P] [US1] End-to-end test",
        ),
    )
    .unwrap();
    let (_f, d) = fake();
    let third = speccoder_runtime::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions {
            max_tasks: Some(0),
            ..in_place()
        },
    );
    assert!(
        !third.message().contains("ticked T005"),
        "{}",
        third.message()
    );
    assert!(tasks(r.path()).contains("- [ ] T005 [P] [US1] End-to-end test"));
}

#[test]
fn the_model_sees_no_ignored_file_and_cannot_move_the_checkout() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    // A private, ignored file next to the code.
    fs::write(r.path().join(".git/info/exclude"), "notes.txt\n").unwrap();
    fs::write(r.path().join("notes.txt"), "private\n").unwrap();
    let (f, d) = fake();
    f.will_call_tool("c0", "read_file", json!({ "path": "notes.txt" }));
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["git", "switch", "main"] }),
    );
    f.will_call_tool(
        "c2",
        "read_file",
        json!({ "path": "specs/001-graph/spec.md" }),
    );
    adds_login(&f);

    let res = run(r.path(), state.path(), d, &in_place());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    let decisions: Vec<&str> = rep
        .tool_calls
        .iter()
        .map(|c| c["decision"].as_str().unwrap())
        .collect();
    assert_eq!(
        decisions,
        ["deny", "deny", "allow", "allow"],
        "{:?}",
        rep.tool_calls
    );
    assert!(rep.ticked);
    assert_eq!(
        fs::read_to_string(r.path().join("notes.txt")).unwrap(),
        "private\n"
    );
}

#[test]
fn accept_in_place_commits_the_attestation_then_ticks_locally() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let res = speccoder_runtime::controls::accept_in(
        r.path(),
        None,
        "T005",
        "tested by hand",
        state.path(),
        true,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(res.message().contains("back on main"), "{}", res.message());
    assert!(tasks(r.path()).contains("- [X] T005"));
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    let message = git(
        r.path(),
        &["log", "-1", "--format=%B", "speccoder/001-graph"],
    );
    for trailer in [
        "Verdict: attested",
        "Accepted-By:",
        "Mode: in-place",
        "Task-Line:",
        "Tick: local specs/001-graph/tasks.md",
    ] {
        assert!(message.contains(trailer), "{trailer} missing in\n{message}");
    }
    let rep = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(rep)) = rep.data() else {
        panic!()
    };
    assert_eq!(
        rep.tasks.iter().find(|t| t.id == "T005").unwrap().state,
        "attested"
    );
}

#[test]
fn converge_in_place_appends_to_the_ignored_task_list() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let file = r.path().join("specs/001-graph/tasks.md");
    let done = "# Tasks\n\n## Phase 1: Setup\n\n- [x] T001 Create the crate\n";
    fs::write(&file, done).unwrap();
    let (f, d) = fake();
    f.will_structure(json!({ "findings": [{ "source_ref": "FR-002", "gap_type": "missing", "severity": "HIGH", "description": "no empty-name check", "evidence": "src/lib.rs", "task": "Reject empty names in login." }], "checked": ["FR-002"] }));
    let res = speccoder_runtime::convergence::converge_with_options(
        r.path(),
        None,
        state.path(),
        d,
        None,
        &in_place(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let now = fs::read_to_string(&file).unwrap();
    assert!(now.starts_with(done), "{now}");
    assert!(
        now.contains("- [ ] T002 Reject empty names in login per FR-002 (missing)"),
        "{now}"
    );
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    let message = git(
        r.path(),
        &["log", "-1", "--format=%B", "speccoder/001-graph"],
    );
    assert!(message.contains("converge round 1"), "{message}");
    assert!(message.contains("Mode: in-place"), "{message}");
    assert!(
        message.contains("Tasks: local specs/001-graph/tasks.md"),
        "{message}"
    );
}

#[test]
fn a_command_that_changes_git_stops_the_task_before_the_tick() {
    // No sandbox: nothing keeps `.git` read-only, so the guard must.
    let r = repo(
        "\n[sandbox]\nmode = \"off\"\n\n[checks.plants-config]\nargv = [\"sh\", \"-c\", \"git config --local core.fsmonitor ./evil.sh\"]\n",
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    adds_login(&f);
    let res = run(r.path(), state.path(), d, &in_place());
    let rep = report(&res);
    assert!(!rep.ticked, "{rep:?}");
    let reason = rep.reason.clone().unwrap_or_default();
    assert!(reason.contains("GIT-CHANGED"), "{reason}");
    assert!(tasks(r.path()).contains("- [ ] T005"));
}

fn waits_on_a_manifest(f: &FakeProvider) {
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "package.json", "content": "{}\n" }),
    );
}

fn listed(r: &Path, state: &Path) -> usize {
    match speccoder_runtime::approvals::decide_with(
        r,
        None,
        speccoder_runtime::approvals::Verb::List,
        state,
    )
    .data()
    {
        Some(CommandData::Approvals(a)) => a.pending.len(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_call_waiting_in_place_keeps_the_checkout_and_resumes_after_approval() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    waits_on_a_manifest(&f);
    let first = run(r.path(), state.path(), d, &in_place());
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    assert!(
        first
            .message()
            .contains("the checkout stays on speccoder/001-graph"),
        "{}",
        first.message()
    );
    assert_eq!(
        git(r.path(), &["symbolic-ref", "--short", "HEAD"]),
        "speccoder/001-graph"
    );
    assert_eq!(listed(r.path(), state.path()), 1);

    let ok = speccoder_runtime::approvals::decide_with(
        r.path(),
        None,
        speccoder_runtime::approvals::Verb::Approve("T005"),
        state.path(),
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());

    let (f, d) = fake();
    f.will_structure(json!({ "summary": "manifest", "status": "done", "files_changed": ["package.json"], "checks_run": [], "notes": [] }));
    let second = run(r.path(), state.path(), d, &in_place());
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    assert!(report(&second).ticked);
    assert_eq!(f.asked().len(), 1, "a finished call was repeated");
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
}

#[test]
fn return_withdraws_a_call_waiting_in_place() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    waits_on_a_manifest(&f);
    let first = run(r.path(), state.path(), d, &in_place());
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    assert_eq!(listed(r.path(), state.path()), 1);
    let back = speccoder_runtime::in_place::return_in_place(r.path(), state.path());
    assert_eq!(back.status(), CommandStatus::Ok, "{}", back.message());
    assert_eq!(git(r.path(), &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(
        listed(r.path(), state.path()),
        0,
        "a set-aside attempt still waits"
    );
}

#[test]
fn a_refused_run_keeps_the_waiting_attempt_resumable() {
    // Found by dogfooding: a stop before the attempt started deleted the
    // waiting attempt's resume marker, and the next run began afresh.
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    waits_on_a_manifest(&f);
    let first = run(r.path(), state.path(), d, &in_place());
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );

    // The repository's config changes while the call waits: the run stops.
    git(r.path(), &["config", "speccoder.test", "1"]);
    let (_f, d) = fake();
    let stopped = run(r.path(), state.path(), d, &in_place());
    assert!(
        stopped.code().map(|c| c.name) == Some("GIT-CHANGED"),
        "{}",
        stopped.message()
    );
    git(r.path(), &["config", "--unset", "speccoder.test"]);

    // Decided, the same attempt continues: nothing is asked again.
    let ok = speccoder_runtime::approvals::decide_with(
        r.path(),
        None,
        speccoder_runtime::approvals::Verb::Approve("T005"),
        state.path(),
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    let (f, d) = fake();
    f.will_structure(json!({ "summary": "manifest", "status": "done", "files_changed": ["package.json"], "checks_run": [], "notes": [] }));
    let resumed = run(r.path(), state.path(), d, &in_place());
    assert_eq!(resumed.status(), CommandStatus::Ok, "{}", resumed.message());
    assert_eq!(f.asked().len(), 1, "the attempt started afresh");
    assert_eq!(report(&resumed).run_id, report(&first).run_id);
}
