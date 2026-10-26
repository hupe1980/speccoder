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
pub(crate) fn intent_tops(repo: &Path, rev: &str) -> Option<Vec<String>> {
    let out = Git::at(repo)
        .run(&["ls-tree", "-r", "-z", "--name-only", "--full-tree", rev])
        .ok()?;
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
    Some(tops.into_iter().collect())
}

/// Writes the blobs of `rev` under `tops` (every top-level entry when
/// `None`) into a new temporary directory.
pub(crate) fn materialize(
    repo: &Path,
    rev: &str,
    tops: Option<&[String]>,
) -> Option<tempfile::TempDir> {
    let git = Git::at(repo);
    let mut args = vec!["ls-tree", "-r", "-z", "--full-tree", rev];
    if let Some(tops) = tops {
        if tops.is_empty() {
            return tempfile::tempdir().ok();
        }
        args.push("--");
        args.extend(tops.iter().map(String::as_str));
    }
    let listing = git.run(&args).ok()?;
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
    let dir = tempfile::tempdir().ok()?;
    let input: String = wanted.keys().map(|oid| format!("{oid}\n")).collect();
    let out = git
        .run_with_stdin(&["cat-file", "--batch"], input.as_bytes())
        .ok()?;
    // `<oid> blob <size>\n<content>\n`, one after another.
    let mut rest = out.as_slice();
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        let mut fields = header.split(' ');
        let (Some(oid), Some("blob"), Some(size)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let size: usize = size.parse().ok()?;
        let body = rest.get(..size)?;
        for path in wanted.get(oid).into_iter().flatten() {
            let file = dir.path().join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).ok()?;
            }
            std::fs::write(&file, body).ok()?;
        }
        rest = rest.get(size + 1..).unwrap_or_default();
    }
    Some(dir)
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
}
