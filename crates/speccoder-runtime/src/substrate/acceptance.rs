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
command that runs only these tests (for Rust: cargo test --test <name>). Each file cites \
the IDs of the scenarios it tests, from the list given (none when there is no list). The tests must \
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

/// What the held-out author is told: the visible tests' scenarios again,
/// with other inputs, for tests the implementer never sees.
#[must_use]
pub fn held_out_policy(w: &Wording, composition: bool) -> String {
    let composition = if composition {
        " This task completes its story: also write one test that exercises the story's scenarios together (mark it `composition`, cite every scenario it covers)."
    } else {
        ""
    };
    format!(
        "You are the second test author of speccoder, writing HELD-OUT acceptance tests for one {product} \
task: tests the implementing model never sees, which decide whether its work does what the spec \
says or only what the visible tests check. Cover the same scenarios as the visible tests below, \
with OTHER inputs and the edge cases the spec itself states — never a behaviour the spec does not \
state, never a weaker example. Each file cites the IDs of the scenarios it covers (from the list \
given), goes to a NEW test path that is neither in the project's file list nor among the visible \
tests, and comes with the command that runs only it (for Rust: tests/held_out_<task id>_<n>.rs, \
cargo test --test <name>). At most four files. They must fail now and pass once the task is done as \
the spec says. An example whose outcome depends on outside state — the date or time, the network, \
files, randomness, a value published elsewhere — is NOT a fixed assertion: never pin such a value.{composition} \
If no such test can be written, return no files.",
        product = w.product,
    )
}

/// The held-out answer's schema: per file its scenarios and command.
#[must_use]
pub fn held_out_schema() -> Value {
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
                        "scenarios": { "type": "array", "items": { "type": "string" } },
                        "argv": { "type": "array", "items": { "type": "string" } },
                        "composition": { "type": "boolean" },
                    },
                    "required": ["path", "content", "scenarios", "argv", "composition"],
                    "additionalProperties": false,
                },
            },
            "notes": { "type": "array", "items": { "type": "string" } },
        },
        "required": ["files", "notes"],
        "additionalProperties": false,
    })
}

/// The held-out author's input: the visible run's, plus the scenarios, the
/// visible tests (so the held-out ones differ) and whether this task
/// completes its story.
#[must_use]
pub fn held_out_input(
    bundle: Value,
    files: &[String],
    manifest: &str,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    visible: &[(String, String)],
    composition: bool,
) -> Tainted<Value> {
    Tainted::object([
        ("set", Tainted::trusted(json!("held_out"))),
        ("bundle", Tainted::trusted(bundle)),
        ("scenarios", Tainted::trusted(json!(scenarios))),
        ("composition", Tainted::trusted(json!(composition))),
        (
            "project",
            Tainted::with_label(
                json!({ "files": files, "manifest": manifest }),
                super::provenance::from(super::provenance::WORKTREE),
            ),
        ),
        (
            // Written by speccoder's own author call from the spec.
            "visible",
            Tainted::trusted(json!(
                visible
                    .iter()
                    .map(|(p, c)| json!({ "path": p, "content": c }))
                    .collect::<Vec<_>>()
            )),
        ),
    ])
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
                        "scenarios": { "type": "array", "items": { "type": "string" } },
                    },
                    "required": ["path", "content", "scenarios"],
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

/// The run input: `{ bundle, scenarios, project: { files, manifest } }`.
#[must_use]
pub fn input(
    bundle: Value,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    files: &[String],
    manifest: &str,
) -> Tainted<Value> {
    // The bundle and its scenarios are the person's; the file list and
    // manifest are the worktree's.
    Tainted::object([
        ("bundle", Tainted::trusted(bundle)),
        ("scenarios", Tainted::trusted(json!(scenarios))),
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
        let held_out = input.peek()["set"] == "held_out";
        let prompt = super::provenance::labelled_prompt(
            {
                let input = input.peek();
                let system = if held_out {
                    held_out_policy(
                        self.format.wording(),
                        input["composition"].as_bool().unwrap_or(false),
                    )
                } else {
                    policy(self.format.wording())
                };
                let mut p = layout(&input["bundle"], ttl, style, &system, self.format);
                let files = input["project"]["files"]
                    .as_array()
                    .map(|f| {
                        f.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let mut text = format!(
                    "# Project files\n\n{files}\n\n# Manifest\n\n{}",
                    input["project"]["manifest"].as_str().unwrap_or("(none)")
                );
                let scenarios = input["scenarios"].as_array().into_iter().flatten();
                if scenarios.clone().next().is_some() {
                    text.push_str("\n\n# Scenarios (cite these IDs)\n");
                    for s in scenarios {
                        text.push_str(&format!(
                            "\n- {}: {}",
                            s["id"].as_str().unwrap_or_default(),
                            s["text"].as_str().unwrap_or_default()
                        ));
                    }
                }
                if held_out {
                    text.push_str("\n\n# Visible acceptance tests (the implementer sees these)\n");
                    for f in input["visible"].as_array().into_iter().flatten() {
                        text.push_str(&format!(
                            "\n## {}\n\n```\n{}\n```\n",
                            f["path"].as_str().unwrap_or_default(),
                            f["content"].as_str().unwrap_or_default()
                        ));
                    }
                }
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
            .complete_with(&prompt, |call| {
                call.expecting(if held_out {
                    held_out_schema()
                } else {
                    schema()
                })
            })
            .await?;
        Ok(Outcome::done(completion.map(|c| {
            json!({
                "answer": c.structured,
                "usage": serde_json::to_value(c.usage).unwrap_or(Value::Null),
            })
        })))
    }
}
