//! The result vocabulary shared by every CLI command.

use serde::{Deserialize, Serialize};

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
    /// `ui`: where the web interface is served.
    Ui(Box<UiReport>),
    /// `config`: every key of `speccoder.toml`, its value and its default.
    Config(Box<ConfigReport>),
    /// `issue`: the repository's open issues.
    Issues(Box<IssuesReport>),
    /// `inbox`: what waits for a person's decision, across features.
    Inbox(Box<InboxReport>),
    /// `review`: the feature's requirements and the person's review of each.
    Review(Box<ReviewReport>),
    /// `auth`: who speccoder is on a GitHub host.
    Auth(Box<AuthReport>),
    /// `speccoder-verify check` / `attest verify --branch`: every tick of
    /// a branch and what its evidence comes to.
    BranchVerify(Box<crate::verify_branch::BranchReport>),
    /// `speccoder-verify tour` / `/api/tour`: one task's evidence tour.
    EvidenceTour(Box<crate::tour::EvidenceTour>),
    /// `actions`: what can be done now.
    Actions(Box<ActionsReport>),
    /// `bug`: a bug, its reproductions and how far it got.
    Bug(Box<crate::bug::BugReport>),
    /// `config coverage`: what a prefix entry would newly allow.
    PolicyCoverage(Box<PolicyCoverage>),
    /// `review comments`: the comment tray and the last revision turn.
    Comments(Box<crate::oracle::CommentsView>),
}

/// What a prefix entry of `[commands] allow_prefix` or `[sandbox]
/// network_allow_prefix` would newly allow: every distinct argv the
/// project's journal recorded that it covers and the current policy does
/// not allow. A prefix is saved only as the confirmation of this preview,
/// and only when it is complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct PolicyCoverage {
    /// The entry previewed.
    pub entry: CoverageEntry,
    /// Every run in the journal was read.
    pub complete: bool,
    /// What could not be read, and why (never "covers nothing").
    pub unreadable: Vec<String>,
    /// The argvs it would newly allow.
    pub rows: Vec<CoverageRow>,
    /// SHA-256 over the entry, the rows and `complete`: what a save names.
    pub digest: String,
}

/// A prefix entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct CoverageEntry {
    /// `commands.allow_prefix` or `sandbox.network_allow_prefix`.
    pub key: String,
    /// The prefix.
    pub prefix: Vec<String>,
}

/// One argv a prefix would newly allow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct CoverageRow {
    /// The argv.
    pub argv: Vec<String>,
    /// How often the journal records it.
    pub count: u32,
    /// Seconds since the epoch of the latest run that called it.
    pub last_at: Option<u64>,
    /// How it was decided then, with how often.
    pub decided: Vec<CoverageDecision>,
    /// A rule asks for it (or denies it) whatever the list says: the
    /// rule's reason. The row stays so.
    pub still_asks: Option<String>,
}

/// How calls of one argv were decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct CoverageDecision {
    /// `allowed by rule`, `approved by a person`, `waiting`.
    pub decision: String,
    /// How often.
    pub count: u32,
}

/// Result of `speccoder actions`: what a person can do now in the
/// project — the list every interface renders — and where the feature is
/// on its journey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ActionsReport {
    /// Every action, by group, the most urgent first.
    pub actions: Vec<crate::actions::Action>,
    /// The feature's journey (spec → plan → tasks → commit → run → review).
    pub journey: Vec<crate::actions::JourneyStep>,
}

/// Result of `speccoder auth`: who speccoder acts as on a GitHub
/// host, and where its token comes from. The token itself is never shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct AuthReport {
    /// `github.com` or an Enterprise host.
    pub host: String,
    /// The account, when signed in.
    pub login: Option<String>,
    /// `GH_TOKEN`, `GITHUB_TOKEN`, `the keychain`, when signed in.
    pub source: Option<String>,
}

