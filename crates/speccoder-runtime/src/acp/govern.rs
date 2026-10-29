//! An external agent's calls under speccoder's policy: its permission
//! requests, and the file and terminal access speccoder offers it.
//!
//! Each call is decided as a native attempt's call is — the same
//! classification ([`Policy`]), the same objects, the same grants, the same
//! person through the same [`Approver`] — and recorded in the attempt's
//! tool-call log in the native shape, so `task approve`, the report and the
//! audit read it alike. A permission request names no stable tool, so it is
//! classified from its declared kind and arguments, never its title; one
//! that cannot be classified goes to a person. With nobody to ask, the
//! first call that needs a person ends the attempt `needs_approval`, saved
//! with that call for `speccoder task approve`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::calls::ToolRequest;
use crate::approvals::{
    Answer, Approver, Decision as Said, PresentContext, Request, Scope, Who, present,
};
use crate::progress::{Event, Progress};
use crate::tools::WorkspaceTools;
use crate::tools::policy::{Decision, Object, Policy};

/// A command allowed to run: its argv, directory and environment.
pub type Command = (Vec<String>, PathBuf, Vec<(String, String)>);

/// The tool name a call speccoder cannot classify is recorded under.
pub const AGENT_CALL: &str = "agent_call";

/// One of the agent's permission options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    /// Its id.
    pub id: String,
    /// `allow_once`, `allow_always`, `reject_once`, `reject_always`.
    pub kind: String,
}

/// What a call came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// It may run (checked again against `object` where it has one).
    Allow(Option<Object>),
    /// It is refused, and why.
    Deny(String),
    /// A person is needed and nobody can be asked (or the person stopped
    /// the task): the attempt ends.
    Stop,
}

/// Who decides for one attempt, and what they decided.
pub struct Governor {
    root: PathBuf,
    tools: WorkspaceTools,
    policy: Policy,
    approver: Option<Arc<dyn Approver>>,
    present: PresentContext,
    task: String,
    task_title: String,
    run: Option<String>,
    frozen_additions: bool,
    progress: Option<(String, Arc<dyn Progress>)>,
    state: Mutex<State>,
}

