//! `speccoder.task.near_miss`: wrong and permitted variants of a task's
//! change, to test its acceptance tests.
//!
//! A judge-side call after a task was verified. It sees the task's spec and
//! the final contents of the changed source files — never an acceptance
//! test, visible or held-out, so its variants measure the tests
//! independently. It writes near misses (plausible, and wrong about exactly
//! one stated scenario) and, where the spec leaves a choice open,
//! alternatives the spec permits. It has no tools and no say: code applies
//! each variant, runs the tests and counts. The implementer never sees any
//! of it.

use crate::format::Wording;
use agentplane::prelude::*;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.near_miss";

/// The changed files' contents the author reads, at most.
pub const MAX_SOURCES: usize = 60_000;

/// What the author is told.
pub fn policy(w: &Wording) -> String {
    format!(
        "You help test the acceptance tests of a {product} task. The task's change is final \
and correct by its checks. Write variants of the changed source files, each a complete \
replacement of one or more of those files:\n\
- `wrong`: a plausible implementation a careful developer could believe correct, that \
violates exactly ONE scenario or rule the spec states (an off-by-one at a stated boundary, \
an ignored second example, a missed stated error case). Keep everything else identical; \
it must still build. Never make a variant wrong by breaking behaviour the spec does not \
mention.\n\
- `permitted`: only where the spec leaves a choice open (an order it does not fix, wording \
it does not quote, a tie it does not break), an implementation that makes a different \
allowed choice and still satisfies every stated scenario. If the spec leaves nothing \
open, return no permitted variant.\n\
Name the scenario each variant concerns and say in one line what it gets wrong or which \
freedom it uses. Change only the files you are shown. Answer only in the requested \
structure.",
        product = w.product,
    )
}

/// The answer's schema.
#[must_use]
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "variants": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["wrong", "permitted"] },
                        "scenario": { "type": "string" },
                        "rationale": { "type": "string" },
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
                    },
                    "required": ["kind", "scenario", "rationale", "files"],
                    "additionalProperties": false,
                },
            },
            "notes": { "type": "array", "items": { "type": "string" } },
        },
        "required": ["variants", "notes"],
        "additionalProperties": false,
    })
}

/// The run input: the task's spec context (the person's), the changed
/// files' final contents and the patch (the worktree's), and how many
/// variants of each kind to write.
#[must_use]
pub fn input(
    bundle: &Value,
    changed: &[(String, String)],
    patch: &str,
    near_miss: u32,
    permitted: u32,
) -> Tainted<Value> {
    let mut budget = MAX_SOURCES;
    let files: Vec<Value> = changed
        .iter()
        .map(|(path, content)| {
            let mut content = content.clone();
            if content.len() > budget {
                let mut end = budget;
                while !content.is_char_boundary(end) {
                    end -= 1;
                }
                content.truncate(end);
            }
            budget = budget.saturating_sub(content.len());
            json!({ "path": path, "content": content })
        })
        .collect();
    let mut patch = patch.to_owned();
    if patch.len() > super::monitor::MAX_PATCH {
        let mut end = super::monitor::MAX_PATCH;
        while !patch.is_char_boundary(end) {
            end -= 1;
        }
        patch.truncate(end);
        patch.push_str("\n… (diff truncated)");
    }
    let worktree = || super::provenance::from(super::provenance::WORKTREE);
    Tainted::object([
        ("story", Tainted::trusted(bundle["story"].clone())),
        ("plan", Tainted::trusted(bundle["plan"].clone())),
        ("task", Tainted::trusted(bundle["task"].clone())),
        (
            "counts",
            Tainted::trusted(json!({ "wrong": near_miss, "permitted": permitted })),
        ),
        ("files", Tainted::with_label(json!(files), worktree())),
        ("patch", Tainted::with_label(json!(patch), worktree())),
    ])
}

fn prompt_raw(input: &Value, ttl: CacheTtl, style: PromptStyle, w: &Wording) -> Value {
    let system_policy = policy(w);
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let files: Vec<String> = input["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| format!("## {}\n\n```\n{}\n```", s(&f["path"]), s(&f["content"])))
        .collect();
    let parts = [
        format!(
            "# Task {}\n\n{}\n\n# Spec (the task's story)\n\n{}",
            s(&input["task"]["id"]),
            s(&input["task"]["title"]),
            s(&input["story"])
        ),
        format!("# Plan\n\n{}", s(&input["plan"])),
        format!(
            "# The change (git diff)\n\n```diff\n{}\n```\n\n# The changed source files as they are now\n\n{}",
            s(&input["patch"]),
            files.join("\n\n")
        ),
        format!(
            "Write {} `wrong` variant(s) and at most {} `permitted` variant(s).",
            input["counts"]["wrong"], input["counts"]["permitted"]
        ),
    ];
    match style {
        PromptStyle::OpenAi => json!({
            "system": system_policy,
            "input": [{ "role": "user", "content": parts.iter().map(|t| json!({ "type": "input_text", "text": t })).collect::<Vec<_>>() }],
        }),
        PromptStyle::Anthropic | PromptStyle::Chat => {
            let breakpoint = match ttl {
                CacheTtl::FiveMinutes => json!({ "type": "ephemeral" }),
                CacheTtl::OneHour => json!({ "type": "ephemeral", "ttl": "1h" }),
            };
            json!({
                "system": system_policy,
                "messages": [{ "role": "user", "content": parts.iter().enumerate().map(|(i, t)| {
                    if i == 1 { json!({ "type": "text", "text": t, "cache_control": breakpoint }) }
                    else { json!({ "type": "text", "text": t }) }
                }).collect::<Vec<_>>() }],
            })
        }
    }
}

fn prompt(input: &Value, ttl: CacheTtl, style: PromptStyle, w: &Wording) -> Value {
    let mut out = prompt_raw(input, ttl, style, w);
    if style == PromptStyle::Chat {
        super::model_config::strip_cache_control(&mut out);
    }
    out
}

/// The coded skill.
#[derive(Debug)]
pub struct NearMiss {
    /// Prompt dialect.
    pub style: PromptStyle,
    /// Cache breakpoint lifetime.
    pub cache_ttl: CacheTtl,
    /// The project's spec format.
    pub format: &'static dyn crate::format::Adapter,
}

#[async_trait]
impl Skill for NearMiss {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.task.near_miss").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let (ttl, style) = (self.cache_ttl, self.style);
        let prompt = super::provenance::labelled_prompt(
            prompt(input.peek(), ttl, style, self.format.wording()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_author_sees_the_spec_and_the_code_never_a_test() {
        let bundle = json!({ "story": "Users log in.", "plan": "p", "task": { "id": "T005", "title": "login" } });
        let input = input(
            &bundle,
            &[("src/lib.rs".to_owned(), "pub fn ok() {}\n".to_owned())],
            "+++ b/src/lib.rs\n",
            2,
            1,
        );
        let v = input.peek();
        assert_eq!(v["counts"], json!({ "wrong": 2, "permitted": 1 }));
        let p = prompt(
            v,
            CacheTtl::FiveMinutes,
            PromptStyle::Anthropic,
            crate::format::at(std::path::Path::new(".")).wording(),
        );
        let text = p.to_string();
        assert!(text.contains("pub fn ok()") && text.contains("Users log in."));
        assert!(!text.contains("acceptance test file"));
        assert_eq!(
            schema()["properties"]["variants"]["items"]["properties"]["kind"]["enum"],
            json!(["wrong", "permitted"])
        );
    }
}
