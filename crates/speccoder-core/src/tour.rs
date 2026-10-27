//! The evidence tour: one task's evidence as six steps in a fixed order —
//! requirement, frozen tests, red, diff, green, strength — each saying
//! where it comes from. The payload `evidence_tour`, and its Markdown (the
//! check run, the pull-request body); the app renders the same JSON.

use serde::Serialize;

/// A step of the tour, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// The requirement passage the task cites, as the tick's commit has it.
    Requirement,
    /// The acceptance tests frozen before the attempt.
    FrozenTests,
    /// The tests failing first, and how.
    Red,
    /// The task commit's diff.
    Diff,
    /// What passed after: acceptance, held-out, the regression checks.
    Green,
    /// How well the tests tell right from wrong.
    Strength,
}

impl StepKind {
    /// The six, in order.
    pub const ALL: [Self; 6] = [
        Self::Requirement,
        Self::FrozenTests,
        Self::Red,
        Self::Diff,
        Self::Green,
        Self::Strength,
    ];

    /// The step's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Requirement => "Requirement",
            Self::FrozenTests => "Frozen tests",
            Self::Red => "Red",
            Self::Diff => "Diff",
            Self::Green => "Green",
            Self::Strength => "Strength",
        }
    }
}

/// Where a step's content comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A signed statement says it (and what is shown hashes to it).
    Signed,
    /// The commit holds it (git objects, not a signature).
    Committed,
    /// Nothing recorded it.
    NotRecorded,
}

impl Source {
    /// The label shown beside the step.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Signed => "signed",
            Self::Committed => "committed",
            Self::NotRecorded => "not recorded",
        }
    }
}

/// One step.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TourStep {
    /// Which.
    pub kind: StepKind,
    /// Where it comes from.
    pub source: Source,
    /// Its title (`Requirement`, `Frozen tests`, …).
    pub title: String,
    /// What it shows, Markdown: a first line, then optional block lines.
    pub body_markdown: String,
    /// The facts shown, as data (names, counts, digests).
    pub facts: serde_json::Value,
}

/// Result of `speccoder attest tour` and `/api/tour`: one task's tour.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct EvidenceTour {
    /// The unit.
    pub feature: String,
    /// Task ID.
    pub task: String,
    /// The task's title.
    pub title: String,
    /// The verdict the evidence claims, when any.
    pub verdict: Option<String>,
    /// The task commit, when one carries the task's evidence.
    pub commit: Option<String>,
    /// The six steps, in order — only the first for an unverifiable tick.
    pub steps: Vec<TourStep>,
    /// For an unverifiable tick: why the evidence does not count.
    pub unverifiable: Option<String>,
}

impl EvidenceTour {
    /// The tour as collapsed Markdown (`<details>`), as the check run and
    /// the pull-request body show it.
    #[must_use]
    pub fn markdown(&self, verdict_words: &str) -> String {
        let mut out = format!(
            "<details><summary><b>{}</b> {} — {}</summary>\n\n",
            self.task,
            escape(&self.title),
            verdict_words
        );
        for (i, step) in self.steps.iter().enumerate() {
            let mut lines = step.body_markdown.lines();
            out.push_str(&format!(
                "{}. **{}** · {} — {}\n",
                i + 1,
                step.title,
                step.source.label(),
                lines.next().unwrap_or_default()
            ));
            for line in lines {
                if line.is_empty() {
                    out.push('\n');
                } else {
                    out.push_str(&format!("   {line}\n"));
                }
            }
        }
        if let Some(why) = &self.unverifiable {
            out.push_str(&format!("\nUnverifiable: {why}. The steps after the requirement are not shown: nothing verifiable backs them.\n"));
        }
        out.push_str("</details>\n");
        out
    }
}

/// `<`, `>` and `&` as entities: a title is text, never markup.
#[must_use]
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_numbers_the_steps_with_their_sources_and_indents_blocks() {
        let tour = EvidenceTour {
            feature: "001-x".to_owned(),
            task: "T012".to_owned(),
            title: "Parse <header>".to_owned(),
            verdict: Some("verified".to_owned()),
            commit: Some("abc".to_owned()),
            steps: vec![
                TourStep {
                    kind: StepKind::Requirement,
                    source: Source::Signed,
                    title: "Requirement".to_owned(),
                    body_markdown: "FR-004\n> The parser MUST reject it.".to_owned(),
                    facts: serde_json::Value::Null,
                },
                TourStep {
                    kind: StepKind::Red,
                    source: Source::NotRecorded,
                    title: "Red".to_owned(),
                    body_markdown: "no red phase".to_owned(),
                    facts: serde_json::Value::Null,
                },
            ],
            unverifiable: None,
        };
        let md = tour.markdown("VERIFIED");
        assert!(
            md.starts_with("<details><summary><b>T012</b> Parse &lt;header&gt; — VERIFIED</summary>\n\n1. **Requirement** · signed — FR-004\n   > The parser MUST reject it.\n2. **Red** · not recorded — no red phase\n"),
            "{md}"
        );
        assert!(md.ends_with("</details>\n"));
    }
}
