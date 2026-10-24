//! Pure domain types of speccoder.
//!
//! This crate has no IO and no async: its state machines, task graph and
//! verdict rules stay property-testable without a runtime. The layering test
//! in the `speccoder` crate keeps its dependency closure free of IO and async
//! crates.

mod command;
pub mod graph;
pub mod next;
pub mod retry;
pub mod scope;
pub mod snapshot;
pub mod spec;
pub mod stage;
pub mod verify;

pub use command::{
    ApprovalView, ApprovalsReport, ArchiveReport, BenchReport, ChecklistStatus, CommandData,
    CommandResult, CommandStatus, ControlReport, ConvergeReport, ConvergenceRound, CostReport,
    DecidedCall, EarlierAttempt, EvalCase, EvalReport, EvalSummary, ExitCode, FeatureReport,
    FullTier, HeldOutGap, HeldOutOracle, InPlaceReport, InitCheck, InitReport, LastAttempt,
    MonitorScore, OperatorView, PendingCall, PrReport, ProposeReport, RefusedPath, ReplayReport,
    RunReport, SCHEMA_VERSION, SkillEntry, SkillRunReport, SkillsReport, StatusReport,
    StatusTotals, TaskCost, TaskStatus, Usage, WorktreeReport,
};
