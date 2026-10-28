//! Every task finds its doer (spec 106): a planning task runs with the
//! planning-task profile and ticks `regression_only` (`planning_work`); a
//! `[YOU]` task is never attempted. The owner's POC shape: T002 names only
//! `/specs/001-x/contracts/`.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::regression::RegressionReason;
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::{RunOptions, run_feature_with, run_with};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";

const TASKS: &str = "# Tasks: X\n\n## Phase 1: Setup\n\n- [ ] T001 Create the parser module in `src/parse.rs`\n- [ ] T002 Create the feature documentation scaffold under `/specs/001-x/contracts/` for input/output interface definitions\n- [ ] T003 Define the table type in `src/lib.rs`\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
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

/// A committed Spec Kit project whose feature `001-x` has `tasks`.
fn repo(tasks: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join(".specify"))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    let unit = r.join("specs/001-x");
    fs::create_dir_all(&unit).unwrap();
    fs::write(unit.join("spec.md"), "# Spec\n\nTables become CSV.\n").unwrap();
    fs::write(unit.join("plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::write(unit.join("tasks.md"), tasks).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "t"]);
    git(r, &["config", "user.email", "t@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn done(summary: &str) -> Value {
    json!({ "summary": summary, "status": "done", "files_changed": [], "checks_run": [], "notes": [] })
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[test]
fn a_planning_task_writes_its_docs_and_never_a_judged_file() {
    let r = repo(&TASKS.replace("- [ ] T001", "- [X] T001"));
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "specs/001-x/spec.md", "content": "# Spec\n\nRewritten.\n" }),
    );
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "specs/001-x/contracts/cli.md", "content": "# CLI\n\nstdin → stdout.\n" }),
    );
    f.will_structure(done("wrote the contract"));

    let res = run_with(r.path(), None, "T002", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::RegressionOnly));
    assert_eq!(
        rep.regression.as_ref().map(|r| &r.reason),
        Some(&RegressionReason::PlanningWork)
    );
    assert!(
        rep.ticked && rep.integrity.is_empty(),
        "{:?}",
        rep.integrity
    );
    // The judged file was refused; the design document written.
    let decisions: Vec<&str> = rep
        .tool_calls
        .iter()
        .map(|c| c["decision"].as_str().unwrap())
        .collect();
    assert_eq!(decisions, ["deny", "allow"]);
    let wt = Path::new(&rep.worktree);
    assert_eq!(
        fs::read_to_string(wt.join("specs/001-x/spec.md")).unwrap(),
        "# Spec\n\nTables become CSV.\n"
    );
    // The document is in the tick commit, with the tick and its trailer.
    let commit = rep.commit.clone().unwrap();
    let files = git(wt, &["show", "--name-only", "--format=", &commit]);
    assert!(files.contains("specs/001-x/contracts/cli.md"), "{files}");
    let message = git(wt, &["log", "-1", "--format=%B", &commit]);
    assert!(
        message.contains("Regression-Only: planning_work"),
        "{message}"
    );
    // The model was told it is a planning task, and where its documents go.
    let prompt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(prompt.contains("# A planning task"), "{prompt}");
    assert!(prompt.contains("under `specs/001-x/`"), "{prompt}");
}