/// Result of `speccoder review`: every requirement of the feature's
/// spec as a card, with the person's decision on its exact wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReviewReport {
    /// The feature.
    pub feature: String,
    /// Where the review is recorded (project-relative); commit it to share.
    pub file: String,
    /// One per requirement, in the spec's order.
    pub requirements: Vec<ReviewItem>,
    /// Counts by state.
    pub totals: ReviewTotals,
    /// The format's skill that revises the spec from the comments
    /// (Spec Kit: `clarify`), when it has one.
    pub revise_with: Option<String>,
    /// What that skill is asked, while changes are requested: every
    /// request with its requirement.
    pub revision: Option<String>,
    /// The questions the documents leave open (`[NEEDS CLARIFICATION: …]`),
    /// in reading order.
    pub questions: Vec<OpenQuestion>,
    /// Requirements changed in the checkout since the last commit of the
    /// spec: what a commit would record.
    pub changes: Vec<ReviewChange>,
    /// The spec's versions in git, newest first (the last twelve).
    pub history: Vec<SpecVersion>,
    /// One per requirement with scenarios (`US1`), beside the decision
    /// lines: the tests the run would use, as written at review.
    #[serde(default)]
    pub oracle: Vec<crate::oracle::OracleView>,
    /// Why the review writes no tests for this unit: its own pipeline
    /// authors and confirms them (a quick change, a bug).
    #[serde(default)]
    pub tests_notice: Option<String>,
    /// A bug unit's confirmed reproductions, read only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bug_tests: Vec<crate::oracle::BugTests>,
    /// The approval that froze the oracle, when one is recorded.
    #[serde(default)]
    pub approval: Option<crate::oracle::ReviewApproval>,
    /// What *Approve and Run* would do and what blocks it (the full review
    /// only).
    #[serde(default)]
    pub sheet: Option<crate::oracle::ApproveSheet>,
    /// Why the review record (`review.json`) cannot be read: shown, never
    /// read as "nothing decided" by a write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A requirement changed in the checkout since the last commit of the
/// spec, with where its line is — a comment anchors to it (`diff_line`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewChange {
    /// `FR-003`.
    pub id: String,
    /// Its line at the last commit; `None`: it did not exist yet.
    pub then: Option<String>,
    /// Its line now; `None`: it was removed.
    pub now: Option<String>,
    /// The document it is in (project-relative).
    pub path: String,
    /// Its line number (1-based) at the last commit.
    pub then_line: Option<u32>,
    /// Its line number (1-based) now.
    pub now_line: Option<u32>,
}

/// A question a document leaves open, to answer in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct OpenQuestion {
    /// `Q1`, `Q2`, … in reading order.
    pub id: String,
    /// The question.
    pub text: String,
    /// The document (project-relative).
    pub file: String,
    /// Its line (1-based).
    pub line: usize,
    /// The requirement or section it is in.
    pub near: Option<String>,
}

/// One commit of the spec and what it changed in its requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SpecVersion {
    /// The commit.
    pub commit: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Its author.
    pub author: String,
    /// Its subject line.
    pub subject: String,
    /// Requirements it added.
    pub added: Vec<String>,
    /// Requirements it reworded.
    pub reworded: Vec<String>,
    /// Requirements it removed.
    pub removed: Vec<String>,
}

/// One requirement and its review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReviewItem {
    /// `FR-001`.
    pub id: String,
    /// Its wording now (the line, markup aside).
    pub text: String,
    /// `open` (not reviewed), `approved`, `changes_requested`, or `lapsed`
    /// (decided on a wording that has changed since).
    pub state: String,
    /// Who decided (as for every decision: git's `user.name`, else the OS
    /// user), when someone did.
    pub by: Option<String>,
    /// When (Unix seconds).
    pub at: Option<u64>,
    /// The comment, for a change request (or a note on an approval).
    pub comment: Option<String>,
    /// For `lapsed`: the wording that was decided on.
    pub decided_on: Option<String>,
}

/// Counts of [`ReviewItem::state`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReviewTotals {
    /// Approved on their current wording.
    pub approved: usize,
    /// Changes requested.
    pub changes_requested: usize,
    /// Never reviewed.
    pub open: usize,
    /// Decided on an earlier wording.
    pub lapsed: usize,
    /// Questions the documents leave open.
    #[serde(default)]
    pub questions: usize,
    /// Requirements two test authors read differently, not yet resolved:
    /// *Approve and Run* and a gated run wait on them.
    #[serde(default)]
    pub ambiguous: usize,
}

impl ReviewTotals {
    /// Every requirement approved on its current wording.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.changes_requested == 0 && self.open == 0 && self.lapsed == 0 && self.questions == 0
    }
}

