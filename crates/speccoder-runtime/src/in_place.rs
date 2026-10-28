//! In-place runs: `speccoder run --in-place` works on
//! `speccoder/<feature>` in the person's own checkout instead of a separate
//! worktree — the way to run features whose spec git ignores (a worktree
//! holds only commits).
//!
//! What the worktree gave, replaced:
//! - a clean checkout before the run, and the person's branch never touched:
//!   the run switches to `speccoder/<feature>` and back;
//! - an intent the run cannot read differently later: every intent, judge
//!   and context file — git-ignored ones included — is hashed when the run
//!   starts (the manifest) and re-checked before every task and every tick;
//!   only the engine's own one-byte tick and convergence append change it;
//! - a checkout nobody else changes: HEAD, the branch and a clean tree are
//!   re-checked before every task and tick (the checkout guard).
//!
//! Evidence is committed before the tick: a gitignored `tasks.md` is never in
//! a commit, so its tick is local, and the commit says so (`Tick: local`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::{CommandData, CommandResult, CommandStatus, InPlaceReport, RefusedPath};

use crate::git::Git;
use crate::integrity::{JudgeSnapshot, sha256_hex};
use crate::tools::policy::Policy;
use crate::worktree;

/// The session record in a project's state directory.
pub const FILE: &str = "in-place.json";

/// Runtime state under the intent that is not intent.
fn runtime_state(path: &str) -> bool {
    path.contains("/workflows/runs/") || path.split('/').any(|c| c == "__pycache__")
}

/// Why an in-place run refuses or stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPlaceError {
    /// `DIRTY-CHECKOUT`, `INTENT-CHANGED`, ….
    pub code: &'static str,
    /// What happened.
    pub message: String,
    /// The paths involved, with why.
    pub paths: Vec<(String, String)>,
    /// What to run: the first is `fix:`, the rest `or:`.
    pub fix: Vec<String>,
    /// The person's own commands among them, as argvs.
    pub commands: Vec<speccoder_core::step::Argvs>,
}

impl InPlaceError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            paths: Vec::new(),
            fix: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// The person's own command as argvs: its line is the fix.
    fn command(mut self, commands: Vec<Vec<String>>) -> Self {
        self.fix = vec![speccoder_core::step::render(&commands)];
        self.commands = vec![commands];
        self
    }

    fn fix(mut self, lines: &[&str]) -> Self {
        self.fix = lines.iter().map(|l| (*l).to_owned()).collect();
        self
    }

    /// A stop during the run (exit 23) rather than a refusal before it (14).
    #[must_use]
    pub fn is_stop(&self) -> bool {
        matches!(
            self.code,
            "INTENT-CHANGED"
                | "JUDGE-CHANGED"
                | "CHECKOUT-CHANGED"
                | "GIT-CHANGED"
                | "RETURN-BLOCKED"
        )
    }

    /// One line naming the code, for a run report's reason and a saved
    /// attempt: `stopped[INTENT-CHANGED]: … (specs/x/spec.md: changed)`.
    #[must_use]
    pub fn line(&self) -> String {
        let word = if self.is_stop() { "stopped" } else { "error" };
        let mut line = format!("{word}[{}]: {}", self.code, self.message);
        for (p, why) in &self.paths {
            line.push_str(&format!("; {p} ({why})"));
        }
        line
    }

    /// The command's result, its code and fixes on the envelope.
    #[must_use]
    pub fn result(&self, command: &str) -> CommandResult {
        let status = match self.code {
            "IN-PLACE-PARALLEL" => CommandStatus::UsageError,
            _ if self.is_stop() => CommandStatus::NeedsHuman,
            _ => CommandStatus::GitPrecondition,
        };
        CommandResult::new(command, status, &self.message)
            .with_code(self.code)
            .with_fix(self.fix.clone())
            .with_commands(self.commands.clone())
            .with_data(CommandData::InPlace(Box::new(InPlaceReport {
                paths: self
                    .paths
                    .iter()
                    .map(|(path, reason)| RefusedPath {
                        path: path.clone(),
                        reason: reason.clone(),
                    })
                    .collect(),
            })))
    }
}

/// The branch the person was on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Original {
    /// A branch name.
    Branch(String),
    /// A detached commit.
    Detached(String),
}

impl Original {
    fn target(&self) -> &str {
        match self {
            Self::Branch(b) | Self::Detached(b) => b,
        }
    }

    /// As a person reads it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Branch(b) => b.clone(),
            Self::Detached(c) => format!("detached {}", c.chars().take(10).collect::<String>()),
        }
    }
}

