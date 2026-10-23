//! The fixed stages of one task's pipeline (F-053): one vocabulary for the
//! plain progress lines, the TUI, ACP updates and `status`.

use serde::{Deserialize, Serialize};

/// A stage of a task, in pipeline order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The checks on the untouched worktree.
    Baseline,
    /// Spec-derived acceptance tests written and shown to fail.
    Red,
    /// The model (or an external agent) works.
    Attempt,
    /// The checks and the acceptance tests after the attempt.
    Verify,
    /// The integrity audit of the attempt's changes.
    Audit,
    /// Mutation testing of the changed code.
    Mutation,
    /// The gaming monitor.
    Monitor,
    /// The tick and its commit.
    Commit,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: [Self; 8] = [
        Self::Baseline,
        Self::Red,
        Self::Attempt,
        Self::Verify,
        Self::Audit,
        Self::Mutation,
        Self::Monitor,
        Self::Commit,
    ];

    /// The wire and display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Red => "red",
            Self::Attempt => "attempt",
            Self::Verify => "verify",
            Self::Audit => "audit",
            Self::Mutation => "mutation",
            Self::Monitor => "monitor",
            Self::Commit => "commit",
        }
    }
}

/// Where a stage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        let names: Vec<&str> = Stage::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "baseline", "red", "attempt", "verify", "audit", "mutation", "monitor", "commit"
            ]
        );
        for s in Stage::ALL {
            assert_eq!(serde_json::to_value(s).unwrap_or_default(), s.name());
        }
    }
}
