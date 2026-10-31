//! Explore with a scripted model: an idea explored read-only, approaches
//! the person chooses from, a brief with its digest, and the spec started
//! from it verbatim — nothing written in the checkout until then.

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
use speccoder_core::explore::{ExplorePayload, FALLBACK};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::RunOptions;
use speccoder_runtime::approvals::{Answer, Approver};
use speccoder_runtime::explore;

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

/// A Spec Kit project with no exploration skill, a `specify` skill to
/// start from, and one commit.
fn repo() -> tempfile::TempDir {
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
    fs::create_dir_all(r.join(".claude/skills/speckit-specify")).unwrap();
    fs::write(
        r.join(".claude/skills/speckit-specify/SKILL.md"),
        "---\nname: speckit-specify\ndescription: Write the spec.\n---\n\n## User Input\n\n$ARGUMENTS\n\nWrite the feature's spec.md.\n",
    )
    .unwrap();
    fs::write(r.join(".gitignore"), ".specify/feature.json\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn report() {}\n").unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn approach(t: &str) -> Value {
    json!({ "title": t, "summary": format!("{t}: how it would work"), "tradeoffs": [format!("{t} costs")], "risks": [format!("{t} risks")] })
}

fn report(n: usize) -> Value {
    let titles = [
        "Stream the rows",
        "Build a file first",
        "Use a library",
        "Ask the database",
    ];
    json!({
        "summary": "three ways to export",
        "status": "done",
        "approaches": titles.iter().take(n).map(|t| approach(t)).collect::<Vec<_>>(),
        "constraints": ["keep the export under a second"],
        "open_questions": ["which columns?"],
    })
}

fn payload(r: &CommandResult) -> &ExplorePayload {
    match r.data() {
        Some(CommandData::Explore(p)) => p,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[derive(Debug)]
struct Person;
impl Approver for Person {
    fn ask(&self, _: &speccoder_runtime::approvals::Presentation) -> Answer {
        Answer::reject("no", None)
    }
    fn answer(&self, _: &str) -> Option<String> {
        Some("only the visible columns".to_owned())
    }
}

fn status(r: &Path) -> String {
    git(r, &["status", "--porcelain"])
}

/// The fallback explores read-only: the person's answer is recorded as an
/// answer, a write is denied without asking, and the checkout is untouched.
#[test]
fn the_fallback_explores_read_only_and_records_the_persons_answer() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "q",
        "ask_person",
        json!({ "question": "Which columns matter?" }),
    );
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "src/export.rs", "content": "pub fn x() {}\n" }),
    );
    f.will_structure(report(3));
    let before = status(r.path());
    let options = RunOptions {
        approver: Some(Arc::new(Person)),
        ..RunOptions::default()
    };
    let res = explore::explore_in(
        r.path(),
        "export reports as CSV",
        None,
        None,
        state.path(),
        d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let p = payload(&res);
    let s = p.session.as_ref().unwrap();
    assert_eq!(
        (
            s.skill.as_str(),
            s.selected_because.as_str(),
            s.state.as_str()
        ),
        (FALLBACK, "fallback", "open")
    );
    assert_eq!(p.approaches.len(), 3);
    assert_eq!(s.qa[0].question, "Which columns matter?");
    assert_eq!(s.qa[0].answer.as_deref(), Some("only the visible columns"));
    assert!(s.qa[0].by.is_some(), "{:?}", s.qa);
    // The write was denied by rule, never sent for approval.
    let tools = format!("{:?}", f.asked().last().unwrap());
    assert!(tools.contains("exploring writes nothing"), "{tools}");
    assert!(!r.path().join("src/export.rs").exists());
    assert_eq!(status(r.path()), before, "the checkout is byte-identical");
    // The prompt is the explore preamble and the fallback's instruction.
    let first = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        first.contains("you cannot write anything") && first.contains("export reports as CSV"),
        "{first}"
    );
}

/// The fallback owes three distinct approaches; fewer is no brief.
#[test]
fn the_fallback_with_fewer_than_three_approaches_ends_incomplete() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(report(2));
    let res = explore::explore_in(
        r.path(),
        "export reports",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::RunFailed, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("EXPLORE-INCOMPLETE"));
    assert!(payload(&res).approaches.is_empty());
    let empty = explore::explore_in(
        r.path(),
        "  ",
        None,
        None,
        state.path(),
        fake().1,
        None,
        &RunOptions::default(),
    );
    assert_eq!(empty.code().map(|c| c.name), Some("EXPLORE-EMPTY"));
}

