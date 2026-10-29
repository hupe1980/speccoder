//! `--events`: the run as JSON Lines on stdout. Every line is one
//! [`Line`] — `{v, seq, ts, type, …}` — rendered from the same
//! [`progress::Event`]s the plain lines and the dashboard show; the first
//! is `session.started`, the last `result`, holding what `--json` prints.
//!
//! Consumers ignore types and fields they do not know. A closed stdout
//! ends the stream quietly; the command goes on to its natural end.

use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;
use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandResult, SCHEMA_VERSION};
use speccoder_runtime::progress::{self, Event, Progress};

/// What a stream can do, for consumers to feature-detect on.
pub const CAPABILITIES: &[&str] = &["events_v2", "approvals_inline"];

/// The longest string an event carries, in characters (then `…`).
pub const MAX_STRING: usize = 2000;

/// One line of the stream.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Line {
    /// The envelope version: the result envelope's `schema_version`.
    pub v: u32,
    /// 1, 2, 3, … — a gap means a lost line.
    pub seq: u64,
    /// RFC 3339 UTC, milliseconds.
    pub ts: String,
    /// The event.
    #[serde(flatten)]
    pub body: Body,
}

/// A task of the plan.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PlannedTask {
    /// Task ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// `done`, `ready`, `blocked` or `skipped`.
    pub state: String,
}

/// One entry of an external agent's plan.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AgentPlanEntry {
    /// Its words.
    pub content: String,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

