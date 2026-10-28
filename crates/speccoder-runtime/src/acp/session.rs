//! One attempt as an ACP session: the agent started in the worktree,
//! `initialize`, `session/new`, its permission mode settled, one
//! `session/prompt`, and every request it makes in between answered by
//! the [`Governor`] — until the turn ends, the attempt's time is up
//! (`session/cancel`, then the process group is killed), or a call waits for
//! a person nobody can ask.
//!
//! Adapted from devplane's `acp::session` (same owner, MIT OR Apache-2.0):
//! the set-up, the mode request, the cancel and the sign-in reading; the
//! watched sessions, questions and resume are left out — an attempt is one
//! fresh session.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AuthCapabilities, CancelNotification, ClientCapabilities, CloseSessionRequest, ContentBlock,
    CreateTerminalRequest, CreateTerminalResponse, FileSystemCapabilities, InitializeRequest,
    KillTerminalRequest, KillTerminalResponse, NewSessionRequest, PromptRequest,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionModeRequest, StopReason, TerminalExitStatus, TerminalOutputRequest,
    TerminalOutputResponse, TextContent, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent as AgentRole, ConnectionTo};
use serde_json::{Value, json};
use tokio::sync::Notify;

use super::agent::Agent;
use super::calls::{ToolReport, ToolRequest};
use super::govern::{Governor, PermissionOption};
use super::modes::{self, Advertised, ModePlan, Outcome, Statement};
use super::terminal::Terminals;
use crate::progress::{Event, Progress};

/// How long a cancelled turn has to acknowledge before the process group is
/// killed anyway.
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// How long an agent that offers `session/close` has to answer one.
const CLOSE_GRACE: Duration = Duration::from_secs(2);

/// How long the agent has to answer the mode (or model) request before the
/// prompt goes anyway, reported as *did not answer*.
const MODE_GRACE: Duration = Duration::from_secs(10);

/// The one protocol version spoken.
const SPOKEN: u64 = 1;

/// Lines of the agent's stderr kept, for a failure's reason.
const STDERR_KEPT: usize = 5;

/// What an attempt is started with.
#[derive(Debug, Clone)]
pub struct Attempt {
    /// The agent.
    pub agent: Agent,
    /// Its command line, as it is started.
    pub argv: Vec<String>,
    /// The worktree.
    pub cwd: PathBuf,
    /// The task prompt.
    pub prompt: String,
    /// `[executor] model`.
    pub model: Option<String>,
    /// How long the attempt may take (a person's time deciding excluded).
    pub timeout: Duration,
    /// The runner terminal commands go through.
    pub runner: crate::sandbox::Runner,
    /// Where the agent's words and calls go (task id, events).
    pub progress: Option<(String, Arc<dyn Progress>)>,
}

/// A way the agent says it can be signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    /// Its id.
    pub id: String,
    /// The agent's name for it.
    pub name: String,
    /// The agent's words about it.
    pub description: Option<String>,
    /// The command a person runs in a terminal, where the agent names one.
    pub command: Option<String>,
}

/// How the attempt ended.
#[derive(Debug, Clone, Default)]
pub struct Finished {
    /// The agent's name, as it gave it.
    pub agent_name: Option<String>,
    /// The agent's final message (its words after its last call).
    pub message: String,
    /// `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`.
    pub stop_reason: Option<String>,
    /// Why it ended early.
    pub error: Option<String>,
    /// The time was up.
    pub timed_out: bool,
    /// The agent needs signing in; its own ways.
    pub sign_in: Option<Vec<SignIn>>,
    /// What its permission mode is, and why.
    pub mode: Option<Statement>,
    /// Lines worth saying on the run (the model, the mode).
    pub notices: Vec<String>,
    /// Tokens, as the agent reported them.
    pub usage: Option<Value>,
    /// The last lines the agent wrote to stderr.
    pub stderr: Vec<String>,
}

/// The agent's words, kept for the narration and the final message.
#[derive(Debug, Default)]
struct Words {
    /// Since the last call: shown as one `said`.
    pending: String,
    /// Reasoning since the last call.
    thinking: String,
    /// Everything said since the last call (the final message at the end).
    last: String,
    /// The calls, by id.
    calls: HashMap<String, ToolRequest>,
    /// Calls already recorded as finished.
    finished: std::collections::HashSet<String>,
    /// Model turns so far (a call ends one).
    turn: u32,
    usage_usd: Option<f64>,
}

