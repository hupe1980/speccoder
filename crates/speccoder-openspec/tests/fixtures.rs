//! Every supported OpenSpec release: speccoder's parse of the sample change
//! agrees with that release's own JSON (`SPECCODER_OPENSPEC_VERSIONS` adds
//! freshly generated fixtures, as the weekly compatibility job does).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use speccoder_core::snapshot::Severity;

/// Releases with committed fixtures.
const SUPPORTED: &[&str] = &["1.14.3"];

fn base() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn json(dir: &Path, name: &str) -> Option<serde_json::Value> {
    std::fs::read_to_string(dir.join("outputs").join(name))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

#[test]
fn every_supported_release_agrees_with_speccoders_parse() {
    let extra = std::env::var("SPECCODER_OPENSPEC_VERSIONS").unwrap_or_default();
    let versions: Vec<&str> = SUPPORTED
        .iter()
        .copied()
        .chain(extra.split(',').map(str::trim).filter(|v| !v.is_empty()))
        .collect();
    for v in versions {
        let dir = base().join(v);
        let s = speccoder_openspec::inspect(&dir.join("sample"), None)
            .unwrap_or_else(|_| panic!("{v}: no OpenSpec project"));
        let errors: Vec<_> = s
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{v}: {errors:?}");
        let f = s.feature.as_ref().unwrap();
        let disagreements = speccoder_openspec::crosscheck::compare(
            &f.feature.dir,
            f.tasks.as_ref(),
            json(&dir, "validate-change2.json").as_ref(),
            json(&dir, "instructions-apply-change2.json").as_ref(),
        );
        assert!(disagreements.is_empty(), "{v}: {disagreements:?}");
    }
}