/// The event, by `type`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Body {
    /// The first line: who is speaking, and what the stream supports.
    #[serde(rename = "session.started")]
    SessionStarted {
        /// speccoder's version.
        speccoder: String,
        /// The command.
        command: String,
        /// Feature-detect on these, never on the version.
        capabilities: Vec<String>,
    },
    /// A feature run begins, with its plan.
    #[serde(rename = "run.started")]
    RunStarted {
        /// Feature directory name.
        feature: String,
        /// Every task, in document order.
        tasks: Vec<PlannedTask>,
    },
    /// A task's run begins.
    #[serde(rename = "task.started")]
    TaskStarted {
        /// Feature.
        feature: String,
        /// Task ID.
        task: String,
        /// Title.
        title: String,
    },
    /// A stage begins.
    #[serde(rename = "stage.started")]
    StageStarted {
        /// Task ID.
        task: String,
        /// The stage.
        stage: Stage,
        /// `1/2`, a short commit id.
        detail: Option<String>,
    },
    /// A stage ends (or does not apply).
    #[serde(rename = "stage.finished")]
    StageFinished {
        /// Task ID.
        task: String,
        /// The stage.
        stage: Stage,
        /// `passed`, `failed` or `skipped`.
        state: StageState,
        /// Detail, or why it was skipped.
        detail: Option<String>,
        /// How long it took.
        elapsed_ms: Option<u64>,
        /// The last lines of a failed check.
        tail: Vec<String>,
    },
    /// A run picks up an attempt a killed process left behind.
    #[serde(rename = "task.resumed")]
    TaskResumed {
        /// Task ID.
        task: String,
        /// Where it was.
        stage: Option<Stage>,
        /// The last tool call.
        last_call: Option<String>,
    },
    /// A tool call and the policy's decision.
    #[serde(rename = "tool_call")]
    ToolCall {
        /// Task ID.
        task: String,
        /// Model turn, 1-based.
        turn: u32,
        /// The tool.
        tool: String,
        /// A path, a command, a pattern.
        target: String,
        /// `allow`, `deny`, `approval`, `unknown`, `stop`.
        decision: String,
        /// Whether an allowed call succeeded.
        ok: Option<bool>,
        /// The last lines of a failed command.
        tail: Vec<String>,
    },
    /// A call waits for a person.
    #[serde(rename = "approval.requested")]
    ApprovalRequested {
        /// Task ID.
        task: String,
        /// The call id.
        call: String,
        /// The tool.
        tool: String,
        /// What waits.
        what: String,
        /// The object's class: `write`, `command`, `network`, `mcp`.
        class: String,
        /// The object's short form (path, `argv#…`, `tool@…`).
        object: String,
    },
    /// A person decided a call.
    #[serde(rename = "approval.decided")]
    ApprovalDecided {
        /// Task ID.
        task: String,
        /// The call id.
        call: String,
        /// `approve` or `reject`.
        decision: String,
        /// `once`, `task`, `run`.
        scope: Option<String>,
        /// Who.
        by: String,
        /// `asserted`, `connected`, `expired`.
        how: String,
        /// The object's class.
        class: String,
        /// The object's short form.
        object: String,
    },
    /// Tokens so far in the current attempt (at most once a second).
    #[serde(rename = "usage")]
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
    /// A task's run ended.
    #[serde(rename = "task.finished")]
    TaskFinished {
        /// Task ID.
        task: String,
        /// `verified`, `regression_only`, `failed`, `needs_approval`, …
        outcome: String,
        /// The run's message.
        message: String,
        /// The attempt's run id.
        run: Option<String>,
        /// Input tokens.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Cache reads.
        cache_read_tokens: u64,
        /// Cache writes.
        cache_write_tokens: u64,
        /// USD, when priced.
        usd: Option<f64>,
        /// A regression-only tick: the reason's name.
        #[serde(skip_serializing_if = "Option::is_none")]
        regression_reason: Option<String>,
        /// A regression-only tick: the reason with its details and wording.
        #[serde(skip_serializing_if = "Option::is_none")]
        regression: Option<speccoder_core::regression::Regression>,
    },
    /// The attempt's changed test files were checked against what the run collected.
    #[serde(rename = "collection.checked")]
    CollectionChecked {
        /// The public projection.
        #[serde(flatten)]
        public: PublicCollectionChecked,
    },
    /// The search for the tick that made already-passing tests pass ended.
    #[serde(rename = "regression.bisected")]
    RegressionBisected {
        /// Task ID.
        task: String,
        /// Acceptance runs it took.
        runs: u32,
        /// The implementing task, `before_feature`, or `null`.
        result: Option<String>,
    },
    /// An external agent makes the attempt: its name and permission mode
    /// (`decides_alone`: it was not put into a mode that asks first).
    #[serde(rename = "agent.started")]
    AgentStarted {
        /// Task ID.
        task: String,
        /// The agent's name.
        agent: String,
        /// Its permission mode, in a sentence.
        mode: String,
        /// Calls it makes without asking never reach speccoder.
        decides_alone: bool,
        /// A recording says whether it asks.
        known: bool,
    },
    /// An external agent changed something without asking.
    #[serde(rename = "agent.unasked")]
    AgentUnasked {
        /// Task ID.
        task: String,
        /// Calls so far.
        calls: u32,
    },
    /// An external agent's plan, as it last sent it: a claim.
    #[serde(rename = "agent.plan")]
    AgentPlan {
        /// Task ID.
        task: String,
        /// Its entries.
        entries: Vec<AgentPlanEntry>,
    },
    /// What the model said in a turn: its account, a claim.
    #[serde(rename = "said")]
    Said {
        /// Task ID.
        task: String,
        /// Model turn (1-based).
        turn: u32,
        /// The words.
        text: String,
    },
    /// The model's summary of its reasoning in a turn: its account, a claim.
    #[serde(rename = "reasoning")]
    Reasoning {
        /// Task ID.
        task: String,
        /// Model turn (1-based).
        turn: u32,
        /// The summary.
        text: String,
    },
    /// Anything else worth a line.
    #[serde(rename = "notice")]
    Notice {
        /// The line.
        text: String,
    },
    /// A bug's triad was recorded.
    #[serde(rename = "bug.recorded")]
    BugRecorded {
        /// Its task.
        task: String,
        /// The public projection.
        #[serde(flatten)]
        public: PublicBugRecorded,
    },
    /// One reproduce try ended.
    #[serde(rename = "bug.reproduce_try")]
    BugReproduceTry {
        /// Its task.
        task: String,
        /// The try.
        n: u32,
        /// Of at most.
        of: u32,
        /// Each statement's class.
        per_statement: Vec<StatementClass>,
    },
    /// Every statement is reproduced.
    #[serde(rename = "bug.reproduced")]
    BugReproduced {
        /// Its task.
        task: String,
        /// The reproductions (never their content).
        reproductions: Vec<ShownReproduction>,
    },
    /// The reproductions wait for the person.
    #[serde(rename = "bug.confirm_waiting")]
    BugConfirmWaiting {
        /// Its task.
        task: String,
        /// The digest to confirm.
        digest: String,
        /// The command.
        next: String,
    },
    /// The person confirmed them.
    #[serde(rename = "bug.confirmed")]
    BugConfirmed {
        /// Its task.
        task: String,
        /// The public projection.
        #[serde(flatten)]
        public: PublicBugConfirmed,
    },
    /// The person rejected them.
    #[serde(rename = "bug.confirm_rejected")]
    BugConfirmRejected {
        /// Its task.
        task: String,
        /// The public projection.
        #[serde(flatten)]
        public: PublicBugConfirmRejected,
    },
    /// The pass-to-pass set was chosen.
    #[serde(rename = "bug.unchanged_selected")]
    BugUnchangedSelected {
        /// Its task.
        task: String,
        /// `selected` or `none_selected`.
        state: String,
        /// Members.
        count: usize,
    },
    /// The revert and the special cases were judged.
    #[serde(rename = "bug.discrimination")]
    BugDiscrimination {
        /// Its task.
        task: String,
        /// `rejected`, `survived`, `unviable`.
        revert: String,
        /// The special cases.
        special_case: Vec<ShownSpecialCase>,
    },
    /// The bug's outcome.
    #[serde(rename = "bug.outcome")]
    BugOutcome {
        /// Its task.
        task: String,
        /// The public projection.
        #[serde(flatten)]
        public: PublicBugOutcome,
    },
    /// Writing tests at review went one stage further.
    #[serde(rename = "oracle.stage")]
    OracleStage {
        /// The requirement.
        requirement: String,
        /// `writing`, `comparing`, `checking_red`, `held_out`.
        stage: String,
        /// This stage's step.
        n: u32,
        /// Of.
        of: u32,
    },
    /// An approval froze a requirement's tests.
    #[serde(rename = "oracle.frozen")]
    OracleFrozen {
        /// The public projection.
        #[serde(flatten)]
        public: PublicOracleFrozen,
    },
    /// A frozen oracle lapsed with its requirement's wording.
    #[serde(rename = "oracle.lapsed")]
    OracleLapsed {
        /// The public projection.
        #[serde(flatten)]
        public: PublicOracleLapsed,
    },
    /// The last line: the command's result, as `--json` prints it.
    #[serde(rename = "result")]
    Result {
        /// The result envelope.
        result: CommandResult,
    },
    /// Plan through stopped where a person decides, at a failure or at the end.
    #[serde(rename = "planning.stopped")]
    PlanningStopped {
        /// The unit, once it exists.
        unit: Option<String>,
        /// `questions`, `review`, `failed`, `done`.
        kind: String,
        /// The skill a continuation runs next.
        next_skill: Option<String>,
    },
    /// A step of a new project's plan began.
    #[serde(rename = "scaffold.started")]
    ScaffoldStarted {
        /// The step (`git_init`, `scaffold.1`, `format_init`, …).
        step: String,
        /// Its command, exactly.
        argv: Vec<String>,
        /// `local` or `network`.
        access: String,
    },
    /// A step of a new project's plan ended.
    #[serde(rename = "scaffold.finished")]
    ScaffoldFinished {
        /// The step.
        step: String,
        /// `done`, `skipped`, `failed`.
        state: String,
        /// Its exit status, when it ran.
        exit: Option<i32>,
        /// How long it took.
        duration_ms: u64,
    },
    /// A new project was created and committed.
    #[serde(rename = "project.created")]
    ProjectCreated {
        /// The public projection.
        #[serde(flatten)]
        public: PublicProjectCreated,
    },
    /// A project speccoder created was undone.
    #[serde(rename = "project.undone")]
    ProjectUndone {
        /// The public projection.
        #[serde(flatten)]
        public: PublicProjectUndone,
    },
}

