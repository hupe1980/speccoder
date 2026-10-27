//! A person's decision that a story's acceptance tests are written at a
//! Setup or Foundational task (FR-034): the fix for a plan that implements
//! a story's behaviour before the story. Recorded in the state directory —
//! never an edit of the spec (constitution I) — and changing no order or
//! dependency (IV): only which task's acceptance author receives the
//! story's scenarios.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::actions::Via;
use speccoder_core::{CommandData, CommandResult, CommandStatus, ControlReport};

/// One decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The feature.
    pub feature: String,
    /// The story.
    pub story: String,
    /// The task whose acceptance carries the story's scenarios.
    pub task: String,
    /// Who decided.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// When (Unix seconds).
    pub at: u64,
}

fn file(plane: &Path) -> PathBuf {
    plane.join("decisions").join("acceptance-at.json")
}

/// Every recorded decision of the project whose state is `plane`.
#[must_use]
pub fn all(plane: &Path) -> Vec<Decision> {
    std::fs::read_to_string(file(plane))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The story `task` of `feature` writes the acceptance tests of, if any.
#[must_use]
pub fn story_at(plane: &Path, feature: &str, task: &str) -> Option<String> {
    all(plane)
        .into_iter()
        .find(|d| d.feature == feature && d.task == task)
        .map(|d| d.story)
}

/// Records (or, with `revoke`, removes) the decision for `story` of the
/// project at `root`; `state` is the state directory.
#[must_use]
pub fn decide(
    root: &Path,
    feature: Option<&str>,
    d: (&str, &str, bool),
    via: Via,
    state: &Path,
) -> CommandResult {
    let (story, task, revoke) = d;
    let refuse = |status, m: String| CommandResult::new("task", status, &m);
    let Ok(opened) = crate::format::open(root, feature) else {
        return refuse(
            CommandStatus::NotAProject,
            "no spec project here".to_owned(),
        );
    };
    let Some(f) = opened.snapshot.feature.as_ref() else {
        return refuse(
            CommandStatus::SelectionNeeded,
            "no feature selected".to_owned(),
        );
    };
    let name = f.feature.name.clone();
    let structure = opened.adapter.plan_structure(&opened.snapshot);
    let Some(planned) = structure
        .as_ref()
        .and_then(|s| s.tasks.iter().find(|t| t.id == task))
    else {
        return refuse(
            CommandStatus::NotFound,
            format!("{task} is not a task of {name} with a phase structure"),
        );
    };
    if !matches!(
        planned.phase,
        crate::format::PhaseKind::Setup | crate::format::PhaseKind::Foundational
    ) {
        return refuse(
            CommandStatus::UsageError,
            format!(
                "{task} is not a Setup or Foundational task: its own story's acceptance applies"
            ),
        );
    }
    if !structure
        .as_ref()
        .is_some_and(|s| s.stories.contains_key(story))
    {
        return refuse(
            CommandStatus::NotFound,
            format!("{story} is not a story of {name}"),
        );
    }
    let plane = crate::worktree::project_state_dir(state, Path::new(&opened.snapshot.root));
    let mut list = all(&plane);
    list.retain(|x| !(x.feature == name && x.story == story));
    if !revoke {
        list.push(Decision {
            feature: name.clone(),
            story: story.to_owned(),
            task: task.to_owned(),
            by: crate::controls::person(root),
            via,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        });
    }
    let path = file(&plane);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_owned()),
            )
        });
    if let Err(e) = written {
        return refuse(
            CommandStatus::Error,
            format!("cannot write {}: {e}", path.display()),
        );
    }
    let message = if revoke {
        format!("{story}'s acceptance tests are written by its own tasks again")
    } else {
        format!(
            "{task} writes {story}'s acceptance tests: its tick can be verified on {story}'s scenarios"
        )
    };
    CommandResult::new("task", CommandStatus::Ok, &message).with_data(CommandData::Control(
        Box::new(ControlReport {
            feature: name,
            task: task.to_owned(),
            action: if revoke {
                "acceptance_at_revoked"
            } else {
                "acceptance_at"
            }
            .to_owned(),
            text: Some(story.to_owned()),
            record: Some(path.display().to_string()),
        }),
    ))
}
