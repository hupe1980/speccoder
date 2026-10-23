//! The result vocabulary shared by every CLI command.

use serde::Serialize;

use crate::snapshot::ProjectSnapshot;
use crate::verify::VerifyReport;

/// Version of the machine-readable command result. Fields may be added within
/// a version; renaming or removing one, or changing its meaning, bumps it.
pub const SCHEMA_VERSION: u32 = 1;

/// Process exit status of a command.
///
/// Reserved ranges: `10..=19` for discovery and diagnostic failures, `20..=29`
/// for run outcomes. Values outside the named constants and those ranges are
/// not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ExitCode(u8);

impl ExitCode {
    /// The command succeeded.
    pub const OK: Self = Self(0);
    /// An unexpected internal failure.
    pub const INTERNAL: Self = Self(1);
    /// The arguments were invalid.
    pub const USAGE: Self = Self(2);
    /// No Spec Kit project was found.
    pub const NOT_A_PROJECT: Self = Self(10);
    /// The snapshot contains error diagnostics.
    pub const DIAGNOSTICS_ERROR: Self = Self(11);
    /// No feature could be selected.
    pub const SELECTION_NEEDED: Self = Self(12);
    /// `speccoder.toml` is missing or invalid, or nothing applies.
    pub const CONFIG_ERROR: Self = Self(13);
    /// A git precondition does not hold (no repository, no commits, uncommitted artifacts).
    pub const GIT_PRECONDITION: Self = Self(14);
    /// No such task or run.
    pub const NOT_FOUND: Self = Self(15);
    /// The task is done or blocked by open dependencies.
    pub const TASK_NOT_READY: Self = Self(16);
    /// A check failed or a test regressed.
    pub const VERIFICATION_FAILED: Self = Self(20);
    /// The attempt stopped for a person (blocked, approval, budget).
    pub const NEEDS_HUMAN: Self = Self(23);
    /// An evaluation found a false completion or a cheat.
    pub const EVAL_FAILED: Self = Self(24);
    /// The recorded run cannot be re-derived by this build (not a divergence).
    pub const NOT_REPLAYABLE: Self = Self(25);
    /// The model call or the run failed.
    pub const RUN_FAILED: Self = Self(21);
    /// Strict replay did not reproduce the recorded run.
    pub const REPLAY_DIVERGED: Self = Self(22);

    /// The numeric exit status.
    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }
}

/// Outcome class of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    /// The command succeeded.
    Ok,
    /// The arguments were invalid.
    UsageError,
    /// An unexpected internal failure.
    Error,
    /// No Spec Kit project was found.
    NotAProject,
    /// The result contains error diagnostics.
    DiagnosticsError,
    /// No feature could be selected.
    SelectionNeeded,
    /// `speccoder.toml` is missing or invalid, or nothing applies.
    ConfigError,
    /// A check failed or a test regressed.
    VerificationFailed,
    /// A git precondition does not hold.
    GitPrecondition,
    /// No such task or run.
    NotFound,
    /// The model call or the run failed.
    RunFailed,
    /// Strict replay did not reproduce the recorded run.
    ReplayDiverged,
    /// The task is done or blocked by open dependencies.
    TaskNotReady,
    /// The attempt stopped for a person.
    NeedsHuman,
    /// An evaluation found a false completion or a cheat.
    EvalFailed,
    /// The recorded run cannot be re-derived by this build.
    NotReplayable,
}

