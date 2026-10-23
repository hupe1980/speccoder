//! What speccoder sees in a Spec Kit project: the read-only snapshot that
//! `inspect` reports and every later subsystem consumes.
//!
//! These are *parsed* facts about artifacts. Runtime task state is a separate
//! type. Paths are workspace-relative with `/` separators,
//! except [`ProjectSnapshot::root`].

use serde::Serialize;

/// Everything discovered about one Spec Kit project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectSnapshot {
    /// Absolute, canonical project root (the directory containing `.specify/`).
    pub root: String,
    /// Spec Kit metadata recorded in the project.
    pub speckit: SpeckitMeta,
    /// State of `.specify/memory/constitution.md`.
    pub constitution: ConstitutionState,
    /// Feature directories, ordered by numeric prefix then name.
    pub features: Vec<FeatureRef>,
    /// Which feature was selected, and why.
    pub selection: Selection,
    /// The selected feature, if any.
    pub feature: Option<FeatureSnapshot>,
    /// Problems found while discovering and parsing.
    pub diagnostics: Vec<Diagnostic>,
}

impl ProjectSnapshot {
    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Spec Kit metadata from `.specify/*.json`; each field is `None` when absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SpeckitMeta {
    /// `speckit_version` from `init-options.json`.
    pub version: Option<String>,
    /// The coding-agent integration, e.g. `claude`.
    pub integration: Option<String>,
    /// Script flavour, e.g. `sh`.
    pub script: Option<String>,
    /// `sequential` or `timestamp`.
    pub feature_numbering: Option<String>,
    /// `feature_directory` from `feature.json`, as recorded.
    pub current_feature: Option<String>,
}

/// State of the project constitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstitutionState {
    /// Present and filled in.
    Present,
    /// Present but still the Spec Kit template.
    Unfilled,
    /// Not present.
    Absent,
}

/// A feature directory under `specs/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureRef {
    /// Workspace-relative directory, e.g. `specs/002-speckit-inspect`.
    pub dir: String,
    /// Directory name, e.g. `002-speckit-inspect`.
    pub name: String,
    /// Numeric prefix as written, e.g. `002` or `20260319-143022`.
    pub prefix: String,
}

/// How the feature was selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Selection {
    /// Why this feature (or none) was selected.
    pub reason: SelectionReason,
    /// Name of the selected feature.
    pub feature: Option<String>,
}

/// Why a feature was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReason {
    /// Named on the command line.
    Argument,
    /// From the `SPECIFY_FEATURE_DIRECTORY` environment variable.
    Environment,
    /// Recorded in `.specify/feature.json`.
    Recorded,
    /// The project has exactly one feature.
    OnlyFeature,
    /// Nothing was selected.
    None,
}

/// The selected feature's artifacts and tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureSnapshot {
    /// The feature directory.
    pub feature: FeatureRef,
    /// Which artifacts exist.
    pub artifacts: Artifacts,
    /// The parsed task list, if `tasks.md` was readable.
    pub tasks: Option<ParsedTasks>,
    /// Dependencies and states of the tasks; present iff `tasks` is.
    pub schedule: Option<Schedule>,
}

/// Presence of a feature's artifacts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)] // one flag per Spec Kit artifact
pub struct Artifacts {
    /// `spec.md`
    pub spec: bool,
    /// `plan.md`
    pub plan: bool,
    /// `tasks.md`
    pub tasks: bool,
    /// `research.md`
    pub research: bool,
    /// `data-model.md`
    pub data_model: bool,
    /// `contracts/`
    pub contracts: bool,
    /// `quickstart.md`
    pub quickstart: bool,
    /// `checklists/`
    pub checklists: bool,
}

/// A parsed `tasks.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedTasks {
    /// Workspace-relative path of the file.
    pub path: String,
    /// Level-2 headings, in document order.
    pub phases: Vec<Phase>,
    /// Tasks, in document order.
    pub tasks: Vec<Task>,
    /// Counts over `tasks`.
    pub totals: Totals,
    /// How dependencies within a phase are determined.
    pub dependency_mode: DependencyMode,
}

