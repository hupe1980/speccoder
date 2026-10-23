//! The format-neutral view of spec-driven work (F-050, D-059): which format
//! a project is written in, and the requirements and acceptance scenarios a
//! unit of work states. Spec Kit, OpenSpec and Kiro adapters produce it;
//! the judge reads only it.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A spec-driven format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
    /// GitHub Spec Kit (`.specify/`, `specs/NNN-name/`).
    #[default]
    Speckit,
    /// OpenSpec (`openspec/changes/<id>/`).
    Openspec,
    /// Kiro and cc-sdd (`.kiro/specs/<name>/`).
    Kiro,
}

impl FormatKind {
    /// Every format, in the order detection reports them.
    pub const ALL: [Self; 3] = [Self::Speckit, Self::Openspec, Self::Kiro];

    /// The name in `--format` and `[spec] format`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Speckit => "speckit",
            Self::Openspec => "openspec",
            Self::Kiro => "kiro",
        }
    }

    /// The name a person reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Speckit => "Spec Kit",
            Self::Openspec => "OpenSpec",
            Self::Kiro => "Kiro",
        }
    }
}

impl FormatKind {
    /// Whether this is the reference format (Spec Kit).
    #[must_use]
    pub fn is_reference(&self) -> bool {
        *self == Self::Speckit
    }
}

impl fmt::Display for FormatKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for FormatKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| format!("`{s}` is not a format (speckit, openspec or kiro)"))
    }
}

/// Where a part of an artifact is: its file and byte range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    /// Path relative to the project root.
    pub path: String,
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

/// One acceptance scenario of a requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable within the unit: `US1/AC2`, `greeting/Default greeting/No name given`, `1.2`.
    pub key: String,
    /// Its text as written.
    pub raw: String,
    /// Where it is.
    pub source: SourceRange,
}

/// A scenario a task's acceptance tests cite: `US1-2` (Spec Kit), or
/// `capability: requirement / scenario` (OpenSpec), with the spec's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioRef {
    /// Stable within the unit.
    pub id: String,
    /// The scenario as written in the spec.
    pub text: String,
}

/// One requirement (a Spec Kit user story, an OpenSpec requirement, a Kiro
/// requirement) and its scenarios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Stable within the unit: `US1`, `greeting/Default greeting`, `3`.
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its whole text.
    pub text: String,
    /// Its acceptance scenarios, in document order.
    pub scenarios: Vec<Scenario>,
    /// Where it is.
    pub source: SourceRange,
}

/// What an OpenSpec project adds to its snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenSpecMeta {
    /// The workflow schema (`spec-driven` is the one speccoder runs).
    pub schema: String,
    /// `context` of `openspec/config.yaml`, if any.
    pub context: Option<String>,
    /// What a unit of work is called.
    pub unit_kind: String,
    /// The selected change's delta requirements, in file order.
    pub requirements: Vec<DeltaRequirement>,
}

/// One requirement of a change's spec delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeltaRequirement {
    /// The capability (`openspec/specs/<capability>/spec.md`).
    pub capability: String,
    /// `added`, `modified`, `removed` or `renamed`.
    pub operation: String,
    /// The requirement's name (the new name for a rename).
    pub name: String,
    /// Its scenarios' names.
    pub scenarios: Vec<String>,
    /// The delta file, relative to the project root.
    pub path: String,
    /// Its heading's line.
    pub line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_parse_and_serialize_by_name() {
        for k in FormatKind::ALL {
            assert_eq!(k.name().parse::<FormatKind>(), Ok(k));
            assert_eq!(serde_json::to_value(k).unwrap_or_default(), k.name());
        }
        let err = "bogus".parse::<FormatKind>().unwrap_err();
        assert_eq!(err, "`bogus` is not a format (speckit, openspec or kiro)");
    }
}
