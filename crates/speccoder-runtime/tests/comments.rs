//! Anchored comments: drafts in the tray, one scoped
//! revision turn of the revising skill, sections outside the comments
//! flagged, a byte-exact undo, and Code-view comments as one nudge.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::actions::{CommentOp, Via};
use speccoder_core::oracle::{CodeComment, CommentAnchor, CommentsView, DraftState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::RunOptions;
use speccoder_runtime::comments;

const SPEC: &str = "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nUsers log in.\n\n**Acceptance Scenarios**:\n\n1. **Given** a user, **When** they log in, **Then** a `login` function exists.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST be refused with an empty password.\n\n## Edge Cases\n\n- An empty name is refused.\n";

const SKILL: &str = "---\nname: \"speckit-clarify\"\ndescription: \"Revise the spec.\"\nuser-invocable: true\n---\n\n## User Input\n\n$ARGUMENTS\n\nRevise specs/001-graph/spec.md as the input says.\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
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
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

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
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join(".claude/skills/speckit-clarify")).unwrap();
    fs::write(r.join(".claude/skills/speckit-clarify/SKILL.md"), SKILL).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn view(r: &CommandResult) -> &CommentsView {
    match r.data() {
        Some(CommandData::Comments(c)) => c,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn passage(start: usize, quote: &str) -> CommentAnchor {
    CommentAnchor::Passage {
        path: "specs/001-graph/spec.md".to_owned(),
        start_line: start,
        end_line: start,
        quote: quote.to_owned(),
        quote_sha256: String::new(),
        section: String::new(),
    }
}

fn add(r: &Path, state: &Path, anchor: CommentAnchor, text: &str) -> CommandResult {
    comments::comment(
        r,
        None,
        CommentOp::Add,
        Some(&anchor),
        None,
        Some(text),
        Via::Web,
        Some(state),
    )
}

fn sha(p: &Path) -> Vec<u8> {
    fs::read(p).unwrap()
}

#[test]
fn us5_drafts_persist_go_out_as_one_scoped_turn_and_undo_restores_bytes() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    // Drafts on two sections, anchored and listed.
    let a = add(
        r.path(),
        state.path(),
        passage(13, "Users MUST log in."),
        "say who logs in",
    );
    assert_eq!(a.status(), CommandStatus::Ok, "{}", a.message());
    let b = add(
        r.path(),
        state.path(),
        passage(18, "An empty name is refused."),
        "and a long one",
    );
    assert_eq!(b.status(), CommandStatus::Ok, "{}", b.message());
    let listed = comments::list(r.path(), None, Some(state.path()));
    let v = view(&listed);
    assert_eq!(v.drafts.len(), 2);
    let CommentAnchor::Passage {
        section,
        quote_sha256,
        ..
    } = &v.drafts[0].anchor
    else {
        panic!()
    };
    assert_eq!(section, "Spec › Requirements");
    assert!(quote_sha256.starts_with("sha256:"));
    // An anchor outside the feature's documents is refused.
    let refused = add(
        r.path(),
        state.path(),
        CommentAnchor::Passage {
            path: "specs/001-graph/review.json".to_owned(),
            start_line: 1,
            end_line: 1,
            quote: "{".to_owned(),
            quote_sha256: String::new(),
            section: String::new(),
        },
        "no",
    );
    assert_eq!(refused.status(), CommandStatus::UsageError);
    let wrong = add(r.path(), state.path(), passage(3, "not there"), "no");
    assert_eq!(wrong.status(), CommandStatus::UsageError);

    // One skill call: both quotes, their sections; it also changes a
    // section nobody commented on.
    let spec = r.path().join("specs/001-graph/spec.md");
    let before = sha(&spec);
    let revised = SPEC
        .replace("Users MUST log in.", "Registered users MUST log in.")
        .replace("Users log in.", "People log in with email.");
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/001-graph/spec.md", "content": revised }),
    );
    f.will_structure(json!({ "summary": "revised", "status": "done", "files_written": ["specs/001-graph/spec.md"], "questions": [] }));
    let sent = comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        Some(d),
        None,
        &RunOptions::default(),
    );
    assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
    let calls: Vec<String> = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .collect();
    let first = &calls[0];
    assert!(
        first.contains("say who logs in") && first.contains("and a long one"),
        "{first}"
    );
    assert!(first.contains("Every other section stays exactly as it is"));
    assert!(first.contains("Spec › Requirements") && first.contains("Spec › Edge Cases"));
    let v = view(&sent);
    assert!(v.drafts.is_empty(), "consumed");
    let turn = v.turn.clone().unwrap();
    assert_eq!(turn.outcome, "pending");
    assert!(
        turn.changed
            .iter()
            .any(|c| c == "specs/001-graph/spec.md: Spec › Requirements"),
        "{turn:?}"
    );
    assert_eq!(
        turn.outside,
        ["specs/001-graph/spec.md: Spec › User Story 1 - Login (Priority: P1)"],
        "{turn:?}"
    );
    assert!(turn.beside.is_empty(), "{turn:?}");
    // The word diff's material: each changed section before and after.
    let req = turn
        .diffs
        .iter()
        .find(|d| d.heading == "Spec › Requirements")
        .unwrap_or_else(|| panic!("{turn:?}"));
    assert!(
        req.before.contains("Users MUST log in.")
            && req.after.contains("Registered users MUST log in.")
    );
    assert!(!req.outside);
    assert!(
        turn.diffs
            .iter()
            .any(|d| d.outside && d.heading.starts_with("Spec › User Story 1"))
    );
    assert_eq!(turn.files, ["specs/001-graph/spec.md"]);
    // A second send of the same drafts is refused.
    let again = comments::send_with(
        r.path(),
        None,
        &["c1".to_owned()],
        Via::Web,
        state.path(),
        None,
        None,
        &RunOptions::default(),
    );
    assert_eq!(
        again.status(),
        CommandStatus::UsageError,
        "{}",
        again.message()
    );

    // Undo: byte for byte, the comments back in the tray.
    let undone = comments::undo(r.path(), None, Some(&turn.id), Some(state.path()));
    assert_eq!(undone.status(), CommandStatus::Ok, "{}", undone.message());
    assert!(
        undone
            .message()
            .contains("restored specs/001-graph/spec.md")
    );
    assert_eq!(sha(&spec), before);
    assert_eq!(view(&undone).drafts.len(), 2);
    assert_eq!(view(&undone).turn.as_ref().unwrap().outcome, "undone");
}

