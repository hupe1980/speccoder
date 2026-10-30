//! `speccoder shape`: a person's words made into a requirement, a bug line,
//! a quick change or a note. One model call proposes; code decides what
//! the proposal added, dropped and assumed (`speccoder_core::shape`).
//! Nothing is written: the field's own sheet or command writes what the
//! person accepts.

use std::path::Path;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use serde_json::Value;
use speccoder_core::shape::{Assumption, check};
use speccoder_core::{CommandData, CommandResult, CommandStatus, ShapeReport};

use crate::substrate;

/// The fields Shape helps with: text that becomes the judge or an
/// instruction. A person's own word on the record (a reason, a ruling) is
/// not among them.
pub const FIELDS: [&str; 8] = [
    "requirement",
    "feature",
    "quick",
    "bug_symptom",
    "bug_expected",
    "bug_unchanged",
    "answer",
    "note",
];

/// Whether `field`'s text is a requirement the writing check reads as one.
#[must_use]
pub fn is_requirement(field: &str) -> bool {
    matches!(field, "requirement" | "feature")
}

/// The model-free writing check of `text` in `field`: what the app shows
/// under the field as the person types, and `shape --check`.
#[must_use]
pub fn findings(field: &str, text: &str) -> Vec<speccoder_core::shape::Finding> {
    let text: String = text
        .chars()
        .take(substrate::shape::MAX_WORDS_CHARS)
        .collect();
    check(&text, is_requirement(field))
}

/// What code makes of a proposal the person changed in the Shape sheet
/// (kept, asked, removed, added back): the diff against their words, the
/// added values no remaining assumption explains, where each sentence
/// went, and the writing check — the same rules as the first proposal.
#[must_use]
pub fn judge(field: &str, original: &str, proposal: &str, assumptions: &[Assumption]) -> Value {
    let cap = |t: &str| -> String { t.chars().take(substrate::shape::MAX_WORDS_CHARS).collect() };
    let (original, proposal) = (cap(original), cap(proposal));
    let (original, proposal) = (original.as_str(), proposal.as_str());
    let diff = speccoder_core::shape::diff(original, proposal);
    serde_json::json!({
        "unexplained": speccoder_core::shape::unexplained(&diff, assumptions),
        "coverage": speccoder_core::shape::coverage(original, proposal),
        "findings": findings(field, proposal),
        "diff": diff,
    })
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new("shape", status, message).with_code(code)
}

/// Shapes `text` for `field` in the project at `path`, with the model in
/// its `speccoder.toml`.
#[must_use]
pub fn shape(path: &Path, field: &str, text: &str, state: Option<&Path>) -> CommandResult {
    let Some(state) = state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
    else {
        return refuse(
            CommandStatus::ConfigError,
            "NO-STATE-DIR",
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    let config = crate::verify::find_config(path)
        .and_then(|root| std::fs::read_to_string(root.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let model = match substrate::model_config::parse(&config) {
        Ok(m) => m,
        Err(e) => {
            return refuse(
                CommandStatus::ConfigError,
                "CONFIG",
                &format!("speccoder.toml: {e}"),
            );
        }
    };
    let Some(model) = model else {
        return refuse(
            CommandStatus::ConfigError,
            "CONFIG",
            "Shape needs a model ([model] in speccoder.toml); the writing check works without one",
        );
    };
    match substrate::model_config::open_driver(&model) {
        Ok((provider, _)) => shape_with(path, field, text, &state, &model, provider),
        Err(e) => substrate::model_config::refused("shape", &e),
    }
}

/// [`shape`] with `provider` answering the model call.
#[must_use]
pub fn shape_with(
    path: &Path,
    field: &str,
    text: &str,
    state: &Path,
    model: &substrate::model_config::ModelConfig,
    provider: Arc<dyn ModelProvider>,
) -> CommandResult {
    if !FIELDS.contains(&field) {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            &format!(
                "no such field to shape: {field} (one of {})",
                FIELDS.join(", ")
            ),
        );
    }
    if !text.chars().any(char::is_alphanumeric) {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            "write a few words first — any words",
        );
    }
    let (original, cut) = match text.char_indices().nth(substrate::shape::MAX_WORDS_CHARS) {
        Some((i, _)) => (text[..i].to_owned(), true),
        None => (text.to_owned(), false),
    };
    let adapter = crate::format::at(path);
    let format = adapter.kind().name();
    let sample = sample(path, adapter);
    let Some(project) = adapter.detect(path) else {
        return refuse(
            CommandStatus::NotAProject,
            "NO-PROJECT",
            "not a spec project here: Shape writes in the project's spec format",
        );
    };
    let plane_dir = crate::worktree::project_state_dir(state, &project).join("shape");
    let rt = match crate::pipeline::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return refuse(CommandStatus::Error, "ERROR", &e),
    };
    let outcome = rt.block_on(async {
        let plane = substrate::Plane::open_with(
            &plane_dir,
            model,
            provider,
            None,
            substrate::Hooks::default(),
        )?;
        plane
            .shape(substrate::shape::input(field, format, &sample, &original))
            .await
    });
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return refuse(CommandStatus::RunFailed, "RUN-FAILED", &e.0),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = &output["answer"];
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        let status = format!("{:?}", outcome.status);
        let why = substrate::provider_refusal(&status).map_or_else(
            || substrate::why_not(&outcome.status),
            |r| format!("the model provider refused the request: {r}"),
        );
        return refuse(
            CommandStatus::RunFailed,
            "RUN-FAILED",
            &format!("Shape got no usable proposal: {why} — your words are unchanged"),
        );
    }
    let report = report(
        field,
        format,
        &original,
        cut,
        answer,
        &output["usage"],
        &outcome.run_id.to_string(),
    );
    let message = summary(&report);
    CommandResult::new("shape", CommandStatus::Ok, &message)
        .with_data(CommandData::Shape(Box::new(report)))
}

