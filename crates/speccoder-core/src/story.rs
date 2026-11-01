//! What judges a task of a story — decided by code, never by a test
//! author's choice. A task is judged by the scenarios it names; the story's
//! checkpoint (its last task that is not planning work) by every scenario
//! no earlier tick verified, together; a task a person deferred by its
//! regression checks, its scenarios going to the checkpoint; any other by
//! the story's scenarios, the author choosing. However the earlier tasks
//! went, every scenario of a finished story was judged.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// What judges one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Share {
    /// The scenarios the task names.
    Linked {
        /// Their ids.
        scenarios: Vec<String>,
    },
    /// The story's checkpoint: every scenario no earlier tick verified,
    /// with its links.
    Checkpoint {
        /// Their ids, in the story's order.
        scenarios: Vec<String>,
        /// Also one test of these scenarios together (two or more left).
        composition: bool,
    },
    /// The story's scenarios; the test author chooses.
    Authored {
        /// Their ids.
        scenarios: Vec<String>,
        /// The story's checkpoint, where what this task leaves is judged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checkpoint: Option<String>,
    },
    /// A person moved its acceptance to the checkpoint.
    Deferred {
        /// The task that judges its scenarios.
        checkpoint: String,
    },
}

impl Share {
    /// The scenarios a test author is given (none when deferred).
    #[must_use]
    pub fn scenarios(&self) -> &[String] {
        match self {
            Self::Linked { scenarios }
            | Self::Checkpoint { scenarios, .. }
            | Self::Authored { scenarios, .. } => scenarios,
            Self::Deferred { .. } => &[],
        }
    }

    /// `linked`, `checkpoint`, `authored` or `deferred`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Linked { .. } => "linked",
            Self::Checkpoint { .. } => "checkpoint",
            Self::Authored { .. } => "authored",
            Self::Deferred { .. } => "deferred",
        }
    }
}

/// A story's scenarios, each verified by a tick or left for its checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StoryScenarios {
    /// The story (`US3`; for OpenSpec the change).
    pub story: String,
    /// Its scenarios in order.
    pub scenarios: Vec<ScenarioMark>,
    /// The task that judges what is left (none: every task is planning work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

/// One scenario of a story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScenarioMark {
    /// Its id (`US3-1`).
    pub id: String,
    /// The task whose tick verified it, worded as it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
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

/// The story's checkpoint: its last open task that is judged by acceptance
/// tests. Whatever order the tasks run in, the one that closes the story
/// judges what is left; none once every such task is done.
#[must_use]
pub fn checkpoint<'a>(tasks: &[StoryTask<'a>]) -> Option<&'a str> {
    tasks
        .iter()
        .rev()
        .find(|t| !t.checks_only && !t.done)
        .map(|t| t.id)
}

