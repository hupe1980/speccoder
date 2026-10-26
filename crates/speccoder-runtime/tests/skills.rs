//! Hosted Agent Skills with a scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::{CommandData, CommandResult, CommandStatus, SkillRunReport};
use speccoder_runtime::RunOptions;
use speccoder_runtime::approvals::{Answer, Approver};
use speccoder_runtime::hosting::{list, run_with};

const SKILL: &str = "---\nname: \"demo-specify\"\ndescription: \"Write a feature spec.\"\nuser-invocable: true\n---\n\n## User Input\n\n$ARGUMENTS\n\nWrite specs/002-login/spec.md for the feature; ask the user about anything unclear.\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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

fn repo() -> tempfile::TempDir {
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
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    for (name, text) in [
        ("demo-specify", SKILL.to_owned()),
        (
            "speckit-implement",
            "---\nname: speckit-implement\ndescription: Implement.\n---\nbody\n".to_owned(),
        ),
    ] {
        fs::create_dir_all(r.join(format!(".claude/skills/{name}"))).unwrap();
        fs::write(r.join(format!(".claude/skills/{name}/SKILL.md")), text).unwrap();
    }
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn report(r: &CommandResult) -> &SkillRunReport {
    match r.data() {
        Some(CommandData::Skill(k)) => k,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn done() -> serde_json::Value {
    json!({ "summary": "wrote the spec", "status": "done", "files_written": ["specs/002-login/spec.md"], "questions": [] })
}

#[derive(Debug)]
struct Answers;
impl Approver for Answers {
    fn ask(&self, _: &speccoder_runtime::approvals::Presentation) -> Answer {
        Answer::reject("no", None)
    }
    fn answer(&self, _: &str) -> Option<String> {
        Some("email and password only".to_owned())
    }
}

#[test]
fn skills_are_listed_and_implement_is_refused() {
    let r = repo();
    let res = list(r.path());
    let Some(CommandData::Skills(k)) = res.data() else {
        panic!("{}", res.message())
    };
    let names: Vec<(&str, bool)> = k
        .skills
        .iter()
        .map(|s| (s.name.as_str(), s.refused.is_some()))
        .collect();
    assert_eq!(
        names,
        [("demo-specify", false), ("speckit-implement", true)]
    );
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = run_with(
        r.path(),
        "implement",
        "",
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::UsageError, "{}", res.message());
    assert!(res.message().contains("speccoder run"));
    assert!(f.asked().is_empty());
}

#[test]
fn a_planning_skill_writes_the_spec_in_the_checkout_and_asks_when_it_can() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "q",
        "ask_person",
        json!({ "question": "Which login methods?" }),
    );
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/002-login/spec.md", "content": "# Spec\n\nLogin with email.\n" }),
    );
    f.will_structure(done());
    let options = RunOptions {
        approver: Some(Arc::new(Answers)),
        ..RunOptions::default()
    };
    let res = run_with(
        r.path(),
        "demo-specify",
        "Add a login page",
        state.path(),
        d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let k = report(&res);
    assert_eq!(
        k.changes,
        [("specs/002-login/spec.md".to_owned(), "added".to_owned())],
        "{:?}",
        k.tool_calls
    );
    // In the user's checkout, uncommitted.
    assert!(r.path().join("specs/002-login/spec.md").exists());
    // The instruction is the skill's body with $ARGUMENTS replaced; the question tool is offered.
    let asked = f.asked();
    let prompt = serde_json::to_string(&asked[0].prompt).unwrap();
    assert!(
        prompt.contains("Add a login page") && !prompt.contains("$ARGUMENTS"),
        "{prompt}"
    );
    assert!(asked[0].tools.iter().any(|t| t.name == "ask_person"));
    // Recorded as an answer, by the person who gave it.
    assert_eq!(k.tool_calls[0]["answered"], true, "{:?}", k.tool_calls);
    assert!(!k.tool_calls[0]["operator"].is_null(), "{:?}", k.tool_calls);
    // The person's answer reached the model.
    assert!(
        serde_json::to_string(&asked[1].prompt)
            .unwrap()
            .contains("email and password only")
            || format!("{:?}", asked[1]).contains("email and password only")
    );
    // Strict replay: the answer comes from the journal, nobody is asked.
    let replayed = speccoder_runtime::replay_with(r.path(), &k.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
}

#[test]
fn without_a_terminal_questions_go_unanswered_and_source_writes_stop() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "q",
        "ask_person",
        json!({ "question": "Which login methods?" }),
    );
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "src/login.rs", "content": "pub fn login() {}\n" }),
    );
    let res = run_with(
        r.path(),
        "demo-specify",
        "Add a login page",
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let k = report(&res);
    assert_eq!(k.status, "needs_approval");
    assert_eq!(k.tool_calls[0]["answered"], false);
    assert!(format!("{:?}", f.asked()[1]).contains("NEEDS CLARIFICATION"));
    assert!(!r.path().join("src/login.rs").exists());
    // The spec and the checks stay out of reach for anything else.
    assert!(k.changes.is_empty());
}

