//! What judges a task of a story, end to end with a scripted model: the
//! test author is given what the plan, the story's ticks and a person's
//! deferral decide — never every scenario of the story to choose from at
//! the checkpoint, and exactly the linked ones for a linked task.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use sha2::Digest as _;

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Tables (Priority: P1)\n\nTables convert.\n\n**Acceptance Scenarios**:\n\n1. **Given** a table, **When** converted, **Then** alpha-one happens.\n2. **Given** a bad table, **When** converted, **Then** beta-two happens.\n3. **Given** two tables, **When** converted, **Then** gamma-three happens.\n";

const TASKS: &str = "# Tasks\n\n## Phase 3: User Story 1 - Tables\n\n- [ ] T001 [P] [US1] Parse tables in src/lib.rs\n- [ ] T002 [P] [US1] Report errors in src/lib.rs (scenarios US1-2)\n- [ ] T003 [P] [US1] Wire the command line in src/lib.rs\n- [ ] T004 [P] [US1] Document it in specs/001-graph/quickstart.md\n";

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

fn repo() -> tempfile::TempDir {
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
    fs::write(r.join("specs/001-graph/tasks.md"), TASKS).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// The scenario texts the test author was given for `task`'s run.
fn given(r: &Path, task: &str) -> String {
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    // The author answers with nothing: the run then goes on without
    // acceptance tests; only what it was given matters here.
    f.will_structure(json!({ "files": [], "argv": [], "notes": ["none"] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_with(r, None, task, state.path(), d, None);
    if f.asked().is_empty() {
        eprintln!(
            "{task}: {} {}",
            res.message(),
            serde_json::to_string(&res).unwrap()
        );
    }
    f.asked()
        .first()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .unwrap_or_default()
}

/// The scenario ids under the prompt's scenarios heading: what judges it.
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

fn digest(id: &str, text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let hex: String = sha2::Sha256::digest(format!("{id}\0{words}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    hex[..12].to_owned()
}

#[test]
fn an_unlinked_task_mid_story_is_offered_the_story_and_sees_its_other_tasks() {
    let r = repo();
    let asked = given(r.path(), "T001");
    assert_eq!(judged(&asked), ["US1-1", "US1-2", "US1-3"], "{asked}");
    for s in ["The story's other tasks", "T002 Report errors", "T003 Wire"] {
        assert!(asked.contains(s), "{s} missing from {asked}");
    }
}

#[test]
fn a_linked_task_is_judged_by_exactly_its_links() {
    let r = repo();
    let asked = given(r.path(), "T002");
    assert_eq!(judged(&asked), ["US1-2"], "{asked}");
    assert!(
        asked.contains("Scenarios this task is judged by"),
        "{asked}"
    );
}

/// A Convergence task closes one gap the stories left: it is judged by the
/// scenario its line cites (`US1/AC3`, as Spec Kit's converge writes it),
/// never the author's pick of the spec, and sees its phase's other gaps.
#[test]
fn a_convergence_task_is_judged_by_the_scenario_it_cites() {
    let r = repo();
    let tasks = format!(
        "{}\n## Phase 7: Convergence\n\n- [ ] T026 Route both tables in src/lib.rs per FR-008, US1/AC3 (partial)\n- [ ] T027 Keep cell whitespace in src/lib.rs per FR-005 (partial)\n",
        TASKS.replace("- [ ]", "- [x]")
    );
    fs::write(r.path().join("specs/001-graph/tasks.md"), tasks).unwrap();
    git(r.path(), &["commit", "-qam", "converge"]);
    let asked = given(r.path(), "T026");
    assert_eq!(judged(&asked), ["US1-3"], "{asked}");
    for s in ["The phase's other tasks", "T027 Keep cell whitespace"] {
        assert!(asked.contains(s), "{s} missing from {asked}");
    }
}

/// A repository whose unit branch holds T001's tick verifying US1-1,
/// digested over `ticked_wording`; `main` has the spec as given.
fn ticked(spec: &str, ticked_wording: &str) -> tempfile::TempDir {
    let r = repo();
    fs::write(r.path().join("specs/001-graph/spec.md"), spec).unwrap();
    git(
        r.path(),
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-am",
            "the spec as it is now",
        ],
    );
    let alpha = speccoder_speckit::stories::scenario_refs(ticked_wording, Some("US1"))
        .into_iter()
        .find(|s| s.id == "US1-1")
        .unwrap();
    git(r.path(), &["checkout", "-q", "-b", "speccoder/001-graph"]);
    git(
        r.path(),
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            &format!(
                "T001 Parse tables\n\nSpec-Task: 001-graph/T001\nVerdict: verified\nScenarios: US1-1@{}\nRun: run_x\n",
                digest(&alpha.id, &alpha.text)
            ),
        ],
    );
    git(r.path(), &["checkout", "-q", "main"]);
    r
}

#[test]
fn the_checkpoint_is_judged_by_every_scenario_no_earlier_tick_verified() {
    let r = ticked(SPEC, SPEC);
    assert_eq!(judged(&given(r.path(), "T003")), ["US1-2", "US1-3"]);
    // Reworded since the tick: it no longer counts for US1-1.
    let reworded = SPEC.replace("alpha-one happens", "alpha-one happens twice");
    let r = ticked(&reworded, SPEC);
    assert_eq!(
        judged(&given(r.path(), "T003")),
        ["US1-1", "US1-2", "US1-3"]
    );
}

/// The real case: a mid-story task whose tests need later tasks is moved,
/// by a person, to the story's checkpoint — refused where nothing later
/// would judge, taken back with `--revoke`.
#[test]
fn a_person_defers_a_mid_story_task_to_the_checkpoint() {
    use speccoder_core::actions::Via;
    use speccoder_core::regression::RegressionReason;
    use speccoder_core::{CommandData, CommandStatus};
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let defer = |task: &str, revoke: bool| {
        speccoder_runtime::acceptance_at::defer(
            r.path(),
            None,
            task,
            revoke,
            Via::Terminal,
            state.path(),
        )
    };
    let at_checkpoint = defer("T003", false);
    assert_eq!(
        at_checkpoint.status(),
        CommandStatus::UsageError,
        "{}",
        at_checkpoint.message()
    );
    assert!(
        at_checkpoint.message().contains("checkpoint"),
        "{}",
        at_checkpoint.message()
    );
    let linked = defer("T002", false);
    assert_eq!(linked.status(), CommandStatus::UsageError);
    assert!(linked.message().contains("US1-2"), "{}", linked.message());

    let ok = defer("T001", false);
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    assert!(ok.message().contains("T003"), "{}", ok.message());

    // Deferred: no test author; the implementer's pass ticks it on the
    // regression checks, naming the checkpoint.
    let f = FakeProvider::new();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn ok() {}\npub fn parse() {}\n" }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_with(r.path(), None, "T001", state.path(), d, None);
    let Some(CommandData::Run(rep)) = res.data() else {
        panic!("{}", res.message());
    };
    assert!(rep.ticked, "{}", res.message());
    assert_eq!(
        rep.regression.as_ref().map(|g| g.reason.clone()),
        Some(RegressionReason::DeferredToCheckpoint {
            checkpoint: "T003".into()
        })
    );
    assert!(
        f.asked().iter().all(|a| !serde_json::to_string(&a.prompt)
            .unwrap()
            .contains("You are the test author")),
        "a deferred task's acceptance author was called"
    );

    let back = defer("T001", true);
    assert_eq!(back.status(), CommandStatus::Ok, "{}", back.message());
    assert!(
        speccoder_runtime::acceptance_at::deferred(
            &speccoder_runtime::worktree::project_state_dir(state.path(), r.path()),
            "001-graph",
            "T001"
        )
        .is_none()
    );
}

/// `accept.sh -- PATTERN`: one libtest-shaped test, "PATTERN is in src/lib.rs".
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

/// A mid-story task whose tests stay red and the model says the rest is
/// later tasks' — the next step and Needs You lead with judging
/// them at the checkpoint, and the saved attempt says where.
#[test]
fn a_mid_story_task_whose_tests_stay_red_is_offered_the_checkpoint() {
    use std::os::unix::fs::PermissionsExt;
    let r = repo();
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[acceptance]\nheld_out = false\n\n[commands]\nallow_prefix = [[\"./accept.sh\", \"--\"]]\n"),
    )
    .unwrap();
    fs::write(r.path().join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(
        r.path().join("accept.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "runner"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_T001.txt", "content": "the command line reports it\n", "scenarios": ["US1-1"] }], "argv": ["./accept.sh", "--", "fn cli_reports"], "notes": [] }));
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(json!({ "summary": "the parser is done; the tests need the command line T003 wires", "status": "incomplete", "files_changed": [], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::next::attach(
        speccoder_runtime::run_with(r.path(), None, "T001", state.path(), d, None),
        None,
    );
    let first = res
        .next()
        .first()
        .map(|s| s.command.clone())
        .unwrap_or_default();
    assert_eq!(
        first,
        "speccoder task defer T001",
        "{}",
        serde_json::to_string(&res).unwrap()
    );
    assert!(
        res.next()[0].why.contains("US1-1") && res.next()[0].why.contains("T003"),
        "{:?}",
        res.next()
    );

    let saved = speccoder_runtime::attempts::list(
        &speccoder_runtime::worktree::project_state_dir(state.path(), r.path())
            .join("worktrees/001-graph"),
        "001-graph",
    );
    let saved = if saved.is_empty() {
        speccoder_runtime::attempts::list(r.path(), "001-graph")
    } else {
        saved
    };
    assert_eq!(
        saved.first().and_then(|a| a.defer_to.clone()).as_deref(),
        Some("T003"),
        "{saved:?}"
    );

    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let text = serde_json::to_string(&inbox).unwrap();
    assert!(
        text.contains("judge it at T003") && text.contains("defer_acceptance"),
        "{text}"
    );
}

/// A link to no scenario of its story, and a story with no code task, are
/// told before anything runs.
#[test]
fn the_plan_says_when_a_link_names_nothing_or_no_task_judges_a_story() {
    let r = repo();
    let tasks = TASKS.replace("(scenarios US1-2)", "(scenarios US1-2, US9-4)")
        + "\n## Phase 4: User Story 2 - Docs\n\n- [ ] T005 [US2] Describe it in specs/001-graph/quickstart.md\n";
    fs::write(r.path().join("specs/001-graph/tasks.md"), tasks).unwrap();
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
    assert!(
        lints.iter().any(|l| l.detail.contains("US9-4")),
        "{lints:?}"
    );
}

fn shares(
    r: &Path,
) -> (
    Vec<(String, Option<speccoder_core::story::Share>)>,
    Vec<speccoder_core::story::StoryScenarios>,
) {
    let res = speccoder_runtime::status::report(r, None);
    let Some(speccoder_core::CommandData::Report(rep)) = res.data() else {
        panic!("{}", res.message());
    };
    (
        rep.tasks
            .iter()
            .map(|t| (t.id.clone(), t.share.clone()))
            .collect(),
        rep.stories.clone(),
    )
}

/// A person's step is never the checkpoint; once the checkpoint ticks, the
/// last open code task judges what is left; a story without scenarios
/// has no share.
#[test]
fn the_checkpoint_is_the_last_open_code_task_never_a_person_s_step() {
    use speccoder_core::story::Share;
    let r = repo();
    let tasks = TASKS.replace("T003 [P] [US1] Wire", "T003 [P] [US1] [YOU] Wire");
    fs::write(r.path().join("specs/001-graph/tasks.md"), &tasks).unwrap();
    git(r.path(), &["commit", "-qam", "T003 is a person's step"]);
    let (s, _) = shares(r.path());
    let of = |id: &str| {
        s.iter()
            .find(|(t, _)| t == id)
            .and_then(|(_, sh)| sh.clone())
    };
    assert!(
        matches!(of("T002"), Some(Share::Checkpoint { .. })),
        "{s:?}"
    );
    assert_eq!(of("T003"), None, "a person's step has no share");

    // T002 ticked (out of order): T001 closes the story now.
    fs::write(
        r.path().join("specs/001-graph/tasks.md"),
        tasks.replace("- [ ] T002", "- [x] T002"),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "T002 ticked"]);
    let (s, _) = shares(r.path());
    assert!(
        matches!(
            s.iter()
                .find(|(t, _)| t == "T001")
                .and_then(|(_, sh)| sh.clone()),
            Some(Share::Checkpoint { .. })
        ),
        "{s:?}"
    );

    // No scenario: no share at all.
    fs::write(
        r.path().join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Tables (Priority: P1)\n\nTables convert.\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "no scenarios"]);
    let (s, stories) = shares(r.path());
    assert!(s.iter().all(|(_, sh)| sh.is_none()), "{s:?}");
    assert!(stories.is_empty());
}
