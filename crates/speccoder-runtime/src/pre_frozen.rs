//! The store of acceptance tests decided before the run
//! ([`PreFrozenAcceptance`]): `<project state>/pre-frozen/<unit>/<requirement>/`
//! holds the record and the files it names. Written only by the producer's
//! person-commanded step; every load checks each file's hash.

use std::path::{Path, PathBuf};

use speccoder_core::acceptance::PreFrozenAcceptance;

use crate::integrity::sha256_hex;

/// A record and its files.
pub type Loaded = (PreFrozenAcceptance, Vec<(String, String)>);

/// A stored set that no longer matches its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tampered(pub String);

/// Where a record lives.
#[must_use]
pub fn dir(project_state: &Path, unit: &str, requirement: &str) -> PathBuf {
    project_state
        .join("pre-frozen")
        .join(unit)
        .join(requirement.replace([':', '/'], "-"))
}

/// Stores `record` with the visible files' `contents` (path, text); each
/// content must hash to the record's entry. A previous record for the
/// requirement is replaced.
pub fn store(
    project_state: &Path,
    record: &PreFrozenAcceptance,
    contents: &[(String, String)],
) -> Result<PathBuf, String> {
    record.validate()?;
    for t in &record.visible {
        let content = contents
            .iter()
            .find(|(p, _)| *p == t.path)
            .map(|(_, c)| c)
            .ok_or_else(|| format!("{} has no content", t.path))?;
        if format!("sha256:{}", sha256_hex(content.as_bytes())) != t.sha256 {
            return Err(format!("{} does not match its digest", t.path));
        }
    }
    let d = dir(project_state, &record.unit, &record.requirement);
    let _ = std::fs::remove_dir_all(&d);
    for (path, content) in contents {
        let file = d.join("files").join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    std::fs::write(d.join("record.json"), text).map_err(|e| e.to_string())?;
    Ok(d)
}

/// The record for `(unit, requirement)` and its files, every hash checked;
/// `Ok(None)`: there is none.
pub fn load(
    project_state: &Path,
    unit: &str,
    requirement: &str,
) -> Result<Option<Loaded>, Tampered> {
    let d = dir(project_state, unit, requirement);
    let Ok(text) = std::fs::read_to_string(d.join("record.json")) else {
        return Ok(None);
    };
    let record: PreFrozenAcceptance = serde_json::from_str(&text)
        .map_err(|e| Tampered(format!("the pre-frozen record is not valid: {e}")))?;
    let mut contents = Vec::new();
    for t in &record.visible {
        let content = std::fs::read_to_string(d.join("files").join(&t.path))
            .map_err(|_| Tampered(format!("pre-frozen test {} is missing", t.path)))?;
        if format!("sha256:{}", sha256_hex(content.as_bytes())) != t.sha256 {
            return Err(Tampered(format!("pre-frozen test {} changed", t.path)));
        }
        contents.push((t.path.clone(), content));
    }
    Ok(Some((record, contents)))
}

/// Removes the record (a reworded triad, a dropped task).
pub fn remove(project_state: &Path, unit: &str, requirement: &str) {
    let _ = std::fs::remove_dir_all(dir(project_state, unit, requirement));
}

/// `sha256:<hex>` of the visible set's manifest.
#[must_use]
pub fn manifest_digest(record: &PreFrozenAcceptance) -> String {
    format!(
        "sha256:{}",
        sha256_hex(speccoder_core::acceptance::manifest_text(&record.visible).as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use speccoder_core::acceptance::{Decided, Producer, VisibleTest};
    use speccoder_core::actions::Via;

    use super::*;

    fn record(content: &str) -> PreFrozenAcceptance {
        PreFrozenAcceptance {
            unit: "007-bugfixes".to_owned(),
            requirement: "FR-001".to_owned(),
            wording_digest: "sha256:w".to_owned(),
            approval_digest: "sha256:a".to_owned(),
            visible: vec![VisibleTest {
                path: "tests/repro_e1.rs".to_owned(),
                sha256: format!("sha256:{}", sha256_hex(content.as_bytes())),
                cites: vec!["E1".to_owned()],
            }],
            held_out_sealed: None,
            producer: Producer::BugConfirmation,
            decided: Decided {
                by: "Ada".to_owned(),
                via: Via::Terminal,
                at: 1,
            },
        }
    }

    #[test]
    fn a_stored_set_loads_back_and_a_changed_file_is_tampering() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let r = record("#[test] fn t() {}");
        let files = vec![(
            "tests/repro_e1.rs".to_owned(),
            "#[test] fn t() {}".to_owned(),
        )];
        let d = store(state.path(), &r, &files).unwrap_or_else(|e| panic!("{e}"));
        let (back, contents) = load(state.path(), "007-bugfixes", "FR-001")
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|| panic!("stored"));
        assert_eq!(back, r);
        assert_eq!(contents, files);
        std::fs::write(d.join("files/tests/repro_e1.rs"), "changed")
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(load(state.path(), "007-bugfixes", "FR-001").is_err());
        assert!(
            store(
                state.path(),
                &r,
                &[("tests/repro_e1.rs".to_owned(), "other".to_owned())]
            )
            .is_err()
        );
        assert!(
            load(state.path(), "007-bugfixes", "FR-002")
                .unwrap_or_default()
                .is_none()
        );
    }
}
