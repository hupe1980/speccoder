//! `speccoder acp`: speccoder as an Agent Client Protocol agent, so
//! an editor (Zed, JetBrains, …) drives it over stdio.
//!
//! Prompts are speccoder's commands — `/run`, `/run T005`, `/run --converge`,
//! `/converge`, `/report`, `/status`, `/cost` and the hosted planning skills
//! (`/specify …`, `/clarify`, `/plan`, `/tasks`, `/analyze`) — so a person
//! types what they would type at the terminal. Tasks and the model's tool
//! calls appear as the editor's tool calls; a call that needs a person is
//! the editor's permission request (allow once, allow for the task,
//! reject). Results are the same text the CLI prints.
//!
//! JSON-RPC 2.0, one message per line (ACP protocol version 1). stdout is
//! the protocol: nothing else may write there.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};
use speccoder_runtime::approvals::{Answer, Approver, Presentation, Scope, Who};
use speccoder_runtime::progress::{Event, Progress};

/// The commands an editor offers (ACP `available_commands_update`).
const COMMANDS: &[(&str, &str, Option<&str>)] = &[
    (
        "run",
        "Work through the ready tasks (or one: /run T005; /run --converge; /run --parallel 2)",
        Some("task id or options"),
    ),
    (
        "converge",
        "Assess the finished feature against its spec and append what remains",
        None,
    ),
    ("report", "Each task's evidence", None),
    ("status", "The current or last run", None),
    ("cost", "Tokens and money per task", None),
    (
        "specify",
        "Write the feature's spec.md from a description (Spec Kit's skill)",
        Some("what to build"),
    ),
    ("clarify", "Ask about what is unclear in the spec", None),
    ("plan", "Write plan.md and the design artifacts", None),
    ("tasks", "Write tasks.md", None),
    (
        "analyze",
        "Check spec, plan and tasks for consistency (read-only)",
        None,
    ),
];

/// The connection: one writer, and the agent's own requests awaiting answers.
struct Conn {
    out: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    next: AtomicU64,
    /// The editor, as it names itself at `initialize` (`clientInfo.name`).
    client: Mutex<String>,
    /// The editor can show a form (`clientCapabilities.elicitation.form`).
    forms: AtomicBool,
}

impl Conn {
    fn send(&self, v: &Value) {
        if let Ok(mut o) = self.out.lock() {
            let _ = writeln!(o, "{v}");
            let _ = o.flush();
        }
    }

    fn respond(&self, id: &Value, result: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn fail(&self, id: &Value, code: i64, message: &str) {
        self.send(
            &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        );
    }

    fn update(&self, session: &str, update: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": update } }));
    }

    fn id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next.fetch_add(1, Ordering::SeqCst))
    }

    /// Sends a request to the editor and waits for its answer.
    fn request(&self, method: &str, params: Value) -> Option<Value> {
        let id = self.id("speccoder");
        let (tx, rx) = mpsc::channel();
        self.pending.lock().ok()?.insert(id.clone(), tx);
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        rx.recv().ok()
    }
}

/// Tasks and tool calls as the editor's tool calls.
#[derive(Debug)]
struct Updates {
    conn: Arc<ConnRef>,
    session: String,
    tasks: Mutex<HashMap<String, String>>,
    /// The run's plan: (id, title, status, outcome suffix).
    plan: Mutex<Vec<(String, String, &'static str, String)>>,
}

impl Updates {
    /// The whole plan, as the editor's checklist.
    fn send_plan(&self) {
        let Ok(plan) = self.plan.lock() else { return };
        if plan.is_empty() {
            return;
        }
        let entries: Vec<Value> = plan
            .iter()
            .map(|(id, title, status, suffix)| {
                json!({ "content": format!("{id} {title}{suffix}"), "priority": "medium", "status": status })
            })
            .collect();
        self.conn.0.update(
            &self.session,
            json!({ "sessionUpdate": "plan", "entries": entries }),
        );
    }

