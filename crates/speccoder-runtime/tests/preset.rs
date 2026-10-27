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
    // Again, the same version active: nothing runs.
    let again = speccoder_runtime::preset::add(r, false, true, state.path());
    assert_eq!(again.status(), CommandStatus::Ok, "{}", again.message());
    assert!(
        again.message().contains("already active"),
        "{}",
        again.message()
    );
    let Some(speccoder_core::CommandData::Preset(p)) = again.data() else {
        panic!()
    };
    assert!(p.command.is_empty());
    // An older version installed: Spec Kit's own update replaces it.
    let manifest = r.join(".specify/presets/speccoder/preset.yml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        text.replace(
            &format!("version: \"{}\"", speccoder_runtime::preset::version()),
            "version: \"0.0.1\"",
        ),
    )
    .unwrap();
    let update = speccoder_runtime::preset::add(r, false, false, state.path());
    let Some(speccoder_core::CommandData::Preset(p)) = update.data() else {
        panic!("{}", update.message())
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

/// A fake `specify` in `<dir>/bin`: its shebang interpreter imports a fake
/// `yaml` when `yaml`.
#[cfg(unix)]
fn fake_specify(dir: &Path, yaml: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::create_dir_all(dir.join("venv")).unwrap();
    let python = dir.join("venv/python");
    fs::write(
        &python,
        "#!/bin/sh\ncase \"$2\" in *'import sys, yaml'*) [ -f \"$(dirname \"$0\")/yaml.py\" ] && exit 0;; esac\nexit 1\n",
    )
    .unwrap();
    if yaml {
        fs::write(dir.join("venv/yaml.py"), "").unwrap();
    }
    let specify = dir.join("bin/specify");
    fs::write(
        &specify,
        format!("#!{}\nprint('specify')\n", python.display()),
    )
    .unwrap();
    for f in [&python, &specify] {
        fs::set_permissions(f, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir.join("bin")
}

/// Spec Kit's scripts resolve preset templates with a Python that imports
/// PyYAML: with a preset installed and none found, doctor warns with the
/// fix; `specify`'s own interpreter importing it is enough.
#[cfg(unix)]
#[test]
fn doctor_warns_when_a_preset_is_installed_and_no_python_has_pyyaml() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q"]);
    fs::create_dir_all(r.join(".specify/presets/speccoder")).unwrap();
    fs::write(r.join(".specify/init-options.json"), "{}\n").unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let tools = tempfile::tempdir().unwrap();
    let tools = fs::canonicalize(tools.path()).unwrap();
    let python_row = |path: &Path| {
        let path = path.display().to_string();
        speccoder_runtime::doctor::checks(r, None, &|k| (k == "PATH").then(|| path.clone()))
            .into_iter()
            .find(|c| c.check == "spec tool python")
    };
    let without = fake_specify(&tools.join("without"), false);
    let row = python_row(&without).unwrap_or_else(|| panic!("no warning"));
    assert_eq!(row.level, speccoder_core::DoctorLevel::Warn, "{row:?}");
    assert!(
        row.detail.contains("speccoder") && row.detail.contains("PyYAML"),
        "{row:?}"
    );
    assert!(
        row.fix.iter().any(|f| f.contains("pip install pyyaml")),
        "{row:?}"
    );
    assert!(
        row.fix
            .iter()
            .any(|f| f.contains("SPECKIT_PYTHON_EXECUTABLE")),
        "{row:?}"
    );
    let with = fake_specify(&tools.join("with"), true);
    assert_eq!(python_row(&with), None);
}
