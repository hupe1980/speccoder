//! `speccoder.task.attempt`: one bounded, governed attempt at a task
//!.
//!
//! The model works through speccoder's tools; every call it asks for is
//! classified by the policy first (deny → refused and told; approval → the
//! attempt stops for a person; allow → dispatched through agentplane). The
//! model's closing report is recorded as its claim — the verdict is computed
//! afterwards by the verifier, never here.

use crate::format::Wording;
use agentplane::core::Label;
use agentplane::model::{ToolDeclaration, Transcript};
use agentplane::prelude::*;
use agentplane::tools::ToolId;
use serde_json::{Value, json};

use super::model_config::{CacheTtl, PromptStyle};
use super::propose::layout;
use super::provenance;
use crate::approvals::{Answer, Decision as Said, PresentContext, Request, Scope, Who};
use crate::tools::policy::{Decision, Object, Policy};

/// The capability the skill provides.
pub const CAPABILITY: &str = "speccoder.task.attempt";

/// The control tool for an honest exit. Handled by the skill, not
/// dispatched anywhere.
pub const REPORT_BLOCKED: &str = "report_blocked";

/// What speccoder tells the model about an attempt; stable for the project.
pub fn policy(w: &Wording) -> String {
    format!(
        "You are speccoder, a coding agent implementing exactly one {product} task in a git \
worktree. Use the tools to inspect the code, make the smallest change that completes the \
task, and run the project's checks with run_command to see whether it works. Stay inside \
the task's scope: the task's paths are the plan's guess — where they differ from the code, \
use the module that holds it; a change elsewhere that the task or its acceptance tests need \
(an entry point they run, a module they import) is in scope and shown to the reviewer, never \
a reason to stop. You cannot edit the spec ({intent}), the checks' configuration or \
.git; such calls are refused. Some commands need a person's approval and will end the \
attempt — prefer configured checks. If the task cannot be done as written — it \
contradicts the spec, needs information or access you lack, or a test is wrong — call \
report_blocked with the reason instead of working around it. Test files that existed \
when the attempt started are frozen: add new tests in new files (or next to the code), \
never by editing those. Never weaken, skip or delete tests to make checks pass. Never \
return values copied from tests or examples in place of the specified behaviour (a \
constant where the spec asks for a computed, fetched or current value is a cheat): if \
the specified behaviour is impossible under the stated constraints, call report_blocked. \
You are not the judge: when you finish, speccoder runs the checks itself and decides. \
Finish with the requested structured report; its status is `incomplete` only when part of \
the task is left undone — a check you could not run is no reason.",
        product = w.product,
        intent = w.intent_paths,
    )
}

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

/// How an attempt waits for a person when a call needs one: on an
/// agentplane human task the person decides — inline, from an editor, or
/// later with `speccoder task approve` — while the run is suspended.
#[derive(Debug, Clone)]
pub struct Asker {
    /// `feature/task`, shown to the person.
    pub task: String,
    /// The task's title.
    pub task_title: String,
    /// What the live run knows for the presentation.
    pub context: PresentContext,
    /// How long a call waits before it is denied as expired.
    pub wait_secs: u64,
}

impl Asker {
    /// The asker the journaled settings describe: `asks = "tasks"` waits on
    /// a task; anything else (`"stop"`, a lane) stops the attempt instead.
    #[must_use]
    pub fn from_settings(
        settings: &Value,
        task: String,
        task_title: String,
        context: PresentContext,
    ) -> Option<Self> {
        (settings["asks"] == "tasks").then(|| Self {
            task,
            task_title,
            context,
            wait_secs: settings["wait_secs"].as_u64().unwrap_or(7 * 86_400),
        })
    }
}

