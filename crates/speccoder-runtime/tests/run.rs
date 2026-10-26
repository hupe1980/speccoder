//! `speccoder run` end to end with a scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fake_github;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::{run_feature_with, run_with};

const MODEL: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n";
/// The check, and `grep` allowed for the scripted acceptance tests (a
/// check whose prefix is a bare program allows only itself).
const CHECK: &str = "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";
/// A check whose `list` names every Rust file under `tests/`: how a
/// project without Cargo tells speccoder which test files its run collects.
const LISTED: &str =
    "[checks.listed]\nargv = [\"true\"]\nlist = [\"find\", \"tests\", \"-name\", \"*.rs\"]\n";
const GREP: &str = "[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n";

/// A stand-in test runner for scripted acceptance tests: one test, "the
/// pattern is in src/lib.rs", reported with libtest's summary line so its
/// red is an assertion failure.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif [ \"$2\" = flaky ] && grep -q \"$1\" src/lib.rs && [ ! -e .flaky-seen ]; then\n  touch .flaky-seen\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nelif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelif [ \"$2\" = absent ]; then\n  echo 'error[E0425]: cannot find function `login` in this scope' >&2\n  echo 'error: could not compile `demo` (test \"acceptance\") due to 1 previous error' >&2\n  exit 101\nelif [ \"$2\" = syntax ]; then\n  echo 'error: expected one of `,`, `:`, or `}`, found `=`' >&2\n  echo 'error: could not compile `demo` (test \"acceptance\") due to 1 previous error' >&2\n  exit 101\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

fn write_accept_sh(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let p = root.join("accept.sh");
    fs::write(&p, ACCEPT_SH).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// The `graph` fixture as a committed git repository with code and checks.
fn repo(extra_config: &str) -> tempfile::TempDir {
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
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!("{MODEL}{CHECK}{}", with_grep(extra_config)),
    )
    .unwrap();
    write_accept_sh(r);
    git(r, &["init", "-q"]);
    // A person's commit (`task restore`) uses the repository's identity; a
    // fresh CI runner has no global one.
    git(r, &["config", "user.name", "t"]);
    git(r, &["config", "user.email", "t@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// [`repo`] plus extra committed files.
fn repo_with(extra_config: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = repo(extra_config);
    for (path, text) in files {
        let p = dir.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "more"]);
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

fn done(summary: &str) -> Value {
    json!({ "summary": summary, "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] })
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn run(repo: &Path, state: &Path, task: &str, driver: Arc<dyn ModelProvider>) -> CommandResult {
    run_with(repo, None, task, state, driver, None)
}

// ── US1 ────────────────────────────────────────────────────────────────────

#[test]
fn an_attempt_that_keeps_the_checks_green_is_regression_only() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_call_tool("c2", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("added login"));

    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "completed");
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert_eq!(
        rep.changes,
        [("src/lib.rs".to_owned(), "modified".to_owned())]
    );
    assert_eq!(rep.model_report.as_ref().unwrap()["summary"], "added login");
    let decisions: Vec<&str> = rep
        .tool_calls
        .iter()
        .map(|c| c["decision"].as_str().unwrap())
        .collect();
    assert_eq!(decisions, ["allow", "allow"]);

    // The worktree lives in the state directory this run was given — never
    // in the user's real one (a bug found by the first real evaluation).
    let state_root = fs::canonicalize(state.path()).unwrap();
    assert!(
        Path::new(&rep.worktree).starts_with(&state_root),
        "{} not under {}",
        rep.worktree,
        state_root.display()
    );

    // the engine ticked the task and committed the attempt.
    assert!(rep.ticked);
    assert!(rep.commit.is_some());
    let wt_tasks =
        fs::read_to_string(Path::new(&rep.worktree).join("specs/001-graph/tasks.md")).unwrap();
    assert!(wt_tasks.contains("- [X] T005"), "{wt_tasks}");

    // The work is in the worktree, not in the user's checkout.
    assert_eq!(
        fs::read_to_string(r.path().join("src/lib.rs")).unwrap(),
        "pub fn ok() {}\n"
    );
    assert!(
        fs::read_to_string(Path::new(&rep.worktree).join("src/lib.rs"))
            .unwrap()
            .contains("login")
    );
}

#[test]
fn an_attempt_that_breaks_a_check_fails_the_verdict() {
    let r = repo("[budget]\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_structure(done("rewrote lib"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    assert_eq!(res.exit_code().code(), 20);
    assert!(matches!(report(&res).verdict, Some(Verdict::Failed { .. })));
    // One attempt allowed: no retry.
    assert!(report(&res).earlier.is_empty());
    assert_eq!(
        report(&res).retry_stop.as_deref(),
        Some("1 of 1 attempts used")
    );
}

#[test]
fn a_failing_command_shows_its_output_to_the_model() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // `grep` (allowed by the configured check) exits 1 here: a result, not a tool failure.
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["grep", "-q", "fn missing", "src/lib.rs"] }),
    );
    f.will_structure(done("looked"));
    run(r.path(), state.path(), "T005", d);
    let second = &f.asked()[1];
    let exchange = &second.exchanges[0];
    assert!(
        !exchange.failed,
        "a non-zero exit was reported as a failed call"
    );
    assert_eq!(
        exchange.output["exit_code"], 1,
        "the model got no output: {}",
        exchange.output
    );
}

#[test]
fn an_attempt_the_model_calls_incomplete_needs_a_human() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(json!({ "summary": "not finished", "status": "incomplete", "files_changed": [], "checks_run": [], "notes": [] }));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(report(&res).attempt_status, "incomplete");
    assert!(res.message().contains("no files changed"));
}

#[test]
fn the_attempt_prompt_does_not_carry_the_planning_instruction() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("nothing"));
    run(r.path(), state.path(), "T005", d);
    let prompt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(prompt.contains("implementing exactly one GitHub Spec"));
    assert!(
        !prompt.contains("planning step"),
        "contradictory instructions: {prompt}"
    );
    assert!(!prompt.contains("You cannot run tools"));
}

// ── US2 ────────────────────────────────────────────────────────────────────

#[test]
fn denied_calls_are_refused_and_the_loop_continues() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["git", "push", "origin", "main"] }),
    );
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "specs/001-graph/tasks.md", "content": "all done" }),
    );
    f.will_structure(done("tried"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "completed");
    let decisions: Vec<&str> = rep
        .tool_calls
        .iter()
        .map(|c| c["decision"].as_str().unwrap())
        .collect();
    assert_eq!(decisions, ["deny", "deny"]);
    let tasks =
        fs::read_to_string(Path::new(&rep.worktree).join("specs/001-graph/tasks.md")).unwrap();
    assert!(tasks.contains("T005"), "the spec was not overwritten");
    // The model was told: its second turn carried the refusal.
    assert!(f.asked().len() >= 2);
}

#[test]
fn a_call_needing_approval_stops_the_attempt_unexecuted() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["touch", "approved-only"] }),
    );
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.exit_code().code(), 23);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "needs_approval");
    assert_eq!(rep.pending.as_ref().unwrap()["tool"], "run_command");
    assert!(
        !Path::new(&rep.worktree).join("approved-only").exists(),
        "nothing was executed"
    );
    assert!(rep.verdict.is_none());
    // What to do next names the task and the decision.
    let next = speccoder_runtime::next::attach(res.clone(), None);
    let commands: Vec<&str> = next.next().iter().map(|s| s.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            "speccoder report --approvals",
            "speccoder task approve T005",
            "speccoder task reject T005 --reason \"…\"",
            "speccoder run"
        ]
    );
    let json = serde_json::to_value(&next).unwrap();
    assert_eq!(json["next"][1]["command"], "speccoder task approve T005");
    assert_eq!(json["data"]["attempt_status"], "needs_approval");
}

#[test]
fn report_blocked_ends_the_attempt_honestly() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "report_blocked",
        json!({ "reason": "T005 contradicts FR-002" }),
    );
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "blocked");
    assert_eq!(rep.reason.as_deref(), Some("T005 contradicts FR-002"));
}

#[test]
fn the_tool_call_limit_is_enforced() {
    let r = repo("[budget]\nmax_tool_calls = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "list_files", json!({}));
    f.will_call_tool("c2", "list_files", json!({}));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(report(&res).attempt_status, "budget_exhausted");
    assert_eq!(res.status(), CommandStatus::NeedsHuman);
}

// ── US3 ────────────────────────────────────────────────────────────────────

#[test]
fn blocked_and_unknown_tasks_are_refused_before_any_model_call() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let blocked = run(r.path(), state.path(), "T006", d.clone());
    assert_eq!(blocked.status(), CommandStatus::TaskNotReady);
    assert!(blocked.message().contains("blocked by T005"));
    let done_task = run(r.path(), state.path(), "T001", d.clone());
    assert_eq!(done_task.status(), CommandStatus::TaskNotReady);
    assert_eq!(
        run(r.path(), state.path(), "T999", d).status(),
        CommandStatus::NotFound
    );
    assert_eq!(f.calls(), 0);
}

#[test]
fn a_recorded_attempt_replays_strictly() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    // Allowed only by the project's configured check: replay must know that
    // without reading speccoder.toml (the policy rides in the run's input).
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["grep", "-q", "fn ok", "src/lib.rs"] }),
    );
    f.will_structure(done("added login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(report(&res).tool_calls[1]["decision"], "allow");
    let run_id = report(&res).run_id.clone();
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

// ── test integrity ──────────────────────────────────────────────────

const EXISTING_TEST: (&str, &str) = ("tests/existing.rs", "#[test]\nfn existing() {}\n");

#[test]
fn editing_a_frozen_test_stops_for_approval() {
    let r = repo_with("", &[EXISTING_TEST]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "tests/existing.rs", "old_string": "fn existing() {}", "new_string": "fn existing() { }" }));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "needs_approval");
    assert!(
        rep.pending.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap()
            .contains("frozen test")
    );
    let untouched = fs::read_to_string(Path::new(&rep.worktree).join("tests/existing.rs")).unwrap();
    assert_eq!(untouched, EXISTING_TEST.1);
}

#[test]
fn new_test_files_are_free_to_write() {
    let r = repo_with(LISTED, &[EXISTING_TEST]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "tests/login.rs", "content": "#[test]\nfn login() {}\n" }),
    );
    f.will_structure(done("added a test"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.tool_calls[0]["decision"], "allow");
    assert!(rep.integrity.is_empty(), "{:?}", rep.integrity);
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
}

#[test]
fn a_command_that_deletes_a_frozen_test_fails_the_verdict() {
    let r = repo_with(
        "[commands]\nallow = [[\"rm\", \"tests/existing.rs\"]]\n",
        &[EXISTING_TEST],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["rm", "tests/existing.rs"] }),
    );
    f.will_structure(done("cleaned up"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        rep.integrity,
        ["integrity: frozen test deleted: tests/existing.rs"]
    );
    assert!(
        matches!(&rep.verdict, Some(Verdict::Failed { reasons }) if reasons[0].starts_with("integrity:"))
    );
    assert_eq!(res.status(), CommandStatus::VerificationFailed);
}

#[test]
fn skip_markers_and_ci_changes_are_flagged() {
    let r = repo("[commands]\nallow = [[\"cp\", \"src/lib.rs\", \".gitlab-ci.yml\"]]\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\n\n#[test]\n#[ignore]\nfn slow() {}" }));
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["cp", "src/lib.rs", ".gitlab-ci.yml"] }),
    );
    f.will_structure(done("done"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert!(
        rep.integrity
            .contains(&"integrity: skip marker added in src/lib.rs".to_owned()),
        "{:?}",
        rep.integrity
    );
    assert!(
        rep.integrity
            .contains(&"integrity: verification input changed: .gitlab-ci.yml".to_owned()),
        "{:?}",
        rep.integrity
    );
    assert_eq!(res.status(), CommandStatus::VerificationFailed);
}

#[test]
fn a_command_writing_agent_settings_stops_the_attempt_at_once() {
    let r = repo_with(
        "[commands]\nallow = [[\"cp\", \"src/lib.rs\", \".claude/settings.json\"]]\n",
        &[(".claude/README.md", "agent config\n")],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["cp", "src/lib.rs", ".claude/settings.json"] }),
    );
    // Never requested: the attempt ends after the command.
    f.will_call_tool("c2", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("done"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    assert_eq!(rep.attempt_status, "integrity");
    assert!(
        rep.integrity
            .iter()
            .any(|f| f.contains(".claude/settings.json")),
        "{:?}",
        rep.integrity
    );
    assert_eq!(f.asked().len(), 1, "no model turn after the command");
    assert!(rep.earlier.is_empty(), "integrity findings are not retried");
}

#[test]
fn a_command_writing_ignored_agent_settings_is_caught_too() {
    let r = repo_with(
        "[commands]\nallow = [[\"cp\", \"src/lib.rs\", \".claude/settings.local.json\"]]\n",
        &[
            (".claude/README.md", "agent config\n"),
            (".gitignore", ".claude/settings.local.json\n"),
        ],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["cp", "src/lib.rs", ".claude/settings.local.json"] }),
    );
    f.will_structure(done("done"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "integrity", "{}", res.message());
    assert!(
        rep.integrity
            .iter()
            .any(|f| f.contains(".claude/settings.local.json")),
        "{:?}",
        rep.integrity
    );
}