    fn set(&self, task: &str, status: &'static str, suffix: String) {
        if let Ok(mut plan) = self.plan.lock()
            && let Some(e) = plan.iter_mut().find(|e| e.0 == task)
        {
            e.2 = status;
            e.3 = suffix;
        }
    }
}

/// `Conn` behind a `Debug` the traits require.
struct ConnRef(Conn);

impl std::fmt::Debug for ConnRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("acp connection")
    }
}

fn kind(tool: &str) -> &'static str {
    match tool {
        "read_file" | "list_files" | "git_status" | "git_diff" => "read",
        "write_file" | "edit_file" => "edit",
        "search_code" => "search",
        "run_command" => "execute",
        _ => "other",
    }
}

fn text(t: &str) -> Value {
    json!([{ "type": "content", "content": { "type": "text", "text": t } }])
}

impl Progress for Updates {
    fn event(&self, event: &Event) {
        let c = &self.conn.0;
        match event {
            Event::RunStarted { tasks, .. } => {
                if let Ok(mut plan) = self.plan.lock() {
                    *plan = tasks
                        .iter()
                        .map(|t| {
                            let (status, suffix) = match t.state.as_str() {
                                "done" => ("completed", String::new()),
                                "skipped" => ("completed", " — skipped".to_owned()),
                                _ => ("pending", String::new()),
                            };
                            (t.id.clone(), t.title.clone(), status, suffix)
                        })
                        .collect();
                }
                self.send_plan();
            }
            Event::TaskStarted { task, title, .. } => {
                self.set(task, "in_progress", String::new());
                self.send_plan();
                let id = c.id("task");
                if let Ok(mut t) = self.tasks.lock() {
                    t.insert(task.clone(), id.clone());
                }
                c.update(
                    &self.session,
                    json!({ "sessionUpdate": "tool_call", "toolCallId": id, "title": format!("{task} {title}"), "kind": "other", "status": "in_progress" }),
                );
            }
            Event::Stage {
                task,
                stage,
                state,
                detail,
                elapsed_ms,
                ..
            } => {
                let line = crate::progress::stage_line(
                    *stage,
                    *state,
                    detail.as_deref(),
                    *elapsed_ms,
                    crate::progress::Verbosity::Normal,
                );
                if let (Some(id), Some(line)) = (
                    self.tasks.lock().ok().and_then(|t| t.get(task).cloned()),
                    line,
                ) {
                    c.update(
                        &self.session,
                        json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "content": text(line.trim_start()) }),
                    );
                }
            }
            Event::ToolCall {
                tool,
                target,
                decision,
                ..
            } => {
                let status = if decision == "allow" {
                    "completed"
                } else {
                    "failed"
                };
                c.update(
                    &self.session,
                    json!({ "sessionUpdate": "tool_call", "toolCallId": c.id("call"), "title": format!("{tool} {target}"), "kind": kind(tool), "status": status }),
                );
            }
            Event::TaskFinished {
                task,
                outcome,
                message,
                ..
            } => {
                match outcome.as_str() {
                    "verified" => self.set(task, "completed", String::new()),
                    "regression_only" | "attested" | "skipped" => {
                        self.set(
                            task,
                            "completed",
                            format!(" — {}", outcome.replace('_', " ")),
                        );
                    }
                    other => self.set(task, "pending", format!(" — {}", other.replace('_', " "))),
                }
                self.send_plan();
                if let Some(id) = self.tasks.lock().ok().and_then(|t| t.get(task).cloned()) {
                    let ok = matches!(
                        outcome.as_str(),
                        "verified" | "regression_only" | "attested"
                    );
                    c.update(
                        &self.session,
                        json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": if ok { "completed" } else { "failed" }, "content": text(&format!("{outcome}: {message}")) }),
                    );
                }
            }
            _ => {}
        }
    }
}

/// A call that needs a person becomes the editor's permission request.
#[derive(Debug)]
struct Permissions {
    conn: Arc<ConnRef>,
    session: String,
    cancel: Arc<AtomicBool>,
}

