//! External agents over ACP: a scripted agent (`fixtures/acp_agent.py`)
//! makes the attempt; every call it asks permission for, and every file and
//! command it runs through speccoder, is decided by speccoder's policy.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::approvals::{Answer, Approver, Presentation, Scope, Verb, decide_with};

const MODEL: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n";
const CHECK: &str = "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n";
/// A value of the checkout's `.env`, long enough to be masked.
const TOKEN: &str = "tok-Zx81-secret-value";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A project whose executor is the scripted agent playing `scenario`.
struct Project {
    dir: tempfile::TempDir,
    side: tempfile::TempDir,
    state: tempfile::TempDir,
}

impl Project {
    fn new(scenario: &Value, extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let side = tempfile::tempdir().unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../speccoder-speckit/tests/fixtures/graph");
        assert!(
            Command::new("cp")
                .arg("-R")
                .arg(fixture.join("."))
                .arg(dir.path())
                .status()
                .unwrap()
                .success()
        );
        let root = dir.path().to_path_buf();
        let r = root.as_path();
        fs::write(
            r.join("specs/001-graph/spec.md"),
            "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
        )
        .unwrap();
        fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
        fs::create_dir_all(r.join("src")).unwrap();
        fs::create_dir_all(r.join("tests")).unwrap();
        fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        fs::write(r.join("tests/existing.rs"), "#[test]\nfn existing() {}\n").unwrap();
        fs::write(r.join("config.txt"), format!("token = {TOKEN}\n")).unwrap();
        fs::write(r.join(".gitignore"), ".env\n").unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp_agent.py");
        let p = Self {
            dir,
            side,
            state: tempfile::tempdir().unwrap(),
        };
        p.script(scenario);
        fs::write(
            r.join("speccoder.toml"),
            format!(
                "{MODEL}{CHECK}[budget]\nmax_attempts = 1\n[worktree]\ninclude = [\".env\"]\n{extra}[executor]\nkind = \"acp\"\nargv = [\"python3\", \"-I\", \"{}\", \"{}\"]\n",
                agent.display(),
                p.scenario_path().display()
            ),
        )
        .unwrap();
        git(r, &["init", "-q"]);
        git(r, &["add", "-A"]);
        git(r, &["commit", "-q", "-m", "init"]);
        fs::write(r.join(".env"), format!("API_TOKEN={TOKEN}\n")).unwrap();
        p
    }

    fn scenario_path(&self) -> PathBuf {
        self.side.path().join("scenario.json")
    }

    fn log_path(&self) -> PathBuf {
        self.side.path().join("log.jsonl")
    }

    /// Replaces the scenario (the next attempt plays it).
    fn script(&self, scenario: &Value) {
        let mut s = scenario.clone();
        s["log"] = json!(self.log_path());
        fs::write(self.scenario_path(), s.to_string()).unwrap();
        let _ = fs::remove_file(self.log_path());
    }

    fn run(&self) -> CommandResult {
        let d: Arc<dyn ModelProvider> = FakeProvider::new();
        speccoder_runtime::run_with(self.dir.path(), None, "T005", self.state.path(), d, None)
    }

    fn run_asking(&self, answers: Vec<Answer>) -> CommandResult {
        let d: Arc<dyn ModelProvider> = FakeProvider::new();
        speccoder_runtime::run_with_approver(
            self.dir.path(),
            None,
            "T005",
            self.state.path(),
            d,
            None,
            Arc::new(Scripted(std::sync::Mutex::new(answers))),
        )
    }

    /// What the agent got back, line by line.
    fn log(&self) -> Vec<Value> {
        fs::read_to_string(self.log_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// The answers to the agent's own requests.
    fn answers(&self) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|e| e.get("step").is_some())
            .collect()
    }
}

#[derive(Debug)]
struct Scripted(std::sync::Mutex<Vec<Answer>>);

