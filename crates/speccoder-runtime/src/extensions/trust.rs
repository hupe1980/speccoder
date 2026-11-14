//! Who trusted which extension's bytes: entries `extension:<name>` in the
//! hooks' trust record, holding the digest, the files and the
//! contributions the person saw — so a changed extension shows what changed
//! and what widened.

use std::path::Path;

use speccoder_core::extensions::{Contribution, ExtensionChanges};
use speccoder_core::hooks::{HookFile, TrustState};

use super::Loaded;
use crate::hooks::trust as store;

/// The trust record's key for `name`.
#[must_use]
pub fn key(name: &str) -> String {
    format!("extension:{name}")
}

/// Whether `e` may run, as it is now.
#[must_use]
pub fn state(dir: &Path, e: &Loaded) -> TrustState {
    store::state(dir, &key(&e.name), &e.digest)
}

/// What changed since `e` was last trusted; `None` when never trusted or
/// unchanged.
#[must_use]
pub fn changes(dir: &Path, e: &Loaded) -> Option<ExtensionChanges> {
    let before = store::last_trusted(dir, &key(&e.name))?;
    if before.digest == e.digest {
        return None;
    }
    let files_before: Vec<HookFile> =
        serde_json::from_value(before.definition["files"].clone()).unwrap_or_default();
    let seen: Vec<Contribution> =
        serde_json::from_value(before.definition["contributions"].clone()).unwrap_or_default();
    let mut files: Vec<String> = e
        .files
        .iter()
        .filter(|f| !files_before.contains(f))
        .map(|f| f.path.clone())
        .chain(
            files_before
                .iter()
                .filter(|b| !e.files.iter().any(|f| f.path == b.path))
                .map(|b| format!("{} (gone)", b.path)),
        )
        .collect();
    files.sort();
    let (widened, gone) = speccoder_core::extensions::widened(&seen, &e.contributions());
    Some(ExtensionChanges {
        files,
        widened,
        gone,
    })
}

/// Records that `by` trusted `e` as it is now, through `via`.
pub fn record(dir: &Path, e: &Loaded, by: &str, via: &str) {
    store::record_as(
        dir,
        &key(&e.name),
        &e.digest,
        serde_json::json!({ "files": e.files, "contributions": e.contributions() }),
        by,
        via,
    );
}

/// `--trust-extension name=sha256:…`, split.
///
/// # Errors
///
/// The value is not `name=digest`.
pub fn parse_flag(value: &str) -> Result<(String, String), String> {
    match value.split_once('=') {
        Some((n, d)) if !n.is_empty() && d.starts_with("sha256:") => {
            Ok((n.to_owned(), d.to_owned()))
        }
        _ => Err(format!(
            "--trust-extension {value}: give <name>=sha256:<digest>, as `speccoder extensions` prints it"
        )),
    }
}

/// Trusts the extensions `flags` name, each only at its current digest
/// (`via = ci`).
///
/// # Errors
///
/// A flag names no extension, one that cannot be loaded, or a digest that is
/// not the current one (the message names it).
pub fn apply_flags(
    dir: &Path,
    all: &[Loaded],
    flags: &[(String, String)],
    by: &str,
) -> Result<(), String> {
    for (name, digest) in flags {
        let Some(e) = all.iter().find(|e| &e.name == name) else {
            return Err(format!(
                "--trust-extension {name}: there is no extension `{name}`"
            ));
        };
        if !e.usable() {
            return Err(format!(
                "--trust-extension {name}: it cannot be loaded: {}",
                e.problems.join("; ")
            ));
        }
        if &e.digest != digest {
            return Err(format!(
                "--trust-extension {name}: the extension is now {}, not {digest} — review it (speccoder extensions) before trusting",
                e.digest
            ));
        }
        if state(dir, e) != TrustState::Trusted {
            record(dir, e, by, "ci");
        }
    }
    Ok(())
}