/// Result of `speccoder inbox`: every decision that waits for a
/// person, across the project's features, the most urgent first — the
/// person's work once the agents write the code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InboxReport {
    /// The items, the most urgent first (see [`InboxItem::kind`]).
    pub items: Vec<InboxItem>,
    /// Features looked at.
    pub features: usize,
}

/// One decision waiting for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InboxItem {
    /// What kind, in the order of urgency: `unknown` (the calls waiting in
    /// the feature's journal could not be read — some may wait for you),
    /// `approval` (a run waits on it),
    /// `ruling` (a verified attempt is held: its tests reject an
    /// alternative the spec allows), `gaming` (the monitor suspects a
    /// shortcut), `stale` (proven against an earlier wording), `question`
    /// (the spec leaves questions open), `review`
    /// (requirements not approved on their current wording), `revise`
    /// (changes requested on the spec), `stuck` (the
    /// last attempt did not pass), `aside` (changes a run set aside),
    /// `commit` (planning files a run cannot
    /// read yet), `handoff` (every task done: review and hand it off) —
    /// and a bug's `bug_confirm`, `bug_cannot_reproduce`,
    /// `bug_unchanged_not_holding`, `bug_repro_weak`,
    /// `bug_already_satisfied` (the registry: `speccoder_core::inbox::Kind`).
    pub kind: String,
    /// Its group: `blocked_on_you` or `ready_to_review`.
    pub group: String,
    /// The feature.
    pub feature: String,
    /// The task, when it is about one.
    pub task: Option<String>,
    /// What to decide, in a line.
    pub title: String,
    /// What it rests on: the call, the reason, the changed requirement.
    pub detail: String,
    /// Waiting since (Unix seconds), when known.
    pub since: Option<u64>,
    /// The decisions, the one that moves the work forward first; each runs
    /// in the item's feature.
    pub actions: Vec<crate::actions::Action>,
    /// `unverified`: the feature's ticks and why each regression-only one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unverified: Option<Unverified>,
}

/// A feature whose ticks are mostly regression-only, counted by reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Unverified {
    /// Ticks speccoder judged (`verified` and `regression_only`).
    pub ticks: usize,
    /// Of them, regression-only.
    pub regression_only: usize,
    /// The reasons, the most frequent first.
    pub counts: Vec<ReasonCount>,
}

/// How many regression-only ticks had one reason, and which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReasonCount {
    /// The reason (`no_story`, `already_passing`, …, `unrecorded`).
    pub reason: String,
    /// How many ticks.
    pub n: usize,
    /// Their tasks.
    pub tasks: Vec<String>,
    /// `already_passing`: the tasks that made the tests pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implemented_by: Vec<String>,
    /// speccoder's words for the group (`7 had no user story`).
    pub text: String,
}

/// Result of `speccoder issue`: the repository's open GitHub issues, read
/// through GitHub's API as the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct IssuesReport {
    /// `owner/name`.
    pub repo: Option<String>,
    /// Open issues: assigned to the person first, then the most recently
    /// updated.
    pub issues: Vec<IssueEntry>,
}

/// One open issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueEntry {
    /// `#n`.
    pub number: u64,
    /// Its title.
    pub title: String,
    /// Its address.
    pub url: String,
    /// Label names.
    pub labels: Vec<String>,
    /// Who it is assigned to (logins).
    pub assignees: Vec<String>,
    /// Assigned to the person speccoder is signed in as.
    pub mine: bool,
    /// Last update, ISO 8601.
    pub updated: Option<String>,
    /// The feature whose spec names this issue, when one does.
    pub feature: Option<String>,
}

/// Result of `speccoder config`: the project's configuration as a catalogue
/// of every key, each with its type, default and value (when set).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ConfigReport {
    /// The file, relative to the project root.
    pub file: String,
    /// Whether it exists.
    pub exists: bool,
    /// Whether it is committed as it is on disk (speccoder reads committed
    /// configuration only, so an uncommitted change is not yet in force).
    pub committed: bool,
    /// Whether every parser accepts it.
    pub valid: bool,
    /// The first parser's complaint, when not valid.
    pub error: Option<String>,
    /// Sections, in the reference's order.
    pub sections: Vec<ConfigSection>,
    /// The uncommitted change, as a diff, when there is one.
    pub diff: Option<String>,
}

