//! Live progress: what a run is doing while it does it.
//!
//! Events are a side channel — not journaled, not evidence — so replay and
//! verdicts are untouched. [`Live`] keeps `live.json` in the project's state
//! directory up to date (read by `speccoder report --live` from another terminal),
//! prices token counts when `[model]` has prices, and forwards each event to
//! the CLI's printer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::substrate::model_config::Prices;

/// The live file in a project's state directory.
pub const FILE: &str = "live.json";

/// One task of a feature run's plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedTask {
    /// Task ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// `done`, `ready`, `blocked` or `skipped`.
    pub state: String,
}

/// Tool calls `live.json` keeps for watchers (`speccoder tui`).
pub const RECENT: usize = 12;

/// Something a run did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A feature run begins: its tasks, as the graph has them (editors show
    /// them as a plan).
    RunStarted {
        /// Feature directory name.
        feature: String,
        /// Every task, in document order.
        tasks: Vec<PlannedTask>,
    },
    /// A task attempt run begins.
    TaskStarted {
        /// Feature directory name.
        feature: String,
        /// Task ID.
        task: String,
        /// Task title.
        title: String,
    },
    /// A stage of the task's pipeline began, ended or does not apply.
    Stage {
        /// Task ID.
        task: String,
        /// Which stage.
        stage: Stage,
        /// Where it is.
        state: StageState,
        /// `1/2 · sample 2/3` for an attempt, a short commit id, a reason.
        detail: Option<String>,
        /// How long it took, once it ended.
        elapsed_ms: Option<u64>,
        /// The last lines of a failed check (at most five).
        tail: Vec<String>,
    },
    /// A run picks up an attempt a killed process left behind.
    Resumed {
        /// Task ID.
        task: String,
        /// The stage it was in, when known.
        stage: Option<Stage>,
        /// The last tool call, when known.
        last_call: Option<String>,
    },
    /// A call waits for a person: notify them.
    ApprovalRequested {
        /// Task ID.
        task: String,
        /// The model's call id.
        call: String,
        /// The tool.
        tool: String,
        /// What waits, e.g. `cargo add x` (no control characters, ≤ 60 chars).
        what: String,
        /// The class of the object a decision covers: `write`, `command`,
        /// `network` or `mcp` (empty: none, the call is decided once).
        class: String,
        /// The object's short form: the path, `argv#<12 hex>`,
        /// `<tool>@<12 hex>` — no argv beyond what `what` shows.
        object: String,
    },
    /// A person decided a call inline.
    ApprovalDecided {
        /// Task ID.
        task: String,
        /// The model's call id.
        call: String,
        /// `approve` or `reject`.
        decision: String,
        /// `once`, `task` or `run`, for an approval.
        scope: Option<String>,
        /// Who decided.
        by: String,
        /// How that is known: `asserted`, `connected` (or `expired`).
        how: String,
        /// The object's class, as for [`Event::ApprovalRequested`].
        class: String,
        /// The object's short form.
        object: String,
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
        /// Whether an allowed call succeeded.
        ok: Option<bool>,
        /// The last lines of a failed command (at most five).
        tail: Vec<String>,
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
        /// Input tokens written to the cache.
        cache_write_tokens: u64,
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
        /// The attempt's agentplane run, when one ran.
        run: Option<String>,
        /// Input tokens, all attempts and the acceptance call.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Input tokens read from the cache.
        cache_read_tokens: u64,
        /// Input tokens written to the cache.
        cache_write_tokens: u64,
        /// USD, when priced.
        usd: Option<f64>,
    },
    /// What the model said in a turn, beside its tool calls: its
    /// account of what it is doing — a claim, never evidence. Control
    /// characters escaped; at most 2000 characters.
    Said {
        /// Task ID.
        task: String,
        /// Model turn (1-based) within the attempt.
        turn: u32,
        /// The words.
        text: String,
    },
    /// The model's summary of its reasoning in a turn, as the provider
    /// gave it (`[model] reasoning_summary`): its account of why — a claim,
    /// never evidence. Control characters escaped; at most 4000 characters.
    Reasoning {
        /// Task ID.
        task: String,
        /// Model turn (1-based) within the attempt.
        turn: u32,
        /// The summary.
        text: String,
    },
    /// Anything else worth a line (convergence rounds, notices).
    Notice {
        /// The line.
        text: String,
    },
    /// A bug's step (`bug.*` on the event stream).
    Bug {
        /// The task (`T003`, `bug:<slug>`).
        task: String,
        /// What happened.
        step: BugEvent,
    },
    /// The oracle at review (`oracle.*` on the event stream): stage names,
    /// counts and digests — never a test's content, never held-out material.
    Oracle {
        /// What happened.
        step: OracleEvent,
    },
}

