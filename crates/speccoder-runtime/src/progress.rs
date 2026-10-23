//! Live progress: what a run is doing while it does it.
//!
//! Events are a side channel — not journaled, not evidence — so replay and
//! verdicts are untouched. [`Live`] keeps `live.json` in the project's state
//! directory up to date (read by `speccoder status` from another terminal),
//! prices token counts when `[model]` has prices, and forwards each event to
//! the CLI's printer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::substrate::model_config::Prices;

/// The live file in a project's state directory.
pub const FILE: &str = "live.json";

/// Tool calls `live.json` keeps for watchers (`speccoder tui`).
pub const RECENT: usize = 12;

/// Something a run did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A task attempt run begins.
    TaskStarted {
        /// Feature directory name.
        feature: String,
        /// Task ID.
        task: String,
        /// Task title.
        title: String,
    },
    /// A step of the run: `baseline checks`, `acceptance tests`, `attempt 2`, …
    Phase {
        /// Task ID.
        task: String,
        /// What is happening.
        phase: String,
    },
    /// A tool call the model asked for, with the policy's decision.
    ToolCall {
        /// Task ID.
        task: String,
        /// Model turn (1-based) within the attempt.
        turn: u32,
        /// The tool.
        tool: String,
        /// Its target: a path, a command, a pattern.
        target: String,
        /// `allow`, `deny`, `approval`, `unknown`, `stop`.
        decision: String,
    },
    /// Tokens so far in the current attempt.
    Usage {
        /// Task ID.
        task: String,
        /// Input tokens.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Input tokens read from the cache.
        cache_read_tokens: u64,
        /// USD, when priced.
        usd: Option<f64>,
    },
    /// The task run ended.
    TaskFinished {
        /// Task ID.
        task: String,
        /// `verified`, `regression_only`, `failed`, `blocked`, …
        outcome: String,
        /// The run's message.
        message: String,
        /// Input tokens, all attempts and the acceptance call.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Input tokens read from the cache.
        cache_read_tokens: u64,
        /// USD, when priced.
        usd: Option<f64>,
    },
    /// Anything else worth a line (convergence rounds, notices).
    Notice {
        /// The line.
        text: String,
    },
}

/// Receives events as they happen.
pub trait Progress: Send + Sync + std::fmt::Debug {
    /// One event.
    fn event(&self, event: &Event);
}

/// What `live.json` holds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveState {
    /// The speccoder process.
    pub pid: u32,
    /// Feature directory name.
    pub feature: String,
    /// Task ID.
    pub task: String,
    /// Task title.
    pub title: String,
    /// The current step.
    pub phase: String,
    /// The last tool call, e.g. `edit_file src/lib.rs (allow)`.
    pub last_call: Option<String>,
    /// The latest tool calls, oldest first (at most [`RECENT`]).
    #[serde(default)]
    pub recent: Vec<String>,
    /// Tokens of the current attempt.
    pub input_tokens: u64,
    /// Output tokens of the current attempt.
    pub output_tokens: u64,
    /// USD of the current attempt, when priced.
    pub usd: Option<f64>,
    /// Set when the task run ended: its outcome and message.
    pub finished: Option<String>,
    /// Unix seconds when the task run started.
    pub started_at: u64,
    /// Unix seconds of the last event.
    pub updated_at: u64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// USD for token counts (input includes cache reads).
#[must_use]
pub fn usd(prices: Option<Prices>, input: u64, cache_read: u64, output: u64) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)] // token counts far below 2^52
    prices.map(|p| {
        (input.saturating_sub(cache_read) as f64 * p.input
            + cache_read as f64 * p.cache_read
            + output as f64 * p.output)
            / 1_000_000.0
    })
}

/// Keeps `live.json`, prices usage, forwards to a printer.
#[derive(Debug)]
pub struct Live {
    file: PathBuf,
    prices: Option<Prices>,
    sink: Option<Arc<dyn Progress>>,
    state: Mutex<LiveState>,
}

impl Live {
    /// A live record in `dir` forwarding to `sink`.
    #[must_use]
    pub fn new(dir: &Path, prices: Option<Prices>, sink: Option<Arc<dyn Progress>>) -> Self {
        let _ = std::fs::create_dir_all(dir);
        Self {
            file: dir.join(FILE),
            prices,
            sink,
            state: Mutex::new(LiveState {
                pid: std::process::id(),
                ..LiveState::default()
            }),
        }
    }

    fn write(&self, state: &LiveState) {
        if let Ok(text) = serde_json::to_string_pretty(state) {
            let tmp = self.file.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &self.file);
            }
        }
    }
}

