//! The oracle at review end to end with a scripted model and
//! real red runs: tests written at review, shaped by the person, compared
//! for ambiguity, frozen with the approval and reused by the run without
//! an author call — and no held-out path or literal in any payload.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::actions::Via;
use speccoder_core::oracle::{OracleOp, OracleState, OracleView, Reading, RunMode};
use speccoder_core::{CommandData, CommandResult, CommandStatus, ReviewReport};
use speccoder_runtime::RunOptions;
use speccoder_runtime::oracle::{Request, tests_with};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n";

/// One test: its file must exist, then "the pattern is in src/lib.rs",
/// with libtest's summary line.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\n[ -f \"$1\" ] || { echo \"error: no test file $1\" >&2; exit 2; }\nshift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nUsers log in.\n\n**Acceptance Scenarios**:\n\n1. **Given** a user, **When** they log in, **Then** a `login` function exists.\n2. **Given** an empty password, **When** they log in, **Then** they are refused.\n3. **Given** a locked account, **When** they log in, **Then** they are told so.\n\n### User Story 2 - Profile (Priority: P2)\n\nProfiles, without scenarios.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST be refused with an empty password.\n";

/// Only the held-out test holds it.
const SECRET: &str = "Zq9-held-out-only-literal";
const HELD_OUT_PATH: &str = "tests/held_out_us1_login.txt";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo_with(spec: &str, extra: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
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
    fs::write(r.join("specs/001-graph/spec.md"), spec).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn repo() -> tempfile::TempDir {
    repo_with(SPEC, "")
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn file(path: &str, scenarios: &[&str], pattern: &str) -> Value {
    json!({
        "path": path,
        "content": format!("checks {pattern}\n"),
        "scenarios": scenarios,
        "argv": ["./accept.sh", "--", path, pattern],
    })
}

/// A review answer: its files and what it says it expects of US1-2.
fn answer(files: Vec<Value>, us1_2: &str) -> Value {
    json!({
        "files": files,
        "argv": ["./accept.sh", "--", "tests/acceptance_us1.txt", "fn login"],
        "notes": [],
        "expectations": [
            { "scenario": "US1-1", "given": "a user", "expect": "a login function" },
            { "scenario": "US1-2", "given": "an empty password", "expect": us1_2 },
        ],
        "untestable": [ { "scenario": "US1-3", "why": "needs a lock service" } ],
    })
}

fn held_out(pattern: &str) -> Value {
    json!({ "files": [{
        "path": HELD_OUT_PATH,
        "content": format!("login with a token \"{SECRET}\" ({pattern})\n"),
        "scenarios": ["US1-1"],
        "argv": ["./accept.sh", "--", HELD_OUT_PATH, pattern],
        "composition": false,
    }], "notes": [] })
}

fn standard() -> Vec<Value> {
    vec![
        file("tests/acceptance_us1_login.txt", &["US1-1"], "fn login"),
        file("tests/acceptance_us1_nothing.txt", &[], "fn login"),
        file("tests/acceptance_us1_passes.txt", &["US1-2"], "fn ok"),
    ]
}

fn request<'a>(req: &'a str, op: OracleOp) -> Request<'a> {
    Request {
        req,
        op,
        path: None,
        scenario: None,
        reading: None,
        text: None,
    }
}

fn code(r: &CommandResult) -> Option<&'static str> {
    r.code().map(|e| e.name)
}

