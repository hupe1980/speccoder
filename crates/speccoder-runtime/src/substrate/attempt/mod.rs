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
task — never work another task of the list is for — and run the project's checks with run_command to see whether it works. Stay inside \
the task's scope: the task's paths are the plan's guess — where they differ from the code, \
use the module that holds it; a change elsewhere that the task or its acceptance tests need \
(an entry point they run, a module they import) is in scope and shown to the reviewer, never \
a reason to stop: an entry point that bypasses the task's module is rewired to it. You \
cannot edit the spec ({intent}), the checks' configuration or .git; such calls are refused. Some commands need a person's approval and will end the \
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
this task's own work is left undone — a check you could not run, a test a later task makes \
pass, or a finding in a frozen test file is no reason.",
        product = w.product,
        intent = w.intent_paths,
    )
}

/// The final report's schema (the model's claim).
#[must_use]
pub fn report_schema() -> Value {
    super::answers::schema::<super::answers::AttemptReport>()
}

fn report_blocked() -> ToolDeclaration {
    ToolDeclaration::new(
        REPORT_BLOCKED,
        "Stop the attempt because the task cannot be done as written; a person will look at the \
reason. `kind`, with the `evidence` speccoder checks: `refused` — a call the task needs was \
refused (the call); `spec_conflict` — the spec contradicts the task (its exact words); \
`wrong_test` — a frozen test asserts the wrong thing (its path); `needs_information` — only a \
person knows (the question). Scope is never a kind: when the tests run code that bypasses the \
task's module (an entry point, a caller), wiring that code to it is part of the task.",
        super::answers::schema::<super::answers::Blocked>(),
    )
}

mod loop_spec;
mod person;
#[cfg(test)]
mod tests;
mod tool_loop;

pub use loop_spec::*;
pub use person::*;
pub(crate) use tool_loop::*;

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

/// A turn's progress, said only where no earlier pass said it: dropped
/// while `replaying` (agentplane's `StepCtx::replaying`, read after each
/// journaled step) holds.
#[derive(Debug)]
struct Quiet {
    replaying: std::sync::Arc<std::sync::atomic::AtomicBool>,
    next: std::sync::Arc<dyn crate::progress::Progress>,
}

impl crate::progress::Progress for Quiet {
    fn event(&self, event: &crate::progress::Event) {
        if !self.replaying.load(std::sync::atomic::Ordering::SeqCst) {
            self.next.event(event);
        }
    }
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
    let contained = settings.contained;
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
        ("left", trusted(json!(settings.left))),
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
    // And only a run whose sandbox confines what it runs.
    if let Some(m) = contained {
        settings.push(("contained", trusted(json!(m))));
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
    /// The enforced sandbox's mechanism when contained calls run without
    /// asking ([`Policy::contained`]).
    pub contained: Option<&'a str>,
    /// What is left of the run's cap for this attempt.
    pub left: Option<Left>,
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
    let policy = match settings["contained"].as_str() {
        Some(m) => policy.contained(m),
        None => policy,
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
        let bundle = &input.peek()["bundle"];
        let stop = StopGate {
            spec: ["story", "plan", "constitution"]
                .iter()
                .filter_map(|k| bundle[*k].as_str())
                .chain(bundle["task"]["title"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            frozen: settings["frozen"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            rejected: !told.is_empty(),
        };
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
                schema: Some(report_schema()),
                max_tokens: None,
                left: serde_json::from_value(settings["left"].clone()).ok(),
                reads: None,
                halt: None,
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
                stop,
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
