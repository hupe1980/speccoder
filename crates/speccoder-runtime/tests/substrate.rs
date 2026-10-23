//! The substrate against agentplane's scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::anthropic::Anthropic;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandStatus, ProposeReport, ReplayReport};
use speccoder_runtime::{propose_with, replay_with};

const CONFIG: &str =
    "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\nmax_output_tokens = 2048\n";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-speckit/tests/fixtures")
        .join(name)
}

/// The `graph` fixture copied to a temp dir, with a spec, a plan and a config.
fn project(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("cp")
        .arg("-R")
        .arg(fixture("graph").join("."))
        .arg(dir.path())
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let feature = dir.path().join("specs/001-graph");
    fs::write(
        feature.join("spec.md"),
        "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nUsers log in.\n\n### User Story 2 - Profile (Priority: P2)\n\nUsers edit profiles.\n",
    )
    .unwrap();
    fs::write(feature.join("plan.md"), "# Plan\n\nRust, axum, sqlx.\n").unwrap();
    fs::write(dir.path().join("speccoder.toml"), config).unwrap();
    dir
}

fn proposal(summary: &str) -> Value {
    json!({
        "summary": summary,
        "steps": ["write the test", "implement"],
        "files": ["src/models/user.rs"],
        "checks": ["cargo test"],
        "risks": [],
    })
}

fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                out.insert(e.path(), fs::read(e.path()).unwrap());
            }
        }
    }
    out
}

fn report(r: &speccoder_core::CommandResult) -> &ProposeReport {
    match r.data() {
        Some(CommandData::Propose(p)) => p,
        other => panic!("{}: {other:?}", r.message()),
    }
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let fake = FakeProvider::new();
    let driver: Arc<dyn ModelProvider> = fake.clone();
    (fake, driver)
}

// ── US1 ────────────────────────────────────────────────────────────────────

#[test]
fn propose_returns_the_models_proposal_and_changes_nothing() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    fake.will_structure(proposal("add the user model"));
    let before = tree(p.path());

    let r = propose_with(p.path(), None, "T006", state.path(), driver, None);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    let rep = report(&r);
    assert_eq!(rep.task, "T006");
    assert_eq!(rep.run_status, "succeeded");
    assert_eq!(
        rep.proposal.as_ref().unwrap(),
        &proposal("add the user model")
    );
    assert!(rep.usage.is_some());
    assert_eq!(fake.calls(), 1);
    assert_eq!(before, tree(p.path()), "propose modified the project");

    // The model saw the task, its story section, the plan and its dependency.
    let ask = &fake.asked()[0];
    let prompt = serde_json::to_string(&ask.prompt).unwrap();
    for needle in [
        "T006",
        "Create User model",
        "User Story 1 - Login",
        "Rust, axum, sqlx",
        "T005",
    ] {
        assert!(prompt.contains(needle), "prompt lacks {needle}: {prompt}");
    }
    assert!(
        !prompt.contains("Users edit profiles"),
        "another story leaked in"
    );
    assert_eq!(ask.max_output_tokens, 2048);
    assert!(
        ask.schema.is_some(),
        "the proposal is requested with a schema"
    );
}

#[test]
fn unknown_task_and_missing_model_config_fail_before_any_call() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    let r = propose_with(p.path(), None, "T999", state.path(), driver.clone(), None);
    assert_eq!(r.status(), CommandStatus::NotFound);
    assert_eq!(r.exit_code().code(), 15);

    let bare = project("[checks.lint]\nargv = [\"true\"]\n");
    let r = propose_with(bare.path(), None, "T006", state.path(), driver, None);
    assert_eq!(r.status(), CommandStatus::ConfigError);
    assert!(r.message().contains("[model]"));
    assert_eq!(fake.calls(), 0);
}

#[test]
fn an_answer_that_is_not_a_proposal_fails_the_command() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    fake.will_say("I would just do it.");
    let r = propose_with(p.path(), None, "T006", state.path(), driver, None);
    assert_eq!(r.status(), CommandStatus::RunFailed, "{}", r.message());
    assert_eq!(r.exit_code().code(), 21);
}

// ── US2 ────────────────────────────────────────────────────────────────────

#[test]
fn strict_replay_reproduces_without_calling_the_model() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    fake.will_structure(proposal("p"));
    let r = propose_with(p.path(), None, "T006", state.path(), driver, None);
    let run = report(&r).run_id.clone();

    let replayed = replay_with(p.path(), &run, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
    let Some(CommandData::Replay(ReplayReport { result, .. })) = replayed.data() else {
        panic!("no replay report");
    };
    assert_eq!(result, "reproduced");
    assert_eq!(fake.calls(), 1, "replay called the model");
}