#[test]
fn us5_undo_is_refused_when_a_document_changed_since_and_outdated_drafts_wait() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
        add(
            r.path(),
            state.path(),
            passage(13, "Users MUST log in."),
            "who?"
        )
        .status(),
        CommandStatus::Ok
    );
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/001-graph/spec.md", "content": SPEC.replace("Users MUST log in.", "Users MUST sign in.") }),
    );
    f.will_structure(
        json!({ "summary": "revised", "status": "done", "files_written": [], "questions": [] }),
    );
    let sent = comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        Some(d),
        None,
        &RunOptions::default(),
    );
    assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
    // The person edits the spec after the turn: undo would overwrite it.
    let spec = r.path().join("specs/001-graph/spec.md");
    fs::write(
        &spec,
        format!("{}\nMore.\n", fs::read_to_string(&spec).unwrap()),
    )
    .unwrap();
    let now = sha(&spec);
    let undo = comments::undo(r.path(), None, None, Some(state.path()));
    assert_eq!(
        undo.status(),
        CommandStatus::NeedsHuman,
        "{}",
        undo.message()
    );
    assert!(
        undo.message()
            .contains("specs/001-graph/spec.md changed since")
    );
    assert_eq!(sha(&spec), now, "nothing written");

    // A draft whose words moved away is outdated, and not sent unless kept.
    let c = add(
        r.path(),
        state.path(),
        passage(18, "An empty name is refused."),
        "and long",
    );
    assert_eq!(c.status(), CommandStatus::Ok, "{}", c.message());
    fs::write(&spec, "# Spec\n\nRewritten entirely.\n").unwrap();
    let listed = comments::list(r.path(), None, Some(state.path()));
    let d = &view(&listed).drafts[0];
    assert_eq!(d.state, DraftState::Outdated);
    let id = d.id.clone();
    let none = comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        None,
        None,
        &RunOptions::default(),
    );
    assert_eq!(
        none.status(),
        CommandStatus::UsageError,
        "{}",
        none.message()
    );
    let kept = comments::comment(
        r.path(),
        None,
        CommentOp::Keep,
        None,
        Some(&id),
        None,
        Via::Web,
        Some(state.path()),
    );
    assert_eq!(kept.status(), CommandStatus::Ok, "{}", kept.message());
    assert_eq!(view(&kept).drafts[0].state, DraftState::Draft);
}

