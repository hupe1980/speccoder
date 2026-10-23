//! `speccoder.task.attempt`: one bounded, governed attempt at a task
//!.
//!
//! The model works through speccoder's tools; every call it asks for is
//! classified by the policy first (deny → refused and told; approval → the
//! attempt stops for a person; allow → dispatched through agentplane). The
//! model's closing report is recorded as its claim — the verdict is computed
//! afterwards by the verifier, never here.

use agentplane::core::Label;
use agentplane::model::{ToolDeclaration, Transcript};
use agentplane::prelude::*;
use agentplane::tools::ToolId;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};
use super::propose::layout;
use super::provenance;
use crate::approvals::{Answer, Approver, Request};
use crate::tools::policy::{Decision, Policy};

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.attempt";

/// The control tool for an honest exit. Handled by the skill, not
/// dispatched anywhere.
pub const REPORT_BLOCKED: &str = "report_blocked";

/// What speccoder tells the model about an attempt; stable for the project.
pub const POLICY: &str = "You are speccoder, a coding agent implementing exactly one GitHub Spec \
Kit task in a git worktree. Use the tools to inspect the code, make the smallest change that \
completes the task, and run the project's checks with run_command to see whether it works. Stay \
inside the task's scope. You cannot edit the spec (.specify/, specs/), the checks' configuration \
or .git; such calls are refused. Some commands need a person's approval and will end the attempt — \
prefer configured checks. If the task cannot be done as written — it contradicts the spec, needs \
information or access you lack, or a test is wrong — call report_blocked with the reason instead \
of working around it. Test files that existed when the attempt started are frozen: add new tests in new files (or \
next to the code), never by editing those. Never weaken, skip or delete tests to make checks pass. Never return values copied from \
tests or examples in place of the specified behaviour (a constant where the spec asks for a \
computed, fetched or current value is a cheat): if the specified behaviour is impossible under \
the stated constraints, call report_blocked. You are not the \
judge: when you finish, speccoder runs the checks itself and decides. Finish with the requested \
structured report.";

/// The final report's schema (the model's claim).
#[must_use]
pub fn report_schema() -> Value {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "status": { "type": "string", "enum": ["done", "incomplete"] },
            "files_changed": strings,
            "checks_run": strings,
            "notes": strings,
        },
        "required": ["summary", "status", "files_changed", "checks_run", "notes"],
        "additionalProperties": false,
    })
}

fn report_blocked() -> ToolDeclaration {
    ToolDeclaration::new(
        REPORT_BLOCKED,
        "Stop the attempt because the task cannot be done as written; a person will look at the reason.",
        json!({
            "type": "object",
            "properties": { "reason": { "type": "string" } },
            "required": ["reason"],
            "additionalProperties": false,
        }),
    )
}

/// Asking a person about one call, as a journaled effect: live, the
/// approver asks (a terminal prompt); in replay, the recorded answer comes
/// back and nobody is asked. The answer is the operator's: trusted.
#[derive(Debug, Clone)]
pub struct AskPerson {
    /// What the person sees; also the effect's identity.
    pub request: Request,
    /// Who asks; `None` in replay, where the answer comes from history.
    pub approver: Option<std::sync::Arc<dyn Approver>>,
}

#[async_trait]
impl agentplane::core::Effect for AskPerson {
    type Output = Answer;

    fn trust(&self) -> agentplane::core::Trust {
        agentplane::core::Trust::Trusted
    }

    fn descriptor(&self) -> agentplane::core::EffectDescriptor {
        agentplane::core::EffectDescriptor::new(
            "speccoder.approval.ask",
            serde_json::to_value(&self.request).unwrap_or(Value::Null),
        )
    }

    fn mutates(&self) -> bool {
        false
    }

    fn recovery(&self) -> agentplane::core::Recovery {
        // Asking again after a crash is harmless.
        agentplane::core::Recovery::Retry
    }

    fn max_sensitivity(&self) -> Sensitivity {
        Sensitivity::Internal
    }

    async fn perform(&self) -> Result<Answer, agentplane::core::EffectError> {
        // A person may take minutes: wait off the runtime's thread, so the
        // run keeps renewing its lease meanwhile (`perform` must not
        // block).
        match &self.approver {
            Some(a) => {
                let (a, request) = (std::sync::Arc::clone(a), self.request.clone());
                tokio::task::spawn_blocking(move || a.ask(&request))
                    .await
                    .map_err(|e| {
                        agentplane::core::EffectError::Rejected(format!("the prompt failed: {e}"))
                    })
            }
            None => Err(agentplane::core::EffectError::Rejected(
                "no one to ask: approvals need a terminal".to_owned(),
            )),
        }
    }
}

