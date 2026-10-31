//! Explore's pure parts: the report a session's run ends with (a claim,
//! checked here), the brief the person's choice composes and its digest,
//! the session's states, and the record a resumed run carries. No IO: the
//! runtime stores sessions and runs the skills.

use serde::{Deserialize, Serialize};

use crate::actions::Via;

/// The most a brief may hold.
pub const BRIEF_MAX: usize = 16 * 1024;

/// What a resumed run carries of the session, at most.
pub const CARRIED_MAX: usize = 24_000;

/// The skill `spec explore` falls back to when the format has none.
pub const FALLBACK: &str = "speccoder.explore";

/// One option the model proposes: a claim, never a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Approach {
    /// A short name.
    pub title: String,
    /// What it is.
    pub summary: String,
    /// What it gives up.
    #[serde(default)]
    pub tradeoffs: Vec<String>,
    /// What could go wrong.
    #[serde(default)]
    pub risks: Vec<String>,
}

/// How a run of a session ended: the model's report, schema-checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExploreReport {
    /// The model's summary of the idea.
    pub summary: String,
    /// `done` or `incomplete`.
    pub status: String,
    /// The options.
    #[serde(default)]
    pub approaches: Vec<Approach>,
    /// What any approach must respect.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// What is still open.
    #[serde(default)]
    pub open_questions: Vec<String>,
}

/// A constraint or open question, by its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Item {
    /// Its words.
    pub text: String,
    /// `model`, `person` or `nobody` (a question nobody could answer).
    pub source: String,
}

/// A question the run asked and how it was answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QaItem {
    /// The model's question.
    pub question: String,
    /// The person's answer; none when nobody could answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Who answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// The person's choice: an approach and what they kept or added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Choice {
    /// 1-based, of the last report's approaches.
    pub approach: usize,
    /// Kept and added constraints.
    pub constraints: Vec<Item>,
    /// Kept and added open questions.
    pub open_questions: Vec<Item>,
    /// Who chose.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// `prompt` (picked at a terminal or in the app) or `flag` (`--choose`).
    pub basis: String,
    /// Unix seconds.
    pub at: u64,
    /// The brief's digest (hex SHA-256 of its body).
    pub digest: String,
    /// The brief's size.
    pub bytes: usize,
}

/// One run of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunRef {
    /// The agentplane run.
    pub run_id: String,
    /// `explore` or `resume`.
    pub kind: String,
    /// What the person added when resuming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
    /// How it ended.
    pub status: String,
}

/// The unit a session started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Started {
    /// The unit.
    pub unit: String,
    /// Unix seconds.
    pub at: u64,
}

/// One exploration of one idea in one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExploreSession {
    /// 1.
    pub version: u32,
    /// `ex-` and hex.
    pub id: String,
    /// The person's idea.
    pub idea: String,
    /// The skill that explores.
    pub skill: String,
    /// `flag`, `format` or `fallback`.
    pub selected_because: String,
    /// `open`, `chosen` or `started`.
    pub state: String,
    /// Unix seconds.
    pub created_at: u64,
    /// Unix seconds.
    pub updated_at: u64,
    /// Its runs, in order.
    #[serde(default)]
    pub runs: Vec<RunRef>,
    /// Each run's report, in order.
    #[serde(default)]
    pub reports: Vec<ExploreReport>,
    /// The questions asked and their answers.
    #[serde(default)]
    pub qa: Vec<QaItem>,
    /// The choices made, the last one standing.
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// The unit it started, once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<Started>,
}

/// One session in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionEntry {
    /// `ex-…`.
    pub id: String,
    /// The idea.
    pub idea: String,
    /// The skill that explores it.
    pub skill: String,
    /// `open`, `chosen` or `started`.
    pub state: String,
    /// Unix seconds.
    pub updated_at: u64,
    /// The chosen brief's digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// The unit it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// `spec explore`'s payload: one session as it stands, or the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExplorePayload {
    /// The session (none for a list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<ExploreSession>,
    /// The last run's approaches (the model's).
    #[serde(default)]
    pub approaches: Vec<Approach>,
    /// The constraints to keep or drop, by source.
    #[serde(default)]
    pub constraints: Vec<Item>,
    /// The open questions, by source.
    #[serde(default)]
    pub open_questions: Vec<Item>,
    /// The person's last choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
    /// The brief it made, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    /// The project's sessions, newest first (`--list`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Vec<SessionEntry>>,
}

