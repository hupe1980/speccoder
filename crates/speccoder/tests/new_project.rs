//! `speccoder new` (F-130) with stand-ins for the ecosystems' tools on
//! the PATH (`tests/fixtures/new-tools/`): the plan, its order, the one
//! commit as the person, the format's files written only by the format's
//! tool, resuming, refusals, and Undo. A temporary HOME and state
//! directory: nothing outside them is written.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// A machine: a home, a state directory, the person's git identity, the
/// fake tools first on the PATH.
struct Machine {
    home: tempfile::TempDir,
    state: tempfile::TempDir,
    work: tempfile::TempDir,
}

fn tools() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/new-tools")
}

impl Machine {
    fn new() -> Self {
        let m = Self {
            home: tempfile::tempdir().unwrap(),
            state: tempfile::tempdir().unwrap(),
            work: tempfile::tempdir().unwrap(),
        };
        fs::write(
            m.home.path().join(".gitconfig"),
            "[user]\n\tname = Sam Doe\n\temail = sam@example.com\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n",
        )
        .unwrap();
        m
    }

    fn path(&self) -> String {
        format!(
            "{}:{}",
            tools().display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_speccoder"));
        c.args(args)
            .current_dir(self.work.path())
            .env_clear()
            .env("PATH", self.path())
            .env("HOME", self.home.path())
            .env("GIT_CONFIG_GLOBAL", self.home.path().join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("SPECCODER_STATE_DIR", self.state.path())
            .env("TMPDIR", std::env::temp_dir());
        if let Some(h) = std::env::var_os("SPECCODER_SANDBOX_HELPER") {
            c.env("SPECCODER_SANDBOX_HELPER", h);
        }
        c
    }

    /// `speccoder --json <args>`: the envelope, schema-checked, and the exit.
    fn json(&self, args: &[&str]) -> (Value, i32) {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let out = self.cmd(&all).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let v: Value = serde_json::from_str(text.trim())
            .unwrap_or_else(|e| panic!("{e}: {text}\n{}", String::from_utf8_lossy(&out.stderr)));
        (support::checked(v), out.status.code().unwrap_or(-1))
    }

    fn target(&self, name: &str) -> PathBuf {
        fs::canonicalize(self.work.path()).unwrap().join(name)
    }

    fn log(&self, tool: &str) -> String {
        fs::read_to_string(self.state.path().join(format!("cache/new/{tool}/argv.log")))
            .unwrap_or_default()
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=Priya", "-c", "user.email=p@example.com"])
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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn ids(v: &Value, list: &str) -> Vec<String> {
    v["data"][list]
        .as_array()
        .or_else(|| v["data"]["plan"][list].as_array())
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect()
}

fn states(v: &Value) -> Vec<String> {
    v["data"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            format!(
                "{}={}",
                s["id"].as_str().unwrap(),
                s["state"].as_str().unwrap()
            )
        })
        .collect()
}

/// Every file under `dir`, relative, sorted, with its bytes.
fn tree(dir: &Path, under: &[&str]) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, p: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(p).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push((
                    path.strip_prefix(base).unwrap().display().to_string(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    for u in under {
        if dir.join(u).is_dir() {
            walk(dir, &dir.join(u), &mut out);
        }
    }
    out.sort();
    out
}

const IDEA: &str = "a CLI that turns Markdown tables into CSV";

#[test]
fn a_rust_spec_kit_project_is_made_by_its_own_tools_and_committed_once_as_the_person() {
    let m = Machine::new();
    let (plan, code) = m.json(&["new", "md2csv", "--idea", IDEA, "--dry-run"]);
    assert_eq!(code, 0, "{plan}");
    assert_eq!(plan["payload"], "new_project");
    assert_eq!(plan["data"]["dry_run"], true);
    assert_eq!(
        ids(&plan, "steps"),
        [
            "git_init",
            "scaffold.1",
            "format_init",
            "speccoder_init",
            "commit"
        ]
    );
    let steps = plan["data"]["plan"]["steps"].as_array().unwrap();
    assert_eq!(
        steps[1]["argv"],
        serde_json::json!(["cargo", "init", "--lib", "--name", "md2csv"])
    );
    let specify: Vec<&str> = steps[2]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        &specify[..10],
        [
            "specify",
            "init",
            "--here",
            "--force",
            "--non-interactive",
            "--integration",
            "claude",
            "--script",
            "sh",
            "--ignore-agent-tools"
        ]
    );
    // The speccoder preset is Spec Kit's own `--preset`, from spec 102's extraction.
    assert_eq!(specify[10], "--preset");
    assert!(specify[11].ends_with(&format!("presets/speccoder-{}", env!("CARGO_PKG_VERSION"))));
    assert!(steps.iter().all(|s| s["access"] == "local"));
    assert_eq!(plan["data"]["plan"]["stack"]["id"], "rust");
    assert!(!m.target("md2csv").exists(), "a dry run changes nothing");
    let digest = plan["data"]["plan"]["digest"].as_str().unwrap().to_owned();
    // The same plan again: the same digest.
    let (again, _) = m.json(&["new", "md2csv", "--idea", IDEA, "--dry-run"]);
    assert_eq!(again["data"]["plan"]["digest"], digest.as_str());

    let (made, code) = m.json(&["new", "md2csv", "--idea", IDEA, "--yes"]);
    assert_eq!(code, 0, "{made}");
    assert_eq!(made["data"]["plan"]["digest"], digest.as_str());
    assert_eq!(
        states(&made),
        [
            "git_init=done",
            "scaffold.1=done",
            "format_init=done",
            "speccoder_init=done",
            "commit=done"
        ]
    );
    let root = m.target("md2csv");
    // One commit, as the person, holding everything the steps produced.
    assert_eq!(
        git(&root, &["log", "--format=%s|%an <%ae>"]).trim(),
        "new project: md2csv|Sam Doe <sam@example.com>"
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    let committed = git(&root, &["ls-files"]);
    for f in [
        "Cargo.toml",
        "src/lib.rs",
        ".gitignore",
        "speccoder.toml",
        ".specify/memory/constitution.md",
        ".claude/skills/speckit-specify/SKILL.md",
        ".specify/presets/speccoder/preset.yml",
    ] {
        assert!(
            committed.lines().any(|l| l == f),
            "{f} not committed:\n{committed}"
        );
    }
    // Each tool was run as the plan said.
    assert_eq!(
        m.log("cargo").lines().next(),
        Some("cargo init --lib --name md2csv")
    );
    assert!(
        m.log("specify").starts_with("specify init --here --force"),
        "{}",
        m.log("specify")
    );
    // The format's files are exactly what its own init writes (SC-004).
    let alone = tempfile::tempdir().unwrap();
    let preset = specify[11];
    let st = Command::new(tools().join("specify"))
        .args(["init", "--here", "--force", "--preset", preset])
        .current_dir(alone.path())
        .env("PATH", m.path())
        .status()
        .unwrap();
    assert!(st.success());
    assert_eq!(
        tree(&root, &[".specify", ".claude"]),
        tree(alone.path(), &[".specify", ".claude"])
    );
    // init's checks ran on the untouched code; the Next step is the first feature.
    assert!(
        !made["data"]["init"]["ran"].as_array().unwrap().is_empty(),
        "{made}"
    );
    assert_eq!(made["data"]["first_feature"], IDEA);
    let next: Vec<&str> = made["next"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["command"].as_str().unwrap())
        .collect();
    assert!(
        next[0].starts_with(&format!(
            "speccoder spec specify \"{IDEA}\" --through tasks"
        )),
        "{next:?}"
    );
    // Tests have a place the runner collects (spec 102's layout).
    let layout =
        speccoder_runtime::layout::detect(&root, &[vec!["cargo".to_owned(), "test".to_owned()]]);
    assert!(
        layout.runners.iter().any(|r| r.story_dir.is_some()),
        "{layout:?}"
    );
    // The undo offer, and the creation record in the state directory only.
    assert_eq!(made["data"]["undo"]["available"], true);
    assert_eq!(made["data"]["undo"]["kind"], "trash");
    let record = speccoder_runtime::new_project::creation(m.state.path(), &root).unwrap();
    assert_eq!(record.idea, IDEA);
    // Nothing in the home directory but the git identity.
    let home: Vec<String> = fs::read_dir(m.home.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(home, [".gitconfig"]);
}

#[test]
fn the_event_stream_names_each_step_and_never_the_idea() {
    let m = Machine::new();
    let out = m
        .cmd(&[
            "--events",
            "new",
            "x1",
            "--idea",
            "a secret plan for widgets",
            "--stack",
            "go",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let types: Vec<&str> = lines.iter().map(|l| l["type"].as_str().unwrap()).collect();
    assert_eq!(
        types.iter().filter(|t| **t == "scaffold.started").count(),
        5,
        "{types:?}"
    );
    assert_eq!(
        types.iter().filter(|t| **t == "scaffold.finished").count(),
        5
    );
    assert!(types.contains(&"project.created"));
    for l in lines.iter().filter(|l| l["type"] != "result") {
        assert!(!l.to_string().contains("secret plan"), "{l}");
    }
    let created = lines
        .iter()
        .find(|l| l["type"] == "project.created")
        .unwrap();
    assert!(
        created["path_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(created["stack"], "go");
}

#[test]
fn a_python_openspec_plan_lists_its_network_step_and_keeps_openspec_settings_out_of_home() {
    let m = Machine::new();
    let (plan, _) = m.json(&[
        "--format",
        "openspec",
        "new",
        "py",
        "--idea",
        "a python script",
        "--dry-run",
    ]);
    let steps = plan["data"]["plan"]["steps"].as_array().unwrap();
    let add = steps.iter().find(|s| s["argv"][1] == "add").unwrap();
    assert_eq!(add["access"], "network");
    assert_eq!(add["hosts"][0], "pypi.org");
    assert_eq!(add["limit_s"], 600);
    let init = steps.iter().find(|s| s["id"] == "format_init").unwrap();
    assert_eq!(
        init["argv"],
        serde_json::json!([
            "openspec",
            "init",
            "--tools",
            "claude",
            "--no-animation",
            "."
        ])
    );
    let (made, code) = m.json(&[
        "--format",
        "openspec",
        "new",
        "py",
        "--idea",
        "a python script",
        "--yes",
    ]);
    assert_eq!(code, 0, "{made}");
    assert!(
        m.state
            .path()
            .join("cache/new/openspec/openspec/config.json")
            .is_file()
    );
    assert!(!m.home.path().join(".config").exists());
    assert!(m.target("py").join("openspec/config.yaml").is_file());
}

#[test]
fn a_failed_step_stops_the_run_and_running_again_resumes_at_it() {
    let m = Machine::new();
    let out = m
        .cmd(&[
            "--json",
            "new",
            "r",
            "--idea",
            "a rust library",
            "--stack",
            "rust",
            "--yes",
        ])
        .env("FAKE_FAIL", "specify")
        .output()
        .unwrap();
    let failed = support::checked(serde_json::from_slice(&out.stdout).unwrap());
    assert_eq!(failed["code"], "NEW-STEP-FAILED", "{failed}");
    assert_eq!(
        states(&failed),
        [
            "git_init=done",
            "scaffold.1=done",
            "format_init=failed",
            "speccoder_init=waiting",
            "commit=waiting"
        ]
    );
    let tail = failed["data"]["results"][2]["output_tail"].to_string();
    assert!(tail.contains("failing as asked"), "{tail}");
    assert!(
        failed["fix"].to_string().contains("specify init"),
        "{failed}"
    );
    // Again, as it was: the steps already done are skipped, not repeated.
    let (resumed, code) = m.json(&[
        "new",
        "r",
        "--idea",
        "a rust library",
        "--stack",
        "rust",
        "--yes",
    ]);
    assert_eq!(code, 0, "{resumed}");
    assert_eq!(
        states(&resumed),
        [
            "git_init=skipped",
            "scaffold.1=skipped",
            "format_init=done",
            "speccoder_init=done",
            "commit=done"
        ]
    );
    assert_eq!(m.log("cargo").matches("cargo init").count(), 1);
}

#[test]
fn a_tool_that_writes_outside_the_folder_fails_its_step_where_there_is_a_sandbox() {
    if speccoder_runtime::sandbox::backend().is_err() {
        eprintln!("no OS sandbox here: nothing to deny");
        return;
    }
    let m = Machine::new();
    let escape = m.home.path().join("escaped");
    let out = m
        .cmd(&[
            "--json", "new", "e", "--idea", "x", "--stack", "go", "--yes",
        ])
        .env("FAKE_ESCAPE", &escape)
        .output()
        .unwrap();
    let v = support::checked(serde_json::from_slice(&out.stdout).unwrap());
    assert_eq!(v["code"], "NEW-STEP-FAILED", "{v}");
    assert!(
        !escape.exists(),
        "the sandbox let a step write outside the folder"
    );
}

#[test]
fn without_a_terminal_and_without_yes_the_plan_is_shown_and_nothing_runs() {
    let m = Machine::new();
    let (v, code) = m.json(&["new", "q", "--idea", "x", "--stack", "none"]);
    assert_eq!(v["code"], "NEW-CONFIRM");
    assert_eq!(code, 23);
    assert!(v["fix"][0].as_str().unwrap().ends_with("--yes"), "{v}");
    assert!(!m.target("q").exists());
}

#[test]
fn the_plan_refuses_what_it_must_not_touch() {
    let m = Machine::new();
    // Not empty and not a repository.
    let loose = m.target("loose");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join("notes.txt"), "mine\n").unwrap();
    assert_eq!(
        m.json(&["new", "loose", "--dry-run"]).0["code"],
        "NEW-TARGET-NOT-EMPTY"
    );
    // Inside another repository.
    let outer = m.target("outer");
    fs::create_dir_all(&outer).unwrap();
    git(&outer, &["init", "-q"]);
    assert_eq!(
        m.json(&["new", "outer/inner", "--dry-run"]).0["code"],
        "NEW-NESTED-REPOSITORY"
    );
    // Uncommitted changes.
    fs::write(outer.join("wip.rs"), "\n").unwrap();
    assert_eq!(
        m.json(&["new", "outer", "--dry-run"]).0["code"],
        "NEW-DIRTY"
    );
    // Already a spec project.
    fs::create_dir_all(outer.join(".specify")).unwrap();
    fs::write(outer.join(".specify/init-options.json"), "{}\n").unwrap();
    git(&outer, &["add", "-A"]);
    git(&outer, &["commit", "-qm", "init"]);
    let (v, _) = m.json(&["new", "outer", "--dry-run"]);
    assert_eq!(v["code"], "NEW-ALREADY-PROJECT", "{v}");
    // No git identity.
    fs::write(m.home.path().join(".gitconfig"), "").unwrap();
    let (v, _) = m.json(&["new", "fresh", "--idea", "x", "--dry-run"]);
    assert_eq!(v["code"], "NEW-NO-IDENTITY", "{v}");
    // A stack whose tool is missing.
    let no_tools = Machine::new();
    let out = no_tools
        .cmd(&["--json", "new", "n", "--stack", "python", "--dry-run"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let v = support::checked(serde_json::from_slice(&out.stdout).unwrap());
    assert_eq!(v["code"], "NEW-TOOL-MISSING", "{v}");
    assert!(
        v["fix"][0].as_str().unwrap().contains("docs.astral.sh/uv"),
        "{v}"
    );
}

#[test]
fn undo_moves_only_an_untouched_project_to_the_trash() {
    let m = Machine::new();
    let (made, _) = m.json(&["new", "u1", "--idea", "x", "--stack", "go", "--yes"]);
    let root = m.target("u1");
    let commit = made["data"]["commit"].as_str().unwrap().to_owned();
    // Something of the person's: refused, naming it.
    fs::write(root.join("mine.go"), "package main\n").unwrap();
    let trash = tempfile::tempdir().unwrap();
    let mover = |p: &Path| fs::rename(p, trash.path().join("u1")).map_err(|e| e.to_string());
    let options = speccoder_runtime::RunOptions::default();
    let refused = speccoder_runtime::new_project::undo::undo_with(
        &root,
        &commit,
        speccoder_core::actions::Via::Terminal,
        m.state.path(),
        &options,
        &mover,
    );
    assert_eq!(
        refused.code().map(|c| c.name),
        Some("NEW-UNDO-REFUSED"),
        "{}",
        refused.message()
    );
    assert!(
        refused.message().contains("mine.go"),
        "{}",
        refused.message()
    );
    assert!(root.exists());
    fs::remove_file(root.join("mine.go")).unwrap();
    let undone = speccoder_runtime::new_project::undo::undo_with(
        &root,
        &commit,
        speccoder_core::actions::Via::Terminal,
        m.state.path(),
        &options,
        &mover,
    );
    assert_eq!(
        undone.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        undone.message()
    );
    assert!(!root.exists() && trash.path().join("u1/go.mod").exists());
}

#[test]
fn an_existing_repository_gets_the_format_and_init_and_undo_reverts_only_speccoders_commit() {
    let m = Machine::new();
    let repo = m.target("existing");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"existing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    fs::write(repo.join(".gitignore"), "/target\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "Priya"]);
    git(&repo, &["config", "user.email", "p@example.com"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "my code"]);
    let (plan, _) = m.json(&["new", "existing", "--dry-run"]);
    assert_eq!(plan["data"]["plan"]["mode"], "existing_repository");
    assert_eq!(
        ids(&plan, "steps"),
        ["format_init", "speccoder_init", "commit"]
    );
    assert_eq!(plan["data"]["plan"]["stack"]["detected"], true);
    assert_eq!(plan["data"]["plan"]["stack"]["id"], "rust");
    let (made, code) = m.json(&["new", "existing", "--yes"]);
    assert_eq!(code, 0, "{made}");
    assert_eq!(made["data"]["undo"]["kind"], "revert");
    let log = git(&repo, &["log", "--format=%s"]);
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        ["new project: existing", "my code"]
    );
    // speccoder's commit holds none of the person's files.
    let changed = git(&repo, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        !changed
            .lines()
            .any(|l| l == "src/lib.rs" || l == "Cargo.toml"),
        "{changed}"
    );
    let commit = made["data"]["commit"].as_str().unwrap().to_owned();
    let undone = speccoder_runtime::new_project::undo::undo_with(
        &repo,
        &commit,
        speccoder_core::actions::Via::Terminal,
        m.state.path(),
        &speccoder_runtime::RunOptions::default(),
        &|_| Err("no trash in this test".to_owned()),
    );
    assert_eq!(
        undone.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        undone.message()
    );
    let log = git(&repo, &["log", "--format=%s"]);
    assert_eq!(log.lines().count(), 3, "{log}");
    assert!(log.starts_with("Revert \"new project: existing\""), "{log}");
}

/// With the real tools (`SPECCODER_REAL_TOOLS=1`, a developer's machine
/// with cargo, specify and openspec): the format's files of a project
/// `speccoder new` made are byte for byte what the same init writes run by
/// hand in an empty repository (SC-004).
#[test]
fn with_the_real_tools_the_formats_files_are_the_formats_own() {
    if std::env::var_os("SPECCODER_REAL_TOOLS").is_none() {
        eprintln!("set SPECCODER_REAL_TOOLS=1 to run against the real cargo, specify and openspec");
        return;
    }
    // The toolchain the developer has: rustup and cargo live under the real home.
    let real = std::env::var("HOME").unwrap_or_default();
    let rustup = std::env::var("RUSTUP_HOME").unwrap_or_else(|_| format!("{real}/.rustup"));
    let cargo = std::env::var("CARGO_HOME").unwrap_or_else(|_| format!("{real}/.cargo"));
    for format in ["speckit", "openspec"] {
        let m = Machine::new();
        let run = |args: &[&str]| {
            let mut c = Command::new(env!("CARGO_BIN_EXE_speccoder"));
            c.args(["--json", "--format", format])
                .args(args)
                .current_dir(m.work.path())
                .env("HOME", m.home.path())
                .env("RUSTUP_HOME", &rustup)
                .env("CARGO_HOME", &cargo)
                .env("GIT_CONFIG_GLOBAL", m.home.path().join(".gitconfig"))
                .env("SPECCODER_STATE_DIR", m.state.path());
            c.output().unwrap()
        };
        let out = run(&[
            "new",
            "real",
            "--idea",
            "a rust library",
            "--stack",
            "rust",
            "--yes",
        ]);
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok", "{format}: {v}");
        // The first task starts from a library the project's checks pass on.
        let fmt = Command::new("cargo")
            .args(["fmt", "--all", "--check"])
            .current_dir(m.target("real"))
            .env("RUSTUP_HOME", &rustup)
            .env("CARGO_HOME", &cargo)
            .output()
            .unwrap();
        assert!(
            fmt.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&fmt.stdout)
        );
        let init = v["data"]["plan"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "format_init")
            .unwrap()["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let alone = tempfile::tempdir().unwrap();
        git(alone.path(), &["init", "-q"]);
        let config = tempfile::tempdir().unwrap();
        let st = Command::new(&init[0])
            .args(&init[1..])
            .current_dir(alone.path())
            .env("HOME", m.home.path())
            .env("XDG_CONFIG_HOME", config.path())
            .stdin(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success(), "{init:?}");
        let dirs: &[&str] = if format == "speckit" {
            &[".specify", ".claude"]
        } else {
            &["openspec", ".claude"]
        };
        // Spec Kit records when it installed things: those registries differ by their times.
        let strip = |t: Vec<(String, Vec<u8>)>| -> Vec<(String, Vec<u8>)> {
            t.into_iter()
                .filter(|(p, _)| {
                    !p.ends_with("init-options.json")
                        && !p.contains("integrations/")
                        && !p.ends_with(".registry")
                        && !p.ends_with("-registry.json")
                })
                .collect()
        };
        let (made, own) = (
            strip(tree(&m.target("real"), dirs)),
            strip(tree(alone.path(), dirs)),
        );
        let names = |t: &[(String, Vec<u8>)]| t.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
        assert_eq!(names(&made), names(&own), "{format}: the same files");
        let differ: Vec<&String> = made
            .iter()
            .zip(&own)
            .filter(|(a, b)| a.1 != b.1)
            .map(|(a, _)| &a.0)
            .collect();
        assert!(differ.is_empty(), "{format}: these differ: {differ:?}");
    }
}

#[test]
fn a_step_cannot_plant_a_git_driver_for_speccoders_own_commit() {
    let m = Machine::new();
    let escape = m.home.path().join("escaped-by-filter");
    let out = m
        .cmd(&[
            "--json", "new", "pwn", "--idea", "x", "--stack", "rust", "--yes",
        ])
        .env("FAKE_GITCONFIG", &escape)
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !escape.exists(),
        "speccoder's git ran a driver a step planted: {v}"
    );
    // Where the sandbox could not keep `.git` read-only, the change is caught.
    let config = fs::read_to_string(m.target("pwn").join(".git/config")).unwrap_or_default();
    if config.contains("pwn") {
        assert_eq!(v["code"], "NEW-STEP-FAILED", "{v}");
    }
}

#[test]
fn a_resume_never_takes_files_the_person_put_there() {
    let m = Machine::new();
    let failed = m
        .cmd(&[
            "--json", "new", "p3", "--idea", "x", "--stack", "rust", "--yes",
        ])
        .env("FAKE_FAIL", "cargo")
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(v["code"], "NEW-STEP-FAILED", "{v}");
    // The folder is replaced by one of the person's.
    let root = m.target("p3");
    fs::remove_dir_all(&root).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("thesis.txt"), "mine\n").unwrap();
    let (again, _) = m.json(&["new", "p3", "--idea", "x", "--stack", "rust", "--yes"]);
    assert_eq!(again["code"], "NEW-TARGET-NOT-EMPTY", "{again}");
    // A partly created folder that gained a file of the person's.
    let m = Machine::new();
    let _ = m
        .cmd(&[
            "--json", "new", "p4", "--idea", "x", "--stack", "rust", "--yes",
        ])
        .env("FAKE_FAIL", "specify")
        .output()
        .unwrap();
    fs::write(m.target("p4").join("notes.md"), "mine\n").unwrap();
    let (again, _) = m.json(&["new", "p4", "--idea", "x", "--stack", "rust", "--yes"]);
    assert_ne!(again["status"], "ok", "{again}");
    assert!(
        !git(
            &m.target("p4"),
            &["log", "--all", "--name-only", "--format="]
        )
        .contains("notes.md")
    );
}

#[test]
fn undo_in_an_existing_repository_ignores_what_was_ignored_before() {
    let m = Machine::new();
    let repo = m.target("withenv");
    fs::create_dir_all(repo.join(".idea")).unwrap();
    fs::write(repo.join(".gitignore"), ".env\n.idea/\n").unwrap();
    fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
    fs::write(repo.join(".idea/ws.xml"), "<x/>\n").unwrap();
    fs::write(repo.join("README.md"), "mine\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "Priya"]);
    git(&repo, &["config", "user.email", "p@example.com"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "mine"]);
    let (made, code) = m.json(&["new", "withenv", "--stack", "none", "--yes"]);
    assert_eq!(code, 0, "{made}");
    assert_eq!(made["data"]["undo"]["available"], true, "{made}");
    // And the success line never says a check passes and has no tests.
    assert!(
        !made["message"].as_str().unwrap().contains("pass"),
        "{made}"
    );
}

#[test]
fn the_formats_init_may_not_change_the_persons_tracked_files() {
    let m = Machine::new();
    let repo = m.target("mine");
    fs::create_dir_all(repo.join(".claude/skills/speckit-specify")).unwrap();
    fs::write(
        repo.join(".claude/skills/speckit-specify/SKILL.md"),
        "my own skill\n",
    )
    .unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "Priya"]);
    git(&repo, &["config", "user.email", "p@example.com"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "mine"]);
    let (v, _) = m.json(&["new", "mine", "--stack", "none", "--yes"]);
    assert_eq!(v["code"], "NEW-STEP-FAILED", "{v}");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains(".claude/skills/speckit-specify/SKILL.md"),
        "{v}"
    );
    assert_eq!(git(&repo, &["log", "--format=%s"]).trim(), "mine");
}

#[test]
fn a_folder_name_that_reads_as_an_option_is_refused() {
    let m = Machine::new();
    let (v, _) = m.json(&["new", "./-rf", "--stack", "rust", "--dry-run"]);
    assert_eq!(v["code"], "USAGE", "{v}");
}
