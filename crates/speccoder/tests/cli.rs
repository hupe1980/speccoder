//! The command-line contract: commands, `--json` output and exit codes.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use assert_cmd::Command;

fn speccoder() -> Command {
    let mut cmd = Command::cargo_bin("speccoder").unwrap();
    cmd.env_remove("SPECIFY_FEATURE_DIRECTORY");
    cmd.env_remove("SPECCODER_FORMAT");
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
fn help_groups_the_daily_commands_and_points_to_the_rest() {
    let out = speccoder()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    for word in [
        "Start",
        "Work",
        "Review",
        "  task",
        "--json",
        "help advanced",
    ] {
        assert!(stdout.contains(word), "{word}: {stdout}");
    }
    assert!(
        !stdout.contains("  inspect "),
        "inspect is advanced: {stdout}"
    );
    let advanced = speccoder()
        .args(["help", "advanced"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        String::from_utf8(advanced.stdout)
            .unwrap()
            .contains("inspect")
    );
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
        .args(["inspect", "-C", &fixture("valid")])
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
        .args(["--json", "inspect", "-C", &fixture("valid")])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let v: serde_json::Value = support::result(stdout.as_bytes());
    assert_eq!(v["schema_version"], 2);
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
        .args(["--json", "inspect", "-C", &fixture("malformed")])
        .assert()
        .code(11)
        .get_output()
        .clone();
    let v: serde_json::Value = support::result(&out.stdout);
    assert_eq!(v["status"], "diagnostics_error");
}

#[test]
fn graph_errors_exit_11() {
    let out = speccoder()
        .args(["inspect", "-C", &fixture("graph-errors")])
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
        .args(["--json", "inspect", "-C", &fixture("graph")])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: serde_json::Value = support::result(&out.stdout);
    let t10 = &v["data"]["feature"]["schedule"]["tasks"][9];
    assert_eq!(t10["id"], "T010");
    assert_eq!(t10["state"], "blocked");
    assert_eq!(t10["blocked_by"], serde_json::json!(["T008", "T009"]));
    assert_eq!(v["data"]["feature"]["schedule"]["totals"]["ready"], 2);
}

#[test]
fn several_features_without_selection_exit_12_and_list_them() {
    let out = speccoder()
        .args(["inspect", "-C", &fixture("multi")])
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
        .args(["inspect", "-C", &fixture("multi"), "--feature", "2"])
        .assert()
        .success();
}

#[test]
fn specify_feature_directory_selects_the_feature() {
    let out = speccoder()
        .args(["inspect", "-C", &fixture("real/speckit-1.2.2")])
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
        .arg("-C")
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
        .arg("-C")
        .arg(dir.path())
        .assert()
        .code(20)
        .get_output()
        .clone();
    let v: serde_json::Value = support::result(&out.stdout);
    assert_eq!(v["status"], "verification_failed");
    assert_eq!(v["data"]["checks"][1]["status"], "failed");
}

#[test]
fn verify_without_config_exits_13() {
    let dir = tempfile::tempdir().unwrap();
    speccoder()
        .arg("verify")
        .arg("-C")
        .arg(dir.path())
        .assert()
        .code(13);
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
            .arg("-C")
            .arg(project.path())
            .env("SPECCODER_STATE_DIR", state.path())
            .assert()
            .success()
            .get_output()
            .clone()
    };
    let first: serde_json::Value = support::result(&run().stdout);
    assert_eq!(first["data"]["branch"], "speccoder/001-user-auth");
    assert_eq!(first["data"]["created"], true);
    let path = first["data"]["path"].as_str().unwrap().to_owned();
    assert!(
        std::path::Path::new(&path)
            .join("specs/001-user-auth/tasks.md")
            .exists()
    );
    let second: serde_json::Value = support::result(&run().stdout);
    assert_eq!(second["data"]["created"], false);
    assert_eq!(second["data"]["path"], path.as_str());
}

#[test]
fn worktree_refuses_ignored_specs_with_exit_14() {
    let project = committed_project(true);
    let state = tempfile::tempdir().unwrap();
    let out = speccoder()
        .arg("worktree")
        .arg("-C")
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
            .arg("-C")
            .arg(dir.path())
            .env_remove("ANTHROPIC_API_KEY")
            .env("SPECCODER_STATE_DIR", state.path())
            .assert()
            .get_output()
            .clone()
    };
    // No speccoder.toml at all: configuration error.
    assert_eq!(run(&["propose", "T006"]).status.code(), Some(13));
    std::fs::write(
        dir.path().join("speccoder.toml"),
        "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n",
    )
    .unwrap();
    // Unknown task: not found, before any key is needed.
    assert_eq!(run(&["propose", "T999"]).status.code(), Some(15));
    // Valid task, no key: configuration error naming the variable, nothing sent.
    let out = run(&["propose", "T006"]);
    assert_eq!(out.status.code(), Some(13));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ANTHROPIC_API_KEY"));
    // Replay of a run that does not exist.
    let missing = "01JD0000000000000000000000";
    let out = run(&["--json", "replay", missing]);
    let v: serde_json::Value = support::result(&out.stdout);
    assert_eq!(v["data"]["result"], "not_found");
}