/// The person's choice: approach 2 verbatim, each item by its source, the
/// brief stored with its digest — and too large a brief refused.
#[test]
fn a_choice_composes_the_brief_with_its_digest_and_sources() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(report(3));
    let res = explore::explore_in(
        r.path(),
        "export reports as CSV",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    let id = payload(&res).session.as_ref().unwrap().id.clone();
    let chosen = explore::choose(
        r.path(),
        Some(state.path()),
        &id,
        2,
        &[
            "keep the export under a second".to_owned(),
            "no new dependency".to_owned(),
        ],
        &["which columns?".to_owned()],
        Via::Terminal,
        "flag",
    );
    assert_eq!(chosen.status(), CommandStatus::Ok, "{}", chosen.message());
    let p = payload(&chosen);
    let c = p.choice.as_ref().unwrap();
    assert_eq!(
        (c.approach, c.basis.as_str(), c.via),
        (2, "flag", Via::Terminal)
    );
    let sources: Vec<(&str, &str)> = c
        .constraints
        .iter()
        .map(|i| (i.text.as_str(), i.source.as_str()))
        .collect();
    assert_eq!(
        sources,
        [
            ("keep the export under a second", "model"),
            ("no new dependency", "person")
        ]
    );
    let brief = p.brief.as_deref().unwrap();
    assert!(
        brief.starts_with(&format!("Explore brief {} (session {id})", &c.digest[..12])),
        "{brief}"
    );
    assert!(
        brief.contains("Build a file first\nBuild a file first: how it would work"),
        "{brief}"
    );
    assert_eq!(p.session.as_ref().unwrap().state, "chosen");
    let huge = vec!["x".repeat(20_000)];
    let refused = explore::choose(
        r.path(),
        Some(state.path()),
        &id,
        1,
        &huge,
        &[],
        Via::Web,
        "prompt",
    );
    assert_eq!(refused.code().map(|c| c.name), Some("EXPLORE-BRIEF-LARGE"));
    assert!(
        git(r.path(), &["status", "--porcelain"]).is_empty(),
        "nothing in the checkout"
    );
}

/// The spec starts from the brief verbatim; its origin names the session
/// and digest; a changed brief, an edited description or a second start
/// are refused before anything runs.
#[test]
fn the_spec_starts_from_the_brief_verbatim_with_its_origin() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(report(3));
    let res = explore::explore_in(
        r.path(),
        "export reports as CSV",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    let id = payload(&res).session.as_ref().unwrap().id.clone();
    let not_chosen = explore::verify_start(r.path(), state.path(), &id, "x", "y").unwrap_err();
    assert_eq!(
        not_chosen.code().map(|c| c.name),
        Some("EXPLORE-NOT-CHOSEN")
    );
    let chosen = explore::choose(
        r.path(),
        Some(state.path()),
        &id,
        1,
        &[],
        &[],
        Via::Web,
        "prompt",
    );
    let p = payload(&chosen);
    let (brief, digest) = (
        p.brief.clone().unwrap(),
        p.choice.as_ref().unwrap().digest.clone(),
    );
    let edited =
        explore::verify_start(r.path(), state.path(), &id, &digest, "my own words").unwrap_err();
    assert_eq!(edited.code().map(|c| c.name), Some("EXPLORE-EDITED"));

    // Started as the perform arm does: verify, plan through, the origin.
    explore::verify_start(r.path(), state.path(), &id, &digest, &brief).unwrap();
    let (f, d) = fake();
    f.will_call_tool("w", "write_file", json!({ "path": "specs/004-csv-export/spec.md", "content": "# Spec\n\n- **FR-001**: Users MUST export a report as CSV.\n" }));
    f.will_structure(json!({ "summary": "wrote it", "status": "done", "files_written": ["specs/004-csv-export/spec.md"], "questions": [] }));
    let source = FeatureSource::Explore {
        session: id.clone(),
        digest: digest.clone(),
    };
    let made = speccoder_runtime::planning_chain::start_with(
        r.path(),
        &brief,
        &source,
        Through::Spec,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(made.status(), CommandStatus::Ok, "{}", made.message());
    // The creating skill was given the brief byte for byte.
    let given = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        given.contains(&serde_json::to_string(&brief).unwrap()[1..40]),
        "{given}"
    );
    explore::explored(r.path(), state.path(), &made, &id, &digest, Via::Web);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let origins = speccoder_runtime::ladder::origins(&plane);
    let explored = origins.values().flatten().find_map(|o| match o {
        speccoder_core::ladder::Origin::ExploredFrom {
            session, digest, ..
        } => Some((session.clone(), digest.clone())),
        _ => None,
    });
    assert_eq!(explored, Some((id.clone(), digest.clone())));
    let again = explore::verify_start(r.path(), state.path(), &id, &digest, &brief).unwrap_err();
    assert_eq!(again.code().map(|c| c.name), Some("EXPLORE-STARTED"));
}

