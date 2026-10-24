//! Killing speccoder mid-attempt and running again: the interrupted attempt
//! resumes from the journal, and a non-idempotent command that had finished
//! is not run a second time.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

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
    let out: Value = support::result(&second.stdout);
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
    let out: Value = support::result(&second.stdout);
    // A person decides: the command is not run again on a guess.
    assert_eq!(out["status"], "needs_human", "{out}");
    assert_eq!(out["data"]["attempt_status"], "interrupted");
    let saved = out["data"]["saved_attempt"].as_str().unwrap().to_owned();
    // Undone: the worktree is back where the task started, and what the
    // interrupted command wrote is kept in the saved attempt.
    assert!(!log.exists(), "the interrupted attempt was not undone");
    let kept = Command::new("git")
        .args(["show", &format!("{saved}:calls.log")])
        .current_dir(r)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&kept.stdout), "started\n");
    // The next run starts a fresh attempt (the mock answers it again).
    let third = run().wait_with_output().unwrap();
    let out: Value = support::result(&third.stdout);
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
    let out: Value = support::result(&second.stdout);
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

// ── In place (F-048) ───────────────────────────────────────────────────────

/// A model mock that runs `./note.sh`, then hangs once before answering.
fn hanging_mock() -> (tempfile::TempDir, Killed, String) {
    let mock_dir = tempfile::tempdir().unwrap();
    let done = json!({ "summary": "noted", "status": "done", "files_changed": [], "checks_run": [], "notes": [] });
    let script = json!([
        { "tool": "run_command", "args": { "argv": ["./note.sh"] } },
        { "content": done.to_string(), "hang_once": true },
    ]);
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
    assert!(
        wait_for(&mock_dir.path().join("port"), 20),
        "mock did not start"
    );
    let port = fs::read_to_string(mock_dir.path().join("port")).unwrap();
    (mock_dir, mock, port)
}

/// The graph fixture with `specs/` and `.specify/` git-ignored, on `main`.
fn ignored_spec_repo(port: &str) -> tempfile::TempDir {
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
    fs::write(r.join(".gitignore"), "/specs/\n/.specify/\n").unwrap();
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
    repo
}

fn head_of(r: &Path) -> String {
    let out = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(r)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn speccoder(r: &Path, state: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_speccoder"));
    c.arg("--json")
        .args(args)
        .arg(r)
        .env("SPECCODER_STATE_DIR", state)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    c
}

fn json_of(out: &std::process::Output) -> Value {
    support::result(&out.stdout)
}

#[test]
fn a_killed_in_place_attempt_resumes_and_returns_to_the_original_branch() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    let (mock_dir, _mock, port) = hanging_mock();
    let repo = ignored_spec_repo(&port);
    let r = repo.path();
    let state = tempfile::tempdir().unwrap();
    let args = ["run", "--in-place", "--task", "T005", "--no-prompt"];

    // 1. Killed while the model is answering, after the command ran.
    let first = Killed(speccoder(r, state.path(), &args).spawn().unwrap());
    assert!(
        wait_for(&mock_dir.path().join("hung"), 60),
        "never reached the second model call"
    );
    drop(first);
    assert_eq!(
        head_of(r),
        "speccoder/001-graph",
        "left on the speccoder branch"
    );
    assert_eq!(
        fs::read_to_string(r.join("calls.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    // 2. `status` names the way out; a worktree-mode run is refused.
    let status = json_of(&speccoder(r, state.path(), &["status"]).output().unwrap());
    assert!(
        status["message"]
            .as_str()
            .unwrap()
            .contains("interrupted in-place run"),
        "{status}"
    );
    let plain = speccoder(r, state.path(), &["run", "--task", "T005", "--no-prompt"])
        .output()
        .unwrap();
    assert_eq!(plain.status.code(), Some(14), "{}", json_of(&plain));
    assert_eq!(json_of(&plain)["code"], "IN-PLACE-SESSION");

    // 3. Run again in place: the attempt resumes, the command does not repeat.
    let second = speccoder(r, state.path(), &args).output().unwrap();
    let out = json_of(&second);
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
    assert_eq!(head_of(r), "main", "{out}");
    let tasks = fs::read_to_string(r.join("specs/001-graph/tasks.md")).unwrap();
    assert!(tasks.contains("- [X] T005"));
    let requests = fs::read_to_string(mock_dir.path().join("requests")).unwrap();
    assert_eq!(requests.lines().count(), 3, "{requests}");
    let log = Command::new("git")
        .args(["show", "speccoder/001-graph:calls.log"])
        .current_dir(r)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 1);
}

#[test]
fn return_leaves_a_killed_in_place_run_without_running_anything() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    let (mock_dir, _mock, port) = hanging_mock();
    let repo = ignored_spec_repo(&port);
    let r = repo.path();
    let state = tempfile::tempdir().unwrap();
    let first = Killed(
        speccoder(
            r,
            state.path(),
            &["run", "--in-place", "--task", "T005", "--no-prompt"],
        )
        .spawn()
        .unwrap(),
    );
    assert!(wait_for(&mock_dir.path().join("hung"), 60));
    drop(first);
    assert_eq!(head_of(r), "speccoder/001-graph");

    let back = speccoder(r, state.path(), &["run", "--in-place", "--return"])
        .output()
        .unwrap();
    let out = json_of(&back);
    assert_eq!(out["status"], "ok", "{out}");
    assert!(
        out["message"].as_str().unwrap().contains("back on main"),
        "{out}"
    );
    assert_eq!(head_of(r), "main");
    assert!(
        !r.join("calls.log").exists(),
        "the interrupted work is undone"
    );
    let refs = Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "refs/speccoder/attempts/001-graph",
        ])
        .current_dir(r)
        .output()
        .unwrap();
    assert!(!refs.stdout.is_empty(), "saved under an attempt ref");
    // No model call was made by --return.
    let requests = fs::read_to_string(mock_dir.path().join("requests")).unwrap();
    assert_eq!(requests.lines().count(), 2, "{requests}");
    // And `--return` needs `--in-place`.
    let bare = speccoder(r, state.path(), &["run", "--return"])
        .output()
        .unwrap();
    assert_eq!(bare.status.code(), Some(2));
}

