//! Ask end to end through its plane, with a scripted model: a conversation
//! is one run, citations are checked against the turn's own reads, a
//! proposal is only a sheet, and nothing in the project changes.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::ask::AskReport;
use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::ask::{self, Request, Settings};

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
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.ok]\nargv = [\"true\"]\n",
    )
    .unwrap();
    fs::write(r.join("notes.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &[
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=a@example.com",
            "add",
            "-A",
        ],
        &[
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=a@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(r)
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

fn model() -> speccoder_runtime::substrate::model_config::ModelConfig {
    speccoder_runtime::substrate::model_config::parse(
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap()
    .unwrap()
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d = Arc::clone(&f) as Arc<dyn ModelProvider>;
    (f, d)
}

fn ask(
    root: &Path,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    conversation: Option<&str>,
    text: &str,
) -> (CommandStatus, AskReport) {
    ask_with(
        root,
        state,
        provider,
        conversation,
        text,
        &Settings::default(),
    )
}

fn ask_with(
    root: &Path,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    conversation: Option<&str>,
    text: &str,
    settings: &Settings,
) -> (CommandStatus, AskReport) {
    let request = Request {
        conversation: conversation.map(str::to_owned),
        text: text.to_owned(),
        ..Request::default()
    };
    let res = ask::turn_with(
        root,
        &request,
        state,
        settings,
        &model(),
        Arc::clone(provider),
        None,
    );
    let Some(CommandData::Ask(r)) = res.data() else {
        panic!("{}", res.message());
    };
    (res.status(), (**r).clone())
}

fn status(root: &Path) -> String {
    String::from_utf8(
        Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
}

/// A conversation is one run: the second message continues the first's
/// transcript. A citation counts only for what this turn read.
#[test]
fn a_conversation_is_one_run_and_cites_only_its_own_reads() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "a",
        "read_file",
        json!({ "path": "notes.txt", "start_line": 1, "end_line": 3 }),
    );
    f.will_say("It counts [r1:1-2]; see also [r9] and [r1:2-4].");
    let (s, first) = ask(r.path(), state.path(), &d, None, "what is in notes.txt?");
    assert_eq!(s, CommandStatus::Ok, "{first:?}");
    assert_eq!(first.turn, 1);
    assert_eq!(first.reads.len(), 1);
    assert_eq!(first.reads[0].id, "r1");
    assert_eq!(
        (first.reads[0].start, first.reads[0].end),
        (Some(1), Some(3))
    );
    let states: Vec<_> = first
        .citations
        .iter()
        .map(|c| (c.marker.as_str(), c.state.as_str()))
        .collect();
    assert_eq!(
        states,
        [
            ("[r1:1-2]", "read"),
            ("[r9]", "not_read"),
            ("[r1:2-4]", "not_read")
        ]
    );

    f.will_say("Still the same [r1].");
    let (_, second) = ask(
        r.path(),
        state.path(),
        &d,
        Some(&first.conversation),
        "and line two?",
    );
    assert_eq!(second.conversation, first.conversation);
    assert_eq!(second.turn, 2);
    // Read in the first turn, not in this one.
    assert_eq!(second.citations[0].state, "not_read");
    let asked = f.asked();
    let last = asked.last().unwrap();
    assert_eq!(last.told.last().map(String::as_str), Some("and line two?"));
    assert!(
        last.continuation.is_some(),
        "the second turn did not continue the conversation"
    );

    let list = ask::conversations(r.path(), Some(state.path()), None);
    let Some(CommandData::AskConversations(c)) = list.data() else {
        panic!()
    };
    assert_eq!(c.conversations.len(), 1);
    assert_eq!(c.conversations[0].turns, 2);
    assert_eq!(c.conversations[0].title, "what is in notes.txt?");
    assert_eq!(status(r.path()), "", "Ask changed the project");
}

/// A change request is a proposal code checked: shown, never performed.
#[test]
fn a_proposal_is_a_sheet_and_nothing_runs() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "p",
        "propose",
        json!({ "kind": "quick_change", "title": "Make the limit configurable", "request": "read the retry limit from speccoder.toml", "checks": ["the default stays 3"] }),
    );
    f.will_call_tool(
        "q",
        "propose",
        json!({ "kind": "nudge", "title": "again", "target": "T001", "note": "x" }),
    );
    f.will_say("I suggested a quick change.");
    let (_, t) = ask(
        r.path(),
        state.path(),
        &d,
        None,
        "make the retry limit configurable",
    );
    let p = t.proposal.expect("a proposal");
    assert_eq!(p.kind, "quick_change");
    assert!(p.blocked.is_none(), "{p:?}");
    assert!(matches!(
        p.runnable,
        Some(speccoder_core::actions::Runnable::QuickChange { run: true, .. })
    ));
    assert_eq!(
        p.command,
        "speccoder do \"read the retry limit from speccoder.toml\" --check \"the default stays 3\""
    );
    assert_eq!(status(r.path()), "", "a proposal changed the project");
}

