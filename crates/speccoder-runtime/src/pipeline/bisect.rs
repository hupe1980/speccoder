//! Which tick made already-passing acceptance tests pass: a fact
//! found by running the tests, never a guess from diffs.
//!
//! The feature branch's tick commits are searched by bisection — at most
//! five runs, each with the acceptance files overlaid on the commit's tree
//! in a scratch worktree of its own under the state directory, sandboxed,
//! under the checks' deadline. A run that builds and fails says "not
//! passing there"; anything that cannot say — a worktree that cannot be
//! made, a run that times out or prints nothing readable — makes the
//! answer "could not be determined", never a task's name. For a section
//! added to a story's file only that section's tests run. No model call;
//! the verdict is never changed.

use std::path::{Path, PathBuf};

use speccoder_core::collection::Toolchain;
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
    /// The task searched for (names its scratch directory).
    pub task: &'a str,
    /// The acceptance files (project-relative) and their contents.
    pub files: &'a [(String, String)],
    /// The command that runs only the task's own tests (a section's,
    /// when the story's file was appended to).
    pub argv: &'a [String],
    /// Where scratch worktrees go (`<state>/tmp`).
    pub scratch: &'a Path,
    /// The sandbox configuration the runs are confined by.
    pub sandbox: &'a sandbox::Config,
    /// The deadline of one run (the checks').
    pub timeout: std::time::Duration,
    /// `[worktree]`: each scratch tree gets its copied files and dependency
    /// folders as the feature's worktree does.
    pub worktree: &'a crate::prepare::Config,
    /// The project's directory in the person's checkout (what `include` copies from).
    pub checkout: &'a Path,
    /// speccoder's state directory (the cache `speccoder new` filled).
    pub state: Option<&'a Path>,
}

/// The search's answer and the runs it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The implementing tick, `before_feature`, or `None`.
    pub implemented_by: Option<ImplementedBy>,
    /// Acceptance runs made.
    pub runs: u32,
}

/// What one run at a commit said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// The tests passed.
    Pass,
    /// They built and failed (an assertion, a build error, an absent symbol).
    Fail,
    /// Nothing can be said (no worktree, no sandbox, a timeout, output not read).
    Undetermined,
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

