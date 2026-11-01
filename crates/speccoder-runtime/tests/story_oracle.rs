//! A story's oracle end to end, with a scripted model and real runs: the
//! story's acceptance tests are written once, before its first code task
//! implements anything, and committed (`Oracle: US1`); every task of the
//! story is judged by the scenarios it turns green; the story's last task
//! closes it or stops (`STORY-OPEN`) with a ruling per scenario.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::story::{Progress, ScenarioState};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"./accept.sh\", \"--\"]]\n\n[acceptance]\nheld_out = false\nstrength = false\n";

/// One test: its file must exist, then "the pattern is in src/lib.rs",
/// with libtest's summary line.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\n[ -f \"$1\" ] || { echo \"error: no test file $1\" >&2; exit 2; }\nshift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Tables (Priority: P1)\n\nTables convert.\n\n**Independent Test**: run the command with a table.\n\n**Acceptance Scenarios**:\n\n1. **Given** a table, **When** converted, **Then** alpha holds.\n2. **Given** a bad table, **When** converted, **Then** beta holds.\n3. **Given** two tables, **When** converted, **Then** gamma holds.\n";

const TASKS: &str = "# Tasks\n\n## Phase 3: User Story 1 - Tables\n\n- [ ] T001 [US1] Parse tables in src/lib.rs\n- [ ] T002 [US1] Report errors in src/lib.rs (scenarios US1-2)\n- [ ] T003 [US1] Wire the command line in src/lib.rs\n- [ ] T004 [US1] Document it in specs/001-graph/quickstart.md\n";

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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo_with(tasks: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("specs/001-graph")).unwrap();
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::write(
        r.join(".specify/memory/constitution.md"),
        "# Constitution\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::write(r.join("specs/001-graph/tasks.md"), tasks).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn repo() -> tempfile::TempDir {
    repo_with(TASKS)
}

fn test_file(path: &str, scenario: &str, pattern: &str) -> Value {
    json!({
        "path": path,
        "content": format!("checks {pattern}\n"),
        "scenarios": [scenario],
        "argv": ["./accept.sh", "--", path, pattern],
    })
}

/// The story's oracle as the author writes it: one test per scenario,
/// each with what it must hold.
fn oracle_answer() -> Value {
    json!({
        "files": [
            test_file("tests/acceptance_us1_alpha.txt", "US1-1", "fn alpha"),
            test_file("tests/acceptance_us1_beta.txt", "US1-2", "fn beta"),
            test_file("tests/acceptance_us1_gamma.txt", "US1-3", "fn gamma"),
        ],
        "argv": [],
        "notes": [],
        "expectations": [
            { "scenario": "US1-1", "given": "a table", "expect": "alpha holds" },
            { "scenario": "US1-2", "given": "a bad table", "expect": "beta holds" },
            { "scenario": "US1-3", "given": "two tables", "expect": "gamma holds" },
        ],
        "untestable": [],
    })
}

fn done() -> Value {
    json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] })
}

/// An attempt that adds `line` to src/lib.rs and reports done.
fn implements(f: &FakeProvider, line: &str) {
    let now = "pub fn ok() {}";
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": now, "new_string": format!("{now}\n{line}") }),
    );
    f.will_structure(done());
}

fn run(r: &Path, state: &Path, task: &str, f: &Arc<FakeProvider>) -> CommandResult {
    let d: Arc<dyn ModelProvider> = f.clone();
    speccoder_runtime::run_with(r, None, task, state, d, None)
}

fn report(res: &CommandResult) -> &RunReport {
    match res.data() {
        Some(CommandData::Run(r)) => r,
        other => panic!("{} → {other:?}", res.message()),
    }
}

fn on_branch(r: &Path, args: &[&str]) -> String {
    let mut all = args.to_vec();
    all.push("speccoder/001-graph");
    git(r, &all)
}

/// Merges a task's tick on the unit branch back onto `main`'s view of
/// tasks.md for the next task's run (the worktree is the unit branch).
fn trailer(r: &Path, key: &str) -> String {
    on_branch(
        r,
        &[
            "log",
            "-n1",
            &format!("--format=%(trailers:key={key},valueonly)"),
        ],
    )
    .trim()
    .to_owned()
}

