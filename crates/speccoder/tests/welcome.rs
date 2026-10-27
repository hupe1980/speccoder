//! A host started outside a project (F-130): the welcome, project routes
//! waiting for a project, and binding once — to a folder opened, a project
//! created (the plan bound to its digest), a repository cloned.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Host {
    child: Child,
    port: u16,
    auth: String,
    events: mpsc::Receiver<Value>,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn tools() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/new-tools")
}

/// `speccoder ui` in `dir`, with a home of its own, the fake tools first
/// on the PATH and extra variables.
fn host(dir: &Path, home: &Path, state: &Path, extra: &[(&str, &str)]) -> Host {
    fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Sam Doe\n\temail = sam@example.com\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n",
    )
    .unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_speccoder"));
    c.args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(dir)
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                tools().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("SPECCODER_STATE_DIR", state)
        .env("TMPDIR", std::env::temp_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(h) = std::env::var_os("SPECCODER_SANDBOX_HELPER") {
        c.env("SPECCODER_SANDBOX_HELPER", h);
    }
    for (k, v) in extra {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let ready: Value = serde_json::from_str(&first).unwrap();
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let url = ready["data"]["url"].as_str().unwrap();
    let auth = format!(
        "Authorization: Bearer {}\r\n",
        url.split_once("#token=").unwrap().1
    );
    let (tx, rx) = mpsc::channel();
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
    let h = Host {
        child,
        port,
        auth,
        events: rx,
    };
    h.next("hello");
    h
}

impl Host {
    fn call(&self, method: &str, path: &str, body: &str) -> (u16, Value) {
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            self.auth,
            body.len()
        );
        let mut s = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn next(&self, want: &str) -> Value {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let v = self
                .events
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no {want} event"));
            if v["type"] == want {
                return v;
            }
        }
    }

    fn run(&self, run: &Value) -> Value {
        let start = json!({ "run": run, "input": null, "feature": null, "command": "test" });
        let (status, body) = self.call("POST", "/api/run", &start.to_string());
        assert_eq!(status, 200, "{body}");
        self.next("done")["result"].clone()
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A committed Spec Kit project to open.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let graph =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(graph.join("."))
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "init"]);
    dir
}

#[test]
fn a_host_outside_a_project_serves_the_welcome_and_binds_once_to_a_folder_opened() {
    let (empty, home, state) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let h = host(empty.path(), home.path(), state.path(), &[]);
    let (status, body) = h.call("GET", "/api/project", "");
    assert_eq!(status, 409);
    assert_eq!(body["code"], "NO-PROJECT-YET");
    let (status, welcome) = h.call("GET", "/api/welcome", "");
    assert_eq!(status, 200);
    assert!(welcome["bound"].is_null());
    assert_eq!(welcome["recents"], json!([]));
    // Only project-making kinds run before a project exists.
    let start =
        json!({ "run": { "kind": "converge" }, "input": null, "feature": null, "command": "x" });
    assert_eq!(h.call("POST", "/api/run", &start.to_string()).0, 409);
    // The folder route lists directories under home only.
    let (_, outside) = h.call("GET", "/api/folders?under=/etc", "");
    assert!(outside["error"].is_string(), "{outside}");
    fs::create_dir_all(home.path().join("Workspaces/alpha")).unwrap();
    let (_, inside) = h.call("GET", "/api/folders?under=~/Workspaces/", "");
    assert_eq!(inside["folders"], json!(["~/Workspaces/alpha"]));
    // Open binds the host, once.
    let p = project();
    let open = json!({ "path": p.path().display().to_string() }).to_string();
    let (status, opened) = h.call("POST", "/api/open", &open);
    assert_eq!(status, 200, "{opened}");
    let (status, proj) = h.call("GET", "/api/project", "");
    assert_eq!(status, 200);
    assert_eq!(proj["format"], "Spec Kit");
    let other = project();
    let again = json!({ "path": other.path().display().to_string() }).to_string();
    assert_eq!(h.call("POST", "/api/open", &again).0, 409);
    let (_, welcome) = h.call("GET", "/api/welcome", "");
    assert_eq!(welcome["recents"].as_array().unwrap().len(), 1, "{welcome}");
}

#[test]
fn new_project_runs_the_plan_on_screen_or_nothing_and_binds_to_the_project() {
    let (empty, home, state) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let h = host(empty.path(), home.path(), state.path(), &[]);
    let target = fs::canonicalize(empty.path()).unwrap().join("md2csv");
    let q = format!(
        "/api/new/plan?path={}&idea=a%20CLI&stack=go",
        target.display()
    );
    let (status, plan) = h.call("GET", &q, "");
    assert_eq!(status, 200, "{plan}");
    let digest = plan["plan"]["digest"].as_str().unwrap().to_owned();
    assert!(plan["plan"]["refusal"].is_null(), "{plan}");
    let choices = plan["choices"].clone();
    // A digest that is not the plan on screen: nothing runs.
    let stale = h.run(&json!({ "kind": "new_project", "target": target.display().to_string(), "choices": choices, "plan_digest": "sha256:old" }));
    assert_eq!(stale["code"], "NEW-PLAN-CHANGED", "{stale}");
    assert!(!target.exists());
    let made = h.run(&json!({ "kind": "new_project", "target": target.display().to_string(), "choices": choices, "plan_digest": digest }));
    assert_eq!(made["status"], "ok", "{made}");
    let (status, proj) = h.call("GET", "/api/project", "");
    assert_eq!(status, 200);
    assert_eq!(proj["name"], "md2csv");
    // The idea is offered to the first feature once.
    let (_, first) = h.call("POST", "/api/new/first-feature", "{}");
    assert_eq!(first["idea"], "a CLI");
    let (_, again) = h.call("POST", "/api/new/first-feature", "{}");
    assert!(again["idea"].is_null());
}

#[test]
fn clone_runs_git_as_the_person_into_the_destination_and_binds() {
    let (empty, home, state, remote) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    // A repository standing in for github.com/octo/hello.
    let src = project();
    fs::create_dir_all(remote.path().join("octo")).unwrap();
    let bare = remote.path().join("octo/hello.git");
    git(
        remote.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &src.path().display().to_string(),
            &bare.display().to_string(),
        ],
    );
    let base = remote.path().display().to_string();
    let h = host(
        empty.path(),
        home.path(),
        state.path(),
        &[("SPECCODER_GIT_BASE", &base)],
    );
    let dest = fs::canonicalize(empty.path()).unwrap().join("hello");
    let bad = h.run(&json!({ "kind": "clone_repository", "repo": "not a repo", "target": dest.display().to_string() }));
    assert_eq!(bad["code"], "USAGE", "{bad}");
    let cloned = h.run(&json!({ "kind": "clone_repository", "repo": "octo/hello", "target": dest.display().to_string() }));
    assert_eq!(cloned["status"], "ok", "{cloned}");
    assert!(dest.join(".specify").is_dir());
    assert_eq!(h.call("GET", "/api/project", "").0, 200);
}
