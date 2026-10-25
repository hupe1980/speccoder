//! The result vocabulary shared by every CLI command.

use serde::Serialize;

use crate::snapshot::ProjectSnapshot;
use crate::verify::VerifyReport;

/// Version of the machine-readable command result. Fields may be added within
/// a version; renaming or removing one, or changing its meaning, bumps it.
pub const SCHEMA_VERSION: u32 = 2;

/// Process exit status of a command.
///
/// Reserved ranges: `10..=19` for discovery and diagnostic failures, `20..=29`
/// for run outcomes. Values outside the named constants and those ranges are
/// not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
    /// `attest verify` found an attestation that does not hold.
    pub const ATTESTATION_FAILED: Self = Self(26);
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
    /// An attestation is missing, unsigned where required, or does not hold.
    AttestationFailed,
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
            Self::AttestationFailed => ExitCode::ATTESTATION_FAILED,
        }
    }
}

/// Command-specific payload of a result.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// `run` stopped by unfinished checklists.
    Checklists(Vec<ChecklistStatus>),
    /// `spec archive`: an OpenSpec change merged into the main specs.
    Archive(Box<ArchiveReport>),
    /// An in-place run refused or stopped (`DIRTY-CHECKOUT`, `INTENT-CHANGED`, …).
    InPlace(Box<InPlaceReport>),
    /// `attest verify` / `attest sign`: what each task's attestation says.
    Attest(Box<AttestReport>),
    /// `explain`: catalog entries.
    Explain(Box<ExplainReport>),
    /// `schema`: the published schemas, or one of them.
    Schema(Box<SchemaReport>),
    /// `doctor`: every check of the machine and the project.
    Doctor(Box<DoctorReport>),
    /// `feature`: the project's features and which one is current.
    Features(Box<FeaturesReport>),
}

/// One feature (or change) of the project, as `speccoder feature` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FeatureEntry {
    /// 1-based position, the number `speccoder feature <n>` takes.
    pub number: usize,
    /// Directory name, e.g. `001-greet`.
    pub name: String,
    /// Repository-relative directory.
    pub dir: String,
    /// The current one.
    pub current: bool,
    /// Ticked tasks.
    pub done: usize,
    /// All tasks (0: no task list yet).
    pub tasks: usize,
    /// Which planning artifacts exist: `spec`, `plan`, `tasks`.
    pub artifacts: Vec<String>,
}

/// Result of `speccoder feature`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FeaturesReport {
    /// `feature` or `change`.
    pub unit: String,
    /// The current one's name.
    pub current: Option<String>,
    /// How it is current: `argument`, `environment`, `recorded`,
    /// `only_feature` or `none`.
    pub reason: String,
    /// The same in words (`recorded in .specify/feature.json`).
    pub reason_text: String,
    /// What this call did: `listed`, `switched` or `cleared`.
    pub action: String,
    /// Every feature, in order.
    pub features: Vec<FeatureEntry>,
}

/// How a doctor check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoctorLevel {
    /// Fine.
    Pass,
    /// Works, with less than it could (no signing key, no spec tool).
    Warn,
    /// A run would fail or judge wrongly.
    Fail,
}

/// One doctor check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct DoctorCheck {
    /// `git`, `repository`, `project`, `config`, `key`, `sandbox`, …
    pub check: String,
    /// Pass, warn or fail.
    pub level: DoctorLevel,
    /// What was found.
    pub detail: String,
    /// The catalog code of a failure or warning.
    pub code: Option<String>,
    /// How to resolve it.
    pub fix: Vec<String>,
}

/// Result of `speccoder doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct DoctorReport {
    /// speccoder's version.
    pub speccoder: String,
    /// `macos aarch64`.
    pub os: String,
    /// Every check, in order.
    pub checks: Vec<DoctorCheck>,
}

