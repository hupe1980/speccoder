//! One dispatcher: every runnable kind, taken with the same input on the
//! same project, gives the result it gave before the dispatcher moved out
//! of the web host. The goldens under `tests/fixtures/perform/` were
//! recorded through a verbatim copy of that host's mapping (at `90439f4`,
//! before `actions::perform` existed; `SPECCODER_BLESS=1` writes them); a
//! golden diff means behaviour changed — fix the code, not the golden.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::actions::{Runnable, Via};
use speccoder_core::{CommandResult, CommandStatus};
use speccoder_runtime::actions::Dispatch;

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[acceptance]\ngenerate = false\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Sessions MUST expire after [NEEDS CLARIFICATION: how long?].\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Dana Example",
            "-c",
            "user.email=d@example.com",
            "-c",
            "commit.gpgsign=false",
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

/// A committed Spec Kit project (the graph fixture: T001–T004 ticked,
/// T005–T011 open) whose spec has two requirements and an open question.
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
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Dana Example"]);
    git(r, &["config", "user.email", "d@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// T005's attempt stops on a write that needs a person: a waiting call.
fn wait_on_t005(r: &Path, state: &Path) {
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "package.json", "content": "{}\n" }),
    );
    let d: Arc<dyn ModelProvider> = f;
    let first = speccoder_runtime::run_with(r, None, "T005", state, d, None);
    assert_eq!(
        first.status(),
        CommandStatus::NeedsHuman,
        "{}",
        first.message()
    );
}

/// One case: its golden's name, what runs, the person's input, and
/// whether a state directory is there.
struct Case {
    name: &'static str,
    run: Runnable,
    input: Option<&'static str>,
    state: bool,
}

fn case(name: &'static str, run: Runnable, input: Option<&'static str>) -> Case {
    Case {
        name,
        run,
        input,
        state: true,
    }
}

fn task(t: &str) -> String {
    t.to_owned()
}

/// Approve `t`'s waiting call, bound to it when the case runs ([`shown`]).
fn approve(t: &str) -> Runnable {
    Runnable::Approve {
        task: task(t),
        call: format!("attempt:{t}"),
        digest: String::new(),
    }
}

/// Reject it, bound the same way.
fn reject(t: &str) -> Runnable {
    Runnable::Reject {
        task: task(t),
        call: format!("attempt:{t}"),
        digest: String::new(),
    }
}

/// `run` bound to the call an interface would show for its task now: the
/// call's id and digest, as listed (unchanged when none is listed, or when
/// it names a digest of its own).
fn shown(root: &Path, state: Option<&Path>, run: &Runnable) -> Runnable {
    let (Runnable::Approve { task, digest, .. } | Runnable::Reject { task, digest, .. }) = run
    else {
        return run.clone();
    };
    if !digest.is_empty() {
        return run.clone();
    }
    let Some(state) = state else {
        return run.clone();
    };
    let listed = speccoder_runtime::approvals::list_with(root, None, state);
    let Some(speccoder_core::CommandData::Approvals(a)) = listed.data() else {
        return run.clone();
    };
    let Some(p) = a.pending.iter().find(|p| p.task == *task) else {
        return run.clone();
    };
    let (task, call, digest) = (task.clone(), p.id(), p.digest.clone());
    match run {
        Runnable::Approve { .. } => Runnable::Approve { task, call, digest },
        _ => Runnable::Reject { task, call, digest },
    }
}

fn review(ids: &[&str], verdict: &str) -> Runnable {
    Runnable::Review {
        ids: ids.iter().map(|s| (*s).to_owned()).collect(),
        verdict: verdict.to_owned(),
    }
}