#[test]
fn us5_a_finding_anchor_must_be_one_the_review_shows() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let ok = add(
        r.path(),
        state.path(),
        CommentAnchor::Finding {
            source: "review".to_owned(),
            id: "FR-002".to_owned(),
            text: "Users MUST be refused with an empty password.".to_owned(),
        },
        "too vague",
    );
    assert_eq!(ok.status(), CommandStatus::Ok, "{}", ok.message());
    for (source, id) in [("review", "FR-999"), ("extension", "X1")] {
        let bad = add(
            r.path(),
            state.path(),
            CommentAnchor::Finding {
                source: source.to_owned(),
                id: id.to_owned(),
                text: "x".to_owned(),
            },
            "no",
        );
        assert_eq!(bad.status(), CommandStatus::UsageError, "{source}:{id}");
    }
}

#[test]
fn us6_code_comments_are_one_nudge_with_their_anchors() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let res = comments::code_nudge(
        r.path(),
        None,
        "T005",
        &[
            CodeComment {
                path: "src/a.rs".to_owned(),
                line: 12,
                quote: "let x = 1;".to_owned(),
                text: "use a constant".to_owned(),
            },
            CodeComment {
                path: "src/b.rs".to_owned(),
                line: 3,
                quote: "fn b() {}".to_owned(),
                text: "name it".to_owned(),
            },
        ],
        Some(state.path()),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(res.message().contains("the next attempt of T005"));
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let d = speccoder_runtime::approvals::Decisions::load(&plane).unwrap();
    let nudges = d.for_task("001-graph", "T005").nudges;
    assert_eq!(
        nudges,
        ["src/a.rs:12 \"let x = 1;\" — use a constant\nsrc/b.rs:3 \"fn b() {}\" — name it"]
    );
    let empty = comments::code_nudge(r.path(), None, "T005", &[], Some(state.path()));
    assert_eq!(empty.status(), CommandStatus::UsageError);
}

/// The scripted model, and a person who edits a document while the
/// revision runs: before the model's `at`-th answer, `edit` is applied.
#[derive(Debug)]
struct EditsMidTurn {
    inner: Arc<FakeProvider>,
    at: usize,
    seen: std::sync::Mutex<usize>,
    file: PathBuf,
    append: &'static str,
}

#[agentplane::prelude::async_trait]
impl ModelProvider for EditsMidTurn {
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
            let now = fs::read_to_string(&self.file).unwrap_or_default();
            fs::write(&self.file, format!("{now}{}", self.append)).unwrap();
        }
        self.inner.complete(request).await
    }

    fn gen_ai_provider(&self) -> Option<&str> {
        None
    }
}

fn revise_spec(f: &FakeProvider, content: &str) {
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/001-graph/spec.md", "content": content }),
    );
    f.will_structure(
        json!({ "summary": "revised", "status": "done", "files_written": [], "questions": [] }),
    );
}