struct Shared {
    governor: Arc<Governor>,
    words: Mutex<Words>,
    terminals: Terminals,
    runner: crate::sandbox::Runner,
    progress: Option<(String, Arc<dyn Progress>)>,
    deadline: Instant,
}

impl Shared {
    fn event(&self, e: Event) {
        if let Some((_, p)) = &self.progress {
            p.event(&e);
        }
    }

    /// The words since the last call, as one `said` (and the reasoning).
    fn flush(&self) {
        let Ok(mut w) = self.words.lock() else { return };
        let task = self
            .progress
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        let turn = w.turn + 1;
        let thinking = std::mem::take(&mut w.thinking);
        if !thinking.trim().is_empty() {
            self.event(Event::Reasoning {
                task: task.clone(),
                turn,
                text: crate::approvals::present::escape(thinking.trim())
                    .chars()
                    .take(4000)
                    .collect(),
            });
        }
        let said = std::mem::take(&mut w.pending);
        if !said.trim().is_empty() {
            self.event(Event::Said {
                task,
                turn,
                text: crate::approvals::present::escape(said.trim())
                    .chars()
                    .take(2000)
                    .collect(),
            });
        }
    }

    /// A report of a call: merged, and recorded once it finished.
    fn call(&self, report: &ToolReport) {
        self.flush();
        let finished = {
            let Ok(mut w) = self.words.lock() else { return };
            w.last.clear();
            let c = w
                .calls
                .entry(report.id.clone())
                .or_insert_with(|| ToolRequest {
                    id: report.id.clone(),
                    ..ToolRequest::default()
                });
            c.merge(report);
            let c = c.clone();
            let done = matches!(c.status.as_deref(), Some("completed" | "failed"));
            if done && w.finished.insert(c.id.clone()) {
                w.turn += 1;
                Some(c)
            } else {
                None
            }
        };
        if let Some(c) = finished {
            self.governor.reported(&c);
        }
    }

    fn update(&self, update: SessionUpdate) {
        match update {
            SessionUpdate::AgentMessageChunk(c) => {
                if let ContentBlock::Text(t) = &c.content {
                    self.governor.said(&t.text);
                    if let Ok(mut w) = self.words.lock() {
                        w.pending.push_str(&t.text);
                        w.last.push_str(&t.text);
                    }
                }
            }
            SessionUpdate::AgentThoughtChunk(c) => {
                if let (ContentBlock::Text(t), Ok(mut w)) = (&c.content, self.words.lock()) {
                    w.thinking.push_str(&t.text);
                }
            }
            SessionUpdate::ToolCall(t) => {
                // A first report's status and kind have schema defaults that
                // are not written on the wire: read from the typed call.
                let mut report = ToolReport::from_wire(
                    t.tool_call_id.to_string(),
                    &serde_json::to_value(&t).unwrap_or_default(),
                );
                report.status = wire(&t.status);
                report.kind = wire(&t.kind);
                self.call(&report);
            }
            SessionUpdate::ToolCallUpdate(t) => {
                let report = ToolReport::from_wire(
                    t.tool_call_id.to_string(),
                    &serde_json::to_value(&t.fields).unwrap_or_default(),
                );
                self.call(&report);
            }
            SessionUpdate::UsageUpdate(u) => {
                if let (Some(c), Ok(mut w)) = (u.cost.as_ref(), self.words.lock())
                    && c.currency.eq_ignore_ascii_case("usd")
                {
                    w.usage_usd = Some(c.amount);
                }
            }
            _ => {}
        }
    }
}

/// The wire name of a protocol enum, asked of serde.
fn wire<T: serde::Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        Value::String(s) => Some(s),
        _ => None,
    }
}

fn stop_reason_name(r: StopReason) -> &'static str {
    match r {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        _ => "unknown",
    }
}

