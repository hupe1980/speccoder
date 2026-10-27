//! A run's worktree gets its dependency folders: installed with the
//! toolchain's locked offline install, or linked from the checkout
//! (`[worktree] link`). A shell script stands in for the agent (an
//! `[executor]`), so no model call is made. Tests that install need the
//! real `npm` or `uv` and are skipped, with a note, without them.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::prepare::{self, Mode, Prepare};
use speccoder_runtime::progress::{Event, Progress};
use speccoder_runtime::{RunOptions, run_with_options, sandbox};

const MODEL: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n";
/// The check needs the dependency folder: it cannot pass in a bare worktree.
const CHECK: &str = "[checks.deps]\nargv = [\"sh\", \"-c\", \"test \\\"$(cat node_modules/tiny/index.js)\\\" = 'module.exports = 42;'\"]\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The sandbox this machine has, or a note that it has none.
fn sandboxed() -> bool {
    let ok = sandbox::backend().is_ok();
    if !ok {
        eprintln!("skipped: no OS sandbox here");
    }
    ok
}

#[derive(Debug, Default)]
struct Recorder(std::sync::Mutex<Vec<Event>>);

impl Progress for Recorder {
    fn event(&self, e: &Event) {
        self.0.lock().unwrap().push(e.clone());
    }
}

impl Recorder {
    /// The prepare stage's events: `(state, detail)`.
    fn prepare(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Stage {
                    stage,
                    state,
                    detail,
                    ..
                } if stage.name() == "prepare" => Some((
                    serde_json::to_value(state)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    detail.clone().unwrap_or_default(),
                )),
                _ => None,
            })
            .collect()
    }
}

/// A checkout: the `graph` fixture with a Node project whose one
/// dependency (`tiny`) is ignored in `node_modules/`, and an agent script
/// (kept outside the repository).
struct Fixture {
    dir: tempfile::TempDir,
    _bin: tempfile::TempDir,
    state: tempfile::TempDir,
}

impl Fixture {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The checkout's `node_modules`, as file paths and contents.
    fn modules(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let root = self.path().join("node_modules");
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push((
                        p.strip_prefix(&root).unwrap().display().to_string(),
                        fs::read_to_string(&p).unwrap_or_default(),
                    ));
                }
            }
        }
        out.sort();
        out
    }

    fn run(&self, task: &str) -> (CommandResult, Arc<Recorder>) {
        let d: Arc<dyn ModelProvider> = FakeProvider::new();
        let rec = Arc::new(Recorder::default());
        let options = RunOptions {
            progress: Some(rec.clone()),
            ..RunOptions::default()
        };
        let res = run_with_options(
            self.path(),
            None,
            task,
            self.state.path(),
            d,
            None,
            &options,
        );
        (res, rec)
    }
}

/// How the checkout's `node_modules` comes to be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    /// `npm install --offline` of a packed local tarball: a real lockfile
    /// `npm ci --offline` installs from, no registry needed.
    Npm,
    /// Written by hand: enough to link, nothing to install from.
    Handmade,
}

