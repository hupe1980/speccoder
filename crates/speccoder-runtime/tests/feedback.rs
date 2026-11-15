//! Feedback between projects (spec 125): an upstream lists what its
//! downstream projects ask of it; its decisions become their statuses; no
//! project's files change because another read or decided on them.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod fake_github;

use speccoder_core::feedback::Status;
use speccoder_runtime::{feedback, welcome};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A project `name` under `parent` whose `speccoder.toml` is `toml`.
fn project(parent: &Path, name: &str, toml: &str) -> PathBuf {
    let r = parent.join(name);
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::write(r.join(".specify/memory/constitution.md"), "# C\n").unwrap();
    fs::write(
        r.join("specs/001-x/tasks.md"),
        "# Tasks\n\n- [ ] T001 Do it\n",
    )
    .unwrap();
    fs::write(r.join("speccoder.toml"), toml).unwrap();
    git(&r, &["init", "-q"]);
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    fs::canonicalize(r).unwrap()
}

const OUTBOX: &str = "# For lib\n\n## Open\n\n### AP-1 — retry on 429\n\nPlease retry.\n\n### AP-2 — export the journal\n\nAs JSON.\n";

#[test]
fn an_upstream_sees_what_is_asked_and_its_decisions_become_statuses() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let lib = project(dir.path(), "lib", "");
    let app = project(
        dir.path(),
        "app",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(app.join("FEEDBACK.md"), OUTBOX).unwrap();
    welcome::remember(&state, &lib, "lib", false);
    welcome::remember(&state, &app, "app", false);
    let before = (
        git(&app, &["status", "--porcelain"]),
        git(&lib, &["status", "--porcelain"]),
    );

    // The upstream lists both, not seen.
    let asked_of_lib = feedback::requests(&state, &lib);
    assert_eq!(asked_of_lib.len(), 2, "{asked_of_lib:#?}");
    assert_eq!(asked_of_lib[0].source, "app");
    assert_eq!(asked_of_lib[0].item.id, "AP-1");
    assert_eq!(asked_of_lib[0].status, Status::NotSeen);

    // The downstream: not seen yet.
    let began = std::time::Instant::now();
    let ups = feedback::asked(&state, &app);
    assert!(
        began.elapsed() < std::time::Duration::from_secs(2),
        "SC-002"
    );
    assert_eq!(ups[0].items[0].status, Status::NotSeen);

    // Decisions in the upstream's own state become the downstream's statuses.
    feedback::decide(
        &state,
        &lib,
        "app",
        "AP-1",
        Status::TakenUp {
            unit: "002-retry".into(),
        },
    )
    .unwrap();
    feedback::decide(
        &state,
        &lib,
        "app",
        "AP-2",
        Status::Declined {
            reason: "out of scope".into(),
        },
    )
    .unwrap();
    let ups = feedback::asked(&state, &app);
    assert_eq!(
        ups[0].items[0].status,
        Status::TakenUp {
            unit: "002-retry".into()
        }
    );
    assert_eq!(
        ups[0].items[1].status,
        Status::Declined {
            reason: "out of scope".into()
        }
    );

    // An agent's word needs a commit of the upstream.
    let head = git(&lib, &["rev-parse", "HEAD"]);
    assert!(feedback::resolve(&state, &lib, "app", "AP-1", "deadbeefdeadbeef").is_err());
    assert!(feedback::resolve(&state, &lib, "app", "AP-1", "--all").is_err());
    let short = feedback::resolve(&state, &lib, "app", "AP-1", &head[..10]).unwrap();
    assert!(head.starts_with(&short));
    let ups = feedback::asked(&state, &app);
    assert_eq!(
        ups[0].items[0].status,
        Status::Fixed {
            commit: short,
            by_agent: true
        }
    );

    // A changed item says so on both sides.
    fs::write(
        app.join("FEEDBACK.md"),
        OUTBOX.replace("As JSON.", "As JSON lines."),
    )
    .unwrap();
    assert!(feedback::asked(&state, &app)[0].items[1].changed);
    assert!(feedback::requests(&state, &lib)[1].changed);

    // No project's files changed.
    fs::write(app.join("FEEDBACK.md"), OUTBOX).unwrap();
    assert_eq!(
        before,
        (
            git(&app, &["status", "--porcelain"]),
            git(&lib, &["status", "--porcelain"])
        )
    );

    // An unknown request is refused.
    assert!(feedback::decide(&state, &lib, "app", "AP-9", Status::NotSeen).is_err());
}