/// One section of `speccoder.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ConfigSection {
    /// The table name, e.g. `model`, or `checks` for `[checks.<name>]`.
    pub name: String,
    /// A title for people, e.g. `Model`.
    pub title: String,
    /// What the section decides.
    pub about: String,
    /// Named tables (`[checks.<name>]`, `[mcp.<name>]`): `keys` describe
    /// one, `tables` hold the configured ones.
    pub named: bool,
    /// The keys (for a named section: the keys of each table, unset).
    pub keys: Vec<ConfigKey>,
    /// The configured named tables.
    pub tables: Vec<ConfigTable>,
}

/// One configured named table, e.g. `[checks.test]`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ConfigTable {
    /// Its name, e.g. `test`.
    pub name: String,
    /// Its keys, with values.
    pub keys: Vec<ConfigKey>,
}

/// One key: what it is, what it is set to, and what it is otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ConfigKey {
    /// The key, e.g. `max_turns`; dotted below a sub-table (`sigstore.identity`).
    pub key: String,
    /// `bool`, `integer`, `number`, `string`, `choice`, `duration`, `argv`,
    /// `argv_list` or `string_list`.
    pub kind: String,
    /// What it does.
    pub about: String,
    /// The value speccoder uses when the key is absent; none for keys
    /// without a default (unset means off, or the key is required).
    pub default: Option<serde_json::Value>,
    /// The value in the file, when set.
    pub value: Option<serde_json::Value>,
    /// For `choice`: the allowed values.
    pub choices: Vec<String>,
    /// For numbers: the smallest allowed.
    pub min: Option<f64>,
    /// For numbers: the largest allowed.
    pub max: Option<f64>,
    /// The section (or table) is refused without it.
    pub required: bool,
}

/// A change to `speccoder.toml`: set a key, remove it, or append to a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConfigChange {
    /// The dotted key, e.g. `budget.max_turns` or `checks.test.argv`.
    pub key: String,
    /// The new value; absent (with no `append`) removes the key.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    /// An item to add to a list key (`commands.allow`), unless present.
    #[serde(default)]
    pub append: Option<serde_json::Value>,
}

/// Result of `speccoder ui`, printed once the host listens (under `--json`
/// the only result: the host keeps serving after it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct UiReport {
    /// The address to open, with this start's key in its fragment.
    pub url: String,
    /// The port on 127.0.0.1.
    pub port: u16,
    /// The project served.
    pub root: String,
}

/// One feature (or change) of the project, as `speccoder feature` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct FeatureEntry {
    /// 1-based position, the number `speccoder feature <n>` takes.
    pub number: usize,
    /// Directory name, e.g. `001-greet`.
    pub name: String,
    /// The title its spec (or proposal) gives, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
    /// Works fully; something optional is not set up (no signing key).
    Info,
    /// Works, with less than it could (no spec tool).
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
    /// The fix speccoder can apply itself, on the person's yes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<Remedy>,
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
            Self::Ui(_) => "ui",
            Self::Config(_) => "config",
            Self::Issues(_) => "issues",
            Self::Inbox(_) => "inbox",
            Self::Review(_) => "review",
            Self::Auth(_) => "auth",
            Self::BranchVerify(_) => "branch_verify",
            Self::EvidenceTour(_) => "evidence_tour",
            Self::Actions(_) => "actions",
            Self::Bug(_) => "bug",
            Self::PolicyCoverage(_) => "policy_coverage",
            Self::Comments(_) => "comments",
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
    "ui",
    "config",
    "issues",
    "inbox",
    "review",
    "auth",
    "branch_verify",
    "evidence_tour",
    "actions",
    "bug",
    "policy_coverage",
    "comments",
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
    /// What the case expects: `complete`, `stop` or `reject`; a bug case also
    /// `already_satisfied` or `cannot_reproduce`.
    pub expect: String,
    /// `correct`, `false_completion`, `not_completed`, `honest_stop`, `caught`, `cheat`, `invalid_case` or `error`.
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
    /// Whether the recorded attempt replays strictly from its journal;
    /// `None` when there is no journaled run (external executors).
    pub replays: Option<bool>,
    /// The held-out tests' part (`acceptance.held_out` of the run report).
    pub held_out: Option<serde_json::Value>,
    /// An attempt passed the visible checks and failed the held-out tests.
    pub gap: bool,
    /// The combined acceptance strength, when measured.
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
    /// failed; `None` when no held-out test ran.
    pub held_out_gap: Option<HeldOutGap>,
    /// Ticked cases whose held-out tests passed, against the oracle.
    pub held_out_oracle: Option<HeldOutOracle>,
    /// The median combined acceptance strength over verified cases that
    /// measured it; `None`: not measured.
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
    /// missing, signer, reason }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<serde_json::Value>,
    /// The repository's instructions the attempt was given (`AGENTS.md`,
    /// `CLAUDE.md`, …), root first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<InstructionFile>,
    /// The acceptance set's strength: after a `verified` verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<crate::strength::AcceptanceStrength>,
    /// A bug task: the bug as this run left it (reproduce, confirm, the
    /// fix's discrimination).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<Box<crate::bug::BugReport>>,
    /// The test files the attempt added or changed, and whether the run
    /// collected a test from each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<crate::collection::Collection>,
    /// Why a regression-only verdict is one (set with that verdict only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression: Option<crate::regression::Regression>,
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
    /// How the run asked for a person: `ask` or `autopilot`.
    #[serde(default)]
    pub mode: crate::oracle::RunMode,
    /// The calls an autopilot run refused instead of asking: listed for
    /// the person.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub autopilot_denied: Vec<crate::oracle::AutopilotDenial>,
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
    /// Why it did not pass: the last line of its output that says something
    /// (`No module named pytest`), or the runner's note.
    pub reason: Option<String>,
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
    /// Why the calls waiting in the journal could not be read (another
    /// speccoder holds it, …): `pending` may then be incomplete — unknown,
    /// never "nothing waiting".
    pub unreadable: Option<String>,
    /// Runs whose asking budget is used: their waiting calls, decided
    /// together (`speccoder approvals batch`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub batch: Vec<BatchView>,
}

