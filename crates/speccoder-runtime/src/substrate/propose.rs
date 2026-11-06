//! `speccoder.task.propose`: one governed model call that proposes how to do
//! a task. The answer is recorded, never executed.

use crate::format::{Adapter, Wording};
use agentplane::prelude::*;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.propose";

/// What speccoder tells every model call first. Stable across tasks, so it
/// sits inside the cached prefix.
pub fn policy(w: &Wording) -> String {
    format!(
        "You are the planning step of speccoder, a coding agent that executes \
{product} tasks. Propose how to implement exactly one task. You cannot run tools in this \
step and nothing you write is executed: a deterministic engine decides what happens, and \
verification evidence — not your claims — decides when a task is done. Stay inside the task's \
scope; name the files you expect to touch and the checks that would show the task works; say \
what is unclear or risky instead of guessing. Answer only in the requested structure.",
        product = w.product
    )
}

/// The proposal's JSON schema.
#[must_use]
pub fn schema() -> Value {
    super::answers::schema::<super::answers::Proposal>()
}

fn breakpoint(ttl: CacheTtl) -> Value {
    match ttl {
        CacheTtl::FiveMinutes => json!({ "type": "ephemeral" }),
        CacheTtl::OneHour => json!({ "type": "ephemeral", "ttl": "1h" }),
    }
}

/// The story's oracle as the implementer is told it (`bundle.oracle`):
/// the scenarios still red with what must hold and the tests that check
/// them, what this task must turn green, and that a test contradicting the
/// spec is reported, never worked around. Empty outside a story.
#[must_use]
pub fn oracle_text(bundle: &Value) -> String {
    let o = &bundle["oracle"];
    let red: Vec<String> = o["red"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let tests: Vec<&str> = r["tests"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            format!(
                "- {}: {}{}",
                r["id"].as_str().unwrap_or_default(),
                r["contract"].as_str().unwrap_or("(as the spec states it)"),
                if tests.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", tests.join(", "))
                }
            )
        })
        .collect();
    if red.is_empty() && o.is_null() {
        return String::new();
    }
    let links: Vec<&str> = o["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut out =
        String::from("\n# The story's acceptance tests (frozen: you cannot change them)\n\n");
    if red.is_empty() {
        out.push_str("Every scenario of the story passes already: keep them passing.\n");
    } else {
        out.push_str("Still failing — what must hold, and the tests that check it:\n");
        out.push_str(&red.join("\n"));
        out.push('\n');
    }
    if o["checkpoint"] == true {
        out.push_str("\nThis is the story's last task: every scenario must pass after it.\n");
    } else if !links.is_empty() {
        out.push_str(&format!(
            "\nThis task must make these pass: {}. The others belong to later tasks of the story: once yours pass, the task is done.\n",
            links.join(", ")
        ));
    } else {
        out.push_str("\nMake pass what this task's work makes hold; the rest belong to later tasks of the story, and their failing does not make this task incomplete. Never break one that passes.\n");
    }
    out.push_str("If a test asserts what the specification does not say, stop with report_blocked, kind wrong_test, naming the test — never work around it.\n");
    out
}

/// The prompt for a bundle (the run's input `{bundle, cache_ttl}`).
///
/// Order is stability order: policy and constitution (identical for the whole
/// project), then the plan (identical for the feature), then the task. A
/// breakpoint closes each stable part. For [`PromptStyle::Parts`] this is
/// the Anthropic layout; callers convert it once they have appended to it.
#[must_use]
pub fn layout(
    bundle: &Value,
    ttl: CacheTtl,
    style: PromptStyle,
    policy: &str,
    format: &dyn Adapter,
) -> Value {
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
    if let Some(others) = bundle["story_tasks"].as_array().filter(|d| !d.is_empty()) {
        task_text.push_str(
            if bundle["task"]["story"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
            {
                "\nThe phase's other tasks (their work, not this task's):\n"
            } else {
                "\nThe story's other tasks (their work, not this task's):\n"
            },
        );
        for d in others {
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

    task_text.push_str(&oracle_text(bundle));

    let w = format.wording();
    let constitution = format!("# {}\n\n{}", w.constitution, or_none(&text("constitution")));
    let plan = format!(
        "# {} ({})\n\n{}",
        w.plan,
        text("feature"),
        or_none(&text("plan"))
    );
    let story = text("story");
    let heading = if format.is_convergence(task["phase"].as_str().unwrap_or_default()) {
        "# Specification (this task closes a gap found against it)".to_owned()
    } else {
        format!("# {}", format.wording().story)
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
    /// The project's spec format.
    pub format: &'static dyn Adapter,
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
        let policy = policy(self.format.wording());
        let prompt = input.map(|bundle| {
            let mut p = layout(&bundle, ttl, style, &policy, self.format);
            if let PromptStyle::Parts { caches } = style {
                super::model_config::to_parts(&mut p, caches);
            }
            p
        });
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
