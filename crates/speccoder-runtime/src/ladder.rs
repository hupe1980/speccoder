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

/// A quick change or bug that grew: the signals that hold for one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The lower unit.
    pub unit: String,
    /// Its task.
    pub task: String,
    /// The requirement the task cites, with its words.
    pub requirement: Option<(String, String)>,
    /// What holds, measured.
    pub signals: Vec<speccoder_core::ladder::Signal>,
    /// A bug's unit, not a quick change's.
    pub bug: bool,
}

/// `[ladder] promote_files`: more changed files than this is a signal.
const PROMOTE_FILES: u64 = 8;

fn promote_files(root: &Path) -> u64 {
    crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("ladder")?.get("promote_files")?.as_integer())
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(PROMOTE_FILES)
}

/// What decides verdicts: a change to it is a signal of its own.
fn judges(path: &str) -> bool {
    path == crate::verify::config::FILE || path.starts_with(".github/workflows/")
}

/// The manifest roots of `files` (a file outside any is the root's).
fn packages(root: &Path, files: &[String]) -> u64 {
    const MANIFESTS: [&str; 4] = ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
    let mut found: Vec<PathBuf> = files
        .iter()
        .map(|f| {
            let mut dir = Path::new(f).parent().map(Path::to_path_buf);
            while let Some(d) = dir {
                if MANIFESTS.iter().any(|m| root.join(&d).join(m).is_file()) {
                    return d;
                }
                dir = d.parent().map(Path::to_path_buf);
            }
            PathBuf::new()
        })
        .collect();
    found.sort();
    found.dedup();
    found.len() as u64
}