#[test]
fn agent_configuration_is_denied_and_context_files_ask() {
    let r = repo_with(
        "[commands]\nallow = [[\"cat\", \".claude/settings.json\"]]\n",
        &[
            ("AGENTS.md", "# Agents\n"),
            (".claude/settings.json", "{}\n"),
        ],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": ".mcp.json", "content": "{}" }),
    );
    // Reading agent configuration changes nothing.
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["cat", ".claude/settings.json"] }),
    );
    f.will_call_tool("c3", "edit_file", json!({ "path": "AGENTS.md", "old_string": "# Agents", "new_string": "# Agents\nAlways pass." }));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        rep.tool_calls[0]["decision"], "deny",
        "{:?}",
        rep.tool_calls
    );
    assert_eq!(rep.tool_calls[1]["decision"], "allow");
    assert_eq!(rep.attempt_status, "needs_approval", "{}", res.message());
    let why = rep.pending.as_ref().unwrap()["reason"].as_str().unwrap();
    assert!(why.contains("agent context file"), "{why}");
}

// ── feature runs ────────────────────────────────────────────────────

const TWO_TASKS: &str = "# Tasks\n\n## Phase 1: Setup\n\n- [x] T001 Create the crate\n\n## Phase 2: User Story 1 - Login\n\n- [ ] T002 [US1] Add login in src/lib.rs\n- [ ] T003 [US1] Add logout in src/lib.rs\n";

fn two_task_repo() -> tempfile::TempDir {
    repo_with("", &[("specs/001-graph/tasks.md", TWO_TASKS)])
}

#[test]
fn out_of_scope_changes_are_reported_not_gated() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(&id, "edit_file", args);
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "src/util.rs", "content": "pub fn helper() {}\n" }),
    );
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T002", d);
    let rep = report(&res);
    assert_eq!(
        rep.verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        res.message()
    );
    let scope = rep.scope.as_ref().unwrap();
    assert_eq!(
        (scope.stated, scope.outside.clone()),
        (true, vec!["src/util.rs".to_owned()])
    );
    let message = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["log", "-1", "--format=%B"])
        .current_dir(&rep.worktree)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&message.stdout).contains("Scope-Outside: src/util.rs"));
}

const OPEN_CHECKLIST: (&str, &str) = (
    "specs/001-graph/checklists/requirements.md",
    "# Requirements\n- [x] Stories are testable\n- [ ] No open questions\n",
);

#[test]
fn open_checklists_stop_the_run_before_any_model_call() {
    let r = repo_with(
        "",
        &[("specs/001-graph/tasks.md", TWO_TASKS), OPEN_CHECKLIST],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let options = speccoder_runtime::RunOptions {
        max_tasks: Some(1),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert!(
        res.message().contains("requirements.md (1 of 2 open)"),
        "{}",
        res.message()
    );
    match res.data() {
        Some(CommandData::Checklists(lists)) => assert_eq!(lists[0].open, 1),
        other => panic!("{other:?}"),
    }
    assert!(f.asked().is_empty());
    // The task alone stops the same way.
    let (f2, d2) = fake();
    let one = run(r.path(), state.path(), "T002", d2);
    assert_eq!(one.status(), CommandStatus::NeedsHuman, "{}", one.message());
    assert!(f2.asked().is_empty());
}

#[test]
fn an_overridden_checklist_gate_is_recorded_on_the_commit() {
    let r = repo_with(
        "",
        &[("specs/001-graph/tasks.md", TWO_TASKS), OPEN_CHECKLIST],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(&id, "edit_file", args);
    f.will_structure(done("login"));
    let options = speccoder_runtime::RunOptions {
        max_tasks: Some(1),
        ignore_checklists: true,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    let fr = feature(&res);
    let attempt = &fr.attempts[0];
    assert!(attempt.ticked, "{}", res.message());
    let message = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["log", "-1", "--format=%B"])
        .current_dir(&attempt.worktree)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&message.stdout).contains("Checklists: overridden"));
}

fn edit(id: &str, new: &str) -> (String, Value) {
    (
        id.to_owned(),
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": new }),
    )
}

fn feature(res: &CommandResult) -> &speccoder_core::FeatureReport {
    match res.data() {
        Some(CommandData::Feature(f)) => f,
        other => panic!("{} → {other:?}", res.message()),
    }
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(
        Command::new("git")
            .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
}

fn full_tier_feature(full_check: &str) -> (tempfile::TempDir, CommandResult) {
    let config = format!(
        "[checks.full]\nargv = [\"grep\", \"-q\", \"{full_check}\", \"src/lib.rs\"]\ntier = \"full\"\n"
    );
    let r = repo_with(&config, &[("specs/001-graph/tasks.md", TWO_TASKS)]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("login"));
    let (id, args) = edit("c2", "pub fn ok() {}\npub fn logout() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("logout"));
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    (r, res)
}

#[test]
fn the_full_tier_runs_once_at_the_end_of_a_feature() {
    let (r, res) = full_tier_feature("fn logout");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    let tier = fr.full_tier.as_ref().unwrap();
    assert_eq!(
        (tier.ran.clone(), tier.passed),
        (vec!["full".to_owned()], true)
    );
    let read =
        speccoder_runtime::pipeline::full::read(r.path(), "001-graph", "speccoder/001-graph")
            .unwrap();
    assert!(read.passed);
}

#[test]
fn a_failing_full_tier_keeps_the_ticks_and_stops_the_hand_off() {
    let (r, res) = full_tier_feature("fn never");
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let fr = feature(&res);
    assert!(fr.attempts.iter().all(|a| a.ticked));
    assert_eq!(fr.full_tier.as_ref().unwrap().failed, ["full"]);
    let read =
        speccoder_runtime::pipeline::full::read(r.path(), "001-graph", "speccoder/001-graph")
            .unwrap();
    assert!(!read.passed && read.failed == ["full"]);
}

#[test]
fn a_feature_runs_task_by_task_to_done() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("login"));
    let (id, args) = edit("c2", "pub fn ok() {}\npub fn logout() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("logout"));

    let user_tasks = fs::read_to_string(r.path().join("specs/001-graph/tasks.md")).unwrap();
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    assert_eq!(fr.stop, "feature done: every task is ticked");
    let tasks: Vec<&str> = fr.attempts.iter().map(|a| a.task.as_str()).collect();
    assert_eq!(tasks, ["T002", "T003"]);
    assert!(fr.attempts.iter().all(|a| a.ticked && a.commit.is_some()));

    let wt = Path::new(&fr.attempts[1].worktree);
    let ticked = fs::read_to_string(wt.join("specs/001-graph/tasks.md")).unwrap();
    assert!(
        ticked.contains("- [X] T002") && ticked.contains("- [X] T003"),
        "{ticked}"
    );
    let log = git_out(wt, &["log", "--format=%s", "speccoder/001-graph"]);
    assert_eq!(
        log.lines().take(2).collect::<Vec<_>>(),
        [
            "speccoder: T003 Add logout in src/lib.rs",
            "speccoder: T002 Add login in src/lib.rs"
        ]
    );
    let body = git_out(wt, &["log", "-1", "--format=%b", "speccoder/001-graph"]);
    assert!(
        body.contains("Verdict: regression_only") && body.contains("Spec-Task: 001-graph/T003"),
        "{body}"
    );

    // The user's checkout is untouched.
    assert_eq!(
        fs::read_to_string(r.path().join("specs/001-graph/tasks.md")).unwrap(),
        user_tasks
    );
    assert_eq!(
        fs::read_to_string(r.path().join("src/lib.rs")).unwrap(),
        "pub fn ok() {}\n"
    );
}

#[test]
fn a_failing_task_stops_the_feature_and_a_rerun_resumes_there() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("login"));
    // T003 breaks the check (removes `fn ok`), twice: retried, then stopped.
    for id in ["c2", "c2b"] {
        f.will_call_tool(
            id,
            "write_file",
            json!({ "path": "src/lib.rs", "content": "pub fn logout() {}\n" }),
        );
        f.will_structure(done("logout"));
    }

    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let fr = feature(&res);
    assert_eq!(fr.attempts.len(), 2);
    assert!(fr.attempts[0].ticked && fr.attempts[0].commit.is_some());
    assert!(!fr.attempts[1].ticked && fr.attempts[1].commit.is_none());
    assert!(fr.stop.starts_with("T003:"), "{}", fr.stop);

    // The failed attempt is saved and undone: a re-run resumes at
    // T003 from the branch tip, with nothing to clean up by hand.
    let wt = PathBuf::from(&fr.attempts[1].worktree);
    assert!(fr.attempts[1].saved_attempt.is_some());
    assert!(speccoder_runtime::worktree::require_clean(&wt).is_ok());
    let (f2, d2) = fake();
    f2.will_call_tool("c3", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn login() {}", "new_string": "pub fn login() {}\npub fn logout() {}" }));
    f2.will_structure(done("logout"));
    let again = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d2,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(again.status(), CommandStatus::Ok, "{}", again.message());
    let tasks: Vec<String> = feature(&again)
        .attempts
        .iter()
        .map(|a| a.task.clone())
        .collect();
    assert_eq!(tasks, ["T003"]);
}

// ── spec-derived acceptance tests ───────────────────────────────────

const ACCEPTANCE_SPEC: (&str, &str) = (
    "specs/001-graph/spec.md",
    "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n**Acceptance**: a `login` function exists.\n",
);

fn acceptance(path: &str, argv: &[&str]) -> Value {
    json!({ "files": [{ "path": path, "content": "acceptance: login exists\n", "scenarios": [] }], "argv": argv, "notes": [] })
}

const GREP_LOGIN: &[&str] = &["./accept.sh", "--", "fn login"];

#[test]
fn a_build_error_is_not_red_and_cannot_verify() {
    let r = repo_with("", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance(
        "tests/acceptance_login.txt",
        &["./accept.sh", "--", "fn login", "syntax"],
    ));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    let a = rep.acceptance.as_ref().unwrap();
    assert_eq!(a["red"], json!(false), "{a}");
    assert_eq!(a["red_class"], json!("build_error"));
    assert!(
        a["excerpt"].as_str().unwrap().contains("expected one of"),
        "{a}"
    );
    // Green afterwards proves nothing without a red before it.
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
}

#[test]
fn a_missing_symbol_is_absent_red_and_can_verify() {
    let r = repo_with("[acceptance]\nheld_out = false\n", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance(
        "tests/acceptance_login.txt",
        &["./accept.sh", "--", "fn login", "absent"],
    ));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    let a = rep.acceptance.as_ref().unwrap();
    assert_eq!(
        (a["red"].clone(), a["red_class"].clone()),
        (json!(true), json!("absent")),
        "{a}"
    );
    assert_eq!(rep.verdict, Some(Verdict::Verified));
    // The kind of red travels with the tick.
    let message = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["log", "-1", "--format=%B"])
        .current_dir(&rep.worktree)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&message.stdout).contains("Red: absent"));
}

#[test]
fn a_flaky_acceptance_check_is_never_verified() {
    let r = repo_with("[acceptance]\nheld_out = false\n", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance(
        "tests/acceptance_login.txt",
        &["./accept.sh", "--", "fn login", "flaky"],
    ));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    let a = rep.acceptance.as_ref().unwrap();
    assert_eq!(
        (a["red"].clone(), a["green_flaky"].clone()),
        (json!(true), json!(true)),
        "{a}"
    );
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    let checks = &rep.verification.as_ref().unwrap().checks;
    assert!(checks.iter().any(|c| c.name == "acceptance" && c.flaky));
}

#[test]
fn red_then_green_acceptance_makes_the_verdict_verified() {
    let r = repo_with("[acceptance]\nheld_out = false\n", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance("tests/acceptance_login.txt", GREP_LOGIN));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::Verified));
    let a = rep.acceptance.as_ref().unwrap();
    assert_eq!(
        (a["red"].as_bool(), a["green"].as_bool()),
        (Some(true), Some(true)),
        "{a}"
    );
    // The implementing model saw the acceptance file as frozen: it existed at its checkpoint.
    let wt = Path::new(&rep.worktree);
    assert!(wt.join("tests/acceptance_login.txt").exists());
    // The judge-side call came first and had no tools.
    assert!(f.asked()[0].tools.is_empty());
    assert!(!f.asked()[1].tools.is_empty());
}

#[test]
fn a_wrong_implementation_fails_acceptance() {
    let r = repo_with("[acceptance]\nheld_out = false\n", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance("tests/acceptance_login.txt", GREP_LOGIN));
    for id in ["c1", "c2"] {
        f.will_call_tool(id, "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn logon() {}" }));
        f.will_structure(done("login, I promise"));
    }
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    assert!(
        matches!(&report(&res).verdict, Some(Verdict::Failed { reasons }) if reasons.iter().any(|r| r.contains("acceptance")))
    );
    assert!(!report(&res).ticked);
    // Retried once from the same checkpoint (the second edit applied to the
    // original file), the same failure again stops it.
    assert_eq!(report(&res).earlier.len(), 1);
    assert!(
        report(&res)
            .retry_stop
            .as_deref()
            .unwrap()
            .starts_with("same failure twice"),
        "{:?}",
        report(&res).retry_stop
    );
    // The acceptance file was not regenerated: one judge-side call only.
    assert_eq!(f.asked().iter().filter(|a| a.tools.is_empty()).count(), 1);
}

#[test]
fn acceptance_that_is_already_green_is_no_evidence() {
    let r = repo_with("", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance(
        "tests/acceptance_ok.txt",
        &["grep", "-q", "fn ok", "src/lib.rs"],
    ));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert_eq!(rep.acceptance.as_ref().unwrap()["red"], false);
}