/// What code makes of the model's answer: the diff (against the back
/// translation when the person wrote in another language), the coverage,
/// unexplained additions and the writing check.
#[must_use]
pub fn report(
    field: &str,
    format: &str,
    original: &str,
    cut: bool,
    answer: &Value,
    usage: &Value,
    run_id: &str,
) -> ShapeReport {
    let s = |v: &Value| v.as_str().unwrap_or_default().trim().to_owned();
    let proposal = s(&answer["proposal"]);
    let back = Some(s(&answer["back_translation"])).filter(|b| !b.is_empty());
    let declared: Vec<Assumption> = answer["assumptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| serde_json::from_value(a.clone()).ok())
        .filter(|a: &Assumption| !a.words.trim().is_empty())
        .collect();
    // An assumption is words in the proposal; one that is not there was
    // left open, and is a question.
    // Exactly as written (trimmed): the sheet finds them by these words.
    let (assumptions, open): (Vec<Assumption>, Vec<Assumption>) = declared
        .into_iter()
        .map(|mut a| {
            a.words = a.words.trim().to_owned();
            a
        })
        .partition(|a| proposal.contains(&a.words));
    let mut questions: Vec<String> = Vec::new();
    let asked = answer["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| q.as_str().map(|q| q.trim().to_owned()));
    for q in asked.chain(open.into_iter().map(|a| a.question)) {
        if !q.is_empty() && !questions.contains(&q) {
            questions.push(q);
        }
    }
    // Added values and the writing check are about the proposal itself,
    // whatever its language: numbers and quoted terms carry across, and a
    // back translation is the model's word, never ground truth.
    let unexplained = speccoder_core::shape::unexplained(
        &speccoder_core::shape::diff(original, &proposal),
        &assumptions,
    );
    // In another language, the person checks the meaning in theirs.
    let against = back.as_deref().unwrap_or(&proposal);
    let diff = speccoder_core::shape::diff(original, against);
    ShapeReport {
        field: field.to_owned(),
        format: format.to_owned(),
        original: original.to_owned(),
        cut,
        coverage: speccoder_core::shape::coverage(original, against),
        findings: findings(field, &proposal),
        findings_before: findings(field, original),
        language: back.as_ref().map(|_| s(&answer["language"])),
        back_translation: back,
        diff,
        unexplained,
        questions,
        assumptions,
        proposal,
        run_id: (!run_id.is_empty()).then(|| run_id.to_owned()),
        usage: (!usage.is_null()).then(|| usage.clone()),
    }
}

/// `shaped: 2 assumptions, 1 unexplained addition, 1 sentence dropped`.
fn summary(r: &ShapeReport) -> String {
    let n = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let mut parts = Vec::new();
    if !r.assumptions.is_empty() {
        parts.push(n(r.assumptions.len(), "assumption", "assumptions"));
    }
    if !r.unexplained.is_empty() {
        parts.push(n(
            r.unexplained.len(),
            "unexplained addition",
            "unexplained additions",
        ));
    }
    let dropped = r.coverage.iter().filter(|c| c.dropped).count();
    if dropped > 0 {
        parts.push(n(dropped, "sentence dropped", "sentences dropped"));
    }
    if !r.questions.is_empty() {
        parts.push(n(r.questions.len(), "open question", "open questions"));
    }
    if parts.is_empty() {
        return "shaped: nothing was added without saying so — read it, then use it".to_owned();
    }
    format!(
        "shaped: {} — decide each before you use it",
        parts.join(", ")
    )
}

/// A sample of the project's spec, for its language and style: the
/// selected feature's spec, else the newest feature's, else the rules.
fn sample(path: &Path, adapter: &dyn crate::format::Adapter) -> String {
    let text = adapter
        .inspect(path, None)
        .and_then(|snap| {
            let root = Path::new(&snap.root).to_path_buf();
            let dir = snap
                .feature
                .as_ref()
                .map(|f| f.feature.dir.clone())
                .or_else(|| snap.features.last().map(|f| f.dir.clone()))?;
            let [rules, spec, _, _] = adapter.convergence_texts(&root, &root.join(dir));
            Some(if spec.trim().is_empty() { rules } else { spec })
        })
        .unwrap_or_default();
    let cut: String = text.chars().take(1500).collect();
    if cut.trim().is_empty() {
        "(no spec yet: write in the person's language, in plain English if unsure)".to_owned()
    } else {
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn code_decides_what_the_proposal_added_dropped_and_assumed() {
        let answer = json!({
            "proposal": "The export MUST write every row as CSV within 2 seconds for up to 10 000 rows.",
            "assumptions": [{ "words": "up to 10 000 rows", "from": "big tables", "question": "How many rows at most?" }],
            "questions": [],
            "language": "en",
            "back_translation": "",
        });
        let original = "export every row as csv fast. it must never delete the source file.";
        let r = report(
            "requirement",
            "speckit",
            original,
            false,
            &answer,
            &Value::Null,
            "run_1",
        );
        assert_eq!(r.unexplained, ["2 seconds"]);
        assert!(r.coverage[1].dropped, "{:?}", r.coverage);
        assert!(r.findings_before.iter().any(|f| f.words == "fast"));
        assert!(r.findings.iter().all(|f| f.words != "fast"));
        assert_eq!(r.language, None);
        assert_eq!(
            summary(&r),
            "shaped: 1 assumption, 1 unexplained addition, 1 sentence dropped — decide each before you use it"
        );
    }

    #[test]
    fn in_another_language_the_diff_is_against_the_back_translation() {
        let answer = json!({
            "proposal": "The export MUST write CSV.",
            "assumptions": [],
            "questions": [],
            "language": "de",
            "back_translation": "Der Export MUSS CSV schreiben.",
        });
        let r = report(
            "requirement",
            "speckit",
            "Der Export soll CSV schreiben",
            false,
            &answer,
            &Value::Null,
            "",
        );
        assert_eq!(r.language.as_deref(), Some("de"));
        let added: Vec<&str> = r
            .diff
            .proposal
            .iter()
            .filter(|s| s.origin == speccoder_core::shape::Origin::Added)
            .map(|s| s.text.trim())
            .collect();
        assert_eq!(added, ["MUSS"]);
        // The added value is found in the proposal, whatever the back
        // translation says.
        let sly = json!({
            "proposal": "The export MUST write CSV and delete the source after 3 days.",
            "assumptions": [], "questions": ["Which?", "Which?"], "language": "de",
            "back_translation": "Der Export MUSS CSV schreiben.",
        });
        let r = report(
            "requirement",
            "speckit",
            "Der Export soll CSV schreiben",
            false,
            &sly,
            &Value::Null,
            "",
        );
        assert_eq!(r.unexplained, ["3 days"]);
        assert_eq!(r.questions, ["Which?"]);
    }
}