/// Every case, in the order they run on one project. The cases of a kind
/// that runs the model (skill, run, converge, a quick change that runs)
/// read the state directory from the environment, which a test cannot
/// set; the web host's job test covers them (`speccoder/tests/jobs.rs`).
fn cases() -> Vec<Case> {
    let no_state = |mut c: Case| {
        c.state = false;
        c
    };
    vec![
        // The open question blocks Approve and Run before anything is recorded.
        case(
            "approve-and-run-question",
            Runnable::ApproveAndRun {
                mode: speccoder_core::oracle::RunMode::Ask,
                grants: Vec::new(),
                sheet: None,
            },
            None,
        ),
        case("approve-unreadable", approve("T005"), None),
        case("reject-unreadable", reject("T005"), Some("no new files")),
        case("reject-empty", reject("T005"), None),
        case("reject-blank", reject("T005"), Some("   ")),
        case("approve-none", approve("T006"), None),
        no_state(case("approve-no-state", approve("T005"), None)),
        // Shown another call than the one waiting now: nothing is decided.
        case(
            "approve-changed",
            Runnable::Approve {
                task: task("T005"),
                call: "attempt:T005".to_owned(),
                digest: "0".repeat(64),
            },
            None,
        ),
        case("reject-waiting", reject("T005"), Some("  no new files  ")),
        case(
            "accept-empty",
            Runnable::Accept { task: task("T006") },
            None,
        ),
        case(
            "accept-blank",
            Runnable::Accept { task: task("T006") },
            Some(" \n"),
        ),
        no_state(case(
            "accept-no-state",
            Runnable::Accept { task: task("T006") },
            Some("done by hand"),
        )),
        case(
            "accept-reason",
            Runnable::Accept { task: task("T006") },
            Some("done by hand"),
        ),
        case("skip-empty", Runnable::Skip { task: task("T007") }, None),
        case(
            "skip-reason",
            Runnable::Skip { task: task("T007") },
            Some("not this release"),
        ),
        case("nudge-empty", Runnable::Nudge { task: task("T008") }, None),
        no_state(case(
            "nudge-no-state",
            Runnable::Nudge { task: task("T008") },
            Some("use the existing session type"),
        )),
        case(
            "nudge-note",
            Runnable::Nudge { task: task("T008") },
            Some("use the existing session type"),
        ),
        case("rule-empty", Runnable::Rule { task: task("T009") }, None),
        no_state(case(
            "rule-no-state",
            Runnable::Rule { task: task("T009") },
            Some("the tests are right"),
        )),
        case(
            "rule-reason",
            Runnable::Rule { task: task("T009") },
            Some("the tests are right"),
        ),
        case("review-answer-empty", review(&["Q1"], "answer"), None),
        case(
            "review-answer-no-id",
            review(&[], "answer"),
            Some("thirty minutes"),
        ),
        case(
            "review-answer",
            review(&["Q1"], "answer"),
            Some("thirty minutes"),
        ),
        case("review-edit-empty", review(&["FR-001"], "edit"), Some(" ")),
        case(
            "review-edit",
            review(&["FR-001"], "edit"),
            Some("Users MUST log in with a passkey."),
        ),
        case(
            "review-edit-approve",
            review(&["FR-002"], "edit-approve"),
            Some("Sessions MUST expire after thirty minutes."),
        ),
        case("review-request-empty", review(&["FR-001"], "request"), None),
        case(
            "review-request",
            review(&["FR-001"], "request"),
            Some("say which passkeys"),
        ),
        case("review-clear", review(&["FR-001"], "clear"), None),
        case(
            "review-approve",
            review(&[], "approve"),
            Some("looks right"),
        ),
        // A path that starts with the unit's name and leaves it.
        case(
            "commit-outside",
            Runnable::Commit {
                paths: vec!["specs/001-graph/../../src/lib.rs".to_owned()],
            },
            None,
        ),
        case(
            "commit",
            Runnable::Commit {
                paths: vec!["specs/001-graph/review.json".to_owned()],
            },
            None,
        ),
        case(
            "keep-set-aside-none",
            Runnable::KeepSetAside { task: task("T010") },
            None,
        ),
        case(
            "drop-set-aside-none",
            Runnable::DropSetAside { task: task("T010") },
            None,
        ),
        case("fix-setup-nothing", Runnable::FixSetup, None),
        case(
            "quick-change-recorded",
            Runnable::QuickChange {
                request: "Log every sign-in".to_owned(),
                checks: vec!["a sign-in is logged".to_owned()],
                run: false,
            },
            None,
        ),
        case(
            "issue-comment-no-remote",
            Runnable::IssueComment {
                issue: 42,
                assign: true,
            },
            None,
        ),
        case("pull-request-no-remote", Runnable::PullRequest, None),
        case(
            "bug-fix-empty",
            Runnable::BugFix {
                symptom: "signing in twice logs out".to_owned(),
                expected: Vec::new(),
                unchanged: Vec::new(),
                issue: None,
                tests: Vec::new(),
                candidate: None,
                slug: None,
                run: false,
            },
            None,
        ),
        case(
            "bug-fix-recorded",
            Runnable::BugFix {
                symptom: "signing in twice logs out".to_owned(),
                expected: vec!["a second sign-in keeps the session".to_owned()],
                unchanged: vec!["signing out ends the session".to_owned()],
                issue: None,
                tests: Vec::new(),
                candidate: None,
                slug: None,
                run: false,
            },
            None,
        ),
        case(
            "bug-confirm-unknown",
            Runnable::BugConfirm {
                task: "002-bugfixes/T009".to_owned(),
                digest: "sha256:0".to_owned(),
                reject: false,
            },
            None,
        ),
        case(
            "bug-run-unknown",
            Runnable::BugRun {
                slug: None,
                task: Some("002-bugfixes/T009".to_owned()),
            },
            None,
        ),
        case("feature-empty", Runnable::Feature, Some("")),
        case("feature-name", Runnable::Feature, Some("001-graph")),
        case(
            "allow-always-not-a-call",
            Runnable::AllowAlways {
                task: task("T005"),
                call: "zz".to_owned(),
            },
            None,
        ),
        case(
            "allow-always-no-such-call",
            Runnable::AllowAlways {
                task: task("T005"),
                call: "00".repeat(32),
            },
            None,
        ),
        case("widen-policy-bare-program", widen(&["npm"], "d"), None),
        case(
            "widen-policy-stale-digest",
            widen(&["npm", "run"], "stale"),
            None,
        ),
        case(
            "widen-policy-unreadable",
            widen(&["npm", "run"], &empty_preview(&["npm", "run"])),
            None,
        ),
        case(
            "widen-policy-ok",
            widen(&["npm", "run"], &empty_preview(&["npm", "run"])),
            None,
        ),
        case(
            "decide-batch-none",
            Runnable::DecideBatch {
                run: "run-1-0a0b0c0d".to_owned(),
                verdict: speccoder_core::actions::BatchVerdict::Approve,
                calls: Vec::new(),
                digest: "d".to_owned(),
                resume_asking: false,
            },
            None,
        ),
        case(
            "decide-batch-reject-empty",
            Runnable::DecideBatch {
                run: "run-1-0a0b0c0d".to_owned(),
                verdict: speccoder_core::actions::BatchVerdict::Reject,
                calls: Vec::new(),
                digest: "d".to_owned(),
                resume_asking: false,
            },
            None,
        ),
        case(
            "oracle-tests-acceptance-off",
            Runnable::OracleTests {
                req: "US1".to_owned(),
                op: speccoder_core::oracle::OracleOp::Reject,
                path: Some("tests/a.rs".to_owned()),
                scenario: None,
                reading: None,
            },
            Some("wrong"),
        ),
        case(
            "comment-outside",
            Runnable::Comment {
                op: speccoder_core::actions::CommentOp::Add,
                anchor: Some(speccoder_core::oracle::CommentAnchor::Passage {
                    path: "src/lib.rs".to_owned(),
                    start_line: 1,
                    end_line: 1,
                    quote: "pub fn ok() {}".to_owned(),
                    quote_sha256: String::new(),
                    section: String::new(),
                }),
                id: None,
            },
            Some("rename it"),
        ),
        case(
            "comment-delete-none",
            Runnable::Comment {
                op: speccoder_core::actions::CommentOp::Delete,
                anchor: None,
                id: Some("c9".to_owned()),
            },
            None,
        ),
        case(
            "revision-undo-none",
            Runnable::RevisionUndo { turn: None },
            None,
        ),
        case(
            "code-nudge-empty",
            Runnable::CodeNudge {
                task: task("T008"),
                comments: Vec::new(),
            },
            None,
        ),
        case(
            "code-nudge-note",
            Runnable::CodeNudge {
                task: task("T008"),
                comments: vec![speccoder_core::oracle::CodeComment {
                    path: "src/lib.rs".to_owned(),
                    line: 1,
                    quote: "pub fn ok() {}".to_owned(),
                    text: "name it".to_owned(),
                }],
            },
            None,
        ),
    ]
}

