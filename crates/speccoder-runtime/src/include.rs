//! `[worktree] include` and `.worktreeinclude` (Claude Code's file,
//! `.gitignore` syntax): git-ignored files of the person's checkout — a
//! `.env`, a local config — copied into each worktree speccoder runs checks
//! in, to the same relative path.
//!
//! Only files git ignores in the checkout are copied, never a tracked one;
//! regular files only, never through a symlink out of the checkout, never
//! over a file the worktree tracks, and only inside the worktree. A copied
//! secret file's values are masked in everything speccoder shows or hands
//! on ([`crate::secrets`]), a commit never holds a copied file, and an
//! attempt whose diff adds one of those values is refused.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git::{Git, nul_fields};

/// Claude Code's file of patterns, read beside `[worktree] include`.
pub const FILE: &str = ".worktreeinclude";

/// The files the patterns name in the checkout's project directory that git
/// ignores there, project-relative: `[worktree] include`'s patterns and
/// `.worktreeinclude`'s, together. Regular files only, inside the checkout.
#[must_use]
pub fn matched(checkout: &Path, patterns: &[String]) -> Vec<String> {
    let file = checkout.join(FILE);
    let from_file = file.is_file();
    if patterns.is_empty() && !from_file {
        return Vec::new();
    }
    let g = Git::at(checkout);
    let mut args: Vec<String> = ["ls-files", "-z", "--others", "--ignored"]
        .map(str::to_owned)
        .to_vec();
    args.extend(patterns.iter().map(|p| format!("--exclude={p}")));
    if from_file {
        args.push(format!("--exclude-from={}", file.display()));
    }
    args.extend(["--".to_owned(), ".".to_owned()]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let named = g.run(&argv).map(|o| nul_fields(&o)).unwrap_or_default();
    if named.is_empty() {
        return Vec::new();
    }
    // Of those, what git ignores in the checkout (never a tracked file:
    // `--others` lists none).
    let mut input = Vec::new();
    for n in &named {
        input.extend_from_slice(n.as_bytes());
        input.push(0);
    }
    let ignored = g
        .run_with_stdin(&["check-ignore", "-z", "--stdin"], &input)
        .map(|o| nul_fields(&o))
        .unwrap_or_default();
    let home = std::fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf());
    let mut out: Vec<String> = named
        .into_iter()
        .filter(|n| ignored.contains(n))
        .filter(|n| {
            let p = checkout.join(n);
            std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_file())
                && std::fs::canonicalize(&p).is_ok_and(|c| c.starts_with(&home))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What a worktree holds copied from the checkout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The copied files, relative to the worktree's root (as git names them).
    #[serde(default)]
    pub files: Vec<String>,
}

/// Where a worktree's record of copies is kept: beside it, never inside.
#[must_use]
pub fn record_path(worktree: &Path) -> PathBuf {
    let name = worktree
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    worktree.with_file_name(format!("{name}.include.json"))
}

/// The worktree's record of copies (none: nothing copied).
#[must_use]
pub fn load(worktree: &Path) -> Record {
    std::fs::read_to_string(record_path(worktree))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Forgets a worktree's record of copies (before the worktree is removed).
pub fn forget(worktree: &Path) {
    let _ = std::fs::remove_file(record_path(worktree));
}

/// What copying did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copied {
    /// Every file the worktree holds copied, worktree-root-relative.
    pub files: Vec<String>,
    /// Those copied now (new, or changed in the checkout).
    pub now: Vec<String>,
    /// Why a matched file was not copied, one line each.
    pub skipped: Vec<String>,
}

/// Copies the files [`matched`] names from `checkout` (the project's
/// directory there) to the same paths under `project` (its directory in
/// the worktree `worktree`): new ones, and again those whose content
/// changed. Registers each copied secret file's values for masking.
///
/// # Errors
///
/// The record of copies could not be written.
pub fn copy(
    worktree: &Path,
    project: &Path,
    checkout: &Path,
    patterns: &[String],
) -> Result<Copied, String> {
    let names = matched(checkout, patterns);
    let mut out = Copied::default();
    let root =
        std::fs::canonicalize(worktree).map_err(|e| format!("{}: {e}", worktree.display()))?;
    let inner = std::fs::canonicalize(project)
        .ok()
        .and_then(|p| p.strip_prefix(&root).ok().map(Path::to_path_buf))
        .unwrap_or_default();
    // Never over a file the worktree tracks.
    let tracked: Vec<String> = if names.is_empty() {
        Vec::new()
    } else {
        let mut args = vec!["ls-files", "-z", "--"];
        args.extend(names.iter().map(String::as_str));
        Git::at(project)
            .run(&args)
            .map(|o| nul_fields(&o))
            .unwrap_or_default()
    };
    for n in &names {
        if tracked.contains(n) {
            out.skipped
                .push(format!("{n}: tracked in the worktree, left as committed"));
            continue;
        }
        let rel = inner.join(n);
        match copy_one(&root, &rel, &checkout.join(n)) {
            Ok(changed) => {
                let shown = rel.to_string_lossy().into_owned();
                if changed {
                    out.now.push(shown.clone());
                }
                out.files.push(shown);
            }
            Err(why) => out.skipped.push(format!("{n}: {why}")),
        }
    }
    let record = Record {
        files: out.files.clone(),
    };
    if record != load(worktree) {
        let path = record_path(worktree);
        let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
        if record.files.is_empty() {
            let _ = std::fs::remove_file(&path);
        } else {
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    register(worktree);
    Ok(out)
}

/// Copies `source` to `root/rel`, every directory on the way the
/// worktree's own (made, never followed through a link); `true` when the
/// file was written.
fn copy_one(root: &Path, rel: &Path, source: &Path) -> Result<bool, String> {
    let mut at = root.to_path_buf();
    let parts: Vec<_> = rel.components().collect();
    let Some((name, dirs)) = parts.split_last() else {
        return Err("not a file".to_owned());
    };
    for d in dirs {
        let std::path::Component::Normal(d) = d else {
            return Err("not a plain path".to_owned());
        };
        at.push(d);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("`{}` in the worktree is a link", at.display()));
            }
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(format!("`{}` in the worktree is a file", at.display())),
            Err(_) => std::fs::create_dir(&at).map_err(|e| format!("{}: {e}", at.display()))?,
        }
    }
    let std::path::Component::Normal(name) = name else {
        return Err("not a plain path".to_owned());
    };
    let dest = at.join(name);
    let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
    match std::fs::symlink_metadata(&dest) {
        // A link there (an attempt can make one) goes — itself, never
        // what it points to.
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::remove_file(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        }
        Ok(m) if m.is_dir() => return Err("a directory in the worktree".to_owned()),
        Ok(_) if std::fs::read(&dest).is_ok_and(|b| b == bytes) => return Ok(false),
        _ => {}
    }
    std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(true)
}

