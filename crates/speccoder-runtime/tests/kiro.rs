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
    // The oracle's author reads the design's Correctness Properties.
    let author = f.asked().into_iter().find(|a| a.tools.is_empty()).unwrap();
    assert!(
        author
            .prompt
            .to_string()
            .contains("Property 1: One label per issue"),
        "{}",
        author.prompt
    );
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
    // Kiro's conditions, stated; references inlined from inside the project.
    assert!(
        b.constitution.contains(
            "## .kiro/steering/ui.md — when you change files matching `web/**`\n\n# UI\n\n- Labels are shown as coloured chips."
        ),
        "{}",
        b.constitution
    );
    assert!(
        b.constitution
            .contains("## .kiro/steering/audit.md — when the work is about: the audit log"),
        "{}",
        b.constitution
    );
    assert!(
        !b.constitution.contains("Written by hand"),
        "manual is left out"
    );
    assert!(
        b.constitution
            .contains("Errors are values, never panics.\nNames are full words."),
        "{}",
        b.constitution
    );
    assert!(b.constitution.contains("#[[file:../outside.md]]"));
    let steering: Vec<&str> = opened
        .snapshot
        .diagnostics
        .iter()
        .filter(|d| d.code == "kiro.steering")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        steering,
        [
            "#[[file:../outside.md]] does not resolve inside the project; the model reads it as written",
            "release.md is not read (manual: read only when a person names it)",
        ]
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

/// The bugfix spec over the bug fixture's seeded code (`save` panics on
/// an empty title; `check.sh` stands in for a test runner).
fn bugfix_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let seeded = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bug/seeded");
    for from in [seeded, fixture("bugfix")] {
        assert!(
            Command::new("cp")
                .arg("-R")
                .arg(from.join("."))
                .arg(r)
                .status()
                .unwrap()
                .success()
        );
    }
    // Kiro only.
    fs::remove_dir_all(r.join(".specify")).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

const BUGFIX_TASKS: &str = ".kiro/specs/empty-title/tasks.md";

/// An approver who confirms the reproductions shown, and is asked nothing else.
#[derive(Debug)]
struct Confirms;
impl speccoder_runtime::approvals::Approver for Confirms {
    fn ask(
        &self,
        _: &speccoder_runtime::approvals::Presentation,
    ) -> speccoder_runtime::approvals::Answer {
        panic!("no call should need approval")
    }
    fn confirm(
        &self,
        _: &speccoder_runtime::approvals::ConfirmRequest,
    ) -> Option<speccoder_runtime::approvals::ConfirmAnswer> {
        Some(speccoder_runtime::approvals::ConfirmAnswer::Confirm)
    }
}

fn the_bug(r: &CommandResult) -> &speccoder_core::bug::BugReport {
    match r.data() {
        Some(CommandData::Bug(b)) => b,
        other => panic!("{:?} {} → {other:?}", r.status(), r.message()),
    }
}

/// The reproduction author: E1 (Expected 2.1) and U1 (Unchanged 3.1);
/// a held-out variant of E1.
fn reproductions(f: &FakeProvider) {
    let file = |path: &str, statement: &str, pattern: &str| json!({ "path": path, "content": format!("expect: {pattern}\n"), "statement": statement, "test": pattern, "argv": ["./check.sh", "--", path] });
    f.will_structure(json!({ "files": [
        file("tests/repro_e1.txt", "E1", "refuses_empty"),
        file("tests/repro_u1.txt", "U1", "saves_title"),
    ], "notes": [] }));
    f.will_structure(json!({ "files": [{ "path": "tests/held_e1.txt", "content": "expect: refuses_blank\n", "scenarios": ["E1"], "argv": ["./check.sh", "--", "tests/held_e1.txt"], "composition": false }], "notes": [] }));
}

