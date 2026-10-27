//! Try the sample: the bundled recording replays strictly with
//! this build (when the build has it), and the mechanics — the copy in the
//! state directory, strict replay per run, a divergence reported as one —
//! on a sample made here from a scripted run. That sample is built for the
//! test in a temporary directory and never bundled: the bundled sample is
//! a real-model recording (`just record-sample`), or there is none.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::json;
use speccoder_core::{CommandData, CommandResult, CommandStatus, SampleReport};
use speccoder_runtime::RunOptions;
use speccoder_runtime::sample::{Asset, open_from};

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

/// A tiny project and one scripted run of a hosted skill in it: the
/// shape of a recorded sample (bundle, journal, manifest).
fn scripted_sample() -> Asset {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path().join("wordcount");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    fs::create_dir_all(&r).unwrap();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(&r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n",
    )
    .unwrap();
    let skill = r.join(".claude/skills/speckit-specify");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: speckit-specify\ndescription: specify.\n---\n\n$ARGUMENTS\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn count() {}\n").unwrap();
    git(&r, &["init", "-q"]);
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "wordcount"]);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_call_tool(
        "w",
        "write_file",
        json!({ "path": "specs/002-words/spec.md", "content": "# Spec\n\nCount words.\n" }),
    );
    f.will_structure(json!({ "summary": "wrote it", "status": "done", "files_written": ["specs/002-words/spec.md"], "questions": [] }));
    let d: Arc<dyn ModelProvider> = f;
    let res = speccoder_runtime::hosting::run_with(
        &r,
        "specify",
        "Count words",
        state.path(),
        d,
        None,
        &RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Skill(k)) = res.data() else {
        panic!("{}", res.message())
    };
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "the recorded run"]);
    let bundle = dir.path().join("sample.bundle");
    git(
        &r,
        &["bundle", "create", &bundle.display().to_string(), "--all"],
    );
    let journal = speccoder_runtime::worktree::project_state_dir(state.path(), &r)
        .join(speccoder_runtime::substrate::JOURNAL);
    Asset {
        bundle: fs::read(&bundle).unwrap(),
        journal: fs::read(&journal).unwrap(),
        manifest: format!(
            "case = \"wordcount\"\nrun_ids = [\"{}\"]\nprovider = \"openai\"\nmodel = \"scripted\"\nagentplane = \"0.54.0\"\nspeccoder = \"{}\"\nrecorded_at = \"2026-10-27\"\nexpect = \"reproduced\"\n",
            k.run_id,
            env!("CARGO_PKG_VERSION")
        ),
    }
}

fn report(r: &CommandResult) -> &SampleReport {
    match r.data() {
        Some(CommandData::Sample(s)) => s,
        other => panic!("{} → {other:?}", r.message()),
    }
}

#[test]
fn a_build_without_the_recording_says_so_and_with_it_the_recording_reproduces() {
    let state = tempfile::tempdir().unwrap();
    let res = speccoder_runtime::sample::open(state.path(), false);
    if speccoder_runtime::sample::available() {
        // The bundled recording, replayed strictly by this build (no
        // model, no key): an agentplane upgrade that cannot reproduce it
        // fails here, before a release.
        let began = std::time::Instant::now();
        assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
        assert!(report(&res).runs.iter().all(|r| r.result == "reproduced"));
        assert!(began.elapsed() < std::time::Duration::from_secs(10));
    } else {
        eprintln!("no recorded sample in this build: `just record-sample` makes one");
        assert_eq!(res.status(), CommandStatus::NotFound);
        assert_eq!(res.code().map(|c| c.name), Some("SAMPLE-MISSING"));
        assert!(res.data().is_none(), "never an empty playback");
        assert!(!state.path().join("sample").exists());
    }
}

#[test]
fn a_sample_is_copied_into_the_state_directory_and_replayed_strictly() {
    let asset = scripted_sample();
    let state = tempfile::tempdir().unwrap();
    let res = open_from(&asset, state.path(), false);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let r = report(&res);
    assert!(r.fresh);
    assert_eq!(r.runs.len(), 1);
    assert_eq!(r.runs[0].result, "reproduced", "{:?}", r.runs[0].finding);
    let copy = PathBuf::from(&r.path);
    assert!(
        copy.starts_with(fs::canonicalize(state.path()).unwrap()) || copy.starts_with(state.path())
    );
    assert!(copy.join("specs/002-words/spec.md").is_file());
    // Opened again: the copy is kept as it is.
    fs::write(copy.join("mine.txt"), "a change of mine\n").unwrap();
    let again = open_from(&asset, state.path(), false);
    assert!(!report(&again).fresh);
    assert!(copy.join("mine.txt").exists());
    // Fresh: replaced.
    let fresh = open_from(&asset, state.path(), true);
    assert!(report(&fresh).fresh);
    assert!(!copy.join("mine.txt").exists());
}

