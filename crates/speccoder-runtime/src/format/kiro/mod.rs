//! Kiro (and cc-sdd): a spec in `.kiro/specs/<name>/` is the unit of work.
//! Each `### Requirement N` is a story and its numbered EARS criteria
//! (`N.M`) are its scenarios; a task's `_Requirements: 1.2, 1.3_` are the
//! criteria that judge it. Parent tasks are closed by the engine with their
//! last required sub-task; optional (`[ ]*`) tasks are left alone.
//! Reading and the two writes are the `speccoder-kiro` crate's; this
//! adapter is the only runtime code that knows Kiro.

mod texts;

use std::path::{Path, PathBuf};

use speccoder_core::ChecklistStatus;
use speccoder_core::snapshot::{Diagnostic, ParsedTasks, ProjectSnapshot, Severity};
use speccoder_core::spec::{FormatKind, ScenarioRef};

pub use texts::{MESSAGES, PLANNING, RULES, WORDING};

use crate::context::{Bundle, BundleError, DependencyContext, TaskContext, cap};
use crate::format::{Adapter, CommitPrecondition, Detect, Messages, Planning, Rules, Wording};

/// The Kiro adapter.
#[derive(Debug)]
pub struct Kiro;

/// The one instance.
pub static KIRO: Kiro = Kiro;

impl Detect for Kiro {
    fn kind(&self) -> FormatKind {
        FormatKind::Kiro
    }

    fn root(&self, start: &Path) -> Option<PathBuf> {
        speccoder_kiro::find_root(start)
    }

    fn adapter(&self) -> Option<&'static dyn Adapter> {
        Some(&KIRO)
    }
}

fn spec_name(snapshot: &ProjectSnapshot) -> Option<&str> {
    snapshot.feature.as_ref().map(|f| f.feature.name.as_str())
}

impl Adapter for Kiro {
    fn kind(&self) -> FormatKind {
        FormatKind::Kiro
    }

    fn detect(&self, start: &Path) -> Option<PathBuf> {
        Detect::root(self, start)
    }

    fn inspect(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot> {
        speccoder_kiro::inspect(start, feature).ok()
    }

    fn parse_tasks(&self, path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>) {
        let (t, d) = speccoder_kiro::parse::tasks(path, text);
        (t.parsed, d)
    }

    fn rules(&self) -> &'static Rules {
        &RULES
    }