impl Approver for Scripted {
    fn ask(&self, _: &Presentation) -> Answer {
        self.0.lock().unwrap().remove(0)
    }
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn permission(kind: &str, raw: Value) -> Value {
    json!({ "permission": { "kind": kind, "rawInput": raw } })
}

fn chosen(answer: &Value) -> Option<&str> {
    answer["result"]["outcome"]["optionId"].as_str()
}

const EDIT: &str = "pub fn ok() {}\npub fn login() {}\n";

#[test]
fn an_allowed_call_goes_ahead_and_the_verdict_is_speccoders() {
    let p = Project::new(
        &json!({ "steps": [
            { "say": "Looking at the code." },
            { "plan": [{ "content": "Add login", "status": "in_progress" }] },
            permission("execute", json!({ "command": ["grep", "-q", "fn ok", "src/lib.rs"] })),
            permission("read", json!({ "path": "src/lib.rs" })),
            { "edit": "src/lib.rs", "content": EDIT },
            { "say": "Added login." },
        ] }),
        "",
    );
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.run_id.starts_with("external_acp_"), "{}", rep.run_id);
    assert!(rep.ticked && rep.commit.is_some());
    assert_eq!(
        rep.model_report.as_ref().unwrap()["summary"],
        "Added login."
    );
    // The check prefix allows the command, a workspace read the read: each
    // answered with the agent's allow-once option, never "always".
    let answers = p.answers();
    assert_eq!(chosen(&answers[0]), Some("allow"));
    assert_eq!(chosen(&answers[1]), Some("allow"));
    let calls = &rep.tool_calls;
    assert_eq!(calls[0]["tool"], "run_command");
    assert_eq!(
        calls[0]["arguments"]["argv"],
        json!(["grep", "-q", "fn ok", "src/lib.rs"])
    );
    assert_eq!(
        (calls[0]["decision"].as_str(), calls[0]["by"].as_str()),
        (Some("allow"), Some("rule"))
    );
    // The prompt is the one every executor gets.
    let prompt = p
        .log()
        .into_iter()
        .find(|e| e["got"] == "session/prompt")
        .unwrap();
    let text = prompt["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("# Task T005") && text.contains("BLOCKED:"),
        "{text}"
    );
    // The client offered files and terminals, and opened the session in
    // the worktree with no MCP servers.
    let init = p
        .log()
        .into_iter()
        .find(|e| e["got"] == "initialize")
        .unwrap();
    assert_eq!(init["params"]["clientCapabilities"]["terminal"], true);
    assert_eq!(
        init["params"]["clientCapabilities"]["fs"]["writeTextFile"],
        true
    );
    let new = p
        .log()
        .into_iter()
        .find(|e| e["got"] == "session/new")
        .unwrap();
    assert_eq!(
        fs::canonicalize(new["params"]["cwd"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&rep.worktree).unwrap()
    );
    assert_eq!(new["params"]["mcpServers"], json!([]));
}

#[test]
fn a_call_that_needs_a_person_waits_and_continues_after_task_approve() {
    let ask = json!({ "steps": [
        permission("execute", json!({ "command": "make lint" })),
        { "edit": "src/lib.rs", "content": EDIT },
        { "say": "Done." },
    ] });
    let p = Project::new(&ask, "");
    let first = p.run();
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let rep = report(&first);
    assert_eq!(rep.attempt_status, "needs_approval");
    assert_eq!(
        rep.pending.as_ref().unwrap()["arguments"]["argv"],
        json!(["make", "lint"])
    );
    assert!(
        first.message().contains("speccoder task approve T005"),
        "{}",
        first.message()
    );
    // The agent was told no more (cancelled) and its turn cancelled.
    assert_eq!(p.answers()[0]["result"]["outcome"]["outcome"], "cancelled");
    assert!(p.log().iter().any(|e| e["got"] == "session/cancel"));
    assert!(!rep.ticked);

    let ok = decide_with(
        p.dir.path(),
        None,
        Verb::Approve("T005"),
        p.state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    p.script(&ask);
    let second = p.run();
    assert_eq!(second.status(), CommandStatus::Ok, "{}", second.message());
    // The person's grant answers the same call now.
    assert_eq!(chosen(&p.answers()[0]), Some("allow"));
    let call = &report(&second).tool_calls[0];
    assert_eq!(
        (call["by"].as_str(), call["scope"].as_str()),
        (Some("person"), Some("task-grant"))
    );
}

#[test]
fn a_person_at_the_terminal_decides_and_a_rejection_reaches_the_agent() {
    let p = Project::new(
        &json!({ "steps": [
            permission("execute", json!({ "command": "make lint" })),
            permission("execute", json!({ "command": "curl https://example.com" })),
            { "edit": "src/lib.rs", "content": EDIT },
            { "say": "Done." },
        ] }),
        "",
    );
    let res = p.run_asking(vec![
        Answer::approve(Scope::Once, None, None),
        Answer::reject("no network", None),
    ]);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let answers = p.answers();
    assert_eq!(
        (chosen(&answers[0]), chosen(&answers[1])),
        (Some("allow"), Some("reject"))
    );
    let calls = &report(&res).tool_calls;
    assert_eq!(calls[0]["approved_by"], "person");
    assert_eq!(
        (
            calls[1]["rejected_by"].as_str(),
            calls[1]["rejection"].as_str()
        ),
        (Some("person"), Some("no network"))
    );
    assert_eq!(report(&res).attempt_status, "completed");
}

#[test]
fn protected_files_and_secrets_are_refused_and_copied_secrets_masked() {
    let p = Project::new(
        &json!({ "steps": [
            { "read": ".env" },
            { "write": ".env", "content": "X=1\n" },
            { "write": "speccoder.toml", "content": "" },
            permission("edit", json!({ "path": "specs/001-graph/spec.md" })),
            { "read": "config.txt" },
            { "read": "src/lib.rs", "line": 1, "limit": 1 },
            { "write": "src/lib.rs", "content": EDIT },
            { "say": "Done." },
        ] }),
        "",
    );
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let a = p.answers();
    assert!(
        a[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("credential"),
        "{}",
        a[0]
    );
    assert!(a[0]["result"].is_null(), "{}", a[0]);
    assert!(a[1]["error"].is_object(), "{}", a[1]);
    assert!(
        a[2]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("protected"),
        "{}",
        a[2]
    );
    assert_eq!(chosen(&a[3]), Some("reject"));
    let config = a[4]["result"]["content"].as_str().unwrap();
    assert!(
        !config.contains(TOKEN) && config.starts_with("token = "),
        "{config}"
    );
    assert_eq!(a[5]["result"]["content"], "pub fn ok() {}\n");
    assert!(a[6]["error"].is_null(), "{}", a[6]);
    assert!(report(&res).ticked);
    let wt = PathBuf::from(&report(&res).worktree);
    assert_eq!(
        fs::read_to_string(wt.join(".env")).unwrap(),
        format!("API_TOKEN={TOKEN}\n")
    );
}

#[test]
fn a_frozen_test_written_through_speccoder_is_a_persons_decision() {
    let p = Project::new(
        &json!({ "steps": [
            { "write": "tests/existing.rs", "content": "#[test]\nfn existing() {}\n#[test]\nfn more() {}\n" },
            { "write": "src/lib.rs", "content": EDIT },
            { "say": "Done." },
        ] }),
        "",
    );
    let res = p.run();
    assert_eq!(
        report(&res).attempt_status,
        "needs_approval",
        "{}",
        res.message()
    );
    assert_eq!(
        report(&res).pending.as_ref().unwrap()["arguments"]["path"],
        "tests/existing.rs"
    );
    // With `[tests] frozen = "additions"`, a write that keeps every line goes ahead.
    let p = Project::new(
        &json!({ "steps": [
            { "write": "tests/existing.rs", "content": "#[test]\nfn existing() {}\n#[test]\nfn more() {}\n" },
            { "write": "src/lib.rs", "content": EDIT },
            { "say": "Done." },
        ] }),
        "[tests]\nfrozen = \"additions\"\n",
    );
    let res = p.run();
    assert_ne!(
        report(&res).attempt_status,
        "needs_approval",
        "{}",
        res.message()
    );
    assert!(p.answers()[0]["error"].is_null(), "{}", p.answers()[0]);
    assert_eq!(report(&res).tool_calls[0]["frozen_additions"], true);
}

#[test]
fn terminals_run_sandboxed_with_masked_output_and_need_the_policy() {
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("escaped");
    let p = Project::new(
        &json!({ "steps": [
            { "terminal": "cat", "args": ["config.txt"] },
            { "terminal": "touch", "args": [target] },
            { "terminal": "make", "args": ["deploy"] },
            { "edit": "src/lib.rs", "content": EDIT },
            { "say": "Done." },
        ] }),
        &format!(
            "[commands]\nallow = [[\"cat\", \"config.txt\"], [\"touch\", \"{}\"]]\n",
            target.display()
        ),
    );
    let res = p.run();
    let a = p.answers();
    let outputs: Vec<&Value> = a
        .iter()
        .filter(|e| e["method"] == "terminal/output")
        .collect();
    let cat = outputs[0]["result"]["output"].as_str().unwrap();
    assert!(cat.starts_with("token = ") && !cat.contains(TOKEN), "{cat}");
    assert_eq!(outputs[0]["exit"]["exitCode"], 0);
    // Confined to the worktree where the OS sandbox exists.
    if speccoder_runtime::sandbox::backend().is_ok() {
        assert_ne!(outputs[1]["exit"]["exitCode"], 0, "{}", outputs[1]);
        assert!(!target.exists());
    }
    // A command no rule allows waits for a person: the attempt ends.
    assert_eq!(
        report(&res).attempt_status,
        "needs_approval",
        "{}",
        res.message()
    );
    assert_eq!(
        report(&res).pending.as_ref().unwrap()["arguments"]["argv"],
        json!(["make", "deploy"])
    );
}

#[test]
fn an_honest_stop_a_crash_and_a_timeout_are_reported_as_such() {
    let p = Project::new(
        &json!({ "steps": [{ "say": "BLOCKED: the spec contradicts itself" }] }),
        "",
    );
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(report(&res).attempt_status, "blocked");
    assert_eq!(
        report(&res).reason.as_deref(),
        Some("the spec contradicts itself")
    );

    let p = Project::new(
        &json!({ "steps": [{ "exit": 3, "stderr": "out of credits" }] }),
        "",
    );
    let res = p.run();
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "failed");
    assert!(
        rep.reason
            .as_deref()
            .unwrap_or_default()
            .contains("out of credits"),
        "{:?}",
        rep.reason
    );

    let p = Project::new(
        &json!({ "steps": [{ "say": "Thinking…" }, { "sleep": 30 }] }),
        "",
    );
    let toml = fs::read_to_string(p.dir.path().join("speccoder.toml")).unwrap();
    fs::write(
        p.dir.path().join("speccoder.toml"),
        format!("{toml}timeout_secs = 2\n"),
    )
    .unwrap();
    git(p.dir.path(), &["commit", "-qam", "timeout"]);
    let began = std::time::Instant::now();
    let res = p.run();
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "failed");
    assert!(
        rep.reason
            .as_deref()
            .unwrap_or_default()
            .contains("did not finish within 2 s"),
        "{:?}",
        rep.reason
    );
    assert!(p.log().iter().any(|e| e["got"] == "session/cancel"));
    assert!(began.elapsed() < std::time::Duration::from_secs(25));
}

#[test]
fn calls_speccoder_cannot_classify_need_a_person_and_fetches_are_refused() {
    let p = Project::new(
        &json!({ "steps": [
            permission("fetch", json!({ "url": "https://example.com" })),
            permission("switch_mode", json!({ "mode": "bypassPermissions" })),
            permission("other", json!({ "something": true })),
        ] }),
        "",
    );
    let res = p.run();
    let a = p.answers();
    assert_eq!(
        (chosen(&a[0]), chosen(&a[1])),
        (Some("reject"), Some("reject"))
    );
    assert_eq!(a[2]["result"]["outcome"]["outcome"], "cancelled");
    let rep = report(&res);
    assert_eq!(rep.attempt_status, "needs_approval");
    assert_eq!(rep.pending.as_ref().unwrap()["tool"], "agent_call");
    // Options the agent offers without an allow-once are never "always".
    let p = Project::new(
        &json!({ "steps": [
            { "permission": { "kind": "read", "rawInput": { "path": "src/lib.rs" } },
              "options": [{ "optionId": "yes", "kind": "allow_always", "name": "Always" },
                          { "optionId": "no", "kind": "reject_once", "name": "No" }] },
        ] }),
        "",
    );
    let _ = p.run();
    assert_eq!(chosen(&p.answers()[0]), Some("no"));
}

#[test]
fn an_agent_that_needs_signing_in_stops_with_its_own_login() {
    let p = Project::new(
        &json!({
            "init": { "authMethods": [{ "id": "copilot-login", "name": "Log in with Copilot CLI",
                "description": "Run `copilot login` in the terminal",
                "_meta": { "terminal-auth": { "command": "/opt/copilot", "args": ["login"] } } }] },
            "new_error": { "code": -32000, "message": "Authentication required" },
        }),
        "",
    );
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("EXECUTOR-LOGIN"));
    assert!(
        res.fix().contains(&"copilot login".to_owned()),
        "{:?}",
        res.fix()
    );
    let steps = serde_json::to_value(&res).unwrap()["fix_steps"].clone();
    assert_eq!(steps[0]["command"], "copilot login", "{steps}");
    assert_eq!(
        (steps[0]["how"].as_str(), steps[0]["interactive"].as_bool()),
        (Some("command"), Some(true)),
        "{steps}"
    );
}

