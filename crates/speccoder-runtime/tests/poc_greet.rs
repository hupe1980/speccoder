//! The POC replayed: the one-function Rust feature whose
//! fourteen ticks were all regression-only with nothing saying why. A real
//! Cargo crate judged by `cargo nextest` with its JUnit report; a scripted
//! model plays the POC's tasks: T001–T002 (Setup), T003–T004 (Foundational;
//! T003 implements `greet`) and T005 (US1, which writes its test where Cargo
//! never builds it, then — told the rule — where it does).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, RunReport};
use speccoder_runtime::run_with;

fn has(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "core.hooksPath=/dev/null",
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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// `[commands]` as `speccoder init` writes it: exact argvs, so the author's
// `cargo test --test <file>` runs because the layout names it, not by a prefix.
const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.test]\nargv = [\"cargo\", \"nextest\", \"run\", \"--profile\", \"speccoder\", \"--no-tests=warn\"]\njunit = \"target/nextest/speccoder/junit.xml\"\n\n[commands]\nallow = [[\"cargo\", \"test\"], [\"cargo\", \"check\"], [\"cargo\", \"build\"]]\n\n[acceptance]\nheld_out = false\n";

/// The POC at its planning commit, as a committed repository.
fn poc() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poc-greet");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::rename(r.join("Cargo.toml.fixture"), r.join("Cargo.toml")).unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    fs::create_dir_all(r.join(".config")).unwrap();
    fs::write(
        r.join(".config/nextest.toml"),
        "[profile.speccoder.junit]\npath = \"junit.xml\"\n",
    )
    .unwrap();
    fs::write(r.join(".gitignore"), "target\n").unwrap();
    assert!(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(r)
            .status()
            .unwrap()
            .success()
    );
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec(001-greet-function)"]);
    dir
}

