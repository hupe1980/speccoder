//! A task's evidence, as rows every interface renders.

use serde::Serialize;

/// One row: `Acceptance  red → green: 1 failing before, 2 passing after`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Row {
    /// `Acceptance`, `Held-out`, `Strength`, …
    pub label: String,
    /// Its mark, as a task state word: `verified`, `regression_only`,
    /// `attested`, `failed`, `needs_approval`, `ready`, …
    pub mark: String,
    /// What it says.
    pub text: String,
    /// What it lists, one each (a contained command), for an interface to
    /// show apart; `text` says them too.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
}