/// Writing is not among Ask's tools; a credential is not readable.
#[test]
fn ask_cannot_write_or_read_secrets() {
    let r = repo();
    fs::write(r.path().join(".env"), "KEY=secret\n").unwrap();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "notes.txt", "content": "gone" }),
    );
    f.will_call_tool("e", "read_file", json!({ "path": ".env" }));
    f.will_say("I cannot.");
    let (_, t) = ask(r.path(), state.path(), &d, None, "rewrite notes.txt");
    assert!(t.reads.is_empty(), "{:?}", t.reads);
    assert_eq!(
        fs::read_to_string(r.path().join("notes.txt")).unwrap(),
        "one\ntwo\nthree\nfour\n"
    );
    assert!(!f.asked().iter().any(|a| {
        a.continuation
            .as_ref()
            .is_some_and(|c| c.state.to_string().contains("KEY=secret"))
    }));
}

/// The day's tokens spent: no model call, the turn says which limit.
#[test]
fn a_spent_day_makes_no_model_call() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let settings = Settings {
        daily_tokens: 0,
        ..Settings::default()
    };
    let (s, t) = ask_with(r.path(), state.path(), &d, None, "anything?", &settings);
    assert_eq!(s, CommandStatus::NeedsHuman);
    assert_eq!(t.end, "budget");
    assert!(t.reason.unwrap().contains("[ask] daily_tokens"));
    assert_eq!(f.calls(), 0);
}

/// The status readers work inside the plane (they open runtimes of their
/// own), and every read has a name a person can follow.
#[test]
fn the_readers_answer_and_are_named() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool("a", "report", json!({}));
    f.will_call_tool("b", "inbox", json!({}));
    f.will_call_tool("c", "explain", json!({ "code": "BLOCKED" }));
    f.will_call_tool("e", "git_log", json!({ "n": 5 }));
    f.will_say("Done [r1] [r2] [r3] [r4].");
    let (_, t) = ask(r.path(), state.path(), &d, None, "what is going on?");
    let targets: Vec<&str> = t.reads.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        ["the report", "Needs You", "code BLOCKED", "git history"]
    );
    assert!(
        t.citations.iter().all(|c| c.state == "read"),
        "{:?}",
        t.citations
    );
}

/// The scripted model, and a person who presses Stop while its `at`-th
/// answer is being written.
#[derive(Debug)]
struct StopsMidTurn {
    inner: Arc<FakeProvider>,
    at: usize,
    seen: std::sync::Mutex<usize>,
    conversation: std::sync::Mutex<Option<String>>,
}

#[agentplane::prelude::async_trait]
impl ModelProvider for StopsMidTurn {
    async fn complete(
        &self,
        request: agentplane::model::Request<'_>,
    ) -> Result<agentplane::model::Completion, agentplane::model::ModelError> {
        let n = {
            let mut seen = self.seen.lock().unwrap();
            *seen += 1;
            *seen
        };
        if n == self.at {
            let id = self.conversation.lock().unwrap().clone().unwrap();
            assert!(ask::stop(Some(&id)), "no turn in flight to stop");
        }
        self.inner.complete(request).await
    }

    fn gen_ai_provider(&self) -> Option<&str> {
        None
    }
}