impl ExploreSession {
    /// What the person chooses from now: the last report's approaches and
    /// its constraints and open questions (with the questions nobody
    /// could answer).
    #[must_use]
    pub fn on_the_table(&self) -> (Vec<Approach>, Vec<Item>, Vec<Item>) {
        let Some(r) = self.reports.last() else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let item = |text: &String, source: &str| Item {
            text: text.clone(),
            source: source.to_owned(),
        };
        let mut questions: Vec<Item> = r.open_questions.iter().map(|q| item(q, "model")).collect();
        for q in self.qa.iter().filter(|q| q.answer.is_none()) {
            if !questions.iter().any(|i| i.text == q.question) {
                questions.push(item(&q.question, "nobody"));
            }
        }
        (
            r.approaches.clone(),
            r.constraints.iter().map(|c| item(c, "model")).collect(),
            questions,
        )
    }

    /// Its line in a list.
    #[must_use]
    pub fn entry(&self) -> SessionEntry {
        SessionEntry {
            id: self.id.clone(),
            idea: self.idea.clone(),
            skill: self.skill.clone(),
            state: self.state.clone(),
            updated_at: self.updated_at,
            digest: self.choices.last().map(|c| c.digest.clone()),
            unit: self.started.as_ref().map(|s| s.unit.clone()),
        }
    }
}

/// A composed brief.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    /// The whole text, its header line first.
    pub text: String,
    /// Hex SHA-256 of the body (everything after the header line).
    pub digest: String,
    /// The text's size.
    pub bytes: usize,
}

