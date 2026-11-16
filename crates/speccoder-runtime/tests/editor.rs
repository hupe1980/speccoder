//! Opening a project file in the person's editor: the branch's
//! version from the feature's worktree, else the checkout; the launcher
//! gets one argument per piece, never a shell.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use speccoder_core::editor::OpenedIn;
use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::editor::{Env, open_with, resolve_editor};
use speccoder_runtime::user_config::EditorChoice;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[cfg(unix)]
#[test]
fn a_file_opens_from_the_features_worktree_else_the_checkout() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let top = d.path().join("repo");
    std::fs::create_dir_all(top.join("src")).unwrap();
    std::fs::write(top.join("src/old.rs"), "a\nb\n").unwrap();
    git(&top, &["init", "-q", "-b", "main"]);
    git(&top, &["add", "-A"]);
    git(&top, &["commit", "-qm", "init"]);
    let wt = d.path().join("wt");
    git(
        &top,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "speccoder/001-x",
            wt.to_str().unwrap(),
        ],
    );
    std::fs::write(wt.join("src/new file.rs"), "1\n2\n3\n4\n").unwrap();

    // A launcher that records its arguments, one per line.
    let log = d.path().join("calls");
    let launcher = d.path().join("record");
    std::fs::write(
        &launcher,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done >> '{}'\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let choice = EditorChoice::Command(vec![
        launcher.display().to_string(),
        "{file}".into(),
        "{line}".into(),
    ]);
    let editor = resolve_editor(Some(&choice), &Env::default(), false).unwrap();

    let res = open_with(
        &editor,
        &top,
        Some("001-x"),
        "src/new file.rs",
        Some(9),
        None,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Opened(o)) = res.data() else {
        panic!("{}", res.message())
    };
    assert_eq!(
        (o.location, o.line, o.path.as_str()),
        (OpenedIn::Worktree, Some(4), "src/new file.rs")
    );
    let calls = std::fs::read_to_string(&log).unwrap();
    let wt_real = std::fs::canonicalize(&wt).unwrap();
    assert_eq!(
        calls,
        format!("{}\n4\n", wt_real.join("src/new file.rs").display()),
        "one argument for a path with a space"
    );

    // Not on the branch's worktree list: the checkout's file.
    let res = open_with(&editor, &top, None, "src/old.rs", Some(1), None);
    let Some(CommandData::Opened(o)) = res.data() else {
        panic!("{}", res.message())
    };
    assert_eq!(o.location, OpenedIn::Checkout);

    // Outside, nothing runs.
    std::fs::remove_file(&log).unwrap();
    let refused = open_with(&editor, &top, Some("001-x"), "../wt/src/old.rs", None, None);
    assert_eq!(refused.code().map(|c| c.name), Some("OPEN-REFUSED"));
    assert!(!log.exists(), "the launcher ran for a refused path");
}
