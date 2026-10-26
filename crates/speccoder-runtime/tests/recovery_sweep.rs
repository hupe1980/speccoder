//! Recovery at every crash point (the M3 exit criterion): a recorded
//! attempt is cut after each of its records, and each prefix is resumed as
//! a restarted process would; agentplane's testkit checks every resume
//! against the journal — finished effects read back, a quarantine only with
//! an effect in flight.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use agentplane::model::fake::FakeProvider;
use agentplane::model::{Completion, ModelError, ModelProvider, Request};
use agentplane::store::RedbStore;
use agentplane::testkit::{assert_resumed_honestly, crash_points};
use serde_json::{Value, json};
use speccoder_runtime::substrate::Plane;
use speccoder_runtime::substrate::attempt::{Settings, input};
use speccoder_runtime::tools::WorkspaceTools;
use speccoder_runtime::tools::policy::Policy;

/// The attempt's model, answering by how far the conversation has come: a
/// resume from any prefix asks for the turn the run had reached.
#[derive(Debug)]
struct Steps;

#[agentplane::prelude::async_trait]
impl ModelProvider for Steps {
    async fn complete(&self, request: Request<'_>) -> Result<Completion, ModelError> {
        // A request carries the turn's latest tool result (earlier context
        // rides in the provider's continuation): the next step follows it.
        let answered = match request.exchanges.last().map(|e| e.call.id.as_str()) {
            None => 0,
            Some("c1") => 1,
            Some("c2") => 2,
            Some(_) => 3,
        };
        let fake = FakeProvider::new();
        let _ = match answered {
            0 => fake.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" })),
            1 => fake.will_call_tool(
                "c2",
                "write_file",
                json!({ "path": "src/lib.rs", "content": "pub fn ok() {}\npub fn login() {}\n" }),
            ),
            2 => fake.will_call_tool("c3", "run_command", json!({ "argv": ["echo", "built"] })),
            _ => fake.will_structure(json!({
                "summary": "login", "status": "done", "files_changed": ["src/lib.rs"],
                "checks_run": [], "notes": []
            })),
        };
        fake.complete(request).await
    }

    fn gen_ai_provider(&self) -> Option<&str> {
        None
    }
}

fn bundle() -> Value {
    json!({
        "feature": "001-login",
        "constitution": "",
        "plan": "",
        "story": "Users log in.",
        "task": { "id": "T001", "title": "Add login in src/lib.rs", "phase": "Phase 1", "subsection": "", "story": "", "parallel": false },
        "dependencies": []
    })
}

#[test]
fn every_crash_point_of_an_attempt_resumes_honestly() {
    let work = tempfile::tempdir().unwrap();
    fs::create_dir_all(work.path().join("src")).unwrap();
    fs::write(work.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    let root = fs::canonicalize(work.path()).unwrap().display().to_string();
    let model = speccoder_runtime::substrate::model_config::parse(
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap()
    .unwrap();
    let allow = vec![vec!["echo".to_owned(), "built".to_owned()]];
    let policy = Policy::new(&[], &allow);
    let tools = || {
        Some(Arc::new(
            WorkspaceTools::new(work.path(), policy.clone()).unwrap(),
        ))
    };
    let settings = Settings {
        checks: &[],
        allow: &allow,
        allow_prefix: &[],
        max_turns: 8,
        max_tool_calls: 8,
        reasoning: false,
        frozen: &[],
        approved: &[],
        rejected: &[],
        run_approved: &[],
        nudges: &[],
        mcp: &[],
        root: &root,
        frozen_additions: false,
        asks: "stop",
        wait_secs: 0,
        network_allow: &[],
        network_allow_prefix: &[],
        run_id: "run-1-00000000",
        asked: 0,
        budget: 12,
        prompts: false,
        in_place: false,
        hidden: &[],
        autopilot: false,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // The recorded attempt: read, write, a command, then the report.
    let origin = Arc::new(RedbStore::open_in_memory().unwrap());
    let plane = Plane::on_store(Arc::clone(&origin), &model, Arc::new(Steps), tools()).unwrap();
    let (outcome, _) = rt
        .block_on(plane.attempt_keyed(
            "001-login",
            "T001",
            input(bundle(), &settings, None, &[]),
            "k1",
        ))
        .unwrap();
    assert!(
        matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded),
        "{:?}",
        outcome.status
    );

    let output = outcome.output.as_ref().unwrap().peek().clone();
    assert_eq!(output["status"], "completed", "{output}");
    let called: Vec<&str> = output["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["tool"].as_str())
        .collect();
    assert_eq!(called, ["read_file", "write_file", "run_command"]);

    let points = rt.block_on(crash_points(&*origin, outcome.run_id)).unwrap();
    assert!(points.len() >= 8, "{} crash points", points.len());
    let mut quarantined = 0;
    for point in &points {
        let resumed_plane =
            Plane::on_store(Arc::clone(&point.store), &model, Arc::new(Steps), tools()).unwrap();
        let resumed = rt.block_on(resumed_plane.resume(outcome.run_id));
        rt.block_on(assert_resumed_honestly(point, outcome.run_id, &resumed));
        if let Ok(o) = &resumed
            && matches!(o.status, agentplane::prelude::RunStatus::Quarantined(_))
        {
            quarantined += 1;
        }
    }
    // A crash while the write or the command was running is a person's to
    // decide; every other point resumes to the end.
    assert!(
        quarantined >= 1,
        "no crash point fell inside a mutating call"
    );
    eprintln!(
        "{} crash points resumed honestly, {quarantined} quarantined",
        points.len()
    );
}
