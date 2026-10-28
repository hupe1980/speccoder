//! `speccoder actions`: the list every interface renders, for scripts —
//! in the versioned envelope, valid against its published schema, the
//! same list the web host serves, at every step of the journey.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
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

/// `speccoder actions --json` in `dir`, checked against the schemas, and
/// its exit code.
fn actions(dir: &Path, state: &Path, extra: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "actions"])
        .args(extra)
        .arg("-C")
        .arg(dir)
        .env("SPECCODER_STATE_DIR", state)
        .env_remove("SPECIFY_FEATURE_DIRECTORY")
        .env_remove("SPECCODER_FORMAT")
        .output()
        .unwrap();
    (support::result(&out.stdout), out.status.code().unwrap())
}

/// The first `next` action an interface can run: its kind and subject.
fn first_next(v: &Value) -> (String, Value) {
    let a = v["data"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["group"] == "next" && !a["run"].is_null() && a["blocked"].is_null())
        .unwrap_or_else(|| panic!("no next step: {v}"));
    (
        a["id"]["kind"].as_str().unwrap().to_owned(),
        a["id"]["subject"].clone(),
    )
}

#[test]
fn the_list_follows_the_journey_and_validates() {
    let state = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::write(r.join(".specify/memory/constitution.md"), "# C\n").unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let at = |extra: &[&str]| actions(r, state.path(), extra);

    let (v, code) = at(&[]);
    assert_eq!(
        (v["status"].as_str(), v["payload"].as_str(), code),
        (Some("ok"), Some("actions"), 0)
    );
    assert_eq!(first_next(&v), ("skill".to_owned(), "specify".into()));
    let create = v["data"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["role"] == "create")
        .unwrap();
    assert!(create["input"].is_string(), "{create}");

    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::write(
        r.join("specs/001-x/spec.md"),
        "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n",
    )
    .unwrap();
    assert_eq!(first_next(&at(&[]).0), ("skill".to_owned(), "plan".into()));
    fs::write(r.join("specs/001-x/plan.md"), "# Plan\n").unwrap();
    assert_eq!(first_next(&at(&[]).0), ("skill".to_owned(), "tasks".into()));
    fs::write(
        r.join("specs/001-x/tasks.md"),
        "# Tasks\n\n## Phase 1\n\n- [ ] T001 Do it\n- [ ] T002 Then this\n",
    )
    .unwrap();
    assert_eq!(first_next(&at(&[]).0), ("commit".to_owned(), Value::Null));
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "plan"]);
    let (v, _) = at(&[]);
    assert_eq!(first_next(&v), ("run_feature".to_owned(), Value::Null));
    // A selected task brings its own actions.
    let has_task = |v: &Value| {
        v["data"]["actions"].as_array().unwrap().iter().any(|a| {
            a["group"] == "task"
                && a["id"] == serde_json::json!({ "kind": "accept", "subject": "T002" })
        })
    };
    assert!(!has_task(&v));
    let (with, _) = at(&["--task", "T002"]);
    assert!(has_task(&with), "{with}");
    assert_eq!(with["data"]["journey"][4]["state"], "now", "{with}");
}

#[test]
fn outside_a_project_it_ends_as_report_does_with_an_empty_list() {
    let state = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (v, code) = actions(dir.path(), state.path(), &[]);
    let report = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "report", "-C"])
        .arg(dir.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .output()
        .unwrap();
    let r = support::result(&report.stdout);
    assert_eq!(
        (&v["status"], &v["code"], code),
        (&r["status"], &r["code"], report.status.code().unwrap())
    );
    assert_eq!(v["data"]["actions"], serde_json::json!([]));
}

#[test]
fn an_openspec_project_gets_its_list_too() {
    let state = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-openspec/tests/fixtures/1.14.3/sample");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let (v, code) = actions(dir.path(), state.path(), &[]);
    assert_eq!((v["status"].as_str(), code), (Some("ok"), 0), "{v}");
    assert!(
        v["data"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"]["kind"] == "command" && a["id"]["subject"] == "doctor"),
        "{v}"
    );
}

/// The web host's `/api/actions` and the command are one list.
#[test]
fn the_command_and_the_web_host_list_the_same_actions() {
    let state = tempfile::tempdir().unwrap();
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
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(r)
        .env("SPECCODER_STATE_DIR", state.path())
        .env_remove("SPECIFY_FEATURE_DIRECTORY")
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
    let token = ready["data"]["url"]
        .as_str()
        .unwrap()
        .split_once("#token=")
        .unwrap()
        .1
        .to_owned();
    let get = |path: &str| -> Value {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        serde_json::from_str(out.split_once("\r\n\r\n").unwrap().1).unwrap()
    };
    for (feature, task) in [(None, None), (Some("001-graph"), Some("T005"))] {
        let mut path = "/api/actions".to_owned();
        let mut extra = Vec::new();
        if let (Some(f), Some(t)) = (feature, task) {
            path.push_str(&format!("?feature={f}&task={t}"));
            extra = vec!["--feature", f, "--task", t];
        }
        let mut web = get(&path);
        // The host signs the person's own commands it lists (a token per
        // host); otherwise the lists are the same.
        for a in web.as_array_mut().into_iter().flatten() {
            if let Some(step) = a.get_mut("step").and_then(Value::as_object_mut)
                && step.get("how").and_then(Value::as_str) == Some("command")
            {
                assert!(
                    step.remove("token").is_some_and(|t| t.is_string()),
                    "{path}: {a}"
                );
            }
        }
        let (cli, _) = actions(r, state.path(), &extra);
        assert_eq!(cli["data"]["actions"], web, "{path}");
    }
}
