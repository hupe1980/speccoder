//! `speccoder hooks` and the gates around `pr`, through the binary (spec
//! 095): trust by digest, CI's `--trust-hook`, and a pull request a gate
//! stops before anything is pushed.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The `graph` fixture with `hooks`, committed, with a bare `origin` and a
/// unit branch holding one tick.
fn project(hooks: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let r = dir.path();
    std::fs::write(
        r.join("speccoder.toml"),
        format!("[model]\nprovider = \"openai\"\nmodel = \"m\"\n\n[checks.ok]\nargv = [\"true\"]\n{hooks}"),
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare"]);
    git(
        r,
        &["remote", "add", "origin", &origin.path().to_string_lossy()],
    );
    git(r, &["branch", "speccoder/001-graph"]);
    (dir, origin)
}

fn speccoder(dir: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(args)
        .current_dir(dir)
        .env("SPECCODER_STATE_DIR", state)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

const GATE: &str = "\n[hooks.licence]\nat = \"pr\"\nargv = [\"sh\", \"scripts/licence.sh\"]\n";

#[test]
fn a_hook_is_listed_untrusted_with_the_flag_that_trusts_it_in_ci() {
    let (p, _origin) = project(GATE);
    let state = tempfile::tempdir().unwrap();
    let list = json(&speccoder(p.path(), state.path(), &["hooks", "--json"]));
    let h = &list["data"]["hooks"][0];
    assert_eq!(h["name"], "licence");
    assert_eq!(h["state"], "untrusted");
    let flag = h["trust_flag"].as_str().unwrap().to_owned();
    assert!(flag.starts_with("--trust-hook licence=sha256:"), "{flag}");
    // A stale digest is refused, naming the current one.
    let stale = speccoder(
        p.path(),
        state.path(),
        &["hooks", "--json", "--trust-hook", "licence=sha256:00"],
    );
    let stale = json(&stale);
    assert_eq!(stale["code"], "HOOK-UNTRUSTED", "{stale}");
    assert!(
        stale["message"]
            .as_str()
            .unwrap()
            .contains(h["digest"].as_str().unwrap())
    );
    // The printed one trusts it.
    let value = flag.trim_start_matches("--trust-hook ");
    let trusted = json(&speccoder(
        p.path(),
        state.path(),
        &["hooks", "--json", "--trust-hook", value],
    ));
    assert_eq!(trusted["data"]["hooks"][0]["state"], "trusted", "{trusted}");
    // Without a terminal, `hooks trust` never trusts on its own.
    let p2 = project(GATE.replace("licence", "other").as_str()).0;
    let asked = speccoder(p2.path(), state.path(), &["hooks", "trust"]);
    assert_eq!(
        asked.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&asked.stderr)
    );
}

/// A gate before a pull request: blocked, nothing is pushed.
#[test]
fn a_blocking_pr_gate_pushes_nothing() {
    let (p, origin) = project(GATE);
    std::fs::create_dir_all(p.path().join("scripts")).unwrap();
    std::fs::write(
        p.path().join("scripts/licence.sh"),
        "echo 'GPL-3.0 in foo' >&2\nexit 1\n",
    )
    .unwrap();
    git(p.path(), &["add", "-A"]);
    git(p.path(), &["commit", "-q", "-m", "gate"]);
    git(p.path(), &["branch", "-f", "speccoder/001-graph"]);
    git(p.path(), &["checkout", "-q", "speccoder/001-graph"]);
    git(
        p.path(),
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "T001 Do it\n\nSpec-Task: 001-graph/T001\nVerdict: verified\n",
        ],
    );
    git(p.path(), &["checkout", "-q", "main"]);
    let state = tempfile::tempdir().unwrap();
    let flag = json(&speccoder(p.path(), state.path(), &["hooks", "--json"]))["data"]["hooks"][0]["trust_flag"]
        .as_str()
        .unwrap()
        .trim_start_matches("--trust-hook ")
        .to_owned();
    let out = speccoder(
        p.path(),
        state.path(),
        &[
            "pr",
            "--feature",
            "001-graph",
            "--yes",
            "--json",
            "--trust-hook",
            &flag,
        ],
    );
    let r = json(&out);
    assert_eq!(r["code"], "HOOK-BLOCKED", "{r}");
    let message = r["message"].as_str().unwrap();
    assert!(message.contains("nothing was pushed"), "{message}");
    // Blocked by the gate itself, or (no sandbox here) because none is enforced.
    assert!(
        message.contains("GPL-3.0 in foo") || message.contains("no sandbox"),
        "{message}"
    );
    assert_eq!(
        git(origin.path(), &["for-each-ref"]),
        "",
        "nothing reached the remote"
    );
}
