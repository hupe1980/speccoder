//! `--candidate PATCH|pr:N`: someone else's fix, judged as the attempt —
//! no implementer, no retries. A diff that touches a frozen reproduction, a
//! held-out path or a judge input is refused before it is applied; one that
//! does not apply is refused with the hunks git rejected.

use std::path::Path;

use speccoder_core::bug::{Candidate, CandidateOrigin, CandidateRefusal};

use crate::integrity::sha256_hex;

/// What `--candidate` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    /// A unified diff file.
    Patch(String),
    /// A GitHub pull request.
    Pr(u64),
}

/// `pr:N` or a path.
#[must_use]
pub fn parse(spec: &str) -> Option<Spec> {
    let spec = spec.trim();
    if let Some(n) = spec.strip_prefix("pr:") {
        return n.trim().trim_start_matches('#').parse().ok().map(Spec::Pr);
    }
    (!spec.is_empty()).then(|| Spec::Patch(spec.to_owned()))
}

/// The candidate's record for `diff`.
#[must_use]
pub fn record(spec: &Spec, diff: &str, head: Option<String>) -> Candidate {
    Candidate {
        origin: match spec {
            Spec::Patch(_) => CandidateOrigin::Patch,
            Spec::Pr(_) => CandidateOrigin::Pr,
        },
        path: match spec {
            Spec::Patch(p) => Some(p.clone()),
            Spec::Pr(_) => None,
        },
        number: match spec {
            Spec::Pr(n) => Some(*n),
            Spec::Patch(_) => None,
        },
        digest: format!("sha256:{}", sha256_hex(diff.as_bytes())),
        head_sha: head,
        applied: false,
        refused: None,
    }
}

/// The patch file `spec` names, read. From the terminal a path is the
/// person's to give; through any other interface (web, ACP, MCP) it must be
/// a file inside the project at `root`.
///
/// # Errors
///
/// It cannot be read, or it lies outside the project.
pub fn read_patch(
    root: &Path,
    cwd: &Path,
    spec: &str,
    via: speccoder_core::actions::Via,
) -> Result<String, String> {
    if via == speccoder_core::actions::Via::Terminal {
        return std::fs::read_to_string(cwd.join(spec))
            .or_else(|_| std::fs::read_to_string(spec))
            .map_err(|e| format!("{spec}: {e}"));
    }
    let outside = || {
        format!(
            "{spec}: a candidate patch given through {} must be a file in the project",
            via.as_str()
        )
    };
    let base = std::fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let file = std::fs::canonicalize(root.join(spec)).map_err(|_| outside())?;
    if !file.starts_with(&base) || !file.is_file() {
        return Err(outside());
    }
    std::fs::read_to_string(&file).map_err(|e| format!("{spec}: {e}"))
}