#[test]
fn outside_a_spec_kit_project_exit_10() {
    let dir = tempfile::tempdir().unwrap();
    let out = speccoder()
        .args(["--json", "inspect"])
        .arg("-C")
        .arg(dir.path())
        .assert()
        .code(10)
        .get_output()
        .clone();
    let v: serde_json::Value = support::result(&out.stdout);
    assert_eq!(v["status"], "not_a_project");
    assert!(v.get("data").is_none());
}

#[test]
fn json_flag_is_global() {
    speccoder()
        .args(["inspect", "-C", &fixture("valid"), "--json"])
        .assert()
        .success();
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    speccoder().arg("frobnicate").assert().code(2);
}

#[test]
fn without_a_command_and_a_terminal_it_is_the_report() {
    let out = speccoder()
        .args(["-C", &fixture("valid"), "--json"])
        .output()
        .unwrap();
    let v = support::result(&out.stdout);
    assert_eq!(v["command"], "report", "{v}");
}

#[test]
fn a_former_command_names_its_new_form() {
    let out = speccoder()
        .args(["--json", "approve", "--task", "T004"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v = support::result(&out.stdout);
    assert_eq!(v["fix"][0], "speccoder task approve <TASK>", "{v}");
    let out = speccoder().args(["run", "T004"]).output().unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("--task T004"), "{err}");
}

/// `--json` keeps stdout one object and writes no terminal escapes, even
/// when the person's settings ask for notifications (F-053).
#[test]
fn json_output_carries_no_bell_or_escape() {
    let project = committed_project(false);
    let state = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(config.path().join("speccoder")).unwrap();
    std::fs::write(
        config.path().join("speccoder/config.toml"),
        "[ui.notify]\nmethod = \"bel\"\nwhen = \"always\"\n",
    )
    .unwrap();
    for args in [
        &["--json", "status"][..],
        &["--json", "report"][..],
        &["--json", "run"][..],
    ] {
        let out = speccoder()
            .args(args)
            .arg("-C")
            .arg(project.path())
            .env("SPECCODER_STATE_DIR", state.path())
            .env("XDG_CONFIG_HOME", config.path())
            .env_remove("OPENAI_API_KEY")
            .env_remove("ANTHROPIC_API_KEY")
            .output()
            .unwrap();
        for stream in [&out.stdout, &out.stderr] {
            assert!(!stream.contains(&0x07), "{args:?}: a BEL byte");
            assert!(
                !stream.windows(2).any(|w| w == b"\x1b]"),
                "{args:?}: an OSC sequence"
            );
        }
    }
}

/// A person's UI settings are refused in the committed project config,
/// naming where they belong.
#[test]
fn a_ui_table_in_speccoder_toml_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("speccoder.toml"),
        "[checks.ok]\nargv = [\"true\"]\n\n[ui.notify]\nmethod = \"bel\"\n",
    )
    .unwrap();
    let out = speccoder()
        .arg("verify")
        .arg("-C")
        .arg(dir.path())
        .env("XDG_CONFIG_HOME", config.path())
        .assert()
        .code(13)
        .get_output()
        .clone();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("[ui] is a person's setting")
            && err.contains(
                &config
                    .path()
                    .join("speccoder/config.toml")
                    .display()
                    .to_string()
            ),
        "{err}"
    );
}

