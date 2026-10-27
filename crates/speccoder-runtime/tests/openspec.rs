//! An OpenSpec change run end to end with a scripted model.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::{RunOptions, run_feature_with, run_with};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n";

const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif grep -q \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const CHANGE: &str = "add-farewell-and-locale";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-openspec/tests/fixtures/1.14.3")
}

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

/// A fixture as a committed repository with code, a check and `accept.sh`.
fn repo(fixture: &str, extra: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixtures().join(fixture).join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    git(r, &["init", "-q"]);
    // speccoder's own commits (the archive) use the repository's identity;
    // a fresh CI runner has no global one.
    git(r, &["config", "user.name", "t"]);
    git(r, &["config", "user.email", "t@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

fn done(summary: &str) -> Value {
    json!({ "summary": summary, "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] })
}

fn add(f: &FakeProvider, id: &str, function: &str) {
    f.will_call_tool(
        id,
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": format!("pub fn ok() {{}}\npub fn {function}() {{}}") }),
    );
    f.will_structure(done(function));
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

const NO_ACCEPTANCE: &str = "\n[acceptance]\ngenerate = false\n";

#[test]
fn a_change_runs_as_a_chain_and_is_ticked_only_on_a_verdict() {
    let r = repo("sample", NO_ACCEPTANCE);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    add(&f, "c1", "empty_default");
    add(&f, "c2", "bye");
    add(&f, "c3", "documented");
    let user_tasks =
        fs::read_to_string(r.path().join(format!("openspec/changes/{CHANGE}/tasks.md"))).unwrap();

    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let fr = match res.data() {
        Some(CommandData::Feature(fr)) => fr,
        other => panic!("{other:?}"),
    };
    assert_eq!(fr.stop, "feature done: every task is ticked");
    let tasks: Vec<&str> = fr.attempts.iter().map(|a| a.task.as_str()).collect();
    assert_eq!(
        tasks,
        ["1.2", "2.1", "2.2"],
        "1.1 was ticked before the run"
    );
    assert!(fr.attempts.iter().all(|a| a.ticked && a.commit.is_some()));

    let wt = Path::new(&fr.attempts[2].worktree);
    let ticked =
        fs::read_to_string(wt.join(format!("openspec/changes/{CHANGE}/tasks.md"))).unwrap();
    for id in ["1.2", "2.1", "2.2"] {
        assert!(ticked.contains(&format!("- [x] {id} ")), "{ticked}");
    }
    // Each tick is one byte of the line, in the verdict's commit.
    assert_eq!(ticked.len(), user_tasks.len());
    let log = git(wt, &["log", "--format=%s", &format!("speccoder/{CHANGE}")]);
    assert!(
        log.lines().next().unwrap().starts_with("speccoder: 2.2 "),
        "{log}"
    );
    let body = git(
        wt,
        &["log", "-1", "--format=%b", &format!("speccoder/{CHANGE}")],
    );
    assert!(body.contains(&format!("Spec-Task: {CHANGE}/2.2")), "{body}");
    // The person's checkout is untouched.
    assert_eq!(
        fs::read_to_string(r.path().join(format!("openspec/changes/{CHANGE}/tasks.md"))).unwrap(),
        user_tasks
    );
    // The attempt saw the deltas, the proposal and the design, never the archive.
    let attempt = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(attempt.contains("# Spec deltas"), "{attempt}");
    assert!(attempt.contains("Configured default"));
    assert!(attempt.contains("Change proposal and design"));
    assert!(
        !attempt.contains("GitHub Spec Kit") && !attempt.contains("2026-10-23-add-greeting-cli")
    );
    assert!(
        attempt.contains("implementing exactly one OpenSpec task"),
        "the attempt policy names OpenSpec"
    );
}

#[test]
fn acceptance_tests_come_from_the_scenarios() {
    let r = repo("sample", "\n[acceptance]\nheld_out = false\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_1_2.txt", "content": "greeting / Default greeting / Configured default\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn empty_default"], "notes": [] }));
    add(&f, "c1", "empty_default");
    let res = run_with(r.path(), Some(CHANGE), "1.2", state.path(), d, None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert_eq!(rep.verdict, Some(Verdict::Verified));
    let author = serde_json::to_string(&f.asked()[0].prompt).unwrap();
    assert!(
        author.contains("GREET_DEFAULT=Grace"),
        "the GIVEN reaches the test author"
    );
    assert!(
        author.contains("the scenarios of the change's spec deltas"),
        "{author}"
    );
}

#[test]
fn two_active_changes_need_a_choice() {
    let r = repo("sample", NO_ACCEPTANCE);
    let other = r.path().join("openspec/changes/b-other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("tasks.md"), "## 1. X\n\n- [ ] 1.1 x\n").unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "second change"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(
        res.status(),
        CommandStatus::SelectionNeeded,
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty());
    let inspected = speccoder_runtime::inspect(r.path(), None);
    let Some(CommandData::Inspect(s)) = inspected.data() else {
        panic!()
    };
    let names: Vec<_> = s.features.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, [CHANGE, "b-other"]);
}

#[test]
fn a_broken_change_is_refused_before_any_model_call() {
    let r = repo("broken-example", NO_ACCEPTANCE);
    fs::write(
        r.path().join("openspec/changes/broken-example/tasks.md"),
        "## 1. Do\n\n- [ ] 1.1 do it\n",
    )
    .unwrap();
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-q", "-m", "tasks"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    let res = run_with(r.path(), None, "1.1", state.path(), d, None);
    assert_eq!(
        res.status(),
        CommandStatus::DiagnosticsError,
        "{}",
        res.message()
    );
    assert_eq!(
        res.message().split(':').next(),
        Some("change broken-example cannot run")
    );
    assert!(f.asked().is_empty(), "no model call");
    let Some(CommandData::Inspect(s)) = res.data() else {
        panic!()
    };
    let missing = s
        .diagnostics
        .iter()
        .find(|d| d.code == "missing_target")
        .unwrap();
    assert!(missing.message.contains("Does not exist"));
}

#[test]
fn intent_and_openspecs_rewriting_commands_are_out_of_reach() {
    let r = repo("sample", NO_ACCEPTANCE);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "openspec/specs/greeting/spec.md", "content": "x" }),
    );
    f.will_call_tool(
        "c2",
        "run_command",
        json!({ "argv": ["openspec", "archive", CHANGE, "--yes"] }),
    );
    f.will_call_tool(
        "c4",
        "run_command",
        json!({ "argv": ["openspec", "list", "--json"] }),
    );
    add(&f, "c5", "empty_default");
    let res = run_with(r.path(), None, "1.2", state.path(), d, None);
    let rep = report(&res);
    let decisions: Vec<(&str, &str)> = rep
        .tool_calls
        .iter()
        .map(|c| {
            (
                c["tool"].as_str().unwrap_or_default(),
                c["decision"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        decisions.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
        ["deny", "deny", "allow", "allow"],
        "{decisions:?}"
    );
    let reasons = serde_json::to_string(&rep.tool_calls).unwrap();
    assert!(reasons.contains("OpenSpec intent (openspec/)"), "{reasons}");
    assert!(reasons.contains("speccoder spec archive"), "{reasons}");
}

#[test]
fn convergence_appends_the_next_numbered_group() {
    let r = repo("sample", NO_ACCEPTANCE);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    add(&f, "c1", "empty_default");
    add(&f, "c2", "bye");
    add(&f, "c3", "documented");
    let res = run_feature_with(
        r.path(),
        None,
        state.path(),
        &d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    f.will_structure(json!({ "findings": [{ "source_ref": "greeting: Farewell", "gap_type": "missing", "severity": "HIGH", "description": "no farewell without a name (missing)", "evidence": "src/lib.rs", "task": "Print a farewell without a name." }], "checked": ["greeting: Farewell"] }));
    let res = speccoder_runtime::convergence::converge_with(
        r.path(),
        None,
        state.path(),
        Arc::clone(&d),
        None,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Converge(c)) = res.data() else {
        panic!("{res:?}")
    };
    assert_eq!(
        c.appended,
        ["3.1 Print a farewell without a name per greeting: Farewell (missing)"]
    );
    let wt = fs::canonicalize(state.path()).unwrap();
    let tasks = walk_find(&wt, &format!("openspec/changes/{CHANGE}/tasks.md"));
    let text = fs::read_to_string(tasks).unwrap();
    assert!(
        text.contains("\n## 3. Convergence\n\n- [ ] 3.1 Print a farewell without a name"),
        "{text}"
    );
    assert!(
        text.contains("## Workflow follow-up"),
        "the old content is kept"
    );
}

fn walk_find(dir: &Path, suffix: &str) -> PathBuf {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.to_string_lossy().ends_with(suffix) {
                return p;
            }
            if p.is_dir() && !p.ends_with(".git") {
                stack.push(p);
            }
        }
    }
    panic!("no {suffix} under {}", dir.display());
}

// ── US4: archive is the person's, diff first ───────────────────────────────

use speccoder_runtime::format::openspec::archive;

/// A fake `openspec` in its own directory: logs its argv to `argv.log`
/// and answers by `mode` (`incomplete` or `confirm`).
fn fake_openspec(mode: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let outputs = fixtures().join("outputs");
    let script = format!(
        r#"#!/bin/sh
here="{here}"
echo "$*" >> "$here/argv.log"
case "$1" in
  --version) echo 1.14.3 ;;
  validate) cat "{outputs}/validate-broken2.json" ;;
  instructions) cat "{outputs}/instructions-apply-change2.json" ;;
  show) echo "greeting: + Farewell, ~ Default greeting, Greet a named person -> Greet by name" ;;
  archive)
    case "$*" in
      *--yes*)
        mkdir -p openspec/changes/archive
        mv "openspec/changes/$2" "openspec/changes/archive/2026-10-23-$2"
        echo '{{"archive":{{"change":"'"$2"'","archivedAs":"2026-10-23-'"$2"'","specsUpdated":true,"totals":{{"added":1,"modified":1,"removed":0,"renamed":1}}}}}}' ;;
      *)
        if [ "{mode}" = incomplete ]; then cat "{outputs}/archive-incomplete.json"
        else echo '{{"archive":null,"status":[{{"severity":"error","code":"archive_confirmation_required","message":"Archive requires confirmation.","fix":"Rerun with --yes."}}]}}'
        fi ;;
    esac ;;
