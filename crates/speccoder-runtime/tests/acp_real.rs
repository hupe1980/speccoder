//! A real agent makes one small attempt over ACP, governed and judged by
//! speccoder. Not run by default: it starts the agent, which needs its own
//! sign-in and may cost money. Run one with
//! `SPECCODER_REAL_ACP=copilot cargo nextest run -p speccoder-runtime --test acp_real --run-ignored only`
//! (`copilot`, `opencode`, `claude`, `codex` or `gemini`; the `npx` ones
//! download their adapter first).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use speccoder_core::{CommandData, CommandStatus};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
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

const TASKS: &str = "# Tasks: Greet\n\n## Phase 1: User Story 1 - Greet (Priority: P1) 🎯 MVP\n\n- [ ] T001 [US1] Add `pub fn greet() -> &'static str` returning \"hello\" to src/lib.rs\n";

/// One POC task for `agent`: add `greet` to `src/lib.rs`; the check is red
/// before and green after.
fn poc(agent: &str) -> tempfile::TempDir {
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
    fs::write(r.join("specs/001-graph/tasks.md"), TASKS).unwrap();
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Greet\n\n`greet()` returns \"hello\".\n",
    )
    .unwrap();
    fs::write(
        r.join("specs/001-graph/plan.md"),
        "# Plan\n\nOne Rust function.\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "//! Greetings.\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n[checks.greets]\nargv = [\"grep\", \"-q\", \"fn greet\", \"src/lib.rs\"]\n[budget]\nmax_attempts = 1\n[acceptance]\nheld_out = false\n[executor]\nagent = \"{agent}\"\ntimeout_secs = 900\n"
        ),
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec"]);
    dir
}

#[derive(Debug)]
struct Yes;

impl speccoder_runtime::approvals::Approver for Yes {
    fn ask(
        &self,
        p: &speccoder_runtime::approvals::Presentation,
    ) -> speccoder_runtime::approvals::Answer {
        eprintln!("asked: {} {}", p.title, p.subject);
        speccoder_runtime::approvals::Answer::approve(
            speccoder_runtime::approvals::Scope::Once,
            None,
            None,
        )
    }
}

#[test]
#[ignore = "starts a real agent: SPECCODER_REAL_ACP=<agent>"]
fn a_real_agent_makes_one_attempt_judged_by_speccoder() {
    let Ok(agent) = std::env::var("SPECCODER_REAL_ACP") else {
        eprintln!("SPECCODER_REAL_ACP is not set: nothing to run");
        return;
    };
    let r = poc(&agent);
    let state = tempfile::tempdir().unwrap();
    let d: Arc<dyn ModelProvider> = FakeProvider::new();
    // A person at the terminal who says yes to what speccoder asks them.
    let res = speccoder_runtime::run_with_approver(
        r.path(),
        None,
        "T001",
        state.path(),
        d,
        None,
        Arc::new(Yes),
    );
    if res.code().is_some_and(|c| c.name == "EXECUTOR-LOGIN") {
        panic!(
            "{agent} is not logged in: {} — {:?}",
            res.message(),
            res.fix()
        );
    }
    let Some(CommandData::Run(rep)) = res.data() else {
        panic!("{} → {:?}", res.message(), res.data());
    };
    eprintln!(
        "{agent}: {} · {} · {:?}\nnotices: {:#?}\ncalls: {}",
        res.message(),
        rep.attempt_status,
        rep.verdict,
        rep.notices,
        serde_json::to_string_pretty(&rep.tool_calls).unwrap_or_default()
    );
    let a = rep.agent.as_ref().expect("an agent attempt");
    eprintln!("agent: {a:?}");
    // speccoder judged it, as any attempt: a verdict, or an honest stop.
    assert!(
        rep.verdict.is_some()
            || matches!(rep.attempt_status.as_str(), "blocked" | "needs_approval"),
        "{}: {}",
        rep.attempt_status,
        res.message()
    );
    if res.status() == CommandStatus::Ok {
        assert!(rep.ticked);
        assert!(
            fs::read_to_string(PathBuf::from(&rep.worktree).join("src/lib.rs"))
                .unwrap()
                .contains("fn greet")
        );
    }
}
