//! A scripted run end to end: `--events` (F-056) streams JSON Lines that a
//! consumer reads with nothing but the published schemas — `seq` without
//! gaps, the lifecycle in order, the result last — and a reader that goes
//! away does not break the run; at a terminal the dashboard hosts it
//! (F-058), driven through a pseudo-terminal.

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

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A model mock answering `script`, and a committed graph project using it.
fn project(
    script: &Value,
) -> (
    (Killed, tempfile::TempDir),
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let mock_dir = tempfile::tempdir().unwrap();
    fs::write(mock_dir.path().join("script.json"), script.to_string()).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py");
    let mock = Killed(
        Command::new("python3")
            .arg("-I")
            .arg(&fixture)
            .arg(mock_dir.path())
            .arg(mock_dir.path().join("script.json"))
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !mock_dir.path().join("port").exists() && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let port = fs::read_to_string(mock_dir.path().join("port")).unwrap();
    let repo = tempfile::tempdir().unwrap();
    let r = repo.path();
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
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n\n[acceptance]\ngenerate = false\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    ((mock, mock_dir), repo, tempfile::tempdir().unwrap())
}

fn script() -> Value {
    let done = json!({ "summary": "looked", "status": "done", "files_changed": [], "checks_run": [], "notes": [] });
    json!([
        { "tool": "list_files", "args": { "path": "src" } },
        { "content": done.to_string() },
    ])
}

fn has_python() -> bool {
    Command::new("python3").arg("--version").output().is_ok()
}

#[test]
fn a_run_streams_its_lifecycle_and_ends_in_the_result() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let (_mock, repo, state) = project(&script());
    let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--events", "run", "--task", "T005", "--no-prompt"])
        .arg("-C")
        .arg(repo.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .output()
        .unwrap();
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    // Read as a consumer would: every line through the published schemas.
    let lines: Vec<Value> = text.lines().map(support::event).collect();
    let seqs: Vec<u64> = lines.iter().map(|l| l["seq"].as_u64().unwrap()).collect();
    assert_eq!(
        seqs,
        (1..=lines.len() as u64).collect::<Vec<_>>(),
        "seq has gaps"
    );
    let types: Vec<&str> = lines.iter().map(|l| l["type"].as_str().unwrap()).collect();
    assert_eq!(types.first(), Some(&"session.started"), "{types:?}");
    assert_eq!(types.last(), Some(&"result"), "{types:?}");
    assert_eq!(types.iter().filter(|t| **t == "result").count(), 1);
    let at = |t: &str| {
        types
            .iter()
            .position(|x| *x == t)
            .unwrap_or_else(|| panic!("no {t}: {types:?}"))
    };
    assert!(at("task.started") < at("stage.started"));
    assert!(at("stage.started") < at("tool_call"));
    assert!(at("tool_call") < at("task.finished"));
    let finished = &lines[at("task.finished")];
    assert!(
        finished["run"].as_str().is_some_and(|r| !r.is_empty()),
        "{finished}"
    );
    let result = &lines.last().unwrap()["result"];
    assert_eq!(result["command"], "run");
    assert_eq!(
        result["exit_code"].as_i64(),
        out.status.code().map(i64::from)
    );
    assert_eq!(lines[0]["capabilities"][0], "events_v2");
}

#[test]
fn events_and_json_together_are_one_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--events", "--json", "report"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<Value> = text.lines().map(support::event).collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[1]["result"]["code"], "USAGE-CONFLICT");
    // And a bad flag under --json is an envelope too, not clap's text.
    let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "run", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v = support::result(&out.stdout);
    assert_eq!(v["code"], "USAGE");
    assert_eq!(v["fix"][0], "speccoder run --help");
}

#[test]
fn a_reader_that_goes_away_does_not_break_the_run() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let (_mock, repo, state) = project(&script());
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--events", "run", "--task", "T005", "--no-prompt"])
        .arg("-C")
        .arg(repo.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert!(first.contains("session.started"));
    drop(reader); // like `| head -1`
    let status = child.wait().unwrap();
    let mut err = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut err).unwrap();
    assert_ne!(status.code(), Some(101), "panicked: {err}");
    assert!(!err.contains("panicked"), "{err}");
    // The run still finished its work: the task is ticked on the branch.
    let log = Command::new("git")
        .args(["log", "--format=%s", "speccoder/001-graph"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&log.stdout).contains("T005"),
        "{}",
        String::from_utf8_lossy(&log.stdout)
    );
}

