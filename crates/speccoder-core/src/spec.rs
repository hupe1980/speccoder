//! The format-neutral view of spec-driven work: which format
//! a project is written in, and the requirements and acceptance scenarios a
//! unit of work states. The Spec Kit, OpenSpec and Kiro adapters produce it;
//! the judge reads only it.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A spec-driven format.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceRange {
    /// Path relative to the project root.
    pub path: String,
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

/// One acceptance scenario of a requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScenarioRef {
    /// Stable within the unit.
    pub id: String,
    /// The scenario as written in the spec.
    pub text: String,
}

/// One requirement (a Spec Kit user story, an OpenSpec requirement, a Kiro
/// requirement) and its scenarios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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

/// The scenario ids a task's line names — `(scenarios US3-1, US3-2)`, any
/// case, separated by commas, spaces, `and` or `&`, or cited as Spec Kit's
/// converge cites them, `US3/AC2` — upper-cased, in written order, once
/// each. Inline code is quoted text, never a link.
#[must_use]
pub fn scenario_links(line: &str) -> Vec<String> {
    let mut in_code = false;
    let plain: String = line
        .chars()
        .map(|c| {
            if c == '`' {
                in_code = !in_code;
                ' '
            } else if in_code {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Matched on the text itself, ASCII case folded: lower-casing other
    // characters changes their byte length (`İ`), so its offsets are not
    // the text's.
    const KEY: &[u8] = b"(scenarios";
    let bytes = plain.as_bytes();
    let starts = (0..bytes.len().saturating_sub(KEY.len() - 1))
        .filter(|&i| bytes[i..i + KEY.len()].eq_ignore_ascii_case(KEY));
    let mut found: Vec<(usize, String)> = Vec::new();
    for i in starts {
        let rest = &plain[i + KEY.len()..];
        let Some(end) = rest.find(')') else { continue };
        let mut at = i + KEY.len();
        for word in
            rest[..end].split(|c: char| c == ',' || c == '&' || c == ':' || c.is_whitespace())
        {
            let id = word.to_uppercase();
            if is_scenario_id(&id) {
                found.push((at, id));
            }
            at += word.len() + 1;
        }
    }
    found.extend(cited(&plain));
    found.sort_by_key(|(at, _)| *at);
    let mut out: Vec<String> = Vec::new();
    for (_, id) in found {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// `US3/AC2` citations, as `US3-2`, with where each starts: letters and
/// digits, `/AC`, digits, standing apart from the words around them.
fn cited(plain: &str) -> Vec<(usize, String)> {
    let bytes = plain.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = plain[i..].find(['/']) {
        let slash = i + off;
        i = slash + 1;
        let head_start = bytes[..slash]
            .iter()
            .rposition(|b| !b.is_ascii_alphanumeric())
            .map_or(0, |p| p + 1);
        let head = &plain[head_start..slash];
        let tail = &bytes[slash + 1..];
        if tail.len() < 3 || !tail[..2].eq_ignore_ascii_case(b"AC") {
            continue;
        }
        let digits = tail[2..].iter().take_while(|b| b.is_ascii_digit()).count();
        let after = tail.get(2 + digits);
        if digits == 0 || after.is_some_and(u8::is_ascii_alphanumeric) {
            continue;
        }
        let k = &plain[slash + 3..slash + 3 + digits];
        let id = format!("{}-{k}", head.to_ascii_uppercase());
        if is_scenario_id(&id) {
            out.push((head_start, id));
        }
    }
    out
}

/// `US3-1`: letters, digits, `-`, digits.
fn is_scenario_id(s: &str) -> bool {
    let Some((head, n)) = s.split_once('-') else {
        return false;
    };
    let letters = head.chars().take_while(char::is_ascii_alphabetic).count();
    letters > 0
        && head.len() > letters
        && head[letters..].chars().all(|c| c.is_ascii_digit())
        && !n.is_empty()
        && n.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_names_its_scenarios_in_its_line() {
        assert_eq!(
            scenario_links("Add validation in src/p.py (scenarios US3-1, us3-2 and US3-1)"),
            ["US3-1", "US3-2"]
        );
        assert_eq!(
            scenario_links("Do X (Scenarios: US1-1 & US1-3)"),
            ["US1-1", "US1-3"]
        );
        assert!(scenario_links("Write `(scenarios US1-1)` in the docs").is_empty());
        assert!(scenario_links("Cover the scenarios US1-1").is_empty());
        assert!(scenario_links("(scenarios US1, 3-1, US-1, US1-)").is_empty());
        // Characters whose lower case is longer or shorter keep the offsets.
        assert_eq!(
            scenario_links("İİİ (scenarios US1-1, US1-2)"),
            ["US1-1", "US1-2"]
        );
        assert_eq!(scenario_links("İİ (scenarios ü US1-1)"), ["US1-1"]);
        assert_eq!(
            scenario_links("\u{212A}\u{212A} (SCENARIOS us2-3)"),
            ["US2-3"]
        );
    }

    #[test]
    fn a_converge_citation_names_a_scenario() {
        assert_eq!(
            scenario_links("Route tables per FR-008, US3/AC2, plan: single table (partial)"),
            ["US3-2"]
        );
        assert_eq!(
            scenario_links("Do it per us1/ac2 and (scenarios US1-1, US1-2)"),
            ["US1-2", "US1-1"]
        );
        assert!(scenario_links("Read `US1/AC2` and src/AC2 and US1/ACK and /AC1").is_empty());
        assert_eq!(scenario_links("İ US2/AC10."), ["US2-10"]);
    }

    proptest::proptest! {
        #[test]
        fn links_written_read_back(ids in proptest::collection::vec((1u8..20, 1u8..20), 1..6)) {
            let mut want: Vec<String> = Vec::new();
            for (s, k) in &ids {
                let id = format!("US{s}-{k}");
                if !want.contains(&id) {
                    want.push(id);
                }
            }
            let line = format!("Implement it in src/x.rs (scenarios {})", want.join(", "));
            proptest::prop_assert_eq!(scenario_links(&line), want);
        }
    }

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
