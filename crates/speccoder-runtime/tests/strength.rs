//! Acceptance strength end to end with a scripted model: wrong and
//! permitted variants of a verified change are run against its acceptance
//! tests; what gets through is named, an over-strict test holds the attempt
//! for a person's ruling, and `min_strength` strengthens the set once.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::strength::{Gate, Outcome, PermittedOutcome, State};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[commands]\nallow_prefix = [[\"grep\", \"-q\"], [\"./accept.sh\", \"--\"]]\n\n[acceptance]\nheld_out = false\n";

/// One test: "the pattern is in src/lib.rs", with libtest's summary line.
// A runner: the file a command names (its second argument) must exist, or
// it collects nothing — so a held-out file is red only by what it adds.
const ACCEPT_SH: &str = "#!/bin/sh\n[ \"$1\" = -- ] && shift\nif [ -n \"$2\" ] && [ ! -f \"$2\" ]; then\n  echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 0\nfi\nif grep -qF \"$1\" src/lib.rs; then\n  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'\nelse\n  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n  exit 101\nfi\n";

const SPEC: &str = "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n**Acceptance**: a `login` function exists and does not panic.\n";

const FINAL: &str = "pub fn ok() {}\npub fn login() {}\n";
/// Wrong: no login at all — the set rejects it.
const MISSING: &str = "pub fn ok() {}\n";
/// Wrong: login panics — `fn login` is still there, so the thin set lets it through.
const PANICS: &str = "pub fn ok() {}\npub fn login() { panic!(\"nope\") }\n";
/// Claimed permitted: another name — the set rejects it on an assertion.
const RENAMED: &str = "pub fn ok() {}\npub fn sign_in() {}\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Dana Example",
            "-c",
            "user.email=d@example.com",
        ])
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
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo(extra: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::write(r.join("specs/001-graph/plan.md"), "# Plan\n\nRust.\n").unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), format!("{CONFIG}{extra}")).unwrap();
    fs::write(r.join("accept.sh"), ACCEPT_SH).unwrap();
    fs::set_permissions(r.join("accept.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.name", "Dana Example"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn fake() -> (Arc<FakeProvider>, Arc<dyn ModelProvider>) {
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    (f, d)
}

/// The visible set: only "`fn login` exists".
fn thin() -> Value {
    json!({ "files": [{ "path": "tests/acceptance_login.txt", "content": "login exists\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn login"], "notes": [] })
}

fn implement(f: &FakeProvider) {
    f.will_call_tool(
        "c1",
        "write_file",
        json!({ "path": "src/lib.rs", "content": FINAL }),
    );
    f.will_structure(json!({ "summary": "done", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
}

fn variant(kind: &str, scenario: &str, rationale: &str, lib: &str) -> Value {
    json!({ "kind": kind, "scenario": scenario, "rationale": rationale, "files": [{ "path": "src/lib.rs", "content": lib }] })
}

fn variants(list: &[Value]) -> Value {
    json!({ "variants": list, "notes": [] })
}

fn run(r: &Path, state: &Path, d: Arc<dyn ModelProvider>) -> CommandResult {
    speccoder_runtime::run_with(r, None, "T005", state, d, None)
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

/// Every request the implementer got (the calls with tools).
fn implementer_requests(f: &FakeProvider) -> Vec<String> {
    f.asked()
        .iter()
        .filter(|a| !a.tools.is_empty())
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .collect()
}

fn tip_message(r: &Path) -> String {
    git(r, &["log", "-1", "--format=%B", "speccoder/001-graph"])
}

#[test]
fn a_thin_set_is_measured_and_what_got_through_is_named() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[
        variant("wrong", "US1-1", "no login at all", MISSING),
        variant("wrong", "US1-1", "login panics", PANICS),
        variant("permitted", "US1-1", "nothing is open", FINAL),
    ]));

    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked);
    let st = rep.strength.as_ref().expect("strength");
    assert_eq!(st.combined.state, State::Measured);
    assert_eq!(st.combined.share, Some(0.5));
    assert_eq!(st.near_misses[0].combined, Outcome::Rejected);
    assert_eq!(st.near_misses[1].combined, Outcome::Survived);
    assert!(
        st.combined.survivors[0].starts_with("near-miss-2 (US1-1: login panics)"),
        "{:?}",
        st.combined.survivors
    );
    // A permitted variant identical to the change is dropped, not run.
    assert!(st.permitted.is_empty());
    assert!(
        st.notes
            .iter()
            .any(|n| n.contains("identical to the change")),
        "{:?}",
        st.notes
    );
    assert!(
        st.notes.iter().any(|n| n.contains("no [mutation] tool")),
        "{:?}",
        st.notes
    );
    assert_eq!(st.gate, Gate::Off);
    // The tick says so, `report` reads it back, the attestation carries it.
    let msg = tip_message(r.path());
    assert!(
        msg.contains("Acceptance-Strength: 0.50 near-miss=1/2\n"),
        "{msg}"
    );
    let status = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(s)) = status.data() else {
        panic!()
    };
    let t = s.tasks.iter().find(|t| t.id == "T005").unwrap();
    assert_eq!(t.strength.as_deref(), Some("0.50 near-miss=1/2"));
    // The survivor's patch is kept for a person; the worktree is as the change left it.
    let keep = speccoder_runtime::strength::dir(
        &speccoder_runtime::worktree::project_state_dir(
            state.path(),
            &fs::canonicalize(r.path()).unwrap(),
        ),
        "001-graph",
        "T005",
    );
    assert!(
        fs::read_to_string(keep.join("near-miss-2.patch"))
            .unwrap()
            .contains("panic!")
    );
    // The variant author never saw a test.
    let author = f
        .asked()
        .iter()
        .map(|a| serde_json::to_string(&a.prompt).unwrap())
        .find(|p| p.contains("help test the acceptance tests"))
        .expect("the near-miss request");
    assert!(
        !author.contains("login exists") && !author.contains("acceptance_login"),
        "{author}"
    );
    // The implementer never saw a variant.
    for req in implementer_requests(&f) {
        assert!(!req.contains("nope") && !req.contains("near-miss"), "{req}");
    }
}

#[test]
fn an_over_strict_test_holds_the_attempt_and_a_ruling_ticks_it_without_a_model_call() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[
        variant("wrong", "US1-1", "no login at all", MISSING),
        variant(
            "permitted",
            "US1-1",
            "the spec does not fix the name",
            RENAMED,
        ),
    ]));

    let res = run(r.path(), state.path(), Arc::clone(&d));
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let rep = report(&res);
    assert!(!rep.ticked);
    assert_eq!(rep.attempt_status, "over_strict");
    assert!(
        res.message().contains("task rule T005 --tests-right"),
        "{}",
        res.message()
    );
    let st = rep.strength.as_ref().unwrap();
    assert_eq!(st.permitted[0].outcome, PermittedOutcome::Rejected);
    assert!(
        st.permitted[0]
            .patch
            .as_deref()
            .is_some_and(|p| Path::new(p).exists())
    );
    // Held: the worktree keeps the change, another task waits.
    let wt = PathBuf::from(&rep.worktree);
    assert_eq!(fs::read_to_string(wt.join("src/lib.rs")).unwrap(), FINAL);
    let other =
        speccoder_runtime::run_with(r.path(), None, "T006", state.path(), Arc::clone(&d), None);
    assert_eq!(other.status(), CommandStatus::NeedsHuman);
    assert!(
        other.message().contains("T005's acceptance tests reject"),
        "{}",
        other.message()
    );
    let asked_before = f.asked().len();

    // The person rules: the tests are right.
    let ruled = speccoder_runtime::controls::rule(
        r.path(),
        None,
        "T005",
        "the spec names it login",
        None,
        state.path(),
    );
    assert_eq!(ruled.status(), CommandStatus::Ok, "{}", ruled.message());

    // The next run judges the held attempt again from the journal: no model call.
    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked);
    assert_eq!(f.asked().len(), asked_before, "a model call was repeated");
    let st = rep.strength.as_ref().unwrap();
    assert_eq!(st.permitted[0].outcome, PermittedOutcome::Ruled);
    assert_eq!(st.rulings[0].by, "Dana Example");
    let msg = tip_message(r.path());
    assert!(msg.contains("Ruling: tests-right US1-1 sha256:"), "{msg}");
    assert!(
        msg.contains("by Dana Example — the spec names it login"),
        "{msg}"
    );
}