/// `run` at a terminal hosts the run in the dashboard (F-058): the call that
/// needs the person is answered in the overlay with one key, the run ends,
/// and leaving prints the run's result as the plain `run` prints it.
#[test]
fn a_run_at_a_terminal_is_answered_in_the_dashboard() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let done = json!({ "summary": "wrote it", "status": "done", "files_changed": ["package.json"], "checks_run": [], "notes": [] });
    let (_mock, repo, state) = project(&json!([
        { "tool": "write_file", "args": { "path": "package.json", "content": "{}\n" } },
        { "content": done.to_string() },
    ]));
    let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pty_drive.py");
    let out = Command::new("python3")
        .arg("-I")
        .arg(&driver)
        .args(["140", "40"])
        .args([
            "wait:Approval needed",
            "wait:package.json",
            "send:y",
            "wait:run ended",
            // The Evidence screen of the task just ticked: rows read from
            // its commit's attestation note.
            "send:2",
            "wait:REGRESSION",
            "wait:Instructions",
            "wait:Attestation",
            "send:q",
            "--",
        ])
        .arg(env!("CARGO_BIN_EXE_speccoder"))
        .args(["run", "--task", "T005", "-C"])
        .arg(repo.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let text = v["text"].as_str().unwrap();
    assert_eq!(
        v["met"],
        json!([true, true, true, true, true, true]),
        "{text}"
    );
    assert_eq!(v["exit"], 0, "{text}");
    // The decision names who made it and through what.
    assert!(text.contains("(dashboard)"), "{text}");
    // On leaving, the run's result, as plain `run` prints it.
    assert!(text.contains("verdict: regression_only"), "{text}");
    assert!(text.contains("Next:"), "{text}");
}

/// `--plain` keeps lines: no dashboard, even at a terminal.
#[test]
fn plain_keeps_lines_at_a_terminal() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let (_mock, repo, state) = project(&script());
    let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pty_drive.py");
    let out = Command::new("python3")
        .arg("-I")
        .arg(&driver)
        .args(["120", "30", "--"])
        .arg(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--plain", "run", "--task", "T005", "-C"])
        .arg(repo.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = v["text"].as_str().unwrap();
    assert!(!text.contains("1 Run"), "{text}");
    assert!(text.contains("T005"), "{text}");
}

/// The next step, taken at the terminal (F-075): after `spec tasks` the CLI
/// offers to commit exactly the planning file it wrote, then the run.
#[test]
fn after_planning_the_cli_offers_the_commit_and_then_the_run() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let done = json!({ "summary": "wrote the tasks", "status": "done", "files_written": ["specs/001-graph/tasks.md"], "questions": [] });
    let tasks = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../speccoder-speckit/tests/fixtures/graph/specs/001-graph/tasks.md"),
    )
    .unwrap();
    let (_mock, repo, state) = project(&json!([
        { "tool": "write_file", "args": { "path": "specs/001-graph/tasks.md", "content": tasks } },
        { "content": done.to_string() },
    ]));
    let r = repo.path();
    // A feature being planned: a spec and a plan, no task list yet.
    git(r, &["rm", "-q", "specs/001-graph/tasks.md"]);
    fs::create_dir_all(r.join("specs/001-graph")).unwrap();
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join(".claude/skills/speckit-tasks")).unwrap();
    fs::write(
        r.join(".claude/skills/speckit-tasks/SKILL.md"),
        "---\nname: speckit-tasks\ndescription: Break the plan into tasks.\n---\n\nWrite tasks.md.\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "planned"]);
    let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pty_drive.py");
    let out = Command::new("python3")
        .arg("-I")
        .arg(&driver)
        .args([
            "120",
            "40",
            "wait:Commit them?",
            "send:y\n",
            "wait:Start it now?",
            "send:n\n",
            "--",
        ])
        .arg(env!("CARGO_BIN_EXE_speccoder"))
        .args(["spec", "tasks", "-C"])
        .arg(r)
        .env("SPECCODER_STATE_DIR", state.path())
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let text = v["text"].as_str().unwrap();
    assert_eq!(v["met"], json!([true, true]), "{text}");
    // Where the feature was, and the card with the file.
    assert!(text.contains("● commit"), "{text}");
    assert!(text.contains("+ specs/001-graph/tasks.md"), "{text}");
    // Committed: exactly the planning file, under its own message.
    let log = Command::new("git")
        .args(["log", "-1", "--format=%s", "--name-only"])
        .current_dir(r)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(log.starts_with("spec(001-graph): tasks.md"), "{log}");
    assert_eq!(
        log.lines().filter(|l| l.contains('/')).collect::<Vec<_>>(),
        ["specs/001-graph/tasks.md"]
    );
    assert_eq!(v["exit"], 0, "{text}");
}

/// One HTTP/1.1 exchange with the web host: (status, head, body).
fn http(port: u16, req: &str) -> (u16, String, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head.to_ascii_lowercase(), body.to_owned())
}

