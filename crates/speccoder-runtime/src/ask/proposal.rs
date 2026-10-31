//! A `propose` call made a person's action: code checks the target now and
//! builds the `Runnable` the sheet confirms. Nothing here performs it.

use std::path::Path;

use serde_json::Value;
use speccoder_core::CommandData;
use speccoder_core::actions::{FeatureSource, PlanFrom, Runnable, Through};
use speccoder_core::ask::{Proposal, ProposalFields};
use speccoder_core::ladder::quoted;

/// The kinds a proposal may be.
pub const KINDS: [&str; 5] = ["quick_change", "bug_fix", "nudge", "answer", "start_spec"];

fn text(v: &Value, key: &str, max: usize) -> Option<String> {
    v[key]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(max).collect())
}

fn list(v: &Value, key: &str, max: usize) -> Vec<String> {
    v[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::trim).filter(|x| !x.is_empty()))
        .take(max)
        .map(|x| x.chars().take(1000).collect())
        .collect()
}

/// The proposal for `arguments`, suggested in `conversation`, checked
/// against the project at `root`.
#[must_use]
pub fn validate(
    root: &Path,
    feature: Option<&str>,
    conversation: &str,
    arguments: &Value,
) -> Proposal {
    let kind = arguments["kind"].as_str().unwrap_or_default().to_owned();
    let title = text(arguments, "title", 120).unwrap_or_else(|| kind.replace('_', " "));
    let fields = ProposalFields {
        target: text(arguments, "target", 40),
        request: text(arguments, "request", 2000),
        checks: list(arguments, "checks", 8),
        note: text(arguments, "note", 2000),
        answer: text(arguments, "answer", 4000),
        description: text(arguments, "description", 4000),
        symptom: text(arguments, "symptom", 2000),
        expected: list(arguments, "expected", 8),
        unchanged: list(arguments, "unchanged", 8),
    };
    let mut p = Proposal {
        kind: kind.clone(),
        title,
        fields,
        runnable: None,
        input: None,
        blocked: None,
        command: String::new(),
        conversation: Some(conversation.to_owned()),
    };
    let result = build(root, feature, &mut p);
    if let Err(why) = result {
        p.runnable = None;
        p.blocked = Some(why);
    }
    p
}

/// Re-checks `p` as the person edited it.
#[must_use]
pub fn revalidate(root: &Path, feature: Option<&str>, p: &Proposal) -> Proposal {
    let mut p = p.clone();
    p.blocked = None;
    if let Err(why) = build(root, feature, &mut p) {
        p.runnable = None;
        p.blocked = Some(why);
    }
    p
}

fn build(root: &Path, feature: Option<&str>, p: &mut Proposal) -> Result<(), String> {
    let f = &p.fields;
    match p.kind.as_str() {
        "quick_change" => {
            let request = f
                .request
                .clone()
                .ok_or("a quick change needs its request")?;
            let checks: String = f
                .checks
                .iter()
                .map(|c| format!(" --check {}", quoted(c)))
                .collect();
            p.command = format!("speccoder do {}{checks}", quoted(&request));
            p.runnable = Some(Runnable::QuickChange {
                request,
                checks: f.checks.clone(),
                run: true,
                asked: p.conversation.clone(),
            });
        }
        "bug_fix" => {
            let symptom = f.symptom.clone().ok_or("a bug fix needs its symptom")?;
            if f.expected.is_empty() {
                return Err("a bug fix needs what should happen instead".to_owned());
            }
            let flags: String = f
                .expected
                .iter()
                .map(|e| format!(" --expected {}", quoted(e)))
                .chain(
                    f.unchanged
                        .iter()
                        .map(|u| format!(" --unchanged {}", quoted(u))),
                )
                .collect();
            p.command = format!("speccoder bug {}{flags}", quoted(&symptom));
            p.runnable = Some(Runnable::BugFix {
                symptom,
                expected: f.expected.clone(),
                unchanged: f.unchanged.clone(),
                issue: None,
                tests: Vec::new(),
                candidate: None,
                slug: None,
                run: true,
                asked: p.conversation.clone(),
            });
        }
        "nudge" => {
            let task = f.target.clone().ok_or("a nudge names its task")?;
            let note = f.note.clone().ok_or("a nudge needs its note")?;
            match task_state(root, feature, &task)?.as_str() {
                "ready" | "blocked" => {}
                "skipped" => return Err(format!("{task} is skipped")),
                _ => return Err(format!("{task} is already ticked")),
            }
            p.command = format!("speccoder task nudge {task} --note {}", quoted(&note));
            p.runnable = Some(Runnable::Nudge { task });
            p.input = Some(note);
        }
        "answer" => {
            let question = f.target.clone().ok_or("an answer names its question")?;
            let words = f.answer.clone().ok_or("an answer needs its words")?;
            let report = crate::review::report(root, feature);
            let Some(CommandData::Review(r)) = report.data() else {
                return Err(report.message().to_owned());
            };
            if !r.questions.iter().any(|q| q.id == question) {
                return Err(format!(
                    "{question} is not an open question of {}",
                    r.feature
                ));
            }
            p.command = format!("speccoder review answer {question} {}", quoted(&words));
            p.runnable = Some(Runnable::Review {
                ids: vec![question],
                verdict: "answer".to_owned(),
            });
            p.input = Some(words);
        }
        "start_spec" => {
            let description = f
                .description
                .clone()
                .ok_or("a new spec needs its description")?;
            let creates = crate::format::at(root).planning().creates;
            let Some(skill) = creates.first() else {
                return Err("this spec format has no skill that creates a spec".to_owned());
            };
            p.command = format!(
                "speccoder spec {skill} {} --through tasks",
                quoted(&description)
            );
            p.runnable = Some(Runnable::PlanThrough {
                from: PlanFrom::Create {
                    description,
                    source: FeatureSource::Description,
                },
                through: Through::Tasks,
            });
        }
        other => {
            return Err(format!(
                "`{other}` is not a kind of proposal ({})",
                KINDS.join(", ")
            ));
        }
    }
    Ok(())
}

fn task_state(root: &Path, feature: Option<&str>, task: &str) -> Result<String, String> {
    let report = crate::status::report(root, feature);
    let Some(CommandData::Report(r)) = report.data() else {
        return Err(report.message().to_owned());
    };
    r.tasks
        .iter()
        .find(|t| t.id == task)
        .map(|t| t.state.clone())
        .ok_or_else(|| format!("no task {task} in {}", r.feature))
}