impl Progress for Live {
    fn event(&self, event: &Event) {
        let mut event = event.clone();
        // Price what can be priced.
        match &mut event {
            Event::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                usd: price,
                ..
            } => {
                *price = usd(
                    self.prices,
                    *input_tokens,
                    *cache_read_tokens,
                    *output_tokens,
                )
            }
            Event::TaskFinished {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                usd: price,
                ..
            } => {
                *price = usd(
                    self.prices,
                    *input_tokens,
                    *cache_read_tokens,
                    *output_tokens,
                )
            }
            _ => {}
        }
        if let Ok(mut s) = self.state.lock() {
            match &event {
                Event::TaskStarted {
                    feature,
                    task,
                    title,
                } => {
                    *s = LiveState {
                        pid: s.pid,
                        feature: feature.clone(),
                        task: task.clone(),
                        title: title.clone(),
                        phase: "starting".to_owned(),
                        started_at: now(),
                        ..LiveState::default()
                    };
                }
                Event::Phase { phase, .. } => phase.clone_into(&mut s.phase),
                Event::ToolCall {
                    turn,
                    tool,
                    target,
                    decision,
                    ..
                } => {
                    let call = format!("{tool} {target} ({decision})");
                    s.recent.push(format!("turn {turn}  {call}"));
                    if s.recent.len() > RECENT {
                        s.recent.remove(0);
                    }
                    s.last_call = Some(call);
                }
                Event::Usage {
                    input_tokens,
                    output_tokens,
                    usd,
                    ..
                } => {
                    s.input_tokens = *input_tokens;
                    s.output_tokens = *output_tokens;
                    s.usd = *usd;
                }
                Event::TaskFinished {
                    outcome, message, ..
                } => s.finished = Some(format!("{outcome}: {message}")),
                Event::Notice { .. } => {}
            }
            s.updated_at = now();
            self.write(&s);
        }
        if let Some(sink) = &self.sink {
            sink.event(&event);
        }
    }
}

/// The target of a tool call, for one line.
#[must_use]
pub fn target(tool: &str, args: &serde_json::Value) -> String {
    let text = match tool {
        "run_command" => args["argv"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default(),
        "search_code" => args["pattern"].as_str().unwrap_or_default().to_owned(),
        // An MCP tool: its first string argument, whatever it is called.
        name if name.starts_with(crate::mcp::PREFIX) => args
            .as_object()
            .and_then(|o| o.values().find_map(|v| v.as_str().map(str::to_owned)))
            .unwrap_or_default(),
        _ => args["path"].as_str().unwrap_or_default().to_owned(),
    };
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 60 {
        format!("{}…", one.chars().take(59).collect::<String>())
    } else {
        one
    }
}

// ── `speccoder status` ──────────────────────────────────────────────────────

/// The project's `live.json` and whether its process is still alive.
#[must_use]
pub fn read_live(path: &Path, state: &Path) -> Option<(LiveState, bool)> {
    let root = match crate::inspect(path, None).data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => return None,
    };
    let text = std::fs::read_to_string(crate::worktree::project_state_dir(state, &root).join(FILE))
        .ok()?;
    let live: LiveState = serde_json::from_str(&text).ok()?;
    let running = live.finished.is_none() && alive(live.pid);
    Some((live, running))
}

/// Whether process `pid` is alive.
pub fn alive(pid: u32) -> bool {
    i32::try_from(pid)
        .is_ok_and(|p| nix::sys::signal::kill(nix::unistd::Pid::from_raw(p), None).is_ok())
}

/// `speccoder status [PATH]`: the project's current (or last) run, from
/// `live.json`, in the state directory `state`.
pub fn status_with(path: &Path, state: &Path) -> CommandResult {
    let inspected = crate::inspect(path, None);
    let root = match inspected.data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => return CommandResult::new("status", inspected.status(), inspected.message()),
    };
    let file = crate::worktree::project_state_dir(state, &root).join(FILE);
    let live: LiveState = match std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
    {
        Some(l) => l,
        None => {
            return CommandResult::new(
                "status",
                CommandStatus::Ok,
                "no run has been recorded for this project yet",
            );
        }
    };
    let ago = now().saturating_sub(live.updated_at);
    let tokens = format!(
        "{} in · {} out{}",
        live.input_tokens,
        live.output_tokens,
        live.usd.map(|u| format!(" · ${u:.4}")).unwrap_or_default()
    );
    let message = match (&live.finished, alive(live.pid)) {
        (None, true) => format!(
            "running (pid {}): {} {} — {} for {}s; last: {}; {tokens}",
            live.pid,
            live.feature,
            live.task,
            live.phase,
            now().saturating_sub(live.started_at),
            live.last_call.as_deref().unwrap_or("-"),
        ),
        (None, false) => format!(
            "interrupted: {} {} stopped during {} {ago}s ago (pid {} is gone); the next `speccoder run` sets its leftovers aside",
            live.feature, live.task, live.phase, live.pid
        ),
        (Some(outcome), _) => format!(
            "last run: {} {} — {outcome} ({ago}s ago); {tokens}",
            live.feature, live.task
        ),
    };
    CommandResult::new("status", CommandStatus::Ok, &message)
}

/// [`status_with`] in the default state directory.
#[must_use]
pub fn status(path: &Path) -> CommandResult {
    match crate::worktree::state_dir() {
        Some(state) => status_with(path, &state),
        None => CommandResult::new(
            "status",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        ),
    }
}