/// An in-place run in progress: written before the checkout is switched,
/// removed after it is switched back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The feature.
    pub feature: String,
    /// `speccoder/<feature>`.
    pub branch: String,
    /// Where the person was.
    pub original: Original,
    /// The checkout (the repository's root).
    pub checkout: PathBuf,
    /// The project root inside it.
    pub project: PathBuf,
    /// The branch's tip as speccoder last left it.
    pub tip: String,
    /// Intent, judge and context files by hash, as the run expects them.
    pub manifest: BTreeMap<String, String>,
    /// The running process.
    pub pid: u32,
    /// Seconds since the epoch.
    pub started_at: u64,
    /// What starting this run did on its own (a resume, work set aside, a
    /// tick restored): told with the outcome, never recorded.
    #[serde(skip)]
    pub notes: Vec<String>,
    /// What git ignored when the session began (directories end in `/`):
    /// hidden from the model's file tools, as a worktree would not hold it.
    #[serde(default)]
    pub hidden: Vec<String>,
    /// The repository's configuration, hooks, `info/` and refs (other than
    /// the session's branch) by hash: nothing a run does may change them.
    #[serde(default)]
    pub git: String,
}

/// The manifest of `project`: every intent, judge and context file, ignored
/// ones included, minus runtime state.
#[must_use]
pub fn manifest(project: &Path, policy: &Policy) -> BTreeMap<String, String> {
    JudgeSnapshot::take(project, policy)
        .entries()
        .iter()
        .filter(|(p, _)| !runtime_state(p))
        .map(|(p, h)| (p.clone(), h.clone()))
        .collect()
}

/// The paths that differ between two manifests, with how.
#[must_use]
pub fn diff(
    expected: &BTreeMap<String, String>,
    now: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (p, h) in expected {
        match now.get(p) {
            None => out.push((p.clone(), "deleted".to_owned())),
            Some(n) if n != h => out.push((p.clone(), "modified".to_owned())),
            _ => {}
        }
    }
    for p in now.keys().filter(|p| !expected.contains_key(*p)) {
        out.push((p.clone(), "added".to_owned()));
    }
    out
}

/// Whether `after` is `before` with exactly one byte changed, from a space
/// to `x`/`X` (the engine's tick).
#[must_use]
pub fn is_tick(before: &[u8], after: &[u8]) -> bool {
    let changed: Vec<(u8, u8)> = before
        .iter()
        .zip(after)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (*a, *b))
        .collect();
    before.len() == after.len() && matches!(changed.as_slice(), [(b' ', b'x' | b'X')])
}

/// SHA-256 over the intent files except the task lists (files named
/// `tasks_file`, whose task lines the commits pin one by one with
/// `Task-Line:`): what the verdict commits name (`Intent-Digest:`), so a
/// later reader can tell the spec, plan or constitution changed — not a
/// tick or a convergence round.
#[must_use]
pub fn intent_digest(project: &Path, policy: &Policy, tasks_file: &str) -> String {
    let mut parts = Vec::new();
    for (path, hash) in manifest(project, policy) {
        if policy.write_protected(&path) != Some(crate::tools::paths::Class::Intent) {
            continue;
        }
        if path.rsplit('/').next() == Some(tasks_file) {
            continue;
        }
        parts.push(format!("{path} {hash}"));
    }
    sha256_hex(parts.join("\n").as_bytes())
}

