//! What reached a model call (constitution VIII): one manifest of every
//! part handed to it — its kind, where it came from, its digest and size —
//! journaled with the call and named in the evidence. Producers add
//! entries through the runtime's one builder; nothing hashes or labels
//! context itself.

use serde::{Deserialize, Serialize};

/// What a part is. A new kind is a new variant, never a free string.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    /// The project's rules.
    Constitution,
    /// The unit's plan.
    Plan,
    /// The requirement's text (a bug's triad).
    Story,
    /// The task.
    Task,
    /// A person's rejection reason.
    Rejection,
    /// The verifier's digest of an earlier attempt.
    RetryDigest,
    /// A GitHub issue's title and body (untrusted).
    Issue,
}

/// One part handed to a call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextEntry {
    /// What it is.
    pub kind: ContextKind,
    /// Where it came from: `person`, `worktree`, `speccoder://<producer>/<name>`,
    /// `github:<owner>/<repo>#<n>`.
    pub source: String,
    /// A path, an id or a part pointer (`/bundle/plan`).
    pub reference: String,
    /// Hex SHA-256 of the exact bytes given.
    pub sha256: String,
    /// Their size.
    pub bytes: u64,
    /// Cut to a budget.
    pub cut: bool,
}

/// Every part of one call, sorted by kind and reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextManifest {
    /// The parts.
    pub entries: Vec<ContextEntry>,
    /// Hex SHA-256 of [`ContextManifest::canonical`].
    pub digest: String,
}

impl ContextManifest {
    /// The entries' canonical JSON (sorted): what the digest is over.
    #[must_use]
    pub fn canonical(entries: &[ContextEntry]) -> String {
        let mut sorted = entries.to_vec();
        sorted.sort_by(|a, b| (a.kind, &a.reference).cmp(&(b.kind, &b.reference)));
        serde_json::to_string(&sorted).unwrap_or_default()
    }

    /// The entries, sorted, with the digest `hash` gives over them.
    #[must_use]
    pub fn of(mut entries: Vec<ContextEntry>, hash: impl Fn(&[u8]) -> String) -> Self {
        let digest = hash(Self::canonical(&entries).as_bytes());
        entries.sort_by(|a, b| (a.kind, &a.reference).cmp(&(b.kind, &b.reference)));
        Self { entries, digest }
    }

    /// The entries of `kind`.
    pub fn of_kind(&self, kind: ContextKind) -> impl Iterator<Item = &ContextEntry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ContextKind, reference: &str) -> ContextEntry {
        ContextEntry {
            kind,
            source: "person".to_owned(),
            reference: reference.to_owned(),
            sha256: "00".to_owned(),
            bytes: 2,
            cut: false,
        }
    }

    #[test]
    fn the_digest_does_not_depend_on_the_order_parts_were_added() {
        let h = |b: &[u8]| format!("{}", b.len());
        let a = ContextManifest::of(
            vec![
                entry(ContextKind::Issue, "#42"),
                entry(ContextKind::Story, "/triad"),
            ],
            h,
        );
        let b = ContextManifest::of(
            vec![
                entry(ContextKind::Story, "/triad"),
                entry(ContextKind::Issue, "#42"),
            ],
            h,
        );
        assert_eq!(a, b);
        assert_eq!(a.entries[0].kind, ContextKind::Story);
        assert_eq!(a.of_kind(ContextKind::Issue).count(), 1);
    }
}
