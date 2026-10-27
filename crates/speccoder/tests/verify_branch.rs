//! `speccoder attest verify --branch` on the branch fixtures: the same
//! function as the library's `attest::branch::verify`, the same outcomes; and every
//! statement the fixtures write validates against the published
//! task-verification schema.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../speccoder-runtime/tests/fake_github/mod.rs"]
mod fake_github;
#[path = "branch_fixture/mod.rs"]
mod fixture;

use assert_cmd::Command;
use base64::Engine as _;
use fixture::{FEATURE, Repo, Sign, has, outcomes};
use serde_json::{Value, json};

fn speccoder(dir: &std::path::Path, args: &[&str]) -> (i32, Value) {
    let out = Command::cargo_bin("speccoder")
        .unwrap()
        .args(args)
        .current_dir(dir)
        .env("SPECCODER_STATE_DIR", dir.join(".git/speccoder-state"))
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {} / {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap_or(-1), v)
}

#[test]
fn attest_verify_branch_is_the_librarys_check() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let r = Repo::new(4);
    r.tick("T001", "verified", Sign::Trusted);
    r.tick("T002", "attested", Sign::Trusted);
    r.hand_tick("T003");
    r.tick("T004", "verified", Sign::Stranger);
    let allowed = r.allowed.display().to_string();
    let (code, v) = speccoder(
        r.path(),
        &[
            "attest",
            "verify",
            "--branch",
            "--base",
            "main",
            "--allowed-signers",
            &allowed,
            "--json",
        ],
    );
    assert_eq!(code, 26, "{v}");
    assert_eq!(v["payload"], "branch_verify");
    assert_eq!(v["status"], "attestation_failed");
    let opts = speccoder_runtime::attest::branch::Options {
        base: Some("main".to_owned()),
        allowed_signers: Some(speccoder_runtime::attest::branch::AllowedSigners::File(
            r.allowed.clone(),
        )),
        ..speccoder_runtime::attest::branch::Options::default()
    };
    let lib = speccoder_runtime::attest::branch::verify(r.path(), &opts, "x");
    let lib = serde_json::to_value(&lib).unwrap();
    assert_eq!(outcomes(&v), outcomes(&lib));
    assert_eq!(v["data"]["ticks"], lib["data"]["ticks"]);
    crate_contract(&v);
}

/// The payload validates against its published schema.
fn crate_contract(v: &Value) {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../site/static/schemas/v2/branch_verify.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&v["data"])
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// Every task-verification predicate in `note`.
fn predicates(note: &str) -> Vec<Value> {
    note.lines()
        .filter_map(|l| {
            let e: Value = serde_json::from_str(l).ok()?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(e["payload"].as_str()?)
                .ok()?;
            let s: Value = serde_json::from_slice(&bytes).ok()?;
            (s["predicateType"] == "https://speccoder.dev/task-verification/v0")
                .then(|| s["predicate"].clone())
        })
        .collect()
}

#[test]
fn predicate_schema_validates_every_statement_and_rejects_a_missing_field() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../site/static/schemas/task-verification-v0.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let r = Repo::new(4);
    let mut all = Vec::new();
    for (task, verdict) in [
        ("T001", "verified"),
        ("T002", "regression_only"),
        ("T003", "attested"),
    ] {
        let c = r.tick(task, verdict, Sign::No);
        all.extend(predicates(&r.read_note(&c)));
    }
    // Every reserved field filled (opaque), and a field of a newer revision.
    let c = r.tick_with(
        "T004",
        "verified",
        Sign::No,
        &[
            (
                "bug",
                json!({ "home": ".specify/bugs/x", "triad_sha256": "aa" }),
            ),
            ("acceptance", json!({ "source": "review" })),
            ("context", json!({ "entries": [] })),
            ("dependencies", json!([{ "name": "serde", "version": "1" }])),
            ("provenance", json!({ "builder": "next" })),
            ("revision", json!(3)),
        ],
    );
    all.extend(predicates(&r.read_note(&c)));
    assert_eq!(all.len(), 4);
    for p in &all {
        let errors: Vec<String> = validator.iter_errors(p).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{p}");
    }
    let mut broken = all[0].clone();
    broken.as_object_mut().unwrap().remove("task");
    assert!(!validator.is_valid(&broken));
    assert_eq!(all[0]["unit"]["feature"], FEATURE);
}

