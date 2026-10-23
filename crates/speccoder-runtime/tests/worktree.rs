//! Worktree isolation and checkpoints against temp git repositories
//!.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_runtime::worktree::{
    Artifacts, ChangeKind, checkpoint, diff, ensure_worktree, require_clean, restore,
};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// A repository with a committed Spec Kit feature.
fn spec_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q"]);
    write(
        r,
        ".specify/init-options.json",
        "{\"speckit_version\":\"1.1.3\"}",
    );
    write(r, ".specify/.gitignore", "feature.json\n");
    write(
        r,
        ".specify/feature.json",
        "{\"feature_directory\":\"specs/001-x\"}",
    );
    write(r, "specs/001-x/spec.md", "# Spec\n");
    write(r, "specs/001-x/tasks.md", "## Phase 1\n- [ ] T001 a\n");
    write(r, "README.md", "hello\n");
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn artifacts() -> Artifacts {
    Artifacts {
        specify_dir: ".specify".to_owned(),
        feature_dir: "specs/001-x".to_owned(),
    }
}

/// Everything a user could notice: files (incl. .git/index), status, HEAD, refs.
fn user_view(root: &Path) -> (BTreeMap<PathBuf, Vec<u8>>, String, String, String) {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let meta = fs::symlink_metadata(&p).unwrap();
            let skip = p.strip_prefix(root).unwrap().starts_with(".git")
                && p.file_name().is_some_and(|n| n != "index" && n != ".git");
            if meta.is_dir() {
                if !p.ends_with(".git/objects")
                    && !p.ends_with(".git/logs")
                    && !p.ends_with(".git/worktrees")
                {
                    stack.push(p);
                }
            } else if !skip || p.ends_with(".git/index") {
                files.insert(p.clone(), fs::read(&p).unwrap_or_default());
            }
        }
    }
    (
        files,
        git(root, &["status", "--porcelain=v1", "--untracked-files=all"]),
        git(root, &["rev-parse", "HEAD"]),
        git(root, &["for-each-ref"])
            .lines()
            .filter(|l| !l.contains("refs/heads/speccoder/"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

// ── US1 ────────────────────────────────────────────────────────────────────

#[test]
fn worktree_is_created_beside_the_checkout_and_reused() {
    let repo = spec_repo();
    let state = tempfile::tempdir().unwrap();
    let r = repo.path();
    // The user's own uncommitted work.
    write(r, "README.md", "edited by the user\n");
    write(r, "notes.txt", "untracked\n");
    write(r, "src.rs", "fn main() {}\n");
    git(r, &["add", "src.rs"]);
    let before = user_view(r);

    let wt = ensure_worktree(r, state.path(), "001-x", &artifacts()).unwrap();
    assert!(wt.created);
    assert_eq!(wt.branch, "speccoder/001-x");
    assert!(wt.path.starts_with(fs::canonicalize(state.path()).unwrap()));
    assert_eq!(wt.base, git(r, &["rev-parse", "HEAD"]).trim());
    assert_eq!(
        fs::read_to_string(wt.path.join("README.md")).unwrap(),
        "hello\n"
    );
    assert!(
        !wt.path.join("notes.txt").exists(),
        "user's untracked files stay in their checkout"
    );
    assert!(wt.path.join("specs/001-x/tasks.md").exists());

    assert_eq!(before, user_view(r), "the user's checkout changed");

    let again = ensure_worktree(r, state.path(), "001-x", &artifacts()).unwrap();
    assert!(!again.created);
    assert_eq!(again.path, wt.path);
}

#[test]
fn a_branch_whose_worktree_was_removed_is_reattached() {
    let repo = spec_repo();
    let state = tempfile::tempdir().unwrap();
    let wt = ensure_worktree(repo.path(), state.path(), "001-x", &artifacts()).unwrap();
    write(&wt.path, "work.txt", "progress\n");
    git(&wt.path, &["add", "work.txt"]);
    git(&wt.path, &["commit", "-q", "-m", "progress"]);
    let progress = git(&wt.path, &["rev-parse", "HEAD"]);
    fs::remove_dir_all(&wt.path).unwrap();

    let back = ensure_worktree(repo.path(), state.path(), "001-x", &artifacts()).unwrap();
    assert!(back.created);
    assert_eq!(back.base, progress.trim(), "history on the branch is kept");
    assert!(back.path.join("work.txt").exists());
}

// ── US2 ────────────────────────────────────────────────────────────────────

#[test]
fn ignored_specs_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q"]);
    write(r, ".gitignore", "/specs/\n");
    write(r, ".specify/init-options.json", "{}");
    write(r, "specs/001-x/spec.md", "# Spec\n");
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let state = tempfile::tempdir().unwrap();
    let refusal = ensure_worktree(r, state.path(), "001-x", &artifacts()).unwrap_err();
    assert!(
        refusal
            .paths
            .iter()
            .any(|(p, why)| p.starts_with("specs/001-x") && why == "ignored"),
        "{refusal:?}"
    );
}

#[test]
fn modified_and_untracked_artifacts_are_refused() {
    let repo = spec_repo();
    let state = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        "specs/001-x/tasks.md",
        "## Phase 1\n- [x] T001 a\n",
    );
    write(repo.path(), "specs/001-x/research.md", "# Research\n");
    let refusal = ensure_worktree(repo.path(), state.path(), "001-x", &artifacts()).unwrap_err();
    assert_eq!(
        refusal.paths,
        [
            ("specs/001-x/research.md".to_owned(), "untracked".to_owned()),
            ("specs/001-x/tasks.md".to_owned(), "modified".to_owned()),
        ]
    );
    // Spec Kit's own ignored per-checkout pointer is not a reason to refuse.
    assert!(
        !refusal
            .paths
            .iter()
            .any(|(p, _)| p.ends_with("feature.json"))
    );
}

#[test]
fn a_repository_without_commits_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    let state = tempfile::tempdir().unwrap();
    let refusal = ensure_worktree(dir.path(), state.path(), "001-x", &artifacts()).unwrap_err();
    assert!(refusal.message.contains("no commits"), "{refusal:?}");
}

