//! Ask: a read-only conversation beside every view (L0 of the ladder).
//!
//! A conversation is one agentplane run on a plane of its own
//! (`<project state>/ask`): each message is delivered to it, it answers
//! with reading tools only and waits again. What a turn showed — reads,
//! checked citations, a proposal — is kept beside the journal as the
//! conversation's thread, for the panel and `speccoder ask` to read back.
//! Nothing here writes to the project; a proposal is performed only when a
//! person confirms it, through the ordinary actions.

pub mod chips;
pub mod proposal;
pub mod skill;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agentplane::model::ModelProvider;
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::ask::{AskConversations, AskReport, AskThread, AskUsage, Conversation, Read};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::progress::{Event, Progress};
use crate::substrate::{self, model_config::ModelConfig};

pub use chips::ChipRef;

/// The tool that suggests a change.
pub const PROPOSE: &str = "propose";

/// The longest question.
pub const MAX_QUESTION: usize = 8_000;

/// A turn whose input alone is this large leaves no room for another: the
/// conversation is full (Ask never summarises).
const FULL_AT: u64 = 150_000;

/// `[ask]` in `speccoder.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// The model; `[model]`'s when absent.
    pub model: Option<String>,
    /// Tokens one turn may use (input and output, cache reads apart).
    #[serde(default = "turn_default")]
    pub max_turn_tokens: u64,
    /// Tokens Ask may use in a UTC day.
    #[serde(default = "day_default")]
    pub daily_tokens: u64,
}

const fn turn_default() -> u64 {
    100_000
}

const fn day_default() -> u64 {
    1_000_000
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: None,
            max_turn_tokens: turn_default(),
            daily_tokens: day_default(),
        }
    }
}

/// `[ask]` from the text of `speccoder.toml`.
///
/// # Errors
///
/// The section is malformed.
pub fn settings(config: &str) -> Result<Settings, String> {
    let table: toml::Table = config.parse().map_err(|e: toml::de::Error| e.to_string())?;
    match table.get("ask") {
        None => Ok(Settings::default()),
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e: toml::de::Error| format!("[ask]: {e}")),
    }
}

/// The progress tag of conversation `id`'s turn `n`.
#[must_use]
pub fn turn_tag(id: &str, n: u32) -> String {
    format!("ask:{id}#{n}")
}

/// What a tool call read, as the answer may cite it.
#[must_use]
pub fn read_of(id: &str, tool: &str, arguments: &Value, output: &Value) -> Read {
    let digest = crate::integrity::sha256_hex(output.to_string().as_bytes());
    let line = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    let (path, start, end) = match tool {
        "read_file" => (
            output["path"].as_str().map(str::to_owned),
            line(&output["start_line"]),
            line(&output["end_line"]),
        ),
        "git_show" => (arguments["path"].as_str().map(str::to_owned), None, None),
        _ => (None, None, None),
    };
    let a = |k: &str| arguments[k].as_str().unwrap_or_default().to_owned();
    let target = match tool {
        "task_evidence" => format!("evidence of {}", a("task")),
        "report" => match a("feature") {
            f if f.is_empty() => "the report".to_owned(),
            f => format!("the report of {f}"),
        },
        "inbox" => "Needs You".to_owned(),
        "explain" => format!("code {}", a("code")),
        "journal_account" => format!("account of run {}", a("run")),
        "git_log" => match a("path") {
            p if p.is_empty() => "git history".to_owned(),
            p => format!("git history of {p}"),
        },
        "git_show" => format!("{} at {}", a("path"), a("rev")).trim().to_owned(),
        "git_status" => "the working tree".to_owned(),
        "git_diff" => format!("diff {}", a("path")).trim().to_owned(),
        "list_files" => match a("path") {
            p if p.is_empty() || p == "." => "the files".to_owned(),
            p => format!("files in {p}"),
        },
        _ => crate::progress::target(tool, arguments),
    };
    Read {
        id: id.to_owned(),
        tool: tool.to_owned(),
        target,
        path,
        start,
        end,
        digest,
    }
}

