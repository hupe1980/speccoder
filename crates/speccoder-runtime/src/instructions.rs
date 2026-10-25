//! The repository's instructions for agents: the committed
//! `AGENTS.md` files that apply to a task, as agents.md defines them —
//! "the closest AGENTS.md to the edited file wins".
//!
//! Per directory from the repository root down to each directory the task
//! names (else down to the project directory), the first of the configured
//! file names that exists; root first, nearest last, so the nearest has the
//! last word. At most [`BUDGET`] bytes in all: the nearest files are kept,
//! farther ones cut, and every cut is said. The text is the repository's,
//! never the person's: callers label it as worktree content.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bytes the instructions may take in a prompt (Codex's default too).
pub const BUDGET: usize = 32 * 1024;

/// One file given to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instruction {
    /// Repository-relative path, `/`-separated.
    pub path: String,
    /// sha256 of the file as committed.
    pub sha256: String,
    /// Its size.
    pub bytes: usize,
    /// Cut to fit the budget.
    pub cut: bool,
    /// What the model is given (cut files end in a marker).
    pub text: String,
}

/// The directories from the repository root down to `leaf` (both included),
/// as repository-relative paths (`""` is the root).
fn chain(leaf: &Path) -> Vec<PathBuf> {
    let mut out = vec![PathBuf::new()];
    let mut at = PathBuf::new();
    for c in leaf.components() {
        if let std::path::Component::Normal(part) = c {
            at.push(part);
            out.push(at.clone());
        }
    }
    out
}

/// The instructions for a task in `project` (inside the repository at
/// `repo`) that names `named` (project-relative paths), reading the first
/// of `names` per directory.
#[must_use]
pub fn collect(
    repo: &Path,
    project: &Path,
    named: &[String],
    names: &[String],
) -> Vec<Instruction> {
    if names.is_empty() {
        return Vec::new();
    }
    let Ok(repo) = std::fs::canonicalize(repo) else {
        return Vec::new();
    };
    let project = std::fs::canonicalize(project).unwrap_or_else(|_| repo.clone());
    let rel = project
        .strip_prefix(&repo)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let leaves: Vec<PathBuf> = if named.is_empty() {
        vec![rel.clone()]
    } else {
        named
            .iter()
            .map(|p| {
                rel.join(p)
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
            })
            .collect()
    };
    let mut dirs: BTreeSet<(usize, PathBuf)> = BTreeSet::new();
    for leaf in &leaves {
        for d in chain(leaf) {
            dirs.insert((d.components().count(), d));
        }
    }
    let mut found: Vec<(usize, String, String)> = Vec::new();
    for (depth, dir) in &dirs {
        for name in names {
            let file = repo.join(dir).join(name);
            // Inside the repository only: a symlink out of it is not read.
            let Ok(real) = std::fs::canonicalize(&file) else {
                continue;
            };
            if !real.starts_with(&repo) || !real.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&real) else {
                continue; // not UTF-8
            };
            if text.trim().is_empty() {
                continue;
            }
            let path = dir.join(name).to_string_lossy().replace('\\', "/");
            found.push((*depth, path, text));
            break;
        }
    }
    fit(found)
}

/// Root first in the result; the budget is spent nearest first.
fn fit(mut found: Vec<(usize, String, String)>) -> Vec<Instruction> {
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut left = BUDGET;
    let mut out: Vec<Instruction> = Vec::with_capacity(found.len());
    for (_, path, text) in found.into_iter().rev() {
        let sha256 = crate::integrity::sha256_hex(text.as_bytes());
        let bytes = text.len();
        let (text, cut) = if bytes <= left {
            left -= bytes;
            (text, false)
        } else {
            let mut end = left;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            left = 0;
            (
                format!(
                    "{}\n\n[… cut: the repository's instructions are limited to {} KiB, and nearer files come first]\n",
                    &text[..end],
                    BUDGET / 1024
                ),
                true,
            )
        };
        out.push(Instruction {
            path,
            sha256,
            bytes,
            cut,
            text,
        });
    }
    out.reverse();
    out
}