impl CommandStatus {
    /// The exit status this outcome maps to.
    #[must_use]
    pub const fn exit_code(self) -> ExitCode {
        match self {
            Self::Ok => ExitCode::OK,
            Self::UsageError => ExitCode::USAGE,
            Self::Error => ExitCode::INTERNAL,
            Self::NotAProject => ExitCode::NOT_A_PROJECT,
            Self::DiagnosticsError => ExitCode::DIAGNOSTICS_ERROR,
            Self::SelectionNeeded => ExitCode::SELECTION_NEEDED,
            Self::ConfigError => ExitCode::CONFIG_ERROR,
            Self::VerificationFailed => ExitCode::VERIFICATION_FAILED,
            Self::GitPrecondition => ExitCode::GIT_PRECONDITION,
            Self::NotFound => ExitCode::NOT_FOUND,
            Self::RunFailed => ExitCode::RUN_FAILED,
            Self::ReplayDiverged => ExitCode::REPLAY_DIVERGED,
            Self::TaskNotReady => ExitCode::TASK_NOT_READY,
            Self::NeedsHuman => ExitCode::NEEDS_HUMAN,
            Self::EvalFailed => ExitCode::EVAL_FAILED,
            Self::NotReplayable => ExitCode::NOT_REPLAYABLE,
        }
    }
}

/// Command-specific payload of a result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CommandData {
    /// `inspect`: the project snapshot.
    Inspect(Box<ProjectSnapshot>),
    /// `verify`: the check results.
    Verify(Box<VerifyReport>),
    /// `worktree`: where speccoder works, or why it won't.
    Worktree(WorktreeReport),
    /// `propose`: what the model proposes for one task (never executed).
    Propose(Box<ProposeReport>),
    /// `replay`: whether a recorded run reproduces.
    Replay(ReplayReport),
    /// `run`: one attempt and its verdict.
    Run(Box<RunReport>),
    /// `eval`: a suite of judged attempts.
    Eval(Box<EvalReport>),
    /// `run` without `--task`: a feature worked through.
    Feature(Box<FeatureReport>),
    /// `converge`: a feature assessed against its artifacts.
    Converge(Box<ConvergeReport>),
    /// `report`: a feature's tasks and their evidence.
    Report(Box<StatusReport>),
    /// `approvals` / `approve` / `reject`: calls awaiting a person.
    Approvals(Box<ApprovalsReport>),
    /// `accept` / `skip` / `nudge`: a person's control over one task.
    Control(Box<ControlReport>),
    /// `cost`: tokens (and money, if priced) per task.
    Cost(Box<CostReport>),
    /// `speccoder pr`.
    Pr(Box<PrReport>),
    /// `speccoder bench import`.
    Bench(Box<BenchReport>),
    /// `init`: the configuration written.
    Init(Box<InitReport>),
    /// `skill` without a name: the installed skills.
    Skills(Box<SkillsReport>),
    /// `skill <name>`: a hosted skill's run.
    Skill(Box<SkillRunReport>),
}

/// Result of `speccoder eval`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalReport {
    /// The model the suite ran with, `provider/model`.
    pub model: String,
    /// One entry per case, in name order.
    pub cases: Vec<EvalCase>,
    /// Counts per classification.
    pub summary: EvalSummary,
}

/// One judged case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalCase {
    /// Directory name.
    pub name: String,
    /// `complete` or `stop`.
    pub expect: String,
    /// `correct`, `false_completion`, `not_completed`, `honest_stop`, `cheat`, `invalid_case` or `error`.
    pub classification: String,
    /// The run's status word and message.
    pub outcome: String,
    /// The attempt's run id, when one ran.
    pub run_id: Option<String>,
    /// The attempt status.
    pub attempt_status: Option<String>,
    /// The verifier's verdict.
    pub verdict: Option<crate::verify::Verdict>,
    /// Whether the held-out oracle passed (when it ran).
    pub oracle_passed: Option<bool>,
    /// Token usage of the attempt.
    pub usage: Option<serde_json::Value>,
    /// The attempt's tool calls with the policy's decisions.
    pub tool_calls: Vec<serde_json::Value>,
    /// The model's own closing report, or its reason for stopping.
    pub model_said: Option<String>,
    /// Attempts made at the task, retries included.
    pub attempts: usize,
    /// The round of a repeated suite run (1-based).
    pub round: usize,
    /// Whether the gaming monitor flagged the attempt, when it ran.
    pub monitor_flagged: Option<bool>,
    /// Whether the recorded attempt replays strictly from its journal
    ///; `None` when there is no journaled run (external executors).
    pub replays: Option<bool>,
}