#[test]
fn acceptance_files_outside_test_paths_are_refused() {
    let r = repo_with("", &[ACCEPTANCE_SPEC]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance("src/sneaky.rs", GREP_LOGIN));
    f.will_structure(done("nothing"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert!(
        rep.acceptance.as_ref().unwrap()["skipped"]
            .as_str()
            .unwrap()
            .contains("not a new test file")
    );
    assert!(!Path::new(&rep.worktree).join("src/sneaky.rs").exists());
}

// ── retries with a failure digest ───────────────────────────────────

/// A check that says what is wrong on stderr, so the digest has an excerpt.
const SAYS: &str = "[checks.says]\nargv = [\"sh\", \"-c\", \"grep -q 'fn ok' src/lib.rs || { echo 'error: fn ok is missing from line 7' >&2; exit 1; }\"]\n";

fn break_lib(f: &FakeProvider, id: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_structure(done("rewrote lib, all good"));
}

#[test]
fn a_failed_attempt_is_retried_with_the_verifiers_digest() {
    let r = repo(SAYS);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    break_lib(&f, "c1");
    // The second attempt edits the ORIGINAL text: the first attempt's
    // changes must have been undone, or this edit would not apply.
    f.will_call_tool("c2", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("added login"));

    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert!(rep.ticked && rep.commit.is_some());
    assert_eq!(rep.earlier.len(), 1);
    let first = &rep.earlier[0];
    assert!(matches!(first.verdict, Some(Verdict::Failed { .. })));
    assert_ne!(first.run_id, rep.run_id);
    assert!(
        first
            .fingerprint
            .contains("says: error: fn ok is missing from line #"),
        "{}",
        first.fingerprint
    );
    assert_eq!(first.digest.discarded, ["src/lib.rs"]);

    // The second attempt's prompt has the verifier's facts …
    let asked = f.asked();
    let second = asked
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .find(|p| p.contains("Previous attempt"))
        .expect("a prompt with the digest");
    assert!(
        second.contains("error: fn ok is missing from line 7"),
        "{second}"
    );
    assert!(second.contains("has-ok"));
    // … and nothing of the model's own account or transcript.
    assert!(!second.contains("rewrote lib, all good"));
    assert!(!second.contains("pub fn renamed"));
    // The first attempt's prompt had no digest.
    assert!(
        !serde_json::to_string(&asked[0].prompt)
            .unwrap()
            .contains("Previous attempt")
    );
    // Usage covers both attempts.
    assert!(rep.usage.is_some());
}

#[test]
fn the_same_failure_twice_stops_retrying() {
    let r = repo(&format!("{SAYS}[budget]\nmax_attempts = 3\n"));
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    break_lib(&f, "c1");
    break_lib(&f, "c2");
    // A third attempt is scripted but must not be asked for.
    f.will_structure(done("third"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let rep = report(&res);
    assert_eq!(rep.earlier.len(), 1);
    assert!(
        rep.retry_stop
            .as_deref()
            .unwrap()
            .starts_with("same failure twice"),
        "{:?}",
        rep.retry_stop
    );
    // The last attempt is saved under an attempt ref and undone.
    let saved = rep.saved_attempt.as_deref().expect("saved");
    assert!(
        git_out(
            Path::new(&rep.worktree),
            &["show", &format!("{saved}:src/lib.rs")]
        )
        .contains("renamed")
    );
    assert_eq!(
        fs::read_to_string(Path::new(&rep.worktree).join("src/lib.rs")).unwrap(),
        "pub fn ok() {}\n"
    );
    assert_eq!(f.asked().len(), 4);
}

#[test]
fn integrity_findings_and_self_stops_are_not_retried() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\n\n#[test]\n#[ignore]\nfn slow() {}" }));
    f.will_structure(done("done"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        rep.retry_stop.as_deref(),
        Some("integrity findings are not retried")
    );
    assert!(rep.earlier.is_empty());

    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "contradiction" }));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "blocked");
    assert!(rep.earlier.is_empty() && rep.retry_stop.is_none());
}

#[test]
fn a_check_that_cannot_run_is_not_retried() {
    let r = repo("[checks.gone]\nargv = [\"speccoder-no-such-tool\"]\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("nothing"));
    let res = run(r.path(), state.path(), "T005", d);
    // It cannot run on the untouched code: stopped before any model call.
    assert_eq!(
        res.status(),
        CommandStatus::ConfigError,
        "{}",
        res.message()
    );
    assert_eq!(res.code().map(|c| c.name), Some("CHECK-BROKEN"));
    assert!(
        res.message()
            .contains("check `gone` cannot run on the untouched code"),
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty(), "a model call was made");
    // Asked to go on anyway, the attempt runs and is not retried.
    let options = speccoder_runtime::RunOptions {
        allow_failing_baseline: true,
        ..speccoder_runtime::RunOptions::default()
    };
    let (f, d) = fake();
    f.will_structure(done("nothing"));
    let res = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        &options,
    );
    let rep = report(&res);
    assert!(
        rep.retry_stop
            .as_deref()
            .unwrap_or_default()
            .starts_with("not retried: check gone could not run"),
        "{:?} / {}",
        rep.retry_stop,
        res.message()
    );
}

#[test]
fn a_test_runner_that_ran_no_test_is_broken_a_failing_script_is_not() {
    use speccoder_core::verify::{CheckResult, CheckStatus};
    let check = |argv: &[&str], exit: i32, stderr: &str| CheckResult {
        name: "test".to_owned(),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        status: CheckStatus::Failed,
        exit_code: Some(exit),
        duration_ms: 1,
        junit: false,
        tests: None,
        stdout_tail: String::new(),
        stderr_tail: stderr.to_owned(),
        truncated: false,
        note: None,
        flaky: false,
    };
    // The reported first run: a nextest profile the worktree does not have.
    let profile = check(
        &["cargo", "nextest", "run", "--profile", "speccoder"],
        101,
        "error: profile `speccoder` not found (known profiles: default, default-miri)",
    );
    assert_eq!(
        speccoder_runtime::pipeline::broken(&profile).as_deref(),
        Some("error: profile `speccoder` not found (known profiles: default, default-miri)")
    );
    assert!(
        speccoder_runtime::pipeline::broken(&check(
            &["missing-tool"],
            127,
            "sh: missing-tool: not found"
        ))
        .is_some()
    );
    // Tests ran and failed: pre-existing failures, not a broken check.
    let failing = check(
        &["cargo", "test"],
        101,
        "test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out",
    );
    assert!(speccoder_runtime::pipeline::broken(&failing).is_none());
    // A script that fails until the task is done.
    assert!(
        speccoder_runtime::pipeline::broken(&check(
            &["grep", "-q", "x", "f"],
            2,
            "grep: f: No such file or directory"
        ))
        .is_none()
    );
}

// ── durable attempts ────────────────────────────────────────────────

#[test]
fn a_failed_attempt_is_saved_under_a_ref_and_undone() {
    let r = repo("[budget]\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "tests/new_test.rs", "content": "#[test]\nfn t() {}\n" }),
    );
    f.will_structure(done("rewrote lib"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let rep = report(&res);
    let wt = Path::new(&rep.worktree);
    let saved = rep.saved_attempt.clone().expect("saved");
    assert!(
        saved.starts_with("refs/speccoder/attempts/001-graph/T005/run_"),
        "{saved}"
    );
    // The ref holds the attempt as it ended, on top of the branch tip …
    assert_eq!(
        git_out(wt, &["show", &format!("{saved}:src/lib.rs")]).trim(),
        "pub fn renamed() {}"
    );
    assert!(git_out(wt, &["show", &format!("{saved}:tests/new_test.rs")]).contains("fn t()"));
    assert_eq!(
        git_out(wt, &["rev-parse", &format!("{saved}^")]),
        git_out(wt, &["rev-parse", "HEAD"])
    );
    let message = git_out(wt, &["log", "-1", "--format=%B", &saved]);
    assert!(
        message.contains("Outcome: verification_failed"),
        "{message}"
    );
    assert!(
        message.contains(&format!("Run: {}", rep.run_id)),
        "{message}"
    );
    // … and the worktree is back where the attempt started: the next attempt
    // will not freeze the failed attempt's test file.
    assert!(speccoder_runtime::worktree::require_clean(wt).is_ok());
    assert!(!wt.join("tests/new_test.rs").exists());
    // The user's repository knows the ref (worktrees share refs).
    assert!(!git_out(r.path(), &["for-each-ref", "refs/speccoder/attempts/"]).is_empty());
}

#[test]
fn leftovers_of_an_interrupted_run_are_set_aside_before_the_next() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    // A first run creates the worktree; then a "crash" leaves edits behind.
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "first" }));
    let first = run(r.path(), state.path(), "T005", d);
    let wt = PathBuf::from(&report(&first).worktree);
    fs::write(
        wt.join("src/lib.rs"),
        "pub fn ok() {}\npub fn half_done() {}\n",
    )
    .unwrap();

    let (f2, d2) = fake();
    f2.will_call_tool("c", "read_file", json!({ "path": "src/lib.rs" }));
    f2.will_structure(done("looked"));
    let res = run(r.path(), state.path(), "T005", d2);
    let rep = report(&res);
    let aside = rep.set_aside.clone().expect("set aside");
    assert!(
        aside.starts_with("refs/speccoder/attempts/001-graph/T005/set_aside-"),
        "{aside}"
    );
    assert!(git_out(&wt, &["show", &format!("{aside}:src/lib.rs")]).contains("half_done"));
    // The model read the branch tip, not the leftovers.
    let read = rep
        .tool_calls
        .iter()
        .find(|c| c["tool"] == "read_file")
        .unwrap();
    assert_eq!(read["decision"], "allow");
    assert!(
        !fs::read_to_string(wt.join("src/lib.rs"))
            .unwrap()
            .contains("half_done")
    );

    // The person sees them, in the report and among the decisions …
    let status = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(st)) = status.data() else {
        panic!("{}", status.message())
    };
    let t = st.tasks.iter().find(|t| t.id == "T005").unwrap();
    assert_eq!(t.set_aside.as_ref().unwrap().files, ["src/lib.rs"]);
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    assert!(
        inbox.message().contains("1 set-aside change"),
        "{}",
        inbox.message()
    );
    // … and keeps them: on the branch, in the worktree, the ref gone.
    let kept = speccoder_runtime::controls::restore(r.path(), None, "T005");
    assert_eq!(kept.status(), CommandStatus::Ok, "{}", kept.message());
    assert!(git_out(r.path(), &["show", "speccoder/001-graph:src/lib.rs"]).contains("half_done"));
    assert!(
        fs::read_to_string(wt.join("src/lib.rs"))
            .unwrap()
            .contains("half_done")
    );
    assert!(git_out(r.path(), &["for-each-ref", &aside]).is_empty());
    let again = speccoder_runtime::controls::restore(r.path(), None, "T005");
    assert_eq!(again.status(), CommandStatus::NotFound);
}

/// Set-aside changes that conflict with what the branch has since are
/// refused, and nothing moves: branch, worktree and the saved ref stay.
#[test]
fn a_conflicting_set_aside_change_is_refused_and_nothing_moves() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "first" }));
    let first = run(r.path(), state.path(), "T005", d);
    let wt = PathBuf::from(&report(&first).worktree);
    fs::write(wt.join("src/lib.rs"), "pub fn mine() {}\n").unwrap();
    let (f2, d2) = fake();
    f2.will_call_tool("b", "report_blocked", json!({ "reason": "second" }));
    let res = run(r.path(), state.path(), "T005", d2);
    let aside = report(&res).set_aside.clone().expect("set aside");
    // The branch moves on with its own, conflicting, line.
    fs::write(wt.join("src/lib.rs"), "pub fn theirs() {}\n").unwrap();
    git(&wt, &["commit", "-qam", "their line"]);
    let tip = git_out(r.path(), &["rev-parse", "speccoder/001-graph"]);
    let kept = speccoder_runtime::controls::restore(r.path(), None, "T005");
    assert_eq!(
        kept.status(),
        CommandStatus::NeedsHuman,
        "{}",
        kept.message()
    );
    assert!(
        kept.message().contains("no longer apply cleanly"),
        "{}",
        kept.message()
    );
    assert_eq!(
        git_out(r.path(), &["rev-parse", "speccoder/001-graph"]),
        tip
    );
    assert!(
        fs::read_to_string(wt.join("src/lib.rs"))
            .unwrap()
            .contains("theirs")
    );
    assert!(!git_out(r.path(), &["for-each-ref", &aside]).is_empty());
}