/// A tampered brief never starts a spec.
#[test]
fn a_brief_changed_after_its_choice_is_refused() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(report(3));
    let res = explore::explore_in(
        r.path(),
        "export",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    let id = payload(&res).session.as_ref().unwrap().id.clone();
    let chosen = explore::choose(
        r.path(),
        Some(state.path()),
        &id,
        3,
        &[],
        &[],
        Via::Web,
        "prompt",
    );
    let digest = payload(&chosen).choice.as_ref().unwrap().digest.clone();
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let file = plane.join(format!("explore/{id}.brief.md"));
    let tampered = fs::read_to_string(&file)
        .unwrap()
        .replace("Use a library", "Rewrite everything");
    fs::write(&file, &tampered).unwrap();
    let refused =
        explore::verify_start(r.path(), state.path(), &id, &digest, &tampered).unwrap_err();
    assert_eq!(refused.code().map(|c| c.name), Some("EXPLORE-DIGEST"));
}

/// Sessions are listed newest first and resumed with what came before.
#[test]
fn a_session_resumes_carrying_its_record() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "q",
        "ask_person",
        json!({ "question": "Which columns matter?" }),
    );
    f.will_structure(report(3));
    let options = RunOptions {
        approver: Some(Arc::new(Person)),
        ..RunOptions::default()
    };
    let first = explore::explore_in(
        r.path(),
        "export reports as CSV",
        None,
        None,
        state.path(),
        d,
        None,
        &options,
    );
    let id = payload(&first).session.as_ref().unwrap().id.clone();
    let _ = explore::choose(
        r.path(),
        Some(state.path()),
        &id,
        1,
        &[],
        &[],
        Via::Web,
        "prompt",
    );
    let (f, d) = fake();
    f.will_structure(report(3));
    let resumed = explore::explore_in(
        r.path(),
        "also XLSX",
        None,
        Some(&id),
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(resumed.status(), CommandStatus::Ok, "{}", resumed.message());
    let s = payload(&resumed).session.clone().unwrap();
    assert_eq!(
        (s.runs.len(), s.state.as_str(), s.choices.len()),
        (2, "open", 1)
    );
    let input = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    for part in [
        "Which columns matter?",
        "only the visible columns",
        "source=\\\"model output\\\"",
        "also XLSX",
    ] {
        assert!(input.contains(part), "{part} missing from {input}");
    }
    let listed = explore::list(r.path(), Some(state.path()));
    let sessions = payload(&listed).sessions.clone().unwrap();
    assert_eq!(sessions.len(), 1);
    let unknown = explore::explore_in(
        r.path(),
        "",
        None,
        Some("ex-0000"),
        state.path(),
        fake().1,
        None,
        &RunOptions::default(),
    );
    assert_eq!(unknown.status(), CommandStatus::NotFound);
    assert!(
        unknown.message().contains("spec explore list"),
        "{}",
        unknown.message()
    );
}

/// An OpenSpec project explores with its own `openspec-explore`, under
/// the explore policy: its read-only CLI runs, a change it would create is
/// denied — and Spec Kit's `clarify` is never taken for exploring.
#[test]
fn openspec_explores_with_its_own_skill_and_only_reads() {
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
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "n",
        "run_command",
        json!({ "argv": ["openspec", "new", "change", "export"] }),
    );
    f.will_structure(report(1));
    let before = git(r, &["status", "--porcelain"]);
    let res = explore::explore_in(
        r,
        "exports",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let s = payload(&res).session.clone().unwrap();
    assert_eq!(
        (s.skill.as_str(), s.selected_because.as_str()),
        ("openspec-explore", "format")
    );
    let told = format!("{:?}", f.asked().last().unwrap());
    assert!(told.contains("read-only commands"), "{told}");
    assert_eq!(git(r, &["status", "--porcelain"]), before);

    // Spec Kit has no exploration skill of its own: `clarify` is never one.
    let sk = repo();
    fs::create_dir_all(sk.path().join(".claude/skills/speckit-clarify")).unwrap();
    fs::write(
        sk.path().join(".claude/skills/speckit-clarify/SKILL.md"),
        "---\nname: speckit-clarify\ndescription: Clarify.\n---\nbody\n",
    )
    .unwrap();
    let (f, d) = fake();
    f.will_structure(report(3));
    let res = explore::explore_in(
        sk.path(),
        "exports",
        None,
        None,
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(payload(&res).session.as_ref().unwrap().skill, FALLBACK);
}
