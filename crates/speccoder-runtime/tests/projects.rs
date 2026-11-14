//! Several projects, one view (spec 097): the roll-up from evidence, an
//! unknown never calmer, forget, opening only a recent project, and a stop
//! reported to another project as a draft.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::projects::ProjectState;
use speccoder_runtime::projects::{self, Opened};
use speccoder_runtime::welcome;

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

/// A spec project under `parent/name`, canonical.
fn project(parent: &Path, name: &str) -> PathBuf {
    let r = parent.join(name);
    fs::create_dir_all(&r).unwrap();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../speccoder-speckit/tests/fixtures/graph/.")
            )
            .arg(&r)
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
    fs::write(r.join("speccoder.toml"), BASE).unwrap();
    git(&r, &["init", "-q"]);
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    fs::canonicalize(r).unwrap()
}

/// A call waiting for the person in `root`'s T005.
fn waiting(root: &Path, state: &Path) {
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "run_command",
        json!({ "argv": ["curl", "-s", "https://crates.io/x"], "network": true }),
    );
    let d: Arc<dyn ModelProvider> = f.clone();
    let _ = speccoder_runtime::run_with(root, None, "T005", state, d, None);
}

#[test]
fn every_recent_project_is_rolled_up_from_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let api = project(dir.path(), "api");
    let web = project(dir.path(), "web");
    let docs = project(dir.path(), "docs");
    waiting(&api, &state);
    // A run going on in web: its live record, by a living process.
    let live = speccoder_runtime::worktree::project_state_dir(&state, &web).join("live.json");
    fs::create_dir_all(live.parent().unwrap()).unwrap();
    let state_now = speccoder_runtime::progress::LiveState {
        pid: std::process::id(),
        feature: "001-graph".into(),
        task: "T004".into(),
        title: "x".into(),
        started_at: 1,
        updated_at: 1,
        ..Default::default()
    };
    fs::write(&live, serde_json::to_string(&state_now).unwrap()).unwrap();
    for (p, n) in [(&docs, "docs"), (&web, "web"), (&api, "api")] {
        welcome::remember(&state, p, n, false);
    }
    welcome::remember(&state, &dir.path().join("moved"), "moved", false);

    let began = std::time::Instant::now();
    let r = projects::list(&state, Some(&web));
    // SC-001: under 2 s in a release build; a debug build is slower.
    let bound = if cfg!(debug_assertions) { 10 } else { 2 };
    assert!(
        began.elapsed() < std::time::Duration::from_secs(bound),
        "{:?}",
        began.elapsed()
    );
    let by = |n: &str| r.projects.iter().find(|p| p.name == n).unwrap();
    assert_eq!(by("api").state, ProjectState::BlockedOnYou, "{r:#?}");
    assert_eq!(by("api").waiting, Some(1));
    assert_eq!(by("web").state, ProjectState::Working, "{r:#?}");
    assert_eq!(
        by("web").live.as_ref().map(|l| l.task.as_str()),
        Some("T004")
    );
    assert!(by("web").current);
    assert!(!matches!(
        by("docs").state,
        ProjectState::BlockedOnYou | ProjectState::Working
    ));
    assert_eq!(by("moved").state, ProjectState::Missing);
    assert_eq!(by("moved").waiting, None);
    // By state: blocked first, gone last.
    assert_eq!(r.projects[0].name, "api");
    assert_eq!(r.projects.last().unwrap().name, "moved");
    // The merged list: speccoder's own title, no detail, blocked first.
    let first = &r.items[0];
    assert_eq!(
        (first.project.as_str(), first.task.as_deref()),
        ("api", Some("T005"))
    );
    assert_eq!(first.group, "blocked_on_you");
    let text = serde_json::to_string(&r).unwrap();
    assert!(!text.contains("\"detail\""), "{text}");
    assert!(
        !text.contains("crates.io/x"),
        "a call's arguments crossed projects: {text}"
    );
}

