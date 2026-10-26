//! A revision's intent, written out from git objects into a temporary
//! directory, so the format adapters can read the spec as a commit holds
//! it. Only `git` runs; the working tree is never read, and nothing in the
//! written tree is executed (symlinks and submodules are left out).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::git::Git;
use crate::tools::paths::Class;
use crate::tools::policy::Policy;

/// The top-level entries of `rev` a spec format reads: its files, its dot
/// directories, and every directory holding a format's intent.
///
/// # Errors
///
/// `git ls-tree` fails (the revision is not here, or git cannot read it).
pub(crate) fn intent_tops(repo: &Path, rev: &str) -> Result<Vec<String>, String> {
    let out = Git::at(repo)
        .run(&["ls-tree", "-r", "-z", "--name-only", "--full-tree", rev])
        .map_err(|e| format!("cannot list the tree of {rev}: {}", e.0))?;
    let policies: Vec<Policy> = crate::format::adapters()
        .into_iter()
        .map(|a| Policy::new(&[], &[]).rules(a.rules()))
        .collect();
    let mut tops = BTreeSet::new();
    for path in out.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = String::from_utf8_lossy(path);
        match path.split_once('/') {
            None => {
                tops.insert(path.into_owned());
            }
            Some((top, _)) if top.starts_with('.') => {
                tops.insert(top.to_owned());
            }
            Some((top, _)) => {
                if !tops.contains(top)
                    && policies
                        .iter()
                        .any(|p| p.write_protected(&path) == Some(Class::Intent))
                {
                    tops.insert(top.to_owned());
                }
            }
        }
    }
    Ok(tops.into_iter().collect())
}

/// Writes the blobs of `rev` under `tops` (every top-level entry when
/// `None`) into a new temporary directory. Every blob listed is written,
/// or none is: a tree this cannot write out whole (a blob git cannot read,
/// a path the file system refuses, such as one colliding by letter case)
/// is an error, never a smaller tree.
///
/// # Errors
///
/// git fails, a blob is missing or malformed, or a file cannot be written.
pub(crate) fn materialize(
    repo: &Path,
    rev: &str,
    tops: Option<&[String]>,
) -> Result<tempfile::TempDir, String> {
    let git = Git::at(repo);
    let tmp = || tempfile::tempdir().map_err(|e| format!("no temporary directory: {e}"));
    let mut args = vec!["ls-tree", "-r", "-z", "--full-tree", rev];
    if let Some(tops) = tops {
        if tops.is_empty() {
            return tmp();
        }
        args.push("--");
        args.extend(tops.iter().map(String::as_str));
    }
    let listing = git
        .run(&args)
        .map_err(|e| format!("cannot list the tree of {rev}: {}", e.0))?;
    // `<mode> blob <oid>\t<path>`; links and submodules stay out.
    let mut wanted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split(' ');
        let (Some(mode), Some("blob"), Some(oid)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if mode == "120000" || !safe(path) {
            continue;
        }
        wanted
            .entry(oid.to_owned())
            .or_default()
            .push(path.to_owned());
    }
    folds_apart(wanted.values().flatten())?;
    let dir = tmp()?;
    let input: String = wanted.keys().map(|oid| format!("{oid}\n")).collect();
    let out = git
        .run_with_stdin(&["cat-file", "--batch"], input.as_bytes())
        .map_err(|e| format!("cannot read the blobs of {rev}: {}", e.0))?;
    // `<oid> blob <size>\n<content>\n`, one after another.
    let mut rest = out.as_slice();
    let mut written = 0usize;
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        let mut fields = header.split(' ');
        let (Some(oid), Some("blob"), Some(size)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!("git cannot read a blob of {rev}: `{header}`"));
        };
        let size: usize = size
            .parse()
            .map_err(|_| format!("git cannot read a blob of {rev}: `{header}`"))?;
        let body = rest
            .get(..size)
            .ok_or_else(|| format!("the blob {oid} of {rev} is cut short"))?;
        let paths = wanted
            .get(oid)
            .ok_or_else(|| format!("git returned a blob of {rev} not asked for: {oid}"))?;
        for path in paths {
            let file = dir.path().join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot write {path} of {rev}: {e}"))?;
            }
            std::fs::write(&file, body)
                .map_err(|e| format!("cannot write {path} of {rev}: {e}"))?;
        }
        written += 1;
        rest = rest.get(size + 1..).unwrap_or_default();
    }
    if written != wanted.len() {
        return Err(format!(
            "git returned {written} of the {} blobs of {rev}",
            wanted.len()
        ));
    }
    Ok(dir)
}

