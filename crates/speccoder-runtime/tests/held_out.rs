//! Held-out acceptance end to end with a scripted model: tests the
//! implementer never sees decide `verified`; a retry learns the scenario,
//! nothing else.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n";

/// One test: "the pattern is in src/lib.rs", with libtest's summary line.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n**Acceptance**: a `login` function exists.\n";

/// Only the held-out test holds it.
const SECRET: &str = "Zq9-held-out-only-literal";
const HELD_OUT_PATH: &str = "tests/held_out_login.txt";

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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo(extra: &str) -> tempfile::TempDir {
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
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
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

fn visible() -> Value {
    json!({ "files": [{ "path": "tests/acceptance_login.txt", "content": "login exists\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn login"], "notes": [] })
}

fn held_out(pattern: &str, scenarios: &[&str]) -> Value {
    json!({ "files": [{
        "path": HELD_OUT_PATH,
        "content": format!("login takes a password; token \"{SECRET}\"\n"),
        "scenarios": scenarios,
        "argv": ["./accept.sh", "--", pattern],
        "composition": false,
    }], "notes": [] })
}

fn write_lib(f: &FakeProvider, id: &str, lib: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": "src/lib.rs", "content": lib }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
}

const TAILORED: &str = "pub fn ok() {}\npub fn login() {}\n";
const GENERAL: &str = "pub fn ok() {}\npub fn login() {}\npub fn login_with_password() {}\n";

fn run(r: &Path, state: &Path, d: Arc<dyn ModelProvider>) -> CommandResult {
    speccoder_runtime::run_with(r, None, "T005", state, d, None)
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

/// Every request the implementer got (the calls with tools).
fn implementer_requests(f: &FakeProvider) -> Vec<String> {
    f.asked()
        .iter()
        .filter(|a| !a.tools.is_empty())
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .collect()
}

#[test]
fn tailored_work_is_not_verified_and_the_retry_learns_only_the_scenario() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(visible());
    f.will_structure(held_out("fn login_with_password", &["US1-1"]));
    write_lib(&f, "c1", TAILORED);
    write_lib(&f, "c2", GENERAL);

    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::Verified));
    assert!(rep.ticked);
    // Attempt 1 passed visibly and failed held-out: a failed verdict naming the scenario.
    let first = &rep.earlier[0];
    let Some(Verdict::Failed { reasons }) = &first.verdict else {
        panic!("{first:?}")
    };
    assert_eq!(
        reasons,
        &["held-out acceptance failed: US1-1 (\"**Acceptance**: a `login` function exists.\")"]
    );
    let h = &rep.acceptance.as_ref().unwrap()["held_out"];
    assert_eq!(h["state"], "on", "{h}");
    assert_eq!(h["gap"], true);
    let outcomes: Vec<&str> = h["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, ["failed", "passed"]);

    // Nothing of the held-out set ever reached the implementer.
    let requests = implementer_requests(&f);
    assert!(requests.len() >= 2);
    for q in &requests {
        assert!(!q.contains(HELD_OUT_PATH), "held-out path in {q}");
        assert!(!q.contains(SECRET), "held-out literal in {q}");
        assert!(
            !q.contains("login_with_password"),
            "held-out instance in {q}"
        );
    }
    assert!(
        requests.last().unwrap().contains("US1-1"),
        "the retry names the scenario"
    );

    // …nor is it in the tree; the tick names the manifest.
    let wt = Path::new(&rep.worktree);
    assert!(!wt.join(HELD_OUT_PATH).exists());
    let body = git(wt, &["log", "-1", "--format=%B"]);
    let trailer = body
        .lines()
        .find(|l| l.starts_with("Held-Out: "))
        .unwrap_or_else(|| panic!("{body}"));
    assert!(
        trailer.starts_with("Held-Out: passed=1/1 manifest=sha256:"),
        "{trailer}"
    );
    assert_eq!(
        trailer.len(),
        "Held-Out: passed=1/1 manifest=sha256:".len() + 64
    );
}

#[test]
fn the_same_scenarios_failing_twice_stop_retrying() {
    let r = repo("\n[budget]\nmax_attempts = 3\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(visible());
    f.will_structure(held_out("fn login_with_password", &["US1-1"]));
    write_lib(&f, "c1", TAILORED);
    write_lib(&f, "c2", TAILORED);
    let res = run(r.path(), state.path(), d);
    assert_eq!(
        res.status(),
        CommandStatus::VerificationFailed,
        "{}",
        res.message()
    );
    let rep = report(&res);
    assert!(!rep.ticked);
    assert!(
        rep.retry_stop
            .as_deref()
            .unwrap()
            .starts_with("same failure twice"),
        "{:?}",
        rep.retry_stop
    );
}

#[test]
fn held_out_tests_that_are_not_red_or_cite_unknown_scenarios_are_discarded() {
    for (pattern, scenarios, reason) in [
        ("fn ok", vec!["US1-1"], "not_red"),
        ("fn login_with_password", vec!["US9-9"], "unknown_scenario"),
    ] {
        let r = repo("");
        let state = tempfile::tempdir().unwrap();
        let (f, d) = fake();
        f.will_structure(visible());
        f.will_structure(held_out(pattern, &scenarios));
        write_lib(&f, "c1", TAILORED);
        let res = run(r.path(), state.path(), d);
        assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
        let rep = report(&res);
        // Verified on the visible evidence, and honest that no held-out test judged it.
        assert_eq!(rep.verdict, Some(Verdict::Verified));
        let h = &rep.acceptance.as_ref().unwrap()["held_out"];
        assert_eq!(
            (h["state"].as_str(), h["reason"].as_str()),
            (Some("absent"), Some("all_discarded")),
            "{h}"
        );
        assert_eq!(h["discarded"][0]["reason"], reason);
        let body = git(Path::new(&rep.worktree), &["log", "-1", "--format=%B"]);
        assert!(body.contains("Held-Out: absent=all_discarded"), "{body}");
    }
}

#[test]
fn a_held_out_literal_in_the_attempt_stops_for_a_person() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(visible());
    f.will_structure(held_out("fn login_with_password", &["US1-1"]));
    write_lib(
        &f,
        "c1",
        &format!("{GENERAL}pub const TOKEN: &str = \"{SECRET}\";\n"),
    );
    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = report(&res);
    assert!(!rep.ticked);
    assert!(rep.saved_attempt.is_some());
    assert!(
        res.message().contains("only the held-out tests hold"),
        "{}",
        res.message()
    );
}

#[test]
fn each_visible_test_is_traced_to_the_scenario_it_cites() {
    let r = repo("\n[acceptance]\nheld_out = false\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(json!({ "files": [
        { "path": "tests/acceptance_login.txt", "content": "login exists\n", "scenarios": ["US1-1", "US9-9"] },
        { "path": "tests/acceptance_more.txt", "content": "more\n", "scenarios": [] },
    ], "argv": ["./accept.sh", "--", "fn login"], "notes": [] }));
    write_lib(&f, "c1", TAILORED);
    let res = run(r.path(), state.path(), d);
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::Verified), "{}", res.message());
    // The author saw the scenarios to cite; an ID the spec lacks is dropped.
    let author = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(author.contains("US1-1"), "{author}");
    let tests = &rep.acceptance.as_ref().unwrap()["tests"];
    assert_eq!(
        tests,
        &json!([
            { "path": "tests/acceptance_login.txt", "scenarios": ["US1-1"] },
            { "path": "tests/acceptance_more.txt", "scenarios": [] },
        ])
    );
    // The signed trace: scenario → test → verdict; untagged ⇒ unknown.
    let commit = rep.commit.clone().unwrap();
    let note = git(r.path(), &["notes", "--ref=speccoder", "show", &commit]);
    let trace = note
        .lines()
        .map(|l| {
            use base64::Engine as _;
            let e: Value = serde_json::from_str(l).unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(e["payload"].as_str().unwrap())
                .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        })
        .find(|s| s["predicateType"] == "https://speccoder.dev/task-verification/v0")
        .unwrap()["predicate"]["trace"]
        .clone();
    let rows: Vec<(Value, Value)> = trace
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["scenario"].clone(), r["test"].clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (json!("US1-1"), json!("tests/acceptance_login.txt")),
            (Value::Null, json!("tests/acceptance_more.txt")),
        ]
    );
}