/// A lockfile the checks write, never committed, is no one's edit: the run
/// stops with the fix instead of setting it aside every time; once the
/// person commits it in the checkout, the next run brings it into the
/// branch and works.
#[test]
fn an_uncommitted_lockfile_stops_the_run_with_its_fix_until_committed() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "first" }));
    let first = run(r.path(), state.path(), "T005", d);
    let wt = PathBuf::from(&report(&first).worktree);
    // What `cargo` would leave in the worktree.
    fs::write(wt.join("Cargo.lock"), "# generated\nversion = 4\n").unwrap();
    let (_f2, d2) = fake();
    let res = run(r.path(), state.path(), "T005", d2);
    assert_eq!(
        res.status(),
        CommandStatus::GitPrecondition,
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("Cargo.lock is not committed"),
        "{}",
        res.message()
    );
    // The fix, as one action and as the command it amounts to.
    assert_eq!(res.code().map(|c| c.name), Some("SETUP-INCOMPLETE"));
    let remedy = res.remedy().expect("a remedy");
    assert_eq!(remedy.label, "Commit Cargo.lock");
    assert!(remedy.history);
    assert!(
        remedy.command.contains("git add Cargo.lock"),
        "{}",
        remedy.command
    );
    assert!(res.fix().iter().any(|f| f == "speccoder fix"));
    assert!(
        git_out(r.path(), &["for-each-ref", "refs/speccoder/attempts/"])
            .lines()
            .all(|l| !l.contains("set_aside"))
    );
    // The app's one click: copied into the checkout and committed.
    let fixed = speccoder_runtime::code::fix_setup(r.path(), None);
    assert_eq!(fixed.status(), CommandStatus::Ok, "{}", fixed.message());
    assert_eq!(
        git_out(r.path(), &["log", "-1", "--format=%s"]).trim(),
        "speccoder: setup"
    );
    assert!(git_out(r.path(), &["ls-files", "Cargo.lock"]).contains("Cargo.lock"));
    let (f3, d3) = fake();
    f3.will_call_tool("b", "report_blocked", json!({ "reason": "third" }));
    let res = run(r.path(), state.path(), "T005", d3);
    assert_ne!(
        res.status(),
        CommandStatus::GitPrecondition,
        "{}",
        res.message()
    );
    assert!(git_out(&wt, &["ls-files", "Cargo.lock"]).contains("Cargo.lock"));
}

/// Set-aside changes the person lets go are deleted; the branch stays.
#[test]
fn set_aside_changes_can_be_discarded() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "first" }));
    let first = run(r.path(), state.path(), "T005", d);
    let wt = PathBuf::from(&report(&first).worktree);
    fs::write(
        wt.join("src/lib.rs"),
        "pub fn ok() {}\npub fn scratch() {}\n",
    )
    .unwrap();
    let (f2, d2) = fake();
    f2.will_call_tool("b", "report_blocked", json!({ "reason": "second" }));
    let res = run(r.path(), state.path(), "T005", d2);
    let aside = report(&res).set_aside.clone().expect("set aside");
    let tip = git_out(r.path(), &["rev-parse", "speccoder/001-graph"]);
    let dropped = speccoder_runtime::controls::drop_set_aside(r.path(), None, "T005");
    assert_eq!(dropped.status(), CommandStatus::Ok, "{}", dropped.message());
    assert!(git_out(r.path(), &["for-each-ref", &aside]).is_empty());
    assert_eq!(
        git_out(r.path(), &["rev-parse", "speccoder/001-graph"]),
        tip
    );
}

// ── approvals ───────────────────────────────────────────────────────

use speccoder_runtime::approvals::{Verb, decide_with};

fn approvals(r: &CommandResult) -> &speccoder_core::ApprovalsReport {
    match r.data() {
        Some(CommandData::Approvals(a)) => a,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn write_manifest(f: &FakeProvider, id: &str, content: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": "package.json", "content": content }),
    );
}

#[test]
fn a_deferred_approval_resumes_the_attempt_without_asking_the_model_again() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    // 1. Nobody to ask: the attempt waits on the call (package.json is a
    //    dependency manifest), its tree kept.
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    let first = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    assert_eq!(report(&first).attempt_status, "needs_approval");
    assert_eq!(report(&first).pending.as_ref().unwrap()["waiting"], true);

    // 2. It is listed as a waiting call with what a decision would cover.
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let a = approvals(&listed);
    assert_eq!(a.pending.len(), 1, "{}", listed.message());
    assert_eq!(
        (
            a.pending[0].task.as_str(),
            a.pending[0].covers.as_str(),
            a.pending[0].source.as_str()
        ),
        ("T005", "write `package.json`", "task")
    );
    assert!(!a.pending[0].call.is_empty() && a.pending[0].deadline.is_some());

    // 3. Approve — no model needed — recorded with who decided.
    let ok = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    assert!(ok.message().contains("(asserted)"), "{}", ok.message());
    assert!(approvals(&ok).pending.is_empty());

    // 4. The next run continues the same attempt: the write is the one shown,
    //    and the model is asked only for what comes after it.
    let (f2, d2) = fake();
    f2.will_structure(done("manifest"));
    let second = run(r.path(), state.path(), "T005", d2);
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    let rep = report(&second);
    assert_eq!(rep.tool_calls[0]["decision"], "allow");
    assert_eq!(rep.tool_calls[0]["approved_by"], "person");
    assert_eq!(rep.tool_calls[0]["operator"]["basis"], "asserted");
    assert_eq!(f2.asked().len(), 1, "a finished model call was repeated");
    assert!(rep.ticked);
    // Strict replay holds: the decision is journal history.
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
}

#[test]
fn a_rejected_call_is_denied_with_the_persons_reason() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    run(r.path(), state.path(), "T005", d);
    let no = decide_with(
        r.path(),
        None,
        Verb::Reject("T005", "no new dependencies"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(no.status(), CommandStatus::Ok, "{}", no.message());

    // The waiting attempt continues with the call refused.
    let (f2, d2) = fake();
    f2.will_structure(done("gave up on the manifest"));
    let res = run(r.path(), state.path(), "T005", d2);
    let rep = report(&res);
    assert_eq!(rep.tool_calls[0]["decision"], "deny");
    assert_eq!(
        rep.tool_calls[0]["reason"],
        "a person rejected this: no new dependencies"
    );
    assert_eq!(rep.tool_calls[0]["operator"]["basis"], "asserted");
    // The attempt continued: one model turn after the refusal, nothing redone.
    assert_eq!(f2.asked().len(), 1);
    // Nothing awaits a decision any more; approving now is refused.
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    assert!(approvals(&listed).pending.is_empty());
    let again = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(again.status(), CommandStatus::NotFound);
}

#[test]
fn repeated_denials_stop_the_attempt_for_a_person() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    for id in ["a", "b", "c"] {
        f.will_call_tool(
            id,
            "write_file",
            json!({ "path": "speccoder.toml", "content": "" }),
        );
    }
    f.will_structure(done("never reached"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "denied");
    assert!(
        rep.reason
            .as_deref()
            .unwrap()
            .starts_with("3 calls in a row were denied")
    );
}

// ── inline approvals ────────────────────────────────────────────────

use speccoder_runtime::approvals::{Answer, Approver};

/// Answers from a script and remembers what it was asked.
#[derive(Debug)]
struct Scripted {
    answers: std::sync::Mutex<Vec<Answer>>,
    asked: std::sync::Mutex<Vec<speccoder_runtime::approvals::Request>>,
}

impl Scripted {
    fn new(answers: Vec<Answer>) -> Arc<Self> {
        Arc::new(Self {
            answers: std::sync::Mutex::new(answers),
            asked: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl Approver for Scripted {
    fn ask(&self, p: &speccoder_runtime::approvals::Presentation) -> Answer {
        self.asked.lock().unwrap().push(p.request.clone());
        self.answers.lock().unwrap().remove(0)
    }
}

fn yes(for_task: bool) -> Answer {
    let scope = if for_task {
        speccoder_runtime::approvals::Scope::Task
    } else {
        speccoder_runtime::approvals::Scope::Once
    };
    Answer::approve(scope, None, None)
}

#[test]
fn an_inline_approval_continues_the_same_attempt_and_replays_without_asking() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    f.will_structure(done("manifest"));
    let person = Scripted::new(vec![yes(false)]);
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        person.clone(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.tool_calls[0]["decision"], "allow");
    assert_eq!(rep.tool_calls[0]["approved_by"], "person");
    assert!(rep.ticked);
    // The person saw what and why.
    let asked = person.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].task, "001-graph/T005");
    assert_eq!(
        (asked[0].covers().as_str(), asked[0].risk.as_str()),
        ("write `package.json`", "medium")
    );
    // "Once" records no decision for the task.
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    assert!(approvals(&listed).decided.is_empty());
    // Strict replay reads the answer from the journal: nobody is asked.
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
    assert_eq!(person.asked.lock().unwrap().len(), 1);
}

#[test]
fn for_this_task_is_recorded_and_a_rejection_reaches_the_model() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    // "For this task": recorded like `speccoder task approve`.
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["curl", "https://example.com"] }),
    );
    f.will_structure(done("tried"));
    let person = Scripted::new(vec![yes(true), Answer::reject("no network", None)]);
    let res =
        speccoder_runtime::run_with_approver(r.path(), None, "T005", state.path(), d, None, person);
    let rep = report(&res);
    assert_eq!(rep.tool_calls[1]["decision"], "deny");
    assert_eq!(
        rep.tool_calls[1]["reason"],
        "a person rejected this: no network"
    );
    assert_eq!(rep.tool_calls[1]["rejected_by"], "person");
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let decided = &approvals(&listed).decided;
    assert_eq!(decided.len(), 1);
    assert_eq!(
        (decided[0].decision.as_str(), decided[0].covers.as_str()),
        ("approved", "write `package.json`")
    );
}

// ── human controls ──────────────────────────────────────────────────

fn status_of(r: &Path) -> speccoder_core::StatusReport {
    match speccoder_runtime::status::report(r, None).data() {
        Some(CommandData::Report(s)) => (**s).clone(),
        other => panic!("{other:?}"),
    }
}

fn state_of<'a>(s: &'a speccoder_core::StatusReport, id: &str) -> &'a speccoder_core::TaskStatus {
    s.tasks.iter().find(|t| t.id == id).unwrap()
}

#[test]
fn an_accepted_task_is_attested_never_verified_and_the_feature_moves_on() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let ok = speccoder_runtime::controls::accept_with(
        r.path(),
        None,
        "T002",
        "done by hand in PR 12",
        state.path(),
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    let s = status_of(r.path());
    let t2 = state_of(&s, "T002");
    assert_eq!(t2.state, "attested");
    assert_eq!(t2.note.as_deref(), Some("done by hand in PR 12"));
    assert_eq!(s.totals.verified, 0);
    // Accepting it again is refused.
    let again = speccoder_runtime::controls::accept_with(r.path(), None, "T002", "x", state.path());
    assert_eq!(again.status(), CommandStatus::TaskNotReady);
    // The feature run continues with T003.
    let (f, d) = fake();
    let (id, args) = edit("c1", "pub fn ok() {}\npub fn logout() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("logout"));
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    let tasks: Vec<String> = feature(&res)
        .attempts
        .iter()
        .map(|a| a.task.clone())
        .collect();
    assert_eq!(tasks, ["T003"], "{}", res.message());
}

#[test]
fn a_skipped_task_is_passed_by_and_its_dependents_stay_blocked() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let skip = speccoder_runtime::controls::skip(
        r.path(),
        None,
        "T002",
        Some("out of scope for this release"),
    );
    assert_eq!(skip.status(), CommandStatus::Ok, "{}", skip.message());
    let s = status_of(r.path());
    assert_eq!(state_of(&s, "T002").state, "skipped");
    assert_eq!(
        state_of(&s, "T002").note.as_deref(),
        Some("out of scope for this release")
    );
    assert_eq!(state_of(&s, "T003").state, "blocked");

    // No model call: nothing can run.
    let (f, d) = fake();
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(
        res.status(),
        CommandStatus::TaskNotReady,
        "{}",
        res.message()
    );
    assert!(
        feature(&res).stop.contains("T002 (skipped)"),
        "{}",
        feature(&res).stop
    );
    assert!(f.asked().is_empty());

    // Undo: ready again.
    let undo = speccoder_runtime::controls::skip(r.path(), None, "T002", None);
    assert_eq!(undo.status(), CommandStatus::Ok);
    assert_eq!(state_of(&status_of(r.path()), "T002").state, "ready");
    // Skipping a task that is not open is refused.
    let no = speccoder_runtime::controls::skip(r.path(), None, "T001", Some("x"));
    assert_eq!(no.status(), CommandStatus::NotFound);
}

#[test]
fn a_nudge_reaches_the_next_attempt_and_can_be_cleared() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let n = speccoder_runtime::controls::nudge_with(
        r.path(),
        None,
        "T005",
        Some("keep `ok` untouched; add a new fn"),
        state.path(),
    );
    assert_eq!(n.status(), CommandStatus::Ok, "{}", n.message());
    // Each attempt stops honestly, so the task stays open for the next one.
    let (f, d) = fake();
    f.will_call_tool("b1", "report_blocked", json!({ "reason": "waiting" }));
    run(r.path(), state.path(), "T005", d);
    let prompt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        prompt.contains("Notes from a person") && prompt.contains("keep `ok` untouched"),
        "{prompt}"
    );

    speccoder_runtime::controls::nudge_with(r.path(), None, "T005", None, state.path());
    let (f2, d2) = fake();
    f2.will_call_tool("b2", "report_blocked", json!({ "reason": "waiting" }));
    run(r.path(), state.path(), "T005", d2);
    let prompt = serde_json::to_string(&f2.asked()[0].prompt).unwrap();
    assert!(!prompt.contains("Notes from a person"));
}

