//! Pure domain types of speccoder.
//!
//! This crate has no IO and no async: its state machines, task graph and
//! verdict rules stay property-testable without a runtime. The layering test
//! in the `speccoder` crate keeps its dependency closure free of IO and async
//! crates.

pub mod acceptance;
pub mod actions;
pub mod bug;
pub mod codes;
pub mod collection;
mod command;
pub mod context;
pub mod event;
pub mod evidence;
pub mod graph;
pub mod inbox;
pub mod ladder;
pub mod lane;
pub mod new_project;
pub mod next;
pub mod oracle;
pub mod predicate;
pub mod regression;
pub mod retry;
pub mod scope;
pub mod similar;
pub mod snapshot;
pub mod spec;
pub mod stage;
pub mod step;
pub mod strength;
pub mod tour;
pub mod verify;
pub mod verify_branch;
pub mod words;
pub mod workflow;

pub use command::{
    ActionsReport, AgentCard, AgentTick, ApprovalView, ApprovalsReport, ArchiveReport,
    AttestReport, AuthReport, BatchGroup, BatchView, BenchReport, ChecklistStatus, CodeInfo,
    CommandData, CommandResult, CommandStatus, ConfigChange, ConfigKey, ConfigReport,
    ConfigSection, ConfigTable, ControlReport, ConvergeReport, ConvergenceRound, CostReport,
    CoverageDecision, CoverageEntry, CoverageRow, DecidedCall, DoctorCheck, DoctorLevel,
    DoctorReport, Drift, EarlierAttempt, EvalCase, EvalReport, EvalSummary, ExitCode,
    ExplainReport, FeatureEntry, FeatureReport, FeaturesReport, FullTier, HeldOutGap,
    HeldOutOracle, InPlaceReport, InboxItem, InboxReport, InitCheck, InitKey, InitReport, InitRun,
    InstructionFile, IssueEntry, IssuesReport, LastAttempt, MonitorScore, ObjectVerification,
    ObjectView, OpenQuestion, OperatorView, PAYLOADS, PendingCall, PolicyCoverage, PrReport,
    PresetReport, PresetState, ProposeReport, ReasonCount, RefusedPath, Remedy, ReplayReport,
    RequirementDrift, ReviewChange, ReviewItem, ReviewReport, ReviewTotals, RunReport,
    SCHEMA_VERSION, SchemaName, SchemaReport, SetAside, SignerView, SkillEntry, SkillRunReport,
    SkillsReport, SpecVersion, StatusReport, StatusTotals, TaskCost, TaskStatus, TickState,
    TraceRow, UiReport, Unverified, Usage, VerifyOutcome, VerifyReason, WorktreeReport,
};
pub use command::{
    ChainStep, ChainStop, PlanCheck, PlanningChainReport, Readiness, SampleRecording, SampleReport,
    SampleRun,
};
