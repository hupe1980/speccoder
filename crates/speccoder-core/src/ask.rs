//! Ask: a read-only conversation about the project. What a turn shows —
//! the model's words, the reads it made, the citations code checked
//! against those reads, and at most one proposal the person may confirm.
//!
//! Citations are `[r3]` or `[r3:12-20]` in the answer, naming a read the
//! engine issued; a citation counts only when that read happened in the
//! same turn and the range lies inside it.

use serde::{Deserialize, Serialize};

/// One Ask turn: the question, what was read, the answer and its sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskReport {
    /// The conversation (`c-…`).
    pub conversation: String,
    /// The turn's number in the conversation, from 1.
    pub turn: u32,
    /// The person's words.
    pub question: String,
    /// What was on screen or added as context.
    pub chips: Vec<Chip>,
    /// What the model read, in order.
    pub reads: Vec<Read>,
    /// The model's words (a claim), citation markers included.
    pub answer: String,
    /// Every marker in the answer, checked.
    pub citations: Vec<Citation>,
    /// A change the model suggests; nothing ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
    /// Tokens the turn used (input and output, cache reads apart).
    pub usage: AskUsage,
    /// How the turn ended: `answered`, `stopped`, `budget`, `full` or `failed`.
    pub end: String,
    /// Why it ended so, when it did not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Tokens a turn used.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct AskUsage {
    /// Input tokens, cache reads not counted.
    pub input: u64,
    /// Output tokens.
    pub output: u64,
    /// Input read from the provider's cache.
    pub cache_read: u64,
}

impl AskUsage {
    /// What counts against Ask's limits.
    #[must_use]
    pub const fn billed(&self) -> u64 {
        self.input + self.output
    }
}

/// Something on screen given to the model as context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Chip {
    /// Its id in the turn (`c1`…), citable like a read.
    pub id: String,
    /// `feature`, `task`, `file_range`, `requirement`, `question`,
    /// `inbox_item`, `evidence_row`, `diff_hunk`, `report_row`, `finding`.
    pub kind: String,
    /// What a person reads on the chip.
    pub label: String,
    /// Where it points: `feature`, `task`, `path`, `start`, `end`, …
    pub reference: serde_json::Value,
    /// The content's digest when it was sent.
    pub digest: String,
    /// The current view's subject, attached by itself.
    #[serde(default)]
    pub from_view: bool,
    /// Its content changed since it was added; the current one was sent.
    #[serde(default)]
    pub changed: bool,
}

/// A read the engine issued for a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Read {
    /// `r1`, `r2`, … across the conversation.
    pub id: String,
    /// The tool.
    pub tool: String,
    /// What it read, in one line.
    pub target: String,
    /// The file it read, when it read one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The first line it returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    /// The last line it returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u32>,
    /// The digest of what it returned.
    pub digest: String,
}

/// A citation marker in the answer, checked against the turn's reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Citation {
    /// The marker as written (`[r3:12-20]`).
    pub marker: String,
    /// The read or chip it names.
    pub id: String,
    /// The lines it narrows to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<(u32, u32)>,
    /// `read`: it names what this turn read; `not_read`: it does not.
    pub state: String,
    /// The file, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// What a person reads for it ("crates/x.rs 12–20").
    pub label: String,
}

/// A change the model suggests, as code made it a person's action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Proposal {
    /// `quick_change`, `bug_fix`, `nudge`, `answer` or `start_spec`.
    pub kind: String,
    /// One line.
    pub title: String,
    /// The editable words, by kind.
    pub fields: ProposalFields,
    /// What confirming performs; `None` when it cannot be done now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runnable: Option<crate::actions::Runnable>,
    /// The words the action records (the input of a nudge or an answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// Why it cannot be done now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    /// The same action as a command.
    pub command: String,
}

/// A proposal's words; which ones a kind uses is the kind's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalFields {
    /// The task (nudge) or question (answer) it is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// A quick change, in the person's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// When a quick change is done.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    /// A nudge's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// A question's answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// A new spec's description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A bug's current behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symptom: Option<String>,
    /// What should happen instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected: Vec<String>,
    /// What must stay as it is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unchanged: Vec<String>,
}

/// One conversation in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Conversation {
    /// `c-…`.
    pub id: String,
    /// The feature it was about, when one was current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// Its first question.
    pub title: String,
    /// When its last turn ended (seconds since the epoch).
    pub at: u64,
    /// How many turns it has.
    pub turns: u32,
    /// Whether it takes another turn (`false`: ended, idle or full).
    pub open: bool,
}

/// `ask --list` / `GET /api/ask/conversations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskConversations {
    /// Newest first.
    pub conversations: Vec<Conversation>,
}

/// One conversation with its turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskThread {
    /// The conversation.
    pub conversation: Conversation,
    /// Its turns, in order.
    pub turns: Vec<AskReport>,
}

/// A marker found in an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// As written.
    pub text: String,
    /// The id it names.
    pub id: String,
    /// The lines it narrows to.
    pub lines: Option<(u32, u32)>,
}