/// `project.created`, as published: no path, no idea text.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicProjectCreated {
    /// SHA-256 of the folder's canonical path.
    pub path_digest: String,
    /// `new_folder` or `existing_repository`.
    pub mode: String,
    /// The stack.
    pub stack: String,
    /// The spec format.
    pub format: String,
    /// speccoder's commit.
    pub commit: String,
    /// Through what.
    pub via: String,
}

/// `project.undone`, as published.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicProjectUndone {
    /// SHA-256 of the folder's canonical path.
    pub path_digest: String,
    /// `trash` or `revert`.
    pub kind: String,
    /// Through what.
    pub via: String,
}

/// One statement's class in a reproduce try.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StatementClass {
    /// `E1`, `U1`.
    pub statement: String,
    /// `assertion`, `passed`, `build_error`, …
    pub class: String,
}

/// A reproduction as the stream shows it.
#[derive(Debug, Serialize, JsonSchema)]
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
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShownSpecialCase {
    /// The statement it targets.
    pub statement: String,
    /// Rejected (or ruled).
    pub rejected: bool,
}

/// `bug.recorded`, as published: ids and digests only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicBugRecorded {
    /// The home's kind.
    pub home: String,
    /// The unit.
    pub unit: String,
    /// The requirement.
    pub requirement: String,
    /// The triad's digest.
    pub triad_digest: String,
    /// Through what.
    pub via: String,
}

