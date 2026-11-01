//! What judges a task of a story — decided by code from the story's oracle,
//! never by a test author's choice. A story's acceptance is written once,
//! through the product, before its first code task (the oracle); every task
//! of the story is then judged by the progress it makes on it: the
//! scenarios it turns green. A task naming scenarios must leave them green;
//! the story's checkpoint (its last open code task) must leave every one
//! green. A tick claims exactly what it turned, so no scenario is claimed
//! twice and none is left unproven when the story closes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A scenario's state on a tree: green when every oracle test citing it
/// passes, red when one fails, untested when no kept test cites it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioState {
    /// Every test citing it passes.
    Green,
    /// A test citing it fails.
    Red,
    /// No kept test cites it.
    Untested,
}

/// What an attempt did to its story, by the oracle's states before and
/// after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// It turned these scenarios green (in the story's order).
    Verified {
        /// Red before, green after.
        turned: Vec<String>,
    },
    /// It turned none and broke none; the story goes on.
    InProgress,
    /// The checkpoint, every scenario green before it already.
    Closed,
    /// Scenarios green before it are red after it.
    Regressed {
        /// Green before, not green after.
        scenarios: Vec<String>,
    },
    /// Scenarios the task names are not green after it.
    LinksOpen {
        /// Its links still not green.
        scenarios: Vec<String>,
    },
    /// The checkpoint left scenarios not green.
    StoryOpen {
        /// Not green after it.
        scenarios: Vec<String>,
    },
}

impl Progress {
    /// Whether the attempt may tick.
    #[must_use]
    pub const fn passes(&self) -> bool {
        matches!(
            self,
            Self::Verified { .. } | Self::InProgress | Self::Closed
        )
    }
}

/// What an attempt did, from the oracle's states `before` and `after` it
/// (keyed by scenario id, iterated in the story's order by the caller's
/// `order`), the task's `links`, and whether it is the story's checkpoint.
/// A regression fails first; then the checkpoint's whole story — except
/// the scenarios its author declared `untestable`, shown, never blocking —
/// then the task's links.
#[must_use]
pub fn progress(
    order: &[String],
    before: &BTreeMap<String, ScenarioState>,
    after: &BTreeMap<String, ScenarioState>,
    links: &[String],
    checkpoint: bool,
    untestable: &[String],
) -> Progress {
    let state = |m: &BTreeMap<String, ScenarioState>, id: &str| {
        m.get(id).copied().unwrap_or(ScenarioState::Untested)
    };
    let green =
        |m: &BTreeMap<String, ScenarioState>, id: &str| state(m, id) == ScenarioState::Green;
    let pick = |f: &dyn Fn(&str) -> bool| -> Vec<String> {
        order.iter().filter(|id| f(id)).cloned().collect()
    };
    let regressed = pick(&|id| green(before, id) && !green(after, id));
    if !regressed.is_empty() {
        return Progress::Regressed {
            scenarios: regressed,
        };
    }
    if checkpoint {
        let open = pick(&|id| !green(after, id) && !untestable.iter().any(|u| u == id));
        if !open.is_empty() {
            return Progress::StoryOpen { scenarios: open };
        }
    }
    let open = pick(&|id| links.iter().any(|l| l == id) && !green(after, id));
    if !open.is_empty() {
        return Progress::LinksOpen { scenarios: open };
    }
    let turned = pick(&|id| !green(before, id) && green(after, id));
    match (turned.is_empty(), checkpoint) {
        (false, _) => Progress::Verified { turned },
        (true, true) => Progress::Closed,
        (true, false) => Progress::InProgress,
    }
}

/// What judges one open task of a story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Judged {
    /// The progress it makes on the story's oracle.
    Progress,
    /// The scenarios it names, which it must leave green.
    Linked {
        /// Their ids.
        scenarios: Vec<String>,
    },
    /// The story's checkpoint: every scenario must be green after it.
    Checkpoint,
}

impl Judged {
    /// `progress`, `linked` or `checkpoint`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Linked { .. } => "linked",
            Self::Checkpoint => "checkpoint",
        }
    }

    /// The scenarios it must leave green: its links (none otherwise).
    #[must_use]
    pub fn links(&self) -> &[String] {
        match self {
            Self::Linked { scenarios } => scenarios,
            _ => &[],
        }
    }
}