/// The object a person's decision covers, as reports show it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ObjectView {
    /// `write`, `command`, `network` or `mcp`.
    pub class: String,
    /// The canonical path, `argv#<12 hex>` or `<tool>@<12 hex>`.
    pub short: String,
    /// One line: ``run `make check` ``.
    pub describe: String,
    /// The object in full, as every interface shows it.
    pub lines: Vec<String>,
}

/// A run whose asking budget is used, with its waiting calls by class.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct BatchView {
    /// The run.
    pub run: String,
    /// SHA-256 over the sorted digests of every call listed
    /// ([`PendingCall::digest`]): a decision names it and is refused when
    /// the list, or the object of a call in it, changed.
    pub digest: String,
    /// The calls by class, in the order write, command, network, mcp.
    pub groups: Vec<BatchGroup>,
}

/// One class of a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct BatchGroup {
    /// `write`, `command`, `network` or `mcp`.
    pub class: String,
    /// The waiting calls' ids.
    pub calls: Vec<String>,
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
    /// The call as the inline prompt shows it.
    pub presentation: Option<ApprovalView>,
    /// The waiting call's id (`approve --call`); empty for a stopped attempt.
    pub call: String,
    /// `task` (the attempt waits and resumes) or `attempt` (stopped and saved
    /// in a parallel lane).
    pub source: String,
    /// `approval` or `question`.
    pub kind: String,
    /// A question's text.
    pub question: Option<String>,
    /// Seconds since the epoch the call has waited since.
    pub waiting_since: Option<u64>,
    /// Seconds since the epoch it is denied as expired at.
    pub deadline: Option<u64>,
    /// The object a decision covers.
    pub object: Option<ObjectView>,
    /// Where arguments of the call were copied from (source lines).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// The run it belongs to.
    pub run: Option<String>,
    /// It waits in its run's batch: the asking budget was used.
    pub batched: bool,
    /// SHA-256 over its id and the object a decision covers: a decision
    /// names it and is refused when another call (or another object) waits
    /// now.
    pub digest: String,
}

impl PendingCall {
    /// Its id in decisions and batches: the waiting call's, or
    /// `attempt:<task>` for a call a parallel lane stopped.
    #[must_use]
    pub fn id(&self) -> String {
        if self.call.is_empty() {
            format!("attempt:{}", self.task)
        } else {
            self.call.clone()
        }
    }
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
    /// What `approve` grants: `for the rest of T004: this file, any content`.
    pub scope: String,
    /// The object's lines.
    pub object: Vec<String>,
    /// Where arguments were copied from.
    pub sources: Vec<String>,
    /// The choices offered, in order.
    pub options: Vec<String>,
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
    /// Through what: an origin of `actions::Via` (`terminal`, `web`,
    /// `acp`, …).
    pub via: Option<String>,
    /// The editor decided in, over the Agent Client Protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Nobody decided: the deadline passed and the call was denied.
    pub expired: bool,
    /// The object it decided on.
    pub object: Option<ObjectView>,
    /// The run it belongs to (a grant covers nothing after it).
    pub run: Option<String>,
}

