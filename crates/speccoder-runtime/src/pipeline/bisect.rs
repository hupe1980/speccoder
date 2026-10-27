//! Which tick made already-passing acceptance tests pass (FR-014): a fact
//! found by running the tests, never a guess from diffs.
//!
//! The feature branch's tick commits are searched by bisection — at most
//! five runs, each with the acceptance files overlaid on the commit's tree
//! in a scratch worktree under the state directory, sandboxed, under the
//! acceptance command's deadline. A run that does not build or times out
//! counts as "not passing there"; evidence that is not monotonic, or a
//! budget that runs out, answers "could not be determined". No model call;
//! the verdict is never changed.

use std::path::{Path, PathBuf};

use speccoder_core::regression::ImplementedBy;
use speccoder_core::verify::{CheckStatus, RedClass, classify_red};

use crate::git::Git;
use crate::sandbox;
use crate::verify::process::{CommandRunner, Invocation, OUTPUT_CAP, scrubbed_env};

/// What the search needs.
pub struct Search<'a> {
    /// The feature worktree (its `HEAD` is the branch before this tick).
    pub repo: &'a Path,
    /// The spec project's directory inside it (`""` at the root).
    pub prefix: &'a str,
    /// The feature: its ticks carry `Spec-Task: <feature>/…`.
    pub feature: &'a str,
    /// The acceptance files (project-relative) and their contents.
    pub files: &'a [(String, String)],
    /// The acceptance command.
    pub argv: &'a [String],
    /// Where scratch worktrees go (`<state>/tmp`).
    pub scratch: &'a Path,
    /// The sandbox configuration the runs are confined by.
    pub sandbox: &'a sandbox::Config,
    /// The deadline of one run.
    pub timeout: std::time::Duration,
}

/// The search's answer and the runs it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The implementing tick, `before_feature`, or `None`.
    pub implemented_by: Option<ImplementedBy>,
    /// Acceptance runs made.
    pub runs: u32,
}

/// The scratch worktrees' name prefix (pruned at run start).
const PREFIX: &str = "bisect-";

/// The run budget for `n` ticks: `min(5, ceil(log2(n + 1)) + 1)`.
#[must_use]
pub fn budget(n: usize) -> u32 {
    let mut bits = 0u32;
    while (1usize << bits) < n + 1 {
        bits += 1;
    }
    (bits + 1).min(5)
}

