//! The hooks of a project: the `[hooks]` of `speccoder.toml`, each hook's
//! digest, whether a person trusted it, and its recent runs. Gates run in
//! [`gate`]; events are projected in [`events`] and delivered in
//! [`deliver`]. Nothing here runs before a person trusted the exact bytes.

pub mod deliver;
pub mod events;
pub mod gate;
pub mod report;
pub mod trust;

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest as _, Sha256};
pub use speccoder_core::hooks::{HookDef, HookFile, HookRun, Point, Role, Runner, TrustState};

/// Every `[hooks.<name>]` of the configuration `text`, by name.
///
/// # Errors
///
/// A `HOOK-MISCONFIGURED` message naming the hook and the fix.
pub fn parse(text: &str) -> Result<Vec<HookDef>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(hooks) = table.get("hooks") else {
        return Ok(Vec::new());
    };
    let Some(hooks) = hooks.as_table() else {
        return Err("[hooks] holds one table per hook: [hooks.<name>]".to_owned());
    };
    let mut out = hooks
        .iter()
        .map(|(name, v)| {
            let v = serde_json::to_value(v).map_err(|e| e.to_string())?;
            speccoder_core::hooks::parse(name, &v)
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The hooks of the project at `root`: its `[hooks]` and its extensions'.
///
/// # Errors
///
/// The configuration cannot be read, a hook is misconfigured, or an
/// extension cannot be loaded.
pub fn load(root: &Path) -> Result<Vec<HookDef>, String> {
    let mut hooks = match crate::verify::find_config(root) {
        Some(dir) => {
            let file = dir.join(crate::verify::config::FILE);
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            parse(&text)?
        }
        None => Vec::new(),
    };
    // An extension's hooks, trusted or not: an untrusted gate blocks. One
    // that cannot be loaded blocks every point, as a broken `[hooks]` table
    // does — its gates must not quietly fall away.
    for e in crate::extensions::load(root) {
        if !e.usable() {
            return Err(format!(
                "the extension {} cannot be loaded ({}): fix or remove it (speccoder extensions)",
                e.name,
                e.problems.join("; ")
            ));
        }
        hooks.extend(e.hooks);
    }
    hooks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(hooks)
}

/// The hooks of a configuration's gate tables, as the judge's configuration
/// hashes them: an observer's change does not move the hash.
#[must_use]
pub fn gate_tables(table: &toml::Table) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    if let Some(hooks) = table.get("hooks").and_then(toml::Value::as_table) {
        for (name, t) in hooks {
            if t.get("at").is_some()
                && let Ok(v) = serde_json::to_value(t)
            {
                out.insert(name.clone(), v);
            }
        }
    }
    if out.is_empty() {
        serde_json::Value::Null
    } else {
        speccoder_core::hooks::canonical(&serde_json::Value::Object(out))
    }
}

/// A hook with the digest of what it would run here.
#[derive(Debug, Clone)]
pub struct Digested {
    /// The definition.
    pub def: HookDef,
    /// `sha256:<hex>` over the definition and its files.
    pub digest: String,
    /// The repository files its `argv` names.
    pub files: Vec<HookFile>,
    /// Programs it names outside the repository (named in the trust prompt,
    /// not digested).
    pub outside: Vec<String>,
    /// What its trust is recorded under: its name, or `extension:<name>`
    /// for an extension's hook (whose digest is the extension's).
    pub trust_key: String,
}

impl Digested {
    /// Whether it may run, against the records in `dir`.
    #[must_use]
    pub fn trust(&self, dir: &Path) -> TrustState {
        trust::state(dir, &self.trust_key, &self.digest)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `def`'s digest with the files it runs as they are under `root`.
#[must_use]
pub fn digest(def: &HookDef, root: &Path) -> Digested {
    if let Some((extension, folder)) = &def.extension {
        let (digest, files) = crate::extensions::digest(Path::new(folder))
            .unwrap_or_else(|e| (format!("unreadable: {e}"), Vec::new()));
        return Digested {
            def: def.clone(),
            digest,
            files,
            outside: Vec::new(),
            trust_key: crate::extensions::trust::key(extension),
        };
    }
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut files = Vec::new();
    let mut outside = Vec::new();
    // Each declared file counts, present or not: one that appears later
    // changes the digest too.
    for f in &def.files {
        let content = std::fs::read(canonical_root.join(f)).ok();
        files.push(HookFile {
            path: f.clone(),
            digest: content.map_or_else(
                || "missing".to_owned(),
                |c| format!("sha256:{}", sha256_hex(&c)),
            ),
        });
    }
    for arg in def.argv().unwrap_or_default() {
        let candidate = Path::new(arg);
        let full = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            canonical_root.join(candidate)
        };
        let Ok(resolved) = std::fs::canonicalize(&full) else {
            continue;
        };
        if !resolved.is_file() {
            continue;
        }
        match resolved.strip_prefix(&canonical_root) {
            Ok(rel) => {
                let content = std::fs::read(&resolved).unwrap_or_default();
                let path = rel.to_string_lossy().replace('\\', "/");
                if !files.iter().any(|f: &HookFile| f.path == path) {
                    files.push(HookFile {
                        path,
                        digest: format!("sha256:{}", sha256_hex(&content)),
                    });
                }
            }
            // Absolute, or a link out of the repository: named, not digested.
            Err(_) => outside.push(arg.clone()),
        }
    }
    let mut bytes = def.definition.to_string().into_bytes();
    for f in &files {
        bytes.push(0);
        bytes.extend_from_slice(f.path.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(f.digest.as_bytes());
    }
    Digested {
        def: def.clone(),
        digest: format!("sha256:{}", sha256_hex(&bytes)),
        files,
        outside,
        trust_key: def.name.clone(),
    }
}

/// The files every hook of the project at `project` runs, relative to the
/// worktree `wt_root`: judge files, which the model may not write.
#[must_use]
pub fn judge_files(project: &Path, wt_root: &Path) -> Vec<String> {
    let prefix = std::fs::canonicalize(project)
        .ok()
        .zip(std::fs::canonicalize(wt_root).ok())
        .and_then(|(p, w)| p.strip_prefix(&w).ok().map(Path::to_path_buf))
        .unwrap_or_default();
    // An extension's files are protected by its folder (`tools::paths`).
    load(project)
        .unwrap_or_default()
        .iter()
        .filter(|d| d.extension.is_none())
        .flat_map(|d| digest(d, project).files)
        .map(|f| prefix.join(&f.path).to_string_lossy().replace('\\', "/"))
        .collect()
}

/// Where hooks keep their state for the project at `root`, under
/// speccoder's state directory `state`.
#[must_use]
pub fn dir(state: &Path, root: &Path) -> PathBuf {
    crate::worktree::project_state_dir(state, root).join("hooks")
}

/// The project's id, as an event's `source` names it.
#[must_use]
pub fn project_id(state: &Path, root: &Path) -> String {
    crate::worktree::project_state_dir(state, root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// How many runs each hook keeps.
pub const RECENT: usize = 20;

const RUNS: &str = "runs.json";

/// Every hook's recent runs, oldest first.
#[must_use]
pub fn runs(dir: &Path) -> std::collections::BTreeMap<String, Vec<HookRun>> {
    std::fs::read_to_string(dir.join(RUNS))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Adds `run` to `hook`'s recent runs, keeping the last [`RECENT`].
pub fn record_run(dir: &Path, hook: &str, run: HookRun) {
    let mut all = runs(dir);
    let list = all.entry(hook.to_owned()).or_default();
    list.push(run);
    let excess = list.len().saturating_sub(RECENT);
    list.drain(..excess);
    write_json(dir, RUNS, &all);
}

pub(crate) fn write_json(dir: &Path, name: &str, value: &impl Serialize) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let tmp = dir.join(format!(".{name}.tmp"));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
[hooks.licence]
at = "tick"
argv = ["sh", "scripts/licence.sh"]

[hooks.chat]
on = ["*"]
url = "https://hooks.example.com/x"
"#;

    #[test]
    fn hooks_are_read_by_name_and_a_bad_one_is_an_error() {
        let hooks = parse(TOML).unwrap();
        assert_eq!(
            hooks.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["chat", "licence"]
        );
        assert!(parse("[hooks]\nx = 1\n").is_err());
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn a_byte_of_the_definition_or_its_script_changes_the_digest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
        std::fs::write(dir.path().join("scripts/licence.sh"), "exit 0\n").unwrap();
        let hooks = parse(TOML).unwrap();
        let gate = &hooks[1];
        let first = digest(gate, dir.path());
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.files[0].path, "scripts/licence.sh");
        assert_eq!(digest(gate, dir.path()).digest, first.digest);
        std::fs::write(dir.path().join("scripts/licence.sh"), "exit 1\n").unwrap();
        assert_ne!(digest(gate, dir.path()).digest, first.digest);
        let reordered = parse(
            "[hooks.licence]\nargv = [\"sh\", \"scripts/licence.sh\"]\n# a comment\nat = \"tick\"\n",
        )
        .unwrap();
        assert_eq!(
            digest(&reordered[0], dir.path()).digest,
            digest(gate, dir.path()).digest
        );
        let longer =
            parse("[hooks.licence]\nat = \"tick\"\nargv = [\"sh\", \"scripts/licence.sh\"]\ntimeout = 61\n")
                .unwrap();
        assert_ne!(
            digest(&longer[0], dir.path()).digest,
            digest(gate, dir.path()).digest
        );
    }

    #[test]
    fn only_gates_join_the_judges_configuration() {
        let table: toml::Table = toml::from_str(TOML).unwrap();
        let v = gate_tables(&table);
        assert!(v.get("licence").is_some() && v.get("chat").is_none());
        let moved: toml::Table =
            toml::from_str(&TOML.replace("hooks.example.com", "other.example.com")).unwrap();
        assert_eq!(gate_tables(&moved), v);
    }

    #[test]
    fn a_hook_keeps_its_last_twenty_runs() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..25 {
            record_run(
                dir.path(),
                "licence",
                HookRun {
                    at: format!("{i}"),
                    what: "test".into(),
                    outcome: "passed".into(),
                    duration_ms: 1,
                    error: None,
                    findings: Vec::new(),
                    findings_error: None,
                },
            );
        }
        let r = &runs(dir.path())["licence"];
        assert_eq!(r.len(), RECENT);
        assert_eq!(r[0].at, "5");
    }
}
