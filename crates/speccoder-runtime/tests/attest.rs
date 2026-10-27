//! Evidence attestations: every tick leaves signed in-toto
//! statements in a git note, and `attest verify` catches every tampering.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport, VerifyOutcome};
use speccoder_runtime::attest::verify::{Trust, verify_at};

const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n\n[acceptance]\ngenerate = false\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
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
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn has(program: &str) -> bool {
    Command::new(program).arg("-?").output().is_ok()
}

/// The graph fixture, committed; with `key` set up as git's SSH signing key.
fn repo(key: Option<&Path>) -> tempfile::TempDir {
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
    fs::write(
        r.join("specs/001-graph/spec.md"),
        "# Spec\n\n### User Story 1 - Login\n\nUsers log in.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST log out.\n",
    )
    .unwrap();
    let tasks = r.join("specs/001-graph/tasks.md");
    let text = fs::read_to_string(&tasks).unwrap();
    fs::write(
        &tasks,
        text.replace(
            "Integration test for login",
            "Integration test for login (FR-001)",
        ),
    )
    .unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(r.join("speccoder.toml"), CONFIG).unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    if let Some(key) = key {
        git(r, &["config", "gpg.format", "ssh"]);
        git(
            r,
            &["config", "user.signingKey", &key.display().to_string()],
        );
    }
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

/// A throwaway ed25519 key and an allowed-signers file trusting it.
fn key(dir: &Path) -> (PathBuf, PathBuf) {
    let key = dir.join("key");
    assert!(
        Command::new("ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                "ada@example.com",
                "-f"
            ])
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    let public = fs::read_to_string(dir.join("key.pub")).unwrap();
    let allowed = dir.join("allowed_signers");
    fs::write(
        &allowed,
        format!("ada@example.com namespaces=\"in-toto\" {public}"),
    )
    .unwrap();
    (key, allowed)
}

fn passes(r: &Path, state: &Path) -> CommandResult {
    let f = FakeProvider::new();
    f.will_call_tool(
        "c1",
        "edit_file",
        json!({ "path": "src/lib.rs", "old_string": "pub fn ok() {}", "new_string": "pub fn ok() {}\npub fn login() {}" }),
    );
    f.will_structure(json!({ "summary": "login", "status": "done", "files_changed": ["src/lib.rs"], "checks_run": [], "notes": [] }));
    let d: Arc<dyn ModelProvider> = f;
    speccoder_runtime::run_with(r, None, "T005", state, d, None)
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn outcome(res: &CommandResult) -> VerifyOutcome {
    match res.data() {
        Some(CommandData::Attest(a)) => a.objects[0].outcome,
        other => panic!("{} → {other:?}", res.message()),
    }
}

fn trust(allowed: &Path) -> Trust {
    Trust {
        allowed_signers: Some(allowed.to_path_buf()),
        ..Trust::default()
    }
}

fn note(r: &Path, commit: &str) -> String {
    git(r, &["notes", "--ref=speccoder", "show", commit])
}

fn set_note(r: &Path, commit: &str, text: &str) {
    let file = r.join(".git/note-under-test");
    fs::write(&file, text).unwrap();
    git(
        r,
        &[
            "notes",
            "--ref=speccoder",
            "add",
            "-f",
            "-F",
            &file.display().to_string(),
            commit,
        ],
    );
}

fn statements(note: &str) -> Vec<Value> {
    note.lines()
        .map(|l| {
            let e: Value = serde_json::from_str(l).unwrap();
            serde_json::from_slice(&STANDARD.decode(e["payload"].as_str().unwrap()).unwrap())
                .unwrap()
        })
        .collect()
}

#[test]
fn a_tick_leaves_signed_statements_that_verify() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let rep = report(&res);
    assert!(rep.ticked, "{}", res.message());
    let att = rep.attestation.as_ref().unwrap();
    assert_eq!(att["state"], "signed", "{att}");
    assert_eq!(att["signer"], "ssh");

    // The note holds the statements, each about the tick commit.
    let commit = rep.commit.clone().unwrap();
    let note_text = note(r.path(), &commit);
    let all = statements(&note_text);
    let types: Vec<&str> = all
        .iter()
        .map(|s| s["predicateType"].as_str().unwrap())
        .collect();
    assert!(
        types.contains(&"https://in-toto.io/attestation/test-result/v0.1"),
        "{types:?}"
    );
    assert!(types.contains(&"https://slsa.dev/verification_summary/v1"));
    assert!(types.contains(&"https://speccoder.dev/task-verification/v0"));
    // The trace names the requirement the task cites, bound to its line.
    let verification = all
        .iter()
        .find(|s| s["predicateType"] == "https://speccoder.dev/task-verification/v0")
        .unwrap();
    let trace = verification["predicate"]["trace"].as_array().unwrap();
    assert_eq!(trace.len(), 1, "{trace:?}");
    assert_eq!(trace[0]["requirement"], "FR-001");
    assert_eq!(trace[0]["requirement_hash"].as_str().unwrap().len(), 64);
    // The typed predicate reads it back to the same signed bytes.
    let typed: speccoder_core::predicate::TaskVerificationV0 =
        serde_json::from_value(verification["predicate"].clone()).unwrap();
    assert_eq!(typed.revision(), speccoder_core::predicate::REVISION);
    assert!(typed.newer_fields().is_empty());
    let back = serde_json::to_value(&typed).unwrap();
    assert_eq!(
        speccoder_runtime::attest::statement::canonical(&back),
        speccoder_runtime::attest::statement::canonical(&verification["predicate"])
    );
    for s in &all {
        assert_eq!(s["subject"][0]["digest"]["gitCommit"], commit.as_str());
        assert_eq!(s["subject"][0]["name"], "001-graph/T005");
        // No absolute local path, no secret.
        let text = s.to_string();
        for leak in ["/Users/", "/private/", "/home/", "/var/folders/", "/tmp/"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }
    let vsa = all
        .iter()
        .find(|s| s["predicateType"] == "https://slsa.dev/verification_summary/v1")
        .unwrap();
    assert_eq!(
        vsa["predicate"]["verifiedLevels"][0],
        "SPECCODER_REGRESSION_ONLY"
    );

    // `report` reads the note (signatures are verify's job).
    let rep = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(status)) = rep.data() else {
        panic!()
    };
    let t5 = status.tasks.iter().find(|t| t.id == "T005").unwrap();
    assert!(
        t5.attestation
            .as_deref()
            .unwrap()
            .starts_with("signed ssh SHA256:"),
        "{t5:?}"
    );
    assert_eq!(status.totals.without_attestation, 0);
    // The matrix: FR-001 traced to T005's verdict, FR-002 cited by none.
    let rows: Vec<_> = status
        .trace
        .iter()
        .map(|t| (t.requirement.as_str(), t.task.as_deref()))
        .collect();
    assert_eq!(rows, [("FR-001", Some("T005")), ("FR-002", None)]);
    assert!(
        rep.message().contains("1 requirement(s) no task cites"),
        "{}",
        rep.message()
    );
    git(r.path(), &["notes", "--ref=speccoder", "remove", &commit]);
    let rep = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(status)) = rep.data() else {
        panic!()
    };
    assert_eq!(status.totals.without_attestation, 1);
    assert!(
        rep.message().contains("1 tick(s) without an attestation"),
        "{}",
        rep.message()
    );
    set_note(r.path(), &commit, &note_text);

    // For scripts/attest-interop.sh: the bundle and who may have signed it.
    if let Some(out) = std::env::var_os("SPECCODER_INTEROP_OUT") {
        let out = PathBuf::from(out);
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("bundle.jsonl"), &note_text).unwrap();
        fs::copy(&allowed, out.join("allowed_signers")).unwrap();
    }

    // Anyone can re-check it.
    let checked = verify_at(r.path(), None, None, &trust(&allowed), true);
    assert_eq!(checked.status(), CommandStatus::Ok, "{}", checked.message());
    let Some(CommandData::Attest(a)) = checked.data() else {
        panic!()
    };
    assert_eq!(
        a.objects[0].outcome,
        VerifyOutcome::Ok,
        "{:?}",
        a.objects[0].reasons
    );
    assert_eq!(
        a.objects[0]
            .signer
            .as_ref()
            .and_then(|s| s.principal.as_deref()),
        Some("ada@example.com")
    );
}