fn changed(root: &Path, commit: &str) -> Vec<String> {
    crate::git::Git::at(root)
        .run(&["diff", "--name-only", &format!("{commit}^"), commit])
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .map(str::to_owned)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Declined offers: `<unit>/<task>` → the signals that held then.
fn declined_file(project_state: &Path) -> PathBuf {
    project_state.join("ladder").join("declined.json")
}

fn declined(project_state: &Path) -> BTreeMap<String, Vec<String>> {
    std::fs::read_to_string(declined_file(project_state))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Records that the person declined `offer`: it comes back only when a
/// signal that did not hold then holds.
///
/// # Errors
///
/// The store cannot be written.
pub fn decline(project_state: &Path, offer: &Offer) -> Result<(), String> {
    let mut all = declined(project_state);
    all.insert(
        format!("{}/{}", offer.unit, offer.task),
        offer.signals.iter().map(|s| s.id.clone()).collect(),
    );
    let f = declined_file(project_state);
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    std::fs::write(&f, text).map_err(|e| format!("{}: {e}", f.display()))
}

/// The promotion offers of `unit`, a quick-change or bug unit: one per
/// task whose recorded change holds a signal, none declined on the same
/// signals, none promoted already.
#[must_use]
pub fn offers(root: &Path, project_state: &Path, unit: &str) -> Vec<Offer> {
    let format = crate::format::at(root);
    let rung = crate::oracle::rung(format, root, unit);
    if rung == speccoder_core::ladder::Rung::Feature {
        return Vec::new();
    }
    let report = crate::status::report(root, Some(unit));
    let Some(speccoder_core::CommandData::Report(r)) = report.data() else {
        return Vec::new();
    };
    let review = crate::review::report(root, Some(unit));
    let requirements: Vec<(String, String)> = match review.data() {
        Some(speccoder_core::CommandData::Review(v)) => v
            .requirements
            .iter()
            .map(|q| (q.id.clone(), q.text.clone()))
            .collect(),
        _ => Vec::new(),
    };
    let saved = crate::attempts::list(root, unit);
    let promoted = origins(project_state);
    let declined = declined(project_state);
    let limit = promote_files(root);
    let mut out = Vec::new();
    for t in &r.tasks {
        let attempts: Vec<&crate::attempts::Saved> =
            saved.iter().filter(|a| a.task == t.id).collect();
        let Some(commit) = t
            .commit
            .clone()
            .or_else(|| attempts.last().map(|a| a.commit.clone()))
        else {
            continue;
        };
        if promoted.values().flatten().any(|o| {
            matches!(o, Origin::PromotedFrom { from_unit, task, .. } if from_unit == unit && *task == t.id)
        }) {
            continue;
        }
        // The unit's own planning files (its tick among them) are not the change.
        let files: Vec<String> = changed(root, &commit)
            .into_iter()
            .filter(|f| !Path::new(f).components().any(|c| c.as_os_str() == unit))
            .collect();
        let requirement = requirements
            .iter()
            .find(|(id, _)| {
                t.title
                    .split_whitespace()
                    .any(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-') == id)
            })
            .cloned();
        let ambiguity = requirement.as_ref().map_or(0, |(_, text)| {
            let flagged = crate::shape::findings("requirement", text)
                .iter()
                .filter(|f| matches!(f.rule.as_str(), "vague" | "escape_clause" | "open_ended"))
                .count();
            (flagged + text.matches("[NEEDS CLARIFICATION").count()) as u64
        });
        let input = speccoder_core::ladder::SignalInput {
            files_changed: files.len() as u64,
            packages_changed: packages(root, &files),
            red_causes: attempts
                .iter()
                .filter(|a| {
                    !matches!(
                        a.outcome.as_str(),
                        "blocked" | "interrupted" | "awaiting_approval"
                    )
                })
                .map(|a| {
                    a.reason
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_owned()
                })
                .collect(),
            ambiguity_markers: ambiguity,
            judge_paths: files.iter().filter(|f| judges(f)).cloned().collect(),
            promote_files: limit,
        };
        let signals = speccoder_core::ladder::signals(&input);
        let before = declined.get(&format!("{unit}/{}", t.id));
        if signals.is_empty() || before.is_some_and(|b| signals.iter().all(|s| b.contains(&s.id))) {
            continue;
        }
        out.push(Offer {
            unit: unit.to_owned(),
            task: t.id.clone(),
            requirement,
            signals,
            bug: rung == speccoder_core::ladder::Rung::Bug,
        });
    }
    out
}

/// The conversations an offer carries by default: the ones that suggested
/// its requirement.
#[must_use]
pub fn suggested_by(project_state: &Path, offer: &Offer) -> Vec<String> {
    let requirement = offer.requirement.as_ref().map_or("", |(id, _)| id.as_str());
    of(
        project_state,
        &speccoder_core::ladder::subject(&offer.unit, requirement),
    )
    .into_iter()
    .filter_map(|x| match x {
        Origin::Asked { conversation, .. } => Some(conversation),
        _ => None,
    })
    .collect()
}

/// Promotion as an action: plan through creates a unit from the offer's
/// requirement word for word, carrying `conversations`.
#[must_use]
pub fn promotion(offer: &Offer, conversations: Vec<String>) -> speccoder_core::actions::Runnable {
    use speccoder_core::actions::{FeatureSource, PlanFrom, Runnable, Through};
    Runnable::PlanThrough {
        from: PlanFrom::Create {
            description: offer
                .requirement
                .clone()
                .map(|(_, w)| w)
                .unwrap_or_default(),
            source: FeatureSource::Promotion {
                task: format!("{}/{}", offer.unit, offer.task),
                conversations,
            },
        },
        through: Through::Tasks,
    }
}

/// `speccoder spec promote <unit>/<task>`: the offer for it, if one holds.
#[must_use]
pub fn offer_for(root: &Path, project_state: &Path, task: &str) -> Option<Offer> {
    let (unit, id) = task.rsplit_once('/')?;
    offers(root, project_state, unit)
        .into_iter()
        .find(|o| o.task == id)
}

/// `speccoder spec promote --decline`: declines the offer for `task`
/// (`<unit>/<task>`).
#[must_use]
pub fn decline_task(root: &Path, state: &Path, task: &str) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus};
    let plane = crate::worktree::project_state_dir(state, root);
    let Some((unit, id)) = task.rsplit_once('/') else {
        return CommandResult::new(
            "promote",
            CommandStatus::UsageError,
            "name the task as <unit>/<task>",
        );
    };
    let Some(offer) = offers(root, &plane, unit)
        .into_iter()
        .find(|o| o.task == id)
    else {
        return CommandResult::new(
            "promote",
            CommandStatus::NotFound,
            &format!("no promotion is offered for {task}"),
        );
    };
    match decline(&plane, &offer) {
        Ok(()) => CommandResult::ok(
            "promote",
            &format!(
                "not now: {task} stays a {}; offered again only when another signal holds",
                if offer.bug { "bug fix" } else { "quick change" }
            ),
        ),
        Err(e) => CommandResult::new("promote", CommandStatus::Error, &e),
    }
}

/// What the creating skill reads for a promotion: the description the
/// person confirmed, then each carried conversation word for word.
#[must_use]
pub fn promotion_input(
    root: &Path,
    state: &Path,
    description: &str,
    conversations: &[String],
) -> String {
    let mut out = description.trim().to_owned();
    for id in conversations {
        if let Some((words, _)) = crate::ask::words(root, state, id) {
            out.push_str(&format!(
                "\n\n<context kind=\"conversation\" id=\"{id}\">\nAn Ask conversation that led to this change; the person chose to carry it.\n\n{words}</context>"
            ));
        }
    }
    out
}

/// Once plan through made the new unit: its origin, written once.
pub fn promoted(
    root: &Path,
    state: &Path,
    made: &speccoder_core::CommandResult,
    task: &str,
    conversations: &[String],
    via: speccoder_core::actions::Via,
) {
    let Some(speccoder_core::CommandData::PlanningChain(r)) = made.data() else {
        return;
    };
    let (Some(unit), Some((from_unit, id))) = (r.unit.as_deref(), task.rsplit_once('/')) else {
        return;
    };
    let plane = crate::worktree::project_state_dir(state, root);
    let offer = offers(root, &plane, from_unit)
        .into_iter()
        .find(|o| o.task == id);
    let (requirement, words) = offer
        .as_ref()
        .and_then(|o| o.requirement.clone())
        .unwrap_or_default();
    let _ = record_origin(
        &plane,
        unit,
        vec![Origin::PromotedFrom {
            from_unit: from_unit.to_owned(),
            requirement,
            task: id.to_owned(),
            requirement_sha256: crate::integrity::sha256_hex(words.as_bytes()),
            conversations: conversations
                .iter()
                .filter_map(|c| crate::ask::words(root, state, c).map(|(_, d)| (c.clone(), d)))
                .collect(),
            signals: offer
                .map(|o| o.signals.into_iter().map(|s| s.id).collect())
                .unwrap_or_default(),
            by: crate::approvals::who_in(root),
            via,
            at: crate::bug::confirm::now(),
        }],
    );
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
