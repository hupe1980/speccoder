//! A trusted checkout, story 1: speccoder's own git never runs
//! a program the checkout chose — a worktree whose `.git` file an attempt
//! rewrote is refused, inherited `GIT_*` is ignored, and diffs read for
//! evidence never run an external diff or text conversion.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_runtime::git::Git;

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

/// A repository with one commit and a linked worktree beside it.
fn repo_with_worktree(parent: &Path) -> (PathBuf, PathBuf) {
    let main = parent.join("main");
    fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q"]);
    fs::write(main.join("a.txt"), "a\n").unwrap();
    git(&main, &["add", "a.txt"]);
    git(&main, &["commit", "-q", "-m", "init"]);
    let wt = parent.join("wt");
    git(&main, &["worktree", "add", "-q", wt.to_str().unwrap()]);
    (
        fs::canonicalize(&main).unwrap(),
        fs::canonicalize(&wt).unwrap(),
    )
}

/// A non-bare git directory at `at` whose config defines filter `f`,
/// which touches `marker` when it runs.
fn evil_git_dir(at: &Path, marker: &Path) {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    git(
        at.parent().unwrap(),
        &["init", "-q", "--bare", at.to_str().unwrap()],
    );
    let mut config = fs::read_to_string(at.join("config")).unwrap();
    config = config.replace("bare = true", "bare = false");
    config.push_str(&format!(
        "[filter \"f\"]\n\tclean = touch {}\n",
        marker.display()
    ));
    fs::write(at.join("config"), config).unwrap();
}

fn redirect(wt: &Path, to: &Path) {
    fs::write(wt.join(".git"), format!("gitdir: {}\n", to.display())).unwrap();
    fs::write(wt.join(".gitattributes"), "* filter=f\n").unwrap();
    fs::write(wt.join("b.txt"), "b\n").unwrap();
}

#[test]
fn a_git_dir_in_a_private_tmp_is_refused_and_its_filter_never_runs() {
    let t = tempfile::tempdir().unwrap();
    let (_main, wt) = repo_with_worktree(t.path());
    // Where a sandboxed command may write: its private tmp.
    let private = std::env::temp_dir().join(format!(
        "speccoder-{:012x}",
        u64::from(std::process::id()) * 7919 % 0xffff_ffff_ffff
    ));
    let _ = fs::remove_dir_all(&private);
    let evil = private.join("fake.git");
    let marker = t.path().join("PWNED");
    evil_git_dir(&evil, &marker);
    redirect(&wt, &evil);

    let err = Git::at(&wt).run(&["add", "-A"]).unwrap_err();
    assert!(
        err.0
            .contains("the worktree's .git was changed to point at"),
        "{err}"
    );
    assert!(err.0.contains("fake.git"), "{err}");
    assert!(!marker.exists(), "the filter ran");
    // From a subdirectory too.
    fs::create_dir_all(wt.join("src")).unwrap();
    assert!(Git::at(&wt.join("src")).run(&["status"]).is_err());
    assert!(!marker.exists());
    let _ = fs::remove_dir_all(&private);
}

#[test]
fn a_git_dir_inside_the_worktree_is_refused() {
    let t = tempfile::tempdir().unwrap();
    let (_main, wt) = repo_with_worktree(t.path());
    let evil = wt.join(".cache/fake.git");
    let marker = t.path().join("PWNED");
    evil_git_dir(&evil, &marker);
    redirect(&wt, &evil);
    assert!(Git::at(&wt).run(&["add", "-A"]).is_err());
    assert!(!marker.exists(), "the filter ran");
}

#[test]
fn a_git_dir_elsewhere_whose_back_link_does_not_name_this_worktree_is_refused() {
    let t = tempfile::tempdir().unwrap();
    let (main, wt) = repo_with_worktree(t.path());
    // Another worktree's admin dir: real, but not this one's.
    let other = t.path().join("other");
    git(&main, &["worktree", "add", "-q", other.to_str().unwrap()]);
    let admin = main.join(".git/worktrees/other");
    fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    let err = Git::at(&wt).run(&["status"]).unwrap_err();
    assert!(err.0.contains("refuses"), "{err}");
}

#[test]
fn a_healthy_worktree_and_a_separate_git_dir_work() {
    let t = tempfile::tempdir().unwrap();
    let (_main, wt) = repo_with_worktree(t.path());
    fs::write(wt.join("b.txt"), "b\n").unwrap();
    Git::at(&wt).run(&["add", "-A"]).unwrap();
    assert_eq!(
        Git::at(&wt)
            .toplevel()
            .unwrap()
            .map(|p| fs::canonicalize(p).unwrap()),
        Some(wt.clone())
    );
    // A submodule-style `.git` file: no back-link, outside the tree.
    let sep = t.path().join("sep");
    fs::create_dir_all(&sep).unwrap();
    git(
        &sep,
        &[
            "init",
            "-q",
            "--separate-git-dir",
            t.path().join("sep.git").to_str().unwrap(),
        ],
    );
    fs::write(sep.join("c.txt"), "c\n").unwrap();
    Git::at(&sep).run(&["add", "-A"]).unwrap();
}

