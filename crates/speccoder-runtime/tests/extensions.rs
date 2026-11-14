//! Extensions end to end (spec 096): a folder trusted by one digest, its
//! gate blocking a tick with findings, its views, its skills and MCP
//! servers — and nothing of it before a person trusted it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::extensions::ViewData;
use speccoder_core::hooks::TrustState;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::extensions::{self, report, trust, views};

const BASE: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"]]\n";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extensions")
        .join(name)
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn cp(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(from.join("."))
            .arg(to)
            .status()
            .unwrap()
            .success()
    );
}

/// The `graph` fixture with the extensions `exts` installed, committed.
fn repo(exts: &[&str], files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    cp(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../speccoder-speckit/tests/fixtures/graph"),
        r,
    );
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n",
    )
    .unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), BASE).unwrap();
    for e in exts {
        cp(&fixture(e), &r.join(".speccoder/extensions").join(e));
    }
    for (path, text) in files {
        let p = r.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// Commits what changed: a run, and trust, use what is committed.
fn commit(repo: &Path) {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "change"]);
}

fn trust_now(repo: &Path, state: &Path, name: &str) {
    let e = extensions::load(repo)
        .into_iter()
        .find(|e| e.name == name)
        .unwrap();
    trust::record(
        &speccoder_runtime::hooks::dir(state, repo),
        &e,
        "t",
        "terminal",
    );
}

fn run(repo: &Path, state: &Path) -> (CommandResult, Arc<FakeProvider>) {
    let f = FakeProvider::new();
    f.will_call_tool("c1", "edit_file", json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }));
    f.will_structure(json!({ "summary": "added login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f.clone();
    (
        speccoder_runtime::run_with(repo, None, "T005", state, d, None),
        f,
    )
}

fn run_report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn sandboxed() -> bool {
    speccoder_runtime::sandbox::backend().is_ok()
}

// ── loading ────────────────────────────────────────────────────────────

#[test]
fn an_agent_plugins_folder_loads_with_every_contribution() {
    let r = repo(&["kitchen", "plain-plugin"], &[]);
    let all = extensions::load(r.path());
    assert_eq!(all.len(), 2);
    let k = all.iter().find(|e| e.name == "kitchen").unwrap();
    assert!(k.usable(), "{:?}", k.problems);
    assert_eq!(k.version.as_deref(), Some("1.0.0"));
    let names: Vec<String> = k.hooks.iter().map(|h| h.name.clone()).collect();
    assert_eq!(names, ["kitchen.check", "kitchen.notify"]);
    assert_eq!(k.views.len(), 3);
    assert_eq!(k.skills[0].0, "kitchen:hello");
    let lines: Vec<String> = k.contributions().iter().map(|c| c.line.clone()).collect();
    assert!(
        lines
            .iter()
            .any(|l| l == "gate before tick: runs ./scripts/check.sh — no network"),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("observer, hears task.stopped: runs ./scripts/notify.sh")),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("view “Notes” on each feature: reads notes.md"))
    );
    assert!(lines.iter().any(|l| l.starts_with("skill /kitchen:hello")));
    let p = all.iter().find(|e| e.name == "plain-plugin").unwrap();
    assert!(p.usable(), "{:?}", p.problems);
    assert_eq!(p.servers[0].name, "plain_plugin_notes");
    assert_eq!(p.servers[0].command[0], "python3");
}

