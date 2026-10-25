//! Pure domain types of speccoder.
//!
//! This crate has no IO and no async: its state machines, task graph and
//! verdict rules stay property-testable without a runtime. The layering test
//! in the `speccoder` crate keeps its dependency closure free of IO and async
//! crates.

pub mod actions;
pub mod codes;
mod command;
pub mod evidence;
pub mod graph;
pub mod next;
pub mod retry;
pub mod scope;
pub mod snapshot;
pub mod spec;
pub mod stage;
pub mod strength;
pub mod verify;

pub use command::{
    ApprovalView, ApprovalsReport, ArchiveReport, AttestReport, BenchReport, ChecklistStatus,
    CodeInfo, CommandData, CommandResult, CommandStatus, ConfigChange, ConfigKey, ConfigReport,
    ConfigSection, ConfigTable, ControlReport, ConvergeReport, ConvergenceRound, CostReport,
    DecidedCall, DoctorCheck, DoctorLevel, DoctorReport, EarlierAttempt, EvalCase, EvalReport,
    EvalSummary, ExitCode, ExplainReport, FeatureEntry, FeatureReport, FeaturesReport, FullTier,
    HeldOutGap, HeldOutOracle, InPlaceReport, InitCheck, InitKey, InitReport, InitRun,
    InstructionFile, IssueEntry, IssuesReport, LastAttempt, MonitorScore, ObjectVerification,
    OperatorView, PAYLOADS, PendingCall, PrReport, ProposeReport, RefusedPath, ReplayReport,
    RunReport, SCHEMA_VERSION, SchemaName, SchemaReport, SignerView, SkillEntry, SkillRunReport,
    SkillsReport, StatusReport, StatusTotals, TaskCost, TaskStatus, TraceRow, UiReport, Usage,
    VerifyOutcome, VerifyReason, WorktreeReport,
};