#[test]
fn the_agent_is_asked_to_ask_first_and_for_the_model() {
    let modes = json!([
        { "id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": "agent",
          "options": [{ "value": "read-only", "name": "Read-only" }, { "value": "workspace-write", "name": "Write" },
                      { "value": "agent", "name": "Auto review" }, { "value": "agent-full-access", "name": "Full" }] },
        { "id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "m1",
          "options": [{ "value": "m1", "name": "M1" }, { "value": "m2", "name": "M2" }] },
    ]);
    let p = Project::new(
        &json!({
            "init": { "agentInfo": { "name": "@agentclientprotocol/codex-acp", "version": "2.1.1" } },
            "session": { "configOptions": modes },
            "steps": [{ "edit": "src/lib.rs", "content": EDIT }, { "say": "Done." }],
        }),
        "",
    );
    let toml = fs::read_to_string(p.dir.path().join("speccoder.toml")).unwrap();
    fs::write(
        p.dir.path().join("speccoder.toml"),
        format!("{toml}model = \"m2\"\n"),
    )
    .unwrap();
    git(p.dir.path(), &["commit", "-qam", "model"]);
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let set: Vec<Value> = p
        .log()
        .into_iter()
        .filter(|e| e["got"] == "session/set_config_option")
        .map(|e| e["params"].clone())
        .collect();
    assert!(!set.is_empty(), "{:?} {:?}", report(&res).notices, p.log());
    assert_eq!(set[0]["configId"], "mode");
    assert_eq!(set[0]["value"], "read-only");
    assert_eq!(set[1]["configId"], "model");
    assert_eq!(set[1]["value"], "m2");
    let rep = report(&res);
    assert!(
        rep.notices
            .iter()
            .any(|n| n.contains("asked") && n.contains("ask before acting")),
        "{:?}",
        rep.notices
    );
    assert!(
        rep.notices.iter().any(|n| n.ends_with("model: m2")),
        "{:?}",
        rep.notices
    );

    // An agent no recording knows is not asked anything, and the run says
    // it may decide alone.
    let p = Project::new(
        &json!({ "steps": [{ "edit": "src/lib.rs", "content": EDIT }, { "say": "Done." }] }),
        "",
    );
    let res = p.run();
    let rep = report(&res);
    assert!(
        rep.notices
            .iter()
            .any(|n| n.contains("not known whether it asks")),
        "{:?}",
        rep.notices
    );
    assert!(
        !p.log()
            .iter()
            .any(|e| e["got"] == "session/set_config_option")
    );
}

#[test]
fn the_old_claude_kind_is_refused_with_its_replacement() {
    let p = Project::new(&json!({ "steps": [] }), "");
    let toml = fs::read_to_string(p.dir.path().join("speccoder.toml")).unwrap();
    let cut = toml.find("[executor]").unwrap();
    fs::write(
        p.dir.path().join("speccoder.toml"),
        format!("{}[executor]\nkind = \"claude\"\n", &toml[..cut]),
    )
    .unwrap();
    git(p.dir.path(), &["commit", "-qam", "old"]);
    let res = p.run();
    assert_eq!(res.status(), CommandStatus::ConfigError);
    assert!(
        res.message().contains("agent = \"claude\""),
        "{}",
        res.message()
    );
}
