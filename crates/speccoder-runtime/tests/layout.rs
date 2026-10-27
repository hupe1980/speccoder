//! One acceptance file per story, grown by additions (spec 102, US3): two
//! tasks of one story write one `tests/acceptance_us1.rs`, the second as a
//! section added to the first's, red only by what it adds — both verified.

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
use speccoder_core::{CommandData, CommandResult, RunReport};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const SPEC: &str = "# Feature Specification: greet\n\n## User Scenarios & Testing\n\n### User Story 1 - Greeting (Priority: P1)\n\nA caller is greeted.\n\n**Acceptance Scenarios**:\n\n1. **Given** a name, **When** greeted, **Then** it returns `Hello, <name>!`.\n2. **Given** an empty name, **When** greeted, **Then** it returns `Hello, world!`.\n";

const TASKS: &str = "# Tasks: greet\n\n## Phase 1: User Story 1 - Greeting (Priority: P1)\n\n- [ ] T001 [US1] Implement `greet` for names in src/lib.rs\n- [ ] T002 [US1] Implement the empty-name default of `greet` in src/lib.rs\n";

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.build]\nargv = [\"cargo\", \"build\", \"--offline\"]\n\n[commands]\nallow_prefix = [[\"cargo\", \"test\"]]\n\n[acceptance]\nheld_out = false\nstrength = false\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let specify = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-speckit/tests/fixtures/graph/.specify");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(&specify)
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    for (p, t) in [
        (
            "Cargo.toml",
            "[package]\nname = \"greet\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        ("src/lib.rs", "pub fn hello() {}\n"),
        ("specs/001-greet/spec.md", SPEC),
        ("specs/001-greet/plan.md", "# Plan\n\nRust.\n"),
        ("specs/001-greet/tasks.md", TASKS),
        ("speccoder.toml", CONFIG),
        (".gitignore", "target\n"),
    ] {
        let f = r.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    }
    assert!(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(r)
            .status()
            .unwrap()
            .success()
    );
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec"]);
    dir
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn done(s: &str) -> Value {
    json!({ "summary": s, "status": "done", "files_changed": [], "checks_run": [], "notes": [] })
}

fn run(repo: &Path, state: &Path, task: &str, f: Arc<FakeProvider>) -> CommandResult {
    let d: Arc<dyn ModelProvider> = f;
    speccoder_runtime::run_with(repo, None, task, state, d, None)
}

const T001: &str = "// T001 — US1-1\nuse greet::greet;\n\n#[test]\nfn t001_greets_by_name() {\n    assert_eq!(greet(\"Ada\"), \"Hello, Ada!\");\n}\n";
const T002: &str = "\n// T002 — US1-2\n#[test]\nfn t002_greets_the_world() {\n    assert_eq!(greet(\"\"), \"Hello, world!\");\n}\n";

fn author(content: &str) -> Value {
    json!({
        "files": [{ "path": "tests/acceptance_us1.rs", "content": content, "scenarios": ["US1-1"] }],
        "argv": ["cargo", "test", "--offline", "--test", "acceptance_us1"],
        "notes": []
    })
}

#[test]
fn a_story_s_tasks_share_one_acceptance_file_and_both_verify() {
    let r = project();
    let state = tempfile::tempdir().unwrap();

    // T001 creates the story's file: red (greet does not exist), then green.
    let f = FakeProvider::new();
    f.will_structure(author(T001));
    f.will_call_tool("c1", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n" }));
    f.will_structure(done("greet"));
    let first = run(r.path(), state.path(), "T001", f.clone());
    let rep = report(&first);
    assert_eq!(
        rep.verdict,
        Some(Verdict::Verified),
        "{} {:?}",
        first.message(),
        rep.acceptance
    );
    let asked = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        asked.contains("into ONE file: `tests/acceptance_us1.rs`"),
        "{asked}"
    );
    assert!(
        asked.contains("Test layout (detected by speccoder"),
        "{asked}"
    );
    let wt = PathBuf::from(&rep.worktree);
    assert!(
        git(&wt, &["log", "-1", "--format=%B"]).contains("Story-File: tests/acceptance_us1.rs\n")
    );

    // T002 adds its section: red only by what it adds — T001's passing
    // test neither makes it red nor hides its red.
    let f = FakeProvider::new();
    f.will_structure(author(&format!("{T001}{T002}")));
    f.will_call_tool("c1", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn greet(name: &str) -> String {\n    if name.is_empty() {\n        return \"Hello, world!\".to_owned();\n    }\n    format!(\"Hello, {name}!\")\n}\n" }));
    f.will_structure(done("default"));
    let second = run(r.path(), state.path(), "T002", f.clone());
    let rep = report(&second);
    assert_eq!(
        rep.verdict,
        Some(Verdict::Verified),
        "{} {:?}",
        second.message(),
        rep.acceptance
    );
    let a = rep.acceptance.as_ref().unwrap();
    assert_eq!(
        (a["appended"].clone(), a["red_class"].clone()),
        (json!(true), json!("assertion")),
        "{a}"
    );
    let asked = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        asked.contains("It already holds the tests of earlier tasks"),
        "{asked}"
    );
    assert!(
        asked.contains("t001_greets_by_name"),
        "the current content is given: {asked}"
    );
    // One file for the story, holding both sections.
    let file = fs::read_to_string(wt.join("tests/acceptance_us1.rs")).unwrap();
    assert!(file.contains("t001_greets_by_name") && file.contains("t002_greets_the_world"));
    assert!(!wt.join("tests/acceptance_t002.rs").exists());
}