impl CommandData {
    /// The payload's schema name (`run`, `feature`, `in-place`, …): the
    /// envelope's `payload`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Inspect(_) => "inspect",
            Self::Verify(_) => "verify",
            Self::Worktree(_) => "worktree",
            Self::Propose(_) => "propose",
            Self::Replay(_) => "replay",
            Self::Run(_) => "run",
            Self::Eval(_) => "eval",
            Self::Feature(_) => "feature",
            Self::Converge(_) => "converge",
            Self::Report(_) => "report",
            Self::Approvals(_) => "approvals",
            Self::Control(_) => "control",
            Self::Cost(_) => "cost",
            Self::Pr(_) => "pr",
            Self::Bench(_) => "bench",
            Self::Init(_) => "init",
            Self::Skills(_) => "skills",
            Self::Skill(_) => "skill",
            Self::Checklists(_) => "checklists",
            Self::Archive(_) => "archive",
            Self::InPlace(_) => "in-place",
            Self::Attest(_) => "attest",
            Self::Explain(_) => "explain",
            Self::Schema(_) => "schema",
            Self::Doctor(_) => "doctor",
            Self::Features(_) => "features",
        }
    }
}

/// Every payload's schema name, in [`CommandData`]'s order.
pub const PAYLOADS: &[&str] = &[
    "inspect",
    "verify",
    "worktree",
    "propose",
    "replay",
    "run",
    "eval",
    "feature",
    "converge",
    "report",
    "approvals",
    "control",
    "cost",
    "pr",
    "bench",
    "init",
    "skills",
    "skill",
    "checklists",
    "archive",
    "in-place",
    "attest",
    "explain",
    "schema",
    "doctor",
    "features",
];

/// A repository instructions file given to an attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InstructionFile {
    /// Repository-relative path.
    pub path: String,
    /// sha256 of the file.
    pub sha256: String,
    /// Its size.
    pub bytes: usize,
    /// Cut to fit the instructions budget.
    pub cut: bool,
}

/// One code, as `explain` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct CodeInfo {
    /// `NEEDS-APPROVAL`.
    pub code: String,
    /// Error, stop or warning.
    pub kind: crate::codes::Kind,
    /// The status results with this code have.
    pub status: CommandStatus,
    /// Its exit code.
    pub exit_code: u8,
    /// One line.
    pub title: String,
    /// What it means and why it happens.
    pub explain: String,
    /// How to resolve it (templates: `<task>`, `<feature>` are blanks).
    pub fix: Vec<String>,
    /// Its section on the codes page.
    pub docs: String,
}

impl CodeInfo {
    /// The view of a catalog entry.
    #[must_use]
    pub fn of(e: &crate::codes::Entry) -> Self {
        Self {
            code: e.name.to_owned(),
            kind: e.kind,
            status: e.status,
            exit_code: e.status.exit_code().code(),
            title: e.title.to_owned(),
            explain: e.explain.to_owned(),
            fix: crate::codes::fixes(e, "<command>", None, None),
            docs: e.docs(),
        }
    }
}

/// Result of `speccoder explain`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ExplainReport {
    /// The code asked for, or every code with `--list`.
    pub codes: Vec<CodeInfo>,
}

/// One published schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SchemaName {
    /// `result`, `event`, `run`, …
    pub name: String,
    /// What it describes.
    pub describes: String,
    /// Where it is published.
    pub url: String,
}

/// Result of `speccoder schema`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct SchemaReport {
    /// Every schema speccoder publishes.
    pub names: Vec<SchemaName>,
    /// The schema asked for.
    pub schema: Option<serde_json::Value>,
}

/// Why an in-place run refused or stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InPlaceReport {
    /// The paths involved, with why (the code and the fixes are the
    /// envelope's).
    pub paths: Vec<RefusedPath>,
}

/// Result of `speccoder spec archive` (an OpenSpec change).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ArchiveReport {
    /// The change.
    pub change: String,
    /// `probe`, `incomplete`, `unverified`, `awaiting_confirmation`,
    /// `declined`, `archived` or `failed`.
    pub stage: String,
    /// OpenSpec's own `{severity, code, message, fix}` from its last answer.
    pub openspec_status: Option<serde_json::Value>,
    /// Tasks ticked without a speccoder verdict commit.
    pub unverified_tasks: Vec<String>,
    /// What archiving changes (`openspec show <id> --diff`), when shown.
    pub diff: Option<String>,
    /// Where OpenSpec moved the change.
    pub archived_as: Option<String>,
    /// OpenSpec's counts of added, modified, removed and renamed requirements.
    pub totals: Option<serde_json::Value>,
    /// The commit `archive <id>`.
    pub commit: Option<String>,
    /// Who confirmed.
    pub confirmed_by: Option<String>,
}