#[test]
fn without_a_key_the_tick_stands_and_the_statements_are_unsigned() {
    let r = repo(None);
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let rep = report(&res);
    assert!(rep.ticked, "{}", res.message());
    let att = rep.attestation.as_ref().unwrap();
    assert_eq!(att["state"], "unsigned", "{att}");
    assert!(
        att["reason"].as_str().unwrap().contains("no signing key"),
        "{att}"
    );

    let checked = verify_at(r.path(), None, None, &Trust::default(), false);
    assert_eq!(checked.status(), CommandStatus::Ok, "{}", checked.message());
    assert_eq!(outcome(&checked), VerifyOutcome::Unsigned);
    let strict = verify_at(r.path(), None, None, &Trust::default(), true);
    assert_eq!(strict.status(), CommandStatus::AttestationFailed);
    assert_eq!(strict.exit_code().code(), 26);
}

#[test]
fn every_tampering_is_caught() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let commit = report(&res).commit.clone().unwrap();
    let original = note(r.path(), &commit);
    let check = |trust: &Trust| outcome(&verify_at(r.path(), None, None, trust, true));
    let restore = || set_note(r.path(), &commit, &original);

    // An edited statement no longer matches its signature.
    let mut lines: Vec<Value> = original
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let tv = lines
        .iter()
        .position(|l| {
            let s: Value =
                serde_json::from_slice(&STANDARD.decode(l["payload"].as_str().unwrap()).unwrap())
                    .unwrap();
            s["predicateType"] == "https://speccoder.dev/task-verification/v0"
        })
        .unwrap();
    let mut s: Value = serde_json::from_slice(
        &STANDARD
            .decode(lines[tv]["payload"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    s["predicate"]["verdict"] = json!("verified");
    lines[tv]["payload"] = json!(STANDARD.encode(serde_json::to_vec(&s).unwrap()));
    let edited: String = lines.iter().map(|l| format!("{l}\n")).collect();
    set_note(r.path(), &commit, &edited);
    assert_eq!(check(&trust(&allowed)), VerifyOutcome::BadSignature);
    restore();

    // A removed line is caught by the summary that names every line.
    let without: String = original
        .lines()
        .filter(|l| {
            let e: Value = serde_json::from_str(l).unwrap();
            let s: Value =
                serde_json::from_slice(&STANDARD.decode(e["payload"].as_str().unwrap()).unwrap())
                    .unwrap();
            s["predicateType"] != "https://in-toto.io/attestation/test-result/v0.1"
        })
        .map(|l| format!("{l}\n"))
        .collect();
    set_note(r.path(), &commit, &without);
    assert_eq!(check(&trust(&allowed)), VerifyOutcome::Mismatch);
    restore();

    // A key nobody trusts.
    let empty = keys.path().join("nobody");
    fs::write(&empty, "").unwrap();
    assert_eq!(check(&trust(&empty)), VerifyOutcome::UntrustedSigner);

    // Garbage.
    set_note(r.path(), &commit, "not an attestation\n");
    assert_eq!(check(&trust(&allowed)), VerifyOutcome::Malformed);
    restore();

    // Copied onto another commit claiming the same task: wrong subject.
    let branch = "speccoder/001-graph";
    let wt = PathBuf::from(&report(&res).worktree);
    git(
        &wt,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "speccoder: T005 again\n\nSpec-Task: 001-graph/T005\nVerdict: verified\n",
        ],
    );
    let copy = git(r.path(), &["rev-parse", branch]);
    set_note(r.path(), &copy, &original);
    let res2 = verify_at(r.path(), None, Some(&copy), &trust(&allowed), true);
    assert_eq!(
        outcome(&res2),
        VerifyOutcome::Mismatch,
        "{}",
        res2.message()
    );

    // Evidence re-issued, signed, for a commit whose spec says something
    // else: the requirement it was judged against is not this one.
    git(&wt, &["reset", "-q", "--hard", &commit]);
    let spec = wt.join("specs/001-graph/spec.md");
    fs::write(
        &spec,
        fs::read_to_string(&spec)
            .unwrap()
            .replace("Users log in.", "Users log in by magic link."),
    )
    .unwrap();
    git(
        &wt,
        &["-c", "commit.gpgsign=false", "commit", "-qa", "-C", &commit],
    );
    let respecced = git(&wt, &["rev-parse", "HEAD"]);
    let attest = speccoder_runtime::substrate::run_config::parse(CONFIG)
        .unwrap()
        .attest;
    let copied = speccoder_runtime::attest::reissue(r.path(), &commit, respecced.trim(), &attest);
    assert_eq!(copied["state"], "signed", "{copied}");
    let res3 = verify_at(
        r.path(),
        None,
        Some(respecced.trim()),
        &trust(&allowed),
        true,
    );
    let Some(CommandData::Attest(a)) = res3.data() else {
        panic!("{}", res3.message())
    };
    assert_eq!(
        a.objects[0].outcome,
        VerifyOutcome::Mismatch,
        "{:?}",
        a.objects[0].reasons
    );
    assert!(
        a.objects[0]
            .reasons
            .iter()
            .any(|r| r.field.as_deref() == Some("spec.section_hash")),
        "{:?}",
        a.objects[0].reasons
    );

    // Amended (a rebase does the same): the new commit has no note, and the
    // one with the same content is named.
    git(&wt, &["reset", "-q", "--hard", &commit]);
    git(
        &wt,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--amend",
            "-m",
            "speccoder: T005\n\nSpec-Task: 001-graph/T005\nVerdict: verified\n",
        ],
    );
    let amended = git(&wt, &["rev-parse", "HEAD"]);
    let res4 = verify_at(r.path(), None, Some(amended.trim()), &trust(&allowed), true);
    let Some(CommandData::Attest(a)) = res4.data() else {
        panic!("{}", res4.message())
    };
    assert_eq!(a.objects[0].outcome, VerifyOutcome::Missing);
    let same = a.objects[0]
        .same_tree_as
        .clone()
        .expect("a noted commit with the same tree");
    assert_eq!(
        git(r.path(), &["rev-parse", &format!("{same}^{{tree}}")]),
        git(
            r.path(),
            &["rev-parse", &format!("{}^{{tree}}", amended.trim())]
        )
    );
    // …and the note copied over: the trailer and the subject disagree.
    git(
        r.path(),
        &[
            "notes",
            "--ref=speccoder",
            "copy",
            "-f",
            &commit,
            amended.trim(),
        ],
    );
    let res5 = verify_at(r.path(), None, Some(amended.trim()), &trust(&allowed), true);
    let Some(CommandData::Attest(a)) = res5.data() else {
        panic!("{}", res5.message())
    };
    assert_eq!(a.objects[0].outcome, VerifyOutcome::Mismatch);
    let fields: Vec<_> = a.objects[0]
        .reasons
        .iter()
        .filter_map(|r| r.field.as_deref())
        .collect();
    assert!(
        fields.contains(&"Verdict") && fields.contains(&"subject.digest.gitCommit"),
        "{fields:?}"
    );

    // Deleted.
    git(r.path(), &["notes", "--ref=speccoder", "remove", &commit]);
    let gone = verify_at(r.path(), None, Some(&commit), &trust(&allowed), true);
    assert_eq!(outcome(&gone), VerifyOutcome::Missing);
    assert_eq!(gone.exit_code().code(), 26);
}

