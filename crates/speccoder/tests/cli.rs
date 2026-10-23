//! The command-line contract: commands, `--json` output and exit codes.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;

fn speccoder() -> Command {
    let mut cmd = Command::cargo_bin("speccoder").unwrap();
    cmd.env_remove("SPECIFY_FEATURE_DIRECTORY");
    cmd
}

#[test]
fn version_prints_name_and_workspace_version() {
    let out = speccoder()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("speccoder {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_inspect_and_json() {
    let out = speccoder()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("inspect"), "{stdout}");
    assert!(stdout.contains("--json"), "{stdout}");
}

fn fixture(name: &str) -> String {
    format!(
        "{}/../speccoder-speckit/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn inspect_reports_the_project_and_modifies_nothing() {
    let out = speccoder()
        .args(["inspect", &fixture("valid")])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Spec Kit 1.1.3 (claude, sh)"), "{stdout}");
    assert!(stdout.contains("001-user-auth"), "{stdout}");
    assert!(
        stdout.contains("7 total · 2 done · 1 ready · 4 blocked"),
        "{stdout}"
    );
    assert!(
        stdout.contains("blocked by T003, T004, T005 (+1)"),
        "{stdout}"
    );
    assert!(stdout.contains("[x] T001"), "{stdout}");
    assert!(
        stdout.trim_end().ends_with("No files modified."),
        "{stdout}"
    );
}

#[test]
fn json_inspect_prints_one_versioned_object_with_the_snapshot() {
    let out = speccoder()
        .args(["--json", "inspect", &fixture("valid")])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["command"], "inspect");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["exit_code"], 0);
    assert_eq!(v["data"]["selection"]["reason"], "recorded");
    assert_eq!(v["data"]["feature"]["tasks"]["totals"]["total"], 7);
    assert_eq!(v["data"]["feature"]["tasks"]["tasks"][4]["story"], "US1");
}

#[test]
fn error_diagnostics_exit_11() {
    let out = speccoder()
        .args(["--json", "inspect", &fixture("malformed")])
        .assert()
        .code(11)
        .get_output()
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "diagnostics_error");
}

#[test]
fn graph_errors_exit_11() {
    let out = speccoder()
        .args(["inspect", &fixture("graph-errors")])
        .assert()
        .code(11)
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("graph.cycle"), "{stdout}");
    assert!(stdout.contains("graph.dangling"), "{stdout}");
}

#[test]
fn json_schedule_carries_states_and_blockers() {
    let out = speccoder()
        .args(["--json", "inspect", &fixture("graph")])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let t10 = &v["data"]["feature"]["schedule"]["tasks"][9];
    assert_eq!(t10["id"], "T010");
    assert_eq!(t10["state"], "blocked");
    assert_eq!(t10["blocked_by"], serde_json::json!(["T008", "T009"]));
    assert_eq!(v["data"]["feature"]["schedule"]["totals"]["ready"], 2);
}

#[test]
fn several_features_without_selection_exit_12_and_list_them() {
    let out = speccoder()
        .args(["inspect", &fixture("multi")])
        .assert()
        .code(12)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    let order: Vec<_> = ["001-a", "002-b", "010-c"]
        .iter()
        .map(|n| stderr.find(n).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{stderr}");
    speccoder()
        .args(["inspect", &fixture("multi"), "--feature", "2"])
        .assert()
        .success();
}

#[test]
fn specify_feature_directory_selects_the_feature() {
    let out = speccoder()
        .args(["inspect", &fixture("real/speckit-1.2.2")])
        .env("SPECIFY_FEATURE_DIRECTORY", "specs/002-reports")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("002-reports"), "{stdout}");
    assert!(
        stdout.contains("from SPECIFY_FEATURE_DIRECTORY"),
        "{stdout}"
    );
}