/// A step of the oracle at review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OracleEvent {
    /// Writing tests went one stage further (progress).
    Stage {
        /// The requirement.
        requirement: String,
        /// `writing`, `comparing`, `checking_red`, `held_out`.
        stage: String,
        /// This stage's step.
        n: u32,
        /// Of.
        of: u32,
    },
    /// The approval froze a requirement's tests (domain).
    Frozen {
        /// The unit.
        unit: String,
        /// The requirement.
        requirement: String,
        /// The visible set's manifest digest.
        manifest_sha256: String,
        /// The approval's digest.
        approval_sha256: String,
        /// Visible tests frozen.
        visible_count: usize,
        /// Held-out tests sealed with them.
        held_out_count: usize,
        /// Who.
        by: String,
        /// Through what.
        via: String,
    },
    /// A frozen oracle lapsed with its requirement's wording (domain).
    Lapsed {
        /// The unit.
        unit: String,
        /// The requirement.
        requirement: String,
        /// The wording it was frozen on.
        frozen_wording_sha256: String,
        /// The wording now.
        current_wording_sha256: String,
    },
}

/// One statement's class in a reproduce try.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementClass {
    /// `E1`, `U1`.
    pub statement: String,
    /// The red class (`assertion`, `passed`, …).
    pub class: String,
}

/// A reproduction as the stream shows it: never its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShownReproduction {
    /// Its statement.
    pub statement: String,
    /// Its file.
    pub file: String,
    /// Its test.
    pub test: String,
    /// Its content's digest.
    pub digest: String,
}

/// A special case as the stream shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShownSpecialCase {
    /// The statement it targets.
    pub statement: String,
    /// Rejected by the combined set.
    pub rejected: bool,
}

/// A bug's step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BugEvent {
    /// The triad is recorded (domain).
    Recorded {
        /// The home's kind.
        home: String,
        /// The unit.
        unit: String,
        /// The requirement.
        requirement: String,
        /// The triad's digest.
        triad_digest: String,
        /// Through what.
        via: String,
    },
    /// One reproduce try ended.
    ReproduceTry {
        /// The try.
        n: u32,
        /// Of at most.
        of: u32,
        /// Per statement.
        per_statement: Vec<StatementClass>,
    },
    /// Every statement is reproduced.
    Reproduced {
        /// The reproductions.
        reproductions: Vec<ShownReproduction>,
    },
    /// The reproductions wait for the person.
    ConfirmWaiting {
        /// The digest to confirm.
        digest: String,
        /// The command.
        next: String,
    },
    /// The person confirmed them (domain).
    Confirmed {
        /// The digest.
        digest: String,
        /// Who.
        by: String,
        /// Through what.
        via: String,
        /// `shown` or `named_test`.
        basis: String,
    },
    /// The person rejected them (domain).
    ConfirmRejected {
        /// The digest.
        digest: String,
        /// Why.
        reason: String,
    },
    /// The pass-to-pass set was chosen.
    UnchangedSelected {
        /// `selected` or `none_selected`.
        state: String,
        /// Members.
        count: usize,
    },
    /// The revert and special cases were judged.
    Discrimination {
        /// `rejected`, `survived`, `unviable`.
        revert: String,
        /// The special cases.
        special_case: Vec<ShownSpecialCase>,
    },
    /// The bug's outcome (domain).
    Outcome {
        /// `awaiting_confirmation`, `cannot_reproduce`, …
        outcome: String,
        /// The fix's verdict.
        verdict: Option<String>,
        /// The stop's code.
        code: Option<String>,
        /// What to do next.
        next: Vec<String>,
    },
}