esac
"#,
        here = dir.path().display(),
        outputs = outputs.display(),
        mode = mode,
    );
    let p = dir.path().join("openspec");
    fs::write(&p, script).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    (dir, p)
}

fn argv_log(fake: &tempfile::TempDir) -> String {
    fs::read_to_string(fake.path().join("argv.log")).unwrap_or_default()
}

fn options<'a>(
    program: &Path,
    yes: bool,
    allow: &'a [String],
    confirm: Option<&'a dyn Fn(&speccoder_core::ArchiveReport) -> bool>,
) -> archive::Options<'a> {
    archive::Options {
        change: None,
        yes,
        allow_unverified: allow,
        confirm,
        program: Some(program.to_path_buf()),
    }
}

fn archived(r: &CommandResult) -> &speccoder_core::ArchiveReport {
    match r.data() {
        Some(CommandData::Archive(a)) => a,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[test]
fn a_tick_without_a_verdict_is_named_before_archiving() {
    let r = repo("sample", NO_ACCEPTANCE);
    let (fake, program) = fake_openspec("confirm");
    let res = archive::archive(r.path(), &options(&program, true, &[], None));
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(archived(&res).stage, "unverified");
    assert_eq!(archived(&res).unverified_tasks, ["1.1"]);
    assert!(res.message().contains("--allow-unverified"));
    assert_eq!(argv_log(&fake), "", "OpenSpec is not even asked");
    // Naming a different task does not do.
    let other = ["2.1".to_owned()];
    let res = archive::archive(r.path(), &options(&program, true, &other, None));
    assert_eq!(archived(&res).stage, "unverified");
}

#[test]
fn open_tasks_stop_the_archive_and_yes_is_never_sent_first() {
    let r = repo("sample", NO_ACCEPTANCE);
    let (fake, program) = fake_openspec("incomplete");
    let allow = ["1.1".to_owned()];
    let res = archive::archive(r.path(), &options(&program, true, &allow, None));
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(archived(&res).stage, "incomplete");
    assert!(res.message().contains("incomplete task(s)"));
    assert_eq!(argv_log(&fake).trim(), format!("archive {CHANGE} --json"));
}

#[test]
fn archiving_waits_for_a_yes_and_commits_once() {
    let r = repo("sample", NO_ACCEPTANCE);
    let (_fake, program) = fake_openspec("confirm");
    let allow = ["1.1".to_owned()];
    let head = git(r.path(), &["rev-parse", "HEAD"]);

    // Nobody to ask, no --yes: the diff and what to run next.
    let res = archive::archive(r.path(), &options(&program, false, &allow, None));
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let a = archived(&res);
    assert_eq!(a.stage, "awaiting_confirmation");
    assert!(a.diff.as_deref().unwrap().contains("Farewell"));
    let res = speccoder_runtime::next::attach(res, None);
    let json = serde_json::to_value(&res).unwrap();
    assert_eq!(
        json["next"][1]["command"],
        format!("speccoder spec archive {CHANGE} --yes")
    );
    // A person says no: nothing changes.
    let no = |_: &speccoder_core::ArchiveReport| false;
    let res = archive::archive(r.path(), &options(&program, false, &allow, Some(&no)));
    assert_eq!(res.status(), CommandStatus::Ok);
    assert_eq!(archived(&res).stage, "declined");
    assert_eq!(git(r.path(), &["status", "--porcelain"]), "");
    assert_eq!(git(r.path(), &["rev-parse", "HEAD"]), head);

    // Yes: one commit, the unverified tick recorded.
    let res = archive::archive(r.path(), &options(&program, true, &allow, None));
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let a = archived(&res);
    assert_eq!(a.stage, "archived");
    assert_eq!(
        a.archived_as.as_deref(),
        Some("2026-10-23-add-farewell-and-locale")
    );
    let count = git(
        r.path(),
        &["rev-list", "--count", &format!("{}..HEAD", head.trim())],
    );
    assert_eq!(count.trim(), "1");
    let body = git(r.path(), &["log", "-1", "--format=%B"]);
    assert!(body.starts_with(&format!("archive {CHANGE}")), "{body}");
    assert!(body.contains("Archived-Unverified: 1.1"), "{body}");
    assert!(
        body.contains("Speccoder-Archive: 2026-10-23-add-farewell-and-locale"),
        "{body}"
    );
    assert_eq!(git(r.path(), &["status", "--porcelain"]), "");
}

#[test]
fn a_dirty_openspec_directory_is_refused() {
    let r = repo("sample", NO_ACCEPTANCE);
    let (_fake, program) = fake_openspec("confirm");
    fs::write(
        r.path()
            .join("openspec/changes/add-farewell-and-locale/notes.md"),
        "x",
    )
    .unwrap();
    let res = archive::archive(
        r.path(),
        &options(&program, true, &["1.1".to_owned()], None),
    );
    assert_eq!(
        res.status(),
        CommandStatus::GitPrecondition,
        "{}",
        res.message()
    );
}

#[test]
fn openspecs_own_cli_is_a_second_opinion() {
    let r = repo("sample", NO_ACCEPTANCE);
    let (fake, program) = fake_openspec("confirm");
    let inspected = speccoder_runtime::inspect(r.path(), None);
    let Some(CommandData::Inspect(s)) = inspected.data() else {
        panic!()
    };
    let runner = speccoder_runtime::verify::process::LocalRunner;
    let d = speccoder_runtime::format::openspec::cli::crosscheck_with(s, &runner, Some(program));
    // The recorded `validate` of the broken example says archive would refuse.
    assert!(
        d.iter().any(|d| d.code == "crosscheck_error"
            && d.severity == speccoder_core::snapshot::Severity::Error),
        "{d:?}"
    );
    let log = argv_log(&fake);
    assert!(
        log.contains(&format!("validate {CHANGE} --strict --json")),
        "{log}"
    );
    let d = speccoder_runtime::format::openspec::cli::crosscheck_with(s, &runner, None);
    assert_eq!(d[0].code, "crosscheck_skipped");
}

// ── US6: OpenSpec's planning skills, hosted ────────────────────────────────

#[test]
fn openspecs_planning_skills_are_hosted_and_its_apply_is_refused() {
    let r = repo("sample", NO_ACCEPTANCE);
    let listed = speccoder_runtime::hosting::list(r.path());
    let Some(CommandData::Skills(k)) = listed.data() else {
        panic!("{listed:?}")
    };
    let hosted: Vec<&str> = k
        .skills
        .iter()
        .filter(|s| s.refused.is_none())
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        hosted,
        [
            "openspec-explore",
            "openspec-propose",
            "openspec-update-change"
        ]
    );
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    for name in ["apply-change", "archive-change", "sync-specs"] {
        let res = speccoder_runtime::hosting::run_with(
            r.path(),
            name,
            "",
            state.path(),
            Arc::clone(&d),
            None,
            &RunOptions::default(),
        );
        assert_eq!(
            res.status(),
            CommandStatus::UsageError,
            "{name}: {}",
            res.message()
        );
    }
    assert!(f.asked().is_empty());
}

#[test]
fn a_skill_that_stops_incomplete_says_why() {
    let r = repo("sample", NO_ACCEPTANCE);
    let state = tempfile::tempdir().unwrap();
    for (summary, says) in [
        (
            "the design needs a decision on the storage",
            "the skill says: the design needs a decision on the storage",
        ),
        (
            "",
            "the skill ended without saying why — run it again (speccoder spec explore)",
        ),
    ] {
        let (f, d) = fake();
        f.will_structure(json!({ "summary": summary, "status": "incomplete", "files_written": [], "questions": [] }));
        let res = speccoder_runtime::hosting::run_with(
            r.path(),
            "explore",
            "storage",
            state.path(),
            Arc::clone(&d),
            None,
            &RunOptions::default(),
        );
        assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
        assert!(res.message().ends_with(says), "{}", res.message());
    }
}