fn fixture(extra: &str, script: &str, node: Node) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let agent = bin.path().join("agent.sh");
    fs::write(&agent, format!("#!/bin/sh\nset -e\n{script}\n")).unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(source.join("."))
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
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nNode.\n").unwrap();
    fs::write(r.join(".gitignore"), "node_modules/\n").unwrap();
    match node {
        Node::Npm => {
            let tiny = r.join("vendor/tiny");
            fs::create_dir_all(&tiny).unwrap();
            fs::write(
                tiny.join("package.json"),
                "{\"name\": \"tiny\", \"version\": \"1.0.0\", \"main\": \"index.js\"}\n",
            )
            .unwrap();
            fs::write(tiny.join("index.js"), "module.exports = 42;").unwrap();
            let npm = |dir: &Path, args: &[&str]| {
                let out = Command::new("npm")
                    .args(args)
                    .args(["--no-audit", "--no-fund", "--offline"])
                    .current_dir(dir)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "npm {args:?}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            };
            npm(&tiny, &["pack", "--pack-destination", ".."]);
            fs::remove_dir_all(&tiny).unwrap();
            fs::write(
                r.join("package.json"),
                "{\"name\": \"fixture\", \"version\": \"1.0.0\", \"private\": true, \"dependencies\": {\"tiny\": \"file:vendor/tiny-1.0.0.tgz\"}}\n",
            )
            .unwrap();
            npm(r, &["install"]);
        }
        Node::Handmade => {
            fs::write(
                r.join("package.json"),
                "{\"name\": \"fixture\", \"version\": \"1.0.0\", \"private\": true, \"dependencies\": {\"tiny\": \"1.0.0\"}}\n",
            )
            .unwrap();
            fs::write(r.join("package-lock.json"), "{\"lockfileVersion\": 3}\n").unwrap();
            fs::create_dir_all(r.join("node_modules/tiny")).unwrap();
            fs::write(r.join("node_modules/tiny/index.js"), "module.exports = 42;").unwrap();
            fs::write(r.join("node_modules/.package-lock.json"), "{}\n").unwrap();
        }
    }
    fs::write(
        r.join("speccoder.toml"),
        format!(
            "{MODEL}{CHECK}[budget]\nmax_attempts = 1\n[executor]\nkind = \"command\"\nargv = [\"sh\", \"{}\"]\n{extra}",
            agent.display()
        ),
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    Fixture {
        dir,
        _bin: bin,
        state: tempfile::tempdir().unwrap(),
    }
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

const AGENT: &str = "echo done > done.txt\necho 'Did it.'";

#[test]
fn a_node_worktree_is_installed_once_and_reused() {
    if !have("npm") {
        eprintln!("skipped: npm is not on PATH");
        return;
    }
    let f = fixture("", AGENT, Node::Npm);
    let before = f.modules();
    let (res, rec) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let prepared = rec.prepare();
    assert_eq!(prepared.len(), 2, "{prepared:?}");
    assert_eq!(prepared[1].0, "passed");
    assert!(
        prepared[1].1.starts_with("npm ci --offline"),
        "{prepared:?}"
    );
    eprintln!("prepare: {}", prepared[1].1);
    let wt = PathBuf::from(&report(&res).worktree);
    let own = fs::symlink_metadata(wt.join("node_modules")).unwrap();
    assert!(own.is_dir() && !own.file_type().is_symlink());
    assert!(report(&res).linked_dependencies.is_empty());

    // The next task reuses it: nothing installed again.
    let (res, rec) = f.run("T006");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(
        rec.prepare().last().map(|(s, d)| (s.as_str(), d.as_str())),
        Some(("skipped", "reused: the lockfiles are unchanged"))
    );
    // The checkout: untouched.
    assert_eq!(f.modules(), before);
    assert_eq!(git(f.path(), &["status", "--porcelain"]), "");
}

#[test]
fn a_branch_that_changes_its_lockfile_is_prepared_again() {
    if !have("npm") {
        eprintln!("skipped: npm is not on PATH");
        return;
    }
    // The agent changes the lockfile (a byte npm ignores): the tick commits it.
    let f = fixture(
        "",
        "printf '\\n' >> package-lock.json\necho done > done.txt\necho 'Did it.'",
        Node::Npm,
    );
    let (res, _) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let (res, rec) = f.run("T006");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let last = rec.prepare().pop().unwrap();
    assert_eq!(last.0, "passed", "{last:?}");
    assert!(last.1.starts_with("npm ci --offline"), "{last:?}");
}

#[test]
fn a_package_missing_from_the_cache_stops_with_the_fix() {
    if !have("npm") {
        eprintln!("skipped: npm is not on PATH");
        return;
    }
    let f = fixture("", AGENT, Node::Handmade);
    // A lockfile naming a package no cache holds.
    let zeros = format!("sha512-{}==", "A".repeat(86));
    fs::write(
        f.path().join("package.json"),
        "{\"name\": \"fixture\", \"version\": \"1.0.0\", \"dependencies\": {\"speccoder-never-cached\": \"1.0.0\"}}\n",
    )
    .unwrap();
    fs::write(
        f.path().join("package-lock.json"),
        format!(
            "{{\"name\": \"fixture\", \"version\": \"1.0.0\", \"lockfileVersion\": 3, \"requires\": true, \"packages\": {{\"\": {{\"name\": \"fixture\", \"version\": \"1.0.0\", \"dependencies\": {{\"speccoder-never-cached\": \"1.0.0\"}}}}, \"node_modules/speccoder-never-cached\": {{\"version\": \"1.0.0\", \"resolved\": \"https://registry.npmjs.org/speccoder-never-cached/-/speccoder-never-cached-1.0.0.tgz\", \"integrity\": \"{zeros}\"}}}}}}\n"
        ),
    )
    .unwrap();
    git(f.path(), &["commit", "-qam", "a dependency nobody has"]);
    let (res, rec) = f.run("T005");
    assert_eq!(
        res.status(),
        CommandStatus::ConfigError,
        "{}",
        res.message()
    );
    assert_eq!(res.code().map(|c| c.name), Some("PREPARE-FAILED"));
    assert!(
        res.message()
            .contains("`npm ci --offline` failed in the worktree"),
        "{}",
        res.message()
    );
    let fix = res.fix().join("\n");
    assert!(fix.contains("`npm install` once in your checkout"), "{fix}");
    assert!(fix.contains("network_allow = [[\"npm\", \"ci\"]]"), "{fix}");
    assert_eq!(rec.prepare().last().map(|p| p.0.as_str()), Some("failed"));
}

#[test]
fn a_manifest_without_its_lockfile_stops_naming_it() {
    let f = fixture("", AGENT, Node::Handmade);
    git(f.path(), &["rm", "-q", "package-lock.json"]);
    git(f.path(), &["commit", "-qm", "no lockfile"]);
    let (res, _) = f.run("T005");
    assert_eq!(res.code().map(|c| c.name), Some("PREPARE-FAILED"));
    assert!(
        res.message().contains("no lockfile is committed"),
        "{}",
        res.message()
    );
    assert!(res.fix().join("\n").contains("npm install"));
}

#[test]
fn prepare_off_leaves_the_worktree_as_committed() {
    let f = fixture("[worktree]\nprepare = \"off\"\n", AGENT, Node::Handmade);
    let (res, rec) = f.run("T005");
    // As before: the check fails without the folder.
    assert_eq!(
        res.code().map(|c| c.name),
        Some("VERIFICATION-FAILED"),
        "{}",
        res.message()
    );
    assert!(rec.prepare().is_empty());
    assert!(
        !PathBuf::from(&report(&res).worktree)
            .join("node_modules")
            .exists()
    );
}

#[test]
fn a_go_or_rust_project_has_no_prepare_step() {
    let f = fixture("", AGENT, Node::Handmade);
    for p in ["package.json", "package-lock.json"] {
        git(f.path(), &["rm", "-q", p]);
    }
    fs::write(f.path().join("go.mod"), "module x\n").unwrap();
    git(f.path(), &["add", "-A"]);
    git(f.path(), &["commit", "-qm", "go"]);
    let (_, rec) = f.run("T005");
    assert!(rec.prepare().is_empty());
}

#[test]
fn a_linked_node_modules_is_the_checkouts_read_only_and_invisible_to_git() {
    if !sandboxed() {
        return;
    }
    let f = fixture(
        "[worktree]\nlink = [\"node_modules\"]\n",
        AGENT,
        Node::Handmade,
    );
    let before = f.modules();
    let status = git(f.path(), &["status", "--porcelain"]);
    let (res, rec) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(rec.prepare().last().unwrap().1, "linked node_modules");
    let rep = report(&res);
    let wt = PathBuf::from(&rep.worktree);
    let link = wt.join("node_modules");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::canonicalize(&link).unwrap(),
        fs::canonicalize(f.path().join("node_modules")).unwrap()
    );
    // Evidence: the report, the trailer.
    let sha = prepare::sha256_file(&f.path().join("package-lock.json")).unwrap();
    assert_eq!(rep.linked_dependencies.len(), 1);
    assert_eq!(rep.linked_dependencies[0].path, "node_modules");
    assert_eq!(rep.linked_dependencies[0].lockfile, "package-lock.json");
    assert_eq!(rep.linked_dependencies[0].sha256, sha);
    let message = git(&wt, &["log", "-1", "--format=%B"]);
    assert!(
        message.contains(&format!(
            "Linked-Dependencies: node_modules (package-lock.json sha256:{sha})"
        )),
        "{message}"
    );
    // The signed statement carries it too.
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let note = speccoder_runtime::attest::notes::read(&wt, head.trim()).unwrap_or_default();
    let statement = note
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|e| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(e["payload"].as_str()?)
                .ok()
        })
        .filter_map(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .find(|s| s["predicateType"] == "https://speccoder.dev/task-verification/v0")
        .unwrap_or_else(|| panic!("no task-verification statement: {note}"));
    assert_eq!(
        statement["predicate"]["linked_dependencies"],
        serde_json::json!([{ "path": "node_modules", "lockfile": "package-lock.json", "sha256": sha }])
    );
    assert_eq!(statement["predicate"]["revision"], 4);
    // git in the worktree never sees the link, and no tick commit holds it.
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    assert_eq!(git(&wt, &["ls-files", "node_modules"]), "");
    let exclude = fs::read_to_string(f.path().join(".git/info/exclude")).unwrap();
    assert!(
        exclude.contains("# speccoder: [worktree] link\n/node_modules\n"),
        "{exclude}"
    );
    // A sandboxed command cannot write through it.
    let (runner, _) = sandbox::runner_at(&wt).unwrap();
    let out = sandbox::Runner::run_with(
        &runner,
        &speccoder_runtime::verify::process::Invocation {
            argv: [
                "sh",
                "-c",
                "mkdir node_modules/left-pad && echo x > node_modules/tiny/index.js",
            ]
            .map(str::to_owned)
            .to_vec(),
            cwd: wt.clone(),
            env: speccoder_runtime::verify::process::scrubbed_env(std::env::vars()),
            timeout: std::time::Duration::from_secs(30),
            output_cap: 64 * 1024,
        },
        false,
    );
    assert_ne!(out.exit_code, Some(0));
    let said = String::from_utf8_lossy(&out.stderr.kept).into_owned();
    assert_eq!(
        prepare::linked_note(&said, &["node_modules".to_owned()]).as_deref(),
        Some("`node_modules` is linked from the checkout, read-only ([worktree] link)"),
        "{said}"
    );
    // The checkout: unchanged, its status too.
    assert_eq!(f.modules(), before);
    assert_eq!(git(f.path(), &["status", "--porcelain"]), status);

    // The next task reuses the link.
    let (res, rec) = f.run("T006");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(rec.prepare().last().unwrap().0, "skipped");

    // Removing the worktree removes the link first; the checkout's folder stays.
    prepare::unlink(&wt);
    assert!(fs::symlink_metadata(&link).is_err());
    git(f.path(), &["worktree", "remove", &wt.display().to_string()]);
    assert_eq!(f.modules(), before);
}