/// Stop ends the turn at its next step and the run with it; the next
/// question goes on in the same conversation, carrying the earlier words.
#[test]
fn a_stopped_turn_keeps_its_words_and_the_conversation_goes_on() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let stops = Arc::new(StopsMidTurn {
        inner: Arc::clone(&f),
        at: 2,
        seen: std::sync::Mutex::new(0),
        conversation: std::sync::Mutex::new(None),
    });
    let d = Arc::clone(&stops) as Arc<dyn ModelProvider>;
    f.will_say("notes.txt has four lines.");
    let (_, first) = ask(r.path(), state.path(), &d, None, "what is in notes.txt?");
    *stops.conversation.lock().unwrap() = Some(first.conversation.clone());

    // Stopped while the second turn's first answer is written: the
    // tool it asked for never runs.
    f.will_call_tool("a", "read_file", json!({ "path": "notes.txt" }));
    let (s, second) = ask(
        r.path(),
        state.path(),
        &d,
        Some(&first.conversation),
        "and the last line?",
    );
    assert_eq!(s, CommandStatus::Ok);
    assert_eq!(second.end, "stopped", "{second:?}");
    assert!(second.reads.is_empty(), "{:?}", second.reads);
    assert_eq!(f.calls(), 2, "the stopped turn called the model again");

    f.will_say("The last line is four.");
    let (s, third) = ask(
        r.path(),
        state.path(),
        &d,
        Some(&first.conversation),
        "the last line, please",
    );
    assert_eq!(s, CommandStatus::Ok, "{third:?}");
    assert_eq!(third.conversation, first.conversation);
    assert_eq!(third.turn, 3);
    assert_eq!(third.answer, "The last line is four.");
    let told = f.asked().last().unwrap().told.join("\n");
    assert!(
        told.contains("<context id=\"earlier\"")
            && told.contains("Q: what is in notes.txt?\nA: notes.txt has four lines."),
        "the new run did not carry the earlier turns: {told}"
    );
    assert!(!ask::stop(None), "a turn is still listed in flight");
}

/// Each kind is checked against the project now; a blocked one says why
/// and carries nothing to run.
#[test]
fn each_kind_of_proposal_is_checked_now() {
    let r = repo();
    let check = |args: serde_json::Value| ask::proposal::validate(r.path(), None, "c-1", &args);

    let open = check(
        json!({ "kind": "nudge", "title": "t", "target": "T005", "note": "use the fixture" }),
    );
    assert!(open.blocked.is_none(), "{open:?}");
    assert_eq!(open.input.as_deref(), Some("use the fixture"));
    assert_eq!(
        open.command,
        "speccoder task nudge T005 --note \"use the fixture\""
    );
    for (target, why) in [("T001", "already ticked"), ("T099", "no task T099")] {
        let p = check(json!({ "kind": "nudge", "target": target, "note": "x" }));
        assert!(p.runnable.is_none(), "{p:?}");
        assert!(
            p.blocked.as_deref().unwrap_or_default().contains(why),
            "{p:?}"
        );
    }

    let bug = check(json!({
        "kind": "bug_fix",
        "symptom": "login fails on a trailing space",
        "expected": ["a trailing space is trimmed"],
        "unchanged": ["passwords keep their spaces"],
    }));
    assert_eq!(
        bug.runnable,
        Some(speccoder_core::actions::Runnable::BugFix {
            symptom: "login fails on a trailing space".to_owned(),
            expected: vec!["a trailing space is trimmed".to_owned()],
            unchanged: vec!["passwords keep their spaces".to_owned()],
            issue: None,
            tests: Vec::new(),
            candidate: None,
            slug: None,
            run: true,
            asked: Some("c-1".to_owned()),
        })
    );
    let vague = check(json!({ "kind": "bug_fix", "symptom": "it breaks" }));
    assert!(vague.runnable.is_none());
    assert!(vague.blocked.unwrap().contains("should happen instead"));

    let spec = check(json!({ "kind": "start_spec", "description": "profiles get avatars" }));
    assert!(matches!(
        spec.runnable,
        Some(speccoder_core::actions::Runnable::PlanThrough { .. })
    ));

    let question = check(json!({ "kind": "answer", "target": "Q9", "answer": "yes" }));
    assert!(question.runnable.is_none(), "{question:?}");

    let unknown = check(json!({ "kind": "delete_everything" }));
    assert!(unknown.blocked.unwrap().contains("not a kind of proposal"));

    // The person's edit is checked again: a request added unblocks it.
    let mut edited = check(json!({ "kind": "quick_change" }));
    assert!(edited.blocked.is_some());
    edited.fields.request = Some("trim the login name".to_owned());
    let now = ask::proposal::revalidate(r.path(), None, &edited);
    assert!(now.blocked.is_none(), "{now:?}");
    assert_eq!(now.command, "speccoder do \"trim the login name\"");
}