fn widen(prefix: &[&str], digest: &str) -> Runnable {
    Runnable::WidenPolicy {
        key: speccoder_core::actions::PolicyList::CommandsPrefix,
        prefix: prefix.iter().map(|s| (*s).to_owned()).collect(),
        digest: digest.to_owned(),
    }
}

/// The digest of a complete preview with nothing to show.
fn empty_preview(prefix: &[&str]) -> String {
    speccoder_runtime::approvals::coverage::digest(
        &speccoder_core::CoverageEntry {
            key: "commands.allow_prefix".to_owned(),
            prefix: prefix.iter().map(|s| (*s).to_owned()).collect(),
        },
        &[],
        true,
    )
}

/// Placeholders for what differs between runs: the project's and the
/// state's paths, commit ids, times.
struct Norm {
    paths: Vec<(String, &'static str)>,
}

impl Norm {
    fn new(root: &Path, state: &Path) -> Self {
        let mut paths = Vec::new();
        for (p, name) in [(root, "<root>"), (state, "<state>")] {
            if let Ok(c) = fs::canonicalize(p) {
                paths.push((c.display().to_string(), name));
            }
            paths.push((p.display().to_string(), name));
        }
        // The longest first, so a path is not replaced by its prefix.
        paths.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
        Self { paths }
    }

