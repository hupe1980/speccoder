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
use crate::verify::process::{Invocation, OUTPUT_CAP};

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
                findings: Vec::new(),
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
                findings: Vec::new(),
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
    let trust = digested.trust(dir);
    let began = Instant::now();
    let mut findings = Left::default();
    let (outcome, code) = if trust == TrustState::Trusted {
        // Only where the sandbox enforces what it hides: speccoder's state
        // and the held-out tests stay out of a gate's reach.
        let ended = if runner.hides() {
            let (outcome, left) = run_one(def, runner, worktree, facts);
            findings = left;
            outcome
        } else {
            GateOutcome::Blocked(
                "no sandbox is enforced here that hides speccoder's state, and a gate runs only in one"
                    .to_owned(),
            )
        };
        (ended, "HOOK-BLOCKED")
    } else {
        let why = match (&def.extension, trust == TrustState::Changed) {
            (Some((ext, _)), true) => format!(
                "{CHANGED}: the extension {ext} changed — trust it again (speccoder extensions trust {ext})"
            ),
            (Some((ext, _)), false) => {
                format!("{AWAITS_TRUST} of the extension {ext} (speccoder extensions trust {ext})")
            }
            (None, true) => format!("{CHANGED}: trust it again (speccoder hooks trust)"),
            (None, false) => format!("{AWAITS_TRUST} (speccoder hooks trust)"),
        };
        (GateOutcome::Blocked(why), "HOOK-UNTRUSTED")
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
            findings: Vec::new(),
        },
        GateOutcome::Blocked(reason) => GateRecord {
            name: def.name.clone(),
            point: facts.point,
            outcome: GateState::Blocked,
            // Findings say more than a checker's last line ("written: …").
            reason: Some(summary_of(&findings.findings).unwrap_or(reason)),
            code: Some(code.to_owned()),
            duration_ms,
            findings: findings.findings.clone(),
        },
    };
    let mut run = run_record(
        facts,
        record.outcome.as_str(),
        duration_ms,
        record.reason.clone(),
    );
    run.findings = findings.findings;
    run.findings_error = findings.error;
    super::record_run(dir, &def.name, run);
    record
}

/// The findings a gate left at `SPECCODER_FINDINGS`, or why they cannot
/// be shown.
#[derive(Debug, Default)]
struct Left {
    findings: Vec<speccoder_core::extensions::SarifFinding>,
    error: Option<String>,
}

/// `speccoder hooks test`: the gate `def` alone, on `root`, as it would
/// stand at its point.
#[must_use]
pub fn test(def: &speccoder_core::hooks::HookDef, root: &Path, state: &Path) -> GateRecord {
    let point = def.at().unwrap_or(Point::Tick);
    // The current feature, so a gate that reads its folder reads a real one.
    let current = crate::format::open(root, None)
        .ok()
        .and_then(|o| o.snapshot.selection.feature);
    let facts = Facts {
        point,
        feature: current.as_deref().unwrap_or("TEST"),
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
                findings: Vec::new(),
            };
        }
    };
    judge(def, &runner, root, &super::dir(state, root), &facts)
}

/// speccoder's state a gate may not read: other projects', and this one's
/// journal, trust, approvals and held-out tests — everything but the
/// worktrees (the gate runs in one) and the scratch directory.
pub(crate) fn hidden_state(state: &Path, worktree: &Path) -> Vec<std::path::PathBuf> {
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
        findings: Vec::new(),
        findings_error: None,
    }
}

