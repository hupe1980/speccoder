//! The `full` check tier: once, after a run ticks a feature's last task,
//! and recorded so `pr` can refuse a branch whose full checks failed.

use std::path::Path;

use speccoder_core::FullTier;

use crate::git::Git;
use crate::{attempts, sandbox, verify};

/// Where the record of a feature's full tier lives.
pub const PREFIX: &str = "refs/speccoder/full";

/// Runs the `full` checks of `project` (in the worktree `wt_root`), confined
/// like every other command.
#[must_use]
pub fn run_once(project: &Path, wt_root: &Path) -> FullTier {
    let config = verify::find_config(project)
        .and_then(|root| std::fs::read_to_string(root.join(verify::config::FILE)).ok())
        .unwrap_or_default();
    let runner = match sandbox::parse(&config).and_then(|c| sandbox::runner(wt_root, &c)) {
        Ok((r, _)) => r,
        Err(e) => {
            return FullTier {
                note: Some(format!("the full tier could not run: {e}")),
                ..FullTier::default()
            };
        }
    };
    match verify::run_checks(project, Some(verify::config::Tier::Full), &runner) {
        Ok(report) => FullTier {
            ran: report.checks.iter().map(|c| c.name.clone()).collect(),
            passed: report.passed,
            failed: report
                .checks
                .iter()
                .filter(|c| c.status.is_failure())
                .map(|c| c.name.clone())
                .collect(),
            note: None,
        },
        // "No applicable checks": nothing in the full tier to fail.
        Err(e) => FullTier {
            passed: true,
            note: Some(if e.starts_with("no applicable checks") {
                "no full-tier checks declared".to_owned()
            } else {
                e
            }),
            ..FullTier::default()
        },
    }
}

/// Records `tier` for the branch head of `wt_root` under
/// `refs/speccoder/full/<feature>`: a commit on the head's tree, its parent
/// the head, its trailers the outcome.
pub fn record(wt_root: &Path, feature: &str, tier: &FullTier) {
    let git = Git::at(wt_root);
    let (Ok(head), Ok(tree)) = (
        git.line(&["rev-parse", "--verify", "HEAD^{commit}"]),
        git.line(&["rev-parse", "--verify", "HEAD^{tree}"]),
    ) else {
        return;
    };
    let outcome = if tier.passed { "passed" } else { "failed" };
    let message = format!(
        "speccoder: full tier {outcome}\n\nTier: full\nOutcome: {outcome}\nRan: {}\n{}",
        tier.ran.join(", "),
        if tier.failed.is_empty() {
            String::new()
        } else {
            format!("Failed: {}\n", tier.failed.join(", "))
        }
    );
    let mut args = attempts::IDENTITY.to_vec();
    args.extend(["commit-tree", &tree, "-p", &head, "-m", &message]);
    if let Ok(commit) = git.line(&args) {
        let _ = git.run(&["update-ref", &format!("{PREFIX}/{feature}"), &commit]);
    }
}

/// The recorded full tier for `branch` of the repository at `repo_root`, if
/// it was recorded for the branch's current head.
#[must_use]
pub fn read(repo_root: &Path, feature: &str, branch: &str) -> Option<FullTier> {
    let git = Git::at(repo_root);
    let reference = format!("{PREFIX}/{feature}");
    let parent = git
        .line(&["rev-parse", "--verify", &format!("{reference}^1")])
        .ok()?;
    let head = git
        .line(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")])
        .ok()?;
    if parent != head {
        return None;
    }
    let body = git.line(&["log", "-1", "--format=%B", &reference]).ok()?;
    let field = |key: &str| {
        body.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| {
                v.split(", ")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    Some(FullTier {
        ran: field("Ran: "),
        passed: body.contains("\nOutcome: passed"),
        failed: field("Failed: "),
        note: None,
    })
}