#[test]
fn an_executor_that_changes_a_linked_folder_stops_the_task() {
    if !sandboxed() {
        return;
    }
    // Outside the sandbox the agent can write through the link.
    let f = fixture(
        "[worktree]\nlink = [\"node_modules\"]\n",
        "echo '{\"changed\": true}' > node_modules/.package-lock.json\necho done > done.txt\necho 'Did it.'",
        Node::Handmade,
    );
    let (res, _) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("LINK-CHANGED"));
    assert!(res.message().contains("node_modules"), "{}", res.message());
    let tasks = fs::read_to_string(f.path().join("specs/001-graph/tasks.md")).unwrap();
    assert!(tasks.contains("- [ ] T005"));
    let wt_tasks = git(
        f.path(),
        &["show", "speccoder/001-graph:specs/001-graph/tasks.md"],
    );
    assert!(wt_tasks.contains("- [ ] T005"), "nothing ticked");
}

/// A worktree of the fixture's `HEAD` and a sandboxed runner in it, for
/// preparing by hand.
fn worktree(f: &Fixture) -> (PathBuf, sandbox::Runner) {
    let wt = f.state.path().join("wts").join("001-x");
    fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(
        f.path(),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &wt.display().to_string(),
            "HEAD",
        ],
    );
    let wt = fs::canonicalize(&wt).unwrap();
    let (runner, _) = sandbox::runner(&wt, &sandbox::Config::default()).unwrap();
    (wt, runner)
}

