//! Approvals as agentplane human tasks: every decision names who
//! made it and how that is known, binds to what was shown, waits durably,
//! and expires as nobody's decision.

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
use speccoder_runtime::approvals::{Answer, Approver, Scope, Verb, decide_with};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[acceptance]\ngenerate = false\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn repo(extra: &str) -> tempfile::TempDir {
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
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    git(r, &["init", "-q"]);
    // Whoever decides here is named as git names them.
    git(r, &["config", "user.name", "Dana Example"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn done() -> Value {
    json!({ "summary": "done", "status": "done", "files_changed": ["package.json"], "checks_run": [], "notes": [] })
}

/// A dependency manifest: a write that needs a person.
fn write_manifest(f: &FakeProvider, id: &str) {
    f.will_call_tool(
        id,
        "write_file",
        json!({ "path": "package.json", "content": "{}\n" }),
    );
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn pending(r: &Path, state: &Path) -> Vec<speccoder_core::PendingCall> {
    match speccoder_runtime::approvals::list_with(r, None, state).data() {
        Some(CommandData::Approvals(a)) => a.pending.clone(),
        other => panic!("{other:?}"),
    }
}

/// The project's state directory, where its journal lives.
fn plane_dir(r: &Path, state: &Path) -> PathBuf {
    speccoder_runtime::worktree::project_state_dir(state, &fs::canonicalize(r).unwrap())
}

#[derive(Debug)]
struct Scripted(std::sync::Mutex<Vec<Answer>>);

impl Approver for Scripted {
    fn ask(&self, _: &speccoder_runtime::approvals::Presentation) -> Answer {
        self.0.lock().unwrap().remove(0)
    }
}

fn person(answers: Vec<Answer>) -> Arc<Scripted> {
    Arc::new(Scripted(std::sync::Mutex::new(answers)))
}

#[test]
fn inline_decisions_name_their_operator_and_replay_without_asking() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["curl", "https://example.com"] }),
    );
    f.will_structure(done());
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        person(vec![
            Answer::approve(Scope::Task, Some("small is fine".to_owned()), None),
            Answer::reject("no network", None),
        ]),
    );
    let rep = report(&res);
    let (write, curl) = (&rep.tool_calls[0], &rep.tool_calls[1]);
    // Who decided, and how that is known: git's name, typed at a terminal.
    assert_eq!(write["approved_by"], "person");
    assert_eq!(
        write["operator"],
        json!({ "actor": "Dana Example", "basis": "asserted" })
    );
    assert_eq!(write["scope"], "task");
    assert_eq!(write["note"], "small is fine");
    assert_eq!(curl["decision"], "deny");
    assert_eq!(curl["rejected_by"], "person");
    assert_eq!(curl["operator"]["actor"], "Dana Example");
    assert_eq!(
        (write["by"].as_str(), curl["by"].as_str()),
        (Some("person"), Some("person"))
    );
    // Strict replay reads both decisions back; nobody is asked.
    let replayed = speccoder_runtime::replay_with(r.path(), &rep.run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
}

#[test]
fn an_approval_of_something_other_than_what_was_shown_is_refused() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let waiting = pending(r.path(), state.path());
    assert_eq!(waiting.len(), 1);
    // `status` says what waits, and the way forward.
    let status = speccoder_runtime::progress::status_with(r.path(), state.path());
    assert!(
        status
            .message()
            .contains("waiting for a person: 001-graph/T005 — write `package.json`"),
        "{}",
        status.message()
    );
    assert_eq!(status.next()[0].command, "speccoder report --approvals");
    let id = agentplane::core::TaskId::parse(&waiting[0].call).unwrap();

    // A decision naming a different version of the call.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let store = Arc::new(
        agentplane::store::RedbStore::open(plane_dir(r.path(), state.path()).join("journal.redb"))
            .unwrap(),
    );
    let plane = agentplane::prelude::Runtime::builder_on(store)
        .try_build()
        .unwrap();
    let other = agentplane::core::Justification::new(
        agentplane::core::Tainted::trusted("something else".to_owned()),
        json!({ "tool": "write_file", "arguments": { "path": "package.json", "content": "evil" } }),
    );
    let refused = rt.block_on(plane.decide_task_at(
        id,
        &agentplane::core::Decision::approve(
            agentplane::core::Operator::asserted("mallory").unwrap(),
            "",
        ),
        &[],
        Some(other.digest()),
    ));
    assert!(refused.is_err(), "a stale approval was accepted");
    drop(plane);
    drop(rt);
    // The call still waits; nothing was written.
    assert_eq!(pending(r.path(), state.path()).len(), 1);
    assert!(!r.path().join("package.json").exists());
}

#[test]
fn a_waiting_call_past_its_deadline_is_denied_as_nobodys_decision() {
    let r = repo("\n[approvals]\nwait = \"1m\"\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let waiting = pending(r.path(), state.path());
    assert_eq!(waiting.len(), 1);
    let deadline = waiting[0].deadline.unwrap();
    let since = waiting[0].waiting_since.unwrap();
    assert!(
        (50..=120).contains(&(deadline - since)),
        "{since} → {deadline}"
    );

    // Two minutes later: the deadline has passed.
    let later = agentplane::core::Timestamp::now_utc() + std::time::Duration::from_secs(120);
    speccoder_runtime::approvals::expire_due(&plane_dir(r.path(), state.path()), later).unwrap();

    // The attempt continues with the call refused, and nobody is named.
    let (f2, d2) = fake();
    f2.will_structure(done());
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d2, None);
    let rep = report(&res);
    let call = &rep.tool_calls[0];
    assert_eq!(call["decision"], "deny", "{:?}", rep.tool_calls);
    assert_eq!(call["expired"], true);
    assert_eq!(call["by"], "expiry");
    assert!(call.get("operator").is_none_or(Value::is_null));
    assert!(
        call["reason"]
            .as_str()
            .unwrap()
            .contains("not approved before its deadline"),
        "{call}"
    );
    assert!(pending(r.path(), state.path()).is_empty());
}