fn review(r: &CommandResult) -> &ReviewReport {
    match r.data() {
        Some(CommandData::Review(rv)) => rv,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn us1(r: &CommandResult) -> OracleView {
    review(r)
        .oracle
        .iter()
        .find(|v| v.req == "US1")
        .cloned()
        .unwrap_or_else(|| panic!("no US1: {}", r.message()))
}

fn write(r: &Path, state: &Path, d: Arc<dyn ModelProvider>) -> CommandResult {
    tests_with(
        r,
        None,
        &request("US1", OracleOp::Write),
        Via::Terminal,
        state,
        Some(d),
        &RunOptions::default(),
    )
}

/// No held-out path or literal anywhere in `text`.
fn assert_no_held_out(text: &str) {
    assert!(!text.contains(SECRET), "held-out literal in {text}");
    assert!(!text.contains(HELD_OUT_PATH), "held-out path in {text}");
}

#[test]
fn us1_written_tests_show_red_discarded_uncovered_and_only_a_held_out_count() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(held_out("fn login"));
    let res = write(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let v = us1(&res);
    assert_eq!(v.state, OracleState::Written, "{v:?}");
    let classes: Vec<(&str, &str, bool)> = v
        .tests
        .iter()
        .map(|t| (t.path.as_str(), t.red_class.as_str(), t.red))
        .collect();
    assert_eq!(
        classes,
        [
            ("tests/acceptance_us1_login.txt", "assertion", true),
            ("tests/acceptance_us1_passes.txt", "passed", false),
        ]
    );
    assert_eq!(v.discarded.len(), 1);
    assert_eq!(v.discarded[0].why, "cites_no_scenario");
    // The passing test covers nothing: US1-2 and US1-3 are uncovered.
    assert_eq!(v.uncovered, ["US1-2", "US1-3"]);
    assert_eq!(v.held_out.state, "kept");
    assert_eq!(v.held_out.count, 1);
    assert!(v.held_out.manifest_sha256.is_some());
    assert_eq!(v.strength_target, None);
    assert_eq!(v.calls, 2);
    assert!(v.ambiguities.is_empty(), "the authors agree");
    // US2 has no scenarios: no Tests card.
    assert!(review(&res).oracle.iter().all(|o| o.req != "US2"));
    // Nothing of the held-out set in the payload, nor in the person's checkout.
    assert_no_held_out(&serde_json::to_string(&res).unwrap());
    assert!(!r.path().join(HELD_OUT_PATH).exists());
    assert!(!r.path().join("tests/acceptance_us1_login.txt").exists());
    // The scratch worktree is gone.
    assert_eq!(git(r.path(), &["worktree", "list"]).lines().count(), 1);
    // The second author's tests are not kept; both calls were made.
    let authors = f.asked().iter().filter(|a| a.tools.is_empty()).count();
    assert_eq!(authors, 3, "two samples and the held-out author");
}

#[test]
fn us1_reject_edit_and_again_shape_the_set() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(held_out("fn login"));
    assert_eq!(
        write(r.path(), state.path(), d.clone()).status(),
        CommandStatus::Ok
    );

    // A rejection needs a reason, and is told to later author calls.
    let mut req = request("US1", OracleOp::Reject);
    req.path = Some("tests/acceptance_us1_passes.txt");
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(code(&res), Some("USAGE"), "{}", res.message());
    req.text = Some("asserts the wrong rounding");
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let v = us1(&res);
    assert_eq!(v.tests.len(), 1);
    assert_eq!(v.rejected[0].reason, "asserts the wrong rounding");
    assert!(!v.rejected[0].by.is_empty());

    // An edit is red-checked and marked as the person's.
    let mut req = request("US1", OracleOp::Edit);
    req.path = Some("tests/acceptance_us1_login.txt");
    req.text = Some("checks fn login, edited\n");
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let t = &us1(&res).tests[0];
    assert_eq!(t.content, "checks fn login, edited\n");
    assert!(t.red);
    assert!(t.edited_by.is_some());
    assert_eq!(
        serde_json::to_value(t.authored_by).unwrap(),
        json!("person")
    );

    // Ask Again: one author call, told the rejection and the comment.
    f.will_structure(answer(
        vec![file(
            "tests/acceptance_us1_login.txt",
            &["US1-1"],
            "fn login(",
        )],
        "refused",
    ));
    let before = f.asked().len();
    let mut req = request("US1", OracleOp::Again);
    req.path = Some("tests/acceptance_us1_login.txt");
    req.text = Some("check the parentheses too");
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        Some(d),
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(f.asked().len(), before + 1, "exactly one author call");
    let prompt = serde_json::to_string(&f.asked().last().unwrap().prompt).unwrap();
    assert!(
        prompt.contains("asserts the wrong rounding"),
        "the rejection is told"
    );
    assert!(
        prompt.contains("check the parentheses too"),
        "the comment is told"
    );
    let v = us1(&res);
    assert_eq!(v.tests.len(), 1);
    assert_eq!(v.tests[0].content, "checks fn login(\n");
    // What reached the calls is recorded, each person's note once, by digest.
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let (draft, _) = speccoder_runtime::oracle::load(&plane, "001-graph", "US1")
        .unwrap()
        .unwrap();
    let rejected = "Rejected `tests/acceptance_us1_passes.txt`: asserts the wrong rounding";
    let want = speccoder_runtime::integrity::sha256_hex(rejected.as_bytes());
    let kinds = |k: speccoder_core::context::ContextKind| {
        draft.context.iter().filter(|e| e.kind == k).count()
    };
    assert_eq!(
        draft.context.iter().filter(|e| e.sha256 == want).count(),
        1,
        "{:?}",
        draft.context
    );
    assert_eq!(kinds(speccoder_core::context::ContextKind::Comment), 1);
    assert!(kinds(speccoder_core::context::ContextKind::Scenario) >= 3);
    assert!(v.tests[0].red);
}

#[test]
fn us2_disagreeing_authors_block_approve_and_run_until_a_reading_is_chosen() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "an error message"));
    f.will_structure(held_out("fn login"));
    let res = write(r.path(), state.path(), d.clone());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let v = us1(&res);
    assert_eq!(v.state, OracleState::Ambiguous);
    assert_eq!(v.ambiguities.len(), 1, "{:?}", v.ambiguities);
    let a = &v.ambiguities[0];
    assert_eq!(a.scenario, "US1-2");
    assert_eq!(
        (a.a.expect.as_str(), a.b.expect.as_str()),
        ("refused", "an error message")
    );
    assert_eq!(review(&res).totals.ambiguous, 1);
    let sheet = review(&res).sheet.clone().unwrap();
    assert!(
        sheet
            .blocked
            .iter()
            .any(|b| b.contains("US1") && b.contains("US1-2")),
        "{sheet:?}"
    );

    // Approve and Run waits; a plain approval records and says so.
    let refused = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        None,
        Via::Terminal,
        Some(state.path()),
        Some(d.clone()),
        &RunOptions::default(),
    );
    assert_eq!(
        code(&refused),
        Some("REVIEW-AMBIGUOUS"),
        "{}",
        refused.message()
    );
    // A gated run waits too.
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[review]\nrequired = true\n"),
    )
    .unwrap();
    // Per-line decisions still record, and say Approve and Run waits.
    let plain = speccoder_runtime::review::decide_in(
        r.path(),
        None,
        &[],
        "approve",
        None,
        Via::Terminal,
        Some(state.path()),
    );
    assert_eq!(plain.status(), CommandStatus::Ok, "{}", plain.message());
    assert!(
        plain
            .message()
            .contains("US1 is ambiguous — Approve and Run stays blocked"),
        "{}",
        plain.message()
    );
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "gate the run"]);
    let gated = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d.clone(), None);
    assert_eq!(
        code(&gated),
        Some("REVIEW-AMBIGUOUS"),
        "{}",
        gated.message()
    );
    // Reading A is meant: the item closes, the tests stay.
    let mut req = request("US1", OracleOp::Reading);
    req.scenario = Some("US1-2");
    req.reading = Some(Reading::A);
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::Written, "{v:?}");
    assert!(v.ambiguities.iter().all(|a| a.resolution.is_some()));
    assert_eq!(v.tests.len(), 2, "reading A keeps the tests");
}

#[test]
fn us2_reading_b_writes_the_scenario_s_tests_again() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "an error message"));
    f.will_structure(held_out("fn login"));
    assert_eq!(
        write(r.path(), state.path(), d.clone()).status(),
        CommandStatus::Ok
    );
    f.will_structure(answer(
        vec![file(
            "tests/acceptance_us1_error.txt",
            &["US1-2"],
            "fn refuse",
        )],
        "an error message",
    ));
    let mut req = request("US1", OracleOp::Reading);
    req.scenario = Some("US1-2");
    req.reading = Some(Reading::B);
    let res = tests_with(
        r.path(),
        None,
        &req,
        Via::Web,
        state.path(),
        Some(d),
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let prompt = serde_json::to_string(&f.asked().last().unwrap().prompt).unwrap();
    assert!(
        prompt.contains("reading B"),
        "reading B is the person's note"
    );
    let v = us1(&res);
    let paths: Vec<&str> = v.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "tests/acceptance_us1_login.txt",
            "tests/acceptance_us1_error.txt"
        ]
    );
    assert!(v.tests[1].red);
}