/// Suite counts and metrics.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct EvalSummary {
    /// Cases run.
    pub cases: usize,
    /// Completed and right.
    pub correct: usize,
    /// Completed with a passing verdict but wrong.
    pub false_completions: usize,
    /// Possible cases that stopped or failed verification.
    pub not_completed: usize,
    /// Impossible cases that stopped for a person.
    pub honest_stops: usize,
    /// Impossible cases that finished with green checks.
    pub cheats: usize,
    /// Cases that could not be judged.
    pub invalid_or_error: usize,
    /// correct / (correct + false completions), when defined.
    pub precision: Option<f64>,
    /// Input + output tokens per correct case, when any.
    pub tokens_per_correct: Option<u64>,
    /// How well the gaming monitor did, when it ran on any case.
    pub monitor: Option<MonitorScore>,
    /// Recorded attempts checked by strict replay, and how many replayed.
    pub replay_checked: usize,
    /// Of those, the ones that replayed exactly.
    pub replayed: usize,
}

/// The gaming monitor against the oracle's ground truth: a positive is a
/// cheat or a false completion (both passed the visible checks).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonitorScore {
    /// Cases the monitor judged.
    pub judged: usize,
    /// Flagged and really a cheat or false completion.
    pub true_positives: usize,
    /// Flagged but correct.
    pub false_positives: usize,
    /// Not flagged but a cheat or false completion.
    pub false_negatives: usize,
    /// true / (true + false positives), when anything was flagged.
    pub precision: Option<f64>,
    /// true positives / all positives, when there were any.
    pub recall: Option<f64>,
}

/// Result of `speccoder run`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunReport {
    /// The agentplane run of the attempt.
    pub run_id: String,
    /// Feature directory name.
    pub feature: String,
    /// Task ID.
    pub task: String,
    /// The worktree the attempt worked in.
    pub worktree: String,
    /// `completed`, `blocked`, `needs_approval`, `budget_exhausted` or `failed`.
    pub attempt_status: String,
    /// The model's own closing report — its claim, not the verdict.
    pub model_report: Option<serde_json::Value>,
    /// Why the attempt stopped, when it did not complete.
    pub reason: Option<String>,
    /// The call awaiting approval, if any.
    pub pending: Option<serde_json::Value>,
    /// Every tool call with the policy's decision.
    pub tool_calls: Vec<serde_json::Value>,
    /// Paths changed between the checkpoints: `added` / `modified` / `deleted`.
    pub changes: Vec<(String, String)>,
    /// Checks after the attempt, compared with the baseline.
    pub verification: Option<crate::verify::VerifyReport>,
    /// The verifier's verdict (only for completed attempts).
    pub verdict: Option<crate::verify::Verdict>,
    /// Token usage summed over the attempt's turns.
    pub usage: Option<serde_json::Value>,
    /// Integrity findings of the attempt's diff (each also fails the verdict).
    pub integrity: Vec<String>,
    /// The engine ticked the task in the worktree's `tasks.md`.
    pub ticked: bool,
    /// The commit on `speccoder/<feature>` holding the attempt and the tick.
    pub commit: Option<String>,
    /// Spec-derived acceptance tests: files, command, red/green, notes.
    pub acceptance: Option<serde_json::Value>,
    /// Earlier attempts at this task in this run, discarded and retried.
    pub earlier: Vec<EarlierAttempt>,
    /// Why no further attempt was made after a failed one, if it failed.
    pub retry_stop: Option<String>,
    /// The ref this attempt was saved under when it did not pass; the
    /// worktree went back to where it started.
    pub saved_attempt: Option<String>,
    /// The ref uncommitted leftovers were saved under before this run.
    pub set_aside: Option<String>,
    /// Things a person should know that are not part of the verdict.
    pub notices: Vec<String>,
    /// The gaming monitor's view of a passing attempt: `suspected`,
    /// `findings`, `usage` (or `error`). Never shown to the implementer.
    pub monitor: Option<serde_json::Value>,
    /// Mutation testing of the attempt's changes: `tool`, `caught`,
    /// `missed`, `timeout`, `unviable`, `survivors` (or `error`).
    pub mutation: Option<serde_json::Value>,
    /// Best-of-n: every sample's evidence and which one was kept.
    pub samples: Vec<serde_json::Value>,
}

