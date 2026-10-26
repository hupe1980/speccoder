//! Spec Kit's installed extensions: the one reader of `.specify/extensions*`.
//! `specify extension list --json` when the CLI is installed (read-only),
//! else the registry files it keeps.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::format::{Extension, Inventory, InventorySource};

/// The extensions installed in the project at `root`; `None` when it has
/// no extension files and the CLI lists nothing.
#[must_use]
pub fn inventory(root: &Path) -> Option<Inventory> {
    if !root.join(".specify/extensions").is_dir() && !root.join(".specify/extensions.yml").is_file()
    {
        return None;
    }
    cli(root).or_else(|| files(root))
}

/// The CLI's answer, if it is installed and answers within a few seconds.
fn cli(root: &Path) -> Option<Inventory> {
    let mut child = std::process::Command::new("specify")
        .args(["extension", "list", "--json"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if began.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
    parse_cli(&out)
}

/// `specify extension list --json`: `[{id, version, enabled, source: {kind}}]`.
#[must_use]
pub fn parse_cli(text: &str) -> Option<Inventory> {
    let start = text.find('[')?;
    let v: serde_json::Value = serde_json::from_str(text[start..].trim()).ok()?;
    let entries = v
        .as_array()?
        .iter()
        .filter_map(|e| {
            Some(Extension {
                id: e["id"].as_str()?.to_owned(),
                version: e["version"].as_str().unwrap_or_default().to_owned(),
                enabled: e["enabled"].as_bool().unwrap_or(false),
                source: e["source"]["kind"]
                    .as_str()
                    .or_else(|| e["source"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect();
    Some(Inventory {
        source: InventorySource::Cli,
        entries,
    })
}

/// The registry (`.specify/extensions/.registry`) and the installed list
/// (`.specify/extensions.yml`): an extension the list does not name is
/// listed as not enabled.
#[must_use]
pub fn files(root: &Path) -> Option<Inventory> {
    let registry: serde_json::Value =
        std::fs::read_to_string(root.join(".specify/extensions/.registry"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
    let installed = std::fs::read_to_string(root.join(".specify/extensions.yml"))
        .ok()
        .map(|t| installed_list(&t));
    let mut entries: Vec<Extension> = registry["extensions"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, e)| Extension {
            id: id.clone(),
            version: e["version"].as_str().unwrap_or_default().to_owned(),
            enabled: e["enabled"].as_bool().unwrap_or(false)
                && installed.as_ref().is_none_or(|i| i.contains(id)),
            source: e["source"].as_str().unwrap_or_default().to_owned(),
        })
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    if entries.is_empty() && installed.is_none() {
        return None;
    }
    Some(Inventory {
        source: InventorySource::Files,
        entries,
    })
}

/// The ids under `installed:` in `extensions.yml` (a YAML list of words).
fn installed_list(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("installed:") {
            inside = true;
            let rest = line.trim_start_matches("installed:").trim();
            // `installed: [bug, git]`
            if let Some(list) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                out.extend(
                    list.split(',')
                        .map(|w| w.trim().trim_matches(['"', '\'']).to_owned())
                        .filter(|w| !w.is_empty()),
                );
                inside = false;
            }
            continue;
        }
        if inside {
            match line.trim_start().strip_prefix("- ") {
                Some(id) if line.starts_with([' ', '-']) => {
                    out.push(id.trim().trim_matches(['"', '\'']).to_owned());
                }
                _ if line.trim().is_empty() => {}
                _ => inside = false,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_list_is_read_in_both_yaml_forms() {
        assert_eq!(
            installed_list("installed:\n- bug\n- git\nsettings:\n  x: 1\n"),
            ["bug", "git"]
        );
        assert_eq!(
            installed_list("installed: [bug, \"git\"]\n"),
            ["bug", "git"]
        );
        assert!(installed_list("settings: {}\n").is_empty());
    }

    #[test]
    fn the_cli_answer_names_id_version_state_and_source() {
        let inv = parse_cli("[{\"id\": \"bug\", \"version\": \"1.0.1\", \"enabled\": false, \"source\": {\"kind\": \"local\"}}]")
            .unwrap_or_else(|| panic!("parsed"));
        assert_eq!(inv.source, InventorySource::Cli);
        assert_eq!(inv.entries[0].source, "local");
        assert!(inv.enabled("bug").is_none());
    }
}
