//! `speccoder.task.acceptance`: derive acceptance tests from the spec before
//! an attempt.
//!
//! A judge-side call: no tools, no say in the verdict, and the implementing
//! model never sees it as its own work. speccoder validates what comes back,
//! writes it as new (hence frozen) test files, and requires red before and
//! green after the attempt.

use crate::format::Wording;
use agentplane::prelude::*;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};
use super::propose::layout;

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.acceptance";

/// What the test author is told.
pub fn policy(w: &Wording) -> String {
    format!(
        "You are the test author of speccoder, writing acceptance tests for one {product} task \
BEFORE it is implemented. Turn {examples} into \
executable tests in the project's own language and test framework, exactly as the spec \
states them — do not invent behaviour the spec does not state, and do not weaken an \
example. Put them in NEW files under the project's test directory — paths that are not \
in the project's file list; name them after the task ID (for Rust: \
tests/acceptance_<task id>.rs, using the crate name from Cargo.toml) — and give the \
command that runs only these tests (for Rust: cargo test --test <name>). The tests must \
fail now (the task is not implemented) and pass once it is. Another model will implement \
the task; it cannot change your tests. If the spec has no testable acceptance example \
for this task, return no files. An example whose outcome depends on outside state — the \
date or time, the network, files, randomness, a value published elsewhere (\"on a day \
when …\") — is NOT a fixed assertion: never pin such a value as a constant; if the \
behaviour cannot be tested without that state, return no files and say why in notes. \
{convergence_task} closes a gap found after implementation: write tests that \
show exactly that gap is closed, using only cases whose outcome the spec's wording \
decides unambiguously.",
        product = w.product,
        examples = w.examples,
        convergence_task = w.convergence_task,
    )
}

/// The answer's schema.
#[must_use]
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "files": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" },
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false,
                },
            },
            "argv": { "type": "array", "items": { "type": "string" } },
            "notes": { "type": "array", "items": { "type": "string" } },
        },
        "required": ["files", "argv", "notes"],
        "additionalProperties": false,
    })
}

/// The run input: `{ bundle, project: { files, manifest } }`.
#[must_use]
pub fn input(bundle: Value, files: &[String], manifest: &str) -> Tainted<Value> {
    // The bundle is the person's; the file list and manifest are the worktree's.
    Tainted::object([
        ("bundle", Tainted::trusted(bundle)),
        (
            "project",
            Tainted::with_label(
                json!({ "files": files, "manifest": manifest }),
                super::provenance::from(super::provenance::WORKTREE),
            ),
        ),
    ])
}

/// The coded skill.
#[derive(Debug)]
pub struct Acceptance {
    /// Prompt dialect.
    pub style: PromptStyle,
    /// Cache breakpoint lifetime.
    pub cache_ttl: CacheTtl,
    /// The project's spec format.
    pub format: &'static dyn crate::format::Adapter,
}

#[async_trait]
impl Skill for Acceptance {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.task.acceptance").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let (ttl, style) = (self.cache_ttl, self.style);
        let prompt = super::provenance::labelled_prompt(
            {
                let input = input.peek();
                let mut p = layout(
                    &input["bundle"],
                    ttl,
                    style,
                    &policy(self.format.wording()),
                    self.format,
                );
                let files = input["project"]["files"]
                    .as_array()
                    .map(|f| {
                        f.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let text = format!(
                    "# Project files\n\n{files}\n\n# Manifest\n\n{}",
                    input["project"]["manifest"].as_str().unwrap_or("(none)")
                );
                let (key, kind) = match style {
                    PromptStyle::OpenAi => ("input", "input_text"),
                    PromptStyle::Anthropic | PromptStyle::Chat => ("messages", "text"),
                };
                if let Some(content) = p[key][0]["content"].as_array_mut() {
                    content.push(json!({ "type": kind, "text": text }));
                }
                p
            },
            &super::provenance::from(super::provenance::WORKTREE),
        );
        let completion = cx
            .complete_with(&prompt, |call| call.expecting(schema()))
            .await?;
        Ok(Outcome::done(completion.map(|c| {
            json!({
                "answer": c.structured,
                "usage": serde_json::to_value(c.usage).unwrap_or(Value::Null),
            })
        })))
    }
}