/// `bug.confirmed`, as published.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicBugConfirmed {
    /// The digest confirmed.
    pub digest: String,
    /// Who.
    pub by: String,
    /// Through what.
    pub via: String,
    /// `shown` or `named_test`.
    pub basis: String,
}

/// `bug.confirm_rejected`, as published.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicBugConfirmRejected {
    /// The digest rejected.
    pub digest: String,
    /// The person's reason.
    pub reason: String,
}

/// `bug.outcome`, as published.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicBugOutcome {
    /// `awaiting_confirmation`, `cannot_reproduce`, `fixed`, …
    pub outcome: String,
    /// The fix's verdict.
    pub verdict: Option<String>,
    /// The stop's code.
    pub code: Option<String>,
    /// What to do next.
    pub next: Vec<String>,
}

/// `oracle.frozen`, as published: digests and counts, never a test.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicOracleFrozen {
    /// The unit.
    pub unit: String,
    /// The requirement.
    pub requirement: String,
    /// The visible set's manifest digest.
    pub manifest_sha256: String,
    /// The approval's digest.
    pub approval_sha256: String,
    /// Visible tests frozen.
    pub visible_count: usize,
    /// Held-out tests sealed with them.
    pub held_out_count: usize,
    /// Who.
    pub by: String,
    /// Through what.
    pub via: String,
}

/// `collection.checked`, as published: paths and states.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicCollectionChecked {
    /// The task.
    pub task: String,
    /// Each judged test file.
    pub files: Vec<PublicCollectionFile>,
}

/// One file of [`PublicCollectionChecked`].
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicCollectionFile {
    /// Workspace-relative path.
    pub path: String,
    /// `collected`, `not_collected`, `unknown`.
    pub state: speccoder_core::collection::FileState,
    /// `static`, `junit`, `listing`; `null` when unknown.
    pub decided_by: Option<speccoder_core::collection::DecidedBy>,
}

/// `oracle.lapsed`, as published.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PublicOracleLapsed {
    /// The unit.
    pub unit: String,
    /// The requirement.
    pub requirement: String,
    /// The wording it was frozen on.
    pub frozen_wording_sha256: String,
    /// The wording now.
    pub current_wording_sha256: String,
}

/// The stream's body for the oracle's step.
fn oracle_body(step: &progress::OracleEvent) -> Body {
    use progress::OracleEvent as O;
    match step {
        O::Stage {
            requirement,
            stage,
            n,
            of,
        } => Body::OracleStage {
            requirement: clean(requirement),
            stage: stage.clone(),
            n: *n,
            of: *of,
        },
        O::Frozen {
            unit,
            requirement,
            manifest_sha256,
            approval_sha256,
            visible_count,
            held_out_count,
            by,
            via,
        } => Body::OracleFrozen {
            public: PublicOracleFrozen {
                unit: clean(unit),
                requirement: clean(requirement),
                manifest_sha256: manifest_sha256.clone(),
                approval_sha256: approval_sha256.clone(),
                visible_count: *visible_count,
                held_out_count: *held_out_count,
                by: clean(by),
                via: via.clone(),
            },
        },
        O::Lapsed {
            unit,
            requirement,
            frozen_wording_sha256,
            current_wording_sha256,
        } => Body::OracleLapsed {
            public: PublicOracleLapsed {
                unit: clean(unit),
                requirement: clean(requirement),
                frozen_wording_sha256: frozen_wording_sha256.clone(),
                current_wording_sha256: current_wording_sha256.clone(),
            },
        },
    }
}