impl BugEvent {
    /// Its name on the event stream (registered in
    /// `speccoder_core::event::REGISTRY`).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Recorded { .. } => "bug.recorded",
            Self::ReproduceTry { .. } => "bug.reproduce_try",
            Self::Reproduced { .. } => "bug.reproduced",
            Self::ConfirmWaiting { .. } => "bug.confirm_waiting",
            Self::Confirmed { .. } => "bug.confirmed",
            Self::ConfirmRejected { .. } => "bug.confirm_rejected",
            Self::UnchangedSelected { .. } => "bug.unchanged_selected",
            Self::Discrimination { .. } => "bug.discrimination",
            Self::Outcome { .. } => "bug.outcome",
        }
    }

    /// The line a plain progress printer shows.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Recorded {
                unit, requirement, ..
            } => format!("bug recorded as {requirement} in {unit}"),
            Self::ReproduceTry {
                n,
                of,
                per_statement,
            } => format!(
                "reproduce try {n}/{of}: {}",
                per_statement
                    .iter()
                    .map(|s| format!("{} {}", s.statement, s.class))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Reproduced { reproductions } => format!(
                "reproduced: {} test(s) fail on an assertion",
                reproductions
                    .iter()
                    .filter(|r| r.statement.starts_with('E'))
                    .count()
            ),
            Self::ConfirmWaiting { next, .. } => format!("the reproductions wait for you: {next}"),
            Self::Confirmed { by, basis, .. } => {
                format!("reproductions confirmed by {by} ({basis})")
            }
            Self::ConfirmRejected { reason, .. } => format!("reproductions rejected: {reason}"),
            Self::UnchangedSelected { state, count } => {
                format!("pass-to-pass: {count} test(s) ({state})")
            }
            Self::Discrimination {
                revert,
                special_case,
            } => format!(
                "revert {revert}; {} special case(s), {} rejected",
                special_case.len(),
                special_case.iter().filter(|s| s.rejected).count()
            ),
            Self::Outcome {
                outcome, verdict, ..
            } => match verdict {
                Some(v) => format!("bug {outcome}: {v}"),
                None => format!("bug {outcome}"),
            },
        }
    }
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
    /// The current stage.
    #[serde(default)]
    pub stage: Option<Stage>,
    /// Its detail (`1/2`, a commit id).
    #[serde(default)]
    pub stage_detail: Option<String>,
    /// The last tool call, e.g. `edit_file src/lib.rs (allow)`.
    pub last_call: Option<String>,
    /// What the model last said, its first line (a claim).
    #[serde(default)]
    pub said: Option<String>,
    /// The last line of the model's latest reasoning summary (a claim).
    #[serde(default)]
    pub thinking: Option<String>,
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

/// USD for token counts (input includes cache reads and writes).
#[must_use]
pub fn usd(
    prices: Option<Prices>,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> Option<f64> {
    prices.map(|p| p.usd(input, output, cache_read, cache_write))
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
                cache_write_tokens,
                usd: price,
                ..
            } => {
                *price = usd(
                    self.prices,
                    *input_tokens,
                    *output_tokens,
                    *cache_read_tokens,
                    *cache_write_tokens,
                )
            }
            Event::TaskFinished {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                usd: price,
                ..
            } => {
                *price = usd(
                    self.prices,
                    *input_tokens,
                    *output_tokens,
                    *cache_read_tokens,
                    *cache_write_tokens,
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
                        started_at: now(),
                        ..LiveState::default()
                    };
                }
                Event::Stage {
                    stage,
                    state: StageState::Started,
                    detail,
                    ..
                } => {
                    s.stage = Some(*stage);
                    s.stage_detail.clone_from(detail);
                }
                Event::Said { text, .. } => {
                    s.said = text
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .map(|l| l.chars().take(160).collect());
                }
                // Its conclusion: where the summary ends up.
                Event::Reasoning { text, .. } => {
                    s.thinking = text
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .map(|l| l.chars().take(160).collect());
                }
                Event::Stage { .. }
                | Event::Resumed { .. }
                | Event::ApprovalRequested { .. }
                | Event::ApprovalDecided { .. } => {}
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
                Event::Notice { .. }
                | Event::RunStarted { .. }
                | Event::Bug { .. }
                | Event::Oracle { .. } => {}
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

// ── `speccoder report --live` ──────────────────────────────────────────────────────

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

/// `speccoder report --live`: the project's current (or last) run, from
/// `live.json`, in the state directory `state`.
pub fn status_with(path: &Path, state: &Path) -> CommandResult {
    // One inspection for both: what runs, and what waits.
    let inspected = crate::inspect(path, None);
    let root = match inspected.data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => return CommandResult::carried("report", &inspected),
    };
    let plane_dir = crate::worktree::project_state_dir(state, &root);
    let res = status_inner(&plane_dir);
    let lines = waiting_lines(&plane_dir);
    if lines.is_empty() {
        return res;
    }
    let mut next = res.next().to_vec();
    next.insert(
        0,
        speccoder_core::next::NextStep {
            command: "speccoder report --approvals".to_owned(),
            why: "see the waiting calls, then approve or reject them".to_owned(),
        },
    );
    let message = format!("{}\n{}", res.message(), lines.join("\n"));
    let status = res.status();
    res.reworded(status, &message).with_next(next)
}