#[test]
fn a_format_is_chosen_by_flag_or_environment() {
    for (flag, env) in [(Some("kiro"), None), (None, Some("kiro"))] {
        let mut cmd = speccoder();
        if let Some(f) = flag {
            cmd.args(["--format", f]);
        }
        if let Some(e) = env {
            cmd.env("SPECCODER_FORMAT", e);
        }
        let out = cmd
            .args(["--json", "inspect", "-C", &fixture("valid")])
            .assert()
            .code(10)
            .get_output()
            .clone();
        let v: serde_json::Value = support::result(&out.stdout);
        assert_eq!(v["status"], "not_a_project", "{v}");
        assert!(
            v["message"]
                .as_str()
                .unwrap()
                .contains("not yet supported by this version of speccoder"),
            "{v}"
        );
    }
    speccoder()
        .args(["--format", "speckit", "inspect", "-C", &fixture("valid")])
        .assert()
        .success();
    let out = speccoder()
        .args([
            "--json",
            "--format",
            "openspec",
            "inspect",
            "-C",
            &fixture("valid"),
        ])
        .assert()
        .code(10)
        .get_output()
        .clone();
    let v: serde_json::Value = support::result(&out.stdout);
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .starts_with("no OpenSpec project (openspec/)"),
        "{v}"
    );
}

/// Colour follows the flag, then the environment; machine output never has
/// it (F-055, SC-002).
#[test]
fn escape_codes_appear_only_when_colour_is_wanted() {
    let esc = |args: &[&str], env: &[(&str, &str)]| {
        let mut cmd = speccoder();
        cmd.env_remove("NO_COLOR")
            .env_remove("FORCE_COLOR")
            .env_remove("CLICOLOR_FORCE");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd
            .args(args)
            .arg("-C")
            .arg(fixture("valid"))
            .output()
            .unwrap();
        out.stdout.contains(&0x1b) || out.stderr.contains(&0x1b)
    };
    assert!(!esc(&["inspect"], &[]), "piped: no colour");
    assert!(
        esc(&["--color", "always", "inspect"], &[]),
        "forced by the flag"
    );
    assert!(
        esc(&["inspect"], &[("FORCE_COLOR", "1")]),
        "forced by the environment"
    );
    assert!(
        !esc(&["inspect"], &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]),
        "NO_COLOR wins"
    );
    assert!(
        esc(&["--color", "always", "inspect"], &[("NO_COLOR", "1")]),
        "the flag wins"
    );
    assert!(!esc(
        &["inspect"],
        &[("FORCE_COLOR", "1"), ("TERM", "dumb"), ("NO_COLOR", "x")]
    ));
    assert!(
        !esc(&["--color", "always", "--json", "inspect"], &[]),
        "never in --json"
    );
    assert!(
        !esc(&["--color", "always", "--events", "inspect"], &[]),
        "never in --events"
    );
}