/// A story's scenarios as its oracle and ticks show them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StoryScenarios {
    /// The story (`US3`; for OpenSpec the change).
    pub story: String,
    /// Its scenarios in order.
    pub scenarios: Vec<ScenarioMark>,
    /// The task that closes it (none: every open task is planning work, or
    /// all are done).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// The story's oracle, once written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<OracleMark>,
    /// Every task done and every scenario green: the spec shows it, the
    /// report and the board leave it out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proven: bool,
}

/// Where a story's oracle was written, and who froze it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleMark {
    /// The `Oracle:` commit (short).
    pub commit: String,
    /// `speccoder run`, or the person who approved it at review.
    pub decided_by: String,
}

/// One scenario of a story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScenarioMark {
    /// Its id (`US3-1`).
    pub id: String,
    /// Its state at the branch's tip (red before the oracle exists).
    pub state: ScenarioState,
    /// The task whose tick turned it green, worded as it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turned_by: Option<String>,
    /// What must hold, as the oracle states it (given → must hold).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

/// One task of the story, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryTask<'a> {
    /// Its id.
    pub id: &'a str,
    /// Judged by the checks alone — planning work, or a step only a person
    /// takes (`[YOU]`): never a checkpoint.
    pub checks_only: bool,
    /// Ticked (or skipped, or accepted): it judges nothing any more.
    pub done: bool,
    /// The scenarios it names.
    pub links: &'a [String],
}

/// The story's checkpoint: its last open task that is judged by the
/// oracle. Whatever order the tasks run in, the one that closes the story
/// judges all of it; none once every such task is done.
#[must_use]
pub fn checkpoint<'a>(tasks: &[StoryTask<'a>]) -> Option<&'a str> {
    tasks
        .iter()
        .rev()
        .find(|t| !t.checks_only && !t.done)
        .map(|t| t.id)
}

/// The story's first open code task: where its oracle is written.
#[must_use]
pub fn first<'a>(tasks: &[StoryTask<'a>]) -> Option<&'a str> {
    tasks
        .iter()
        .find(|t| !t.checks_only && !t.done)
        .map(|t| t.id)
}