    fn wording(&self) -> &'static Wording {
        &WORDING
    }

    fn messages(&self) -> &'static Messages {
        &MESSAGES
    }

    fn planning(&self) -> &'static Planning {
        &PLANNING
    }

    fn bundle(&self, snapshot: &ProjectSnapshot, task_id: &str) -> Result<Bundle, BundleError> {
        let feature = snapshot.feature.as_ref().ok_or(BundleError::NoFeature)?;
        let tasks = feature.tasks.as_ref().ok_or(BundleError::NoTasks)?;
        let task = tasks
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| BundleError::NotFound(task_id.to_owned()))?;
        let root = Path::new(&snapshot.root);
        let spec = speccoder_kiro::load(root, &feature.feature.name);
        // The requirements its criteria belong to; all of them when it
        // cites none (setup, a checkpoint).
        let cited = |r: &&speccoder_kiro::parse::Requirement| {
            task.scenarios.is_empty() || r.criteria.iter().any(|c| task.scenarios.contains(&c.id))
        };
        let story = spec
            .requirements
            .iter()
            .filter(cited)
            .map(|r| r.text.trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n\n");
        let context = |t: &speccoder_core::snapshot::Task| DependencyContext {
            id: t.id.clone(),
            title: t.title.clone(),
            done: t.done,
        };
        let dependencies = feature
            .schedule
            .as_ref()
            .and_then(|s| s.tasks.iter().find(|t| t.id == task.id))
            .map(|st| {
                st.depends_on
                    .iter()
                    .filter_map(|d| tasks.tasks.iter().find(|t| t.id == d.id))
                    .map(context)
                    .collect()
            })
            .unwrap_or_default();
        // The tasks on the same requirements.
        let number = |c: &String| c.split('.').next().unwrap_or_default().to_owned();
        let mine: std::collections::BTreeSet<String> = task.scenarios.iter().map(number).collect();
        let story_tasks = tasks
            .tasks
            .iter()
            .filter(|t| t.id != task.id && t.scenarios.iter().any(|c| mine.contains(&number(c))))
            .map(context)
            .collect();
        Ok(Bundle {
            feature: feature.feature.name.clone(),
            constitution: cap(&speccoder_kiro::steering_text(root)),
            plan: cap(spec.design.as_deref().unwrap_or_default()),
            story: cap(&story),
            task: TaskContext {
                id: task.id.clone(),
                title: task.title.clone(),
                phase: task.phase.clone().unwrap_or_default(),
                subsection: String::new(),
                story: String::new(),
                parallel: task.parallel,
            },
            dependencies,
            story_tasks,
        })
    }

    /// Every acceptance criterion of the spec, by its `N.M`.
    fn scenarios(&self, snapshot: &ProjectSnapshot, _task: &str) -> Vec<ScenarioRef> {
        let Some(name) = spec_name(snapshot) else {
            return Vec::new();
        };
        speccoder_kiro::load(Path::new(&snapshot.root), name)
            .requirements
            .iter()
            .flat_map(|r| {
                r.criteria.iter().map(|c| ScenarioRef {
                    id: c.id.clone(),
                    text: c.text.clone(),
                })
            })
            .collect()
    }

    /// Each requirement with its criteria: the story a task's citations
    /// are judged in.
    fn oracle_requirements(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Vec<crate::format::OracleRequirement> {
        let Some(name) = spec_name(snapshot) else {
            return Vec::new();
        };
        speccoder_kiro::load(Path::new(&snapshot.root), name)
            .requirements
            .iter()
            .map(|r| crate::format::OracleRequirement {
                key: r.key(),
                title: r.title.clone(),
                text: r.text.clone(),
                scenarios: r
                    .criteria
                    .iter()
                    .map(|c| ScenarioRef {
                        id: c.id.clone(),
                        text: c.text.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Requirements with acceptance criteria: the criteria are the examples.
    fn acceptance_testable(&self, bundle: &Bundle) -> bool {
        bundle.story.to_lowercase().contains("acceptance criteria")
    }

    /// A Convergence task, appended by converge.
    fn is_convergence(&self, phase: &str) -> bool {
        phase.to_lowercase().contains("convergence")
    }

    fn convergence_texts(&self, root: &Path, unit_dir: &Path) -> [String; 4] {
        let read = |f: &str| std::fs::read_to_string(unit_dir.join(f)).unwrap_or_default();
        [
            speccoder_kiro::steering_text(root),
            read("requirements.md"),
            read("design.md"),
            read("tasks.md"),
        ]
    }

    /// Errors of the spec itself stop a run before any model call; a
    /// selection problem is the selection's, a dependency problem blocks
    /// only the tasks involved.
    fn blocking<'s>(&self, snapshot: &'s ProjectSnapshot) -> Vec<&'s Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .filter(|d| !d.code.starts_with("select.") && !d.code.starts_with("graph."))
            .collect()
    }

    fn crosscheck(
        &self,
        _snapshot: &ProjectSnapshot,
        _runner: &dyn crate::verify::process::CommandRunner,
    ) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn checklists(&self, _root: &Path, _unit_dir: &str) -> Vec<ChecklistStatus> {
        Vec::new()
    }

    fn bug_unit(
        &self,
        _root: &Path,
    ) -> Result<speccoder_core::bug::BugHome, crate::format::BugRefusal> {
        Err(crate::format::BugRefusal::new(
            "BUG-NO-UNIT",
            "Kiro keeps a bug in a bugfix spec, which speccoder does not run yet: nothing was written — write the fix as a feature spec, or `speccoder do` for a change without a reproduction",
        ))
    }

    /// The requirements as the living spec, the design as the plan.
    fn documents(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("spec", "requirements.md"),
            ("plan", "design.md"),
            ("tasks", "tasks.md"),
        ]
    }

    fn tick(&self, tasks_file: &Path, task: &str, line_hash: &str) -> Result<(), String> {
        speccoder_kiro::write::tick(tasks_file, task, line_hash)
    }

    fn append(
        &self,
        tasks_file: &Path,
        file_hash: &str,
        items: &[String],
    ) -> Result<Vec<String>, String> {
        speccoder_kiro::write::append_convergence(tasks_file, file_hash, items)
    }

    fn commit_precondition(
        &self,
        _root: &Path,
        join: &dyn Fn(&str) -> String,
        unit_dir: &str,
    ) -> CommitPrecondition {
        CommitPrecondition {
            strict: vec![join(unit_dir)],
            tracked: vec![join(".kiro/steering")],
            runtime: |_| false,
            essentials: vec![format!("{}/tasks.md", join(unit_dir))],
            message: "the spec's Kiro files are not committed, so a worktree would not contain them",
            hint: "commit them (git add .kiro && git commit), or run in place: speccoder run --in-place",
            no_commits_hint: "commit the Kiro files first (git add .kiro && git commit)",
        }
    }
}