/// The stream's body for a bug's step.
fn bug_body(task: &str, step: &progress::BugEvent) -> Body {
    use progress::BugEvent as B;
    let task = clean(task);
    match step {
        B::Recorded {
            home,
            unit,
            requirement,
            triad_digest,
            via,
        } => Body::BugRecorded {
            task,
            public: PublicBugRecorded {
                home: home.clone(),
                unit: clean(unit),
                requirement: clean(requirement),
                triad_digest: triad_digest.clone(),
                via: via.clone(),
            },
        },
        B::ReproduceTry {
            n,
            of,
            per_statement,
        } => Body::BugReproduceTry {
            task,
            n: *n,
            of: *of,
            per_statement: per_statement
                .iter()
                .map(|s| StatementClass {
                    statement: s.statement.clone(),
                    class: s.class.clone(),
                })
                .collect(),
        },
        B::Reproduced { reproductions } => Body::BugReproduced {
            task,
            reproductions: reproductions
                .iter()
                .map(|r| ShownReproduction {
                    statement: r.statement.clone(),
                    file: clean(&r.file),
                    test: clean(&r.test),
                    digest: r.digest.clone(),
                })
                .collect(),
        },
        B::ConfirmWaiting { digest, next } => Body::BugConfirmWaiting {
            task,
            digest: digest.clone(),
            next: clean(next),
        },
        B::Confirmed {
            digest,
            by,
            via,
            basis,
        } => Body::BugConfirmed {
            task,
            public: PublicBugConfirmed {
                digest: digest.clone(),
                by: clean(by),
                via: via.clone(),
                basis: basis.clone(),
            },
        },
        B::ConfirmRejected { digest, reason } => Body::BugConfirmRejected {
            task,
            public: PublicBugConfirmRejected {
                digest: digest.clone(),
                reason: clean(reason),
            },
        },
        B::UnchangedSelected { state, count } => Body::BugUnchangedSelected {
            task,
            state: state.clone(),
            count: *count,
        },
        B::Discrimination {
            revert,
            special_case,
        } => Body::BugDiscrimination {
            task,
            revert: revert.clone(),
            special_case: special_case
                .iter()
                .map(|s| ShownSpecialCase {
                    statement: s.statement.clone(),
                    rejected: s.rejected,
                })
                .collect(),
        },
        B::Outcome {
            outcome,
            verdict,
            code,
            next,
        } => Body::BugOutcome {
            task,
            public: PublicBugOutcome {
                outcome: outcome.clone(),
                verdict: verdict.clone(),
                code: code.clone(),
                next: cleaned(next),
            },
        },
    }
}

/// The `type` of a body, as registered (`speccoder_core::event`).
#[must_use]
pub fn type_of(body: &Body) -> String {
    serde_json::to_value(body)
        .ok()
        .and_then(|v| v["type"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// No control characters, at most [`MAX_STRING`] characters.
#[must_use]
pub fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    let mut n = 0;
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // An escape sequence: drop it whole (CSI `ESC [ … final`).
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
            continue;
        }
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        if n == MAX_STRING {
            out.push('…');
            break;
        }
        out.push(c);
        n += 1;
    }
    out
}

fn cleaned(v: &[String]) -> Vec<String> {
    v.iter().map(|s| clean(s)).collect()
}