/// What judges `task` of the story `tasks`, whose scenarios are
/// `scenarios`. `None`: the story has no scenario, or the task is judged by
/// its checks alone. Links naming no scenario of the story are ignored
/// (the plan reports them); the checkpoint comes first.
#[must_use]
pub fn judged(tasks: &[StoryTask<'_>], task: &str, scenarios: &[String]) -> Option<Judged> {
    let me = tasks.iter().find(|t| t.id == task)?;
    if scenarios.is_empty() || me.checks_only {
        return None;
    }
    if checkpoint(tasks) == Some(task) {
        return Some(Judged::Checkpoint);
    }
    let links: Vec<String> = me
        .links
        .iter()
        .filter(|l| scenarios.contains(l))
        .cloned()
        .collect();
    Some(if links.is_empty() {
        Judged::Progress
    } else {
        Judged::Linked { scenarios: links }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ScenarioState::{Green, Red, Untested};
    use proptest::prelude::*;

    fn ids(n: usize) -> Vec<String> {
        (1..=n).map(|k| format!("US3-{k}")).collect()
    }

    fn states(s: &[ScenarioState]) -> BTreeMap<String, ScenarioState> {
        ids(s.len()).into_iter().zip(s.iter().copied()).collect()
    }

    const NONE: &[String] = &[];

    fn open(id: &str) -> StoryTask<'_> {
        StoryTask {
            id,
            checks_only: false,
            done: false,
            links: NONE,
        }
    }

    #[test]
    fn a_task_claims_what_it_turned_and_nothing_else() {
        let o = ids(3);
        assert_eq!(
            progress(
                &o,
                &states(&[Red, Red, Red]),
                &states(&[Green, Red, Green]),
                &[],
                false,
                &[],
            ),
            Progress::Verified {
                turned: vec!["US3-1".into(), "US3-3".into()]
            }
        );
        assert_eq!(
            progress(
                &o,
                &states(&[Green, Red, Red]),
                &states(&[Green, Red, Red]),
                &[],
                false,
                &[],
            ),
            Progress::InProgress
        );
        assert_eq!(
            progress(
                &o,
                &states(&[Green, Red, Red]),
                &states(&[Red, Green, Green]),
                &[],
                false,
                &[],
            ),
            Progress::Regressed {
                scenarios: vec!["US3-1".into()]
            },
            "a regression fails whatever else turned"
        );
    }

    #[test]
    fn links_and_the_checkpoint_must_be_green_after() {
        let o = ids(2);
        let link = vec!["US3-2".to_owned()];
        assert_eq!(
            progress(
                &o,
                &states(&[Red, Red]),
                &states(&[Green, Red]),
                &link,
                false,
                &[],
            ),
            Progress::LinksOpen {
                scenarios: link.clone()
            }
        );
        assert_eq!(
            progress(
                &o,
                &states(&[Red, Red]),
                &states(&[Green, Untested]),
                &[],
                true,
                &[],
            ),
            Progress::StoryOpen {
                scenarios: vec!["US3-2".into()]
            },
            "untested is not green"
        );
        assert_eq!(
            progress(
                &o,
                &states(&[Green, Green]),
                &states(&[Green, Green]),
                &[],
                true,
                &[],
            ),
            Progress::Closed
        );
    }

    #[test]
    fn a_scenario_its_author_could_not_test_never_blocks_the_story() {
        let o = ids(2);
        let untestable = vec!["US3-2".to_owned()];
        assert_eq!(
            progress(
                &o,
                &states(&[Red, Untested]),
                &states(&[Green, Untested]),
                &[],
                true,
                &untestable
            ),
            Progress::Verified {
                turned: vec!["US3-1".into()]
            }
        );
    }

    #[test]
    fn the_checkpoint_is_the_last_open_code_task_and_the_oracle_goes_first() {
        let mut docs = open("T022");
        docs.checks_only = true;
        let mut done = open("T018");
        done.done = true;
        let link = vec!["US3-2".to_owned()];
        let mut linked = open("T020");
        linked.links = &link;
        let tasks = [done, open("T019"), linked, open("T021"), docs];
        let sc = ids(2);
        assert_eq!(checkpoint(&tasks), Some("T021"));
        assert_eq!(first(&tasks), Some("T019"));
        assert_eq!(judged(&tasks, "T019", &sc), Some(Judged::Progress));
        assert_eq!(
            judged(&tasks, "T020", &sc),
            Some(Judged::Linked {
                scenarios: link.clone()
            })
        );
        assert_eq!(judged(&tasks, "T021", &sc), Some(Judged::Checkpoint));
        assert_eq!(judged(&tasks, "T022", &sc), None, "documentation");
        assert_eq!(judged(&tasks, "T019", &[]), None, "no scenario");
    }

    fn state() -> impl Strategy<Value = ScenarioState> {
        prop_oneof![Just(Green), Just(Red), Just(Untested)]
    }

    proptest! {
        /// The rule is a function of its inputs; a verified tick names what
        /// it turned and only that; a checkpoint never passes with a scenario
        /// not green; a green scenario never goes red on a passing tick.
        #[test]
        fn progress_is_honest(
            before in proptest::collection::vec(state(), 1..6),
            after_seed in proptest::collection::vec(state(), 6),
            link_mask in proptest::collection::vec(any::<bool>(), 6),
            checkpoint in any::<bool>(),
        ) {
            let n = before.len();
            let o = ids(n);
            let b = states(&before);
            let a = states(&after_seed[..n]);
            let links: Vec<String> = o.iter().zip(&link_mask).filter(|(_, m)| **m).map(|(id, _)| id.clone()).collect();
            let p = progress(&o, &b, &a, &links, checkpoint, &[]);
            prop_assert_eq!(&p, &progress(&o, &b, &a, &links, checkpoint, &[]));
            if let Progress::Verified { turned } = &p {
                prop_assert!(!turned.is_empty());
                for id in turned {
                    prop_assert!(b[id] != Green && a[id] == Green);
                }
            }
            if p.passes() {
                for id in &o {
                    prop_assert!(!(b[id] == Green && a[id] != Green));
                }
                for id in &links {
                    prop_assert_eq!(a[id], Green);
                }
                if checkpoint {
                    prop_assert!(o.iter().all(|id| a[id] == Green));
                }
            }
        }

        /// Ticks applied in sequence never claim a scenario twice: once
        /// green it stays green on every later passing tick.
        #[test]
        fn a_scenario_is_claimed_once(
            steps in proptest::collection::vec(proptest::collection::vec(state(), 4), 1..6),
        ) {
            let o = ids(4);
            let mut now = states(&[Red, Red, Red, Red]);
            let mut claimed: BTreeMap<String, usize> = BTreeMap::new();
            for (i, s) in steps.iter().enumerate() {
                let next = states(s);
                let p = progress(&o, &now, &next, &[], false, &[]);
                if p.passes() {
                    if let Progress::Verified { turned } = &p {
                        for id in turned {
                            prop_assert!(claimed.insert(id.clone(), i).is_none());
                        }
                    }
                    now = next;
                }
            }
        }
    }
}