/// One Spec Kit checklist (`specs/<feature>/checklists/*.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ChecklistStatus {
    /// Path relative to the project root.
    pub file: String,
    /// Checklist items.
    pub total: usize,
    /// Items checked.
    pub done: usize,
    /// Items not checked.
    pub open: usize,
}

/// Result of `speccoder eval`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct EvalReport {
    /// The model the suite ran with, `provider/model`.
    pub model: String,
    /// One entry per case, in name order.
    pub cases: Vec<EvalCase>,
    /// Counts per classification.
    pub summary: EvalSummary,
}

/// One judged case.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// The held-out tests' part (`acceptance.held_out` of the run report).
    pub held_out: Option<serde_json::Value>,
    /// An attempt passed the visible checks and failed the held-out tests.
    pub gap: bool,
    /// The combined acceptance strength, when measured (F-060).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f64>,
    /// Stopped because a test rejected a permitted alternative.
    #[serde(default)]
    pub over_strict: bool,
}

/// Suite counts and metrics.
#[derive(Debug, Clone, Default, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// `expect = "reject"` cases that were not ticked.
    pub caught: usize,
    /// Attempts that passed visibly, and how many of them the held-out tests
    /// failed (F-059); `None` when no held-out test ran.
    pub held_out_gap: Option<HeldOutGap>,
    /// Ticked cases whose held-out tests passed, against the oracle.
    pub held_out_oracle: Option<HeldOutOracle>,
    /// The median combined acceptance strength over verified cases that
    /// measured it (F-060); `None`: not measured.
    pub median_acceptance_strength: Option<f64>,
    /// Cases stopped for a person because a test rejected a permitted
    /// alternative.
    pub over_strict_stops: usize,
}

/// Visible passes the held-out tests caught.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct HeldOutGap {
    /// Attempts the held-out tests judged (they had passed visibly).
    pub visible_passed: usize,
    /// Of those, the ones the held-out tests failed.
    pub held_out_failed: usize,
    /// `held_out_failed / visible_passed`.
    pub rate: f64,
}

/// Held-out passes the hidden oracle agreed with, or not.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct HeldOutOracle {
    /// The oracle passed too.
    pub agree: usize,
    /// The oracle failed: the held-out tests missed something.
    pub disagree: usize,
}

/// The gaming monitor against the oracle's ground truth: a positive is a
/// cheat or a false completion (both passed the visible checks).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// Changes outside the paths the task names: reported, never gated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::scope::ScopeFinding>,
    /// What the tick's attestation came to: `{ state: signed | unsigned |
    /// missing, signer, reason }` (F-061).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<serde_json::Value>,
    /// The repository's instructions the attempt was given (`AGENTS.md`,
    /// F-073), root first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<InstructionFile>,
    /// The acceptance set's strength (F-060): after a `verified` verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<crate::strength::AcceptanceStrength>,
}

/// A failed attempt that was undone and retried.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// The `full` check tier, run once after the run ticked the last task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_tier: Option<FullTier>,
}

/// The `full` check tier at the end of a feature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FullTier {
    /// The checks that ran.
    pub ran: Vec<String>,
    /// Every check passed (or none applied).
    pub passed: bool,
    /// The checks that failed.
    pub failed: Vec<String>,
    /// Why nothing ran, or the record could not be written.
    pub note: Option<String>,
}

/// Result of `speccoder spec` without a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SkillsReport {
    /// Installed skills by name.
    pub skills: Vec<SkillEntry>,
}

/// One installed Agent Skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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

/// Result of `speccoder spec <name>`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// The unit the skill worked on, or created and made current, and how
    /// it is known (`001-x (recorded in .specify/feature.json)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// Result of `speccoder init`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InitReport {
    /// The configuration file.
    pub path: String,
    /// The checks it declares.
    pub checks: Vec<InitCheck>,
    /// `provider/model`.
    pub model: String,
    /// Files written, relative to the project root.
    pub wrote: Vec<String>,
    /// The checks run on the untouched code (empty: not run).
    pub ran: Vec<InitRun>,
    /// The repository instructions found (`AGENTS.md`, `CLAUDE.md`).
    pub instructions: Vec<String>,
    /// The model key: the variable and whether it is set.
    pub key: InitKey,
    /// The commit holding `speccoder.toml`, when one was made.
    pub committed: Option<String>,
}