/// Refuses paths that one file system would write as one: two entries, or
/// a file and a directory, equal but for letter case. Written out on a
/// case-insensitive file system (macOS, Windows) the later would replace or
/// block the earlier, and the adapters would read another tree than git
/// holds.
fn folds_apart<'a>(paths: impl Iterator<Item = &'a String>) -> Result<(), String> {
    let mut seen: BTreeMap<String, &'a str> = BTreeMap::new();
    for path in paths {
        let mut end = 0;
        for part in path.split('/') {
            end += part.len();
            let entry = &path[..end];
            end += 1;
            let folded = entry.to_lowercase();
            match seen.get(&folded) {
                Some(other) if *other != entry => {
                    return Err(format!(
                        "the tree holds {other} and {entry}, equal but for letter case: not written out"
                    ));
                }
                Some(_) => {}
                None => {
                    seen.insert(folded, entry);
                }
            }
        }
    }
    Ok(())
}

/// A relative path of plain components: nothing a tree entry could use
/// to write outside the directory.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_relative_paths_are_written() {
        assert!(safe("specs/001-x/spec.md"));
        assert!(!safe("../x"));
        assert!(!safe("/etc/passwd"));
        assert!(!safe("a/../../b"));
        assert!(!safe(""));
    }

    fn repo_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap();
            assert!(
                ok.status.success(),
                "{}",
                String::from_utf8_lossy(&ok.stderr)
            );
            ok.stdout
        };
        git(&["init", "-q"]);
        // Through the index, so paths that collide here can be committed.
        for (i, f) in files.iter().enumerate() {
            let blob = std::process::Command::new("git")
                .args(["hash-object", "-w", "--stdin"])
                .current_dir(dir.path())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut c| {
                    use std::io::Write as _;
                    c.stdin
                        .take()
                        .unwrap()
                        .write_all(format!("{i}\n").as_bytes())?;
                    c.wait_with_output()
                })
                .unwrap();
            let oid = String::from_utf8_lossy(&blob.stdout).trim().to_owned();
            git(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{oid},{f}"),
            ]);
        }
        git(&["commit", "-qm", "x"]);
        dir
    }

    #[test]
    fn a_tree_is_written_whole_or_refused() {
        let ok = repo_with(&["specs/001/tasks.md", ".specify/memory/constitution.md"]);
        let dir = materialize(ok.path(), "HEAD", None).unwrap();
        assert!(dir.path().join("specs/001/tasks.md").is_file());
        // A revision git cannot read is an error, never an empty tree.
        assert!(intent_tops(ok.path(), "no-such-rev").is_err());
        assert!(materialize(ok.path(), "no-such-rev", None).is_err());
    }

    #[test]
    fn paths_equal_but_for_case_are_refused() {
        // A file and a directory, and two files: on macOS the later would
        // block or replace the earlier.
        for files in [
            &["Specs", "specs/001/tasks.md"][..],
            &["specs/001/tasks.md", "specs/001/TASKS.md"][..],
        ] {
            let repo = repo_with(files);
            let Err(e) = materialize(repo.path(), "HEAD", None) else {
                panic!("{files:?} written out");
            };
            assert!(e.contains("letter case"), "{e}");
        }
    }
}