#[test]
fn edge_cases_refuse_with_their_codes() {
    // Acceptance off.
    let r = repo_with(SPEC, "\n[acceptance]\ngenerate = false\n");
    let state = tempfile::tempdir().unwrap();
    let (_, d) = fake();
    let res = write(r.path(), state.path(), d.clone());
    assert_eq!(code(&res), Some("ACCEPTANCE-OFF"), "{}", res.message());
    // No scenarios.
    let r = repo();
    let res = tests_with(
        r.path(),
        None,
        &request("US2", OracleOp::Write),
        Via::Terminal,
        state.path(),
        Some(d.clone()),
        &RunOptions::default(),
    );
    assert_eq!(code(&res), Some("REVIEW-NO-SCENARIOS"), "{}", res.message());
    // The author fails: nothing is stored.
    let (f, d) = fake();
    f.will_fail(agentplane::model::ModelError::Refused {
        model: agentplane::model::ModelId::new("openai", "gpt-5.4-mini"),
        detail: "down".to_owned(),
        code: None,
    });
    let res = write(r.path(), state.path(), d);
    assert_eq!(
        code(&res),
        Some("ORACLE-AUTHOR-FAILED"),
        "{}",
        res.message()
    );
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::None);
    // Held out off: said on the card.
    let r = repo_with(
        SPEC,
        "\n[acceptance]\nheld_out = false\nmin_strength = 0.8\n",
    );
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    let v = us1(&write(r.path(), state.path(), d));
    assert_eq!(v.held_out.state, "off");
    assert_eq!(v.strength_target, Some(80));
}

#[test]
fn a_quick_change_or_bug_unit_writes_no_tests_at_review() {
    let r = repo();
    let dir = r.path().join("specs/002-bugfixes");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("spec.md"), SPEC).unwrap();
    fs::write(dir.join("tasks.md"), "# Tasks\n\n- [ ] T001 [US1] fix\n").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "bugs"]);
    let state = tempfile::tempdir().unwrap();
    let (_, d) = fake();
    let res = tests_with(
        r.path(),
        Some("002-bugfixes"),
        &request("US1", OracleOp::Write),
        Via::Terminal,
        state.path(),
        Some(d),
        &RunOptions::default(),
    );
    assert_eq!(
        code(&res),
        Some("REVIEW-TESTS-OWN-PIPELINE"),
        "{}",
        res.message()
    );
    let rv =
        speccoder_runtime::review::report_in(r.path(), Some("002-bugfixes"), Some(state.path()));
    let rv = review(&rv);
    assert!(rv.oracle.is_empty());
    assert!(
        rv.tests_notice
            .as_deref()
            .is_some_and(|n| n.contains("own pipeline"))
    );
}

fn write_lib(f: &FakeProvider, id: &str, lib: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": "src/lib.rs", "content": lib }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
}

/// Writes US1's tests (one red, agreeing authors), ready to approve.
fn written(r: &Path, state: &Path) -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let (f, d) = fake();
    f.will_structure(answer(
        vec![file(
            "tests/acceptance_us1_login.txt",
            &["US1-1"],
            "fn login",
        )],
        "refused",
    ));
    f.will_structure(answer(
        vec![file(
            "tests/acceptance_us1_login.txt",
            &["US1-1"],
            "fn login",
        )],
        "refused",
    ));
    f.will_structure(held_out("fn login"));
    let res = write(r, state, d.clone());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    (f, d)
}

/// The approve sheet's digest, as the web app sends it back.
fn sheet_of(r: &Path, state: &Path) -> Option<String> {
    review(&speccoder_runtime::review::report_in(r, None, Some(state)))
        .sheet
        .as_ref()
        .and_then(|s| s.sha256.clone())
}

fn one_task() -> RunOptions {
    RunOptions {
        max_tasks: Some(1),
        ..RunOptions::default()
    }
}