#[test]
fn a_journal_this_build_cannot_reproduce_is_reported_never_shown_as_working() {
    let mut asset = scripted_sample();
    // Fault injection: the manifest names a run the journal does not hold.
    asset.manifest = asset.manifest.replace("run_ids = [\"", "run_ids = [\"x");
    let state = tempfile::tempdir().unwrap();
    let res = open_from(&asset, state.path(), false);
    assert_eq!(
        res.status(),
        CommandStatus::ReplayDiverged,
        "{}",
        res.message()
    );
    assert_eq!(res.code().map(|c| c.name), Some("SAMPLE-DIVERGED"));
    assert!(report(&res).runs.iter().all(|r| r.result != "reproduced"));
    // A journal this build cannot read (its header damaged).
    let mut broken = scripted_sample();
    for b in &mut broken.journal[..64] {
        *b ^= 0xff;
    }
    let state = tempfile::tempdir().unwrap();
    let res = open_from(&broken, state.path(), false);
    assert_eq!(
        res.code().map(|c| c.name),
        Some("SAMPLE-DIVERGED"),
        "{}",
        res.message()
    );
}

/// The recording's credential check reads the history, not the bundle: a
/// key committed once and deleted after is invisible in the compressed
/// pack and found in the history.
#[test]
fn the_credential_check_reads_the_history_not_the_compressed_bundle() {
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sample-credentials.sh");
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path().join("p");
    fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q"]);
    fs::write(r.join("a.txt"), "clean\n").unwrap();
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-qm", "clean"]);
    let journal = dir.path().join("journal.redb");
    fs::write(&journal, b"no secrets").unwrap();
    let check = || {
        Command::new("bash")
            .arg(&script)
            .arg(&r)
            .arg(&journal)
            .status()
            .unwrap()
            .success()
    };
    assert!(check(), "a clean sample passes");
    let key = format!("sk-ant-{}", "x".repeat(32));
    fs::write(r.join("config.env"), format!("KEY={key}\n")).unwrap();
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-qm", "oops"]);
    fs::remove_file(r.join("config.env")).unwrap();
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-qm", "removed"]);
    let bundle = dir.path().join("s.bundle");
    git(
        &r,
        &["bundle", "create", &bundle.display().to_string(), "--all"],
    );
    let raw = fs::read(&bundle).unwrap();
    assert!(
        !String::from_utf8_lossy(&raw).contains(&key),
        "the pack is compressed: grep cannot see it"
    );
    assert!(!check(), "the history holds the key");
}

/// A recording that verified nothing is refused: the run must have ticked
/// a task `verified`, in its payload and in a tick's trailers, and the
/// refusal quotes the run's own message.
#[test]
fn a_recording_without_a_verified_tick_is_refused_with_the_run_s_words() {
    // The recording itself needs `jq` (record-sample checks for it).
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("skipped: needs `jq`");
        return;
    }
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sample-verified.sh");
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path().join("p");
    fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q"]);
    fs::write(r.join("a.txt"), "x\n").unwrap();
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-qm", "plan"]);
    let run = dir.path().join("run.json");
    let check = || {
        let out = Command::new("bash")
            .arg(&script)
            .arg(&run)
            .arg(&r)
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    // The empty recording: approved, then nothing to run.
    let empty = json!({
        "message": "approved 2 decision line(s) — no tasks yet: 001-x has no tasks.md",
        "code": "NO-TASKS",
        "fix": ["speccoder spec tasks"],
        "data": { "attempts": [] },
    });
    fs::write(&run, empty.to_string()).unwrap();
    let (ok, err) = check();
    assert!(!ok, "an empty recording passes");
    assert!(
        err.contains("ticked no task verified")
            && err.contains("no tasks yet: 001-x has no tasks.md")
            && err.contains("code: NO-TASKS")
            && err.contains("fix: speccoder spec tasks"),
        "{err}"
    );
    // Verified in the payload, but no tick in the history: refused.
    let ticked = json!({
        "message": "feature done",
        "data": { "attempts": [
            { "task": "T001", "ticked": true, "verdict": { "verdict": "regression_only" } },
            { "task": "T002", "ticked": true, "verdict": { "verdict": "verified" } },
        ] },
    });
    fs::write(&run, ticked.to_string()).unwrap();
    assert!(!check().0, "a payload alone is not a tick");
    // Both: accepted.
    fs::write(r.join("a.txt"), "y\n").unwrap();
    git(&r, &["add", "-A"]);
    git(
        &r,
        &[
            "commit",
            "-qm",
            "speccoder: T002 x\n\nSpec-Task: 001-x/T002\nVerdict: verified\n",
        ],
    );
    let (ok, err) = check();
    assert!(ok, "{err}");
    // Not a result at all: refused, its output quoted.
    fs::write(&run, "thread 'main' panicked").unwrap();
    let (ok, err) = check();
    assert!(
        !ok && err.contains("not a result: thread 'main' panicked"),
        "{err}"
    );
}