/// The stream's body for a progress event.
#[must_use]
pub fn body(event: &Event) -> Body {
    match event {
        Event::RunStarted { feature, tasks } => Body::RunStarted {
            feature: clean(feature),
            tasks: tasks
                .iter()
                .map(|t: &progress::PlannedTask| PlannedTask {
                    id: clean(&t.id),
                    title: clean(&t.title),
                    state: t.state.clone(),
                })
                .collect(),
        },
        Event::TaskStarted {
            feature,
            task,
            title,
        } => Body::TaskStarted {
            feature: clean(feature),
            task: clean(task),
            title: clean(title),
        },
        Event::Stage {
            task,
            stage,
            state: StageState::Started,
            detail,
            ..
        } => Body::StageStarted {
            task: clean(task),
            stage: *stage,
            detail: detail.as_deref().map(clean),
        },
        Event::Stage {
            task,
            stage,
            state,
            detail,
            elapsed_ms,
            tail,
        } => Body::StageFinished {
            task: clean(task),
            stage: *stage,
            state: *state,
            detail: detail.as_deref().map(clean),
            elapsed_ms: *elapsed_ms,
            tail: cleaned(tail),
        },
        Event::Resumed {
            task,
            stage,
            last_call,
        } => Body::TaskResumed {
            task: clean(task),
            stage: *stage,
            last_call: last_call.as_deref().map(clean),
        },
        Event::ApprovalRequested {
            task,
            call,
            tool,
            what,
            class,
            object,
        } => Body::ApprovalRequested {
            task: clean(task),
            call: clean(call),
            tool: clean(tool),
            what: clean(what),
            class: clean(class),
            object: clean(object),
        },
        Event::ApprovalDecided {
            task,
            call,
            decision,
            scope,
            by,
            how,
            class,
            object,
        } => Body::ApprovalDecided {
            task: clean(task),
            call: clean(call),
            decision: decision.clone(),
            scope: scope.clone(),
            by: clean(by),
            how: how.clone(),
            class: clean(class),
            object: clean(object),
        },
        Event::ToolCall {
            task,
            turn,
            tool,
            target,
            decision,
            ok,
            tail,
        } => Body::ToolCall {
            task: clean(task),
            turn: *turn,
            tool: clean(tool),
            target: clean(target),
            decision: decision.clone(),
            ok: *ok,
            tail: cleaned(tail),
        },
        Event::Usage {
            task,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            usd,
        } => Body::Usage {
            task: clean(task),
            input_tokens: *input_tokens,
            output_tokens: *output_tokens,
            cache_read_tokens: *cache_read_tokens,
            cache_write_tokens: *cache_write_tokens,
            usd: *usd,
        },
        Event::TaskFinished {
            task,
            outcome,
            message,
            run,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            usd,
            regression,
        } => Body::TaskFinished {
            task: clean(task),
            outcome: outcome.clone(),
            message: clean(message),
            run: run.clone(),
            input_tokens: *input_tokens,
            output_tokens: *output_tokens,
            cache_read_tokens: *cache_read_tokens,
            cache_write_tokens: *cache_write_tokens,
            usd: *usd,
            regression_reason: regression.as_ref().map(|r| r.reason.name().to_owned()),
            regression: regression.clone(),
        },
        Event::CollectionChecked { task, files } => Body::CollectionChecked {
            public: PublicCollectionChecked {
                task: clean(task),
                files: files
                    .iter()
                    .map(|f| PublicCollectionFile {
                        path: clean(&f.path),
                        state: f.state,
                        decided_by: f.decided_by,
                    })
                    .collect(),
            },
        },
        Event::RegressionBisected { task, runs, result } => Body::RegressionBisected {
            task: clean(task),
            runs: *runs,
            result: result.clone(),
        },
        Event::AgentStarted {
            task,
            agent,
            mode,
            decides_alone,
            known,
        } => Body::AgentStarted {
            task: clean(task),
            agent: clean(agent),
            mode: clean(mode),
            decides_alone: *decides_alone,
            known: *known,
        },
        Event::AgentUnasked { task, calls } => Body::AgentUnasked {
            task: clean(task),
            calls: *calls,
        },
        Event::AgentPlan { task, entries } => Body::AgentPlan {
            task: clean(task),
            entries: entries
                .iter()
                .map(|e| AgentPlanEntry {
                    content: clean(&e.content),
                    status: clean(&e.status),
                })
                .collect(),
        },
        Event::Said { task, turn, text } => Body::Said {
            task: clean(task),
            turn: *turn,
            text: text.clone(),
        },
        Event::Reasoning { task, turn, text } => Body::Reasoning {
            task: clean(task),
            turn: *turn,
            text: text.clone(),
        },
        Event::Notice { text } => Body::Notice { text: clean(text) },
        Event::Bug { task, step } => bug_body(task, step),
        Event::Oracle { step } => oracle_body(step),
        Event::PlanningStopped {
            unit,
            kind,
            next_skill,
        } => Body::PlanningStopped {
            unit: unit.as_deref().map(clean),
            kind: kind.clone(),
            next_skill: next_skill.as_deref().map(clean),
        },
        Event::Project { step } => project_body(step),
    }
}

