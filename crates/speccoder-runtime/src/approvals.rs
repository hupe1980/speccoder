//! A person's approvals and rejections.
//!
//! When an attempt stops for approval, its pending call is saved with the
//! attempt. `speccoder task approve` turns that call's resolved object —
//! the canonical file of a write, the exact argv of a command with its
//! directory and executable, an MCP tool with its definition's digest —
//! into an approval for the task, bound to the run it was made in (a grant
//! never outlives its run); `speccoder task reject` into a denial with a
//! reason the model is told. The next attempt carries the task's decisions
//! in its journaled input, so strict replay needs nothing else. Standing
//! approvals for commands are `[commands] allow` in `speccoder.toml`
//! (exact argvs), committed by the person.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use speccoder_core::actions::Via;
use speccoder_core::{
    ApprovalsReport, CommandData, CommandResult, CommandStatus, DecidedCall, PendingCall,
};

use crate::tools::policy::{Object, Rejection};

/// The decisions file in a project's state directory.
pub const FILE: &str = "approvals.json";

/// An approved object and the run it belongs to: it covers calls of that
/// run only. `run` is `None` for a decision on a call a parallel lane
/// stopped (its run ended): the next run that attempts the task claims it
/// ([`Decisions::claim`]), and it ends with that run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunGrant {
    /// What it covers.
    pub object: Object,
    /// The run (`run-<secs>-<hex>`), once claimed.
    #[serde(default)]
    pub run: Option<String>,
    /// It covers one call: the next attempt of the task in its run is
    /// given it ([`Decisions::take_once`]) and may make the call one time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once: bool,
}

/// One task's decisions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDecisions {
    /// Approved objects, each with its run.
    #[serde(default)]
    pub approved: Vec<RunGrant>,
    /// Rejected objects with reasons (a rejection holds for the task).
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
#[serde(from = "WhoRecord")]
pub struct Who {
    /// The person (see [`who`]).
    pub name: String,
    /// The interface ([`Via::as_str`]): `terminal`, `web`, `acp`, …
    pub via: String,
    /// The editor's name, over the Agent Client Protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// `asserted` (typed at a local terminal or the CLI) or `connected`
    /// (reported by a client over its connection). speccoder authenticates
    /// nobody, so never `authenticated`.
    pub basis: String,
}

/// [`Who`] as recorded, by this version or an earlier one (`cli`,
/// `dashboard`, `editor <name>`).
#[derive(Deserialize)]
struct WhoRecord {
    name: String,
    via: String,
    #[serde(default)]
    client: Option<String>,
    #[serde(default = "asserted")]
    basis: String,
}

impl From<WhoRecord> for Who {
    fn from(r: WhoRecord) -> Self {
        let (via, client) = match Via::read(&r.via) {
            Some((v, named)) => (v.as_str().to_owned(), named.or(r.client)),
            None => (r.via, r.client),
        };
        Self {
            name: r.name,
            via,
            client,
            basis: r.basis,
        }
    }
}

fn asserted() -> String {
    "asserted".to_owned()
}

impl Who {
    /// The local person, deciding through `via`: the name is the person's
    /// own (`asserted`).
    #[must_use]
    pub fn local(via: Via) -> Self {
        Self::local_in(Path::new("."), via)
    }

    /// [`Who::local`], named as git names the person in `dir` (the project).
    #[must_use]
    pub fn local_in(dir: &Path, via: Via) -> Self {
        Self {
            name: who_in(dir),
            via: via.as_str().to_owned(),
            client: None,
            basis: "asserted".to_owned(),
        }
    }

