//! An attempt as one thread: the model's thoughts and words (claims), its
//! calls (what happened), and the evidence (the only bold rows). The pure
//! parts live here: rows, the verb a call reads as, an edit's line counts,
//! folding runs of reads, and a thought's conclusion. The runtime builds a
//! thread from the journal and git, live and afterwards alike.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The most rows a thread shows; earlier ones are counted.
pub const MAX_ROWS: usize = 200;

/// What a row is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    /// The model's reasoning (a claim).
    Thought,
    /// The model's words beside its calls (a claim).
    Words,
    /// One call.
    #[default]
    Call,
    /// Consecutive read-only calls, folded.
    Fold,
    /// A call that waits for a person.
    Approval,
    /// Evidence: acceptance, a scenario turning green, a check, the end.
    Divider,
}

/// One row of a thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Row {
    /// What it is.
    pub kind: RowKind,
    /// The model turn (1-based).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
    /// A thought's or words' text; a divider's words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// A thought's last paragraph, shown first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
    /// A call's verb: `Read`, `Searched`, `Edited`, `Wrote`, `Ran`, `Called`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    /// What it acted on: a path, a command, a tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// An edit's lines added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    /// An edit's lines removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    /// `ok`, `failed`, `refused`, `waiting`, `stopped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// An exit code and last lines, a refusal's reason, a duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Who let it run, when the record says: `contained`, `you`, or
    /// `refused`; none when a rule allowed it without asking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided: Option<String>,
    /// The model's call id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    /// A fold's calls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Row>,
    /// A divider's tone: `verified`, `failed`, `stopped`, `blocked`, `info`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
}

impl Row {
    /// A row of `kind`.
    #[must_use]
    pub fn of(kind: RowKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// A divider with `text` in `tone`.
    #[must_use]
    pub fn divider(text: impl Into<String>, tone: &str) -> Self {
        Self {
            text: text.into(),
            tone: Some(tone.to_owned()),
            ..Self::of(RowKind::Divider)
        }
    }
}

/// One attempt of a task, as the switcher names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttemptRef {
    /// Its run.
    pub run: String,
    /// `verified`, `failed`, `stopped`, `running`, …
    pub outcome: String,
    /// When it ended or started (RFC 3339), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

/// One attempt as one thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Thread {
    /// The feature.
    pub feature: String,
    /// The task.
    pub task: String,
    /// The attempt shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<AttemptRef>,
    /// The task's attempts, newest first.
    pub attempts: Vec<AttemptRef>,
    /// The rows, oldest first.
    pub rows: Vec<Row>,
    /// The attempt is running now.
    pub live: bool,
    /// Rows before the first shown.
    pub more: u32,
}

/// A waiting call's facts, without any model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Explanation {
    /// The task (`feature/task`) and the call.
    pub task: String,
    /// The model's call id.
    pub call: String,
    /// The task's title.
    pub title: String,
    /// What it asks: `Run this command?`.
    pub question: String,
    /// The program, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Where it resolves to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Hosts its arguments name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Paths outside the workspace its arguments name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside: Vec<String>,
    /// What it can reach if allowed, in words.
    pub reach: String,
    /// Why it asks.
    pub rule: String,
    /// Where its arguments came from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
    /// The model's words in that turn (a claim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The model's last reasoning before it (a claim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// The scenarios the task's story is checked by: `US1-2 — …`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<String>,
}

/// A call's verb and object, from its tool and arguments.
#[must_use]
pub fn verb_of(tool: &str, args: &Value) -> (String, String) {
    let s = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match tool {
        "read_file" => ("Read".into(), s("path")),
        "list_files" => ("Listed".into(), {
            let p = s("path");
            if p.is_empty() { ".".into() } else { p }
        }),
        "search_code" => ("Searched".into(), s("pattern")),
        "write_file" => ("Wrote".into(), s("path")),
        "edit_file" => ("Edited".into(), s("path")),
        "run_command" => (
            "Ran".into(),
            args.get("argv")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default(),
        ),
        "git_status" => ("Checked".into(), "git status".into()),
        "git_diff" => ("Read".into(), "the diff".into()),
        "report_blocked" => ("Reported".into(), "it is blocked".into()),
        t if t.starts_with("mcp__") => {
            let rest = t.trim_start_matches("mcp__");
            ("Called".into(), rest.replacen("__", " / ", 1))
        }
        t => ("Called".into(), t.to_owned()),
    }
}

/// Whether `tool` only reads.
#[must_use]
pub fn reads(tool: &str) -> bool {
    matches!(
        tool,
        "read_file" | "list_files" | "search_code" | "git_status" | "git_diff"
    )
}