#[test]
fn one_byte_changes_the_digest_and_problems_keep_it_from_loading() {
    let r = repo(&["kitchen"], &[]);
    let before = extensions::load(r.path())[0].digest.clone();
    let script = r
        .path()
        .join(".speccoder/extensions/kitchen/scripts/notify.sh");
    let mut text = fs::read_to_string(&script).unwrap();
    text.push(' ');
    fs::write(&script, text).unwrap();
    // Not committed: named, and nothing of it loads.
    let pending = &extensions::load(r.path())[0];
    assert!(
        pending
            .problems
            .iter()
            .any(|p| p.contains("has changes not committed")),
        "{:?}",
        pending.problems
    );
    commit(r.path());
    assert_ne!(extensions::load(r.path())[0].digest, before);

    let ext = r.path().join(".speccoder/extensions/kitchen");
    let toml = ext.join("dev.speccoder/extension.toml");
    let good = fs::read_to_string(&toml).unwrap();
    for (bad, words) in [
        ("[menus.x]\na = 1\n", "unknown table `[menus]`"),
        (
            "[views.x]\nfile = \"a.md\"\nwidget = \"chart\"\n",
            "not supported by this version",
        ),
        (
            "[hooks.x]\nat = \"tick\"\nargv = [\"true\"]\ncolour = 1\n",
            "unknown key `colour`",
        ),
    ] {
        fs::write(&toml, format!("{good}\n{bad}")).unwrap();
        commit(r.path());
        let e = &extensions::load(r.path())[0];
        assert!(!e.usable(), "{bad}");
        assert!(
            e.problems.iter().any(|p| p.contains(words)),
            "{:?}",
            e.problems
        );
    }
    fs::write(&toml, good).unwrap();
    commit(r.path());

    #[cfg(unix)]
    {
        // Any link is refused, inside the folder or out of it.
        std::os::unix::fs::symlink("scripts", ext.join("tools")).unwrap();
        commit(r.path());
        let e = &extensions::load(r.path())[0];
        assert!(
            e.problems.iter().any(|p| p.contains("is a link")),
            "{:?}",
            e.problems
        );
        fs::remove_file(ext.join("tools")).unwrap();
        commit(r.path());
    }
    // A program outside its own files, or a path out of its folder.
    let good = fs::read_to_string(&toml).unwrap();
    for (argv, words) in [
        (
            "[\"./../../../src/lib.rs\"]",
            "leaves the extension's folder",
        ),
        (
            "[\"./scripts/missing.sh\"]",
            "not one of its committed files",
        ),
    ] {
        fs::write(
            &toml,
            good.replace("argv = [\"./scripts/check.sh\"]", &format!("argv = {argv}")),
        )
        .unwrap();
        commit(r.path());
        let e = &extensions::load(r.path())[0];
        assert!(
            e.problems.iter().any(|p| p.contains(words)),
            "{argv}: {:?}",
            e.problems
        );
    }
    fs::write(&toml, good).unwrap();
    commit(r.path());
    let json = ext.join("plugin.json");
    fs::write(
        &json,
        fs::read_to_string(&json)
            .unwrap()
            .replace("\"kitchen\"", "\"Kitchen\""),
    )
    .unwrap();
    commit(r.path());
    assert!(!extensions::load(r.path())[0].usable());
}

