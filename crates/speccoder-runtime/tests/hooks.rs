//! Hooks end to end with a scripted model (spec 095): a gate the model
//! cannot pass, trusted by digest.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::hooks;

const BASE: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"]]\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// The `graph` fixture, committed, with `hooks` added to its configuration
/// and `files` beside it.
fn repo(hooks: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
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
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{BASE}{hooks}")).unwrap();
    for (path, text) in files {
        let p = r.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "t"]);
    git(r, &["config", "user.email", "t@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// Trusts every hook of `repo` as it is now.
fn trust_all(repo: &Path, state: &Path) {
    let dir = hooks::dir(state, repo);
    for def in hooks::load(repo).unwrap() {
        hooks::trust::record(&dir, &hooks::digest(&def, repo), "t", "terminal");
    }
}

fn model() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn run(repo: &Path, state: &Path) -> (CommandResult, Arc<FakeProvider>) {
    let (f, d) = model();
    (
        speccoder_runtime::run_with(repo, None, "T005", state, d, None),
        f,
    )
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn message_of(worktree: &str, rev: &str) -> String {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%B", rev])
        .current_dir(worktree)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn sandboxed() -> bool {
    speccoder_runtime::sandbox::backend().is_ok()
}

#[test]
fn a_gate_that_exits_zero_lets_the_tick_through_and_is_on_it() {
    if !sandboxed() {
        return;
    }
    let r = repo("\n[hooks.ok]\nat = \"tick\"\nargv = [\"true\"]\n", &[]);
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    let (res, _) = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked);
    assert_eq!(rep.gates.len(), 1);
    let tick = message_of(&rep.worktree, rep.commit.as_deref().unwrap());
    assert!(tick.contains("\nGate: ok passed\n"), "{tick}");
}

/// SC-001: every end but a clean exit 0 blocks, and nothing is ticked.
#[test]
fn every_other_end_of_a_gate_blocks_the_tick() {
    if !sandboxed() {
        return;
    }
    for (argv, extra, words) in [
        (r#"["false"]"#, "", "exit 1"),
        (
            r#"["sh", "-c", "echo checking; echo 'GPL-3.0 in foo' >&2; exit 2"]"#,
            "",
            "GPL-3.0 in foo",
        ),
        (r#"["sh", "-c", "kill -9 $$"]"#, "", ""),
        (r#"["sleep", "5"]"#, "timeout = 1\n", "timed out after 1 s"),
        // Refused by the sandbox, or caught as a change: blocked either way.
        (r#"["sh", "-c", "echo x >> README.md"]"#, "", "README.md"),
        (
            r#"["sh", "-c", "echo x > new.txt"]"#,
            "",
            "changed `new.txt`",
        ),
        (r#"["no-such-program-anywhere"]"#, "", "could not start"),
    ] {
        let r = repo(
            &format!("\n[hooks.licence]\nat = \"tick\"\nargv = {argv}\n{extra}"),
            &[("README.md", "hello\n")],
        );
        let state = tempfile::tempdir().unwrap();
        trust_all(r.path(), state.path());
        let (res, f) = run(r.path(), state.path());
        assert_eq!(
            res.status(),
            CommandStatus::NeedsHuman,
            "{argv}: {}",
            res.message()
        );
        assert_eq!(res.code().map(|c| c.name), Some("HOOK-BLOCKED"), "{argv}");
        let rep = report(&res);
        assert!(!rep.ticked && rep.commit.is_none(), "{argv}");
        assert_eq!(rep.attempt_status, "blocked_by_gate", "{argv}");
        let reason = rep.reason.as_deref().unwrap();
        assert!(reason.starts_with("Blocked by licence — "), "{reason}");
        assert!(reason.contains(words), "{argv}: {reason}");
        // The verdict it earned is kept on the saved attempt.
        let saved = rep.saved_attempt.as_deref().unwrap();
        let record = message_of(&rep.worktree, saved);
        assert!(record.contains("\nOutcome: blocked_by_gate\n"), "{record}");
        assert!(record.contains("\nGate: licence blocked\n"), "{record}");
        assert!(record.contains("\nEarned-Verdict: "), "{record}");
        // The gate's words reach the person, never the model.
        let asked: Vec<Value> = f.asked().into_iter().map(|a| a.prompt).collect();
        assert!(
            !serde_json::to_string(&asked).unwrap().contains("GPL-3.0"),
            "{argv}"
        );
        // The tick did not land in the worktree either.
        let tasks =
            fs::read_to_string(Path::new(&rep.worktree).join("specs/001-graph/tasks.md")).unwrap();
        assert!(!tasks.contains("- [X] T005"), "{argv}");
    }
}

/// SC-006: a new gate, and one whose script changed by a byte, block until
/// a person trusts them.
#[test]
fn an_untrusted_or_changed_gate_blocks_until_trusted() {
    if !sandboxed() {
        return;
    }
    let config = "\n[hooks.licence]\nat = \"tick\"\nargv = [\"sh\", \"scripts/licence.sh\"]\n";
    let r = repo(config, &[("scripts/licence.sh", "exit 0\n")]);
    let state = tempfile::tempdir().unwrap();
    let (res, _) = run(r.path(), state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-UNTRUSTED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("waits for your trust"),
        "{}",
        res.message()
    );

    // Trusted, then changed by one byte in a new commit: blocked again.
    let r = repo(config, &[("scripts/licence.sh", "exit 0\n")]);
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    fs::write(r.path().join("scripts/licence.sh"), "exit 0 \n").unwrap();
    git(r.path(), &["commit", "-qam", "change the gate"]);
    let (res, _) = run(r.path(), state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-UNTRUSTED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("changed since it was trusted"),
        "{}",
        res.message()
    );
}

#[test]
fn without_an_enforced_sandbox_a_gate_blocks() {
    let r = repo(
        "\n[sandbox]\nmode = \"off\"\n\n[hooks.ok]\nat = \"tick\"\nargv = [\"true\"]\n",
        &[],
    );
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    let (res, _) = run(r.path(), state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-BLOCKED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("no sandbox is enforced"),
        "{}",
        res.message()
    );
}

/// A person's acceptance does not pass the person's own gates.
#[test]
fn accepting_a_task_is_gated_too() {
    if !sandboxed() {
        return;
    }
    let r = repo("\n[hooks.no]\nat = \"tick\"\nargv = [\"false\"]\n", &[]);
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    let res = speccoder_runtime::controls::accept_with(
        r.path(),
        None,
        "T005",
        "done by hand",
        state.path(),
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("HOOK-BLOCKED"));
}

/// A gate cannot read speccoder's state: the trust it runs under, the
/// journal, the held-out tests.
#[test]
fn a_gate_cannot_read_speccoders_state() {
    if !sandboxed() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let state_root = fs::canonicalize(state.path()).unwrap();
    // Where its trust will be: the gate tries to read it.
    let r = repo(
        &format!(
            "\n[hooks.peek]\nat = \"tick\"\nargv = [\"sh\", \"-c\", \"cat {}/*/hooks/trust.json\"]\n",
            state_root.display()
        ),
        &[],
    );
    trust_all(r.path(), state.path());
    let (res, _) = run(r.path(), state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-BLOCKED"),
        "{}",
        res.message()
    );
    assert!(!res.message().contains("sha256:"), "{}", res.message());
}

/// A project in a folder of its repository: its gates stand at its ticks
/// (review finding: they were looked up at the repository's top only).
#[test]
fn a_nested_projects_gate_stands_at_its_tick() {
    if !sandboxed() {
        return;
    }
    let r = repo("\n[hooks.no]\nat = \"tick\"\nargv = [\"false\"]\n", &[]);
    // Move the project one folder down, keep the repository at the top.
    let pkg = r.path().join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    for entry in ["specs", "src", "speccoder.toml", ".specify"] {
        if r.path().join(entry).exists() {
            fs::rename(r.path().join(entry), pkg.join(entry)).unwrap();
        }
    }
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "nest"]);
    let state = tempfile::tempdir().unwrap();
    trust_all(&pkg, state.path());
    let (res, _) = run(&pkg, state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-BLOCKED"),
        "{}",
        res.message()
    );
}