#[test]
fn fresh_acceptance_sets_a_held_attempt_aside() {
    let r = repo("");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[variant(
        "permitted",
        "US1-1",
        "another name",
        RENAMED,
    )]));
    let res = run(r.path(), state.path(), Arc::clone(&d));
    assert_eq!(report(&res).attempt_status, "over_strict");
    // Fresh tests that allow either name, a fresh attempt. The variants are
    // read back: the same spec and change get the same ones.
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_login.txt", "content": "login or sign_in exists\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "pub fn login\npub fn sign_in"], "notes": [] }));
    implement(&f);
    let options = speccoder_runtime::RunOptions {
        fresh_acceptance: true,
        ..speccoder_runtime::RunOptions::default()
    };
    let res = speccoder_runtime::run_with_options(
        r.path(),
        None,
        "T005",
        state.path(),
        d,
        None,
        &options,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked && rep.set_aside.is_some(), "{rep:?}");
    let st = rep.strength.as_ref().unwrap();
    assert_eq!(st.permitted[0].outcome, PermittedOutcome::Accepted);
}

#[test]
fn min_strength_strengthens_the_set_once() {
    let r = repo("min_strength = 1.0\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[variant(
        "wrong",
        "US1-1",
        "login panics",
        PANICS,
    )]));
    // Told what got through, the author adds a test that rejects it.
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_login_body.txt", "content": "login has an empty body\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn login() {}"], "notes": [] }));

    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    assert!(rep.ticked);
    let st = rep.strength.as_ref().unwrap();
    assert_eq!(st.gate, Gate::RegeneratedMet);
    assert_eq!(st.combined.share, Some(1.0));
    // The strengthening author was told the survivor.
    let told = f.asked().iter().any(|a| {
        serde_json::to_string(&a.prompt)
            .unwrap()
            .contains("login panics")
    });
    assert!(told);
    let files = git(
        r.path(),
        &["show", "--name-only", "--format=", "speccoder/001-graph"],
    );
    assert!(files.contains("tests/acceptance_login_body.txt"), "{files}");
}

