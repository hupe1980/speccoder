//! The current feature: which unit a command works on when none is named,
//! shown and switched by `speccoder feature`.
//!
//! speccoder keeps no pointer of its own: the current feature is the
//! format's (Spec Kit: `.specify/feature.json`, which Spec Kit's scripts,
//! `/speckit-*` in an editor and speccoder all read), so every tool agrees
//! on it. `--feature` and the format's variable still override it for one
//! command.

use std::path::Path;

use speccoder_core::snapshot::{FeatureRef, ProjectSnapshot, SelectionReason, closest};
use speccoder_core::{CommandData, CommandResult, CommandStatus, FeatureEntry, FeaturesReport};

use crate::format::Adapter;

/// How the current feature was chosen, in words.
#[must_use]
pub fn reason_text(reason: SelectionReason, format: &dyn Adapter) -> String {
    let env = format
        .planning()
        .current_env
        .unwrap_or("the format's variable");
    let unit = format.messages().unit;
    match reason {
        SelectionReason::Argument => "named with --feature".to_owned(),
        SelectionReason::Environment => format!("from {env}"),
        SelectionReason::Recorded => format
            .current_pointer()
            .map_or_else(|| "recorded".to_owned(), |p| format!("recorded in {p}")),
        SelectionReason::OnlyFeature => format!("the only {unit}"),
        SelectionReason::None => format!("no current {unit}"),
    }
}

/// The feature `query` names: its exact name or directory; a number — the
/// numeric prefix (`2` is `002-…`), else the position in the list; or a
/// part of the name that only one feature has.
pub fn resolve<'f>(features: &'f [FeatureRef], query: &str) -> Result<&'f FeatureRef, String> {
    let q = query.trim().trim_end_matches('/');
    if let Some(f) = features
        .iter()
        .find(|f| f.name == q || f.dir == q || f.dir.ends_with(&format!("/{q}")))
    {
        return Ok(f);
    }
    if !q.is_empty() && q.chars().all(|c| c.is_ascii_digit()) {
        let n: u64 = q.parse().unwrap_or(0);
        if let Some(f) = features
            .iter()
            .find(|f| f.prefix.parse::<u64>().ok() == Some(n))
        {
            return Ok(f);
        }
        if let Some(f) = usize::try_from(n)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| features.get(i))
        {
            return Ok(f);
        }
    }
    let lower = q.to_lowercase();
    let found: Vec<&FeatureRef> = features
        .iter()
        .filter(|f| f.name.to_lowercase().contains(&lower))
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(match closest(features, q) {
            Some(c) => format!("no feature matches `{q}`; did you mean {}?", c.name),
            None => format!("no feature matches `{q}`"),
        }),
        many => Err(format!(
            "`{q}` matches {}: {}",
            many.len(),
            many.iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn open(
    path: &Path,
    feature: Option<&str>,
) -> Result<(&'static dyn Adapter, ProjectSnapshot), Box<CommandResult>> {
    let format = crate::format::at(path);
    match format.open(path, feature) {
        Some(s) => Ok((format, s)),
        None => Err(Box::new(CommandResult::new(
            "feature",
            CommandStatus::NotAProject,
            &(format.messages().not_a_project)(path),
        ))),
    }
}

fn report(format: &dyn Adapter, snap: &ProjectSnapshot, action: &str) -> FeaturesReport {
    let root = Path::new(&snap.root);
    let current = snap.feature.as_ref().map(|f| f.feature.name.clone());
    let features = snap
        .features
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let detail = format.inspect(root, Some(&f.name)).and_then(|s| s.feature);
            let (done, tasks) = detail
                .as_ref()
                .and_then(|d| d.tasks.as_ref())
                .map_or((0, 0), |t| {
                    (t.tasks.iter().filter(|t| t.done).count(), t.tasks.len())
                });
            let artifacts = detail.as_ref().map_or_else(Vec::new, |d| {
                [
                    ("spec", d.artifacts.spec),
                    ("plan", d.artifacts.plan),
                    ("tasks", d.artifacts.tasks),
                ]
                .into_iter()
                .filter(|(_, on)| *on)
                .map(|(n, _)| n.to_owned())
                .collect()
            });
            FeatureEntry {
                number: i + 1,
                name: f.name.clone(),
                dir: f.dir.clone(),
                current: current.as_deref() == Some(f.name.as_str()),
                done,
                tasks,
                artifacts,
            }
        })
        .collect();
    let reason = snap.selection.reason;
    FeaturesReport {
        unit: format.messages().unit.to_owned(),
        current,
        reason: serde_json::to_value(reason)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        reason_text: reason_text(reason, format),
        action: action.to_owned(),
        features,
    }
}

