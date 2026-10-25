//! What a person can do now (D-087): one list, computed by the runtime and
//! rendered by every interface — the dashboard's `n`, `c` and `:` today, a
//! desktop app or another workbench later. Every action is a CLI command
//! too, so nothing an interface offers is out of reach of a script.

use serde::{Deserialize, Serialize};

/// How an interface runs an action itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Runnable {
    /// A planning skill (`speccoder spec <name> <arguments>`).
    Skill {
        /// The skill.
        name: String,
        /// Its arguments (the input, when one is asked for).
        arguments: String,
    },
    /// `speccoder run`, the feature or one task.
    Run {
        /// One task, or the feature's ready tasks.
        task: Option<String>,
    },
    /// Commit exactly these planning files.
    Commit {
        /// Repository-relative paths.
        paths: Vec<String>,
    },
    /// Approve the task's waiting call.
    Approve {
        /// The task.
        task: String,
    },
    /// Reject it (the input is the reason).
    Reject {
        /// The task.
        task: String,
    },
    /// Tick the task on the person's word (the input is the reason).
    Accept {
        /// The task.
        task: String,
    },
    /// Skip the task (the input is the reason).
    Skip {
        /// The task.
        task: String,
    },
    /// Leave a note for the task's next attempts (the input is the note).
    Nudge {
        /// The task.
        task: String,
    },
    /// Pick the current feature.
    Feature,
    /// Push the feature's branch and open (or update) its draft pull request.
    PullRequest,
}

/// One thing a person can do now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Action {
    /// `next`, `plan`, `work`, `task`, `feature` or `review`.
    pub group: String,
    /// What it does, in a few words.
    pub label: String,
    /// Why, or what follows (shown beside it).
    pub why: String,
    /// The equivalent command; `"…"` marks the input.
    pub command: String,
    /// How an interface runs it; `None`: run the command yourself (it leaves
    /// the machine, or it is not something an interface hosts).
    pub run: Option<Runnable>,
    /// Why it cannot run now; `None`: it can.
    pub blocked: Option<String>,
    /// What to ask for before it runs (`Describe the feature`).
    pub input: Option<String>,
    /// It leaves the machine (a push, a pull request): what an interface
    /// says in a confirmation before it runs it.
    pub confirm: Option<String>,
}

impl Action {
    /// It can be run from an interface, now.
    #[must_use]
    pub const fn runnable(&self) -> bool {
        self.run.is_some() && self.blocked.is_none()
    }
}

/// The primary action: the first runnable one of the `next` group.
#[must_use]
pub fn primary(actions: &[Action]) -> Option<&Action> {
    actions.iter().find(|a| a.group == "next" && a.runnable())
}

/// Where a step of the journey is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    /// Done.
    Done,
    /// The one to take now.
    Now,
    /// Later.
    Todo,
    /// Passed without its file: a task list written without a spec or a
    /// plan. Not a step left to take.
    Skipped,
}

/// One step of a feature's journey: `spec`, `plan`, `tasks`, `commit`,
/// `run`, `review`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct JourneyStep {
    /// Its name.
    pub name: String,
    /// Where it is.
    pub state: StepState,
}

/// Facts the journey is derived from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JourneyFacts {
    /// `spec.md` exists.
    pub spec: bool,
    /// `plan.md` exists.
    pub plan: bool,
    /// The task list exists.
    pub tasks: bool,
    /// Planning files are not committed.
    pub uncommitted: bool,
    /// Tasks in the list.
    pub total: usize,
    /// Of them ticked or skipped.
    pub finished: usize,
}

/// The journey from a spec to a reviewed branch, and where the feature is
/// on it: everything before the first open step is done (or skipped, when a
/// later planning file exists without it), that one is now.
#[must_use]
pub fn journey(f: JourneyFacts) -> Vec<JourneyStep> {
    let skipped = [!f.spec && (f.plan || f.tasks), !f.plan && f.tasks];
    let done = [
        ("spec", f.spec),
        ("plan", f.plan),
        ("tasks", f.tasks),
        ("commit", f.tasks && !f.uncommitted),
        ("run", f.total > 0 && f.finished == f.total),
        ("review", false),
    ];
    let passed = |i: usize| skipped.get(i).copied().unwrap_or(false);
    let now = done
        .iter()
        .enumerate()
        .position(|(i, (_, d))| !d && !passed(i))
        .unwrap_or(done.len());
    done.iter()
        .enumerate()
        .map(|(i, (name, d))| JourneyStep {
            name: (*name).to_owned(),
            state: if *d && i < now {
                StepState::Done
            } else if passed(i) && i < now {
                StepState::Skipped
            } else if i == now {
                StepState::Now
            } else {
                StepState::Todo
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(f: JourneyFacts) -> Vec<(String, StepState)> {
        journey(f).into_iter().map(|s| (s.name, s.state)).collect()
    }

    #[test]
    fn the_journey_marks_what_is_done_and_what_is_now() {
        use StepState::{Done, Now, Todo};
        let names = ["spec", "plan", "tasks", "commit", "run", "review"].map(String::from);
        let at = |v: [StepState; 6]| names.iter().cloned().zip(v).collect::<Vec<_>>();
        assert_eq!(
            states(JourneyFacts::default()),
            at([Now, Todo, Todo, Todo, Todo, Todo])
        );
        let planned = JourneyFacts {
            spec: true,
            plan: true,
            ..JourneyFacts::default()
        };
        assert_eq!(states(planned), at([Done, Done, Now, Todo, Todo, Todo]));
        let written = JourneyFacts {
            tasks: true,
            uncommitted: true,
            total: 3,
            ..planned
        };
        assert_eq!(states(written), at([Done, Done, Done, Now, Todo, Todo]));
        let running = JourneyFacts {
            uncommitted: false,
            finished: 1,
            ..written
        };
        assert_eq!(states(running), at([Done, Done, Done, Done, Now, Todo]));
        let finished = JourneyFacts {
            finished: 3,
            ..running
        };
        assert_eq!(states(finished), at([Done, Done, Done, Done, Done, Now]));
    }

    #[test]
    fn a_task_list_without_a_spec_skips_to_the_run() {
        use StepState::{Now, Skipped, Todo};
        let names = ["spec", "plan", "tasks", "commit", "run", "review"].map(String::from);
        let at = |v: [StepState; 6]| names.iter().cloned().zip(v).collect::<Vec<_>>();
        let only_tasks = JourneyFacts {
            tasks: true,
            total: 4,
            finished: 1,
            ..JourneyFacts::default()
        };
        assert_eq!(
            states(only_tasks),
            at([
                Skipped,
                Skipped,
                StepState::Done,
                StepState::Done,
                Now,
                Todo
            ])
        );
        // A plan without a spec: the spec is skipped, the tasks are next.
        let planned = JourneyFacts {
            plan: true,
            ..JourneyFacts::default()
        };
        assert_eq!(
            states(planned),
            at([Skipped, StepState::Done, Now, Todo, Todo, Todo])
        );
    }
}
