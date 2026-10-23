//! `speccoder.task.propose`: one governed model call that proposes how to do
//! a task. The answer is recorded, never executed.

use agentplane::prelude::*;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.propose";

/// What speccoder tells every model call first. Stable across tasks, so it
/// sits inside the cached prefix.
pub const POLICY: &str = "You are the planning step of speccoder, a coding agent that executes \
GitHub Spec Kit tasks. Propose how to implement exactly one task. You cannot run tools in this \
step and nothing you write is executed: a deterministic engine decides what happens, and \
verification evidence — not your claims — decides when a task is done. Stay inside the task's \
scope; name the files you expect to touch and the checks that would show the task works; say \
what is unclear or risky instead of guessing. Answer only in the requested structure.";

/// The proposal's JSON schema.
#[must_use]
pub fn schema() -> Value {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "steps": strings,
            "files": strings,
            "checks": strings,
            "risks": strings,
        },
        "required": ["summary", "steps", "files", "checks", "risks"],
        "additionalProperties": false,
    })
}

fn breakpoint(ttl: CacheTtl) -> Value {
    match ttl {
        CacheTtl::FiveMinutes => json!({ "type": "ephemeral" }),
        CacheTtl::OneHour => json!({ "type": "ephemeral", "ttl": "1h" }),
    }
}

/// The prompt for a bundle (the run's input `{bundle, cache_ttl}`).
///
/// Order is stability order: policy and constitution (identical for the whole
/// project), then the plan (identical for the feature), then the task. A
/// breakpoint closes each stable part.
#[must_use]
pub fn layout(bundle: &Value, ttl: CacheTtl, style: PromptStyle, policy: &str) -> Value {
    let text = |key: &str| bundle[key].as_str().unwrap_or_default().to_owned();
    let task = &bundle["task"];
    let mut task_text = format!(
        "# Task {}\n\n{}\n\nPhase: {}\n",
        task["id"].as_str().unwrap_or_default(),
        task["title"].as_str().unwrap_or_default(),
        task["phase"].as_str().unwrap_or_default(),
    );
    for (key, label) in [("subsection", "Subsection"), ("story", "Story")] {
        if let Some(v) = task[key].as_str().filter(|v| !v.is_empty()) {
            task_text.push_str(&format!("{label}: {v}\n"));
        }
    }
    if task["parallel"].as_bool() == Some(true) {
        task_text.push_str("Marked [P]: may run in parallel with other [P] tasks.\n");
    }
    if let Some(deps) = bundle["dependencies"].as_array().filter(|d| !d.is_empty()) {
        task_text.push_str("\nDepends on:\n");
        for d in deps {
            let mark = if d["done"].as_bool() == Some(true) {
                "x"
            } else {
                " "
            };
            task_text.push_str(&format!(
                "- [{mark}] {} {}\n",
                d["id"].as_str().unwrap_or_default(),
                d["title"].as_str().unwrap_or_default()
            ));
        }
    }

    let constitution = format!(
        "# Project constitution\n\n{}",
        or_none(&text("constitution"))
    );
    let plan = format!(
        "# Feature plan ({})\n\n{}",
        text("feature"),
        or_none(&text("plan"))
    );
    let story = text("story");
    let heading = if crate::context::is_convergence(task["phase"].as_str().unwrap_or_default()) {
        "# Specification (this task closes a gap found against it)"
    } else {
        "# User story"
    };
    let story = (!story.is_empty()).then(|| format!("{heading}\n\n{story}"));

    if style == PromptStyle::OpenAi {
        let mut parts = vec![plan];
        parts.extend(story);
        parts.push(task_text);
        let content: Vec<Value> = parts
            .into_iter()
            .map(|t| json!({ "type": "input_text", "text": t }))
            .collect();
        return json!({
            "system": format!("{policy}\n\n{constitution}"),
            "input": [ { "role": "user", "content": content } ],
        });
    }

    let mut user = vec![json!({
        "type": "text",
        "text": plan,
        "cache_control": breakpoint(ttl),
    })];
    if let Some(story) = story {
        user.push(json!({ "type": "text", "text": story }));
    }
    user.push(json!({ "type": "text", "text": task_text }));

    let mut out = json!({
        "system": [
            { "type": "text", "text": policy },
            { "type": "text", "text": constitution, "cache_control": breakpoint(ttl) },
        ],
        "messages": [ { "role": "user", "content": user } ],
    });
    if style == PromptStyle::Chat {
        super::model_config::strip_cache_control(&mut out);
    }
    out
}

fn or_none(s: &str) -> &str {
    if s.trim().is_empty() { "(none)" } else { s }
}

/// The coded skill.
#[derive(Debug)]
pub struct Propose {
    /// Lifetime of the prompt's cache breakpoints.
    pub cache_ttl: CacheTtl,
    /// The provider's prompt dialect.
    pub style: PromptStyle,
}

#[async_trait]
impl Skill for Propose {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.task.propose").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let (ttl, style) = (self.cache_ttl, self.style);
        let prompt = input.map(|bundle| layout(&bundle, ttl, style, POLICY));
        let completion = cx
            .complete_with(&prompt, |call| call.expecting(schema()))
            .await?;
        Ok(Outcome::done(completion.map(|c| {
            json!({
                "proposal": c.structured,
                "text": c.text,
                "usage": serde_json::to_value(c.usage).unwrap_or(Value::Null),
            })
        })))
    }
}