/// Whether the fallback's report stands: at least three approaches with
/// distinct titles, each with a trade-off and a risk. A format's own skill
/// is not held to this (its shape is its own).
///
/// # Errors
///
/// What is missing, in words the model is told.
pub fn validate_fallback(r: &ExploreReport) -> Result<(), String> {
    if r.approaches.len() < 3 {
        return Err(format!(
            "give at least three approaches (you gave {})",
            r.approaches.len()
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for a in &r.approaches {
        let title = a.title.trim().to_lowercase();
        if title.is_empty() {
            return Err("every approach needs a title".to_owned());
        }
        if !seen.insert(title) {
            return Err(format!(
                "two approaches are both called \"{}\"",
                a.title.trim()
            ));
        }
        if a.tradeoffs.iter().all(|t| t.trim().is_empty()) {
            return Err(format!(
                "\"{}\" needs at least one trade-off",
                a.title.trim()
            ));
        }
        if a.risks.iter().all(|t| t.trim().is_empty()) {
            return Err(format!("\"{}\" needs at least one risk", a.title.trim()));
        }
    }
    Ok(())
}

fn bullets(out: &mut String, items: &[String]) {
    if items.is_empty() {
        out.push_str("- none\n");
    }
    for i in items {
        out.push_str(&format!("- {}\n", i.trim()));
    }
}

/// The brief the person's choice makes, deterministically: the approach
/// verbatim, then the kept constraints and open questions. `hash` is the
/// hex SHA-256 the runtime supplies; the digest covers the body only, so
/// the header can name it.
///
/// # Errors
///
/// The brief is larger than [`BRIEF_MAX`].
pub fn compose(
    approach: &Approach,
    constraints: &[Item],
    questions: &[Item],
    session: &str,
    hash: impl Fn(&[u8]) -> String,
) -> Result<Brief, String> {
    let mut body = String::new();
    body.push_str("\n## Approach\n\n");
    body.push_str(approach.title.trim());
    body.push('\n');
    body.push_str(approach.summary.trim());
    body.push('\n');
    if !approach.tradeoffs.is_empty() {
        body.push_str(&format!("Trade-offs: {}\n", approach.tradeoffs.join("; ")));
    }
    if !approach.risks.is_empty() {
        body.push_str(&format!("Risks: {}\n", approach.risks.join("; ")));
    }
    body.push_str("\n## Constraints\n\n");
    bullets(
        &mut body,
        &constraints
            .iter()
            .map(|i| i.text.clone())
            .collect::<Vec<_>>(),
    );
    body.push_str("\n## Open questions\n\n");
    bullets(
        &mut body,
        &questions.iter().map(|i| i.text.clone()).collect::<Vec<_>>(),
    );
    let digest = hash(body.as_bytes());
    let text = format!(
        "Explore brief {} (session {session})\n{body}",
        digest.get(..12).unwrap_or(&digest)
    );
    if text.len() > BRIEF_MAX {
        return Err(format!(
            "the brief is {} KiB; a brief is at most 16 KiB — shorten its constraints",
            text.len().div_ceil(1024)
        ));
    }
    let bytes = text.len();
    Ok(Brief {
        text,
        digest,
        bytes,
    })
}

/// The digest of a stored brief's body (after its header line).
#[must_use]
pub fn body_digest(text: &str, hash: impl Fn(&[u8]) -> String) -> String {
    let body = text.split_once('\n').map_or("", |(_, b)| b);
    hash(format!("\n{}", body.strip_prefix('\n').unwrap_or(body)).as_bytes())
}

/// What a session event does to its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The person chose an approach.
    Choose,
    /// A new run of the session.
    Resume,
    /// The spec was started from its brief.
    Start,
}

/// The state after `event` in `state`.
///
/// # Errors
///
/// The event does not apply: why, in words.
pub fn next_state(state: &str, event: Event) -> Result<&'static str, &'static str> {
    match (state, event) {
        ("open" | "chosen", Event::Choose) => Ok("chosen"),
        ("open" | "chosen", Event::Resume) => Ok("open"),
        ("chosen", Event::Start) => Ok("started"),
        ("open", Event::Start) => Err("choose an approach first"),
        ("started", _) => Err("this session already started a spec; explore again"),
        _ => Err("not a state of a session"),
    }
}

/// What a resumed run carries of the session: earlier questions with the
/// person's answers, earlier approaches (the model's), the person's notes
/// — verbatim, newest first, at most [`CARRIED_MAX`] bytes. Never a summary.
#[must_use]
pub fn carried(s: &ExploreSession) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = s.choices.last() {
        let notes: Vec<&str> = c
            .constraints
            .iter()
            .chain(&c.open_questions)
            .filter(|i| i.source == "person")
            .map(|i| i.text.as_str())
            .collect();
        if !notes.is_empty() {
            parts.push(format!(
                "<context kind=\"notes\" source=\"person\">\n{}\n</context>",
                notes.join("\n")
            ));
        }
    }
    for q in s.qa.iter().rev() {
        let answer = q.answer.as_deref().map_or_else(
            || "(nobody could answer)".to_owned(),
            |a| {
                format!(
                    "{a}\n(answered by {})",
                    q.by.as_deref().unwrap_or("the person")
                )
            },
        );
        parts.push(format!(
            "<context kind=\"question\" source=\"person\">\nQ: {}\nA: {answer}\n</context>",
            q.question
        ));
    }
    for r in s.reports.iter().rev() {
        let list: Vec<String> = r
            .approaches
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {} — {}", i + 1, a.title, a.summary))
            .collect();
        parts.push(format!(
            "<context kind=\"approaches\" source=\"model output\">\n{}\n</context>",
            list.join("\n")
        ));
    }
    let mut out = String::new();
    for p in parts {
        if out.len() + p.len() + 2 > CARRIED_MAX {
            break;
        }
        out.push_str(&p);
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn h(b: &[u8]) -> String {
        // A stand-in hash for the pure tests: the bytes' length and sum.
        format!(
            "{:032x}{:032x}",
            b.len(),
            b.iter().map(|x| u64::from(*x)).sum::<u64>()
        )
    }

    fn approach(t: &str) -> Approach {
        Approach {
            title: t.to_owned(),
            summary: format!("{t} summary"),
            tradeoffs: vec!["slower".to_owned()],
            risks: vec!["drift".to_owned()],
        }
    }

    fn report(n: usize) -> ExploreReport {
        ExploreReport {
            summary: "s".to_owned(),
            status: "done".to_owned(),
            approaches: (0..n).map(|i| approach(&format!("A{i}"))).collect(),
            constraints: vec![],
            open_questions: vec![],
        }
    }

    #[test]
    fn the_fallback_needs_three_distinct_approaches_each_with_a_cost() {
        assert!(validate_fallback(&report(3)).is_ok());
        assert!(validate_fallback(&report(2)).unwrap_err().contains("three"));
        let mut dup = report(3);
        dup.approaches[2].title = "a0".to_owned();
        assert!(validate_fallback(&dup).unwrap_err().contains("both called"));
        let mut free = report(3);
        free.approaches[1].risks.clear();
        assert!(validate_fallback(&free).unwrap_err().contains("risk"));
    }

    #[test]
    fn a_brief_names_its_digest_and_the_digest_covers_its_body() {
        let c = [Item {
            text: "no new dependency".to_owned(),
            source: "person".to_owned(),
        }];
        let b = compose(&approach("CSV"), &c, &[], "ex-1", h).unwrap();
        assert!(b.text.starts_with(&format!(
            "Explore brief {} (session ex-1)\n",
            &b.digest[..12]
        )));
        assert!(b.text.contains("## Approach\n\nCSV\nCSV summary\n"));
        assert!(b.text.contains("- no new dependency\n"));
        assert_eq!(body_digest(&b.text, h), b.digest);
        let changed = b.text.replace("CSV summary", "XLSX summary");
        assert_ne!(body_digest(&changed, h), b.digest);
    }

    #[test]
    fn states_follow_the_session() {
        assert_eq!(next_state("open", Event::Choose), Ok("chosen"));
        assert_eq!(next_state("chosen", Event::Resume), Ok("open"));
        assert_eq!(next_state("chosen", Event::Start), Ok("started"));
        assert!(next_state("open", Event::Start).is_err());
        assert!(next_state("started", Event::Resume).is_err());
    }

    proptest! {
        #[test]
        fn compose_is_deterministic_and_bounded(title in "[a-zA-Z ]{1,40}", n in 0usize..400) {
            let items: Vec<Item> = (0..n).map(|i| Item { text: format!("constraint {i}"), source: "model".to_owned() }).collect();
            let a = compose(&approach(&title), &items, &[], "ex-1", h);
            let b = compose(&approach(&title), &items, &[], "ex-1", h);
            prop_assert_eq!(&a, &b);
            if let Ok(brief) = a {
                prop_assert!(brief.bytes <= BRIEF_MAX);
                prop_assert_eq!(body_digest(&brief.text, h), brief.digest);
            }
        }

        #[test]
        fn carried_keeps_every_answer_labelled_within_its_cap(n in 0usize..40) {
            let s = ExploreSession {
                version: 1, id: "ex-1".to_owned(), idea: "i".to_owned(), skill: FALLBACK.to_owned(),
                selected_because: "fallback".to_owned(), state: "open".to_owned(), created_at: 0, updated_at: 0,
                runs: vec![], reports: vec![report(3)],
                qa: (0..n).map(|i| QaItem { question: format!("q{i}?"), answer: Some(format!("a{i}")), by: Some("Ada".to_owned()) }).collect(),
                choices: vec![], started: None,
            };
            let c = carried(&s);
            prop_assert!(c.len() <= CARRIED_MAX);
            for i in 0..n {
                let pair = format!("Q: q{i}?\nA: a{i}\n(answered by Ada)");
                prop_assert!(c.contains(&pair), "missing {}", pair);
            }
            prop_assert!(c.contains("source=\"model output\""));
        }
    }
}
