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
use speccoder_runtime::approvals::{Answer, Approver, Request};
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
            Event::TaskStarted { task, title, .. } => {
                let id = c.id("task");
                if let Ok(mut t) = self.tasks.lock() {
                    t.insert(task.clone(), id.clone());
                }
                c.update(
                    &self.session,
                    json!({ "sessionUpdate": "tool_call", "toolCallId": id, "title": format!("{task} {title}"), "kind": "other", "status": "in_progress" }),
                );
            }
            Event::Phase { task, phase } => {
                if let Some(id) = self.tasks.lock().ok().and_then(|t| t.get(task).cloned()) {
                    c.update(
                        &self.session,
                        json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "content": text(phase) }),
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

impl Approver for Permissions {
    fn ask(&self, request: &Request) -> Answer {
        // Who answers in the editor: the local person, through it.
        let editor = self
            .conn
            .0
            .client
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        let by = Some(speccoder_runtime::approvals::Who::local(&format!(
            "editor {}",
            if editor.is_empty() {
                "unknown"
            } else {
                editor.as_str()
            }
        )));
        let reject = |reason: &str| Answer {
            approved: false,
            for_task: false,
            reason: Some(reason.to_owned()),
            by: by.clone(),
        };
        if self.cancel.load(Ordering::SeqCst) {
            return reject("the person cancelled the run");
        }
        let c = &self.conn.0;
        let reply = c.request(
            "session/request_permission",
            json!({
                "sessionId": self.session,
                "toolCall": {
                    "toolCallId": c.id("approval"),
                    "title": format!("{} {} — {} ({} risk)", request.task, request.tool, request.reason, request.risk),
                    "kind": kind(&request.tool),
                    "status": "pending",
                    "rawInput": request.arguments,
                },
                "options": [
                    { "optionId": "once", "name": "Allow once", "kind": "allow_once" },
                    { "optionId": "task", "name": format!("Allow for this task: {}", request.covers), "kind": "allow_always" },
                    { "optionId": "reject", "name": "Reject", "kind": "reject_once" },
                ],
            }),
        );
        let outcome = reply.as_ref().map(|r| &r["result"]["outcome"]);
        match outcome
            .and_then(|o| (o["outcome"] == "selected").then(|| o["optionId"].as_str()))
            .flatten()
        {
            Some("once") => Answer {
                approved: true,
                for_task: false,
                reason: None,
                by: by.clone(),
            },
            Some("task") => Answer {
                approved: true,
                for_task: true,
                reason: None,
                by: by.clone(),
            },
            Some(_) => reject("rejected in the editor"),
            None => reject("the person cancelled the request"),
        }
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

    #[test]
    fn a_call_that_needs_a_person_is_a_permission_request() {
        let (tx, rx) = mpsc::channel();
        let conn = Arc::new(ConnRef(Conn {
            out: Mutex::new(Box::new(Pipe(tx))),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            client: Mutex::new("Zed".to_owned()),
        }));
        let perms = Permissions {
            conn: Arc::clone(&conn),
            session: "s".to_owned(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let asking = std::thread::spawn(move || {
            perms.ask(&Request {
                task: "001/T005".to_owned(),
                tool: "run_command".to_owned(),
                arguments: json!({ "argv": ["cargo", "build"] }),
                reason: "not a configured check".to_owned(),
                covers: "`cargo build`".to_owned(),
                risk: "high".to_owned(),
            })
        });
        let mut line = String::new();
        while !line.ends_with('\n') {
            line.push_str(&String::from_utf8_lossy(&rx.recv().unwrap_or_default()));
        }
        let req: Value = serde_json::from_str(line.trim()).unwrap_or_default();
        assert_eq!(req["method"], "session/request_permission");
        assert_eq!(req["params"]["toolCall"]["kind"], "execute");
        assert_eq!(req["params"]["options"][1]["kind"], "allow_always");
        let id = req["id"].as_str().unwrap_or_default().to_owned();
        let tx = conn
            .0
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        let _ = tx.map(|t| t.send(json!({ "id": id, "result": { "outcome": { "outcome": "selected", "optionId": "task" } } })));
        let answer = asking.join().unwrap_or(Answer {
            approved: false,
            for_task: false,
            reason: None,
            by: None,
        });
        assert!(answer.approved && answer.for_task);
        // the local person, through the editor that asked.
        let by = answer.by.unwrap_or_else(|| panic!("no approver"));
        assert_eq!(by.via, "editor Zed");
    }
}
