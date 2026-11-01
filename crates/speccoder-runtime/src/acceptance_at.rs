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

/// A person's ruling that a task's tests are wrong: one scenario's oracle
/// tests (an oracle amendment, nothing else of the oracle), or the task's
/// own test outside a story. The next run writes them again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruling {
    /// The feature.
    pub feature: String,
    /// The scenario (`US3-2`; OpenSpec: its name); none: the task's own
    /// test (a task outside a story).
    #[serde(default)]
    pub scenario: Option<String>,
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
        .filter(|r| r.feature == feature && r.story == story && r.scenario.is_some())
        .collect())
}

/// Forgets the rulings an amendment acted on.
///
/// # Errors
///
/// The list cannot be read or written.
pub fn acted_on(plane: &Path, feature: &str, scenarios: &[String]) -> Result<(), String> {
    write_rulings(plane, |l| {
        l.retain(|r| {
            !(r.feature == feature && r.scenario.as_ref().is_some_and(|s| scenarios.contains(s)))
        });
    })
    .map(|_| ())
}

/// The rulings on `task`'s own test (outside a story), oldest first. An
/// unreadable list is an error, never "none".
///
/// # Errors
///
/// The list exists but cannot be read.
pub fn rulings_for_task(plane: &Path, feature: &str, task: &str) -> Result<Vec<Ruling>, String> {
    Ok(rulings(plane)?
        .into_iter()
        .filter(|r| r.feature == feature && r.task == task && r.scenario.is_none())
        .collect())
}

/// Forgets the rulings on `task`'s own test: written again, or ticked.
///
/// # Errors
///
/// The list cannot be read or written.
pub fn acted_on_task(plane: &Path, feature: &str, task: &str) -> Result<(), String> {
    write_rulings(plane, |l| {
        l.retain(|r| !(r.feature == feature && r.task == task && r.scenario.is_none()));
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

/// Records a person's ruling that a task's tests are wrong: for a story
/// task one `scenario`'s oracle tests, for a task outside a story its own
/// test. The next run writes them again, the reason told to their author.
/// Refused, with the choices there are, for a scenario the task's story
/// does not have, a story task without one, a scenario on a task outside a
/// story, or a task without tests of its own.
#[must_use]
pub fn rule_tests_wrong(
    root: &Path,
    feature: Option<&str>,
    task: &str,
    scenario: Option<&str>,
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
    let Some(t) = f
        .tasks
        .as_ref()
        .and_then(|ts| ts.tasks.iter().find(|t| t.id == task))
    else {
        return refuse(CommandStatus::NotFound, format!("no task {task} in {name}"));
    };
    let openspec = opened.adapter.kind() == speccoder_core::spec::FormatKind::Openspec;
    let plane = crate::worktree::project_state_dir(state, Path::new(&snap.root));
    // The story the run judges the task in: the one a person had its
    // acceptance written at, its label; OpenSpec: the change.
    let story = if openspec {
        Some(name.clone())
    } else {
        story_at(&plane, &name, task).or_else(|| t.story.clone())
    };
    let known = opened.adapter.scenarios(snap, task);
    let ids = || {
        known
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let rules = opened.adapter.rules().judged;
    let unit_dir = f.feature.dir.clone();
    let scenario = match (&story, scenario) {
        (Some(_), Some(id)) if known.iter().any(|s| s.id == id) => Some(id.to_owned()),
        (Some(_), Some(id)) => {
            return refuse(
                CommandStatus::NotFound,
                format!("{id} is not a scenario of {task}'s story ({})", ids()),
            );
        }
        (Some(st), None) => {
            return refuse(
                CommandStatus::UsageError,
                format!(
                    "{task} is of {st}: name the scenario whose tests are wrong with --scenario ({})",
                    ids()
                ),
            );
        }
        (None, Some(_)) => {
            return refuse(
                CommandStatus::UsageError,
                format!("{task} is in no story: its own test is ruled, without --scenario"),
            );
        }
        (None, None)
            if known.is_empty()
                || crate::pipeline::acceptance::checks_only(t, &unit_dir, &rules) =>
        {
            return refuse(
                CommandStatus::UsageError,
                format!("{task} has no tests of its own to rule on: its checks judge it"),
            );
        }
        (None, None) => None,
    };
    let ruling = Ruling {
        feature: name.clone(),
        scenario: scenario.clone(),
        story: story.unwrap_or_default(),
        task: task.to_owned(),
        reason: reason.trim().to_owned(),
        by: crate::controls::person(root),
        via,
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let path = match write_rulings(&plane, |l| {
        // A scenario is ruled once (the latest reason); a task's own test
        // gathers every reason until it is written again.
        if scenario.is_some() {
            l.retain(|r| !(r.feature == name && r.scenario == scenario));
        }
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
    let (what, rest) = match &scenario {
        Some(id) => (
            format!("{id}'s tests are"),
            ", and nothing else of the story's oracle",
        ),
        None => (
            format!("{task}'s own test is"),
            ", your reason given to its author",
        ),
    };
    CommandResult::new(
        "task",
        CommandStatus::Ok,
        &format!(
            "{what} ruled wrong: the next run writes them again{rest} — `speccoder run --task {task}`"
        ),
    )
    .with_data(CommandData::Control(Box::new(ControlReport {
        feature: name,
        task: task.to_owned(),
        action: "tests_ruled_wrong".to_owned(),
        text: scenario.or_else(|| Some(task.to_owned())),
        record: Some(path.display().to_string()),
    })))
}
