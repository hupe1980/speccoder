//! Chips: what is on screen, given to Ask as context. The engine resolves
//! a feature, a task or a file range itself; anything else carries the
//! words the view shows. Every chip gets a digest, so a changed one says so.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::CommandData;
use speccoder_core::ask::Chip;

/// At most this many per turn.
pub const MAX: usize = 8;

/// What a chip's content may be at most.
const CAP: usize = 24_000;

/// A chip as an interface names it.
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ChipRef {
    /// `feature`, `task`, `file_range`, or another kind with `text`.
    pub kind: String,
    /// `{feature}`, `{task, feature?}`, `{path, start?, end?}`.
    #[serde(default)]
    pub reference: Value,
    /// What the chip reads as; the engine's own label wins where it has one.
    #[serde(default)]
    pub label: Option<String>,
    /// The words the view shows, for a kind the engine does not resolve.
    #[serde(default)]
    pub text: Option<String>,
    /// The digest when it was added: a different one now marks it changed.
    #[serde(default)]
    pub digest: Option<String>,
    /// The current view's subject.
    #[serde(default)]
    pub from_view: bool,
}

/// `r`'s content and its chip (numbered `id`).
///
/// # Errors
///
/// It points at nothing, outside the workspace, or at a credential.
pub fn resolve(root: &Path, id: &str, r: &ChipRef) -> Result<(Chip, String), String> {
    let (label, content) = match r.kind.as_str() {
        "feature" => feature(root, r.reference["feature"].as_str())?,
        "task" => task(
            root,
            r.reference["feature"].as_str(),
            r.reference["task"]
                .as_str()
                .ok_or("a task chip names its task")?,
        )?,
        "file_range" => file(root, &r.reference)?,
        _ => {
            let text = r
                .text
                .clone()
                .filter(|t| !t.trim().is_empty())
                .ok_or("this chip carries no words")?;
            (
                r.label.clone().unwrap_or_else(|| r.kind.replace('_', " ")),
                text,
            )
        }
    };
    let content: String = content.chars().take(CAP).collect();
    let digest = crate::integrity::sha256_hex(content.as_bytes());
    let chip = Chip {
        id: id.to_owned(),
        kind: r.kind.clone(),
        label,
        reference: r.reference.clone(),
        changed: r.digest.as_deref().is_some_and(|d| d != digest),
        digest,
        from_view: r.from_view,
    };
    Ok((chip, content))
}

/// The message the model reads: the person's words, then each chip.
#[must_use]
pub fn message(text: &str, chips: &[(Chip, String)]) -> String {
    let mut out = text.trim().to_owned();
    for (chip, content) in chips {
        let changed = if chip.changed {
            " (changed since it was added)"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n\n<context id=\"{}\" kind=\"{}\" label=\"{}\"{changed}>\n{content}\n</context>",
            chip.id,
            chip.kind,
            chip.label.replace('"', "'"),
        ));
    }
    out
}

fn report(root: &Path, feature: Option<&str>) -> Result<Box<speccoder_core::StatusReport>, String> {
    let report = crate::status::report(root, feature);
    match report.data() {
        Some(CommandData::Report(r)) => Ok(r.clone()),
        _ => Err(report.message().to_owned()),
    }
}

fn feature(root: &Path, feature: Option<&str>) -> Result<(String, String), String> {
    let r = report(root, feature)?;
    let tasks: Vec<String> = r
        .tasks
        .iter()
        .map(|t| format!("- {} [{}] {}", t.id, t.state, t.title))
        .collect();
    Ok((
        r.feature.clone(),
        format!("Feature {}\n\nTasks:\n{}", r.feature, tasks.join("\n")),
    ))
}

fn task(root: &Path, feature: Option<&str>, id: &str) -> Result<(String, String), String> {
    let r = report(root, feature)?;
    let t = r
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("no task {id} in {}", r.feature))?;
    let statements = t
        .commit
        .as_deref()
        .map(|c| crate::evidence::statements(root, c))
        .unwrap_or_default();
    let rows = crate::evidence::rows(t, &statements, None);
    Ok((
        format!("{} {}", t.id, t.title),
        json!({ "feature": r.feature, "task": t.id, "title": t.title, "phase": t.phase, "state": t.state, "evidence": rows })
            .to_string(),
    ))
}

fn file(root: &Path, reference: &Value) -> Result<(String, String), String> {
    let path = reference["path"]
        .as_str()
        .ok_or("a file chip names its path")?;
    let rel = crate::tools::paths::normalise(path).map_err(|e| e.0)?;
    // Resolved as the workspace's own reads are: a link out of the project
    // is refused, and the credential check is on what it points at.
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let full = crate::tools::paths::resolve_existing(&root, &rel).map_err(|e| e.0)?;
    let target = full
        .strip_prefix(&root)
        .map_or_else(|_| rel.clone(), |p| p.to_string_lossy().into_owned());
    let policy = crate::tools::policy::Policy::new(&[], &[]);
    if policy.protected().is_secret(&rel) || policy.protected().is_secret(&target) {
        return Err(format!("`{rel}` looks like a credential"));
    }
    let text = std::fs::read_to_string(&full).map_err(|e| format!("`{rel}`: {e}"))?;
    let lines: Vec<&str> = text.lines().collect();
    let line = |k: &str| reference[k].as_u64().and_then(|n| usize::try_from(n).ok());
    let start = line("start").unwrap_or(1).max(1);
    let end = line("end").unwrap_or(lines.len()).min(lines.len());
    if start > end {
        return Err(format!("`{rel}` has no lines {start}–{end}"));
    }
    let numbered: Vec<String> = (start..=end)
        .map(|n| format!("{n}: {}", lines[n - 1]))
        .collect();
    let label = if start == 1 && end == lines.len() {
        rel.clone()
    } else {
        format!("{rel} {start}–{end}")
    };
    Ok((label, crate::secrets::mask(&numbered.join("\n"))))
}