/// One line per call waiting for a person: what, since when, until when.
fn waiting_lines(plane_dir: &Path) -> Vec<String> {
    // Unreadable is said, never shown as nothing waiting. Read beside the
    // journal's writer (a run in another process); no journal, nothing
    // waits — and a read never creates one.
    let unknown = |why: String| vec![format!("waiting for a person: unknown — {why}")];
    let rt = match crate::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return unknown(e),
    };
    let plane = match crate::substrate::Plane::open_for_reading(plane_dir) {
        Ok(Some(p)) => p,
        Ok(None) => return Vec::new(),
        Err(e) => return unknown(e.0),
    };
    let now = agentplane::core::Timestamp::now_utc();
    let open = match rt.block_on(plane.open_tasks()) {
        Ok(open) => open,
        Err(e) => return unknown(e.0),
    };
    open.into_iter()
        .filter_map(|t| {
            let request = crate::approvals::tasks::request_of(&t.justification.proposed_action)?;
            let since = (now - t.created_at).whole_minutes().max(0);
            let until = t
                .due_at
                .map(|d| {
                    format!(
                        ", denied as expired in {} h",
                        (d - now).whole_hours().max(0)
                    )
                })
                .unwrap_or_default();
            Some(format!(
                "waiting for a person: {} — {} (for {since} min{until})",
                request.task,
                request.covers()
            ))
        })
        .collect()
}

fn status_inner(plane_dir: &Path) -> CommandResult {
    let plane_dir = plane_dir.to_path_buf();
    // An in-place session: the mode, and for an interrupted one both ways out.
    if let Some(s) = crate::in_place::Session::load(&plane_dir) {
        let mode = s.mode_line();
        if !alive(s.pid) {
            let step = |command: &str, why: &str| speccoder_core::next::NextStep {
                command: command.to_owned(),
                why: why.to_owned(),
            };
            return CommandResult::new(
                "status",
                CommandStatus::Ok,
                &format!(
                    "interrupted in-place run of {} (pid {} is gone): the checkout is still on {}\n{mode}",
                    s.feature, s.pid, s.branch
                ),
            )
            .with_next(vec![
                step("speccoder run --in-place", "resume where it stopped"),
                step(
                    "speccoder run --in-place --return",
                    "save what it left and switch back without running",
                ),
            ]);
        }
        let res = status_live(&plane_dir);
        let message = format!("{}\n{mode}", res.message());
        let status = res.status();
        return res.reworded(status, &message);
    }
    status_live(&plane_dir)
}

/// `status` from `live.json`.
fn status_live(plane_dir: &Path) -> CommandResult {
    let file = plane_dir.join(FILE);
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
    use speccoder_core::next::{Outcome, next};
    let outcome = match (&live.finished, alive(live.pid)) {
        (None, true) => Some(Outcome::Running),
        (None, false) => Some(Outcome::Interrupted {
            task: live.task.clone(),
        }),
        (Some(_), _) => None,
    };
    let message = match (&live.finished, alive(live.pid)) {
        (None, true) => format!(
            "running (pid {}): {} {} — {} for {}s; last: {}; {tokens}{}{}",
            live.pid,
            live.feature,
            live.task,
            stage_word(&live),
            now().saturating_sub(live.started_at),
            live.last_call.as_deref().unwrap_or("-"),
            live.thinking
                .as_deref()
                .map(|s| format!("\nthe model thinks (a claim): {s}"))
                .unwrap_or_default(),
            live.said
                .as_deref()
                .map(|s| format!("\nthe model says (a claim): “{s}”"))
                .unwrap_or_default(),
        ),
        (None, false) => format!(
            "interrupted: {} {} stopped during {} {ago}s ago (pid {} is gone); the next `speccoder run` resumes it from its journal",
            live.feature,
            live.task,
            stage_word(&live),
            live.pid
        ),
        (Some(outcome), _) => format!(
            "last run: {} {} — {outcome} ({ago}s ago); {tokens}",
            live.feature, live.task
        ),
    };
    CommandResult::new("status", CommandStatus::Ok, &message)
        .with_next(outcome.map(|o| next(&o, None)).unwrap_or_default())
}

/// `verify`, `attempt 1/2`, or `starting`.
#[must_use]
pub fn stage_word(live: &LiveState) -> String {
    match (live.stage, &live.stage_detail) {
        (Some(st), Some(d)) => format!("{} {d}", st.name()),
        (Some(st), None) => st.name().to_owned(),
        (None, _) => "starting".to_owned(),
    }
}

/// The last lines of command output worth showing: at most five non-empty
/// lines, each at most 200 characters, control characters removed.
#[must_use]
pub fn tail(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            l.chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect::<String>()
        })
        .filter(|l| !l.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(5)..].to_vec()
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
        )
        .with_code("NO-STATE-DIR"),
    }
}
