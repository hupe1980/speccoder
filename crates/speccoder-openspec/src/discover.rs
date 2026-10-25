//! Where the OpenSpec project is, how it is configured, and which change is
//! selected — without OpenSpec's CLI: the nearest `openspec/` with
//! `config.yaml`, `specs/` or `changes/`, as OpenSpec's "nearest" root.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use speccoder_core::snapshot::{Diagnostic, Severity};

/// The directory of a project's specs and changes.
pub const DIR: &str = "openspec";

/// The workflow schema speccoder runs.
pub const SCHEMA: &str = "spec-driven";

/// The nearest ancestor of `start` (inclusive) containing an OpenSpec
/// directory, canonical.
#[must_use]
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = fs::canonicalize(start).ok()?;
    start
        .ancestors()
        .find(|d| {
            let o = d.join(DIR);
            o.is_dir()
                && (o.join("config.yaml").is_file()
                    || o.join("specs").is_dir()
                    || o.join("changes").is_dir()
                    || o.join("project.md").is_file())
        })
        .map(Path::to_path_buf)
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    schema: Option<String>,
    context: Option<String>,
    store: Option<serde_json::Value>,
}

/// `openspec/config.yaml`, as far as speccoder reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `schema` (default `spec-driven`).
    pub schema: String,
    /// `context`: constraints for every artifact and operation.
    pub context: Option<String>,
    /// `store:` is set (OpenSpec's beta shared stores).
    pub store: bool,
    /// There is a `config.yaml` at all.
    pub present: bool,
}

/// Reads `openspec/config.yaml` under `root`.
pub fn config(root: &Path, diags: &mut Vec<Diagnostic>) -> Config {
    let path = root.join(DIR).join("config.yaml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Config {
            schema: SCHEMA.to_owned(),
            context: None,
            store: false,
            present: false,
        };
    };
    let raw: RawConfig = if text.trim().is_empty() || is_only_comments(&text) {
        RawConfig::default()
    } else {
        match serde_saphyr::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "config_unreadable",
                        format!("openspec/config.yaml: {e}"),
                    )
                    .at("openspec/config.yaml"),
                );
                RawConfig::default()
            }
        }
    };
    Config {
        schema: raw.schema.unwrap_or_else(|| SCHEMA.to_owned()),
        context: raw.context.filter(|c| !c.trim().is_empty()),
        store: raw.store.is_some_and(|s| !s.is_null()),
        present: true,
    }
}

fn is_only_comments(text: &str) -> bool {
    text.lines()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
}

/// `schema` of a change's `.openspec.yaml`, if any.
#[must_use]
pub fn change_schema(change_dir: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Meta {
        schema: Option<String>,
    }
    let text = fs::read_to_string(change_dir.join(".openspec.yaml")).ok()?;
    serde_saphyr::from_str::<Meta>(&text).ok()?.schema
}

/// Active changes under `root`: the directories directly under
/// `openspec/changes/` except `archive`, sorted.
#[must_use]
pub fn changes(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join(DIR).join("changes")) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "archive" && !n.starts_with('.'))
        .collect();
    ids.sort();
    ids
}

/// Capability files under `dir` (`…/specs`): (capability, path) pairs, the
/// capability being the path between `specs/` and `/spec.md`.
#[must_use]
pub fn spec_files(dir: &Path) -> Vec<(String, PathBuf)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(base, &p, out);
            } else if p.file_name().is_some_and(|n| n == "spec.md")
                && let Some(cap) = p
                    .parent()
                    .and_then(|d| d.strip_prefix(base).ok())
                    .map(|c| c.to_string_lossy().replace('\\', "/"))
                    .filter(|c| !c.is_empty())
            {
                out.push((cap, p.clone()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}
