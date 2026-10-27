//! A run's worktree gets its dependency folders by the toolchain's locked
//! offline install, and the git-ignored files `[worktree] include` names
//! copied from the checkout — their secret values masked and never
//! committed. A shell script stands in for the agent (an `[executor]`), so
//! no model call is made. Tests that install need the real `npm` or `uv`
//! and are skipped, with a note, without them — except where
//! `SPECCODER_REQUIRE_TOOLS` names the tool (CI).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::prepare::{self, Prepare};
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

/// Whether `tool` is on `PATH`; skipped with a note when not — except
/// where `SPECCODER_REQUIRE_TOOLS` names it (CI: `npm`), where a missing
/// tool fails the test instead of passing it silently.
fn have(tool: &str) -> bool {
    let ok = Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let required = std::env::var("SPECCODER_REQUIRE_TOOLS")
        .is_ok_and(|v| v.split([',', ' ']).any(|t| t == tool || t == "all"));
    assert!(
        ok || !required,
        "{tool} is required here (SPECCODER_REQUIRE_TOOLS) but not on PATH"
    );
    if !ok {
        eprintln!("skipped: {tool} is not on PATH");
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
    /// Written by hand: nothing to install from.
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
        return;
    }
    // The agent changes the lockfile (a byte npm ignores): the tick commits it.
    let f = fixture(
        "",
        "printf '\\n' >> package-lock.json\necho done > done.txt\necho 'Did it.'",
        Node::Npm,
    );
    let (res, rec) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    // Installed again before the checks, for the lockfile committed.
    let last = rec.prepare().pop().unwrap();
    assert_eq!(last.0, "passed", "{last:?}");
    assert!(
        last.1.starts_with("npm ci --offline")
            && last.1.ends_with(": the attempt changed its lockfile"),
        "{last:?}"
    );
    let wt = PathBuf::from(&report(&res).worktree);
    let committed = git(&wt, &["show", "HEAD:package-lock.json"]);
    assert_eq!(
        prepare::load(&wt).roots[0].lock_sha256,
        speccoder_runtime::integrity::sha256_hex(committed.as_bytes())
    );
    // The next task starts from that lockfile: nothing to install first.
    let (res, rec) = f.run("T006");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(rec.prepare()[1].0, "skipped", "{:?}", rec.prepare());
}

#[test]
fn an_attempt_whose_lockfile_cannot_install_is_saved_and_undone() {
    if !have("npm") {
        return;
    }
    let zeros = format!("sha512-{}==", "A".repeat(86));
    let lock = format!(
        "{{\"name\": \"fixture\", \"version\": \"1.0.0\", \"lockfileVersion\": 3, \"requires\": true, \"packages\": {{\"\": {{\"name\": \"fixture\", \"version\": \"1.0.0\", \"dependencies\": {{\"speccoder-never-cached\": \"1.0.0\"}}}}, \"node_modules/speccoder-never-cached\": {{\"version\": \"1.0.0\", \"resolved\": \"https://registry.npmjs.org/speccoder-never-cached/-/speccoder-never-cached-1.0.0.tgz\", \"integrity\": \"{zeros}\"}}}}}}"
    );
    let f = fixture(
        "",
        &format!(
            "cat > package-lock.json <<'EOF'\n{lock}\nEOF\necho done > done.txt\necho 'Did it.'"
        ),
        Node::Npm,
    );
    let (res, _) = f.run("T005");
    assert_eq!(
        res.code().map(|c| c.name),
        Some("PREPARE-FAILED"),
        "{}",
        res.message()
    );
    assert!(
        res.message()
            .contains("is saved as refs/speccoder/attempts/"),
        "{}",
        res.message()
    );
    // Undone: the worktree's lockfile is the committed one again.
    let wt = PathBuf::from(
        git(f.path(), &["worktree", "list", "--porcelain"])
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .nth(1)
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(wt.join("package-lock.json")).unwrap(),
        git(&wt, &["show", "HEAD:package-lock.json"])
    );
    assert!(!wt.join("done.txt").exists());
}