fn prepare_in(
    f: &Fixture,
    wt: &Path,
    runner: &sandbox::Runner,
    link: &[&str],
) -> Result<Option<prepare::Prepared>, prepare::Failure> {
    let config = prepare::Config {
        prepare: true,
        link: link.iter().map(|s| (*s).to_owned()).collect(),
    };
    prepare::prepare(&Prepare {
        worktree: wt,
        project: wt,
        checkout: f.path(),
        config: &config,
        runner,
        state: None,
    })
}

#[test]
fn a_lockfile_that_differs_is_installed_with_the_reason() {
    if !have("npm") || !sandboxed() {
        eprintln!("skipped: needs npm and the OS sandbox");
        return;
    }
    let f = fixture("", AGENT, Node::Npm);
    let (wt, runner) = worktree(&f);
    // The checkout's dependencies moved on, uncommitted.
    let lock = f.path().join("package-lock.json");
    fs::write(&lock, format!("{}\n", fs::read_to_string(&lock).unwrap())).unwrap();
    let p = prepare_in(&f, &wt, &runner, &["node_modules"])
        .unwrap()
        .unwrap();
    assert_eq!(p.record.roots[0].mode, Mode::Installed);
    assert!(p.done[0].starts_with("npm ci --offline"), "{:?}", p.done);
    assert_eq!(
        p.notes,
        [
            "node_modules not linked: package-lock.json differs from your checkout's (the branch changed its dependencies); installed in the worktree instead"
        ]
    );
    assert!(p.record.linked().is_empty());
    assert!(
        !fs::symlink_metadata(wt.join("node_modules"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn a_folder_the_checkout_does_not_ignore_or_lacks_is_never_linked() {
    if !sandboxed() {
        return;
    }
    let f = fixture("", AGENT, Node::Handmade);
    fs::write(f.path().join(".gitignore"), "").unwrap();
    git(f.path(), &["commit", "-qam", "nothing ignored"]);
    let (wt, runner) = worktree(&f);
    let root = prepare::roots(&wt).remove(0);
    assert_eq!(
        prepare::link_state(f.path(), &root, None).unwrap_err(),
        "not git-ignored in your checkout"
    );
    fs::rename(f.path().join("node_modules"), f.path().join("elsewhere")).unwrap();
    assert_eq!(
        prepare::link_state(f.path(), &root, None).unwrap_err(),
        "missing in your checkout"
    );
    // Preparing falls back to an install (none possible here: no npm cache
    // holds the handmade lockfile), so the link is never made.
    let _ = prepare_in(&f, &wt, &runner, &["node_modules"]);
    assert!(
        fs::symlink_metadata(wt.join("node_modules")).map_or(true, |m| !m.file_type().is_symlink())
    );
    assert!(
        !fs::read_to_string(f.path().join(".git/info/exclude"))
            .unwrap_or_default()
            .contains("speccoder")
    );
}

#[test]
fn a_link_resolving_outside_the_checkout_or_unsandboxed_is_refused() {
    let f = fixture("", AGENT, Node::Handmade);
    let outside = tempfile::tempdir().unwrap();
    fs::rename(f.path().join("node_modules"), outside.path().join("nm")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("nm"), f.path().join("node_modules")).unwrap();
    let root = prepare::roots(f.path()).remove(0);
    assert_eq!(
        prepare::link_state(f.path(), &root, None).unwrap_err(),
        "it resolves outside your checkout"
    );
    let (wt, _) = worktree(&f);
    let p = prepare_in(&f, &wt, &sandbox::Runner::Plain, &["node_modules"]);
    let notes = p.map(|p| p.map(|p| p.notes).unwrap_or_default());
    // Unsandboxed, a link would be writable: refused before anything else.
    if let Ok(notes) = notes {
        assert!(
            notes[0].contains("commands run unsandboxed here"),
            "{notes:?}"
        );
    }
    assert!(
        fs::symlink_metadata(wt.join("node_modules")).map_or(true, |m| !m.file_type().is_symlink())
    );
}

#[test]
fn linking_a_large_folder_takes_under_a_second() {
    if !sandboxed() {
        return;
    }
    let f = fixture("", AGENT, Node::Handmade);
    for i in 0..200 {
        let d = f.path().join(format!("node_modules/p{i}"));
        fs::create_dir_all(&d).unwrap();
        for j in 0..100 {
            fs::write(d.join(format!("f{j}.js")), "x").unwrap();
        }
    }
    let (wt, runner) = worktree(&f);
    let began = std::time::Instant::now();
    let p = prepare_in(&f, &wt, &runner, &["node_modules"])
        .unwrap()
        .unwrap();
    let took = began.elapsed();
    assert_eq!(p.done, ["linked node_modules"]);
    eprintln!("linked a node_modules of 20 000 files in {took:?}");
    assert!(took < std::time::Duration::from_secs(1), "{took:?}");
}

/// A uv project with a `src/` package and no dependencies (nothing to
/// download), synced in the checkout; `packaged`: built and installed
/// editable, else a virtual project.
fn uv_fixture(packaged: bool) -> Option<Fixture> {
    if !have("uv") {
        eprintln!("skipped: uv is not on PATH");
        return None;
    }
    let f = fixture("", AGENT, Node::Handmade);
    let r = f.path();
    for p in ["package.json", "package-lock.json"] {
        git(r, &["rm", "-q", p]);
    }
    let _ = fs::remove_dir_all(r.join("node_modules"));
    let build = if packaged {
        "\n[build-system]\nrequires = [\"uv_build>=0.8,<1\"]\nbuild-backend = \"uv_build\"\n"
    } else {
        ""
    };
    fs::write(
        r.join("pyproject.toml"),
        format!(
            "[project]\nname = \"greet\"\nversion = \"0.1.0\"\nrequires-python = \">=3.9\"\n{build}"
        ),
    )
    .unwrap();
    fs::create_dir_all(r.join("src/greet")).unwrap();
    fs::write(r.join("src/greet/__init__.py"), "WHO = 'world'\n").unwrap();
    fs::write(r.join(".gitignore"), ".venv/\n").unwrap();
    let uv = |args: &[&str]| {
        Command::new("uv")
            .args(args)
            .current_dir(r)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if !uv(&["lock", "--offline"]) || !uv(&["sync", "--offline"]) {
        eprintln!(
            "skipped: uv cannot lock or sync offline here (no Python, or uv_build not cached)"
        );
        return None;
    }
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "python"]);
    Some(f)
}

#[test]
fn a_uv_worktree_is_synced_once_and_its_checks_import_it() {
    let Some(f) = uv_fixture(true) else { return };
    let (wt, runner) = worktree(&f);
    let p = prepare_in(&f, &wt, &runner, &[]).unwrap().unwrap();
    assert!(
        p.done[0].starts_with("uv sync --locked --offline"),
        "{:?}",
        p.done
    );
    assert!(wt.join(".venv/bin/python").exists());
    let again = prepare_in(&f, &wt, &runner, &[]).unwrap().unwrap();
    assert!(again.done.is_empty(), "{:?}", again.done);
}

#[test]
fn a_linked_venv_is_used_only_when_the_worktrees_package_imports() {
    let Some(f) = uv_fixture(true) else { return };
    if !sandboxed() {
        return;
    }
    let (wt, runner) = worktree(&f);
    let p = prepare_in(&f, &wt, &runner, &[".venv"]).unwrap().unwrap();
    assert_eq!(p.done, ["linked .venv"], "{:?}", p.notes);
    let src = wt.join("src").display().to_string();
    assert_eq!(p.record.roots[0].pythonpath.as_deref(), Some(src.as_str()));
    assert_eq!(
        p.record.env(),
        [
            ("PYTHONPATH".to_owned(), src.clone()),
            ("UV_NO_SYNC".to_owned(), "1".to_owned())
        ]
    );
    // Every command in the worktree gets it, and imports the worktree's code.
    let (runner, _) = sandbox::runner_at(&wt).unwrap();
    let out = sandbox::Runner::run_with(
        &runner,
        &speccoder_runtime::verify::process::Invocation {
            argv: [
                "uv",
                "run",
                "--locked",
                "--offline",
                "python",
                "-c",
                "import greet; print(greet.__file__)",
            ]
            .map(str::to_owned)
            .to_vec(),
            cwd: wt.clone(),
            env: speccoder_runtime::verify::process::scrubbed_env(std::env::vars()),
            timeout: std::time::Duration::from_secs(60),
            output_cap: 64 * 1024,
        },
        false,
    );
    let printed = String::from_utf8_lossy(&out.stdout.kept).trim().to_owned();
    assert_eq!(
        out.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr.kept)
    );
    assert!(
        fs::canonicalize(&printed).unwrap().starts_with(&wt),
        "{printed}"
    );
    prepare::unlink(&wt);
    assert!(fs::symlink_metadata(wt.join(".venv")).is_err());
}

#[test]
fn a_venv_that_would_import_the_checkouts_code_is_never_linked() {
    let Some(f) = uv_fixture(false) else { return };
    if !sandboxed() {
        return;
    }
    // The checkout's .venv imports `greet` from the checkout (as an
    // editable install does); the worktree's own copy is gone.
    let lib = f.path().join(".venv/lib");
    let site = fs::read_dir(&lib)
        .unwrap()
        .flatten()
        .next()
        .map(|py| py.path().join("site-packages"))
        .unwrap();
    let pth = site.join("_greet.pth");
    let points = format!("{}\n", f.path().join("src").display());
    fs::write(&pth, &points).unwrap();
    let (wt, runner) = worktree(&f);
    fs::remove_dir_all(wt.join("src/greet")).unwrap();
    let p = prepare_in(&f, &wt, &runner, &[".venv"]).unwrap().unwrap();
    assert_eq!(p.record.roots[0].mode, Mode::Installed);
    assert!(
        p.notes[0].starts_with(".venv not linked: `import greet` resolves outside the worktree"),
        "{:?}",
        p.notes
    );
    assert!(p.record.env().is_empty());
    assert!(
        !fs::symlink_metadata(wt.join(".venv"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // The checkout's own .venv: untouched.
    assert_eq!(fs::read_to_string(&pth).unwrap(), points);
}
