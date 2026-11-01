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

/// A person's ruling that one scenario's oracle tests are wrong: the next
/// run writes that scenario's tests again (an oracle amendment), and
/// nothing else of the story's oracle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruling {
    /// The feature.
    pub feature: String,
    /// The scenario (`US3-2`; OpenSpec: its name).
    pub scenario: String,
    /// Its story (`US3`; OpenSpec: the change).
    #[serde(default)]
    pub story: String,
    /// The task that met the tests.
    pub task: String,
    /// Why they are wrong (told to the author).
    pub reason: String,
    /// Who ruled.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// When (Unix seconds).
    pub at: u64,
}

fn rulings_file(plane: &Path) -> PathBuf {
    plane.join("decisions").join("oracle-rulings.json")
}

/// Every ruling not yet acted on; an absent file is none, an unreadable
/// one an error (never silently none).
///
/// # Errors
///
/// The file exists but cannot be read or parsed.
pub fn rulings(plane: &Path) -> Result<Vec<Ruling>, String> {
    let path = rulings_file(plane);
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Replaces the rulings whole, under a lock: a reader never sees half a list.
fn write_rulings(plane: &Path, change: impl FnOnce(&mut Vec<Ruling>)) -> Result<PathBuf, String> {
    let path = rulings_file(plane);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let lock = std::fs::File::create(path.with_extension("lock"))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    lock.lock()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut list = rulings(plane)?;
    change(&mut list);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_owned()),
    )
    .and_then(|()| std::fs::rename(&tmp, &path))
    .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The rulings on `story`'s scenarios of `feature`. An unreadable list is
/// an error, never "none".
///
/// # Errors
///
/// The list exists but cannot be read.
pub fn rulings_for(plane: &Path, feature: &str, story: &str) -> Result<Vec<Ruling>, String> {
    Ok(rulings(plane)?
        .into_iter()
        .filter(|r| r.feature == feature && r.story == story)
        .collect())
}

/// Forgets the rulings an amendment acted on.
///
/// # Errors
///
/// The list cannot be read or written.
pub fn acted_on(plane: &Path, feature: &str, scenarios: &[String]) -> Result<(), String> {
    write_rulings(plane, |l| {
        l.retain(|r| !(r.feature == feature && scenarios.contains(&r.scenario)));
    })
    .map(|_| ())
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

/// Records a person's ruling that `scenario`'s oracle tests are wrong
/// (`task` met them): the next run writes them again. Refused for an id
/// that is not a scenario of the task's story.
#[must_use]
pub fn rule_scenario(
    root: &Path,
    feature: Option<&str>,
    task: &str,
    scenario: &str,
    reason: &str,
    via: Via,
    state: &Path,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("task", status, &m);
    if reason.trim().is_empty() {
        return refuse(
            CommandStatus::UsageError,
            "a ruling needs a reason".to_owned(),
        );
    }
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
    let known = opened.adapter.scenarios(snap, task);
    if !known.iter().any(|s| s.id == scenario) {
        return refuse(
            CommandStatus::NotFound,
            format!(
                "{scenario} is not a scenario of {task}'s story ({})",
                known
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    let plane = crate::worktree::project_state_dir(state, Path::new(&snap.root));
    // The story the scenario is of: the task's label; OpenSpec: the change.
    let story = if opened.adapter.kind() == speccoder_core::spec::FormatKind::Openspec {
        name.clone()
    } else {
        f.tasks
            .as_ref()
            .and_then(|ts| ts.tasks.iter().find(|t| t.id == task))
            .and_then(|t| t.story.clone())
            .unwrap_or_default()
    };
    let ruling = Ruling {
        feature: name.clone(),
        scenario: scenario.to_owned(),
        story,
        task: task.to_owned(),
        reason: reason.trim().to_owned(),
        by: crate::controls::person(root),
        via,
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let path = match write_rulings(&plane, |l| {
        l.retain(|r| !(r.feature == name && r.scenario == scenario));
        l.push(ruling);
    }) {
        Ok(p) => p,
        Err(e) => {
            return refuse(
                CommandStatus::Error,
                format!("the ruling cannot be recorded: {e}"),
            );
        }
    };
    CommandResult::new(
        "task",
        CommandStatus::Ok,
        &format!(
            "{scenario}'s tests are ruled wrong: the next run writes them again, and nothing else of the story's oracle — `speccoder run --task {task}`"
        ),
    )
    .with_data(CommandData::Control(Box::new(ControlReport {
        feature: name,
        task: task.to_owned(),
        action: "tests_ruled_wrong".to_owned(),
        text: Some(scenario.to_owned()),
        record: Some(path.display().to_string()),
    })))
}
