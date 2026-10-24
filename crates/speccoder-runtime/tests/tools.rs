//! speccoder's tools against a temp worktree.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use speccoder_runtime::tools::WorkspaceTools;
use speccoder_runtime::tools::policy::Policy;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn worktree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q"]);
    write(r, ".gitignore", "/target/\n");
    write(
        r,
        "src/lib.rs",
        "pub fn a() -> u8 { 1 }\npub fn b() -> u8 { 2 }\n// TODO: c\n",
    );
    write(r, "src/util.rs", "pub fn helper() {}\n");
    write(r, "README.md", "# demo\n");
    write(r, "specs/001-x/tasks.md", "## Phase 1\n- [ ] T001 a\n");
    write(r, "speccoder.toml", "[checks.t]\nargv = [\"true\"]\n");
    write(r, ".env", "OPENAI_API_KEY=sk-not-real\n");
    write(r, "target/debug/out.txt", "TODO: ignored build output\n");
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn tools(root: &Path) -> WorkspaceTools {
    let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
    WorkspaceTools::new(
        root,
        Policy::new(
            &[argv("cargo test")],
            &[argv("echo"), argv("cat"), argv("sleep")],
        ),
    )
    .unwrap()
}

fn call(t: &WorkspaceTools, name: &str, args: Value) -> Value {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(t.run(name, &args))
}

fn err(v: &Value) -> &str {
    assert_eq!(v["ok"], false, "expected a failure: {v}");
    v["error"].as_str().unwrap()
}

// ── US1: reading ────────────────────────────────────────────────────────────

#[test]
fn read_file_returns_line_ranges() {
    let w = worktree();
    let t = tools(w.path());
    let v = call(
        &t,
        "read_file",
        json!({"path": "src/lib.rs", "start_line": 2, "end_line": 2}),
    );
    assert_eq!(v["content"], "pub fn b() -> u8 { 2 }");
    assert_eq!(v["total_lines"], 3);
}

#[test]
fn list_and_search_respect_gitignore_and_skip_secrets() {
    let w = worktree();
    let t = tools(w.path());
    let files = call(&t, "list_files", json!({}));
    let names: Vec<&str> = files["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert!(names.contains(&"src/lib.rs") && names.contains(&"README.md"));
    assert!(!names.iter().any(|n| n.starts_with("target/")), "{names:?}");
    assert!(!names.contains(&".env"), "secrets are not listed");
    assert!(!names.iter().any(|n| n.starts_with(".git/")));

    let rs = call(&t, "list_files", json!({"path": "src", "glob": "**/*.rs"}));
    assert_eq!(rs["files"], json!(["src/lib.rs", "src/util.rs"]));

    let found = call(&t, "search_code", json!({"pattern": "TODO"}));
    assert_eq!(found["matches"], json!(["src/lib.rs:3: // TODO: c"]));
    assert!(err(&call(&t, "search_code", json!({"pattern": "("}))).contains("invalid pattern"));
}

#[test]
fn escapes_and_secrets_are_refused_for_reading() {
    let w = worktree();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.txt", "outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), w.path().join("link.txt"))
        .unwrap();
    let t = tools(w.path());
    assert!(err(&call(&t, "read_file", json!({"path": "../x"}))).contains(".."));
    assert!(err(&call(&t, "read_file", json!({"path": ".env"}))).contains("credential"));
    #[cfg(unix)]
    assert!(
        err(&call(&t, "read_file", json!({"path": "link.txt"}))).contains("outside the workspace")
    );
}

// ── US2: writing ────────────────────────────────────────────────────────────