/// The paths a diff touches, as git itself reads it (both sides of a
/// rename, quoted paths, mode-only changes), from `git apply --numstat` run
/// forward and in reverse at the repository `repo`. `Err`: git cannot read
/// it as a patch, or it carries a binary hunk or a symbolic link — neither
/// is applied as a candidate (`BUG-CANDIDATE-APPLY`).
pub fn touched(repo: &Path, diff: &str) -> Result<Vec<String>, CandidateRefusal> {
    let refuse = |paths: Vec<String>| CandidateRefusal {
        code: "BUG-CANDIDATE-APPLY".to_owned(),
        paths,
    };
    let git = crate::git::Git::at(repo);
    let mut out: Vec<String> = Vec::new();
    let mut binary: Vec<String> = Vec::new();
    for reverse in [false, true] {
        let mut args = vec!["apply", "--numstat", "-z"];
        if reverse {
            args.push("-R");
        }
        args.push("-");
        let o = git
            .run_with_stdin(&args, diff.as_bytes())
            .map_err(|e| refuse(vec![e.0]))?;
        let fields = crate::git::nul_fields(&o);
        let mut i = 0;
        while i < fields.len() {
            let mut parts = fields[i].splitn(3, '\t');
            let (added, deleted, path) = (
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
            );
            i += 1;
            // A rename may be written `n\tm\t\0old\0new\0`.
            let paths: Vec<String> = if path.is_empty() {
                let p = fields[i..].iter().take(2).cloned().collect();
                i += 2;
                p
            } else {
                vec![path.to_owned()]
            };
            for p in paths.into_iter().filter(|p| !p.is_empty()) {
                if added == "-" && deleted == "-" && !binary.contains(&p) {
                    binary.push(p.clone());
                }
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    if out.is_empty() {
        return Err(refuse(vec!["no file in the patch".to_owned()]));
    }
    if !binary.is_empty() {
        return Err(refuse(
            binary
                .into_iter()
                .map(|p| format!("{p}: a binary hunk"))
                .collect(),
        ));
    }
    let summary = git
        .run_with_stdin(&["apply", "--summary", "-"], diff.as_bytes())
        .map_err(|e| refuse(vec![e.0]))?;
    let links: Vec<String> = String::from_utf8_lossy(&summary)
        .lines()
        .filter(|l| l.contains("120000"))
        .map(|l| format!("{}: a symbolic link", l.trim()))
        .collect();
    if !links.is_empty() {
        return Err(refuse(links));
    }
    Ok(out)
}

/// The touched paths that are judge inputs: protected paths, frozen tests,
/// held-out paths.
#[must_use]
pub fn judge_paths(
    touched: &[String],
    policy: &crate::tools::policy::Policy,
    frozen: &[String],
    held_out: &[String],
) -> Vec<String> {
    touched
        .iter()
        .filter(|p| {
            policy.protected().write_class(p).is_some()
                || frozen.contains(p)
                || held_out.contains(p)
        })
        .cloned()
        .collect()
}

/// Applies `diff` in `wt_root` after `git apply --check`. `Err`: the
/// refusal with git's rejected hunks.
pub fn apply(wt_root: &Path, tmp: &Path, diff: &str) -> Result<(), CandidateRefusal> {
    let refuse = |paths: Vec<String>| CandidateRefusal {
        code: "BUG-CANDIDATE-APPLY".to_owned(),
        paths,
    };
    let _ = std::fs::create_dir_all(tmp);
    let file = tmp.join(format!("candidate-{}.patch", std::process::id()));
    std::fs::write(&file, diff).map_err(|e| refuse(vec![e.to_string()]))?;
    let path = file.display().to_string();
    let git = crate::git::Git::at(wt_root);
    let result = git
        .run(&["apply", "--check", &path])
        .and_then(|_| git.run(&["apply", &path]));
    let _ = std::fs::remove_file(&file);
    result.map(|_| ()).map_err(|e| {
        refuse(
            e.0.lines()
                .filter(|l| l.contains("error") || l.contains("patch failed"))
                .map(str::to_owned)
                .collect(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_names_its_paths() {
        assert_eq!(parse("pr:12"), Some(Spec::Pr(12)));
        assert_eq!(
            parse("fix.patch"),
            Some(Spec::Patch("fix.patch".to_owned()))
        );
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-x\n+y\ndiff --git a/tests/new.rs b/tests/new.rs\nnew file mode 100644\n--- /dev/null\n+++ b/tests/new.rs\n@@ -0,0 +1 @@\n+t\n";
        assert_eq!(
            touched(dir.path(), diff).unwrap_or_else(|e| panic!("{e:?}")),
            ["src/a.rs", "tests/new.rs"]
        );
        let r = record(&Spec::Pr(3), diff, Some("abc".to_owned()));
        assert_eq!(r.number, Some(3));
        assert!(r.digest.starts_with("sha256:"));
    }

    #[test]
    fn hunks_without_plus_minus_lines_still_name_their_paths() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let d = dir.path();
        let paths = |diff: &str| touched(d, diff);
        // A rename at 100% similarity: both sides, no `---`/`+++` lines.
        let rename = "diff --git a/tests/repro.rs b/src/gone.rs\nsimilarity index 100%\nrename from tests/repro.rs\nrename to src/gone.rs\n";
        assert_eq!(
            paths(rename).unwrap_or_else(|e| panic!("{e:?}")),
            ["src/gone.rs", "tests/repro.rs"]
        );
        // A mode-only change.
        let mode =
            "diff --git a/speccoder.toml b/speccoder.toml\nold mode 100644\nnew mode 100755\n";
        assert_eq!(
            paths(mode).unwrap_or_else(|e| panic!("{e:?}")),
            ["speccoder.toml"]
        );
        // A quoted path.
        let quoted = "diff --git \"a/tests/sp\\303\\244ce.rs\" \"b/tests/sp\\303\\244ce.rs\"\n--- \"a/tests/sp\\303\\244ce.rs\"\n+++ \"b/tests/sp\\303\\244ce.rs\"\n@@ -1 +1 @@\n-x\n+y\n";
        assert_eq!(
            paths(quoted).unwrap_or_else(|e| panic!("{e:?}")),
            ["tests/sp\u{e4}ce.rs"]
        );
        // A binary hunk and a symbolic link are refused outright.
        let binary = "diff --git a/b.bin b/b.bin\nindex 88768ef..3e3315e 100644\nGIT binary patch\nliteral 5\nMcmZQzO3KUw00MIXJOBUy\n\nliteral 5\nMcmZQzOv=my00M6TI{*Lx\n\n";
        assert!(paths(binary).is_err_and(|r| r.code == "BUG-CANDIDATE-APPLY"));
        let link = "diff --git a/link b/link\nnew file mode 120000\nindex 0000000..8d14cbf\n--- /dev/null\n+++ b/link\n@@ -0,0 +1 @@\n+speccoder.toml\n\\ No newline at end of file\n";
        assert!(paths(link).is_err_and(|r| r.code == "BUG-CANDIDATE-APPLY"));
        assert!(paths("not a patch\n").is_err());
    }
}