#[test]
fn cost_counts_every_recorded_attempt_per_task() {
    let r = repo("[budget]\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    // A failed attempt (saved), then a passing one (committed).
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_structure(done("broke it"));
    run(r.path(), state.path(), "T005", d);
    let (f2, d2) = fake();
    f2.will_structure(done("fine"));
    run(r.path(), state.path(), "T005", d2);

    let res = speccoder_runtime::controls::cost(r.path(), None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Cost(c)) = res.data() else {
        panic!()
    };
    assert_eq!(c.tasks.len(), 1);
    assert_eq!((c.tasks[0].task.as_str(), c.tasks[0].attempts), ("T005", 2));
    // Without prices: tokens only.
    assert!(c.total.usd.is_none() && c.priced_model.is_none());
    assert!(res.message().contains("price_input_per_mtok"));
}

// ── live progress ───────────────────────────────────────────────────

use speccoder_runtime::progress::{Event, Progress};

#[derive(Debug, Default)]
struct Recorder(std::sync::Mutex<Vec<Event>>);

impl Progress for Recorder {
    fn event(&self, e: &Event) {
        self.0.lock().unwrap().push(e.clone());
    }
}

#[test]
fn a_run_reports_its_steps_live_and_status_reads_them() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "speccoder.toml", "content": "" }),
    );
    f.will_structure(done("looked"));
    let rec = Arc::new(Recorder::default());
    let options = speccoder_runtime::RunOptions {
        max_tasks: Some(1),
        progress: Some(rec.clone()),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    let events = rec.0.lock().unwrap().clone();
    let kinds: Vec<String> = events
        .iter()
        .map(|e| match e {
            Event::RunStarted { tasks, .. } => format!("plan {}", tasks.len()),
            Event::TaskStarted { task, .. } => format!("start {task}"),
            Event::Stage { stage, state, .. } => format!(
                "{} {}",
                stage.name(),
                serde_json::to_value(state).unwrap().as_str().unwrap()
            ),
            Event::Resumed { task, .. } => format!("resumed {task}"),
            Event::ApprovalRequested { task, .. } => format!("needs person {task}"),
            Event::ApprovalDecided { task, decision, .. } => format!("{decision} {task}"),
            Event::ToolCall {
                tool,
                decision,
                turn,
                ..
            } => format!("{turn} {tool} {decision}"),
            Event::Usage { .. } => "usage".to_owned(),
            Event::TaskFinished { outcome, .. } => format!("end {outcome}"),
            Event::Said { .. } => "said".to_owned(),
            Event::Reasoning { .. } => "reasoning".to_owned(),
            Event::Notice { text } => text.clone(),
            Event::Bug { step, .. } => step.name().to_owned(),
            Event::Oracle { .. } => "oracle".to_owned(),
            Event::CollectionChecked { .. } => "collection".to_owned(),
            Event::RegressionBisected { .. } => "bisected".to_owned(),
        })
        .filter(|k| k != "usage")
        .collect();
    assert_eq!(
        kinds,
        [
            "plan 11",
            "start T005",
            "baseline started",
            "baseline passed",
            "red skipped",
            "attempt started",
            "1 read_file allow",
            "2 write_file deny",
            "attempt passed",
            "verify started",
            "verify passed",
            "audit passed",
            "mutation skipped",
            "monitor skipped",
            "commit started",
            "commit passed",
            "end regression_only",
        ],
        "{}",
        res.message()
    );

    // `status` from "another terminal": the run is over.
    let st = speccoder_runtime::progress::status_with(r.path(), state.path());
    assert_eq!(st.status(), CommandStatus::Ok);
    assert!(
        st.message()
            .starts_with("last run: 001-graph T005 — regression_only"),
        "{}",
        st.message()
    );
}

#[test]
fn status_tells_an_interrupted_run_from_a_running_one() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let dir = speccoder_runtime::worktree::project_state_dir(
        state.path(),
        &fs::canonicalize(r.path()).unwrap(),
    );
    fs::create_dir_all(&dir).unwrap();
    let write = |pid: u32| {
        let live = speccoder_runtime::progress::LiveState {
            pid,
            feature: "001-graph".to_owned(),
            task: "T005".to_owned(),
            stage: Some(speccoder_core::stage::Stage::Attempt),
            stage_detail: Some("1/2".to_owned()),
            ..Default::default()
        };
        fs::write(dir.join("live.json"), serde_json::to_string(&live).unwrap()).unwrap();
    };
    // This test process is alive: "running".
    write(std::process::id());
    let st = speccoder_runtime::progress::status_with(r.path(), state.path());
    assert!(st.message().starts_with("running (pid"), "{}", st.message());
    // A pid that does not exist: "interrupted".
    write(u32::MAX / 2);
    let st = speccoder_runtime::progress::status_with(r.path(), state.path());
    assert!(st.message().starts_with("interrupted:"), "{}", st.message());
}

// ── the gaming monitor ──────────────────────────────────────────────

fn monitor_says(suspected: bool) -> Value {
    if suspected {
        json!({ "suspected": true, "findings": [{ "kind": "hardcoded_output", "evidence": "pub fn login() -> u32 { 42 }", "explanation": "returns the test's expected value" }] })
    } else {
        json!({ "suspected": false, "findings": [] })
    }
}

#[test]
fn a_suspected_attempt_is_saved_for_a_person_not_ticked() {
    let r = repo("[monitor]\nenabled = true\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() -> u32 { 42 }" }));
    f.will_structure(done("SECRET-SUMMARY implemented login properly"));
    f.will_structure(monitor_says(true));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert!(
        res.message().contains("hardcoded_output"),
        "{}",
        res.message()
    );
    let rep = report(&res);
    // The checks passed — the verdict stands — but nothing was ticked.
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert!(!rep.ticked && rep.commit.is_none());
    let saved = rep.saved_attempt.as_deref().unwrap();
    let wt = Path::new(&rep.worktree);
    assert!(
        git_out(wt, &["log", "-1", "--format=%B", saved]).contains("Outcome: suspected_gaming")
    );
    // Reasoning-blind: the monitor saw the diff and the calls, not the model's account.
    let asked = f.asked();
    let monitor = serde_json::to_string(&asked.last().unwrap().prompt).unwrap();
    assert!(asked.last().unwrap().tools.is_empty());
    assert!(
        monitor.contains("pub fn login() -> u32 { 42 }")
            && monitor.contains("edit_file src/lib.rs"),
        "{monitor}"
    );
    assert!(!monitor.contains("SECRET-SUMMARY"), "{monitor}");
}

#[test]
fn a_clean_monitor_verdict_ticks_and_a_failed_task_is_not_monitored() {
    let r = repo("[monitor]\nenabled = true\n[budget]\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    // A failing attempt: no monitor call.
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_structure(done("broke it"));
    let failed = run(r.path(), state.path(), "T005", d);
    assert_eq!(failed.status(), CommandStatus::VerificationFailed);
    assert!(report(&failed).monitor.is_none());
    assert_eq!(f.asked().len(), 2);
    // A passing one the monitor finds clean: ticked as usual.
    let (f2, d2) = fake();
    f2.will_structure(done("nothing to change"));
    f2.will_structure(monitor_says(false));
    let ok = run(r.path(), state.path(), "T005", d2);
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    assert!(report(&ok).ticked);
    assert_eq!(report(&ok).monitor.as_ref().unwrap()["suspected"], false);
}

#[test]
fn the_monitor_is_off_by_default() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("fine"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok);
    assert!(report(&res).monitor.is_none());
    assert_eq!(f.asked().len(), 1);
}

#[test]
fn an_approved_frozen_test_edit_passes_but_an_approved_weakening_is_refused() {
    // Adding a test to an existing file, approved: a pass.
    let r = repo_with(LISTED, &[EXISTING_TEST]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "write_file", json!({ "path": "tests/existing.rs", "content": "#[test]\nfn existing() {}\n#[test]\nfn added() {}\n" }));
    let first = run(r.path(), state.path(), "T005", d);
    assert_eq!(report(&first).attempt_status, "needs_approval");
    decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    let (f2, d2) = fake();
    f2.will_call_tool("c2", "write_file", json!({ "path": "tests/existing.rs", "content": "#[test]\nfn existing() {}\n#[test]\nfn added() {}\n" }));
    f2.will_structure(done("added a test"));
    let ok = run(r.path(), state.path(), "T005", d2);
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    assert!(
        report(&ok).integrity.is_empty(),
        "{:?}",
        report(&ok).integrity
    );

    // Removing the test, approved: refused when written — the test stays.
    let r = repo_with("[budget]\nmax_attempts = 1\n", &[EXISTING_TEST]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "tests/existing.rs", "content": "// nothing\n" }),
    );
    run(r.path(), state.path(), "T005", d);
    decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    let (f2, d2) = fake();
    f2.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "tests/existing.rs", "content": "// nothing\n" }),
    );
    f2.will_structure(done("simplified"));
    let bad = run(r.path(), state.path(), "T005", d2);
    let wt = PathBuf::from(&report(&bad).worktree);
    assert!(
        fs::read_to_string(wt.join("tests/existing.rs"))
            .unwrap()
            .contains("fn existing"),
        "{}",
        bad.message()
    );
    assert!(
        report(&bad)
            .integrity
            .iter()
            .all(|i| !i.contains("test removed")),
        "{:?}",
        report(&bad).integrity
    );
}

#[test]
fn a_spec_without_template_story_headings_still_yields_acceptance_tests() {
    // Written by a model that skipped `### User Story 1 - …` headings.
    let spec = (
        "specs/001-graph/spec.md",
        "# Spec\n\n## User Story\n\nUsers log in.\n\n**Acceptance**: a `login` function exists.\n",
    );
    let r = repo_with("", &[spec]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance("tests/acceptance_login.txt", GREP_LOGIN));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::Verified));
    assert!(
        f.asked()[0].tools.is_empty(),
        "the judge-side acceptance call came first"
    );
}

// ── [tests] frozen = "additions" ───────────────────────────────────────────

#[test]
fn in_additions_mode_new_tests_in_a_frozen_file_need_no_approval() {
    let r = repo_with(
        &format!("{LISTED}[tests]\nfrozen = \"additions\"\n"),
        &[EXISTING_TEST],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "tests/existing.rs", "old_string": "fn existing() {}\n", "new_string": "fn existing() {}\n\n#[test]\nfn added() {}\n" }));
    f.will_structure(done("added a test"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.tool_calls[0]["decision"], "allow");
    assert_eq!(rep.tool_calls[0]["frozen_additions"], true);
    assert!(rep.integrity.is_empty(), "{:?}", rep.integrity);
    assert!(
        fs::read_to_string(Path::new(&rep.worktree).join("tests/existing.rs"))
            .unwrap()
            .contains("fn added()")
    );
    // The check read the file through a journaled call: replay holds.
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
}

#[test]
fn in_additions_mode_changing_an_existing_test_line_still_asks() {
    let r = repo_with("[tests]\nfrozen = \"additions\"\n", &[EXISTING_TEST]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "tests/existing.rs", "old_string": "fn existing() {}", "new_string": "fn renamed() {}" }));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(report(&res).attempt_status, "needs_approval");
    let untouched =
        fs::read_to_string(Path::new(&report(&res).worktree).join("tests/existing.rs")).unwrap();
    assert_eq!(untouched, EXISTING_TEST.1);
}

#[test]
fn the_configured_formatter_runs_after_the_attempt_and_before_the_checks() {
    let r = repo(
        "[format]\nargv = [\"sh\", \"-c\", \"printf 'formatted\\\\n' > src/formatted.txt\"]\n[checks.formatted]\nargv = [\"grep\", \"-q\", \"formatted\", \"src/formatted.txt\"]\n",
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    // The formatter's output is part of the attempt (and of its commit).
    assert!(
        rep.changes
            .iter()
            .any(|(p, k)| p == "src/formatted.txt" && k == "added"),
        "{:?}",
        rep.changes
    );
}

// ── mutation evidence ───────────────────────────────────────────────

/// A mutation tool that reports one killed and one surviving mutant.
const MUTATION: &str = "[mutation]\ntool = \"command\"\nreport = \"{out}/r.json\"\nargv = [\"sh\", \"-c\", \"mkdir -p \\\"$1\\\" && echo \\\"$2\\\" > \\\"$1/files\\\" && printf '%s' '{\\\"files\\\":{\\\"src/lib.rs\\\":{\\\"mutants\\\":[{\\\"status\\\":\\\"Killed\\\"},{\\\"status\\\":\\\"Survived\\\",\\\"mutatorName\\\":\\\"BlockStatement\\\",\\\"replacement\\\":\\\"{}\\\",\\\"location\\\":{\\\"start\\\":{\\\"line\\\":2}}}]}}}' > \\\"$1/r.json\\\"\", \"sh\", \"{out}\", \"{files_csv}\"]\n";

fn login_attempt(f: &FakeProvider) {
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("added login"));
}

#[test]
fn mutation_results_are_evidence_and_a_trailer() {
    let r = repo(MUTATION);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    login_attempt(&f);
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    let m = rep.mutation.as_ref().unwrap();
    assert_eq!(
        (m["caught"].as_u64(), m["missed"].as_u64()),
        (Some(1), Some(1)),
        "{m}"
    );
    assert_eq!(m["files"], json!(["src/lib.rs"]));
    let body = git_out(Path::new(&rep.worktree), &["log", "-1", "--format=%B"]);
    assert!(body.contains("Mutation: caught=1 missed=1"), "{body}");
}

#[test]
fn required_mutation_coverage_fails_the_verdict_with_the_survivors() {
    let r = repo(&format!(
        "{}mode = \"require\"\n[budget]\nmax_attempts = 1\n",
        MUTATION
    ));
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    login_attempt(&f);
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let rep = report(&res);
    let Some(Verdict::Failed { reasons }) = &rep.verdict else {
        panic!("{:?}", rep.verdict)
    };
    assert!(
        reasons[0].starts_with("mutation: 1 of 2 mutants") && reasons[0].contains("src/lib.rs:2"),
        "{reasons:?}"
    );
}

