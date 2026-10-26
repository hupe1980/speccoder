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
            digest: None,
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
            digest: None,
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
    let r = repo("\n[commands]\nallow = [[\"grep\", \"-q\", \"ok\", \"src/lib.rs\"]]\n");
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
    assert_eq!(
        rules[1],
        ("rule", "exact command `grep -q ok src/lib.rs`"),
        "{calls:?}"
    );
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

    // A writer that holds the journal alone (as speccoder 0.2.0 did): it
    // cannot be read beside.
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

/// A decisions file that cannot be read (an earlier shape, a torn write)
/// leaves a lane's stopped calls unknown, never none: the listing and the
/// inbox say so, and nothing is decided against it.
#[test]
fn decisions_that_cannot_be_read_are_unknown_never_none() {
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
    std::fs::write(
        plane_dir(r.path(), state.path()).join(speccoder_runtime::approvals::FILE),
        "{\"tasks\": {\"001-graph/T005\": {\"grants\": [\"write\"]}}}",
    )
    .unwrap();
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    let Some(CommandData::Approvals(a)) = listed.data() else {
        panic!("{listed:?}");
    };
    let why = a.unreadable.as_deref().unwrap_or_default();
    assert!(why.contains(speccoder_runtime::approvals::FILE), "{why}");
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{inbox:?}");
    };
    assert!(i.items.iter().any(|i| i.kind == "unknown"), "{:?}", i.items);
    let approve = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert_eq!(
        approve.status(),
        CommandStatus::ConfigError,
        "{}",
        approve.message()
    );
}

// ── a journal another process writes ────────────────────────────────

const WRITER: &str = "SPECCODER_TEST_JOURNAL_WRITER";

/// Wait for a marker file the other process writes.
fn until(path: &Path) {
    for _ in 0..600 {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{} never appeared", path.display());
}

/// The writer half of `a_run_in_another_process_leaves_the_journal_readable`,
/// run in a child process: opens the project's journal to write, as a run
/// or a decision does, and holds it until told.
#[test]
#[ignore = "the writer half of a_run_in_another_process_leaves_the_journal_readable, run in a child process"]
fn the_other_process_writer() {
    let Ok(dir) = std::env::var(WRITER).map(PathBuf::from) else {
        return;
    };
    let plane_dir = PathBuf::from(fs::read_to_string(dir.join("plane")).unwrap());
    let plane = speccoder_runtime::substrate::Plane::open_for_decisions(&plane_dir)
        .unwrap_or_else(|e| panic!("{}", e.0));
    fs::write(dir.join("ready"), "").unwrap();
    until(&dir.join("done"));
    drop(plane);
}

/// **Another process writes the journal; this one reads it.** The waiting
/// call is listed (not `unknown`) in the approvals, the inbox and status,
/// the run's account reads, all without waiting for the writer — and a
/// second writer (a decision, a run) is refused at once, the file untouched.
#[test]
fn a_run_in_another_process_leaves_the_journal_readable() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_answer(agentplane::model::Completion {
        tool_calls: vec![agentplane::model::ToolCall {
            id: "c1".to_owned(),
            name: "write_file".to_owned(),
            arguments: json!({ "path": "package.json", "content": "{}\n" }),
        }],
        text: "I'll add the manifest.".to_owned(),
        model: None,
        usage: agentplane::model::Usage::default(),
        stop_reason: Some("tool_use".to_owned()),
        truncated: false,
        commentary: None,
        compacted: None,
        structured: None,
        continuation: None,
        reasoning: None,
    });
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let run_id = report(&first).run_id.clone();
    let journal = plane_dir(r.path(), state.path());

    let markers = tempfile::tempdir().unwrap();
    fs::write(
        markers.path().join("plane"),
        journal.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "the_other_process_writer",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env(WRITER, markers.path())
        .spawn()
        .unwrap();
    until(&markers.path().join("ready"));
    let before = fs::read(journal.join(speccoder_runtime::substrate::JOURNAL)).unwrap();

    let timed = |what: &str, started: std::time::Instant| {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{what} waited {:?} for the other process",
            started.elapsed()
        );
    };
    // The approvals list the waiting call itself.
    let started = std::time::Instant::now();
    let listed = speccoder_runtime::approvals::list_with(r.path(), None, state.path());
    timed("the listing", started);
    let Some(CommandData::Approvals(a)) = listed.data() else {
        panic!("{listed:?}");
    };
    assert_eq!(a.pending.len(), 1, "{}", listed.message());
    assert!(a.unreadable.is_none(), "{:?}", a.unreadable);
    // So does the inbox.
    let started = std::time::Instant::now();
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    timed("the inbox", started);
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{inbox:?}");
    };
    assert!(
        i.items.iter().all(|i| i.kind != "unknown") && !i.items.is_empty(),
        "{:?}",
        i.items
    );
    // The model's account of the run.
    let started = std::time::Instant::now();
    let account = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(speccoder_runtime::substrate::account(&journal, &run_id))
        .unwrap_or_else(|e| panic!("{}", e.0));
    timed("the account", started);
    assert_eq!(
        account.first().and_then(|t| t.said.as_deref()),
        Some("I'll add the manifest."),
        "{account:?}"
    );

    // A second writer is refused at once: a decision…
    let started = std::time::Instant::now();
    let approve = decide_with(
        r.path(),
        None,
        Verb::Approve("T005"),
        state.path(),
        speccoder_core::actions::Via::Terminal,
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    assert_ne!(approve.status(), CommandStatus::Ok, "{}", approve.message());
    assert!(
        approve.message().contains("another speccoder"),
        "{}",
        approve.message()
    );
    // …and a run.
    let (_, d2) = fake();
    let second = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d2, None);
    assert_ne!(second.status(), CommandStatus::Ok, "{}", second.message());
    assert!(
        second.message().contains("another speccoder"),
        "{}",
        second.message()
    );
    assert_eq!(
        fs::read(journal.join(speccoder_runtime::substrate::JOURNAL)).unwrap(),
        before,
        "a refused writer or a reader touched the journal"
    );

    fs::write(markers.path().join("done"), "").unwrap();
    assert!(child.wait().unwrap().success(), "the writer process failed");
    // Once it is gone, the call is decided as ever.
    assert_eq!(pending(r.path(), state.path()).len(), 1);
}

/// A waiting call shows what the model said when it asked: its
/// commentary, where the provider marks it apart from the answer.
#[test]
fn a_waiting_call_shows_the_model_s_commentary() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_answer(agentplane::model::Completion {
        tool_calls: vec![agentplane::model::ToolCall {
            id: "c1".to_owned(),
            name: "write_file".to_owned(),
            arguments: json!({ "path": "package.json", "content": "{}\n" }),
        }],
        text: String::new(),
        model: None,
        usage: agentplane::model::Usage::default(),
        stop_reason: None,
        truncated: false,
        commentary: Some("The manifest comes first.".to_owned()),
        compacted: None,
        structured: None,
        continuation: None,
        reasoning: None,
    });
    let first = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
    let waiting = pending(r.path(), state.path());
    assert_eq!(
        waiting[0]
            .presentation
            .as_ref()
            .and_then(|p| p.claim.as_deref()),
        Some("The manifest comes first."),
        "{waiting:?}"
    );
}