/// One check `init` ran on the untouched code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InitRun {
    /// `[checks.<name>]`.
    pub name: String,
    /// Passed on the untouched code.
    pub passed: bool,
    /// `passed`, `failed`, `timed_out`, `skipped`, `error`.
    pub status: String,
    /// How long it took.
    pub duration_ms: u64,
}

/// The model key `init` looked for.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, schemars::JsonSchema)]
pub struct InitKey {
    /// `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`.
    pub variable: String,
    /// Set in this environment.
    pub set: bool,
}

/// One detected check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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

/// Result of `speccoder report --cost`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TaskCost {
    /// Task ID.
    pub task: String,
    /// Attempts recorded in git (the passing commit and saved attempts).
    pub attempts: usize,
    /// Tokens and money.
    pub usage: Usage,
}

/// Token counts, and USD when prices are configured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, schemars::JsonSchema)]
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

/// Result of `speccoder report --approvals`, `approve` and `reject`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ApprovalsReport {
    /// Feature directory name.
    pub feature: String,
    /// Calls that stopped an open task and await a decision.
    pub pending: Vec<PendingCall>,
    /// Decisions recorded for the feature's tasks.
    pub decided: Vec<DecidedCall>,
}

/// A call that stopped an attempt for approval, or a question, waiting for
/// a person.
#[derive(Debug, Clone, Default, PartialEq, Serialize, schemars::JsonSchema)]
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
    /// The call as the inline prompt shows it (F-054).
    pub presentation: Option<ApprovalView>,
    /// The waiting call's id (`approve --call`); empty for a stopped attempt.
    pub call: String,
    /// `task` (the attempt waits and resumes) or `attempt` (stopped and saved:
    /// a lane, or before F-072).
    pub source: String,
    /// `approval` or `question`.
    pub kind: String,
    /// A question's text.
    pub question: Option<String>,
    /// Seconds since the epoch the call has waited since.
    pub waiting_since: Option<u64>,
    /// Seconds since the epoch it is denied as expired at.
    pub deadline: Option<u64>,
}

/// Who decided, and how speccoder knows the name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct OperatorView {
    /// The name.
    pub actor: String,
    /// `asserted` (typed at a local terminal or the CLI) or `connected`
    /// (reported by a client over its connection, e.g. an editor).
    pub basis: String,
}

/// A pending call as a person sees it: what, why, what it can reach, what
/// the model claimed, and what approving it grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ApprovalView {
    /// `write`, `edit`, `run`, `network` or `mcp`.
    pub kind: String,
    /// `Edit this file?`, `Run this command?`, ….
    pub title: String,
    /// The path, the whole command, or the MCP tool.
    pub subject: String,
    /// The diff, the file's head, the command or the arguments.
    pub body_lines: Vec<String>,
    /// What the call can reach.
    pub sandbox: String,
    /// What the model said when it asked (a claim).
    pub claim: Option<String>,
    /// `low`, `medium` or `high`.
    pub risk: String,
    /// What `approve` grants: `for the rest of T004: this path, any content`.
    pub scope: String,
}

/// A person's decision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
    /// How far an approval reaches: `once`, `task` or `run`.
    pub scope: Option<String>,
    /// An approval's note to the model.
    pub note: Option<String>,
    /// The waiting call it decided, when it was one.
    pub call: Option<String>,
    /// Who decided; `None` for an expiry.
    pub operator: Option<OperatorView>,
    /// Through what: `terminal`, `cli` or `editor <name>`.
    pub via: Option<String>,
    /// Nobody decided: the deadline passed and the call was denied.
    pub expired: bool,
}

/// What `attest verify` found for one task object (F-061).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyOutcome {
    /// Every line parses, every signature verifies against the trust input,
    /// and every statement agrees with the commit.
    Ok,
    /// The statements hold but are not signed.
    Unsigned,
    /// No speccoder attestation on the object.
    Missing,
    /// A signature does not verify.
    BadSignature,
    /// A signature verifies, but its key is not trusted by the verifier.
    UntrustedSigner,
    /// A statement disagrees with the commit, its trailers, the spec or the bundle.
    Mismatch,
    /// A line does not parse.
    Malformed,
}