#[test]
fn a_set_still_too_weak_stops_for_a_person() {
    let r = repo("min_strength = 1.0\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[variant(
        "wrong",
        "US1-1",
        "login panics",
        PANICS,
    )]));
    // An added test that passes before the change is no evidence.
    f.will_structure(json!({ "files": [{ "path": "tests/acceptance_more.txt", "content": "ok exists\n", "scenarios": [] }], "argv": ["./accept.sh", "--", "fn ok"], "notes": [] }));

    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    let res = speccoder_runtime::next::attach(res, None);
    assert_eq!(res.code().map(|e| e.name), Some("ACCEPTANCE-WEAK"));
    let rep = report(&res);
    assert!(!rep.ticked);
    assert!(
        rep.reason
            .as_deref()
            .unwrap_or_default()
            .contains("acceptance set too weak")
    );
    assert!(
        rep.reason
            .as_deref()
            .unwrap_or_default()
            .contains("do not fail without the change"),
        "{:?}",
        rep.reason
    );
    // No implementer retry: one attempt, its two turns.
    assert_eq!(implementer_requests(&f).len(), 2);
}

#[test]
fn strength_can_be_switched_off() {
    let r = repo("strength = false\n");
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(report(&res).strength.is_none());
    assert!(!tip_message(r.path()).contains("Acceptance-Strength"));
}