#[test]
fn signing_later_signs_what_the_tick_left_unsigned() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[attest]\nsign_at = \"later\"\n"),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "sign later"]);
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let att = report(&res).attestation.clone().unwrap();
    assert_eq!(att["state"], "unsigned", "{att}");
    let before = verify_at(r.path(), None, None, &trust(&allowed), true);
    assert_eq!(outcome(&before), VerifyOutcome::Unsigned);

    let signed = speccoder_runtime::attest::sign_at(r.path(), None, None);
    assert_eq!(signed.status(), CommandStatus::Ok, "{}", signed.message());
    assert!(
        signed.message().contains("on 1 task object"),
        "{}",
        signed.message()
    );
    let after = verify_at(r.path(), None, None, &trust(&allowed), true);
    assert_eq!(outcome(&after), VerifyOutcome::Ok, "{}", after.message());
}

#[test]
fn decisions_are_attested_as_decisions() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    let state = tempfile::tempdir().unwrap();
    let accepted = speccoder_runtime::controls::accept_in(
        r.path(),
        None,
        "T005",
        "tested by hand on staging",
        state.path(),
        false,
    );
    assert_eq!(
        accepted.status(),
        CommandStatus::Ok,
        "{}",
        accepted.message()
    );
    assert!(
        accepted.message().contains("attested: signed (ssh)"),
        "{}",
        accepted.message()
    );
    let skipped = speccoder_runtime::controls::skip(r.path(), None, "T006", Some("out of scope"));
    assert_eq!(skipped.status(), CommandStatus::Ok, "{}", skipped.message());
    assert!(
        skipped.message().contains("attested: signed (ssh)"),
        "{}",
        skipped.message()
    );

    let checked = verify_at(r.path(), None, None, &trust(&allowed), true);
    assert_eq!(checked.status(), CommandStatus::Ok, "{}", checked.message());
    let Some(CommandData::Attest(a)) = checked.data() else {
        panic!()
    };
    let seen: Vec<(&str, &str, Option<&str>)> = a
        .objects
        .iter()
        .map(|o| (o.kind.as_str(), o.task.as_str(), o.verdict.as_deref()))
        .collect();
    assert_eq!(
        seen,
        [
            ("task", "T005", Some("attested")),
            ("skip", "T006", Some("skipped"))
        ],
        "{:?}",
        a.objects
    );
    // A decision is not a verification: no summary, no test results.
    let commit = a.objects[0].object.clone();
    let all = statements(&note(r.path(), &commit));
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0]["predicate"]["decider"]["reason"],
        "tested by hand on staging"
    );
}