/// The sign-in methods the agent advertised, in its own words.
fn sign_in_methods(methods: &Value, argv: &[String]) -> Vec<SignIn> {
    methods
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let s = |k: &str| m[k].as_str().map(str::to_owned);
            let legacy = &m["_meta"]["terminal-auth"];
            let words = |command: &str, args: &Value| {
                let mut line = vec![command.to_owned()];
                line.extend(
                    args.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|a| a.as_str().map(str::to_owned)),
                );
                line.join(" ")
            };
            let command = if m["type"] == "terminal" {
                Some(words(&argv.join(" "), &m["args"]))
            } else {
                legacy["command"].as_str().map(|c| {
                    let program = std::path::Path::new(c)
                        .file_name()
                        .map_or_else(|| c.to_owned(), |n| n.to_string_lossy().into_owned());
                    words(&program, &legacy["args"])
                })
            };
            SignIn {
                id: s("id").unwrap_or_default(),
                name: s("name").unwrap_or_default(),
                description: s("description"),
                command,
            }
        })
        .collect()
}

/// What the client declares: the file system and terminals, served
/// through speccoder; terminal sign-in, so an agent names its login.
fn declared() -> ClientCapabilities {
    ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(true))
        .terminal(true)
        .auth(AuthCapabilities::new().terminal(true))
}

/// The launch configuration, run from `dir`: the protocol crate starts the
/// agent in this process's directory, and an adapter resolves its own
/// configuration and `npx` packages from where it starts — so the command
/// is started through `sh`, which changes directory and `exec`s it.
fn in_directory(config: AcpAgentConfig, dir: &std::path::Path) -> AcpAgentConfig {
    if cfg!(not(unix)) {
        return config;
    }
    AcpAgentConfig::new("/bin/sh")
        .args(["-c", "cd \"$0\" && exec \"$@\""])
        .arg(dir.to_string_lossy().into_owned())
        .arg(config.command().to_string_lossy().into_owned())
        .args(config.arguments().iter().cloned())
        .envs(config.environment().clone())
}

/// Runs the attempt to its end.
#[must_use]
pub fn run(attempt: &Attempt, governor: Arc<Governor>) -> Finished {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            return Finished {
                error: Some(format!("the runtime could not start: {e}")),
                ..Finished::default()
            };
        }
    };
    rt.block_on(drive(attempt, governor))
}