/// A person's message for a conversation.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Request {
    /// The conversation to continue; a new one when absent.
    pub conversation: Option<String>,
    /// The person's words.
    pub text: String,
    /// Context from the screen.
    #[serde(default)]
    pub chips: Vec<ChipRef>,
    /// The feature in view.
    pub feature: Option<String>,
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new("ask", status, message).with_code(code)
}

/// Where the project's Ask plane and threads live.
fn plane_dir(path: &Path, state: &Path) -> Result<(PathBuf, PathBuf), Box<CommandResult>> {
    let Some(root) = crate::format::at(path).detect(path) else {
        return Err(Box::new(refuse(
            CommandStatus::NotAProject,
            "NO-PROJECT",
            "not a spec project here",
        )));
    };
    let dir = crate::worktree::project_state_dir(state, &root).join("ask");
    Ok((root, dir))
}

fn state_or(state: Option<&Path>) -> Result<PathBuf, Box<CommandResult>> {
    state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
        .ok_or_else(|| {
            Box::new(refuse(
                CommandStatus::ConfigError,
                "NO-STATE-DIR",
                "no state directory (set SPECCODER_STATE_DIR or HOME)",
            ))
        })
}

fn thread_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("threads").join(format!("{id}.json"))
}

fn load(dir: &Path, id: &str) -> Option<AskThread> {
    serde_json::from_str(&std::fs::read_to_string(thread_path(dir, id)).ok()?).ok()
}