/// `speccoder feature`: every feature, the current one marked.
#[must_use]
pub fn list(path: &Path, feature: Option<&str>) -> CommandResult {
    let (format, snap) = match open(path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let r = report(format, &snap, "listed");
    let message = match (&r.current, r.features.len()) {
        (_, 0) => format!(
            "no {unit}s yet — `speccoder spec specify \"…\"` creates one",
            unit = r.unit
        ),
        (Some(c), _) => format!("current {}: {c} ({})", r.unit, r.reason_text),
        (None, n) => format!(
            "{n} {}s and none current — `speccoder feature <name or number>` picks one",
            r.unit
        ),
    };
    CommandResult::new("feature", CommandStatus::Ok, &message)
        .with_data(CommandData::Features(Box::new(r)))
}

/// `speccoder feature <query>`: makes the feature `query` names current.
#[must_use]
pub fn switch(path: &Path, query: &str) -> CommandResult {
    let (format, snap) = match open(path, None) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let f = match resolve(&snap.features, query) {
        Ok(f) => f.clone(),
        Err(e) => {
            return CommandResult::new("feature", CommandStatus::NotFound, &e)
                .with_code("NOT-FOUND")
                .with_fix(vec!["speccoder feature".to_owned()])
                .with_data(CommandData::Features(Box::new(report(
                    format, &snap, "listed",
                ))));
        }
    };
    let root = Path::new(&snap.root);
    let pointer = match format.set_current(root, Some(&f.dir)) {
        Ok(p) => p,
        Err(e) => return CommandResult::new("feature", CommandStatus::UsageError, &e),
    };
    let after = format.open(root, None).unwrap_or(snap);
    let r = report(format, &after, "switched");
    let progress = r
        .features
        .iter()
        .find(|e| e.name == f.name)
        .filter(|e| e.tasks > 0)
        .map(|e| format!(", {}/{} tasks done", e.done, e.tasks))
        .unwrap_or_default();
    // The variable outranks the pointer: say so rather than pretend.
    let shadowed = format
        .planning()
        .current_env
        .filter(|v| std::env::var(v).is_ok_and(|x| !x.is_empty()))
        .map(|v| format!("; {v} is set in this shell and still wins until you unset it"))
        .unwrap_or_default();
    CommandResult::new(
        "feature",
        CommandStatus::Ok,
        &format!(
            "switched to {}{progress} (recorded in {pointer}{}){shadowed}",
            f.name,
            Some(format.messages().current_shared)
                .filter(|s| !s.is_empty())
                .map(|s| format!(": {s}"))
                .unwrap_or_default()
        ),
    )
    .with_data(CommandData::Features(Box::new(r)))
}

/// `speccoder feature --clear`.
#[must_use]
pub fn clear(path: &Path) -> CommandResult {
    let (format, snap) = match open(path, None) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let root = Path::new(&snap.root);
    let pointer = match format.set_current(root, None) {
        Ok(p) => p,
        Err(e) => return CommandResult::new("feature", CommandStatus::UsageError, &e),
    };
    let after = format.open(root, None).unwrap_or(snap);
    let r = report(format, &after, "cleared");
    let message = match &r.current {
        Some(c) => format!(
            "removed {pointer}; {c} is still current ({})",
            r.reason_text
        ),
        None => format!("removed {pointer}; no {} is current", r.unit),
    };
    CommandResult::new("feature", CommandStatus::Ok, &message)
        .with_data(CommandData::Features(Box::new(r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(names: &[&str]) -> Vec<FeatureRef> {
        names
            .iter()
            .map(|n| FeatureRef {
                dir: format!("specs/{n}"),
                name: (*n).to_owned(),
                prefix: n.split('-').next().unwrap_or_default().to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_feature_is_named_by_name_number_or_a_unique_part() {
        let f = refs(&["001-greet-function", "002-farewell", "010-greet-twice"]);
        assert_eq!(
            resolve(&f, "002-farewell").map(|f| f.name.as_str()),
            Ok("002-farewell")
        );
        assert_eq!(
            resolve(&f, "specs/002-farewell/").map(|f| f.name.as_str()),
            Ok("002-farewell")
        );
        assert_eq!(
            resolve(&f, "2").map(|f| f.name.as_str()),
            Ok("002-farewell")
        );
        assert_eq!(
            resolve(&f, "10").map(|f| f.name.as_str()),
            Ok("010-greet-twice")
        );
        assert_eq!(
            resolve(&f, "fare").map(|f| f.name.as_str()),
            Ok("002-farewell")
        );
        let e = resolve(&f, "greet").unwrap_err();
        assert!(
            e.contains("matches 2: 001-greet-function, 010-greet-twice"),
            "{e}"
        );
        let e = resolve(&f, "farwell").unwrap_err();
        assert!(e.contains("did you mean 002-farewell?"), "{e}");
        assert!(resolve(&f, "zzzzzzzz").unwrap_err().ends_with("`zzzzzzzz`"));
    }

    #[test]
    fn the_closest_name_ignores_the_number() {
        let f = refs(&["001-greet-function", "002-farewell"]);
        assert_eq!(
            closest(&f, "specs/003-greet-functon").map(|f| f.name.as_str()),
            Some("001-greet-function")
        );
        assert!(closest(&f, "something-else-entirely").is_none());
    }
}