#[test]
fn add_copies_without_running_and_remove_forgets() {
    let r = repo(&[], &[]);
    let state = tempfile::tempdir().unwrap();
    let added = report::add_cmd(r.path(), state.path(), fixture("kitchen").to_str().unwrap());
    assert_eq!(added.status(), CommandStatus::Ok, "{}", added.message());
    assert!(added.message().contains("commit it, then trust it"));
    assert!(
        r.path()
            .join(".speccoder/extensions/kitchen/plugin.json")
            .is_file()
    );
    let again = report::add_cmd(r.path(), state.path(), fixture("kitchen").to_str().unwrap());
    assert_eq!(again.status(), CommandStatus::UsageError);
    assert!(
        again.message().contains("already here"),
        "{}",
        again.message()
    );

    // A repository only at a full commit.
    let src = tempfile::tempdir().unwrap();
    cp(&fixture("plain-plugin"), src.path());
    git(src.path(), &["init", "-q"]);
    git(src.path(), &["add", "-A"]);
    git(src.path(), &["commit", "-q", "-m", "plugin"]);
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(src.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    commit(r.path());
    let url = src.path().to_string_lossy().into_owned();
    let short = report::add_cmd(r.path(), state.path(), &format!("{url}@main"));
    assert_eq!(short.status(), CommandStatus::UsageError);
    assert!(
        short.message().contains("not a full commit"),
        "{}",
        short.message()
    );
    let pinned = report::add_cmd(r.path(), state.path(), &format!("{url}@{}", head.trim()));
    assert_eq!(pinned.status(), CommandStatus::Ok, "{}", pinned.message());
    commit(r.path());
    let p = extensions::load(r.path())
        .into_iter()
        .find(|e| e.name == "plain-plugin")
        .unwrap();
    assert_eq!(p.origin.unwrap().commit.as_deref(), Some(head.trim()));
    assert!(
        !r.path()
            .join(".speccoder/extensions/plain-plugin/.git")
            .exists()
    );

    trust_now(r.path(), state.path(), "kitchen");
    let removed = report::remove_cmd(r.path(), state.path(), "kitchen");
    assert_eq!(removed.status(), CommandStatus::Ok, "{}", removed.message());
    assert!(!r.path().join(".speccoder/extensions/kitchen").exists());
    let records = speccoder_runtime::hooks::trust::read(&speccoder_runtime::hooks::dir(
        state.path(),
        r.path(),
    ));
    assert!(!records.contains_key("extension:kitchen"));
}

// ── US1: trust ─────────────────────────────────────────────────────────

#[test]
fn a_change_is_untrusted_again_and_names_what_widened_first() {
    let r = repo(&["kitchen"], &[]);
    let state = tempfile::tempdir().unwrap();
    let dir = speccoder_runtime::hooks::dir(state.path(), r.path());
    let e = &extensions::load(r.path())[0];
    assert_eq!(trust::state(&dir, e), TrustState::Untrusted);
    trust_now(r.path(), state.path(), "kitchen");
    assert_eq!(
        trust::state(&dir, &extensions::load(r.path())[0]),
        TrustState::Trusted
    );

    let toml = r
        .path()
        .join(".speccoder/extensions/kitchen/dev.speccoder/extension.toml");
    let text = fs::read_to_string(&toml).unwrap().replace(
        "argv = [\"./scripts/check.sh\"]\n",
        "argv = [\"./scripts/check.sh\"]\nnetwork = true\n",
    );
    fs::write(&toml, text).unwrap();
    commit(r.path());
    let listed = report::list(r.path(), state.path());
    let k = &listed.extensions[0];
    assert_eq!(k.trust, TrustState::Changed);
    let c = k.changes.as_ref().unwrap();
    assert_eq!(c.files, ["dev.speccoder/extension.toml"]);
    assert!(
        c.widened[0].starts_with("reaches network — gate before tick"),
        "{c:?}"
    );

    // CI: only the current digest.
    let stale = report::apply_flags(r.path(), state.path(), &["kitchen=sha256:0000".to_owned()])
        .unwrap_err();
    assert!(stale.message().contains(&k.digest), "{}", stale.message());
    report::apply_flags(r.path(), state.path(), &[format!("kitchen={}", k.digest)]).unwrap();
    assert_eq!(
        trust::state(&dir, &extensions::load(r.path())[0]),
        TrustState::Trusted
    );
}

#[test]
fn nothing_of_an_untrusted_extension_is_offered() {
    let r = repo(&["kitchen", "plain-plugin"], &[]);
    let state = tempfile::tempdir().unwrap();
    assert!(extensions::trusted(r.path(), state.path()).is_empty());
    let shown = views::views(r.path(), state.path(), Some("001-graph"), None);
    assert!(
        shown
            .views
            .iter()
            .all(|v| v.waits_for_trust && v.result.is_none())
    );
    let lines = report::doctor_lines(r.path(), state.path());
    assert!(
        lines
            .iter()
            .any(|(_, l)| l.contains("extension kitchen waits for your trust"))
    );
    assert_eq!(
        report::waiting(r.path(), state.path()),
        ["kitchen", "plain-plugin"]
    );
}

// ── US2: gates with findings ───────────────────────────────────────────

#[test]
fn an_untrusted_extensions_gate_blocks_the_tick() {
    if !sandboxed() {
        return;
    }
    let r = repo(&["threatspec-like"], &[]);
    let state = tempfile::tempdir().unwrap();
    let (res, _) = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = run_report(&res);
    assert!(!rep.ticked);
    let reason = rep.reason.as_deref().unwrap();
    assert!(
        reason.contains("waits for your trust of the extension threatspec-like"),
        "{reason}"
    );
}

#[test]
fn a_trusted_gate_blocks_with_its_findings_and_passes_once_mitigated() {
    if !sandboxed() {
        return;
    }
    let r = repo(&["threatspec-like"], &[]);
    let state = tempfile::tempdir().unwrap();
    trust_now(r.path(), state.path(), "threatspec-like");
    let (res, f) = run(r.path(), state.path());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("HOOK-BLOCKED"),
        "{}",
        res.message()
    );
    let rep = run_report(&res);
    // Its findings say more than its last line: they are the reason.
    let reason = rep.reason.as_deref().unwrap();
    assert!(
        reason
            .contains("1 finding, 1 error — T-001 tampering with the session token is unmitigated"),
        "{reason}"
    );
    let dir = speccoder_runtime::hooks::dir(state.path(), r.path());
    let runs = speccoder_runtime::hooks::runs(&dir);
    let last = runs["threatspec-like.check"].last().unwrap();
    assert_eq!(last.outcome, "blocked");
    assert_eq!(last.findings.len(), 1, "{last:?}");
    assert_eq!(last.findings[0].rule, "TS-C1");
    assert_eq!(
        last.findings[0].file.as_deref(),
        Some("specs/001-graph/spec.md")
    );
    // The findings reach the person, never the model.
    let asked = serde_json::to_string(&f.asked().into_iter().map(|a| a.prompt).collect::<Vec<_>>())
        .unwrap();
    assert!(!asked.contains("TS-C1"));

    // Mitigated: the next run ticks and names the gate.
    fs::write(r.path().join("specs/001-graph/MITIGATED"), "T-001\n").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "mitigate"]);
    let (res, _) = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = run_report(&res);
    assert!(rep.ticked);
    assert!(rep.gates.iter().any(|g| g.name == "threatspec-like.check"));
}

