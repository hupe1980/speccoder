//! speccoder's parse against OpenSpec 1.14.3's recorded JSON.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use speccoder_core::snapshot::Severity;
use speccoder_openspec::crosscheck::compare;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/1.14.3")
}

fn json(name: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join("outputs").join(name)).unwrap())
        .unwrap()
}

#[test]
fn an_info_that_archive_would_refuse_is_an_error() {
    let d = compare(
        "openspec/changes/broken-example",
        None,
        Some(&json("validate-broken2.json")),
        None,
    );
    assert_eq!(d.len(), 1);
    assert_eq!(
        (d[0].severity, d[0].code.as_str()),
        (Severity::Error, "crosscheck_error")
    );
    assert!(d[0].message.contains("Does not exist"));
}

#[test]
fn openspec_and_speccoder_count_the_sample_alike() {
    let s = speccoder_openspec::inspect(&fixtures().join("sample"), None).unwrap();
    let tasks = s.feature.unwrap().tasks.unwrap();
    let d = compare(
        "openspec/changes/add-farewell-and-locale",
        Some(&tasks),
        Some(&json("validate-change2.json")),
        Some(&json("instructions-apply-change2.json")),
    );
    assert!(d.is_empty(), "{d:?}");
    let mut wrong = json("instructions-apply-change2.json");
    wrong["progress"]["total"] = 5.into();
    let d = compare("x", Some(&tasks), None, Some(&wrong));
    assert_eq!(d[0].code, "crosscheck_mismatch");
}