/// Checking 50 signed ticks takes under five seconds. One real
/// signed tick, then 49 more commits doing the same task (same trailers,
/// same tree), each with that evidence re-issued and signed for it.
#[test]
#[ignore = "benchmark: cargo nextest run --run-ignored only -E 'test(fifty)'"]
fn fifty_signed_ticks_verify_in_under_five_seconds() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let first = report(&res).commit.clone().unwrap();
    let attest = speccoder_runtime::substrate::run_config::parse(CONFIG)
        .unwrap()
        .attest;
    git(r.path(), &["branch", "bench", &first]);
    let wt = tempfile::tempdir().unwrap();
    let wt = wt.path().join("bench");
    git(
        r.path(),
        &["worktree", "add", "-q", &wt.display().to_string(), "bench"],
    );
    for _ in 1..50 {
        git(
            &wt,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-C",
                "HEAD",
            ],
        );
        let head = git(&wt, &["rev-parse", "HEAD"]);
        let copied = speccoder_runtime::attest::reissue(r.path(), &first, head.trim(), &attest);
        assert_eq!(copied["state"], "signed", "{copied}");
    }
    let trust = trust(&allowed);
    let range = format!("{first}^..bench");
    let started = std::time::Instant::now();
    let checked = verify_at(r.path(), None, Some(&range), &trust, true);
    let took = started.elapsed();
    let Some(CommandData::Attest(a)) = checked.data() else {
        panic!("{}", checked.message())
    };
    assert_eq!(a.objects.len(), 50, "{}", checked.message());
    assert_eq!(checked.status(), CommandStatus::Ok, "{}", checked.message());
    eprintln!("50 signed ticks verified in {took:?}");
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}

