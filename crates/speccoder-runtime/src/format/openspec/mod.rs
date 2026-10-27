//! OpenSpec: a change in `openspec/changes/<id>/` is the unit of
//! work, its spec deltas' scenarios are the acceptance examples, its
//! `tasks.md` is a chain. Parsing and preflight are the `speccoder-openspec`
//! crate's; this adapter is the only runtime code that knows OpenSpec.

pub mod archive;
mod bug;
pub mod cli;
mod texts;

use std::path::{Path, PathBuf};

use speccoder_core::ChecklistStatus;
use speccoder_core::snapshot::{
    Diagnostic, FeatureSnapshot, ParsedTasks, ProjectSnapshot, Severity,
};
use speccoder_core::spec::{FormatKind, ScenarioRef};

pub use texts::{MESSAGES, PLANNING, RULES, WORDING};

use crate::context::{Bundle, BundleError, DependencyContext, TaskContext, cap};
use crate::format::{Adapter, CommitPrecondition, Detect, Messages, Planning, Rules, Wording};

/// The OpenSpec adapter.
#[derive(Debug)]
pub struct OpenSpec;

/// The one instance.
pub static OPENSPEC: OpenSpec = OpenSpec;

impl Detect for OpenSpec {
    fn kind(&self) -> FormatKind {
        FormatKind::Openspec
    }

    fn root(&self, start: &Path) -> Option<PathBuf> {
        speccoder_openspec::discover::find_root(start)
    }

    fn adapter(&self) -> Option<&'static dyn Adapter> {
        Some(&OPENSPEC)
    }
}

