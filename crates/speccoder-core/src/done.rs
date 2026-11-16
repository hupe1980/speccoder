//! The definition of done: one line per part a change must have before it
//! ships — a changelog entry, docs that link and read right, current
//! screenshots, no stale version, green CI — each done, not done, or
//! can't tell, with what it saw and how to fix it.

use serde::{Deserialize, Serialize};

/// Findings shown per line; the rest are counted.
pub const FINDINGS_SHOWN: usize = 20;

/// A part of the definition of done, in the order they are shown.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DonePart {
    /// `[Unreleased]` has an entry when something user-facing changed.
    Changelog,
    /// Every relative and site-internal link in the docs resolves.
    Links,
    /// No screenshot is older than what it shows.
    Screenshots,
    /// Docs describe what is there: no history, dates or backlog.
    Tone,
    /// No docs page names the last release's version after a bump.
    Versions,
    /// GitHub's checks for this commit.
    Ci,
}

impl DonePart {
    /// Its name as shown.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Changelog => "Changelog",
            Self::Links => "Links",
            Self::Screenshots => "Screenshots",
            Self::Tone => "Tone",
            Self::Versions => "Versions",
            Self::Ci => "CI",
        }
    }
}

/// How a part stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoneState {
    /// Done, by evidence.
    Done,
    /// Not done: its fixes say what to do.
    NotDone,
    /// The evidence is not there (not pushed, no access, no base).
    CantTell,
}

/// One thing a line found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoneFinding {
    /// Repository-relative path.
    pub path: String,
    /// 1-based line, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// What it is: the phrase, the missing target, the newer source.
    pub what: String,
}

/// One part of the definition of done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoneLine {
    /// Which part.
    pub part: DonePart,
    /// How it stands.
    pub state: DoneState,
    /// What it saw, in one line.
    pub summary: String,
    /// The first findings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<DoneFinding>,
    /// Findings beyond those shown.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: usize,
    /// What to do, commands as typed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl DoneLine {
    /// A line; findings beyond [`FINDINGS_SHOWN`] are counted in `more`.
    #[must_use]
    pub fn new(part: DonePart, state: DoneState, summary: impl Into<String>) -> Self {
        Self {
            part,
            state,
            summary: summary.into(),
            findings: Vec::new(),
            more: 0,
            fixes: Vec::new(),
        }
    }

    /// With these findings, the first [`FINDINGS_SHOWN`] kept.
    #[must_use]
    pub fn with_findings(mut self, mut findings: Vec<DoneFinding>) -> Self {
        self.more = findings.len().saturating_sub(FINDINGS_SHOWN);
        findings.truncate(FINDINGS_SHOWN);
        self.findings = findings;
        self
    }

    /// With these fixes.
    #[must_use]
    pub fn with_fixes<I: IntoIterator<Item = S>, S: Into<String>>(mut self, fixes: I) -> Self {
        self.fixes = fixes.into_iter().map(Into::into).collect();
        self
    }
}

/// Result of `speccoder done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoneReport {
    /// What it compared with: a branch, tag or commit; none found.
    pub base: Option<String>,
    /// One line per part with something to judge, in [`DonePart`] order.
    pub lines: Vec<DoneLine>,
    /// No line is not done.
    pub done: bool,
}

impl DoneReport {
    /// The report of these lines.
    #[must_use]
    pub fn new(base: Option<String>, mut lines: Vec<DoneLine>) -> Self {
        lines.sort_by_key(|l| l.part);
        let done = lines.iter().all(|l| l.state != DoneState::NotDone);
        Self { base, lines, done }
    }

    /// The lines not done.
    pub fn open(&self) -> impl Iterator<Item = &DoneLine> {
        self.lines.iter().filter(|l| l.state == DoneState::NotDone)
    }

    /// The report as a pull request body section; `forced`: handed off
    /// with lines open.
    #[must_use]
    pub fn markdown(&self, forced: bool) -> String {
        let mut out = String::from("\n### Definition of done\n\n");
        for l in &self.lines {
            let mark = if l.state == DoneState::Done { "x" } else { " " };
            let note = match l.state {
                DoneState::NotDone if forced => " (handed off anyway)",
                DoneState::CantTell => " (can't tell)",
                _ => "",
            };
            out.push_str(&format!(
                "- [{mark}] {} — {}{note}\n",
                l.part.title(),
                l.summary
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_beyond_twenty_are_counted() {
        let f = (0..23)
            .map(|i| DoneFinding {
                path: format!("p{i}"),
                line: None,
                what: "x".into(),
            })
            .collect();
        let l = DoneLine::new(DonePart::Links, DoneState::NotDone, "s").with_findings(f);
        assert_eq!((l.findings.len(), l.more), (20, 3));
    }

    #[test]
    fn a_report_is_done_unless_a_line_is_not_done_and_lists_in_order() {
        let r = DoneReport::new(
            None,
            vec![
                DoneLine::new(DonePart::Ci, DoneState::CantTell, "not pushed"),
                DoneLine::new(DonePart::Changelog, DoneState::Done, "3 entries"),
            ],
        );
        assert!(r.done);
        assert_eq!(r.lines[0].part, DonePart::Changelog);
        let md = r.markdown(false);
        assert!(md.contains("- [x] Changelog — 3 entries"), "{md}");
        assert!(md.contains("- [ ] CI — not pushed (can't tell)"), "{md}");
    }
}