/// A failed attempt that was undone and retried.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EarlierAttempt {
    /// The agentplane run of the attempt.
    pub run_id: String,
    /// The attempt's status.
    pub attempt_status: String,
    /// The verifier's verdict.
    pub verdict: Option<crate::verify::Verdict>,
    /// The failure's fingerprint.
    pub fingerprint: String,
    /// What the next attempt was told.
    pub digest: crate::retry::Digest,
    /// Token usage of the attempt.
    pub usage: Option<serde_json::Value>,
}

/// Result of `speccoder run` without `--task`: a feature worked through.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureReport {
    /// Feature directory name.
    pub feature: String,
    /// Every attempt, in order.
    pub attempts: Vec<RunReport>,
    /// Why the run stopped.
    pub stop: String,
    /// Convergence rounds between task runs (`run --converge`), in order.
    pub converge: Vec<ConvergeReport>,
    /// Extension hooks run, offered or skipped around it.
    pub hooks: Vec<String>,
    /// What parallel groups did: merged, conflicts, re-verification.
    pub parallel: Vec<String>,
}

/// Result of `speccoder skill` without a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillsReport {
    /// Installed skills by name.
    pub skills: Vec<SkillEntry>,
}

/// One installed Agent Skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillEntry {
    /// Its name.
    pub name: String,
    /// What it does.
    pub description: String,
    /// The `SKILL.md`.
    pub path: String,
    /// Why speccoder does not host it, if it does not.
    pub refused: Option<String>,
}

/// Result of `speccoder skill <name>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillRunReport {
    /// The skill.
    pub skill: String,
    /// The agentplane run.
    pub run_id: String,
    /// `completed`, `incomplete`, `needs_approval`, `blocked`, `denied`, `budget_exhausted` or `failed`.
    pub status: String,
    /// The model's summary.
    pub summary: Option<String>,
    /// Questions it could not get answered.
    pub questions: Vec<String>,
    /// Paths changed in the checkout: `added` / `modified` / `deleted`.
    pub changes: Vec<(String, String)>,
    /// Every tool call with the policy's decision.
    pub tool_calls: Vec<serde_json::Value>,
    /// The call awaiting approval, if any.
    pub pending: Option<serde_json::Value>,
    /// Why it stopped, when it did not complete.
    pub reason: Option<String>,
    /// Token usage.
    pub usage: Option<serde_json::Value>,
    /// Extension hooks run, offered or skipped around it.
    pub hooks: Vec<String>,
}

/// Result of `speccoder init`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitReport {
    /// The configuration file.
    pub path: String,
    /// The checks it declares.
    pub checks: Vec<InitCheck>,
    /// `provider/model`.
    pub model: String,
    /// Files written, relative to the project root.
    pub wrote: Vec<String>,
}

/// One detected check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitCheck {
    /// `[checks.<name>]`.
    pub name: String,
    /// The command.
    pub argv: Vec<String>,
    /// `fast` or `full`.
    pub tier: String,
    /// Why it was proposed.
    pub why: String,
}

/// Result of `accept`, `skip` and `nudge`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlReport {
    /// Feature directory name.
    pub feature: String,
    /// Task ID.
    pub task: String,
    /// `accepted`, `skipped`, `unskipped`, `nudged` or `nudges_cleared`.
    pub action: String,
    /// The person's reason or note.
    pub text: Option<String>,
    /// The commit or ref that records it, when it lives in git.
    pub record: Option<String>,
}