    fn text(&self, s: &str) -> String {
        let mut s = s.to_owned();
        for (p, name) in &self.paths {
            s = s.replace(p.as_str(), name);
        }
        hex(&s)
    }

    fn value(&self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.text(s)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.value(x)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, x)| {
                        let x = match x {
                            // Seconds or milliseconds since the epoch.
                            Value::Number(n) if n.as_u64().is_some_and(|n| n > 1_000_000_000) => {
                                json!(0)
                            }
                            x => self.value(x),
                        };
                        (k.clone(), x)
                    })
                    .collect(),
            ),
            v => v.clone(),
        }
    }
}

/// Runs of 7 or more hex digits with a letter and a digit (commit ids,
/// run ids) as `<hex>`.
fn hex(s: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let letters = run.chars().any(|c| c.is_ascii_alphabetic());
        let digits = run.chars().any(|c| c.is_ascii_digit());
        // After `as ` a run is the commit's id even when it happens to be
        // all digits or all letters; elsewhere a plain number stays.
        let named = out.ends_with(" as ");
        if run.len() >= 7 && (named || (letters && digits)) {
            out.push_str("<hex>");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if matches!(c, '0'..='9' | 'a'..='f') {
            run.push(c);
        } else if c.is_ascii_alphanumeric() {
            // Part of a longer word: not an id.
            out.push_str(&run);
            run.clear();
            out.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The result and what it left behind: the last commit's subject, the
/// checkout's status, the review record.
fn observed(r: &Path, result: &CommandResult, norm: &Norm) -> Value {
    let review = fs::read_to_string(r.join("specs/001-graph/review.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    json!({
        "result": norm.value(&serde_json::to_value(result).unwrap()),
        "head": git(r, &["log", "-1", "--format=%s"]).trim(),
        "status": norm.text(&git(r, &["status", "--porcelain", "--untracked-files=all"])),
        "review": review.map(|v| norm.value(&v)),
    })
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perform")
}

/// Every case through `dispatch`, each against its golden.
fn against_goldens(
    dispatch: impl Fn(&Path, Option<&str>, &Runnable, Option<&str>, Option<&Path>) -> CommandResult,
) {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    wait_on_t005(r.path(), state.path());
    let norm = Norm::new(r.path(), state.path());
    let journal = speccoder_runtime::worktree::project_state_dir(
        state.path(),
        &fs::canonicalize(r.path()).unwrap(),
    )
    .join("journal.redb");
    let bless = std::env::var_os("SPECCODER_BLESS").is_some();
    let dir = golden_dir();
    let mut differ = Vec::new();
    let mut held = None;
    for c in cases() {
        // A writer that holds the journal alone, as speccoder 0.2.0 did.
        if c.name.ends_with("-unreadable") {
            if held.is_none() {
                held = Some(agentplane::store::RedbStore::open(&journal).unwrap());
            }
        } else {
            held = None;
        }
        let s = c.state.then(|| state.path());
        let run = shown(r.path(), s, &c.run);
        let result = dispatch(r.path(), None, &run, c.input, s);
        let got = observed(r.path(), &result, &norm);
        let text = format!("{}\n", serde_json::to_string_pretty(&got).unwrap());
        let file = dir.join(format!("{}.json", c.name));
        if bless {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&file, text).unwrap();
        } else if fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
            differ.push(format!("{}:\n{text}", c.name));
        }
    }
    drop(held);
    assert!(
        differ.is_empty(),
        "results differ from the goldens (run with SPECCODER_BLESS=1 only for an intended change):\n{}",
        differ.join("\n")
    );
}

#[test]
fn every_kind_gives_the_result_it_gave_before() {
    against_goldens(|root, feature, run, input, state| {
        speccoder_runtime::actions::perform_in(
            root,
            feature,
            Dispatch {
                run,
                input,
                via: Via::Web,
                options: speccoder_runtime::RunOptions::default(),
            },
            state,
        )
    });
}

#[test]
fn the_normaliser_replaces_ids_and_keeps_words() {
    assert_eq!(
        hex("committed 1a2b3c4d5e as deadbeef1; feed facade 1234567"),
        "committed <hex> as <hex>; feed facade 1234567"
    );
    // A commit id of digits only, or letters only, is still an id.
    assert_eq!(
        hex("committed 1 file(s) as 1234567"),
        "committed 1 file(s) as <hex>"
    );
    assert_eq!(
        hex("committed 1 file(s) as deadbee"),
        "committed 1 file(s) as <hex>"
    );
}

/// The action lists of a project on its way: no feature yet, a spec, a
/// plan, a task list not yet committed, committed with a task selected,
/// a call waiting — and an OpenSpec project.
fn lists() -> Vec<(String, Vec<speccoder_core::actions::Action>)> {
    use speccoder_core::CommandData;
    let state = tempfile::tempdir().unwrap();
    let mut out = Vec::new();
    let mut list = |name: &str, r: &Path, selected: Option<&str>| {
        let listed = match speccoder_runtime::approvals::list_with(r, None, state.path()).data() {
            Some(CommandData::Approvals(a)) => Some((**a).clone()),
            _ => None,
        };
        out.push((
            name.to_owned(),
            speccoder_runtime::actions::available_with(r, None, selected, listed.as_ref()),
        ));
    };
    let empty = tempfile::tempdir().unwrap();
    let e = empty.path();
    fs::create_dir_all(e.join(".specify/memory")).unwrap();
    fs::write(e.join(".specify/memory/constitution.md"), "# C\n").unwrap();
    git(e, &["init", "-q"]);
    git(e, &["add", "-A"]);
    git(e, &["commit", "-q", "-m", "init"]);
    list("no feature", e, None);
    fs::create_dir_all(e.join("specs/001-x")).unwrap();
    fs::write(e.join("specs/001-x/spec.md"), SPEC).unwrap();
    list("spec", e, None);
    fs::write(e.join("specs/001-x/plan.md"), "# Plan\n").unwrap();
    list("plan", e, None);
    fs::write(
        e.join("specs/001-x/tasks.md"),
        "# Tasks\n\n## Phase 1\n\n- [ ] T001 Do it\n- [ ] T002 Then this\n",
    )
    .unwrap();
    list("tasks uncommitted", e, Some("T001"));
    git(e, &["add", "-A"]);
    git(e, &["commit", "-q", "-m", "plan"]);
    list("tasks committed", e, Some("T001"));
    let waiting = repo();
    wait_on_t005(waiting.path(), state.path());
    list("waiting", waiting.path(), Some("T005"));
    let openspec = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-openspec/tests/fixtures/1.14.3/sample");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(openspec.path())
            .status()
            .unwrap()
            .success()
    );
    git(openspec.path(), &["init", "-q"]);
    git(openspec.path(), &["add", "-A"]);
    git(openspec.path(), &["commit", "-q", "-m", "init"]);
    list("openspec", openspec.path(), None);
    out
}