/// The editor's kind for a call.
fn approval_kind(kind: &str) -> &'static str {
    match kind {
        "write" | "edit" => "edit",
        "run" => "execute",
        "network" => "fetch",
        _ => "other",
    }
}

/// The permission request's `toolCall.content`: a diff for a write or
/// edit, and what the person needs to decide as text.
fn approval_content(p: &Presentation) -> Value {
    let mut content = Vec::new();
    if let (Some(path), Some(new)) = (&p.file, &p.new_text) {
        let mut diff = json!({ "type": "diff", "path": path, "newText": new });
        if let Some(old) = &p.old_text {
            diff["oldText"] = json!(old);
        }
        content.push(diff);
    }
    let mut text = String::new();
    if !matches!(p.request.kind.as_str(), "write" | "edit") || p.new_text.is_none() {
        text.push_str(&p.body.join("\n"));
        text.push('\n');
    }
    text.push_str(&format!(
        "Why stopped: {}\nSandbox: {}\n",
        p.request.reason, p.sandbox
    ));
    if let Some(claim) = &p.request.claim {
        text.push_str(&format!("Model says (a claim): \"{claim}\"\n"));
    }
    text.push_str(&format!("Risk: {}", p.request.risk));
    content.push(json!({ "type": "content", "content": { "type": "text", "text": text } }));
    Value::Array(content)
}

impl Approver for Permissions {
    fn via(&self) -> String {
        let editor = self
            .conn
            .0
            .client
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        format!(
            "editor {}",
            if editor.is_empty() {
                "unknown"
            } else {
                &editor
            }
        )
    }

    fn ask(&self, p: &Presentation) -> Answer {
        // Who answers in the editor: the local person, through it.
        let via = self.via();
        let editor = via.trim_start_matches("editor ").to_owned();
        let by = || Some(Who::local(&via));
        if self.cancel.load(Ordering::SeqCst) {
            return Answer::stop(by());
        }
        let c = &self.conn.0;
        let call_id = c.id("approval");
        let task = p.request.task.rsplit('/').next().unwrap_or(&p.request.task);
        let options: Vec<Value> = p
            .options
            .iter()
            .map(|o| {
                let kind = match o.choice.as_str() {
                    "once" => "allow_once",
                    "task" | "run" => "allow_always",
                    _ => "reject_once",
                };
                json!({ "optionId": o.choice, "name": o.label, "kind": kind })
            })
            .collect();
        let reply = c.request(
            "session/request_permission",
            json!({
                "sessionId": self.session,
                "toolCall": {
                    "toolCallId": call_id,
                    "title": format!("{} {} — {task} · risk {}", p.title, p.subject, p.request.risk),
                    "kind": approval_kind(&p.request.kind),
                    "status": "pending",
                    "rawInput": p.request.arguments,
                    "content": approval_content(p),
                },
                "options": options,
            }),
        );
        let Some(reply) = reply else {
            return Answer::reject("the person cancelled the request", by());
        };
        let outcome = &reply["result"]["outcome"];
        if outcome["outcome"] == "cancelled" {
            return Answer::stop(by());
        }
        match outcome["optionId"].as_str() {
            Some("once") => Answer::approve(Scope::Once, None, by()),
            Some("task") => Answer::approve(Scope::Task, None, by()),
            Some("run") => Answer::approve(Scope::Run, None, by()),
            Some("stop") => Answer::stop(by()),
            _ => {
                let reason = self
                    .reason(&call_id, p)
                    .unwrap_or_else(|| format!("rejected in {editor}; the editor gave no reason"));
                Answer::reject(reason, by())
            }
        }
    }
}