#[test]
fn an_upstream_that_cannot_be_read_keeps_its_last_status() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let lib = project(dir.path(), "lib", "");
    let app = project(
        dir.path(),
        "app",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(app.join("FEEDBACK.md"), OUTBOX).unwrap();
    welcome::remember(&state, &app, "app", false);
    feedback::decide(
        &state,
        &lib,
        "app",
        "AP-1",
        Status::Bug {
            task: "002-bugfixes/T001".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        feedback::asked(&state, &app)[0].items[0].status,
        Status::Bug { .. }
    ));
    fs::rename(&lib, dir.path().join("moved")).unwrap();
    let s = &feedback::asked(&state, &app)[0].items[0].status;
    match s {
        Status::Unknown { why } => {
            assert!(why.contains("recorded as bug 002-bugfixes/T001"), "{why}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_outbox_lists_its_upstream_and_where_to_write() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let app = project(dir.path(), "app", "[upstream.lib]\nrepo = \"you/lib\"\n");
    let ups = feedback::asked(&state, &app);
    assert_eq!(ups.len(), 1);
    assert!(!ups[0].exists);
    assert_eq!(ups[0].outbox, ".speccoder/outbox/lib.md");
    assert_eq!(ups[0].repo.as_deref(), Some("you/lib"));
    // A repo that is not owner/name is refused.
    let bad = project(dir.path(), "bad", "[upstream.lib]\nrepo = \"lib\"\n");
    assert!(feedback::upstreams(&bad).is_err());
}

#[test]
fn an_item_for_a_repository_is_a_prepared_issue_filed_once() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let app = project(
        dir.path(),
        "app",
        "[upstream.lib]\nrepo = \"o/lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(app.join("FEEDBACK.md"), OUTBOX).unwrap();
    let p = feedback::prepared(&state, &app, "lib", "AP-1").unwrap();
    assert_eq!(
        (p.repo.as_str(), p.title.as_str()),
        ("o/lib", "retry on 429")
    );
    assert!(
        p.body.starts_with("Please retry.") && p.body.contains("From app (AP-1) at "),
        "{}",
        p.body
    );
    assert!(
        p.url.starts_with(
            "https://github.com/o/lib/issues/new?title=retry%20on%20429&body=Please%20retry."
        ),
        "{}",
        p.url
    );
    // A text too long for an address is shortened there, never in the issue.
    fs::write(
        app.join("FEEDBACK.md"),
        OUTBOX.replace("Please retry.", &"x".repeat(9000)),
    )
    .unwrap();
    let long = feedback::prepared(&state, &app, "lib", "AP-1").unwrap();
    assert!(
        long.url.len() <= 7000 && long.url.contains("shortened"),
        "{}",
        long.url.len()
    );
    assert!(long.body.len() > 9000);
    fs::write(app.join("FEEDBACK.md"), OUTBOX).unwrap();

    let api = fake_github::serve(vec![(
        "POST /repos/o/lib/issues",
        201,
        r#"{"number":57,"html_url":"https://github.com/o/lib/issues/57"}"#.to_owned(),
    )]);
    let ctx = speccoder_runtime::github::Ctx::fixed(&api.url, Some("t"));
    let (n, _) = feedback::file_issue(&ctx, &state, &app, "lib", "AP-1").unwrap();
    assert_eq!(n, 57);
    let sent = api.requests.lock().unwrap().clone();
    assert!(sent[0].body.contains("retry on 429"), "{:?}", sent[0].body);
    // Never twice; its status says where it is.
    assert!(
        feedback::file_issue(&ctx, &state, &app, "lib", "AP-1")
            .unwrap_err()
            .contains("filed already")
    );
    assert_eq!(api.requests.lock().unwrap().len(), 1);
    assert!(matches!(
        feedback::asked(&state, &app)[0].items[0].status,
        Status::Filed { number: 57, .. }
    ));
}

#[test]
fn an_outbox_stays_in_its_project_and_a_resolve_needs_history() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    // Outside the project, by its setting or through a link.
    let out = project(
        dir.path(),
        "out",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"../lib/CHANGELOG.md\"\n",
    );
    assert!(
        feedback::upstreams(&out)
            .unwrap_err()
            .contains("inside this project")
    );
    let named = project(dir.path(), "named", "[upstream.\"../x\"]\nrepo = \"o/x\"\n");
    assert!(feedback::upstreams(&named).is_err());
    let lib = project(dir.path(), "lib", "");
    let app = project(dir.path(), "app", "[upstream.lib]\npath = \"../lib\"\n");
    std::os::unix::fs::symlink(&lib, app.join(".speccoder")).unwrap();
    let e = feedback::report_item(&app, "lib", "retry", "Please.").unwrap_err();
    assert!(e.contains("leads out of this project"), "{e}");
    assert!(!lib.join("outbox").exists());

    // A commit of the upstream that is not in its history is refused.
    let app2 = project(
        dir.path(),
        "app2",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(app2.join("FEEDBACK.md"), OUTBOX).unwrap();
    welcome::remember(&state, &app2, "app2", false);
    git(&lib, &["checkout", "-q", "-b", "side"]);
    fs::write(lib.join("side.txt"), "x").unwrap();
    git(&lib, &["add", "-A"]);
    git(&lib, &["commit", "-q", "-m", "side"]);
    let side = git(&lib, &["rev-parse", "HEAD"]);
    git(&lib, &["checkout", "-q", "main"]);
    let e = feedback::resolve(&state, &lib, "app2", "AP-1", &side).unwrap_err();
    assert!(e.contains("not in this project's history"), "{e}");
}