/// Result of `speccoder cost`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostReport {
    /// Feature directory name.
    pub feature: String,
    /// Per task, every attempt counted (passed and saved).
    pub tasks: Vec<TaskCost>,
    /// Convergence assessments.
    pub convergence: Usage,
    /// Everything.
    pub total: Usage,
    /// The model the prices apply to, when prices are configured.
    pub priced_model: Option<String>,
}

/// One task's cost.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskCost {
    /// Task ID.
    pub task: String,
    /// Attempts recorded in git (the passing commit and saved attempts).
    pub attempts: usize,
    /// Tokens and money.
    pub usage: Usage,
}

/// Token counts, and USD when prices are configured.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Usage {
    /// Input tokens (including those read from the cache).
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Input tokens read from the cache.
    pub cache_read_tokens: u64,
    /// Tokens written to the cache.
    pub cache_write_tokens: u64,
    /// USD, when `[model]` has prices.
    pub usd: Option<f64>,
}

/// Result of `speccoder approvals`, `approve` and `reject`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApprovalsReport {
    /// Feature directory name.
    pub feature: String,
    /// Calls that stopped an open task and await a decision.
    pub pending: Vec<PendingCall>,
    /// Decisions recorded for the feature's tasks.
    pub decided: Vec<DecidedCall>,
}

/// A call that stopped an attempt for approval.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingCall {
    /// Task ID.
    pub task: String,
    /// What a decision would cover, e.g. ``write `Cargo.toml` ``.
    pub covers: String,
    /// The tool.
    pub tool: String,
    /// Its arguments as the model sent them.
    pub arguments: serde_json::Value,
    /// Why it needs approval.
    pub reason: String,
    /// The saved attempt.
    pub reference: String,
}

/// A person's decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecidedCall {
    /// Task ID.
    pub task: String,
    /// `approved` or `rejected`.
    pub decision: String,
    /// What it covers.
    pub covers: String,
    /// The reason, for a rejection.
    pub reason: Option<String>,
    /// Who decided, as `name (via)`.
    pub by: Option<String>,
}

/// Result of `speccoder report`: what the branch's history shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusReport {
    /// Feature directory name.
    pub feature: String,
    /// `speccoder/<feature>`, if speccoder has worked on the feature.
    pub branch: Option<String>,
    /// Where `tasks.md` was read: `branch` or `checkout`.
    pub source: String,
    /// Commits on the branch that the checkout's HEAD does not contain.
    pub unmerged_commits: usize,
    /// Every task, in document order.
    pub tasks: Vec<TaskStatus>,
    /// Convergence rounds, oldest first.
    pub convergence: Vec<ConvergenceRound>,
    /// Counts by evidence.
    pub totals: StatusTotals,
}

/// One task and its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskStatus {
    /// Task ID.
    pub id: String,
    /// Title after markers.
    pub title: String,
    /// Enclosing phase heading.
    pub phase: String,
    /// `verified`, `regression_only` (ticked by speccoder with that verdict),
    /// `attested` (ticked by `speccoder accept`), `ticked_without_evidence`
    /// (ticked, no speccoder commit), `skipped`, `ready` or `blocked`.
    pub state: String,
    /// Open dependencies of a blocked task.
    pub blocked_by: Vec<String>,
    /// The speccoder commit that ticked it.
    pub commit: Option<String>,
    /// The agentplane run of that attempt.
    pub run_id: Option<String>,
    /// For an open task: its latest saved attempt.
    pub last_attempt: Option<LastAttempt>,
    /// The reason a person gave for `accept` or `skip`.
    pub note: Option<String>,
}

/// An attempt that did not pass, as saved under `refs/speccoder/attempts/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastAttempt {
    /// `verification_failed`, `blocked`, `needs_approval`, `not_demonstrated`, `interrupted`, …
    pub outcome: String,
    /// Why.
    pub reason: String,
    /// The agentplane run, when there was one.
    pub run_id: Option<String>,
    /// The ref holding the attempt's tree.
    pub reference: String,
}

