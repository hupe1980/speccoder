//! A person's approvals and rejections.
//!
//! When an attempt stops for approval, its pending call is saved with the
//! attempt. `speccoder task approve` turns that call's class — the path
//! of a write, the exact argv of a command — into an approval for the task;
//! `speccoder task reject` into a denial with a reason the model is told. The
//! next attempt carries the task's decisions in its journaled input, so
//! strict replay needs nothing else. Standing approvals for commands are
//! `[commands] allow` in `speccoder.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use speccoder_core::{
    ApprovalsReport, CommandData, CommandResult, CommandStatus, DecidedCall, PendingCall,
};

use crate::tools::policy::{Grant, Rejection};

/// The decisions file in a project's state directory.
pub const FILE: &str = "approvals.json";

/// One task's decisions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDecisions {
    /// Approved classes.
    #[serde(default)]
    pub approved: Vec<Grant>,
    /// Rejected classes with reasons.
    #[serde(default)]
    pub rejected: Vec<Rejection>,
    /// A person's notes for the task's next attempts (`speccoder task nudge`).
    #[serde(default)]
    pub nudges: Vec<String>,
    /// Every decision, in order, with who made it.
    #[serde(default)]
    pub decided: Vec<Decided>,
}

/// Who decided, through what, and how speccoder knows the name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    /// The person (see [`who`]).
    pub name: String,
    /// `terminal`, `editor <name>` or `cli`.
    pub via: String,
    /// `asserted` (typed at a local terminal or the CLI) or `connected`
    /// (reported by a client over its connection). speccoder authenticates
    /// nobody, so never `authenticated`.
    #[serde(default = "asserted")]
    pub basis: String,
}

fn asserted() -> String {
    "asserted".to_owned()
}

impl Who {
    /// The local person, deciding through `via`: over an editor's
    /// connection the name is the connection's (`connected`), otherwise
    /// the person's own (`asserted`).
    #[must_use]
    pub fn local(via: &str) -> Self {
        Self::local_in(Path::new("."), via)
    }

    /// [`Who::local`], named as git names the person in `dir` (the project).
    #[must_use]
    pub fn local_in(dir: &Path, via: &str) -> Self {
        Self {
            name: who_in(dir),
            via: via.to_owned(),
            basis: if via.starts_with("editor") {
                "connected"
            } else {
                "asserted"
            }
            .to_owned(),
        }
    }

    /// One line: `hupe (terminal)`.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} ({})", self.name, self.via)
    }

    /// agentplane's operator: the name and its basis.
    #[must_use]
    pub fn operator(&self) -> Option<agentplane::core::Operator> {
        agentplane::core::Operator::from_parts(&self.name, &self.basis).ok()
    }

    /// The operator as reports show it.
    #[must_use]
    pub fn view(&self) -> speccoder_core::OperatorView {
        speccoder_core::OperatorView {
            actor: self.name.clone(),
            basis: self.basis.clone(),
        }
    }
}

/// The local person's name, the same for every decision and attestation:
/// git's `user.name` (what their commits say), else the operating-system
/// user, else `unknown` — never a guess.
#[must_use]
pub fn who() -> String {
    who_in(Path::new("."))
}