// ── Waiting for a person (F-072) ───────────────────────────────────────────

#[test]
fn an_approved_call_is_never_run_or_asked_again_after_a_kill() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    // The model asks for a shell command (needs a person), then hangs once
    // before its final answer.
    let mock_dir = tempfile::tempdir().unwrap();
    let done = json!({ "summary": "noted", "status": "done", "files_changed": [], "checks_run": [], "notes": [] });
    let script = json!([
        { "tool": "run_command", "args": { "argv": ["sh", "-c", "echo \"$SPECCODER_CALL_ID\" >> calls.log"] } },
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
    let state = tempfile::tempdir().unwrap();
    let run = || speccoder(r, state.path(), &["run", "--task", "T005", "--no-prompt"]);

    // 1. Nobody to ask: the call waits; nothing ran.
    let first = json_of(&run().output().unwrap());
    assert_eq!(first["status"], "needs_human", "{first}");
    assert_eq!(first["data"]["attempt_status"], "needs_approval", "{first}");
    assert_eq!(first["data"]["pending"]["waiting"], true, "{first}");
    assert!(walk(state.path(), "calls.log").is_none());

    // 2. A person approves it from the CLI (no model involved).
    let approved = json_of(
        &speccoder(
            r,
            state.path(),
            &["approve", "--task", "T005", "--scope", "once"],
        )
        .output()
        .unwrap(),
    );
    assert_eq!(approved["status"], "ok", "{approved}");
    assert_eq!(
        approved["data"]["pending"].as_array().map(Vec::len),
        Some(0)
    );

    // 3. The attempt resumes, runs the approved command, and is killed
    //    while the model is answering.
    let second = Killed(run().spawn().unwrap());
    assert!(
        wait_for(&mock_dir.path().join("hung"), 60),
        "never reached the last model call"
    );
    drop(second);
    let log = walk(state.path(), "calls.log").expect("the approved command ran");
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);

    // 4. Again: nothing is asked, nothing runs twice, the task is ticked.
    let third = json_of(&run().output().unwrap());
    assert_eq!(third["status"], "ok", "{third}");
    assert_eq!(third["data"]["ticked"], true, "{third}");
    assert_eq!(
        fs::read_to_string(&log).unwrap().lines().count(),
        1,
        "the command ran twice"
    );
    let waiting = json_of(&speccoder(r, state.path(), &["approvals"]).output().unwrap());
    assert_eq!(
        waiting["data"]["pending"].as_array().map(Vec::len),
        Some(0),
        "{waiting}"
    );
}
