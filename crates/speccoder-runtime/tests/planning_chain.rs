//! Plan through: one description, the format's own skills in
//! its order, stopping only where a person decides — with a scripted
//! model (no recorded journal is committed yet; `just record-journals`
//! adds real ones).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::actions::{FeatureSource, Through, Via};
use speccoder_core::{ChainStop, CommandData, CommandResult, CommandStatus, PlanningChainReport};
use speccoder_runtime::RunOptions;
use speccoder_runtime::planning_chain::{resume_with, start_with};

const REPORTS: &str = "# Feature Specification: Reports\n\n### User Story 1 - Export (Priority: P1)\n\n**Acceptance Scenarios**:\n\n1. **Given** a report, **When** I export it, **Then** I get a PDF\n\n## Requirements\n\n- **FR-001**: Users MUST be able to create a report from a saved filter.\n- **FR-002**: Reports MUST show totals per month.\n- **FR-003**: Users MUST be able to schedule a report to run every week.\n- **FR-004**: Users can export a report as PDF.\n";

/// The spec the scripted `specify` writes: two requirements, a story
/// with two scenarios, a story without one.
fn csv_spec(question: bool) -> String {
    let fr2 = if question {
        "- **FR-002**: The export MUST include [NEEDS CLARIFICATION: which columns?].\n"
    } else {
        "- **FR-002**: The export MUST include the columns the user chose.\n"
    };
    format!(
        "# Feature Specification: CSV export\n\n### User Story 1 - Export as CSV (Priority: P1)\n\n**Acceptance Scenarios**:\n\n1. **Given** a report, **When** I export it as CSV, **Then** I get a CSV file\n2. **Given** chosen columns, **When** I export, **Then** only those columns are in it\n\n### User Story 2 - Remember the choice (Priority: P2)\n\nThe columns are remembered.\n\n## Requirements\n\n- **FR-001**: Users MUST be able to export a report as CSV.\n{fr2}"
    )
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Dana Example",
            "-c",
            "user.email=d@example.com",
        ])
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

fn skill(r: &Path, name: &str) {
    let dir = r.join(format!(".claude/skills/speckit-{name}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!(
            "---\nname: speckit-{name}\ndescription: {name}.\n---\n\n## User Input\n\n$ARGUMENTS\n"
        ),
    )
    .unwrap();
}