/// The command that runs only the tests `added` declares: `argv` with the
/// runner's own filter (`None`: no filter is known for the runner — the
/// search then cannot run only the section).
#[must_use]
pub fn section_command(argv: &[String], path: &str, added: &str) -> Option<Vec<String>> {
    let t = speccoder_core::collection::toolchain_of(path)?;
    let code = speccoder_core::collection::strip_comments(added);
    let after = |kw: &str| -> Vec<String> {
        code.lines()
            .filter_map(|l| {
                let l = l.trim_start();
                let rest = l
                    .strip_prefix("pub ")
                    .unwrap_or(l)
                    .strip_prefix("async ")
                    .unwrap_or_else(|| l.strip_prefix("pub ").unwrap_or(l));
                let rest = rest.strip_prefix(kw)?;
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (!name.is_empty()).then_some(name)
            })
            .collect()
    };
    let mut out = argv.to_vec();
    match t {
        Toolchain::Rust => {
            let names = after("fn ");
            if names.is_empty() {
                return None;
            }
            if !out.iter().any(|a| a == "--") {
                out.push("--".to_owned());
            }
            out.extend(names);
            out.push("--exact".to_owned());
        }
        Toolchain::Python => {
            let names: Vec<String> = after("def ")
                .into_iter()
                .filter(|n| n.starts_with("test"))
                .collect();
            if names.is_empty() {
                return None;
            }
            out.extend(["-k".to_owned(), names.join(" or ")]);
        }
        Toolchain::Go => {
            let names = speccoder_core::collection::go_declared_tests(added);
            if names.is_empty() {
                return None;
            }
            out.extend(["-run".to_owned(), format!("^({})$", names.join("|"))]);
        }
        // A test's title is free text: no filter speccoder can trust.
        Toolchain::JavaScript => return None,
    }
    Some(out)
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

/// One run at `commit`: its tree in a scratch worktree of this search's
/// own, the acceptance files overlaid, the command sandboxed there.
fn probe_at(s: &Search<'_>, commit: &str) -> Probe {
    if std::fs::create_dir_all(s.scratch).is_err() {
        return Probe::Undetermined;
    }
    let safe: String = s.task.chars().filter(char::is_ascii_alphanumeric).collect();
    let Ok(holder) = tempfile::Builder::new()
        .prefix(&format!("{PREFIX}{}-{safe}-", std::process::id()))
        .tempdir_in(s.scratch)
    else {
        return Probe::Undetermined;
    };
    let dir = holder.path().join("wt");
    let Some(path) = dir.to_str() else {
        return Probe::Undetermined;
    };
    let git = Git::at(s.repo);
    // Worktrees of one repository are added and removed one at a time.
    let added = {
        let _setup = super::lanes::LANE_SETUP.lock();
        git.run(&["worktree", "add", "--detach", "-q", path, commit])
            .is_ok()
    };
    if !added {
        return Probe::Undetermined;
    }
    let project = if s.prefix.is_empty() {
        dir.clone()
    } else {
        dir.join(s.prefix)
    };
    let overlaid = s.files.iter().all(|(rel, content)| {
        let f = project.join(rel);
        f.parent()
            .is_none_or(|p| std::fs::create_dir_all(p).is_ok())
            && std::fs::write(&f, content).is_ok()
    });
    let probe = if !overlaid {
        Probe::Undetermined
    } else {
        match sandbox::runner(&dir, s.sandbox).map(|(r, _)| prepared(s, &dir, &project, r)) {
            Err(_) | Ok(None) => Probe::Undetermined,
            Ok(Some(runner)) => {
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
                match (status, classify_red(&result, summary.as_ref())) {
                    (CheckStatus::Error | CheckStatus::TimedOut, _) => Probe::Undetermined,
                    (_, RedClass::Passed) => Probe::Pass,
                    (_, RedClass::Assertion | RedClass::Absent | RedClass::BuildError) => {
                        Probe::Fail
                    }
                    _ => Probe::Undetermined,
                }
            }
        }
    };
    crate::prepare::forget(&dir);
    crate::include::forget(&dir);
    {
        let _setup = super::lanes::LANE_SETUP.lock();
        let _ = git.run(&["worktree", "remove", "--force", path]);
    }
    drop(holder);
    probe
}

/// `runner` for a scratch tree prepared like the feature's worktree (the
/// files `[worktree] include` names copied, its dependency folders
/// installed); `None` when that failed — then the run could say nothing.
fn prepared(
    s: &Search<'_>,
    dir: &Path,
    project: &Path,
    runner: sandbox::Runner,
) -> Option<sandbox::Runner> {
    crate::include::copy(dir, project, s.checkout, &s.worktree.include).ok()?;
    crate::seed::seed(s.repo, &s.repo.join(s.prefix), dir, project);
    if !s.worktree.prepare || !crate::prepare::needed(project) {
        return Some(runner);
    }
    crate::prepare::prepare(&crate::prepare::Prepare {
        worktree: dir,
        project,
        config: s.worktree,
        runner: &runner,
        state: s.state,
    })
    .ok()?;
    Some(runner)
}

/// Searches the feature's ticks for the first one at which the acceptance
/// tests pass (they pass on the branch's tip: the red run said so).
#[must_use]
pub fn search(s: &Search<'_>) -> Found {
    let (ticks, base) = ticks(s.repo, s.feature);
    search_with(&ticks, base.as_deref(), &mut |c, _| probe_at(s, c))
}

/// [`search`] over `ticks` (oldest first) with `probe(commit, n)` telling
/// what the tests did at a commit. Any undetermined probe ends the search
/// with no answer.
pub fn search_with(
    ticks: &[(String, String)],
    base: Option<&str>,
    probe: &mut dyn FnMut(&str, u32) -> Probe,
) -> Found {
    let short = |c: &str| c.chars().take(7).collect::<String>();
    let none = |runs| Found {
        implemented_by: None,
        runs,
    };
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
            return none(runs);
        }
        let mid = lo + (hi - lo) / 2;
        runs += 1;
        let ok = match probe(&ticks[mid].0, runs) {
            Probe::Pass => true,
            Probe::Fail => false,
            Probe::Undetermined => return none(runs),
        };
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
            return none(runs);
        }
        runs += 1;
        if probe(&ticks[lo].0, runs) != Probe::Pass {
            return none(runs);
        }
    }
    if lo == 0 {
        // The first tick passes: did the tests pass before the feature?
        let Some(base) = base.filter(|_| runs < budget) else {
            return none(runs);
        };
        runs += 1;
        match probe(base, runs) {
            Probe::Pass => {
                return Found {
                    implemented_by: Some(ImplementedBy::BeforeFeature {
                        before_feature: true,
                    }),
                    runs,
                };
            }
            Probe::Fail => {}
            Probe::Undetermined => return none(runs),
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

/// Whether a process with `pid` still runs.
fn alive(pid: u32) -> bool {
    i32::try_from(pid).is_ok_and(|p| {
        // Signal 0: delivered to nobody, refused only for a pid that is gone.
        !matches!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(p), None),
            Err(nix::errno::Errno::ESRCH)
        )
    })
}