#[test]
fn us5_undo_never_reverts_what_the_person_wrote_while_the_revision_ran() {
    // P6: the person edits the spec (after the skill's write) and the plan
    // while the turn runs: those edits are not the turn's.
    for (file, at) in [
        ("specs/001-graph/spec.md", 2),
        ("specs/001-graph/plan.md", 1),
    ] {
        let r = repo();
        let state = tempfile::tempdir().unwrap();
        assert_eq!(
            add(
                r.path(),
                state.path(),
                passage(13, "Users MUST log in."),
                "who?"
            )
            .status(),
            CommandStatus::Ok
        );
        let f = FakeProvider::new();
        revise_spec(
            &f,
            &SPEC.replace("Users MUST log in.", "Users MUST sign in."),
        );
        let d: Arc<dyn ModelProvider> = Arc::new(EditsMidTurn {
            inner: f.clone(),
            at,
            seen: std::sync::Mutex::new(0),
            file: r.path().join(file),
            append: "\nMy own note.\n",
        });
        let sent = comments::send_with(
            r.path(),
            None,
            &[],
            Via::Web,
            state.path(),
            Some(d),
            None,
            &RunOptions::default(),
        );
        assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
        let turn = view(&sent).turn.clone().unwrap();
        assert_eq!(turn.beside, [file], "{file}: {turn:?}");
        assert!(
            sent.message().contains("changed beside the revision"),
            "{}",
            sent.message()
        );
        let mine = fs::read_to_string(r.path().join(file)).unwrap();
        assert!(mine.contains("My own note."));
        let undo = comments::undo(r.path(), None, None, Some(state.path()));
        assert_eq!(
            undo.status(),
            CommandStatus::NeedsHuman,
            "{}",
            undo.message()
        );
        assert_eq!(
            fs::read_to_string(r.path().join(file)).unwrap(),
            mine,
            "{file}: the person's edit was reverted"
        );
    }
}

#[test]
fn us5_a_heading_is_a_section_of_its_own_document() {
    // P6: the plan gains a section named like the commented one in the spec.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
        add(
            r.path(),
            state.path(),
            passage(13, "Users MUST log in."),
            "who?"
        )
        .status(),
        CommandStatus::Ok
    );
    let f = FakeProvider::new();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/001-graph/plan.md", "content": "# Spec\n\n## Requirements\n\nRewritten.\n" }),
    );
    f.will_structure(
        json!({ "summary": "revised", "status": "done", "files_written": [], "questions": [] }),
    );
    let d: Arc<dyn ModelProvider> = f.clone();
    let sent = comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        Some(d),
        None,
        &RunOptions::default(),
    );
    let turn = view(&sent).turn.clone().unwrap();
    assert!(
        turn.outside
            .iter()
            .any(|s| s == "specs/001-graph/plan.md: Spec › Requirements"),
        "{turn:?}"
    );
}

#[test]
fn an_unreadable_tray_is_refused_never_read_as_empty() {
    // C6: a corrupt tray is not overwritten by the next add.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
        add(
            r.path(),
            state.path(),
            passage(13, "Users MUST log in."),
            "who?"
        )
        .status(),
        CommandStatus::Ok
    );
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let tray = plane.join("comments/001-graph.json");
    fs::write(&tray, "{ not json").unwrap();
    for res in [
        comments::list(r.path(), None, Some(state.path())),
        add(
            r.path(),
            state.path(),
            passage(18, "An empty name is refused."),
            "x",
        ),
        comments::undo(r.path(), None, None, Some(state.path())),
    ] {
        assert_eq!(
            res.code().map(|c| c.name),
            Some("REVIEW-UNREADABLE"),
            "{}",
            res.message()
        );
    }
    assert_eq!(fs::read_to_string(&tray).unwrap(), "{ not json");
}

#[test]
fn undo_writes_only_the_unit_s_documents_a_tampered_tray_names() {
    // P6: a tray naming another file (here: src/lib.rs, "absent" before)
    // never makes Undo delete or write it.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
        add(
            r.path(),
            state.path(),
            passage(13, "Users MUST log in."),
            "who?"
        )
        .status(),
        CommandStatus::Ok
    );
    let f = FakeProvider::new();
    revise_spec(
        &f,
        &SPEC.replace("Users MUST log in.", "Users MUST sign in."),
    );
    let d: Arc<dyn ModelProvider> = f.clone();
    let sent = comments::send_with(
        r.path(),
        None,
        &[],
        Via::Web,
        state.path(),
        Some(d),
        None,
        &RunOptions::default(),
    );
    assert_eq!(sent.status(), CommandStatus::Ok, "{}", sent.message());
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let file = plane.join("comments/001-graph.json");
    let mut tray: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let lib = fs::read(r.path().join("src/lib.rs")).unwrap();
    let digest = format!("sha256:{}", speccoder_runtime::integrity::sha256_hex(&lib));
    let turn = &mut tray["turns"][0];
    turn["before"] = json!([{ "path": "src/lib.rs", "sha256": "absent" }]);
    turn["after"] = json!([{ "path": "src/lib.rs", "sha256": digest }]);
    fs::write(&file, serde_json::to_string(&tray).unwrap()).unwrap();
    let undo = comments::undo(r.path(), None, None, Some(state.path()));
    assert_ne!(undo.status(), CommandStatus::Ok, "{}", undo.message());
    assert_eq!(fs::read(r.path().join("src/lib.rs")).unwrap(), lib);
}

