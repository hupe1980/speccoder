//! The fixed stages of one task's pipeline: one vocabulary for the
//! plain progress lines, the TUI, ACP updates and `status`.

use serde::{Deserialize, Serialize};

/// A stage of a task, in pipeline order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The checks on the untouched worktree.
    Baseline,
    /// A bug's reproductions written and shown to fail by an assertion.
    Reproduce,
    /// A bug's reproductions wait for the person's confirmation.
    Confirm,
    /// Spec-derived acceptance tests written and shown to fail.
    Red,
    /// The model (or an external agent) works.
    Attempt,
    /// The checks and the acceptance tests after the attempt.
    Verify,
    /// A bug's pass-to-pass set and the revert candidate.
    Unchanged,
    /// The integrity audit of the attempt's changes.
    Audit,
    /// Mutation testing of the changed code.
    Mutation,
    /// The acceptance set run against wrong and permitted candidates.
    Strength,
    /// The gaming monitor.
    Monitor,
    /// The tick and its commit.
    Commit,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: [Self; 12] = [
        Self::Baseline,
        Self::Reproduce,
        Self::Confirm,
        Self::Red,
        Self::Attempt,
        Self::Verify,
        Self::Unchanged,
        Self::Audit,
        Self::Mutation,
        Self::Strength,
        Self::Monitor,
        Self::Commit,
    ];

    /// The stages every task passes, in order: what a progress strip shows
    /// (a bug task adds [`Stage::BUG`]).
    pub const TASK: [Self; 9] = [
        Self::Baseline,
        Self::Red,
        Self::Attempt,
        Self::Verify,
        Self::Audit,
        Self::Mutation,
        Self::Strength,
        Self::Monitor,
        Self::Commit,
    ];

    /// The stages only a bug task has.
    pub const BUG: [Self; 3] = [Self::Reproduce, Self::Confirm, Self::Unchanged];

    /// The wire and display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Reproduce => "reproduce",
            Self::Confirm => "confirm",
            Self::Red => "red",
            Self::Attempt => "attempt",
            Self::Verify => "verify",
            Self::Unchanged => "unchanged",
            Self::Audit => "audit",
            Self::Mutation => "mutation",
            Self::Strength => "strength",
            Self::Monitor => "monitor",
            Self::Commit => "commit",
        }
    }
}

/// Where a stage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    /// Begun.
    Started,
    /// Done, and it held.
    Passed,
    /// Done, and it did not hold.
    Failed,
    /// Does not apply (not configured, nothing to do).
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_in_pipeline_order_with_their_wire_names() {
        let names: Vec<&str> = Stage::TASK.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "baseline", "red", "attempt", "verify", "audit", "mutation", "strength", "monitor",
                "commit"
            ]
        );
        let all: Vec<&str> = Stage::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(
            all,
            [
                "baseline",
                "reproduce",
                "confirm",
                "red",
                "attempt",
                "verify",
                "unchanged",
                "audit",
                "mutation",
                "strength",
                "monitor",
                "commit"
            ]
        );
        for s in Stage::BUG {
            assert!(!Stage::TASK.contains(&s));
        }
        for s in Stage::ALL {
            assert_eq!(serde_json::to_value(s).unwrap_or_default(), s.name());
        }
    }
}
