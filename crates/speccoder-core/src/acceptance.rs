//! Acceptance tests decided before the run: one record for every producer
//! — a review that froze the oracle (spec 093) and a bug's confirmed
//! reproduction (spec 090). The fix's checkpoint reuses the record instead
//! of calling the acceptance author; a change of the wording it was decided
//! on lapses it.

use serde::{Deserialize, Serialize};

use crate::actions::Via;

/// Who produced a pre-frozen set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Producer {
    /// A person froze the oracle in review.
    Review,
    /// A person confirmed a bug's authored reproductions.
    BugConfirmation,
    /// A person named the bug's reproduction tests (`--test`).
    PersonTest,
}

impl Producer {
    /// The acceptance source a run reports for it.
    #[must_use]
    pub const fn source(self) -> AcceptanceSource {
        match self {
            Self::Review => AcceptanceSource::Review,
            Self::BugConfirmation => AcceptanceSource::BugConfirmation,
            Self::PersonTest => AcceptanceSource::PersonTest,
        }
    }

    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::BugConfirmation => "bug_confirmation",
            Self::PersonTest => "person_test",
        }
    }
}

/// Where a task's visible acceptance set came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceSource {
    /// Written by the acceptance author at the run's checkpoint.
    Authored,
    /// Frozen in review.
    Review,
    /// A bug's confirmed reproductions.
    BugConfirmation,
    /// Tests the person named for a bug.
    PersonTest,
}

/// One visible test of the set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VisibleTest {
    /// Project-relative path.
    pub path: String,
    /// `sha256:<hex>` of its content.
    pub sha256: String,
    /// The scenario or statement ids it cites; never empty.
    pub cites: Vec<String>,
}

/// A sealed held-out set: kept in the state directory, never shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SealedRef {
    /// The requirement it is keyed by.
    pub key: String,
    /// The manifest's hash.
    pub manifest_sha256: String,
    /// Files in it.
    pub count: usize,
}

/// Who decided, through what, when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Decided {
    /// The person.
    pub by: String,
    /// The interface.
    pub via: Via,
    /// Unix seconds.
    pub at: u64,
}

/// Acceptance tests a person decided before the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreFrozenAcceptance {
    /// The unit, as the adapter names it.
    pub unit: String,
    /// The requirement key (`US1`, `FR-003`, `bug:<slug>`).
    pub requirement: String,
    /// The wording it was decided on: a different current digest lapses it.
    pub wording_digest: String,
    /// The decision's digest (a review's approval, a bug's confirmation).
    pub approval_digest: String,
    /// The visible tests.
    pub visible: Vec<VisibleTest>,
    /// The held-out set, unless `[acceptance] held_out = false`.
    pub held_out_sealed: Option<SealedRef>,
    /// Who produced it.
    pub producer: Producer,
    /// The decision.
    pub decided: Decided,
}

impl PreFrozenAcceptance {
    /// Whether it is well formed: a visible test at least, each citing
    /// something.
    ///
    /// # Errors
    ///
    /// What is missing.
    pub fn validate(&self) -> Result<(), String> {
        if self.visible.is_empty() {
            return Err("a pre-frozen set needs at least one visible test".to_owned());
        }
        if let Some(t) = self.visible.iter().find(|t| t.cites.is_empty()) {
            return Err(format!("{} cites nothing", t.path));
        }
        Ok(())
    }

    /// Whether it still stands for the wording as it reads now ([`current`]).
    #[must_use]
    pub fn is_current(&self, wording_now: &str) -> bool {
        current(self, wording_now)
    }

    /// The `Acceptance:` trailer: one form for every producer;
    /// `manifest` is the visible set's digest.
    #[must_use]
    pub fn trailer(&self, manifest: &str) -> String {
        format!(
            "Acceptance: {} manifest={manifest} approval={} wording={} by={}\n",
            self.producer.as_str(),
            self.approval_digest,
            self.wording_digest,
            self.decided
                .by
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

/// Whether `record` still stands for the wording as it reads now.
#[must_use]
pub fn current(record: &PreFrozenAcceptance, wording_now: &str) -> bool {
    record.wording_digest == wording_now
}

/// The text a visible set's manifest digest is over: `path=sha256` lines,
/// sorted by path.
#[must_use]
pub fn manifest_text(visible: &[VisibleTest]) -> String {
    let mut lines: Vec<String> = visible
        .iter()
        .map(|t| format!("{}={}", t.path, t.sha256))
        .collect();
    lines.sort();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn record(wording: &str) -> PreFrozenAcceptance {
        PreFrozenAcceptance {
            unit: "007-bugfixes".to_owned(),
            requirement: "FR-003".to_owned(),
            wording_digest: wording.to_owned(),
            approval_digest: "sha256:a".to_owned(),
            visible: vec![VisibleTest {
                path: "tests/repro_e1.rs".to_owned(),
                sha256: "sha256:t".to_owned(),
                cites: vec!["E1".to_owned()],
            }],
            held_out_sealed: None,
            producer: Producer::BugConfirmation,
            decided: Decided {
                by: "Ada".to_owned(),
                via: Via::Terminal,
                at: 1,
            },
        }
    }

    proptest! {
        #[test]
        fn any_wording_change_lapses_it(a in "[a-z0-9:]{1,20}", b in "[a-z0-9:]{1,20}") {
            let r = record(&a);
            prop_assert!(current(&r, &a));
            prop_assert_eq!(current(&r, &b), a == b);
            prop_assert_eq!(r.is_current(&b), a == b);
        }
    }

    #[test]
    fn the_trailer_has_one_form_for_every_producer() {
        let r = record("sha256:w");
        assert_eq!(
            r.trailer("sha256:m"),
            "Acceptance: bug_confirmation manifest=sha256:m approval=sha256:a wording=sha256:w by=Ada\n"
        );
        assert!(r.validate().is_ok());
        let mut empty = r;
        empty.visible[0].cites.clear();
        assert!(empty.validate().is_err());
        assert_eq!(Producer::PersonTest.source(), AcceptanceSource::PersonTest);
    }
}