impl std::fmt::Debug for Governor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Governor")
            .field("root", &self.root)
            .field("task", &self.task)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct State {
    /// Objects a person approved for one call each.
    once: Vec<Object>,
    /// Objects a person approved for the run or the task, with the scope.
    remembered: Vec<(Object, &'static str)>,
    /// Every decision and every call the agent reported, in order.
    log: Vec<Value>,
    /// The call waiting for a person nobody could ask.
    pending: Option<Value>,
    /// The agent's words since its last call (the claim a person sees).
    said: String,
    /// The attempt is ending: nothing more is allowed.
    stopped: bool,
    /// Prompts shown to a person.
    asked: u32,
}

/// What a [`Governor`] is built from.
pub struct Setup {
    /// The worktree (canonical).
    pub root: PathBuf,
    /// The attempt's tools (with the sandboxed runner).
    pub tools: WorkspaceTools,
    /// The attempt's policy, with the task's decisions.
    pub policy: Policy,
    /// Who answers a call that needs a person; `None` with `--no-prompt`.
    pub approver: Option<Arc<dyn Approver>>,
    /// What a person is shown about the sandbox.
    pub present: PresentContext,
    /// `feature/task`.
    pub task: String,
    /// The task's title.
    pub task_title: String,
    /// The run.
    pub run: Option<String>,
    /// `[tests] frozen = "additions"`.
    pub frozen_additions: bool,
    /// Objects approved for every task of this run.
    pub run_approved: Vec<Object>,
    /// Objects approved for one call each.
    pub approved_once: Vec<Object>,
    /// The task id and where its events go.
    pub progress: Option<(String, Arc<dyn Progress>)>,
}

/// A command line as an argv: split on whitespace when it is a plain
/// command, else given to `sh -c` whole — which the policy asks a person
/// about, so a shell's meaning is never guessed.
#[must_use]
pub fn argv_of_line(line: &str) -> Vec<String> {
    const SHELL: &[char] = &[
        '|', '&', ';', '<', '>', '(', ')', '$', '`', '\\', '"', '\'', '*', '?', '[', ']', '{', '}',
        '~', '!', '#', '\n', '=',
    ];
    let line = line.trim();
    if line.contains(SHELL) {
        return vec!["sh".to_owned(), "-c".to_owned(), line.to_owned()];
    }
    line.split_whitespace().map(str::to_owned).collect()
}

/// A call in the policy's vocabulary: the speccoder tool calls it amounts to,
/// or the reason no rule covers it.
#[derive(Debug, Clone, PartialEq)]
enum Subject {
    /// These calls, each decided (the first refusal refuses all).
    Calls(Vec<(&'static str, Value)>),
    /// Refused, whoever asks.
    Deny(String),
    /// Nothing to decide (no effect outside the session).
    Allow(&'static str),
    /// No rule covers it: a person decides.
    Person(String),
}

impl Governor {
    /// The governor for one attempt.
    #[must_use]
    pub fn new(s: Setup) -> Self {
        let remembered = s
            .run_approved
            .into_iter()
            .map(|o| (o, "run-grant"))
            .collect();
        Self {
            root: s.root,
            tools: s.tools,
            policy: s.policy,
            approver: s.approver,
            present: s.present,
            task: s.task,
            task_title: s.task_title,
            run: s.run,
            frozen_additions: s.frozen_additions,
            progress: s.progress,
            state: Mutex::new(State {
                once: s.approved_once,
                remembered,
                ..State::default()
            }),
        }
    }

    /// The worktree.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The decisions and calls so far.
    #[must_use]
    pub fn log(&self) -> Vec<Value> {
        self.state.lock().map(|s| s.log.clone()).unwrap_or_default()
    }

    /// The call waiting for a person nobody could ask, if one is.
    #[must_use]
    pub fn pending(&self) -> Option<Value> {
        self.state.lock().ok().and_then(|s| s.pending.clone())
    }

    /// Whether the attempt is ending.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.state.lock().map_or(true, |s| s.stopped)
    }

    /// Prompts a person was shown.
    #[must_use]
    pub fn asked(&self) -> u32 {
        self.state.lock().map_or(0, |s| s.asked)
    }

    /// The agent's words since its last call.
    pub fn said(&self, text: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.said.push_str(text);
        }
    }

    /// The agent reported a call it makes: recorded beside the decisions,
    /// as the agent's own (`decision: "reported"`) — its claim, never
    /// evidence.
    pub fn reported(&self, call: &ToolRequest) {
        let entry = json!({
            "tool": format!("agent:{}", call.kind.as_deref().unwrap_or("other")),
            "arguments": call.raw_input.clone().unwrap_or(Value::Null),
            "title": call.shown_title(),
            "decision": "reported",
            "ok": call.status.as_deref() == Some("completed"),
            "by": "agent",
        });
        self.emit(&entry);
        if let Ok(mut s) = self.state.lock() {
            s.said.clear();
            s.log.push(entry);
        }
    }

    /// Ends the attempt: nothing more is allowed.
    pub fn stop(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.stopped = true;
        }
    }

    /// A path the agent named, relative to the worktree; refused when it is
    /// outside.
    fn rel(&self, path: &Path) -> Result<String, String> {
        let inside = |p: &Path| {
            p.strip_prefix(&self.root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        };
        if path.is_relative() {
            return Ok(path.to_string_lossy().replace('\\', "/"));
        }
        if let Some(r) = inside(path) {
            return Ok(if r.is_empty() { ".".to_owned() } else { r });
        }
        // The same place by another spelling (`/tmp` and `/private/tmp`):
        // its nearest existing ancestor resolved.
        let mut base = path.to_path_buf();
        let mut rest = Vec::new();
        while !base.exists() {
            match (
                base.file_name().map(std::ffi::OsStr::to_os_string),
                base.parent(),
            ) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    base = parent.to_path_buf();
                }
                _ => break,
            }
        }
        if let Ok(mut canonical) = std::fs::canonicalize(&base) {
            for name in rest.iter().rev() {
                canonical.push(name);
            }
            if let Some(r) = inside(&canonical) {
                return Ok(if r.is_empty() { ".".to_owned() } else { r });
            }
        }
        Err(format!(
            "`{}` is outside the worktree; an attempt works only inside it",
            path.display()
        ))
    }

    /// What a permission request amounts to in speccoder's tools.
    fn subject(&self, call: &ToolRequest) -> Subject {
        let raw = call.raw_input.clone().unwrap_or(Value::Null);
        let text = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_owned);
        let mut paths: Vec<PathBuf> = call.locations.clone();
        if paths.is_empty() {
            paths.extend(
                ["path", "file_path", "filePath", "fileName", "notebook_path"]
                    .iter()
                    .find_map(|k| text(k))
                    .map(PathBuf::from),
            );
        }
        let rels =
            || -> Result<Vec<String>, String> { paths.iter().map(|p| self.rel(p)).collect() };
        let command = || -> Option<Vec<String>> {
            match raw.get("command") {
                Some(Value::String(line)) => {
                    let mut argv = argv_of_line(line);
                    if let Some(args) = raw.get("args").and_then(Value::as_array) {
                        argv.extend(args.iter().filter_map(|a| a.as_str().map(str::to_owned)));
                    }
                    Some(argv)
                }
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect(),
                ),
                _ => raw
                    .get("commands")
                    .and_then(Value::as_array)
                    .filter(|c| c.len() == 1)
                    .and_then(|c| c[0].as_str())
                    .map(argv_of_line),
            }
            .filter(|a: &Vec<String>| !a.is_empty())
        };
        let kind = call.kind.as_deref().unwrap_or("other");
        // A call that carries a command is a command whatever it is labelled.
        let kind = if matches!(kind, "other") && command().is_some() {
            "execute"
        } else {
            kind
        };
        match kind {
            "execute" => {
                let Some(argv) = command() else {
                    return Subject::Person(
                        "the agent did not send what it means to run".to_owned(),
                    );
                };
                let mut args = json!({ "argv": argv });
                if let Some(cwd) = text("cwd").or_else(|| text("workdir")) {
                    match self.rel(Path::new(&cwd)) {
                        Ok(r) => args["cwd"] = json!(r),
                        Err(e) => return Subject::Deny(e),
                    }
                }
                Subject::Calls(vec![("run_command", args)])
            }
            "read" | "search" | "edit" | "delete" | "move" => {
                let rels = match rels() {
                    Ok(r) => r,
                    Err(e) => return Subject::Deny(e),
                };
                if rels.is_empty() {
                    return Subject::Person(format!(
                        "the agent did not say which file it means to {kind}"
                    ));
                }
                let tool = if matches!(kind, "read" | "search") {
                    "read_file"
                } else {
                    "write_file"
                };
                let content = text("content").or_else(|| text("file_text"));
                Subject::Calls(
                    rels.into_iter()
                        .map(|p| {
                            let mut a = json!({ "path": p });
                            if let (Some(c), "write_file") = (&content, tool) {
                                a["content"] = json!(c);
                            }
                            (tool, a)
                        })
                        .collect(),
                )
            }
            "fetch" => Subject::Deny(
                "fetching from the network: an attempt's commands run without it".to_owned(),
            ),
            "think" => Subject::Allow("no effect outside the session"),
            "switch_mode" => {
                Subject::Deny("the agent is kept in the mode that asks before acting".to_owned())
            }
            _ => Subject::Person("no rule can tell what this call does".to_owned()),
        }
    }

    /// Answers a permission request: the agent's allow-once option when
    /// speccoder's policy (or a person) allows the call, its reject option
    /// when not, `None` (cancelled) when the attempt ends here.
    pub fn permission(&self, call: &ToolRequest, options: &[PermissionOption]) -> Option<String> {
        let pick = |kinds: &[&str]| {
            kinds
                .iter()
                .find_map(|k| options.iter().find(|o| o.kind == *k))
                .map(|o| o.id.clone())
        };
        let reject = || pick(&["reject_once", "reject_always"]);
        if self.stopped() {
            return None;
        }
        let verdict = match self.subject(call) {
            Subject::Calls(calls) => {
                let mut verdict = Verdict::Allow(None);
                for (tool, args) in calls {
                    match self.decide(tool, &args, Some(call)) {
                        Verdict::Allow(_) => {}
                        other => {
                            verdict = other;
                            break;
                        }
                    }
                }
                verdict
            }
            Subject::Deny(reason) => {
                self.record(
                    json!({ "tool": AGENT_CALL, "arguments": call_args(call), "decision": "deny", "reason": reason, "by": "rule", "rule": "agent call" }),
                );
                Verdict::Deny(reason)
            }
            Subject::Allow(rule) => {
                self.record(
                    json!({ "tool": AGENT_CALL, "arguments": call_args(call), "decision": "allow", "by": "rule", "rule": rule }),
                );
                Verdict::Allow(None)
            }
            Subject::Person(reason) => self.decide_unclassified(call, &reason),
        };
        match verdict {
            // Never an option the agent would remember: a later call must
            // reach speccoder again.
            Verdict::Allow(_) => pick(&["allow_once"]).or_else(|| {
                self.record(json!({ "tool": AGENT_CALL, "arguments": call_args(call), "decision": "deny", "reason": "the agent offered no allow-once option", "by": "rule", "rule": "agent options" }));
                reject()
            }),
            Verdict::Deny(_) => reject(),
            Verdict::Stop => None,
        }
    }

    /// A call no rule covers: a person, or the attempt ends.
    fn decide_unclassified(&self, call: &ToolRequest, reason: &str) -> Verdict {
        let args = call_args(call);
        let request = self.request(AGENT_CALL, &args, reason, None);
        self.ask(AGENT_CALL, &args, None, reason, request)
    }

    /// Decides one speccoder tool call as a native attempt would.
    pub fn decide(&self, tool: &str, args: &Value, call: Option<&ToolRequest>) -> Verdict {
        let _ = call;
        if self.stopped() {
            return Verdict::Deny("the attempt is ending".to_owned());
        }
        let mut decision = self.policy.base(tool, args);
        // `[tests] frozen = "additions"`: a write that keeps every line of
        // the frozen test is allowed.
        let mut additions_only = false;
        if self.frozen_additions
            && tool == "write_file"
            && matches!(&decision, Decision::Approval(r) if r.contains("frozen test"))
            && let (Some(path), Some(new)) = (args["path"].as_str(), args["content"].as_str())
            && let Ok(old) = std::fs::read_to_string(self.root.join(path))
            && crate::integrity::only_additions(&old, new)
        {
            decision = Decision::Allow;
            additions_only = true;
        }
        let mut object: Option<Object> = None;
        if matches!(decision, Decision::Approval(_)) {
            let resolved = self.tools.resolve(tool, args);
            if resolved["ok"] == true {
                object = serde_json::from_value(resolved["object"].clone()).ok();
                decision = self.policy.classify(tool, args, object.as_ref());
            } else {
                decision = Decision::Deny(
                    resolved["error"]
                        .as_str()
                        .unwrap_or("the call's object cannot be resolved")
                        .to_owned(),
                );
            }
        }
        let mut person: Option<Value> = None;
        if decision == Decision::Allow && object.is_some() {
            person = Some(
                json!({ "approved_by": "person", "scope": "task-grant", "grant": { "scope": "task", "run": self.run } }),
            );
        }
        if matches!(decision, Decision::Approval(_))
            && let Some(o) = &object
            && let Ok(mut s) = self.state.lock()
        {
            if let Some(at) = s.once.iter().position(|r| r == o) {
                s.once.remove(at);
                person = Some(
                    json!({ "approved_by": "person", "scope": "once-grant", "grant": { "scope": "once", "run": self.run } }),
                );
                decision = Decision::Allow;
            } else if let Some((_, scope)) = s.remembered.iter().find(|(r, _)| r == o) {
                person = Some(
                    json!({ "approved_by": "person", "scope": scope, "grant": { "scope": scope.trim_end_matches("-grant"), "run": self.run } }),
                );
                decision = Decision::Allow;
            }
        }
        decision = self.policy.settle(decision);
        match decision {
            Decision::Allow => {
                let mut entry =
                    json!({ "tool": tool, "arguments": args, "decision": "allow", "ok": true });
                self.decided_by(&mut entry, object.as_ref(), person.as_ref(), tool, args);
                if additions_only {
                    entry["frozen_additions"] = json!(true);
                }
                self.record(entry);
                Verdict::Allow(object)
            }
            Decision::Deny(reason) => {
                let mut entry = json!({ "tool": tool, "arguments": args, "decision": "deny", "reason": reason });
                self.decided_by(&mut entry, object.as_ref(), person.as_ref(), tool, args);
                self.record(entry);
                Verdict::Deny(reason)
            }
            Decision::Approval(reason) => {
                let request = self.request(tool, args, &reason, object.as_ref());
                self.ask(tool, args, object, &reason, request)
            }
        }
    }

    /// Who decided an entry: a person (by a grant), or a rule.
    fn decided_by(
        &self,
        entry: &mut Value,
        object: Option<&Object>,
        person: Option<&Value>,
        tool: &str,
        args: &Value,
    ) {
        if let Some(o) = object {
            entry["object"] = json!(o);
        }
        match person.and_then(Value::as_object) {
            Some(p) => {
                entry["by"] = json!("person");
                if let Some(e) = entry.as_object_mut() {
                    e.extend(p.clone());
                }
            }
            None => {
                entry["by"] = json!("rule");
                entry["rule"] = json!(self.policy.rule(tool, args, object));
            }
        }
    }

    /// The question for a call that needs a person, as a native attempt
    /// builds it.
    fn request(&self, tool: &str, args: &Value, reason: &str, object: Option<&Object>) -> Request {
        let kind = crate::approvals::kind_of(tool, args);
        let risk = crate::approvals::risk(tool, reason);
        let said = self
            .state
            .lock()
            .map(|s| s.said.clone())
            .unwrap_or_default();
        let changed = object.and_then(|o| {
            self.policy
                .approved()
                .iter()
                .find_map(|earlier| o.changed_from(earlier))
        });
        Request {
            task: self.task.clone(),
            task_title: self.task_title.clone(),
            turn: 0,
            tool: tool.to_owned(),
            arguments: args.clone(),
            kind: kind.to_owned(),
            reason: reason.to_owned(),
            object: object.cloned(),
            changed,
            risk: risk.to_owned(),
            claim: crate::approvals::claim(&said),
            options: crate::approvals::options(risk, object.is_some(), false, kind, risk),
            run: self.run.clone(),
            ..Request::default()
        }
    }

    /// Asks a person, or ends the attempt with the call waiting.
    fn ask(
        &self,
        tool: &str,
        args: &Value,
        object: Option<Object>,
        reason: &str,
        mut request: Request,
    ) -> Verdict {
        self.event(Event::ApprovalRequested {
            task: self.task_id(),
            call: String::new(),
            tool: tool.to_owned(),
            what: format!("{tool} {}", crate::progress::target(tool, args))
                .chars()
                .filter(|c| !c.is_control())
                .take(60)
                .collect(),
            class: object
                .as_ref()
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.as_ref().map(Object::short).unwrap_or_default(),
        });
        let Some(approver) = &self.approver else {
            return self.wait(tool, args, object, reason, request);
        };
        if let Ok(mut s) = self.state.lock() {
            s.asked += 1;
            request.asked = s.asked;
        }
        let context = PresentContext {
            root: Some(self.root.clone()),
            ..self.present.clone()
        };
        let answer: Answer = approver.ask(&present(&request, &context));
        let by = answer.by.clone();
        let approver_v = serde_json::to_value(&by).unwrap_or(Value::Null);
        let operator = by.as_ref().map(Who::view);
        self.event(Event::ApprovalDecided {
            task: self.task_id(),
            call: String::new(),
            decision: if answer.decision == Said::Approve {
                "approve"
            } else {
                "reject"
            }
            .to_owned(),
            scope: (answer.decision == Said::Approve).then(|| answer.scope.name().to_owned()),
            by: by
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), |w| w.name.clone()),
            how: by
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), |w| w.basis.clone()),
            class: object
                .as_ref()
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.as_ref().map(Object::short).unwrap_or_default(),
        });
        let mut entry =
            json!({ "tool": tool, "arguments": args, "reason": reason, "by": "person" });
        if let Some(o) = &object {
            entry["object"] = json!(o);
        }
        match answer.decision {
            Said::Approve => {
                entry["decision"] = json!("allow");
                entry["ok"] = json!(true);
                entry["approved_by"] = json!("person");
                entry["scope"] = json!(answer.scope.name());
                entry["note"] = json!(answer.note);
                entry["approver"] = approver_v;
                entry["operator"] = json!(operator);
                if answer.always {
                    entry["always"] = json!(true);
                }
                if let (Some(o), Scope::Task | Scope::Run) = (&object, answer.scope) {
                    entry["grant"] = json!({ "scope": answer.scope.name(), "run": self.run });
                    if let Ok(mut s) = self.state.lock() {
                        s.remembered.push((
                            o.clone(),
                            if answer.scope == Scope::Run {
                                "run-grant"
                            } else {
                                "task-grant"
                            },
                        ));
                    }
                }
                self.record(entry);
                Verdict::Allow(object)
            }
            Said::Reject => {
                let why = answer
                    .reason
                    .unwrap_or_else(|| "no reason given".to_owned());
                entry["decision"] = json!("deny");
                entry["rejected_by"] = json!("person");
                entry["approver"] = approver_v;
                entry["operator"] = json!(operator);
                entry["rejection"] = json!(why);
                self.record(entry);
                Verdict::Deny(format!("a person rejected this: {why}"))
            }
            // The task ends here; the call waits for `task approve`.
            Said::Stop => self.wait(tool, args, object, reason, request),
        }
    }

    /// The call waits for a person: the attempt ends `needs_approval`.
    fn wait(
        &self,
        tool: &str,
        args: &Value,
        object: Option<Object>,
        reason: &str,
        request: Request,
    ) -> Verdict {
        let mut entry =
            json!({ "tool": tool, "arguments": args, "decision": "approval", "reason": reason });
        if let Some(o) = &object {
            entry["object"] = json!(o);
        }
        self.record(entry);
        if let Ok(mut s) = self.state.lock() {
            if s.pending.is_none() {
                s.pending = Some(json!({
                    "tool": tool,
                    "arguments": args,
                    "reason": reason,
                    "object": object,
                    "run": self.run,
                    "request": request,
                }));
            }
            s.stopped = true;
        }
        Verdict::Stop
    }

    /// `fs/read_text_file`: a file in the worktree, read through the
    /// workspace tools (credentials refused, copied secrets masked).
    ///
    /// # Errors
    ///
    /// Why it is refused or cannot be read.
    pub async fn read_text_file(
        &self,
        path: &Path,
        line: Option<u32>,
        limit: Option<u32>,
    ) -> Result<String, String> {
        let rel = self.rel(path)?;
        let start = line.map(|l| l.max(1) as usize);
        let end = limit.map(|n| start.unwrap_or(1) + (n as usize).saturating_sub(1));
        let args = json!({ "path": rel, "start_line": start, "end_line": end });
        match self.decide("read_file", &json!({ "path": rel }), None) {
            Verdict::Allow(_) => {}
            Verdict::Deny(r) => return Err(format!("denied: {r}")),
            Verdict::Stop => return Err("the attempt is ending".to_owned()),
        }
        let out = self.tools.run("read_file", &args).await;
        if out["ok"] != true {
            return Err(out["error"].as_str().unwrap_or("not readable").to_owned());
        }
        if out["binary"] == true {
            return Err(format!("`{rel}` is not a text file"));
        }
        let mut content = out["content"].as_str().unwrap_or_default().to_owned();
        // The tool joins lines with `\n`; the file's last one is kept.
        if out["end_line"] == out["total_lines"]
            && out["truncated"] != true
            && std::fs::read(self.root.join(&rel)).is_ok_and(|b| b.ends_with(b"\n"))
        {
            content.push('\n');
        }
        Ok(content)
    }

    /// `fs/write_text_file`: a file in the worktree, written through the
    /// workspace tools (protected paths refused, frozen tests a person's).
    ///
    /// # Errors
    ///
    /// Why it is refused or cannot be written.
    pub async fn write_text_file(&self, path: &Path, content: &str) -> Result<(), String> {
        let rel = self.rel(path)?;
        let args = json!({ "path": rel, "content": content });
        let object = match self.decide("write_file", &args, None) {
            Verdict::Allow(o) => o,
            Verdict::Deny(r) => return Err(format!("denied: {r}")),
            Verdict::Stop => {
                return Err("the attempt is ending: this write waits for a person".to_owned());
            }
        };
        let mut call = args;
        if let Some(o) = object {
            call[crate::tools::EXPECT] = json!(o);
        }
        let out = self.tools.run("write_file", &call).await;
        if out["ok"] == true {
            Ok(())
        } else {
            Err(out["error"].as_str().unwrap_or("not written").to_owned())
        }
    }

    /// `terminal/create`: whether the command may run, and how — its argv,
    /// directory and scrubbed environment.
    ///
    /// # Errors
    ///
    /// Why it is refused.
    pub fn command(&self, argv: Vec<String>, cwd: Option<&Path>) -> Result<Command, String> {
        let mut args = json!({ "argv": argv });
        if let Some(c) = cwd {
            args["cwd"] = json!(self.rel(c)?);
        }
        let object = match self.decide("run_command", &args, None) {
            Verdict::Allow(o) => o,
            Verdict::Deny(r) => return Err(format!("denied: {r}")),
            Verdict::Stop => {
                return Err("the attempt is ending: this command waits for a person".to_owned());
            }
        };
        let dir = self.tools.command_cwd(args["cwd"].as_str()).map_err(|e| {
            e["error"]
                .as_str()
                .unwrap_or("no such directory")
                .to_owned()
        })?;
        // Decided on an object: it runs only as that object.
        if let Some(Object::Command { executable, .. }) = object {
            let now = self.tools.resolve("run_command", &args);
            if now["object"]["executable"] != json!(executable) {
                return Err(format!(
                    "the object changed since it was approved: {executable} → {}",
                    now["object"]["executable"]
                ));
            }
        }
        Ok((
            crate::tools::policy::argv_of(&args),
            dir,
            self.tools.command_env(None),
        ))
    }

    fn task_id(&self) -> String {
        self.task
            .rsplit('/')
            .next()
            .unwrap_or(&self.task)
            .to_owned()
    }

    fn event(&self, e: Event) {
        if let Some((_, p)) = &self.progress {
            p.event(&e);
        }
    }

    fn emit(&self, entry: &Value) {
        if let Some((task, p)) = &self.progress {
            let tool = entry["tool"].as_str().unwrap_or_default();
            p.event(&Event::ToolCall {
                task: task.clone(),
                turn: 0,
                tool: tool.to_owned(),
                target: match entry["title"].as_str() {
                    Some(t) => crate::progress::target("", &json!({ "path": t })),
                    None => crate::progress::target(tool, &entry["arguments"]),
                },
                decision: entry["decision"].as_str().unwrap_or_default().to_owned(),
                ok: entry["ok"].as_bool(),
                tail: Vec::new(),
            });
        }
    }

    fn record(&self, mut entry: Value) {
        entry["via"] = json!("acp");
        self.emit(&entry);
        if let Ok(mut s) = self.state.lock() {
            s.log.push(entry);
        }
    }
}

/// What a person sees of a call no rule covers.
fn call_args(call: &ToolRequest) -> Value {
    json!({
        "title": call.shown_title(),
        "kind": call.kind,
        "rawInput": call.raw_input,
        "locations": call.locations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_command_is_split_and_a_shell_line_is_left_to_sh() {
        assert_eq!(
            argv_of_line("cargo test -p core"),
            ["cargo", "test", "-p", "core"]
        );
        assert_eq!(argv_of_line("ls | grep x"), ["sh", "-c", "ls | grep x"]);
        assert_eq!(
            argv_of_line("git commit -m \"x\""),
            ["sh", "-c", "git commit -m \"x\""]
        );
        assert_eq!(argv_of_line("A=1 make"), ["sh", "-c", "A=1 make"]);
    }
}