/// The change's proposal and design, as the plan.
fn plan_text(c: &speccoder_openspec::Change) -> String {
    [("proposal.md", &c.proposal), ("design.md", &c.design)]
        .iter()
        .filter_map(|(name, text)| {
            text.as_ref()
                .map(|t| format!("## {}/{name}\n\n{}", c.dir, t.trim_end()))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Every spec delta of the change, whole, as the intent of every task: the
/// format does not link a task to a requirement.
fn deltas_text(c: &speccoder_openspec::Change) -> String {
    c.deltas
        .iter()
        .map(|d| format!("## {}\n\n{}", d.path, d.text.trim_end()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl Adapter for OpenSpec {
    fn tool(&self) -> Option<(&'static str, &'static str)> {
        Some(("openspec", "npm install -g @fission-ai/openspec"))
    }

    fn kind(&self) -> FormatKind {
        FormatKind::Openspec
    }

    fn detect(&self, start: &Path) -> Option<PathBuf> {
        Detect::root(self, start)
    }

    fn inspect(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot> {
        speccoder_openspec::inspect(start, feature).ok()
    }

    fn parse_tasks(&self, path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>) {
        speccoder_openspec::tasks::parse(path, text)
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
        let feature: &FeatureSnapshot = snapshot.feature.as_ref().ok_or(BundleError::NoFeature)?;
        let tasks = feature.tasks.as_ref().ok_or(BundleError::NoTasks)?;
        let task = tasks
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| BundleError::NotFound(task_id.to_owned()))?;
        let root = Path::new(&snapshot.root);
        let schema = snapshot
            .openspec
            .as_ref()
            .map_or(speccoder_openspec::discover::SCHEMA, |m| m.schema.as_str());
        let (change, _) = speccoder_openspec::load_change(root, &feature.feature.name, schema);
        let context = snapshot
            .openspec
            .as_ref()
            .and_then(|m| m.context.clone())
            .unwrap_or_default();
        let dependencies = feature
            .schedule
            .as_ref()
            .and_then(|s| s.tasks.iter().find(|t| t.id == task.id))
            .map(|st| {
                st.depends_on
                    .iter()
                    .filter_map(|d| tasks.tasks.iter().find(|t| t.id == d.id))
                    .map(|t| DependencyContext {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        done: t.done,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Bundle {
            feature: feature.feature.name.clone(),
            constitution: cap(&context),
            plan: cap(&plan_text(&change)),
            story: cap(&deltas_text(&change)),
            task: TaskContext {
                id: task.id.clone(),
                title: task.title.clone(),
                phase: task.phase.clone().unwrap_or_default(),
                subsection: String::new(),
                story: String::new(),
                parallel: false,
            },
            dependencies,
        })
    }

    /// Every scenario of the change's ADDED and MODIFIED requirements: the
    /// format does not link a task to a requirement.
    fn scenarios(&self, snapshot: &ProjectSnapshot, _task: &str) -> Vec<ScenarioRef> {
        let Some(f) = snapshot.feature.as_ref() else {
            return Vec::new();
        };
        let schema = snapshot
            .openspec
            .as_ref()
            .map_or(speccoder_openspec::discover::SCHEMA, |m| m.schema.as_str());
        let (change, _) =
            speccoder_openspec::load_change(Path::new(&snapshot.root), &f.feature.name, schema);
        change
            .deltas
            .iter()
            .flat_map(|d| {
                d.requirements
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.operation,
                            speccoder_openspec::delta::Operation::Added
                                | speccoder_openspec::delta::Operation::Modified
                        )
                    })
                    .flat_map(move |r| {
                        r.scenarios.iter().map(move |sc| ScenarioRef {
                            id: format!("{}: {} / {}", d.capability, r.name, sc.name),
                            text: sc.raw.clone(),
                        })
                    })
            })
            .collect()
    }

    /// Each ADDED or MODIFIED delta requirement with its scenarios, keyed
    /// `<capability>: <requirement>`.
    fn oracle_requirements(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Vec<crate::format::OracleRequirement> {
        let Some(f) = snapshot.feature.as_ref() else {
            return Vec::new();
        };
        let schema = snapshot
            .openspec
            .as_ref()
            .map_or(speccoder_openspec::discover::SCHEMA, |m| m.schema.as_str());
        let (change, _) =
            speccoder_openspec::load_change(Path::new(&snapshot.root), &f.feature.name, schema);
        change
            .deltas
            .iter()
            .flat_map(|d| {
                d.requirements
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.operation,
                            speccoder_openspec::delta::Operation::Added
                                | speccoder_openspec::delta::Operation::Modified
                        )
                    })
                    .map(move |r| crate::format::OracleRequirement {
                        key: format!("{}: {}", d.capability, r.name),
                        title: r.name.clone(),
                        text: r.text.clone(),
                        scenarios: r
                            .scenarios
                            .iter()
                            .map(|sc| ScenarioRef {
                                id: format!("{}: {} / {}", d.capability, r.name, sc.name),
                                text: sc.raw.clone(),
                            })
                            .collect(),
                    })
            })
            .collect()
    }

    /// The standing bug-fix change.
    fn standing_units(&self, _root: &Path) -> Vec<(String, speccoder_core::ladder::Rung)> {
        vec![("bugfixes".to_owned(), speccoder_core::ladder::Rung::Bug)]
    }

    /// A delta with a `#### Scenario:`: the scenarios are the examples.
    fn acceptance_testable(&self, bundle: &Bundle) -> bool {
        bundle.story.contains("#### Scenario:")
    }

    /// A Convergence group, appended by converge.
    fn is_convergence(&self, phase: &str) -> bool {
        phase.to_lowercase().contains("convergence")
    }

    fn convergence_texts(&self, root: &Path, unit_dir: &Path) -> [String; 4] {
        let id = unit_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut diags = Vec::new();
        let config = speccoder_openspec::discover::config(root, &mut diags);
        let (change, _) = speccoder_openspec::load_change(root, &id, &config.schema);
        [
            config.context.unwrap_or_default(),
            deltas_text(&change),
            plan_text(&change),
            std::fs::read_to_string(unit_dir.join("tasks.md")).unwrap_or_default(),
        ]
    }

    /// Errors of the change itself — preflight, task list, configuration —
    /// stop a run before any model call. A selection problem is the
    /// selection's; a dependency problem blocks only the tasks involved.
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
        snapshot: &ProjectSnapshot,
        runner: &dyn crate::verify::process::CommandRunner,
    ) -> Vec<Diagnostic> {
        cli::crosscheck(snapshot, runner)
    }

    fn checklists(&self, _root: &Path, _unit_dir: &str) -> Vec<ChecklistStatus> {
        Vec::new()
    }

    fn bug_unit(
        &self,
        _root: &Path,
    ) -> Result<speccoder_core::bug::BugHome, crate::format::BugRefusal> {
        Ok(bug::unit())
    }

    fn record_bug(
        &self,
        root: &Path,
        _home: &speccoder_core::bug::BugHome,
        triad: &speccoder_core::bug::Triad,
    ) -> Result<crate::format::RecordedBug, crate::format::BugRefusal> {
        bug::record(root, triad)
    }

    fn read_bug(
        &self,
        root: &Path,
        _home: &speccoder_core::bug::BugHome,
        key: &str,
    ) -> Result<crate::format::ReadBug, crate::format::BugRefusal> {
        bug::read(root, key)
    }

    fn tick(&self, tasks_file: &Path, task: &str, line_hash: &str) -> Result<(), String> {
        speccoder_openspec::write::tick(tasks_file, task, line_hash)
    }

    fn append(
        &self,
        tasks_file: &Path,
        file_hash: &str,
        items: &[String],
    ) -> Result<Vec<String>, String> {
        speccoder_openspec::write::append_convergence(tasks_file, file_hash, items)
    }

    fn commit_precondition(
        &self,
        root: &Path,
        join: &dyn Fn(&str) -> String,
        unit_dir: &str,
    ) -> CommitPrecondition {
        // The main specs the change's deltas touch: what archive will rewrite,
        // and what the tests' intent is measured against.
        let touched = speccoder_openspec::discover::spec_files(&root.join(unit_dir).join("specs"))
            .into_iter()
            .map(|(cap, _)| join(&format!("openspec/specs/{cap}/spec.md")));
        CommitPrecondition {
            strict: vec![join(unit_dir)],
            tracked: std::iter::once(join("openspec/config.yaml"))
                .chain(touched)
                .collect(),
            runtime: |_| false,
            essentials: vec![format!("{}/tasks.md", join(unit_dir))],
            message: "the change's OpenSpec files are not committed, so a worktree would not contain them",
            hint: "commit them (git add openspec && git commit), or run in place: speccoder run --in-place",
            no_commits_hint: "commit the OpenSpec files first (git add openspec && git commit)",
        }
    }
}