#[test]
fn findings_that_do_not_fit_are_named_and_change_no_outcome() {
    if !sandboxed() {
        return;
    }
    let r = repo(&["kitchen"], &[("specs/001-graph/ALLOW", "")]);
    let script = r
        .path()
        .join(".speccoder/extensions/kitchen/scripts/check.sh");
    let text = fs::read_to_string(&script)
        .unwrap()
        .replace("\"uri\": \"src/lib.rs\"", "\"uri\": \"/etc/passwd\"");
    fs::write(&script, text).unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "outside"]);
    let state = tempfile::tempdir().unwrap();
    trust_now(r.path(), state.path(), "kitchen");
    let (res, _) = run(r.path(), state.path());
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let runs =
        speccoder_runtime::hooks::runs(&speccoder_runtime::hooks::dir(state.path(), r.path()));
    let last = runs["kitchen.check"].last().unwrap();
    assert_eq!(last.outcome, "passed");
    assert!(last.findings.is_empty());
    assert!(
        last.findings_error
            .as_deref()
            .unwrap()
            .contains("outside the project"),
        "{last:?}"
    );
}

// ── US3: views ─────────────────────────────────────────────────────────

#[test]
fn views_render_are_kept_and_run_again() {
    if !sandboxed() {
        return;
    }
    let r = repo(
        &["kitchen", "threatspec-like"],
        &[(
            "specs/001-graph/threat-model.md",
            "# Threats\n\n<b>raw</b>\n\n```mermaid\ngraph TD; A-->B\n```\n",
        )],
    );
    let state = tempfile::tempdir().unwrap();
    trust_now(r.path(), state.path(), "kitchen");
    trust_now(r.path(), state.path(), "threatspec-like");
    let shown = views::views(r.path(), state.path(), Some("001-graph"), None);
    let find = |ext: &str, view: &str| {
        shown
            .views
            .iter()
            .find(|v| v.extension == ext && v.view.name == view)
            .unwrap()
            .clone()
    };
    let sec = find("threatspec-like", "findings");
    let result = sec.result.unwrap();
    match result.data {
        Some(ViewData::Findings { findings }) => assert_eq!(findings[0].rule, "TS-C1"),
        other => panic!("{other:?} {:?}", result.error),
    }
    let model = find("threatspec-like", "model").result.unwrap();
    match model.data {
        Some(ViewData::Markdown { html }) => {
            assert!(html.contains("&lt;b&gt;raw"), "{html}");
            assert!(html.contains("mermaid"), "{html}");
        }
        other => panic!("{other:?}"),
    }
    // A missing file says so, in words.
    let notes = find("kitchen", "notes").result.unwrap();
    assert!(notes.error.unwrap().contains("notes.md does not exist yet"));
    // Kept while nothing changed, and fast.
    let began = std::time::Instant::now();
    let again = views::views(r.path(), state.path(), Some("001-graph"), None);
    assert!(began.elapsed() < std::time::Duration::from_secs(1));
    let kept = again
        .views
        .iter()
        .find(|v| v.view.name == "findings" && v.extension == "threatspec-like")
        .unwrap();
    assert_eq!(kept.result.as_ref().unwrap().ran_at, result.ran_at);
    // Run again: anew.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let anew = views::views(
        r.path(),
        state.path(),
        Some("001-graph"),
        Some(("threatspec-like", "findings")),
    );
    let fresh = anew
        .views
        .iter()
        .find(|v| v.view.name == "findings" && v.extension == "threatspec-like")
        .unwrap();
    assert_ne!(fresh.result.as_ref().unwrap().ran_at, result.ran_at);
    // The project's view.
    let project = views::views(r.path(), state.path(), None, None);
    let counts = &project.views[0];
    match counts.result.as_ref().unwrap().data.as_ref().unwrap() {
        ViewData::Metrics { metrics } => {
            assert!(metrics.contains(&("tasks".to_owned(), "3".to_owned())));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_view_whose_exit_is_not_listed_shows_why() {
    if !sandboxed() {
        return;
    }
    let r = repo(&["kitchen"], &[]);
    let toml = r
        .path()
        .join(".speccoder/extensions/kitchen/dev.speccoder/extension.toml");
    let text = fs::read_to_string(&toml)
        .unwrap()
        .replace("exit = [0, 1]", "exit = [0]");
    fs::write(&toml, text).unwrap();
    commit(r.path());
    let state = tempfile::tempdir().unwrap();
    trust_now(r.path(), state.path(), "kitchen");
    let shown = views::views(r.path(), state.path(), Some("001-graph"), None);
    let f = shown
        .views
        .iter()
        .find(|v| v.view.name == "findings")
        .unwrap();
    let res = f.result.as_ref().unwrap();
    assert!(res.data.is_none());
    assert!(
        res.error.as_deref().unwrap().starts_with("it exited 1"),
        "{res:?}"
    );
}

// ── US4: skills and MCP servers from any Agent Plugins folder ──────────

#[test]
fn a_plugin_made_elsewhere_offers_its_skill_and_server_once_trusted() {
    let r = repo(&["plain-plugin"], &[]);
    let state = tempfile::tempdir().unwrap();
    let planning = speccoder_runtime::format::reference().planning();
    let before = speccoder_runtime::skills::discover_trusted(r.path(), planning, state.path());
    assert!(!before.iter().any(|s| s.name == "plain-plugin:summarise"));
    assert!(extensions::servers(r.path(), state.path()).is_empty());
    trust_now(r.path(), state.path(), "plain-plugin");
    let after = speccoder_runtime::skills::discover_trusted(r.path(), planning, state.path());
    let s = after
        .iter()
        .find(|s| s.name == "plain-plugin:summarise")
        .unwrap();
    assert_eq!(s.description, "Summarise the spec in three sentences.");
    assert_eq!(
        s.path,
        ".speccoder/extensions/plain-plugin/skills/summarise/SKILL.md"
    );
    let servers = extensions::servers(r.path(), state.path());
    assert_eq!(servers[0].name, "plain_plugin_notes");
    assert!(servers[0].command[1].ends_with(".speccoder/extensions/plain-plugin/bin/notes.py"));
    assert!(servers[0].data.as_ref().unwrap().is_dir());
}

#[test]
fn a_trusted_plugins_mcp_tool_is_offered_and_its_call_asks() {
    if !sandboxed() {
        return;
    }
    let r = repo(&["plain-plugin"], &[]);
    let state = tempfile::tempdir().unwrap();
    trust_now(r.path(), state.path(), "plain-plugin");
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "mcp__plain_plugin_notes__add_note",
        json!({ "text": "x" }),
    );
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_with(r.path(), None, "T005", state.path(), d, None);
    let asked = f.asked();
    assert!(!asked.is_empty(), "{}", res.message());
    let offered: Vec<String> = asked[0].tools.iter().map(|t| t.name.clone()).collect();
    assert!(
        offered.contains(&"mcp__plain_plugin_notes__add_note".to_owned()),
        "{offered:?} {} {:?}",
        res.message(),
        res.data()
    );
    // Not allowed by any rule: it waits for a person.
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
}

#[test]
fn a_remote_server_or_a_placeholder_in_command_is_named() {
    let r = repo(&["plain-plugin"], &[]);
    let mcp = r.path().join(".speccoder/extensions/plain-plugin/mcp.json");
    for (body, words) in [
        (
            r#"{"mcpServers": {"web": {"type": "streamable-http", "url": "https://x.example"}}}"#,
            "not supported by this version",
        ),
        (
            r#"{"mcpServers": {"x": {"type": "stdio", "command": "${PLUGIN_ROOT}/bin/x"}}}"#,
            "bare program name",
        ),
        (
            r#"{"mcpServers": {"x": {"type": "stdio", "command": "x", "args": ["${HOME}"]}}}"#,
            "only ${PLUGIN_ROOT} and ${PLUGIN_DATA}",
        ),
    ] {
        fs::write(&mcp, body).unwrap();
        commit(r.path());
        let e = &extensions::load(r.path())[0];
        assert!(
            e.problems.iter().any(|p| p.contains(words)),
            "{:?}",
            e.problems
        );
    }
}

// ── US5: Spec Kit extensions ───────────────────────────────────────────

/// A Spec Kit extension `ts` with one optional `before_implement` hook,
/// its command installed as a skill, and a gate of speccoder's own.
fn with_spec_kit_extension(r: &Path) {
    let ext = r.join(".specify/extensions/ts");
    fs::create_dir_all(ext.join("dev.speccoder")).unwrap();
    fs::write(
        ext.join("extension.yml"),
        "schema_version: \"1.0\"\nextension:\n  id: ts\n  version: \"0.2.1\"\n  description: \"Threat modelling\"\nprovides:\n  commands:\n    - name: speckit.ts.check\n      file: commands/check.md\nhooks:\n  before_implement:\n    command: speckit.ts.check\n    optional: true\n    prompt: \"Run the security gap check before implementing?\"\n",
    )
    .unwrap();
    fs::write(
        ext.join("dev.speccoder/extension.toml"),
        "[hooks.gate]\nat = \"tick\"\nargv = [\"true\"]\n",
    )
    .unwrap();
    fs::create_dir_all(r.join(".claude/skills/speckit-ts-check")).unwrap();
    fs::write(
        r.join(".claude/skills/speckit-ts-check/SKILL.md"),
        "---\nname: speckit-ts-check\ndescription: Check the threats.\n---\n\nCheck.\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec kit extension"]);
}

#[test]
fn a_spec_kit_extension_is_listed_trusted_and_offers_its_hook_at_its_step() {
    let r = repo(&[], &[]);
    with_spec_kit_extension(r.path());
    let state = tempfile::tempdir().unwrap();
    let all = extensions::load(r.path());
    let e = all.iter().find(|e| e.name == "ts").unwrap();
    assert_eq!(
        e.source,
        speccoder_core::extensions::ExtensionSource::SpecKit
    );
    assert_eq!(e.version.as_deref(), Some("0.2.1"));
    // speccoder's part beside extension.yml: a gate, named for the extension.
    assert_eq!(e.hooks[0].name, "ts.gate");
    assert!(
        e.contributions()
            .iter()
            .any(|c| c.line == "offers speckit.ts.check at before_implement"),
        "{:?}",
        e.contributions()
    );
    // Offered at its step once trusted: crates/speccoder/tests/extensions.rs.
    let _ = state;
}