#[test]
fn held_out_tests_count_in_the_combined_strength() {
    // Held-out acceptance on (the default): the CONFIG above turns it off.
    let r = repo("");
    let toml = fs::read_to_string(r.path().join("speccoder.toml"))
        .unwrap()
        .replace("held_out = false\n", "");
    fs::write(r.path().join("speccoder.toml"), toml).unwrap();
    git(r.path(), &["commit", "-qam", "held-out on"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    f.will_structure(json!({ "files": [{
        "path": "tests/held_out_login.txt",
        "content": "login has an empty body\n",
        "scenarios": ["US1-1"],
        "argv": ["./accept.sh", "--", "fn login() {}", "tests/held_out_login.txt"],
        "composition": false,
    }], "notes": [] }));
    implement(&f);
    f.will_structure(variants(&[variant(
        "wrong",
        "US1-1",
        "login panics",
        PANICS,
    )]));

    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let st = report(&res).strength.clone().unwrap();
    assert!(st.held_out);
    assert_eq!(st.visible.share, Some(0.0));
    assert_eq!(st.combined.share, Some(1.0));
    let msg = tip_message(r.path());
    assert!(msg.contains("Acceptance-Strength: 1.00 near-miss=1/1\nAcceptance-Strength-Visible: 0.00 near-miss=0/1\n"), "{msg}");
    // The held-out file was overlaid only while judging: never committed.
    let files = git(
        r.path(),
        &["show", "--name-only", "--format=", "speccoder/001-graph"],
    );
    assert!(!files.contains("held_out_login"), "{files}");
}

#[test]
fn mutants_are_judged_by_the_acceptance_command_alone() {
    let tool = "[mutation]\ntool = \"command\"\nsuite = false\nreport = \"{out}/r.json\"\nargv = [\"true\"]\nacceptance_argv = [\"sh\", \"-c\", \"mkdir -p \\\"$1\\\" && printf '%s' '{\\\"files\\\":{\\\"src/lib.rs\\\":{\\\"mutants\\\":[{\\\"status\\\":\\\"Killed\\\"},{\\\"status\\\":\\\"Killed\\\"},{\\\"status\\\":\\\"Killed\\\"},{\\\"status\\\":\\\"Survived\\\",\\\"mutatorName\\\":\\\"BlockStatement\\\",\\\"replacement\\\":\\\"{}\\\",\\\"location\\\":{\\\"start\\\":{\\\"line\\\":2}}}]}}}' > \\\"$1/r.json\\\"\", \"sh\", \"{out}\", \"{tests}\"]\n";
    let r = repo("");
    fs::write(
        r.path().join("speccoder.toml"),
        format!(
            "{}{tool}",
            fs::read_to_string(r.path().join("speccoder.toml")).unwrap()
        ),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "mutation"]);
    let state = tempfile::tempdir().unwrap();
    let (f, d) = fake();
    f.will_structure(thin());
    implement(&f);
    f.will_structure(variants(&[variant(
        "wrong",
        "US1-1",
        "no login at all",
        MISSING,
    )]));
    let res = run(r.path(), state.path(), d);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let rep = report(&res);
    // suite = false: no whole-suite run, only acceptance strength.
    assert!(rep.mutation.is_none());
    let st = rep.strength.as_ref().unwrap();
    let m = st.mutants.as_ref().unwrap();
    assert_eq!((m.caught, m.missed), (3, 1));
    assert_eq!(st.combined.share, Some(0.8));
    assert!(
        tip_message(r.path()).contains("Acceptance-Strength: 0.80 mutants=3/4 near-miss=1/1\n")
    );
}