#[test]
fn a_planning_task_that_writes_nothing_fails() {
    let r = repo(&TASKS.replace("- [ ] T001", "- [X] T001"));
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[budget]\nmax_attempts = 1\n"),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "one attempt"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(done("nothing to do"));
    let res = run_with(r.path(), None, "T002", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::VerificationFailed);
    match &report(&res).verdict {
        Some(Verdict::Failed { reasons }) => assert!(
            reasons
                .iter()
                .any(|r| r == "wrote no planning file under specs/001-x/"),
            "{reasons:?}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_poc_runs_code_then_planning_then_code_without_a_command() {
    let r = repo(TASKS);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    // T001: code.
    f.will_call_tool(
        "a1",
        "write_file",
        json!({ "path": "src/parse.rs", "content": "pub fn parse() {}\n" }),
    );
    f.will_structure(done("parser module"));
    // T002: planning.
    f.will_call_tool(
        "b1",
        "write_file",
        json!({ "path": "specs/001-x/contracts/cli.md", "content": "# CLI\n" }),
    );
    f.will_structure(done("contract"));
    // T003: code.
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub struct Table;" }),
    );
    f.will_structure(done("table type"));
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{res:?}");
    };
    let ticked: Vec<(&str, Option<&str>)> = fr
        .attempts
        .iter()
        .filter(|a| a.ticked)
        .map(|a| {
            (
                a.task.as_str(),
                a.regression.as_ref().map(|r| r.reason.name()),
            )
        })
        .collect();
    assert_eq!(
        ticked,
        [
            ("T001", Some("no_story")),
            ("T002", Some("planning_work")),
            ("T003", Some("no_story"))
        ]
    );
}

/// A person at the terminal answering each stop from a script.
#[derive(Debug)]
struct Person {
    answers: std::sync::Mutex<Vec<speccoder_runtime::approvals::StopAnswer>>,
    shown: std::sync::Mutex<Vec<speccoder_runtime::approvals::StopRequest>>,
}

impl Person {
    fn answering(answers: Vec<speccoder_runtime::approvals::StopAnswer>) -> Arc<Self> {
        Arc::new(Self {
            answers: std::sync::Mutex::new(answers),
            shown: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl speccoder_runtime::approvals::Approver for Person {
    fn ask(
        &self,
        p: &speccoder_runtime::approvals::Presentation,
    ) -> speccoder_runtime::approvals::Answer {
        panic!("no call needs approval here: {:?}", p.request.tool)
    }

    fn at_stop(
        &self,
        stop: &speccoder_runtime::approvals::StopRequest,
    ) -> Option<speccoder_runtime::approvals::StopAnswer> {
        self.shown.lock().unwrap().push(stop.clone());
        let mut a = self.answers.lock().unwrap();
        (!a.is_empty()).then(|| a.remove(0))
    }
}

fn blocked(f: &FakeProvider, id: &str, why: &str) {
    f.will_call_tool(id, "report_blocked", json!({ "reason": why }));
}

fn options(person: &Arc<Person>) -> RunOptions {
    RunOptions {
        approver: Some(Arc::clone(person) as _),
        ..RunOptions::default()
    }
}

fn state_of(r: &Path, task: &str) -> String {
    match speccoder_runtime::status::report(r, None).data() {
        Some(CommandData::Report(rep)) => rep
            .tasks
            .iter()
            .find(|t| t.id == task)
            .map(|t| t.state.clone())
            .unwrap_or_default(),
        other => panic!("{other:?}"),
    }
}

const TWO: &str = "# Tasks: X\n\n## Phase 1: Setup\n\n- [ ] T001 Wire the release in `src/lib.rs`\n- [ ] T002 Define the table type in `src/lib.rs`\n";

#[test]
fn a_hint_at_the_stop_starts_a_new_attempt_that_reads_it() {
    let r = repo(TWO);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    blocked(&f, "a1", "needs a GitHub repository");
    // The new attempt, then T002.
    f.will_call_tool(
        "a2",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub const REPO: &str = \"octo/md2csv\";" }),
    );
    f.will_structure(done("release wired"));
    f.will_call_tool(
        "b1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub struct Table;" }),
    );
    f.will_structure(done("table type"));
    let person = Person::answering(vec![speccoder_runtime::approvals::StopAnswer::Hint(
        "the repository is octo/md2csv".to_owned(),
    )]);
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options(&person));
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let shown = person.shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].task, "001-x/T001");
    assert_eq!(shown[0].claim.as_deref(), Some("needs a GitHub repository"));
    assert_eq!(shown[0].dependents, ["T002"]);
    // The second attempt's prompt carries the hint as the person's note.
    let retried = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .filter(|p| p.contains("Notes from a person"))
        .count();
    assert!(retried >= 1, "the hint never reached an attempt");
    assert_eq!(state_of(r.path(), "T001"), "regression_only");
    assert_eq!(state_of(r.path(), "T002"), "regression_only");
}