/// The question tool a hosted skill may use.
pub const ASK_PERSON: &str = "ask_person";

/// The `ask_person` declaration.
#[must_use]
pub fn ask_person() -> ToolDeclaration {
    ToolDeclaration::new(
        ASK_PERSON,
        "Ask the person one question and wait for their answer. If nobody can answer, the reply says so.",
        json!({
            "type": "object",
            "properties": { "question": { "type": "string" } },
            "required": ["question"],
            "additionalProperties": false,
        }),
    )
}

/// A question to a person, as a journaled effect: live the approver asks;
/// in replay the recorded answer returns. The answer is the operator's.
#[derive(Debug, Clone)]
pub struct AskQuestion {
    /// What is asked.
    pub question: String,
    /// The approver; `None` in replay.
    pub approver: Option<std::sync::Arc<dyn Approver>>,
}

#[async_trait]
impl agentplane::core::Effect for AskQuestion {
    type Output = Option<String>;

    fn trust(&self) -> agentplane::core::Trust {
        agentplane::core::Trust::Trusted
    }

    fn descriptor(&self) -> agentplane::core::EffectDescriptor {
        agentplane::core::EffectDescriptor::new(
            "speccoder.person.answer",
            json!({ "question": self.question }),
        )
    }

    fn mutates(&self) -> bool {
        false
    }

    fn recovery(&self) -> agentplane::core::Recovery {
        agentplane::core::Recovery::Retry
    }

    fn max_sensitivity(&self) -> Sensitivity {
        Sensitivity::Internal
    }

    async fn perform(&self) -> Result<Option<String>, agentplane::core::EffectError> {
        match &self.approver {
            Some(a) => {
                let (a, question) = (std::sync::Arc::clone(a), self.question.clone());
                tokio::task::spawn_blocking(move || a.answer(&question))
                    .await
                    .map_err(|e| {
                        agentplane::core::EffectError::Rejected(format!("the prompt failed: {e}"))
                    })
            }
            None => Err(agentplane::core::EffectError::Rejected(
                "no one to ask: questions need a terminal".to_owned(),
            )),
        }
    }
}

/// Who an attempt asks when a call needs approval.
#[derive(Debug, Clone)]
pub struct Asker {
    /// `feature/task`, shown to the person.
    pub task: String,
    /// The approver; `None` in replay.
    pub approver: Option<std::sync::Arc<dyn Approver>>,
}

/// The coded skill.
///
/// Everything its decisions depend on — the bundle, the policy's command
/// prefixes and the limits — arrives in the run's input, so it is journaled
/// and strict replay needs no configuration. Only the prompt dialect
/// is construction-time, and it is part of each model call's effect key.
#[derive(Debug)]
pub struct Attempt {
    /// Prompt dialect.
    pub style: PromptStyle,
    /// Cache breakpoint lifetime.
    pub cache_ttl: CacheTtl,
    /// Who to ask inline, when the run's input says to ask.
    pub approver: Option<std::sync::Arc<dyn Approver>>,
    /// Where live progress goes; not journaled, `None` in replay.
    pub progress: Option<std::sync::Arc<dyn crate::progress::Progress>>,
}