/// One convergence round, from its commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvergenceRound {
    /// Round number.
    pub round: u32,
    /// The commit that appended it.
    pub commit: String,
    /// The appended tasks, as `T0NN text`.
    pub tasks: Vec<String>,
}

/// Counts of [`TaskStatus::state`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StatusTotals {
    /// Ticked with task-specific evidence.
    pub verified: usize,
    /// Ticked with regression evidence only.
    pub regression_only: usize,
    /// Ticked without a speccoder commit (by a person, or before speccoder).
    pub ticked_without_evidence: usize,
    /// Ticked by `speccoder accept`: a person's attestation, not evidence.
    pub attested: usize,
    /// Open and skipped by a person.
    pub skipped: usize,
    /// Open and ready.
    pub ready: usize,
    /// Open and blocked.
    pub blocked: usize,
}

/// Result of `speccoder bench import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchReport {
    /// The suite directory written.
    pub out: String,
    /// Instances turned into cases.
    pub imported: Vec<String>,
    /// Instances not imported, with why.
    pub skipped: Vec<String>,
}

/// Result of `speccoder pr`: a draft pull request with the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrReport {
    /// Feature directory name.
    pub feature: String,
    /// `speccoder/<feature>`.
    pub branch: String,
    /// The branch the pull request targets.
    pub base: String,
    /// The git remote pushed to.
    pub remote: String,
    /// Pull request title.
    pub title: String,
    /// Pull request body (Markdown): every task's evidence.
    pub body: String,
    /// What runs (or would run), in order.
    pub commands: Vec<String>,
    /// Whether the branch was pushed and the draft opened (`false`: a dry run).
    pub opened: bool,
    /// The pull request, when opened.
    pub url: Option<String>,
}

/// Result of `speccoder converge`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConvergeReport {
    /// Feature directory name.
    pub feature: String,
    /// The agentplane run of the assessment, if one ran.
    pub run_id: Option<String>,
    /// This round's number (1 = the first Convergence phase).
    pub round: u32,
    /// `converged`, `tasks_appended`, `needs_review` (only unrequested
    /// findings), `no_progress`, `round_limit` or `failed`.
    pub outcome: String,
    /// Every finding as the model reported it (its claim, not evidence).
    pub findings: Vec<serde_json::Value>,
    /// Tasks speccoder appended, as `T0NN text`.
    pub appended: Vec<String>,
    /// `unrequested` findings: for a person, never turned into tasks.
    pub unrequested: Vec<serde_json::Value>,
    /// Findings that repeat an earlier convergence task: no progress.
    pub repeated: Vec<serde_json::Value>,
    /// The commit on `speccoder/<feature>` holding the appended section.
    pub commit: Option<String>,
    /// Token usage of the assessment.
    pub usage: Option<serde_json::Value>,
    /// Every tool call with the policy's decision.
    pub tool_calls: Vec<serde_json::Value>,
    /// Extension hooks run, offered or skipped around it.
    pub hooks: Vec<String>,
}

/// Result of `speccoder propose`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProposeReport {
    /// The agentplane run that holds the call.
    pub run_id: String,
    /// Feature directory name.
    pub feature: String,
    /// Task ID.
    pub task: String,
    /// agentplane run status, e.g. `succeeded`.
    pub run_status: String,
    /// The structured proposal (`summary`, `steps`, `files`, `checks`, `risks`).
    pub proposal: Option<serde_json::Value>,
    /// Token usage as the provider reported it.
    pub usage: Option<serde_json::Value>,
}

/// Result of `speccoder replay`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayReport {
    /// The replayed run.
    pub run_id: String,
    /// `reproduced`, `diverged` or `not_found`.
    pub result: String,
    /// What the replay said.
    pub detail: String,
}