#[test]
fn group_and_identity_name_one_action_in_every_list() {
    for (name, list) in lists() {
        assert!(!list.is_empty(), "{name}: no actions");
        let mut seen = std::collections::BTreeSet::new();
        for a in &list {
            let key = format!("{}:{:?}", a.group, a.id);
            assert!(seen.insert(key.clone()), "{name}: {key} twice in {list:#?}");
            assert_eq!(
                a.id,
                speccoder_core::actions::identity(a.run.as_ref(), &a.command),
                "{name}: {}",
                a.label
            );
            assert_eq!(
                a.role.is_some(),
                matches!(a.run, Some(Runnable::Skill { .. })),
                "{name}: only a skill has a role: {}",
                a.label
            );
        }
    }
}

#[test]
fn a_relabelled_list_keeps_every_identity_and_role() {
    for (name, list) in lists() {
        for a in list {
            let mut b = a.clone();
            b.label = format!("x {}", b.label);
            b.why = format!("x {}", b.why);
            // Recomputed from what runs: the wording takes no part.
            let again = speccoder_core::actions::identity(b.run.as_ref(), &b.command);
            assert_eq!((again, b.role), (a.id, a.role), "{name}");
        }
    }
}

#[test]
fn spec_kit_offers_its_create_step_by_role() {
    use speccoder_core::actions::SkillRole;
    let lists = lists();
    let first = &lists[0].1;
    let create = first
        .iter()
        .find(|a| a.group == "next" && a.role == Some(SkillRole::Create))
        .unwrap_or_else(|| panic!("{first:#?}"));
    assert!(create.input.is_some(), "the create step asks for words");
}