/// An edit's lines added and removed.
#[must_use]
pub fn edit_lines(old: &str, new: &str) -> (u32, u32) {
    let count = |s: &str| u32::try_from(s.lines().count()).unwrap_or(u32::MAX);
    let (o, n) = (count(old), count(new));
    // Lines both share at the start and end are context, not change.
    let ol: Vec<&str> = old.lines().collect();
    let nl: Vec<&str> = new.lines().collect();
    let head = ol.iter().zip(&nl).take_while(|(a, b)| a == b).count();
    let tail = ol[head..]
        .iter()
        .rev()
        .zip(nl[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let shared = u32::try_from(head + tail).unwrap_or(0);
    (n.saturating_sub(shared), o.saturating_sub(shared))
}

/// A thought's conclusion: its last paragraph, when there are several.
#[must_use]
pub fn conclusion(text: &str) -> Option<String> {
    let paras: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    (paras.len() > 1).then(|| paras[paras.len() - 1].to_owned())
}

/// `rows` with runs of read-only calls folded: two or more consecutive
/// reads (with nothing but other reads between them) become one row that
/// names their counts.
#[must_use]
pub fn fold(rows: Vec<Row>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let mut run: Vec<Row> = Vec::new();
    let flush = |run: &mut Vec<Row>, out: &mut Vec<Row>| {
        if run.len() >= 2 {
            let n = |v: &str| run.iter().filter(|r| r.verb.as_deref() == Some(v)).count();
            let mut parts = Vec::new();
            let read = n("Read");
            if read > 0 {
                parts.push(format!(
                    "Read {read} {}",
                    if read == 1 { "file" } else { "files" }
                ));
            }
            let searched = n("Searched");
            if searched > 0 {
                parts.push(format!("searched {searched}"));
            }
            let listed = n("Listed");
            if listed > 0 {
                parts.push(format!("listed {listed}"));
            }
            let checked = n("Checked");
            if checked > 0 {
                parts.push(format!("checked {checked}"));
            }
            let mut text = parts.join(" · ");
            if let Some(first) = text.get(..1) {
                text = first.to_uppercase() + &text[1..];
            }
            out.push(Row {
                turn: run[0].turn,
                text,
                items: std::mem::take(run),
                ..Row::of(RowKind::Fold)
            });
        } else {
            out.append(run);
        }
    };
    for row in rows {
        let is_read = row.kind == RowKind::Call
            && row.outcome.as_deref() != Some("refused")
            && matches!(
                row.verb.as_deref(),
                Some("Read" | "Searched" | "Listed" | "Checked")
            );
        if is_read {
            run.push(row);
        } else {
            flush(&mut run, &mut out);
            out.push(row);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(tool: &str, args: Value) -> Row {
        let (verb, object) = verb_of(tool, &args);
        Row {
            verb: Some(verb),
            object: Some(object),
            outcome: Some("ok".into()),
            ..Row::of(RowKind::Call)
        }
    }

    #[test]
    fn verbs_read_as_what_happened() {
        assert_eq!(
            verb_of("read_file", &json!({"path": "a.rs"})),
            ("Read".into(), "a.rs".into())
        );
        assert_eq!(
            verb_of("run_command", &json!({"argv": ["cargo", "test"]})),
            ("Ran".into(), "cargo test".into())
        );
        assert_eq!(
            verb_of("mcp__notes__add_note", &json!({})),
            ("Called".into(), "notes / add_note".into())
        );
    }

    #[test]
    fn an_edit_counts_only_changed_lines() {
        assert_eq!(edit_lines("fn a() {}\n", "fn a() {}\nfn b() {}\n"), (1, 0));
        assert_eq!(edit_lines("a\nb\nc\n", "a\nx\nc\n"), (1, 1));
        assert_eq!(edit_lines("", "x\ny\n"), (2, 0));
    }

    #[test]
    fn reads_fold_and_a_single_read_stays() {
        let rows = vec![
            call("read_file", json!({"path": "a"})),
            call("read_file", json!({"path": "b"})),
            call("search_code", json!({"pattern": "x"})),
            call("edit_file", json!({"path": "a"})),
            call("read_file", json!({"path": "c"})),
        ];
        let f = fold(rows);
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].kind, RowKind::Fold);
        assert_eq!(f[0].text, "Read 2 files · searched 1");
        assert_eq!(f[0].items.len(), 3);
        assert_eq!(f[2].verb.as_deref(), Some("Read"));
    }

    /// SC-004: 50 calls, 40 of them reads, fit in 20 rows.
    #[test]
    fn fifty_calls_fit_in_twenty_rows() {
        let mut rows = Vec::new();
        for i in 0..10 {
            for j in 0..4 {
                rows.push(call("read_file", json!({"path": format!("{i}/{j}")})));
            }
            rows.push(call("edit_file", json!({"path": format!("{i}")})));
        }
        assert_eq!(rows.len(), 50);
        assert!(fold(rows).len() <= 20);
    }

    #[test]
    fn a_thought_concludes_with_its_last_paragraph() {
        assert_eq!(conclusion("one"), None);
        assert_eq!(
            conclusion("first\n\nthen this").as_deref(),
            Some("then this")
        );
    }
}