/// The values of the worktree's copied secret files (`.env*` and the rest
/// the model may not read), as `(NAME, value)`, read from the copies.
#[must_use]
pub fn values(worktree: &Path) -> Vec<(String, String)> {
    let protected = crate::tools::paths::Protected::default();
    load(worktree)
        .files
        .iter()
        .filter(|f| protected.is_secret(f))
        .filter_map(|f| std::fs::read_to_string(worktree.join(f)).ok())
        .flat_map(|t| crate::secrets::dotenv(&t))
        .filter(|(_, v)| crate::secrets::worth_masking(v))
        .collect()
}

/// Masks the worktree's copied values from now on ([`values`]).
pub fn register(worktree: &Path) {
    for (k, v) in values(worktree) {
        crate::secrets::register(&k, &v);
    }
}

/// The pathspecs keeping each copied file out of what `git add` stages in
/// the worktree at `top`, whatever the ignore rules say: one for each that
/// git does not ignore there now (an attempt can un-ignore one). An
/// ignored one needs none — and naming it would make `git add` fail.
#[must_use]
pub fn pathspecs(top: &Path) -> Vec<String> {
    let files = load(top).files;
    if files.is_empty() {
        return Vec::new();
    }
    let mut input = Vec::new();
    for f in &files {
        input.extend_from_slice(f.as_bytes());
        input.push(0);
    }
    let ignored = Git::at(top)
        .run_with_stdin(&["check-ignore", "-z", "--stdin"], &input)
        .map(|o| nul_fields(&o))
        .unwrap_or_default();
    files
        .iter()
        .filter(|f| !ignored.contains(f))
        .map(|f| format!(":(exclude,top,literal){f}"))
        .collect()
}