/// What judges `task` of the story `tasks`, whose scenarios are `scenarios`
/// (in the story's order), of which earlier ticks verified `verified`, and
/// which a person deferred to the checkpoint when `deferred`. `None`: the
/// story has no scenario, or the task is judged by its checks alone.
///
/// Links naming no scenario of the story are ignored (the plan reports
/// them). The checkpoint comes first: a link never narrows what it judges.
#[must_use]
pub fn share(
    tasks: &[StoryTask<'_>],
    task: &str,
    scenarios: &[String],
    verified: &BTreeSet<String>,
    deferred: bool,
) -> Option<Share> {
    let me = tasks.iter().find(|t| t.id == task)?;
    if scenarios.is_empty() || me.checks_only {
        return None;
    }
    let links: Vec<String> = me
        .links
        .iter()
        .filter(|l| scenarios.contains(l))
        .cloned()
        .collect();
    let at = checkpoint(tasks);
    if at == Some(task) {
        let left: Vec<String> = scenarios
            .iter()
            .filter(|s| !verified.contains(*s) || links.contains(s))
            .cloned()
            .collect();
        let composition = left.len() >= 2;
        return Some(Share::Checkpoint {
            scenarios: left,
            composition,
        });
    }
    if !links.is_empty() {
        return Some(Share::Linked { scenarios: links });
    }
    if deferred && let Some(at) = at {
        return Some(Share::Deferred {
            checkpoint: at.to_owned(),
        });
    }
    Some(Share::Authored {
        scenarios: scenarios.to_vec(),
        checkpoint: at.map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ids(n: usize) -> Vec<String> {
        (1..=n).map(|k| format!("US3-{k}")).collect()
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
    fn the_real_case_t018_to_t022() {
        let mut docs = open("T022");
        docs.checks_only = true;
        let tasks = [open("T018"), open("T019"), open("T020"), open("T021"), docs];
        let sc = ids(2);
        let none = BTreeSet::new();
        assert_eq!(checkpoint(&tasks), Some("T021"));
        assert_eq!(
            share(&tasks, "T018", &sc, &none, true),
            Some(Share::Deferred {
                checkpoint: "T021".into()
            })
        );
        assert_eq!(
            share(&tasks, "T018", &sc, &none, false),
            Some(Share::Authored {
                scenarios: sc.clone(),
                checkpoint: Some("T021".into())
            })
        );
        assert_eq!(
            share(&tasks, "T021", &sc, &none, false),
            Some(Share::Checkpoint {
                scenarios: sc.clone(),
                composition: true
            })
        );
        assert_eq!(
            share(&tasks, "T022", &sc, &none, false),
            None,
            "documentation has no share"
        );
    }

    #[test]
    fn a_link_beats_a_deferral_and_never_narrows_the_checkpoint() {
        let one = vec!["US3-1".to_owned()];
        let unknown = vec!["US9-4".to_owned()];
        let tasks = [
            StoryTask {
                id: "T1",
                checks_only: false,
                done: false,
                links: &one,
            },
            StoryTask {
                id: "T2",
                checks_only: false,
                done: false,
                links: &unknown,
            },
            StoryTask {
                id: "T3",
                checks_only: false,
                done: false,
                links: &one,
            },
        ];
        let sc = ids(3);
        let verified: BTreeSet<String> = ["US3-1".to_owned()].into();
        assert_eq!(
            share(&tasks, "T1", &sc, &verified, true),
            Some(Share::Linked {
                scenarios: one.clone()
            })
        );
        assert_eq!(
            share(&tasks, "T2", &sc, &verified, false),
            Some(Share::Authored {
                scenarios: sc.clone(),
                checkpoint: Some("T3".into())
            })
        );
        assert_eq!(
            share(&tasks, "T3", &sc, &verified, false),
            Some(Share::Checkpoint {
                scenarios: ids(3),
                composition: true
            })
        );
    }

    #[test]
    fn a_person_s_step_is_never_the_checkpoint_and_a_done_one_hands_it_on() {
        let mut you = open("T3");
        you.checks_only = true;
        let mut done = open("T2");
        done.done = true;
        let tasks = [open("T1"), done, you];
        assert_eq!(
            checkpoint(&tasks),
            Some("T1"),
            "T2 ticked first, T3 is yours"
        );
        assert!(matches!(
            share(&tasks, "T1", &ids(2), &BTreeSet::new(), true),
            Some(Share::Checkpoint { .. })
        ));
        assert_eq!(
            share(&[open("T1")], "T1", &[], &BTreeSet::new(), false),
            None,
            "no scenario, no share"
        );
    }

    proptest! {
        /// Same inputs, same share; and in ANY order the tasks finish — lanes,
        /// `--task`, a person's ticks — every scenario of the story is
        /// given to some task's test author before the story closes:
        /// verified by an earlier tick, or judged at the checkpoint.
        #[test]
        fn every_scenario_of_a_story_is_judged(
            n in 1usize..6,
            plan in proptest::collection::vec((any::<bool>(), proptest::collection::vec(1usize..7, 0..3), any::<bool>(), proptest::collection::vec(1usize..7, 0..3)), 1..7),
            order in proptest::collection::vec(any::<prop::sample::Index>(), 7),
        ) {
            let sc = ids(n);
            let links: Vec<Vec<String>> = plan.iter().map(|(_, l, _, _)| l.iter().map(|k| format!("US3-{k}")).collect()).collect();
            let names = ["T1", "T2", "T3", "T4", "T5", "T6"];
            let mut done = vec![false; plan.len()];
            let mut remaining: Vec<usize> = (0..plan.len()).collect();
            let mut verified = BTreeSet::new();
            let mut judged = BTreeSet::new();
            let any_judged_task = plan.iter().any(|p| !p.0);
            let mut k = 0;
            while !remaining.is_empty() {
                let i = remaining.remove(order[k % order.len()].index(remaining.len()));
                k += 1;
                let tasks: Vec<StoryTask<'_>> = (0..plan.len()).map(|j| StoryTask {
                    id: names[j], checks_only: plan[j].0, done: done[j], links: &links[j],
                }).collect();
                let s = share(&tasks, names[i], &sc, &verified, plan[i].2);
                prop_assert_eq!(&s, &share(&tasks, names[i], &sc, &verified, plan[i].2));
                if let Some(s) = &s {
                    judged.extend(s.scenarios().iter().cloned());
                    // The model verifies some of what it was given, never more.
                    for k in &plan[i].3 {
                        let id = format!("US3-{k}");
                        if s.scenarios().contains(&id) { verified.insert(id); }
                    }
                }
                done[i] = true;
            }
            if any_judged_task {
                let all: BTreeSet<String> = sc.iter().cloned().collect();
                prop_assert_eq!(judged.union(&verified).cloned().collect::<BTreeSet<_>>(), all);
            }
        }
    }
}
