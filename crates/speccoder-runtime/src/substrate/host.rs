//! `speccoder.skill.run`: an installed Agent Skill, hosted.
//!
//! The skill's `SKILL.md` body is the instruction; speccoder's governed tool
//! loop runs it with the planning policy (Spec Kit's artifacts and scripts;
//! anything else needs approval) and an `ask_person` tool for clarifying
//! questions. Everything it decides by is in the journaled input.

use crate::format::Wording;
use agentplane::prelude::*;
use serde_json::{Value, json};

use super::attempt::{Asker, LoopSpec, ask_person, tool_loop};
use crate::tools::policy::Policy;

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.skill.run";

/// What the host tells the model, before the skill itself.
pub fn policy(w: &Wording) -> String {
    format!(
        "You are speccoder, running the Agent Skill below for a person in their {product} \
project. Follow the skill's instructions. You can read the project, {authoring} with run_command \
(argv, no shell). Do not write source code — implementation is a separate step that \
speccoder runs with tests; other writes need a person's approval. When the skill says to \
ask the user, call ask_person with one question at a time; if nobody can answer, follow \
the skill's rule for unanswered questions. Skip steps about extension hooks, other \
agents' slash commands, committing, branching or pushing unless {authoring_tool} does it. \
Finish with the requested structured report.",
        product = w.product,
        authoring = w.authoring,
        authoring_tool = w.authoring_tool,
    )
}

/// The closing report's schema.
#[must_use]
pub fn schema() -> Value {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "status": { "type": "string", "enum": ["done", "incomplete"] },
            "files_written": strings,
            "questions": strings,
        },
        "required": ["summary", "status", "files_written", "questions"],
        "additionalProperties": false,
    })
}

/// The run input.
#[must_use]
pub fn input(skill: &str, instruction: &str, root: &str, limits: (u32, u32), ask: bool) -> Value {
    json!({
        "skill": skill,
        "instruction": instruction,
        // With a person there, a question waits on a task they answer
        // inline; without one the skill writes [NEEDS CLARIFICATION].
        "settings": { "max_turns": limits.0, "max_tool_calls": limits.1, "asks": if ask { "tasks" } else { "stop" }, "root": root },
    })
}

/// The coded skill.
#[derive(Debug)]
pub struct Host {
    /// Prompt dialect.
    pub style: super::model_config::PromptStyle,
    /// Where live progress goes.
    pub progress: Option<std::sync::Arc<dyn crate::progress::Progress>>,
    /// The project's spec format.
    pub format: &'static dyn crate::format::Adapter,
    /// The sandbox, as a person asked about a command is told; live only.
    pub present: crate::approvals::PresentContext,
}

#[async_trait]
impl Skill for Host {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.skill.run").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let style = self.style;
        let settings = input.peek()["settings"].clone();
        let skill = input.peek()["skill"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let limit = |key: &str, default: u32| {
            settings[key]
                .as_u64()
                .map_or(default, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        // Built from labelled parts, so `/system` stays speccoder's trusted
        // instruction after the conversation has read tool output.
        let label = input.label().clone();
        let prompt = super::provenance::labelled_prompt(
            {
                let input = input.peek();
                let text = format!(
                    "# Skill `{}`\n\n{}",
                    input["skill"].as_str().unwrap_or_default(),
                    input["instruction"].as_str().unwrap_or_default()
                );
                match style {
                    super::model_config::PromptStyle::OpenAi => json!({
                        "system": policy(self.format.wording()),
                        "input": [{ "role": "user", "content": [{ "type": "input_text", "text": text }] }],
                    }),
                    super::model_config::PromptStyle::Anthropic
                    | super::model_config::PromptStyle::Chat => json!({
                        "system": policy(self.format.wording()),
                        "messages": [{ "role": "user", "content": [{ "type": "text", "text": text }] }],
                    }),
                }
            },
            &label,
        );
        let mut tools = crate::tools::declarations();
        tools.push(ask_person());
        let ask = Asker::from_settings(
            &settings,
            format!("skill/{skill}"),
            skill.clone(),
            self.present.clone(),
        );
        tool_loop(
            cx,
            &prompt,
            LoopSpec {
                // The account is shown for attempts; these read as documents.
                reasoning: false,
                tools,
                policy: Policy::new(&[], &[]).rules(self.format.rules()).authoring(),
                max_turns: limit("max_turns", 30).max(1),
                max_tool_calls: limit("max_tool_calls", 100),
                schema: schema(),
                routes: Vec::new(),
                root: settings["root"].as_str().map(str::to_owned),
                frozen_additions: false,
                ask,
                run_approved: Vec::new(),
                observer: self.progress.clone().map(|p| (skill.clone(), p)),
                finish: |report| {
                    if report["status"] == "incomplete" {
                        "incomplete"
                    } else {
                        "completed"
                    }
                },
            },
        )
        .await
    }
}
