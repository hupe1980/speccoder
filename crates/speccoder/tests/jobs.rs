//! The web host's jobs: every runnable kind, started over `POST /api/run`
//! with the same input on the same project, ends in the result it ended
//! in before the host's own mapping moved into the runtime's one
//! dispatcher. The goldens under `tests/fixtures/jobs/` were recorded
//! through that mapping (`SPECCODER_BLESS=1` writes them); a golden diff
//! means behaviour changed — fix the code, not the golden.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[acceptance]\ngenerate = false\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Sessions MUST expire after [NEEDS CLARIFICATION: how long?].\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Dana Example",
            "-c",
            "user.email=d@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The graph fixture, committed, with a spec to review and a model
/// configured whose key is not there (a model run stops before it starts).
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let graph =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(graph.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Dana Example"]);
    git(r, &["config", "user.email", "d@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn http(port: u16, method: &str, path: &str, auth: &str, body: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

/// One case per kind: its golden's name, the runnable as the page sends
/// it, and the person's input.
fn cases() -> Vec<(&'static str, Value, Value)> {
    vec![
        (
            "approve-and-run-question",
            json!({ "kind": "approve_and_run", "mode": "ask", "grants": [] }),
            Value::Null,
        ),
        (
            "skill-input",
            json!({ "kind": "skill", "name": "specify", "arguments": "" }),
            json!("a login page"),
        ),
        (
            "skill-carried",
            json!({ "kind": "skill", "name": "plan", "arguments": "" }),
            Value::Null,
        ),
        (
            "run-task",
            json!({ "kind": "run", "task": "T005" }),
            Value::Null,
        ),
        (
            "run-feature",
            json!({ "kind": "run", "task": null }),
            Value::Null,
        ),
        ("converge", json!({ "kind": "converge" }), Value::Null),
        (
            "approve-none",
            json!({ "kind": "approve", "task": "T005", "call": "attempt:T005", "digest": "" }),
            Value::Null,
        ),
        (
            "reject-empty",
            json!({ "kind": "reject", "task": "T005", "call": "attempt:T005", "digest": "" }),
            json!(" "),
        ),
        (
            "reject-reason",
            json!({ "kind": "reject", "task": "T005", "call": "attempt:T005", "digest": "" }),
            json!("no new files"),
        ),
        (
            "accept-empty",
            json!({ "kind": "accept", "task": "T006" }),
            Value::Null,
        ),
        (
            "accept-reason",
            json!({ "kind": "accept", "task": "T006" }),
            json!("done by hand"),
        ),
        (
            "skip-reason",
            json!({ "kind": "skip", "task": "T007" }),
            json!("not this release"),
        ),
        (
            "nudge-note",
            json!({ "kind": "nudge", "task": "T008" }),
            json!("use the session type"),
        ),
        (
            "done",
            json!({ "kind": "done", "task": "T010" }),
            Value::Null,
        ),
        // The hint is kept, then the attempt stops before it starts (no key).
        (
            "hint-note",
            json!({ "kind": "hint", "task": "T005" }),
            json!("the repository is octo/md2csv"),
        ),
        (
            "rule-reason",
            json!({ "kind": "rule", "task": "T009" }),
            json!("the tests are right"),
        ),
        (
            "review-answer",
            json!({ "kind": "review", "ids": ["Q1"], "verdict": "answer" }),
            json!("thirty minutes"),
        ),
        (
            "review-edit-empty",
            json!({ "kind": "review", "ids": ["FR-001"], "verdict": "edit" }),
            Value::Null,
        ),
        (
            "review-approve",
            json!({ "kind": "review", "ids": [], "verdict": "approve" }),
            Value::Null,
        ),
        (
            "commit",
            json!({ "kind": "commit", "paths": ["specs/001-graph/review.json"] }),
            Value::Null,
        ),
        (
            "keep-set-aside-none",
            json!({ "kind": "keep_set_aside", "task": "T010" }),
            Value::Null,
        ),
        (
            "drop-set-aside-none",
            json!({ "kind": "drop_set_aside", "task": "T010" }),
            Value::Null,
        ),
        (
            "fix-setup-nothing",
            json!({ "kind": "fix_setup" }),
            Value::Null,
        ),
        (
            "issue-comment-no-remote",
            json!({ "kind": "issue_comment", "issue": 42, "assign": false }),
            Value::Null,
        ),
        (
            "pull-request-no-remote",
            json!({ "kind": "pull_request" }),
            Value::Null,
        ),
        ("feature-empty", json!({ "kind": "feature" }), Value::Null),
        (
            "feature-name",
            json!({ "kind": "feature" }),
            json!("001-graph"),
        ),
        (
            "oracle-tests-acceptance-off",
            json!({ "kind": "oracle_tests", "req": "US1", "op": "write", "path": null, "scenario": null, "reading": null }),
            Value::Null,
        ),
        (
            "comment-delete-none",
            json!({ "kind": "comment", "op": "delete", "anchor": null, "id": "c9" }),
            Value::Null,
        ),
        (
            "comments-send-none",
            json!({ "kind": "comments_send", "ids": [] }),
            Value::Null,
        ),
        (
            "revision-undo-none",
            json!({ "kind": "revision_undo", "turn": null }),
            Value::Null,
        ),
        (
            "code-nudge-note",
            json!({ "kind": "code_nudge", "task": "T008", "comments": [{ "path": "src/lib.rs", "line": 1, "quote": "pub fn ok() {}", "text": "name it" }] }),
            Value::Null,
        ),
        (
            "quick-change-recorded",
            json!({ "kind": "quick_change", "request": "Log every sign-in", "checks": ["a sign-in is logged"], "run": false }),
            Value::Null,
        ),
        (
            "quick-change-run",
            json!({ "kind": "quick_change", "request": "Log every sign-out", "checks": [], "run": true }),
            Value::Null,
        ),
    ]
}

/// Paths, commit ids and times as placeholders.
fn normalise(v: &Value, paths: &[(String, &str)]) -> Value {
    match v {
        Value::String(s) => {
            let mut s = s.clone();
            for (p, name) in paths {
                s = s.replace(p.as_str(), name);
            }
            Value::String(hex(&s))
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| normalise(x, paths)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    let x = match x {
                        Value::Number(n) if n.as_u64().is_some_and(|n| n > 1_000_000_000) => {
                            json!(0)
                        }
                        // A commit's day: the day the test runs.
                        Value::String(d) if k == "date" && d.len() == 10 => json!("<date>"),
                        x => normalise(x, paths),
                    };
                    (k.clone(), x)
                })
                .collect(),
        ),
        v => v.clone(),
    }
}

fn hex(s: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let letters = run.chars().any(|c| c.is_ascii_alphabetic());
        let digits = run.chars().any(|c| c.is_ascii_digit());
        // After `as ` a run is the commit's id even when it happens to be
        // all digits or all letters; elsewhere a plain number stays.
        let named = out.ends_with(" as ");
        if run.len() >= 7 && (named || (letters && digits)) {
            out.push_str("<hex>");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if matches!(c, '0'..='9' | 'a'..='f') {
            run.push(c);
        } else if c.is_ascii_alphanumeric() {
            out.push_str(&run);
            run.clear();
            out.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[test]
fn every_kind_through_the_job_endpoint_gives_the_result_it_gave_before() {
    let repo = repo();
    let state = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(repo.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let _host = Killed(child);
    let ready = support::checked(serde_json::from_str(&first).unwrap());
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let url = ready["data"]["url"].as_str().unwrap();
    let auth = format!(
        "Authorization: Bearer {}\r\n",
        url.split_once("#token=").unwrap().1
    );

    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    let listen = format!(
        "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    std::thread::spawn(move || {
        use std::io::Write;
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(listen.as_bytes()).unwrap();
        for line in BufReader::new(s).lines().map_while(Result::ok) {
            if let Some(data) = line.strip_prefix("data:")
                && let Ok(v) = serde_json::from_str::<Value>(data.trim())
                && tx.send(v).is_err()
            {
                break;
            }
        }
    });
    let next = |want: &str| -> Value {
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let v = rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no {want} event"));
            if v["type"] == want {
                return v;
            }
        }
    };
    next("hello");

    let mut paths = Vec::new();
    for (p, name) in [
        (repo.path(), "<root>"),
        (state.path(), "<state>"),
        (home.path(), "<home>"),
    ] {
        if let Ok(c) = fs::canonicalize(p) {
            paths.push((c.display().to_string(), name));
        }
        paths.push((p.display().to_string(), name));
    }
    paths.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
    let bless = std::env::var_os("SPECCODER_BLESS").is_some();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jobs");
    let mut differ = Vec::new();
    for (name, run, input) in cases() {
        let start = json!({ "run": run, "input": input, "feature": null, "command": name });
        let (status, body) = http(port, "POST", "/api/run", &auth, &start.to_string());
        assert_eq!(status, 200, "{name}: {body}");
        let end = next("done");
        let result = support::checked(end["result"].clone());
        let got = json!({
            "result": normalise(&result, &paths),
            "head": git(repo.path(), &["log", "-1", "--format=%s"]).trim(),
        });
        let text = format!("{}\n", serde_json::to_string_pretty(&got).unwrap());
        let file = dir.join(format!("{name}.json"));
        if bless {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&file, text).unwrap();
        } else if fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
            differ.push(format!("{name}:\n{text}"));
        }
    }
    assert!(
        differ.is_empty(),
        "job results differ from the goldens (run with SPECCODER_BLESS=1 only for an intended change):\n{}",
        differ.join("\n")
    );
}
