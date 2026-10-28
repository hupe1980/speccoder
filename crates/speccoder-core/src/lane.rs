//! Who does a task: the implementer (`code`), the plan's author
//! (`planning`: design documents inside the unit's directory) or a person
//! (`yours`). Derived, never stored — from the task's line, the unit's
//! directory, the format's judged files and an honest stop.

use serde::{Deserialize, Serialize};

/// A task's lane.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// The implementer's: code, judged by the checks and its acceptance tests.
    #[default]
    Code,
    /// Design documents the plan should have written: every path the task
    /// names lies inside the unit's directory.
    Planning,
    /// A step only a person can take: a `[YOU]` label, or an attempt that
    /// stopped `BLOCKED`.
    Yours,
}

impl Lane {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Planning => "planning",
            Self::Yours => "yours",
        }
    }

    /// The mark beside a task (none for code).
    #[must_use]
    pub const fn mark(self) -> &'static str {
        match self {
            Self::Code => "",
            Self::Planning => "📐",
            Self::Yours => "🙋",
        }
    }

    /// Whether it is the default, `code` (left out of a report).
    #[must_use]
    pub const fn is_code(&self) -> bool {
        matches!(self, Self::Code)
    }
}

/// The lane a task's named paths give it — its backticked paths when it
/// has any (prose such as `input/output` is no path), else every path it
/// names: `planning` when there is at least one and every one lies inside
/// `unit_dir` (repository- or project-relative, a leading `/` read as the
/// root) and none is a judged file — `judged` is asked with the path
/// relative to `unit_dir`, without a trailing `/`; else `code`.
#[must_use]
pub fn by_paths(title: &str, unit_dir: &str, judged: &dyn Fn(&str) -> bool) -> Lane {
    let dir = unit_dir.trim_matches('/');
    let paths = task_paths(title);
    let inside = |p: &str| {
        let p = p.trim_end_matches('/');
        let rel = if p == dir {
            Some("")
        } else {
            p.strip_prefix(dir).and_then(|r| r.strip_prefix('/'))
        };
        rel.is_some_and(|r| !r.split('/').any(|s| s == "..") && (r.is_empty() || !judged(r)))
    };
    if !dir.is_empty() && !paths.is_empty() && paths.iter().all(|p| inside(p)) {
        Lane::Planning
    } else {
        Lane::Code
    }
}

/// The paths a task's lane is read from: its backticked paths when it has
/// any, else every path it names.
#[must_use]
pub fn task_paths(title: &str) -> Vec<String> {
    let quoted = crate::scope::quoted_paths(title);
    if quoted.is_empty() {
        crate::scope::named_paths(title)
    } else {
        quoted
    }
}

/// A task's lane: `yours` for a `[YOU]` label or after an honest
/// `BLOCKED:` stop, else [`by_paths`].
#[must_use]
pub fn classify(
    title: &str,
    labelled_yours: bool,
    stopped_blocked: bool,
    unit_dir: &str,
    judged: &dyn Fn(&str) -> bool,
) -> Lane {
    if labelled_yours || stopped_blocked {
        Lane::Yours
    } else {
        by_paths(title, unit_dir, judged)
    }
}

/// A unit's open tasks by lane, for *Before you run*.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Lanes {
    /// Open tasks in the `code` lane.
    pub code: usize,
    /// The `planning` tasks, in document order.
    pub planning: Vec<String>,
    /// The `yours` tasks, in document order.
    pub yours: Vec<String>,
}

