//! `speccoder attest verify --branch` (and `attest badge`): every
//! ticked task of the units a branch touches, mapped to the commit that
//! carries its evidence and checked with `attest verify`'s own rules
//! ([`super::verify::object`]), plus what only a branch can show: a tick
//! with no evidence at all, a task line edited after its tick, a cited
//! requirement changed since, a commit a squash merge lost.
//!
//! Everything is read from git objects of the repository at the given
//! head, never from a working tree, and trust comes only from the caller:
//! a key the branch itself declares is never trusted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use speccoder_core::predicate::TaskVerificationV0;
use speccoder_core::verify_branch::{
    self, Anchor, BranchReport, BranchTotals, Conclusion, Mode, SkippedTask, Tick, TickKind,
    TickOutcome, TickReport, Unverifiable,
};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ObjectVerification, TraceRow, VerifyOutcome,
};

use super::verify::{Context, Target, Trust};
use super::{notes, statement};
use crate::format::Adapter;
use crate::git::Git;

/// Field and record separators for `git log` output.
const FS: char = '\u{1f}';
const RS: char = '\u{1e}';

/// Where the verifier's SSH trust comes from.
#[derive(Debug, Clone)]
pub enum AllowedSigners {
    /// A file on the verifier's machine.
    File(PathBuf),
    /// A file read from git at a revision (`<rev>:<path>`): in CI, the base
    /// branch's, never the head's.
    Rev {
        /// The revision (the base).
        rev: String,
        /// The repository path.
        path: String,
    },
}

impl AllowedSigners {
    /// `<rev>:<path>`, as `--allowed-signers-rev` takes it.
    ///
    /// # Errors
    ///
    /// No `:` separating a revision and a path.
    pub fn parse_rev(spec: &str) -> Result<Self, String> {
        match spec.split_once(':') {
            Some((rev, path)) if !rev.is_empty() && !path.is_empty() => Ok(Self::Rev {
                rev: rev.to_owned(),
                path: path.to_owned(),
            }),
            _ => Err(format!(
                "--allowed-signers-rev takes <rev>:<path>, e.g. origin/main:.github/speccoder/allowed_signers (got `{spec}`)"
            )),
        }
    }
}

/// What a branch check is asked.
#[derive(Debug, Clone)]
pub struct Options {
    /// The commit to check (`HEAD` by default).
    pub head: String,
    /// The branch it is compared with; units touched since their merge
    /// base are checked. `None`: every unit.
    pub base: Option<String>,
    /// Every unit, whatever the base.
    pub all: bool,
    /// SSH trust.
    pub allowed_signers: Option<AllowedSigners>,
    /// The Sigstore certificate identity.
    pub identity: Option<String>,
    /// The Sigstore OIDC issuer.
    pub issuer: Option<String>,
    /// Unsigned evidence is unverifiable (default).
    pub require_signed: bool,
    /// `regression_only` and `attested` fail too.
    pub require_verified: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            head: "HEAD".to_owned(),
            base: None,
            all: false,
            allowed_signers: None,
            identity: None,
            issuer: None,
            require_signed: true,
            require_verified: false,
        }
    }
}

/// A check that could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// How it ends.
    pub status: CommandStatus,
    /// Why, and what to do.
    pub message: String,
}

impl Refusal {
    /// A check that could not read what it checks: an error (exit 1), never
    /// `neutral`.
    fn internal(what: impl std::fmt::Display) -> Self {
        Self::new(
            CommandStatus::Error,
            format!("the check could not read the branch: {what}; nothing was verified"),
        )
    }