#[test]
fn verify_reports_checks_and_exits_20_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("speccoder.toml"),
        "[checks.ok]\nargv = [\"true\"]\n\n[checks.bad]\nargv = [\"false\"]\n",
    )
    .unwrap();
    let out = speccoder()
        .arg("verify")
        .arg(dir.path())
        .assert()
        .code(20)
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("✓ ok"), "{stdout}");
    assert!(stdout.contains("✗ bad"), "{stdout}");
    assert!(
        stdout.trim_end().ends_with("Verification failed."),
        "{stdout}"
    );

    let out = speccoder()
        .args(["--json", "verify", "--tier", "fast"])
        .arg(dir.path())
        .assert()
        .code(20)
        .get_output()
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "verification_failed");
    assert_eq!(v["data"]["checks"][1]["status"], "failed");
}

#[test]
fn verify_without_config_exits_13() {
    let dir = tempfile::tempdir().unwrap();
    speccoder().arg("verify").arg(dir.path()).assert().code(13);
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A committed Spec Kit project (copied from the `valid` fixture) in a git repo.
fn committed_project(ignore_specs: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = std::path::PathBuf::from(fixture("valid"));
    let copy = std::process::Command::new("cp")
        .arg("-R")
        .arg(src.join("."))
        .arg(dir.path())
        .status()
        .unwrap();
    assert!(copy.success());
    if ignore_specs {
        std::fs::write(dir.path().join(".gitignore"), "/specs/\n").unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

#[test]
fn worktree_is_created_then_reused() {
    let project = committed_project(false);
    let state = tempfile::tempdir().unwrap();
    let run = || {
        speccoder()
            .args(["--json", "worktree"])
            .arg(project.path())
            .env("SPECCODER_STATE_DIR", state.path())
            .assert()
            .success()
            .get_output()
            .clone()
    };
    let first: serde_json::Value = serde_json::from_slice(&run().stdout).unwrap();
    assert_eq!(first["data"]["branch"], "speccoder/001-user-auth");
    assert_eq!(first["data"]["created"], true);
    let path = first["data"]["path"].as_str().unwrap().to_owned();
    assert!(
        std::path::Path::new(&path)
            .join("specs/001-user-auth/tasks.md")
            .exists()
    );
    let second: serde_json::Value = serde_json::from_slice(&run().stdout).unwrap();
    assert_eq!(second["data"]["created"], false);
    assert_eq!(second["data"]["path"], path.as_str());
}

#[test]
fn worktree_refuses_ignored_specs_with_exit_14() {
    let project = committed_project(true);
    let state = tempfile::tempdir().unwrap();
    let out = speccoder()
        .arg("worktree")
        .arg(project.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .assert()
        .code(14)
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("ignored    specs/001-user-auth/spec.md"),
        "{stderr}"
    );
}

#[test]
fn propose_checks_task_and_config_before_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("cp")
        .arg("-R")
        .arg(std::path::PathBuf::from(fixture("graph")).join("."))
        .arg(dir.path())
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let state = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        speccoder()
            .args(args)
            .arg(dir.path())
            .env_remove("ANTHROPIC_API_KEY")
            .env("SPECCODER_STATE_DIR", state.path())
            .assert()
            .get_output()
            .clone()
    };
    // No speccoder.toml at all: configuration error.
    assert_eq!(run(&["propose", "--task", "T006"]).status.code(), Some(13));
    std::fs::write(
        dir.path().join("speccoder.toml"),
        "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n",
    )
    .unwrap();
    // Unknown task: not found, before any key is needed.
    assert_eq!(run(&["propose", "--task", "T999"]).status.code(), Some(15));
    // Valid task, no key: configuration error naming the variable, nothing sent.
    let out = run(&["propose", "--task", "T006"]);
    assert_eq!(out.status.code(), Some(13));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ANTHROPIC_API_KEY"));
    // Replay of a run that does not exist.
    let missing = "01JD0000000000000000000000";
    let out = run(&["--json", "replay", missing]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["result"], "not_found");
}

#[test]
fn outside_a_spec_kit_project_exit_10() {
    let dir = tempfile::tempdir().unwrap();
    let out = speccoder()
        .args(["--json", "inspect"])
        .arg(dir.path())
        .assert()
        .code(10)
        .get_output()
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "not_a_project");
    assert!(v.get("data").is_none());
}

#[test]
fn json_flag_is_global() {
    speccoder()
        .args(["inspect", &fixture("valid"), "--json"])
        .assert()
        .success();
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    speccoder().arg("frobnicate").assert().code(2);
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    speccoder().assert().code(2);
}