async fn drive(attempt: &Attempt, governor: Arc<Governor>) -> Finished {
    let Some((program, args)) = attempt.argv.split_first() else {
        return Finished {
            error: Some("the agent's command is empty".to_owned()),
            ..Finished::default()
        };
    };
    let stderr: Arc<Mutex<VecDeque<String>>> = Arc::default();
    let mut config = AcpAgentConfig::new(program).args(args.iter().cloned());
    // An agent whose modes do not govern permissions is told to ask where
    // it starts — unless the person chose its mode.
    if attempt.agent.mode.is_none()
        && let Some((k, v)) = modes::launch_profile(&attempt.agent.id).and_then(|p| p.launch_env)
    {
        config = config.env(k, v);
    }
    let lines = Arc::clone(&stderr);
    let agent = AcpAgent::new(in_directory(config, &attempt.cwd)).with_debug(move |line, dir| {
        if dir == agent_client_protocol::LineDirection::Stderr
            && !line.trim().is_empty()
            && let Ok(mut l) = lines.lock()
        {
            l.push_back(
                crate::secrets::mask(line.trim())
                    .chars()
                    .take(500)
                    .collect(),
            );
            while l.len() > STDERR_KEPT {
                l.pop_front();
            }
        }
    });
    let shared = Arc::new(Shared {
        governor: Arc::clone(&governor),
        words: Mutex::new(Words::default()),
        terminals: Terminals::default(),
        runner: attempt.runner.clone(),
        progress: attempt.progress.clone(),
        deadline: Instant::now() + attempt.timeout,
    });
    let stop = Arc::new(Notify::new());
    let result: Arc<Mutex<Finished>> = Arc::default();

    let (s1, s2, s3, s4, s5, s6, s7, s8) = (
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
        Arc::clone(&shared),
    );
    let stop_for_perm = Arc::clone(&stop);
    let stop_for_fs = Arc::clone(&stop);
    let stop_for_term = Arc::clone(&stop);
    let result_for_run = Arc::clone(&result);
    let connected = agent_client_protocol::Client
        .builder()
        .name("speccoder")
        .on_receive_notification(
            async move |n: SessionNotification, _cx| {
                s1.update(n.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, cx| {
                let shared = Arc::clone(&s2);
                let stop = Arc::clone(&stop_for_perm);
                cx.spawn(async move {
                    let report = ToolReport::from_wire(
                        request.tool_call.tool_call_id.to_string(),
                        &serde_json::to_value(&request.tool_call.fields).unwrap_or_default(),
                    );
                    shared.flush();
                    let call = {
                        let Ok(mut w) = shared.words.lock() else {
                            return responder.respond(RequestPermissionResponse::new(
                                RequestPermissionOutcome::Cancelled,
                            ));
                        };
                        let c = w
                            .calls
                            .entry(report.id.clone())
                            .or_insert_with(|| ToolRequest {
                                id: report.id.clone(),
                                ..ToolRequest::default()
                            });
                        c.merge(&report);
                        c.clone()
                    };
                    let options: Vec<PermissionOption> = request
                        .options
                        .iter()
                        .map(|o| PermissionOption {
                            id: o.option_id.to_string(),
                            kind: wire(&o.kind).unwrap_or_default(),
                        })
                        .collect();
                    let governor = Arc::clone(&shared.governor);
                    let answer =
                        tokio::task::spawn_blocking(move || governor.permission(&call, &options))
                            .await
                            .unwrap_or(None);
                    if shared.governor.stopped() {
                        stop.notify_one();
                    }
                    responder.respond(RequestPermissionResponse::new(match answer {
                        Some(id) => {
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id))
                        }
                        None => RequestPermissionOutcome::Cancelled,
                    }))
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReadTextFileRequest, responder, cx| {
                let shared = Arc::clone(&s3);
                cx.spawn(async move {
                    match shared
                        .governor
                        .read_text_file(&request.path, request.line, request.limit)
                        .await
                    {
                        Ok(content) => responder.respond(ReadTextFileResponse::new(content)),
                        Err(e) => responder.respond_with_error(refused(&e)),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WriteTextFileRequest, responder, cx| {
                let shared = Arc::clone(&s4);
                let stop = Arc::clone(&stop_for_fs);
                cx.spawn(async move {
                    let governor = Arc::clone(&shared.governor);
                    let (path, content) = (request.path.clone(), request.content.clone());
                    // A write may wait for a person: off the connection.
                    let written = tokio::task::spawn_blocking(move || {
                        tokio::runtime::Handle::current()
                            .block_on(governor.write_text_file(&path, &content))
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    if shared.governor.stopped() {
                        stop.notify_one();
                    }
                    match written {
                        Ok(()) => responder.respond(WriteTextFileResponse::new()),
                        Err(e) => responder.respond_with_error(refused(&e)),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: CreateTerminalRequest, responder, cx| {
                let shared = Arc::clone(&s5);
                let stop = Arc::clone(&stop_for_term);
                cx.spawn(async move {
                    let mut argv = if request.args.is_empty() {
                        super::govern::argv_of_line(&request.command)
                    } else {
                        vec![request.command.clone()]
                    };
                    argv.extend(request.args.iter().cloned());
                    let cwd = request.cwd.clone();
                    let limit = request
                        .output_byte_limit
                        .and_then(|n| usize::try_from(n).ok());
                    let worker = Arc::clone(&shared);
                    let created = tokio::task::spawn_blocking(move || {
                        let (argv, dir, env) = worker.governor.command(argv, cwd.as_deref())?;
                        let left = worker.deadline.saturating_duration_since(Instant::now());
                        let most = Duration::from_secs(crate::tools::MAX_COMMAND_SECS);
                        worker.terminals.create(
                            &worker.runner,
                            &argv,
                            dir,
                            env,
                            limit,
                            left.min(most),
                        )
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    if shared.governor.stopped() {
                        stop.notify_one();
                    }
                    match created {
                        Ok(id) => responder.respond(CreateTerminalResponse::new(id)),
                        Err(e) => responder.respond_with_error(refused(&e)),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: TerminalOutputRequest, responder, _cx| match s6
                .terminals
                .output(&request.terminal_id.to_string())
            {
                Ok((output, truncated, exit)) => responder.respond(
                    TerminalOutputResponse::new(output, truncated)
                        .exit_status(exit.map(exit_status)),
                ),
                Err(e) => responder.respond_with_error(refused(&e)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WaitForTerminalExitRequest, responder, cx| {
                let shared = Arc::clone(&s7);
                cx.spawn(async move {
                    let id = request.terminal_id.to_string();
                    let worker = Arc::clone(&shared);
                    let exit = tokio::task::spawn_blocking(move || worker.terminals.wait(&id))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                    match exit {
                        Ok(e) => {
                            responder.respond(WaitForTerminalExitResponse::new(exit_status(e)))
                        }
                        Err(e) => responder.respond_with_error(refused(&e)),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: KillTerminalRequest, responder, _cx| match s8
                .terminals
                .kill(&request.terminal_id.to_string())
            {
                Ok(()) => responder.respond(KillTerminalResponse::new()),
                Err(e) => responder.respond_with_error(refused(&e)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = Arc::clone(&shared);
                async move |request: ReleaseTerminalRequest, responder, _cx| match shared
                    .terminals
                    .release(&request.terminal_id.to_string())
                {
                    Ok(()) => responder.respond(ReleaseTerminalResponse::new()),
                    Err(e) => responder.respond_with_error(refused(&e)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, {
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(&stop);
            let attempt = attempt.clone();
            move |connection: ConnectionTo<AgentRole>| async move {
                let finished = session(&connection, &attempt, &shared, &stop).await;
                if let Ok(mut r) = result_for_run.lock() {
                    *r = finished;
                }
                Ok(())
            }
        })
        .await;

    shared.terminals.kill_all();
    shared.flush();
    let mut finished = result.lock().map(|r| r.clone()).unwrap_or_default();
    finished.stderr = stderr
        .lock()
        .map(|l| l.iter().cloned().collect())
        .unwrap_or_default();
    if let Err(e) = connected
        && finished.error.is_none()
        && finished.stop_reason.is_none()
    {
        finished.error = Some(match finished.stderr.last() {
            Some(line) => format!("{e}: {line}"),
            None => e.to_string(),
        });
    }
    if let Ok(w) = shared.words.lock()
        && let Some(usd) = w.usage_usd
    {
        let usage = finished.usage.get_or_insert_with(|| json!({}));
        usage["cost_usd"] = json!(usd);
    }
    finished
}

fn exit_status(e: super::terminal::Exit) -> TerminalExitStatus {
    TerminalExitStatus::new().exit_code(e.code).signal(e.signal)
}

/// A refusal the agent reads, in speccoder's words.
fn refused(message: &str) -> agent_client_protocol::Error {
    agent_client_protocol::Error::new(
        agent_client_protocol::schema::v1::ErrorCode::InvalidRequest.into(),
        message.to_owned(),
    )
}

/// The session itself, on the connection.
async fn session(
    connection: &ConnectionTo<AgentRole>,
    attempt: &Attempt,
    shared: &Arc<Shared>,
    stop: &Arc<Notify>,
) -> Finished {
    let mut finished = Finished::default();
    let init = match connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(declared()))
        .block_task()
        .await
    {
        Ok(i) => i,
        Err(e) => {
            finished.error = Some(format!("the agent did not initialize: {}", e.message));
            return finished;
        }
    };
    let answered = serde_json::to_value(init.protocol_version)
        .ok()
        .and_then(|v| v.as_u64());
    if answered != Some(SPOKEN) {
        finished.error = Some(format!(
            "the agent speaks ACP protocol version {}; speccoder speaks version {SPOKEN}",
            answered.map_or_else(|| "unknown".to_owned(), |v| v.to_string())
        ));
        return finished;
    }
    finished.agent_name = init.agent_info.as_ref().map(|i| i.name.clone());
    let auth_methods = serde_json::to_value(&init.auth_methods).unwrap_or_default();
    let shown = finished
        .agent_name
        .clone()
        .unwrap_or_else(|| attempt.agent.name.clone());

    let new = match connection
        .send_request(NewSessionRequest::new(attempt.cwd.clone()))
        .block_task()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // An agent that needs signing in says so, or fails while it
            // advertises ways to sign in: its own ways are named.
            let needs = e.code == agent_client_protocol::ErrorCode::AuthRequired
                || !init.auth_methods.is_empty();
            if needs {
                finished.sign_in = Some(sign_in_methods(&auth_methods, &attempt.argv));
            }
            finished.error = Some(if needs {
                format!(
                    "{shown} needs signing in before a session can start: {}",
                    e.message
                )
            } else {
                format!("{shown} could not start a session: {}", e.message)
            });
            return finished;
        }
    };
    let setup = serde_json::to_value(&new).unwrap_or_default();
    let session_id = new.session_id.clone();
    let can_close = init.agent_capabilities.session_capabilities.close.is_some();

    // The permission mode, settled before the first prompt.
    let profile = init.agent_info.as_ref().and_then(|i| {
        modes::profile_for(
            &i.name,
            &i.version,
            &setup["configOptions"],
            &setup["modes"],
        )
    });
    let advertised = Advertised::read(&setup["configOptions"], &setup["modes"], profile);
    let preference = attempt.agent.mode.as_deref();
    let plan = ModePlan::of(profile, advertised.as_ref(), preference);
    let mut options = setup["configOptions"].clone();
    let outcome = match (&plan.request, &advertised) {
        (Some(value), Some(ad)) => {
            let (o, now) = request_mode(connection, &session_id, ad, value).await;
            if let Some(now) = now {
                options = now;
            }
            Some(o)
        }
        _ => None,
    };
    let statement = modes::statement(
        &shown,
        profile,
        advertised.as_ref(),
        &plan,
        outcome.as_ref(),
        preference,
        &options,
    );
    finished
        .notices
        .push(format!("permission mode: {}", statement.sentence()));
    finished.mode = Some(statement);

    // The model, where the agent offers a choice of it.
    if let Some(model) = &attempt.model {
        finished
            .notices
            .push(choose_model(connection, &session_id, &options, model, &shown).await);
    }

    let turn = connection
        .send_request(PromptRequest::new(
            session_id.clone(),
            vec![ContentBlock::Text(TextContent::new(attempt.prompt.clone()))],
        ))
        .block_task();
    tokio::pin!(turn);
    let left = shared.deadline.saturating_duration_since(Instant::now());
    let ended = tokio::select! {
        r = &mut turn => Some(r),
        () = stop.notified() => None,
        () = tokio::time::sleep(left) => {
            finished.timed_out = true;
            None
        }
    };
    let response = match ended {
        Some(r) => r,
        None => {
            // Cancelled: speccoder's time is up, or a call waits for a person.
            shared.governor.stop();
            let _ = connection.send_notification(CancelNotification::new(session_id.clone()));
            match tokio::time::timeout(CANCEL_GRACE, &mut turn).await {
                Ok(r) => r,
                Err(_) => {
                    finished.stop_reason = Some("cancelled".to_owned());
                    finished.message = last_words(shared);
                    return finished;
                }
            }
        }
    };
    match response {
        Ok(r) => {
            finished.stop_reason = Some(stop_reason_name(r.stop_reason).to_owned());
            if let Some(u) = &r.usage {
                finished.usage = Some(json!({
                    "input_tokens": u.input_tokens,
                    "output_tokens": u.output_tokens,
                    "cache_read_tokens": u.cached_read_tokens.unwrap_or(0),
                    "cache_write_tokens": u.cached_write_tokens.unwrap_or(0),
                }));
            }
        }
        Err(e) => {
            if e.code == agent_client_protocol::ErrorCode::AuthRequired {
                finished.sign_in = Some(sign_in_methods(&auth_methods, &attempt.argv));
            }
            finished.error = Some(format!("{shown} failed the turn: {}", e.message));
        }
    }
    finished.message = last_words(shared);
    if can_close {
        let _ = tokio::time::timeout(
            CLOSE_GRACE,
            connection
                .send_request(CloseSessionRequest::new(session_id))
                .block_task(),
        )
        .await;
    }
    finished
}

/// The agent's words after its last call.
fn last_words(shared: &Shared) -> String {
    shared
        .words
        .lock()
        .map(|w| w.last.trim().to_owned())
        .unwrap_or_default()
}

/// Sends the one mode request a set-up may make; the agent's full option
/// list afterwards, where it sends one.
async fn request_mode(
    connection: &ConnectionTo<AgentRole>,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    advertised: &Advertised,
    value: &str,
) -> (Outcome, Option<Value>) {
    match &advertised.via {
        modes::Via::Config(id) => {
            let asked = connection
                .send_request(SetSessionConfigOptionRequest::new(
                    session_id.clone(),
                    id.clone(),
                    value,
                ))
                .block_task();
            match tokio::time::timeout(MODE_GRACE, asked).await {
                Err(_) => (Outcome::NoAnswer, None),
                Ok(Err(e)) => (Outcome::Refused(e.message), None),
                Ok(Ok(r)) => {
                    let all = serde_json::to_value(&r.config_options).unwrap_or_default();
                    (
                        Outcome::Confirmed(
                            advertised
                                .current_in(&all)
                                .unwrap_or_else(|| value.to_owned()),
                        ),
                        Some(all),
                    )
                }
            }
        }
        modes::Via::Modes => {
            let asked = connection
                .send_request(SetSessionModeRequest::new(
                    session_id.clone(),
                    value.to_owned(),
                ))
                .block_task();
            match tokio::time::timeout(MODE_GRACE, asked).await {
                Err(_) => (Outcome::NoAnswer, None),
                Ok(Err(e)) => (Outcome::Refused(e.message), None),
                Ok(Ok(_)) => (Outcome::Confirmed(value.to_owned()), None),
            }
        }
    }
}

/// Selects `model` through the agent's model option; what came of it.
async fn choose_model(
    connection: &ConnectionTo<AgentRole>,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    options: &Value,
    model: &str,
    agent: &str,
) -> String {
    let option = options
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["category"] == "model" && o["type"] == "select");
    let Some(option) = option else {
        return format!(
            "model `{model}`: {agent} offers no choice of model over ACP; it uses its own"
        );
    };
    let id = option["id"].as_str().unwrap_or("model").to_owned();
    let values: Vec<String> = option["options"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|o| match o["options"].as_array() {
            Some(group) => group.clone(),
            None => vec![o.clone()],
        })
        .filter_map(|o| o["value"].as_str().map(str::to_owned))
        .collect();
    let current = option["currentValue"].as_str().unwrap_or("its default");
    if !values.iter().any(|v| v == model) {
        let mut offered = values.clone();
        offered.dedup();
        return format!(
            "model `{model}` is not offered by {agent} (it offers {}); it uses {current}",
            offered.join(", ")
        );
    }
    if current == model {
        return format!("model: {model}");
    }
    let asked = connection
        .send_request(SetSessionConfigOptionRequest::new(
            session_id.clone(),
            id,
            model,
        ))
        .block_task();
    match tokio::time::timeout(MODE_GRACE, asked).await {
        Ok(Ok(_)) => format!("model: {model}"),
        Ok(Err(e)) => format!(
            "model `{model}`: {agent} refused it ({}); it uses {current}",
            e.message
        ),
        Err(_) => format!("model `{model}`: {agent} did not answer; it uses {current}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_methods_name_the_agents_own_login() {
        let copilot = json!([{"id": "copilot-login", "name": "Log in with Copilot CLI",
            "description": "Run `copilot login` in the terminal",
            "_meta": {"terminal-auth": {"command": "/opt/homebrew/Caskroom/copilot-cli/1.0.105/copilot", "args": ["login"]}}}]);
        let m = sign_in_methods(&copilot, &["copilot".to_owned(), "--acp".to_owned()]);
        assert_eq!(m[0].command.as_deref(), Some("copilot login"));
        assert_eq!(
            m[0].description.as_deref(),
            Some("Run `copilot login` in the terminal")
        );
        let terminal =
            json!([{"id": "t", "name": "Sign in", "type": "terminal", "args": ["--login"]}]);
        let m = sign_in_methods(&terminal, &["/opt/agent".to_owned()]);
        assert_eq!(m[0].command.as_deref(), Some("/opt/agent --login"));
        let opencode = json!([{"id": "opencode-login", "name": "Login with opencode", "description": "Run `opencode auth login` in the terminal"}]);
        assert_eq!(sign_in_methods(&opencode, &[])[0].command, None);
    }

    #[test]
    fn the_client_declares_files_and_terminals() {
        let v = serde_json::to_value(declared()).unwrap_or_default();
        assert_eq!(
            v["fs"],
            json!({"readTextFile": true, "writeTextFile": true})
        );
        assert_eq!(v["terminal"], json!(true));
    }
}
