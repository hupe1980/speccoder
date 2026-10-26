//! `speccoder converge` with a scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandResult, CommandStatus, ConvergeReport};
use speccoder_runtime::convergence::converge_with;
use speccoder_runtime::run_converged_with;

const MODEL: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n";
/// The check, and `grep` allowed for the scripted acceptance tests (a
/// check whose prefix is a bare program allows only itself).
const CHECK: &str = "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";
const GREP: &str = "[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n";

/// A stand-in test runner for scripted acceptance tests: one test, "the
/// pattern is in src/lib.rs", reported with libtest's summary line so its
/// red is an assertion failure.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

fn write_accept_sh(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let p = root.join("accept.sh");
    fs::write(&p, ACCEPT_SH).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

const DONE: &str = "# Tasks\n\n## Phase 1: Setup\n\n- [x] T001 Create the crate\n\n## Phase 2: User Story 1 - Login\n\n- [x] T002 [US1] Add login in src/lib.rs\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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

/// The `graph` fixture with `tasks` as its task list, committed.
fn repo(tasks: &str, extra_config: &str) -> tempfile::TempDir {
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
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n- **FR-001**: Users can log in.\n- **FR-002**: Empty names are rejected.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::write(r.join("specs/001-graph/tasks.md"), tasks).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\npub fn login() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!("{MODEL}{CHECK}{}", with_grep(extra_config)),
    )
    .unwrap();
    write_accept_sh(r);
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// `extra_config` with `grep` added to its `[commands] allow` (or a new one).
fn with_grep(extra_config: &str) -> String {
    if extra_config.contains("[commands]") {
        extra_config.replace(
            "[commands]\n",
            "[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n",
        )
    } else {
        format!("{GREP}{extra_config}")
    }
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn finding(gap: &str, severity: &str, source: &str, task: &str) -> Value {
    json!({ "source_ref": source, "gap_type": gap, "severity": severity, "description": format!("{task} ({gap})"), "evidence": "src/lib.rs", "task": task })
}

fn answer(findings: &[Value]) -> Value {
    json!({ "findings": findings, "checked": ["FR-001", "FR-002"] })
}

fn report(r: &CommandResult) -> &ConvergeReport {
    match r.data() {
        Some(CommandData::Converge(c)) => c,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn converge(repo: &Path, state: &Path, d: Arc<dyn ModelProvider>) -> CommandResult {
    converge_with(repo, None, state, d, None)
}

fn worktree_tasks(state: &Path) -> String {
    let mut found = None;
    for entry in walk(state) {
        if entry.ends_with("specs/001-graph/tasks.md")
            && entry.to_string_lossy().contains("worktrees")
        {
            found = Some(entry);
        }
    }
    fs::read_to_string(found.expect("a worktree tasks.md")).unwrap()
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && !p.ends_with(".git") && !p.ends_with("target") {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// Adds an acceptance example to the spec (committed).
fn with_acceptance(repo: &Path) {
    let spec = repo.join("specs/001-graph/spec.md");
    fs::write(
        &spec,
        format!(
            "{}\n**Acceptance**: `login(\"\")` is rejected.\n",
            fs::read_to_string(&spec).unwrap()
        ),
    )
    .unwrap();
    git(repo, &["commit", "-qam", "acceptance"]);
}

fn no_tests() -> Value {
    json!({ "files": [], "argv": [], "notes": ["none"] })
}

fn reject_empty_test() -> Value {
    json!({ "files": [{ "path": "tests/acceptance_t003.txt", "content": "x\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn reject_empty"], "notes": [] })
}

#[test]
fn findings_become_a_convergence_phase_gravest_first() {
    let r = repo(DONE, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(answer(&[
        finding("missing", "HIGH", "FR-002", "Reject empty names in login."),
        finding(
            "contradicts",
            "CRITICAL",
            "Constitution I",
            "Add unit tests for login",
        ),
        finding("unrequested", "LOW", "-", "Review the ok function"),
    ]));
    let res = converge(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let c = report(&res);
    assert_eq!(c.outcome, "tasks_appended");
    assert_eq!(c.round, 1);
    assert_eq!(
        c.appended,
        [
            "T003 Add unit tests for login per Constitution I (contradicts)",
            "T004 Reject empty names in login per FR-002 (missing)",
        ]
    );
    assert_eq!(c.unrequested.len(), 1);
    assert!(c.commit.is_some());
    let tasks = worktree_tasks(state.path());
    assert!(tasks.starts_with(DONE), "{tasks}");
    assert!(tasks.ends_with("\n## Phase 3: Convergence\n\n- [ ] T003 Add unit tests for login per Constitution I (contradicts)\n- [ ] T004 Reject empty names in login per FR-002 (missing)\n"), "{tasks}");
    // Unrequested work never becomes a task.
    assert!(!tasks.contains("Review the ok function"));

    // The assessor could only read.
    let names: Vec<String> = f.asked()[0].tools.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, speccoder_runtime::substrate::converge::READ_ONLY);
    // The user's checkout is untouched.
    assert_eq!(
        fs::read_to_string(r.path().join("specs/001-graph/tasks.md")).unwrap(),
        DONE
    );
}

#[test]
fn nothing_remaining_leaves_tasks_byte_for_byte_unchanged() {
    let r = repo(DONE, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(&[]));
    let res = converge(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let c = report(&res);
    assert_eq!(c.outcome, "converged");
    assert!(c.commit.is_none());
    assert_eq!(worktree_tasks(state.path()), DONE);
}

#[test]
fn write_tools_are_refused() {
    let r = repo(DONE, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "" }),
    );
    f.will_call_tool(
        "x",
        "run_command",
        json!({ "argv": ["grep", "-q", "fn ok", "src/lib.rs"] }),
    );
    f.will_structure(answer(&[]));
    let res = converge(r.path(), state.path(), d);
    let c = report(&res);
    let decisions: Vec<&str> = c
        .tool_calls
        .iter()
        .map(|t| t["decision"].as_str().unwrap())
        .collect();
    assert_eq!(decisions, ["unknown", "unknown"]);
    let wt_lib = walk(state.path())
        .into_iter()
        .find(|p| p.ends_with("src/lib.rs") && p.to_string_lossy().contains("worktrees"))
        .unwrap();
    assert!(fs::read_to_string(wt_lib).unwrap().contains("login"));
}

#[test]
fn open_tasks_are_refused_before_any_model_call() {
    let r = repo(&DONE.replace("- [x] T002", "- [ ] T002"), "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = converge(r.path(), state.path(), d);
    assert_eq!(
        res.status(),
        CommandStatus::TaskNotReady,
        "{}",
        res.message()
    );
    assert!(res.message().contains("T002"));
    assert!(f.asked().is_empty());
}

#[test]
fn a_repeated_finding_is_no_progress() {
    let earlier = format!(
        "{DONE}\n## Phase 3: Convergence\n\n- [x] T003 Reject empty names in login per FR-002 (missing)\n"
    );
    let r = repo(&earlier, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(&[finding(
        "partial",
        "HIGH",
        "FR-002",
        "Reject empty names in login",
    )]));
    let res = converge(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let c = report(&res);
    assert_eq!((c.outcome.as_str(), c.round), ("no_progress", 2));
    assert_eq!(c.repeated.len(), 1);
    assert_eq!(worktree_tasks(state.path()), earlier);
}

#[test]
fn the_round_limit_stops_before_any_model_call() {
    let earlier = format!(
        "{DONE}\n## Phase 3: Convergence\n\n- [x] T003 Reject empty names per FR-002 (missing)\n"
    );
    let r = repo(&earlier, "[converge]\nmax_rounds = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = converge(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(report(&res).outcome, "round_limit");
    assert!(f.asked().is_empty());
}

#[test]
fn run_converge_works_the_appended_tasks_until_converged() {
    let r = repo(
        &DONE.replace("- [x] T002", "- [ ] T002"),
        "[acceptance]\nheld_out = false\nstrength = false\n",
    );
    with_acceptance(r.path());
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // T002: no acceptance tests, an attempt that changes nothing the checks mind.
    f.will_structure(no_tests());
    f.will_structure(json!({ "summary": "login exists", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    // Round 1: one gap.
    f.will_structure(answer(&[finding(
        "missing",
        "HIGH",
        "FR-002",
        "Reject empty names in login",
    )]));
    // T003: its acceptance test demonstrates the gap (red); the attempt closes it.
    f.will_structure(reject_empty_test());
    f.will_call_tool("e", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn login() {}", "new_string": "pub fn login() {}\npub fn reject_empty() {}" }));
    f.will_structure(json!({ "summary": "rejects empty", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    // Round 2: nothing remains.
    f.will_structure(answer(&[]));

    let res = run_converged_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    let tasks: Vec<&str> = fr.attempts.iter().map(|a| a.task.as_str()).collect();
    assert_eq!(tasks, ["T002", "T003"]);
    let outcomes: Vec<&str> = fr.converge.iter().map(|c| c.outcome.as_str()).collect();
    assert_eq!(outcomes, ["tasks_appended", "converged"]);
    assert!(fr.stop.starts_with("converge: converged"), "{}", fr.stop);
    assert!(
        worktree_tasks(state.path())
            .contains("- [X] T003 Reject empty names in login per FR-002 (missing)")
    );
}

#[test]
fn a_recorded_assessment_replays_strictly() {
    let r = repo(DONE, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(answer(&[]));
    let res = converge(r.path(), state.path(), d);
    let run_id = report(&res).run_id.clone().unwrap();
    let calls = f.calls();
    let replayed = speccoder_runtime::replay_with(r.path(), &run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
    assert_eq!(f.calls(), calls, "replay called the model");
}

#[test]
fn a_convergence_task_gets_acceptance_tests_from_the_whole_spec() {
    let tasks = format!(
        "{DONE}\n## Phase 3: Convergence\n\n- [ ] T003 Reject empty names in login per FR-002 (missing)\n"
    );
    let r = repo(&tasks, "[acceptance]\nheld_out = false\n");
    with_acceptance(r.path());
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(reject_empty_test());
    f.will_call_tool("e", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn login() {}", "new_string": "pub fn login() {}\npub fn reject_empty() {}" }));
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let res = speccoder_runtime::run_with(r.path(), None, "T003", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Run(rep)) = res.data() else {
        panic!()
    };
    assert_eq!(rep.verdict, Some(speccoder_core::verify::Verdict::Verified));
    // The judge-side call saw the whole spec, FR-002 included.
    let first = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(f.asked()[0].tools.is_empty());
    assert!(
        first.contains("FR-002") && first.contains("closes a gap"),
        "{first}"
    );
}

#[test]
fn only_unrequested_findings_need_a_person_not_converged() {
    let r = repo(DONE, "");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(&[finding(
        "unrequested",
        "LOW",
        "-",
        "Review the ok function",
    )]));
    let res = converge(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(report(&res).outcome, "needs_review");
    assert_eq!(worktree_tasks(state.path()), DONE);
}

#[test]
fn an_undemonstrated_convergence_gap_changes_no_code() {
    let tasks = format!(
        "{DONE}\n## Phase 3: Convergence\n\n- [ ] T003 Reject empty names in login per FR-002 (missing)\n"
    );
    let r = repo(&tasks, "");
    with_acceptance(r.path());
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // The author's test already passes: the claimed gap is not shown to exist.
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_t003.txt", "content": "x\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn login"], "notes": [] }));
    let res = speccoder_runtime::run_with(r.path(), None, "T003", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let Some(CommandData::Run(rep)) = res.data() else {
        panic!()
    };
    assert_eq!(rep.attempt_status, "not_demonstrated");
    assert!(res.message().contains("already pass"), "{}", res.message());
    // No attempt was made, and the worktree is as it was.
    assert_eq!(f.asked().len(), 1);
    assert!(
        !Path::new(&rep.worktree)
            .join("tests/acceptance_t003.txt")
            .exists()
    );
    assert!(!rep.ticked);
    assert!(speccoder_runtime::worktree::require_clean(Path::new(&rep.worktree)).is_ok());
}

// ── the feature report ──────────────────────────────────────────────

fn status(r: &CommandResult) -> &speccoder_core::StatusReport {
    match r.data() {
        Some(CommandData::Report(s)) => s,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[test]
fn the_report_shows_each_ticks_evidence_from_the_branch() {
    let r = repo(
        &DONE.replace("- [x] T002", "- [ ] T002"),
        "[acceptance]\nheld_out = false\nstrength = false\n",
    );
    with_acceptance(r.path());

    // Before speccoder: the checkout's ticks, none with evidence.
    let before = speccoder_runtime::status::report(r.path(), None);
    assert_eq!(before.status(), CommandStatus::Ok, "{}", before.message());
    let s = status(&before);
    assert_eq!((s.source.as_str(), s.branch.as_deref()), ("checkout", None));
    let states: Vec<&str> = s.tasks.iter().map(|t| t.state.as_str()).collect();
    assert_eq!(states, ["ticked_without_evidence", "ready"]);

    // A feature run with convergence: T002 verified by red-then-green,
    // a round appends T003, which is verified too.
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_t002.txt", "content": "x\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn greet"], "notes": [] }));
    f.will_call_tool("g", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn greet() {}" }));
    f.will_structure(json!({ "summary": "greet", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    f.will_structure(answer(&[finding(
        "missing",
        "HIGH",
        "FR-002",
        "Reject empty names in login",
    )]));
    f.will_structure(reject_empty_test());
    f.will_call_tool("e", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn login() {}", "new_string": "pub fn login() {}\npub fn reject_empty() {}" }));
    f.will_structure(json!({ "summary": "rejects", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    f.will_structure(answer(&[]));
    let run = run_converged_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(run.status(), CommandStatus::Ok, "{}", run.message());

    let after = speccoder_runtime::status::report(r.path(), None);
    let s = status(&after);
    assert_eq!(s.source, "branch");
    assert_eq!(s.branch.as_deref(), Some("speccoder/001-graph"));
    let states: Vec<(&str, &str)> = s
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.state.as_str()))
        .collect();
    assert_eq!(
        states,
        [
            ("T001", "ticked_without_evidence"),
            ("T002", "verified"),
            ("T003", "verified")
        ]
    );
    assert!(s.tasks[1].commit.is_some() && s.tasks[1].run_id.is_some());
    assert_eq!(s.convergence.len(), 1);
    assert_eq!(
        s.convergence[0].tasks,
        ["T003 Reject empty names in login per FR-002 (missing)"]
    );
    // Two task commits and one convergence commit are not merged yet.
    assert_eq!(s.unmerged_commits, 3);
    assert!(
        after.message().starts_with("3/3 done: 2 verified"),
        "{}",
        after.message()
    );
    // Read-only: the checkout is untouched and still clean.
    assert!(speccoder_runtime::worktree::require_clean(r.path()).is_ok());
}

#[test]
fn the_report_shows_an_open_tasks_last_attempt() {
    let tasks = format!(
        "{DONE}\n## Phase 3: Convergence\n\n- [ ] T003 Reject empty names in login per FR-002 (missing)\n"
    );
    let r = repo(&tasks, "");
    with_acceptance(r.path());
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // Not demonstrated: the author's test already passes.
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_t003.txt", "content": "x\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn login"], "notes": [] }));
    let res = speccoder_runtime::run_with(r.path(), None, "T003", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());

    let rep = speccoder_runtime::status::report(r.path(), None);
    let s = status(&rep);
    let t3 = s.tasks.iter().find(|t| t.id == "T003").unwrap();
    assert_eq!(t3.state, "ready");
    let last = t3.last_attempt.as_ref().expect("last attempt");
    assert_eq!(last.outcome, "not_demonstrated");
    assert!(last.reason.contains("already pass"), "{}", last.reason);
    // The undemonstrated test is kept in the ref, not in the worktree.
    let Some(CommandData::Run(run)) = res.data() else {
        panic!()
    };
    assert_eq!(run.saved_attempt.as_deref(), Some(last.reference.as_str()));
}
