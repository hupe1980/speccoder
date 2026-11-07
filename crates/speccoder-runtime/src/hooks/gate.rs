//! Gates at their point: each runs where the checks run, in the same
//! sandbox, and only a clean exit 0 lets the point through. A gate that is
//! not trusted, cannot run sandboxed, crashes, times out or changes a file
//! the tick would commit blocks — never passes. Its words are for the
//! person; nothing here reaches a model.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use speccoder_core::hooks::{GateExit, GateOutcome, GateRecord, GateState, Point};

use super::{HookRun, TrustState};
use crate::sandbox::Runner;
use crate::verify::process::{CommandRunner as _, Invocation, OUTPUT_CAP};

/// The words of a gate that waits for a person's trust.
pub const AWAITS_TRUST: &str = "waits for your trust";
/// The words of a gate whose bytes changed since they were trusted.
pub const CHANGED: &str = "changed since it was trusted";

/// Where a point stands: what a gate is told in `SPECCODER_*`.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The point.
    pub point: Point,
    /// The unit.
    pub feature: &'a str,
    /// The task, at a tick.
    pub task: Option<&'a str>,
    /// The branch to push, at a pull request.
    pub branch: Option<&'a str>,
}

impl Facts<'_> {
    fn what(&self) -> String {
        match (self.point, self.task) {
            (Point::Tick, Some(t)) => format!("tick {t}"),
            (Point::Tick, None) => "tick".to_owned(),
            (Point::Pr, _) => "pr".to_owned(),
        }
    }

    fn env(&self) -> Vec<(String, String)> {
        let mut v = vec![
            ("SPECCODER_POINT".to_owned(), self.point.as_str().to_owned()),
            ("SPECCODER_FEATURE".to_owned(), self.feature.to_owned()),
        ];
        if let Some(t) = self.task {
            v.push(("SPECCODER_TASK".to_owned(), t.to_owned()));
        }
        if let Some(b) = self.branch {
            v.push(("SPECCODER_BRANCH".to_owned(), b.to_owned()));
        }
        v
    }
}

/// Runs the gates of `facts.point` declared in the configuration of the
/// project at `worktree` (the project's folder in the worktree, where its
/// checks run),
/// in name order, through `runner` (the checks' runner); trust and runs are
/// read from speccoder's state directory `state`. The first block stops the
/// point; the gates after it are `not_run`. No gates: empty.
#[must_use]
pub fn run(worktree: &Path, runner: &Runner, state: &Path, facts: Facts<'_>) -> Vec<GateRecord> {
    let hooks = match super::load(worktree) {
        Ok(h) => h,
        // A configuration no gate can be read from: the point does not pass.
        Err(e) => {
            return vec![GateRecord {
                name: "hooks".to_owned(),
                point: facts.point,
                outcome: GateState::Blocked,
                reason: Some(speccoder_core::hooks::clean(&e)),
                code: Some("HOOK-MISCONFIGURED".to_owned()),
                duration_ms: 0,
            }];
        }
    };
    let gates: Vec<_> = hooks
        .iter()
        .filter(|h| h.at() == Some(facts.point))
        .collect();
    if gates.is_empty() {
        return Vec::new();
    }
    let dir = super::dir(state, worktree);
    let (runner, _) = runner.clone().carve(&[], &hidden_state(state, worktree));
    let mut out = Vec::new();
    for def in gates {
        if out
            .iter()
            .any(|g: &GateRecord| g.outcome == GateState::Blocked)
        {
            out.push(GateRecord {
                name: def.name.clone(),
                point: facts.point,
                outcome: GateState::NotRun,
                reason: None,
                code: None,
                duration_ms: 0,
            });
            super::record_run(&dir, &def.name, run_record(&facts, "not_run", 0, None));
            continue;
        }
        out.push(judge(def, &runner, worktree, &dir, &facts));
    }
    out
}

/// One gate at its point: trusted and sandboxed, it runs; else it blocks.
/// The run is recorded in its recent runs.
fn judge(
    def: &speccoder_core::hooks::HookDef,
    runner: &Runner,
    worktree: &Path,
    dir: &Path,
    facts: &Facts<'_>,
) -> GateRecord {
    let digested = super::digest(def, worktree);
    let trust = super::trust::state(dir, &def.name, &digested.digest);
    let began = Instant::now();
    let (outcome, code) = if trust == TrustState::Trusted {
        // Only where the sandbox enforces what it hides: speccoder's state
        // and the held-out tests stay out of a gate's reach.
        let ended = if runner.hides() {
            run_one(def, runner, worktree, facts)
        } else {
            GateOutcome::Blocked(
                "no sandbox is enforced here that hides speccoder's state, and a gate runs only in one"
                    .to_owned(),
            )
        };
        (ended, "HOOK-BLOCKED")
    } else {
        let why = if trust == TrustState::Changed {
            "changed since it was trusted: trust it again (speccoder hooks trust)"
        } else {
            "waits for your trust (speccoder hooks trust)"
        };
        (GateOutcome::Blocked(why.to_owned()), "HOOK-UNTRUSTED")
    };
    let duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    let record = match outcome {
        GateOutcome::Passed => GateRecord {
            name: def.name.clone(),
            point: facts.point,
            outcome: GateState::Passed,
            reason: None,
            code: None,
            duration_ms,
        },
        GateOutcome::Blocked(reason) => GateRecord {
            name: def.name.clone(),
            point: facts.point,
            outcome: GateState::Blocked,
            reason: Some(reason),
            code: Some(code.to_owned()),
            duration_ms,
        },
    };
    super::record_run(
        dir,
        &def.name,
        run_record(
            facts,
            record.outcome.as_str(),
            duration_ms,
            record.reason.clone(),
        ),
    );
    record
}

