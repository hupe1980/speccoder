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
