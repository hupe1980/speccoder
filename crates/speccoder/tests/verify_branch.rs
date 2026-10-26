//! `speccoder attest verify --branch` on the verifier's fixtures: the same
//! function as `speccoder-verify check`, the same outcomes; and every
//! statement the fixtures write validates against the published
//! task-verification schema.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../speccoder-verify/tests/support/mod.rs"]
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
fn attest_verify_branch_is_the_verifiers_check() {
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
        require_signed: false,
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
