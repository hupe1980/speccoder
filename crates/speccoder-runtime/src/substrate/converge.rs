//! `speccoder.feature.converge`: assess a feature's code against its spec,
//! plan and tasks, read-only.
//!
//! The model may only read; it returns structured findings. speccoder — not
//! the model — turns actionable findings into a `## Phase N: Convergence`
//! section, appended behind a prefix guard. `unrequested` findings go to a
//! person and are never turned into tasks.

use agentplane::prelude::*;
use serde_json::{Value, json};

use super::attempt::{LoopSpec, tool_loop};
use super::model_config::{CacheTtl, PromptStyle};
use crate::tools::policy::Policy;

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.feature.converge";

/// The tools converge may use: reading only.
pub const READ_ONLY: [&str; 5] = [
    "read_file",
    "list_files",
    "search_code",
    "git_status",
    "git_diff",
];

/// What the assessor is told (after Spec Kit's `/speckit-converge`).
pub const POLICY: &str = "You are the convergence step of speccoder. Assess the CURRENT code of \
a GitHub Spec Kit feature against its spec.md, plan.md and tasks.md — the sole source of intent — \
with the constitution as governing constraints. Ticked tasks are not evidence: read the code. You \
can only read (read_file, list_files, search_code, git_status, git_diff); you change nothing. For \
every functional requirement (FR-###), success criterion that needs buildable work (SC-###), user \
story acceptance scenario (USn/ACm), plan decision and constitution MUST principle, report a \
finding only where there is a gap, classified as `missing` (absent), `partial` (exists but does \
not fully satisfy), `contradicts` (conflicts with stated intent or a constitution MUST) or \
`unrequested` (code that exists although the spec, plan and tasks do not call for it). A \
requirement or rule the code does not meet is ALWAYS missing, partial or contradicts — never \
unrequested. Severity: CRITICAL (constitution \
MUST violation, or missing/contradicts that blocks a P1 story), HIGH (missing/partial on a core \
requirement), MEDIUM, LOW. For each finding give `task`: one imperative sentence that a coding \
agent can complete (no task ID, no checkbox, no file contents), `source_ref` (e.g. FR-003, \
US1/AC2, plan: storage, Constitution II), and `evidence` (the file or area you observed). Do not \
report what is already satisfied; do not invent requirements. If nothing remains, return no \
findings. Finish with the requested structured answer.";

/// The answer's schema.
#[must_use]
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "source_ref": { "type": "string" },
                        "gap_type": { "type": "string", "enum": ["missing", "partial", "contradicts", "unrequested"] },
                        "severity": { "type": "string", "enum": ["CRITICAL", "HIGH", "MEDIUM", "LOW"] },
                        "description": { "type": "string" },
                        "evidence": { "type": "string" },
                        "task": { "type": "string" },
                    },
                    "required": ["source_ref", "gap_type", "severity", "description", "evidence", "task"],
                    "additionalProperties": false,
                },
            },
            "checked": { "type": "array", "items": { "type": "string" } },
        },
        "required": ["findings", "checked"],
        "additionalProperties": false,
    })
}

/// The run input: the feature's artifacts and limits — everything the
/// skill's decisions depend on, journaled.
#[must_use]
pub fn input(
    feature: &str,
    artifacts: [&str; 4], // constitution, spec, plan, tasks
    files: &[String],
    max_turns: u32,
    max_tool_calls: u32,
) -> Tainted<Value> {
    let [constitution, spec, plan, tasks] = artifacts;
    // The artifacts are the person's; the file list is the worktree's.
    Tainted::object([
        ("feature", Tainted::trusted(json!(feature))),
        ("constitution", Tainted::trusted(json!(constitution))),
        ("spec", Tainted::trusted(json!(spec))),
        ("plan", Tainted::trusted(json!(plan))),
        ("tasks", Tainted::trusted(json!(tasks))),
        (
            "files",
            Tainted::with_label(
                json!(files),
                super::provenance::from(super::provenance::WORKTREE),
            ),
        ),
        (
            "settings",
            Tainted::trusted(json!({ "max_turns": max_turns, "max_tool_calls": max_tool_calls })),
        ),
    ])
}

fn prompt_raw(input: &Value, ttl: CacheTtl, style: PromptStyle) -> Value {
    let text = |key: &str| {
        let t = input[key].as_str().unwrap_or_default();
        if t.trim().is_empty() {
            "(none)".to_owned()
        } else {
            t.to_owned()
        }
    };
    let files = input["files"]
        .as_array()
        .map(|f| {
            f.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let feature = input["feature"].as_str().unwrap_or_default();
    let parts = [
        format!("# spec.md ({feature})\n\n{}", text("spec")),
        format!("# plan.md\n\n{}", text("plan")),
        format!("# tasks.md\n\n{}", text("tasks")),
        format!("# Project files\n\n{files}"),
    ];
    let system = format!(
        "{POLICY}\n\n# Project constitution\n\n{}",
        text("constitution")
    );
    match style {
        PromptStyle::OpenAi => json!({
            "system": system,
            "input": [{ "role": "user", "content": parts.iter().map(|t| json!({ "type": "input_text", "text": t })).collect::<Vec<_>>() }],
        }),
        PromptStyle::Anthropic | PromptStyle::Chat => {
            let breakpoint = match ttl {
                CacheTtl::FiveMinutes => json!({ "type": "ephemeral" }),
                CacheTtl::OneHour => json!({ "type": "ephemeral", "ttl": "1h" }),
            };
            let content: Vec<Value> = parts
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    if i == 1 {
                        json!({ "type": "text", "text": t, "cache_control": breakpoint })
                    } else {
                        json!({ "type": "text", "text": t })
                    }
                })
                .collect();
            json!({
                "system": system,
                "messages": [{ "role": "user", "content": content }],
            })
        }
    }
}

fn prompt(input: &Value, ttl: CacheTtl, style: PromptStyle) -> Value {
    let mut out = prompt_raw(input, ttl, style);
    if style == PromptStyle::Chat {
        super::model_config::strip_cache_control(&mut out);
    }
    out
}

/// The coded skill.
#[derive(Debug)]
pub struct Converge {
    /// Prompt dialect.
    pub style: PromptStyle,
    /// Cache breakpoint lifetime.
    pub cache_ttl: CacheTtl,
}

#[async_trait]
impl Skill for Converge {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.feature.converge").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let (ttl, style) = (self.cache_ttl, self.style);
        let settings = input.peek()["settings"].clone();
        let limit = |key: &str, default: u32| {
            settings[key]
                .as_u64()
                .map_or(default, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        let prompt = super::provenance::labelled_prompt(
            prompt(input.peek(), ttl, style),
            &super::provenance::from(super::provenance::WORKTREE),
        );
        let tools = crate::tools::declarations()
            .into_iter()
            .filter(|d| READ_ONLY.contains(&d.name.as_str()))
            .collect();
        tool_loop(
            cx,
            &prompt,
            LoopSpec {
                tools,
                policy: Policy::new(&[], &[]),
                max_turns: limit("max_turns", 30).max(1),
                max_tool_calls: limit("max_tool_calls", 100),
                schema: schema(),
                ask: None,
                observer: None,
                routes: Vec::new(),
                root: None,
                frozen_additions: false,
                finish: |_| "completed",
            },
        )
        .await
    }
}