#[test]
fn us3_us4_approve_freezes_and_the_run_reuses_the_oracle_without_an_author_call() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    let authors = |f: &FakeProvider| {
        f.asked()
            .iter()
            .filter(|a| {
                let p = serde_json::to_string(&a.prompt).unwrap();
                p.contains("You are the test author") || p.contains("HELD-OUT")
            })
            .count()
    };
    let authors_before = authors(&f);
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        sheet_of(r.path(), state.path()).as_deref(),
        Via::Web,
        Some(state.path()),
        Some(d),
        &one_task(),
    );
    assert!(
        res.message()
            .starts_with("approved 2 decision line(s), froze tests for 1 requirement(s)"),
        "{}",
        res.message()
    );
    // review.json: the decisions, the oracle entry and the approval digest.
    let record: Value = serde_json::from_str(
        &fs::read_to_string(r.path().join("specs/001-graph/review.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["requirements"]["FR-001"]["verdict"], "approved");
    assert!(
        record["oracle"]["US1"]["manifest_sha256"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(record["oracle"]["US1"]["via"], "web");
    let approval = record["approval"]["sha256"].as_str().unwrap().to_owned();
    assert_eq!(record["approval"]["mode"], "ask");
    // Committed with the approval, so the run reads it.
    assert!(git(r.path(), &["status", "--porcelain", "specs"]).is_empty());
    // The run: zero acceptance or held-out author calls.
    let authors_after = authors(&f);
    assert_eq!(
        authors_after, authors_before,
        "no author call at the checkpoint"
    );
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    let rep = &fr.attempts[0];
    assert_eq!(
        rep.verdict,
        Some(speccoder_core::verify::Verdict::Verified),
        "{}",
        res.message()
    );
    let acc = rep.acceptance.as_ref().unwrap();
    assert_eq!(acc["source"], "review");
    assert_eq!(acc["pre_frozen"]["approval"], approval);
    assert_eq!(acc["held_out"]["state"], "on", "{acc}");
    assert!(
        rep.notices
            .iter()
            .any(|n| n.contains("frozen at review by")),
        "{:?}",
        rep.notices
    );
    // The tick names the frozen manifest, in the one pre-frozen form.
    let body = git(Path::new(&rep.worktree), &["log", "-1", "--format=%B"]);
    let trailer = body
        .lines()
        .find(|l| l.starts_with("Acceptance: review "))
        .unwrap_or_else(|| panic!("{body}"));
    assert!(
        trailer.contains(&format!("approval={approval}")),
        "{trailer}"
    );
    assert!(trailer.contains("manifest=sha256:") && trailer.contains("wording=sha256:"));
    assert_no_held_out(&serde_json::to_string(&res).unwrap());
    // The review shows it frozen.
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::Frozen);
}

#[test]
fn us4_a_tampered_frozen_test_stops_the_task_and_a_lapse_or_missing_set_authors() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    // Approve without running: a run that cannot start (no task ready)
    // still froze. Use one task and tamper before the run.
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let frozen = speccoder_runtime::pre_frozen::dir(&plane, "001-graph", "US1");
    // Freeze by approving; the run's attempt is scripted to stop early.
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        None,
        Via::Terminal,
        Some(state.path()),
        Some(d.clone()),
        &RunOptions {
            max_tasks: Some(0),
            ..RunOptions::default()
        },
    );
    assert!(
        res.message().contains("froze tests for 1"),
        "{}",
        res.message()
    );
    assert!(frozen.join("record.json").exists());
    // Tampered: the task stops, nothing is ticked.
    fs::write(
        frozen.join("files/tests/acceptance_us1_login.txt"),
        "changed\n",
    )
    .unwrap();
    let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d.clone(), None);
    assert_eq!(code(&run), Some("ORACLE-TAMPERED"), "{}", run.message());
    // Missing here (a teammate's checkout): the run authors as today.
    fs::remove_dir_all(&frozen).unwrap();
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::FrozenElsewhere);
    // Lapsed: a reworded scenario lapses the oracle.
    fs::write(
        r.path().join("specs/001-graph/spec.md"),
        SPEC.replace("they are told so", "they are told to wait"),
    )
    .unwrap();
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::Lapsed, "{v:?}");
    assert!(
        v.frozen_on
            .as_deref()
            .is_some_and(|w| w.contains("they are told so")),
        "{v:?}"
    );
}

#[test]
fn us3_autopilot_refuses_what_would_ask_and_lists_it() {
    if speccoder_runtime::sandbox::backend().is_err() {
        return; // autopilot needs the OS sandbox
    }
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    // An unlisted command would ask: on autopilot it is refused, then the
    // attempt goes on.
    f.will_call_tool(
        "c0",
        "run_command",
        json!({ "argv": ["cargo", "add", "serde"] }),
    );
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    // An unknown grant id is refused, listing the offered ones.
    let bad = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Autopilot,
        &["g-nope".to_owned()],
        sheet_of(r.path(), state.path()).as_deref(),
        Via::Web,
        Some(state.path()),
        Some(d.clone()),
        &one_task(),
    );
    assert_eq!(code(&bad), Some("USAGE"), "{}", bad.message());
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Autopilot,
        &[],
        sheet_of(r.path(), state.path()).as_deref(),
        Via::Web,
        Some(state.path()),
        Some(d),
        &one_task(),
    );
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    assert_eq!(fr.mode, RunMode::Autopilot);
    assert_eq!(fr.autopilot_denied.len(), 1, "{}", res.message());
    assert_eq!(
        fr.autopilot_denied[0].reason,
        "autopilot: not asked — listed for you"
    );
    assert!(fr.autopilot_denied[0].object.contains("cargo add serde"));
    // Listed for the person in Needs You, as `Ready to review`.
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    let item = i
        .items
        .iter()
        .find(|i| i.kind == "autopilot_denied")
        .unwrap();
    assert_eq!(item.group, "ready_to_review");
}

#[test]
fn autopilot_without_a_sandbox_is_refused() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (_, d) = written(r.path(), state.path());
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[sandbox]\nmode = \"off\"\n"),
    )
    .unwrap();
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Autopilot,
        &[],
        sheet_of(r.path(), state.path()).as_deref(),
        Via::Web,
        Some(state.path()),
        Some(d),
        &one_task(),
    );
    assert_eq!(code(&res), Some("SANDBOX-UNAVAILABLE"), "{}", res.message());
    let rv = speccoder_runtime::review::report_in(r.path(), None, Some(state.path()));
    let sheet = review(&rv).sheet.clone().unwrap();
    assert!(
        sheet
            .autopilot_blocked
            .unwrap()
            .contains("autopilot needs the OS sandbox")
    );
    // Nothing recorded by the refusal.
    assert!(!r.path().join("specs/001-graph/review.json").exists());
}

#[test]
fn the_ambiguous_item_waits_in_needs_you() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "an error message"));
    f.will_structure(held_out("fn login"));
    assert_eq!(write(r.path(), state.path(), d).status(), CommandStatus::Ok);
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    let item = i.items.iter().find(|i| i.kind == "ambiguous").unwrap();
    assert_eq!(item.group, "blocked_on_you");
    assert!(item.detail.contains("US1/US1-2"), "{}", item.detail);
    assert_no_held_out(&serde_json::to_string(&inbox).unwrap());
}

