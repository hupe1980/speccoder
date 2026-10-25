//! `report --markdown`: the report as GitHub-flavoured Markdown, for pull
//! requests, chats and CI step summaries (F-056).
//!
//! Only facts speccoder established appear — never the model's words
//! (a last attempt's reason can be its claim) — and every cell is escaped,
//! so text from the repository cannot inject links, HTML or table cells.

use std::fmt::Write as _;
use std::io::Write;

use speccoder_core::{CommandData, CommandResult, StatusReport};

/// A section of the summary, most important first (dropped last).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Its name, for "dropped: …".
    pub name: &'static str,
    /// The Markdown.
    pub text: String,
}

/// Escapes text for a table cell or a line: `|`, backticks, `<`, `>`, `[`, `]`,
/// `*`, `_` and line breaks.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '|' | '`' | '[' | ']' | '*' | '_' | '\\' | '#' => {
                out.push('\\');
                out.push(c);
            }
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' | '\r' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn verdict(state: &str) -> &'static str {
    match state {
        "verified" => "✅ verified",
        "regression_only" => "〰️ regression only",
        "attested" => "🅰️ attested by a person (not verified)",
        "ticked_without_evidence" => "⚠️ ticked without speccoder evidence",
        "evidence_without_tick" => "❔ committed, local tick missing",
        "skipped" => "⏭️ skipped",
        "ready" => "▫️ ready",
        _ => "⏳ blocked",
    }
}

/// The report's sections, most important first.
#[must_use]
pub fn sections(r: &StatusReport) -> Vec<Section> {
    let t = &r.totals;
    let done = t.verified + t.regression_only + t.attested + t.ticked_without_evidence;
    let mut headline = format!(
        "## speccoder — `{}`\n\n**{done}/{} done**: {} verified · {} regression only · {} attested · {} without evidence · {} ready · {} blocked · {} skipped\n",
        escape(&r.feature),
        r.tasks.len(),
        t.verified,
        t.regression_only,
        t.attested,
        t.ticked_without_evidence,
        t.ready,
        t.blocked,
        t.skipped
    );
    if t.without_attestation > 0 {
        let _ = writeln!(
            headline,
            "\n{} tick(s) without a signed attestation.",
            t.without_attestation
        );
    }
    let mut out = vec![Section {
        name: "headline",
        text: headline,
    }];

    let waiting: Vec<String> = r
        .tasks
        .iter()
        .filter_map(|task| {
            let a = task.last_attempt.as_ref()?;
            let why = match a.outcome.as_str() {
                "needs_approval" => "a call waits for your decision",
                "suspected_gaming" => "the gaming monitor suspects a shortcut",
                "blocked" => "the model says it cannot be done as written",
                "not_demonstrated" => "the evidence could not be produced",
                _ => return None,
            };
            Some(format!(
                "- **{}** {} — {why}",
                escape(&task.id),
                escape(&task.title)
            ))
        })
        .collect();
    if !waiting.is_empty() {
        out.push(Section {
            name: "needs you",
            text: format!("### Needs you\n\n{}\n", waiting.join("\n")),
        });
    }

    let mut table =
        String::from("### Tasks\n\n| Task | Title | Verdict | Attestation |\n|---|---|---|---|\n");
    for task in &r.tasks {
        let title: String = task.title.chars().take(80).collect();
        let state = if task.state == "blocked" && !task.blocked_by.is_empty() {
            format!(
                "{} by {}",
                verdict(&task.state),
                escape(&task.blocked_by.join(", "))
            )
        } else {
            verdict(&task.state).to_owned()
        };
        let _ = writeln!(
            table,
            "| {} | {} | {state} | {} |",
            escape(&task.id),
            escape(&title),
            task.attestation.as_deref().map(escape).unwrap_or_default()
        );
    }
    out.push(Section {
        name: "tasks",
        text: table,
    });

    let uncited: Vec<String> = r
        .trace
        .iter()
        .filter(|row| row.task.is_none())
        .map(|row| escape(&row.requirement))
        .collect();
    if !uncited.is_empty() {
        out.push(Section {
            name: "trace",
            text: format!("### Requirements no task cites\n\n{}\n", uncited.join(", ")),
        });
    }
    out
}