/// Keyless Sigstore end to end: the summary is signed with a short-lived
/// certificate (one identity login), logged in Rekor, and verified against
/// the expected identity; the other lines are covered by its digests.
/// Needs `cosign`, the network and an identity: `SPECCODER_SIGSTORE_TEST=1`
/// plus `SPECCODER_SIGSTORE_IDENTITY` / `_ISSUER` (not in `just ci`).
#[test]
fn sigstore_signs_the_summary_once_and_verifies_against_the_identity() {
    let (Some(_), Ok(identity), Ok(issuer)) = (
        std::env::var_os("SPECCODER_SIGSTORE_TEST"),
        std::env::var("SPECCODER_SIGSTORE_IDENTITY"),
        std::env::var("SPECCODER_SIGSTORE_ISSUER"),
    ) else {
        eprintln!("skipped: SPECCODER_SIGSTORE_TEST, _IDENTITY and _ISSUER are not all set");
        return;
    };
    if !has("cosign") {
        eprintln!("skipped: no cosign");
        return;
    }
    let r = repo(None);
    fs::write(
        r.path().join("speccoder.toml"),
        format!("{CONFIG}\n[attest]\nsigner = \"sigstore\"\n"),
    )
    .unwrap();
    git(r.path(), &["commit", "-qam", "sigstore"]);
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let att = report(&res).attestation.clone().unwrap();
    assert_eq!(att["state"], "signed", "{att}");
    let commit = report(&res).commit.clone().unwrap();
    let lines: Vec<Value> = note(r.path(), &commit)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let signed: Vec<&Value> = lines
        .iter()
        .filter(|l| l["signatures"].as_array().is_some_and(|s| !s.is_empty()))
        .collect();
    assert_eq!(signed.len(), 1, "only the summary is signed");
    assert!(
        signed[0]["sigstore"].is_object(),
        "the line carries its Sigstore bundle"
    );
    let trust = Trust {
        identity: Some(identity),
        issuer: Some(issuer),
        ..Trust::default()
    };
    let checked = verify_at(r.path(), None, None, &trust, true);
    assert_eq!(
        outcome(&checked),
        VerifyOutcome::Ok,
        "{}",
        checked.message()
    );
    let wrong = Trust {
        identity: Some("someone-else@example.invalid".to_owned()),
        ..trust
    };
    assert_ne!(
        outcome(&verify_at(r.path(), None, None, &wrong, true)),
        VerifyOutcome::Ok
    );
}

