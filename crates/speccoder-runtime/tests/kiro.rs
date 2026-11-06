//! A Kiro spec run end to end with a scripted model: each requirement's
//! criteria become its oracle, a sub-task is judged by the criteria it
//! cites, its parent is closed with its last required sub-task, and Kiro's
//! own ticks are labelled for what they are.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::spec::FormatKind;
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"./accept.sh\", \"--\"]]\n\n[acceptance]\nheld_out = false\nstrength = false\n";

/// One test: its file must exist, then "the pattern is in src/lib.rs".
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\n[ -f \"$1\" ] || { echo \"error: no test file $1\" >&2; exit 2; }\nshift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const TASKS: &str = ".kiro/specs/issue-triage/tasks.md";

fn fixture(project: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-kiro/tests/fixtures")
        .join(project)
}

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

/// A fixture project as a committed repository with code, a check and `accept.sh`.
fn repo(project: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture(project).join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
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

fn test_file(path: &str, criterion: &str, pattern: &str) -> Value {
    json!({
        "path": path,
        "content": format!("checks {pattern}\n"),
        "scenarios": [criterion],
        "argv": ["./accept.sh", "--", path, pattern],
    })
}

/// Requirement 1's oracle: one test per criterion.
fn requirement_1() -> Value {
    json!({
        "files": [
            test_file("tests/acceptance_1_1.txt", "1.1", "fn crash"),
            test_file("tests/acceptance_1_2.txt", "1.2", "fn request"),
            test_file("tests/acceptance_1_3.txt", "1.3", "fn needs_triage"),
        ],
        "argv": [],
        "notes": [],
        "expectations": [
            { "scenario": "1.1", "given": "an issue mentioning crash", "expect": "the label bug" },
            { "scenario": "1.2", "given": "an issue saying please add", "expect": "the label enhancement" },
            { "scenario": "1.3", "given": "no rule matches", "expect": "the label needs-triage" },
        ],
        "untestable": [],
    })
}

/// An attempt that adds `lines` to src/lib.rs and reports done.
fn implements(f: &FakeProvider, lines: &str) {
    let now = "pub fn ok() {}";
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": now, "new_string": format!("{now}\n{lines}") }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
}

fn run(r: &Path, state: &Path, task: &str, f: &Arc<FakeProvider>) -> CommandResult {
    let d: Arc<dyn ModelProvider> = f.clone();
    speccoder_runtime::run_with(r, None, task, state, d, None)
}

fn report(res: &CommandResult) -> &RunReport {
    match res.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", res.message()),
    }
}

fn on_branch(r: &Path, file: &str) -> String {
    git(r, &["show", &format!("speccoder/issue-triage:{file}")])
}

#[test]
fn a_kiro_spec_reads_as_requirements_criteria_and_tasks() {
    let r = repo("issue-triage");
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    assert_eq!(opened.adapter.kind(), FormatKind::Kiro);
    let f = opened.snapshot.feature.as_ref().unwrap();
    assert_eq!(f.feature.name, "issue-triage");
    let tasks: Vec<(&str, &Vec<String>)> = f
        .tasks
        .as_ref()
        .unwrap()
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), &t.scenarios))
        .collect();
    assert_eq!(
        tasks,
        [
            ("1", &vec![]),
            ("2.1", &vec!["1.1".to_owned()]),
            ("2.2", &vec!["1.2".to_owned(), "1.3".to_owned()]),
            ("3", &vec![]),
            ("4", &vec!["2.1".to_owned()]),
        ],
        "a whole requirement is its criteria"
    );
    let stories: Vec<String> = opened
        .adapter
        .oracle_requirements(&opened.snapshot)
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(
        stories,
        [
            "Requirement 1: Automatic labels",
            "Requirement 2: Audit log"
        ]
    );
    assert!(
        opened
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.message == "optional task 2.3 is not run"),
        "{:?}",
        opened.snapshot.diagnostics
    );

    // cc-sdd: sub-tasks found by number, `(P)` and `_Depends:_` read.
    let r = repo("photo-albums");
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    let tasks = opened.snapshot.feature.unwrap().tasks.unwrap().tasks;
    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["1.1", "1.2", "2.1", "3"]);
    assert!(tasks[0].parallel);
    assert_eq!(tasks[2].depends_on, ["1.1"]);
}

#[test]
fn a_sub_task_is_judged_by_the_criteria_it_cites_and_closes_its_parent() {
    let r = repo("issue-triage");
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    // 2.1 writes Requirement 1's oracle once, then turns 1.1.
    f.will_structure(requirement_1());
    implements(&f, "pub fn crash() {}");
    let res = run(r.path(), state.path(), "2.1", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::Verified));
    let tasks = on_branch(r.path(), TASKS);
    assert!(tasks.contains("- [x] 2.1 Label crashes"), "{tasks}");
    assert!(
        tasks.contains("- [ ] 2. Implement"),
        "2.2 is still open: {tasks}"
    );

    // 2.2 reuses the oracle and turns 1.2 and 1.3: the parent closes with it.
    let f = FakeProvider::new();
    implements(&f, "pub fn request() {}\npub fn needs_triage() {}");
    let res = run(r.path(), state.path(), "2.2", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::Verified));
    assert_eq!(
        f.asked().iter().filter(|a| a.tools.is_empty()).count(),
        0,
        "no second author call: the requirement's oracle exists"
    );
    let after = on_branch(r.path(), TASKS);
    let before = fs::read_to_string(r.path().join(TASKS)).unwrap();
    let changed: Vec<(&str, &str)> = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        changed,
        [
            (
                "- [ ] 2. Implement the labelling rules",
                "- [x] 2. Implement the labelling rules"
            ),
            (
                "  - [ ] 2.1 Label crashes as bugs in `src/lib.rs`",
                "  - [x] 2.1 Label crashes as bugs in `src/lib.rs`"
            ),
            (
                "  - [ ] 2.2 Label requests and the rest in `src/lib.rs`",
                "  - [x] 2.2 Label requests and the rest in `src/lib.rs`"
            ),
        ],
        "the ticks, and nothing else: 2.3* stays open"
    );
}

#[test]
fn kiro_s_own_ticks_are_ticked_without_evidence() {
    let r = repo("issue-triage");
    let res = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(rep)) = res.data() else {
        panic!("{}", res.message());
    };
    let state = |id: &str| {
        rep.tasks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.state.clone())
    };
    assert_eq!(state("1").as_deref(), Some("ticked_without_evidence"));
    assert_eq!(state("2.1").as_deref(), Some("ready"));
}

#[test]
fn each_task_reads_the_steering_the_design_and_its_requirements() {
    let r = repo("issue-triage");
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    let b = opened.adapter.bundle(&opened.snapshot, "2.1").unwrap();
    assert!(
        b.constitution.contains("Rust, standard library only"),
        "{}",
        b.constitution
    );
    assert!(
        !b.constitution.contains("coloured chips"),
        "fileMatch steering is not always read"
    );
    assert!(
        b.plan.contains("the first rule that matches wins"),
        "{}",
        b.plan
    );
    assert!(
        b.story.contains("Requirement 1: Automatic labels"),
        "{}",
        b.story
    );
    assert!(
        !b.story.contains("Audit log"),
        "only the requirements it cites"
    );
}

#[test]
fn a_bugfix_spec_stops_before_any_model_call() {
    let r = repo("bugfix");
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let res = run(r.path(), state.path(), "1", &f);
    assert_ne!(res.status(), CommandStatus::Ok);
    assert!(res.message().contains("bugfix spec"), "{}", res.message());
    assert!(f.asked().is_empty());
}