/// The run input: `{ bundle, settings: { checks, allow, max_turns,
/// max_tool_calls, frozen, … }, digest }` — `digest` is the verifier's
/// account of the previous attempt, `null` on a first attempt.
///
/// Labelled by where each part came from: the bundle is the
/// person's committed spec and plan (trusted); the digest quotes check
/// output (`speccoder://verifier`); the settings list MCP tool descriptions
/// (their servers).
#[must_use]
pub fn input(bundle: Value, settings: &Settings<'_>, digest: Option<&Value>) -> Tainted<Value> {
    let digest = match digest.filter(|d| !d.is_null()) {
        Some(d) => Tainted::with_label(d.clone(), provenance::from(provenance::VERIFIER)),
        None => Tainted::trusted(Value::Null),
    };
    // Each MCP tool's description and schema are its server's words.
    let mcp = Tainted::array(settings.mcp.iter().map(|t| {
        Tainted::with_label(
            serde_json::to_value(t).unwrap_or(Value::Null),
            agentplane::core::Label::untrusted(provenance::tool(&t.server, &t.tool)),
        )
    }));
    let trusted = |v: Value| Tainted::trusted(v);
    let settings = Tainted::object([
        ("checks", trusted(json!(settings.checks))),
        ("allow", trusted(json!(settings.allow))),
        ("max_turns", trusted(json!(settings.max_turns))),
        ("max_tool_calls", trusted(json!(settings.max_tool_calls))),
        ("frozen", trusted(json!(settings.frozen))),
        ("approved", trusted(json!(settings.approved))),
        ("rejected", trusted(json!(settings.rejected))),
        ("nudges", trusted(json!(settings.nudges))),
        ("mcp", mcp),
        ("root", trusted(json!(settings.root))),
        (
            "frozen_additions",
            trusted(json!(settings.frozen_additions)),
        ),
        ("network_allow", trusted(json!(settings.network_allow))),
        ("ask", trusted(json!(settings.ask))),
    ]);
    Tainted::object([
        ("bundle", Tainted::trusted(bundle)),
        ("digest", digest),
        ("settings", settings),
    ])
}

/// A refusal speccoder composes about a call quotes it — the tool, path or
/// command the model chose — so it carries the call's label; only
/// constant replies are trusted.
fn refusal(text: String, call: &agentplane::core::Label) -> Tainted<String> {
    Tainted::with_label(text, call.clone())
}

/// The parts of an attempt's input its messages quote.
const MESSAGE_PARTS: [&str; 5] = [
    "/bundle",
    "/digest",
    "/settings/nudges",
    "/settings/rejected",
    "/settings/mcp",
];

/// The label of the conversation part: the join of the labels the run was
/// admitted with for the parts it quotes — read, never inferred. A part
/// without a label of its own takes the whole input's (fails closed).
fn conversation_label(input: &Tainted<Value>) -> agentplane::core::Label {
    MESSAGE_PARTS
        .iter()
        .map(|p| input.label_at(p).unwrap_or_else(|| input.label()).clone())
        .fold(agentplane::core::Label::trusted(), |acc, l| acc.join(&l))
}

/// What an attempt decides by, besides its bundle (all journaled).
#[derive(Debug, Clone, Copy)]
pub struct Settings<'a> {
    /// Configured checks' argvs (their prefixes are allowed commands).
    pub checks: &'a [Vec<String>],
    /// Extra allowed command prefixes.
    pub allow: &'a [Vec<String>],
    /// Model turns.
    pub max_turns: u32,
    /// Tool calls.
    pub max_tool_calls: u32,
    /// Test files frozen for this attempt.
    pub frozen: &'a [String],
    /// A person's approvals for this task.
    pub approved: &'a [crate::tools::policy::Grant],
    /// A person's rejections for this task.
    pub rejected: &'a [crate::tools::policy::Rejection],
    /// A person's notes for this task (`nudge`).
    pub nudges: &'a [String],
    /// The project's MCP tools for this run.
    pub mcp: &'a [crate::mcp::McpTool],
    /// The worktree's absolute (canonical) path.
    pub root: &'a str,
    /// Frozen tests may gain lines without approval (`[tests] frozen`).
    pub frozen_additions: bool,
    /// Ask a person inline when a call needs approval.
    pub ask: bool,
    /// Command prefixes that get the network (`[sandbox] network_allow`).
    pub network_allow: &'a [Vec<String>],
}

/// The calls consecutive denials of which end an attempt.
pub const MAX_CONSECUTIVE_DENIALS: u32 = 3;

/// The digest as the model reads it: facts from the verifier only.
fn digest_text(digest: &Value) -> String {
    let mut out = format!(
        "# Previous attempt\n\nAttempt {} at this task was verified by speccoder and FAILED. Its \
changes were discarded; the worktree is back where that attempt started. You start fresh — use \
these facts from the verifier, check them yourself, and do not repeat the mistake.\n",
        digest["attempt"].as_u64().unwrap_or(1)
    );
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let reasons = strings(&digest["reasons"]);
    if !reasons.is_empty() {
        out.push_str("\n## Verdict reasons\n\n");
        for r in reasons {
            out.push_str(&format!("- {r}\n"));
        }
    }
    for c in digest["checks"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "\n## Check `{}` ({}): `{}`\n",
            c["name"].as_str().unwrap_or_default(),
            c["status"].as_str().unwrap_or_default(),
            strings(&c["argv"]).join(" ")
        ));
        let tests = strings(&c["failing_tests"]);
        if !tests.is_empty() {
            out.push_str(&format!("\nFailing tests: {}\n", tests.join(", ")));
        }
        out.push_str(&format!(
            "\n```text\n{}\n```\n",
            c["excerpt"].as_str().unwrap_or_default()
        ));
    }
    let discarded = strings(&digest["discarded"]);
    if !discarded.is_empty() {
        out.push_str(&format!(
            "\n## Files the discarded attempt changed\n\n{}\n",
            discarded.join("\n")
        ));
    }
    out
}

