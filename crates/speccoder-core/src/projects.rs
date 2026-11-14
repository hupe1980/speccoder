//! Several projects, one view (spec 097): each recent project rolled up
//! read-only — its state from evidence, what waits in it, what it costs —
//! and its Needs You items merged with the others'. A count that could not
//! be read is unknown, never 0, and never makes a project look calmer.

use serde::{Deserialize, Serialize};

use crate::Usage;

/// Where a project stands, in the order the view shows them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectState {
    /// A decision of the person's holds work up.
    BlockedOnYou,
    /// A run is going on.
    Working,
    /// What waits could not be read: it may need the person.
    CantTell,
    /// Work is done and waits to be looked at.
    ReadyToReview,
    /// Nothing waits, nothing runs.
    Idle,
    /// Its folder is gone, or no longer a project: only Forget.
    Missing,
}

/// The state from what was read (FR-002): `blocked` decisions known to
/// wait, whether a run is alive, whether every waiting call could be read,
/// whether anything else waits.
#[must_use]
pub fn state_of(blocked: usize, running: bool, readable: bool, other: usize) -> ProjectState {
    if blocked > 0 {
        ProjectState::BlockedOnYou
    } else if running {
        ProjectState::Working
    } else if !readable {
        ProjectState::CantTell
    } else if other > 0 {
        ProjectState::ReadyToReview
    } else {
        ProjectState::Idle
    }
}

/// A run going on in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LiveTask {
    /// Its task.
    pub task: String,
    /// The stage it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

/// Another speccoder serving the project now (never its key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Elsewhere {
    /// Its process.
    pub pid: u32,
    /// Unix seconds it started.
    pub since: u64,
}

/// One recent project, rolled up.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    /// Its name.
    pub name: String,
    /// Its folder.
    pub path: String,
    /// Its folder, `~`-abbreviated.
    pub shown: String,
    /// Where it stands.
    pub state: ProjectState,
    /// The page asking is this project's.
    #[serde(default)]
    pub current: bool,
    /// Its current feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// A run going on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveTask>,
    /// Decisions waiting for the person; absent when not all could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<usize>,
    /// Why something could not be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown: Option<String>,
    /// What its current feature has cost so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend: Option<Usage>,
    /// Open in another speccoder now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elsewhere: Option<Elsewhere>,
}

/// A Needs You item of some project: speccoder's own title and kind,
/// never its detail (which may quote model text or command output).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ItemAcross {
    /// The project's name.
    pub project: String,
    /// The project's folder.
    pub path: String,
    /// The feature.
    pub feature: String,
    /// The task, when the item is a task's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The item's kind (`approval`, `question`, …).
    pub kind: String,
    /// `blocked_on_you` or `ready_to_review`.
    pub group: String,
    /// speccoder's title of it.
    pub title: String,
    /// Unix seconds it has waited since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

/// `speccoder projects`: the recent projects and what waits across them.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ProjectsReport {
    /// The projects, by state, then the most recently used.
    pub projects: Vec<ProjectSummary>,
    /// Every project's Needs You, merged: blocked first, the oldest first.
    pub items: Vec<ItemAcross>,
}

/// Where in a project to open it: `inbox`, `inbox:001-auth`,
/// `inbox:001-auth/T005`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Place {
    /// The view (only `inbox`).
    pub view: String,
    /// The feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// The task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

impl Place {
    /// Reads `inbox[:feature[/task]]`; anything else is refused.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (view, rest) = text
            .split_once(':')
            .map_or((text, None), |(v, r)| (v, Some(r)));
        if view != "inbox" {
            return None;
        }
        let (feature, task) = match rest {
            None => (None, None),
            Some(r) => match r.split_once('/') {
                Some((f, t)) => (Some(f), Some(t)),
                None => (Some(r), None),
            },
        };
        let ok = |s: &str, extra: &[char]| {
            !s.is_empty()
                && s.len() <= 120
                && !s.starts_with('.')
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || extra.contains(&c))
        };
        if feature.is_some_and(|f| !ok(f, &['.', '_'])) || task.is_some_and(|t| !ok(t, &[])) {
            return None;
        }
        Some(Self {
            view: view.to_owned(),
            feature: feature.map(str::to_owned),
            task: task.map(str::to_owned),
        })
    }

    /// The page's address query: `view=inbox&feature=…&task=…`.
    #[must_use]
    pub fn query(&self) -> String {
        let mut q = format!("view={}", self.view);
        if let Some(f) = &self.feature {
            q.push_str("&feature=");
            q.push_str(f);
        }
        if let Some(t) = &self.task {
            q.push_str("&task=");
            q.push_str(t);
        }
        q
    }

    /// The text form `parse` reads.
    #[must_use]
    pub fn text(&self) -> String {
        match (&self.feature, &self.task) {
            (Some(f), Some(t)) => format!("{}:{f}/{t}", self.view),
            (Some(f), None) => format!("{}:{f}", self.view),
            _ => self.view.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_count_never_makes_a_project_look_calmer() {
        assert_eq!(state_of(1, true, false, 3), ProjectState::BlockedOnYou);
        assert_eq!(state_of(0, true, false, 0), ProjectState::Working);
        assert_eq!(state_of(0, false, false, 2), ProjectState::CantTell);
        assert_eq!(state_of(0, false, false, 0), ProjectState::CantTell);
        assert_eq!(state_of(0, false, true, 2), ProjectState::ReadyToReview);
        assert_eq!(state_of(0, false, true, 0), ProjectState::Idle);
    }

    #[test]
    fn a_place_is_the_inbox_a_feature_and_a_task_and_nothing_else() {
        let p = Place::parse("inbox:001-auth/T005").unwrap();
        assert_eq!(p.query(), "view=inbox&feature=001-auth&task=T005");
        assert_eq!(p.text(), "inbox:001-auth/T005");
        assert_eq!(Place::parse("inbox").unwrap().query(), "view=inbox");
        for bad in [
            "board:x",
            "inbox:../x",
            "inbox:a/b/c",
            "inbox:a b",
            "inbox:a/T5&x=1",
            "inbox:",
            "inbox:.hidden",
        ] {
            assert!(Place::parse(bad).is_none(), "{bad}");
        }
    }
}