/// [`who`], reading git's configuration as it applies in `dir`.
#[must_use]
pub fn who_in(dir: &Path) -> String {
    crate::git::Git::at(dir)
        .line(&["config", "user.name"])
        .ok()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| {
            ["USER", "USERNAME", "LOGNAME"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// One decision, as it was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    /// `approve` or `reject`.
    pub verb: String,
    /// The call class.
    pub grant: Grant,
    /// A rejection's reason.
    #[serde(default)]
    pub reason: Option<String>,
    /// How far an approval reaches: `once`, `task` or `run` (a rejection
    /// holds for the task).
    #[serde(default = "task_scope")]
    pub scope: String,
    /// An approval's note to the model.
    #[serde(default)]
    pub note: Option<String>,
    /// Who decided.
    pub by: Who,
    /// Seconds since the epoch.
    pub at: u64,
}

fn task_scope() -> String {
    "task".to_owned()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// All decisions, by `feature/task`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decisions {
    /// By `feature/task`.
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskDecisions>,
}

impl Decisions {
    /// Reads the decisions in `dir`; none when there is no file yet.
    pub fn load(dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(dir.join(FILE)) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{}: {e}", dir.join(FILE).display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", dir.join(FILE).display())),
        }
    }

    /// Writes the decisions to `dir` (temporary file, then rename).
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dir.join(FILE)).map_err(|e| format!("{}: {e}", dir.display()))
    }

    /// The decisions for `feature/task`.
    #[must_use]
    pub fn for_task(&self, feature: &str, task: &str) -> TaskDecisions {
        self.tasks
            .get(&format!("{feature}/{task}"))
            .cloned()
            .unwrap_or_default()
    }

    /// Records an approval; a rejection of the same class is withdrawn.
    pub fn approve(&mut self, feature: &str, task: &str, grant: Grant, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.rejected.retain(|r| r.grant != grant);
        if !d.approved.contains(&grant) {
            d.approved.push(grant.clone());
        }
        d.decided.push(Decided {
            verb: "approve".to_owned(),
            grant,
            reason: None,
            scope: task_scope(),
            note: None,
            by: by.clone(),
            at: now(),
        });
    }

    /// Records a decision that changes nothing for the task (an inline
    /// approval or rejection of one call): the history only.
    pub fn note(&mut self, feature: &str, task: &str, decided: Decided) {
        self.tasks
            .entry(format!("{feature}/{task}"))
            .or_default()
            .decided
            .push(decided);
    }

    /// Records a rejection; an approval of the same class is withdrawn.
    pub fn reject(&mut self, feature: &str, task: &str, grant: Grant, reason: &str, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.approved.retain(|g| *g != grant);
        d.rejected.retain(|r| r.grant != grant);
        d.rejected.push(Rejection {
            grant: grant.clone(),
            reason: reason.to_owned(),
        });
        d.decided.push(Decided {
            verb: "reject".to_owned(),
            grant,
            reason: Some(reason.to_owned()),
            scope: task_scope(),
            note: None,
            by: by.clone(),
            at: now(),
        });
    }

    /// Whether `grant` was decided for `feature/task` either way.
    #[must_use]
    pub fn decided(&self, feature: &str, task: &str, grant: &Grant) -> bool {
        let d = self.for_task(feature, task);
        d.approved.contains(grant) || d.rejected.iter().any(|r| &r.grant == grant)
    }
}

// ── Inline approvals ────────────────────────────────────────────────

pub mod present;
pub mod tasks;

pub use present::{PresentContext, Presentation, present};

/// A call that needs a person: the identity of the journaled question, so
/// only what the journal already holds goes in (the presentation — the diff,
/// the sandbox line — is built live from it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// `feature/task`.
    pub task: String,
    /// The task's title.
    #[serde(default)]
    pub task_title: String,
    /// The model turn (1-based).
    #[serde(default)]
    pub turn: u32,
    /// The tool.
    pub tool: String,
    /// Its arguments as the model sent them.
    pub arguments: serde_json::Value,
    /// `write`, `edit`, `run`, `network` or `mcp`.
    #[serde(default)]
    pub kind: String,
    /// Why it needs approval.
    pub reason: String,
    /// What a "for this task" answer would cover.
    pub covers: String,
    /// `high` (runs arbitrary code or uses the network), `medium` (changes
    /// dependencies, frozen tests or repository state; any MCP tool) or `low`.
    pub risk: String,
    /// What the model said in that turn: one line, at most 200 characters,
    /// control characters escaped — a claim, not a fact.
    #[serde(default)]
    pub claim: Option<String>,
    /// The choices offered, in order: of `once`, `task`, `run`, `reject`, `stop`.
    #[serde(default)]
    pub options: Vec<String>,
}

/// How far an approval reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// This call.
    Once,
    /// The call's class for the rest of the task.
    Task,
    /// The call's class in every task of this run.
    Run,
}

impl Scope {
    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Task => "task",
            Self::Run => "run",
        }
    }
}

/// What a person decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Run the call.
    Approve,
    /// Refuse it; the model is told why.
    Reject,
    /// End the task here; the call waits for `speccoder task approve`.
    Stop,
}