/// `speccoder hooks test`: the gate `def` alone, on `root`, as it would
/// stand at its point.
#[must_use]
pub fn test(def: &speccoder_core::hooks::HookDef, root: &Path, state: &Path) -> GateRecord {
    let point = def.at().unwrap_or(Point::Tick);
    let facts = Facts {
        point,
        feature: "TEST",
        task: (point == Point::Tick).then_some("TEST"),
        branch: (point == Point::Pr).then_some("TEST"),
    };
    let runner = match crate::sandbox::runner_at(root) {
        Ok((r, _)) => r.carve(&[], &hidden_state(state, root)).0,
        Err(e) => {
            return GateRecord {
                name: def.name.clone(),
                point,
                outcome: GateState::Blocked,
                reason: Some(speccoder_core::hooks::clean(&e)),
                code: Some("HOOK-BLOCKED".to_owned()),
                duration_ms: 0,
            };
        }
    };
    judge(def, &runner, root, &super::dir(state, root), &facts)
}

/// speccoder's state a gate may not read: other projects', and this one's
/// journal, trust, approvals and held-out tests — everything but the
/// worktrees (the gate runs in one) and the scratch directory.
pub(super) fn hidden_state(state: &Path, worktree: &Path) -> Vec<std::path::PathBuf> {
    let project = crate::worktree::project_state_dir(state, worktree);
    let entries = |d: &Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect()
    };
    let mut hidden: Vec<_> = entries(state)
        .into_iter()
        .filter(|p| *p != project)
        .collect();
    hidden.extend(entries(&project).into_iter().filter(|p| {
        !matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("worktrees" | "tmp")
        )
    }));
    hidden
}

fn run_record(
    facts: &Facts<'_>,
    outcome: &str,
    duration_ms: u64,
    error: Option<String>,
) -> HookRun {
    HookRun {
        at: crate::attest::now_rfc3339(),
        what: facts.what(),
        outcome: outcome.to_owned(),
        duration_ms,
        error,
    }
}

fn run_one(
    def: &speccoder_core::hooks::HookDef,
    runner: &Runner,
    worktree: &Path,
    facts: &Facts<'_>,
) -> GateOutcome {
    let Some(argv) = def.argv() else {
        return GateOutcome::Blocked("a gate runs `argv`".to_owned());
    };
    let network = matches!(
        def.role,
        speccoder_core::hooks::Role::Gate { network: true, .. }
    );
    let mut env = crate::verify::process::scrubbed_env(std::env::vars());
    // The names the hook asks for, even secret-looking ones: the person put
    // them there.
    for name in &def.env {
        if let Ok(v) = std::env::var(name) {
            env.retain(|(k, _)| k != name);
            env.push((name.clone(), v));
        }
    }
    env.extend(facts.env());
    let inv = Invocation {
        argv: argv.to_vec(),
        cwd: worktree.to_path_buf(),
        env,
        timeout: Duration::from_secs(def.timeout),
        output_cap: OUTPUT_CAP,
    };
    let Some(before) = dirty(worktree) else {
        return GateOutcome::Blocked("git could not say what the tick would commit".to_owned());
    };
    let ended = if network {
        runner.run_with(&inv, true)
    } else {
        runner.run(&inv)
    };
    let Some(after) = dirty(worktree) else {
        return GateOutcome::Blocked("git could not say what the tick would commit".to_owned());
    };
    let mut changed: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|p| before.get(*p) != after.get(*p))
        .cloned()
        .collect();
    changed.sort();
    changed.dedup();
    let stdout = String::from_utf8_lossy(&ended.stdout.kept);
    let stderr = String::from_utf8_lossy(&ended.stderr.kept);
    GateOutcome::of(&GateExit {
        exit_code: ended.exit_code,
        timed_out: ended.timed_out,
        spawn_error: ended.spawn_error.as_deref(),
        stdout: &stdout,
        stderr: &stderr,
        changed: &changed,
        timeout: def.timeout,
    })
}

/// Every file git would commit that differs from `HEAD` (tracked changes
/// and untracked files git does not ignore), with its content's hash.
fn dirty(worktree: &Path) -> Option<BTreeMap<String, String>> {
    let git = crate::git::Git::at(worktree);
    let top = PathBuf::from(git.line(&["rev-parse", "--show-toplevel"]).ok()?.trim());
    let listing = git.run(&["status", "--porcelain=v1", "-z", "-uall"]).ok()?;
    let listing = String::from_utf8_lossy(&listing);
    let mut out = BTreeMap::new();
    let mut entries = listing.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = entries.next() {
        if e.len() < 4 {
            continue;
        }
        let (status, path) = e.split_at(3);
        // A rename lists its source next; both count.
        if status.starts_with('R') || status.starts_with('C') {
            let _ = entries.next();
        }
        let hash = std::fs::read(top.join(path)).map_or_else(
            |_| "deleted".to_owned(),
            |bytes| {
                use sha2::{Digest as _, Sha256};
                Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            },
        );
        out.insert(path.to_owned(), hash);
    }
    Some(out)
}