impl Lanes {
    /// Counted from `(task, lane)` pairs.
    #[must_use]
    pub fn of<'a>(tasks: impl IntoIterator<Item = (&'a str, Lane)>) -> Self {
        let mut out = Self::default();
        for (id, lane) in tasks {
            match lane {
                Lane::Code => out.code += 1,
                Lane::Planning => out.planning.push(id.to_owned()),
                Lane::Yours => out.yours.push(id.to_owned()),
            }
        }
        out
    }

    /// Whether any task is not code (the line is worth saying).
    #[must_use]
    pub fn notable(&self) -> bool {
        !self.planning.is_empty() || !self.yours.is_empty()
    }

    /// `25 tasks · 23 code · 1 📐 planning (T002) · 1 🙋 yours (T019)`.
    #[must_use]
    pub fn line(&self) -> String {
        let total = self.code + self.planning.len() + self.yours.len();
        let mut parts = vec![
            format!("{total} task{}", if total == 1 { "" } else { "s" }),
            format!("{} code", self.code),
        ];
        for (ids, lane) in [(&self.planning, Lane::Planning), (&self.yours, Lane::Yours)] {
            if !ids.is_empty() {
                parts.push(format!(
                    "{} {} {} ({})",
                    ids.len(),
                    lane.mark(),
                    lane.as_str(),
                    ids.join(", ")
                ));
            }
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judged(rel: &str) -> bool {
        matches!(rel, "spec.md" | "tasks.md" | "review.json")
            || rel == "checklists"
            || rel.starts_with("checklists/")
    }

    const DIR: &str = "specs/001-x";

    #[test]
    fn a_task_inside_the_unit_is_planning() {
        // The owner's POC: a repository-root path with a leading `/`.
        assert_eq!(
            by_paths(
                "Create the feature documentation scaffold under `/specs/001-x/contracts/` for input/output interface definitions",
                DIR,
                &judged
            ),
            Lane::Planning
        );
        assert_eq!(
            by_paths("Write specs/001-x/data-model.md", DIR, &judged),
            Lane::Planning
        );
        assert_eq!(
            by_paths("Fill in `specs/001-x/`", DIR, &judged),
            Lane::Planning
        );
    }

    #[test]
    fn mixed_paths_judged_files_and_no_paths_are_code() {
        // Spec folder and code: the normal profile.
        assert_eq!(
            by_paths(
                "Create the directories in `/specs/001-x/plan.md` at `src/`, `tests/unit/`",
                DIR,
                &judged
            ),
            Lane::Code
        );
        assert_eq!(
            by_paths("Update specs/001-x/tasks.md", DIR, &judged),
            Lane::Code
        );
        assert_eq!(
            by_paths("Add specs/001-x/checklists/ux.md", DIR, &judged),
            Lane::Code
        );
        assert_eq!(by_paths("Make it faster", DIR, &judged), Lane::Code);
        // Another unit's folder, a prefix that is not a directory, an escape.
        assert_eq!(
            by_paths("Write specs/002-y/contracts/a.md", DIR, &judged),
            Lane::Code
        );
        assert_eq!(
            by_paths("Write specs/001-xy/a.md", DIR, &judged),
            Lane::Code
        );
        assert_eq!(
            by_paths("Write specs/001-x/../../src/a.rs", DIR, &judged),
            Lane::Code
        );
        assert_eq!(by_paths("Write specs/001-x/a.md", "", &judged), Lane::Code);
    }

    #[test]
    fn a_label_or_an_honest_stop_makes_it_yours() {
        assert_eq!(
            classify("Create the GitHub repository", true, false, DIR, &judged),
            Lane::Yours
        );
        assert_eq!(
            classify("Write specs/001-x/data-model.md", false, true, DIR, &judged),
            Lane::Yours
        );
        assert_eq!(
            classify(
                "Write specs/001-x/data-model.md",
                false,
                false,
                DIR,
                &judged
            ),
            Lane::Planning
        );
    }

    #[test]
    fn the_line_names_the_tasks_that_are_not_code() {
        let mut tasks: Vec<(String, Lane)> =
            (1..=25).map(|n| (format!("T{n:03}"), Lane::Code)).collect();
        tasks[1].1 = Lane::Planning;
        tasks[18].1 = Lane::Yours;
        let lanes = Lanes::of(tasks.iter().map(|(id, l)| (id.as_str(), *l)));
        assert!(lanes.notable());
        assert_eq!(
            lanes.line(),
            "25 tasks · 23 code · 1 📐 planning (T002) · 1 🙋 yours (T019)"
        );
        assert!(!Lanes::of([("T001", Lane::Code)]).notable());
        assert_eq!(Lanes::of([("T001", Lane::Code)]).line(), "1 task · 1 code");
    }
}
