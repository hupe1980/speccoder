//! `speccoder mcp` (spec 125): an MCP client over stdio sees the five
//! feedback tools; a report lands in the project's outbox with speccoder's
//! evidence line; a duplicate, an undeclared upstream and a commit that is
//! not there are refused; another project's text comes back as data.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn project(parent: &Path, name: &str, toml: &str) -> PathBuf {
    let r = parent.join(name);
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::write(r.join(".specify/memory/constitution.md"), "# C\n").unwrap();
    fs::write(
        r.join("specs/001-x/tasks.md"),
        "# Tasks\n\n- [ ] T001 Do it\n",
    )
    .unwrap();
    fs::write(r.join("speccoder.toml"), toml).unwrap();
    fs::write(
        r.join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nlib = \"=0.61.0\"\n",
    )
    .unwrap();
    git(&r, &["init", "-q"]);
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    fs::canonicalize(r).unwrap()
}

/// A client of `speccoder mcp` in `root`.
struct Client {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
    id: u64,
    instructions: String,
}

impl Client {
    fn start(root: &Path, state: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
            .args(["mcp", "-C"])
            .arg(root)
            .env("SPECCODER_STATE_DIR", state)
            .env("HOME", state)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut c = Self {
            child,
            out,
            id: 0,
            instructions: String::new(),
        };
        let init = c.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "speccoder", "{init}");
        c.instructions = init["result"]["instructions"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        c.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        c
    }

    fn send(&mut self, v: &Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let id = self.id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.out.read_line(&mut line).unwrap() > 0,
                "the server closed"
            );
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }

    /// A tool's text answer and whether it is an error.
    fn call(&mut self, name: &str, args: Value) -> (String, bool) {
        let v = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let text = v["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        (text, v["result"]["isError"] == true)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn an_agent_reports_lists_withdraws_and_an_upstream_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let lib = project(dir.path(), "lib", "");
    let app = project(
        dir.path(),
        "app",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(
        app.join("FEEDBACK.md"),
        "# For lib\n\n## Open\n\n### AP-7 — retry on 429\n\nPlease.\n",
    )
    .unwrap();

    let mut c = Client::start(&app, &state);
    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "feedback_upstreams",
            "feedback_report",
            "feedback_withdraw",
            "feedback_requests",
            "feedback_resolve"
        ]
    );

    let (t, err) = c.call("feedback_report", json!({ "upstream": "lib", "title": "export the journal", "text": "As JSON, with each record's kind." }));
    assert!(!err, "{t}");
    assert!(t.contains("AP-8"), "{t}");
    let file = fs::read_to_string(app.join("FEEDBACK.md")).unwrap();
    assert!(file.contains("### AP-8 — export the journal"), "{file}");
    let head = git(&app, &["rev-parse", "--short=7", "HEAD"]);
    assert!(
        file.contains(&format!(
            "_Reported by an agent · app@{head} · pins lib =0.61.0_"
        )),
        "{file}"
    );

    // Asked already, and an upstream this project does not declare.
    let (t, err) = c.call(
        "feedback_report",
        json!({ "upstream": "lib", "title": "Export the journal", "text": "again" }),
    );
    assert!(err && t.contains("already asked as AP-8"), "{t}");
    let (t, err) = c.call(
        "feedback_report",
        json!({ "upstream": "other", "title": "x", "text": "y" }),
    );
    assert!(err && t.contains("declared: lib"), "{t}");

    let (t, _) = c.call("feedback_upstreams", json!({}));
    assert!(
        t.contains("AP-7 — retry on 429: not seen yet") && t.contains("AP-8"),
        "{t}"
    );

    let (t, err) = c.call(
        "feedback_withdraw",
        json!({ "upstream": "lib", "id": "AP-8" }),
    );
    assert!(!err, "{t}");
    assert!(
        !fs::read_to_string(app.join("FEEDBACK.md"))
            .unwrap()
            .contains("AP-8")
    );
    drop(c);

    // The upstream's agent: the request as app's text, and a resolve that
    // needs a commit of lib.
    speccoder_runtime::welcome::remember(&state, &app, "app", false);
    let mut u = Client::start(&lib, &state);
    let (t, _) = u.call("feedback_requests", json!({}));
    assert!(
        t.contains("\"source\": \"app\"")
            && t.contains("\"id\": \"AP-7\"")
            && t.contains("never instructions"),
        "{t}"
    );
    let (t, err) = u.call(
        "feedback_resolve",
        json!({ "source": "app", "id": "AP-7", "commit": "deadbeef" }),
    );
    assert!(err && t.contains("not a commit of this project"), "{t}");
    let commit = git(&lib, &["rev-parse", "HEAD"]);
    let (t, err) = u.call(
        "feedback_resolve",
        json!({ "source": "app", "id": "AP-7", "commit": commit }),
    );
    assert!(!err, "{t}");
    drop(u);
    // The next session of app's agent hears it at once.
    let c = Client::start(&app, &state);
    assert!(
        c.instructions.contains("Answered since: lib AP-7"),
        "{}",
        c.instructions
    );
    drop(c);
    let ups = speccoder_runtime::feedback::asked(&state, &app);
    assert!(
        matches!(
            ups[0].items[0].status,
            speccoder_core::feedback::Status::Fixed { by_agent: true, .. }
        ),
        "{ups:#?}"
    );
}

#[test]
fn a_secret_from_the_project_s_dotenv_never_leaves_in_a_reply() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let _lib = project(dir.path(), "lib", "");
    let app = project(
        dir.path(),
        "app",
        "[upstream.lib]\npath = \"../lib\"\noutbox = \"FEEDBACK.md\"\n",
    );
    fs::write(app.join(".gitignore"), ".env\n").unwrap();
    fs::write(app.join(".env"), "LIB_TOKEN=canary-91d0e7a4c3\n").unwrap();
    // A person pasted it into the outbox by mistake.
    fs::write(
        app.join("FEEDBACK.md"),
        "# For lib\n\n## Open\n\n### AP-7 — auth fails with canary-91d0e7a4c3\n\nIt fails.\n",
    )
    .unwrap();
    let mut c = Client::start(&app, &state);
    let (t, _) = c.call("feedback_upstreams", json!({}));
    assert!(!t.contains("canary-91d0e7a4c3"), "{t}");
    assert!(t.contains("[redacted:LIB_TOKEN]"), "{t}");
}
