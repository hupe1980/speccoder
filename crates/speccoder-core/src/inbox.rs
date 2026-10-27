//! The kinds of Needs You items: one registry, each kind with its group
//! and urgency rank. A feature adds a kind here, never a free
//! string; the runtime's list is derived from it.

use serde::{Deserialize, Serialize};

/// Where an item puts the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Nothing moves until the person decides.
    BlockedOnYou,
    /// Work is done or held; the person reviews it.
    ReadyToReview,
}

impl Group {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BlockedOnYou => "blocked_on_you",
            Self::ReadyToReview => "ready_to_review",
        }
    }

    /// As people read it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::BlockedOnYou => "Blocked on you",
            Self::ReadyToReview => "Ready to review",
        }
    }
}

/// A kind of decision waiting for a person, most urgent first.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The calls waiting in a journal could not be read.
    Unknown,
    /// A run waits on an approval.
    Approval,
    /// A run's batch of calls waits for one decision.
    ApprovalBatch,
    /// A verified attempt is held: its tests reject what the spec allows.
    Ruling,
    /// A bug's reproductions wait for the person's confirmation.
    BugConfirm,
    /// A bug could not be reproduced.
    BugCannotReproduce,
    /// An *Unchanged* statement of a bug does not hold today.
    BugUnchangedNotHolding,
    /// A special case survived a bug's reproductions.
    BugReproWeak,
    /// The monitor suspects a shortcut.
    Gaming,
    /// Most of a feature's ticks are regression-only: one diagnosis.
    Unverified,
    /// Proven against an earlier wording.
    Stale,
    /// The spec leaves questions open.
    Question,
    /// Two independent test authors read a requirement differently: the
    /// person says which reading is meant, or rewords it.
    Ambiguous,
    /// Requirements not approved on their current wording.
    Review,
    /// Changes requested on the spec.
    Revise,
    /// The last attempt did not pass.
    Stuck,
    /// An autopilot run refused calls it would have asked about: listed
    /// for the person afterwards.
    AutopilotDenied,
    /// A bug is already fixed: keep its reproduction, or drop it.
    BugAlreadySatisfied,
    /// Changes a run set aside.
    Aside,
    /// Planning files a run cannot read yet.
    Commit,
    /// Every task done: review and hand it off.
    Handoff,
}

impl Kind {
    /// Every kind, the most urgent first.
    pub const ALL: [Self; 21] = [
        Self::Unknown,
        Self::Approval,
        Self::ApprovalBatch,
        Self::Ruling,
        Self::BugConfirm,
        Self::BugCannotReproduce,
        Self::BugUnchangedNotHolding,
        Self::BugReproWeak,
        Self::Gaming,
        Self::Unverified,
        Self::Stale,
        Self::Question,
        Self::Ambiguous,
        Self::Review,
        Self::Revise,
        Self::Stuck,
        Self::AutopilotDenied,
        Self::BugAlreadySatisfied,
        Self::Aside,
        Self::Commit,
        Self::Handoff,
    ];

    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Approval => "approval",
            Self::ApprovalBatch => "approval_batch",
            Self::Ruling => "ruling",
            Self::BugConfirm => "bug_confirm",
            Self::BugCannotReproduce => "bug_cannot_reproduce",
            Self::BugUnchangedNotHolding => "bug_unchanged_not_holding",
            Self::BugReproWeak => "bug_repro_weak",
            Self::Gaming => "gaming",
            Self::Unverified => "unverified",
            Self::Stale => "stale",
            Self::Question => "question",
            Self::Ambiguous => "ambiguous",
            Self::Review => "review",
            Self::Revise => "revise",
            Self::Stuck => "stuck",
            Self::AutopilotDenied => "autopilot_denied",
            Self::BugAlreadySatisfied => "bug_already_satisfied",
            Self::Aside => "aside",
            Self::Commit => "commit",
            Self::Handoff => "handoff",
        }
    }

    /// Its group.
    #[must_use]
    pub const fn group(self) -> Group {
        match self {
            Self::Unknown
            | Self::Approval
            | Self::ApprovalBatch
            | Self::Ruling
            | Self::BugConfirm
            | Self::BugCannotReproduce
            | Self::BugUnchangedNotHolding
            | Self::BugReproWeak
            | Self::Gaming
            | Self::Question
            | Self::Ambiguous
            | Self::Revise
            | Self::Stuck
            | Self::Commit => Group::BlockedOnYou,
            Self::Unverified
            | Self::Stale
            | Self::AutopilotDenied
            | Self::Review
            | Self::BugAlreadySatisfied
            | Self::Aside
            | Self::Handoff => Group::ReadyToReview,
        }
    }

    /// Its urgency: lower is more urgent.
    #[must_use]
    pub fn rank(self) -> usize {
        Self::ALL
            .iter()
            .position(|k| *k == self)
            .unwrap_or(Self::ALL.len())
    }

    /// The kind recorded as `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == name)
    }

    /// Every recorded name, the most urgent first.
    #[must_use]
    pub fn names() -> [&'static str; 21] {
        Self::ALL.map(Self::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_name_a_group_and_its_place() {
        let schema = serde_json::to_value(schemars::schema_for!(Kind)).unwrap_or_default();
        let mut named: Vec<String> = schema["oneOf"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["const"].as_str().map(str::to_owned))
            .collect();
        named.sort();
        let mut listed: Vec<String> = Kind::names().iter().map(|n| (*n).to_owned()).collect();
        listed.sort();
        assert_eq!(named, listed, "Kind::ALL is not the enum");
        for (i, k) in Kind::ALL.iter().enumerate() {
            assert_eq!(k.rank(), i);
            assert_eq!(Kind::named(k.as_str()), Some(*k));
            assert_eq!(serde_json::to_value(k).unwrap_or_default(), k.as_str());
        }
        // Today's kinds keep their order among themselves.
        let today: Vec<&str> = Kind::names()
            .into_iter()
            .filter(|n| {
                !n.starts_with("bug_")
                    && !matches!(*n, "ambiguous" | "autopilot_denied" | "unverified")
            })
            .collect();
        assert_eq!(
            today,
            [
                "unknown",
                "approval",
                "approval_batch",
                "ruling",
                "gaming",
                "stale",
                "question",
                "review",
                "revise",
                "stuck",
                "aside",
                "commit",
                "handoff"
            ]
        );
        assert_eq!(Kind::BugAlreadySatisfied.group(), Group::ReadyToReview);
        assert_eq!(Kind::Ambiguous.group(), Group::BlockedOnYou);
        assert_eq!(Kind::AutopilotDenied.group(), Group::ReadyToReview);
        // First of its group: before `stale`.
        assert_eq!(Kind::Unverified.group(), Group::ReadyToReview);
        assert_eq!(Kind::Unverified.rank() + 1, Kind::Stale.rank());
        for k in [
            Kind::BugConfirm,
            Kind::BugCannotReproduce,
            Kind::BugUnchangedNotHolding,
            Kind::BugReproWeak,
        ] {
            assert_eq!(k.group(), Group::BlockedOnYou, "{k:?}");
        }
    }
}