#[test]
fn forget_removes_only_the_list_entry() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let api = project(dir.path(), "api");
    welcome::remember(&state, &api, "api", false);
    assert_eq!(projects::forget(&state, "api").unwrap(), "api");
    assert!(welcome::recent(&state).is_empty());
    assert!(api.join("speccoder.toml").is_file());
    assert!(projects::forget(&state, "api").is_err());
}

#[test]
fn only_a_recent_project_is_opened_and_one_open_elsewhere_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let api = project(dir.path(), "api");
    let stranger = project(dir.path(), "stranger");
    welcome::remember(&state, &api, "api", false);
    let e = projects::open(&state, &stranger.display().to_string(), None, None, false).unwrap_err();
    assert!(e.contains("not a recent project"), "{e}");
    // A record whose port answers nothing is stale: not "open elsewhere".
    projects::present(&state, &api, 1);
    assert!(projects::presence_of(&state, &api).is_none());
    // Served by a living process that listens: named, nothing started.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    projects::present(&state, &api, listener.local_addr().unwrap().port());
    // The presence is the folder's, not its repository's.
    assert!(projects::presence_of(&state, &stranger).is_none());
    match projects::open(&state, "api", None, None, false).unwrap() {
        Opened::Elsewhere { pid, .. } => assert_eq!(pid, std::process::id()),
        o @ Opened::Started { .. } => panic!("{o:?}"),
    }
    projects::absent(&state, &api);
    assert!(projects::presence_of(&state, &api).is_none());
}

#[test]
fn a_stop_is_reported_as_a_draft_naming_where_it_came_from() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let web = project(dir.path(), "web");
    assert!(projects::report_text(&web, "001-graph", "T005").is_err());
    // An attempt that breaks the check: saved, not ticked.
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": "pub fn broken() {}\n" }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    let _ = speccoder_runtime::run_with(&web, None, "T005", &state, d, None);
    let text = projects::report_text(&web, "001-graph", "T005").unwrap();
    assert!(text.starts_with("From web · 001-graph · T005: "), "{text}");
    assert!(text.chars().count() <= projects::MAX_REPORT);
    assert!(!text.contains("pub fn broken"), "{text}");
}

#[test]
fn a_report_needs_what_should_happen_and_another_project() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let web = project(dir.path(), "web");
    let api = project(dir.path(), "api");
    welcome::remember(&state, &api, "api", false);
    welcome::remember(&state, &web, "web", false);
    for (which, expected, why) in [
        ("api", "  ", "what should happen instead"),
        ("api", "two\nlines", "what should happen instead"),
        ("web", "it works", "this project"),
        ("nowhere", "it works", "not a recent project"),
        ("api", "it works", "no stopped attempt"),
    ] {
        let e =
            projects::file_report(&state, &web, which, "001-graph", "T005", expected).unwrap_err();
        assert!(e.contains(why), "{which}/{expected:?}: {e}");
    }
    // api runs a task now: its checkout must not change under it.
    let live = speccoder_runtime::worktree::project_state_dir(&state, &api).join("live.json");
    fs::create_dir_all(live.parent().unwrap()).unwrap();
    let running = speccoder_runtime::progress::LiveState {
        pid: std::process::id(),
        task: "T004".into(),
        ..Default::default()
    };
    fs::write(&live, serde_json::to_string(&running).unwrap()).unwrap();
    let e =
        projects::file_report(&state, &web, "api", "001-graph", "T005", "it works").unwrap_err();
    assert!(e.contains("running T004"), "{e}");
    // Nothing was written into api.
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&api)
        .output()
        .unwrap();
    assert!(status.stdout.is_empty());
}

#[test]
fn a_repository_never_set_up_is_idle_not_a_worry() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let plain = dir.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    fs::write(plain.join("README.md"), "hi\n").unwrap();
    git(&plain, &["init", "-q"]);
    git(&plain, &["add", "-A"]);
    git(&plain, &["commit", "-q", "-m", "init"]);
    welcome::remember(&state, &plain, "plain", false);
    let r = projects::list(&state, None);
    let p = &r.projects[0];
    assert_eq!(p.state, ProjectState::Idle, "{r:#?}");
    assert_eq!(p.unknown.as_deref(), Some("not set up yet"));
}
