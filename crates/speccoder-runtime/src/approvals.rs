//! A person's approvals and rejections.
//!
//! When an attempt stops for approval, its pending call is saved with the
//! attempt. `speccoder approve` turns that call's class — the path
//! of a write, the exact argv of a command — into an approval for the task;
//! `speccoder reject` into a denial with a reason the model is told. The
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
    /// A person's notes for the task's next attempts (`speccoder nudge`).
    #[serde(default)]
    pub nudges: Vec<String>,
    /// Every decision, in order, with who made it.
    #[serde(default)]
    pub decided: Vec<Decided>,
}

/// Who decided, and through what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    /// The person: the operating-system user, or `unknown`.
    pub name: String,
    /// `terminal`, `editor <name>` or `cli`.
    pub via: String,
}

impl Who {
    /// The local person, deciding through `via`.
    #[must_use]
    pub fn local(via: &str) -> Self {
        Self {
            name: who(),
            via: via.to_owned(),
        }
    }

    /// One line: `hupe (terminal)`.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} ({})", self.name, self.via)
    }
}

/// The local person's name — the operating-system user; `unknown` when
/// there is none, never a guess.
#[must_use]
pub fn who() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
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

pub use present::{PresentContext, Presentation, present};

/// A call that needs a person: the identity of the journaled question, so
/// only what the journal already holds goes in (the presentation — the diff,
/// the sandbox line — is built live from it, F-054).
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
    /// End the task here; the call waits for `speccoder approve`.
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

// ── `speccoder approvals` / `approve` / `reject` ────────────────────────────

/// The feature, its repository root and its state directory.
fn locate(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    state: &Path,
) -> Result<(String, std::path::PathBuf, std::path::PathBuf), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::new(
            command,
            inspected.status(),
            inspected.message(),
        )));
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
    let config = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let sandbox = crate::sandbox::parse(&config).ok();
    let p = present(
        &request,
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
        }));
        out.extend(d.rejected.iter().map(|r| DecidedCall {
            task: task.to_owned(),
            decision: "rejected".to_owned(),
            covers: r.grant.describe(),
            reason: Some(r.reason.clone()),
            by: last(&r.grant).map(|x| x.by.describe()),
            scope: Some("task".to_owned()),
            note: None,
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
                }),
        );
    }
    out
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
    let waiting = pending(&root, &feature, &open, &decisions);

    let message = match verb {
        Verb::List => {
            if waiting.is_empty() {
                "nothing awaits a decision".to_owned()
            } else {
                format!("{} call(s) await a decision", waiting.len())
            }
        }
        Verb::Approve(task) | Verb::ApproveNoted(task, _) | Verb::Reject(task, _) => {
            let Some((call, grant)) = waiting.iter().find(|(c, _)| c.task == task) else {
                return CommandResult::new(
                    command,
                    CommandStatus::NotFound,
                    &format!("{task} has no call awaiting a decision (see `speccoder approvals`)"),
                );
            };
            let what = match verb {
                Verb::Reject(_, reason) => {
                    decisions.reject(&feature, task, grant.clone(), reason, &Who::local("cli"));
                    format!(
                        "rejected: {} for {task} — the model will be told: {reason}",
                        call.covers
                    )
                }
                _ => {
                    decisions.approve(&feature, task, grant.clone(), &Who::local("cli"));
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
        pending: pending(&root, &feature, &open, &decisions)
            .into_iter()
            .map(|(c, _)| c)
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
        ),
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