fn save(dir: &Path, thread: &AskThread) -> Result<(), String> {
    let path = thread_path(dir, &thread.conversation.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(thread).map_err(|e| e.to_string())?,
    )
    .and_then(|()| std::fs::rename(&tmp, &path))
    .map_err(|e| e.to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Today's UTC day number and what Ask used in it.
fn used_today(dir: &Path) -> (u64, u64) {
    let day = now() / 86_400;
    let used = std::fs::read_to_string(dir.join("usage.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v["day"].as_u64() == Some(day))
        .and_then(|v| v["tokens"].as_u64())
        .unwrap_or(0);
    (day, used)
}

fn add_today(dir: &Path, tokens: u64) {
    let (day, used) = used_today(dir);
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(
        dir.join("usage.json"),
        json!({ "day": day, "tokens": used + tokens }).to_string(),
    );
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "c-{:x}{:04x}",
        nanos / 1_000_000,
        std::process::id() & 0xffff
    )
}

/// Answers `request` in the project at `path` with the model in its
/// `speccoder.toml` (`[ask] model`, else `[model]`).
#[must_use]
pub fn turn(
    path: &Path,
    request: &Request,
    state: Option<&Path>,
    sink: Option<Arc<dyn Progress>>,
) -> CommandResult {
    let state = match state_or(state) {
        Ok(s) => s,
        Err(e) => return *e,
    };
    let config = crate::verify::find_config(path)
        .and_then(|root| std::fs::read_to_string(root.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let ask = match settings(&config) {
        Ok(a) => a,
        Err(e) => {
            return refuse(
                CommandStatus::ConfigError,
                "CONFIG",
                &format!("speccoder.toml: {e}"),
            );
        }
    };
    let model = match substrate::model_config::parse(&config) {
        Ok(Some(mut m)) => {
            if let Some(name) = &ask.model {
                m.model.clone_from(name);
            }
            m
        }
        Ok(None) => {
            return refuse(
                CommandStatus::ConfigError,
                "ASK-NO-MODEL",
                "Ask needs a model ([model] in speccoder.toml); \"Make this a…\" works without one",
            );
        }
        Err(e) => {
            return refuse(
                CommandStatus::ConfigError,
                "CONFIG",
                &format!("speccoder.toml: {e}"),
            );
        }
    };
    match substrate::model_config::open_driver(&model) {
        Ok((provider, _)) => turn_with(path, request, &state, &ask, &model, provider, sink),
        Err(e) => substrate::model_config::refused("ask", &e),
    }
}

/// Forwards a turn's events: only this turn's (a resumed run replays the
/// earlier ones), steps as `ask.step`, and keeps the turn's end.
#[derive(Debug)]
struct Collector {
    conversation: String,
    turn: u32,
    tag: String,
    sink: Option<Arc<dyn Progress>>,
    ended: Mutex<Option<Value>>,
}

impl Progress for Collector {
    fn event(&self, event: &Event) {
        match event {
            Event::ToolCall {
                task,
                tool,
                target,
                decision,
                ok,
                ..
            } if *task == self.tag => {
                if let Some(sink) = &self.sink {
                    sink.event(&Event::AskStep {
                        conversation: self.conversation.clone(),
                        turn: self.turn,
                        tool: tool.clone(),
                        target: target.clone(),
                        read: None,
                        refused: decision != "allow" && decision != "proposed"
                            || *ok == Some(false),
                    });
                }
            }
            Event::AskTurnEnded { tag, output } if *tag == self.tag => {
                if let Ok(mut e) = self.ended.lock() {
                    *e = Some((**output).clone());
                }
            }
            _ => {}
        }
    }
}

/// The answer's words as the model writes them.
#[derive(Debug)]
struct Words {
    conversation: String,
    turn: u32,
    sink: Arc<dyn Progress>,
}

impl agentplane::runtime::RunStreamObserver for Words {
    fn event(
        &self,
        _run: agentplane::core::RunId,
        event: agentplane::core::Tainted<agentplane::model::ModelStreamEvent>,
    ) {
        if let agentplane::model::ModelStreamEvent::TextDelta(delta) = event.peek() {
            self.sink.event(&Event::AskText {
                conversation: self.conversation.clone(),
                turn: self.turn,
                delta: delta.clone(),
            });
        }
    }
}

/// [`turn`] with `provider` answering.
#[must_use]
pub fn turn_with(
    path: &Path,
    request: &Request,
    state: &Path,
    ask: &Settings,
    model: &ModelConfig,
    provider: Arc<dyn ModelProvider>,
    sink: Option<Arc<dyn Progress>>,
) -> CommandResult {
    let text = request.text.trim();
    if !text.chars().any(char::is_alphanumeric) {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            "ask a question first — any words",
        );
    }
    if text.chars().count() > MAX_QUESTION {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            &format!("a question is at most {MAX_QUESTION} characters"),
        );
    }
    if request.chips.len() > chips::MAX {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            &format!("at most {} chips per question", chips::MAX),
        );
    }
    let (root, dir) = match plane_dir(path, state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    let fresh = request.conversation.is_none();
    let mut thread = match &request.conversation {
        Some(id) => match load(&dir, id) {
            Some(t) if t.conversation.open => t,
            Some(_) => {
                return refuse(
                    CommandStatus::UsageError,
                    "ASK-FULL",
                    "this conversation has ended; start a new one — its chips come along",
                );
            }
            None => {
                return refuse(
                    CommandStatus::NotFound,
                    "NOT-FOUND",
                    &format!("no conversation {id}"),
                );
            }
        },
        None => AskThread {
            conversation: Conversation {
                id: new_id(),
                feature: request.feature.clone(),
                title: text.chars().take(120).collect(),
                at: now(),
                turns: 0,
                open: true,
            },
            turns: Vec::new(),
        },
    };
    let id = thread.conversation.id.clone();
    let n = u32::try_from(thread.turns.len()).unwrap_or(u32::MAX - 1) + 1;
    let reads_from = 1 + thread.turns.iter().map(|t| t.reads.len()).sum::<usize>();
    let resolved: Result<Vec<_>, String> = request
        .chips
        .iter()
        .enumerate()
        .map(|(i, c)| chips::resolve(&root, &format!("c{}", i + 1), c))
        .collect();
    let resolved = match resolved {
        Ok(r) => r,
        Err(e) => {
            return refuse(
                CommandStatus::UsageError,
                "USAGE",
                &format!("a chip cannot be read: {e}"),
            );
        }
    };
    let mut report = AskReport {
        conversation: id.clone(),
        turn: n,
        question: text.to_owned(),
        chips: resolved.iter().map(|(c, _)| c.clone()).collect(),
        reads: Vec::new(),
        answer: String::new(),
        citations: Vec::new(),
        proposal: None,
        usage: AskUsage::default(),
        end: "answered".to_owned(),
        reason: None,
    };
    let (_, used) = used_today(&dir);
    let left = ask.daily_tokens.saturating_sub(used);
    if left == 0 {
        report.end = "budget".to_owned();
        report.reason = Some(format!(
            "Ask's tokens for today are spent ([ask] daily_tokens = {})",
            ask.daily_tokens
        ));
        return finish(&dir, thread, report, sink.as_ref());
    }
    let collector = Arc::new(Collector {
        conversation: id.clone(),
        turn: n,
        tag: turn_tag(&id, n),
        sink: sink.clone(),
        ended: Mutex::new(None),
    });
    let journal = crate::worktree::project_state_dir(state, &root);
    let workspace =
        match crate::tools::WorkspaceTools::new(&root, crate::tools::policy::Policy::new(&[], &[]))
        {
            Ok(t) => Arc::new(t),
            Err(e) => return refuse(CommandStatus::Error, "ERROR", &e.to_string()),
        };
    let wiring = substrate::ask::AskWiring {
        workspace,
        readers: Arc::new(tools::AskTools::new(&root, &journal)),
        progress: Some(Arc::clone(&collector) as Arc<dyn Progress>),
        stream: sink.clone().map(|s| {
            Arc::new(Words {
                conversation: id.clone(),
                turn: n,
                sink: s,
            }) as Arc<dyn agentplane::runtime::RunStreamObserver>
        }),
    };
    let format = crate::format::at(&root);
    let rt = match crate::pipeline::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return refuse(CommandStatus::Error, "ERROR", &e),
    };
    let payload = json!({
        "text": chips::message(text, &resolved),
        "turn": n,
        "limit": ask.max_turn_tokens.min(left),
        "reads": reads_from,
    });
    let waited = rt.block_on(async {
        let plane = substrate::Plane::open_ask(&dir, model, provider, wiring)?;
        if fresh {
            plane
                .ask_start(
                    &id,
                    agentplane::core::Tainted::trusted(json!({
                        "conversation": id,
                        "root": root.to_string_lossy(),
                        "format": format.kind().name(),
                        "feature": request.feature,
                    })),
                )
                .await?;
        }
        plane.ask_deliver(&id, &format!("{id}-{n}"), payload).await
    });
    match waited {
        Ok(true) => {}
        Ok(false) => {
            thread.conversation.open = false;
            let _ = save(&dir, &thread);
            return refuse(
                CommandStatus::UsageError,
                "ASK-FULL",
                "this conversation went idle and ended; start a new one — its chips come along",
            );
        }
        Err(e) if e.0.contains("another speccoder is working") => {
            return refuse(
                CommandStatus::UsageError,
                "USAGE",
                "another question is being answered in this project; ask when it is done",
            );
        }
        Err(e) => return refuse(CommandStatus::RunFailed, "RUN-FAILED", &e.0),
    }
    let output = collector
        .ended
        .lock()
        .ok()
        .and_then(|e| e.clone())
        .unwrap_or(Value::Null);
    let u = &output["usage"];
    let n_of = |k: &str| u[k].as_u64().unwrap_or(0);
    report.usage = AskUsage {
        input: n_of("input_tokens"),
        output: n_of("output_tokens"),
        cache_read: n_of("cache_read_tokens"),
    };
    let log = output["tool_calls"].as_array().cloned().unwrap_or_default();
    report.reads = log
        .iter()
        .filter_map(|e| serde_json::from_value::<Read>(e["read"].clone()).ok())
        .collect();
    if let Some(p) = log
        .iter()
        .find(|e| e["tool"] == PROPOSE && e["decision"] == "proposed")
    {
        report.proposal = Some(proposal::validate(
            &root,
            request.feature.as_deref(),
            &p["arguments"],
        ));
    }
    match output["status"].as_str() {
        Some("answered") => {
            report.answer = output["text"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned()
        }
        Some("budget") => {
            report.end = "budget".to_owned();
            report.reason = Some(format!(
                "{} ([ask] max_turn_tokens = {})",
                output["reason"]
                    .as_str()
                    .unwrap_or("the turn's tokens are spent"),
                ask.max_turn_tokens
            ));
        }
        _ => {
            report.end = "failed".to_owned();
            report.reason = Some(
                output["reason"]
                    .as_str()
                    .unwrap_or("the turn did not finish")
                    .to_owned(),
            );
        }
    }
    report.citations = speccoder_core::ask::check(&report.answer, &report.reads, &report.chips);
    add_today(&dir, report.usage.billed());
    if report.usage.input + report.usage.cache_read > FULL_AT {
        thread.conversation.open = false;
    }
    finish(&dir, thread, report, sink.as_ref())
}

fn finish(
    dir: &Path,
    mut thread: AskThread,
    report: AskReport,
    sink: Option<&Arc<dyn Progress>>,
) -> CommandResult {
    thread.conversation.turns += 1;
    thread.conversation.at = now();
    thread.turns.push(report.clone());
    if let Err(e) = save(dir, &thread) {
        return refuse(
            CommandStatus::Error,
            "ERROR",
            &format!("the conversation could not be saved: {e}"),
        );
    }
    if let Some(sink) = sink {
        sink.event(&Event::AskDone {
            report: Box::new(report.clone()),
        });
    }
    let sources = report
        .citations
        .iter()
        .filter(|c| c.state == "read")
        .count();
    let (status, code, message) = match report.end.as_str() {
        "answered" if !thread.conversation.open => (
            CommandStatus::Ok,
            None,
            format!(
                "answered with {sources} sources; the conversation is full — a new one keeps its chips"
            ),
        ),
        "answered" => (
            CommandStatus::Ok,
            None,
            format!("answered with {sources} sources"),
        ),
        "budget" => (
            CommandStatus::NeedsHuman,
            Some("ASK-BUDGET"),
            report.reason.clone().unwrap_or_default(),
        ),
        _ => (
            CommandStatus::RunFailed,
            Some("RUN-FAILED"),
            report.reason.clone().unwrap_or_default(),
        ),
    };
    let mut result = CommandResult::new("ask", status, &message);
    if let Some(code) = code {
        result = result.with_code(code);
    }
    result.with_data(CommandData::Ask(Box::new(report)))
}

/// The project's conversations, newest first; `feature` narrows them.
#[must_use]
pub fn conversations(path: &Path, state: Option<&Path>, feature: Option<&str>) -> CommandResult {
    let state = match state_or(state) {
        Ok(s) => s,
        Err(e) => return *e,
    };
    let (_, dir) = match plane_dir(path, &state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    let mut list: Vec<Conversation> = std::fs::read_dir(dir.join("threads"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let t: AskThread =
                serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some(t.conversation)
        })
        .filter(|c| feature.is_none() || c.feature.as_deref() == feature)
        .collect();
    list.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    let message = format!("{} conversations", list.len());
    CommandResult::new("ask", CommandStatus::Ok, &message).with_data(CommandData::AskConversations(
        Box::new(AskConversations {
            conversations: list,
        }),
    ))
}

/// One conversation with its turns.
#[must_use]
pub fn thread(path: &Path, state: Option<&Path>, id: &str) -> CommandResult {
    let state = match state_or(state) {
        Ok(s) => s,
        Err(e) => return *e,
    };
    let (_, dir) = match plane_dir(path, &state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    match load(&dir, id) {
        Some(t) => CommandResult::new("ask", CommandStatus::Ok, &t.conversation.title.clone())
            .with_data(CommandData::AskThread(Box::new(t))),
        None => refuse(
            CommandStatus::NotFound,
            "NOT-FOUND",
            &format!("no conversation {id}"),
        ),
    }
}
