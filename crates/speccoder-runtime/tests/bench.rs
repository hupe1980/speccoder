//! `speccoder bench import`: a benchmark instance becomes an eval
//! case whose oracle is the benchmark's own test patch.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::bench::{ImportOptions, import};
use speccoder_runtime::eval::run_suite;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn an_instance_becomes_a_case_judged_by_its_own_tests() {
    // The "upstream" repository, with the bug the issue reports.
    let repos = tempfile::tempdir().unwrap();
    let up = repos.path().join("acme__dbl");
    fs::create_dir_all(up.join("src")).unwrap();
    fs::write(
        up.join("Cargo.toml"),
        "[package]\nname = \"dbl\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        up.join("src/lib.rs"),
        "pub fn double(x: i32) -> i32 {\n    x + x + 1\n}\n",
    )
    .unwrap();
    fs::write(up.join(".gitignore"), "target/\nCargo.lock\n").unwrap();
    git(&up, &["init", "-q", "-b", "main"]);
    git(&up, &["add", "-A"]);
    git(&up, &["commit", "-qm", "dbl"]);
    let base = git(&up, &["rev-parse", "HEAD"]).trim().to_owned();
    // The benchmark's test patch, made the way the dataset has it.
    fs::create_dir_all(up.join("tests")).unwrap();
    fs::write(
        up.join("tests/oracle.rs"),
        "#[test]\nfn doubles() {\n    assert_eq!(dbl::double(2), 4);\n}\n",
    )
    .unwrap();
    git(&up, &["add", "-N", "tests/oracle.rs"]);
    let test_patch = git(&up, &["diff"]);
    git(&up, &["reset", "-q"]);
    fs::remove_dir_all(up.join("tests")).unwrap();
    let instance = json!({
        "instance_id": "acme__dbl-1",
        "repo": "acme/dbl",
        "base_commit": base,
        "problem_statement": "double(2) returns 5\n\n`double` should return twice its argument.",
        "test_patch": test_patch,
        "FAIL_TO_PASS": "[\"doubles\"]",
    });
    let work = tempfile::tempdir().unwrap();
    let file = work.path().join("instances.jsonl");
    fs::write(&file, format!("{instance}\n{}\n", json!({ "instance_id": "gone-1", "repo": "acme/gone", "base_commit": "x", "problem_statement": "p", "test_patch": "" }))).unwrap();
    let out = work.path().join("suite");

    let res = import(
        &file,
        &ImportOptions {
            repos: repos.path(),
            out: &out,
            test_cmd: None,
            limit: None,
        },
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Bench(b)) = res.data() else {
        panic!()
    };
    assert_eq!(b.imported, ["acme__dbl-1"]);
    assert!(
        b.skipped[0].starts_with("gone-1: no clone of acme/gone"),
        "{:?}",
        b.skipped
    );
    let case = out.join("acme--dbl-1");
    let spec = fs::read_to_string(case.join("project/specs/001-acme--dbl-1/spec.md")).unwrap();
    assert!(
        spec.contains("### User Story 1 - double(2) returns 5")
            && spec.contains("twice its argument"),
        "{spec}"
    );
    // The agent never sees the benchmark's test; the oracle has it.
    assert!(!case.join("project/tests/oracle.rs").exists());
    assert!(case.join("oracle/tests/oracle.rs").exists());
    let case_toml = fs::read_to_string(case.join("case.toml")).unwrap();
    assert!(
        case_toml.contains("argv = [\"cargo\", \"test\", \"--quiet\"]")
            && case_toml.contains("FAIL_TO_PASS: doubles"),
        "{case_toml}"
    );
    assert!(out.join("eval.toml").exists());

    // It runs like any case: a fix the oracle confirms is correct.
    let fake = FakeProvider::new();
    fake.will_call_tool("w", "write_file", json!({ "path": "src/lib.rs", "content": "pub fn double(x: i32) -> i32 {\n    x * 2\n}\n" }));
    fake.will_structure(json!({ "summary": "fixed double", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = fake;
    let r = run_suite(&out, None, &d, None, None, 1);
    let Some(CommandData::Eval(e)) = r.data() else {
        panic!("{}", r.message())
    };
    assert_eq!(e.cases[0].classification, "correct", "{:?}", e.cases[0]);
    assert_eq!(e.cases[0].oracle_passed, Some(true));
}