#[test]
fn switched_off_it_is_visible_evidence_only_and_says_so() {
    let r = repo("\n[acceptance]\nheld_out = false\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(visible());
    write_lib(&f, "c1", TAILORED);
    let res = run(r.path(), state.path(), d);
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::Verified));
    let body = git(Path::new(&rep.worktree), &["log", "-1", "--format=%B"]);
    assert!(body.contains("Held-Out: off"), "{body}");
    // No held-out author was asked.
    assert!(
        !f.asked().iter().any(|a| serde_json::to_string(&a.prompt)
            .unwrap()
            .contains("HELD-OUT")),
        "a held-out author call"
    );
}

#[test]
fn a_finished_feature_publishes_its_held_out_sets() {
    let r = repo("");
    // Only T005 is open: the feature ends with its tick.
    let tasks = r.path().join("specs/001-graph/tasks.md");
    let text = fs::read_to_string(&tasks)
        .unwrap()
        .replace("- [ ] T0", "- [x] T0")
        .replace("- [x] T005", "- [ ] T005");
    fs::write(&tasks, text).unwrap();
    git(r.path(), &["commit", "-qam", "only T005"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(visible());
    f.will_structure(held_out("fn login_with_password", &["US1-1"]));
    write_lib(&f, "c1", GENERAL);
    let res = speccoder_runtime::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!()
    };
    let wt = Path::new(&fr.attempts[0].worktree);
    let reference = "refs/speccoder/held-out/001-graph";
    // Never in the project's repository, which every later attempt reads:
    // no ref, and not one object of the publication.
    let local = Command::new("git")
        .args(["rev-parse", "--verify", "-q", reference])
        .current_dir(wt)
        .output()
        .unwrap();
    assert!(
        !local.status.success(),
        "the held-out ref is in the project"
    );
    let project_state = speccoder_runtime::worktree::project_state_dir(
        state.path(),
        &std::fs::canonicalize(r.path()).unwrap(),
    );
    let store = project_state.join("held-out.git");
    let files = git(&store, &["ls-tree", "-r", "--name-only", reference]);
    assert!(files.lines().any(|l| l == "T005/manifest.json"), "{files}");
    assert!(
        files
            .lines()
            .any(|l| l == format!("T005/files/{HELD_OUT_PATH}")),
        "{files}"
    );
    // Its parent is the feature branch the tick is on, and the manifest is
    // the one the tick names.
    let parent = git(&store, &["rev-parse", &format!("{reference}^1")]);
    assert_eq!(parent, git(wt, &["rev-parse", "speccoder/001-graph"]));
    let blob = git(
        &store,
        &["rev-parse", &format!("{reference}:T005/manifest.json")],
    );
    let in_project = Command::new("git")
        .args(["cat-file", "-e", blob.trim()])
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", "")
        .current_dir(wt)
        .output()
        .unwrap();
    assert!(
        !in_project.status.success(),
        "a held-out object is in the project"
    );
    let manifest = git(
        &store,
        &["show", &format!("{reference}:T005/manifest.json")],
    );
    let m: speccoder_runtime::held_out::Manifest = serde_json::from_str(&manifest).unwrap();
    let body = git(wt, &["log", "-1", "--format=%B", "speccoder/001-graph"]);
    assert!(
        body.contains(&format!("manifest=sha256:{}", m.sha256())),
        "{body}"
    );
}

fn no_credits() -> agentplane::model::ModelError {
    agentplane::model::ModelError::Refused {
        model: agentplane::model::ModelId::new("openai", "gpt-5.4-mini"),
        detail: "credit_balance_exhausted: You have no credits remaining.".to_owned(),
    }
}

#[test]
fn a_refused_author_call_stops_the_task_instead_of_ticking_it() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_fail(no_credits());
    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert!(
        res.message()
            .contains("the acceptance tests could not be written")
            && res.message().contains("credit_balance_exhausted"),
        "{}",
        res.message()
    );
    let rep = report(&res);
    assert!(!rep.ticked && rep.verdict.is_none());
    assert_eq!(rep.attempt_status, "acceptance_failed");
    assert!(
        f.asked().iter().all(|a| a.tools.is_empty()),
        "no attempt was made"
    );
}
