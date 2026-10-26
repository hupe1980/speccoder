//! The ladder: where a unit of work came from. One provenance
//! record per subject, written once. This build knows the variants
//! a bug writes; later rungs add theirs as variants.

use serde::{Deserialize, Serialize};

use crate::actions::Via;

/// Where a unit or a requirement came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// Recorded as a bug (`speccoder bug`).
    Bug {
        /// The unit.
        unit: String,
        /// The requirement.
        requirement: String,
        /// The task.
        task: String,
        /// The triad's digest.
        triad_sha256: String,
        /// Who.
        by: String,
        /// Through what.
        via: Via,
        /// Unix seconds.
        at: u64,
    },
    /// From a GitHub issue.
    Issue {
        /// Its number.
        number: u64,
        /// `owner/repo`.
        repo: Option<String>,
        /// The digest of its context-manifest entry.
        context_sha256: Option<String>,
    },
}

impl Origin {
    /// One line for `status` and the trace.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Bug {
                requirement,
                by,
                via,
                ..
            } => format!("bug {requirement}, recorded by {by} ({via})"),
            Self::Issue { number, repo, .. } => match repo {
                Some(r) => format!("issue {r}#{number}"),
                None => format!("issue #{number}"),
            },
        }
    }
}

/// The rung a unit stands on: a quick change (L1), a bug (L2) or
/// a feature. A quick-change or bug unit's pipeline authors and confirms
/// its own acceptance, so the review writes no tests for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    /// A quick change's standing unit (`speccoder do`).
    Quick,
    /// A bug's home: a standing bug-fix unit or a bug of the format's own
    /// bug structure.
    Bug,
    /// Any other unit.
    Feature,
}

impl Rung {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Bug => "bug",
            Self::Feature => "feature",
        }
    }
}

/// The rung of `unit`, from the standing units the format names for the
/// project (`(unit, rung)`): a standing unit, or a unit under one
/// (`<standing>/<slug>`), is on its rung; every other unit is a feature.
#[must_use]
pub fn rung_of(unit: &str, standing: &[(String, Rung)]) -> Rung {
    standing
        .iter()
        .find(|(name, _)| {
            unit == name
                || unit
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .map_or(Rung::Feature, |(_, r)| *r)
}

/// The subject a bug's origin is recorded for: `<unit>/<requirement>`.
#[must_use]
pub fn subject(unit: &str, requirement: &str) -> String {
    format!("{unit}/{requirement}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_carry_their_kind_and_read_back() {
        let o = Origin::Issue {
            number: 42,
            repo: Some("o/r".to_owned()),
            context_sha256: None,
        };
        let v = serde_json::to_value(&o).unwrap_or_default();
        assert_eq!(v["kind"], "issue");
        assert_eq!(serde_json::from_value::<Origin>(v).ok(), Some(o.clone()));
        assert_eq!(o.describe(), "issue o/r#42");
        assert_eq!(subject("007-bugfixes", "FR-003"), "007-bugfixes/FR-003");
    }

    #[test]
    fn every_rung_is_decided_by_the_standing_units() {
        let standing = vec![
            ("004-quick-changes".to_owned(), Rung::Quick),
            ("007-bugfixes".to_owned(), Rung::Bug),
            (".specify/bugs".to_owned(), Rung::Bug),
        ];
        assert_eq!(rung_of("004-quick-changes", &standing), Rung::Quick);
        assert_eq!(rung_of("007-bugfixes", &standing), Rung::Bug);
        assert_eq!(rung_of(".specify/bugs/empty-title", &standing), Rung::Bug);
        assert_eq!(rung_of("007-bugfixes-extra", &standing), Rung::Feature);
        assert_eq!(rung_of("001-greet", &standing), Rung::Feature);
        assert_eq!(rung_of("001-greet", &[]), Rung::Feature);
        assert_eq!(Rung::Quick.as_str(), "quick");
    }
}