/// Removes the scratch worktrees a crashed search left behind: only those
/// whose process is gone, each by its own `git worktree remove` — never
/// another live search's, never the repository's other worktrees.
pub fn prune(repo: &Path, scratch: &Path) {
    let leftover: Vec<PathBuf> = std::fs::read_dir(scratch)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let pid: u32 = name.strip_prefix(PREFIX)?.split('-').next()?.parse().ok()?;
            (!alive(pid)).then(|| e.path())
        })
        .collect();
    for d in &leftover {
        let wt = d.join("wt");
        crate::prepare::forget(&wt);
        crate::include::forget(&wt);
        if let Some(p) = wt.to_str() {
            let _setup = super::lanes::LANE_SETUP.lock();
            let _ = Git::at(repo).run(&["worktree", "remove", "--force", p]);
        }
        let _ = std::fs::remove_dir_all(d);
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

    fn of(pass: bool) -> Probe {
        if pass { Probe::Pass } else { Probe::Fail }
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
        let mut calls = 0;
        let found = search_with(&t, Some("base"), &mut |c, _| {
            calls += 1;
            of(c >= t[2].0.as_str())
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
        let found = search_with(&t, Some("base"), &mut |_, _| Probe::Pass);
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
        let found = search_with(&t, Some("base"), &mut |c, _| of(c != "base"));
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
        let found = search_with(&t, Some("base"), &mut |_, _| Probe::Fail);
        assert_eq!(found.implemented_by, None);
        assert!(found.runs <= budget(6));
    }

    /// H4: a probe that could not run never moves the search to a later task.
    #[test]
    fn an_undetermined_probe_ends_the_search_with_no_name() {
        let t = ticks(6);
        // Passing from T002, but the first probe's worktree could not be made.
        let found = search_with(&t, Some("base"), &mut |c, n| {
            if n == 1 {
                Probe::Undetermined
            } else {
                of(c >= t[1].0.as_str())
            }
        });
        assert_eq!(found.implemented_by, None);
        let found = search_with(&ticks(1), Some("base"), &mut |c, _| {
            if c == "base" {
                Probe::Undetermined
            } else {
                Probe::Pass
            }
        });
        assert_eq!(
            found.implemented_by, None,
            "an unreadable base is not 'implemented by T001'"
        );
    }

    #[test]
    fn an_exhausted_budget_or_no_base_cannot_decide() {
        let t = ticks(1);
        assert_eq!(
            search_with(&t, None, &mut |_, _| Probe::Pass).implemented_by,
            None
        );
        let t = ticks(30);
        let found = search_with(&t, Some("base"), &mut |_, n| of(n % 2 == 0));
        assert!(found.runs <= 5);
    }

    /// H6: a section of the story's file runs alone.
    #[test]
    fn a_section_runs_only_its_own_tests() {
        let argv: Vec<String> = ["cargo", "test", "--test", "acceptance_us1"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            section_command(
                &argv,
                "tests/acceptance_us1.rs",
                "// T006\n#[test]\nfn t006_a() {}\n#[test]\nasync fn t006_b() {}\n"
            )
            .unwrap(),
            [
                "cargo",
                "test",
                "--test",
                "acceptance_us1",
                "--",
                "t006_a",
                "t006_b",
                "--exact"
            ]
        );
        let py: Vec<String> = ["python3", "-m", "pytest", "tests/test_acceptance_us1.py"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            section_command(
                &py,
                "tests/test_acceptance_us1.py",
                "def test_t006_a():\n    pass\n"
            )
            .unwrap()
            .last()
            .unwrap(),
            "test_t006_a"
        );
        assert_eq!(
            section_command(&argv, "src/acceptance-us1.test.ts", "it('a', () => {})"),
            None
        );
    }

    /// H5: only a gone process's scratch is pruned.
    #[test]
    fn prune_leaves_a_live_search_s_scratch() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let live = d
            .path()
            .join(format!("{PREFIX}{}-T005-x", std::process::id()));
        let dead = d.path().join(format!("{PREFIX}{}-T006-y", u32::MAX - 7));
        std::fs::create_dir_all(&live).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir_all(&dead).unwrap_or_else(|e| panic!("{e}"));
        prune(d.path(), d.path());
        assert!(live.exists() && !dead.exists());
    }
}