#[test]
fn one_oracle_per_story_and_each_task_claims_what_it_turned() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    // T001: the story's oracle first (one author call), then the attempt.
    let f = FakeProvider::new();
    f.will_structure(oracle_answer());
    implements(&f, "pub fn alpha() {}");
    let res = run(r.path(), state.path(), "T001", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked, "{rep:?}");
    assert_eq!(
        rep.progress,
        Some(Progress::Verified {
            turned: vec!["US1-1".into()]
        })
    );
    let oracle = on_branch(
        r.path(),
        &["log", "--format=%s%n%(trailers)", "--grep=^Oracle: US1$"],
    );
    assert!(
        oracle.contains("Oracle-Scenarios: US1-1=red, US1-2=red, US1-3=red"),
        "{oracle}"
    );
    assert!(oracle.contains("Oracle-By: speccoder run"), "{oracle}");
    assert!(
        trailer(r.path(), "Scenarios").starts_with("US1-1@"),
        "{}",
        trailer(r.path(), "Scenarios")
    );

    // T002 (linked to US1-2): no author call; turning US1-2 green verifies it.
    let f = FakeProvider::new();
    implements(&f, "pub fn alpha() {}\npub fn beta() {}");
    let res = run(r.path(), state.path(), "T002", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(f.asked().len(), 2, "two implementer turns, no author call");
    // The implementer was told what is still red, what it must hold, what
    // this task must turn, and how to report a wrong test.
    let prompt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    for said in [
        "Still failing",
        "US1-2: a bad table → beta holds — tests/acceptance_us1_beta.txt",
        "This task must make these pass: US1-2.",
        "kind wrong_test",
    ] {
        assert!(prompt.contains(said), "{said} missing from {prompt}");
    }
    assert!(!prompt.contains("US1-1: a table"), "US1-1 is green already");
    assert_eq!(
        report(&res).progress,
        Some(Progress::Verified {
            turned: vec!["US1-2".into()]
        })
    );

    // The report: the strip with who turned what, and the checkpoint.
    let status = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(st)) = status.data() else {
        panic!("{}", status.message());
    };
    let story = st.stories.first().expect("US1 shown while open");
    let marks: Vec<(String, ScenarioState, Option<String>)> = story
        .scenarios
        .iter()
        .map(|m| (m.id.clone(), m.state, m.turned_by.clone()))
        .collect();
    assert_eq!(
        marks,
        vec![
            ("US1-1".into(), ScenarioState::Green, Some("T001".into())),
            ("US1-2".into(), ScenarioState::Green, Some("T002".into())),
            ("US1-3".into(), ScenarioState::Red, None),
        ]
    );
    assert_eq!(story.checkpoint.as_deref(), Some("T003"));
    assert_eq!(
        story.scenarios[0].contract.as_deref(),
        Some("a table → alpha holds")
    );
}

#[test]
fn a_task_that_turns_nothing_is_in_progress_and_a_regression_fails() {
    let r = repo_with(&TASKS.replace(" (scenarios US1-2)", ""));
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(oracle_answer());
    implements(&f, "// a helper, nothing a scenario sees");
    let res = run(r.path(), state.path(), "T001", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.progress, Some(Progress::InProgress));
    assert_eq!(
        rep.regression.as_ref().map(|g| g.reason.name()),
        Some("in_progress")
    );
    assert_eq!(trailer(r.path(), "Regression-Only"), "in_progress");
}

#[test]
fn the_story_s_last_task_must_close_it_or_stop_for_a_person() {
    let r = repo_with(
        &TASKS
            .replace("T002 [US1]", "T002 [P] [US1]")
            .replace("T003 [US1]", "T003 [P] [US1]"),
    );
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(oracle_answer());
    implements(&f, "pub fn alpha() {}");
    assert_eq!(
        run(r.path(), state.path(), "T001", &f).status(),
        CommandStatus::Ok
    );
    // T003 is the checkpoint (T004 is documentation): gamma alone leaves
    // US1-2 red — two attempts, then a person.
    let f = FakeProvider::new();
    implements(&f, "pub fn gamma() {}");
    implements(&f, "pub fn gamma() {}");
    let res = run(r.path(), state.path(), "T003", &f);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("STORY-OPEN"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("US1-2 (a bad table → beta holds)"),
        "{}",
        res.message()
    );
    assert!(
        res.fix()
            .iter()
            .any(|f| f.contains("task rule T003 --tests-wrong --scenario US1-2")),
        "{:?}",
        res.fix()
    );
    let saved = speccoder_runtime::attempts::list(r.path(), "001-graph");
    assert_eq!(
        saved.first().map(|a| a.story_open.clone()),
        Some(vec!["US1-2".to_owned()])
    );
    // The ruling is a person's decision, kept in speccoder's state.
    let ruled = speccoder_runtime::acceptance_at::rule_scenario(
        r.path(),
        None,
        "T003",
        "US1-2",
        "beta is not what the spec says",
        speccoder_core::actions::Via::Terminal,
        state.path(),
    );
    assert_eq!(ruled.status(), CommandStatus::Ok, "{}", ruled.message());
    assert!(ruled.message().contains("the next run writes them again"));

    // The next run writes US1-2's test again — only its file — and T003
    // closes the story with it.
    let f = FakeProvider::new();
    f.will_structure(json!({
        "files": [test_file("tests/acceptance_us1_beta.txt", "US1-2", "fn gamma")],
        "argv": [],
        "notes": [],
        "expectations": [{ "scenario": "US1-2", "given": "a bad table", "expect": "gamma holds" }],
        "untestable": [],
    }));
    implements(&f, "pub fn gamma() {}");
    let res = run(r.path(), state.path(), "T003", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(
        report(&res).progress,
        Some(Progress::Verified {
            turned: vec!["US1-2".into(), "US1-3".into()]
        })
    );
    let amended = on_branch(
        r.path(),
        &[
            "log",
            "-n1",
            "--format=%(trailers:key=Oracle-Amends,valueonly)",
            "--name-only",
            "--grep=^Oracle-Amends:",
        ],
    );
    let lines: Vec<&str> = amended.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines,
        vec!["US1-2", "tests/acceptance_us1_beta.txt"],
        "{amended}"
    );
}