fn request(port: u16, method: &str, path: &str, extra: &str, body: &str) -> String {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// `speccoder ui` (F-076): the host refuses a request without the token,
/// with a foreign `Host` (DNS rebinding) and a cross-origin write; with the
/// token, a run started over the API stops at the call that needs a person,
/// the page's answer lets it finish, and the decision is recorded `via web`.
#[test]
fn the_web_host_guards_itself_and_a_run_is_answered_over_its_api() {
    if !has_python() {
        eprintln!("skipped: no python3");
        return;
    }
    let done = json!({ "summary": "wrote it", "status": "done", "files_changed": ["package.json"], "checks_run": [], "notes": [] });
    let (_mock, repo, state) = project(&json!([
        { "tool": "write_file", "args": { "path": "package.json", "content": "{}\n" } },
        { "content": done.to_string() },
    ]));
    // `--json` prints the documented result once the host listens.
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(repo.path())
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
    assert_eq!(ready["payload"], "ui", "{ready}");
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let url = ready["data"]["url"].as_str().unwrap();
    let token = url.split_once("#token=").unwrap().1.to_owned();
    let auth = format!("Authorization: Bearer {token}\r\n");

    // Refused: no token, a foreign Host, a cross-origin write.
    assert_eq!(
        http(port, &request(port, "GET", "/api/project", "", "")).0,
        401
    );
    let rebound = format!(
        "GET /api/project HTTP/1.1\r\nHost: evil.example:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    assert_eq!(http(port, &rebound).0, 421);
    let foreign = format!("{auth}Origin: https://evil.example\r\n");
    assert_eq!(
        http(port, &request(port, "POST", "/api/stop", &foreign, "{}")).0,
        403
    );
    // The page itself: served with the policy, never cached or framed.
    let (status, head, body) = http(port, &request(port, "GET", "/", "", ""));
    assert_eq!(status, 200);
    assert!(
        body.contains("<div id=\"app\">") || body.contains("id=\"app\""),
        "{body}"
    );
    for h in [
        "content-security-policy: default-src 'self'",
        "cache-control: no-store",
        "x-frame-options: deny",
        "referrer-policy: no-referrer",
    ] {
        assert!(head.contains(h), "{h}: {head}");
    }
    // With the token: the project, and the next step offered.
    let (status, _, body) = http(port, &request(port, "GET", "/api/actions", &auth, ""));
    assert_eq!(status, 200, "{body}");
    let actions: Value = serde_json::from_str(&body).unwrap();
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["run"]["kind"] == "run"),
        "{actions}"
    );

    // Listen, then start the task; answer the ask; wait for the end.
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    let listen = request(port, "GET", "/api/events", &auth, "");
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
    let start = json!({ "run": { "kind": "run", "task": "T005" }, "input": null, "feature": null, "command": "speccoder run --task T005" });
    let (status, _, body) = http(
        port,
        &request(port, "POST", "/api/run", &auth, &start.to_string()),
    );
    assert_eq!(status, 200, "{body}");
    // A second job while one runs is refused.
    assert_eq!(
        http(
            port,
            &request(port, "POST", "/api/run", &auth, &start.to_string())
        )
        .0,
        409
    );
    let ask = next("ask");
    assert!(
        ask["presentation"].to_string().contains("package.json"),
        "{ask}"
    );
    let answer = json!({ "id": ask["id"], "answer": { "decision": "approve", "scope": "once" } });
    let (status, _, body) = http(
        port,
        &request(port, "POST", "/api/answer", &auth, &answer.to_string()),
    );
    assert_eq!(status, 200, "{body}");
    let end = next("done");
    assert_eq!(end["result"]["status"], "ok", "{end}");
    // The decision names who made it and through what.
    assert!(end.to_string().contains(r#""via":"web""#), "{end}");

    // Ticked on the feature's branch; the decision names the web.
    let log = Command::new("git")
        .args(["log", "--format=%s%n%b%n%N", "speccoder/001-graph"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(log.contains("T005"), "{log}");
}

/// Without the `app` feature, `--window` says how to get it.
#[cfg(not(feature = "app"))]
#[test]
fn a_window_without_the_app_feature_says_how_to_build_it() {
    let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--window", "--no-open"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v = support::result(&out.stdout);
    assert!(
        v["message"].as_str().unwrap().contains("--features app"),
        "{v}"
    );
}

/// ctrl-c stops the web host even while a page holds its event stream open
/// (a graceful shutdown alone would wait for that stream forever).
#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_web_host_with_a_page_listening() {
    use std::io::Write;
    let graph =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "-C"])
        .arg(&graph)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let ready: Value = serde_json::from_str(&first).unwrap();
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let token = ready["data"]["url"]
        .as_str()
        .unwrap()
        .split_once("#token=")
        .unwrap()
        .1
        .to_owned();
    // A page listening: the stream stays open.
    let mut page = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    page.write_all(
        request(
            port,
            "GET",
            "/api/events",
            &format!("Authorization: Bearer {token}\r\n"),
            "",
        )
        .as_bytes(),
    )
    .unwrap();
    let mut hello = String::new();
    BufReader::new(page.try_clone().unwrap())
        .read_line(&mut hello)
        .unwrap();
    assert!(hello.contains("200"), "{hello}");
    let pid = child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .unwrap()
            .success()
    );
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(
            Instant::now() < until,
            "the host did not stop on ctrl-c with a page listening"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(page);
}
