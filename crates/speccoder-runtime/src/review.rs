//! `speccoder review` (F-087): the person's review of the spec, one
//! requirement at a time. A decision — approved, or changes requested with a
//! comment — is bound to the exact line it was made on, and names who made
//! it; when the line changes, the decision lapses. It is recorded in
//! `review.json` beside the spec, to commit and share like the spec itself.
//! The model never writes it ([`crate::tools::paths::REVIEW_FILE`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ReviewItem, ReviewReport, ReviewTotals,
};

use crate::tools::paths::REVIEW_FILE;

/// One decision as recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Decision {
    /// `approved` or `changes_requested`.
    verdict: String,
    /// The line decided on.
    line: String,
    /// `sha256:<hex>` of the line, trailing space aside.
    sha256: String,
    /// Who (git's `user.name`, else the OS user).
    by: String,
    /// `cli`, `web`, `editor <name>`.
    via: String,
    /// Unix seconds.
    at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

/// The file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    requirements: BTreeMap<String, Decision>,
}

fn digest(line: &str) -> String {
    format!(
        "sha256:{}",
        crate::integrity::sha256_hex(line.trim_end().as_bytes())
    )
}

/// `- **FR-001**: Users MUST …` → `Users MUST …`.
fn wording(line: &str) -> String {
    let t = line.trim_start().trim_start_matches("- ");
    match t.strip_prefix("**").and_then(|r| r.split_once("**")) {
        Some((_, rest)) => rest.trim_start_matches(':').trim().to_owned(),
        None => t.trim().to_owned(),
    }
}

/// What a review is about: the feature, its directory, the project root.
struct Unit {
    feature: String,
    root: PathBuf,
    dir: String,
    format: &'static dyn crate::format::Adapter,
}

