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

/// The paths a unified diff touches (both sides, `/dev/null` aside).
#[must_use]
pub fn touched(diff: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in diff.lines() {
        let p = l
            .strip_prefix("+++ ")
            .or_else(|| l.strip_prefix("--- "))
            .map(|p| p.split('\t').next().unwrap_or(p).trim())
            .filter(|p| *p != "/dev/null")
            .map(|p| {
                p.strip_prefix("a/")
                    .or_else(|| p.strip_prefix("b/"))
                    .unwrap_or(p)
                    .to_owned()
            });
        if let Some(p) = p
            && !out.contains(&p)
        {
            out.push(p);
        }
    }
    out
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
        let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-x\n+y\n--- /dev/null\n+++ b/tests/new.rs\n";
        assert_eq!(touched(diff), ["src/a.rs", "tests/new.rs"]);
        let r = record(&Spec::Pr(3), diff, Some("abc".to_owned()));
        assert_eq!(r.number, Some(3));
        assert!(r.digest.starts_with("sha256:"));
    }
}