/// A person's answer ("later" is no answer: nothing is journaled until one exists).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Approve, reject or stop.
    pub decision: Decision,
    /// How far an approval reaches.
    pub scope: Scope,
    /// An approval's note to the model.
    #[serde(default)]
    pub note: Option<String>,
    /// A rejection's reason — the model is told.
    #[serde(default)]
    pub reason: Option<String>,
    /// Who answered; journaled with the answer.
    #[serde(default)]
    pub by: Option<Who>,
}

impl Answer {
    /// An approval with `scope` and an optional note to the model.
    #[must_use]
    pub fn approve(scope: Scope, note: Option<String>, by: Option<Who>) -> Self {
        Self {
            decision: Decision::Approve,
            scope,
            note: note.filter(|n| !n.trim().is_empty()),
            reason: None,
            by,
        }
    }

    /// A rejection; the model is told `reason`.
    #[must_use]
    pub fn reject(reason: impl Into<String>, by: Option<Who>) -> Self {
        Self {
            decision: Decision::Reject,
            scope: Scope::Once,
            note: None,
            reason: Some(reason.into()),
            by,
        }
    }

    /// Stop the task; the call waits.
    #[must_use]
    pub fn stop(by: Option<Who>) -> Self {
        Self {
            decision: Decision::Stop,
            scope: Scope::Once,
            note: None,
            reason: None,
            by,
        }
    }
}

/// Asks a person, synchronously, while an attempt waits (a terminal prompt
/// in the CLI, a permission request in an editor). The answer is journaled;
/// replay never asks again.
pub trait Approver: Send + Sync + std::fmt::Debug {
    /// The person's answer to the presented call.
    fn ask(&self, p: &Presentation) -> Answer;

    /// The person's free-text answer to a question a hosted skill asks
    ///; `None` when nobody can answer.
    fn answer(&self, question: &str) -> Option<String> {
        let _ = question;
        None
    }

    /// Through what the person answers: `terminal`, or `editor <name>`.
    fn via(&self) -> String {
        "terminal".to_owned()
    }
}

/// The kind of call, from the call alone (it is part of the journaled
/// question, so it never depends on the worktree): `write` (`write_file`),
/// `edit` (`edit_file`), `run`, `network` (a command asking for the network)
/// or `mcp`. Whether a write replaces a file is the presentation's to say.
#[must_use]
pub fn kind_of(tool: &str, arguments: &serde_json::Value) -> &'static str {
    match tool {
        "write_file" => "write",
        "edit_file" => "edit",
        "run_command" if arguments["network"] == true => "network",
        "run_command" => "run",
        t if t.starts_with(crate::mcp::PREFIX) => "mcp",
        _ => "edit",
    }
}

/// The choices offered: "for this run" never for high risk; neither "task"
/// nor "run" when the class is not shown in full.
#[must_use]
pub fn options(risk: &str, subject_complete: bool) -> Vec<String> {
    let mut o = vec!["once"];
    if subject_complete {
        o.push("task");
        if risk != "high" {
            o.push("run");
        }
    }
    o.extend(["reject", "stop"]);
    o.into_iter().map(str::to_owned).collect()
}

/// A model's text, one line, at most 200 characters, control characters
/// escaped: what it claimed in the turn that asked.
#[must_use]
pub fn claim(text: &str) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let line = present::escape(&line);
    if line.is_empty() {
        return None;
    }
    Some(if line.chars().count() > 200 {
        format!("{}…", line.chars().take(199).collect::<String>())
    } else {
        line
    })
}

/// The risk tier of a call that needs approval, from the policy's reason.
#[must_use]
pub fn risk(tool: &str, reason: &str) -> &'static str {
    let r = reason.to_lowercase();
    if tool == "run_command" && (r.contains("arbitrary") || r.contains("network")) {
        "high"
    } else if tool.starts_with(crate::mcp::PREFIX)
        || r.contains("manifest")
        || r.contains("frozen")
        || r.contains("repository state")
        || r.contains("recursive")
    {
        "medium"
    } else {
        "low"
    }
}

// ── `speccoder report --approvals` / `approve` / `reject` ────────────────────────────

