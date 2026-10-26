//! The ladder (D-103): where a unit of work came from. One provenance
//! record per subject, written once (D-126). This build knows the variants
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
}