    /// The local person, deciding in the editor `client` over its
    /// connection (`acp`): the name is the connection's (`connected`).
    #[must_use]
    pub fn connected(client: &str) -> Self {
        Self {
            name: who(),
            via: Via::Acp.as_str().to_owned(),
            client: Some(client.to_owned()),
            basis: "connected".to_owned(),
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
#[serde(deny_unknown_fields)]
pub struct Decided {
    /// `approve`, `reject` or `resume_asking` (a run's asking budget
    /// started again).
    pub verb: String,
    /// The object decided on; none for `resume_asking`.
    #[serde(default)]
    pub object: Option<Object>,
    /// The run it was made in.
    #[serde(default)]
    pub run: Option<String>,
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
    /// Reads the decisions in `dir`; none when there is no file yet. A file
    /// in an earlier shape (grants by class, not object) is an error naming
    /// it: reading it as "no decisions" would drop its rejections.
    pub fn load(dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(dir.join(FILE)) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                format!(
                    "{}: {e} — written before decisions named objects; remove it to start afresh (its decisions are not carried over)",
                    dir.join(FILE).display()
                )
            }),
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

    /// The objects approved for `feature/task` in `run`: never another
    /// run's (a grant ends with its run).
    #[must_use]
    pub fn for_run(&self, feature: &str, task: &str, run: &str) -> Vec<Object> {
        self.for_task(feature, task)
            .approved
            .into_iter()
            .filter(|g| !g.once && g.run.as_deref() == Some(run))
            .map(|g| g.object)
            .collect()
    }

    /// Removes and returns the once-grants of `feature/task` claimed by
    /// `run`: the attempt they are handed to may make each call one time,
    /// and no later attempt has them.
    pub fn take_once(&mut self, feature: &str, task: &str, run: &str) -> Vec<Object> {
        let Some(d) = self.tasks.get_mut(&format!("{feature}/{task}")) else {
            return Vec::new();
        };
        let (once, kept): (Vec<RunGrant>, Vec<RunGrant>) = std::mem::take(&mut d.approved)
            .into_iter()
            .partition(|g| g.once && g.run.as_deref() == Some(run));
        d.approved = kept;
        once.into_iter().map(|g| g.object).collect()
    }

    /// Records an approval of `object` for one call of the task's next
    /// attempt (a call a lane stopped, decided in its run's batch); a
    /// rejection of the same object is withdrawn.
    pub fn approve_once(&mut self, feature: &str, task: &str, object: Object, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.rejected.retain(|r| r.object != object);
        let grant = RunGrant {
            object: object.clone(),
            run: None,
            once: true,
        };
        if !d.approved.contains(&grant) {
            d.approved.push(grant);
        }
        d.decided.push(Decided {
            verb: "approve".to_owned(),
            object: Some(object),
            run: None,
            reason: None,
            scope: "once".to_owned(),
            note: None,
            by: by.clone(),
            at: now(),
        });
    }

    /// Binds the task's unclaimed grants (decisions on a call a lane
    /// stopped) to `run`, the run that now attempts it; whether any was.
    pub fn claim(&mut self, feature: &str, task: &str, run: &str) -> bool {
        let Some(d) = self.tasks.get_mut(&format!("{feature}/{task}")) else {
            return false;
        };
        let mut claimed = false;
        for g in d.approved.iter_mut().filter(|g| g.run.is_none()) {
            g.run = Some(run.to_owned());
            claimed = true;
        }
        claimed
    }

    /// Records an approval of `object` in `run`; a rejection of the same
    /// object is withdrawn.
    pub fn approve(
        &mut self,
        feature: &str,
        task: &str,
        object: Object,
        run: Option<&str>,
        by: &Who,
    ) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.rejected.retain(|r| r.object != object);
        let grant = RunGrant {
            object: object.clone(),
            run: run.map(str::to_owned),
            once: false,
        };
        if !d.approved.contains(&grant) {
            d.approved.push(grant);
        }
        d.decided.push(Decided {
            verb: "approve".to_owned(),
            object: Some(object),
            run: run.map(str::to_owned),
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

    /// Records a rejection; an approval of the same object is withdrawn.
    pub fn reject(&mut self, feature: &str, task: &str, object: Object, reason: &str, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.approved.retain(|g| g.object != object);
        d.rejected.retain(|r| r.object != object);
        d.rejected.push(Rejection {
            object: object.clone(),
            reason: reason.to_owned(),
        });
        d.decided.push(Decided {
            verb: "reject".to_owned(),
            object: Some(object),
            run: None,
            reason: Some(reason.to_owned()),
            scope: task_scope(),
            note: None,
            by: by.clone(),
            at: now(),
        });
    }

    /// Whether `object` was decided for `feature/task` either way. An
    /// earlier run's grant does not count: it covers nothing now.
    #[must_use]
    pub fn decided(&self, feature: &str, task: &str, object: &Object) -> bool {
        let d = self.for_task(feature, task);
        d.approved
            .iter()
            .any(|g| g.object == *object && g.run.is_none())
            || d.rejected.iter().any(|r| r.object == *object)
    }
}

// ── Inline approvals ────────────────────────────────────────────────

pub mod coverage;
pub mod derived;
pub mod present;
pub mod tasks;

/// Calls answered Always Allow inline in this process, approved once
/// there: the one other state `actions::perform(AllowAlways)` accepts
/// besides a call still waiting.
static ANSWERED_ALWAYS: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Records that the waiting call `call` was approved once by an Always
/// Allow answer, here and now.
pub fn answered_always(call: &str) {
    if let Ok(mut set) = ANSWERED_ALWAYS.lock() {
        set.insert(call.to_owned());
    }
}

/// Whether `call` was approved by an Always Allow answer in this process;
/// it is forgotten (one policy write per answer).
pub(crate) fn take_answered_always(call: &str) -> bool {
    ANSWERED_ALWAYS
        .lock()
        .map(|mut set| set.remove(call))
        .unwrap_or(false)
}

pub use present::{PresentContext, Presentation, present};

/// An argument of a call found verbatim in content the attempt received
/// from a source other than the model, the person or speccoder: where it
/// came from, so the person can tell a planted command from the model's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The matched argument, escaped, at most 120 characters.
    pub argument: String,
    /// The read path (`README.md`), the tool (`tool://fs/run_command`,
    /// `tool://<server>/<tool>`), `speccoder://worktree` or
    /// `speccoder://verifier`.
    pub from: String,
    /// The model turn it arrived in; 0 for the attempt's prompt.
    pub turn: u32,
}

/// A call that needs a person: the identity of the journaled question, so
/// only what the journal already holds goes in (the presentation — the diff,
/// the sandbox line — is built live from it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The resolved object a decision covers; `None` only for a tool that
    /// has none.
    #[serde(default)]
    pub object: Option<Object>,
    /// What differs from an object of the same name a person approved
    /// earlier (`executable /usr/bin/make → /w/bin/make`).
    #[serde(default)]
    pub changed: Option<String>,
    /// `high` (runs arbitrary code or uses the network), `medium` (changes
    /// dependencies, frozen tests or repository state; any MCP tool) or `low`.
    pub risk: String,
    /// What the model said in that turn: one line, at most 200 characters,
    /// control characters escaped — a claim, not a fact.
    #[serde(default)]
    pub claim: Option<String>,
    /// Arguments copied from content the attempt read, with where from (at
    /// most five; `more` counts the rest).
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Sources beyond the five kept.
    #[serde(default)]
    pub more: u32,
    /// The choices offered, in order: of `once`, `task`, `run`, `always`,
    /// `reject`, `stop`.
    #[serde(default)]
    pub options: Vec<String>,
    /// The run (`run-<secs>-<hex>`) the call belongs to.
    #[serde(default)]
    pub run: Option<String>,
    /// Prompts shown in the run, this one included (0 when not prompted).
    #[serde(default)]
    pub asked: u32,
    /// The run's asking budget (`[approvals] budget`).
    #[serde(default)]
    pub budget: u32,
    /// The budget was used: the call waits in the run's batch, unprompted.
    #[serde(default)]
    pub batched: bool,
}

impl Request {
    /// What a "for this task" answer would cover, in one line.
    #[must_use]
    pub fn covers(&self) -> String {
        self.object
            .as_ref()
            .map_or_else(|| format!("call `{}`", self.tool), Object::describe)
    }
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
    /// "Always Allow": approved once, and the exact argv added to the
    /// project's allow list (the runtime does that, never the interface).
    #[serde(default)]
    pub always: bool,
    /// With a batch decision: start the run's asking budget again.
    #[serde(default)]
    pub resume_asking: bool,
}

impl Answer {
    /// Always Allow: this call once, and exactly its argv into
    /// `[commands] allow` (or `[sandbox] network_allow`).
    #[must_use]
    pub fn always(by: Option<Who>) -> Self {
        Self {
            always: true,
            ..Self::approve(Scope::Once, None, by)
        }
    }

    /// An approval with `scope` and an optional note to the model.
    #[must_use]
    pub fn approve(scope: Scope, note: Option<String>, by: Option<Who>) -> Self {
        Self {
            decision: Decision::Approve,
            scope,
            note: note.filter(|n| !n.trim().is_empty()),
            reason: None,
            by,
            always: false,
            resume_asking: false,
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
            always: false,
            resume_asking: false,
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
            always: false,
            resume_asking: false,
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

    /// Through what the person answers.
    fn via(&self) -> Via {
        Via::Terminal
    }

    /// The client the person answers in (an editor's name), if it says.
    fn client(&self) -> Option<String> {
        None
    }

    /// The person's decision on a bug's reproductions, bound to the digest
    /// shown; `None` when nobody can decide here (the bug waits in the
    /// inbox for `speccoder bug confirm`).
    fn confirm(&self, request: &ConfirmRequest) -> Option<ConfirmAnswer> {
        let _ = request;
        None
    }
}

/// One reproduction as a person is shown it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownReproduction {
    /// The statement it reproduces, with its words.
    pub statement: String,
    /// The statement's words.
    pub words: String,
    /// The test file.
    pub file: String,
    /// The test's name.
    pub test: String,
    /// How it failed on the untouched code (up to 20 lines).
    pub excerpt: String,
    /// The test's content.
    pub content: String,
}

/// A bug's reproductions, waiting for the person's one confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmRequest {
    /// `<unit>/<task>`.
    pub bug: String,
    /// The bug's title.
    pub title: String,
    /// What a confirmation is bound to.
    pub digest: String,
    /// The reproductions.
    pub reproductions: Vec<ShownReproduction>,
}

/// The person's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAnswer {
    /// They show the bug: freeze them and fix it.
    Confirm,
    /// They do not; the reason goes to the author's next try.
    Reject(String),
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

/// The choices offered, in the fixed order `once, task, run, always,
/// reject, stop`. Remembered options (`task`, `run`, `always`) only for a
/// call with a resolved object whose arguments were not copied from content
/// the attempt read (`derived`); "for this run" never for high risk;
/// "always" only for a command (`run`, `network`) whose argv is not high
/// risk without the network (`always_risk`: an exact allow-list entry is
/// committed, reviewed policy).
#[must_use]
pub fn options(
    risk: &str,
    has_object: bool,
    derived: bool,
    kind: &str,
    always_risk: &str,
) -> Vec<String> {
    let mut o = vec!["once"];
    if has_object && !derived {
        o.push("task");
        if risk != "high" {
            o.push("run");
        }
        if matches!(kind, "run" | "network") && always_risk != "high" {
            o.push("always");
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

/// The object as reports show it.
#[must_use]
pub fn object_view(object: &Object) -> speccoder_core::ObjectView {
    speccoder_core::ObjectView {
        class: object.class().as_str().to_owned(),
        short: object.short(),
        describe: object.describe(),
        lines: present::object_lines(Some(object), None),
    }
}

/// The question a saved (lane) attempt stopped on: the request it
/// recorded. An attempt saved before requests named objects has none.
fn saved_request(feature: &str, task: &str, pending: &serde_json::Value) -> Option<Request> {
    let mut r: Request = serde_json::from_value(pending["request"].clone()).ok()?;
    if r.task.is_empty() {
        r.task = format!("{feature}/{task}");
    }
    Some(r)
}

/// [`speccoder_core::PendingCall::digest`]: SHA-256 over the call's id
/// and the object a decision covers, as recorded.
#[must_use]
pub fn call_digest(id: &str, object: Option<&Object>) -> String {
    use sha2::{Digest as _, Sha256};
    let object = serde_json::to_string(&object).unwrap_or_default();
    Sha256::digest(format!("{id}\n{object}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `p` with its digest.
fn with_digest(mut p: PendingCall, object: Option<&Object>) -> PendingCall {
    p.digest = call_digest(&p.id(), object);
    p
}

/// Open tasks' latest attempts that stopped for approval and are undecided.
fn pending(
    root: &Path,
    feature: &str,
    open: &[String],
    decisions: &Decisions,
) -> Vec<(PendingCall, Object)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for saved in crate::attempts::list(root, feature) {
        if !open.contains(&saved.task) || !seen.insert(saved.task.clone()) {
            continue; // newest first: only each open task's latest attempt
        }
        let Some(p) = saved.pending.filter(|_| saved.outcome == "needs_approval") else {
            continue;
        };
        let Some(request) = saved_request(feature, &saved.task, &p) else {
            continue;
        };
        let Some(object) = request.object.clone() else {
            continue;
        };
        if decisions.decided(feature, &saved.task, &object) {
            continue;
        }
        let presentation = view(root, &saved.task, &saved.reference, &request);
        out.push((
            with_digest(
                PendingCall {
                    task: saved.task.clone(),
                    covers: object.describe(),
                    tool: request.tool.clone(),
                    arguments: request.arguments.clone(),
                    reason: request.reason.clone(),
                    reference: saved.reference.clone(),
                    presentation: Some(presentation),
                    source: "attempt".to_owned(),
                    kind: "approval".to_owned(),
                    object: Some(object_view(&object)),
                    sources: present::source_lines(&request),
                    run: request.run.clone(),
                    batched: request.batched,
                    ..PendingCall::default()
                },
                Some(&object),
            ),
            object,
        ));
    }
    out
}

/// The saved call as the inline prompt would have shown it: the file as the
/// saved attempt left it, the project's sandbox, the model's claim.
fn view(
    root: &Path,
    task: &str,
    reference: &str,
    request: &Request,
) -> speccoder_core::ApprovalView {
    let path = request.arguments["path"].as_str().unwrap_or_default();
    let file_text = (!path.is_empty())
        .then(|| {
            crate::git::Git::at(root)
                .run(&["show", &format!("{reference}:./{path}")])
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        })
        .flatten();
    presented(root, request, task, file_text)
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
        object: p.object,
        sources: p.sources,
        options: request.options.clone(),
    }
}

fn decided(feature: &str, decisions: &Decisions) -> Vec<DecidedCall> {
    let prefix = format!("{feature}/");
    let mut out = Vec::new();
    for (key, d) in &decisions.tasks {
        let Some(task) = key.strip_prefix(&prefix) else {
            continue;
        };
        let last = |o: &Object| {
            d.decided
                .iter()
                .rev()
                .find(|x| x.object.as_ref() == Some(o))
        };
        out.extend(d.approved.iter().map(|g| DecidedCall {
            task: task.to_owned(),
            decision: "approved".to_owned(),
            covers: g.object.describe(),
            reason: None,
            by: last(&g.object).map(|x| x.by.describe()),
            note: last(&g.object).and_then(|x| x.note.clone()),
            operator: last(&g.object).map(|x| x.by.view()),
            via: last(&g.object).map(|x| x.by.via.clone()),
            client: last(&g.object).and_then(|x| x.by.client.clone()),
            object: Some(object_view(&g.object)),
            run: g.run.clone(),
            scope: Some(if g.once { "once" } else { "task" }.to_owned()),
            ..DecidedCall::default()
        }));
        out.extend(d.rejected.iter().map(|r| DecidedCall {
            task: task.to_owned(),
            decision: "rejected".to_owned(),
            covers: r.object.describe(),
            reason: Some(r.reason.clone()),
            by: last(&r.object).map(|x| x.by.describe()),
            scope: Some("task".to_owned()),
            note: None,
            operator: last(&r.object).map(|x| x.by.view()),
            via: last(&r.object).map(|x| x.by.via.clone()),
            client: last(&r.object).and_then(|x| x.by.client.clone()),
            object: Some(object_view(&r.object)),
            ..DecidedCall::default()
        }));
        // "For this run" decisions live in the history only.
        out.extend(
            d.decided
                .iter()
                .filter(|x| x.verb == "approve" && x.scope == "run")
                .filter_map(|x| {
                    let object = x.object.as_ref()?;
                    Some(DecidedCall {
                        task: task.to_owned(),
                        decision: "approved".to_owned(),
                        covers: object.describe(),
                        reason: None,
                        by: Some(x.by.describe()),
                        scope: Some("run".to_owned()),
                        note: x.note.clone(),
                        operator: Some(x.by.view()),
                        via: Some(x.by.via.clone()),
                        client: x.by.client.clone(),
                        object: Some(object_view(object)),
                        run: x.run.clone(),
                        ..DecidedCall::default()
                    })
                }),
        );
    }
    out
}

/// Calls of `feature` waiting for a person on a human task, oldest
/// first: the attempt is suspended and resumes once they are decided.
///
/// `Err` says why they could not be read: an unreadable journal means the
/// calls waiting are unknown, never that none wait. A project without a
/// journal has none, and a read never creates one.
fn waiting_tasks(
    root: &Path,
    dir: &Path,
    feature: &str,
) -> Result<Vec<(PendingCall, agentplane::core::Task)>, String> {
    if !dir.join(crate::substrate::JOURNAL).exists() {
        return Ok(Vec::new());
    }
    let rt = crate::tokio_runtime()?;
    let plane = crate::substrate::Plane::open_for_decisions(dir).map_err(|e| e.0)?;
    // A call past its deadline is denied as expired before it is listed.
    let _ = rt.block_on(plane.expire_due(agentplane::core::Timestamp::now_utc()));
    let open = rt
        .block_on(plane.open_tasks())
        .map_err(|e| format!("listing the waiting calls: {}", e.0))?;
    let prefix = format!("{feature}/");
    Ok(open
        .into_iter()
        .filter(|t| t.kind == tasks::APPROVAL)
        .filter_map(|t| {
            let request = tasks::request_of(&t.justification.proposed_action)?;
            let task = request.task.strip_prefix(&prefix)?.to_owned();
            let view = presented(root, &request, &task, None);
            Some((
                with_digest(
                    PendingCall {
                        task,
                        covers: request.covers(),
                        object: request.object.as_ref().map(object_view),
                        sources: present::source_lines(&request),
                        run: request.run.clone(),
                        batched: request.batched,
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
                        digest: String::new(),
                    },
                    request.object.as_ref(),
                ),
                t,
            ))
        })
        .collect())
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
    // Unreadable: the run that would wait opens the journal itself and
    // stops there with the reason, so nothing is hidden by `None` here.
    waiting_tasks(root, dir, feature)
        .ok()?
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
    let rows = waiting_tasks(root, dir, feature)?;
    if rows.is_empty() {
        return Ok(0);
    }
    let rt = crate::tokio_runtime()?;
    let plane = crate::substrate::Plane::open_for_decisions(dir).map_err(|e| e.0)?;
    let by = agentplane::core::Operator::from_parts(who_in(root), "asserted")
        .ok()
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
    /// The digest of the call the person was shown
    /// ([`speccoder_core::PendingCall::digest`]): refused when the call
    /// that waits now has another. `None` from the command line, which
    /// names the call itself.
    pub digest: Option<&'a str>,
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
            digest: None,
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
            digest: None,
        }
    }

    /// The same, bound to the call shown: its id (a waiting call's, or
    /// `attempt:<task>`) and digest.
    #[must_use]
    pub fn shown(self, call: &'a str, digest: &'a str) -> Self {
        Self {
            call: (!call.starts_with("attempt:")).then_some(call),
            digest: Some(digest),
            ..self
        }
    }
}

/// `approvals` against `state`: the calls waiting and the decisions made.
pub fn list_with(path: &Path, feature: Option<&str>, state: &Path) -> CommandResult {
    // Listing records nothing, so through no interface in particular.
    decide_with(path, feature, Verb::List, state, Via::Terminal)
}

/// [`list_with`] in the default state directory.
#[must_use]
pub fn list(path: &Path, feature: Option<&str>) -> CommandResult {
    decide(path, feature, Verb::List, Via::Terminal)
}

/// `approvals`, `approve --task` and `reject --task --reason` against
/// `state`, decided through `via`.
pub fn decide_with(
    path: &Path,
    feature: Option<&str>,
    verb: Verb<'_>,
    state: &Path,
    via: Via,
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
    let (tasks_waiting, unreadable) = match waiting_tasks(&root, &dir, &feature) {
        Ok(rows) => (rows, None),
        Err(e) => (Vec::new(), Some(e)),
    };
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
    if let (Some(_), Some(why)) = (choice, &unreadable) {
        return CommandResult::new(
            command,
            CommandStatus::Error,
            &format!("the calls waiting for a decision could not be read: {why}"),
        );
    }
    if let Some(c) = choice {
        let matching: Vec<&(PendingCall, agentplane::core::Task)> = tasks_waiting
            .iter()
            .filter(|(p, _)| c.task.is_none_or(|t| p.task == t))
            .filter(|(p, _)| {
                c.call
                    .is_none_or(|id| !id.is_empty() && p.call.starts_with(id))
            })
            .collect();
        if let Some(shown) = c.digest
            && matching.iter().any(|(p, _)| p.digest != shown)
        {
            return CommandResult::new(
                command,
                CommandStatus::UsageError,
                "the call changed since it was shown; see `speccoder report --approvals` for the one waiting now",
            )
            .with_code("CALL-CHANGED");
        }
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
            let by = Who::local_in(&root, via);
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
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(c, _)| c)
                    .chain(
                        pending(&root, &feature, &open, &decisions)
                            .into_iter()
                            .map(|(c, _)| c),
                    )
                    .collect(),
                feature,
                unreadable: None,
                batch: Vec::new(),
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
    // deciding it records a grant of exactly that object, which the next
    // run that attempts the task claims (it ends with that run).
    let shown = match verb {
        Verb::Decide(c) => c.digest,
        _ => None,
    };
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
            match (&unreadable, n) {
                (Some(why), 0) => format!("the calls waiting for a decision are unknown: {why}"),
                (Some(why), n) => {
                    format!("{n} call(s) await a decision; others may wait unseen: {why}")
                }
                (None, 0) => "nothing awaits a decision".to_owned(),
                (None, n) => format!("{n} call(s) await a decision"),
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
            if shown.is_some_and(|d| d != call.digest) {
                return CommandResult::new(
                    command,
                    CommandStatus::UsageError,
                    "the call changed since it was shown; see `speccoder report --approvals` for the one waiting now",
                )
                .with_code("CALL-CHANGED");
            }
            let what = match verb {
                Verb::Reject(_, reason) => {
                    decisions.reject(
                        &feature,
                        task,
                        grant.clone(),
                        reason,
                        &Who::local_in(&root, via),
                    );
                    format!(
                        "rejected: {} for {task} — the model will be told: {reason}",
                        call.covers
                    )
                }
                _ => {
                    decisions.approve(
                        &feature,
                        task,
                        grant.clone(),
                        None,
                        &Who::local_in(&root, via),
                    );
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
        unreadable,
        batch: Vec::new(),
    };
    let report = ApprovalsReport {
        batch: batches(&report.pending),
        ..report
    };
    CommandResult::new(command, CommandStatus::Ok, &message)
        .with_data(CommandData::Approvals(Box::new(report)))
}

// ── The asking budget's batch ───────────────────────────────────────

/// SHA-256 over the sorted digests of the calls listed
/// ([`speccoder_core::PendingCall::digest`]): each binds its id and its
/// object, so a lane's newer attempt stopped on another object changes it.
#[must_use]
pub fn batch_digest(digests: &[String]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut ids = digests.to_vec();
    ids.sort();
    Sha256::digest(ids.join("\n").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The runs whose asking budget is used, each with every call of it that
/// waits (its lanes' included), by class.
#[must_use]
pub fn batches(pending: &[PendingCall]) -> Vec<speccoder_core::BatchView> {
    let mut runs: Vec<String> = pending
        .iter()
        .filter(|p| p.batched)
        .filter_map(|p| p.run.clone())
        .collect();
    runs.sort();
    runs.dedup();
    runs.into_iter()
        .map(|run| {
            let of_run: Vec<&PendingCall> = pending
                .iter()
                .filter(|p| p.run.as_deref() == Some(run.as_str()) && p.kind == tasks::APPROVAL)
                .collect();
            let groups = ["write", "command", "network", "mcp"]
                .iter()
                .filter_map(|class| {
                    let calls: Vec<String> = of_run
                        .iter()
                        .filter(|p| {
                            p.object.as_ref().map_or("write", |o| o.class.as_str()) == *class
                        })
                        .map(|p| p.id())
                        .collect();
                    (!calls.is_empty()).then(|| speccoder_core::BatchGroup {
                        class: (*class).to_owned(),
                        calls,
                    })
                })
                .collect::<Vec<_>>();
            let digests: Vec<String> = of_run.iter().map(|p| p.digest.clone()).collect();
            speccoder_core::BatchView {
                digest: batch_digest(&digests),
                run,
                groups,
            }
        })
        .collect()
}

/// What a batch decision decides.
#[derive(Debug, Clone, Copy)]
pub struct BatchChoice<'a> {
    /// The run.
    pub run: &'a str,
    /// Approve (each once) or reject.
    pub approve: bool,
    /// The calls decided: a non-empty subset of the batch.
    pub calls: &'a [String],
    /// The digest of the list the person saw.
    pub digest: &'a str,
    /// Start the run's asking budget again.
    pub resume_asking: bool,
    /// A rejection's reason, for each call.
    pub reason: Option<&'a str>,
}

/// `DecideBatch`: decides the listed calls of a run's batch, each once —
/// never for the task or the run — recorded per call with who decided and
/// through what; refused when the list changed since the person saw it.
#[must_use]
pub fn decide_batch(
    path: &Path,
    feature: Option<&str>,
    choice: BatchChoice<'_>,
    state: Option<&Path>,
    via: Via,
) -> CommandResult {
    let command = "approvals";
    let Some(state) = state else {
        return CommandResult::new(command, CommandStatus::ConfigError, "no state directory")
            .with_code("NO-STATE-DIR");
    };
    let (feature, dir, root) = match locate(command, path, feature, state) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let listed = decide_with(path, Some(&feature), Verb::List, state, via);
    let Some(CommandData::Approvals(report)) = listed.data().cloned() else {
        return listed;
    };
    if let Some(why) = &report.unreadable {
        return CommandResult::new(
            command,
            CommandStatus::Error,
            &format!("the calls waiting for a decision could not be read: {why}"),
        );
    }
    let Some(batch) = report.batch.iter().find(|b| b.run == choice.run) else {
        return CommandResult::new(
            command,
            CommandStatus::NotFound,
            &format!("no calls wait together in run {}", choice.run),
        );
    };
    let listed_ids: Vec<String> = batch.groups.iter().flat_map(|g| g.calls.clone()).collect();
    if batch.digest != choice.digest {
        return CommandResult::new(
            command,
            CommandStatus::UsageError,
            &format!(
                "the list changed; {} calls are waiting now",
                listed_ids.len()
            ),
        )
        .with_data(CommandData::Approvals(report.clone()));
    }
    if choice.calls.is_empty() || choice.calls.iter().any(|c| !listed_ids.contains(c)) {
        return CommandResult::new(
            command,
            CommandStatus::UsageError,
            "name calls of the batch (or none to decide them all)",
        );
    }
    let by = Who::local_in(&root, via);
    let mut decisions = match Decisions::load(&dir) {
        Ok(d) => d,
        Err(e) => return CommandResult::new(command, CommandStatus::ConfigError, &e),
    };
    let rows = match waiting_tasks(&root, &dir, &feature) {
        Ok(r) => r,
        Err(e) => return CommandResult::new(command, CommandStatus::Error, &e),
    };
    let reason = choice.reason.unwrap_or("no reason given");
    let mut lines = Vec::new();
    for id in choice.calls {
        let mut answer = if choice.approve {
            Answer::approve(Scope::Once, None, Some(by.clone()))
        } else {
            Answer::reject(reason, Some(by.clone()))
        };
        answer.resume_asking = choice.resume_asking;
        if let Some(task) = id.strip_prefix("attempt:") {
            // A lane's stopped call: decided for the task's next attempt.
            let Some(p) = report
                .pending
                .iter()
                .find(|p| p.call.is_empty() && p.task == task)
            else {
                continue;
            };
            // The object the listing showed, never one found now.
            let object = pending(&root, &feature, &[task.to_owned()], &decisions)
                .into_iter()
                .find(|(now, _)| now.digest == p.digest)
                .map(|(_, o)| o);
            let Some(object) = object else {
                return CommandResult::new(
                    command,
                    CommandStatus::UsageError,
                    &format!("the list changed; {task}'s attempt stopped on another call"),
                )
                .with_code("CALL-CHANGED");
            };
            if choice.approve {
                // Once: the task's next attempt may make this call one time.
                decisions.approve_once(&feature, task, object, &by);
            } else {
                decisions.reject(&feature, task, object, reason, &by);
            }
            lines.push(format!("{}: {}", p.task, p.covers));
            continue;
        }
        let Some((p, row)) = rows.iter().find(|(p, _)| p.call == *id) else {
            continue;
        };
        if let Err(e) = decide_waiting(&dir, row, &answer, &by) {
            return CommandResult::new(command, CommandStatus::Error, &e);
        }
        lines.push(format!("{}: {}", p.task, p.covers));
    }
    if choice.resume_asking
        && let Some(first) = report.pending.iter().find(|p| listed_ids.contains(&p.id()))
    {
        decisions.note(
            &feature,
            &first.task,
            Decided {
                verb: "resume_asking".to_owned(),
                object: None,
                run: Some(choice.run.to_owned()),
                reason: None,
                scope: "run".to_owned(),
                note: None,
                by: by.clone(),
                at: now(),
            },
        );
    }
    if let Err(e) = decisions.save(&dir) {
        return CommandResult::new(command, CommandStatus::Error, &e);
    }
    let after = decide_with(path, Some(&feature), Verb::List, state, via);
    CommandResult::new(
        command,
        CommandStatus::Ok,
        &format!(
            "{} {} call(s) once — by {} ({}){}: {}; `speccoder run` continues",
            if choice.approve {
                "approved"
            } else {
                "rejected"
            },
            lines.len(),
            by.name,
            by.via,
            if choice.resume_asking {
                "; the run asks again"
            } else {
                ""
            },
            lines.join(", ")
        ),
    )
    .with_data(
        after
            .data()
            .cloned()
            .unwrap_or(CommandData::Approvals(report)),
    )
}

// ── Always Allow ────────────────────────────────────────────────────

/// `AllowAlways`: the waiting command `call` of `task` runs once and
/// exactly its argv joins `[commands] allow` (or `[sandbox] network_allow`
/// for a network call). Only for a call that offered it; a call still
/// waiting is decided here, one answered Always Allow inline in this
/// process was decided there.
#[must_use]
pub fn allow_always(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    call: &str,
    state: Option<&Path>,
    via: Via,
) -> CommandResult {
    let command = "config";
    let refuse = |m: &str| CommandResult::new(command, CommandStatus::UsageError, m);
    let Some(state) = state else {
        return CommandResult::new(command, CommandStatus::ConfigError, "no state directory")
            .with_code("NO-STATE-DIR");
    };
    let (feature, dir, root) = match locate(command, path, feature, state) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    if !dir.join(crate::substrate::JOURNAL).exists() {
        return CommandResult::new(command, CommandStatus::NotFound, "no such waiting call");
    }
    let Ok(id) = agentplane::core::TaskId::parse(call) else {
        return refuse("not a call id");
    };
    let row = match crate::tokio_runtime().and_then(|rt| {
        let plane = crate::substrate::Plane::open_for_decisions(&dir).map_err(|e| e.0)?;
        rt.block_on(plane.task(id)).map_err(|e| e.0)
    }) {
        Ok(Some(row)) => row,
        Ok(None) => {
            return CommandResult::new(command, CommandStatus::NotFound, "no such waiting call");
        }
        Err(e) => return CommandResult::new(command, CommandStatus::Error, &e),
    };
    let Some(request) = tasks::request_of(&row.justification.proposed_action)
        .filter(|r| r.task == format!("{feature}/{task}"))
    else {
        return CommandResult::new(
            command,
            CommandStatus::NotFound,
            &format!("{task} has no such call"),
        );
    };
    let offered = matches!(request.kind.as_str(), "run" | "network")
        && request.options.iter().any(|o| o == "always");
    let Some(Object::Command { argv, network, .. }) = request.object.clone().filter(|_| offered)
    else {
        return refuse("Always Allow is not offered for this call");
    };
    let by = Who::local_in(&root, via);
    match row.state {
        agentplane::core::TaskState::Open | agentplane::core::TaskState::Claimed => {
            if let Err(e) = decide_waiting(&dir, &row, &Answer::always(Some(by.clone())), &by) {
                return CommandResult::new(command, CommandStatus::Error, &e);
            }
        }
        _ if take_answered_always(call) => {}
        _ => return refuse("this call was already decided"),
    }
    let key = if network {
        "sandbox.network_allow"
    } else {
        "commands.allow"
    };
    let r = crate::settings::apply_confirmed(
        &root,
        &[speccoder_core::ConfigChange {
            key: key.to_owned(),
            value: None,
            append: Some(serde_json::json!(argv)),
        }],
    );
    if r.status() != CommandStatus::Ok {
        return r;
    }
    let table = if network {
        "[sandbox] network_allow"
    } else {
        "[commands] allow"
    };
    CommandResult::new(
        command,
        CommandStatus::Ok,
        &format!(
            "allowed once; added exactly `{}` to {table} — nothing after it runs because of this (by {} via {}); commit speccoder.toml to keep it",
            argv.join(" "),
            by.name,
            by.via
        ),
    )
    .with_next(r.next().to_vec())
    .with_data(r.data().cloned().unwrap_or(CommandData::Config(Box::new(crate::settings::read(&root)))))
}

/// [`decide_with`] in the default state directory.
#[must_use]
pub fn decide(path: &Path, feature: Option<&str>, verb: Verb<'_>, via: Via) -> CommandResult {
    decide_in(
        path,
        feature,
        verb,
        crate::worktree::state_dir().as_deref(),
        via,
    )
}

/// [`decide_with`] in `state`, if there is one.
#[must_use]
pub fn decide_in(
    path: &Path,
    feature: Option<&str>,
    verb: Verb<'_>,
    state: Option<&Path>,
    via: Via,
) -> CommandResult {
    match state {
        Some(state) => decide_with(path, feature, verb, state, via),
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

    fn file(path: &str) -> Object {
        Object::Path {
            canonical: path.to_owned(),
            asked: path.to_owned(),
        }
    }

    #[test]
    fn decisions_round_trip_and_replace_each_other() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let g = file("Cargo.toml");
        let mut d = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let by = Who::local(Via::Terminal);
        d.reject("f", "T1", g.clone(), "no new dependencies", &by);
        d.approve("f", "T1", g.clone(), Some("run-1"), &by);
        assert_eq!(d.tasks["f/T1"].decided.len(), 2);
        assert_eq!(d.tasks["f/T1"].decided[1].by, by);
        d.save(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let back = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.for_run("f", "T1", "run-1"), std::slice::from_ref(&g));
        assert!(back.for_task("f", "T1").rejected.is_empty());
        // Another run's grant covers nothing, and is history only.
        assert!(back.for_run("f", "T1", "run-2").is_empty());
        assert!(!back.decided("f", "T1", &g) && !back.decided("f", "T2", &g));
    }

    #[test]
    fn a_lane_decision_is_claimed_by_the_next_run_only() {
        let mut d = Decisions::default();
        let by = Who::local(Via::Terminal);
        d.approve("f", "T1", file("package.json"), None, &by);
        assert!(d.decided("f", "T1", &file("package.json")));
        assert!(d.for_run("f", "T1", "run-2").is_empty());
        assert!(d.claim("f", "T1", "run-2"));
        assert_eq!(d.for_run("f", "T1", "run-2"), [file("package.json")]);
        // Claimed once: a later run gets nothing.
        assert!(!d.claim("f", "T1", "run-3"));
        assert!(d.for_run("f", "T1", "run-3").is_empty());
    }

    #[test]
    fn a_decisions_file_from_before_objects_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(
            dir.path().join(FILE),
            r#"{"tasks":{"f/T1":{"approved":[{"kind":"write","path":"Cargo.toml"}]}}}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let e = Decisions::load(dir.path()).err().unwrap_or_default();
        assert!(
            e.contains(FILE) && e.contains("before decisions named objects"),
            "{e}"
        );
    }

    #[test]
    fn remembered_options_need_an_object_and_text_not_copied_from_content() {
        let o = |risk, has, derived, kind, always| options(risk, has, derived, kind, always);
        assert_eq!(
            o("low", true, false, "run", "low"),
            ["once", "task", "run", "always", "reject", "stop"]
        );
        // A network call is high risk; Always Allow is judged without it.
        assert_eq!(
            o("high", true, false, "network", "low"),
            ["once", "task", "always", "reject", "stop"]
        );
        assert_eq!(
            o("high", true, false, "run", "high"),
            ["once", "task", "reject", "stop"]
        );
        assert_eq!(
            o("low", true, false, "write", "low"),
            ["once", "task", "run", "reject", "stop"]
        );
        assert_eq!(
            o("low", true, true, "run", "low"),
            ["once", "reject", "stop"]
        );
        assert_eq!(
            o("low", false, false, "run", "low"),
            ["once", "reject", "stop"]
        );
    }

    #[test]
    fn a_runs_batch_lists_its_lanes_stopped_calls_too() {
        let call = |task: &str, id: &str, batched: bool, class: &str, run: &str| {
            with_digest(
                PendingCall {
                    task: task.to_owned(),
                    call: id.to_owned(),
                    kind: tasks::APPROVAL.to_owned(),
                    run: Some(run.to_owned()),
                    batched,
                    object: Some(speccoder_core::ObjectView {
                        class: class.to_owned(),
                        ..speccoder_core::ObjectView::default()
                    }),
                    ..PendingCall::default()
                },
                Some(&file(task)),
            )
        };
        let pending = vec![
            call("T1", "aa", true, "write", "run-1"),
            // A lane's stop in the same run: no task id, listed by its task.
            call("T2", "", false, "command", "run-1"),
            // Another run, within its budget: no batch of its own.
            call("T3", "bb", false, "write", "run-2"),
        ];
        let b = batches(&pending);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].run, "run-1");
        let groups: Vec<(&str, Vec<String>)> = b[0]
            .groups
            .iter()
            .map(|g| (g.class.as_str(), g.calls.clone()))
            .collect();
        assert_eq!(
            groups,
            [
                ("write", vec!["aa".to_owned()]),
                ("command", vec!["attempt:T2".to_owned()])
            ]
        );
        assert_eq!(
            b[0].digest,
            batch_digest(&[pending[1].digest.clone(), pending[0].digest.clone()])
        );
        // A newer attempt of T2 stopped on another object: the same ids,
        // another digest, so a decision on the list seen is refused.
        let mut newer = pending.clone();
        newer[1] = with_digest(newer[1].clone(), Some(&file("other.json")));
        assert_eq!(newer[1].id(), pending[1].id());
        assert_ne!(batches(&newer)[0].digest, b[0].digest);
    }

    /// A call's digest binds its id and its object: another call, or the
    /// same lane's attempt stopped on another object, has another.
    #[test]
    fn a_calls_digest_binds_its_id_and_object() {
        let a = call_digest("attempt:T1", Some(&file("a.json")));
        assert_eq!(a, call_digest("attempt:T1", Some(&file("a.json"))));
        assert_ne!(a, call_digest("attempt:T1", Some(&file("b.json"))));
        assert_ne!(a, call_digest("attempt:T2", Some(&file("a.json"))));
        assert_ne!(a, call_digest("attempt:T1", None));
    }

    /// A once-grant (a lane's call decided in its batch) is claimed by the
    /// next run, handed to one attempt and gone after it: never a grant
    /// for the task or the run.
    #[test]
    fn a_once_grant_is_given_to_one_attempt_only() {
        let mut d = Decisions::default();
        let by = Who::local(Via::Terminal);
        d.approve_once("f", "T1", file("package.json"), &by);
        assert!(d.decided("f", "T1", &file("package.json")));
        assert!(d.claim("f", "T1", "run-2"));
        assert!(d.for_run("f", "T1", "run-2").is_empty(), "not for the run");
        assert_eq!(d.take_once("f", "T1", "run-2"), [file("package.json")]);
        assert!(
            d.take_once("f", "T1", "run-2").is_empty(),
            "one attempt only"
        );
        assert_eq!(d.for_task("f", "T1").decided[0].scope, "once");
        // It reads back as written; a task grant carries no `once`.
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Decisions>(&text).unwrap(), d);
        d.approve("f", "T1", file("x"), Some("run-2"), &by);
        let grant = serde_json::to_value(&d.for_task("f", "T1").approved[0]).unwrap();
        assert!(grant.get("once").is_none(), "{grant}");
    }
}