#[test]
fn us6_a_frozen_test_is_known_by_its_code_view_path_in_a_project_below_the_top() {
    // P7: the project sits in `app/` of its repository; the Code view names
    // `app/tests/…`, the frozen record `tests/…`.
    let top = tempfile::tempdir().unwrap();
    let app = top.path().join("app");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    fs::create_dir_all(&app).unwrap();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(&app)
            .status()
            .unwrap()
            .success()
    );
    fs::write(app.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(
        app.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    git(top.path(), &["init", "-q"]);
    git(top.path(), &["add", "-A"]);
    git(top.path(), &["commit", "-q", "-m", "init"]);
    let state = tempfile::tempdir().unwrap();
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), &app);
    let content = "checks fn login\n".to_owned();
    let record = speccoder_core::acceptance::PreFrozenAcceptance {
        unit: "001-graph".to_owned(),
        requirement: "US1".to_owned(),
        wording_digest: "sha256:w".to_owned(),
        approval_digest: "sha256:a".to_owned(),
        visible: vec![speccoder_core::acceptance::VisibleTest {
            path: "tests/acceptance_us1.txt".to_owned(),
            sha256: format!(
                "sha256:{}",
                speccoder_runtime::integrity::sha256_hex(content.as_bytes())
            ),
            cites: vec!["US1-1".to_owned()],
        }],
        held_out_sealed: None,
        producer: speccoder_core::acceptance::Producer::Review,
        decided: speccoder_core::acceptance::Decided {
            by: "Ada".to_owned(),
            via: Via::Web,
            at: 0,
        },
    };
    speccoder_runtime::pre_frozen::store(
        &plane,
        &record,
        &[("tests/acceptance_us1.txt".to_owned(), content)],
    )
    .unwrap();
    let res = comments::code_nudge(
        &app,
        None,
        "T005",
        &[CodeComment {
            path: "app/tests/acceptance_us1.txt".to_owned(),
            line: 1,
            quote: "checks fn login".to_owned(),
            text: "loosen it".to_owned(),
        }],
        Some(state.path()),
    );
    assert_eq!(res.status(), CommandStatus::UsageError, "{}", res.message());
    assert!(
        res.message().contains("is a frozen test"),
        "{}",
        res.message()
    );
}

#[test]
fn a_comment_anchors_to_a_line_changed_since_the_last_commit() {
    // T063: the Review's "since the last commit" lines carry their place.
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let spec = r.path().join("specs/001-graph/spec.md");
    fs::write(
        &spec,
        SPEC.replace("Users MUST log in.", "Users MUST sign in."),
    )
    .unwrap();
    let rv = speccoder_runtime::review::report_in(r.path(), None, Some(state.path()));
    let Some(CommandData::Review(rv)) = rv.data() else {
        panic!("{}", rv.message())
    };
    let c = rv.changes.iter().find(|c| c.id == "FR-001").unwrap();
    assert_eq!(c.path, "specs/001-graph/spec.md");
    assert_eq!((c.then_line, c.now_line), (Some(13), Some(13)));
    let added = add(
        r.path(),
        state.path(),
        CommentAnchor::DiffLine {
            path: c.path.clone(),
            side: "new".to_owned(),
            line: c.now_line.unwrap() as usize,
            quote: c.now.clone().unwrap(),
        },
        "sign in or log in?",
    );
    assert_eq!(added.status(), CommandStatus::Ok, "{}", added.message());
}