pub(crate) use crate::integrity::untick;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Session {
    /// The record's path in a project's state directory.
    #[must_use]
    pub fn path(plane_dir: &Path) -> PathBuf {
        plane_dir.join(FILE)
    }

    /// The record, if a session exists.
    #[must_use]
    pub fn load(plane_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(plane_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Writes the record (temporary file, then rename).
    pub fn save(&self, plane_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(plane_dir).map_err(|e| e.to_string())?;
        let tmp = plane_dir.join(format!("{FILE}.tmp"));
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, Self::path(plane_dir)).map_err(|e| e.to_string())
    }

    /// Whether the process that owns it is running.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.pid != std::process::id() && crate::progress::alive(self.pid)
    }

    /// Checks the preconditions, records the session, and switches the
    /// checkout to `speccoder/<feature>`.
    pub fn begin(
        project: &Path,
        plane_dir: &Path,
        feature: &str,
        policy: &Policy,
    ) -> Result<Self, InPlaceError> {
        let root_of = Git::at(project).toplevel().ok().flatten().ok_or_else(|| {
            InPlaceError::new("UNBORN-HEAD", "the project is not inside a git repository")
                .fix(&["git init && git add -A && git commit -m init"])
        })?;
        let checkout = std::fs::canonicalize(&root_of).unwrap_or(root_of);
        let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        let git = Git::at(&checkout);
        if let Some(s) = Self::load(plane_dir).filter(Self::is_live) {
            return Err(InPlaceError::new(
                "IN-PLACE-ACTIVE",
                format!(
                    "an in-place run of {} is running (pid {})",
                    s.feature, s.pid
                ),
            ));
        }
        if git.head().ok().flatten().is_none() {
            return Err(
                InPlaceError::new("UNBORN-HEAD", "the repository has no commits yet")
                    .fix(&["git add -A && git commit -m init"]),
            );
        }
        // A run of this feature that died on its branch, at the commit it
        // recorded: with the intent unchanged it is taken over — an attempt
        // the journal can resume resumes; with the intent changed, what it
        // left is saved and the task starts fresh.
        let mut notes = Vec::new();
        if let Some(mut prior) = Self::load(plane_dir).filter(|p| {
            p.feature == feature
                && git
                    .line(&["symbolic-ref", "-q", "HEAD"])
                    .unwrap_or_default()
                    == format!("refs/heads/{}", p.branch)
                && git.line(&["rev-parse", "HEAD"]).unwrap_or_default() == p.tip
        }) {
            let dirty = worktree::require_clean(&checkout).is_err();
            if diff(&prior.manifest, &manifest(&project, policy)).is_empty() {
                if !dirty || resumable(plane_dir, feature, &prior.tip) {
                    prior.pid = std::process::id();
                    prior.started_at = now();
                    prior
                        .save(plane_dir)
                        .map_err(|e| InPlaceError::new("IN-PLACE-ACTIVE", e))?;
                    prior.notes.push(format!(
                        "took over the interrupted in-place run (returns to {})",
                        prior.original.describe()
                    ));
                    return Ok(prior);
                }
            } else if dirty
                && let Some(r) = prior.set_aside(
                    plane_dir,
                    "left by an interrupted in-place run; the spec changed since, so it starts fresh",
                )? {
                    notes.push(format!(
                        "the spec changed since the run was interrupted: what it left is saved as {r} and the task starts fresh"
                    ));
                }
        }
        if let Err(r) = worktree::require_clean(&checkout) {
            let mut e = InPlaceError::new(
                "DIRTY-CHECKOUT",
                "the checkout has uncommitted changes; an in-place run needs a clean one",
            )
            .fix(&["git stash -u", "git commit"]);
            if let Some((p, _)) = r.paths.iter().find(|(p, _)| {
                policy.write_protected(p) == Some(crate::tools::paths::Class::Intent)
            }) {
                e.fix.push(format!("echo {p} >> .git/info/exclude"));
            }
            e.paths = r.paths;
            return Err(e);
        }
        let config =
            crate::verify::find_config(&project).map(|d| d.join(crate::verify::config::FILE));
        let tracked = config.as_ref().is_some_and(|c| {
            git.succeeds(&["ls-files", "--error-unmatch", "--", &c.to_string_lossy()])
                .unwrap_or(false)
        });
        if !tracked {
            return Err(InPlaceError::new(
                "CONFIG-NOT-TRACKED",
                "speccoder.toml is not committed; the checks that judge a run are reviewed like code",
            )
            .fix(&["git add speccoder.toml && git commit -m \"speccoder config\""]));
        }
        let branch = format!("speccoder/{feature}");
        let listing = git
            .run(&["worktree", "list", "--porcelain"])
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let mut current_wt = String::new();
        for line in listing.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                current_wt = p.to_owned();
            } else if line == format!("branch refs/heads/{branch}")
                && Path::new(&current_wt) != checkout
            {
                return Err(InPlaceError::new(
                    "BRANCH-IN-WORKTREE",
                    format!("{branch} is checked out in the worktree {current_wt}"),
                )
                .command(vec![
                    // An absolute path (git's listing): never a flag.
                    ["git", "worktree", "remove"]
                        .map(str::to_owned)
                        .into_iter()
                        .chain([current_wt.clone()])
                        .collect(),
                ]));
            }
        }
        let head_ref = git
            .line(&["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default();
        // A run that died leaves its record: on its branch, the branch to
        // go back to is the recorded one, not the one checked out now.
        let prior = Self::load(plane_dir);
        if let Some(p) = prior.as_ref().filter(|p| p.feature != feature) {
            return Err(InPlaceError::new(
                "IN-PLACE-SESSION",
                format!(
                    "an interrupted in-place run of {} holds this checkout",
                    p.feature
                ),
            )
            .fix(&["speccoder run --in-place --return"]));
        }
        let original = match (prior, head_ref.strip_prefix("refs/heads/")) {
            (Some(p), Some(b)) if b == branch => p.original,
            (_, Some(b)) => Original::Branch(b.to_owned()),
            (_, None) => Original::Detached(git.line(&["rev-parse", "HEAD"]).unwrap_or_default()),
        };
        let mut session = Self {
            feature: feature.to_owned(),
            branch: branch.clone(),
            original: original.clone(),
            checkout: checkout.clone(),
            project: project.clone(),
            tip: String::new(),
            manifest: BTreeMap::new(),
            pid: std::process::id(),
            started_at: now(),
            notes,
            hidden: Vec::new(),
            git: String::new(),
        };
        // Written before switching: a crash after the switch still knows
        // where to go back to.
        session
            .save(plane_dir)
            .map_err(|e| InPlaceError::new("IN-PLACE-ACTIVE", e))?;
        if original != Original::Branch(branch.clone()) {
            let exists = git
                .succeeds(&[
                    "rev-parse",
                    "--verify",
                    "-q",
                    &format!("refs/heads/{branch}"),
                ])
                .unwrap_or(false);
            let switched = if exists {
                git.run(&["switch", "-q", &branch])
            } else {
                git.run(&["switch", "-q", "-c", &branch, "HEAD"])
            };
            if let Err(e) = switched {
                let _ = std::fs::remove_file(Self::path(plane_dir));
                return Err(InPlaceError::new(
                    "DIRTY-CHECKOUT",
                    format!("cannot switch to {branch}: {}", e.0),
                ));
            }
            // The person's committed spec changes reach the branch.
            if exists
                && let Original::Branch(b) = &original
                && !git
                    .succeeds(&["merge-base", "--is-ancestor", b, "HEAD"])
                    .unwrap_or(true)
            {
                let mut merge = crate::attempts::IDENTITY.to_vec();
                merge.extend(["merge", "-q", "--no-edit", b.as_str()]);
                if git.run(&merge).is_err() {
                    let _ = git.run(&["merge", "--abort"]);
                    let _ = git.run(&["switch", "-q", original.target()]);
                    let _ = std::fs::remove_file(Self::path(plane_dir));
                    return Err(InPlaceError::new(
                        "MERGE-CONFLICT",
                        format!("{b} does not merge cleanly into {branch}"),
                    )
                    .fix(&[&format!(
                        "git switch {branch} && git merge {b}   # resolve, commit, then run again"
                    )]));
                }
            }
        }
        session.tip = git.line(&["rev-parse", "HEAD"]).unwrap_or_default();
        session.manifest = manifest(&project, policy);
        session.git = git_fingerprint(&checkout, &branch);
        session.hidden = git
            .run(&[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ])
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .split('\0')
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        session
            .save(plane_dir)
            .map_err(|e| InPlaceError::new("IN-PLACE-ACTIVE", e))?;
        Ok(session)
    }

    /// Saves what a dead run left in the checkout under an attempt ref,
    /// restores the branch tip, and forgets the attempts that cannot resume
    /// any more. `None` when there was nothing to save.
    fn set_aside(&self, plane_dir: &Path, reason: &str) -> Result<Option<String>, InPlaceError> {
        // A call that waited for a person waits on nothing now. One that
        // cannot be withdrawn stops this: its attempt would resume into a
        // checkout that no longer holds its writes.
        crate::approvals::withdraw_waiting(&self.project, plane_dir, &self.feature, reason)
            .map_err(|e| {
                InPlaceError::new(
                    "RETURN-BLOCKED",
                    format!("the calls waiting for a person could not be withdrawn: {e}"),
                )
            })?;
        let _ = std::fs::remove_dir_all(plane_dir.join("resume").join(&self.feature));
        if worktree::require_clean(&self.checkout).is_ok() {
            return Ok(None);
        }
        let tmp = plane_dir.join("tmp");
        let record = crate::attempts::Record {
            run: None,
            outcome: "interrupted",
            reason,
            pending: None,
            usage: None,
        };
        let saved = crate::attempts::save(&self.checkout, &tmp, &self.feature, "in-place", &record)
            .map_err(|e| InPlaceError::new("RETURN-BLOCKED", e.0))?;
        let git = Git::at(&self.checkout);
        let head_tree =
            worktree::Checkpoint(git.line(&["rev-parse", "HEAD^{tree}"]).unwrap_or_default());
        worktree::restore(&self.checkout, &tmp, &head_tree)
            .map_err(|e| InPlaceError::new("RETURN-BLOCKED", e.0))?;
        Ok(Some(saved.reference))
    }

    /// The checkout guard and the manifest, before every task: HEAD where
    /// speccoder left it, nothing changed, the protected files as recorded.
    pub fn check(&self, policy: &Policy) -> Result<(), InPlaceError> {
        self.check_with(policy, true)
    }

    /// [`Session::check`] before a tick: the attempt's own writes are in the
    /// checkout then, so only HEAD and the protected files are compared.
    pub fn check_before_tick(&self, policy: &Policy) -> Result<(), InPlaceError> {
        self.check_with(policy, false)
    }

    fn check_with(&self, policy: &Policy, clean: bool) -> Result<(), InPlaceError> {
        // Where the sandbox cannot keep `.git` read-only, a change is caught
        // here — before speccoder runs git there again for a tick.
        if !self.git.is_empty() && git_fingerprint(&self.checkout, &self.branch) != self.git {
            return Err(InPlaceError::new(
                "GIT-CHANGED",
                "the repository's configuration, hooks or branches changed during the run; speccoder does not run git against them",
            )
            .fix(&[
                "git -C . config --list --local   # review, restore what changed",
                "speccoder run --in-place --return",
            ]));
        }
        let git = Git::at(&self.checkout);
        let head_ref = git
            .line(&["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default();
        let head = git.line(&["rev-parse", "HEAD"]).unwrap_or_default();
        let fix = [
            format!("git stash -u && git switch {}", self.original.target()),
            "speccoder run --in-place --return".to_owned(),
        ];
        if head_ref != format!("refs/heads/{}", self.branch) || head != self.tip {
            let mut e = InPlaceError::new(
                "CHECKOUT-CHANGED",
                format!(
                    "the checkout moved during the run (HEAD is {}, speccoder left {} at {}); nothing was switched",
                    if head_ref.is_empty() {
                        head.clone()
                    } else {
                        head_ref.clone()
                    },
                    self.branch,
                    self.tip.chars().take(10).collect::<String>()
                ),
            );
            e.fix = fix.to_vec();
            return Err(e);
        }
        if let Some(r) = clean
            .then(|| worktree::require_clean(&self.checkout).err())
            .flatten()
        {
            let mut e = InPlaceError::new(
                "CHECKOUT-CHANGED",
                "the checkout changed during the run; nothing was saved, restored or switched",
            );
            e.paths = r.paths;
            e.fix = fix.to_vec();
            return Err(e);
        }
        let now = manifest(&self.project, policy);
        let changed = diff(&self.manifest, &now);
        if !changed.is_empty() {
            let intent = changed.iter().any(|(p, _)| {
                policy.write_protected(p) == Some(crate::tools::paths::Class::Intent)
            });
            let mut e = InPlaceError::new(
                if intent {
                    "INTENT-CHANGED"
                } else {
                    "JUDGE-CHANGED"
                },
                if intent {
                    "the spec changed under the running judge; the run stops before judging against it"
                } else {
                    "a judge input changed under the running judge"
                },
            );
            e.paths = changed;
            e.fix = vec![
                "review the change, then run again: the next run judges against the spec as it is"
                    .to_owned(),
            ];
            return Err(e);
        }
        Ok(())
    }

    /// After a speccoder commit: the new tip.
    pub fn advance(&mut self, plane_dir: &Path) {
        self.tip = Git::at(&self.checkout)
            .line(&["rev-parse", "HEAD"])
            .unwrap_or_default();
        let _ = self.save(plane_dir);
    }

    /// After the engine's tick or append of `file` (absolute): the manifest
    /// takes its new hash — only when the change is exactly that (`accept`
    /// says what the bytes before and after may be).
    pub fn accept(
        &mut self,
        plane_dir: &Path,
        file: &Path,
        before: &[u8],
        accept: impl Fn(&[u8], &[u8]) -> bool,
    ) -> Result<(), InPlaceError> {
        let after = std::fs::read(file).unwrap_or_default();
        let rel = file
            .strip_prefix(&self.project)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !accept(before, &after) {
            return Err(InPlaceError::new(
                "INTENT-CHANGED",
                format!("{rel} changed beyond the engine's own write"),
            ));
        }
        self.manifest.insert(rel, sha256_hex(&after));
        let _ = self.save(plane_dir);
        Ok(())
    }

    /// Switches the checkout back to where the person was, when it is as
    /// speccoder left it; removes the record after.
    pub fn finish(&self, plane_dir: &Path) -> Result<(), InPlaceError> {
        let git = Git::at(&self.checkout);
        let head = git.line(&["rev-parse", "HEAD"]).unwrap_or_default();
        let on_branch = git
            .line(&["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default()
            == format!("refs/heads/{}", self.branch);
        if !on_branch || head != self.tip || worktree::require_clean(&self.checkout).is_err() {
            return Err(InPlaceError::new(
                "RETURN-BLOCKED",
                format!(
                    "the checkout is not as speccoder left it, so it stays on {}",
                    self.branch
                ),
            )
            .fix(&[
                &format!("git stash -u && git switch {}", self.original.target()),
                "speccoder run --in-place --return",
            ]));
        }
        if self.original != Original::Branch(self.branch.clone()) {
            let back = match &self.original {
                Original::Branch(b) => git.run(&["switch", "-q", b]),
                Original::Detached(c) => git.run(&["switch", "-q", "--detach", c]),
            };
            if let Err(e) = back {
                return Err(InPlaceError::new("RETURN-BLOCKED", e.0)
                    .fix(&[&format!("git switch {}", self.original.target())]));
            }
        }
        let _ = std::fs::remove_file(Self::path(plane_dir));
        Ok(())
    }
}

impl Session {
    /// A run killed between its verdict commit and the local tick: each
    /// open task whose newest commit names its line exactly (`Task-Line`) is
    /// ticked now, the manifest accepting it. Returns what was ticked.
    pub fn reconcile(
        &mut self,
        plane_dir: &Path,
        tasks_file: &Path,
        tasks: &speccoder_core::snapshot::ParsedTasks,
        tick: impl Fn(&Path, &str, &str) -> Result<(), String>,
    ) -> Vec<String> {
        let log = Git::at(&self.checkout)
            .run(&[
                "log",
                "--format=%(trailers:key=Spec-Task,valueonly)\u{1f}%(trailers:key=Task-Line,valueonly)\u{1e}",
                &self.branch,
            ])
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let mut newest: BTreeMap<String, String> = BTreeMap::new();
        for record in log.split('\u{1e}') {
            let Some((task, line)) = record.trim().split_once('\u{1f}') else {
                continue;
            };
            if let Some(id) = task.trim().strip_prefix(&format!("{}/", self.feature)) {
                newest
                    .entry(id.to_owned())
                    .or_insert_with(|| line.trim().to_owned());
            }
        }
        let mut ticked = Vec::new();
        for t in tasks.tasks.iter().filter(|t| !t.done) {
            if newest
                .get(&t.id)
                .is_none_or(|l| l.is_empty() || *l != t.line_hash)
            {
                continue;
            }
            let before = std::fs::read(tasks_file).unwrap_or_default();
            if tick(tasks_file, &t.id, &t.line_hash).is_ok()
                && self.accept(plane_dir, tasks_file, &before, is_tick).is_ok()
            {
                ticked.push(t.id.clone());
            }
        }
        if !ticked.is_empty() {
            self.notes.push(format!(
                "ticked {} from {} verdict commit(s): the run was interrupted before the tick",
                ticked.join(", "),
                ticked.len()
            ));
        }
        ticked
    }
}

/// What committing a task in place did.
#[derive(Debug, Default)]
pub struct Committed {
    /// The verdict commit.
    pub commit: Option<String>,
    /// Whether the task is ticked.
    pub ticked: bool,
    /// What went wrong, for the report's integrity list.
    pub problems: Vec<String>,
}

impl Session {
    /// Commits a passed (or attested) task and ticks it. The evidence comes
    /// first: a `tasks_file` git ignores is never in a commit, so the commit
    /// names the tick as local (`Tick: local`), and the tick follows it. A
    /// tracked one is ticked and committed together, as in a worktree.
    /// Every commit says it was made in place, against which intent
    /// (`intent_digest`) and which task line (`line_hash`, before the tick).
    pub fn commit_and_tick(
        &mut self,
        plane_dir: &Path,
        tasks_file: &Path,
        line_hash: &str,
        message: &str,
        intent_digest: &str,
        tick: impl Fn() -> Result<(), String>,
    ) -> Committed {
        let mut out = Committed::default();
        let rel = tasks_file
            .strip_prefix(&self.checkout)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let tracked = Git::at(&self.checkout)
            .succeeds(&["ls-files", "--error-unmatch", "--", &rel])
            .unwrap_or(false);
        let mut trailers = format!(
            "Mode: in-place\nIntent-Digest: sha256:{intent_digest}\nTask-Line: {line_hash}\n"
        );
        if !tracked {
            trailers.push_str(&format!("Tick: local {rel}\n"));
        }
        let message = format!("{}\n{trailers}", message.trim_end());
        let before = std::fs::read(tasks_file).unwrap_or_default();
        let tick_now = |out: &mut Committed| match tick() {
            Ok(()) => out.ticked = true,
            Err(e) => out.problems.push(format!("tick refused: {e}")),
        };
        if tracked {
            tick_now(&mut out);
        }
        if out.ticked || !tracked {
            match crate::pipeline::commit::commit_attempt_as(&self.checkout, &message, !tracked) {
                Ok(id) => out.commit = Some(id),
                Err(e) => out.problems.push(format!("commit failed: {e}")),
            }
        }
        if !tracked && out.commit.is_some() {
            tick_now(&mut out);
        }
        if out.ticked
            && let Err(e) = self.accept(plane_dir, tasks_file, &before, is_tick)
        {
            out.problems.push(e.message);
        }
        self.advance(plane_dir);
        out
    }
}

impl Session {
    /// Commits a convergence round in place: the append to `tasks_file`
    /// (old bytes an exact prefix of the new) is accepted into the manifest,
    /// and the commit says it was made in place, against which intent; a
    /// git-ignored task list is not in it, so the commit may be empty.
    pub fn commit_round(
        &mut self,
        plane_dir: &Path,
        tasks_file: &Path,
        before: &[u8],
        message: &str,
        intent_digest: &str,
    ) -> Result<String, String> {
        self.accept(plane_dir, tasks_file, before, |b, a| a.starts_with(b))
            .map_err(|e| e.message)?;
        let rel = tasks_file
            .strip_prefix(&self.checkout)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let tracked = Git::at(&self.checkout)
            .succeeds(&["ls-files", "--error-unmatch", "--", &rel])
            .unwrap_or(false);
        let mut trailers = format!("Mode: in-place\nIntent-Digest: sha256:{intent_digest}\n");
        if !tracked {
            trailers.push_str(&format!("Tasks: local {rel}\n"));
        }
        let message = format!("{}\n{trailers}", message.trim_end());
        let id = crate::pipeline::commit::commit_attempt_as(&self.checkout, &message, !tracked)?;
        self.advance(plane_dir);
        Ok(id)
    }

    /// The model's view of this checkout: the policy denies git that moves
    /// it and hides what git ignored; the sandbox keeps `.git` read-only
    /// and the ignored secrets unreadable.
    #[must_use]
    pub fn confine(
        &self,
        policy: Policy,
        runner: crate::sandbox::Runner,
    ) -> (Policy, crate::sandbox::Runner, Option<String>) {
        let secrets: Vec<PathBuf> = self
            .hidden
            .iter()
            .map(|h| h.trim_end_matches('/'))
            .filter(|h| policy.protected().is_secret(h))
            .map(|h| self.checkout.join(h))
            .collect();
        let (runner, notice) = runner.carve(&[self.checkout.join(".git")], &secrets);
        (
            policy.in_place().with_hidden(self.hidden.clone()),
            runner,
            notice,
        )
    }
}

/// A hash over what in `.git` could make git run code or move a branch the
/// person owns: `config`, `hooks/`, `info/`, and every branch and tag except
/// `branch` (speccoder's own).
fn git_fingerprint(checkout: &Path, branch: &str) -> String {
    let git = Git::at(checkout);
    let Some(dir) = git
        .line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .map(PathBuf::from)
    else {
        return String::new();
    };
    let mut parts = Vec::new();
    let mut files = vec![dir.join("config")];
    for sub in ["hooks", "info"] {
        files.extend(
            ignore::WalkBuilder::new(dir.join(sub))
                .standard_filters(false)
                .build()
                .flatten()
                .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
                .map(ignore::DirEntry::into_path),
        );
    }
    files.sort();
    for f in files {
        let hash = std::fs::read(&f).map_or_else(|_| "-".to_owned(), |b| sha256_hex(&b));
        parts.push(format!("{} {hash}", f.display()));
    }
    let own = format!("refs/heads/{branch} ");
    let refs = git
        .run(&[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads",
            "refs/tags",
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    parts.extend(
        refs.lines()
            .filter(|l| !l.starts_with(&own))
            .map(str::to_owned),
    );
    sha256_hex(parts.join("\n").as_bytes())
}

/// Whether the journal holds an attempt of `feature` that started at `head`.
fn resumable(plane_dir: &Path, feature: &str, head: &str) -> bool {
    std::fs::read_dir(plane_dir.join("resume").join(feature))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| crate::resume::load(&e.path()))
        .any(|m| m.head == head)
}

/// `run --in-place --return`: saves what an interrupted run left, switches
/// back to the person's branch, removes the record. No model call.
pub fn return_checkout(plane_dir: &Path) -> Result<String, InPlaceError> {
    let Some(session) = Session::load(plane_dir) else {
        return Err(InPlaceError::new(
            "IN-PLACE-SESSION",
            "there is no in-place run to return from",
        ));
    };
    if session.is_live() {
        return Err(InPlaceError::new(
            "IN-PLACE-ACTIVE",
            format!("the in-place run (pid {}) is still running", session.pid),
        ));
    }
    let git = Git::at(&session.checkout);
    let saved = session.set_aside(
        plane_dir,
        "left by an interrupted in-place run; saved by --return",
    )?;
    let fresh = Session {
        tip: git.line(&["rev-parse", "HEAD"]).unwrap_or_default(),
        ..session.clone()
    };
    fresh.finish(plane_dir)?;
    Ok(match saved {
        Some(r) => format!(
            "back on {}; what the interrupted run left is saved as {r}",
            session.original.describe()
        ),
        None => format!("back on {}", session.original.describe()),
    })
}

/// The project's in-place session, if one is open or was left by a run
/// that died: what `status` and the TUI show.
#[must_use]
pub fn session_at(path: &Path, state: &Path) -> Option<Session> {
    let root = crate::format::open(path, None).map_or_else(
        |_| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        |o| PathBuf::from(&o.snapshot.root),
    );
    Session::load(&worktree::project_state_dir(state, &root))
}

impl Session {
    /// `in-place · speccoder/<f> in <checkout> (returns to <original>)`.
    #[must_use]
    pub fn mode_line(&self) -> String {
        format!(
            "in-place · {} in {} (returns to {})",
            self.branch,
            self.checkout.display(),
            self.original.describe()
        )
    }
}

/// A worktree-mode run of a feature that an in-place session holds: the
/// branch is checked out in the person's checkout, so it is refused.
#[must_use]
pub fn session_elsewhere(path: &Path, feature: Option<&str>, state: &Path) -> Option<InPlaceError> {
    let opened = crate::format::open(path, feature).ok()?;
    let plane_dir = worktree::project_state_dir(state, Path::new(&opened.snapshot.root));
    let session = Session::load(&plane_dir)?;
    let wanted = opened
        .snapshot
        .feature
        .as_ref()
        .map(|f| f.feature.name.as_str());
    if wanted.is_some_and(|w| w != session.feature) {
        return None;
    }
    Some(
        InPlaceError::new(
            "IN-PLACE-SESSION",
            format!(
                "{} is being run in place in {} (on {})",
                session.feature,
                session.checkout.display(),
                session.branch
            ),
        )
        .fix(&[
            "speccoder run --in-place",
            "speccoder run --in-place --return",
        ]),
    )
}

/// `run --in-place --return` for the project at `path`, with the state in `state`.
#[must_use]
pub fn return_in_place(path: &Path, state: &Path) -> CommandResult {
    let root = crate::format::open(path, None).map_or_else(
        |_| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        |o| PathBuf::from(&o.snapshot.root),
    );
    let plane_dir = worktree::project_state_dir(state, &root);
    match return_checkout(&plane_dir) {
        Ok(message) => CommandResult::new("run", speccoder_core::CommandStatus::Ok, &message),
        Err(e) => e.result("run"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_engines_tick_is_a_tick() {
        let before = b"- [ ] T001 a\n- [ ] T002 b\n";
        let mut after = before.to_vec();
        after[3] = b'x';
        assert!(is_tick(before, &after));
        let mut two = after.clone();
        two[16] = b'X';
        assert!(!is_tick(before, &two), "two ticks are not one");
        let mut other = before.to_vec();
        other[8] = b'Z';
        assert!(!is_tick(before, &other), "any other byte is not a tick");
        assert!(!is_tick(before, b"- [ ] T001 a\n"));
    }

    #[test]
    fn the_intent_digest_ignores_ticks_only() {
        assert_eq!(
            untick(b"- [x] T1\n- [X] T2\n- [ ] T3"),
            b"- [ ] T1\n- [ ] T2\n- [ ] T3"
        );
        assert_eq!(untick(b"a [xy] b"), b"a [xy] b");
    }

    /// Re-checking a 2,000-file, 20 MB protected set takes at
    /// most a second (`cargo test --release -- --ignored`).
    #[test]
    #[ignore = "a timing check: run in release"]
    fn a_large_protected_set_is_rechecked_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let chunk = "x".repeat(10 * 1024);
        for f in 0..40 {
            let feature = dir.path().join(format!("specs/{f:03}-f"));
            std::fs::create_dir_all(&feature).unwrap();
            for i in 0..50 {
                std::fs::write(feature.join(format!("checklist-{i}.md")), &chunk).unwrap();
            }
        }
        let policy = Policy::new(&[], &[]);
        let first = manifest(dir.path(), &policy);
        assert!(first.len() >= 2000, "{} files", first.len());
        let start = std::time::Instant::now();
        let again = manifest(dir.path(), &policy);
        let took = start.elapsed();
        assert!(diff(&first, &again).is_empty());
        assert!(took <= std::time::Duration::from_secs(1), "took {took:?}");
    }

    #[test]
    fn manifests_diff_by_path() {
        let a: BTreeMap<String, String> = [("s.md", "1"), ("t.md", "2")]
            .iter()
            .map(|(p, h)| ((*p).to_owned(), (*h).to_owned()))
            .collect();
        let mut b = a.clone();
        b.insert("t.md".to_owned(), "3".to_owned());
        b.remove("s.md");
        b.insert("n.md".to_owned(), "4".to_owned());
        let d = diff(&a, &b);
        assert_eq!(
            d,
            [
                ("s.md".to_owned(), "deleted".to_owned()),
                ("t.md".to_owned(), "modified".to_owned()),
                ("n.md".to_owned(), "added".to_owned())
            ]
        );
        assert!(
            runtime_state(".specify/workflows/runs/x.json") && runtime_state("a/__pycache__/b.pyc")
        );
    }
}
