//! The speccoder Spec Kit preset (spec 102, US5): installed by Spec Kit's
//! own `specify preset add --dev`, resolved for the tasks template, and a
//! plan written to it raises no plan check while the default template's
//! raises three.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use speccoder_core::CommandStatus;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Spec Kit 1.2.3 or newer, else the test is skipped with that reason.
fn speckit() -> bool {
    match speccoder_runtime::preset::speckit_version() {
        Some(v) if v >= (1, 2, 3) => true,
        found => {
            eprintln!("skipped: needs Spec Kit's `specify` 1.2.3 or newer (found {found:?})");
            false
        }
    }
}

#[test]
fn the_preset_installs_through_spec_kit_and_resolves_the_tasks_template() {
    if !speckit() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q"]);
    let init = Command::new("specify")
        .args(["init", "--here", "--integration", "claude", "--force"])
        .current_dir(r)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let state = tempfile::tempdir().unwrap();
    // Unconfirmed: the command and what it writes, nothing run.
    let asked = speccoder_runtime::preset::add(r, false, false, state.path());
    assert_eq!(
        asked.status(),
        CommandStatus::NeedsHuman,
        "{}",
        asked.message()
    );
    assert!(
        asked.message().contains("specify preset add --dev"),
        "{}",
        asked.message()
    );
    assert!(!r.join(".specify/presets/speccoder").exists());
    // Confirmed: Spec Kit installs it, and resolves the tasks template to it.
    let added = speccoder_runtime::preset::add(r, false, true, state.path());
    assert_eq!(added.status(), CommandStatus::Ok, "{}", added.message());
    let s = speccoder_runtime::preset::state(r);
    assert_eq!(
        s.installed.as_deref(),
        Some(speccoder_runtime::preset::version())
    );
    assert!(s.active_for_tasks_template, "{s:?}");
    // Again: an update through Spec Kit's own update.
    let again = speccoder_runtime::preset::add(r, false, true, state.path());
    assert_eq!(again.status(), CommandStatus::Ok, "{}", again.message());
    let Some(speccoder_core::CommandData::Preset(p)) = again.data() else {
        panic!()
    };
    assert_eq!(p.command[1..3], ["preset".to_owned(), "update".to_owned()]);
    // Nothing in the spec tree was touched.
    assert!(!r.join("specs").exists());
}

#[test]
fn a_project_that_is_not_spec_kit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    let state = tempfile::tempdir().unwrap();
    let res = speccoder_runtime::preset::add(dir.path(), false, true, state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("PRESET-NOT-SPECKIT"),
        "{}",
        res.message()
    );
}

/// SC-007: a plan written to the preset's template raises no plan check;
/// the POC's (Spec Kit's default template) raises three.
#[test]
fn a_plan_written_to_the_preset_raises_no_plan_check() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poc-greet");
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
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
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let lints = |r: &Path| {
        let opened = speccoder_runtime::format::open(r, Some("001-greet-function")).unwrap();
        speccoder_runtime::plan_lint::lints(opened.adapter, &opened.snapshot, r)
    };
    assert_eq!(lints(r).len(), 3);
    fs::copy(
        r.join("recorded/tasks-from-preset.md"),
        r.join("specs/001-greet-function/tasks.md"),
    )
    .unwrap();
    git(r, &["commit", "-qam", "the preset's plan"]);
    assert!(lints(r).is_empty(), "{:?}", lints(r));
}