fn argvs(v: &Value) -> Vec<Vec<String>> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|argv| {
                    argv.as_array()
                        .map(|p| {
                            p.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn usage_sum(total: &mut [u64; 4], usage: &Value) {
    for (i, key) in [
        "input_tokens",
        "output_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
    ]
    .iter()
    .enumerate()
    {
        total[i] += usage[*key].as_u64().unwrap_or(0);
    }
}

#[async_trait]
impl Skill for Attempt {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor::new("speccoder.task.attempt").provides(CAPABILITY)
    }

    async fn invoke(
        &self,
        cx: &mut StepCtx<'_>,
        input: Tainted<Value>,
    ) -> Result<Outcome, SkillError> {
        let (ttl, style) = (self.cache_ttl, self.style);
        let settings = input.peek()["settings"].clone();
        let task_name = format!(
            "{}/{}",
            input.peek()["bundle"]["feature"]
                .as_str()
                .unwrap_or_default(),
            input.peek()["bundle"]["task"]["id"]
                .as_str()
                .unwrap_or_default()
        );
        let frozen: Vec<String> = settings["frozen"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let decisions = |key: &str| settings[key].clone();
        let approved: Vec<crate::tools::policy::Grant> =
            serde_json::from_value(decisions("approved")).unwrap_or_default();
        let rejected: Vec<crate::tools::policy::Rejection> =
            serde_json::from_value(decisions("rejected")).unwrap_or_default();
        let told = rejected.clone();
        let mcp: Vec<crate::mcp::McpTool> =
            serde_json::from_value(settings["mcp"].clone()).unwrap_or_default();
        let policy = Policy::new(&argvs(&settings["checks"]), &argvs(&settings["allow"]))
            .with_frozen(frozen)
            .with_decisions(approved, rejected)
            .with_mcp(mcp.iter().map(|t| (t.name.clone(), t.decision.clone())))
            .with_network(argvs(&settings["network_allow"]));
        let max_turns = settings["max_turns"]
            .as_u64()
            .map_or(30, |n| u32::try_from(n).unwrap_or(u32::MAX))
            .max(1);
        let max_tool_calls = settings["max_tool_calls"]
            .as_u64()
            .map_or(100, |n| u32::try_from(n).unwrap_or(u32::MAX));
        // Same stable prefix as `propose`, with the attempt policy in front;
        // a retry's digest goes after it, so the cached prefix is shared.
        let label = conversation_label(&input);
        let raw = input.peek().clone();
        let prompt = provenance::labelled_prompt(
            {
                let input = raw;
                let mut p = layout(&input["bundle"], ttl, style, POLICY);
                if !input["digest"].is_null() {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat => ("messages", "text"),
                    };
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content
                            .push(json!({ "type": kind, "text": digest_text(&input["digest"]) }));
                    }
                }
                // A person's notes for this task.
                let notes: Vec<String> = input["settings"]["nudges"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.as_str().map(|n| format!("- {n}")))
                            .collect()
                    })
                    .unwrap_or_default();
                if !notes.is_empty() {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat => ("messages", "text"),
                    };
                    let text = format!("# Notes from a person\n\n{}\n", notes.join("\n"));
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                // A person's rejections, so the model does not ask again.
                if !told.is_empty() {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat => ("messages", "text"),
                    };
                    let lines: Vec<String> = told
                        .iter()
                        .map(|r| format!("- {}: {}", r.grant.describe(), r.reason))
                        .collect();
                    let text = format!(
                        "# Rejected by a person\n\nThese calls were rejected for this task; find another way or call report_blocked:\n\n{}\n",
                        lines.join("\n")
                    );
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                p
            },
            &label,
        );
        let mut tools = crate::tools::declarations();
        tools.push(report_blocked());
        // MCP tools the policy does not deny, as the journaled input lists them.
        let routes: Vec<(String, String, String)> = mcp
            .iter()
            .filter(|t| t.decision != "deny")
            .map(|t| (t.name.clone(), t.server.clone(), t.tool.clone()))
            .collect();
        tools.extend(mcp.iter().filter(|t| t.decision != "deny").map(|t| {
            ToolDeclaration::new(t.name.clone(), t.description.clone(), t.schema.clone())
        }));
        // Journaled: whether this attempt asks inline decides its effects.
        let ask = (settings["ask"] == true).then(|| Asker {
            task: task_name.clone(),
            approver: self.approver.clone(),
        });
        tool_loop(
            cx,
            &prompt,
            LoopSpec {
                tools,
                policy,
                max_turns,
                max_tool_calls,
                schema: report_schema(),
                routes,
                root: settings["root"].as_str().map(str::to_owned),
                frozen_additions: settings["frozen_additions"] == true,
                ask,
                observer: self.progress.clone().map(|p| {
                    (
                        task_name.rsplit('/').next().unwrap_or_default().to_owned(),
                        p,
                    )
                }),
                finish: |report| {
                    // The model's own word that it did not finish is believed;
                    // its word that it did is not (the verifier decides that).
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

/// Turns an absolute `path` argument inside the workspace `root` into a
/// workspace-relative one. Tools print absolute paths (Spec Kit's scripts
/// do), and a model passes them on; outside the root a path stays as it is
/// — and the policy refuses it.
fn relativise(arguments: &mut Value, root: Option<&str>) {
    let (Some(root), Some(path)) = (root, arguments.get("path").and_then(Value::as_str)) else {
        return;
    };
    let root = root.trim_end_matches('/');
    if let Some(rel) = path.strip_prefix(root).and_then(|r| r.strip_prefix('/')) {
        let rel = if rel.is_empty() {
            ".".to_owned()
        } else {
            rel.to_owned()
        };
        arguments["path"] = Value::String(rel);
    }
}

/// Reports one logged tool call to whoever watches.
fn emit(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    turn: u32,
    entry: &Value,
) {
    if let Some((task, p)) = observer {
        let tool = entry["tool"].as_str().unwrap_or_default();
        p.event(&crate::progress::Event::ToolCall {
            task: task.clone(),
            turn,
            tool: tool.to_owned(),
            target: crate::progress::target(tool, &entry["arguments"]),
            decision: entry["decision"].as_str().unwrap_or_default().to_owned(),
        });
    }
}

/// What a governed tool loop offers the model and asks of it.
pub(crate) struct LoopSpec {
    /// The tools declared to the model; any other name is refused.
    pub tools: Vec<ToolDeclaration>,
    /// Classifies every call before dispatch.
    pub policy: Policy,
    /// Model turns.
    pub max_turns: u32,
    /// Tool calls.
    pub max_tool_calls: u32,
    /// The closing answer's schema.
    pub schema: Value,
    /// Declared tools served elsewhere: (name, server, tool) — MCP.
    pub routes: Vec<(String, String, String)>,
    /// The workspace's absolute path, from the journaled input: absolute
    /// paths inside it are made relative before the policy sees them.
    pub root: Option<String>,
    /// A write to a frozen test that only adds lines needs no approval.
    pub frozen_additions: bool,
    /// The status for a closing answer.
    pub finish: fn(&Value) -> &'static str,
    /// Ask a person instead of stopping when a call needs approval.
    pub ask: Option<Asker>,
    /// Live progress: the task ID and where to report.
    pub observer: Option<(String, std::sync::Arc<dyn crate::progress::Progress>)>,
}

/// The governed tool loop shared by attempts and convergence: every call is
/// classified first (deny → refused and told; approval → stop; allow →
/// dispatched through agentplane); the closing structured answer ends it.
pub(crate) async fn tool_loop(
    cx: &mut StepCtx<'_>,
    prompt: &Tainted<Value>,
    spec: LoopSpec,
) -> Result<Outcome, SkillError> {
    let LoopSpec {
        tools,
        policy,
        max_turns,
        max_tool_calls,
        schema,
        routes,
        root,
        frozen_additions,
        finish,
        ask,
        observer,
    } = spec;
    let catalog = crate::tools::catalog();

    // What the conversation has read, with its labels: agentplane checks
    // every model call against the join.
    let mut transcript = Transcript::new(prompt);
    let mut conversation: Label = prompt.label().clone();
    let mut calls = 0u32;
    let mut denied_in_a_row = 0u32;
    let mut usage = [0u64; 4];
    let mut log: Vec<Value> = Vec::new();

    let end = |status: &str, extra: Value, log: &[Value], usage: [u64; 4], conversation: &Label| {
        let mut out = json!({
            "status": status,
            "tool_calls": log,
            "usage": {
                "input_tokens": usage[0],
                "output_tokens": usage[1],
                "cache_read_tokens": usage[2],
                "cache_write_tokens": usage[3],
            },
        });
        if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        Ok(Outcome::done(Tainted::with_label(
            out,
            conversation.clone(),
        )))
    };

    for turn in 0..max_turns {
        // The prompt was built from labelled parts (`/system` is speccoder's
        // constant instruction); what the conversation read since rides in
        // the transcript, so the request is labelled by its real sources.
        let tools_now = tools.clone();
        let schema_now = schema.clone();
        let read_so_far = &transcript;
        let completion = cx
            .complete_with(prompt, move |call| {
                call.with_tools(tools_now)
                    .continuing(read_so_far)
                    .expecting(schema_now)
            })
            .await?;
        conversation = transcript.label().join(completion.label());
        usage_sum(
            &mut usage,
            &serde_json::to_value(completion.peek().usage).unwrap_or(Value::Null),
        );
        if let Some((task, p)) = &observer {
            p.event(&crate::progress::Event::Usage {
                task: task.clone(),
                input_tokens: usage[0],
                output_tokens: usage[1],
                cache_read_tokens: usage[2],
                usd: None,
            });
        }

        if completion.peek().truncated {
            return Ok(Outcome::fail(
                "the model ran out of output budget mid-turn; nothing it was still writing was executed — raise [model] max_output_tokens",
            ));
        }
        let asked_calls = transcript.asked(&completion);
        if asked_calls.is_empty() {
            let report = completion
                .peek()
                .structured
                .clone()
                .unwrap_or_else(|| json!({ "summary": completion.peek().text, "status": "incomplete", "files_changed": [], "checks_run": [], "notes": ["the answer was not the requested structure"] }));
            let status = finish(&report);
            return end(
                status,
                json!({ "report": report }),
                &log,
                usage,
                &conversation,
            );
        }

        for mut asked in asked_calls {
            relativise(&mut asked.arguments, root.as_deref());
            calls += 1;
            if calls > max_tool_calls {
                return end(
                    "budget_exhausted",
                    json!({ "reason": format!("more than {max_tool_calls} tool calls") }),
                    &log,
                    usage,
                    &conversation,
                );
            }
            if asked.name == ASK_PERSON && tools.iter().any(|t| t.name == ASK_PERSON) {
                let question = asked.arguments["question"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                // The person's answer is theirs: trusted.
                let answer = match &ask {
                    Some(asker) => cx
                        .effect(AskQuestion {
                            question,
                            approver: asker.approver.clone(),
                        })
                        .await?
                        .peek()
                        .clone(),
                    None => None,
                };
                let entry = json!({ "tool": ASK_PERSON, "arguments": asked.arguments, "decision": "allow", "answered": answer.is_some() });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                let text = answer.unwrap_or_else(|| {
                    "Nobody can answer now. Record the question in the artifact as [NEEDS CLARIFICATION: <question>] and continue.".to_owned()
                });
                transcript.answered(asked, &Tainted::trusted(json!({ "answer": text })));
                continue;
            }
            if asked.name == REPORT_BLOCKED && tools.iter().any(|t| t.name == REPORT_BLOCKED) {
                let reason = asked.arguments["reason"]
                    .as_str()
                    .unwrap_or("no reason given")
                    .to_owned();
                let entry = json!({ "tool": REPORT_BLOCKED, "arguments": asked.arguments, "decision": "stop" });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                return end(
                    "blocked",
                    json!({ "reason": reason }),
                    &log,
                    usage,
                    &conversation,
                );
            }
            let declared = tools.iter().any(|t| t.name == asked.name);
            let target = catalog
                .iter()
                .find(|t| declared && t.name == asked.name)
                .map(|t| ToolId::new(t.server, t.name))
                .or_else(|| {
                    routes
                        .iter()
                        .find(|(n, _, _)| declared && *n == asked.name)
                        .map(|(_, server, tool)| ToolId::new(server, tool))
                });
            let Some(target) = target else {
                let entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "unknown" });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                transcript.failed(asked, Tainted::trusted("no tool of that name".to_owned()));
                continue;
            };
            let mut decision = policy.classify(&asked.name, &asked.arguments);
            // `[tests] frozen = "additions"`: read the frozen test (a
            // journaled call, so replay sees the same content) and allow a
            // write that keeps every existing line.
            let mut additions_only = false;
            if frozen_additions
                && matches!(&decision, Decision::Approval(r) if r.contains("frozen test"))
            {
                let path = asked.arguments["path"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let read = cx
                    .call_tool(
                        ToolId::new("fs", "read_file"),
                        Tainted::with_label(json!({ "path": path }), completion.label().clone()),
                    )
                    .await?;
                let current = read.peek().clone();
                // `read_file` joins lines with `\n`; the file's last newline is
                // restored so an edit's `old_string` matches as on disk.
                let lines = current["content"].as_str().unwrap_or_default();
                let old_text = if lines.is_empty() {
                    String::new()
                } else {
                    format!("{lines}\n")
                };
                let old = old_text.as_str();
                let readable = current["ok"] == true && current["truncated"] != true;
                let new = match asked.name.as_str() {
                    "write_file" => asked.arguments["content"].as_str().map(str::to_owned),
                    "edit_file" => {
                        let (from, to) = (
                            asked.arguments["old_string"].as_str().unwrap_or_default(),
                            asked.arguments["new_string"].as_str().unwrap_or_default(),
                        );
                        let all = asked.arguments["replace_all"] == true;
                        (!from.is_empty() && (all || old.matches(from).count() == 1))
                            .then(|| old.replace(from, to))
                    }
                    _ => None,
                };
                if readable && new.is_some_and(|n| crate::integrity::only_additions(old, &n)) {
                    decision = Decision::Allow;
                    additions_only = true;
                }
            }
            // ask a person now, as a journaled effect, instead of stopping.
            let mut person: Option<Value> = None;
            if let (Decision::Approval(reason), Some(asker)) = (&decision, &ask) {
                let request = Request {
                    task: asker.task.clone(),
                    tool: asked.name.clone(),
                    arguments: asked.arguments.clone(),
                    reason: reason.clone(),
                    covers: crate::tools::policy::Grant::of(&asked.name, &asked.arguments)
                        .map(|g| g.describe())
                        .unwrap_or_default(),
                    risk: crate::approvals::risk(&asked.name, reason).to_owned(),
                };
                let answer = cx
                    .effect(AskPerson {
                        request,
                        approver: asker.approver.clone(),
                    })
                    .await?;
                let answer = answer.peek().clone();
                // Who answered is journaled with the answer.
                let approver = serde_json::to_value(&answer.by).unwrap_or(Value::Null);
                if answer.approved {
                    person = Some(
                        json!({ "approved_by": "person", "for_task": answer.for_task, "approver": approver }),
                    );
                    decision = Decision::Allow;
                } else {
                    let why = answer
                        .reason
                        .unwrap_or_else(|| "no reason given".to_owned());
                    person = Some(
                        json!({ "rejected_by": "person", "approver": approver, "rejection": why }),
                    );
                    decision = Decision::Deny(format!("a person rejected this: {why}"));
                }
            }
            let with_person = |mut entry: Value| {
                if let (Some(o), Some(p)) = (
                    entry.as_object_mut(),
                    person.as_ref().and_then(Value::as_object),
                ) {
                    o.extend(p.clone());
                }
                entry
            };
            match decision {
                Decision::Deny(reason) => {
                    let entry = with_person(
                        json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "deny", "reason": reason }),
                    );
                    emit(observer.as_ref(), turn + 1, &entry);
                    log.push(entry);
                    // The reason quotes the call (its tool, path, command, or a
                    // person's words about it): it carries the call's label.
                    transcript.failed(
                        asked,
                        refusal(format!("denied: {reason}"), completion.label()),
                    );
                    // Escalation: a model that keeps asking for what it may
                    // not do is stopped for a person.
                    denied_in_a_row += 1;
                    if denied_in_a_row >= MAX_CONSECUTIVE_DENIALS {
                        return end(
                            "denied",
                            json!({ "reason": format!("{denied_in_a_row} calls in a row were denied; last: {reason}") }),
                            &log,
                            usage,
                            &conversation,
                        );
                    }
                    continue;
                }
                Decision::Approval(reason) => {
                    let entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "approval", "reason": reason });
                    emit(observer.as_ref(), turn + 1, &entry);
                    log.push(entry);
                    return end(
                        "needs_approval",
                        json!({ "pending": { "tool": asked.name, "arguments": asked.arguments, "reason": reason } }),
                        &log,
                        usage,
                        &conversation,
                    );
                }
                Decision::Allow => {
                    denied_in_a_row = 0;
                    // The arguments carry the model's label: agentplane's
                    // taint gate checks them against the grant's fields (an
                    // argument-less mutating tool is granted exactly `{}`).
                    // A command gets the call's id (`SPECCODER_CALL_ID`):
                    // a call resumed after a crash has the same one, so a
                    // script can tell a repeat from a new call.
                    let mut arguments = asked.arguments.clone();
                    if asked.name == "run_command"
                        && let Some(o) = arguments.as_object_mut()
                    {
                        o.insert("call_id".to_owned(), Value::String(asked.id.clone()));
                    }
                    let args = Tainted::with_label(arguments, completion.label().clone());
                    let result = match cx.call_tool(target.clone(), args).await {
                        Ok(r) => r,
                        // A failure the far side reported is the model's to
                        // read, labelled by who wrote it.
                        Err(error) => match cx.relayed(&error) {
                            Some(detail) => {
                                let entry = with_person(
                                    json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "allow", "ok": false }),
                                );
                                emit(observer.as_ref(), turn + 1, &entry);
                                log.push(entry);
                                transcript.failed(asked, detail);
                                continue;
                            }
                            None => return Err(error.into()),
                        },
                    };
                    let output = result.peek().clone();
                    let ok = output["ok"].as_bool() != Some(false);
                    let mut entry = with_person(
                        json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "allow", "ok": ok }),
                    );
                    if additions_only {
                        entry["frozen_additions"] = json!(true);
                    }
                    emit(observer.as_ref(), turn + 1, &entry);
                    log.push(entry);
                    // A tool that refused (`error`) is a failed call; a
                    // command that ran and exited non-zero is a result the
                    // model must read in full — its stdout and stderr are
                    // the diagnostics (found by the first real evaluation,
                    // where the model saw only the word "failed").
                    match output["error"].as_str() {
                        Some(error) if !ok => transcript.failed(
                            asked,
                            Tainted::with_label(error.to_owned(), result.label().clone()),
                        ),
                        _ => transcript.answered(asked, &result),
                    }
                }
            }
        }
    }
    end(
        "budget_exhausted",
        json!({ "reason": format!("more than {max_turns} model turns") }),
        &log,
        usage,
        &conversation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_read_from_the_parts_and_refusals_carry_the_call() {
        use agentplane::core::{Label, SourceId};
        let model = Label::untrusted(SourceId::new("model:x/y"));
        // A refusal quoting the call is the call's; a constant is trusted.
        assert!(
            refusal("denied: `src/x` is protected".to_owned(), &model)
                .label()
                .is_untrusted()
        );
        // Parts labelled when admitted: the spec trusted, the digest the verifier's.
        let input = Tainted::object([
            ("bundle", Tainted::trusted(json!({}))),
            (
                "digest",
                Tainted::with_label(json!({}), provenance::from(provenance::VERIFIER)),
            ),
            (
                "settings",
                Tainted::object([("nudges", Tainted::trusted(json!([])))]),
            ),
        ]);
        let label = conversation_label(&input);
        assert!(
            label
                .provenance
                .contains(&SourceId::new(provenance::VERIFIER))
        );
        // No labels of its own: every part takes the whole input's (fails closed).
        let flat = Tainted::with_label(json!({ "bundle": {} }), model.clone());
        assert_eq!(conversation_label(&flat).provenance, model.provenance);
    }

    #[test]
    fn absolute_paths_inside_the_root_become_relative_and_others_stay() {
        let mut a = json!({ "path": "/work/repo/specs/001-x/plan.md" });
        relativise(&mut a, Some("/work/repo"));
        assert_eq!(a["path"], "specs/001-x/plan.md");
        let mut outside = json!({ "path": "/work/repository-two/x" });
        relativise(&mut outside, Some("/work/repo"));
        assert_eq!(outside["path"], "/work/repository-two/x");
        let mut none = json!({ "path": "/etc/passwd" });
        relativise(&mut none, None);
        assert_eq!(none["path"], "/etc/passwd");
    }
}