    fn new(status: CommandStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

/// The refusal when the caller gave no trust.
pub const NO_TRUST: &str = "no trust configured: pass --allowed-signers or --certificate-identity/--certificate-oidc-issuer";

/// A finished check: the report and where it was read from.
#[derive(Debug)]
pub struct Checked {
    /// The report.
    pub report: BranchReport,
    /// The repository's top level.
    pub repo: PathBuf,
    /// The project's path in the repository (`""`, or ending in `/`).
    pub prefix: String,
}

/// `attest verify --branch` for the repository
/// containing `start`, as a command result named `command` (payload
/// `branch_verify`).
#[must_use]
pub fn verify(start: &Path, opts: &Options, command: &str) -> CommandResult {
    match check(start, opts) {
        Ok(c) => result(command, c.report),
        Err(r) => CommandResult::new(command, r.status, &r.message),
    }
}

/// The report as a command result.
#[must_use]
pub fn result(command: &str, report: BranchReport) -> CommandResult {
    let status = match report.conclusion {
        Conclusion::Success => CommandStatus::Ok,
        Conclusion::Failure => CommandStatus::AttestationFailed,
        Conclusion::Neutral => CommandStatus::NotFound,
    };
    let mut message = report.title();
    if report.accepted_unsigned {
        message.push_str("; unsigned ticks were accepted (--require-signed=false)");
    }
    CommandResult::new(command, status, &message)
        .with_data(CommandData::BranchVerify(Box::new(report)))
}

/// Resolves the caller's trust; the temporary directory holds a file read
/// from git, and must live as long as the trust is used.
/// With `optional`, no trust is no refusal: signatures go unchecked (a
/// tour, which shows what is recorded and says so).
fn trust(
    top: &Path,
    opts: &Options,
    optional: bool,
) -> Result<(Option<Trust>, Value, Option<tempfile::TempDir>), Refusal> {
    let sigstore = opts.identity.is_some() && opts.issuer.is_some();
    if opts.identity.is_some() != opts.issuer.is_some() {
        return Err(Refusal::new(
            CommandStatus::UsageError,
            "--certificate-identity and --certificate-oidc-issuer go together",
        ));
    }
    let mut held = None;
    let (allowed, source) = match &opts.allowed_signers {
        None => (None, Value::Null),
        Some(AllowedSigners::File(p)) => {
            if !p.is_file() {
                return Err(Refusal::new(
                    CommandStatus::UsageError,
                    format!("--allowed-signers {}: no such file", p.display()),
                ));
            }
            (Some(p.clone()), json!({ "file": p.display().to_string() }))
        }
        Some(AllowedSigners::Rev { rev, path }) => {
            let bytes = Git::at(top)
                .run(&["show", &format!("{rev}:{path}")])
                .map_err(|e| {
                    Refusal::new(
                        CommandStatus::UsageError,
                        format!(
                            "--allowed-signers-rev {rev}:{path}: cannot read it ({})",
                            e.0
                        ),
                    )
                })?;
            let dir = tempfile::tempdir().map_err(|e| {
                Refusal::new(CommandStatus::Error, format!("no temporary directory: {e}"))
            })?;
            let file = dir.path().join("allowed_signers");
            std::fs::write(&file, bytes).map_err(|e| {
                Refusal::new(CommandStatus::Error, format!("cannot write the trust: {e}"))
            })?;
            held = Some(dir);
            (Some(file), json!({ "rev": rev, "path": path }))
        }
    };
    if allowed.is_none() && !sigstore {
        if optional {
            return Ok((
                None,
                json!({ "signatures": "not checked: no trust given" }),
                None,
            ));
        }
        return Err(Refusal::new(CommandStatus::UsageError, NO_TRUST));
    }
    let shown = json!({
        "ssh_allowed_signers": source,
        "gpg": true,
        "sigstore": opts.identity.as_ref().map(|i| json!({ "identity": i, "issuer": opts.issuer })),
    });
    Ok((
        Some(Trust {
            allowed_signers: allowed,
            identity: opts.identity.clone(),
            issuer: opts.issuer.clone(),
        }),
        shown,
        held,
    ))
}

/// `path` relative to `root`, `/`-separated; `""` for the root itself.
fn relative(path: &Path, root: &Path) -> Option<String> {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let rel = canonical(path)
        .strip_prefix(canonical(root))
        .ok()?
        .to_path_buf();
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// `rel` as a prefix: `""`, or ending in `/`.
fn as_prefix(rel: &str) -> String {
    if rel.is_empty() {
        String::new()
    } else {
        format!("{}/", rel.trim_end_matches('/'))
    }
}

/// What the history says about tasks, read once: the latest commit
/// carrying each `<unit>/<task>`'s verdict, and the trailer-only claims in
/// the checked range.
struct History {
    latest: BTreeMap<String, String>,
    bug_claims: BTreeSet<String>,
}

fn history(git: &Git, head: &str, base: Option<&str>) -> Result<History, Refusal> {
    let log = git
        .run(&[
            "log",
            "--grep=^Spec-Task: ",
            "--format=%H%x1f%(trailers:key=Spec-Task,valueonly)%x1f%(trailers:key=Verdict,valueonly)%x1e",
            head,
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| Refusal::internal(format!("git log {head}: {}", e.0)))?;
    let in_range: Option<BTreeSet<String>> = base
        .map(|b| {
            git.run(&["rev-list", &format!("{b}..{head}")])
                .map(|o| {
                    String::from_utf8_lossy(&o)
                        .lines()
                        .map(str::to_owned)
                        .collect()
                })
                .map_err(|e| Refusal::internal(format!("git rev-list {b}..{head}: {}", e.0)))
        })
        .transpose()?;
    let mut latest = BTreeMap::new();
    let mut bug_claims = BTreeSet::new();
    for record in log.split(RS) {
        let f: Vec<&str> = record.trim().split(FS).collect();
        let [commit, task, verdict] = f[..] else {
            continue;
        };
        let (task, verdict) = (task.trim(), verdict.trim());
        if task.is_empty() || verdict.is_empty() {
            continue;
        }
        if task.contains("/bug:") && in_range.as_ref().is_none_or(|r| r.contains(commit)) {
            bug_claims.insert(task.to_owned());
        }
        latest
            .entry(task.to_owned())
            .or_insert_with(|| commit.to_owned());
    }
    Ok(History { latest, bug_claims })
}

/// Noted commits not reachable from `head`, by the task their
/// task-verification names (`<unit>/<task>`): evidence a squash or rebase
/// merge left behind.
fn unreachable_evidence(
    git: &Git,
    repo: &Path,
    head: &str,
) -> Result<BTreeMap<String, String>, Refusal> {
    let reachable: BTreeSet<String> = git
        .run(&["rev-list", head])
        .map(|o| {
            String::from_utf8_lossy(&o)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .map_err(|e| Refusal::internal(format!("git rev-list {head}: {}", e.0)))?;
    let noted = notes::try_noted(repo)
        .map_err(|e| Refusal::internal(format!("git notes list: {}", e.0)))?;
    let mut out = BTreeMap::new();
    for commit in noted {
        if reachable.contains(&commit) {
            continue;
        }
        for s in crate::evidence::statements(repo, &commit) {
            if s["predicateType"] == statement::TASK_VERIFICATION
                && let Some(name) = s["subject"][0]["name"].as_str()
            {
                out.insert(name.to_owned(), commit.clone());
            }
        }
    }
    Ok(out)
}

/// The changed paths between `base` and `head`. A diff git cannot make is
/// an error: an empty list would touch no unit, and check nothing.
fn changed(git: &Git, base: &str, head: &str) -> Result<Vec<String>, Refusal> {
    git.run(&["diff", "--name-only", "-z", base, head])
        .map(|o| crate::git::nul_fields(&o))
        .map_err(|e| Refusal::internal(format!("git diff {base} {head}: {}", e.0)))
}

/// Whether `rev` holds a spec project at `start_rel` (read as the head is).
fn project_at(top: &Path, rev: &str, start_rel: &str) -> Result<bool, Refusal> {
    let dir = super::tree::intent_tops(top, rev)
        .and_then(|tops| super::tree::materialize(top, rev, Some(&tops)))
        .map_err(Refusal::internal)?;
    let at = dir.path().join(start_rel);
    Ok(crate::format::at(&at).inspect(&at, None).is_some())
}

/// One unit's ticks at the head.
struct Unit {
    name: String,
    dir: String,
    ticks: Vec<Tick>,
    skipped: Vec<SkippedTask>,
}

/// One unit's ticks; a unit the project lists but this cannot read is an
/// error, never a unit left out.
fn unit(
    format: &dyn Adapter,
    project: &Path,
    name: &str,
    prefix: &str,
    skips: &BTreeSet<String>,
) -> Result<Unit, Refusal> {
    let f = format
        .inspect(project, Some(name))
        .and_then(|snap| snap.feature)
        .ok_or_else(|| Refusal::internal(format!("the unit {name} cannot be read at the head")))?;
    let mut ticks = Vec::new();
    let mut skipped = Vec::new();
    if let Some(tasks) = &f.tasks {
        let text = std::fs::read_to_string(project.join(&tasks.path))
            .map_err(|e| Refusal::internal(format!("{prefix}{}: {e}", tasks.path)))?;
        let lines: Vec<&str> = text.lines().collect();
        for t in &tasks.tasks {
            if !t.done {
                if skips.contains(&format!("{name}/{}", t.id)) {
                    skipped.push(SkippedTask {
                        feature: name.to_owned(),
                        task: t.id.clone(),
                    });
                }
                continue;
            }
            let line = usize::try_from(t.line)
                .ok()
                .and_then(|l| l.checked_sub(1))
                .and_then(|i| lines.get(i))
                .copied()
                .unwrap_or_default();
            ticks.push(Tick {
                feature: name.to_owned(),
                task: t.id.clone(),
                title: t.title.clone(),
                kind: TickKind::Tick,
                anchor: Anchor {
                    path: format!("{prefix}{}", tasks.path),
                    line: t.line,
                },
                line_sha256: Some(crate::integrity::sha256_hex(&crate::integrity::untick(
                    line.as_bytes(),
                ))),
                claim_sha256: None,
            });
        }
    }
    Ok(Unit {
        name: name.to_owned(),
        dir: f.feature.dir,
        ticks,
        skipped,
    })
}

/// The trailer-only task `claim` (`<unit>/bug:<slug>`) as the head holds
/// it: anchored on its home's file (the first Expected line), its claim the
/// triad's digest; `None` when the home cannot be read.
fn trailer_only(format: &dyn Adapter, project: &Path, prefix: &str, claim: &str) -> Option<Tick> {
    let (unit, task) = claim.rsplit_once('/')?;
    // The claim comes from a commit's trailer (an untrusted pull request
    // writes those): only a slug, and only the unit that slug's bug is.
    let slug = task
        .strip_prefix("bug:")
        .filter(|s| speccoder_core::bug::valid_slug(s))?;
    let home = format.bug_unit(project).ok().filter(|h| h.kind.native())?;
    if format.bug_dir(&home, slug).map(|(_, u)| u).as_deref() != Some(unit) {
        return None;
    }
    let read = format.read_bug(project, &home, slug).ok()?;
    let triad = speccoder_core::bug::Triad::new(
        read.current.first().map_or("", String::as_str),
        &read.expected,
        &read.unchanged,
        crate::integrity::sha256_hex,
    )
    .ok()?;
    let text = std::fs::read_to_string(project.join(&read.file)).unwrap_or_default();
    let line = text
        .lines()
        .position(|l| l.contains("Expected:"))
        .and_then(|i| u32::try_from(i + 1).ok())
        .unwrap_or(1);
    Some(Tick {
        feature: unit.to_owned(),
        task: task.to_owned(),
        title: read.title.clone().unwrap_or_else(|| triad.title()),
        kind: TickKind::TrailerOnly,
        anchor: Anchor {
            path: format!("{prefix}{}", read.file),
            line,
        },
        line_sha256: None,
        claim_sha256: Some(triad.digest),
    })
}

/// The skipped tasks' refs, as `<unit>/<task>`.
fn skips(git: &Git) -> Result<BTreeSet<String>, Refusal> {
    git.run(&[
        "for-each-ref",
        "--format=%(refname)",
        &format!("{}/", notes::SKIPPED),
    ])
    .map(|o| {
        String::from_utf8_lossy(&o)
            .lines()
            .filter_map(|r| r.strip_prefix(&format!("{}/", notes::SKIPPED)))
            // `bug:<slug>` is kept as `bug%3A<slug>` (no `:` in a ref).
            .map(|r| r.replace("%3A", ":").replace("%25", "%"))
            .collect()
    })
    .map_err(|e| Refusal::internal(format!("git for-each-ref {}: {}", notes::SKIPPED, e.0)))
}

/// A reason's line: `trailer Spec-Task: expected …, found …`.
fn detail(v: &ObjectVerification) -> Option<String> {
    let r = v.reasons.first()?;
    Some(match (&r.field, &r.expected, &r.found) {
        (Some(field), Some(e), Some(f)) => format!("{} {field}: expected {e}, found {f}", r.code),
        (_, _, Some(f)) => format!("{}: {f}", r.code),
        _ => r.code.clone(),
    })
}

/// The tick's outcome from its commit's verification, the head's task line
/// and the typed predicate.
fn outcome_of(
    tick: &Tick,
    commit: &str,
    v: ObjectVerification,
    statements: &[Value],
    require_signed: bool,
) -> TickReport {
    let mut r = TickReport {
        tick: tick.clone(),
        commit: Some(commit.to_owned()),
        outcome: TickOutcome::Evidenced,
        verdict: v.verdict.clone(),
        reason: None,
        detail: None,
        signer: v.signer.clone(),
        decider: None,
        stale: Vec::new(),
        stale_anchor: None,
        regression: None,
        newer_fields: Vec::new(),
        verification: None,
    };
    let unverifiable = |r: &mut TickReport, reason: Unverifiable, detail: Option<String>| {
        r.outcome = TickOutcome::Unverifiable;
        r.reason = Some(reason);
        r.detail = detail;
    };
    // A predicate of another major version is not one this verifier reads.
    let other = statements.iter().find_map(|s| {
        s["predicateType"]
            .as_str()
            .filter(|t| t.starts_with("https://speccoder.dev/task-verification/"))
            .filter(|t| *t != statement::TASK_VERIFICATION)
    });
    let tv = statements
        .iter()
        .find(|s| s["predicateType"] == statement::TASK_VERIFICATION)
        .map(|s| serde_json::from_value::<TaskVerificationV0>(s["predicate"].clone()));
    match v.outcome {
        VerifyOutcome::Ok => {}
        VerifyOutcome::Unsigned if !require_signed => {}
        o => {
            let reason = Unverifiable::of(o).unwrap_or(Unverifiable::Malformed);
            match other {
                Some(t) if tv.is_none() => unverifiable(
                    &mut r,
                    Unverifiable::Malformed,
                    Some(format!("predicate {t} not supported by this verifier")),
                ),
                _ => unverifiable(&mut r, reason, detail(&v)),
            }
        }
    }
    if r.outcome == TickOutcome::Evidenced {
        match tv {
            Some(Ok(tv)) => {
                // A trailer-only task has no line: its claim is anchored on
                // the bug's triad, as the verified predicate (the one the
                // signature check covered) records it — never another
                // envelope of the note.
                let triad = tv
                    .bug
                    .as_ref()
                    .and_then(|b| b["triad_sha256"].as_str().map(str::to_owned));
                let agree = statements
                    .iter()
                    .filter(|s| s["predicateType"] == statement::TASK_VERIFICATION)
                    .all(|s| s["predicate"]["bug"]["triad_sha256"].as_str() == triad.as_deref());
                let anchored = match tick.kind {
                    TickKind::Tick => Some(&tv.task.line_hash) == tick.line_sha256.as_ref(),
                    TickKind::TrailerOnly => triad.is_some() && agree,
                };
                if tick.kind == TickKind::TrailerOnly && triad.is_some() && !agree {
                    unverifiable(
                        &mut r,
                        Unverifiable::Mismatch,
                        Some(
                            "the note's task-verification statements name different triads"
                                .to_owned(),
                        ),
                    );
                } else if !anchored {
                    unverifiable(
                        &mut r,
                        Unverifiable::Mismatch,
                        Some("the task line changed since its tick: task.line_hash differs from the line at the head".to_owned()),
                    );
                }
                // The triad read at the head against the one the fix was judged on.
                if r.outcome == TickOutcome::Evidenced
                    && tick.kind == TickKind::TrailerOnly
                    && triad.as_deref() != tick.claim_sha256.as_deref()
                {
                    r.outcome = TickOutcome::Stale;
                    r.stale_anchor = Some(tick.anchor.clone());
                    r.detail = Some(
                        "the bug's triad changed since its fix: the verdict is for the earlier words"
                            .to_owned(),
                    );
                }
                r.newer_fields = tv.newer_fields();
                r.regression = tv
                    .regression
                    .as_ref()
                    .map(speccoder_core::regression::wording);
                r.decider = tv.decider.map(|d| d.by);
            }
            Some(Err(e)) => unverifiable(
                &mut r,
                Unverifiable::Malformed,
                Some(format!("the predicate is not task-verification v0: {e}")),
            ),
            None => unverifiable(
                &mut r,
                Unverifiable::Mismatch,
                Some("no task-verification statement".to_owned()),
            ),
        }
    }
    if v.outcome == VerifyOutcome::Unsigned && require_signed {
        r.detail =
            Some("the statements are not signed (sign with `speccoder attest sign`)".to_owned());
    }
    r.verification = Some(v);
    r
}

/// A tick no reachable commit carries evidence for.
fn without_commit(tick: &Tick, lost: Option<&String>) -> TickReport {
    let (reason, detail) = match lost {
        Some(c) => (
            Unverifiable::CommitUnreachable,
            format!(
                "its evidence is on {} — not reachable from the head (a squash or rebase merge?); merge with a merge commit to keep it",
                &c[..c.len().min(12)]
            ),
        ),
        None => (
            Unverifiable::TickedWithoutEvidence,
            "no commit carries this task's verdict (ticked by hand?)".to_owned(),
        ),
    };
    TickReport {
        tick: tick.clone(),
        commit: None,
        outcome: TickOutcome::Unverifiable,
        verdict: None,
        reason: Some(reason),
        detail: Some(detail),
        signer: None,
        decider: None,
        stale: Vec::new(),
        stale_anchor: None,
        regression: None,
        newer_fields: Vec::new(),
        verification: None,
    }
}

/// The 1-based line of `text` holding `line` (trimmed), if any.
fn line_of(text: &str, line: &str) -> Option<u32> {
    let want = line.trim();
    text.lines()
        .position(|l| l.trim() == want)
        .and_then(|i| u32::try_from(i + 1).ok())
}

/// Marks evidenced ticks whose cited requirements changed between their
/// commit and the head as stale.
fn mark_stale(
    top: &Path,
    format: &dyn Adapter,
    project: &Path,
    prefix: &str,
    unit_dir: &str,
    head: &str,
    reports: &mut [TickReport],
) {
    let asked: Vec<(&str, Option<&str>)> = reports
        .iter()
        .map(|r| {
            (
                r.tick.title.as_str(),
                r.commit
                    .as_deref()
                    .filter(|_| r.outcome == TickOutcome::Evidenced),
            )
        })
        .collect();
    let Some(found) = crate::drift::at(top, format, unit_dir, prefix, head, &asked) else {
        return;
    };
    let spec = format
        .requirements_file()
        .map(|f| format!("{}/{f}", unit_dir.trim_end_matches('/')));
    let spec_text = spec
        .as_ref()
        .and_then(|s| std::fs::read_to_string(project.join(s)).ok())
        .unwrap_or_default();
    for (r, drift) in reports.iter_mut().zip(found) {
        let Some(drift) = drift.filter(|d| !d.requirements.is_empty()) else {
            continue;
        };
        r.outcome = TickOutcome::Stale;
        r.stale_anchor = drift.requirements.iter().find_map(|d| {
            let now = d.now.as_deref()?;
            Some(Anchor {
                path: format!("{prefix}{}", spec.as_deref()?),
                line: line_of(&spec_text, now)?,
            })
        });
        r.detail = Some(format!(
            "{} changed since the tick: the verdict is for the earlier wording",
            drift
                .requirements
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        r.stale = drift.requirements;
    }
}

/// The word a trace row shows for a tick.
fn word(r: &TickReport) -> String {
    match r.outcome {
        TickOutcome::Evidenced => r.verdict.clone().unwrap_or_default(),
        TickOutcome::Stale => "stale".to_owned(),
        TickOutcome::Unverifiable => "unverifiable".to_owned(),
        TickOutcome::Regressed => "regressed".to_owned(),
    }
}

/// Requirement → task → verdict rows for one unit (a requirement no tick
/// cites is a row of its own), then the scenario/test rows the signed
/// `trace[]` of each evidenced tick holds.
fn trace(
    requirements: &[(String, String)],
    reports: &[TickReport],
    signed: &BTreeMap<String, Vec<Value>>,
) -> Vec<TraceRow> {
    let mut rows = Vec::new();
    for (id, _) in requirements {
        let citing: Vec<&TickReport> = reports
            .iter()
            .filter(|r| super::cites(&r.tick.title, id))
            .collect();
        if citing.is_empty() {
            rows.push(TraceRow {
                requirement: id.clone(),
                scenario: None,
                test: None,
                task: None,
                verdict: None,
                changed: false,
            });
        }
        for r in citing {
            rows.push(TraceRow {
                requirement: id.clone(),
                scenario: None,
                test: None,
                task: Some(r.tick.task.clone()),
                verdict: Some(word(r)),
                changed: r.stale.iter().any(|d| d.id == *id),
            });
        }
    }
    for r in reports {
        for row in signed.get(&r.tick.task).into_iter().flatten() {
            let Some(test) = row["test"].as_str() else {
                continue;
            };
            rows.push(TraceRow {
                requirement: row["requirement"].as_str().unwrap_or("—").to_owned(),
                scenario: row["scenario"].as_str().map(str::to_owned),
                test: Some(test.to_owned()),
                task: Some(r.tick.task.clone()),
                verdict: Some(word(r)),
                changed: false,
            });
        }
    }
    rows
}

/// Checks the ticks of the units the branch touches.
///
/// # Errors
///
/// Not a git repository, a revision that does not resolve, or no trust.
pub fn check(start: &Path, opts: &Options) -> Result<Checked, Refusal> {
    run(start, opts, None)
}

/// One task of `feature` at the head, for its tour: checked with the
/// caller's trust when given, else read as recorded (signatures not
/// checked, said in its detail). The report holds that tick alone, or none.
///
/// # Errors
///
/// As [`check`], except that no trust is no refusal.
pub fn locate(start: &Path, opts: &Options, feature: &str, task: &str) -> Result<Checked, Refusal> {
    let opts = Options {
        all: true,
        base: None,
        ..opts.clone()
    };
    run(start, &opts, Some((feature, Some(task))))
}

/// Every tick of `feature` at `head`, read as recorded (no trust, no
/// signature checked): what `pr`'s body tours show before a maintainer's
/// verifier checks them.
///
/// # Errors
///
/// Not a git repository, or `head` does not resolve.
pub fn recorded(start: &Path, head: &str, feature: &str) -> Result<Checked, Refusal> {
    let opts = Options {
        head: head.to_owned(),
        all: true,
        ..Options::default()
    };
    run(start, &opts, Some((feature, None)))
}

/// The tasks of `feature` closed on `head`: ticked there, with the commit
/// reachable from it that carries the task's verdict — `(task, commit)`.
/// What `pr` publishes held-out sets for; read as recorded, no trust.
#[must_use]
pub fn closed(start: &Path, head: &str, feature: &str) -> Vec<(String, String)> {
    recorded(start, head, feature)
        .map(|c| {
            c.report
                .ticks
                .into_iter()
                .filter_map(|t| t.commit.map(|c| (t.tick.task, c)))
                .collect()
        })
        .unwrap_or_default()
}

/// The statements' own word, when there is no trust to check signatures
/// with: what [`super::verify::object`] would say short of them.
fn as_recorded(commit: &str, task: &str, statements: &[Value]) -> ObjectVerification {
    let tv = statements
        .iter()
        .find(|s| s["predicateType"] == statement::TASK_VERIFICATION);
    ObjectVerification {
        object: commit.to_owned(),
        kind: "task".to_owned(),
        task: task.to_owned(),
        verdict: tv.and_then(|s| s["predicate"]["verdict"].as_str().map(str::to_owned)),
        outcome: if statements.is_empty() {
            VerifyOutcome::Missing
        } else {
            VerifyOutcome::Ok
        },
        reasons: Vec::new(),
        signer: None,
        statements: statements.len(),
        same_tree_as: None,
    }
}

#[allow(clippy::too_many_lines)] // one pass, in the report's order
fn run(
    start: &Path,
    opts: &Options,
    only: Option<(&str, Option<&str>)>,
) -> Result<Checked, Refusal> {
    let top =
        Git::at(start).toplevel().ok().flatten().ok_or_else(|| {
            Refusal::new(CommandStatus::GitPrecondition, "not in a git repository")
        })?;
    let git = Git::at(&top);
    let resolve = |rev: &str| {
        git.line(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ])
        .ok()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            Refusal::new(
                CommandStatus::GitPrecondition,
                format!("{rev} is not a commit here (fetch it first)"),
            )
        })
    };
    let head = resolve(&opts.head)?;
    let base = match opts.base.as_deref() {
        Some(b) => {
            let b = resolve(b)?;
            Some(
                git.line(&["merge-base", &b, &head])
                    .ok()
                    .filter(|m| !m.is_empty())
                    .ok_or_else(|| {
                        Refusal::new(
                            CommandStatus::GitPrecondition,
                            format!("{b} and {head} share no history"),
                        )
                    })?,
            )
        }
        None => None,
    };
    let (trust, trust_shown, _held) = trust(&top, opts, only.is_some())?;
    let mode = if opts.all || base.is_none() {
        Mode::All
    } else {
        Mode::Touched
    };
    let mut report = BranchReport {
        head: head.clone(),
        base: base.clone(),
        mode,
        features: Vec::new(),
        ticks: Vec::new(),
        skipped: Vec::new(),
        totals: BranchTotals::default(),
        conclusion: Conclusion::Neutral,
        trust: trust_shown,
        accepted_unsigned: !opts.require_signed,
        require_verified: opts.require_verified,
        trace: Vec::new(),
        share: None,
        open_no_claim: Vec::new(),
        not_evaluated: vec![verify_branch::EARLIER_TASK_REGRESSIONS.to_owned()],
        newer_fields: Vec::new(),
    };
    let history = history(
        &git,
        &head,
        base.as_deref().filter(|_| mode == Mode::Touched),
    )?;
    let start_rel = relative(start, &top).unwrap_or_default();
    let dir = super::tree::intent_tops(&top, &head)
        .and_then(|tops| super::tree::materialize(&top, &head, Some(&tops)))
        .map_err(Refusal::internal)?;
    let opened = {
        let at = dir.path().join(&start_rel);
        let format = crate::format::at(&at);
        format.inspect(&at, None).map(|snap| (format, snap))
    };
    let Some((format, snap)) = opened else {
        // Nothing to verify only when there is no project to verify: a head
        // that drops the project its base (or the trust's revision) holds
        // has not shown there is nothing to check.
        let reference = base.clone().or_else(|| match &opts.allowed_signers {
            Some(AllowedSigners::Rev { rev, .. }) => resolve(rev).ok(),
            _ => None,
        });
        if let Some(r) = reference
            && project_at(&top, &r, &start_rel)?
        {
            return Err(Refusal::new(
                CommandStatus::AttestationFailed,
                format!(
                    "{head} holds no spec project where {r} holds one: the head removes or breaks it, and its ticks cannot be checked"
                ),
            ));
        }
        report.conclusion = Conclusion::of(&[], opts.require_verified, &report.not_evaluated);
        return Ok(Checked {
            report,
            repo: top,
            prefix: as_prefix(&start_rel),
        });
    };
    let project = PathBuf::from(&snap.root);
    let prefix = as_prefix(&relative(&project, dir.path()).unwrap_or_default());
    let mut units: Vec<&speccoder_core::snapshot::FeatureRef> = snap
        .features
        .iter()
        .filter(|f| only.is_none_or(|(name, _)| f.name == name))
        .collect();
    if mode == Mode::Touched
        && let Some(b) = &base
    {
        let paths = changed(&git, b, &head)?;
        let dirs: Vec<String> = units
            .iter()
            .map(|f| format!("{prefix}{}/", f.dir.trim_end_matches('/')))
            .collect();
        let policy = crate::tools::policy::Policy::new(&[], &[]).rules(format.rules());
        // A shared intent file (a constitution) speaks for every unit.
        let shared = paths.iter().any(|p| {
            p.strip_prefix(&prefix).is_some_and(|rel| {
                !dirs.iter().any(|d| p.starts_with(d))
                    && policy.write_protected(rel) == Some(crate::tools::paths::Class::Intent)
            })
        });
        if !shared {
            units = units
                .into_iter()
                .zip(&dirs)
                .filter(|(_, d)| paths.iter().any(|p| p.starts_with(d.as_str())))
                .map(|(f, _)| f)
                .collect();
        }
    }
    let skipped = skips(&git)?;
    let config_path = crate::verify::find_config(&project)
        .and_then(|d| relative(&d, dir.path()))
        .map_or_else(
            || format!("{prefix}{}", crate::verify::config::FILE),
            |d| format!("{}{}", as_prefix(&d), crate::verify::config::FILE),
        );
    let mut lost: Option<BTreeMap<String, String>> = None;
    for f in units {
        let mut u = unit(format, &project, &f.name, &prefix, &skipped)?;
        if let Some((_, Some(task))) = only {
            u.ticks.retain(|t| t.task == task);
        }
        report.features.push(u.name.clone());
        report.skipped.extend(u.skipped);
        let tasks_path = u.ticks.first().map(|t| t.anchor.path.clone());
        let unchecked = Trust::default();
        let cx = Context {
            repo: &top,
            feature: &u.name,
            tasks_path: tasks_path.as_deref(),
            config_path: &config_path,
            trust: trust.as_ref().unwrap_or(&unchecked),
            format: Some(format),
        };
        let checking = trust.is_some();
        // Where the evidence of a tick no reachable commit carries went:
        // read once, before the checks, so a git failure is an error.
        if lost.is_none()
            && u.ticks.iter().any(|t| {
                !history
                    .latest
                    .contains_key(&format!("{}/{}", t.feature, t.task))
            })
        {
            lost = Some(unreachable_evidence(&git, &top, &head)?);
        }
        // Each commit's check is its own processes; a few side by side.
        let width = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        let mut reports: Vec<TickReport> = Vec::new();
        let mut signed: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for chunk in u.ticks.chunks(width.max(1)) {
            let done: Vec<(TickReport, Vec<Value>)> = std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|tick| {
                        let commit = history
                            .latest
                            .get(&format!("{}/{}", tick.feature, tick.task))
                            .cloned();
                        let cx = &cx;
                        scope.spawn(move || {
                            let commit = commit?;
                            let statements = crate::evidence::statements(cx.repo, &commit);
                            if !checking {
                                let v = as_recorded(&commit, &tick.task, &statements);
                                let mut r = outcome_of(tick, &commit, v, &statements, false);
                                if r.outcome == TickOutcome::Evidenced {
                                    r.detail = Some(
                                        "as recorded: signatures not checked (no trust given)"
                                            .to_owned(),
                                    );
                                }
                                return Some((r, statements));
                            }
                            let v = super::verify::object(
                                cx,
                                &Target {
                                    object: commit.clone(),
                                    kind: "task",
                                    task: tick.task.clone(),
                                },
                            );
                            Some((
                                outcome_of(tick, &commit, v, &statements, opts.require_signed),
                                statements,
                            ))
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .zip(chunk)
                    .map(|(h, tick)| match h.join() {
                        Ok(Some(done)) => done,
                        Ok(None) => (
                            without_commit(
                                tick,
                                lost.as_ref().and_then(|l| {
                                    l.get(&format!("{}/{}", tick.feature, tick.task))
                                }),
                            ),
                            Vec::new(),
                        ),
                        Err(_) => {
                            let mut r = without_commit(tick, None);
                            r.reason = Some(Unverifiable::Malformed);
                            r.detail = Some("the check panicked".to_owned());
                            (r, Vec::new())
                        }
                    })
                    .collect()
            });
            for (r, statements) in done {
                let rows: Vec<Value> = statements
                    .iter()
                    .filter(|s| s["predicateType"] == statement::TASK_VERIFICATION)
                    .flat_map(|s| {
                        s["predicate"]["trace"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    })
                    .collect();
                signed.insert(r.tick.task.clone(), rows);
                reports.push(r);
            }
        }
        mark_stale(&top, format, &project, &prefix, &u.dir, &head, &mut reports);
        let requirements = format.requirement_ids(&project, &u.dir);
        report.trace.extend(trace(&requirements, &reports, &signed));
        report.ticks.extend(reports);
    }
    // Trailer-only tasks: a bug of the format's own bug structure, closed by
    // its verdict commit and anchored on its triad.
    for claim in &history.bug_claims {
        let Some(tick) = trailer_only(format, &project, &prefix, claim) else {
            // A claim whose home this build cannot read is not proof.
            if !report
                .not_evaluated
                .iter()
                .any(|n| n == verify_branch::TRAILER_ONLY_TASKS)
            {
                report
                    .not_evaluated
                    .push(verify_branch::TRAILER_ONLY_TASKS.to_owned());
            }
            continue;
        };
        if only.is_some_and(|(name, task)| {
            tick.feature != name || task.is_some_and(|t| t != tick.task)
        }) {
            continue;
        }
        let Some(commit) = history.latest.get(claim).cloned() else {
            continue;
        };
        let statements = crate::evidence::statements(&top, &commit);
        let unchecked = Trust::default();
        let cx = Context {
            repo: &top,
            feature: &tick.feature,
            tasks_path: None,
            config_path: &config_path,
            trust: trust.as_ref().unwrap_or(&unchecked),
            format: Some(format),
        };
        let r = if trust.is_some() {
            let v = super::verify::object(
                &cx,
                &Target {
                    object: commit.clone(),
                    kind: "task",
                    task: tick.task.clone(),
                },
            );
            outcome_of(&tick, &commit, v, &statements, opts.require_signed)
        } else {
            let v = as_recorded(&commit, &tick.task, &statements);
            let mut r = outcome_of(&tick, &commit, v, &statements, false);
            if r.outcome == TickOutcome::Evidenced {
                r.detail = Some("as recorded: signatures not checked (no trust given)".to_owned());
            }
            r
        };
        if !report.features.contains(&tick.feature) {
            report.features.push(tick.feature.clone());
        }
        report.ticks.push(r);
    }
    let mut newer: BTreeSet<String> = BTreeSet::new();
    for t in &report.ticks {
        newer.extend(t.newer_fields.iter().cloned());
    }
    report.newer_fields = newer.into_iter().collect();
    report.totals = BranchTotals::of(&report.ticks, report.skipped.len());
    report.share = verify_branch::share(&report.ticks);
    report.conclusion = Conclusion::of(&report.ticks, opts.require_verified, &report.not_evaluated);
    Ok(Checked {
        report,
        repo: top,
        prefix,
    })
}

/// Where `attest verify --branch` writes the check besides its result.
#[derive(Debug, Clone, Default)]
pub struct Outputs {
    /// The check as Markdown, to this file.
    pub markdown: Option<PathBuf>,
    /// The check-run request bodies (`{create, updates}`), to this file.
    pub github_check: Option<PathBuf>,
    /// The check run's name ([`super::check::NAME`] when empty).
    pub check_name: String,
    /// A neutral outcome is published as `neutral` (else `failure`).
    pub allow_neutral: bool,
    /// The workflow-command annotations, and the check appended to
    /// `$GITHUB_STEP_SUMMARY` (where the check cannot be published).
    pub annotations: bool,
}

/// A published check: the result and the workflow-command lines to print.
#[derive(Debug)]
pub struct Published {
    /// The result (payload `branch_verify`, or a refusal).
    pub result: CommandResult,
    /// With [`Outputs::annotations`]: the lines for the job's log.
    pub workflow_commands: Vec<String>,
}

/// [`verify`], and the check written where `out` asks. A file that cannot
/// be written makes the result an error (the report stays its payload).
#[must_use]
pub fn verify_and_publish(start: &Path, opts: &Options, out: &Outputs, command: &str) -> Published {
    let checked = match check(start, opts) {
        Ok(c) => c,
        Err(r) => {
            return Published {
                result: CommandResult::new(command, r.status, &r.message),
                workflow_commands: Vec::new(),
            };
        }
    };
    // Tours only where they are shown: each reads its commit's tree.
    let tours = if out.markdown.is_some() || out.github_check.is_some() || out.annotations {
        crate::tour::markdown_all(&checked)
    } else {
        Vec::new()
    };
    let report = &checked.report;
    let mut failed = Vec::new();
    if let Some(path) = &out.markdown
        && let Err(e) = write(path, &super::check::markdown(report, &tours))
    {
        failed.push(e);
    }
    if let Some(path) = &out.github_check {
        let name = if out.check_name.is_empty() {
            super::check::NAME
        } else {
            &out.check_name
        };
        let body = super::check::request(report, &tours, name, out.allow_neutral);
        if let Err(e) = write(path, &format!("{body}\n")) {
            failed.push(e);
        }
    }
    let mut workflow_commands = Vec::new();
    if out.annotations {
        workflow_commands = super::check::workflow_commands(report);
        if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
            use std::io::Write as _;
            let appended = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&summary)
                .and_then(|mut f| writeln!(f, "{}", super::check::markdown(report, &tours)));
            if let Err(e) = appended {
                failed.push(format!("the step summary was not written: {e}"));
            }
        }
    }
    let result = result(command, checked.report);
    let result = if failed.is_empty() {
        result
    } else {
        failed_writes(command, result, &failed)
    };
    Published {
        result,
        workflow_commands,
    }
}

/// `attest badge`: the share of ticks with verifying evidence over every
/// unit, as an SVG and a shields.io endpoint. A measurement: `ok` whenever
/// it was written, whatever the share.
#[must_use]
pub fn badge(
    start: &Path,
    opts: &Options,
    svg: Option<&Path>,
    endpoint: Option<&Path>,
    command: &str,
) -> CommandResult {
    let opts = Options {
        all: true,
        ..opts.clone()
    };
    let checked = match check(start, &opts) {
        Ok(c) => c,
        Err(r) => return CommandResult::new(command, r.status, &r.message),
    };
    let badge = verify_branch::Badge::of(&checked.report);
    let mut failed = Vec::new();
    if let Some(path) = svg
        && let Err(e) = write(path, &badge.svg())
    {
        failed.push(e);
    }
    if let Some(path) = endpoint {
        let json = serde_json::to_string_pretty(&badge).unwrap_or_default();
        if let Err(e) = write(path, &format!("{json}\n")) {
            failed.push(e);
        }
    }
    let result = CommandResult::new(
        command,
        CommandStatus::Ok,
        &format!("evidence: {} ({})", badge.message, badge.color),
    )
    .with_data(CommandData::BranchVerify(Box::new(checked.report)));
    if failed.is_empty() {
        result
    } else {
        failed_writes(command, result, &failed)
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// `result` as an error naming the files that were not written.
fn failed_writes(command: &str, result: CommandResult, failed: &[String]) -> CommandResult {
    let message = format!("{}; {}", result.message(), failed.join("; "));
    let shown = CommandResult::new(command, CommandStatus::Error, &message);
    match result.data() {
        Some(CommandData::BranchVerify(r)) => shown.with_data(CommandData::BranchVerify(r.clone())),
        _ => shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trust_revision_needs_both_parts() {
        assert!(matches!(
            AllowedSigners::parse_rev("origin/main:.github/speccoder/allowed_signers"),
            Ok(AllowedSigners::Rev { rev, path }) if rev == "origin/main" && path == ".github/speccoder/allowed_signers"
        ));
        assert!(AllowedSigners::parse_rev("main").is_err());
        assert!(AllowedSigners::parse_rev(":x").is_err());
    }

    #[test]
    fn the_line_of_a_requirement_is_found_trimmed() {
        let text = "# Spec\n\n- **FR-001**: Users MUST log in.\n";
        assert_eq!(
            line_of(text, "  - **FR-001**: Users MUST log in. "),
            Some(3)
        );
        assert_eq!(line_of(text, "nope"), None);
    }

    #[test]
    fn a_git_failure_is_an_error_never_an_empty_answer() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                    .args(["-c", "commit.gpgsign=false"])
                    .args(args)
                    .current_dir(dir.path())
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        };
        run(&["init", "-q"]);
        run(&["commit", "-q", "--allow-empty", "-m", "x"]);
        let git = Git::at(dir.path());
        // An empty diff would touch no unit and check nothing.
        let Err(r) = changed(&git, "no-such-base", "HEAD") else {
            panic!("a failed diff was an empty one");
        };
        assert_eq!(r.status, CommandStatus::Error);
        assert!(history(&git, "no-such-head", None).is_err());
        assert!(history(&git, "HEAD", Some("no-such-base")).is_err());
        assert!(unreachable_evidence(&git, dir.path(), "no-such-head").is_err());
        assert!(changed(&git, "HEAD", "HEAD").unwrap().is_empty());
    }

    #[test]
    fn no_trust_is_refused_even_when_unsigned_ticks_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let opts = Options {
            require_signed: false,
            ..Options::default()
        };
        let Err(r) = trust(dir.path(), &opts, false) else {
            panic!("trusted nothing")
        };
        assert_eq!(r.message, NO_TRUST);
    }
}