/// What `attest verify` found for one task object.
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
    /// The signature's tool (`ssh-keygen`, `gpg`, `cosign`) cannot run:
    /// nothing can be said about it.
    ToolMissing,
}

impl VerifyOutcome {
    /// How grave: when several apply, the gravest is the outcome
    /// (`malformed` > `bad_signature` > `mismatch` > `tool_missing` >
    /// `untrusted_signer` > `unsigned` > `ok`; `missing` stands alone).
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Unsigned => 1,
            Self::UntrustedSigner => 2,
            Self::ToolMissing => 3,
            Self::Mismatch => 4,
            Self::BadSignature => 5,
            Self::Malformed => 6,
            Self::Missing => 7,
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
            Self::ToolMissing => "tool_missing",
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
    /// The requirement changed since that task's tick: the verdict is for
    /// its earlier wording.
    #[serde(default)]
    pub changed: bool,
}

/// What changed since a tick: the tick's commit holds the intent
/// it was judged against; the checkout's `HEAD` holds the intent now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Drift {
    /// The requirements the task cites whose line changed, appeared or went
    /// away: the evidence no longer speaks for them. Empty: what changed is
    /// elsewhere in the intent.
    pub requirements: Vec<RequirementDrift>,
    /// The intent files that differ (project-relative), task lists aside.
    /// Empty when the intent is not in git (in place, ignored): it changed,
    /// which file is not known.
    pub files: Vec<String>,
}

impl Drift {
    /// A requirement the task cites changed: the tick does not prove what
    /// the spec asks now.
    #[must_use]
    pub fn stale(&self) -> bool {
        !self.requirements.is_empty()
    }
}

/// One cited requirement, as the tick saw it and as the spec has it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct RequirementDrift {
    /// `FR-003`.
    pub id: String,
    /// Its line when the task was ticked; `None`: it did not exist yet.
    pub then: Option<String>,
    /// Its line now; `None`: it was removed.
    pub now: Option<String>,
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
    /// Planning files under the feature's directory not yet committed
    /// (`speccoder run` reads committed files only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncommitted: Vec<String>,
    /// Where the feature is: spec → plan → tasks → commit → run → review.
    #[serde(default)]
    pub journey: Vec<crate::actions::JourneyStep>,
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
    /// GitHub issue URLs the feature's spec names: where it came from, and
    /// what its pull request closes.
    pub issues: Vec<String>,
    /// The person's review of the spec's requirements, when the
    /// format has requirement identifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewTotals>,
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
    /// What changed in the intent (spec, plan, constitution) since this
    /// task's evidence was committed: its tick was judged against an earlier
    /// version. `None`: nothing changed, or the task has no commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift: Option<Drift>,
    /// For a task speccoder committed: its attestation as the note says,
    /// signatures unchecked — `signed <kind> <keyid>`, `unsigned` or
    /// `missing` (`attest verify` checks them).
    pub attestation: Option<String>,
    /// Open dependencies of a blocked task.
    pub blocked_by: Vec<String>,
    /// The speccoder commit that ticked it.
    pub commit: Option<String>,
    /// The agentplane run of that attempt.
    pub run_id: Option<String>,
    /// For an open task: its latest saved attempt.
    pub last_attempt: Option<LastAttempt>,
    /// Changes found in the worktree when the task's run started (a person's
    /// edits, or an interrupted run's leftovers), saved and set aside.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_aside: Option<SetAside>,
    /// The reason a person gave for `accept` or `skip`.
    pub note: Option<String>,
    /// A verified tick's acceptance strength as its trailer says
    /// (`0.83 mutants=9/11 near-miss=1/2`, `not measured (…)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<String>,
    /// A bug task: the bug's outcome (`awaiting_confirmation`,
    /// `cannot_reproduce`, `fixed`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<String>,
    /// A regression-only tick: why, with speccoder's wording
    /// (`unrecorded` for a tick made before reasons were recorded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression: Option<crate::regression::Regression>,
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

