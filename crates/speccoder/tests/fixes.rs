//! The person's own commands from the web app: a fix speccoder printed
//! runs from the page exactly as issued — in the project folder, its
//! output streamed — and nothing else does: changed argvs, argvs the host
//! never sent, a wrong token are refused before anything runs.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
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

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
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

fn http(port: u16, method: &str, path: &str, auth: &str, body: &str) -> (u16, Value) {
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
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

#[test]
fn only_a_fix_the_host_issued_runs_and_it_runs_exactly() {
    // A project whose configuration is changed and not committed: doctor's
    // fix is the person's own `git add … && git commit …`.
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::write(
        r.join("specs/001-x/tasks.md"),
        "# Tasks\n\n- [ ] T001 Do it\n",
    )
    .unwrap();
    fs::write(r.join("speccoder.toml"), "[checks.ok]\nargv = [\"true\"]\n").unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Dana Example"]);
    git(r, &["config", "user.email", "d@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    fs::write(
        r.join("speccoder.toml"),
        "[checks.ok]\nargv = [\"true\", \"x\"]\n",
    )
    .unwrap();

    let state = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(r)
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
    let ready: Value = serde_json::from_str(&first).unwrap();
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let url = ready["data"]["url"].as_str().unwrap();
    let auth = format!(
        "Authorization: Bearer {}\r\n",
        url.split_once("#token=").unwrap().1
    );

    let (_, doctor) = http(port, "GET", "/api/doctor", &auth, "");
    let doctor = support::checked(doctor);
    let config = doctor["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["check"] == "config")
        .cloned()
        .unwrap();
    assert_eq!(config["code"], "CONFIG-NOT-TRACKED", "{config}");
    let step = config["fix_steps"][0].clone();
    assert_eq!(step["how"], "command", "{step}");
    assert_eq!(
        step["argvs"],
        json!([
            ["git", "add", "speccoder.toml"],
            ["git", "commit", "-m", "speccoder: config"]
        ])
    );
    let token = step["token"].as_str().unwrap().to_owned();

    // Refused before anything runs: another argv with this token, a token
    // the host never issued, argvs it never sent.
    let refused = [
        json!({ "argvs": [["git", "add", "-A"], ["git", "commit", "-m", "speccoder: config"]], "token": token }),
        json!({ "argvs": step["argvs"], "token": "0".repeat(64) }),
        json!({ "argvs": [["touch", "planted"]], "token": token }),
        json!({ "argvs": [], "token": token }),
    ];
    for body in refused {
        let (status, reply) = http(port, "POST", "/api/fix/run", &auth, &body.to_string());
        assert_eq!(status, 403, "{body}: {reply}");
    }
    // A tab opens no terminal.
    let (status, _) = http(
        port,
        "POST",
        "/api/terminal",
        &auth,
        &json!({ "argvs": step["argvs"], "token": token }).to_string(),
    );
    assert_eq!(status, 400);
    assert!(!r.join("planted").exists());
    assert_eq!(git(r, &["log", "-1", "--format=%s"]).trim(), "init");

    // Exactly as issued: it runs, its output streams, then it is done.
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    let listen = format!(
        "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    std::thread::spawn(move || {
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
    let until = Instant::now() + Duration::from_secs(30);
    let mut events = Vec::new();
    let mut next = |want: &str| -> Value {
        loop {
            let v = rx
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| panic!("no {want} event; got {events:?}"));
            events.push(v.clone());
            if v["type"] == want {
                return v;
            }
        }
    };
    next("hello");
    let (status, reply) = http(
        port,
        "POST",
        "/api/fix/run",
        &auth,
        &json!({ "argvs": step["argvs"], "token": token }).to_string(),
    );
    assert_eq!(status, 200, "{reply}");
    let started = next("started");
    assert_eq!(
        started["command"],
        "git add speccoder.toml && git commit -m \"speccoder: config\""
    );
    let output = next("output");
    assert!(output["line"].is_string(), "{output}");
    let done = next("done");
    let result = support::checked(done["result"].clone());
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        git(r, &["log", "-1", "--format=%s"]).trim(),
        "speccoder: config"
    );
}