/// A line the signed summary names by digest needs no signature of its
/// own — and editing it breaks the digest the signature covers.
#[test]
fn a_signed_summary_covers_the_lines_it_names() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let keys = tempfile::tempdir().unwrap();
    let (key, allowed) = key(keys.path());
    let r = repo(Some(&key));
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let commit = report(&res).commit.clone().unwrap();
    let original = note(r.path(), &commit);
    let is_vsa = |l: &Value| {
        let s: Value =
            serde_json::from_slice(&STANDARD.decode(l["payload"].as_str().unwrap()).unwrap())
                .unwrap();
        s["predicateType"] == "https://slsa.dev/verification_summary/v1"
    };
    let rewrite = |f: &dyn Fn(&mut Value)| -> String {
        original
            .lines()
            .map(|l| {
                let mut v: Value = serde_json::from_str(l).unwrap();
                f(&mut v);
                format!("{v}\n")
            })
            .collect()
    };
    let check = || {
        outcome(&verify_at(
            r.path(),
            None,
            Some(&commit),
            &trust(&allowed),
            true,
        ))
    };

    // Only the summary signed: the rest are covered.
    set_note(
        r.path(),
        &commit,
        &rewrite(&|v| {
            if !is_vsa(v) {
                v["signatures"] = json!([])
            }
        }),
    );
    assert_eq!(check(), VerifyOutcome::Ok);

    // A covered line edited: its digest no longer matches.
    set_note(
        r.path(),
        &commit,
        &rewrite(&|v| {
            if !is_vsa(v) {
                v["signatures"] = json!([]);
                let mut s: Value = serde_json::from_slice(
                    &STANDARD.decode(v["payload"].as_str().unwrap()).unwrap(),
                )
                .unwrap();
                if s["predicateType"] == "https://speccoder.dev/task-verification/v0" {
                    s["predicate"]["verdict"] = json!("verified");
                    v["payload"] = json!(STANDARD.encode(serde_json::to_vec(&s).unwrap()));
                }
            }
        }),
    );
    assert_eq!(check(), VerifyOutcome::Mismatch);

    // The summary unsigned: nothing covers the rest of what it names.
    set_note(
        r.path(),
        &commit,
        &rewrite(&|v| {
            if is_vsa(v) {
                v["signatures"] = json!([])
            }
        }),
    );
    assert_eq!(check(), VerifyOutcome::Unsigned);
}