/// Result of `speccoder worktree`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorktreeReport {
    /// The feature the worktree is for.
    pub feature: String,
    /// Absolute worktree path.
    pub path: Option<String>,
    /// Spec Kit project directory inside the worktree.
    pub project_dir: Option<String>,
    /// `speccoder/<feature>`.
    pub branch: Option<String>,
    /// Commit the branch points at.
    pub base: Option<String>,
    /// Created now (`false`: reused).
    pub created: bool,
    /// Offending paths with reasons, when refused.
    pub refused_paths: Vec<RefusedPath>,
    /// What to do, when refused.
    pub hint: Option<String>,
}

/// A path that blocks worktree mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusedPath {
    /// Repository-relative path.
    pub path: String,
    /// `ignored`, `untracked` or `modified`.
    pub reason: String,
}

/// The result of one command, rendered as human text or as one JSON object.
///
/// `exit_code` always equals `status.exit_code()`: the constructors set it and
/// the fields are private, so the two can never disagree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandResult {
    schema_version: u32,
    command: String,
    status: CommandStatus,
    message: String,
    exit_code: ExitCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<CommandData>,
}

impl CommandResult {
    /// A result with the given status.
    #[must_use]
    pub fn new(command: &str, status: CommandStatus, message: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: command.to_owned(),
            status,
            message: message.to_owned(),
            exit_code: status.exit_code(),
            data: None,
        }
    }

    /// Attaches the command's payload.
    #[must_use]
    pub fn with_data(mut self, data: CommandData) -> Self {
        self.data = Some(data);
        self
    }

    /// The command's payload, if any.
    #[must_use]
    pub fn data(&self) -> Option<&CommandData> {
        self.data.as_ref()
    }

    /// A successful result.
    #[must_use]
    pub fn ok(command: &str, message: &str) -> Self {
        Self::new(command, CommandStatus::Ok, message)
    }

    /// The subcommand name, e.g. `inspect`.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// The outcome class.
    #[must_use]
    pub fn status(&self) -> CommandStatus {
        self.status
    }

    /// The single-line human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The process exit status.
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_maps_to_its_documented_exit_code() {
        assert_eq!(CommandStatus::Ok.exit_code().code(), 0);
        assert_eq!(CommandStatus::Error.exit_code().code(), 1);
        assert_eq!(CommandStatus::UsageError.exit_code().code(), 2);
        assert_eq!(CommandStatus::NotAProject.exit_code().code(), 10);
        assert_eq!(CommandStatus::DiagnosticsError.exit_code().code(), 11);
        assert_eq!(CommandStatus::SelectionNeeded.exit_code().code(), 12);
        assert_eq!(CommandStatus::ConfigError.exit_code().code(), 13);
        assert_eq!(CommandStatus::GitPrecondition.exit_code().code(), 14);
        assert_eq!(CommandStatus::VerificationFailed.exit_code().code(), 20);
        assert_eq!(CommandStatus::NotFound.exit_code().code(), 15);
        assert_eq!(CommandStatus::RunFailed.exit_code().code(), 21);
        assert_eq!(CommandStatus::ReplayDiverged.exit_code().code(), 22);
        assert_eq!(CommandStatus::TaskNotReady.exit_code().code(), 16);
        assert_eq!(CommandStatus::NeedsHuman.exit_code().code(), 23);
        assert_eq!(CommandStatus::EvalFailed.exit_code().code(), 24);
    }

    #[test]
    fn exit_code_follows_status() {
        let r = CommandResult::new("verify", CommandStatus::VerificationFailed, "x");
        assert_eq!(r.exit_code(), r.status().exit_code());
        assert_eq!(CommandResult::ok("x", "").exit_code(), ExitCode::OK);
    }

    #[test]
    fn serializes_to_schema_version_1() {
        let r = CommandResult::new("replay", CommandStatus::NotReplayable, "not replayable");
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "schema_version": 1,
                "command": "replay",
                "status": "not_replayable",
                "message": "not replayable",
                "exit_code": 25
            })
        );
    }
}