/// Every `[r3]`, `[r3:12-20]`, `[c1]` or `[c1:4]` in `answer`, in order.
#[must_use]
pub fn markers(answer: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut rest = answer;
    while let Some(open) = rest.find('[') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find(']') else { break };
        let inner = &rest[..close];
        if let Some(m) = marker(inner) {
            out.push(m);
        }
        rest = &rest[close + 1..];
    }
    out
}

fn marker(inner: &str) -> Option<Marker> {
    let (id, lines) = match inner.split_once(':') {
        Some((id, range)) => {
            let (a, b) = range.split_once('-').unwrap_or((range, range));
            let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            (id, Some((a, b)))
        }
        None => (inner, None),
    };
    let mut chars = id.chars();
    let kind = chars.next()?;
    if !matches!(kind, 'r' | 'c') || id.len() < 2 || !chars.all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(Marker {
        text: format!("[{inner}]"),
        id: id.to_owned(),
        lines,
    })
}

/// Each marker of `answer` against what this turn read and was given: a
/// citation is `read` only when its id is one of them and its lines lie
/// inside what was returned.
#[must_use]
pub fn check(answer: &str, reads: &[Read], chips: &[Chip]) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    for m in markers(answer) {
        if out.iter().any(|c| c.marker == m.text) {
            continue;
        }
        let read = reads.iter().find(|r| r.id == m.id);
        let chip = chips.iter().find(|c| c.id == m.id);
        let (state, path, label) = match (read, chip) {
            (Some(r), _) => {
                let inside = match (m.lines, r.start, r.end) {
                    (None, ..) => true,
                    (Some((a, b)), Some(s), Some(e)) => a <= b && a >= s && b <= e,
                    // Lines of something that has none (a list, a log).
                    (Some(_), ..) => false,
                };
                let label = match (&r.path, m.lines.or(r.start.zip(r.end))) {
                    (Some(p), Some((a, b))) if a == b => format!("{p} {a}"),
                    (Some(p), Some((a, b))) => format!("{p} {a}–{b}"),
                    (Some(p), None) => p.clone(),
                    (None, _) => r.target.clone(),
                };
                (inside, r.path.clone(), label)
            }
            (None, Some(c)) => (
                true,
                c.reference["path"].as_str().map(str::to_owned),
                c.label.clone(),
            ),
            (None, None) => (false, None, m.text.clone()),
        };
        out.push(Citation {
            marker: m.text,
            id: m.id,
            lines: m.lines,
            state: if state { "read" } else { "not_read" }.to_owned(),
            path,
            label,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(id: &str, path: &str, start: u32, end: u32) -> Read {
        Read {
            id: id.to_owned(),
            tool: "read_file".to_owned(),
            target: path.to_owned(),
            path: Some(path.to_owned()),
            start: Some(start),
            end: Some(end),
            digest: "d".to_owned(),
        }
    }

    #[test]
    fn markers_are_ids_the_engine_issues_and_nothing_else() {
        let found: Vec<_> = markers("see [r3], [r12:4-9], [c1:2], [FR-001], [x] and [r]")
            .into_iter()
            .map(|m| (m.id, m.lines))
            .collect();
        assert_eq!(
            found,
            [
                ("r3".to_owned(), None),
                ("r12".to_owned(), Some((4, 9))),
                ("c1".to_owned(), Some((2, 2))),
            ]
        );
    }

    /// A citation is a read of this turn, inside what it returned; an id
    /// never issued, or lines it never returned, is not among what was read.
    #[test]
    fn a_citation_counts_only_inside_a_read_of_the_turn() {
        let reads = [read("r1", "src/a.rs", 10, 40)];
        let states: Vec<_> = check("[r1] [r1:12-20] [r1:5-12] [r9] [r1:20-12]", &reads, &[])
            .into_iter()
            .map(|c| (c.marker, c.state, c.label))
            .collect();
        assert_eq!(
            states,
            [
                ("[r1]".into(), "read".into(), "src/a.rs 10–40".into()),
                ("[r1:12-20]".into(), "read".into(), "src/a.rs 12–20".into()),
                (
                    "[r1:5-12]".into(),
                    "not_read".into(),
                    "src/a.rs 5–12".into()
                ),
                ("[r9]".into(), "not_read".into(), "[r9]".into()),
                (
                    "[r1:20-12]".into(),
                    "not_read".into(),
                    "src/a.rs 20–12".into()
                ),
            ]
        );
    }

    #[test]
    fn a_chip_is_citable_like_a_read() {
        let chip = Chip {
            id: "c1".to_owned(),
            kind: "task".to_owned(),
            label: "T004 Handle Unicode".to_owned(),
            reference: serde_json::json!({ "task": "T004" }),
            digest: "d".to_owned(),
            from_view: true,
            changed: false,
        };
        let c = check("as [c1] says", &[], &[chip]);
        assert_eq!(
            (c[0].state.as_str(), c[0].label.as_str()),
            ("read", "T004 Handle Unicode")
        );
    }
}
