//! The attempt's thread, a Stop that stops, and Explain, with a
//! scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::thread::RowKind;
use speccoder_core::{CommandData, CommandResult, RunReport};
use speccoder_runtime::approvals::{Answer, Approver, Presentation, Scope, Who};

const BASE: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"]]\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../speccoder-speckit/tests/fixtures/graph/.")
            )
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("README.md"), "hello\n").unwrap();
    fs::write(r.join("speccoder.toml"), BASE).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn sandboxed() -> bool {
    speccoder_runtime::sandbox::backend().is_ok()
}

fn dana() -> Who {
    Who {
        name: "dana".to_owned(),
        via: "web".to_owned(),
        client: None,
        basis: "asserted".to_owned(),
    }
}

/// Approves every call.
#[derive(Debug)]
struct Yes;
impl Approver for Yes {
    fn ask(&self, _: &Presentation) -> Answer {
        Answer::approve(Scope::Once, None, Some(dana()))
    }
}

/// Presses Stop when a call waits, then approves it: the call in flight
/// completes, nothing after it runs.
#[derive(Debug)]
struct StopThenYes;
impl Approver for StopThenYes {
    fn ask(&self, _: &Presentation) -> Answer {
        let asked = speccoder_runtime::substrate::stop_live(None, "dana", "web");
        assert_eq!(asked, ["001-graph/T005"], "the live attempt is known");
        Answer::approve(Scope::Once, None, Some(dana()))
    }
}

#[test]
fn an_attempt_reads_as_one_thread_with_evidence_last() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_call_tool("c2", "read_file", json!({ "path": "README.md" }));
    f.will_call_tool("c3", "search_code", json!({ "pattern": "fn ok" }));
    f.will_call_tool("c4", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_call_tool(
        "c5",
        "run_command",
        json!({ "argv": ["echo", "online"], "network": true }),
    );
    f.will_structure(json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        Arc::new(Yes),
    );
    let rep = report(&res);
    let thread =
        speccoder_runtime::thread::of(r.path(), state.path(), "001-graph", "T005", None, false)
            .unwrap();
    let kinds: Vec<RowKind> = thread.rows.iter().map(|x| x.kind).collect();
    // Resolving a decision's object is speccoder's step, not the model's call.
    assert!(
        !thread
            .rows
            .iter()
            .any(|x| format!("{:?}{:?}", x.verb, x.object).contains("resolve")),
        "{:#?}",
        thread.rows
    );
    // Three reads fold; the edit and the command follow; the evidence ends it.
    assert_eq!(thread.rows[0].kind, RowKind::Fold, "{:#?}", thread.rows);
    assert_eq!(thread.rows[0].text, "Read 2 files · searched 1");
    let edit = thread
        .rows
        .iter()
        .find(|x| x.verb.as_deref() == Some("Edited"))
        .unwrap();
    assert_eq!((edit.added, edit.removed), (Some(1), Some(0)));
    assert!(
        edit.detail
            .as_deref()
            .unwrap()
            .contains("+ pub fn login() {}"),
        "{edit:?}"
    );
    let ran = thread
        .rows
        .iter()
        .find(|x| x.verb.as_deref() == Some("Ran"))
        .unwrap();
    assert_eq!(ran.object.as_deref(), Some("echo online"));
    assert_eq!(ran.decided.as_deref(), Some("you"), "{ran:?}");
    assert_eq!(ran.outcome.as_deref(), Some("ok"));
    let last = thread.rows.last().unwrap();
    assert_eq!(last.kind, RowKind::Divider, "{kinds:?}");
    if rep.ticked {
        assert!(last.text.starts_with("Ticked T005"), "{last:?}");
        // Only a verified tick is drawn as proof.
        let tone = if last.text.ends_with("verified") {
            "verified"
        } else {
            "regression"
        };
        assert_eq!(last.tone.as_deref(), Some(tone), "{last:?}");
    }
    assert_eq!(thread.attempts.len(), 1);
    assert!(!thread.live);
}