impl Permissions {
    /// Asks the editor why, with a form, when it said it can show one.
    fn reason(&self, call_id: &str, p: &Presentation) -> Option<String> {
        let c = &self.conn.0;
        if !c.forms.load(Ordering::SeqCst) {
            return None;
        }
        let reply = c.request(
            "elicitation/create",
            json!({
                "sessionId": self.session,
                "toolCallId": call_id,
                "mode": "form",
                "message": format!("Why reject `{}`? The model is told.", p.subject),
                "requestedSchema": {
                    "type": "object",
                    "properties": { "reason": { "type": "string", "minLength": 1, "title": "Reason" } },
                    "required": ["reason"],
                },
            }),
        )?;
        let result = &reply["result"];
        (result["action"] == "accept")
            .then(|| result["content"]["reason"].as_str())
            .flatten()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned)
    }
}

/// One prompt: the speccoder command it names, run like the CLI runs it.
fn prompt(
    conn: &Arc<ConnRef>,
    session: &str,
    cwd: &std::path::Path,
    line: &str,
    cancel: &Arc<AtomicBool>,
) {
    let c = &conn.0;
    let say = |t: &str| {
        c.update(session, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": t } }))
    };
    let line = line.trim();
    let Some(command) = line.strip_prefix('/') else {
        let names: Vec<String> = COMMANDS.iter().map(|(n, ..)| format!("/{n}")).collect();
        say(&format!(
            "speccoder runs Spec Kit features with evidence. Use a command: {}. Start a feature with `/specify <what to build>`.",
            names.join(", ")
        ));
        return;
    };
    let (name, args) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    let args = args.trim();
    let options = speccoder_runtime::RunOptions {
        approver: Some(Arc::new(Permissions {
            conn: Arc::clone(conn),
            session: session.to_owned(),
            cancel: Arc::clone(cancel),
        })),
        progress: Some(Arc::new(Updates {
            conn: Arc::clone(conn),
            session: session.to_owned(),
            tasks: Mutex::new(HashMap::new()),
            plan: Mutex::new(Vec::new()),
        })),
        cancel: Some(Arc::clone(cancel)),
        ..speccoder_runtime::RunOptions::default()
    };
    let result = match name {
        "run" => {
            let words: Vec<&str> = args.split_whitespace().collect();
            let converge = words.contains(&"--converge");
            let parallel = words
                .iter()
                .position(|w| *w == "--parallel")
                .and_then(|i| words.get(i + 1))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(1);
            match words
                .iter()
                .find(|w| !w.starts_with("--") && w.parse::<usize>().is_err())
            {
                Some(task) => speccoder_runtime::run(cwd, None, task, &options),
                None => speccoder_runtime::run_feature(
                    cwd,
                    None,
                    &speccoder_runtime::RunOptions {
                        parallel,
                        ..options
                    },
                    converge,
                ),
            }
        }
        "converge" => speccoder_runtime::converge(cwd, None),
        "report" => speccoder_runtime::status::report(cwd, None),
        "status" => speccoder_runtime::progress::status(cwd),
        "cost" => speccoder_runtime::controls::cost(cwd, None),
        skill if COMMANDS.iter().any(|(n, ..)| *n == skill) => {
            speccoder_runtime::hosting::run(cwd, skill, args, &options)
        }
        other => {
            say(&format!("`/{other}` is not a speccoder command."));
            return;
        }
    };
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let _ = crate::output::render_into(&result, false, &mut out, &mut err);
    let mut shown = String::from_utf8_lossy(&out).into_owned();
    shown.push_str(&String::from_utf8_lossy(&err));
    if shown.trim().is_empty() {
        shown = result.message().to_owned();
    }
    say(&format!("```text\n{}\n```", shown.trim_end()));
}

/// Serves ACP on `input`/`output` until the editor closes the connection.
pub fn serve(input: impl BufRead, output: Box<dyn Write + Send>) {
    let conn = Arc::new(ConnRef(Conn {
        out: Mutex::new(output),
        pending: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
        client: Mutex::new(String::new()),
        forms: AtomicBool::new(false),
    }));
    let mut sessions: HashMap<String, (PathBuf, Arc<AtomicBool>)> = HashMap::new();
    let mut workers = Vec::new();
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let c = &conn.0;
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            c.fail(&Value::Null, -32700, "parse error");
            continue;
        };
        let Some(method) = msg["method"].as_str() else {
            // An answer to one of the agent's requests.
            if let Some(id) = msg["id"].as_str()
                && let Some(tx) = c.pending.lock().ok().and_then(|mut p| p.remove(id))
            {
                let _ = tx.send(msg.clone());
            }
            continue;
        };
        let id = msg.get("id").cloned();
        let params = &msg["params"];
        match (method, id) {
            ("initialize", Some(id)) => {
                if let (Some(name), Ok(mut client)) =
                    (params["clientInfo"]["name"].as_str(), c.client.lock())
                {
                    *client = name.to_owned();
                }
                c.forms.store(
                    !params["clientCapabilities"]["elicitation"]["form"].is_null(),
                    Ordering::SeqCst,
                );
                c.respond(
                &id,
                json!({
                    "protocolVersion": 1,
                    "agentCapabilities": { "loadSession": false, "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false } },
                    "authMethods": [],
                    "agentInfo": { "name": "speccoder", "title": "speccoder", "version": env!("CARGO_PKG_VERSION") },
                }),
            );
            }
            ("authenticate", Some(id)) => c.respond(&id, json!({})),
            ("session/new", Some(id)) => {
                let cwd = params["cwd"]
                    .as_str()
                    .map_or_else(|| PathBuf::from("."), PathBuf::from);
                let session = c.id("session");
                sessions.insert(session.clone(), (cwd, Arc::new(AtomicBool::new(false))));
                c.respond(&id, json!({ "sessionId": session }));
                let commands: Vec<Value> = COMMANDS
                    .iter()
                    .map(|(name, description, hint)| match hint {
                        Some(h) => json!({ "name": name, "description": description, "input": { "hint": h } }),
                        None => json!({ "name": name, "description": description }),
                    })
                    .collect();
                c.update(&session, json!({ "sessionUpdate": "available_commands_update", "availableCommands": commands }));
            }
            ("session/prompt", Some(id)) => {
                let session = params["sessionId"].as_str().unwrap_or_default().to_owned();
                let Some((cwd, cancel)) = sessions.get(&session).cloned() else {
                    c.fail(&id, -32602, "unknown session");
                    continue;
                };
                cancel.store(false, Ordering::SeqCst);
                let text: String = params["prompt"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let conn = Arc::clone(&conn);
                workers.push(std::thread::spawn(move || {
                    prompt(&conn, &session, &cwd, &text, &cancel);
                    let stop = if cancel.load(Ordering::SeqCst) {
                        "cancelled"
                    } else {
                        "end_turn"
                    };
                    conn.0.respond(&id, json!({ "stopReason": stop }));
                }));
            }
            ("session/cancel", None) => {
                if let Some((_, cancel)) =
                    params["sessionId"].as_str().and_then(|s| sessions.get(s))
                {
                    cancel.store(true, Ordering::SeqCst);
                }
            }
            (_, Some(id)) => c.fail(&id, -32601, &format!("method not found: {method}")),
            (_, None) => {}
        }
    }
    for w in workers {
        let _ = w.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer the test reads from.
    struct Pipe(mpsc::Sender<Vec<u8>>);

    impl Write for Pipe {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let _ = self.0.send(buf.to_vec());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn conn(forms: bool) -> (Arc<ConnRef>, mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel();
        let conn = Arc::new(ConnRef(Conn {
            out: Mutex::new(Box::new(Pipe(tx))),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            client: Mutex::new("Zed".to_owned()),
            forms: AtomicBool::new(forms),
        }));
        (conn, rx)
    }

    /// The next JSON line the agent sent.
    fn next(rx: &mpsc::Receiver<Vec<u8>>) -> Value {
        let mut line = String::new();
        while !line.ends_with('\n') {
            line.push_str(&String::from_utf8_lossy(&rx.recv().unwrap_or_default()));
        }
        serde_json::from_str(line.trim()).unwrap_or_default()
    }

    /// Answers the agent's request `req` with `result`.
    fn reply(conn: &ConnRef, req: &Value, result: Value) {
        let id = req["id"].as_str().unwrap_or_default().to_owned();
        let tx = conn
            .0
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        let _ = tx.map(|t| t.send(json!({ "id": id, "result": result })));
    }

    fn presentation(tool: &str, arguments: Value, kind: &str, risk: &str) -> Presentation {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        speccoder_runtime::approvals::present(
            &speccoder_runtime::approvals::Request {
                task: "001/T004".to_owned(),
                task_title: "Handle Unicode".to_owned(),
                turn: 2,
                tool: tool.to_owned(),
                arguments,
                kind: kind.to_owned(),
                reason: "why".to_owned(),
                covers: "c".to_owned(),
                risk: risk.to_owned(),
                claim: Some("it is needed".to_owned()),
                options: speccoder_runtime::approvals::options(risk, true),
            },
            &speccoder_runtime::approvals::PresentContext {
                root: Some(dir.path().to_path_buf()),
                sandbox: Some("Seatbelt".to_owned()),
                network: "none".to_owned(),
                file_text: None,
            },
        )
    }

    fn ask(conn: &Arc<ConnRef>, p: Presentation) -> std::thread::JoinHandle<Answer> {
        let perms = Permissions {
            conn: Arc::clone(conn),
            session: "s".to_owned(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        std::thread::spawn(move || perms.ask(&p))
    }

    #[test]
    fn an_edit_is_a_permission_request_with_a_diff() {
        let (conn, rx) = conn(false);
        let asking = ask(
            &conn,
            presentation(
                "edit_file",
                json!({ "path": "Cargo.toml", "old_string": "serde = \"1\"\n", "new_string": "serde = \"1\"\nx = \"2\"\n" }),
                "edit",
                "medium",
            ),
        );
        let req = next(&rx);
        assert_eq!(req["method"], "session/request_permission");
        let call = &req["params"]["toolCall"];
        assert_eq!(call["kind"], "edit");
        assert_eq!(
            call["title"],
            "Edit this file? Cargo.toml — T004 · risk medium"
        );
        assert_eq!(call["content"][0]["type"], "diff");
        assert!(
            call["content"][0]["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("/Cargo.toml")
        );
        assert_eq!(
            call["content"][0]["oldText"],
            "[dependencies]\nserde = \"1\"\n"
        );
        assert_eq!(
            call["content"][0]["newText"],
            "[dependencies]\nserde = \"1\"\nx = \"2\"\n"
        );
        assert!(
            call["content"][1]["content"]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("Model says (a claim): \"it is needed\"")
        );
        let ids: Vec<&str> = req["params"]["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o["optionId"].as_str())
            .collect();
        assert_eq!(ids, ["once", "task", "run", "reject", "stop"]);
        reply(
            &conn,
            &req,
            json!({ "outcome": { "outcome": "selected", "optionId": "run" } }),
        );
        let a = asking.join().unwrap_or_else(|_| panic!("asking"));
        assert_eq!(
            (a.decision, a.scope),
            (speccoder_runtime::approvals::Decision::Approve, Scope::Run)
        );
        // the local person, through the editor that asked — a name the
        // connection reports, recorded as such on the decision.
        let by = a.by.clone().unwrap_or_else(|| panic!("no one answered"));
        assert_eq!(
            (by.via.as_str(), by.basis.as_str()),
            ("editor Zed", "connected")
        );
        let d = speccoder_runtime::approvals::tasks::decision_for(&a, &by, None)
            .unwrap_or_else(|| panic!("no decision"));
        let op = d
            .decided
            .operator()
            .unwrap_or_else(|| panic!("no operator"));
        assert_eq!(op.basis().to_string(), "connected");
        assert_eq!(d.amendment["via"], "editor Zed");
    }

    #[test]
    fn a_new_file_has_no_old_text_and_high_risk_no_run_option() {
        let (conn, rx) = conn(false);
        let asking = ask(
            &conn,
            presentation(
                "write_file",
                json!({ "path": "src/new.rs", "content": "fn x() {}\n" }),
                "write",
                "low",
            ),
        );
        let req = next(&rx);
        assert!(
            req["params"]["toolCall"]["content"][0]
                .get("oldText")
                .is_none()
        );
        reply(
            &conn,
            &req,
            json!({ "outcome": { "outcome": "cancelled" } }),
        );
        let a = asking.join().unwrap_or_else(|_| panic!("asking"));
        assert_eq!(
            a.decision,
            speccoder_runtime::approvals::Decision::Stop,
            "cancelled is a stop, never a rejection"
        );

        let asking = ask(
            &conn,
            presentation(
                "run_command",
                json!({ "argv": ["curl", "x"] }),
                "run",
                "high",
            ),
        );
        let req = next(&rx);
        assert_eq!(req["params"]["toolCall"]["kind"], "execute");
        let ids: Vec<&str> = req["params"]["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o["optionId"].as_str())
            .collect();
        assert_eq!(ids, ["once", "task", "reject", "stop"]);
        reply(
            &conn,
            &req,
            json!({ "outcome": { "outcome": "selected", "optionId": "reject" } }),
        );
        let a = asking.join().unwrap_or_else(|_| panic!("asking"));
        assert_eq!(
            a.reason.as_deref(),
            Some("rejected in Zed; the editor gave no reason")
        );
    }

    #[test]
    fn a_rejection_asks_for_the_reason_with_a_form() {
        let (conn, rx) = conn(true);
        let asking = ask(
            &conn,
            presentation(
                "run_command",
                json!({ "argv": ["curl", "x"] }),
                "run",
                "high",
            ),
        );
        let req = next(&rx);
        reply(
            &conn,
            &req,
            json!({ "outcome": { "outcome": "selected", "optionId": "reject" } }),
        );
        let form = next(&rx);
        assert_eq!(form["method"], "elicitation/create");
        assert_eq!(
            form["params"]["toolCallId"],
            req["params"]["toolCall"]["toolCallId"]
        );
        assert_eq!(form["params"]["requestedSchema"]["required"][0], "reason");
        reply(
            &conn,
            &form,
            json!({ "action": "accept", "content": { "reason": "no network" } }),
        );
        let a = asking.join().unwrap_or_else(|_| panic!("asking"));
        assert_eq!(a.reason.as_deref(), Some("no network"));
    }

    #[test]
    fn the_plan_follows_the_run() {
        use speccoder_runtime::progress::PlannedTask;
        let (conn, rx) = conn(false);
        let u = Updates {
            conn: Arc::clone(&conn),
            session: "s".to_owned(),
            tasks: Mutex::new(HashMap::new()),
            plan: Mutex::new(Vec::new()),
        };
        let t = |id: &str, state: &str| PlannedTask {
            id: id.to_owned(),
            title: format!("do {id}"),
            state: state.to_owned(),
        };
        u.event(&Event::RunStarted {
            feature: "f".to_owned(),
            tasks: vec![t("T001", "done"), t("T002", "ready"), t("T003", "blocked")],
        });
        let plan = next(&rx);
        let statuses: Vec<&str> = plan["params"]["update"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["status"].as_str())
            .collect();
        assert_eq!(statuses, ["completed", "pending", "pending"]);
        u.event(&Event::TaskStarted {
            feature: "f".to_owned(),
            task: "T002".to_owned(),
            title: "do T002".to_owned(),
        });
        let plan = next(&rx);
        assert_eq!(
            plan["params"]["update"]["entries"][1]["status"],
            "in_progress"
        );
        let _tool_call = next(&rx);
        u.event(&Event::TaskFinished {
            task: "T002".to_owned(),
            outcome: "regression_only".to_owned(),
            message: "m".to_owned(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: None,
        });
        let plan = next(&rx);
        assert_eq!(
            plan["params"]["update"]["entries"][1]["content"],
            "T002 do T002 — regression only"
        );
        assert_eq!(
            plan["params"]["update"]["entries"][1]["status"],
            "completed"
        );
    }
}