/// Every optional argument the schema declares nullable is readable as
/// `null`: the schema never promises what the tool would refuse (D-076).
#[test]
fn every_optional_argument_accepts_null() {
    let w = worktree();
    let t = tools(w.path());
    for spec in speccoder_runtime::tools::catalog() {
        let schema = (spec.parameters)();
        let props = schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for (name, prop) in &props {
            let nullable = prop["type"]
                .as_array()
                .is_some_and(|ts| ts.iter().any(|t| t == "null"))
                || prop["anyOf"]
                    .as_array()
                    .is_some_and(|bs| bs.iter().any(|b| b["type"] == "null"));
            assert!(
                required.contains(&name.as_str()),
                "{}: `{name}` is not required: {schema}",
                spec.name
            );
            if nullable {
                // A call that sets this argument to null reads as if it were
                // absent: never a malformed-arguments error.
                let mut args = json!({});
                for (other, p) in &props {
                    if other == name {
                        args[other] = Value::Null;
                    } else {
                        args[other] = match p["type"].as_str() {
                            Some("string") => json!("src/lib.rs"),
                            Some("array") => json!(["echo", "x"]),
                            _ => Value::Null,
                        };
                    }
                }
                let v = call(&t, spec.name, args.clone());
                let error = v["error"].as_str().unwrap_or_default();
                assert!(
                    !error.contains("invalid") && !error.contains("expected"),
                    "{} with {args}: {v}",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn edit_file_reads_a_null_replace_all_as_one_occurrence() {
    let w = worktree();
    let t = tools(w.path());
    let v = call(
        &t,
        "edit_file",
        json!({"path": "src/lib.rs", "old_string": "{ 1 }", "new_string": "{ 10 }", "replace_all": null}),
    );
    assert_eq!(
        (v["ok"].clone(), v["replacements"].clone()),
        (json!(true), json!(1)),
        "{v}"
    );
}

#[test]
fn closing_a_foreign_schema_only_narrows_it() {
    // What MCP servers' schemas go through (agentplane's pass, AP-19).
    let close_objects = |v: serde_json::Value| agentplane::tools::closed_schema(&v);
    let required_only = close_objects(
        json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
    );
    assert_eq!(required_only["additionalProperties"], false);
    assert!(agentplane::model::strict_schema_problem(&required_only).is_none());
    // No `properties`: free-form, so it stays open (closing it would refuse
    // every argument the server takes).
    let free = json!({"type": "object"});
    assert_eq!(close_objects(free.clone()), free);
    // Closed and argument-less: strict-ready.
    let empty = close_objects(json!({"type": "object", "additionalProperties": false}));
    assert_eq!(
        (empty["properties"].clone(), empty["required"].clone()),
        (json!({}), json!([]))
    );
    assert!(agentplane::model::strict_schema_problem(&empty).is_none());
    // An optional property is not made nullable: the schema stays non-strict.
    let optional =
        close_objects(json!({"type": "object", "properties": {"limit": {"type": "integer"}}}));
    assert!(optional["properties"]["limit"]["type"] == "integer");
    assert!(agentplane::model::strict_schema_problem(&optional).is_some());
    // Composed: closing the root would refuse what a branch adds — left as is.
    let composed = json!({"type": "object", "allOf": [{"properties": {"a": {"type": "string"}}}]});
    assert_eq!(close_objects(composed.clone()), composed);
}

#[test]
fn edit_file_replaces_one_unique_occurrence_and_reports_hashes() {
    let w = worktree();
    let t = tools(w.path());
    let v = call(
        &t,
        "edit_file",
        json!({"path": "src/lib.rs", "old_string": "{ 1 }", "new_string": "{ 10 }"}),
    );
    assert_eq!(v["ok"], true, "{v}");
    assert_ne!(v["before"], v["after"]);
    assert!(
        fs::read_to_string(w.path().join("src/lib.rs"))
            .unwrap()
            .contains("{ 10 }")
    );

    let before = fs::read(w.path().join("src/lib.rs")).unwrap();
    assert!(
        err(&call(
            &t,
            "edit_file",
            json!({"path": "src/lib.rs", "old_string": "pub fn", "new_string": "fn"})
        ))
        .contains("occurs 2 times")
    );
    assert!(
        err(&call(
            &t,
            "edit_file",
            json!({"path": "src/lib.rs", "old_string": "nope", "new_string": "x"})
        ))
        .contains("not found")
    );
    assert_eq!(
        before,
        fs::read(w.path().join("src/lib.rs")).unwrap(),
        "failed edits change nothing"
    );

    let all = call(
        &t,
        "edit_file",
        json!({"path": "src/lib.rs", "old_string": "pub fn", "new_string": "pub(crate) fn", "replace_all": true}),
    );
    assert_eq!(all["replacements"], 2);
}

#[test]
fn write_file_creates_nested_files_with_absent_before_hash() {
    let w = worktree();
    let t = tools(w.path());
    let v = call(
        &t,
        "write_file",
        json!({"path": "src/models/user.rs", "content": "pub struct User;\n"}),
    );
    assert_eq!(v["before"], "absent");
    assert_eq!(
        fs::read_to_string(w.path().join("src/models/user.rs")).unwrap(),
        "pub struct User;\n"
    );
}

#[test]
fn every_protected_class_is_refused_for_writing() {
    let w = worktree();
    let t = tools(w.path());
    for path in [
        "specs/001-x/tasks.md",
        ".specify/memory/constitution.md",
        "speccoder.toml",
        ".github/workflows/ci.yml",
        ".git/config",
        ".env",
        "keys/deploy.pem",
    ] {
        let v = call(&t, "write_file", json!({"path": path, "content": "x"}));
        assert!(err(&v).starts_with("denied"), "{path}: {v}");
    }
    assert_eq!(
        fs::read_to_string(w.path().join("specs/001-x/tasks.md")).unwrap(),
        "## Phase 1\n- [ ] T001 a\n"
    );
}

// ── US3: commands and git ───────────────────────────────────────────────────

#[test]
fn run_command_executes_allowed_argv_with_closed_stdin() {
    let w = worktree();
    let t = tools(w.path());
    let v = call(&t, "run_command", json!({"argv": ["echo", "hello"]}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["stdout"], "hello\n");
    let cat = call(
        &t,
        "run_command",
        json!({"argv": ["cat"], "timeout_secs": 5}),
    );
    assert_eq!(cat["exit_code"], 0, "stdin is closed, so cat ends at once");
    let slow = call(
        &t,
        "run_command",
        json!({"argv": ["sleep", "5"], "timeout_secs": 1}),
    );
    assert_eq!(slow["timed_out"], true);
    // `.` is the workspace root (a model passed it, and the tool refused).
    for cwd in [".", "./", ""] {
        let here = call(
            &t,
            "run_command",
            json!({"argv": ["echo", "x"], "cwd": cwd}),
        );
        assert_eq!(here["ok"], true, "{cwd:?}: {here}");
    }
}

#[test]
fn denied_commands_never_run() {
    let w = worktree();
    let t = tools(w.path());
    for argv in [
        json!(["git", "push"]),
        json!(["git", "reset", "--hard"]),
        json!(["sudo", "true"]),
    ] {
        let v = call(&t, "run_command", json!({ "argv": argv }));
        assert!(err(&v).starts_with("denied"), "{v}");
    }
}

#[test]
fn git_status_and_diff_show_edits() {
    let w = worktree();
    let t = tools(w.path());
    call(
        &t,
        "edit_file",
        json!({"path": "README.md", "old_string": "# demo", "new_string": "# demo 2"}),
    );
    call(
        &t,
        "write_file",
        json!({"path": "NOTES.md", "content": "n\n"}),
    );
    let status = call(&t, "git_status", json!({}));
    let entries = status["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e == " M README.md"), "{status}");
    assert!(entries.iter().any(|e| e == "?? NOTES.md"), "{status}");
    let diff = call(&t, "git_diff", json!({"path": "README.md"}));
    assert!(diff["diff"].as_str().unwrap().contains("+# demo 2"));
}

#[test]
fn invalid_arguments_are_reported_not_panicked() {
    let w = worktree();
    let t = tools(w.path());
    assert!(err(&call(&t, "read_file", json!({"file": "x"}))).contains("invalid arguments"));
    assert!(err(&call(&t, "nope", json!({}))).contains("unknown tool"));
}

#[test]
fn declarations_cover_every_tool_with_a_schema() {
    let catalog = speccoder_runtime::tools::catalog();
    let names: Vec<&str> = catalog.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "read_file",
            "list_files",
            "search_code",
            "write_file",
            "edit_file",
            "run_command",
            "git_status",
            "git_diff"
        ]
    );
    for t in &catalog {
        let schema = (t.parameters)();
        assert_eq!(schema["type"], "object", "{}: {schema}", t.name);
        assert_eq!(
            agentplane::model::strict_schema_problem(&schema),
            None,
            "{} is not strict-compatible: {schema}",
            t.name
        );
        assert_eq!(
            t.mutates,
            !t.model_fields.is_empty(),
            "{}: mutating tools declare model-authored fields",
            t.name
        );
    }
    assert_eq!(
        speccoder_runtime::tools::declarations().len(),
        catalog.len()
    );
}

/// speccoder generates its schemas strict-ready (agentplane
/// does not rewrite them), and proves it against agentplane's per-provider
/// allow-lists — so every tool and every answer keeps constrained decoding.
#[test]
fn every_tool_and_answer_schema_is_strict_for_both_providers() {
    use agentplane::model::StrictSubset;
    use speccoder_runtime::substrate::{acceptance, attempt, converge, host, monitor, propose};
    let mut schemas: Vec<(String, serde_json::Value)> = speccoder_runtime::tools::catalog()
        .iter()
        .map(|t| (t.name.to_owned(), (t.parameters)()))
        .collect();
    schemas.extend([
        ("attempt report".to_owned(), attempt::report_schema()),
        ("propose".to_owned(), propose::schema()),
        ("acceptance".to_owned(), acceptance::schema()),
        ("converge".to_owned(), converge::schema()),
        ("monitor".to_owned(), monitor::schema()),
        ("host".to_owned(), host::schema()),
    ]);
    for (name, schema) in &schemas {
        for subset in [StrictSubset::OpenAi, StrictSubset::Anthropic] {
            assert_eq!(
                subset.problem(schema),
                None,
                "{name} under {subset:?}: {schema}"
            );
        }
    }
}