/// A held-out set stored for `unit` under `key`, as the pipeline stores
/// one; its manifest digest.
fn store_set(state: &std::path::Path, root: &std::path::Path, key: &str, task: &str) -> String {
    use speccoder_runtime::held_out::{HeldOutFile, Manifest, store};
    let plane = speccoder_runtime::worktree::project_state_dir(state, root);
    let content = format!(
        "#[test] fn {}() {{}}\n",
        key.replace('-', "_").to_lowercase()
    );
    let m = Manifest {
        version: 1,
        feature: FEATURE.to_owned(),
        task: task.to_owned(),
        author_run: "r".to_owned(),
        checkpoint: "c".to_owned(),
        visible_scenarios: vec!["US1-1".to_owned()],
        files: vec![HeldOutFile {
            path: format!("tests/held_{}.rs", key.to_lowercase()),
            sha256: speccoder_runtime::integrity::sha256_hex(content.as_bytes()),
            scenarios: vec!["US1-1".to_owned()],
            argv: vec!["true".to_owned()],
            composition: false,
            red_tests: None,
        }],
        discarded: Vec::new(),
    };
    store(
        &plane.join("held-out").join(FEATURE).join(key),
        &m,
        &[(format!("tests/held_{}.rs", key.to_lowercase()), content)],
    )
    .unwrap()
}