/// A Spec Kit project with two features (001-graph, 003-reports), the
/// chain's skills installed and a script that echoes the unit variable.
fn repo() -> tempfile::TempDir {
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
    fs::create_dir_all(r.join("specs/003-reports")).unwrap();
    fs::write(r.join("specs/003-reports/spec.md"), REPORTS).unwrap();
    for s in ["specify", "plan", "tasks", "analyze"] {
        skill(r, s);
    }
    let scripts = r.join(".specify/scripts/bash");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(
        scripts.join("where.sh"),
        "#!/bin/sh\necho \"unit=$SPECIFY_FEATURE_DIRECTORY\"\n",
    )
    .unwrap();
    fs::set_permissions(scripts.join("where.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(r.join(".gitignore"), ".specify/feature.json\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
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

fn done(file: &str) -> Value {
    json!({ "summary": "wrote it", "status": "done", "files_written": [file], "questions": [] })
}

/// `specify` writes the new unit's spec.
fn will_specify(f: &FakeProvider, question: bool) {
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/004-csv-export/spec.md", "content": csv_spec(question) }),
    );
    f.will_structure(done("specs/004-csv-export/spec.md"));
}

/// `plan` checks where it is, then writes the plan; `tasks` the tasks.
fn will_plan_and_tasks(f: &FakeProvider) {
    f.will_call_tool(
        "s",
        "run_command",
        json!({ "argv": [".specify/scripts/bash/where.sh"] }),
    );
    f.will_call_tool(
        "p",
        "write_file",
        json!({ "path": "specs/004-csv-export/plan.md", "content": "# Plan\n\nRust.\n" }),
    );
    f.will_structure(done("specs/004-csv-export/plan.md"));
    f.will_call_tool(
        "t",
        "write_file",
        json!({ "path": "specs/004-csv-export/tasks.md", "content": "# Tasks\n\n## Phase 3: User Story 1\n\n- [ ] T001 [US1] Export a report as CSV in src/lib.rs\n" }),
    );
    f.will_structure(done("specs/004-csv-export/tasks.md"));
}

fn chain(r: &CommandResult) -> &PlanningChainReport {
    match r.data() {
        Some(CommandData::PlanningChain(c)) => c,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn ran(r: &CommandResult) -> Vec<String> {
    chain(r).ran.iter().map(|s| s.skill.clone()).collect()
}

fn untracked(r: &Path) -> Vec<String> {
    git(r, &["status", "--porcelain", "--untracked-files=all"])
        .lines()
        .map(str::to_owned)
        .collect()
}

const CSV: &str = "Users can export a report as CSV, with the columns they chose";

#[test]
fn spec_only_runs_the_creating_skill_once_with_the_description_and_commits_nothing() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    will_specify(&f, false);
    let res = start_with(
        r.path(),
        CSV,
        &FeatureSource::Description,
        Through::Spec,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let c = chain(&res);
    assert_eq!(ran(&res), ["speckit-specify"]);
    assert_eq!(c.unit.as_deref(), Some("004-csv-export"));
    assert!(matches!(c.stop, ChainStop::Done { .. }), "{:?}", c.stop);
    // The description reached the skill verbatim.
    let prompt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(prompt.contains(CSV), "{prompt}");
    // The readiness is what the written spec holds.
    let rd = c.readiness.clone().unwrap();
    assert_eq!(
        (rd.requirements, rd.scenarios, rd.open_questions),
        (2, 2, 0)
    );
    assert_eq!(rd.untestable, ["US2"]);
    assert_eq!(
        Some(rd),
        speccoder_runtime::review::readiness(r.path(), Some("004-csv-export"))
    );
    // The similarity hint names the PDF export.
    let m = c.similar.first().unwrap();
    assert_eq!(
        (m.unit.as_str(), m.requirement.as_str()),
        ("003-reports", "FR-004")
    );
    // Nothing committed: the new unit is left for the person.
    assert_eq!(git(r.path(), &["log", "-1", "--format=%s"]).trim(), "init");
    assert_eq!(untracked(r.path()), ["?? specs/004-csv-export/spec.md"]);
    // It became current.
    let cur = speccoder_runtime::current::list(r.path(), None);
    assert!(
        cur.message().contains("004-csv-export"),
        "{}",
        cur.message()
    );
}

#[test]
fn through_tasks_stops_for_the_review_and_continues_only_after_the_approval() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    will_specify(&f, false);
    let options = RunOptions::default();
    let res = start_with(
        r.path(),
        CSV,
        &FeatureSource::Description,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("THROUGH-REVIEW"));
    let c = chain(&res);
    assert_eq!(ran(&res), ["speckit-specify"]);
    let ChainStop::Review { requirements } = &c.stop else {
        panic!("{:?}", c.stop)
    };
    assert_eq!(requirements, &["FR-001", "FR-002"]);
    assert_eq!(c.next_skill.as_deref(), Some("speckit-plan"));
    assert!(!r.path().join("specs/004-csv-export/plan.md").exists());
    let calls = f.asked().len();
    // No flag passes the review: continuing now is refused, no model asked.
    let unit = Some("004-csv-export");
    let early = resume_with(
        r.path(),
        unit,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(
        early.code().map(|c| c.name),
        Some("THROUGH-REVIEW"),
        "{}",
        early.message()
    );
    assert_eq!(f.asked().len(), calls);
    // The person approves; the chain goes on with plan, then tasks.
    let approved =
        speccoder_runtime::review::decide(r.path(), unit, &[], "approve", None, Via::Terminal);
    assert_eq!(
        approved.status(),
        CommandStatus::Ok,
        "{}",
        approved.message()
    );
    will_plan_and_tasks(&f);
    let next = resume_with(
        r.path(),
        unit,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(next.status(), CommandStatus::Ok, "{}", next.message());
    assert_eq!(ran(&next), ["speckit-plan", "speckit-tasks"]);
    assert!(matches!(chain(&next).stop, ChainStop::Done { .. }));
    assert!(r.path().join("specs/004-csv-export/tasks.md").exists());
    // The plan skill was told the unit it plans.
    let canonical = fs::canonicalize(r.path()).unwrap();
    let seen = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap_or_default() + &format!("{a:?}"))
        .collect::<String>();
    assert!(
        seen.contains(&format!(
            "unit={}",
            canonical.join("specs/004-csv-export").display()
        )),
        "the plan skill did not see its unit"
    );
    // Still nothing committed.
    assert_eq!(git(r.path(), &["log", "-1", "--format=%s"]).trim(), "init");
}

#[test]
fn open_questions_stop_the_chain_and_a_changed_wording_lapses_the_approval() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    will_specify(&f, true);
    let options = RunOptions::default();
    let res = start_with(
        r.path(),
        CSV,
        &FeatureSource::Description,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(
        res.code().map(|c| c.name),
        Some("THROUGH-QUESTIONS"),
        "{}",
        res.message()
    );
    assert!(
        matches!(&chain(&res).stop, ChainStop::Questions { questions } if questions == &["Q1"])
    );
    assert_eq!(
        chain(&res).readiness.as_ref().map(|r| r.open_questions),
        Some(1)
    );
    let unit = Some("004-csv-export");
    // Answered, approved — then the wording changes: the approval lapses.
    let answered = speccoder_runtime::review::answer(r.path(), unit, "Q1", "the chosen ones");
    assert_eq!(
        answered.status(),
        CommandStatus::Ok,
        "{}",
        answered.message()
    );
    let approved =
        speccoder_runtime::review::decide(r.path(), unit, &[], "approve", None, Via::Terminal);
    assert_eq!(
        approved.status(),
        CommandStatus::Ok,
        "{}",
        approved.message()
    );
    let spec = r.path().join("specs/004-csv-export/spec.md");
    let text = fs::read_to_string(&spec)
        .unwrap()
        .replace("as CSV.", "as CSV or TSV.");
    fs::write(&spec, text).unwrap();
    let calls = f.asked().len();
    let lapsed = resume_with(
        r.path(),
        unit,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(
        lapsed.code().map(|c| c.name),
        Some("THROUGH-REVIEW"),
        "{}",
        lapsed.message()
    );
    assert_eq!(f.asked().len(), calls, "no model was asked");
}

#[test]
fn a_failed_step_keeps_what_ran_and_a_retry_runs_only_from_it() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    will_specify(&f, false);
    let options = RunOptions::default();
    let _ = start_with(
        r.path(),
        CSV,
        &FeatureSource::Description,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    let unit = Some("004-csv-export");
    let _ = speccoder_runtime::review::decide(r.path(), unit, &[], "approve", None, Via::Terminal);
    f.will_structure(json!({ "summary": "cannot", "status": "failed", "files_written": [], "questions": [], "reason": "no context" }));
    let failed = resume_with(
        r.path(),
        unit,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(
        failed.status(),
        CommandStatus::RunFailed,
        "{}",
        failed.message()
    );
    let ChainStop::Failed { skill, .. } = &chain(&failed).stop else {
        panic!("{:?}", chain(&failed).stop)
    };
    assert_eq!(skill, "speckit-plan");
    assert!(r.path().join("specs/004-csv-export/spec.md").exists());
    // Retry: the chain derives the step again from the files.
    will_plan_and_tasks(&f);
    let again = resume_with(
        r.path(),
        unit,
        Through::Tasks,
        state.path(),
        &d,
        None,
        &options,
    );
    assert_eq!(
        ran(&again),
        ["speckit-plan", "speckit-tasks"],
        "{}",
        again.message()
    );
}

#[test]
fn an_issue_source_records_the_units_origin_once() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    will_specify(&f, false);
    let res = start_with(
        r.path(),
        CSV,
        &FeatureSource::Issue { number: 42 },
        Through::Spec,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let origins = speccoder_runtime::ladder::of(&plane, "004-csv-export");
    assert_eq!(origins.len(), 1, "{origins:?}");
    assert!(matches!(
        origins[0],
        speccoder_core::ladder::Origin::Issue { number: 42, .. }
    ));
    assert_eq!(chain(&res).source, FeatureSource::Issue { number: 42 });
}

#[test]
fn an_empty_description_runs_nothing_and_only_the_chain_takes_through() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = start_with(
        r.path(),
        "  \n ",
        &FeatureSource::Description,
        Through::Spec,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::UsageError);
    assert_eq!(res.message(), "a description is needed");
    assert!(f.asked().is_empty());
    use speccoder_runtime::planning_chain::takes_through;
    assert_eq!(takes_through(r.path(), "specify"), Ok(true));
    assert_eq!(takes_through(r.path(), "speckit-plan"), Ok(false));
    let e = takes_through(r.path(), "clarify").unwrap_err();
    assert!(
        e.contains("speckit-specify, speckit-plan, speckit-tasks, speckit-analyze"),
        "{e}"
    );
}

#[test]
fn similar_requirements_are_found_without_a_model_and_the_same_every_time() {
    let r = repo();
    let first = speccoder_runtime::planning_chain::similar_to(r.path(), "export a report as CSV");
    assert_eq!(first.matches[0].unit, "003-reports");
    assert_eq!(first.matches[0].requirement, "FR-004");
    assert_eq!(first.matches[0].text, "Users can export a report as PDF.");
    assert!(first.compared >= 4);
    for _ in 0..100 {
        assert_eq!(
            speccoder_runtime::planning_chain::similar_to(r.path(), "export a report as CSV"),
            first
        );
    }
    let none = speccoder_runtime::planning_chain::similar_to(r.path(), "dark mode for settings");
    assert!(none.matches.is_empty());
}
