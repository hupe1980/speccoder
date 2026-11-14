//! Several projects, one view (spec 097), over the web host: the roll-up
//! and opening another project never carry a key; only a recent project
//! is opened, at a place of the known form.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

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

struct Host {
    _child: Killed,
    _state: tempfile::TempDir,
    _home: tempfile::TempDir,
    port: u16,
    auth: String,
}

impl Host {
    fn start(r: &Path) -> Self {
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
        let ready: Value = serde_json::from_str(&first).unwrap();
        let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
        let url = ready["data"]["url"].as_str().unwrap();
        let auth = format!(
            "Authorization: Bearer {}\r\n",
            url.split_once("#token=").unwrap().1
        );
        Self {
            _child: Killed(child),
            _state: state,
            _home: home,
            port,
            auth,
        }
    }
}

/// A committed project with one task, its configuration changed since.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::write(r.join(".specify/memory/constitution.md"), "# C\n").unwrap();
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
    dir
}

#[test]
fn the_view_across_projects_never_carries_a_key() {
    let dir = project();
    let host = Host::start(dir.path());
    let (port, auth) = (host.port, host.auth.clone());
    let key = auth
        .trim()
        .trim_start_matches("Authorization: Bearer ")
        .to_owned();

    let (status, list) = http(port, "GET", "/api/projects", &auth, "");
    assert_eq!(status, 200, "{list}");
    let list = support::checked(list);
    let here = &list["data"]["projects"][0];
    assert_eq!(here["current"], true, "{list}");
    assert!(!list.to_string().contains(&key));

    for (body, why) in [
        (json!({ "path": "/nowhere/else" }), "not a recent project"),
        (
            json!({ "path": here["path"], "at": "board:../x" }),
            "not a place",
        ),
    ] {
        let (_, r) = http(port, "POST", "/api/projects/open", &auth, &body.to_string());
        assert_eq!(r["ok"], false, "{r}");
        assert!(r["error"].as_str().unwrap().contains(why), "{r}");
        assert!(!r.to_string().contains(&key));
    }
    // This project is served here: named as open, nothing started, no key.
    let (_, r) = http(
        port,
        "POST",
        "/api/projects/open",
        &auth,
        &json!({ "path": here["path"] }).to_string(),
    );
    assert_eq!(r["opened"]["outcome"], "elsewhere", "{r}");
    assert!(!r.to_string().contains(&key) && !r.to_string().contains("token"));
}