/// The origin is the interface the person used, recorded as given — a
/// reject from the web app records `web` (it once recorded `cli` whatever
/// sent it) — and what earlier versions recorded reads back.
#[test]
fn a_decision_records_the_interface_it_came_through() {
    use speccoder_core::CommandData;
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    wait_on_t005(r.path(), state.path());
    let run = shown(r.path(), Some(state.path()), &reject("T005"));
    let mut d = Dispatch::new(&run, Via::Web);
    d.input = Some("no new files");
    let res = speccoder_runtime::actions::perform_in(r.path(), None, d, Some(state.path()));
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    // The attempt resumes and reads the decision back, with its origin.
    let f = FakeProvider::new();
    f.will_structure(json!({ "summary": "left it", "status": "done", "files_changed": [], "checks_run": [], "notes": [] }));
    let provider: Arc<dyn ModelProvider> = f;
    let resumed = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), provider, None);
    let Some(CommandData::Run(rep)) = resumed.data() else {
        panic!("{}", resumed.message())
    };
    let call = &rep.tool_calls[0];
    assert_eq!(call["decision"], "deny", "{call}");
    assert_eq!(call["approver"]["via"], "web", "{call}");

    for (old, via, client) in [
        ("cli", "terminal", None),
        ("tui", "terminal", None),
        ("dashboard", "terminal", None),
        ("editor Zed", "acp", Some("Zed")),
        ("web", "web", None),
    ] {
        let who: speccoder_runtime::approvals::Who =
            serde_json::from_value(json!({ "name": "Dana", "via": old, "basis": "asserted" }))
                .unwrap();
        assert_eq!(
            (who.via.as_str(), who.client.as_deref()),
            (via, client),
            "{old}"
        );
    }
    let new = speccoder_runtime::approvals::Who::local_in(r.path(), Via::Terminal);
    assert_eq!(
        serde_json::to_value(&new).unwrap()["via"],
        Via::Terminal.as_str()
    );
}
