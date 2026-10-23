//! Killing speccoder mid-attempt and running again: the interrupted attempt
//! resumes from the journal, and a non-idempotent command that had finished
//! is not run a second time.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

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

fn wait_for(path: &Path, secs: u64) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_killed_attempt_resumes_without_repeating_a_finished_command() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    let mock_dir = tempfile::tempdir().unwrap();
    let done = json!({ "summary": "noted", "status": "done", "files_changed": [], "checks_run": [], "notes": [] });
    let script = json!([
        { "tool": "run_command", "args": { "argv": ["./note.sh"] } },
        { "content": done.to_string(), "hang_once": true },
    ]);
    fs::write(mock_dir.path().join("script.json"), script.to_string()).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py");
    let _mock = Killed(
        Command::new("python3")
            .arg("-I")
            .arg(&fixture)
            .arg(mock_dir.path())
            .arg(mock_dir.path().join("script.json"))
            .spawn()
            .unwrap(),
    );
    assert!(
        wait_for(&mock_dir.path().join("port"), 20),
        "mock did not start"
    );
    let port = fs::read_to_string(mock_dir.path().join("port")).unwrap();

    // The graph fixture, a non-idempotent command, and the mock as the model.
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
        r.join("note.sh"),
        "#!/bin/sh\necho \"$SPECCODER_CALL_ID\" >> calls.log\n",
    )
    .unwrap();
    assert!(
        Command::new("chmod")
            .args(["+x", "note.sh"])
            .current_dir(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n\n[acceptance]\ngenerate = false\n\n[commands]\nallow = [[\"./note.sh\"]]\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let state = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_speccoder"))
            .args(["--json", "run", "--task", "T005", "--no-prompt"])
            .arg(r)
            .env("SPECCODER_STATE_DIR", state.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };

    // 1. Killed while the model is answering, after the command ran.
    let first = Killed(run());
    assert!(
        wait_for(&mock_dir.path().join("hung"), 60),
        "never reached the second model call"
    );
    drop(first);
    let worktrees = state.path();
    let log = walk(worktrees, "calls.log").expect("the command ran");
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);

    // 2. Run again: the attempt resumes and finishes.
    let second = run().wait_with_output().unwrap();
    let out: Value = serde_json::from_slice(&second.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&second.stdout)));
    let report = &out["data"];
    assert_eq!(out["status"], "ok", "{out}");
    assert!(
        report["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().starts_with("resumed run")),
        "{out}"
    );
    // The banner says where it was interrupted, from the live record.
    assert!(
        report["notices"].as_array().unwrap().iter().any(|n| n
            .as_str()
            .unwrap()
            .starts_with("Resuming — T005 was interrupted during attempt (run_command")),
        "{out}"
    );
    assert_eq!(report["attempt_status"], "completed");
    // The command ran once in all; the first model call was not sent again.
    let lines: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("call_"),
        "SPECCODER_CALL_ID: {lines:?}"
    );
    let requests = fs::read_to_string(mock_dir.path().join("requests")).unwrap();
    assert_eq!(requests.lines().count(), 3, "{requests}");
}

fn walk(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
        if p.is_dir()
            && !p.ends_with(".git")
            && let Some(found) = walk(&p, name)
        {
            return Some(found);
        }
    }
    None
}

#[test]
fn a_command_killed_mid_run_is_not_guessed_about() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let mock_dir = tempfile::tempdir().unwrap();
    let done = json!({ "summary": "noted", "status": "done", "files_changed": [], "checks_run": [], "notes": [] });
    let script = json!([
        { "tool": "run_command", "args": { "argv": ["./slow.sh"] } },
        { "content": done.to_string() },
    ]);
    fs::write(mock_dir.path().join("script.json"), script.to_string()).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py");
    let _mock = Killed(
        Command::new("python3")
            .arg("-I")
            .arg(&fixture)
            .arg(mock_dir.path())
            .arg(mock_dir.path().join("script.json"))
            .spawn()
            .unwrap(),
    );
    assert!(wait_for(&mock_dir.path().join("port"), 20));
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
        r.join("slow.sh"),
        "#!/bin/sh\necho started >> calls.log\nsleep 30\necho finished >> calls.log\n",
    )
    .unwrap();
    assert!(
        Command::new("chmod")
            .args(["+x", "slow.sh"])
            .current_dir(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n\n[acceptance]\ngenerate = false\n\n[commands]\nallow = [[\"./slow.sh\"]]\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let state = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_speccoder"))
            .args(["--json", "run", "--task", "T005", "--no-prompt"])
            .arg(r)
            .env("SPECCODER_STATE_DIR", state.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let first = Killed(run());
    let start = Instant::now();
    let log = loop {
        if let Some(l) = walk(state.path(), "calls.log") {
            break l;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the command never started"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(first);
    let second = run().wait_with_output().unwrap();
    let out: Value = serde_json::from_slice(&second.stdout).unwrap();
    // A person decides: the command is not run again on a guess.
    assert_eq!(out["status"], "needs_human", "{out}");
    assert_eq!(out["data"]["attempt_status"], "interrupted");
    assert!(out["data"]["saved_attempt"].as_str().is_some());
    assert_eq!(fs::read_to_string(&log).unwrap_or_default(), "started\n");
    // The next run starts a fresh attempt (the mock answers it again).
    let third = run().wait_with_output().unwrap();
    let out: Value = serde_json::from_slice(&third.stdout).unwrap();
    assert!(
        !out["data"]["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().starts_with("resumed")),
        "{out}"
    );
}

#[test]
fn a_finished_attempt_killed_before_its_tick_is_judged_and_ticked_on_the_next_run() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let mock_dir = tempfile::tempdir().unwrap();
    let done = json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] });
    let script = json!([
        { "tool": "write_file", "args": { "path": "src/lib.rs", "content": "pub fn ok() {}\npub fn login() {}\n" } },
        { "content": done.to_string() },
    ]);
    fs::write(mock_dir.path().join("script.json"), script.to_string()).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py");
    let _mock = Killed(
        Command::new("python3")
            .arg("-I")
            .arg(&fixture)
            .arg(mock_dir.path())
            .arg(mock_dir.path().join("script.json"))
            .spawn()
            .unwrap(),
    );
    assert!(wait_for(&mock_dir.path().join("port"), 20));
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
    // A slow check: the window between the attempt and its tick.
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[checks.slow]\nargv = [\"sh\", \"-c\", \"sleep 3; grep -q 'fn ok' src/lib.rs\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n\n[acceptance]\ngenerate = false\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let state = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_speccoder"))
            .args(["--json", "run", "--task", "T005", "--no-prompt"])
            .arg(r)
            .env("SPECCODER_STATE_DIR", state.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let first = Killed(run());
    // Both model answers are given: the attempt has finished; the slow
    // check is verifying it.
    let start = Instant::now();
    while fs::read_to_string(mock_dir.path().join("requests"))
        .unwrap_or_default()
        .lines()
        .count()
        < 2
    {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the attempt never finished"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(500));
    drop(first);
    let second = run().wait_with_output().unwrap();
    let out: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(out["status"], "ok", "{out}");
    assert_eq!(out["data"]["ticked"], true, "{out}");
    assert!(
        out["data"]["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().starts_with("resumed run")),
        "{out}"
    );
    // Read back, not asked again.
    let requests = fs::read_to_string(mock_dir.path().join("requests")).unwrap();
    assert_eq!(requests.lines().count(), 2, "{requests}");
}