#[test]
fn a_waiting_task_holds_the_worktree_until_it_is_decided() {
    let r = repo("");
    // Two tasks that could run now.
    fs::write(
        r.path().join("specs/001-graph/tasks.md"),
        "# Tasks\n\n## Phase 1: Setup\n\n- [ ] T005 [P] Add the manifest\n- [ ] T006 [P] Add another manifest\n",
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "two tasks"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );

    // T005's attempt holds the worktree: T006 does not start beside it.
    let (_f, d) = fake();
    let other = speccoder_runtime::run_with(r.path(), None, "T006", state.path(), d, None);
    assert_eq!(
        other.status(),
        CommandStatus::NeedsHuman,
        "{}",
        other.message()
    );
    assert!(
        other.message().contains("T005 waits for a person"),
        "{}",
        other.message()
    );

    // `--all` decides every waiting call, each on its own record.
    let all = decide_with(
        r.path(),
        None,
        Verb::Decide(speccoder_runtime::approvals::Choice {
            task: None,
            call: None,
            all: true,
            approve: true,
            scope: Scope::Once,
            note: None,
            reason: None,
        }),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(all.status(), CommandStatus::Ok, "{}", all.message());
    assert_eq!(
        all.message().matches("approved:").count(),
        1,
        "{}",
        all.message()
    );
    // A call id that waits nowhere is not found.
    let none = decide_with(
        r.path(),
        None,
        Verb::Decide(speccoder_runtime::approvals::Choice {
            task: None,
            call: Some("ffff"),
            all: false,
            approve: true,
            scope: Scope::Once,
            note: None,
            reason: None,
        }),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(none.status(), CommandStatus::NotFound, "{}", none.message());

    // T005 resumes and finishes; then T006 runs.
    let (f, d) = fake();
    f.will_structure(done());
    let resumed = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(resumed.status(), CommandStatus::Ok, "{}", resumed.message());
    assert_eq!(report(&resumed).tool_calls[0]["scope"], "once");
    assert_eq!(f.asked().len(), 1, "a finished call was repeated");
    let (f, d) = fake();
    f.will_structure(done());
    let next = speccoder_runtime::run_with(r.path(), None, "T006", state.path(), d, None);
    assert_eq!(next.status(), CommandStatus::Ok, "{}", next.message());
}

#[test]
fn calls_decided_without_a_person_name_the_rule() {
    let r = repo("\n[commands]\nallow = [[\"grep\"]]\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("c1", "read_file", json!({ "path": "src/lib.rs" }));
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["grep", "-q", "ok", "src/lib.rs"] }),
    );
    f.will_call_tool(
        "c3",
        "write_file",
        json!({ "path": ".specify/memory/constitution.md", "content": "x" }),
    );
    f.will_structure(done());
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let calls = &report(&res).tool_calls;
    let rules: Vec<(&str, &str)> = calls
        .iter()
        .map(|c| {
            (
                c["by"].as_str().unwrap_or("-"),
                c["rule"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(rules[0], ("rule", "workspace read"), "{calls:?}");
    assert_eq!(rules[1], ("rule", "allowed command `grep`"), "{calls:?}");
    assert_eq!(rules[2].0, "rule", "{calls:?}");
    // No call a rule decided is recorded as a person's.
    assert!(calls.iter().all(|c| c["by"] != "person"));
}

#[test]
fn a_resumed_attempt_that_fails_is_undone_to_where_the_task_started() {
    // Found by dogfooding: the undo went to the tree the resumed run found,
    // which already held the attempt's own writes.
    let r = repo("\n[checks.no-manifest]\nargv = [\"sh\", \"-c\", \"test ! -e package.json\"]\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let ok = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());

    let (f, d) = fake();
    f.will_structure(done());
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let rep = report(&res);
    assert!(
        !rep.ticked && rep.saved_attempt.is_some(),
        "{}",
        res.message()
    );
    let wt = Path::new(&rep.worktree);
    assert!(
        !wt.join("package.json").exists(),
        "the failed attempt's write is still there"
    );
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt)
        .output()
        .unwrap();
    assert!(
        status.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
}

#[test]
fn calls_waiting_in_a_journal_another_process_holds_are_unknown_never_none() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    write_manifest(&f, "c1");
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    assert_eq!(pending(r.path(), state.path()).len(), 1);

    // Another holder of the journal (as another speccoder process would be).
    let held =
        agentplane::store::RedbStore::open(plane_dir(r.path(), state.path()).join("journal.redb"))
            .unwrap();
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let Some(CommandData::Approvals(a)) = listed.data() else {
        panic!("{listed:?}");
    };
    assert!(a.pending.is_empty());
    assert!(
        a.unreadable.is_some(),
        "an unreadable journal was reported as nothing waiting"
    );
    assert!(listed.message().contains("unknown"), "{}", listed.message());
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{inbox:?}");
    };
    assert_eq!(
        i.items.first().map(|i| i.kind.as_str()),
        Some("unknown"),
        "{:?}",
        i.items
    );
    // A decision cannot be made blind.
    let approve = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(
        approve.status(),
        CommandStatus::Error,
        "{}",
        approve.message()
    );

    drop(held);
    assert_eq!(pending(r.path(), state.path()).len(), 1);
}