/// The feature, its repository root and its state directory.
fn locate(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    state: &Path,
) -> Result<(String, std::path::PathBuf, std::path::PathBuf), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(command, &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = std::path::PathBuf::from(&snapshot.root);
    Ok((
        f.feature.name.clone(),
        crate::worktree::project_state_dir(state, &root),
        root,
    ))
}

/// Open tasks' latest attempts that stopped for approval and are undecided.
fn pending(
    root: &Path,
    feature: &str,
    open: &[String],
    decisions: &Decisions,
) -> Vec<(PendingCall, Grant)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for saved in crate::attempts::list(root, feature) {
        if !open.contains(&saved.task) || !seen.insert(saved.task.clone()) {
            continue; // newest first: only each open task's latest attempt
        }
        let Some(p) = saved.pending.filter(|_| saved.outcome == "needs_approval") else {
            continue;
        };
        let tool = p["tool"].as_str().unwrap_or_default().to_owned();
        let Some(grant) = Grant::of(&tool, &p["arguments"]) else {
            continue;
        };
        if decisions.decided(feature, &saved.task, &grant) {
            continue;
        }
        let presentation = view(root, feature, &saved.task, &saved.reference, &tool, &p);
        out.push((
            PendingCall {
                task: saved.task.clone(),
                covers: grant.describe(),
                tool,
                arguments: p["arguments"].clone(),
                reason: p["reason"].as_str().unwrap_or_default().to_owned(),
                reference: saved.reference.clone(),
                presentation: Some(presentation),
                source: "attempt".to_owned(),
                kind: "approval".to_owned(),
                ..PendingCall::default()
            },
            grant,
        ));
    }
    out
}

/// The saved call as the inline prompt would have shown it: the file as the
/// saved attempt left it, the project's sandbox, the model's claim.
fn view(
    root: &Path,
    feature: &str,
    task: &str,
    reference: &str,
    tool: &str,
    pending: &serde_json::Value,
) -> speccoder_core::ApprovalView {
    let arguments = pending["arguments"].clone();
    let reason = pending["reason"].as_str().unwrap_or_default().to_owned();
    let grant = Grant::of(tool, &arguments);
    let risk = risk(tool, &reason);
    let request = Request {
        task: format!("{feature}/{task}"),
        task_title: String::new(),
        turn: pending["turn"]
            .as_u64()
            .and_then(|t| u32::try_from(t).ok())
            .unwrap_or(0),
        tool: tool.to_owned(),
        kind: kind_of(tool, &arguments).to_owned(),
        covers: grant.as_ref().map(|g| g.describe()).unwrap_or_default(),
        risk: risk.to_owned(),
        claim: pending["claim"].as_str().map(str::to_owned),
        options: options(risk, grant.is_some()),
        arguments,
        reason,
    };
    let path = request.arguments["path"].as_str().unwrap_or_default();
    let file_text = (!path.is_empty())
        .then(|| {
            crate::git::Git::at(root)
                .run(&["show", &format!("{reference}:./{path}")])
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        })
        .flatten();
    presented(root, &request, task, file_text)
}

/// `request` as the inline prompt shows it, with the project's sandbox
/// and `file_text` (the file as the call would find it) for a write.
fn presented(
    root: &Path,
    request: &Request,
    task: &str,
    file_text: Option<String>,
) -> speccoder_core::ApprovalView {
    let config = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let sandbox = crate::sandbox::parse(&config).ok();
    let p = present(
        request,
        &PresentContext {
            root: None,
            sandbox: sandbox
                .as_ref()
                .filter(|c| c.mode != crate::sandbox::Mode::Off)
                .map(|_| {
                    if cfg!(target_os = "macos") {
                        "Seatbelt".to_owned()
                    } else {
                        "Landlock+seccomp".to_owned()
                    }
                }),
            network: match sandbox.map(|c| c.network) {
                Some(crate::sandbox::Network::Loopback) => "loopback",
                Some(crate::sandbox::Network::On) => "unrestricted",
                _ => "none",
            }
            .to_owned(),
            file_text,
        },
    );
    speccoder_core::ApprovalView {
        kind: request.kind.clone(),
        title: p.title,
        subject: p.subject,
        body_lines: p.body,
        sandbox: p.sandbox,
        claim: request.claim.clone(),
        risk: request.risk.clone(),
        scope: format!("for the rest of {task}: {}", p.scope),
    }
}

