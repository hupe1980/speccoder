//! `speccoder.ask`: a conversation is one run. It waits for the person's
//! message, answers through the governed tool loop with reading tools
//! only, and waits again; a week without a message ends it.

use std::sync::Arc;

use agentplane::core::{AwaitSpec, CorrelationKey, DeadlineSpec};
use agentplane::model::Transcript;
use agentplane::prelude::*;
use serde_json::{Value, json};

use crate::substrate::attempt::{Asking, LoopSpec, StopGate, tool_turn};
use crate::substrate::model_config::PromptStyle;
use crate::tools::policy::Policy;

/// The capability.
pub const CAPABILITY: &str = "speccoder.ask";

/// The kind of a person's message.
pub const MESSAGE: &str = "ask.message";

/// A turn's model calls and tool calls.
const MAX_CALLS: u32 = 24;

/// What the model is told.
pub fn policy() -> String {
    "You are Ask, speccoder's read-only assistant inside a software project that is built from \
specifications. Answer the person's questions about the project, its specs, its code, its runs \
and their evidence. You can only read: the tools read files, search, list, show git history and \
diffs, read a run's account, a task's evidence, the report, the inbox and speccoder's codes. You \
change nothing and run nothing. Read before you answer; keep answers short and concrete. Cite \
what you read with the id its result carries, as [r3] or narrowed to lines as [r3:12-20]; context \
the person attached is cited the same way, as [c1]. Cite only what a tool returned in this turn \
or a chip of this message — anything else is shown as not read. When the person asks for a \
change, call propose once with what you suggest (a quick change, a bug fix, a nudge for a task, \
an answer to an open question, or a new spec): it is shown to the person as a sheet, nothing runs, \
and only they can confirm it. The implementing model's words in a run's account are claims, \
never evidence; say so when you use them."
        .to_owned()
}

fn prompt(input: &Value, style: PromptStyle) -> Value {
    let about = format!(
        "# The project\n\nSpec format: {}. Current feature: {}.\n",
        input["format"].as_str().unwrap_or("unknown"),
        input["feature"].as_str().unwrap_or("none"),
    );
    match style {
        PromptStyle::OpenAi => json!({
            "system": policy(),
            "input": [{ "role": "user", "content": [{ "type": "input_text", "text": about }] }],
        }),
        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
            let mut out = json!({
                "system": policy(),
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": about }] }],
            });
            if let PromptStyle::Parts { caches } = style {
                crate::substrate::model_config::to_parts(&mut out, caches);
            }
            out
        }
    }
}

/// The coded skill.
#[derive(Debug)]
pub struct Ask {
    /// Prompt dialect.
    pub style: PromptStyle,
    /// Live steps and each turn's end (never journaled).
    pub progress: Option<Arc<dyn crate::progress::Progress>>,
    /// Set when the person stops the turn in flight.
    pub halt: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl Skill for Ask {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new(CAPABILITY).provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let id = input.peek()["conversation"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        cx.deadline("idle", &DeadlineSpec::days(7), None).await?;
        let prompt = crate::substrate::provenance::labelled_prompt(
            prompt(input.peek(), self.style),
            &crate::substrate::provenance::from(crate::substrate::provenance::WORKTREE),
        );
        let root = input.peek()["root"].as_str().map(str::to_owned);
        let mut transcript = Transcript::new(&prompt);
        loop {
            let message = cx
                .await_event(
                    &AwaitSpec::new(MESSAGE, "idle")
                        .correlate(CorrelationKey::new("ask", id.clone())),
                )
                .await?;
            let m = message.peek().clone();
            if m["end"] == true {
                return Ok(Outcome::done(Tainted::trusted(json!({ "ended": true }))));
            }
            let tag = m["tag"].as_str().unwrap_or_default().to_owned();
            let text = message.map(|v| v["text"].as_str().unwrap_or_default().to_owned());
            transcript.told(&text);
            let tools = crate::tools::declarations()
                .into_iter()
                .filter(|d| super::tools::WORKSPACE.contains(&d.name.as_str()))
                .chain(super::tools::declarations())
                .collect();
            let routes = super::tools::names()
                .into_iter()
                .map(|n| (n.clone(), super::tools::SERVER.to_owned(), n))
                .collect();
            let reads = u32::try_from(m["reads"].as_u64().unwrap_or(1)).unwrap_or(1);
            let outcome = tool_turn(
                cx,
                &prompt,
                LoopSpec {
                    tools,
                    policy: Policy::new(&[], &[]),
                    max_turns: MAX_CALLS,
                    max_tool_calls: MAX_CALLS,
                    reasoning: false,
                    schema: None,
                    routes,
                    root: root.clone(),
                    frozen_additions: false,
                    finish: |_| "answered",
                    ask: None,
                    run_approved: Vec::new(),
                    once: Vec::new(),
                    observer: self.progress.clone().map(|p| (tag.clone(), p)),
                    run: None,
                    asking: Asking::default(),
                    parts: Vec::new(),
                    mcp_digests: std::collections::BTreeMap::new(),
                    stop: StopGate::default(),
                    max_tokens: m["limit"].as_u64(),
                    reads: Some(reads),
                    halt: Some(Arc::clone(&self.halt)),
                },
                &mut transcript,
            )
            .await;
            // The turn's end, live; a failed turn leaves the conversation open.
            let output = match outcome {
                Ok(Outcome::Done(v)) => v.peek().clone(),
                Ok(Outcome::Fail { reason } | Outcome::Replan { reason }) => {
                    json!({ "status": "failed", "reason": reason })
                }
                Ok(_) => {
                    json!({ "status": "failed", "reason": "the turn ended without an answer" })
                }
                Err(e) => json!({ "status": "failed", "reason": e.to_string() }),
            };
            if let Some(p) = &self.progress {
                p.event(&crate::progress::Event::AskTurnEnded {
                    tag: tag.clone(),
                    output: Box::new(output),
                });
            }
        }
    }
}