impl VerifyOutcome {
    /// How grave: when several apply, the gravest is the outcome
    /// (`malformed` > `bad_signature` > `mismatch` > `untrusted_signer` >
    /// `unsigned` > `ok`; `missing` stands alone).
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Unsigned => 1,
            Self::UntrustedSigner => 2,
            Self::Mismatch => 3,
            Self::BadSignature => 4,
            Self::Malformed => 5,
            Self::Missing => 6,
        }
    }

    /// The word reports use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unsigned => "unsigned",
            Self::Missing => "missing",
            Self::BadSignature => "bad_signature",
            Self::UntrustedSigner => "untrusted_signer",
            Self::Mismatch => "mismatch",
            Self::Malformed => "malformed",
        }
    }
}

/// Why an object got its outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct VerifyReason {
    /// A stable code, e.g. `subject`, `trailer`, `line_hash`, `signature`.
    pub code: String,
    /// The field that disagrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// What it should be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// What it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
}

/// Who signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SignerView {
    /// `ssh`, `openpgp` or `sigstore`.
    pub kind: String,
    /// The key's fingerprint (`SHA256:…`, an OpenPGP fingerprint).
    pub keyid: String,
    /// The principal the verifier's trust input names for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
}

/// One task object's verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ObjectVerification {
    /// The commit (or skip record) id.
    pub object: String,
    /// `task` or `skip`.
    pub kind: String,
    /// Task ID.
    pub task: String,
    /// The verdict the statements claim.
    pub verdict: Option<String>,
    /// The outcome.
    pub outcome: VerifyOutcome,
    /// Every reason found.
    pub reasons: Vec<VerifyReason>,
    /// Who signed, when signed.
    pub signer: Option<SignerView>,
    /// How many statements the bundle holds.
    pub statements: usize,
    /// For a missing attestation: an attested commit with the same tree.
    pub same_tree_as: Option<String>,
}

/// Result of `speccoder attest verify` and `attest sign`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct AttestReport {
    /// Feature directory name.
    pub feature: String,
    /// Where trust came from: the allowed-signers file, whether GPG's keyring
    /// was used, the expected Sigstore identity.
    pub trust: serde_json::Value,
    /// One per object.
    pub objects: Vec<ObjectVerification>,
    /// Count per outcome word.
    pub totals: std::collections::BTreeMap<String, usize>,
}

/// One row of the requirement → scenario → test → verdict matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TraceRow {
    /// `FR-001`, `SC-002`.
    pub requirement: String,
    /// `US1.2`: story 1, acceptance scenario 2.
    pub scenario: Option<String>,
    /// The acceptance test.
    pub test: Option<String>,
    /// The task that cites it.
    pub task: Option<String>,
    /// That task's verdict.
    pub verdict: Option<String>,
}

/// Result of `speccoder report`: what the branch's history shows.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct StatusReport {
    /// Feature directory name.
    pub feature: String,
    /// `speccoder/<feature>`, if speccoder has worked on the feature.
    pub branch: Option<String>,
    /// Where `tasks.md` was read: `branch` or `checkout`.
    pub source: String,
    /// How the feature was chosen (`recorded in .specify/feature.json`,
    /// `the only feature`, `named with --feature`).
    #[serde(default)]
    pub selected_by: String,
    /// The planning artifacts that exist (`spec`, `plan`, `tasks`).
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// While the feature has no task list: the planning skills that come
    /// next (`plan`, `tasks`); empty once it has one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planning: Vec<String>,
    /// Commits on the branch that the checkout's HEAD does not contain.
    pub unmerged_commits: usize,
    /// Every task, in document order.
    pub tasks: Vec<TaskStatus>,
    /// Convergence rounds, oldest first.
    pub convergence: Vec<ConvergenceRound>,
    /// Requirement → task → verdict, in the spec's order; a requirement no
    /// task cites is a row without a task.
    pub trace: Vec<TraceRow>,
    /// Counts by evidence.
    pub totals: StatusTotals,
}