fn done(summary: &str) -> Value {
    json!({ "summary": summary, "status": "done", "files_changed": [], "checks_run": [], "notes": [] })
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn run(repo: &Path, state: &Path, task: &str, f: Arc<FakeProvider>) -> CommandResult {
    let d: Arc<dyn ModelProvider> = f;
    run_with(repo, None, task, state, d, None)
}

/// `src/lib.rs` as the POC's T003 left it.
const GREET: &str = "pub fn greet(name: &str) -> String {\n    if name.is_empty() {\n        \"Hello, world!\".to_string()\n    } else {\n        format!(\"Hello, {}!\", name)\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn it_works() {\n        assert_eq!(greet(\"world\"), \"Hello, world!\");\n    }\n}\n";

const TEST: &str = "use greet::greet;\n\n#[test]\nfn greets_by_name() {\n    assert_eq!(greet(\"Ada\"), \"Hello, Ada!\");\n}\n";

#[test]
fn the_poc_s_dead_test_files_fail_and_every_tick_says_why() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let r = poc();
    let state = tempfile::tempdir().unwrap();

    // T001, T002 (Setup): nothing to derive a test from.
    for task in ["T001", "T002"] {
        let f = FakeProvider::new();
        f.will_structure(done("the crate structure is in place"));
        let res = run(r.path(), state.path(), task, f);
        let rep = report(&res);
        assert_eq!(
            rep.verdict,
            Some(Verdict::RegressionOnly),
            "{}",
            res.message()
        );
        assert_eq!(
            rep.regression.as_ref().unwrap().text,
            "no user story — nothing to derive a test from"
        );
    }

    // T003 (Foundational) implements `greet`: US1's behaviour, early.
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": GREET }),
    );
    f.will_structure(done("greet"));
    let t003 = run(r.path(), state.path(), "T003", f);
    let rep = report(&t003);
    assert_eq!(
        rep.verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        t003.message()
    );
    assert_eq!(rep.regression.as_ref().unwrap().reason.name(), "no_story");
    let t003_commit: String = rep.commit.clone().unwrap().chars().take(7).collect();
    let f = FakeProvider::new();
    f.will_structure(done("the rules are encoded"));
    let t004 = run(r.path(), state.path(), "T004", f);
    assert_eq!(
        report(&t004).verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        t004.message()
    );

    // T005: its acceptance tests already pass; its own test goes to
    // `tests/unit/`, which Cargo never builds — then, told the rule, to
    // `tests/greet_name.rs`.
    let f = FakeProvider::new();
    f.will_structure(json!({
        "files": [{ "path": "tests/acceptance_us1.rs", "content": TEST, "scenarios": ["US1-1"] }],
        "argv": ["cargo", "test", "--test", "acceptance_us1"],
        "notes": []
    }));
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "tests/unit/greet_name.rs", "content": TEST }),
    );
    f.will_structure(done("added unit test coverage"));
    f.will_call_tool(
        "c2",
        "write_file",
        json!({ "path": "tests/greet_name.rs", "content": TEST }),
    );
    f.will_structure(done("moved the test where Cargo builds it"));
    let t005 = run(r.path(), state.path(), "T005", f.clone());
    let rep = report(&t005);
    assert_eq!(
        rep.verdict,
        Some(Verdict::RegressionOnly),
        "{}",
        t005.message()
    );
    // The dead file failed the first attempt with Cargo's rule …
    assert_eq!(
        rep.earlier[0].verdict,
        Some(Verdict::Failed {
            reasons: vec!["tests not collected: `tests/unit/greet_name.rs` — Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare); this file is neither".to_owned()]
        })
    );
    // … the retry was told, and its file ran (the JUnit report names it).
    let c = rep.collection.as_ref().unwrap();
    assert_eq!(c.files[0].path, "tests/greet_name.rs");
    assert_eq!(
        c.files[0].decided_by,
        Some(speccoder_core::collection::DecidedBy::Junit)
    );
    assert_eq!(c.files[0].tests, 1);
    // Why — T003 implemented it, found by running the tests.
    assert_eq!(
        rep.regression.as_ref().unwrap().text,
        "already passing before the attempt — implemented by T003"
    );
    let message = git(Path::new(&rep.worktree), &["log", "-1", "--format=%B"]);
    assert!(
        message.contains(&format!(
            "Regression-Only: already_passing implemented_by=T003 commit={t003_commit}\n"
        )),
        "{message}"
    );

    // One diagnosis for the feature: 5 of 5 ticks regression-only.
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), None, Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    let u: Vec<_> = i.items.iter().filter(|i| i.kind == "unverified").collect();
    assert_eq!(u.len(), 1, "{:?}", i.items);
    assert_eq!(
        u[0].title,
        "Little was verified in `001-greet-function`: 5 of 5 ticks are regression-only"
    );
    let texts: Vec<&str> = u[0]
        .unverified
        .as_ref()
        .unwrap()
        .counts
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "4 had no user story",
            "1 was already passing — implemented by T003"
        ]
    );
}

/// The author gave its command as one string — the line its prompt shows
/// (`["cargo test --test acceptance_us1"]`, a real answer of 2026-10-29):
/// it is that command's words, and the acceptance tests run.
#[test]
fn an_acceptance_command_given_as_one_string_runs() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let r = poc();
    let state = tempfile::tempdir().unwrap();
    for task in ["T001", "T002", "T003", "T004"] {
        let f = FakeProvider::new();
        if task == "T003" {
            f.will_call_tool(
                "c1",
                "write_file",
                json!({ "path": "src/lib.rs", "content": GREET }),
            );
        }
        f.will_structure(done("done"));
        let _ = run(r.path(), state.path(), task, f);
    }
    let f = FakeProvider::new();
    f.will_structure(json!({
        "files": [{ "path": "tests/acceptance_us1.rs", "content": TEST, "scenarios": ["US1-1"] }],
        "argv": ["cargo test --test acceptance_us1"],
        "notes": []
    }));
    f.will_structure(done("nothing to add"));
    let t005 = run(r.path(), state.path(), "T005", f);
    let acceptance = report(&t005).acceptance.clone().unwrap();
    assert!(acceptance.get("skipped").is_none(), "{acceptance}");
    assert_eq!(
        acceptance["argv"],
        json!(["cargo", "test", "--test", "acceptance_us1"])
    );
}
