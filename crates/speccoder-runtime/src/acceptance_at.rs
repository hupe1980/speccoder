//! A person's decision that a story's acceptance tests are written at a
//! Setup or Foundational task: the fix for a plan that implements
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

/// Every recorded decision of the project whose state is `plane`; an
/// absent file is none, an unreadable one an error (never silently none).
///
/// # Errors
///
/// The file exists but cannot be read or parsed.
pub fn read_all(plane: &Path) -> Result<Vec<Decision>, String> {
    let path = file(plane);
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// [`read_all`], an unreadable file read as no decision.
#[must_use]
pub fn all(plane: &Path) -> Vec<Decision> {
    read_all(plane).unwrap_or_default()
}

/// A person's decision that a task's acceptance moves to its story's
/// checkpoint: it is judged by its regression checks, its scenarios at
/// the checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deferral {
    /// The feature.
    pub feature: String,
    /// The task deferred.
    pub task: String,
    /// The checkpoint when it was decided.
    pub checkpoint: String,
    /// Who decided.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// When (Unix seconds).
    pub at: u64,
}

fn deferrals_file(plane: &Path) -> PathBuf {
    plane.join("decisions").join("acceptance-deferred.json")
}

/// Every recorded deferral; an absent file is none, an unreadable one an
/// error (never silently none).
///
/// # Errors
///
/// The file exists but cannot be read or parsed.
pub fn read_deferrals(plane: &Path) -> Result<Vec<Deferral>, String> {
    let path = deferrals_file(plane);
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The deferral of `task` of `feature`, if a person made one.
#[must_use]
pub fn deferred(plane: &Path, feature: &str, task: &str) -> Option<Deferral> {
    read_deferrals(plane)
        .unwrap_or_default()
        .into_iter()
        .find(|d| d.feature == feature && d.task == task)
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
    let mut list = match read_all(&plane) {
        Ok(l) => l,
        Err(e) => {
            return refuse(
                CommandStatus::Error,
                format!("the recorded decisions cannot be read, so none is changed: {e}"),
            );
        }
    };
    // One decision per story, and one story per task.
    list.retain(|x| !(x.feature == name && (x.story == story || x.task == task)));
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

/// Records (or, with `revoke`, removes) a person's decision that `task`'s
/// acceptance moves to its story's checkpoint. Refused for a task of no
/// story, a task that names its scenarios, and the checkpoint itself —
/// nothing later would judge them.
#[must_use]
pub fn defer(
    root: &Path,
    feature: Option<&str>,
    task: &str,
    revoke: bool,
    via: Via,
    state: &Path,
) -> CommandResult {
    use speccoder_core::story::Share;
    let refuse = |status, m: String| CommandResult::new("task", status, &m);
    let Ok(opened) = crate::format::open(root, feature) else {
        return refuse(
            CommandStatus::NotAProject,
            "no spec project here".to_owned(),
        );
    };
    let snap = &opened.snapshot;
    let Some(f) = snap.feature.as_ref() else {
        return refuse(
            CommandStatus::SelectionNeeded,
            "no feature selected".to_owned(),
        );
    };
    let name = f.feature.name.clone();
    let Some(t) = f
        .tasks
        .as_ref()
        .and_then(|ts| ts.tasks.iter().find(|t| t.id == task))
    else {
        return refuse(
            CommandStatus::NotFound,
            format!("{task} is not a task of {name}"),
        );
    };
    let plane = crate::worktree::project_state_dir(state, Path::new(&snap.root));
    // One writer at a time (the app and a terminal may both decide): the
    // list is read, changed and replaced under a lock.
    let path = deferrals_file(&plane);
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return refuse(
            CommandStatus::Error,
            format!("cannot write {}: {e}", dir.display()),
        );
    }
    let lock = match std::fs::File::create(path.with_extension("lock")) {
        Ok(f) => f,
        Err(e) => {
            return refuse(
                CommandStatus::Error,
                format!("cannot lock {}: {e}", path.display()),
            );
        }
    };
    if let Err(e) = lock.lock() {
        return refuse(
            CommandStatus::Error,
            format!("cannot lock {}: {e}", path.display()),
        );
    }
    let mut list = match read_deferrals(&plane) {
        Ok(l) => l,
        Err(e) => {
            return refuse(
                CommandStatus::Error,
                format!("the recorded decisions cannot be read, so none is changed: {e}"),
            );
        }
    };
    let checkpoint = if revoke {
        String::new()
    } else {
        let scenarios = opened.adapter.scenarios(snap, task);
        let judged = opened.adapter.rules().judged;
        let share =
            crate::pipeline::acceptance::share_of(&crate::pipeline::acceptance::ShareInput {
                repo: root,
                plane: &plane,
                feature: &name,
                task,
                story: t.story.as_deref(),
                whole_unit: opened.adapter.kind() == speccoder_core::spec::FormatKind::Openspec,
                tasks: f.tasks.as_ref(),
                unit_dir: &f.feature.dir,
                judged: &judged,
                scenarios: &scenarios,
                as_deferred: true,
            });
        match share {
            Some(Share::Deferred { checkpoint }) => checkpoint,
            Some(Share::Checkpoint { .. }) => {
                return refuse(
                    CommandStatus::UsageError,
                    format!(
                        "{task} is its story's checkpoint: no later task would judge its scenarios"
                    ),
                );
            }
            Some(Share::Linked { scenarios }) => {
                return refuse(
                    CommandStatus::UsageError,
                    format!(
                        "{task} names its scenarios ({}): it is judged by them — change the link in the task list to move them",
                        scenarios.join(", ")
                    ),
                );
            }
            Some(Share::Authored { .. }) | None => {
                return refuse(
                    CommandStatus::UsageError,
                    format!("{task} is not a task of a story with a later code task"),
                );
            }
        }
    };
    let had = list.iter().any(|d| d.feature == name && d.task == task);
    if revoke && !had {
        return refuse(
            CommandStatus::Ok,
            format!("{task} was not deferred: nothing to take back"),
        );
    }
    list.retain(|d| !(d.feature == name && d.task == task));
    if !revoke {
        list.push(Deferral {
            feature: name.clone(),
            task: task.to_owned(),
            checkpoint: checkpoint.clone(),
            by: crate::controls::person(root),
            via,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        });
    }
    // Replaced whole: a reader never sees half a list.
    let tmp = path.with_extension("json.tmp");
    let written = std::fs::write(
        &tmp,
        serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_owned()),
    )
    .and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(e) = written {
        return refuse(
            CommandStatus::Error,
            format!("cannot write {}: {e}", path.display()),
        );
    }
    let message = if revoke {
        format!("{task} is judged by its own acceptance tests again")
    } else {
        format!(
            "{task} is judged by its regression checks; its story's scenarios at {checkpoint} — run it again: `speccoder run --task {task}`"
        )
    };
    CommandResult::new("task", CommandStatus::Ok, &message).with_data(CommandData::Control(
        Box::new(ControlReport {
            feature: name,
            task: task.to_owned(),
            action: if revoke { "defer_revoked" } else { "deferred" }.to_owned(),
            text: (!revoke).then_some(checkpoint),
            record: Some(path.display().to_string()),
        }),
    ))
}