/// `pr --json [--yes]` against a stand-in GitHub; the envelope.
fn pr(r: &Repo, state: &std::path::Path, api: &str, yes: bool) -> Value {
    let mut c = Command::cargo_bin("speccoder").unwrap();
    c.args(["--json", "pr", "--base", "main"])
        .current_dir(r.path())
        .env("SPECCODER_STATE_DIR", state)
        .env("SPECCODER_GITHUB_API", api)
        .env("SPECCODER_NO_KEYCHAIN", "1")
        .env("GH_TOKEN", "t")
        .env_remove("GITHUB_TOKEN");
    if yes {
        c.arg("--yes");
    }
    let out = c.output().unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn pr_pushes_the_held_out_sets_of_exactly_the_closed_tasks() {
    if !has("ssh-keygen") {
        eprintln!("skipped: no ssh-keygen");
        return;
    }
    let r = Repo::new(3);
    r.git(&["branch", "-m", "feature", &format!("speccoder/{FEATURE}")]);
    let state = tempfile::tempdir().unwrap();
    // T001's set (task-keyed), T002's under another key (a sealed set's
    // shape): both named by their tasks' evidence; T003 stays open.
    let t1 = store_set(state.path(), r.path(), "T001", "T001");
    let t2 = store_set(state.path(), r.path(), "req-login", "T002");
    store_set(state.path(), r.path(), "T003", "T003");
    r.held_out.borrow_mut().insert("T001".to_owned(), t1);
    r.held_out.borrow_mut().insert("T002".to_owned(), t2);
    r.tick("T001", "verified", Sign::Trusted);
    r.tick("T002", "verified", Sign::Trusted);
    r.git(&["checkout", "-q", "main"]);
    let remote = tempfile::tempdir().unwrap();
    fixture::git(remote.path(), &["init", "-q", "--bare"]);
    r.git(&["remote", "add", "origin", "https://github.com/o/r.git"]);
    r.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        &remote.path().display().to_string(),
    ]);
    let api = fake_github::serve(vec![
        (
            "POST /graphql",
            200,
            r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#.to_owned(),
        ),
        (
            "POST /repos/o/r/pulls",
            201,
            r#"{"number":1,"html_url":"https://github.com/o/r/pull/1"}"#.to_owned(),
        ),
    ]);

    let dry = pr(&r, state.path(), &api.url, false);
    assert_eq!(dry["status"], "ok", "{dry}");
    let d = &dry["data"];
    let reference = format!("refs/speccoder/held-out/{FEATURE}");
    assert!(
        d["commands"]
            .as_array()
            .unwrap()
            .contains(&json!(format!("git push origin {reference}"))),
        "{d}"
    );
    assert_eq!(d["held_out_tasks"], json!(["T001", "T002"]));
    assert_eq!(d["held_out_withheld"], 1);
    assert_eq!(d["evidence_check"], "absent");
    let message = dry["message"].as_str().unwrap();
    assert!(
        message.contains("held-out tests of 1 open task(s) stay local"),
        "{message}"
    );
    assert!(message.contains("evidence check: none — see https://hupe1980.github.io/speccoder/docs/github/#evidence-check"));
    assert!(
        d["body"].as_str().unwrap().contains("### Evidence tour"),
        "{d}"
    );
    assert!(
        fixture::git(remote.path(), &["for-each-ref"]).is_empty(),
        "pushed in a dry run"
    );

    let opened = pr(&r, state.path(), &api.url, true);
    assert_eq!(opened["status"], "ok", "{opened}");
    let tree = fixture::git(remote.path(), &["ls-tree", "-r", "--name-only", &reference]);
    let paths: Vec<&str> = tree.lines().collect();
    assert!(paths.contains(&"T001/manifest.json"), "{tree}");
    assert!(paths.contains(&"req-login/manifest.json"), "{tree}");
    assert!(
        !paths.iter().any(|p| p.starts_with("T003/")),
        "an open task's set was pushed: {tree}"
    );
    let first = fixture::git(remote.path(), &["rev-parse", &reference]);

    // T003 closes: the next hand-off adds its set, as a fast-forward.
    r.git(&["checkout", "-q", &format!("speccoder/{FEATURE}")]);
    let t3 = store_set(state.path(), r.path(), "T003", "T003");
    r.held_out.borrow_mut().insert("T003".to_owned(), t3);
    r.tick("T003", "verified", Sign::Trusted);
    r.git(&["checkout", "-q", "main"]);
    let again = pr(&r, state.path(), &api.url, true);
    assert_eq!(again["status"], "ok", "{again}");
    assert!(
        !again["message"].as_str().unwrap().contains("not pushed"),
        "{again}"
    );
    let second = fixture::git(remote.path(), &["rev-parse", &reference]);
    assert_ne!(first, second);
    fixture::git(
        remote.path(),
        &["merge-base", "--is-ancestor", &first, &second],
    );
    assert!(
        fixture::git(remote.path(), &["ls-tree", "-r", "--name-only", &reference])
            .contains("T003/manifest.json")
    );
}

#[test]
fn a_tampered_set_is_never_published() {
    if !has("ssh-keygen") {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let r = Repo::new(2);
    let digest = store_set(state.path(), r.path(), "T001", "T001");
    r.held_out
        .borrow_mut()
        .insert("T001".to_owned(), digest.clone());
    r.tick("T001", "verified", Sign::Trusted);
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let dir = plane.join("held-out").join(FEATURE).join("T001");
    std::fs::write(dir.join("files/tests/held_t001.rs"), "changed").unwrap();
    let head = r.git(&["rev-parse", "HEAD"]);
    let closed = speccoder_runtime::held_out::closed_on(r.path(), &head, FEATURE);
    assert_eq!(closed, [("T001".to_owned(), vec![digest])]);
    let s = speccoder_runtime::held_out::select_closed(&plane, FEATURE, &closed);
    assert!(s.sets.is_empty());
    assert_eq!(s.tampered, ["T001"]);
}