/// The variable whose copied value a patch's added lines hold.
#[must_use]
pub fn added_secret(patch: &str) -> Option<String> {
    patch
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .find_map(crate::secrets::found_in)
}

/// Refuses a commit in the worktree at `top` whose index holds a file
/// copied from the checkout (unstaged then) or whose staged change adds a
/// copied secret value. Nothing is committed then.
///
/// # Errors
///
/// What was refused, naming the file or the variable — never a value.
pub fn check_staged(top: &Path) -> Result<(), String> {
    let copied = load(top).files;
    if copied.is_empty() {
        return Ok(());
    }
    register(top);
    let g = Git::at(top);
    let staged = g
        .run(&["diff", "--cached", "--name-only", "-z", "--no-renames"])
        .map(|o| nul_fields(&o))
        .map_err(|e| e.0)?;
    let held: Vec<&String> = staged.iter().filter(|p| copied.contains(p)).collect();
    if !held.is_empty() {
        let mut reset = vec!["reset", "-q", "--"];
        reset.extend(held.iter().map(|p| p.as_str()));
        if g.run(&reset).is_err() {
            let mut rm = vec!["rm", "--cached", "-q", "--ignore-unmatch", "--"];
            rm.extend(held.iter().map(|p| p.as_str()));
            let _ = g.run(&rm);
        }
        return Err(format!(
            "refused: {} is copied from your checkout ([worktree] include) and never committed; unstaged, nothing committed",
            held.iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let patch = g
        .run(&["diff", "--cached", "-U0", "--no-color", "--no-ext-diff"])
        .map(|o| String::from_utf8_lossy(&o).into_owned())
        .map_err(|e| e.0)?;
    if let Some(name) = added_secret(&patch) {
        return Err(format!(
            "refused: the change adds the value of {name}, copied from your checkout ([worktree] include); nothing committed"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git {args:?}");
    }

    /// A checkout: `.env` and `local/*.json` ignored, `app.json` tracked,
    /// and a worktree of it.
    fn checkout() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path().join("checkout");
        std::fs::create_dir_all(r.join("local")).unwrap();
        std::fs::write(r.join(".gitignore"), ".env\nlocal/\nnotes.txt\n").unwrap();
        std::fs::write(r.join("app.json"), "{}").unwrap();
        git(&r, &["init", "-q"]);
        git(&r, &["add", "-A"]);
        git(&r, &["commit", "-qm", "init"]);
        std::fs::write(r.join(".env"), "API_TOKEN=include-test-Qw7 value\n").unwrap();
        std::fs::write(r.join("local/db.json"), "{\"pw\": 1}").unwrap();
        std::fs::write(r.join("notes.txt"), "x").unwrap();
        std::fs::write(r.join("untracked.json"), "x").unwrap();
        let wt = dir.path().join("wt");
        git(
            &r,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &wt.display().to_string(),
            ],
        );
        (dir, r)
    }

    #[test]
    fn only_ignored_files_the_patterns_name_are_matched() {
        let (_d, r) = checkout();
        let pats = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(matched(&r, &[]).is_empty());
        assert_eq!(matched(&r, &pats(&[".env"])), [".env"]);
        // Tracked (app.json) and untracked-but-not-ignored never.
        assert_eq!(matched(&r, &pats(&["*.json"])), ["local/db.json"]);
        // `.worktreeinclude` joins in.
        std::fs::write(r.join(FILE), "notes.txt\n# a comment\n").unwrap();
        assert_eq!(matched(&r, &pats(&[".env"])), [".env", "notes.txt"]);
        // A symlink is never followed out of the checkout.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "x").unwrap();
        std::fs::remove_file(r.join("notes.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), r.join("notes.txt")).unwrap();
        assert_eq!(matched(&r, &pats(&[".env"])), [".env"]);
    }

    #[test]
    fn copies_land_in_the_worktree_only_and_follow_changes() {
        let (d, r) = checkout();
        let wt = d.path().join("wt");
        let pats = vec![".env".to_owned(), "local/".to_owned()];
        let c = copy(&wt, &wt, &r, &pats).unwrap();
        assert_eq!(c.files, [".env", "local/db.json"]);
        assert_eq!(c.now, c.files);
        assert_eq!(load(&wt).files, c.files);
        assert!(
            std::fs::read_to_string(wt.join(".env"))
                .unwrap()
                .contains("Qw7")
        );
        // Unchanged: nothing copied again; changed: copied again.
        assert!(copy(&wt, &wt, &r, &pats).unwrap().now.is_empty());
        std::fs::write(r.join(".env"), "API_TOKEN=include-test-Qw8 value\n").unwrap();
        assert_eq!(copy(&wt, &wt, &r, &pats).unwrap().now, [".env"]);
        // A worktree directory that is a link is never written through.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::remove_dir_all(wt.join("local")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), wt.join("local")).unwrap();
        let c = copy(&wt, &wt, &r, &pats).unwrap();
        assert_eq!(c.files, [".env"]);
        assert!(
            c.skipped[0].starts_with("local/db.json: "),
            "{:?}",
            c.skipped
        );
        assert!(
            std::fs::read_dir(elsewhere.path())
                .unwrap()
                .next()
                .is_none()
        );
        // A file the worktree tracks stays as committed.
        std::fs::write(wt.join(".gitignore"), "local/\n").unwrap();
        git(&wt, &["add", "-f", ".env", ".gitignore"]);
        git(&wt, &["commit", "-qm", "track it"]);
        std::fs::write(wt.join(".env"), "MINE=1\n").unwrap();
        let c = copy(&wt, &wt, &r, &pats).unwrap();
        assert!(
            c.skipped.iter().any(|s| s.starts_with(".env: tracked")),
            "{:?}",
            c.skipped
        );
        assert_eq!(
            std::fs::read_to_string(wt.join(".env")).unwrap(),
            "MINE=1\n"
        );
    }

    #[test]
    fn a_staged_copy_or_a_staged_value_is_refused() {
        let (d, r) = checkout();
        let wt = d.path().join("wt");
        copy(&wt, &wt, &r, &[".env".to_owned()]).unwrap();
        assert_eq!(
            values(&wt),
            [("API_TOKEN".to_owned(), "include-test-Qw7 value".to_owned())]
        );
        assert!(check_staged(&wt).is_ok());
        git(&wt, &["add", "-f", ".env"]);
        let e = check_staged(&wt).unwrap_err();
        assert!(e.contains("`.env` is copied from your checkout"), "{e}");
        let staged = std::process::Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(&wt)
            .output()
            .unwrap();
        assert!(staged.stdout.is_empty(), "unstaged");
        std::fs::write(wt.join("app.json"), "{\"t\": \"include-test-Qw7 value\"}").unwrap();
        git(&wt, &["add", "app.json"]);
        let e = check_staged(&wt).unwrap_err();
        assert!(e.contains("adds the value of API_TOKEN"), "{e}");
        assert!(!e.contains("Qw7"));
        // Un-ignored by the attempt: kept out by a pathspec.
        assert!(pathspecs(&wt).is_empty(), "ignored: nothing to name");
        std::fs::write(wt.join(".gitignore"), "local/\n").unwrap();
        assert_eq!(pathspecs(&wt), [":(exclude,top,literal).env"]);
    }
}