#[test]
fn a_section_that_changes_an_existing_line_is_refused() {
    let r = project();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(author(T001));
    f.will_call_tool("c1", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n" }));
    f.will_structure(done("greet"));
    let first = run(r.path(), state.path(), "T001", f);
    assert_eq!(
        report(&first).verdict,
        Some(Verdict::Verified),
        "{}",
        first.message()
    );
    let f = FakeProvider::new();
    f.will_structure(author(
        &T001
            .replace("\"Ada\"", "\"Bob\"")
            .replace("Hello, Ada!", "Hello, Bob!"),
    ));
    f.will_structure(done("nothing"));
    let second = run(r.path(), state.path(), "T002", f);
    let rep = report(&second);
    assert_eq!(
        rep.verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        second.message()
    );
    assert_eq!(
        rep.regression.as_ref().unwrap().text,
        "its acceptance tests could not be used: changes existing lines of `tests/acceptance_us1.rs`"
    );
}

#[test]
fn autotests_off_leaves_no_place_and_writes_no_acceptance_test() {
    let r = project();
    let manifest = r.path().join("Cargo.toml");
    fs::write(
        &manifest,
        "[package]\nname = \"greet\"\nversion = \"0.1.0\"\nedition = \"2024\"\nautotests = false\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "no autotests"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(done("nothing to do"));
    let res = run(r.path(), state.path(), "T001", f.clone());
    let rep = report(&res);
    assert_eq!(
        rep.regression.as_ref().map(|r| r.text.as_str()),
        Some("its acceptance tests could not be used: no place the runner collects"),
        "{}",
        res.message()
    );
    // No author was asked.
    assert_eq!(f.asked().len(), 1);
    // doctor names the setting that causes it.
    let checks = speccoder_runtime::doctor::checks(r.path(), None, &|_| None);
    let tests = checks
        .iter()
        .find(|c| c.check == "tests")
        .expect("a tests check");
    assert!(
        tests.detail.contains("autotests = false"),
        "{}",
        tests.detail
    );
}

#[test]
fn a_held_out_file_the_runner_would_not_collect_is_discarded_at_authoring() {
    let r = project();
    fs::write(
        r.path().join("speccoder.toml"),
        CONFIG.replace("held_out = false", "held_out = true"),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "held out on"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(author(T001));
    f.will_structure(json!({
        "files": [{ "path": "tests/held/held_t001.rs", "content": "use greet::greet;\n#[test]\nfn h() { assert_eq!(greet(\"Bo\"), \"Hello, Bo!\"); }\n", "scenarios": ["US1-1"], "argv": ["cargo", "test", "--offline", "--test", "held_t001"], "composition": false }],
        "notes": []
    }));
    f.will_call_tool("c1", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n" }));
    f.will_structure(done("greet"));
    let res = run(r.path(), state.path(), "T001", f.clone());
    let rep = report(&res);
    let h = &rep.acceptance.as_ref().unwrap()["held_out"];
    assert_eq!(
        (h["state"].clone(), h["reason"].clone()),
        (json!("absent"), json!("not_collected")),
        "{h}"
    );
    let asked = serde_json::to_string(&f.asked()[1].prompt).unwrap();
    assert!(
        asked.contains("Place each file where the runner collects it"),
        "{asked}"
    );
}
