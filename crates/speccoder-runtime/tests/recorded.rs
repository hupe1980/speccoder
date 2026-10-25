//! Journals of real-model runs, recorded with `just record-journals`, still
//! replay strictly with this build. An agentplane upgrade that cannot read
//! them or reproduces a different run fails here — before a user's journal
//! meets it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

fn recorded() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recorded")
}

/// Shapes of a credential that must never be in a fixture: provider keys,
/// bearer tokens, and the key variables with a value.
const CREDENTIALS: &str =
    r"sk-(ant-)?[A-Za-z0-9_-]{20,}|Bearer [A-Za-z0-9._~+/-]{20,}|(OPENAI|ANTHROPIC)_API_KEY=\S";

const MAX_BYTES: u64 = 2 * 1024 * 1024;

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn run_ids(manifest: &str) -> Vec<String> {
    let table: toml::Table = toml::from_str(manifest).unwrap();
    table["run_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn recorded_real_runs_replay_strictly() {
    let root = recorded();
    if !root.is_dir() {
        // Until the first recording (T028) there is nothing to replay.
        eprintln!("no recorded journals yet: run `just record-journals`");
        return;
    }
    let mut cases: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "tests/fixtures/recorded has no cases");
    let pin = "agentplane (see Cargo.lock)";
    let credential = regex::Regex::new(CREDENTIALS).unwrap();
    for case in cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let mut all = Vec::new();
        files(&case, &mut all);
        let size: u64 = all.iter().map(|f| f.metadata().unwrap().len()).sum();
        assert!(
            size <= MAX_BYTES,
            "{name}: fixture is {size} bytes (> 2 MB)"
        );
        for f in &all {
            let bytes = fs::read(f).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !credential.is_match(&text),
                "{name}: {} contains something shaped like a credential",
                f.display()
            );
        }

        let manifest = fs::read_to_string(case.join("manifest.toml")).unwrap();
        let config = fs::read_to_string(case.join("speccoder.toml")).unwrap();
        let model = speccoder_runtime::substrate::model_config::parse(&config)
            .unwrap()
            .unwrap_or_else(|| panic!("{name}: speccoder.toml has no [model]"));
        let tmp = tempfile::tempdir().unwrap();
        fs::copy(
            case.join("journal")
                .join(speccoder_runtime::substrate::JOURNAL),
            tmp.path().join(speccoder_runtime::substrate::JOURNAL),
        )
        .unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for run in run_ids(&manifest) {
            let verdict = rt.block_on(speccoder_runtime::substrate::replay_strict(
                tmp.path(),
                &model,
                &run,
                speccoder_runtime::format::reference(),
            ));
            let set_aside = fs::read_dir(tmp.path())
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains("unreadable"));
            match verdict {
                Ok(Some(v)) if !set_aside => assert!(
                    matches!(v.finding, agentplane::runtime::Finding::Verified { .. }),
                    "{name}: run {run} no longer reproduces: {:?}",
                    v.finding
                ),
                other => panic!(
                    "{name}: journal unreadable by this build ({pin}): {other:?}; re-record with `just record-journals`"
                ),
            }
        }
    }
}