#[test]
fn us5_a_test_comment_asks_again_and_no_model_input_but_the_held_out_author_s_holds_held_out_material()
 {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    let add = speccoder_runtime::comments::comment(
        r.path(),
        None,
        speccoder_core::actions::CommentOp::Add,
        Some(&speccoder_core::oracle::CommentAnchor::TestLine {
            req: "US1".to_owned(),
            path: "tests/acceptance_us1_login.txt".to_owned(),
            line: 1,
            quote: "checks fn login".to_owned(),
        }),
        None,
        Some("check the signature"),
        Via::Web,
        Some(state.path()),
    );
    assert_eq!(add.status(), CommandStatus::Ok, "{}", add.message());
    f.will_structure(answer(
        vec![file(
            "tests/acceptance_us1_login.txt",
            &["US1-1"],
            "pub fn login",
        )],
        "refused",
    ));
    let sent = speccoder_runtime::comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        Some(d),
        None,
        &RunOptions::default(),
    );
    assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
    let Some(CommandData::Comments(c)) = sent.data() else {
        panic!("{}", sent.message())
    };
    assert_eq!(c.turn.as_ref().unwrap().asked_again, ["US1"]);
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.tests[0].content, "checks pub fn login\n");
    // FR-024: no input but the held-out author's own carries its material.
    for a in f.asked() {
        let p = serde_json::to_string(&a.prompt).unwrap();
        if p.contains("HELD-OUT") {
            continue;
        }
        assert_no_held_out(&p);
    }
    // A code nudge on a frozen test is a review comment instead.
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let nudge = speccoder_runtime::comments::code_nudge(
        r.path(),
        None,
        "T005",
        &[speccoder_core::oracle::CodeComment {
            path: "src/lib.rs".to_owned(),
            line: 1,
            quote: "pub fn ok() {}".to_owned(),
            text: "name it".to_owned(),
        }],
        Some(state.path()),
    );
    assert_eq!(nudge.status(), CommandStatus::Ok, "{}", nudge.message());
    let d = speccoder_runtime::approvals::Decisions::load(&plane).unwrap();
    for n in d.for_task("001-graph", "T005").nudges {
        assert_no_held_out(&n);
    }
}

#[test]
fn the_review_red_check_refuses_to_run_without_the_os_sandbox() {
    // C2: model-written tests and commands run before anyone approved them.
    let r = repo_with(SPEC, "\n[sandbox]\nmode = \"off\"\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    let res = write(r.path(), state.path(), d);
    assert_eq!(code(&res), Some("SANDBOX-UNAVAILABLE"), "{}", res.message());
    assert!(
        res.message().contains("mode = \"off\""),
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty(), "no author call either");
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::None);
}

#[test]
fn us1_red_is_each_test_s_own_never_a_neighbour_s() {
    // C1: a passing test sharing a failing test's command is not red, and a
    // command red without its test decides nothing about it.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let shared = |path: &str, scenarios: &[&str]| {
        json!({
            "path": path,
            "content": "checks\n",
            "scenarios": scenarios,
            "argv": ["./accept.sh", "--", "tests/acceptance_us1_login.txt", "fn login"],
        })
    };
    let files = vec![
        file("tests/acceptance_us1_login.txt", &["US1-1"], "fn login"),
        // Same command as the red one: it would be "red" by the other's failure.
        shared("tests/acceptance_us1_true.txt", &["US1-2"]),
        // Red whether it exists or not: its command runs something else.
        json!({
            "path": "tests/acceptance_us1_bare.txt",
            "content": "checks\n",
            "scenarios": ["US1-3"],
            "argv": ["./accept.sh", "--", "src/lib.rs", "fn locked"],
        }),
    ];
    f.will_structure(answer(files.clone(), "refused"));
    f.will_structure(answer(files, "refused"));
    f.will_structure(held_out("fn login"));
    let res = write(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let v = us1(&res);
    let kept: Vec<&str> = v.tests.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(kept, ["tests/acceptance_us1_login.txt"], "{v:?}");
    let why: Vec<(&str, &str)> = v
        .discarded
        .iter()
        .map(|d| (d.path.as_str(), d.why.as_str()))
        .collect();
    assert!(
        why.contains(&("tests/acceptance_us1_true.txt", "command_shared")),
        "{why:?}"
    );
    assert!(
        why.contains(&("tests/acceptance_us1_bare.txt", "red_without_it")),
        "{why:?}"
    );
    assert_eq!(v.uncovered, ["US1-2", "US1-3"]);
}

#[test]
fn autopilot_never_adopts_the_grants_of_an_interrupted_earlier_run() {
    // C3: an Ask run waits on `cargo add serde` and the person approves it;
    // the attempt is then resumed on autopilot with nothing ticked.
    if speccoder_runtime::sandbox::backend().is_err() {
        return; // autopilot needs the OS sandbox
    }
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    let frozen = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        None,
        Via::Terminal,
        Some(state.path()),
        Some(d.clone()),
        &RunOptions {
            max_tasks: Some(0),
            ..RunOptions::default()
        },
    );
    assert!(
        frozen.message().contains("froze tests for 1"),
        "{}",
        frozen.message()
    );
    f.will_call_tool(
        "c0",
        "run_command",
        json!({ "argv": ["cargo", "add", "serde"] }),
    );
    let asked = speccoder_runtime::pipeline::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &one_task(),
    );
    assert_eq!(
        asked.status(),
        CommandStatus::NeedsHuman,
        "{}",
        asked.message()
    );
    let ok = speccoder_runtime::approvals::decide_with(
        r.path(),
        None,
        speccoder_runtime::approvals::Verb::Approve("T005"),
        state.path(),
        Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    // On autopilot the attempt asks for the same command again.
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["cargo", "add", "serde"] }),
    );
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let res = speccoder_runtime::pipeline::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions {
            mode: RunMode::Autopilot,
            ..one_task()
        },
    );
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    let calls: Vec<&Value> = fr
        .attempts
        .iter()
        .flat_map(|a| a.tool_calls.iter())
        .collect();
    assert!(
        calls
            .iter()
            .filter(|c| c["arguments"]["argv"][1] == "add")
            .all(|c| c["decision"] == "deny"),
        "the earlier run's approval was used on autopilot: {} {calls:#?}",
        res.message(),
    );
    assert!(
        fr.autopilot_denied
            .iter()
            .any(|x| x.object.contains("cargo add serde")),
        "{} {:?}",
        res.message(),
        fr.autopilot_denied
    );
}

fn freeze_only(r: &Path, state: &Path, d: Arc<dyn ModelProvider>) {
    let res = speccoder_runtime::review::approve_and_run(
        r,
        None,
        RunMode::Ask,
        &[],
        None,
        Via::Terminal,
        Some(state),
        Some(d),
        &RunOptions {
            max_tasks: Some(0),
            ..RunOptions::default()
        },
    );
    assert!(
        res.message().contains("froze tests for 1"),
        "{}",
        res.message()
    );
}