#[test]
fn inherited_git_variables_are_not_used() {
    // The child: started below with GIT_DIR pointing elsewhere.
    if let Some(main) = std::env::var_os("TRUSTED_CHECKOUT_MAIN") {
        let top = Git::at(Path::new(&main)).toplevel().unwrap().unwrap();
        assert_eq!(fs::canonicalize(top).unwrap(), PathBuf::from(main));
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let (main, _wt) = repo_with_worktree(t.path());
    let elsewhere = t.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    git(&elsewhere, &["init", "-q"]);
    let ok = Command::new(std::env::current_exe().unwrap())
        .args([
            "inherited_git_variables_are_not_used",
            "--exact",
            "--nocapture",
        ])
        .env("TRUSTED_CHECKOUT_MAIN", &main)
        .env("GIT_DIR", elsewhere.join(".git"))
        .env("GIT_WORK_TREE", &elsewhere)
        .status()
        .unwrap()
        .success();
    assert!(ok, "speccoder's git used an inherited GIT_DIR");
}

#[test]
fn evidence_diffs_never_run_an_external_diff_or_textconv() {
    let t = tempfile::tempdir().unwrap();
    let (main, _wt) = repo_with_worktree(t.path());
    let marker = t.path().join("EXT");
    let ext = t.path().join("ext.sh");
    fs::write(&ext, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&ext, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&main, &["config", "diff.external", ext.to_str().unwrap()]);
    git(&main, &["config", "diff.t.textconv", ext.to_str().unwrap()]);
    fs::write(main.join(".gitattributes"), "*.txt diff=t\n").unwrap();
    fs::write(main.join("a.txt"), "changed\n").unwrap();
    let out = Git::at(&main).run(&["diff"]).unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(out.contains("+changed"), "{out}");
    let _ = Git::at(&main).run(&["log", "-p", "-1"]).unwrap();
    let _ = Git::at(&main).run(&["show", "HEAD"]).unwrap();
    assert!(!marker.exists(), "an external diff program ran");
}

/// A worktree speccoder made: under `<state>/<id>/worktrees/`, `id` the
/// repository's.
#[test]
fn a_worktree_speccoder_made_is_held_to_its_repository() {
    // The child: started below with the state directory set.
    if let Some(wt) = std::env::var_os("TRUSTED_CHECKOUT_WT") {
        let wt = PathBuf::from(wt);
        let case = std::env::var("TRUSTED_CHECKOUT_CASE").unwrap();
        let r = Git::at(&wt).run(&["status", "--porcelain"]);
        match case.as_str() {
            "healthy" => assert!(r.is_ok(), "{r:?}"),
            _ => assert!(r.unwrap_err().0.contains("refuses"), "{case}"),
        }
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let main = t.path().join("main");
    fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q"]);
    fs::write(main.join("a.txt"), "a\n").unwrap();
    git(&main, &["add", "a.txt"]);
    git(&main, &["commit", "-q", "-m", "init"]);
    let state = fs::canonicalize(t.path()).unwrap().join("state");
    let id = speccoder_runtime::worktree::repo_state_dir(&state, &Git::at(&main)).unwrap();
    let wt = id.join("worktrees").join("001-x");
    fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(&main, &["worktree", "add", "-q", wt.to_str().unwrap()]);
    let child = |case: &str| {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "a_worktree_speccoder_made_is_held_to_its_repository",
                "--exact",
                "--nocapture",
            ])
            .env("TRUSTED_CHECKOUT_WT", &wt)
            .env("TRUSTED_CHECKOUT_CASE", case)
            .env("SPECCODER_STATE_DIR", &state)
            .status()
            .unwrap()
            .success()
    };
    assert!(child("healthy"));
    // A git dir somewhere writable (a cache), with a forged back-link.
    let fake = t.path().join("cache/x.git");
    let marker = t.path().join("PWNED");
    evil_git_dir(&fake, &marker);
    fs::write(
        fake.join("gitdir"),
        format!("{}\n", wt.join(".git").display()),
    )
    .unwrap();
    let real = fs::read_to_string(wt.join(".git")).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", fake.display())).unwrap();
    assert!(child("redirected"));
    // A `.git` directory in its place.
    fs::remove_file(wt.join(".git")).unwrap();
    git(&wt, &["init", "-q"]);
    assert!(child("a directory"));
    fs::remove_dir_all(wt.join(".git")).unwrap();
    fs::write(wt.join(".git"), real).unwrap();
    assert!(child("healthy"));
    assert!(!marker.exists());
}