/// Parallel lanes tick at once: every note written concurrently lands
/// (git lets one writer hold the notes ref at a time).
#[test]
fn notes_written_at_once_all_land() {
    let r = repo(None);
    let mut commits = Vec::new();
    for i in 0..8 {
        git(
            r.path(),
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("c{i}"),
            ],
        );
        commits.push(git(r.path(), &["rev-parse", "HEAD"]).trim().to_owned());
    }
    let path = r.path().to_path_buf();
    std::thread::scope(|s| {
        for c in &commits {
            let path = &path;
            s.spawn(move || {
                speccoder_runtime::attest::notes::write(path, c, &format!("{{\"c\":\"{c}\"}}\n"))
                    .unwrap();
            });
        }
    });
    let noted = speccoder_runtime::attest::notes::noted(r.path());
    for c in &commits {
        assert!(noted.contains(c), "{c} lost its note");
    }
}

/// Two processes on one repository: notes written at once by both all land.
#[test]
fn notes_written_by_two_processes_all_land() {
    let r = repo(None);
    let mut commits = Vec::new();
    for i in 0..12 {
        git(
            r.path(),
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("p{i}"),
            ],
        );
        commits.push(git(r.path(), &["rev-parse", "HEAD"]).trim().to_owned());
    }
    // This test binary, re-run as a writer of half the commits each.
    let me = std::env::current_exe().unwrap();
    let spawn = |half: &[String]| {
        Command::new(&me)
            .args(["--exact", "notes_writer_child", "--nocapture"])
            .env("SPECCODER_NOTES_REPO", r.path())
            .env("SPECCODER_NOTES_COMMITS", half.join(","))
            .spawn()
            .unwrap()
    };
    let (a, b) = commits.split_at(6);
    let (mut x, mut y) = (spawn(a), spawn(b));
    assert!(x.wait().unwrap().success() && y.wait().unwrap().success());
    let noted = speccoder_runtime::attest::notes::noted(r.path());
    for c in &commits {
        assert!(noted.contains(c), "{c} lost its note");
    }
}

/// The child of [`notes_written_by_two_processes_all_land`]; a no-op otherwise.
#[test]
fn notes_writer_child() {
    let (Ok(repo), Ok(commits)) = (
        std::env::var("SPECCODER_NOTES_REPO"),
        std::env::var("SPECCODER_NOTES_COMMITS"),
    ) else {
        return;
    };
    for c in commits.split(',') {
        speccoder_runtime::attest::notes::write(
            Path::new(&repo),
            c,
            &format!("{{\"c\":\"{c}\"}}\n"),
        )
        .unwrap();
    }
}

#[test]
fn a_regression_only_reason_is_signed_and_must_agree_with_its_trailer() {
    let r = repo(None);
    let state = tempfile::tempdir().unwrap();
    let res = passes(r.path(), state.path());
    let rep = report(&res);
    let commit = rep.commit.clone().unwrap();
    let why = rep
        .regression
        .as_ref()
        .expect("a regression-only tick says why");
    let original = note(r.path(), &commit);
    let tv = statements(&original)
        .into_iter()
        .find(|s| s["predicateType"] == "https://speccoder.dev/task-verification/v0")
        .unwrap();
    assert_eq!(tv["predicate"]["revision"], 3);
    assert_eq!(tv["predicate"]["regression"], json!(why.reason));
    let ok = verify_at(r.path(), None, None, &Trust::default(), false);
    assert_eq!(outcome(&ok), VerifyOutcome::Unsigned, "{}", ok.message());

    // The statement claims another reason than the commit's trailer.
    let edited: String = original
        .lines()
        .map(|l| {
            let mut e: Value = serde_json::from_str(l).unwrap();
            let mut s: Value =
                serde_json::from_slice(&STANDARD.decode(e["payload"].as_str().unwrap()).unwrap())
                    .unwrap();
            if s["predicateType"] == "https://speccoder.dev/task-verification/v0" {
                s["predicate"]["regression"] = json!({ "reason": "no_story" });
                e["payload"] = json!(STANDARD.encode(serde_json::to_vec(&s).unwrap()));
            }
            format!("{e}\n")
        })
        .collect();
    set_note(r.path(), &commit, &edited);
    let checked = verify_at(r.path(), None, None, &Trust::default(), false);
    let Some(CommandData::Attest(a)) = checked.data() else {
        panic!("{}", checked.message())
    };
    assert!(
        a.objects[0]
            .reasons
            .iter()
            .any(|r| r.field.as_deref() == Some("Regression-Only")),
        "{:?}",
        a.objects
    );
}