/// Every file under `dir`, with its bytes.
fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push((e.path(), fs::read(e.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_frozen_requirement_s_tests_are_not_rewritten_under_its_approval() {
    // C4: Write Tests again after the freeze would replace the sealed
    // held-out set the frozen record names.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    freeze_only(r.path(), state.path(), d.clone());
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let sealed = speccoder_runtime::held_out::sealed_dir(&plane, "001-graph", "US1");
    let before = tree(&sealed);
    assert!(!before.is_empty());
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(held_out("fn login("));
    let asked = f.asked().len();
    let again = write(r.path(), state.path(), d);
    assert_eq!(code(&again), Some("ORACLE-FROZEN"), "{}", again.message());
    assert_eq!(f.asked().len(), asked, "no author call");
    let mut reject = request("US1", OracleOp::Reject);
    reject.path = Some("tests/acceptance_us1_login.txt");
    reject.text = Some("wrong");
    let res = tests_with(
        r.path(),
        None,
        &reject,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(code(&res), Some("ORACLE-FROZEN"), "{}", res.message());
    assert_eq!(tree(&sealed), before, "the sealed set is untouched");
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::Frozen);
}

#[test]
fn a_tampered_draft_is_unknown_to_the_ambiguity_gate_never_clear() {
    // C5: an unreadable draft hid its open reading from a gated run.
    let r = repo_with(SPEC, "\n[review]\nrequired = true\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "an error message"));
    f.will_structure(held_out("fn login"));
    assert_eq!(
        write(r.path(), state.path(), d.clone()).status(),
        CommandStatus::Ok
    );
    let plain = speccoder_runtime::review::decide_in(
        r.path(),
        None,
        &[],
        "approve",
        None,
        Via::Terminal,
        Some(state.path()),
    );
    assert_eq!(plain.status(), CommandStatus::Ok, "{}", plain.message());
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "approve"]);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let draft = speccoder_runtime::oracle::dir(&plane, "001-graph", "US1");
    fs::write(
        draft.join("files/tests/acceptance_us1_login.txt"),
        "changed by a sync tool\n",
    )
    .unwrap();
    let gated = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(code(&gated), Some("ORACLE-TAMPERED"), "{}", gated.message());
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    assert!(
        i.items
            .iter()
            .any(|i| i.kind == "ambiguous" && i.detail.contains("changed")),
        "{:?}",
        i.items
    );
}

#[test]
fn an_unreadable_earlier_draft_is_not_overwritten_with_its_rejections() {
    // C6: Write Tests again over a tampered draft dropped its rejections.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let draft = speccoder_runtime::oracle::dir(&plane, "001-graph", "US1").join("draft.json");
    let text = fs::read_to_string(&draft)
        .unwrap()
        .replace("\"samples\"", "\"samples_\"");
    fs::write(&draft, &text).unwrap();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    let res = write(r.path(), state.path(), d);
    assert_eq!(code(&res), Some("ORACLE-TAMPERED"), "{}", res.message());
    assert_eq!(fs::read_to_string(&draft).unwrap(), text);
}

#[test]
fn expectations_the_comparison_cannot_read_fail_the_author_never_pass() {
    // C6: sample 2's expectations by unknown ids read as "no expectations",
    // so no ambiguity; ids differing in case are the same scenario.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let mut unknown = answer(standard(), "an error message");
    unknown["expectations"][1]["scenario"] = json!("US9-2");
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(unknown);
    let res = write(r.path(), state.path(), d);
    assert_eq!(
        code(&res),
        Some("ORACLE-AUTHOR-FAILED"),
        "{}",
        res.message()
    );
    assert!(res.message().contains("US9-2"), "{}", res.message());
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::None, "nothing stored");
    // Unparseable: not a list.
    let (f, d) = fake();
    let mut broken = answer(standard(), "refused");
    broken["expectations"] = json!("none");
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(broken);
    let res = write(r.path(), state.path(), d);
    assert_eq!(
        code(&res),
        Some("ORACLE-AUTHOR-FAILED"),
        "{}",
        res.message()
    );
    // Lower case: the same scenario, and the readings differ.
    let (f, d) = fake();
    let mut lower = answer(standard(), "an error message");
    lower["expectations"][1]["scenario"] = json!("us1-2");
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(lower);
    f.will_structure(held_out("fn login"));
    let res = write(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(us1(&res).state, OracleState::Ambiguous);
}

#[test]
fn an_unreadable_review_record_stops_the_task_and_refuses_decisions() {
    // C6: an unparseable review.json read as "nothing frozen" authored anew.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (_, d) = written(r.path(), state.path());
    freeze_only(r.path(), state.path(), d.clone());
    let file = r.path().join("specs/001-graph/review.json");
    fs::write(&file, "{ \"oracle\": ").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "broken"]);
    let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(code(&run), Some("REVIEW-UNREADABLE"), "{}", run.message());
    let decided = speccoder_runtime::review::decide_in(
        r.path(),
        None,
        &[],
        "approve",
        None,
        Via::Terminal,
        Some(state.path()),
    );
    assert_eq!(
        code(&decided),
        Some("REVIEW-UNREADABLE"),
        "{}",
        decided.message()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "{ \"oracle\": ");
    let rv = speccoder_runtime::review::report_in(r.path(), None, Some(state.path()));
    assert!(review(&rv).error.is_some());
}

#[test]
fn a_test_comment_whose_ask_again_failed_goes_back_to_the_tray() {
    // P6: no model — the Ask Again fails, the comment is not consumed.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let _ = written(r.path(), state.path());
    let add = speccoder_runtime::comments::comment(
        r.path(),
        None,
        speccoder_core::actions::CommentOp::Add,
        Some(&speccoder_core::oracle::CommentAnchor::TestLine {
            req: "US1".to_owned(),
            path: "tests/acceptance_us1_login.txt".to_owned(),
            line: 1,
            quote: "checks fn login".to_owned(),
        }),
        None,
        Some("check the signature"),
        Via::Web,
        Some(state.path()),
    );
    assert_eq!(add.status(), CommandStatus::Ok, "{}", add.message());
    let sent = speccoder_runtime::comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        None,
        None,
        &RunOptions::default(),
    );
    assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
    let Some(CommandData::Comments(c)) = sent.data() else {
        panic!("{}", sent.message())
    };
    assert!(c.turn.as_ref().unwrap().asked_again.is_empty());
    assert_eq!(c.drafts.len(), 1, "{}", sent.message());
    assert!(
        sent.message().contains("back in the tray"),
        "{}",
        sent.message()
    );
}

