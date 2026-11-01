//! A stop as the person sees it: the real binary, a scripted model, and
//! the fix lines it prints — the layer a pipeline test cannot see (every
//! result passes through `next::attach` and the renderer first).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;

const SPEC: &str = "# Spec\n\n### User Story 1 - Tables (Priority: P1)\n\nTables convert.\n\n**Independent Test**: run the command with a table.\n\n**Acceptance Scenarios**:\n\n1. **Given** a table, **When** converted, **Then** alpha holds.\n";

const TASKS: &str = "# Tasks\n\n## Phase 3: User Story 1 - Tables\n\n- [x] T001 [US1] Parse tables in src/lib.rs\n\n## Phase 7: Convergence\n\n- [ ] T026 Route both tables in `src/lib.rs` per US1/AC1 (partial)\n";

/// One test: its file must exist, then "the pattern is in src/lib.rs".
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\n[ -f \"$1\" ] || { echo \"error: no test file $1\" >&2; exit 2; }\nshift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@e"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The scripted model: the gap test (wrong: it wants `wrongname`), then an
/// attempt that writes `gamma` and says done — every later call says done.
fn mock() -> (tempfile::TempDir, Killed, String) {
    let dir = tempfile::tempdir().unwrap();
    let gap = json!({
        "files": [{ "path": "tests/acceptance_t026.txt", "content": "checks fn wrongname\n", "scenarios": ["US1-1"] }],
        "argv": ["./accept.sh", "--", "tests/acceptance_t026.txt", "fn wrongname"],
        "notes": [],
    });
    let done = json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] });
    let script = json!([
        { "content": gap.to_string() },
        { "tool": "edit_file", "args": { "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn gamma() {}" } },
        { "content": done.to_string() },
    ]);
    fs::write(dir.path().join("script.json"), script.to_string()).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py");
    let child = Killed(
        Command::new("python3")
            .arg("-I")
            .arg(&fixture)
            .arg(dir.path())
            .arg(dir.path().join("script.json"))
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !dir.path().join("port").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "mock did not start"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let port = fs::read_to_string(dir.path().join("port")).unwrap();
    (dir, child, port)
}

fn project(port: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("specs/001-graph")).unwrap();
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::write(
        r.join(".specify/memory/constitution.md"),
        "# Constitution\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::write(r.join("specs/001-graph/tasks.md"), TASKS).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n\n[acceptance]\nheld_out = false\nstrength = false\n\n[commands]\nallow_prefix = [[\"./accept.sh\", \"--\"]]\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    dir
}

fn run(r: &Path, state: &Path, json: bool) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_speccoder"));
    if json {
        c.arg("--json");
    }
    c.args(["run", "--task", "T026", "--no-prompt"])
        .arg("-C")
        .arg(r)
        .env("SPECCODER_STATE_DIR", state)
        .env("OPENAI_API_KEY", "mock")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

const RULING: &str = "speccoder task rule T026 --tests-wrong --reason";

/// A task outside a story stopped on its own (wrong) test: the stop the
/// person reads offers the ruling, first among its fix lines.
#[test]
fn a_stop_on_a_task_s_own_test_prints_the_ruling() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    // `--json`: the result's fix lines.
    let (_m, _mock, port) = mock();
    let repo = project(&port);
    let state = tempfile::tempdir().unwrap();
    let out = run(repo.path(), state.path(), true);
    let v = support::result(&out.stdout);
    let fix: Vec<String> = v["fix"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str().map(str::to_owned))
        .collect();
    assert!(
        fix.first().is_some_and(|f| f.starts_with(RULING)),
        "{fix:?}\n{v}"
    );
    assert!(
        v["message"]
            .as_str()
            .unwrap_or_default()
            .contains("its own test `tests/acceptance_t026.txt` failed"),
        "{v}"
    );

    // The terminal: the same line, as `fix:`.
    let (_m, _mock, port) = mock();
    let repo = project(&port);
    let state = tempfile::tempdir().unwrap();
    let out = run(repo.path(), state.path(), false);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains(&format!("fix: {RULING}")), "{text}");
}
