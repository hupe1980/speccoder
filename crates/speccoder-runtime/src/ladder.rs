//! Where work came from, written once: `<project state>/ladder/origins.json`,
//! one record per subject (a unit, or `<unit>/<requirement>`). Only
//! [`record_origin`] writes it; a second write for a subject is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use speccoder_core::ladder::Origin;

/// The store's file.
#[must_use]
pub fn file(project_state: &Path) -> PathBuf {
    project_state.join("ladder").join("origins.json")
}

/// Every recorded subject's origins.
#[must_use]
pub fn origins(project_state: &Path) -> BTreeMap<String, Vec<Origin>> {
    std::fs::read_to_string(file(project_state))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The origins of `subject`.
#[must_use]
pub fn of(project_state: &Path, subject: &str) -> Vec<Origin> {
    origins(project_state).remove(subject).unwrap_or_default()
}

/// Records `subject`'s origins, once.
///
/// # Errors
///
/// It was recorded before, or the store cannot be written.
pub fn record_origin(
    project_state: &Path,
    subject: &str,
    origin: Vec<Origin>,
) -> Result<(), String> {
    let mut all = origins(project_state);
    if all.contains_key(subject) {
        return Err(format!(
            "the origin of {subject} is recorded already; it is written once"
        ));
    }
    all.insert(subject.to_owned(), origin);
    let f = file(project_state);
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &f).map_err(|e| format!("{}: {e}", f.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_is_written_once() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let o = vec![Origin::Issue {
            number: 7,
            repo: None,
            context_sha256: None,
        }];
        assert!(record_origin(state.path(), "007-bugfixes/FR-001", o.clone()).is_ok());
        assert!(record_origin(state.path(), "007-bugfixes/FR-001", o.clone()).is_err());
        assert!(record_origin(state.path(), "007-bugfixes/FR-002", o.clone()).is_ok());
        assert_eq!(of(state.path(), "007-bugfixes/FR-001"), o);
    }
}