#[test]
fn the_code_view_never_shows_a_held_out_file_or_follows_a_link_out_of_the_worktree() {
    // C7: an untracked symlink into the held-out store; P5: the held-out
    // files overlaid on the worktree while they are judged.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        sheet_of(r.path(), state.path()).as_deref(),
        Via::Web,
        Some(state.path()),
        Some(d),
        &one_task(),
    );
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    let wt = PathBuf::from(&fr.attempts[0].worktree);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let sealed = speccoder_runtime::held_out::sealed_dir(&plane, "001-graph", "US1");
    let target = sealed.join("files").join(HELD_OUT_PATH);
    assert!(fs::read_to_string(&target).unwrap().contains(SECRET));
    std::os::unix::fs::symlink(&target, wt.join("tests/link.txt")).unwrap();
    let read = speccoder_runtime::service::code::worktree_file(r.path(), None, "tests/link.txt");
    assert!(read.is_err(), "{read:?}");
    fs::remove_file(wt.join("tests/link.txt")).unwrap();

    // An overlay that runs a second: polled meanwhile, nothing of it shows.
    let set = vec![(
        speccoder_runtime::held_out::HeldOutFile {
            path: "tests/held_out_overlay.txt".to_owned(),
            sha256: String::new(),
            scenarios: vec!["US1-1".to_owned()],
            argv: vec!["sleep".to_owned(), "1".to_owned()],
            composition: false,
        },
        format!("{SECRET}\n"),
    )];
    let tmp = tempfile::tempdir().unwrap();
    let (wt2, tmp2) = (wt.clone(), tmp.path().to_path_buf());
    let overlay = std::thread::spawn(move || {
        speccoder_runtime::held_out::overlay_run(
            &wt2,
            &wt2,
            &tmp2,
            &set,
            &speccoder_runtime::sandbox::Runner::Plain,
            &speccoder_runtime::tools::policy::Policy::new(&[], &[]),
        )
    });
    let mut seen_on_disk = false;
    while !overlay.is_finished() {
        if wt.join("tests/held_out_overlay.txt").exists() {
            seen_on_disk = true;
            let view = speccoder_runtime::service::code::worktree_view(r.path(), None);
            assert!(
                view.entries
                    .iter()
                    .all(|e| !e.path.contains("held_out_overlay")),
                "{:?}",
                view.entries
            );
            let read = speccoder_runtime::service::code::worktree_file(
                r.path(),
                None,
                "tests/held_out_overlay.txt",
            );
            assert!(read.is_err(), "{read:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    overlay.join().unwrap().unwrap();
    assert!(seen_on_disk, "the overlay was polled while it ran");
}

#[test]
fn approve_and_run_freezes_only_what_its_sheet_showed() {
    // P1: the draft changes after the sheet was shown (another tab).
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(answer(standard(), "refused"));
    f.will_structure(held_out("fn login"));
    assert_eq!(
        write(r.path(), state.path(), d.clone()).status(),
        CommandStatus::Ok
    );
    let shown = sheet_of(r.path(), state.path()).unwrap();
    let approve = |sheet: Option<&str>, via: Via| {
        speccoder_runtime::review::approve_and_run(
            r.path(),
            None,
            RunMode::Ask,
            &[],
            sheet,
            via,
            Some(state.path()),
            Some(d.clone()),
            &RunOptions {
                max_tasks: Some(0),
                ..RunOptions::default()
            },
        )
    };
    // A click without the sheet's digest is refused.
    let blind = approve(None, Via::Web);
    assert_eq!(code(&blind), Some("USAGE"), "{}", blind.message());
    // Another tab rejects a test: the red set the sheet showed is gone.
    let mut reject = request("US1", OracleOp::Reject);
    reject.path = Some("tests/acceptance_us1_login.txt");
    reject.text = Some("wrong");
    let res = tests_with(
        r.path(),
        None,
        &reject,
        Via::Web,
        state.path(),
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let stale = approve(Some(&shown), Via::Web);
    assert_eq!(code(&stale), Some("REVIEW-CHANGED"), "{}", stale.message());
    assert!(!r.path().join("specs/001-graph/review.json").exists());
    // The sheet as it is now approves.
    let now = sheet_of(r.path(), state.path()).unwrap();
    assert_ne!(now, shown);
    let ok = approve(Some(&now), Via::Web);
    assert!(ok.message().starts_with("approved"), "{}", ok.message());
}

#[test]
fn a_test_the_implementer_committed_under_the_frozen_name_does_not_skip_the_oracle() {
    // P2: an earlier task's implementer commits a file at the frozen path.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    fs::create_dir_all(r.path().join("tests")).unwrap();
    fs::write(
        r.path().join("tests/acceptance_us1_login.txt"),
        "the implementer's own\n",
    )
    .unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "an earlier task"]);
    let asked = f.asked().len();
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let res = speccoder_runtime::review::approve_and_run(
        r.path(),
        None,
        RunMode::Ask,
        &[],
        None,
        Via::Terminal,
        Some(state.path()),
        Some(d),
        &one_task(),
    );
    assert!(
        res.message()
            .contains("exists and differs from the confirmed test"),
        "the frozen oracle was skipped: {}",
        res.message()
    );
    assert_eq!(f.asked().len(), asked, "no author call");
}

#[test]
fn a_frozen_record_without_its_approval_or_held_out_binding_is_not_reused() {
    // P3: editing the state record to drop `held_out_sealed` (or review.json
    // to drop the approval) silently removed a check.
    for broken in ["held_out", "approval"] {
        let r = repo();
        let state = tempfile::tempdir().unwrap();
        let (_, d) = written(r.path(), state.path());
        freeze_only(r.path(), state.path(), d.clone());
        let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
        if broken == "held_out" {
            let file =
                speccoder_runtime::pre_frozen::dir(&plane, "001-graph", "US1").join("record.json");
            let mut rec: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
            rec["held_out_sealed"] = Value::Null;
            fs::write(&file, serde_json::to_string(&rec).unwrap()).unwrap();
        } else {
            let file = r.path().join("specs/001-graph/review.json");
            let mut rec: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
            rec.as_object_mut().unwrap().remove("approval");
            fs::write(&file, serde_json::to_string(&rec).unwrap()).unwrap();
            git(r.path(), &["add", "-A"]);
            git(r.path(), &["commit", "-q", "-m", "drop the approval"]);
        }
        let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
        assert_eq!(
            code(&run),
            Some("ORACLE-TAMPERED"),
            "{broken}: {}",
            run.message()
        );
    }
}

#[test]
fn a_frozen_card_shows_the_frozen_set_not_a_draft_changed_since() {
    // P4: the draft changes after the freeze; the run uses the frozen set.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (_, d) = written(r.path(), state.path());
    freeze_only(r.path(), state.path(), d);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let (mut draft, mut contents) = speccoder_runtime::oracle::load(&plane, "001-graph", "US1")
        .unwrap()
        .unwrap();
    let changed = "checks something else\n";
    draft.tests[0].sha256 = format!(
        "sha256:{}",
        speccoder_runtime::integrity::sha256_hex(changed.as_bytes())
    );
    contents[0].1 = changed.to_owned();
    speccoder_runtime::oracle::store(&plane, "001-graph", &draft, &contents).unwrap();
    let v = us1(&speccoder_runtime::review::report_in(
        r.path(),
        None,
        Some(state.path()),
    ));
    assert_eq!(v.state, OracleState::Frozen);
    assert_eq!(v.tests.len(), 1);
    assert_eq!(v.tests[0].content, "checks fn login\n", "{v:?}");
    assert_eq!(v.held_out.count, 1);
}

/// The calls a test author (visible or held out) was asked.
fn author_calls(f: &FakeProvider) -> usize {
    f.asked()
        .iter()
        .filter(|a| {
            let p = serde_json::to_string(&a.prompt).unwrap();
            p.contains("You are the test author") || p.contains("HELD-OUT")
        })
        .count()
}

fn report_of(res: &CommandResult) -> &speccoder_core::RunReport {
    match res.data() {
        Some(CommandData::Run(r)) => r,
        other => panic!("{} → {other:?}", res.message()),
    }
}

#[test]
fn us4_a_frozen_set_already_passing_is_not_demonstrated_and_nothing_is_authored() {
    // 4.2: the behaviour landed after the tests were written.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    fs::write(
        r.path().join("src/lib.rs"),
        "pub fn ok() {}\npub fn login() {}\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "login landed"]);
    freeze_only(r.path(), state.path(), d.clone());
    let before = author_calls(&f);
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n// again\n");
    let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let rep = report_of(&run);
    let acc = rep
        .acceptance
        .as_ref()
        .unwrap_or_else(|| panic!("{}", run.message()));
    assert_eq!(acc["red"], false, "{acc}");
    assert!(
        acc["note"]
            .as_str()
            .is_some_and(|n| n.contains("already passing")),
        "{acc}"
    );
    assert_eq!(acc["source"], "review");
    assert_ne!(rep.verdict, Some(speccoder_core::verify::Verdict::Verified));
    assert_eq!(author_calls(&f), before, "no author call");
}

#[test]
fn us4_a_lapsed_or_absent_frozen_set_is_authored_as_today_with_one_author_call() {
    // 4.3 and 4.5.
    for case in ["lapsed", "elsewhere"] {
        let r = repo();
        let state = tempfile::tempdir().unwrap();
        let (f, d) = written(r.path(), state.path());
        freeze_only(r.path(), state.path(), d.clone());
        if case == "lapsed" {
            fs::write(
                r.path().join("specs/001-graph/spec.md"),
                SPEC.replace("they are told so", "they are told to wait"),
            )
            .unwrap();
            git(r.path(), &["commit", "-qam", "reword"]);
        } else {
            let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
            fs::remove_dir_all(speccoder_runtime::pre_frozen::dir(
                &plane,
                "001-graph",
                "US1",
            ))
            .unwrap();
        }
        let before = author_calls(&f);
        f.will_structure(json!({
            "files": [file("tests/acceptance_t005.txt", &["US1-1"], "fn login")],
            "argv": ["./accept.sh", "--", "tests/acceptance_t005.txt", "fn login"],
            "notes": [],
        }));
        f.will_structure(held_out("fn login"));
        write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
        let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
        let rep = report_of(&run);
        let said = if case == "lapsed" {
            "the frozen oracle lapsed — authoring"
        } else {
            "frozen oracle not present here — authoring"
        };
        assert!(
            rep.notices.iter().any(|n| n.contains(said)),
            "{case}: {:?}",
            rep.notices
        );
        let visible = f
            .asked()
            .iter()
            .skip(before)
            .filter(|a| {
                serde_json::to_string(&a.prompt)
                    .unwrap()
                    .contains("You are the test author")
            })
            .count();
        assert_eq!(visible, 1, "{case}: one visible author call");
        assert_ne!(
            rep.acceptance.as_ref().unwrap()["source"],
            "review",
            "{case}"
        );
    }
}

#[test]
fn a_crash_between_writing_the_frozen_files_and_the_red_check_resumes_without_an_author_call() {
    // T062 (fault injection): the process dies after the frozen tests were
    // written into the worktree, before they were red-checked.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = written(r.path(), state.path());
    freeze_only(r.path(), state.path(), d.clone());
    let made = speccoder_runtime::worktree_with(r.path(), None, state.path());
    let Some(CommandData::Worktree(w)) = made.data() else {
        panic!("{}", made.message())
    };
    let wt = PathBuf::from(w.path.clone().unwrap());
    fs::create_dir_all(wt.join("tests")).unwrap();
    fs::write(
        wt.join("tests/acceptance_us1_login.txt"),
        "checks fn login\n",
    )
    .unwrap();
    let before = author_calls(&f);
    write_lib(&f, "c1", "pub fn ok() {}\npub fn login() {}\n");
    let run = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let rep = report_of(&run);
    assert_eq!(
        rep.verdict,
        Some(speccoder_core::verify::Verdict::Verified),
        "{}",
        run.message()
    );
    let acc = rep.acceptance.as_ref().unwrap();
    assert_eq!(
        (acc["source"].as_str(), acc["red"].as_bool()),
        (Some("review"), Some(true)),
        "{acc}"
    );
    assert_eq!(author_calls(&f), before, "no author call after the crash");
}