#[test]
fn replaying_an_unknown_run_is_not_found() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let missing = agentplane::core::RunId::generate().to_string();
    let r = replay_with(p.path(), &missing, state.path());
    let Some(CommandData::Replay(ReplayReport { result, .. })) = r.data() else {
        panic!("{}", r.message());
    };
    assert_eq!(result, "not_found");
}

// ── US3 ────────────────────────────────────────────────────────────────────

#[test]
fn the_cached_prefix_is_byte_identical_across_tasks() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    fake.will_structure(proposal("a"));
    fake.will_structure(proposal("b"));
    for task in ["T006", "T009"] {
        let r = propose_with(p.path(), None, task, state.path(), driver.clone(), None);
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    }
    let asked = fake.asked();
    let [a, b] = [&asked[0].prompt, &asked[1].prompt];
    assert_eq!(
        serde_json::to_vec(&a["system"]).unwrap(),
        serde_json::to_vec(&b["system"]).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(&a["messages"][0]["content"][0]).unwrap(),
        serde_json::to_vec(&b["messages"][0]["content"][0]).unwrap(),
        "the plan block (second breakpoint) differs"
    );
    assert_eq!(
        a["system"][1]["cache_control"],
        json!({ "type": "ephemeral" })
    );
    assert_ne!(a["messages"], b["messages"], "the task part differs");
}

#[test]
fn openai_prompts_use_instructions_and_input_items() {
    let p = project("[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n");
    let state = tempfile::tempdir().unwrap();
    let (fake, driver) = fake();
    fake.will_structure(proposal("a"));
    fake.will_structure(proposal("b"));
    for task in ["T006", "T009"] {
        let r = propose_with(p.path(), None, task, state.path(), driver.clone(), None);
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    }
    let asked = fake.asked();
    let a = &asked[0].prompt;
    assert!(
        a["system"].is_string(),
        "Responses `instructions` must be a string"
    );
    assert_eq!(a["input"][0]["role"], "user");
    assert_eq!(a["input"][0]["content"][0]["type"], "input_text");
    assert!(a.get("messages").is_none());
    assert!(!serde_json::to_string(a).unwrap().contains("cache_control"));
    // Same stability order: instructions and the plan item are identical across tasks.
    let b = &asked[1].prompt;
    assert_eq!(a["system"], b["system"]);
    assert_eq!(a["input"][0]["content"][0], b["input"][0]["content"][0]);
}

#[test]
fn the_api_key_never_reaches_output_errors_or_the_journal() {
    let p = project(CONFIG);
    let state = tempfile::tempdir().unwrap();
    let sentinel = "sk-ant-SENTINEL-not-a-real-key-4242";
    // A real Anthropic driver aimed at a port nothing listens on.
    let driver: Arc<dyn ModelProvider> =
        Arc::new(Anthropic::new(sentinel).unwrap().base("http://127.0.0.1:9"));
    let r = propose_with(p.path(), None, "T006", state.path(), driver, Some(sentinel));
    assert_eq!(r.status(), CommandStatus::RunFailed, "{}", r.message());
    let printed = serde_json::to_string(&r).unwrap();
    assert!(!printed.contains(sentinel), "{printed}");
    for (path, bytes) in tree(state.path()) {
        let hit = bytes
            .windows(sentinel.len())
            .any(|w| w == sentinel.as_bytes());
        assert!(!hit, "the key is in {}", path.display());
    }
}

// ── opening the journal ─────────────────────────────────────────────

#[test]
fn a_journal_in_use_is_refused_untouched_and_an_unreadable_one_is_moved_aside() {
    use speccoder_runtime::substrate::{JOURNAL, Plane, model_config};
    let model = model_config::parse(CONFIG).unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (_, d) = fake();
    let first = Plane::open(dir.path(), &model, Arc::clone(&d)).unwrap();
    assert!(first.replaced_journal.is_none());
    let before = fs::read(dir.path().join(JOURNAL)).unwrap();
    let started = std::time::Instant::now();
    let Err(e) = Plane::open(dir.path(), &model, Arc::clone(&d)) else {
        panic!("a second plane opened a journal in use")
    };
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(e.0.contains("another speccoder"), "{}", e.0);
    assert_eq!(
        fs::read(dir.path().join(JOURNAL)).unwrap(),
        before,
        "touched"
    );
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "nothing moved aside"
    );
    drop(first);

    let broken = tempfile::tempdir().unwrap();
    fs::write(broken.path().join(JOURNAL), b"not a redb file at all").unwrap();
    let fresh = Plane::open(broken.path(), &model, d).unwrap();
    let aside = fresh.replaced_journal.clone().expect("moved aside");
    assert_eq!(fs::read(&aside).unwrap(), b"not a redb file at all");
}
