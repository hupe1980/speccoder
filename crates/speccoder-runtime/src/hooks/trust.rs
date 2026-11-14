//! Who trusted which hook's bytes. A record lives in speccoder's state for
//! the project, never in the checkout; it holds until the definition or a
//! file the hook runs changes. The one before it is kept, so trusting a
//! changed hook shows what changed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Digested, TrustState};

const FILE: &str = "trust.json";

/// One person's trust in one hook's bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trust {
    /// What was trusted.
    pub digest: String,
    /// Who.
    pub by: String,
    /// Through what (`terminal`, `web`, `ci`).
    pub via: String,
    /// When (RFC 3339).
    pub at: String,
    /// The definition the person saw.
    pub definition: serde_json::Value,
}

/// A hook's current record and the one before it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The record in force.
    pub current: Trust,
    /// The record it replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Trust>,
}

/// Every record in `dir`.
#[must_use]
pub fn read(dir: &Path) -> BTreeMap<String, Entry> {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Whether `name` with `digest` may run.
#[must_use]
pub fn state(dir: &Path, name: &str, digest: &str) -> TrustState {
    match read(dir).get(name) {
        None => TrustState::Untrusted,
        Some(e) if e.current.digest == digest => TrustState::Trusted,
        Some(_) => TrustState::Changed,
    }
}

/// The record a changed hook was trusted under, for "what changed".
#[must_use]
pub fn last_trusted(dir: &Path, name: &str) -> Option<Trust> {
    read(dir).remove(name).map(|e| e.current)
}

/// Records that `by` trusted `hook` as it is now, through `via`.
pub fn record(dir: &Path, hook: &Digested, by: &str, via: &str) {
    record_as(
        dir,
        &hook.def.name,
        &hook.digest,
        hook.def.definition.clone(),
        by,
        via,
    );
}

/// Records that `by` trusted what `key` names (a hook, or
/// `extension:<name>`) at `digest`, having seen `definition`.
pub fn record_as(
    dir: &Path,
    key: &str,
    digest: &str,
    definition: serde_json::Value,
    by: &str,
    via: &str,
) {
    let mut all = read(dir);
    let now = Trust {
        digest: digest.to_owned(),
        by: by.to_owned(),
        via: via.to_owned(),
        at: crate::attest::now_rfc3339(),
        definition,
    };
    let previous = all.remove(key).and_then(|e| {
        if e.current.digest == now.digest {
            e.previous
        } else {
            Some(e.current)
        }
    });
    all.insert(
        key.to_owned(),
        Entry {
            current: now,
            previous,
        },
    );
    super::write_json(dir, FILE, &all);
}

/// Forgets what `key` names.
pub fn forget(dir: &Path, key: &str) {
    let mut all = read(dir);
    if all.remove(key).is_some() {
        super::write_json(dir, FILE, &all);
    }
}

/// `--trust-hook name=sha256:…`, split.
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
            "--trust-hook {value}: give <name>=sha256:<digest>, as `speccoder hooks` prints it"
        )),
    }
}

/// Trusts the hooks `flags` name, each only for its current digest
/// (`via = ci`): the non-interactive path.
///
/// # Errors
///
/// A flag names no hook, or a digest that is not the hook's current one
/// (the message names the current digest).
pub fn apply_flags(
    dir: &Path,
    hooks: &[Digested],
    flags: &[(String, String)],
    by: &str,
) -> Result<(), String> {
    for (name, digest) in flags {
        let Some(hook) = hooks.iter().find(|h| &h.def.name == name) else {
            return Err(format!("--trust-hook {name}: there is no hook `{name}`"));
        };
        if let Some((ext, _)) = &hook.def.extension {
            return Err(format!(
                "--trust-hook {name}: it comes from the extension {ext}, trusted as a whole: --trust-extension {ext}=<digest>"
            ));
        }
        if &hook.digest != digest {
            return Err(format!(
                "--trust-hook {name}: the hook is now {}, not {digest} — review it (speccoder hooks) before trusting",
                hook.digest
            ));
        }
        if state(dir, name, digest) != TrustState::Trusted {
            record(dir, hook, by, "ci");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(dir: &Path, timeout: u64) -> Digested {
        let defs = super::super::parse(&format!(
            "[hooks.licence]\nat = \"tick\"\nargv = [\"true\"]\ntimeout = {timeout}\n"
        ))
        .unwrap();
        super::super::digest(&defs[0], dir)
    }

    #[test]
    fn trust_holds_until_the_bytes_change() {
        let state_dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let h = hook(repo.path(), 5);
        let d = state_dir.path();
        assert_eq!(state(d, "licence", &h.digest), TrustState::Untrusted);
        record(d, &h, "Ada", "terminal");
        assert_eq!(state(d, "licence", &h.digest), TrustState::Trusted);
        let changed = hook(repo.path(), 6);
        assert_eq!(state(d, "licence", &changed.digest), TrustState::Changed);
        assert_eq!(last_trusted(d, "licence").unwrap().digest, h.digest);
        record(d, &changed, "Ada", "terminal");
        let e = &read(d)["licence"];
        assert_eq!(e.current.digest, changed.digest);
        assert_eq!(e.previous.as_ref().unwrap().digest, h.digest);
    }

    #[test]
    fn a_flag_trusts_only_the_current_digest() {
        let state_dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let h = hook(repo.path(), 5);
        let d = state_dir.path();
        let stale = vec![("licence".to_owned(), "sha256:00".to_owned())];
        let e = apply_flags(d, std::slice::from_ref(&h), &stale, "ci").unwrap_err();
        assert!(e.contains(&h.digest), "{e}");
        assert_eq!(state(d, "licence", &h.digest), TrustState::Untrusted);
        let good = vec![("licence".to_owned(), h.digest.clone())];
        apply_flags(d, std::slice::from_ref(&h), &good, "ci").unwrap();
        assert_eq!(read(d)["licence"].current.via, "ci");
        assert!(apply_flags(d, &[h], &[("nope".into(), "sha256:1".into())], "ci").is_err());
        assert!(parse_flag("licence").is_err());
        assert_eq!(
            parse_flag("licence=sha256:ab").unwrap(),
            ("licence".to_owned(), "sha256:ab".to_owned())
        );
    }
}