/// The prompt section: each file under its path, with what it ranks below.
#[must_use]
pub fn section(instructions: &[Instruction]) -> Option<String> {
    if instructions.is_empty() {
        return None;
    }
    let mut out = String::from(
        "# Repository instructions\n\nThe repository's own conventions for agents (AGENTS.md), root first, the nearest last — a nearer file overrides a farther one. Follow them; where they conflict with the task, the spec, the checks or the rules above, those win.\n",
    );
    for i in instructions {
        out.push_str(&format!("\n## {}\n\n{}\n", i.path, i.text.trim_end()));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (p, text) in files {
            let f = dir.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, text).unwrap();
        }
        dir
    }

    fn paths(v: &[Instruction]) -> Vec<&str> {
        v.iter().map(|i| i.path.as_str()).collect()
    }

    const AGENTS: &[&str] = &[];

    #[test]
    fn the_nearest_file_comes_last_and_others_are_not_read() {
        let t = tree(&[
            ("AGENTS.md", "root"),
            ("services/api/AGENTS.md", "api"),
            ("web/AGENTS.md", "web"),
        ]);
        let names = vec!["AGENTS.md".to_owned()];
        let got = collect(
            t.path(),
            t.path(),
            &["services/api/src/handler.rs".to_owned()],
            &names,
        );
        assert_eq!(paths(&got), ["AGENTS.md", "services/api/AGENTS.md"]);
        let none = collect(t.path(), t.path(), &[], &names);
        assert_eq!(paths(&none), ["AGENTS.md"]);
        let both = collect(
            t.path(),
            t.path(),
            &["web/x.ts".to_owned(), "services/api/y.rs".to_owned()],
            &names,
        );
        assert_eq!(
            paths(&both),
            ["AGENTS.md", "web/AGENTS.md", "services/api/AGENTS.md"],
            "by depth"
        );
        assert!(
            collect(
                t.path(),
                t.path(),
                &[],
                &AGENTS.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
            )
            .is_empty()
        );
    }

    #[test]
    fn fallback_names_are_read_one_per_directory() {
        let t = tree(&[
            ("CLAUDE.md", "claude"),
            ("pkg/AGENTS.md", "a"),
            ("pkg/CLAUDE.md", "c"),
        ]);
        let names = vec!["AGENTS.md".to_owned(), "CLAUDE.md".to_owned()];
        let got = collect(t.path(), t.path(), &["pkg/lib.rs".to_owned()], &names);
        assert_eq!(paths(&got), ["CLAUDE.md", "pkg/AGENTS.md"]);
    }

    #[test]
    fn a_project_in_a_subdirectory_gets_the_root_and_its_own() {
        let t = tree(&[
            ("AGENTS.md", "root"),
            ("app/AGENTS.md", "app"),
            ("app/specs/x", ""),
        ]);
        let got = collect(
            t.path(),
            &t.path().join("app"),
            &[],
            &["AGENTS.md".to_owned()],
        );
        assert_eq!(paths(&got), ["AGENTS.md", "app/AGENTS.md"]);
    }

    #[test]
    fn the_budget_keeps_the_nearest_and_says_what_was_cut() {
        let big = "x".repeat(BUDGET);
        let t = tree(&[("AGENTS.md", &big), ("a/AGENTS.md", "near")]);
        let got = collect(
            t.path(),
            t.path(),
            &["a/f.rs".to_owned()],
            &["AGENTS.md".to_owned()],
        );
        assert_eq!(paths(&got), ["AGENTS.md", "a/AGENTS.md"]);
        assert!(!got[1].cut && got[1].text == "near");
        assert!(got[0].cut && got[0].text.contains("cut:"));
        assert_eq!(got[0].bytes, BUDGET);
        let total: usize = got.iter().map(|i| i.text.len()).sum();
        assert!(total <= BUDGET + 200, "{total}");
    }

    #[test]
    fn symlinks_out_of_the_repository_and_binary_files_are_not_read() {
        let outside = tree(&[("secret.md", "outside")]);
        let t = tree(&[("pkg/AGENTS.md", "")]);
        std::os::unix::fs::symlink(outside.path().join("secret.md"), t.path().join("AGENTS.md"))
            .unwrap();
        std::fs::write(t.path().join("pkg/AGENTS.md"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(
            collect(
                t.path(),
                t.path(),
                &["pkg/x".to_owned()],
                &["AGENTS.md".to_owned()]
            )
            .is_empty()
        );
    }

    #[test]
    fn the_section_ranks_the_instructions_below_the_task() {
        let i = Instruction {
            path: "AGENTS.md".to_owned(),
            sha256: String::new(),
            bytes: 3,
            cut: false,
            text: "use nextest".to_owned(),
        };
        let s = section(&[i]).unwrap();
        assert!(s.contains("## AGENTS.md\n\nuse nextest") && s.contains("those win"));
        assert!(section(&[]).is_none());
    }
}