#[test]
fn a_package_missing_from_the_cache_stops_with_the_fix() {
    if !have("npm") {
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
    wt: &Path,
    runner: &sandbox::Runner,
) -> Result<Option<prepare::Prepared>, prepare::Failure> {
    prepare::prepare(&Prepare {
        worktree: wt,
        project: wt,
        config: &prepare::Config::default(),
        runner,
        state: None,
    })
}

/// A uv project with a `src/` package and no dependencies (nothing to
/// download), synced in the checkout; `packaged`: built and installed
/// editable, else a virtual project.
fn uv_fixture(packaged: bool) -> Option<Fixture> {
    if !have("uv") {
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
        assert!(
            !std::env::var("SPECCODER_REQUIRE_TOOLS").is_ok_and(|v| v.contains("uv")),
            "uv is required here (SPECCODER_REQUIRE_TOOLS) but cannot lock or sync offline"
        );
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
    let p = prepare_in(&wt, &runner).unwrap().unwrap();
    assert!(
        p.done[0].starts_with("uv sync --locked --offline"),
        "{:?}",
        p.done
    );
    assert!(wt.join(".venv/bin/python").exists());
    let again = prepare_in(&wt, &runner).unwrap().unwrap();
    assert!(again.done.is_empty(), "{:?}", again.done);
}

// ── [worktree] include ──────────────────────────────────────────────────────

/// A value of the checkout's `.env`, long enough to be masked.
const TOKEN: &str = "tok-Zx81-secret-value";

/// The check prints the copied `.env` and its token's base64, and passes
/// only where the file was copied.
const ENV_CHECK: &str = "[checks.env]\nargv = [\"sh\", \"-c\", \"cat .env; grep API_TOKEN .env | cut -d= -f2 | tr -d '\\\\n' | base64; test -f .env\"]\n";

/// A checkout with no dependencies and a git-ignored `.env`, copied by
/// `[worktree] include`.
fn env_fixture(script: &str) -> Fixture {
    let f = fixture("[worktree]\ninclude = [\".env\"]\n", script, Node::Handmade);
    let r = f.path();
    for p in ["package.json", "package-lock.json"] {
        git(r, &["rm", "-q", p]);
    }
    let _ = fs::remove_dir_all(r.join("node_modules"));
    let toml = fs::read_to_string(r.join("speccoder.toml"))
        .unwrap()
        .replace(CHECK, ENV_CHECK);
    fs::write(r.join("speccoder.toml"), toml).unwrap();
    fs::write(r.join(".gitignore"), "node_modules/\n.env\n").unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "env"]);
    fs::write(r.join(".env"), format!("API_TOKEN={TOKEN}\nDEBUG=true\n")).unwrap();
    f
}

fn base64_of(s: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// Every file under `dir` but those under `except`, with its content.
fn files_under(dir: &Path, except: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let except = fs::canonicalize(except).unwrap();
    let mut stack = vec![fs::canonicalize(dir).unwrap()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.starts_with(&except) {
                continue;
            }
            match fs::symlink_metadata(&p) {
                Ok(m) if m.is_dir() => stack.push(p),
                Ok(m) if m.is_file() => out.push((p.clone(), fs::read(&p).unwrap_or_default())),
                _ => {}
            }
        }
    }
    out
}

#[test]
fn a_copied_env_value_is_masked_in_every_output_and_never_committed() {
    let f = env_fixture("cat .env\necho done > done.txt\necho 'Did it.'");
    let (res, _) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    let wt = PathBuf::from(&rep.worktree);
    // Copied: the check that needs it passed.
    assert!(fs::read_to_string(wt.join(".env")).unwrap().contains(TOKEN));
    let shown = format!("{}\n{}", res.message(), serde_json::to_string(rep).unwrap());
    assert!(
        shown.contains("[redacted:API_TOKEN]"),
        "the check's output is shown: {shown}"
    );
    for form in [TOKEN.to_owned(), base64_of(TOKEN)] {
        assert!(!shown.contains(&form), "{form} in {shown}");
        // Nothing speccoder keeps — journal, reports, records — holds it.
        let wts = wt.parent().unwrap().to_path_buf();
        for (p, bytes) in files_under(f.state.path(), &wts) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(&form),
                "{form} in {}",
                p.display()
            );
        }
    }
    // Never in the tick's commit.
    assert_eq!(git(&wt, &["ls-files", ".env"]), "");
    assert!(!git(&wt, &["log", "--all", "-p"]).contains(TOKEN));
}

#[test]
fn a_copied_file_stays_out_of_the_commit_whatever_the_ignore_rules_say() {
    // The attempt un-ignores it: still never staged.
    let f = env_fixture("printf '!.env\\n' >> .gitignore\necho done > done.txt\necho 'Did it.'");
    let (res, _) = f.run("T005");
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let wt = PathBuf::from(&report(&res).worktree);
    assert!(git(&wt, &["show", "HEAD:.gitignore"]).contains("!.env"));
    assert_eq!(git(&wt, &["ls-files", ".env"]), "");
    assert!(!git(&wt, &["log", "--all", "-p"]).contains(TOKEN));
}

#[test]
fn an_attempt_that_writes_a_copied_value_into_the_code_is_refused() {
    for leak in [
        "grep API_TOKEN .env | cut -d= -f2 > leaked.txt".to_owned(),
        format!("echo 'const t = \"{}\";' > leaked.js", base64_of(TOKEN)),
    ] {
        let f = env_fixture(&format!("{leak}\necho done > done.txt\necho 'Did it.'"));
        let (res, _) = f.run("T005");
        assert_eq!(
            res.code().map(|c| c.name),
            Some("SECRET-IN-DIFF"),
            "{}",
            res.message()
        );
        assert!(res.message().contains("API_TOKEN"), "{}", res.message());
        assert!(!res.message().contains(TOKEN));
        let wt = PathBuf::from(
            git(f.path(), &["worktree", "list", "--porcelain"])
                .lines()
                .filter_map(|l| l.strip_prefix("worktree "))
                .nth(1)
                .unwrap(),
        );
        assert!(
            !wt.join("leaked.txt").exists() && !wt.join("leaked.js").exists(),
            "undone"
        );
        assert!(!git(f.path(), &["log", "--all", "-p"]).contains(TOKEN));
        let tasks = git(
            f.path(),
            &["show", "speccoder/001-graph:specs/001-graph/tasks.md"],
        );
        assert!(tasks.contains("- [ ] T005"), "nothing ticked");
    }
}