/// The fix, and a special case that knows only the reproduction's literal.
fn fixes(f: &FakeProvider) {
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "// saves_title\n// refuses_empty refuses_blank\npub fn save(title: &str) -> Result<char, String> {\n    title.chars().next().ok_or_else(|| \"a title is needed\".to_owned())\n}\n" }),
    );
    f.will_structure(json!({ "summary": "refuse an empty title", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    f.will_structure(json!({ "variants": [{ "kind": "special_case", "scenario": "E1", "rationale": "recognises the empty string only", "files": [{ "path": "src/lib.rs", "content": "// saves_title\n// refuses_empty\npub fn save(title: &str) -> char { title.chars().next().unwrap_or('?') }\n" }] }], "notes": [] }));
}

#[test]
fn a_bugfix_spec_reads_as_the_bugs_triad() {
    let r = bugfix_repo();
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    let d = opened
        .snapshot
        .diagnostics
        .iter()
        .find(|d| d.code == "kiro.bugfix")
        .unwrap_or_else(|| panic!("{:?}", opened.snapshot.diagnostics));
    assert_eq!(
        d.message,
        "a bugfix spec — Current 1.1, Expected 2.1 (E1), Unchanged 3.1 (U1); it runs as a bug: reproduced, confirmed by you, fixed, the revert rejected"
    );
    assert!(opened.adapter.blocking(&opened.snapshot).is_empty());
    let home = opened.adapter.bug_unit(r.path()).unwrap();
    let read = opened
        .adapter
        .read_bug(r.path(), &home, "empty-title")
        .unwrap();
    assert_eq!(
        read.current,
        ["WHEN a note is saved with an empty title THEN the system panics"],
        "wrapped lines joined"
    );
    assert_eq!(read.unchanged.len(), 1);
    assert_eq!(
        opened.adapter.bug_spec(&opened.snapshot).as_deref(),
        Some("empty-title")
    );

    // Kiro's analysis phase: only bugfix.md, still a spec to run.
    for f in ["design.md", "tasks.md"] {
        fs::remove_file(r.path().join(".kiro/specs/empty-title").join(f)).unwrap();
    }
    let opened = speccoder_runtime::format::open(r.path(), None).unwrap();
    assert_eq!(
        opened.adapter.bug_spec(&opened.snapshot).as_deref(),
        Some("empty-title")
    );
}

#[test]
fn a_bugfix_spec_runs_on_the_bug_rung_and_its_fix_closes_kiros_tasks() {
    let r = bugfix_repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    reproductions(&f);
    fixes(&f);
    let d: Arc<dyn ModelProvider> = f.clone();
    let options = speccoder_runtime::RunOptions {
        approver: Some(Arc::new(Confirms)),
        ..speccoder_runtime::RunOptions::default()
    };
    // `speccoder run` with the bugfix spec selected: the bug, not its tasks.
    let res = speccoder_runtime::run_feature_with(r.path(), None, state.path(), &d, None, &options);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let b = the_bug(&res);
    assert_eq!(b.home.kind, speccoder_core::bug::HomeKind::NativeKiroBugfix);
    assert_eq!(
        (b.unit.as_str(), b.task.as_str()),
        ("empty-title", "bug:empty-title")
    );
    assert_eq!(b.verdict, Some(Verdict::Verified));
    let commit = b.commit.clone().unwrap();
    let message = git(r.path(), &["log", "-1", "--format=%B", &commit]);
    assert!(
        message.contains("Bug: fixed (native_kiro_bugfix)"),
        "{message}"
    );
    // The verdict commit ticks Kiro's tasks, and nothing else of the spec.
    let before = fs::read_to_string(r.path().join(BUGFIX_TASKS)).unwrap();
    let after = git(r.path(), &["show", &format!("{commit}:{BUGFIX_TASKS}")]);
    let changed: Vec<&str> = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .map(|(_, b)| b)
        .collect();
    assert_eq!(
        changed,
        [
            "- [x] 1. Write bug condition exploration test",
            "- [x] 2. Write preservation property tests",
            "- [x] 3. Fix the empty title",
            "  - [x] 3.1 Refuse an empty title in `src/lib.rs`",
            "  - [x] 3.2 Verify the exploration test now passes",
            "- [x] 4. Checkpoint - Ensure all tests pass",
        ],
        "{after}"
    );
    assert!(after.contains("- [ ]* 3.3"), "an optional task stays open");
    for f in ["bugfix.md", "design.md"] {
        let p = format!(".kiro/specs/empty-title/{f}");
        assert_eq!(
            git(r.path(), &["show", &format!("{commit}:{p}")]),
            fs::read_to_string(r.path().join(&p)).unwrap()
        );
    }
    for id in ["1", "3", "3.1", "4"] {
        assert!(
            message.contains(&format!("Closes-Task: empty-title/{id}\n")),
            "{message}"
        );
    }
    // The report reads each closed task as verified by the bug's verdict.
    let wt = git(r.path(), &["worktree", "list", "--porcelain"]);
    let wt = wt
        .split("\n\n")
        .find(|w| w.contains("branch refs/heads/speccoder/empty-title"))
        .and_then(|w| w.lines().next()?.strip_prefix("worktree "))
        .unwrap_or_else(|| panic!("{wt}"))
        .to_owned();
    let rep = speccoder_runtime::status::report(Path::new(&wt), Some("empty-title"));
    let Some(CommandData::Report(rep)) = rep.data() else {
        panic!("{}", rep.message());
    };
    for t in &rep.tasks {
        assert_eq!(t.state, "verified", "{t:?}");
    }
}

#[test]
fn a_new_bug_in_a_kiro_project_is_a_bugfix_spec_to_write() {
    let r = bugfix_repo();
    let state = tempfile::tempdir().unwrap();
    let res = speccoder_runtime::bug::bug_with(
        r.path(),
        &speccoder_runtime::bug::Request {
            symptom: "the list is empty".to_owned(),
            expected: vec!["the list shows the notes".to_owned()],
            ..speccoder_runtime::bug::Request::default()
        },
        speccoder_core::actions::Via::Terminal,
        state.path(),
        None,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(res.code().map(|c| c.name), Some("BUG-NO-UNIT"));
    assert!(
        res.message()
            .ends_with("a Kiro bugfix spec in .kiro/specs/<name>/"),
        "{}",
        res.message()
    );
}

#[test]
fn a_cc_sdd_spec_whose_tasks_are_not_approved_does_not_run() {
    let r = repo("photo-albums");
    let spec = r.path().join(".kiro/specs/photo-albums/spec.json");
    let approved = fs::read_to_string(&spec).unwrap();
    assert!(
        speccoder_runtime::format::open(r.path(), None)
            .unwrap()
            .snapshot
            .diagnostics
            .iter()
            .all(|d| d.code != "kiro.unapproved"),
        "approved: it runs"
    );
    fs::write(
        &spec,
        approved.replace(
            "\"tasks\": { \"approved\": true }",
            "\"tasks\": { \"approved\": false }",
        ),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "tasks not approved"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let res = run(r.path(), state.path(), "1.1", &f);
    assert_ne!(res.status(), CommandStatus::Ok);
    assert!(
        res.message()
            .contains("cc-sdd has not approved this spec's tasks (spec.json)"),
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty(), "no model call");
}

#[test]
fn cc_sdd_group_headings_without_a_parent_line_run_and_tick() {
    let r = repo("photo-albums");
    let tasks = ".kiro/specs/photo-albums/tasks.md";
    fs::write(
        r.path().join(tasks),
        "# Implementation Plan\n\n## 1. Album store\n\n- [ ] 1.1 Create the album model in `src/lib.rs`\n  - _Requirements: 1.1_\n- [ ] 1.2 Delete empty albums in `src/lib.rs`\n  - _Requirements: 1.2_\n\n## 2. Photos\n\n- [ ] 2.1 List photos in `src/lib.rs`\n  - _Requirements: 2.1_\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "grouped by headings"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(json!({
        "files": [
            test_file("tests/acceptance_1_1.txt", "1.1", "fn created_on"),
            test_file("tests/acceptance_1_2.txt", "1.2", "fn delete_empty"),
        ],
        "argv": [],
        "notes": [],
        "expectations": [
            { "scenario": "1.1", "given": "a new album", "expect": "its creation date" },
            { "scenario": "1.2", "given": "an empty album", "expect": "it can be deleted" },
        ],
        "untestable": [],
    }));
    implements(&f, "pub fn created_on() {}");
    let res = run(r.path(), state.path(), "1.1", &f);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::Verified));
    let after = git(
        r.path(),
        &["show", &format!("speccoder/photo-albums:{tasks}")],
    );
    let before = fs::read_to_string(r.path().join(tasks)).unwrap();
    assert_eq!(
        after,
        before.replace("- [ ] 1.1 Create", "- [x] 1.1 Create"),
        "one box, and the heading stays a heading"
    );
}