/// The stream's body for a new project's step.
fn project_body(step: &progress::ProjectEvent) -> Body {
    use progress::ProjectEvent as P;
    match step {
        P::ScaffoldStarted { step, argv, access } => Body::ScaffoldStarted {
            step: clean(step),
            argv: cleaned(argv),
            access: access.clone(),
        },
        P::ScaffoldFinished {
            step,
            state,
            exit,
            duration_ms,
        } => Body::ScaffoldFinished {
            step: clean(step),
            state: state.clone(),
            exit: *exit,
            duration_ms: *duration_ms,
        },
        P::Created {
            path_digest,
            mode,
            stack,
            format,
            commit,
            via,
        } => Body::ProjectCreated {
            public: PublicProjectCreated {
                path_digest: path_digest.clone(),
                mode: mode.clone(),
                stack: stack.clone(),
                format: format.clone(),
                commit: clean(commit),
                via: via.clone(),
            },
        },
        P::Undone {
            path_digest,
            how,
            via,
        } => Body::ProjectUndone {
            public: PublicProjectUndone {
                path_digest: path_digest.clone(),
                kind: how.clone(),
                via: via.clone(),
            },
        },
    }
}

/// RFC 3339 UTC with milliseconds.
#[must_use]
pub fn now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (y, m, day) = civil(i64::try_from(days).unwrap_or(0));
    format!(
        "{y:04}-{m:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        d.subsec_millis()
    )
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (era * 400 + yoe + i64::from(m <= 2), m, d)
}

struct State {
    seq: u64,
    open: bool,
    /// Per task: when the last `usage` went out, and one held back.
    usage: Vec<(String, Instant, Option<Body>)>,
}

/// Writes the stream. Shared by the run's progress and `main`'s result.
pub struct Events {
    out: Mutex<Box<dyn Write + Send>>,
    state: Mutex<State>,
}

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Events")
    }
}

impl Events {
    /// A stream into `out`, opened with `session.started` for `command`.
    #[must_use]
    pub fn start(out: Box<dyn Write + Send>, command: &str) -> Self {
        let events = Self {
            out: Mutex::new(out),
            state: Mutex::new(State {
                seq: 0,
                open: true,
                usage: Vec::new(),
            }),
        };
        events.write(Body::SessionStarted {
            speccoder: env!("CARGO_PKG_VERSION").to_owned(),
            command: command.to_owned(),
            capabilities: CAPABILITIES.iter().map(|c| (*c).to_owned()).collect(),
        });
        events
    }

    fn write(&self, body: Body) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        // A name the registry does not list is never emitted.
        if !state.open || speccoder_core::event::lookup(&type_of(&body)).is_none() {
            return;
        }
        state.seq += 1;
        let line = Line {
            v: SCHEMA_VERSION,
            seq: state.seq,
            ts: now(),
            body,
        };
        let Ok(mut out) = self.out.lock() else {
            return;
        };
        let written = serde_json::to_writer(&mut *out, &line)
            .map_err(std::io::Error::from)
            .and_then(|()| writeln!(out))
            .and_then(|()| out.flush());
        if written.is_err() {
            // The reader went away: stop writing, let the command finish.
            state.open = false;
        }
    }

    /// The last line.
    pub fn result(&self, result: &CommandResult) {
        self.flush_usage(None);
        self.write(Body::Result {
            result: result.clone(),
        });
    }

    /// Sends the `usage` held back for `task` (all tasks: `None`).
    fn flush_usage(&self, task: Option<&str>) {
        let held: Vec<Body> = match self.state.lock() {
            Ok(mut s) => s
                .usage
                .iter_mut()
                .filter(|(t, ..)| task.is_none_or(|w| w == t))
                .filter_map(|(_, _, held)| held.take())
                .collect(),
            Err(_) => Vec::new(),
        };
        for b in held {
            self.write(b);
        }
    }
}

/// `usage` at most once a second per task.
const USAGE_EVERY: Duration = Duration::from_secs(1);