// ── extension hooks ─────────────────────────────────────────────────

const HOOK_SKILL: &str = "---\nname: \"speckit-audit-record\"\ndescription: \"Record that implementation starts.\"\n---\n\nWrite .specify/memory/audit.md saying implementation started.\n";

fn with_hooks(r: &Path, extensions: &str, skills: &[(&str, &str)]) {
    fs::write(r.join(".specify/extensions.yml"), extensions).unwrap();
    for (name, text) in skills {
        fs::create_dir_all(r.join(format!(".claude/skills/{name}"))).unwrap();
        fs::write(r.join(format!(".claude/skills/{name}/SKILL.md")), text).unwrap();
    }
    fs::write(
        r.join("specs/001-graph/tasks.md"),
        "# Tasks\n\n## Phase 1: Setup\n\n- [ ] T001 Create the crate\n",
    )
    .unwrap();
    fs::write(r.join("speccoder.toml"), "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n[checks.ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n").unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "hooks"]);
}

#[test]
fn a_mandatory_before_implement_hook_runs_as_a_hosted_skill_and_optional_ones_are_offered() {
    let r = repo();
    with_hooks(
        r.path(),
        "hooks:\n  before_implement:\n    - extension: audit\n      command: speckit.audit.record\n      optional: false\n    - extension: git\n      command: speckit.git.commit\n      optional: true\n      prompt: Commit outstanding changes before implementation?\n",
        &[("speckit-audit-record", HOOK_SKILL)],
    );
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    // The hook skill (it only reads; one that leaves artifacts uncommitted is the next test).
    f.will_call_tool("h", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_structure(
        json!({ "summary": "recorded", "status": "done", "files_written": [], "questions": [] }),
    );
    // T001's attempt.
    f.will_structure(json!({ "summary": "exists", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let res = speccoder_runtime::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    let st = String::from_utf8(
        Command::new("git")
            .args(["status", "--porcelain", "--ignored"])
            .current_dir(r.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(res.status(), CommandStatus::Ok, "{} / {st}", res.message());
    let Some(CommandData::Feature(fr)) = res.data() else {
        panic!()
    };
    assert!(
        fr.hooks[0].starts_with("ran before_implement hook speckit.audit.record"),
        "{:?}",
        fr.hooks
    );
    assert!(
        fr.hooks[1].contains("optional before_implement hook (git): Commit outstanding changes"),
        "{:?}",
        fr.hooks
    );
    // The hook ran first: its prompt is the skill's body.
    assert!(
        serde_json::to_string(&f.asked()[0].prompt)
            .unwrap()
            .contains("implementation started")
    );
}

#[test]
fn a_mandatory_hook_whose_skill_is_missing_stops_before_any_task() {
    let r = repo();
    with_hooks(
        r.path(),
        "hooks:\n  before_implement:\n    - extension: audit\n      command: speckit.audit.missing\n      optional: false\n",
        &[],
    );
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(
        res.status(),
        CommandStatus::ConfigError,
        "{}",
        res.message()
    );
    assert!(
        res.message()
            .contains("mandatory before_implement hook `speckit.audit.missing`"),
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty());
}

#[test]
fn a_hook_that_leaves_artifacts_uncommitted_stops_the_run_with_the_reason() {
    let r = repo();
    with_hooks(
        r.path(),
        "hooks:\n  before_implement:\n    - extension: audit\n      command: speckit.audit.record\n      optional: false\n",
        &[("speckit-audit-record", HOOK_SKILL)],
    );
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "h",
        "write_file",
        json!({ "path": ".specify/memory/audit.md", "content": "implementation started\n" }),
    );
    f.will_structure(json!({ "summary": "recorded", "status": "done", "files_written": [".specify/memory/audit.md"], "questions": [] }));
    let res = speccoder_runtime::run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(
        res.status(),
        CommandStatus::GitPrecondition,
        "{}",
        res.message()
    );
    assert!(res.message().contains("not committed"), "{}", res.message());
}

/// A Spec Kit-shaped skill: `speckit-<name>`.
fn speckit_skill(r: &Path, name: &str) {
    let dir = r.join(format!(".claude/skills/speckit-{name}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: speckit-{name}\ndescription: {name}.\n---\n\n$ARGUMENTS\n"),
    )
    .unwrap();
}

#[test]
fn a_planning_skill_is_told_the_feature_its_scripts_need() {
    // The reported first run: one feature, no .specify/feature.json, and
    // Spec Kit's setup script reads SPECIFY_FEATURE_DIRECTORY or that file.
    let r = repo();
    speckit_skill(r.path(), "plan");
    let scripts = r.path().join(".specify/scripts/bash");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(
        scripts.join("setup-plan.sh"),
        "#!/bin/sh\necho \"feature=$SPECIFY_FEATURE_DIRECTORY\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        scripts.join("setup-plan.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "plan skill"]);
    assert!(!r.path().join(".specify/feature.json").exists());
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "s",
        "run_command",
        json!({ "argv": [".specify/scripts/bash/setup-plan.sh"] }),
    );
    f.will_structure(
        json!({ "summary": "planned", "status": "done", "files_written": [], "questions": [] }),
    );
    let res = run_with(
        r.path(),
        "plan",
        "",
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let asked = f.asked();
    let seen =
        serde_json::to_string(&asked[1].prompt).unwrap_or_default() + &format!("{:?}", asked[1]);
    let canonical = fs::canonicalize(r.path()).unwrap();
    assert!(
        seen.contains(&format!(
            "feature={}",
            canonical.join("specs/001-graph").display()
        )),
        "the script did not see the feature: {seen}"
    );
    assert_eq!(
        report(&res).current.as_deref(),
        Some("001-graph (the only feature)")
    );
}

#[test]
fn a_feature_the_specify_skill_creates_becomes_current() {
    let r = repo();
    speckit_skill(r.path(), "specify");
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "specify skill"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    // The model writes the spec but not .specify/feature.json.
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/002-login/spec.md", "content": "# Spec\n\nLogin.\n" }),
    );
    f.will_structure(done());
    let res = run_with(
        r.path(),
        "specify",
        "Add a login page",
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(
        report(&res).current.as_deref(),
        Some("002-login (recorded in .specify/feature.json)")
    );
    let pointer: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(r.path().join(".specify/feature.json")).unwrap())
            .unwrap();
    assert_eq!(pointer["feature_directory"], "specs/002-login");
    // Every tool now agrees: the report selects it from the pointer.
    let rep = speccoder_runtime::current::list(r.path(), None);
    assert!(
        rep.message()
            .contains("current feature: 002-login (recorded in .specify/feature.json)"),
        "{}",
        rep.message()
    );
}

#[test]
fn the_current_feature_is_listed_switched_and_cleared() {
    let r = repo();
    fs::create_dir_all(r.path().join("specs/002-login")).unwrap();
    fs::write(r.path().join("specs/002-login/spec.md"), "# Spec\n").unwrap();
    let ambiguous = speccoder_runtime::current::list(r.path(), None);
    let Some(CommandData::Features(f)) = ambiguous.data() else {
        panic!("{}", ambiguous.message())
    };
    assert_eq!(f.current, None);
    assert_eq!(f.features.len(), 2);
    assert_eq!(f.features[0].artifacts, ["tasks"]);
    assert_eq!(f.features[1].artifacts, ["spec"]);
    assert!(
        ambiguous.message().contains("none current"),
        "{}",
        ambiguous.message()
    );
    // By number.
    let s = speccoder_runtime::current::switch(r.path(), "2");
    assert_eq!(s.status(), CommandStatus::Ok, "{}", s.message());
    assert!(
        s.message().starts_with("switched to 002-login"),
        "{}",
        s.message()
    );
    let Some(CommandData::Features(f)) = s.data() else {
        panic!()
    };
    assert_eq!(
        (f.current.as_deref(), f.reason.as_str()),
        (Some("002-login"), "recorded")
    );
    // A typo is answered with a suggestion and the list.
    let miss = speccoder_runtime::current::switch(r.path(), "grph");
    assert_eq!(miss.status(), CommandStatus::NotFound);
    assert!(
        miss.message().contains("did you mean 001-graph?"),
        "{}",
        miss.message()
    );
    // Cleared: nothing current again.
    let c = speccoder_runtime::current::clear(r.path());
    assert!(
        c.message().contains("no feature is current"),
        "{}",
        c.message()
    );
    assert!(!r.path().join(".specify/feature.json").exists());
    // A pointer to a feature that is gone: doctor names the closest one.
    fs::write(
        r.path().join(".specify/feature.json"),
        "{\"feature_directory\": \"specs/003-logn\"}",
    )
    .unwrap();
    let d = speccoder_runtime::doctor::run(r.path(), None, None);
    let Some(CommandData::Doctor(d)) = d.data() else {
        panic!()
    };
    let check = d.checks.iter().find(|c| c.check == "feature").unwrap();
    assert!(
        check
            .fix
            .iter()
            .any(|f| f.contains("did you mean 002-login?")),
        "{check:?}"
    );
}

#[test]
fn a_feature_without_tasks_is_being_planned_not_an_error() {
    let r = repo();
    fs::create_dir_all(r.path().join("specs/002-login")).unwrap();
    fs::write(r.path().join("specs/002-login/spec.md"), "# Spec\n").unwrap();
    let res = speccoder_runtime::next::attach(
        speccoder_runtime::status::report(r.path(), Some("002")),
        None,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(
        res.message()
            .contains("002-login is being planned: spec written, no tasks.md yet"),
        "{}",
        res.message()
    );
    let Some(CommandData::Report(s)) = res.data() else {
        panic!()
    };
    assert_eq!(s.planning, ["plan", "tasks"]);
    assert_eq!(s.artifacts, ["spec"]);
    let steps: Vec<&str> = res.next().iter().map(|n| n.command.as_str()).collect();
    assert_eq!(
        steps,
        [
            "speccoder spec plan",
            "speccoder spec tasks",
            "speccoder run"
        ]
    );
    // With a plan, only the task list is left.
    fs::write(r.path().join("specs/002-login/plan.md"), "# Plan\n").unwrap();
    let res = speccoder_runtime::status::report(r.path(), Some("002"));
    let Some(CommandData::Report(s)) = res.data() else {
        panic!()
    };
    assert_eq!(s.planning, ["tasks"]);
}

#[test]
fn nothing_planned_is_a_beginning_and_a_gone_pointer_names_the_closest() {
    let r = repo();
    fs::remove_dir_all(r.path().join("specs")).unwrap();
    let res =
        speccoder_runtime::next::attach(speccoder_runtime::status::report(r.path(), None), None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(
        res.message().starts_with("no features yet"),
        "{}",
        res.message()
    );
    assert_eq!(res.next()[0].command, "speccoder spec specify \"…\"");
    fs::create_dir_all(r.path().join("specs/002-login")).unwrap();
    fs::write(r.path().join("specs/002-login/spec.md"), "# Spec\n").unwrap();
    fs::write(
        r.path().join(".specify/feature.json"),
        "{\"feature_directory\": \"specs/003-logn\"}",
    )
    .unwrap();
    let res =
        speccoder_runtime::next::attach(speccoder_runtime::status::report(r.path(), None), None);
    assert_eq!(res.code().map(|c| c.name), Some("FEATURE-GONE"));
    assert!(
        res.message().contains("did you mean 002-login?"),
        "{}",
        res.message()
    );
}

/// A feature is listed by its spec's title, and its progress counts the
/// ticks on its branch, where speccoder makes them.
#[test]
fn a_feature_is_listed_by_its_title_with_its_branch_ticks() {
    let r = repo();
    fs::create_dir_all(r.path().join("specs/002-login")).unwrap();
    fs::write(
        r.path().join("specs/002-login/spec.md"),
        "# Feature Specification: Log in with GitHub\n",
    )
    .unwrap();
    let tasks = r.path().join("specs/001-graph/tasks.md");
    git(r.path(), &["checkout", "-q", "-b", "speccoder/001-graph"]);
    let text = fs::read_to_string(&tasks).unwrap();
    fs::write(&tasks, text.replacen("- [ ] T005", "- [x] T005", 1)).unwrap();
    git(r.path(), &["commit", "-q", "-am", "tick T005"]);
    git(r.path(), &["checkout", "-q", "-"]);
    let rep = speccoder_runtime::current::list(r.path(), None);
    let Some(CommandData::Features(f)) = rep.data() else {
        panic!("{}", rep.message())
    };
    assert_eq!(f.features[1].title.as_deref(), Some("Log in with GitHub"));
    assert_eq!(f.features[0].title, None, "the fixture's spec has no title");
    assert_eq!(f.features[0].done, 5, "4 in the checkout, 5 on the branch");
}