fn decided(feature: &str, decisions: &Decisions) -> Vec<DecidedCall> {
    let prefix = format!("{feature}/");
    let mut out = Vec::new();
    for (key, d) in &decisions.tasks {
        let Some(task) = key.strip_prefix(&prefix) else {
            continue;
        };
        let last = |g: &Grant| d.decided.iter().rev().find(|x| x.grant == *g);
        out.extend(d.approved.iter().map(|g| DecidedCall {
            task: task.to_owned(),
            decision: "approved".to_owned(),
            covers: g.describe(),
            reason: None,
            by: last(g).map(|x| x.by.describe()),
            scope: Some("task".to_owned()),
            note: last(g).and_then(|x| x.note.clone()),
            operator: last(g).map(|x| x.by.view()),
            via: last(g).map(|x| x.by.via.clone()),
            ..DecidedCall::default()
        }));
        out.extend(d.rejected.iter().map(|r| DecidedCall {
            task: task.to_owned(),
            decision: "rejected".to_owned(),
            covers: r.grant.describe(),
            reason: Some(r.reason.clone()),
            by: last(&r.grant).map(|x| x.by.describe()),
            scope: Some("task".to_owned()),
            note: None,
            operator: last(&r.grant).map(|x| x.by.view()),
            via: last(&r.grant).map(|x| x.by.via.clone()),
            ..DecidedCall::default()
        }));
        // "For this run" decisions live in the history only.
        out.extend(
            d.decided
                .iter()
                .filter(|x| x.verb == "approve" && x.scope == "run")
                .map(|x| DecidedCall {
                    task: task.to_owned(),
                    decision: "approved".to_owned(),
                    covers: x.grant.describe(),
                    reason: None,
                    by: Some(x.by.describe()),
                    scope: Some("run".to_owned()),
                    note: x.note.clone(),
                    operator: Some(x.by.view()),
                    via: Some(x.by.via.clone()),
                    ..DecidedCall::default()
                }),
        );
    }
    out
}

/// Calls of `feature` waiting for a person on a human task, oldest
/// first: the attempt is suspended and resumes once they are decided.
fn waiting_tasks(
    root: &Path,
    dir: &Path,
    feature: &str,
) -> Vec<(PendingCall, agentplane::core::Task)> {
    let Ok(rt) = crate::tokio_runtime() else {
        return Vec::new();
    };
    let Ok(plane) = crate::substrate::Plane::open_for_decisions(dir) else {
        return Vec::new();
    };
    // A call past its deadline is denied as expired before it is listed.
    let _ = rt.block_on(plane.expire_due(agentplane::core::Timestamp::now_utc()));
    let open = rt.block_on(plane.open_tasks()).unwrap_or_default();
    let prefix = format!("{feature}/");
    open.into_iter()
        .filter(|t| t.kind == tasks::APPROVAL)
        .filter_map(|t| {
            let request = tasks::request_of(&t.justification.proposed_action)?;
            let task = request.task.strip_prefix(&prefix)?.to_owned();
            let view = presented(root, &request, &task, None);
            Some((
                PendingCall {
                    task,
                    covers: request.covers.clone(),
                    tool: request.tool.clone(),
                    arguments: request.arguments.clone(),
                    reason: request.reason.clone(),
                    reference: String::new(),
                    presentation: Some(view),
                    call: t.id.to_hex(),
                    source: "task".to_owned(),
                    kind: tasks::APPROVAL.to_owned(),
                    question: None,
                    waiting_since: Some(t.created_at.unix_timestamp().max(0).unsigned_abs()),
                    deadline: t.due_at.map(|d| d.unix_timestamp().max(0).unsigned_abs()),
                },
                t,
            ))
        })
        .collect()
}

/// Denies, as expired, every call in the journal at `dir` whose deadline
/// passed at `now` (nobody's decision); their attempts read the refusal
/// when they next resume.
pub fn expire_due(dir: &Path, now: agentplane::core::Timestamp) -> Result<(), String> {
    let rt = crate::tokio_runtime()?;
    let plane = crate::substrate::Plane::open_for_decisions(dir).map_err(|e| e.0)?;
    rt.block_on(plane.expire_due(now)).map_err(|e| e.0)
}