#[test]
fn i_did_it_at_the_stop_ticks_it_attested_and_the_run_goes_on() {
    let r = repo(TWO);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    blocked(&f, "a1", "needs a GitHub repository");
    f.will_call_tool(
        "b1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub struct Table;" }),
    );
    f.will_structure(done("table type"));
    let person = Person::answering(vec![speccoder_runtime::approvals::StopAnswer::Done]);
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options(&person));
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(state_of(r.path(), "T001"), "attested");
    assert_eq!(state_of(r.path(), "T002"), "regression_only");
}

#[test]
fn without_a_person_the_stop_waits_with_its_three_actions() {
    let r = repo(TWO);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    blocked(&f, "a1", "needs a GitHub repository");
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let res = speccoder_runtime::next::attach(res, None);
    let next: Vec<&str> = res.next().iter().map(|s| s.command.as_str()).collect();
    assert_eq!(
        next,
        [
            "speccoder task done T001",
            "speccoder task hint T001 \"…\"",
            "speccoder task skip T001 --reason \"…\""
        ]
    );
    // Needs You: the agent's reason, and the three actions as typed ids.
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{inbox:?}");
    };
    let item = i.items.iter().find(|i| i.kind == "yours").unwrap();
    assert_eq!(item.task.as_deref(), Some("T001"));
    assert_eq!(item.detail, "needs a GitHub repository");
    let kinds: Vec<&str> = item.actions.iter().map(|a| a.id.kind.as_str()).collect();
    assert_eq!(kinds, ["done", "hint", "skip"]);
    assert_eq!(item.actions[2].why, "runs pass it by; T002 stays blocked");
    // The board: its lane is yours until it is done, skipped or retried.
    match speccoder_runtime::status::report(r.path(), None).data() {
        Some(CommandData::Report(rep)) => {
            assert_eq!(rep.tasks[0].lane, speccoder_core::lane::Lane::Yours);
            assert_eq!(rep.tasks[1].lane, speccoder_core::lane::Lane::Code);
        }
        other => panic!("{other:?}"),
    }
}

const YOU: &str = "# Tasks: X\n\n## Phase 1: Setup\n\n- [X] T001 Create `src/lib.rs`\n- [ ] T002 [YOU] Create the GitHub repository\n- [ ] T003 Define the table type in `src/lib.rs`\n";

#[test]
fn a_you_task_is_never_attempted_it_waits_or_the_person_answers() {
    let r = repo(YOU);
    let state = tempfile::tempdir().unwrap();
    // Nobody to ask: it waits, named, with its three actions; no model call.
    let (f, d) = fake();
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("TASK-YOURS"));
    assert!(
        res.message().starts_with("T002 is yours"),
        "{}",
        res.message()
    );
    assert_eq!(
        res.fix(),
        [
            "speccoder task done T002",
            "speccoder task hint T002 \"…\"",
            "speccoder task skip T002 --reason \"…\""
        ]
    );
    assert!(f.asked().is_empty());
    // At the terminal: "I did it", and the run goes on with T003.
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub struct Table;" }),
    );
    f.will_structure(done("table type"));
    let person = Person::answering(vec![speccoder_runtime::approvals::StopAnswer::Done]);
    let res = run_feature_with(r.path(), None, state.path(), &d, None, &options(&person));
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let shown = person.shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].claim, None);
    assert_eq!(state_of(r.path(), "T002"), "attested");
    assert_eq!(state_of(r.path(), "T003"), "regression_only");
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!("{res:?}");
    };
    let tried: Vec<&str> = fr.attempts.iter().map(|a| a.task.as_str()).collect();
    assert_eq!(tried, ["T003"], "only T003 was attempted");
}