#[test]
fn an_oracle_test_changed_on_the_branch_stops_the_task() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(oracle_answer());
    implements(&f, "pub fn alpha() {}");
    assert_eq!(
        run(r.path(), state.path(), "T001", &f).status(),
        CommandStatus::Ok
    );
    // Someone edits US1-2's frozen test on the branch, in its worktree.
    let list = git(r.path(), &["worktree", "list", "--porcelain"]);
    let wt = list
        .split("\n\n")
        .find(|w| w.contains("branch refs/heads/speccoder/001-graph"))
        .and_then(|w| w.lines().next()?.strip_prefix("worktree "))
        .map(std::path::PathBuf::from)
        .expect("the unit branch's worktree");
    fs::write(wt.join("tests/acceptance_us1_beta.txt"), "checks nothing\n").unwrap();
    git(&wt, &["commit", "-qam", "loosen a test"]);

    let f = FakeProvider::new();
    implements(&f, "pub fn beta() {}");
    let res = run(r.path(), state.path(), "T002", &f);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("ORACLE-TAMPERED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("tests/acceptance_us1_beta.txt"),
        "{}",
        res.message()
    );
    assert!(
        f.asked().is_empty(),
        "nothing implemented on a changed oracle"
    );
}

/// The test author was given for `task`'s run: the scenario texts.
fn given(r: &Path, task: &str) -> String {
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(json!({ "files": [], "argv": [], "notes": ["none"] }));
    let _ = run(r, state.path(), task, &f);
    f.asked()
        .first()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .unwrap_or_default()
}

/// The scenario ids under the prompt's scenarios heading.
fn judged(asked: &str) -> Vec<String> {
    let text = asked.replace("\\n", "\n");
    let Some(at) = text.find("# Scenarios") else {
        return Vec::new();
    };
    text[at..]
        .lines()
        .skip(1)
        .filter_map(|l| l.trim().strip_prefix("- "))
        .take_while(|l| l.starts_with("US"))
        .filter_map(|l| l.split(':').next().map(str::to_owned))
        .collect()
}

/// A Convergence task closes one gap the stories left: its own test,
/// judged by the scenario its line cites (`US1/AC3`, as Spec Kit's
/// converge writes it), seeing its phase's other gaps.
#[test]
fn a_convergence_task_is_judged_by_the_scenario_it_cites() {
    let tasks = format!(
        "{}\n## Phase 7: Convergence\n\n- [ ] T026 Route both tables in src/lib.rs per FR-008, US1/AC3 (partial)\n- [ ] T027 Keep cell whitespace in src/lib.rs per FR-005 (partial)\n",
        TASKS.replace("- [ ]", "- [x]")
    );
    let r = repo_with(&tasks);
    let asked = given(r.path(), "T026");
    assert_eq!(judged(&asked), ["US1-3"], "{asked}");
    for s in ["The phase's other tasks", "T027 Keep cell whitespace"] {
        assert!(asked.contains(s), "{s} missing from {asked}");
    }
}

/// A link to no scenario of its story, and a story with no code task, are
/// told before anything runs.
#[test]
fn the_plan_says_when_a_link_names_nothing_or_no_task_judges_a_story() {
    let tasks = TASKS.replace("(scenarios US1-2)", "(scenarios US1-2, US9-4)")
        + "\n## Phase 4: User Story 2 - Docs\n\n- [ ] T005 [US2] Describe it in specs/001-graph/quickstart.md\n";
    let r = repo_with(&tasks);
    let spec = format!(
        "{SPEC}\n### User Story 2 - Docs (Priority: P2)\n\nDocs.\n\n**Acceptance Scenarios**:\n\n1. **Given** docs, **When** read, **Then** they help.\n"
    );
    fs::write(r.path().join("specs/001-graph/spec.md"), spec).unwrap();
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    let lints = speccoder_runtime::plan_lint::lints(opened.adapter, &opened.snapshot, r.path());
    let codes: Vec<(&str, Vec<String>)> = lints.iter().map(|l| (l.code, l.tasks.clone())).collect();
    assert!(
        codes.contains(&("PLAN-SCENARIO-UNKNOWN", vec!["T002".to_owned()])),
        "{lints:?}"
    );
    assert!(
        codes.contains(&("PLAN-STORY-UNJUDGED", vec!["T005".to_owned()])),
        "{lints:?}"
    );
}