/// Another task of `feature` whose attempt waits for a person, with what
/// it waits on: its attempt holds the feature's worktree until it is
/// decided and resumed.
#[must_use]
pub fn waiting_elsewhere(
    root: &Path,
    dir: &Path,
    feature: &str,
    task: &str,
) -> Option<(String, String)> {
    waiting_tasks(root, dir, feature)
        .into_iter()
        .map(|(c, _)| c)
        .find(|c| c.task != task && c.source == "task")
        .map(|c| (c.task, c.covers))
}

/// Withdraws every call of `feature` waiting on a person: their attempts
/// were set aside (`--return`, changed intent), so nothing resumes them.
pub fn withdraw_waiting(
    root: &Path,
    dir: &Path,
    feature: &str,
    reason: &str,
) -> Result<usize, String> {
    let rows = waiting_tasks(root, dir, feature);
    if rows.is_empty() {
        return Ok(0);
    }
    let rt = crate::tokio_runtime()?;
    let plane = crate::substrate::Plane::open_for_decisions(dir).map_err(|e| e.0)?;
    let by = Who::local_in(root, "cli")
        .operator()
        .ok_or_else(|| "no name to record the withdrawal under".to_owned())?;
    for (_, task) in &rows {
        rt.block_on(plane.withdraw(task, &by, reason))
            .map_err(|e| e.0)?;
    }
    Ok(rows.len())
}

/// Records `answer` on the waiting call `task` through a plane without the
/// attempt's skill: the run continues when it next resumes.
fn decide_waiting(
    dir: &Path,
    task: &agentplane::core::Task,
    answer: &Answer,
    by: &Who,
) -> Result<(), String> {
    let rt = crate::tokio_runtime()?;
    let plane = crate::substrate::Plane::open_for_decisions(dir).map_err(|e| e.0)?;
    let decision = tasks::decision_for(answer, by, None)
        .ok_or_else(|| format!("`{}` is not a name a decision can carry", by.name))?;
    rt.block_on(plane.decide(task.id, &decision))
        .map(|_| ())
        .map_err(|e| e.0)
}

/// What to do with a pending call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb<'a> {
    /// Only list.
    List,
    /// Approve the task's pending call.
    Approve(&'a str),
    /// Approve it with a note the next attempt is told.
    ApproveNoted(&'a str, &'a str),
    /// Reject the task's pending call, with a reason.
    Reject(&'a str, &'a str),
    /// Decide waiting calls as `speccoder task approve|reject` options say.
    Decide(Choice<'a>),
}

/// Which waiting calls to decide, and how (`approve`/`reject` options).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice<'a> {
    /// The task's calls (`--task`); all of the feature's with `--all` alone.
    pub task: Option<&'a str>,
    /// One call by its id or a prefix of it (`--call`).
    pub call: Option<&'a str>,
    /// Every matching call (`--all`), each decided and recorded on its own.
    pub all: bool,
    /// Approve, or reject.
    pub approve: bool,
    /// How far an approval reaches (`--scope`, default `task`).
    pub scope: Scope,
    /// An approval's note to the model.
    pub note: Option<&'a str>,
    /// A rejection's reason, which the model is told.
    pub reason: Option<&'a str>,
}

impl<'a> Choice<'a> {
    /// Approve the task's waiting call (for the task), with an optional note.
    #[must_use]
    pub fn approve(task: &'a str, note: Option<&'a str>) -> Self {
        Self {
            task: Some(task),
            call: None,
            all: false,
            approve: true,
            scope: Scope::Task,
            note,
            reason: None,
        }
    }

    /// Reject the task's waiting call; the model is told `reason`.
    #[must_use]
    pub fn reject(task: &'a str, reason: &'a str) -> Self {
        Self {
            task: Some(task),
            call: None,
            all: false,
            approve: false,
            scope: Scope::Task,
            note: None,
            reason: Some(reason),
        }
    }
}