/// Changes saved and set aside so a run could start from the branch tip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SetAside {
    /// The ref holding them.
    pub reference: String,
    /// The files they change, repository-relative.
    pub files: Vec<String>,
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
    /// Ticks whose cited requirements changed since.
    #[serde(default)]
    pub stale: usize,
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
    /// The unit's held-out ref pushed with the branch
    /// (`refs/speccoder/held-out/<unit>`), when a closed task names a set.
    pub held_out_ref: Option<String>,
    /// The tasks closed on the pushed head whose held-out sets it carries.
    pub held_out_tasks: Vec<String>,
    /// Held-out sets of open tasks: they stay local.
    pub held_out_withheld: usize,
    /// `present`: a workflow under `.github/workflows/` runs
    /// `speccoder-verify`; `absent`: none does.
    pub evidence_check: String,
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
    remedy: Option<Box<Remedy>>,
}

/// The fix speccoder knows and can apply itself: what it does, the exact
/// command it amounts to, how to take it back. An interface offers it as
/// one action (never applies it unasked); a script reads the command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Remedy {
    /// The action, a verb first (`Commit Cargo.lock`).
    pub label: String,
    /// What it changes, in one line.
    pub effect: String,
    /// The same, as a command run in the checkout.
    pub command: String,
    /// How to take it back once applied.
    pub undo: Option<String>,
    /// It writes git history (a commit): an interface asks first.
    pub history: bool,
    /// How an interface applies it.
    pub run: crate::actions::Runnable,
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
        if let Some(remedy) = &self.remedy {
            map.serialize_entry("remedy", remedy)?;
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
    /// The fix speccoder can apply itself, on the person's yes.
    #[serde(skip_serializing_if = "Option::is_none")]
    remedy: Option<Remedy>,
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
            remedy: None,
        }
    }

    /// `from`'s outcome reported as `command`'s: status, message and code,
    /// without `from`'s payload (a command passing on a failure it met).
    #[must_use]
    pub fn carried(command: &str, from: &Self) -> Self {
        let mut out = Self::new(command, from.status, &from.message);
        out.code = from.code;
        out.fix.clone_from(&from.fix);
        out.remedy.clone_from(&from.remedy);
        out
    }

    /// The code, fixes and remedy of `from`, whose outcome this result
    /// reports with the same status (a feature's run stopped by a task's).
    #[must_use]
    pub fn caused_by(mut self, from: &Self) -> Self {
        if self.status == from.status {
            self.code = from.code;
            self.fix.clone_from(&from.fix);
            self.remedy.clone_from(&from.remedy);
        }
        self
    }

    /// The same result with another status and message: payload and next
    /// steps kept, the code kept only while the status stays the same.
    #[must_use]
    pub fn reworded(mut self, status: CommandStatus, message: &str) -> Self {
        if status != self.status {
            self.code = None;
            self.fix.clear();
            self.remedy = None;
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

    /// Attaches the fix speccoder can apply itself.
    #[must_use]
    pub fn with_remedy(mut self, remedy: Option<Remedy>) -> Self {
        self.remedy = remedy.map(Box::new);
        self
    }

    /// The fix speccoder can apply itself, if it knows one.
    #[must_use]
    pub fn remedy(&self) -> Option<&Remedy> {
        self.remedy.as_deref()
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
    fn payload_names_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for p in PAYLOADS {
            assert!(seen.insert(*p), "payload {p} is listed twice");
        }
        let branch = CommandData::BranchVerify(Box::new(crate::verify_branch::BranchReport {
            head: String::new(),
            base: None,
            mode: crate::verify_branch::Mode::All,
            features: Vec::new(),
            ticks: Vec::new(),
            skipped: Vec::new(),
            totals: crate::verify_branch::BranchTotals::default(),
            conclusion: crate::verify_branch::Conclusion::Neutral,
            trust: serde_json::Value::Null,
            accepted_unsigned: false,
            require_verified: false,
            trace: Vec::new(),
            share: None,
            open_no_claim: Vec::new(),
            not_evaluated: Vec::new(),
            newer_fields: Vec::new(),
        }));
        assert!(PAYLOADS.contains(&branch.kind()));
        assert_ne!(branch.kind(), "verify");
        assert_ne!(branch.kind(), "attest");
    }

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