impl Unit {
    fn file(&self) -> PathBuf {
        self.root.join(&self.dir).join(REVIEW_FILE)
    }
    fn rel(&self) -> String {
        format!("{}/{REVIEW_FILE}", self.dir.trim_end_matches('/'))
    }
    /// The requirements as the spec in the checkout has them now.
    fn requirements(&self) -> Vec<(String, String)> {
        let Some(file) = self.format.requirements_file() else {
            return Vec::new();
        };
        let text =
            std::fs::read_to_string(self.root.join(&self.dir).join(file)).unwrap_or_default();
        self.format.requirements_in(&text)
    }
    fn record(&self) -> Record {
        std::fs::read_to_string(self.file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

fn unit(command: &str, path: &Path, feature: Option<&str>) -> Result<Unit, Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(command, &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    Ok(Unit {
        feature: f.feature.name.clone(),
        root: PathBuf::from(&snapshot.root),
        dir: f.feature.dir.clone(),
        format: crate::format::of(snapshot),
    })
}

/// Each requirement with its state against the record.
fn items(requirements: &[(String, String)], record: &Record) -> Vec<ReviewItem> {
    requirements
        .iter()
        .map(|(id, line)| {
            let d = record.requirements.get(id);
            let current = d.is_some_and(|d| d.sha256 == digest(line));
            let state = match d {
                None => "open",
                Some(_) if !current => "lapsed",
                Some(d) if d.verdict == "approved" => "approved",
                Some(_) => "changes_requested",
            };
            ReviewItem {
                id: id.clone(),
                text: wording(line),
                state: state.to_owned(),
                by: d.map(|d| d.by.clone()),
                at: d.map(|d| d.at),
                comment: d.and_then(|d| d.comment.clone()),
                decided_on: d.filter(|_| !current).map(|d| wording(&d.line)),
            }
        })
        .collect()
}

fn totals(items: &[ReviewItem]) -> ReviewTotals {
    let n = |s: &str| items.iter().filter(|i| i.state == s).count();
    ReviewTotals {
        approved: n("approved"),
        changes_requested: n("changes_requested"),
        open: n("open"),
        lapsed: n("lapsed"),
    }
}

/// The review state of the feature in `dir` (project-relative) under
/// `root`; `None` when the format has no requirement identifiers or the
/// spec names none.
#[must_use]
pub fn totals_of(
    root: &Path,
    format: &'static dyn crate::format::Adapter,
    feature: &str,
    dir: &str,
) -> Option<ReviewTotals> {
    let u = Unit {
        feature: feature.to_owned(),
        root: root.to_path_buf(),
        dir: dir.to_owned(),
        format,
    };
    let requirements = u.requirements();
    (!requirements.is_empty()).then(|| totals(&items(&requirements, &u.record())))
}

fn report_of(u: &Unit) -> ReviewReport {
    let items = items(&u.requirements(), &u.record());
    let skills = crate::skills::discover_with(&u.root, u.format.planning());
    let installed = |name: &str| {
        let full = format!("{}{name}", u.format.planning().prefix);
        skills.iter().any(|s| s.name == full && s.refused.is_none())
    };
    let totals = totals(&items);
    let revision = (totals.changes_requested > 0).then(|| revision_request(&items));
    ReviewReport {
        feature: u.feature.clone(),
        file: u.rel(),
        totals,
        revision,
        requirements: items,
        revise_with: u
            .format
            .planning()
            .revises
            .filter(|s| installed(s))
            .map(str::to_owned),
    }
}

fn message(r: &ReviewReport) -> String {
    let t = r.totals;
    let n = r.requirements.len();
    if n == 0 {
        return format!("{}'s spec names no requirement IDs to review", r.feature);
    }
    if t.complete() {
        return format!("{n}/{n} requirements approved");
    }
    let parts: Vec<String> = [
        (t.open, "to review"),
        (t.changes_requested, "with changes requested"),
        (t.lapsed, "changed since they were decided"),
    ]
    .iter()
    .filter(|(k, _)| *k > 0)
    .map(|(k, w)| format!("{k} {w}"))
    .collect();
    format!(
        "{}/{n} requirements approved; {}",
        t.approved,
        parts.join(", ")
    )
}

/// `speccoder review`: every requirement and its review.
#[must_use]
pub fn report(path: &Path, feature: Option<&str>) -> CommandResult {
    let u = match unit("review", path, feature) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let r = report_of(&u);
    CommandResult::new("review", CommandStatus::Ok, &message(&r))
        .with_data(CommandData::Review(Box::new(r)))
}

/// `speccoder review approve|request|clear [ID…]`: records the person's
/// decision on the requirements named (none: every one not approved on its
/// current wording) and writes `review.json`. `request` needs a comment.
#[must_use]
pub fn decide(
    path: &Path,
    feature: Option<&str>,
    ids: &[String],
    verdict: &str,
    comment: Option<&str>,
    via: &str,
) -> CommandResult {
    let u = match unit("review", path, feature) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let comment = comment.map(str::trim).filter(|c| !c.is_empty());
    if verdict == "request" && comment.is_none() {
        return CommandResult::new(
            "review",
            CommandStatus::UsageError,
            "a change request needs a comment: what should the requirement say?",
        )
        .with_code("USAGE");
    }
    let requirements = u.requirements();
    let mut record = u.record();
    let before = items(&requirements, &record);
    let chosen: Vec<&(String, String)> = if ids.is_empty() {
        requirements
            .iter()
            .filter(|(id, _)| before.iter().any(|i| i.id == *id && i.state != "approved"))
            .collect()
    } else {
        let unknown: Vec<&String> = ids
            .iter()
            .filter(|id| !requirements.iter().any(|(r, _)| r == *id))
            .collect();
        if !unknown.is_empty() {
            return CommandResult::new(
                "review",
                CommandStatus::NotFound,
                &format!(
                    "the spec has no {}; it names {}",
                    unknown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    requirements
                        .iter()
                        .map(|(r, _)| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_code("NOT-FOUND")
            .with_fix(vec!["speccoder review".to_owned()]);
        }
        requirements
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .collect()
    };
    let who = crate::approvals::Who::local_in(&u.root, via);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for (id, line) in &chosen {
        match verdict {
            "clear" => {
                record.requirements.remove(id);
            }
            "approve" | "request" => {
                record.requirements.insert(
                    id.clone(),
                    Decision {
                        verdict: if verdict == "approve" {
                            "approved"
                        } else {
                            "changes_requested"
                        }
                        .to_owned(),
                        line: line.trim_end().to_owned(),
                        sha256: digest(line),
                        by: who.name.clone(),
                        via: who.via.clone(),
                        at: now,
                        comment: comment.map(str::to_owned),
                    },
                );
            }
            other => {
                return CommandResult::new(
                    "review",
                    CommandStatus::UsageError,
                    &format!("`{other}` is not a review verdict: approve, request or clear"),
                )
                .with_code("USAGE");
            }
        }
    }
    // Decisions on requirements the spec no longer names go with them.
    record
        .requirements
        .retain(|id, _| requirements.iter().any(|(r, _)| r == id));
    let text = serde_json::to_string_pretty(&record).unwrap_or_default() + "\n";
    if let Err(e) = std::fs::write(u.file(), text) {
        return CommandResult::new("review", CommandStatus::Error, &format!("{}: {e}", u.rel()));
    }
    let r = report_of(&u);
    let word = match verdict {
        "approve" => "approved",
        "request" => "changes requested on",
        _ => "cleared",
    };
    let message = if chosen.is_empty() {
        format!("nothing to decide: {}", message(&r))
    } else {
        format!(
            "{word} {}; {} — commit {} to share it",
            chosen
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            message(&r),
            r.file
        )
    };
    CommandResult::new("review", CommandStatus::Ok, &message)
        .with_data(CommandData::Review(Box::new(r)))
}

/// The arguments for the revising skill: each change request with its
/// requirement, so the spec is revised to say what the person asked.
#[must_use]
fn revision_request(items: &[ReviewItem]) -> String {
    let lines: Vec<String> = items
        .iter()
        .filter(|i| i.state == "changes_requested")
        .map(|i| {
            format!(
                "- {} (\"{}\"): {}",
                i.id,
                i.text,
                i.comment.as_deref().unwrap_or_default()
            )
        })
        .collect();
    format!(
        "Revise the spec per the reviewer's change requests; keep every requirement ID, reword only what is asked:\n{}",
        lines.join("\n")
    )
}

/// The review records under `root`'s units, as they are before a hosted
/// skill runs: it must not change them, nor leave one in a unit it creates.
#[derive(Debug)]
pub struct Snapshot {
    root: PathBuf,
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
}

/// Every unit's review record under `root` (absent ones as `None`).
fn records(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let format = crate::format::at(root);
    let Some(snap) = format.inspect(root, None) else {
        return Vec::new();
    };
    snap.features
        .iter()
        .map(|f| {
            let file = Path::new(&snap.root).join(&f.dir).join(REVIEW_FILE);
            let bytes = std::fs::read(&file).ok();
            (file, bytes)
        })
        .collect()
}

/// The records as they are now.
#[must_use]
pub fn snapshot(root: &Path) -> Snapshot {
    Snapshot {
        root: root.to_path_buf(),
        files: records(root),
    }
}

/// Puts back every record that changed since `before`, and removes one in
/// a unit that appeared since; the paths restored.
pub fn restore(before: &Snapshot) -> Vec<String> {
    let mut restored = Vec::new();
    let now = records(&before.root);
    for (file, bytes) in &now {
        let was = before
            .files
            .iter()
            .find(|(f, _)| f == file)
            .and_then(|(_, b)| b.clone());
        if *bytes == was {
            continue;
        }
        let ok = match &was {
            Some(b) => std::fs::write(file, b).is_ok(),
            None => std::fs::remove_file(file).is_ok(),
        };
        if ok {
            restored.push(file.display().to_string());
        }
    }
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wording_is_the_line_without_its_markup() {
        assert_eq!(
            wording("- **FR-001**: Users MUST log in."),
            "Users MUST log in."
        );
        assert_eq!(digest("a  "), digest("a"));
    }
}