/// How dependencies within a phase are determined.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyMode {
    /// Default template: subsections, `[P]` runs and document order, plus written references.
    #[default]
    Structural,
    /// The Format line documents `(depends on …)`: written references are the
    /// complete order within a phase; only phase order is added.
    Declared,
}

/// A level-2 heading of `tasks.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Phase {
    /// Heading text, e.g. `Phase 1: Setup`.
    pub title: String,
    /// 1-based line of the heading.
    pub line: u32,
    /// SHA-256 (hex) of the section, heading through the byte before the next phase.
    pub section_hash: String,
}

/// One task line of `tasks.md`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    /// Task ID, e.g. `T014`.
    pub id: String,
    /// `[x]` or `[X]`.
    pub done: bool,
    /// Carries the `[P]` marker.
    pub parallel: bool,
    /// Story label without brackets, e.g. `US1`.
    pub story: Option<String>,
    /// Text after the ID and markers.
    pub title: String,
    /// Task IDs named after `depends on` / `after` in the text, in order.
    pub depends_on: Vec<String>,
    /// Enclosing level-2 heading.
    pub phase: Option<String>,
    /// Enclosing level-3 heading within the phase.
    pub subsection: Option<String>,
    /// 1-based line.
    pub line: u32,
    /// Byte range `[start, end)` of the list item in the file.
    pub byte_range: [usize; 2],
    /// SHA-256 (hex) of the task's line, without its line ending.
    pub line_hash: String,
}

/// Which tasks can start now, and what blocks the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Schedule {
    /// One entry per task, in document order.
    pub tasks: Vec<ScheduledTask>,
    /// Counts over `tasks`.
    pub totals: ScheduleTotals,
}

/// A task's dependencies and state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScheduledTask {
    /// Task ID.
    pub id: String,
    /// Done, ready or blocked.
    pub state: TaskState,
    /// Direct dependencies.
    pub depends_on: Vec<Dependency>,
    /// Open, missing or cyclic dependencies, in document order.
    pub blocked_by: Vec<String>,
}

/// Whether a task can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Ticked in `tasks.md`.
    Done,
    /// Open, and every dependency is done.
    Ready,
    /// Open, and some dependency is open, missing or part of a cycle.
    Blocked,
}

/// One direct dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    /// The task depended on.
    pub id: String,
    /// Where the edge comes from.
    pub kind: DependencyKind,
    /// The ID does not exist in the task list.
    pub missing: bool,
}

/// Origin of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    /// From phase, subsection or `[P]` structure.
    Structural,
    /// Written in the task text.
    Explicit,
}

/// Schedule counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScheduleTotals {
    /// Ticked tasks.
    pub done: usize,
    /// Tasks that can start now.
    pub ready: usize,
    /// Tasks waiting on something.
    pub blocked: usize,
}

/// Task counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// All tasks.
    pub total: usize,
    /// Ticked tasks.
    pub done: usize,
    /// Unticked tasks.
    pub open: usize,
}

/// A problem found while discovering or parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// How serious it is.
    pub severity: Severity,
    /// Stable code, e.g. `tasks.duplicate-id`.
    pub code: String,
    /// Workspace-relative file, if the problem is in one.
    pub path: Option<String>,
    /// 1-based line, if known.
    pub line: Option<u32>,
    /// What is wrong.
    pub message: String,
    /// What to do about it.
    pub hint: Option<String>,
}

impl Diagnostic {
    /// A diagnostic with no location or hint.
    #[must_use]
    pub fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.to_owned(),
            path: None,
            line: None,
            message: message.into(),
            hint: None,
        }
    }

    /// Sets the file.
    #[must_use]
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Sets the line.
    #[must_use]
    pub fn line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Sets the hint.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Unsafe to act on.
    Error,
    /// Suspicious but usable.
    Warning,
    /// For information.
    Info,
}