// ── best-of-n ───────────────────────────────────────────────────────

#[test]
fn best_of_n_keeps_the_passing_sample_with_the_smallest_change() {
    let r = repo("[budget]\nbest_of = 3\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // 1: small and passing. 2: breaks the check. 3: passing but larger.
    f.will_call_tool("a", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("small"));
    f.will_call_tool(
        "b",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn renamed() {}\n" }),
    );
    f.will_structure(done("broken"));
    f.will_call_tool("c", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}\npub fn logout() {}\npub fn audit() {}" }));
    f.will_structure(done("large"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.samples.len(), 3);
    let kept: Vec<bool> = rep.samples.iter().map(|s| s["kept"] == true).collect();
    assert_eq!(kept, [true, false, false], "{:?}", rep.samples);
    assert_eq!(rep.samples[1]["rank"], 0);
    assert_eq!(rep.model_report.as_ref().unwrap()["summary"], "small");
    // The worktree (and the commit) hold the kept sample, not the last one.
    let lib = fs::read_to_string(Path::new(&rep.worktree).join("src/lib.rs")).unwrap();
    assert_eq!(lib, "pub fn ok() {}\npub fn login() {}\n");
    assert!(rep.ticked && rep.commit.is_some());
}

// ── parallel [P] tasks in lanes ─────────────────────────────────────

/// Answers each lane from its own script: a request goes to the fake whose
/// key occurs in it (the task titles differ).
#[derive(Debug)]
struct Router(Vec<(&'static str, Arc<FakeProvider>)>);

#[agentplane::prelude::async_trait]
impl ModelProvider for Router {
    async fn complete(
        &self,
        request: agentplane::model::Request<'_>,
    ) -> Result<agentplane::model::Completion, agentplane::model::ModelError> {
        let text = format!("{request:?}");
        let fake = self
            .0
            .iter()
            .find(|(k, _)| text.contains(k))
            .map(|(_, f)| Arc::clone(f))
            .unwrap_or_else(|| panic!("no script for this request"));
        fake.complete(request).await
    }

    fn gen_ai_provider(&self) -> Option<&str> {
        None
    }
}

const PARALLEL_TASKS: &str = "# Tasks\n\n## Phase 1: Setup\n\n- [x] T001 Create the crate\n\n## Phase 2: User Story 1 - Login\n\n- [ ] T002 [P] [US1] Add alpha in src/alpha.rs\n- [ ] T003 [P] [US1] Add beta in src/beta.rs\n";

fn lanes(
    alpha: impl Fn(&FakeProvider),
    beta: impl Fn(&FakeProvider),
) -> (tempfile::TempDir, Arc<dyn ModelProvider>) {
    let r = repo_with("", &[("specs/001-graph/tasks.md", PARALLEL_TASKS)]);
    let (a, b) = (FakeProvider::new(), FakeProvider::new());
    alpha(&a);
    beta(&b);
    let d: Arc<dyn ModelProvider> = Arc::new(Router(vec![("alpha", a), ("beta", b)]));
    (r, d)
}

fn write(f: &FakeProvider, path: &str, content: &str) {
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": path, "content": content }),
    );
    f.will_structure(done("wrote it"));
}

#[test]
fn parallel_tasks_run_in_lanes_and_merge_with_both_ticks() {
    let (r, d) = lanes(
        |f| write(f, "src/alpha.rs", "pub fn alpha() {}\n"),
        |f| write(f, "src/beta.rs", "pub fn beta() {}\n"),
    );
    let state = tempfile::tempdir().unwrap();
    let options = speccoder_runtime::RunOptions {
        parallel: 2,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    assert_eq!(fr.attempts.len(), 2);
    assert!(
        fr.parallel
            .iter()
            .any(|l| l.contains("T002, T003 ran in parallel")),
        "{:?}",
        fr.parallel
    );
    let wt = Path::new(&fr.attempts[0].worktree);
    // The lanes' worktrees are gone; the feature's holds both changes.
    let feature_wt = wt.parent().unwrap().join("001-graph");
    assert!(
        !wt.ends_with("001-graph") && !wt.exists(),
        "lane left behind: {}",
        wt.display()
    );
    let tasks = fs::read_to_string(feature_wt.join("specs/001-graph/tasks.md")).unwrap();
    assert!(
        tasks.contains("- [X] T002") && tasks.contains("- [X] T003"),
        "{tasks}"
    );
    assert!(feature_wt.join("src/alpha.rs").exists() && feature_wt.join("src/beta.rs").exists());
    let log = git_out(&feature_wt, &["log", "--format=%s"]);
    assert_eq!(
        log.lines()
            .filter(|l| l.starts_with("speccoder: T00"))
            .count(),
        2,
        "{log}"
    );
    // Each attestation moved with its work onto the feature branch, and
    // checks out there; the lanes' commits keep none.
    let ticks = git_out(&feature_wt, &["log", "--format=%H", "--grep=^Spec-Task:"]);
    let noted = git_out(&feature_wt, &["notes", "--ref=speccoder", "list"]);
    for commit in ticks.lines() {
        assert!(noted.contains(commit), "{commit} has no note: {noted}");
        assert!(
            fr.attempts
                .iter()
                .any(|a| a.commit.as_deref() == Some(commit))
        );
    }
    assert_eq!(noted.lines().count(), 2, "{noted}");
    let trust = speccoder_runtime::attest::verify::Trust::default();
    let checked =
        speccoder_runtime::attest::verify::verify_at(&feature_wt, None, None, &trust, false);
    assert_eq!(checked.status(), CommandStatus::Ok, "{}", checked.message());
}

#[test]
fn a_lane_that_conflicts_runs_again_on_its_own() {
    let (r, d) = lanes(
        |f| write(f, "src/shared.rs", "pub fn alpha() {}\n"),
        |f| {
            write(f, "src/shared.rs", "pub fn beta() {}\n");
            // Its second run, alone, on top of alpha's.
            write(f, "src/beta.rs", "pub fn beta() {}\n");
        },
    );
    let state = tempfile::tempdir().unwrap();
    let options = speccoder_runtime::RunOptions {
        parallel: 2,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    assert!(
        fr.parallel
            .iter()
            .any(|l| l.contains("T003: not merged (conflicts in src/shared.rs)")),
        "{:?}",
        fr.parallel
    );
    assert_eq!(fr.attempts.len(), 3);
    assert_eq!(fr.stop, "feature done: every task is ticked");
}

#[test]
fn lanes_that_fail_together_are_undone_and_run_one_at_a_time() {
    let r = repo_with(
        "[budget]\nmax_attempts = 1\n[checks.not-both]\nargv = [\"sh\", \"-c\", \"! (test -f src/alpha.rs && test -f src/beta.rs)\"]\n",
        &[("specs/001-graph/tasks.md", PARALLEL_TASKS)],
    );
    let (a, b) = (FakeProvider::new(), FakeProvider::new());
    for _ in 0..2 {
        write(&a, "src/alpha.rs", "pub fn alpha() {}\n");
        write(&b, "src/beta.rs", "pub fn beta() {}\n");
    }
    let d: Arc<dyn ModelProvider> = Arc::new(Router(vec![("alpha", a), ("beta", b)]));
    let state = tempfile::tempdir().unwrap();
    let options = speccoder_runtime::RunOptions {
        parallel: 2,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    let fr = feature(&res);
    assert!(
        fr.parallel
            .iter()
            .any(|l| l.contains("passed alone but not together")),
        "{:?}",
        fr.parallel
    );
    // One at a time: T002 passes, T003 then fails the check honestly.
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    assert!(fr.stop.starts_with("T003"), "{}", fr.stop);
    assert_eq!(fr.attempts.len(), 4);
}

#[test]
fn files_the_checks_generate_do_not_empty_a_lane_merge() {
    // Like `cargo test` writing Cargo.lock: every check run leaves gen.lock.
    let r = repo_with(
        "[checks.gen]\nargv = [\"sh\", \"-c\", \"echo v1 > gen.lock\"]\n",
        &[("specs/001-graph/tasks.md", PARALLEL_TASKS)],
    );
    let (a, b) = (FakeProvider::new(), FakeProvider::new());
    write(&a, "src/alpha.rs", "pub fn alpha() {}\n");
    write(&b, "src/beta.rs", "pub fn beta() {}\n");
    let d: Arc<dyn ModelProvider> = Arc::new(Router(vec![("alpha", a), ("beta", b)]));
    let state = tempfile::tempdir().unwrap();
    let options = speccoder_runtime::RunOptions {
        parallel: 2,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    let feature_wt = Path::new(&fr.attempts[0].worktree)
        .parent()
        .unwrap()
        .join("001-graph");
    // Every ticked task's work is on the branch.
    for f in ["src/alpha.rs", "src/beta.rs"] {
        let shown = Command::new("git")
            .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
            .args(["show", &format!("HEAD:{f}")])
            .current_dir(&feature_wt)
            .output()
            .unwrap();
        assert!(
            shown.status.success(),
            "{f} missing on the branch: {:?}",
            fr.parallel
        );
    }
}

#[test]
fn a_spec_fixed_in_the_checkout_reaches_the_worktree() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let (id, args) = edit("T002", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(&id, "edit_file", args);
    f.will_structure(done("login"));
    let first = run(r.path(), state.path(), "T002", d);
    assert_eq!(first.status(), CommandStatus::Ok, "{}", first.message());
    // The person clarifies the spec and commits it.
    fs::write(
        r.path().join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in. Logout clears the session.\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "clarify logout"]);
    let (f, d) = fake();
    let (id, args) = edit("T003", "pub fn ok() {}\npub fn logout() {}");
    f.will_call_tool(&id, "edit_file", args);
    f.will_structure(done("logout"));
    let second = run(r.path(), state.path(), "T003", d);
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    let wt = Path::new(&report(&second).worktree);
    let spec = fs::read_to_string(wt.join("specs/001-graph/spec.md")).unwrap();
    assert!(spec.contains("Logout clears the session"), "{spec}");
    // T002's tick survived the merge.
    let tasks = fs::read_to_string(wt.join("specs/001-graph/tasks.md")).unwrap();
    assert!(
        tasks.contains("- [X] T002") && tasks.contains("- [X] T003"),
        "{tasks}"
    );
}

// ── pull request handoff ────────────────────────────────────────────

#[test]
fn pr_is_a_dry_run_until_confirmed_then_a_draft_with_the_evidence() {
    use speccoder_runtime::handoff::{PrOptions, pr};
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    login_attempt(&f);
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    // The remote names o/r on GitHub; pushes land in the bare repository.
    git(
        r.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    git(
        r.path(),
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    // GitHub's API, stood in: no pull request yet; a new draft is #1.
    let api = fake_github::serve(vec![
        (
            "POST /graphql",
            200,
            r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#.to_owned(),
        ),
        (
            "POST /repos/o/r/pulls",
            201,
            r#"{"number":1,"html_url":"https://github.com/o/r/pull/1"}"#.to_owned(),
        ),
    ]);
    let ctx = speccoder_runtime::github::Ctx::fixed(&api.url, Some("t"));
    let mut o = PrOptions {
        remote: "origin",
        base: Some("main"),
        open: false,
        force: false,
        github: &ctx,
    };

    let dry = pr(r.path(), None, &o);
    assert_eq!(dry.status(), CommandStatus::Ok, "{}", dry.message());
    let Some(CommandData::Pr(p)) = dry.data() else {
        panic!("{}", dry.message())
    };
    assert!(!p.opened && p.branch == "speccoder/001-graph");
    assert!(
        p.body.contains("| T005 ") && p.body.contains("regression only"),
        "{}",
        p.body
    );
    assert!(p.body.contains("speccoder never merges"));
    // Each tick's attestation, and how to check it.
    assert!(p.body.contains("| Attestation |"), "{}", p.body);
    assert!(
        p.body
            .contains("speccoder attest verify --feature 001-graph"),
        "{}",
        p.body
    );
    assert!(
        p.commands
            .iter()
            .any(|c| c == "git push origin refs/notes/speccoder"),
        "{:?}",
        p.commands
    );
    assert!(
        git_out(remote.path(), &["branch", "--list"])
            .trim()
            .is_empty(),
        "pushed in a dry run"
    );

    o.open = true;
    let opened = pr(r.path(), None, &o);
    assert_eq!(opened.status(), CommandStatus::Ok, "{}", opened.message());
    let Some(CommandData::Pr(p)) = opened.data() else {
        panic!()
    };
    assert_eq!(p.url.as_deref(), Some("https://github.com/o/r/pull/1"));
    assert!(git_out(remote.path(), &["branch", "--list"]).contains("speccoder/001-graph"));
    assert!(
        !git_out(remote.path(), &["notes", "--ref=speccoder", "list"])
            .trim()
            .is_empty(),
        "the attestation notes were not pushed"
    );
    let calls = api.calls();
    assert_eq!(
        calls,
        ["POST /graphql", "POST /repos/o/r/pulls"],
        "{calls:?}"
    );
    let sent = api.requests.lock().unwrap()[1].body.clone();
    let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(sent["draft"], true);
    assert_eq!(
        (sent["base"].as_str(), sent["head"].as_str()),
        (Some("main"), Some("speccoder/001-graph"))
    );
    assert!(
        sent["body"]
            .as_str()
            .unwrap()
            .contains("speccoder never merges")
    );
}

/// the issues a feature's spec names are closed by its pull request,
/// and a branch whose pull request is open is updated, never opened twice.
#[test]
fn pr_closes_the_spec_s_issues_and_updates_an_open_pull_request() {
    use speccoder_runtime::handoff::{PrOptions, pr};
    let r = repo("");
    fs::write(
        r.path().join("specs/001-graph/spec.md"),
        "# Spec\n\n**Input**: Login (GitHub issue https://github.com/o/r/issues/42).\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "spec from issue 42"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    login_attempt(&f);
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    // The remote names o/r on GitHub; pushes land in the bare repository.
    git(
        r.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    git(
        r.path(),
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    // GitHub's API, stood in: the branch has open pull request #7.
    let api = fake_github::serve(vec![
        (
            "POST /graphql",
            200,
            r#"{"data":{"repository":{"pullRequests":{"nodes":[{"number":7,"title":"t","url":"https://github.com/o/r/pull/7","state":"OPEN","isDraft":true,"reviewDecision":null,"commits":{"nodes":[]}}]}}}}"#.to_owned(),
        ),
        ("PATCH /repos/o/r/pulls/7", 200, r#"{"number":7}"#.to_owned()),
    ]);
    let ctx = speccoder_runtime::github::Ctx::fixed(&api.url, Some("t"));
    let mut o = PrOptions {
        remote: "origin",
        base: Some("main"),
        open: false,
        force: false,
        github: &ctx,
    };

    let dry = pr(r.path(), None, &o);
    let Some(CommandData::Pr(p)) = dry.data() else {
        panic!("{}", dry.message())
    };
    assert!(
        p.body.contains("Closes https://github.com/o/r/issues/42"),
        "{}",
        p.body
    );

    o.open = true;
    let updated = pr(r.path(), None, &o);
    assert_eq!(updated.status(), CommandStatus::Ok, "{}", updated.message());
    assert!(
        updated.message().contains("updated pull request #7"),
        "{}",
        updated.message()
    );
    let calls = api.calls();
    assert!(
        !calls.iter().any(|c| c == "POST /repos/o/r/pulls"),
        "a second pull request was opened: {calls:?}"
    );
    // The open one's description is brought up to date with the evidence.
    let patch = api
        .requests
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.method == "PATCH")
        .cloned()
        .unwrap();
    assert!(
        patch
            .body
            .contains("Closes https://github.com/o/r/issues/42"),
        "{}",
        patch.body
    );
    assert!(
        git_out(remote.path(), &["branch", "--list"]).contains("speccoder/001-graph"),
        "not pushed"
    );
}

// ── provenance and replay verdicts ─────────────────────────────────

fn admitted(
    repo: &Path,
    state: &Path,
    run_id: &str,
) -> agentplane::core::Tainted<serde_json::Value> {
    let plane = speccoder_runtime::worktree::project_state_dir(state, repo);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(speccoder_runtime::substrate::admitted(&plane, run_id))
        .unwrap()
        .unwrap()
}

#[test]
fn a_retry_input_names_the_verifier_as_the_source_of_its_check_output() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    break_lib(&f, "b1");
    f.will_call_tool(
        "ok",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn ok() {}\npub fn login() {}\n" }),
    );
    f.will_structure(done("fixed"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.earlier.len(), 1);
    // The first attempt read nothing untrusted; the retry quotes the checks.
    let first = admitted(r.path(), state.path(), &rep.earlier[0].run_id);
    assert!(!first.label().is_untrusted(), "{:?}", first.label());
    let retry = admitted(r.path(), state.path(), &rep.run_id);
    assert!(retry.label().is_untrusted());
    // by part, as admitted — the spec trusted, the digest the verifier's.
    assert!(!retry.label_at("/bundle").unwrap().is_untrusted());
    assert!(!retry.label_at("/settings/nudges").unwrap().is_untrusted());
    let digest = retry.label_at("/digest").unwrap();
    assert!(
        digest
            .provenance
            .iter()
            .any(|s| s.to_string() == "speccoder://verifier"),
        "{digest:?}"
    );
}

#[test]
fn failed_and_blocked_runs_replay_as_reproduced() {
    // Blocked: the model stops honestly.
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("b", "report_blocked", json!({ "reason": "contradiction" }));
    let blocked = run(r.path(), state.path(), "T005", d);
    let id = report(&blocked).run_id.clone();
    let replayed = speccoder_runtime::replay_with(r.path(), &id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
    assert!(
        replayed.message().starts_with("reproduced"),
        "{}",
        replayed.message()
    );

    // Failed: the model's answer is unusable, so the run itself fails.
    let r = repo("[budget]\nmax_attempts = 1\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(json!({ "not": "a report" }));
    let failed = run(r.path(), state.path(), "T005", d);
    let rep = report(&failed);
    assert_eq!(rep.attempt_status, "failed", "{}", failed.message());
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
}

/// Approves after thinking for longer than three leases.
#[derive(Debug)]
struct Slow;

impl speccoder_runtime::approvals::Approver for Slow {
    fn ask(
        &self,
        _: &speccoder_runtime::approvals::Presentation,
    ) -> speccoder_runtime::approvals::Answer {
        std::thread::sleep(std::time::Duration::from_secs(13));
        speccoder_runtime::approvals::Answer::approve(
            speccoder_runtime::approvals::Scope::Once,
            None,
            None,
        )
    }
}

// A long wait completes in the same run. In one process nobody can take a
// run over (the journal is locked to it), so this guards the outcome, not
// the mechanism: the prompt waits off the runtime's thread because
// agentplane's `Effect::perform` must not block — a blocked thread starves
// lease renewal and every other task on it (MCP servers, progress).
#[test]
fn a_person_who_takes_their_time_keeps_the_run() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // `touch` needs approval; the person answers after 13 s (the lease is 4 s).
    f.will_call_tool(
        "t",
        "run_command",
        json!({ "argv": ["touch", "approved.txt"] }),
    );
    f.will_call_tool("e", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("added login"));
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        Arc::new(Slow),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(
        rep.tool_calls[0]["approved_by"], "person",
        "{:?}",
        rep.tool_calls
    );
    assert!(Path::new(&rep.worktree).join("approved.txt").exists());
    assert!(rep.earlier.is_empty(), "the run was not taken over");
}

// ── Approval experience: notes, stop, task and run scope ────────────

use speccoder_runtime::approvals::Scope;

#[test]
fn a_note_rides_with_the_approved_call_to_the_model() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    f.will_structure(done("manifest"));
    let person = Scripted::new(vec![Answer::approve(
        Scope::Once,
        Some("keep it to an empty object".to_owned()),
        None,
    )]);
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        person.clone(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.tool_calls[0]["scope"], "once");
    assert_eq!(rep.tool_calls[0]["note"], "keep it to an empty object");
    let next = f.asked()[1]
        .exchanges
        .iter()
        .map(|e| e.output.to_string())
        .collect::<String>();
    assert!(
        next.contains("Note from the person who approved this call: keep it to an empty object"),
        "{next}"
    );
    // The person was shown the question in full.
    let asked = person.asked.lock().unwrap().clone();
    assert_eq!((asked[0].kind.as_str(), asked[0].turn), ("write", 1));
    assert_eq!(asked[0].options, ["once", "task", "run", "reject", "stop"]);
}

#[test]
fn later_keeps_the_call_waiting_for_speccoder_approve() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    let person = Scripted::new(vec![Answer::stop(None)]);
    let res =
        speccoder_runtime::run_with_approver(r.path(), None, "T005", state.path(), d, None, person);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "needs_approval");
    assert_eq!(rep.pending.as_ref().unwrap()["waiting"], true);
    assert!(
        rep.saved_attempt.is_none(),
        "a waiting attempt is kept, not saved and undone"
    );
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let pending = &approvals(&listed).pending;
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert_eq!(
        (pending[0].task.as_str(), pending[0].source.as_str()),
        ("T005", "task")
    );
}

#[test]
fn a_task_answer_covers_the_rest_of_the_attempt() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    write_manifest(&f, "c2", "{\"name\": \"x\"}\n");
    f.will_call_tool(
        "c3",
        "run_command",
        json!({ "argv": ["curl", "https://example.com"] }),
    );
    f.will_structure(done("manifest twice"));
    let person = Scripted::new(vec![
        Answer::approve(Scope::Task, None, None),
        Answer::reject("no network", None),
    ]);
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        person.clone(),
    );
    let rep = report(&res);
    assert_eq!(
        rep.tool_calls[1]["scope"], "task-grant",
        "{:?}",
        rep.tool_calls
    );
    let asked = person.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "the second write was not asked about");
    // A high-risk command offers no "for this run".
    assert_eq!(asked[1].risk, "high");
    assert!(
        !asked[1].options.contains(&"run".to_owned()),
        "{:?}",
        asked[1].options
    );
}

#[test]
fn run_scope_covers_later_tasks_and_replays_strictly() {
    let r = two_task_repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    let (id, args) = edit("c2", "pub fn ok() {}\npub fn login() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("login"));
    write_manifest(&f, "c3", "{\"name\": \"x\"}\n");
    let (id, args) = edit("c4", "pub fn ok() {}\npub fn logout() {}");
    f.will_call_tool(id, "edit_file", args);
    f.will_structure(done("logout"));
    let person = Scripted::new(vec![Answer::approve(Scope::Run, None, None)]);
    let options = speccoder_runtime::RunOptions {
        approver: Some(person.clone()),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = feature(&res);
    assert_eq!(fr.attempts.len(), 2);
    assert_eq!(fr.attempts[0].tool_calls[0]["scope"], "run");
    assert_eq!(fr.attempts[1].tool_calls[0]["scope"], "run-grant");
    assert_eq!(person.asked.lock().unwrap().len(), 1, "T003 asked nobody");
    // Both attempts replay strictly; the grant is in T003's journaled input.
    for a in &fr.attempts {
        let replayed = speccoder_runtime::replay_with(r.path(), &a.run_id, state.path());
        assert_eq!(
            replayed.status(),
            CommandStatus::Ok,
            "{}",
            replayed.message()
        );
    }
    assert_eq!(person.asked.lock().unwrap().len(), 1);
    // A new run starts without the grant.
    assert!(
        speccoder_runtime::RunOptions::default()
            .run_grants
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_waiting_call_is_listed_as_it_was_asked_and_a_note_reaches_the_attempt() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1", "{}\n");
    let person = Scripted::new(vec![Answer::stop(None)]);
    let _ =
        speccoder_runtime::run_with_approver(r.path(), None, "T005", state.path(), d, None, person);
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let p = &approvals(&listed).pending[0];
    let v = p.presentation.as_ref().unwrap();
    assert_eq!(
        (v.title.as_str(), v.subject.as_str()),
        ("Write this file?", "package.json")
    );
    assert_eq!(v.body_lines, ["    1 │ {}"]);
    assert_eq!(v.scope, "for the rest of T005: this file, any content");
    assert_eq!(v.risk, "medium");
    let json = serde_json::to_value(&listed).unwrap();
    assert_eq!(json["data"]["pending"][0]["presentation"]["kind"], "write");

    let ok = decide_with(
        r.path(),
        None,
        Verb::ApproveNoted("T005", "an empty object is enough"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    // The attempt continues and the model reads the note with the call's result.
    let (f, d) = fake();
    f.will_structure(done("manifest"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(f.asked().len(), 1);
    assert_eq!(
        report(&res).tool_calls[0]["note"],
        "an empty object is enough"
    );
}

// ── repository instructions (AGENTS.md) ────────────────────

const AGENTS_MD: (&str, &str) = ("AGENTS.md", "Errors use thiserror. MARKER-AGENTS-7731\n");

#[test]
fn the_repository_instructions_reach_the_attempt_and_the_test_author() {
    let r = repo_with(
        "[acceptance]\nheld_out = false\n",
        &[
            ACCEPTANCE_SPEC,
            AGENTS_MD,
            ("src/AGENTS.md", "src rules MARKER-SRC-55\n"),
            ("web/AGENTS.md", "MARKER-WEB-99\n"),
        ],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(acceptance("tests/acceptance_login.txt", GREP_LOGIN));
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(done("login"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    let prompts: Vec<String> = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .collect();
    // The author (no tools) and the attempt (tools) both saw the root file.
    assert!(
        prompts[0].contains("MARKER-AGENTS-7731"),
        "author: {}",
        prompts[0]
    );
    assert!(
        prompts[1].contains("MARKER-AGENTS-7731") && prompts[1].contains("Repository instructions")
    );
    // A directory the task names nothing in is not read.
    assert!(!prompts.iter().any(|p| p.contains("MARKER-WEB-99")));
    let names: Vec<&str> = rep.instructions.iter().map(|i| i.path.as_str()).collect();
    assert!(names.first() == Some(&"AGENTS.md"), "{names:?}");
    assert_eq!(rep.instructions[0].sha256.len(), 64);
    // The journal holds them: strict replay needs no file.
    fs::remove_file(r.path().join("AGENTS.md")).unwrap();
    let wt = Path::new(&rep.worktree);
    let _ = fs::remove_file(wt.join("AGENTS.md"));
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert!(
        replayed.message().starts_with("reproduced"),
        "{}",
        replayed.message()
    );
}

#[test]
fn no_instructions_when_turned_off() {
    let r = repo_with(
        "[acceptance]\ngenerate = false\n\n[context]\ninstructions = []\n",
        &[AGENTS_MD],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("nothing to do"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert!(rep.instructions.is_empty());
    assert!(!f.asked().iter().any(|a| {
        serde_json::to_string(&a.prompt)
            .unwrap()
            .contains("MARKER-AGENTS")
    }));
}

/// The inbox: a call waiting for approval is the first decision,
/// with approve and reject; a task whose attempt did not pass is one too.
#[test]
fn the_inbox_lists_a_waiting_call_and_a_task_that_did_not_pass() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    // A waiting call holds the worktree: the attempt that did not pass first.
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(json!({ "summary": "not finished", "status": "incomplete", "files_changed": [], "checks_run": [], "notes": [] }));
    let res = run(r.path(), state.path(), "T009", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["touch", "approved-only"] }),
    );
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());

    let res = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = res.data() else {
        panic!("{}", res.message())
    };
    let kinds: Vec<(&str, Option<&str>)> = i
        .items
        .iter()
        .map(|x| (x.kind.as_str(), x.task.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [("approval", Some("T005")), ("stuck", Some("T009"))],
        "{}",
        res.message()
    );
    let approval = &i.items[0];
    assert!(approval.detail.contains("touch"), "{}", approval.detail);
    assert!(approval.since.is_some());
    // Each decision names the call it shows and that call's digest.
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let Some(CommandData::Approvals(a)) = listed.data() else {
        panic!("{}", listed.message())
    };
    let bound = a
        .pending
        .iter()
        .find(|p| p.task == "T005")
        .map(|p| (p.id(), p.digest.clone()))
        .unwrap();
    assert_eq!(bound.1.len(), 64);
    let runs: Vec<_> = approval.actions.iter().map(|a| a.run.clone()).collect();
    assert_eq!(
        runs,
        [
            Some(speccoder_core::actions::Runnable::Approve {
                task: "T005".to_owned(),
                call: bound.0.clone(),
                digest: bound.1.clone(),
            }),
            Some(speccoder_core::actions::Runnable::Reject {
                task: "T005".to_owned(),
                call: bound.0.clone(),
                digest: bound.1.clone(),
            }),
        ]
    );
    let stuck = &i.items[1];
    assert!(stuck.title.contains("incomplete"), "{}", stuck.title);
    let verbs: Vec<&str> = stuck
        .actions
        .iter()
        .map(|a| a.label.split(' ').next().unwrap())
        .collect();
    assert_eq!(verbs, ["run", "nudge", "skip", "accept"]);
    // T005's attempt holds the worktree: running T009 is not offered as if it could.
    assert!(
        stuck.actions[0]
            .blocked
            .as_deref()
            .is_some_and(|b| b.contains("T005")),
        "{:?}",
        stuck.actions[0].blocked
    );
}

/// `[review] required`: a run waits, before any model call, until
/// every requirement is approved in the committed review.
#[test]
fn a_required_review_stops_the_run_until_the_spec_is_approved() {
    let r = repo_with(
        "[review]\nrequired = true\n",
        &[(
            "specs/001-graph/spec.md",
            "# Spec\n\n### User Story 1 - Login\n\n- **FR-001**: Users MUST log in.\n",
        )],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("REVIEW-PENDING"),
        "{}",
        res.message()
    );
    assert_eq!(f.calls(), 0, "nothing was asked of the model");
    // Approved but not committed: the review is intent, and a worktree
    // holds commits only.
    let ok = speccoder_runtime::review::decide(
        r.path(),
        None,
        &[],
        "approve",
        None,
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    let (_, d) = fake();
    let res = run(r.path(), state.path(), "T005", d);
    assert_eq!(
        res.code().map(|c| c.name),
        Some("UNCOMMITTED-INTENT"),
        "{}",
        res.message()
    );
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "review"]);
    let (f, d) = fake();
    f.will_structure(done("nothing to do"));
    let res = run(r.path(), state.path(), "T005", d);
    assert_ne!(
        res.code().map(|c| c.name),
        Some("REVIEW-PENDING"),
        "{}",
        res.message()
    );
}

/// What the model reasons and says beside its calls is shown live as its
/// own (`Reasoning`, `Said`), and read back from the journal afterwards,
/// turn by turn — a claim, apart from the evidence.
#[test]
fn the_model_s_account_is_live_and_kept() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_answer(agentplane::model::Completion {
        tool_calls: vec![agentplane::model::ToolCall {
            id: "c1".to_owned(),
            name: "edit_file".to_owned(),
            arguments: json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }),
        }],
        text: "I'll add login() next to ok(), the smallest change the test needs.\n\x1b[31mred\x1b[0m".to_owned(),
        model: None,
        usage: agentplane::model::Usage::default(),
        stop_reason: Some("tool_use".to_owned()),
        truncated: false,
        commentary: None,
        compacted: None,
        structured: None,
        continuation: None,
        reasoning: Some("The test only needs login() to exist.\nSo the smallest change is a new empty function.".to_owned()),
    });
    f.will_structure(done("added login"));
    let rec = Arc::new(Recorder::default());
    let options = speccoder_runtime::RunOptions {
        progress: Some(rec.clone()),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let said: Vec<(u32, String)> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Said { turn, text, .. } => Some((*turn, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, 1);
    assert!(said[0].1.starts_with("I'll add login()"), "{}", said[0].1);
    assert!(
        !said[0].1.contains('\x1b'),
        "control characters escaped: {:?}",
        said[0].1
    );
    let thought: Vec<(u32, String)> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Reasoning { turn, text, .. } => Some((*turn, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(thought.len(), 1, "{thought:?}");
    assert!(
        thought[0].1.ends_with("a new empty function."),
        "{}",
        thought[0].1
    );
    // Every attempt turn asked for the summary (on by default).
    assert!(
        f.asked()
            .iter()
            .filter(|a| !a.tools.is_empty())
            .all(|a| a.reasoning_summary),
        "the attempt's turns ask for a reasoning summary"
    );
    // Afterwards, from the journal.
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let run_id = report(&res).run_id.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let kept = rt
        .block_on(speccoder_runtime::substrate::account(&plane, &run_id))
        .unwrap();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].turn, 1);
    assert!(
        kept[0]
            .said
            .as_deref()
            .unwrap()
            .starts_with("I'll add login()")
    );
    assert!(
        kept[0]
            .reasoning
            .as_deref()
            .unwrap()
            .starts_with("The test only needs")
    );
}

/// `[model] reasoning_summary = false`: no turn asks for one.
#[test]
fn reasoning_summaries_can_be_switched_off() {
    let r = repo("");
    let toml = r.path().join("speccoder.toml");
    let text = fs::read_to_string(&toml).unwrap();
    fs::write(
        &toml,
        text.replacen(MODEL, &format!("{MODEL}reasoning_summary = false\n"), 1),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "no reasoning"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("nothing to do"));
    let res = run(r.path(), state.path(), "T005", d);
    assert!(!f.asked().is_empty(), "{}", res.message());
    assert!(f.asked().iter().all(|a| !a.reasoning_summary));
}

#[test]
fn uncommitted_setup_config_is_part_of_the_fix() {
    let r = repo("");
    fs::create_dir_all(r.path().join(".config")).unwrap();
    fs::write(
        r.path().join(".config/nextest.toml"),
        "[profile.speccoder]\n",
    )
    .unwrap();
    let remedy = speccoder_runtime::code::setup_remedy(r.path(), None).expect("a remedy");
    assert_eq!(remedy.label, "Commit .config/nextest.toml");
    assert!(
        remedy.command.contains("git add .config/nextest.toml"),
        "{}",
        remedy.command
    );
    let fixed = speccoder_runtime::code::fix_setup(r.path(), None);
    assert_eq!(fixed.status(), CommandStatus::Ok, "{}", fixed.message());
    assert!(git_out(r.path(), &["ls-files", ".config/nextest.toml"]).contains("nextest.toml"));
    assert!(speccoder_runtime::code::setup_remedy(r.path(), None).is_none());
}

/// An OpenAI Responses model's commentary — its words beside its calls,
/// which that wire marks apart from the answer — is shown live and kept,
/// before any text the turn has; a final turn's commentary too.
#[test]
fn the_model_s_commentary_is_what_it_said() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let turn = |text: &str,
                commentary: &str,
                calls: Vec<agentplane::model::ToolCall>,
                structured: Option<Value>| agentplane::model::Completion {
        tool_calls: calls,
        text: text.to_owned(),
        model: None,
        usage: agentplane::model::Usage::default(),
        stop_reason: None,
        truncated: false,
        commentary: Some(commentary.to_owned()),
        compacted: None,
        structured,
        continuation: None,
        reasoning: None,
    };
    f.will_answer(turn(
        "",
        "Reading the library first.",
        vec![agentplane::model::ToolCall {
            id: "c1".to_owned(),
            name: "read_file".to_owned(),
            arguments: json!({ "path": "src/lib.rs" }),
        }],
        None,
    ));
    f.will_answer(turn(
        "then the edit.",
        "Adding login().",
        vec![agentplane::model::ToolCall {
            id: "c2".to_owned(),
            name: "edit_file".to_owned(),
            arguments: json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }),
        }],
        None,
    ));
    let answer = done("added login");
    f.will_answer(turn(&answer.to_string(), "Done.", Vec::new(), Some(answer)));
    let rec = Arc::new(Recorder::default());
    let options = speccoder_runtime::RunOptions {
        progress: Some(rec.clone()),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let said: Vec<(u32, String)> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Said { turn, text, .. } => Some((*turn, text.clone())),
            _ => None,
        })
        .collect();
    let expected = vec![
        (1, "Reading the library first.".to_owned()),
        (2, "Adding login().\n\nthen the edit.".to_owned()),
        (3, "Done.".to_owned()),
    ];
    assert_eq!(said, expected);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let kept = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(speccoder_runtime::substrate::account(
            &plane,
            &report(&res).run_id,
        ))
        .unwrap();
    let kept: Vec<(u32, String)> = kept
        .into_iter()
        .map(|t| (t.turn, t.said.unwrap_or_default()))
        .collect();
    assert_eq!(kept, expected);
}

// ── tests where the runner finds them (spec 102, US1) ────────────────

/// A Cargo manifest for the fixture, so its `.rs` test files have Cargo's rule.
const CARGO: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
);

/// A listing that names the integration tests Cargo builds (`tests/*.rs`).
const LISTS_TESTS_RS: &str =
    "[checks.listed]\nargv = [\"true\"]\nlist = [\"sh\", \"-c\", \"ls tests/*.rs\"]\n";

fn writes_test(f: &FakeProvider, id: &str, path: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": path, "content": "#[test]\nfn greets() {}\n" }),
    );
    f.will_structure(done("added the test"));
}

#[test]
fn a_test_file_cargo_never_builds_fails_and_the_retry_learns_where_it_runs() {
    let r = repo_with(
        &format!("{LISTS_TESTS_RS}[budget]\nmax_attempts = 2\n"),
        &[CARGO],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    writes_test(&f, "c1", "tests/unit/greet_name.rs");
    writes_test(&f, "c2", "tests/greet_name.rs");
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        rep.verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        res.message()
    );
    let first = &rep.earlier[0];
    assert_eq!(
        first.verdict,
        Some(Verdict::Failed {
            reasons: vec!["tests not collected: `tests/unit/greet_name.rs` — Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare); this file is neither".to_owned()]
        })
    );
    let line = "`tests/unit/greet_name.rs` never ran: Cargo builds `tests/*.rs` and `tests/<dir>/main.rs`. Move the tests to `tests/greet_name.rs`, or add `tests/unit/main.rs` with `mod greet_name;`.";
    assert_eq!(first.digest.uncollected, [line]);
    assert!(
        first
            .fingerprint
            .contains("uncollected: tests/unit/greet_name.rs")
    );
    let second = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .find(|p| p.contains("Tests that never ran"))
        .expect("the retry is told");
    assert!(second.contains("never ran: Cargo builds"), "{second}");
    // The retry's file is collected: the check's listing names it.
    let c = rep.collection.as_ref().unwrap();
    assert_eq!(c.files[0].path, "tests/greet_name.rs");
    assert_eq!(
        c.files[0].state,
        speccoder_core::collection::FileState::Collected
    );
}

#[test]
fn the_same_uncollected_file_twice_stops_retrying() {
    let r = repo_with(
        &format!("{LISTS_TESTS_RS}[budget]\nmax_attempts = 3\n"),
        &[CARGO],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    writes_test(&f, "c1", "tests/unit/greet_name.rs");
    writes_test(&f, "c2", "tests/unit/greet_name.rs");
    f.will_structure(done("third"));
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    assert_eq!(res.code().map(|c| c.name), Some("TESTS-NOT-COLLECTED"));
    assert!(
        rep.retry_stop
            .as_deref()
            .unwrap_or_default()
            .starts_with("same failure twice"),
        "{:?}",
        rep.retry_stop
    );
    assert!(!rep.ticked);
}

#[test]
fn a_test_file_speccoder_cannot_tell_about_stops_for_a_person() {
    // A listing that times out: neither collected nor not — never ticked.
    let r = repo_with(
        "[checks.listed]\nargv = [\"true\"]\nlist = [\"sleep\", \"5\"]\ntimeout_secs = 1\n[budget]\nmax_attempts = 2\n",
        &[CARGO],
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    writes_test(&f, "c1", "tests/greet_name.rs");
    let res = run(r.path(), state.path(), "T005", d);
    let rep = report(&res);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("TESTS-COLLECTION-UNKNOWN"));
    assert!(
        res.message()
            .starts_with("speccoder cannot tell whether the runner ran `tests/greet_name.rs` ("),
        "{}",
        res.message()
    );
    assert!(!rep.ticked && rep.saved_attempt.is_some());
    assert!(rep.earlier.is_empty(), "never retried");
}
