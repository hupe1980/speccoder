//! The plan checked for an oracle before it runs: the
//! POC's task list predicts its outcome, a format without phases gets only
//! the format-independent check, a run is never stopped by them, and the
//! person's `acceptance_at` decision turns a Foundational implementation
//! into evidence.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::actions::Via;
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, DoctorLevel};
use speccoder_runtime::progress::{Event, Progress};

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

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.build]\nargv = [\"cargo\", \"build\", \"--offline\"]\n\n[commands]\nallow_prefix = [[\"cargo\", \"test\"]]\n\n[acceptance]\nheld_out = false\nstrength = false\n";

/// The POC at its planning commit.
fn poc() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poc-greet");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::rename(r.join("Cargo.toml.fixture"), r.join("Cargo.toml")).unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    fs::write(r.join(".gitignore"), "target\n").unwrap();
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

#[test]
fn the_poc_s_plan_predicts_its_outcome_in_three_warnings() {
    let r = poc();
    let began = std::time::Instant::now();
    let checks: Vec<_> =
        speccoder_runtime::doctor::checks(r.path(), Some("001-greet-function"), &|_| None)
            .into_iter()
            .filter(|c| c.check == "plan")
            .collect();
    assert!(
        began.elapsed().as_secs_f64() < 5.0,
        "the plan checks took {:?}",
        began.elapsed()
    );
    let by = |code: &str| {
        checks
            .iter()
            .find(|c| c.code.as_deref() == Some(code))
            .unwrap_or_else(|| panic!("{code}: {checks:?}"))
    };
    assert!(checks.iter().all(|c| c.level == DoctorLevel::Warn));
    let uncollected = by("PLAN-TESTS-UNCOLLECTED");
    assert_eq!(uncollected.tasks, ["T002", "T005", "T006", "T009", "T012"]);
    assert_eq!(
        uncollected.detail,
        "T002, T005, T006, T009, T012 name `tests/unit/` — Cargo won't build the tests there"
    );
    assert!(
        uncollected
            .fix
            .iter()
            .any(|f| f.contains("tests/unit/main.rs")),
        "{:?}",
        uncollected.fix
    );
    let early = by("PLAN-BEHAVIOUR-EARLY");
    assert_eq!(early.tasks, ["T003"]);
    assert_eq!(
        early.detail,
        "T003 (Foundational) names `greet`, which US1 is about — if T003 implements it, US1's acceptance cannot go red and its tasks will tick regression-only"
    );
    let remedy = early.remedy.as_ref().unwrap();
    assert_eq!(
        remedy.command,
        "speccoder task acceptance-at T003 --story US1"
    );
    let only = by("PLAN-TEST-ONLY-TASKS");
    assert_eq!(only.tasks, ["T005", "T006", "T009"]);
    assert!(
        only.detail
            .ends_with("merge them into the implementing tasks")
    );
    // Nothing is ticked: re-planning is offered.
    assert!(
        only.fix
            .iter()
            .any(|f| f.starts_with("speccoder spec tasks")),
        "{:?}",
        only.fix
    );
    // --json carries the tasks for the chips.
    let v = serde_json::to_value(early).unwrap();
    assert_eq!(v["tasks"], json!(["T003"]));
    assert_eq!(v["remedy"]["run"]["kind"], "acceptance_at");
}

#[test]
fn a_format_without_phases_gets_only_the_uncollected_path_check() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-openspec/tests/fixtures/1.14.3/sample");
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
        r.join("Cargo.toml"),
        "[package]\nname = \"g\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    let tasks = r.join("openspec/changes/add-farewell-and-locale/tasks.md");
    let text = fs::read_to_string(&tasks).unwrap();
    fs::write(
        &tasks,
        text.replace(
            "## Workflow follow-up",
            "- [ ] 2.3 Add tests for `--bye` in `tests/unit/bye.rs`\n\n## Workflow follow-up",
        ),
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let checks: Vec<_> =
        speccoder_runtime::doctor::checks(r, Some("add-farewell-and-locale"), &|_| None)
            .into_iter()
            .filter(|c| c.check == "plan")
            .collect();
    let codes: Vec<&str> = checks.iter().filter_map(|c| c.code.as_deref()).collect();
    assert_eq!(codes, ["PLAN-TESTS-UNCOLLECTED"], "{checks:?}");
    assert_eq!(checks[0].tasks, ["2.3"]);
}

#[test]
fn a_plan_without_these_causes_raises_nothing() {
    let r = poc();
    let tasks = r.path().join("specs/001-greet-function/tasks.md");
    fs::write(
        &tasks,
        "# Tasks: greet\n\n## Phase 1: Setup\n\n- [ ] T001 Create the library crate in `src/lib.rs`\n\n## Phase 2: User Story 1 - Greeting by Name (Priority: P1)\n\n- [ ] T002 [US1] Implement `greet` in src/lib.rs with tests in `tests/greet.rs`\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "re-planned"]);
    let checks: Vec<_> =
        speccoder_runtime::doctor::checks(r.path(), Some("001-greet-function"), &|_| None)
            .into_iter()
            .filter(|c| c.check == "plan")
            .collect();
    assert!(checks.is_empty(), "{checks:?}");
}

#[derive(Debug, Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Progress for Recorder {
    fn event(&self, e: &Event) {
        self.0.lock().unwrap().push(e.clone());
    }
}

const T003_TEST: &str = "// T003 — US1-1\nuse greet::greet;\n\n#[test]\nfn t003_greets_by_name() {\n    assert_eq!(greet(\"Ada\"), \"Hello, Ada!\");\n}\n";

#[test]
fn a_run_starts_with_one_notice_and_acceptance_at_t003_makes_it_verified() {
    let r = poc();
    let state = tempfile::tempdir().unwrap();
    // The run is told once, and starts T001 without asking.
    let rec = Arc::new(Recorder::default());
    let f = FakeProvider::new();
    f.will_structure(json!({ "summary": "s", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f;
    let options = speccoder_runtime::RunOptions {
        max_tasks: Some(1),
        progress: Some(rec.clone()),
        ..speccoder_runtime::RunOptions::default()
    };
    let res = speccoder_runtime::run_feature_with(r.path(), None, state.path(), &d, None, &options);
    let notices: Vec<String> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Notice { text } if text.starts_with("plan:") => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        notices,
        ["plan: 3 warnings — speccoder doctor --feature 001-greet-function"]
    );
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{}", res.message())
    };
    assert_eq!(fr.attempts[0].task, "T001");

    // T002 (Setup), then the decision: US1's acceptance at T003.
    let f = FakeProvider::new();
    f.will_structure(json!({ "summary": "s", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let t002 = speccoder_runtime::run_with(r.path(), None, "T002", state.path(), f, None);
    assert!(
        t002.status() == speccoder_core::CommandStatus::Ok,
        "{}",
        t002.message()
    );
    let decided = speccoder_runtime::acceptance_at::decide(
        r.path(),
        None,
        ("US1", "T003", false),
        Via::Terminal,
        state.path(),
    );
    assert_eq!(
        decided.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        decided.message()
    );
    // T003 implements greet: red on US1's scenario first, then green.
    let f = FakeProvider::new();
    f.will_structure(json!({
        "files": [{ "path": "tests/acceptance_us1.rs", "content": T003_TEST, "scenarios": ["US1-1"] }],
        "argv": ["cargo", "test", "--offline", "--test", "acceptance_us1"],
        "notes": []
    }));
    f.will_call_tool("c1", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {}\n}\n" }));
    f.will_structure(json!({ "summary": "greet", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let t003 = speccoder_runtime::run_with(r.path(), None, "T003", state.path(), f.clone(), None);
    let Some(CommandData::Run(rep)) = t003.data() else {
        panic!("{}", t003.message())
    };
    assert_eq!(
        rep.verdict,
        Some(Verdict::Verified),
        "{} {:?}",
        t003.message(),
        rep.acceptance
    );
    assert_eq!(
        rep.acceptance.as_ref().unwrap()["story_file"],
        "tests/acceptance_us1.rs"
    );
    let asked = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        asked.contains("US1-1"),
        "the author was given US1's scenarios: {asked}"
    );
    // Revocable by the same action.
    let revoked = speccoder_runtime::acceptance_at::decide(
        r.path(),
        None,
        ("US1", "T003", true),
        Via::Terminal,
        state.path(),
    );
    assert_eq!(revoked.status(), speccoder_core::CommandStatus::Ok);
}

#[test]
fn two_hundred_tasks_are_checked_in_under_a_second() {
    let r = poc();
    let mut tasks = String::from(
        "# Tasks\n\n## Phase 1: Setup\n\n- [ ] T001 Create `src/lib.rs`\n\n## Phase 2: User Story 1 - Many (Priority: P1)\n\n",
    );
    for i in 2..=200 {
        tasks.push_str(&format!(
            "- [ ] T{i:03} [US1] Implement `step{i}` in src/lib.rs\n"
        ));
    }
    fs::write(r.path().join("specs/001-greet-function/tasks.md"), tasks).unwrap();
    git(r.path(), &["commit", "-qam", "many"]);
    let opened = speccoder_runtime::format::open(r.path(), Some("001-greet-function")).unwrap();
    let began = std::time::Instant::now();
    let _ = speccoder_runtime::plan_lint::lints(opened.adapter, &opened.snapshot, r.path());
    assert!(began.elapsed().as_secs_f64() < 1.0, "{:?}", began.elapsed());
}