fn run_one(
    def: &speccoder_core::hooks::HookDef,
    runner: &Runner,
    worktree: &Path,
    facts: &Facts<'_>,
) -> (GateOutcome, Left) {
    let Some(argv) = def.argv() else {
        return (
            GateOutcome::Blocked("a gate runs `argv`".to_owned()),
            Left::default(),
        );
    };
    // Where it may leave SARIF findings: the sandbox's scratch directory.
    let findings_file = runner.scratch().map(|d| {
        d.join(format!(
            "findings-{}-{}.sarif",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_nanos())
        ))
    });
    let findings_text = findings_file
        .as_ref()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let feature_dir = crate::extensions::feature_dir(worktree, facts.feature)
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = def.extension.as_ref().map(|(_, f)| PathBuf::from(f));
    let argv = crate::extensions::resolve(
        argv,
        folder.as_deref(),
        &[
            ("SPECCODER_FEATURE_DIR", feature_dir.clone()),
            ("SPECCODER_FINDINGS", findings_text.clone()),
        ],
    );
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
    env.push(("SPECCODER_FEATURE_DIR".to_owned(), feature_dir));
    // A Python checker leaves no bytecode beside its scripts.
    env.push(("PYTHONDONTWRITEBYTECODE".to_owned(), "1".to_owned()));
    if !findings_text.is_empty() {
        env.push(("SPECCODER_FINDINGS".to_owned(), findings_text));
    }
    let inv = Invocation {
        argv,
        cwd: worktree.to_path_buf(),
        env,
        timeout: Duration::from_secs(def.timeout),
        output_cap: OUTPUT_CAP,
    };
    let Some(before) = dirty(worktree) else {
        return (
            GateOutcome::Blocked("git could not say what the tick would commit".to_owned()),
            Left::default(),
        );
    };
    // No network unless the gate says `network = true`: not even loopback.
    let ended = if network {
        runner.run_with(&inv, true)
    } else {
        runner.run_offline(&inv)
    };
    let left = findings_file.map_or_else(Left::default, |f| read_findings(&f, worktree));
    let Some(after) = dirty(worktree) else {
        return (
            GateOutcome::Blocked("git could not say what the tick would commit".to_owned()),
            left,
        );
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
    let outcome = GateOutcome::of(&GateExit {
        exit_code: ended.exit_code,
        timed_out: ended.timed_out,
        spawn_error: ended.spawn_error.as_deref(),
        stdout: &stdout,
        stderr: &stderr,
        changed: &changed,
        timeout: def.timeout,
    });
    (outcome, left)
}

/// `22 findings, 22 errors — Requirement 'SR-001' has no task…`: what a
/// gate's findings say, in one line; `None` without any.
fn summary_of(findings: &[speccoder_core::extensions::SarifFinding]) -> Option<String> {
    let first = findings
        .iter()
        .find(|f| f.level == "error")
        .or_else(|| findings.first())?;
    let errors = findings.iter().filter(|f| f.level == "error").count();
    let count = if findings.len() == 1 {
        "1 finding".to_owned()
    } else {
        format!("{} findings", findings.len())
    };
    let errors = match errors {
        0 => String::new(),
        1 => ", 1 error".to_owned(),
        n => format!(", {n} errors"),
    };
    Some(speccoder_core::hooks::clean(&format!(
        "{count}{errors} — {}",
        first.message
    )))
}

/// The SARIF a gate left in `file`, read once and removed: a regular file
/// only (never through a link, never a device or a pipe), at most
/// [`MAX_SARIF`](speccoder_core::extensions::MAX_SARIF) bytes read.
fn read_findings(file: &Path, worktree: &Path) -> Left {
    use std::io::Read as _;
    let Ok(meta) = std::fs::symlink_metadata(file) else {
        return Left::default();
    };
    let max = speccoder_core::extensions::MAX_SARIF;
    let text = if !meta.file_type().is_file() {
        Err("they are not a regular file".to_owned())
    } else {
        let mut buf = Vec::new();
        std::fs::File::open(file)
            .and_then(|f| f.take(max as u64 + 1).read_to_end(&mut buf))
            .map_err(|e| e.to_string())
            .and_then(|n| {
                if n > max {
                    Err(format!("the findings are more than {} KB", max / 1024))
                } else {
                    String::from_utf8(buf).map_err(|_| "they are not UTF-8".to_owned())
                }
            })
    };
    let _ = std::fs::remove_file(file);
    let project = crate::extensions::project(worktree);
    match text.and_then(|t| {
        speccoder_core::extensions::sarif(&t, |p| crate::extensions::relative_to(&project, p))
    }) {
        Ok(findings) => Left {
            findings,
            error: None,
        },
        Err(e) => Left {
            findings: Vec::new(),
            error: Some(format!("its findings could not be shown: {e}")),
        },
    }
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