#[test]
fn a_contained_command_says_so() {
    if !sandboxed() {
        return;
    }
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["python3", "-c", "print(1)"] }),
    );
    f.will_structure(json!({ "summary": "nothing", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let _ = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let thread =
        speccoder_runtime::thread::of(r.path(), state.path(), "001-graph", "T005", None, false)
            .unwrap();
    let ran = thread
        .rows
        .iter()
        .find(|x| x.verb.as_deref() == Some("Ran"))
        .unwrap();
    assert_eq!(ran.decided.as_deref(), Some("contained"), "{ran:?}");
}

/// A Stop lands before the attempt's next call: the call in flight
/// completes, no model turn follows, and the attempt is saved as stopped
/// by the person — never as failed.
#[test]
fn stop_ends_the_attempt_at_its_next_call() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["echo", "online"], "network": true }),
    );
    f.will_call_tool("c3", "read_file", json!({ "path": "README.md" }));
    f.will_call_tool("c4", "read_file", json!({ "path": "README.md" }));
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let began = std::time::Instant::now();
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        Arc::new(StopThenYes),
    );
    let rep = report(&res);
    // Two model turns: the read, the command; none after the stop.
    assert_eq!(f.asked().len(), 2, "{}", res.message());
    assert_eq!(rep.attempt_status, "stopped", "{}", res.message());
    assert!(!rep.ticked);
    assert!(began.elapsed() < std::time::Duration::from_secs(30));
    let thread =
        speccoder_runtime::thread::of(r.path(), state.path(), "001-graph", "T005", None, false)
            .unwrap();
    let last = thread.rows.last().unwrap();
    assert_eq!(last.kind, RowKind::Divider);
    assert_eq!(last.tone.as_deref(), Some("stopped"), "{last:?}");
    assert!(last.text.contains("dana"), "{last:?}");
    assert_eq!(thread.attempts[0].outcome, "stopped");
    // The saved attempt names who stopped it; `task.stopped` carries it.
    let log = Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%B",
            "--glob=refs/speccoder/attempts/*",
        ])
        .current_dir(r.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&log.stdout).contains("Stopped-By: dana"),
        "{}",
        String::from_utf8_lossy(&log.stdout)
    );
}

/// Stop pressed while the attempt runs a command: the command completes,
/// no model turn follows it.
#[test]
fn stop_during_a_running_command_ends_the_attempt_after_it() {
    if !sandboxed() {
        return;
    }
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["python3", "-c", "import time; time.sleep(2)"] }),
    );
    f.will_call_tool("c2", "read_file", json!({ "path": "README.md" }));
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let stopper = std::thread::spawn(|| {
        for _ in 0..200 {
            if speccoder_runtime::substrate::live_run("001-graph/T005").is_some() {
                std::thread::sleep(std::time::Duration::from_millis(300));
                return speccoder_runtime::substrate::stop_live(None, "dana", "web");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Vec::new()
    });
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(stopper.join().unwrap(), ["001-graph/T005"]);
    let rep = report(&res);
    assert_eq!(f.asked().len(), 1, "{}", res.message());
    assert_eq!(rep.attempt_status, "stopped", "{}", res.message());
    // Run again: a fresh attempt, not the stopped one replayed.
    let f2 = FakeProvider::new();
    f2.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f2.will_structure(json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d2: Arc<dyn ModelProvider> = f2.clone();
    let again = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d2, None);
    assert!(!f2.asked().is_empty(), "{}", again.message());
    assert_ne!(
        report(&again).attempt_status,
        "stopped",
        "{}",
        again.message()
    );
}

/// Explain: a waiting network command's facts, with no model asked; the
/// thread shows the call waiting in place.
#[test]
fn explain_gives_a_waiting_calls_facts_without_a_model() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["curl", "-s", "https://crates.io/api/v1/crates/serde"], "network": true }),
    );
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        res.status(),
        speccoder_core::CommandStatus::NeedsHuman,
        "{}",
        res.message()
    );
    let asked = f.asked().len();
    let listed = speccoder_runtime::approvals::list_with(r.path(), Some("001-graph"), state.path());
    let Some(CommandData::Approvals(a)) = listed.data() else {
        panic!("{}", listed.message())
    };
    let call = a.pending[0].id();
    let began = std::time::Instant::now();
    let x =
        speccoder_runtime::thread::explain(r.path(), state.path(), "001-graph/T005", Some(&call))
            .unwrap();
    // SC-003: under a second in a release build; debug builds run several times slower.
    let bound = if cfg!(debug_assertions) { 4 } else { 1 };
    assert!(began.elapsed() < std::time::Duration::from_secs(bound));
    assert_eq!(f.asked().len(), asked, "Explain asked the model");
    assert_eq!(x.program.as_deref(), Some("curl"));
    assert_eq!(x.hosts, ["crates.io"]);
    assert!(!x.rule.is_empty());
    assert_eq!(x.task, "001-graph/T005");
    let thread =
        speccoder_runtime::thread::of(r.path(), state.path(), "001-graph", "T005", None, false)
            .unwrap();
    let waiting = thread
        .rows
        .iter()
        .find(|x| x.kind == RowKind::Approval)
        .expect("a waiting row");
    // The waiting attempt's own record is read: its words come before the call.
    assert!(
        !thread
            .rows
            .iter()
            .any(|x| x.text.contains("cannot be read")),
        "{:#?}",
        thread.rows
    );
    assert_eq!(
        thread.attempt.as_ref().map(|a| a.outcome.as_str()),
        Some("waiting")
    );
    assert_eq!(waiting.outcome.as_deref(), Some("waiting"));
    assert_eq!(waiting.call.as_deref(), Some(call.as_str()));
}
