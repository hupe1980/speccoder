//! The `task-verification` predicate (`https://speccoder.dev/task-verification/v0`)
//! as one typed body: what speccoder signs about a ticked task, and what a
//! verifier reads back. Published as an open specification, its schema
//! generated from this type.
//!
//! Versioned within v0 by `revision`: additive fields raise it, a removed or
//! redefined field is a new predicate type (`…/v1`). Readers tolerate
//! unknown fields and higher revisions: [`TaskVerificationV0::extra`] keeps
//! them, so a statement round-trips unchanged, and the verifier names them
//! without letting them decide anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The predicate type.
pub const TYPE: &str = "https://speccoder.dev/task-verification/v0";

/// The revision this build writes. 1: the fields before `revision`
/// existed (a statement without it); 2: `revision`, `red` and the reserved
/// fields of other specs.
pub const REVISION: u32 = 2;

/// The longest red-output excerpt a statement carries, in bytes.
pub const RED_EXCERPT_MAX: usize = 2048;

/// A present field, `null` included, as `Some`: a statement that says
/// `"run": null` and one that has no `run` stay apart on round-trip.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// The spec unit (feature, change) the task belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Unit {
    /// The spec format, e.g. `speckit`, `openspec`.
    pub format: String,
    /// The unit's name, e.g. `007-config`.
    pub feature: String,
    /// The unit's directory, project-relative.
    pub dir: String,
}

/// The task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskRef {
    /// Task ID (`T012`, `1.3`).
    pub id: String,
    /// sha256 of the task line with its box read as open (before the tick).
    pub line_hash: String,
    /// The task's title.
    pub title: String,
}

/// The requirement the task belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpecRef {
    /// The requirement or story, e.g. `US1`; `null`: none.
    pub section: Option<String>,
    /// sha256 of that requirement's text as the attempt was given it.
    pub section_hash: String,
}

/// Who decided an `attested` or `skipped` task, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Decider {
    /// The person.
    pub by: String,
    /// Their reason.
    pub reason: String,
}

/// How the acceptance tests first failed (revision 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedKind {
    /// On an assertion.
    Assertion,
    /// Because what the task introduces did not exist yet.
    MissingSymbol,
}

/// The red run's output, bounded and checked clean (revision 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Red {
    /// How the tests failed.
    pub kind: RedKind,
    /// The last bytes of the output (at most 2048); `null` when withheld.
    pub excerpt: Option<String>,
    /// sha256 of `excerpt`.
    pub excerpt_sha256: Option<String>,
    /// sha256 of the captured output the excerpt was cut from.
    pub output_sha256: String,
    /// The excerpt was not clean (a local path, a credential): dropped.
    pub withheld: bool,
}