/// One task and its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TaskStatus {
    /// Task ID.
    pub id: String,
    /// Title after markers.
    pub title: String,
    /// Enclosing phase heading.
    pub phase: String,
    /// `verified`, `regression_only` (ticked by speccoder with that verdict),
    /// `attested` (ticked by `speccoder task accept`), `ticked_without_evidence`
    /// (ticked, no speccoder commit), `evidence_without_tick` (in place: a
    /// verdict commit names the task, its local tick is missing),
    /// `skipped`, `ready` or `blocked`.
    pub state: String,
    /// In place: the spec, plan or constitution changed since this task's
    /// evidence was committed (its `Intent-Digest` differs from now).
    pub intent_changed: bool,
    /// For a task speccoder committed: its attestation as the note says,
    /// signatures unchecked — `signed <kind> <keyid>`, `unsigned` or
    /// `missing` (F-061; `attest verify` checks them).
    pub attestation: Option<String>,
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
    /// A verified tick's acceptance strength as its trailer says
    /// (`0.83 mutants=9/11 near-miss=1/2`, `not measured (…)`; F-060).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<String>,
}

/// An attempt that did not pass, as saved under `refs/speccoder/attempts/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ConvergenceRound {
    /// Round number.
    pub round: u32,
    /// The commit that appended it.
    pub commit: String,
    /// The appended tasks, as `T0NN text`.
    pub tasks: Vec<String>,
}

/// Counts of [`TaskStatus::state`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct StatusTotals {
    /// Ticked with task-specific evidence.
    pub verified: usize,
    /// Ticked with regression evidence only.
    pub regression_only: usize,
    /// Ticked without a speccoder commit (by a person, or before speccoder).
    pub ticked_without_evidence: usize,
    /// In place: committed by speccoder, the local tick missing.
    pub evidence_without_tick: usize,
    /// Ticks speccoder committed without an attestation.
    pub without_attestation: usize,
    /// Ticked by `speccoder task accept`: a person's attestation, not evidence.
    pub attested: usize,
    /// Open and skipped by a person.
    pub skipped: usize,
    /// Open and ready.
    pub ready: usize,
    /// Open and blocked.
    pub blocked: usize,
}

/// Result of `speccoder bench import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct BenchReport {
    /// The suite directory written.
    pub out: String,
    /// Instances turned into cases.
    pub imported: Vec<String>,
    /// Instances not imported, with why.
    pub skipped: Vec<String>,
}

/// Result of `speccoder pr`: a draft pull request with the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReplayReport {
    /// The replayed run.
    pub run_id: String,
    /// `reproduced`, `diverged` or `not_found`.
    pub result: String,
    /// What the replay said.
    pub detail: String,
}

/// Result of `speccoder worktree`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
}

/// A path that blocks worktree mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct CommandResult {
    schema_version: u32,
    command: String,
    status: CommandStatus,
    message: String,
    exit_code: ExitCode,
    data: Option<CommandData>,
    next: Vec<crate::next::NextStep>,
    code: Option<&'static crate::codes::Entry>,
    fix: Vec<String>,
}

/// Envelope v2: `next`, `code`, `fix` and `docs` beside the typed `data`.
impl Serialize for CommandResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("schema_version", &self.schema_version)?;
        map.serialize_entry("command", &self.command)?;
        map.serialize_entry("status", &self.status)?;
        map.serialize_entry("exit_code", &self.exit_code)?;
        map.serialize_entry("message", &self.message)?;
        if let Some(code) = self.code() {
            map.serialize_entry("code", code.name)?;
            map.serialize_entry("fix", &self.fix())?;
            map.serialize_entry("docs", &code.docs())?;
        }
        if !self.next.is_empty() {
            map.serialize_entry("next", &self.next)?;
        }
        if let Some(data) = &self.data {
            map.serialize_entry("payload", data.kind())?;
            map.serialize_entry("data", data)?;
        }
        map.end()
    }
}

/// The envelope as it is serialised — the schema of [`CommandResult`]
/// (whose fields are private and whose serialisation is written by hand).
#[derive(schemars::JsonSchema)]
#[allow(dead_code)] // schema only
struct Envelope {
    /// The envelope's version.
    schema_version: u32,
    /// The command.
    command: String,
    /// The outcome class.
    status: CommandStatus,
    /// The process exit status (always the status's).
    exit_code: ExitCode,
    /// One line, for a person.
    message: String,
    /// The catalog code: present for every non-ok result and for a
    /// degraded success.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    /// How to resolve it: commands, or `→ ` lines for a person.
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<Vec<String>>,
    /// The code's section on the codes page.
    #[serde(skip_serializing_if = "Option::is_none")]
    docs: Option<String>,
    /// The steps that continue.
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<Vec<crate::next::NextStep>>,
    /// The name of `data`'s schema (`schemas/v2/<payload>.json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<String>,
    /// The command's payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<CommandData>,
}

