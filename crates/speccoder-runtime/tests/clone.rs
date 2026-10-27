//! Clone's destination: absolute, never inside another repository; a
//! failure says what git needs.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use speccoder_runtime::github::{Ctx, clone};

#[test]
fn a_relative_destination_is_refused() {
    let r = clone(&Ctx::default(), "octo/hello", Path::new("hello"));
    assert_eq!(r.code().map(|c| c.name), Some("USAGE"), "{}", r.message());
    assert!(r.message().contains("absolute"), "{}", r.message());
}

#[test]
fn a_destination_inside_another_repository_is_refused() {
    let repo = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    let r = clone(
        &Ctx::default(),
        "octo/hello",
        &repo.path().join("sub/hello"),
    );
    assert_eq!(
        r.code().map(|c| c.name),
        Some("NEW-NESTED-REPOSITORY"),
        "{}",
        r.message()
    );
}