/// `approvals`, `approve --task` and `reject --task --reason` against `state`.
pub fn decide_with(
    path: &Path,
    feature: Option<&str>,
    verb: Verb<'_>,
    state: &Path,
) -> CommandResult {
    let command = match verb {
        Verb::List => "approvals",
        Verb::Approve(_) | Verb::ApproveNoted(..) => "approve",
        Verb::Reject(..) => "reject",
        Verb::Decide(c) if c.approve => "approve",
        Verb::Decide(_) => "reject",
    };
    let (feature, dir, root) = match locate(command, path, feature, state) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let mut decisions = match Decisions::load(&dir) {
        Ok(d) => d,
        Err(e) => return CommandResult::new(command, CommandStatus::ConfigError, &e),
    };
    // Open tasks, from the speccoder branch when there is one.
    let status = crate::status::report(&root, Some(&feature));
    let open: Vec<String> = match status.data() {
        Some(CommandData::Report(r)) => r
            .tasks
            .iter()
            .filter(|t| matches!(t.state.as_str(), "ready" | "blocked"))
            .map(|t| t.id.clone())
            .collect(),
        _ => Vec::new(),
    };
    let tasks_waiting = waiting_tasks(&root, &dir, &feature);
    let waiting = pending(&root, &feature, &open, &decisions);

    // A call waiting on a task is decided there: who, how known, bound to
    // what was shown. Its attempt continues on the next run.
    let choice = match verb {
        Verb::List => None,
        Verb::Approve(t) => Some(Choice::approve(t, None)),
        Verb::ApproveNoted(t, n) => Some(Choice::approve(t, Some(n))),
        Verb::Reject(t, r) => Some(Choice::reject(t, r)),
        Verb::Decide(c) => Some(c),
    };
    if let Some(c) = choice {
        let matching: Vec<&(PendingCall, agentplane::core::Task)> = tasks_waiting
            .iter()
            .filter(|(p, _)| c.task.is_none_or(|t| p.task == t))
            .filter(|(p, _)| {
                c.call
                    .is_none_or(|id| !id.is_empty() && p.call.starts_with(id))
            })
            .collect();
        if matching.len() > 1 && !c.all {
            let list: Vec<String> = matching
                .iter()
                .map(|(p, _)| {
                    format!(
                        "{} ({}, {})",
                        &p.call[..12.min(p.call.len())],
                        p.task,
                        p.covers
                    )
                })
                .collect();
            return CommandResult::new(
                command,
                CommandStatus::UsageError,
                &format!(
                    "{} calls are waiting; name one with --call, or decide them all with --all: {}",
                    matching.len(),
                    list.join(", ")
                ),
            );
        }
        if !matching.is_empty() {
            let by = Who::local_in(&root, "cli");
            let mut lines = Vec::new();
            for (call, row) in &matching {
                let answer = if c.approve {
                    Answer::approve(c.scope, c.note.map(str::to_owned), Some(by.clone()))
                } else {
                    Answer::reject(c.reason.unwrap_or("no reason given"), Some(by.clone()))
                };
                if let Err(e) = decide_waiting(&dir, row, &answer, &by) {
                    return CommandResult::new(command, CommandStatus::Error, &e);
                }
                lines.push(if c.approve {
                    format!(
                        "approved: {} for {} ({}) — by {} ({})",
                        call.covers,
                        call.task,
                        c.scope.name(),
                        by.name,
                        by.basis
                    )
                } else {
                    format!(
                        "rejected: {} for {} — by {} ({}); the model will be told: {}",
                        call.covers,
                        call.task,
                        by.name,
                        by.basis,
                        c.reason.unwrap_or("no reason given")
                    )
                });
            }
            let report = ApprovalsReport {
                decided: decided(&feature, &decisions),
                pending: waiting_tasks(&root, &dir, &feature)
                    .into_iter()
                    .map(|(c, _)| c)
                    .chain(
                        pending(&root, &feature, &open, &decisions)
                            .into_iter()
                            .map(|(c, _)| c),
                    )
                    .collect(),
                feature,
            };
            return CommandResult::new(
                command,
                CommandStatus::Ok,
                &format!(
                    "{}; `speccoder run` continues each attempt where it stopped",
                    lines.join("\n")
                ),
            )
            .with_data(CommandData::Approvals(Box::new(report)));
        }
        if c.all || c.call.is_some() || c.task.is_none() {
            return CommandResult::new(
                command,
                CommandStatus::NotFound,
                "no call matching that awaits a decision (see `speccoder report --approvals`)",
            );
        }
    }
    // In a parallel lane, a stopped attempt is saved with its call:
    // deciding it records a grant the task's next attempt starts with.
    let verb = match verb {
        Verb::Decide(c) => match (c.task, c.approve, c.note) {
            (Some(t), true, Some(n)) => Verb::ApproveNoted(t, n),
            (Some(t), true, None) => Verb::Approve(t),
            (Some(t), false, _) => Verb::Reject(t, c.reason.unwrap_or("no reason given")),
            (None, ..) => Verb::List,
        },
        v => v,
    };

    let message = match verb {
        // Mapped onto the verbs above before this point.
        Verb::List | Verb::Decide(_) => {
            let n = waiting.len() + tasks_waiting.len();
            if n == 0 {
                "nothing awaits a decision".to_owned()
            } else {
                format!("{n} call(s) await a decision")
            }
        }
        Verb::Approve(task) | Verb::ApproveNoted(task, _) | Verb::Reject(task, _) => {
            let Some((call, grant)) = waiting.iter().find(|(c, _)| c.task == task) else {
                return CommandResult::new(
                    command,
                    CommandStatus::NotFound,
                    &format!(
                        "{task} has no call awaiting a decision (see `speccoder report --approvals`)"
                    ),
                );
            };
            let what = match verb {
                Verb::Reject(_, reason) => {
                    decisions.reject(
                        &feature,
                        task,
                        grant.clone(),
                        reason,
                        &Who::local_in(&root, "cli"),
                    );
                    format!(
                        "rejected: {} for {task} — the model will be told: {reason}",
                        call.covers
                    )
                }
                _ => {
                    decisions.approve(&feature, task, grant.clone(), &Who::local_in(&root, "cli"));
                    if let Verb::ApproveNoted(_, note) = verb
                        && !note.trim().is_empty()
                    {
                        let t = decisions
                            .tasks
                            .entry(format!("{feature}/{task}"))
                            .or_default();
                        if let Some(last) = t.decided.last_mut() {
                            last.note = Some(note.trim().to_owned());
                        }
                        // The next attempt is told, beside the approval.
                        t.nudges.push(format!(
                            "A person approved {}: {}",
                            call.covers,
                            note.trim()
                        ));
                    }
                    format!("approved: {} for {task}", call.covers)
                }
            };
            if let Err(e) = decisions.save(&dir) {
                return CommandResult::new(command, CommandStatus::Error, &e);
            }
            format!("{what}; run the task again to continue (speccoder run --task {task})")
        }
    };
    let report = ApprovalsReport {
        decided: decided(&feature, &decisions),
        pending: tasks_waiting
            .into_iter()
            .map(|(c, _)| c)
            .chain(
                pending(&root, &feature, &open, &decisions)
                    .into_iter()
                    .map(|(c, _)| c),
            )
            .collect(),
        feature,
    };
    CommandResult::new(command, CommandStatus::Ok, &message)
        .with_data(CommandData::Approvals(Box::new(report)))
}

/// [`decide_with`] in the default state directory.
#[must_use]
pub fn decide(path: &Path, feature: Option<&str>, verb: Verb<'_>) -> CommandResult {
    match crate::worktree::state_dir() {
        Some(state) => decide_with(path, feature, verb, &state),
        None => CommandResult::new(
            "approvals",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_round_trip_and_replace_each_other() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let g = Grant::Write {
            path: "Cargo.toml".to_owned(),
        };
        let mut d = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let by = Who::local("cli");
        d.reject("f", "T1", g.clone(), "no new dependencies", &by);
        d.approve("f", "T1", g.clone(), &by);
        assert_eq!(d.tasks["f/T1"].decided.len(), 2);
        assert_eq!(d.tasks["f/T1"].decided[1].by, by);
        d.save(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let back = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.for_task("f", "T1").approved, std::slice::from_ref(&g));
        assert!(back.for_task("f", "T1").rejected.is_empty());
        assert!(back.decided("f", "T1", &g) && !back.decided("f", "T2", &g));
    }
}