impl schemars::JsonSchema for CommandResult {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CommandResult".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        Envelope::json_schema(generator)
    }
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
            next: Vec::new(),
            code: None,
            fix: Vec::new(),
        }
    }

    /// `from`'s outcome reported as `command`'s: status, message and code,
    /// without `from`'s payload (a command passing on a failure it met).
    #[must_use]
    pub fn carried(command: &str, from: &Self) -> Self {
        let mut out = Self::new(command, from.status, &from.message);
        out.code = from.code;
        out.fix.clone_from(&from.fix);
        out
    }

    /// The same result with another status and message: payload and next
    /// steps kept, the code kept only while the status stays the same.
    #[must_use]
    pub fn reworded(mut self, status: CommandStatus, message: &str) -> Self {
        if status != self.status {
            self.code = None;
            self.fix.clear();
        }
        self.status = status;
        self.exit_code = status.exit_code();
        message.clone_into(&mut self.message);
        self
    }

    /// Names the code (a catalog name); its fixes become the result's
    /// unless [`Self::with_fix`] gives better ones. An unknown name is
    /// ignored, and the status's default code applies later.
    #[must_use]
    pub fn with_code(mut self, name: &str) -> Self {
        if let Some(entry) = crate::codes::lookup(name) {
            self.code = Some(entry);
        }
        self
    }

    /// The fix lines, replacing the catalog's.
    #[must_use]
    pub fn with_fix(mut self, fix: Vec<String>) -> Self {
        self.fix = fix;
        self
    }

    /// The code: the one attached, else the status's default (none for a
    /// plain `ok`) — no non-ok result is without one.
    #[must_use]
    pub fn code(&self) -> Option<&'static crate::codes::Entry> {
        self.code.or_else(|| crate::codes::default_for(self.status))
    }

    /// Whether a code was named explicitly (not the status's default).
    #[must_use]
    pub fn has_explicit_code(&self) -> bool {
        self.code.is_some()
    }

    /// How to resolve it: the fixes given, else the code's own.
    #[must_use]
    pub fn fix(&self) -> Vec<String> {
        match self.code() {
            Some(_) if !self.fix.is_empty() => self.fix.clone(),
            Some(entry) => crate::codes::fixes(entry, &self.command, None, None),
            None => Vec::new(),
        }
    }

    /// Attaches what to do next (`data.next`, the human `Next:` block).
    #[must_use]
    pub fn with_next(mut self, next: Vec<crate::next::NextStep>) -> Self {
        self.next = next;
        self
    }

    /// What to do next.
    #[must_use]
    pub fn next(&self) -> &[crate::next::NextStep] {
        &self.next
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
        assert_eq!(CommandStatus::NotReplayable.exit_code().code(), 25);
        assert_eq!(CommandStatus::AttestationFailed.exit_code().code(), 26);
    }

    #[test]
    fn exit_code_follows_status() {
        let r = CommandResult::new("verify", CommandStatus::VerificationFailed, "x");
        assert_eq!(r.exit_code(), r.status().exit_code());
        assert_eq!(CommandResult::ok("x", "").exit_code(), ExitCode::OK);
    }

    #[test]
    fn serializes_to_schema_version_2() {
        let r = CommandResult::new("replay", CommandStatus::NotReplayable, "not replayable")
            .with_code("NOT-REPLAYABLE")
            .with_fix(vec!["x".to_owned()]);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "schema_version": 2,
                "command": "replay",
                "status": "not_replayable",
                "exit_code": 25,
                "message": "not replayable",
                "code": "NOT-REPLAYABLE",
                "fix": ["x"],
                "docs": "https://hupe1980.github.io/speccoder/docs/codes/#not-replayable"
            })
        );
        let with_next = CommandResult::new("run", CommandStatus::NeedsHuman, "x")
            .with_next(crate::next::next(&crate::next::Outcome::Decided, None));
        let json = serde_json::to_value(&with_next).unwrap();
        assert_eq!(json["next"][0]["command"], "speccoder run");
        assert!(json.get("data").is_none());
    }
}