#[test]
fn ascii_and_screen_reader_modes_change_symbols_not_facts() {
    let run = |args: &[&str]| {
        let out = speccoder()
            .args(args)
            .arg("-C")
            .arg(fixture("valid"))
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let ascii = run(&["--ascii", "inspect"]);
    assert!(ascii.is_ascii(), "{ascii}");
    let reader = run(&["--screen-reader", "inspect"]);
    assert!(
        reader
            .lines()
            .any(|l| l.trim_start().starts_with("task: T001")),
        "{reader}"
    );
    assert!(reader.is_ascii(), "{reader}");
}

/// A committed Spec Kit project with a `make test` that passes.
fn make_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("cp")
        .arg("-R")
        .arg(std::path::PathBuf::from(fixture("graph")).join("."))
        .arg(dir.path())
        .status()
        .unwrap()
        .success();
    assert!(ok);
    std::fs::write(dir.path().join("Makefile"), "test:\n\ttrue\n").unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@e",
                    "-c",
                    "commit.gpgsign=false"
                ])
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    dir
}

#[test]
fn init_runs_the_checks_and_names_the_commit_without_making_it() {
    let dir = make_project();
    let out = speccoder()
        .args(["--json", "init", "-C"])
        .arg(dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    let v = support::result(&out.stdout);
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["data"]["ran"][0]["name"], "test");
    assert_eq!(v["data"]["ran"][0]["passed"], true, "{v}");
    assert_eq!(v["data"]["key"]["set"], false);
    assert!(
        v["data"]["committed"].is_null(),
        "no terminal: nothing committed"
    );
    let next: Vec<&str> = v["next"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["command"].as_str().unwrap())
        .collect();
    assert_eq!(
        next[0], "git add speccoder.toml && git commit -m \"speccoder: config\" -- speccoder.toml",
        "{next:?}"
    );
    assert!(
        next.iter()
            .any(|c| c.starts_with("export ANTHROPIC_API_KEY")),
        "{next:?}"
    );
    let log = std::process::Command::new("git")
        .args(["log", "--format=%s"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "init");
}

#[test]
fn doctor_names_each_problem_with_its_fix() {
    let dir = make_project();
    // Healthy enough to have a project, but no config and no key.
    let out = speccoder()
        .args(["--json", "doctor", "-C"])
        .arg(dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .unwrap();
    let v = support::result(&out.stdout);
    assert_eq!(out.status.code(), Some(13), "{v}");
    let checks = v["data"]["checks"].as_array().unwrap();
    let level = |name: &str| {
        checks
            .iter()
            .find(|c| c["check"] == name)
            .map(|c| c["level"].as_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    assert_eq!(level("project"), "pass");
    assert_eq!(level("config"), "fail");
    assert_eq!(v["code"], "CONFIG-MISSING");
    // The bug report redacts the home directory.
    let report = speccoder()
        .args(["doctor", "--bug-report", "-C"])
        .arg(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.starts_with("### speccoder doctor"), "{text}");
    if let Ok(home) = std::env::var("HOME") {
        assert!(!text.contains(&home), "{text}");
    }
}

#[test]
fn completion_and_help_advanced_work_without_a_project() {
    let out = speccoder().args(["completion", "zsh"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("#compdef speccoder"));
    let out = speccoder().args(["help", "task"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("approve"));
}

/// At a terminal, `init` asks before committing, and commits only the
/// configuration. Driven through `script`'s pseudo-terminal.
#[test]
fn init_at_a_terminal_commits_only_the_config_on_yes() {
    let dir = make_project();
    let bin = assert_cmd::cargo::cargo_bin("speccoder");
    let line = format!("{} init", bin.display());
    let mut cmd = std::process::Command::new("script");
    if cfg!(target_os = "macos") {
        cmd.args(["-q", "/dev/null"]).arg(&bin).arg("init");
    } else {
        cmd.args(["-qec", &line, "/dev/null"]);
    }
    let child = cmd
        .current_dir(dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        eprintln!("skipped: no script(1)");
        return;
    };
    use std::io::Write as _;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Commit speccoder.toml?"), "{text}");
    let log = std::process::Command::new("git")
        .args(["log", "--format=%s"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).lines().next(),
        Some("speccoder: config"),
        "{text}"
    );
    let files = std::process::Command::new("git")
        .args(["show", "--name-only", "--format=", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&files.stdout).trim(),
        "speccoder.toml"
    );
}

#[test]
fn feature_lists_switches_and_validates_against_its_schema() {
    let dir = make_project();
    std::fs::create_dir_all(dir.path().join("specs/002-login")).unwrap();
    std::fs::write(dir.path().join("specs/002-login/spec.md"), "# Spec\n").unwrap();
    let json = |args: &[&str]| {
        let out = speccoder()
            .args(args)
            .arg("-C")
            .arg(dir.path())
            .arg("--json")
            .output()
            .unwrap();
        support::result(&out.stdout)
    };
    let v = json(&["feature"]);
    assert_eq!(v["payload"], "features");
    assert_eq!(v["data"]["current"], serde_json::Value::Null);
    assert_eq!(v["data"]["features"][1]["name"], "002-login");
    let v = json(&["feature", "login"]);
    assert_eq!(v["data"]["current"], "002-login");
    assert_eq!(v["data"]["action"], "switched");
    // A miss is an error with a fix, still a valid envelope.
    let v = json(&["feature", "nothing-like-it"]);
    assert_eq!(v["code"], "NOT-FOUND");
    // Human: the current one marked, the report says why.
    let out = speccoder()
        .args(["feature", "-C"])
        .arg(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("▸  2  002-login"), "{text}");
    assert!(
        text.contains("current feature: 002-login (recorded in .specify/feature.json)"),
        "{text}"
    );
}

/// The first real run lost its nextest profile: `init` wrote
/// `.config/nextest.toml` but committed only `speccoder.toml`, so the
/// worktree's `test` check could not run. Everything init writes is committed.
#[test]
fn init_at_a_terminal_commits_the_nextest_profile_with_the_config() {
    if std::process::Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .map_or(true, |o| !o.status.success())
    {
        eprintln!("skipped: no cargo-nextest");
        return;
    }
    let dir = make_project();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::remove_file(dir.path().join("Makefile")).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap()
    };
    git(&["add", "-A"]);
    git(&["commit", "-qm", "crate"]);
    let bin = assert_cmd::cargo::cargo_bin("speccoder");
    let line = format!("{} init --no-verify", bin.display());
    let mut cmd = std::process::Command::new("script");
    if cfg!(target_os = "macos") {
        cmd.args(["-q", "/dev/null"])
            .arg(&bin)
            .args(["init", "--no-verify"]);
    } else {
        cmd.args(["-qec", &line, "/dev/null"]);
    }
    let Ok(mut child) = cmd
        .current_dir(dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    else {
        eprintln!("skipped: no script(1)");
        return;
    };
    use std::io::Write as _;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Commit speccoder.toml and .config/nextest.toml?"),
        "{text}"
    );
    let files = git(&["show", "--name-only", "--format=", "HEAD"]);
    let mut files: Vec<String> = String::from_utf8_lossy(&files.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    files.sort();
    assert_eq!(files, [".config/nextest.toml", "speccoder.toml"], "{text}");
}

/// `speccoder config` (F-079): a key set from words keeps the file's
/// comments, an invalid value never reaches the file, unset brings the
/// default back, and `commit` commits `speccoder.toml` alone.
#[test]
fn config_sets_checks_and_commits_the_configuration_alone() {
    let dir = committed_project(false);
    let d = dir.path();
    std::fs::write(
        d.join("speccoder.toml"),
        "# Our checks.\n[checks.test]\nargv = [\"true\"] # fast\n",
    )
    .unwrap();
    std::fs::write(d.join("other.txt"), "staged, not ours\n").unwrap();
    git(d, &["add", "speccoder.toml", "other.txt"]);
    git(d, &["commit", "-q", "-m", "config"]);
    std::fs::write(d.join("other.txt"), "changed and staged\n").unwrap();
    git(d, &["add", "other.txt"]);
    let run = |args: &[&str]| {
        let out = speccoder()
            .args(["--json", "config"])
            .args(args)
            .arg("-C")
            .arg(d)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        (
            out.status.code(),
            support::checked(support::result(&out.stdout)),
        )
    };

    let (code, v) = run(&["set", "budget.max_turns", "40"]);
    assert_eq!(code, Some(0), "{v}");
    let text = std::fs::read_to_string(d.join("speccoder.toml")).unwrap();
    assert!(
        text.contains("# Our checks.") && text.contains("argv = [\"true\"] # fast"),
        "{text}"
    );
    assert!(text.contains("max_turns = 40"), "{text}");
    assert_eq!(v["data"]["committed"], false, "{v}");

    // Refused before the file is touched.
    let (code, v) = run(&["set", "budget.max_turns", "lots"]);
    assert_eq!(code, Some(13), "{v}");
    assert_eq!(v["code"], "CONFIG-INVALID");
    assert_eq!(
        std::fs::read_to_string(d.join("speccoder.toml")).unwrap(),
        text
    );

    let (_, v) = run(&["get", "budget.max_turns"]);
    assert_eq!(v["message"], "budget.max_turns = 40", "{v}");
    let (_, v) = run(&["get", "budget.max_attempts"]);
    assert_eq!(v["message"], "budget.max_attempts = 2 (default)", "{v}");

    let (code, v) = run(&["commit"]);
    assert_eq!(code, Some(0), "{v}");
    let shown = std::process::Command::new("git")
        .args(["show", "--name-only", "--format=%s", "HEAD"])
        .current_dir(d)
        .output()
        .unwrap();
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(
        shown.starts_with("speccoder: configuration (budget)"),
        "{shown}"
    );
    assert!(
        shown.contains("speccoder.toml") && !shown.contains("other.txt"),
        "{shown}"
    );

    let (_, v) = run(&["unset", "budget.max_turns"]);
    assert!(
        !std::fs::read_to_string(d.join("speccoder.toml"))
            .unwrap()
            .contains("[budget]"),
        "{v}"
    );
}

/// `speccoder issue` (F-083) through a scripted `gh`: the open issues as the
/// documented payload; without a login, the fix.
#[test]
fn issue_lists_through_gh_and_says_how_to_log_in() {
    let dir = committed_project(false);
    git(
        dir.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    let bin = tempfile::tempdir().unwrap();
    let gh = bin.path().join("gh");
    std::fs::write(
        &gh,
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"issue list\") echo '[{\"number\":42,\"title\":\"Greet\",\"url\":\"https://github.com/o/r/issues/42\",\"labels\":[{\"name\":\"feature\"}],\"assignees\":[],\"updatedAt\":\"2026-10-20T00:00:00Z\"}]';;\n  \"repo view\") echo '{\"nameWithOwner\":\"o/r\"}';;\n  *) exit 1;;\nesac\n",
    )
    .unwrap();
    assert!(
        std::process::Command::new("chmod")
            .arg("+x")
            .arg(&gh)
            .status()
            .unwrap()
            .success()
    );
    let out = speccoder()
        .args(["--json", "issue", "-C"])
        .arg(dir.path())
        .env("SPECCODER_GH", &gh)
        .output()
        .unwrap();
    let v = support::checked(support::result(&out.stdout));
    assert_eq!(v["payload"], "issues", "{v}");
    assert_eq!(v["data"]["repo"], "o/r");
    assert_eq!(v["data"]["issues"][0]["number"], 42);
    assert_eq!(v["data"]["issues"][0]["labels"][0], "feature");

    // Not logged in: the reason and the fix.
    std::fs::write(&gh, "#!/bin/sh\necho 'To get started with GitHub CLI, please run:  gh auth login' >&2\nexit 4\n").unwrap();
    let out = speccoder()
        .args(["--json", "issue", "start", "42", "-C"])
        .arg(dir.path())
        .env("SPECCODER_GH", &gh)
        .output()
        .unwrap();
    let v = support::checked(support::result(&out.stdout));
    assert_eq!(v["status"], "error", "{v}");
    assert_eq!(v["fix"][0], "gh auth login", "{v}");
}