/// Waits for a person's decision on `spec`, registering the `approval`
/// obligation the first time: the run suspends until someone decides or the
/// deadline passes. Journaled — replay reads the decision back.
async fn wait_for_person(
    cx: &mut StepCtx<'_>,
    asker: &Asker,
    obligation: &mut bool,
    spec: &agentplane::core::TaskSpec,
) -> Result<agentplane::core::Decision, SkillError> {
    if !*obligation {
        let minutes = u32::try_from(asker.wait_secs.div_ceil(60).max(1)).unwrap_or(u32::MAX);
        cx.deadline(
            crate::approvals::tasks::DEADLINE,
            &agentplane::core::DeadlineSpec::minutes(minutes),
            None,
        )
        .await?;
        *obligation = true;
    }
    Ok(cx.task(spec).await?)
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
    /// Where live progress goes; not journaled, `None` in replay.
    pub progress: Option<std::sync::Arc<dyn crate::progress::Progress>>,
    /// The project's spec format.
    pub format: &'static dyn crate::format::Adapter,
    /// The sandbox, as a person asked about a command is told; live only.
    pub present: PresentContext,
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
pub fn input(
    bundle: Value,
    settings: &Settings<'_>,
    digest: Option<&Value>,
    instructions: &[crate::instructions::Instruction],
    test_layout: Option<&str>,
) -> Tainted<Value> {
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
    let autopilot = settings.autopilot;
    let planning = settings.planning;
    let settings = vec![
        ("checks", trusted(json!(settings.checks))),
        ("allow", trusted(json!(settings.allow))),
        ("max_turns", trusted(json!(settings.max_turns))),
        ("max_tool_calls", trusted(json!(settings.max_tool_calls))),
        ("reasoning", trusted(json!(settings.reasoning))),
        ("frozen", trusted(json!(settings.frozen))),
        ("approved", trusted(json!(settings.approved))),
        ("rejected", trusted(json!(settings.rejected))),
        ("run_approved", trusted(json!(settings.run_approved))),
        ("approved_once", trusted(json!(settings.approved_once))),
        ("nudges", trusted(json!(settings.nudges))),
        ("mcp", mcp),
        ("root", trusted(json!(settings.root))),
        (
            "frozen_additions",
            trusted(json!(settings.frozen_additions)),
        ),
        ("network_allow", trusted(json!(settings.network_allow))),
        ("allow_prefix", trusted(json!(settings.allow_prefix))),
        (
            "network_allow_prefix",
            trusted(json!(settings.network_allow_prefix)),
        ),
        ("run_id", trusted(json!(settings.run_id))),
        ("asked", trusted(json!(settings.asked))),
        ("budget", trusted(json!(settings.budget))),
        ("prompts", trusted(json!(settings.prompts))),
        ("asks", trusted(json!(settings.asks))),
        ("wait_secs", trusted(json!(settings.wait_secs))),
        ("in_place", trusted(json!(settings.in_place))),
        ("hidden", trusted(json!(settings.hidden))),
    ];
    // Only an autopilot run carries the key: every other input reads as
    // it was recorded before it existed.
    let mut settings = settings;
    if autopilot {
        settings.push(("autopilot", trusted(json!(true))));
    }
    // Likewise only a planning task's.
    if let Some(dir) = planning {
        settings.push(("planning", trusted(json!(dir))));
    }
    let settings = Tainted::object(settings);
    Tainted::object([
        ("bundle", Tainted::trusted(bundle)),
        ("digest", digest),
        ("settings", settings),
        // The repository's text, as if the model had read the files itself;
        // none is no text, and no label.
        (
            "instructions",
            if instructions.is_empty() {
                Tainted::trusted(json!([]))
            } else {
                Tainted::with_label(json!(instructions), provenance::from(provenance::WORKTREE))
            },
        ),
        // The test layout speccoder detected from the checkout.
        (
            "test_layout",
            match test_layout {
                Some(t) => Tainted::with_label(json!(t), provenance::from(provenance::WORKTREE)),
                None => Tainted::trusted(Value::Null),
            },
        ),
    ])
}

/// A refusal speccoder composes about a call quotes it — the tool, path or
/// command the model chose — so it carries the call's label; only
/// constant replies are trusted.
fn refusal(text: String, call: &agentplane::core::Label) -> Tainted<String> {
    Tainted::with_label(text, call.clone())
}

/// The parts of an attempt's input its messages quote.
const MESSAGE_PARTS: [&str; 7] = [
    "/bundle",
    "/instructions",
    "/test_layout",
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
    /// Exact commands allowed (`[commands] allow`).
    pub allow: &'a [Vec<String>],
    /// Allowed command prefixes (`[commands] allow_prefix`).
    pub allow_prefix: &'a [Vec<String>],
    /// Model turns.
    pub max_turns: u32,
    /// Tool calls.
    pub max_tool_calls: u32,
    /// Ask for the model's reasoning summary on every turn.
    pub reasoning: bool,
    /// Test files frozen for this attempt.
    pub frozen: &'a [String],
    /// A person's approvals for this task, in this run.
    pub approved: &'a [Object],
    /// A person's rejections for this task.
    pub rejected: &'a [crate::tools::policy::Rejection],
    /// Objects a person approved for every task of this run.
    pub run_approved: &'a [Object],
    /// Objects a person approved for one call each (a call a lane stopped,
    /// decided in its run's batch): this attempt may make each one time.
    pub approved_once: &'a [Object],
    /// A person's notes for this task (`nudge`).
    pub nudges: &'a [String],
    /// The project's MCP tools for this run.
    pub mcp: &'a [crate::mcp::McpTool],
    /// The worktree's absolute (canonical) path.
    pub root: &'a str,
    /// Frozen tests may gain lines without approval (`[tests] frozen`).
    pub frozen_additions: bool,
    /// How a call needing a person is handled: `"tasks"` (the attempt waits
    /// on a human task) or `"stop"` (it ends `needs_approval`; lanes).
    pub asks: &'a str,
    /// How long a call waits for a person before it is denied as expired.
    pub wait_secs: u64,
    /// Exact commands that get the network (`[sandbox] network_allow`).
    pub network_allow: &'a [Vec<String>],
    /// Prefixes that get it (`[sandbox] network_allow_prefix`).
    pub network_allow_prefix: &'a [Vec<String>],
    /// The run the attempt belongs to.
    pub run_id: &'a str,
    /// Prompts the run has shown a person so far.
    pub asked: u32,
    /// `[approvals] budget`.
    pub budget: u32,
    /// A person is prompted inline (not `--no-prompt`).
    pub prompts: bool,
    /// An in-place attempt: git that moves the checkout is denied.
    pub in_place: bool,
    /// In place: what git ignored at the run's start, hidden from the tools.
    pub hidden: &'a [String],
    /// An autopilot run: a call that would ask is refused instead.
    pub autopilot: bool,
    /// A planning task: its unit's directory (worktree-relative), whose
    /// design documents it writes ([`Policy::planning`]).
    pub planning: Option<&'a str>,
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
    let uncollected = strings(&digest["uncollected"]);
    if !uncollected.is_empty() {
        out.push_str("\n## Tests that never ran\n\n");
        for l in uncollected {
            out.push_str(&format!("- {l}\n"));
        }
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

/// A person's decisions of the journaled input at `key`: none when absent,
/// an error when present and unreadable — never read as "no decisions",
/// which would drop rejections.
fn decisions_of<T: serde::de::DeserializeOwned>(
    settings: &Value,
    key: &str,
) -> Result<Vec<T>, SkillError> {
    match &settings[key] {
        Value::Null => Ok(Vec::new()),
        v => serde_json::from_value(v.clone())
            .map_err(|e| SkillError::Input(format!("settings.{key}: {e}"))),
    }
}

/// The policy an attempt's journaled settings decide by: the checks, the
/// allow lists, the frozen tests, the person's decisions, the MCP tools,
/// the network rules, in place and autopilot. An external agent's attempt
/// is decided by the same one.
pub(crate) fn policy_of(
    settings: &Value,
    rules: &'static crate::format::Rules,
) -> Result<Policy, SkillError> {
    let frozen: Vec<String> = settings["frozen"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let approved: Vec<Object> = decisions_of(settings, "approved")?;
    let rejected: Vec<crate::tools::policy::Rejection> = decisions_of(settings, "rejected")?;
    let mcp: Vec<crate::mcp::McpTool> =
        serde_json::from_value(settings["mcp"].clone()).unwrap_or_default();
    let policy = Policy::new(&argvs(&settings["checks"]), &argvs(&settings["allow"]))
        .with_prefixes(argvs(&settings["allow_prefix"]))
        .with_network_prefixes(argvs(&settings["network_allow_prefix"]))
        .rules(rules)
        .with_frozen(frozen)
        .with_decisions(approved, rejected)
        .with_mcp(mcp.iter().map(|t| (t.name.clone(), t.decision.clone())))
        .with_network(argvs(&settings["network_allow"]))
        .with_hidden(
            serde_json::from_value::<Vec<String>>(settings["hidden"].clone()).unwrap_or_default(),
        );
    let policy = if settings["in_place"] == true {
        policy.in_place()
    } else {
        policy
    };
    let policy = if settings["autopilot"] == true {
        policy.autopilot()
    } else {
        policy
    };
    Ok(match settings["planning"].as_str() {
        Some(dir) => policy.planning(dir),
        None => policy,
    })
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
        let rejected: Vec<crate::tools::policy::Rejection> = decisions_of(&settings, "rejected")?;
        let told = rejected;
        let mcp: Vec<crate::mcp::McpTool> =
            serde_json::from_value(settings["mcp"].clone()).unwrap_or_default();
        let planning = settings["planning"].as_str().map(str::to_owned);
        let policy = policy_of(&settings, self.format.rules())?;
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
                let mut p = layout(
                    &input["bundle"],
                    ttl,
                    style,
                    &self::policy(self.format.wording()),
                    self.format,
                );
                let given: Vec<crate::instructions::Instruction> =
                    serde_json::from_value(input["instructions"].clone()).unwrap_or_default();
                if let Some(text) = crate::instructions::section(&given) {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                if let Some(text) = input["test_layout"].as_str().filter(|t| !t.is_empty()) {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                if !input["digest"].is_null() {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content
                            .push(json!({ "type": kind, "text": digest_text(&input["digest"]) }));
                    }
                }
                // A planning task: where its documents go.
                if let Some(dir) = &planning {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    let text = format!(
                        "# A planning task\n\nThis task writes design documents, not code: write them under `{dir}/` (the plan, contracts, the data model, research and the like are yours to write there). The requirements, the task list and the person's review stay protected, as does everything else of the spec; change no source code. speccoder runs the checks afterwards.\n"
                    );
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                // The checks as configured: the commands that run without asking.
                let checks: Vec<String> = argvs(&input["settings"]["checks"])
                    .iter()
                    .map(|argv| format!("- `{}`", json!(argv)))
                    .collect();
                if !checks.is_empty() {
                    let (key, kind) = match style {
                        PromptStyle::OpenAi => ("input", "input_text"),
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    let text = format!(
                        "# The checks\n\nRun them with run_command as these argvs, word for word; they need no approval:\n\n{}\n",
                        checks.join("\n")
                    );
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
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
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
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
                        PromptStyle::Anthropic | PromptStyle::Chat | PromptStyle::Parts { .. } => {
                            ("messages", "text")
                        }
                    };
                    let lines: Vec<String> = told
                        .iter()
                        .map(|r| format!("- {}: {}", r.object.describe(), r.reason))
                        .collect();
                    let text = format!(
                        "# Rejected by a person\n\nThese calls were rejected for this task; find another way or call report_blocked:\n\n{}\n",
                        lines.join("\n")
                    );
                    if let Some(content) = p[key][0]["content"].as_array_mut() {
                        content.push(json!({ "type": kind, "text": text }));
                    }
                }
                if let PromptStyle::Parts { caches } = style {
                    super::model_config::to_parts(&mut p, caches);
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
        let ask = Asker::from_settings(
            &settings,
            task_name.clone(),
            input.peek()["bundle"]["task"]["title"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            PresentContext {
                root: settings["root"].as_str().map(std::path::PathBuf::from),
                ..self.present.clone()
            },
        );
        let remembered: Vec<Object> = decisions_of(&settings, "run_approved")?;
        let once: Vec<Object> = decisions_of(&settings, "approved_once")?;
        // Untrusted text the prompt itself carries.
        let mut parts = Vec::new();
        let given: Vec<crate::instructions::Instruction> =
            serde_json::from_value(input.peek()["instructions"].clone()).unwrap_or_default();
        if let Some(text) = crate::instructions::section(&given) {
            parts.push(crate::approvals::derived::Part {
                from: provenance::WORKTREE.to_owned(),
                turn: 0,
                text,
            });
        }
        if !input.peek()["digest"].is_null() {
            parts.push(crate::approvals::derived::Part {
                from: provenance::VERIFIER.to_owned(),
                turn: 0,
                text: crate::approvals::derived::text_of(&input.peek()["digest"]),
            });
        }
        let number = |k: &str, d: u32| {
            settings[k]
                .as_u64()
                .map_or(d, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        tool_loop(
            cx,
            &prompt,
            LoopSpec {
                tools,
                policy,
                max_turns,
                max_tool_calls,
                reasoning: settings["reasoning"] == true,
                schema: report_schema(),
                routes,
                root: settings["root"].as_str().map(str::to_owned),
                frozen_additions: settings["frozen_additions"] == true,
                ask,
                run_approved: remembered,
                once,
                run: settings["run_id"].as_str().map(str::to_owned),
                asking: Asking {
                    asked: number("asked", 0),
                    budget: number("budget", DEFAULT_BUDGET),
                    prompts: settings["prompts"] == true,
                },
                parts,
                mcp_digests: mcp
                    .iter()
                    .map(|t| (t.name.clone(), t.digest.clone()))
                    .collect(),
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
    emit_with_tail(observer, turn, entry, Vec::new());
}

/// [`emit`] with the last lines of a failed command's output (live only,
/// never journaled).
fn emit_with_tail(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    turn: u32,
    entry: &Value,
    tail: Vec<String>,
) {
    if let Some((task, p)) = observer {
        let tool = entry["tool"].as_str().unwrap_or_default();
        p.event(&crate::progress::Event::ToolCall {
            task: task.clone(),
            turn,
            tool: tool.to_owned(),
            target: crate::progress::target(tool, &entry["arguments"]),
            decision: entry["decision"].as_str().unwrap_or_default().to_owned(),
            ok: entry["ok"].as_bool(),
            tail,
        });
    }
}

/// A person is needed for `asked`: the run tells whoever watches.
fn needs_person(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    asked: &agentplane::model::ToolCall,
    object: Option<&Object>,
) {
    if let Some((task, p)) = observer {
        let (tool, arguments) = (asked.name.as_str(), &asked.arguments);
        let target = crate::progress::target(tool, arguments);
        let what: String = format!("{tool} {target}")
            .chars()
            .filter(|c| !c.is_control())
            .take(60)
            .collect();
        p.event(&crate::progress::Event::ApprovalRequested {
            task: task.clone(),
            call: asked.id.clone(),
            tool: tool.to_owned(),
            what,
            class: object
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.map(Object::short).unwrap_or_default(),
        });
    }
}

/// A person decided `asked`: the run tells whoever watches.
fn person_decided(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    asked: &agentplane::model::ToolCall,
    decision: &str,
    scope: Option<&str>,
    who: Option<&Who>,
    object: Option<&Object>,
) {
    if let Some((task, p)) = observer {
        p.event(&crate::progress::Event::ApprovalDecided {
            task: task.clone(),
            call: asked.id.clone(),
            decision: decision.to_owned(),
            scope: scope.map(str::to_owned),
            by: who.map_or_else(|| "nobody (expired)".to_owned(), |w| w.name.clone()),
            how: who.map_or_else(|| "expired".to_owned(), |w| w.basis.clone()),
            class: object
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.map(Object::short).unwrap_or_default(),
        });
    }
}

/// What a call needing a person is asked by.
struct Question<'a> {
    asked: &'a agentplane::model::ToolCall,
    reason: &'a str,
    turn: u32,
    said: &'a str,
    asker: Option<&'a Asker>,
    policy: &'a Policy,
    object: Option<&'a Object>,
    remembered: &'a [(Object, &'static str)],
    parts: &'a [crate::approvals::derived::Part],
    run: Option<&'a str>,
}

/// The journaled question for a call needing a person: its object, what
/// changed since an approval of the same name, where its arguments were
/// copied from, and the choices that leaves — all from journaled data.
fn question(q: &Question<'_>) -> Request {
    let (tool, arguments) = (q.asked.name.as_str(), &q.asked.arguments);
    let kind = crate::approvals::kind_of(tool, arguments);
    let risk = crate::approvals::risk(tool, q.reason);
    // Always Allow commits an exact entry: judged without the network.
    let always_risk = if kind == "network" {
        let mut bare = arguments.clone();
        if let Some(o) = bare.as_object_mut() {
            o.remove("network");
        }
        match q.policy.base(tool, &bare) {
            Decision::Allow => "low",
            Decision::Approval(r) => crate::approvals::risk(tool, &r),
            Decision::Deny(_) => "high",
        }
    } else {
        risk
    };
    let (sources, more) = crate::approvals::derived::sources(tool, arguments, q.parts);
    let changed = q.object.and_then(|o| {
        q.remembered
            .iter()
            .map(|(r, _)| r)
            .chain(q.policy.approved())
            .find_map(|earlier| o.changed_from(earlier))
    });
    Request {
        task: q.asker.map(|a| a.task.clone()).unwrap_or_default(),
        task_title: q.asker.map(|a| a.task_title.clone()).unwrap_or_default(),
        turn: q.turn,
        tool: tool.to_owned(),
        arguments: arguments.clone(),
        kind: kind.to_owned(),
        reason: q.reason.to_owned(),
        object: q.object.cloned(),
        changed,
        risk: risk.to_owned(),
        claim: crate::approvals::claim(q.said),
        options: crate::approvals::options(
            risk,
            q.object.is_some(),
            !sources.is_empty(),
            kind,
            always_risk,
        ),
        sources,
        more,
        run: q.run.map(str::to_owned),
        asked: 0,
        budget: 0,
        batched: false,
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
    /// Ask each turn for the model's reasoning summary.
    pub reasoning: bool,
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
    /// Objects a person approved for this run: allowed without asking.
    pub run_approved: Vec<Object>,
    /// Objects a person approved for one call each: the first call of
    /// each is allowed without asking, later ones ask again.
    pub once: Vec<Object>,
    /// Live progress: the task ID and where to report.
    pub observer: Option<(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    /// The run the attempt belongs to (`run-<secs>-<hex>`).
    pub run: Option<String>,
    /// How a run asks: its count so far, its budget and whether a person is
    /// prompted at all.
    pub asking: Asking,
    /// Untrusted content the prompt carries (inlined worktree text, the
    /// verifier's digest): what derived arguments are found in.
    pub parts: Vec<crate::approvals::derived::Part>,
    /// MCP tools' digests by model-facing name: their objects.
    pub mcp_digests: std::collections::BTreeMap<String, String>,
}

/// A run's asking budget as an attempt sees it (all journaled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asking {
    /// Prompts shown to a person in the run so far.
    pub asked: u32,
    /// `[approvals] budget`: at most this many per run.
    pub budget: u32,
    /// A person is prompted inline; `false` with `--no-prompt` (calls wait
    /// as ordinary approvals, nothing is counted).
    pub prompts: bool,
}

impl Default for Asking {
    fn default() -> Self {
        Self {
            asked: 0,
            budget: DEFAULT_BUDGET,
            prompts: false,
        }
    }
}

/// `[approvals] budget` unless configured.
pub use super::run_config::DEFAULT_BUDGET;

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
        reasoning,
        schema,
        routes,
        root,
        frozen_additions,
        finish,
        ask,
        run_approved,
        mut once,
        observer,
        run,
        asking,
        mut parts,
        mcp_digests,
    } = spec;
    // Objects allowed without asking: this run's, then what a person
    // answered "for the task" or "for this run" in this attempt.
    let mut remembered: Vec<(Object, &'static str)> =
        run_approved.into_iter().map(|g| (g, "run-grant")).collect();
    let mut prompted = asking.asked;
    let mut told_budget = false;
    let catalog = crate::tools::catalog();

    // What the conversation has read, with its labels: agentplane checks
    // every model call against the join.
    let mut transcript = Transcript::new(prompt);
    let mut conversation: Label = prompt.label().clone();
    let mut calls = 0u32;
    let mut denied_in_a_row = 0u32;
    // Whether this attempt registered the `approval` obligation yet.
    let mut obligation = false;
    let mut usage = agentplane::model::Usage::default();
    let mut log: Vec<Value> = Vec::new();

    let end = |status: &str,
               extra: Value,
               log: &[Value],
               usage: agentplane::model::Usage,
               conversation: &Label,
               prompted: u32| {
        let mut out = json!({
            "status": status,
            "tool_calls": log,
            "asked": prompted,
            "usage": {
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
                "cache_read_tokens": usage.cache_read_tokens,
                "cache_write_tokens": usage.cache_write_tokens,
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
                let call = call
                    .with_tools(tools_now)
                    .continuing(read_so_far)
                    .expecting(schema_now);
                if reasoning {
                    call.with_reasoning_summary()
                } else {
                    call
                }
            })
            .await?;
        conversation = transcript.label().join(completion.label());
        usage = usage.plus(completion.peek().usage);
        if let Some((task, p)) = &observer {
            p.event(&crate::progress::Event::Usage {
                task: task.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_read_tokens: usage.cache_read_tokens,
                cache_write_tokens: usage.cache_write_tokens,
                usd: None,
            });
        }

        // The model's summary of its reasoning, when asked and given: its
        // account of why, a claim like its words.
        if let (Some((task, p)), Some(reasoning)) =
            (&observer, completion.peek().reasoning.as_deref())
            && !reasoning.trim().is_empty()
        {
            let text: String = crate::approvals::present::escape(reasoning.trim())
                .chars()
                .take(4000)
                .collect();
            p.event(&crate::progress::Event::Reasoning {
                task: task.clone(),
                turn: turn + 1,
                text,
            });
        }
        if completion.peek().truncated {
            return Ok(Outcome::fail(
                "the model ran out of output budget mid-turn; nothing it was still writing was executed — raise [model] max_output_tokens",
            ));
        }
        let asked_calls = transcript.asked(&completion);
        // The model's own words beside its calls, live: its account, a
        // claim. A turn without calls is its final answer, reported as such;
        // its commentary, where the provider marks one, is still words.
        let said = super::said(completion.peek());
        if let (Some((task, p)), Some(said)) = (observer.as_ref(), said.as_deref()) {
            let text: String = crate::approvals::present::escape(said)
                .chars()
                .take(2000)
                .collect();
            p.event(&crate::progress::Event::Said {
                task: task.clone(),
                turn: turn + 1,
                text,
            });
        }
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
                prompted,
            );
        }

        for mut asked in asked_calls {
            relativise(&mut asked.arguments, root.as_deref());
            // Only speccoder sets the object a call is checked against.
            if let Some(o) = asked.arguments.as_object_mut() {
                o.remove(crate::tools::EXPECT);
            }
            calls += 1;
            if calls > max_tool_calls {
                return end(
                    "budget_exhausted",
                    json!({ "reason": format!("more than {max_tool_calls} tool calls (`[budget] max_tool_calls`)") }),
                    &log,
                    usage,
                    &conversation,
                    prompted,
                );
            }
            if asked.name == ASK_PERSON && tools.iter().any(|t| t.name == ASK_PERSON) {
                let question = asked.arguments["question"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                // A person's answer is their input, recorded with who gave it
                // — never speccoder's own trusted text.
                let decided = match &ask {
                    Some(asker) => Some(
                        wait_for_person(
                            cx,
                            asker,
                            &mut obligation,
                            &crate::approvals::tasks::question_task(&asker.task, &question),
                        )
                        .await?,
                    ),
                    None => None,
                };
                let read = decided.as_ref().map(crate::approvals::tasks::read);
                let by = read.as_ref().and_then(|r| r.by.clone());
                let entry = json!({
                    "tool": ASK_PERSON,
                    "arguments": asked.arguments,
                    "decision": "allow",
                    "answered": read.as_ref().is_some_and(|r| r.text.is_some()),
                    "operator": by.as_ref().map(Who::view),
                    "via": by.as_ref().map(|w| w.via.clone()),
                });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                let text = match (&read, &by) {
                    (Some(r), _) if r.text.is_some() => r.text.clone().unwrap_or_default(),
                    (Some(_), Some(w)) => format!("{} declined to answer.", w.name),
                    _ => "Nobody can answer now. Record the question in the artifact as [NEEDS CLARIFICATION: <question>] and continue.".to_owned(),
                };
                let label = match &by {
                    Some(_) => provenance::from(provenance::PERSON),
                    None => Label::trusted(),
                };
                transcript.answered(
                    asked,
                    &Tainted::with_label(json!({ "answer": text }), label),
                );
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
                    prompted,
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
            let mut decision = policy.base(&asked.name, &asked.arguments);
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
            // A person would be needed: the object the call reaches is
            // resolved by a journaled step (replay reads the same object
            // back), and only a decision on that very object applies.
            let mut object: Option<Object> = None;
            if matches!(decision, Decision::Approval(_)) {
                if asked.name.starts_with(crate::mcp::PREFIX) {
                    object = mcp_digests.get(&asked.name).map(|digest| Object::Tool {
                        name: asked.name.clone(),
                        digest: digest.clone(),
                    });
                } else {
                    let resolved = cx
                        .call_tool(
                            ToolId::new("fs", "resolve"),
                            Tainted::with_label(
                                json!({ "tool": asked.name, "arguments": asked.arguments }),
                                completion.label().clone(),
                            ),
                        )
                        .await?;
                    let out = resolved.peek();
                    if out["ok"] == true {
                        object = serde_json::from_value(out["object"].clone()).ok();
                    } else {
                        decision = Decision::Deny(
                            out["error"]
                                .as_str()
                                .unwrap_or("the call's object cannot be resolved")
                                .to_owned(),
                        );
                    }
                }
                if matches!(decision, Decision::Approval(_)) {
                    decision = policy.classify(&asked.name, &asked.arguments, object.as_ref());
                }
            }
            let mut person: Option<Value> = None;
            // Allowed by a person's grant for the task (of this run): the
            // person's decision, not a rule's.
            if decision == Decision::Allow && object.is_some() {
                person = Some(json!({
                    "approved_by": "person",
                    "scope": "task-grant",
                    "grant": { "scope": "task", "run": run },
                }));
            }
            let mut note: Option<String> = None;
            // An object a person approved for one call: this one.
            if matches!(decision, Decision::Approval(_))
                && let Some(at) = object
                    .as_ref()
                    .and_then(|o| once.iter().position(|r| r == o))
            {
                once.remove(at);
                person = Some(json!({
                    "approved_by": "person",
                    "scope": "once-grant",
                    "grant": { "scope": "once", "run": run },
                }));
                decision = Decision::Allow;
            }
            // An object a person already approved for this run or task.
            if matches!(decision, Decision::Approval(_))
                && let Some((_, scope)) = object
                    .as_ref()
                    .and_then(|o| remembered.iter().find(|(r, _)| r == o))
            {
                person = Some(json!({
                    "approved_by": "person",
                    "scope": scope,
                    "grant": { "scope": scope.trim_end_matches("-grant"), "run": run },
                }));
                decision = Decision::Allow;
            }
            // On autopilot nobody is asked: what would wait for a person is
            // refused for the run and listed for them afterwards.
            decision = policy.settle(decision);
            // Still a person's: the question, with everything it is decided by.
            let request = match &decision {
                Decision::Approval(reason) => Some(question(&Question {
                    asked: &asked,
                    reason,
                    turn: turn + 1,
                    said: said.as_deref().unwrap_or_default(),
                    asker: ask.as_ref(),
                    policy: &policy,
                    object: object.as_ref(),
                    remembered: &remembered,
                    parts: &parts,
                    run: run.as_deref(),
                })),
                _ => None,
            };
            // ask a person now, as a journaled effect, instead of stopping.
            if let (Some(mut request), Some(asker)) = (request.clone(), &ask) {
                // The run's asking budget: within it a person is prompted
                // (and counted, whatever they answer); past it the call
                // waits in the run's batch, nobody interrupted.
                if asking.prompts {
                    if prompted < asking.budget {
                        prompted += 1;
                        request.asked = prompted;
                    } else {
                        request.batched = true;
                    }
                    request.budget = asking.budget;
                }
                if request.batched {
                    // The batch item is created: the person is notified
                    // once; calls joining it notify no one.
                    if !told_budget {
                        needs_person(observer.as_ref(), &asked, object.as_ref());
                    }
                    if !told_budget && let Some((_, p)) = &observer {
                        p.event(&crate::progress::Event::Notice {
                            text: format!(
                                "approval budget ({}) reached: calls wait in Needs You (speccoder approvals)",
                                asking.budget
                            ),
                        });
                    }
                    told_budget = true;
                } else {
                    needs_person(observer.as_ref(), &asked, object.as_ref());
                }
                let decided = wait_for_person(
                    cx,
                    asker,
                    &mut obligation,
                    &crate::approvals::tasks::approval_task(&request, &log),
                )
                .await?;
                let read = crate::approvals::tasks::read(&decided);
                // A batch decision may start the run's asking again.
                if read.resume_asking {
                    prompted = 0;
                }
                let answer: Answer = read.answer.clone();
                // Who decided, and how that is known, is journaled with it.
                let approver = serde_json::to_value(&answer.by).unwrap_or(Value::Null);
                let operator = read.by.as_ref().map(Who::view);
                person_decided(
                    observer.as_ref(),
                    &asked,
                    if answer.decision == Said::Approve {
                        "approve"
                    } else {
                        "reject"
                    },
                    (answer.decision == Said::Approve).then(|| answer.scope.name()),
                    read.by.as_ref().filter(|_| !read.expired),
                    object.as_ref(),
                );
                match answer.decision {
                    Said::Approve => {
                        let mut p = json!({
                            "approved_by": "person",
                            "scope": answer.scope.name(),
                            "note": answer.note,
                            "approver": approver,
                            "operator": operator,
                        });
                        if answer.always {
                            p["always"] = json!(true);
                        }
                        if let (Some(o), Scope::Task | Scope::Run) = (&object, answer.scope) {
                            let remembered_as = if answer.scope == Scope::Run {
                                "run-grant"
                            } else {
                                "task-grant"
                            };
                            p["grant"] = json!({ "scope": answer.scope.name(), "run": run });
                            remembered.push((o.clone(), remembered_as));
                        }
                        person = Some(p);
                        note = answer.note;
                        decision = Decision::Allow;
                    }
                    // "Later" is a task left undecided: the run stays
                    // suspended and never reaches here.
                    Said::Reject | Said::Stop => {
                        let why = answer
                            .reason
                            .unwrap_or_else(|| "no reason given".to_owned());
                        person = Some(if read.expired {
                            json!({ "expired": true, "rejection": why })
                        } else {
                            json!({ "rejected_by": "person", "approver": approver, "operator": operator, "rejection": why })
                        });
                        decision = Decision::Deny(format!("a person rejected this: {why}"));
                    }
                }
            }
            // Who decided: a person (now, or by a grant they gave), or a rule
            // — named, so a rule's decision is never read as a person's.
            let rule = person
                .is_none()
                .then(|| policy.rule(&asked.name, &asked.arguments, object.as_ref()));
            let with_person = |mut entry: Value| {
                if let Some(o) = entry.as_object_mut() {
                    // The object decided on, whoever decided.
                    if let Some(obj) = &object {
                        o.insert("object".to_owned(), json!(obj));
                    }
                    match (person.as_ref().and_then(Value::as_object), &rule) {
                        (Some(p), _) => {
                            // An expiry is nobody's decision.
                            let by = if p.contains_key("expired") {
                                "expiry"
                            } else {
                                "person"
                            };
                            o.insert("by".to_owned(), json!(by));
                            o.extend(p.clone());
                        }
                        (None, Some(r)) => {
                            o.insert("by".to_owned(), json!("rule"));
                            o.insert("rule".to_owned(), json!(r));
                        }
                        (None, None) => {}
                    }
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
                            prompted,
                        );
                    }
                    continue;
                }
                Decision::Approval(reason) => {
                    let mut entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "approval", "reason": reason });
                    if let Some(obj) = &object {
                        entry["object"] = json!(obj);
                    }
                    emit(observer.as_ref(), turn + 1, &entry);
                    needs_person(observer.as_ref(), &asked, object.as_ref());
                    log.push(entry);
                    return end(
                        "needs_approval",
                        json!({ "pending": {
                            "tool": asked.name,
                            "arguments": asked.arguments,
                            "reason": reason,
                            "object": object,
                            "run": run,
                            "request": request,
                        } }),
                        &log,
                        usage,
                        &conversation,
                        prompted,
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
                    // Decided on an object: the tool checks it again at
                    // use, and refuses when the call now reaches another.
                    if let (Some(obj), Some(o)) = (&object, arguments.as_object_mut())
                        && matches!(
                            asked.name.as_str(),
                            "write_file" | "edit_file" | "run_command"
                        )
                    {
                        o.insert(crate::tools::EXPECT.to_owned(), json!(obj));
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
                    if let Some(changed) = output["object_changed"].as_object() {
                        entry["decision"] = json!("refused_changed");
                        entry["expected"] = changed.get("expected").cloned().unwrap_or(Value::Null);
                        entry["found"] = changed.get("found").cloned().unwrap_or(Value::Null);
                    }
                    // What the tool returned is content the attempt read:
                    // arguments copied from it later are named as such.
                    if let Some(part) = crate::approvals::derived::part_of(
                        &asked.name,
                        &asked.arguments,
                        &output,
                        turn + 1,
                        &format!("tool://{}/{}", target.server, target.tool),
                        |p: &str| {
                            policy.protected().write_class(p)
                                == Some(crate::tools::paths::Class::Intent)
                        },
                    ) {
                        parts.push(part);
                    }
                    if additions_only {
                        entry["frozen_additions"] = json!(true);
                    }
                    let tail = if asked.name == "run_command" && !ok {
                        crate::progress::tail(&format!(
                            "{}\n{}",
                            output["stdout"].as_str().unwrap_or_default(),
                            output["stderr"].as_str().unwrap_or_default()
                        ))
                    } else {
                        Vec::new()
                    };
                    emit_with_tail(observer.as_ref(), turn + 1, &entry, tail);
                    log.push(entry);
                    // A command that changed a judge input ends the attempt
                    // before the model sees another turn.
                    if let Some(drift) = output["integrity"].as_array().filter(|d| !d.is_empty()) {
                        let paths: Vec<&str> =
                            drift.iter().filter_map(|d| d["path"].as_str()).collect();
                        return end(
                            "integrity",
                            json!({
                                "reason": format!("a command changed what judges the task: {}", paths.join(", ")),
                                "integrity": drift,
                            }),
                            &log,
                            usage,
                            &conversation,
                            prompted,
                        );
                    }
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
                        // A person's note rides with the result it approved.
                        _ => match &note {
                            Some(n) => {
                                let n = format!("Note from the person who approved this call: {n}");
                                transcript.answered(
                                    asked,
                                    &result.map(|mut v| {
                                        v["note_from_person"] = Value::String(n);
                                        v
                                    }),
                                );
                            }
                            None => transcript.answered(asked, &result),
                        },
                    }
                }
            }
        }
    }
    end(
        "budget_exhausted",
        json!({ "reason": format!("more than {max_turns} model turns (`[budget] max_turns`)") }),
        &log,
        usage,
        &conversation,
        prompted,
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
