//! `speccoder.skill.run`: an installed Agent Skill, hosted.
//!
//! The skill's `SKILL.md` body is the instruction; speccoder's governed tool
//! loop runs it with the planning policy (Spec Kit's artifacts and scripts;
//! anything else needs approval) and an `ask_person` tool for clarifying
//! questions. Everything it decides by is in the journaled input.

use agentplane::prelude::*;
use serde_json::{Value, json};

use super::attempt::{Asker, LoopSpec, ask_person, tool_loop};
use crate::approvals::Approver;
use crate::tools::policy::Policy;

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.skill.run";

/// What the host tells the model, before the skill itself.
pub const POLICY: &str = "You are speccoder, running the Agent Skill below for a person in their \
GitHub Spec Kit project. Follow the skill's instructions. You can read the project, write Spec \
Kit artifacts (specs/, .specify/feature.json, .specify/memory/) and run Spec Kit's own scripts \
under .specify/scripts/ with run_command (argv, no shell). Do not write source code — \
implementation is a separate step that speccoder runs with tests; other writes need a person's \
approval. When the skill says to ask the user, call ask_person with one question at a time; if \
nobody can answer, follow the skill's rule for unanswered questions. Skip steps about extension \
hooks, other agents' slash commands, committing, branching or pushing unless a Spec Kit script \
does it. Finish with the requested structured report.";

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
        "settings": { "max_turns": limits.0, "max_tool_calls": limits.1, "ask": ask, "root": root },
    })
}

/// The coded skill.
#[derive(Debug)]
pub struct Host {
    /// Prompt dialect.
    pub style: super::model_config::PromptStyle,
    /// Who answers questions and approvals inline, if anyone.
    pub approver: Option<std::sync::Arc<dyn Approver>>,
    /// Where live progress goes.
    pub progress: Option<std::sync::Arc<dyn crate::progress::Progress>>,
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
                        "system": POLICY,
                        "input": [{ "role": "user", "content": [{ "type": "input_text", "text": text }] }],
                    }),
                    super::model_config::PromptStyle::Anthropic
                    | super::model_config::PromptStyle::Chat => json!({
                        "system": POLICY,
                        "messages": [{ "role": "user", "content": [{ "type": "text", "text": text }] }],
                    }),
                }
            },
            &label,
        );
        let mut tools = crate::tools::declarations();
        tools.push(ask_person());
        let ask = (settings["ask"] == true).then(|| Asker {
            task: format!("skill/{skill}"),
            approver: self.approver.clone(),
        });
        tool_loop(
            cx,
            &prompt,
            LoopSpec {
                tools,
                policy: Policy::new(&[], &[]).authoring(),
                max_turns: limit("max_turns", 30).max(1),
                max_tool_calls: limit("max_tool_calls", 100),
                schema: schema(),
                routes: Vec::new(),
                root: settings["root"].as_str().map(str::to_owned),
                frozen_additions: false,
                ask,
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