/// The report as Markdown, within `budget` bytes: sections are dropped from
/// the least important, and a closing line names them.
#[must_use]
pub fn within(r: &StatusReport, next: &[speccoder_core::next::NextStep], budget: usize) -> String {
    let mut all = sections(r);
    if !next.is_empty() {
        let steps: Vec<String> = next
            .iter()
            .map(|s| format!("- `{}` — {}", s.command.replace('`', "'"), escape(&s.why)))
            .collect();
        all.push(Section {
            name: "next",
            text: format!("### Next\n\n{}\n", steps.join("\n")),
        });
    }
    let mut dropped: Vec<&'static str> = Vec::new();
    loop {
        let mut text: String = all.iter().map(|s| format!("{}\n", s.text)).collect();
        if !dropped.is_empty() {
            let _ = writeln!(
                text,
                "_Shortened to fit: {} not shown — `speccoder report` has everything._",
                dropped.join(", ")
            );
        }
        if text.len() <= budget || all.len() <= 1 {
            return text;
        }
        // The lowest-priority section that is not the headline goes first,
        // except the task table, which goes last.
        let victim = all
            .iter()
            .rposition(|s| s.name != "tasks" && s.name != "headline")
            .or_else(|| all.iter().rposition(|s| s.name != "headline"));
        match victim {
            Some(i) => dropped.push(all.remove(i).name),
            None => return text,
        }
    }
}

/// `report --markdown`: the report, or the result's message for anything else.
pub fn render(result: &CommandResult, out: &mut dyn Write) -> std::io::Result<()> {
    match result.data() {
        Some(CommandData::Report(r)) => write!(out, "{}", within(r, result.next(), usize::MAX)),
        _ => writeln!(out, "{}", escape(result.message())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use speccoder_core::{StatusTotals, TaskStatus};

    fn task(id: &str, state: &str, title: &str) -> TaskStatus {
        TaskStatus {
            id: id.to_owned(),
            title: title.to_owned(),
            phase: "Phase 2".to_owned(),
            state: state.to_owned(),
            intent_changed: false,
            blocked_by: Vec::new(),
            commit: None,
            run_id: None,
            last_attempt: None,
            note: None,
            attestation: Some("signed ssh SHA256:x".to_owned()),
            strength: None,
        }
    }

    fn report(n: usize) -> StatusReport {
        StatusReport {
            feature: "001-x".to_owned(),
            branch: Some("speccoder/001-x".to_owned()),
            source: "branch".to_owned(),
            selected_by: String::new(),
            artifacts: Vec::new(),
            planning: Vec::new(),
            uncommitted: Vec::new(),
            journey: Vec::new(),
            unmerged_commits: 0,
            tasks: (0..n)
                .map(|i| {
                    task(
                        &format!("T{i:03}"),
                        "verified",
                        "Parse | the <b>input</b> [link](x)",
                    )
                })
                .collect(),
            convergence: Vec::new(),
            trace: Vec::new(),
            totals: StatusTotals {
                verified: n,
                ..StatusTotals::default()
            },
        }
    }

    #[test]
    fn cells_are_escaped_and_verdicts_never_blur() {
        let md = within(&report(2), &[], usize::MAX);
        assert!(
            md.contains("Parse \\| the &lt;b&gt;input&lt;/b&gt; \\[link\\](x)"),
            "{md}"
        );
        assert!(md.contains("✅ verified"));
        assert!(!md.contains("<b>"));
        let mut r = report(1);
        r.tasks[0].state = "attested".to_owned();
        assert!(within(&r, &[], usize::MAX).contains("not verified"));
    }

    #[test]
    fn a_large_report_is_shortened_and_says_so() {
        let full = within(&report(200), &[], usize::MAX);
        let next = [speccoder_core::next::NextStep {
            command: "speccoder run".to_owned(),
            why: "continue".to_owned(),
        }];
        let cut = within(&report(200), &next, full.len());
        assert!(cut.len() <= full.len(), "{} > {}", cut.len(), full.len());
        assert!(cut.contains("Shortened to fit: next"), "{cut}");
        let tiny = within(&report(200), &next, 400);
        assert!(tiny.contains("tasks"), "the table goes last: {tiny}");
        assert!(tiny.starts_with("## speccoder"));
    }
}