#[test]
fn require_clean_lists_uncommitted_changes() {
    let repo = spec_repo();
    assert!(require_clean(repo.path()).is_ok());
    write(repo.path(), "README.md", "changed\n");
    let refusal = require_clean(repo.path()).unwrap_err();
    assert_eq!(
        refusal.paths,
        [("README.md".to_owned(), "modified".to_owned())]
    );
}

// ── US3 ────────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn checkpoint_diff_and_restore_round_trip() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let repo = spec_repo();
    let r = repo.path();
    let tmp = tempfile::tempdir().unwrap();
    write(r, ".gitignore", "/target/\n");
    write(r, "a.txt", "a\n");
    write(r, "b.txt", "b\n");
    write(r, "run.sh", "#!/bin/sh\n");
    fs::set_permissions(r.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("a.txt", r.join("link")).unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "files"]);
    write(
        r,
        "untracked new.txt",
        "not committed but part of the checkpoint\n",
    );
    write(r, "odd\nname.txt", "newline in the name\n");
    write(r, "target/out.bin", "build output\n");

    let head = git(r, &["rev-parse", "HEAD"]);
    let refs = git(r, &["for-each-ref"]);
    let index = fs::read(r.join(".git/index")).unwrap();

    let c1 = checkpoint(r, tmp.path()).unwrap();
    assert_eq!(
        c1,
        checkpoint(r, tmp.path()).unwrap(),
        "checkpoints are deterministic"
    );

    // An attempt: modify, delete, add (nested), change mode and link, touch build output.
    write(r, "a.txt", "changed\n");
    fs::remove_file(r.join("b.txt")).unwrap();
    write(r, "new/deep/n.txt", "added\n");
    fs::set_permissions(r.join("run.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::remove_file(r.join("link")).unwrap();
    symlink("b.txt", r.join("link")).unwrap();
    write(r, "target/out.bin", "rebuilt\n");

    let c2 = checkpoint(r, tmp.path()).unwrap();
    let mut changes: Vec<_> = diff(r, &c1, &c2)
        .unwrap()
        .into_iter()
        .map(|c| (c.path, c.kind))
        .collect();
    changes.sort();
    assert_eq!(
        changes,
        [
            ("a.txt".to_owned(), ChangeKind::Modified),
            ("b.txt".to_owned(), ChangeKind::Deleted),
            ("link".to_owned(), ChangeKind::Modified),
            ("new/deep/n.txt".to_owned(), ChangeKind::Added),
            ("run.sh".to_owned(), ChangeKind::Modified),
        ]
    );

    restore(r, tmp.path(), &c1).unwrap();
    assert_eq!(
        checkpoint(r, tmp.path()).unwrap(),
        c1,
        "tree equals the checkpoint"
    );
    assert_eq!(fs::read_to_string(r.join("a.txt")).unwrap(), "a\n");
    assert_eq!(fs::read_to_string(r.join("b.txt")).unwrap(), "b\n");
    assert!(
        !r.join("new").exists(),
        "directories created by the attempt are gone"
    );
    assert_eq!(fs::read_link(r.join("link")).unwrap(), Path::new("a.txt"));
    assert_eq!(
        fs::metadata(r.join("run.sh")).unwrap().permissions().mode() & 0o111,
        0o111
    );
    assert_eq!(
        fs::read_to_string(r.join("odd\nname.txt")).unwrap(),
        "newline in the name\n"
    );
    assert_eq!(
        fs::read_to_string(r.join("target/out.bin")).unwrap(),
        "rebuilt\n",
        "ignored files untouched"
    );

    assert_eq!(git(r, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(r, &["for-each-ref"]), refs);
    assert_eq!(
        fs::read(r.join(".git/index")).unwrap(),
        index,
        "the index is untouched"
    );
}

#[test]
fn restore_without_changes_is_a_no_op() {
    let repo = spec_repo();
    let tmp = tempfile::tempdir().unwrap();
    let c = checkpoint(repo.path(), tmp.path()).unwrap();
    assert!(restore(repo.path(), tmp.path(), &c).unwrap().is_empty());
}