impl Progress for Events {
    fn event(&self, event: &Event) {
        let body = body(event);
        match &body {
            Body::Usage { task, .. } => {
                let send = match self.state.lock() {
                    Ok(mut s) => {
                        let now = Instant::now();
                        match s.usage.iter_mut().find(|(t, ..)| t == task) {
                            Some((_, last, held)) if now.duration_since(*last) < USAGE_EVERY => {
                                *held = Some(body);
                                return;
                            }
                            Some((_, last, held)) => {
                                *last = now;
                                *held = None;
                            }
                            None => s.usage.push((task.clone(), now, None)),
                        }
                        true
                    }
                    Err(_) => false,
                };
                if send {
                    self.write(body);
                }
            }
            Body::TaskFinished { task, .. } => {
                let task = task.clone();
                self.flush_usage(Some(&task));
                self.write(body);
            }
            _ => self.write(body),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_type_is_registered_and_every_registered_name_is_a_type() {
        let schema = serde_json::to_value(schemars::schema_for!(Line)).unwrap();
        let mut types = Vec::new();
        fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Object(o) => {
                    if let Some(t) = o
                        .get("properties")
                        .and_then(|p| p.get("type"))
                        .and_then(|t| t.get("const"))
                        .and_then(|c| c.as_str())
                    {
                        out.push(t.to_owned());
                    }
                    for x in o.values() {
                        walk(x, out);
                    }
                }
                serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                _ => {}
            }
        }
        walk(&schema, &mut types);
        types.sort();
        types.dedup();
        for t in &types {
            assert!(
                speccoder_core::event::lookup(t).is_some(),
                "{t} is not registered"
            );
        }
        for n in speccoder_core::event::REGISTRY {
            assert!(
                types.iter().any(|t| t == n.name),
                "{} is registered but no line has it",
                n.name
            );
        }
        // An unregistered type is dropped, not written.
        let buf = Buf::default();
        let e = Events::start(Box::new(buf.clone()), "bug");
        e.event(&Event::Bug {
            task: "T001".to_owned(),
            step: progress::BugEvent::UnchangedSelected {
                state: "selected".to_owned(),
                count: 2,
            },
        });
        let lines = lines(&buf);
        assert!(
            lines
                .iter()
                .any(|l| l["type"] == "bug.unchanged_selected" && l["count"] == 2),
            "{lines:?}"
        );
    }

    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("poisoned"))?
                .extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn lines(buf: &Buf) -> Vec<serde_json::Value> {
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn usage(task: &str, n: u64) -> Event {
        Event::Usage {
            task: task.to_owned(),
            input_tokens: n,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: None,
        }
    }

    #[test]
    fn a_stream_opens_numbers_throttles_and_ends_in_the_result() {
        let buf = Buf::default();
        let e = Events::start(Box::new(buf.clone()), "run");
        e.event(&usage("T1", 1));
        e.event(&usage("T1", 2)); // held back
        e.event(&usage("T1", 3)); // replaces it
        e.event(&Event::TaskFinished {
            task: "T1".to_owned(),
            outcome: "verified".to_owned(),
            message: "ok\u{1b}[31m red\u{7}".to_owned(),
            run: Some("run_1".to_owned()),
            input_tokens: 3,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: None,
            regression: None,
        });
        e.result(&CommandResult::ok("run", "done"));
        let l = lines(&buf);
        let types: Vec<&str> = l.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            [
                "session.started",
                "usage",
                "usage",
                "task.finished",
                "result"
            ]
        );
        let seqs: Vec<u64> = l.iter().map(|v| v["seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, [1, 2, 3, 4, 5]);
        assert_eq!(l[2]["input_tokens"], 3, "the last usage before the finish");
        assert_eq!(l[3]["message"], "ok red", "control sequences removed");
        assert_eq!(l[4]["result"]["command"], "run");
        assert_eq!(l[0]["v"], SCHEMA_VERSION);
        assert!(l[0]["ts"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn a_closed_reader_ends_the_stream_quietly() {
        let e = Events::start(Box::new(Closed), "run");
        e.event(&Event::Notice {
            text: "x".to_owned(),
        });
        e.result(&CommandResult::ok("run", "done"));
    }

    #[test]
    fn long_strings_are_bounded() {
        let s = "x".repeat(MAX_STRING + 10);
        let c = clean(&s);
        assert_eq!(c.chars().count(), MAX_STRING + 1);
        assert!(c.ends_with('…'));
    }

    #[test]
    fn the_clock_is_rfc3339() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
        let t = now();
        assert_eq!(t.len(), 24, "{t}");
    }
}