/// The `task-verification` predicate, v0.
///
/// Fields `null` in a tick may be absent in a decision (`attested`,
/// `skipped`), which carries `decider` instead of the test evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskVerificationV0 {
    /// Additive revision within v0; absent: 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    /// `verified`, `regression_only`, `attested` or `skipped`.
    pub verdict: String,
    /// The unit.
    pub unit: Unit,
    /// The task.
    pub task: TaskRef,
    /// The task commit's tree.
    pub tree: String,
    /// The requirement and its text's hash (ticks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<SpecRef>,
    /// Requirement → scenario → test rows, each with its phase and result.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trace: Option<Value>,
    /// The diff audit's findings (`{ findings: [] }`).
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub integrity: Option<Value>,
    /// The gaming monitor: `{ state: clean|suspected|unreviewed|off, findings }`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub monitor: Option<Value>,
    /// Mutation testing: `{ caught, missed, … }`, or `null`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub mutation: Option<Value>,
    /// Where the attempt ran: `{ mode, backend, network }`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub sandbox: Option<Value>,
    /// Who implemented: `{ kind, agent? }`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub executor: Option<Value>,
    /// The judging model: `{ provider, model }`, or `null`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<Value>,
    /// The agentplane run, or `null`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub run: Option<Value>,
    /// People's decisions on the attempt's calls.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub decisions: Option<Value>,
    /// The acceptance set's strength (shares, near misses, permitted
    /// alternatives), or `null`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub strength: Option<Value>,
    /// Repository instruction files given to the attempt.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Value>,
    /// The decision's maker (`attested`, `skipped`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decider: Option<Decider>,
    /// sha256 of the judge's configuration at the parent commit.
    pub policy_sha256: String,
    /// When it was recorded (RFC 3339, UTC).
    pub time: String,
    /// The red run's output excerpt (revision 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub red: Option<Red>,
    /// A bug task's evidence: where the bug lives, its triad's digest, the
    /// outcome, the reproductions' discrimination, special-case rejections
    /// and the pass-to-pass set. Opaque to the verifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<Value>,
    /// Where the acceptance tests were decided —
    /// `{ source: authored|review|bug_confirmation|person_test, pre_frozen }`.
    /// Opaque here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<Value>,
    /// Reserved: what reached the model, by digest. Opaque here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
    /// Reserved: dependencies the change added, with their
    /// registry checksums. Opaque here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Value>,
    /// Fields this build does not know (a newer revision): kept, so the
    /// signed bytes survive a round-trip, and never read for an outcome.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl TaskVerificationV0 {
    /// The revision the statement claims (absent: 1).
    #[must_use]
    pub fn revision(&self) -> u32 {
        self.revision.unwrap_or(1)
    }

    /// What this build does not know of the statement: the names of
    /// unknown fields, and `revision N` when it is newer than [`REVISION`].
    #[must_use]
    pub fn newer_fields(&self) -> Vec<String> {
        let mut out: Vec<String> = self.extra.keys().cloned().collect();
        if self.revision() > REVISION {
            out.push(format!("revision {}", self.revision()));
        }
        out
    }

    /// The reserved fields present (opaque here): their names.
    #[must_use]
    pub fn reserved_present(&self) -> Vec<&'static str> {
        [
            ("bug", self.bug.is_some()),
            ("acceptance", self.acceptance.is_some()),
            ("context", self.context.is_some()),
            ("dependencies", self.dependencies.is_some()),
        ]
        .into_iter()
        .filter_map(|(n, p)| p.then_some(n))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tick() -> Value {
        json!({
            "verdict": "verified",
            "unit": { "format": "speckit", "feature": "001-x", "dir": "specs/001-x" },
            "task": { "id": "T001", "line_hash": "ab", "title": "Do it (FR-001)" },
            "tree": "t",
            "spec": { "section": null, "section_hash": "cd" },
            "trace": [],
            "integrity": { "findings": [] },
            "monitor": { "state": "off", "findings": [] },
            "mutation": null,
            "sandbox": { "mode": "off" },
            "executor": { "kind": "speccoder" },
            "model": null,
            "run": null,
            "decisions": [],
            "strength": null,
            "instructions": [],
            "policy_sha256": "ef",
            "time": "2026-10-26T00:00:00Z"
        })
    }

    fn decision() -> Value {
        json!({
            "verdict": "attested",
            "unit": { "format": "speckit", "feature": "001-x", "dir": "specs/001-x" },
            "task": { "id": "T002", "line_hash": "ab", "title": "Accept it" },
            "tree": "t",
            "decider": { "by": "Ada", "reason": "checked by hand" },
            "policy_sha256": "ef",
            "time": "2026-10-26T00:00:00Z"
        })
    }

    fn round_trip(v: &Value) -> Value {
        let typed: TaskVerificationV0 = serde_json::from_value(v.clone()).unwrap();
        serde_json::to_value(&typed).unwrap()
    }

    #[test]
    fn statements_of_both_shapes_round_trip_unchanged() {
        assert_eq!(round_trip(&tick()), tick());
        assert_eq!(round_trip(&decision()), decision());
    }

    #[test]
    fn unknown_fields_and_a_newer_revision_survive_and_are_named() {
        let mut v = tick();
        v["revision"] = json!(7);
        v["provenance"] = json!({ "x": 1 });
        v["bug"] = json!({ "triad_sha256": "aa" });
        assert_eq!(round_trip(&v), v);
        let typed: TaskVerificationV0 = serde_json::from_value(v).unwrap();
        assert_eq!(typed.newer_fields(), ["provenance", "revision 7"]);
        assert_eq!(typed.reserved_present(), ["bug"]);
    }

    #[test]
    fn a_statement_without_revision_is_revision_one() {
        let typed: TaskVerificationV0 = serde_json::from_value(tick()).unwrap();
        assert_eq!(typed.revision(), 1);
        assert!(typed.newer_fields().is_empty());
    }

    #[test]
    fn a_missing_required_field_does_not_parse() {
        let mut v = tick();
        v.as_object_mut().unwrap().remove("tree");
        assert!(serde_json::from_value::<TaskVerificationV0>(v).is_err());
    }
}