/// The feature's tick commits on the branch at `repo`'s `HEAD`, oldest
/// first (first-parent), and the commit before the first of them.
fn ticks(repo: &Path, feature: &str) -> (Vec<(String, String)>, Option<String>) {
    let log = Git::at(repo)
        .run(&[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H%x1f%(trailers:key=Spec-Task,valueonly)%x1e",
            "HEAD",
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let ticks: Vec<(String, String)> = log
        .split('\u{1e}')
        .filter_map(|r| {
            let (commit, task) = r.trim().split_once('\u{1f}')?;
            let task = task.trim().strip_prefix(&format!("{feature}/"))?;
            Some((commit.to_owned(), task.to_owned()))
        })
        .collect();
    let base = ticks.first().and_then(|(c, _)| {
        Git::at(repo)
            .line(&["rev-parse", "--verify", "-q", &format!("{c}^")])
            .ok()
            .map(|l| l.trim().to_owned())
    });
    (ticks, base)
}

/// Whether the acceptance tests pass at `commit`: its tree in a scratch
/// worktree with the acceptance files overlaid, the command sandboxed there.
fn passes_at(s: &Search<'_>, commit: &str, n: u32) -> bool {
    let dir = s
        .scratch
        .join(format!("{PREFIX}{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(s.scratch);
    let git = Git::at(s.repo);
    let Some(path) = dir.to_str() else {
        return false;
    };
    if git
        .run(&["worktree", "add", "--detach", "-q", path, commit])
        .is_err()
    {
        return false;
    }
    let project = if s.prefix.is_empty() {
        dir.clone()
    } else {
        dir.join(s.prefix)
    };
    let mut ok = true;
    for (rel, content) in s.files {
        let f = project.join(rel);
        if f.parent()
            .is_some_and(|p| std::fs::create_dir_all(p).is_err())
            || std::fs::write(&f, content).is_err()
        {
            ok = false;
        }
    }
    let passed = ok
        && sandbox::runner(&dir, s.sandbox).is_ok_and(|(runner, _)| {
            let out = runner.run(&Invocation {
                argv: s.argv.to_vec(),
                cwd: project.clone(),
                env: scrubbed_env(std::env::vars()),
                timeout: s.timeout,
                output_cap: OUTPUT_CAP,
            });
            let status = if out.spawn_error.is_some() {
                CheckStatus::Error
            } else if out.timed_out {
                CheckStatus::TimedOut
            } else if out.exit_code == Some(0) {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            };
            let summary = crate::verify::summary::parse(
                &String::from_utf8_lossy(&out.stdout.kept),
                &String::from_utf8_lossy(&out.stderr.kept),
                out.exit_code,
            );
            let result = speccoder_core::verify::CheckResult {
                name: "acceptance".to_owned(),
                argv: s.argv.to_vec(),
                status,
                exit_code: out.exit_code,
                duration_ms: 0,
                junit: false,
                tests: None,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                truncated: false,
                note: None,
                flaky: false,
            };
            classify_red(&result, summary.as_ref()) == RedClass::Passed
        });
    let _ = git.run(&["worktree", "remove", "--force", path]);
    let _ = std::fs::remove_dir_all(&dir);
    passed
}

/// Searches the feature's ticks for the first one at which the acceptance
/// tests pass (they pass on the branch's tip: the red run said so).
#[must_use]
pub fn search(s: &Search<'_>) -> Found {
    let (ticks, base) = ticks(s.repo, s.feature);
    search_with(&ticks, base.as_deref(), &mut |c, n| passes_at(s, c, n))
}

/// [`search`] over `ticks` (oldest first) with `probe(commit, n)` telling
/// whether the tests pass at a commit.
pub fn search_with(
    ticks: &[(String, String)],
    base: Option<&str>,
    probe: &mut dyn FnMut(&str, u32) -> bool,
) -> Found {
    let short = |c: &str| c.chars().take(7).collect::<String>();
    if ticks.is_empty() {
        // Nothing ticked yet: the branch's tip is the feature's base.
        return Found {
            implemented_by: Some(ImplementedBy::BeforeFeature {
                before_feature: true,
            }),
            runs: 0,
        };
    }
    let budget = budget(ticks.len());
    let mut runs = 0u32;
    let mut seen: Vec<(usize, bool)> = Vec::new();
    // The tests pass at the last tick (the tip); find the first that passes.
    let (mut lo, mut hi) = (0usize, ticks.len() - 1);
    while lo < hi {
        if runs >= budget {
            return Found {
                implemented_by: None,
                runs,
            };
        }
        let mid = lo + (hi - lo) / 2;
        runs += 1;
        let ok = probe(&ticks[mid].0, runs);
        seen.push((mid, ok));
        if ok {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    // The tick found must itself be seen passing: the tip was only assumed
    // to (the red run passed there); a scratch run that disagrees is
    // evidence that is not monotonic — the answer is unknown.
    if !seen.contains(&(lo, true)) {
        if runs >= budget {
            return Found {
                implemented_by: None,
                runs,
            };
        }
        runs += 1;
        if !probe(&ticks[lo].0, runs) {
            return Found {
                implemented_by: None,
                runs,
            };
        }
    }
    if lo == 0 {
        // The first tick passes: did the tests pass before the feature?
        let Some(base) = base.filter(|_| runs < budget) else {
            return Found {
                implemented_by: None,
                runs,
            };
        };
        runs += 1;
        if probe(base, runs) {
            return Found {
                implemented_by: Some(ImplementedBy::BeforeFeature {
                    before_feature: true,
                }),
                runs,
            };
        }
    }
    let (commit, task) = &ticks[lo];
    Found {
        implemented_by: Some(ImplementedBy::Task {
            task: task.clone(),
            commit: short(commit),
        }),
        runs,
    }
}

/// Removes scratch worktrees a crashed search left behind.
pub fn prune(repo: &Path, scratch: &Path) {
    let leftover: Vec<PathBuf> = std::fs::read_dir(scratch)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(PREFIX))
        .map(|e| e.path())
        .collect();
    for d in &leftover {
        let _ = std::fs::remove_dir_all(d);
    }
    if !leftover.is_empty() {
        let _ = Git::at(repo).run(&["worktree", "prune"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(n: usize) -> Vec<(String, String)> {
        (1..=n)
            .map(|i| (format!("{i:07}abcdef"), format!("T{i:03}")))
            .collect()
    }

    #[test]
    fn the_budget_grows_with_the_log_and_stops_at_five() {
        assert_eq!(budget(1), 2);
        assert_eq!(budget(3), 3);
        assert_eq!(budget(6), 4);
        assert_eq!(budget(14), 5);
        assert_eq!(budget(1000), 5);
    }

    #[test]
    fn the_first_passing_tick_is_found_within_budget() {
        let t = ticks(6);
        // Passing from T003 on.
        let mut calls = 0;
        let found = search_with(&t, Some("base"), &mut |c, _| {
            calls += 1;
            c >= t[2].0.as_str()
        });
        assert_eq!(
            found.implemented_by,
            Some(ImplementedBy::Task {
                task: "T003".to_owned(),
                commit: "0000003".to_owned()
            })
        );
        assert!(found.runs <= budget(6) && calls == found.runs as usize);
    }

    #[test]
    fn passing_at_the_base_is_before_the_feature_and_no_ticks_is_too() {
        let t = ticks(6);
        let found = search_with(&t, Some("base"), &mut |_, _| true);
        assert_eq!(
            found.implemented_by,
            Some(ImplementedBy::BeforeFeature {
                before_feature: true
            })
        );
        assert!(found.runs <= 5);
        assert_eq!(search_with(&[], None, &mut |_, _| panic!("no run")).runs, 0);
    }

    #[test]
    fn a_failing_base_names_the_first_tick() {
        let t = ticks(2);
        let found = search_with(&t, Some("base"), &mut |c, _| c != "base");
        assert_eq!(
            found.implemented_by,
            Some(ImplementedBy::Task {
                task: "T001".to_owned(),
                commit: "0000001".to_owned()
            })
        );
    }

    #[test]
    fn a_tip_that_does_not_pass_where_red_said_it_did_is_unknown() {
        let t = ticks(6);
        let found = search_with(&t, Some("base"), &mut |_, _| false);
        assert_eq!(found.implemented_by, None);
        assert!(found.runs <= budget(6));
    }

    #[test]
    fn an_exhausted_budget_or_no_base_cannot_decide() {
        // Every probe fails but the tip passes: the search ends at the
        // last tick; with no base and the first tick passing, unknown.
        let t = ticks(1);
        assert_eq!(search_with(&t, None, &mut |_, _| true).implemented_by, None);
        // Budget: 30 ticks allow 5 runs; a flip-flopping probe ends at the tip.
        let t = ticks(30);
        let found = search_with(&t, Some("base"), &mut |_, n| n % 2 == 0);
        assert!(found.runs <= 5);
    }
}
