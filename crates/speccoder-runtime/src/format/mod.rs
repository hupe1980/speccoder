//! The spec-format seam: everything the runtime knows about
//! a spec format comes from an [`Adapter`] — where the intent lives, what a
//! planning skill may write, what the model is told, how a task is ticked.
//! The judge (verification, evidence, commits) is the same for every format.
//!
//! Spec Kit is the reference format; OpenSpec runs too. Kiro is
//! detected, so a project in it gets a clear answer instead of "not a
//! project".

pub mod openspec;
pub mod speckit;
mod stubs;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use speccoder_core::snapshot::{Diagnostic, ParsedTasks, ProjectSnapshot, Severity};
use speccoder_core::spec::{FormatKind, ScenarioRef};
use speccoder_core::{ChecklistStatus, CommandStatus};

use crate::context::{Bundle, BundleError};

pub use speckit::Workspace;

/// Where a quick change was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// The unit's name (`004-quick-changes`).
    pub feature: String,
    /// Its directory, project-relative.
    pub dir: String,
    /// The task (`T003`).
    pub task: String,
    /// The requirement (`FR-003`).
    pub requirement: String,
    /// The change's title.
    pub title: String,
}

/// An installed extension of the format, as its inventory lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Extension {
    /// Its id (`bug`).
    pub id: String,
    /// Its version.
    pub version: String,
    /// Enabled (a disabled one counts as not installed).
    pub enabled: bool,
    /// Where it was installed from, as the format says (`local`, a catalog).
    pub source: String,
}

/// Which source the inventory was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySource {
    /// The format's own command-line tool.
    Cli,
    /// The format's registry files.
    Files,
}

/// The format's installed extensions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inventory {
    /// What was read.
    pub source: InventorySource,
    /// Every extension listed.
    pub entries: Vec<Extension>,
}

impl Inventory {
    /// The extension `id`, if installed and enabled.
    #[must_use]
    pub fn enabled(&self, id: &str) -> Option<&Extension> {
        self.entries.iter().find(|e| e.id == id && e.enabled)
    }
}

/// Why a bug cannot be recorded or read here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugRefusal {
    /// The catalog code (`BUG-NO-UNIT`, `BUG-ASSESSMENT-DRIFT`, …).
    pub code: &'static str,
    /// What the person is told.
    pub message: String,
}

impl BugRefusal {
    /// A refusal with `code`.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A bug the adapter recorded in a standing unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedBug {
    /// The unit (`007-bugfixes`).
    pub unit: String,
    /// The unit's directory, project-relative.
    pub dir: String,
    /// The requirement (`FR-003`).
    pub requirement: String,
    /// The task (`T003`).
    pub task: String,
    /// The title.
    pub title: String,
    /// The files it wrote, project-relative.
    pub files: Vec<String>,
}

/// How a format's own bug step is run: the hosted skill, its arguments
/// and what it may write.
#[derive(Debug, Clone, Copy)]
pub struct BugStep {
    /// The installed skill that creates a bug.
    pub skill: &'static str,
    /// Its arguments: the person's labelled triad and the slug.
    pub arguments: fn(&speccoder_core::bug::Triad, &str) -> String,
    /// The policy's rules while it runs: it writes only the bug directory.
    pub rules: &'static Rules,
}

/// The triad as a format's own bug file holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBug {
    /// Current statements (the first is the symptom).
    pub current: Vec<String>,
    /// Expected statements.
    pub expected: Vec<String>,
    /// Unchanged statements.
    pub unchanged: Vec<String>,
    /// The title the file gives.
    pub title: Option<String>,
    /// The file it was read from, project-relative.
    pub file: String,
}

/// Why a project in a format this build detects but does not run has no
/// place for a bug (`BUG-NO-UNIT`), if it is one.
#[must_use]
pub fn bug_refusal_unsupported(start: &Path) -> Option<BugRefusal> {
    Detect::root(&stubs::KIRO, start).map(|_| BugRefusal::new("BUG-NO-UNIT", stubs::KIRO_BUGS))
}

/// A requirement a review writes tests for (spec 093): the format's
/// requirement (`US1`) with the scenario ids its tests cite (`US1-1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleRequirement {
    /// Stable within the unit (`US1`).
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its whole text, scenarios included.
    pub text: String,
    /// The scenarios its tests cite, with the spec's words.
    pub scenarios: Vec<ScenarioRef>,
}

impl OracleRequirement {
    /// `sha256:<hex>` of `title + "\n" + text`: the wording a frozen oracle
    /// is bound to; any change of title, body or scenario lapses it.
    #[must_use]
    pub fn digest(&self) -> String {
        format!(
            "sha256:{}",
            crate::integrity::sha256_hex(format!("{}\n{}", self.title, self.text).as_bytes())
        )
    }
}

/// One spec format the runtime can run.
pub trait Adapter: Send + Sync + std::fmt::Debug {
    /// Which format.
    fn kind(&self) -> FormatKind;

    /// The project root containing `start`, by the format's marker.
    fn detect(&self, start: &Path) -> Option<PathBuf>;

    /// Discovers the project containing `start` and selects `feature`.
    /// Read-only.
    fn inspect(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot>;

    /// [`Adapter::inspect`] for a command a person ran: the format's own
    /// selection settings (its environment variables) apply.
    fn open(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot> {
        self.inspect(start, feature)
    }

    /// Parses a task list.
    fn parse_tasks(&self, path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>);

    /// Which paths are intent and what planning may write.
    fn rules(&self) -> &'static Rules;

    /// What the model is told about the format.
    fn wording(&self) -> &'static Wording;

    /// What a person is told.
    fn messages(&self) -> &'static Messages;

    /// Hosted planning skills.
    fn planning(&self) -> &'static Planning;

    /// The context bundle for `task` of the selected unit.
    fn bundle(&self, snapshot: &ProjectSnapshot, task: &str) -> Result<Bundle, BundleError>;

    /// The scenarios `task`'s acceptance tests cite, with the spec's words
    /// (judge-side: the implementer sees the spec text, not this list).
    fn scenarios(&self, snapshot: &ProjectSnapshot, task: &str) -> Vec<ScenarioRef>;

    /// Whether the bundle states acceptance examples to derive tests from.
    fn acceptance_testable(&self, bundle: &Bundle) -> bool;

    /// Whether a task closes a gap found after implementation (its phase).
    fn is_convergence(&self, phase: &str) -> bool;

    /// What the convergence assessment reads, in order: the project's rules,
    /// the spec, the plan, the task list (each may be empty).
    fn convergence_texts(&self, root: &Path, unit_dir: &Path) -> [String; 4];

    /// Diagnostics that stop a run of the selected unit before any model
    /// call (the format's preflight).
    fn blocking<'s>(&self, _snapshot: &'s ProjectSnapshot) -> Vec<&'s Diagnostic> {
        Vec::new()
    }

    /// The format's own tools' opinion of the selected unit, when they are
    /// installed (read-only commands through `runner`).
    fn crosscheck(
        &self,
        _snapshot: &ProjectSnapshot,
        _runner: &dyn crate::verify::process::CommandRunner,
    ) -> Vec<Diagnostic> {
        Vec::new()
    }

    /// The unit's unfinished-checklist state.
    fn checklists(&self, root: &Path, unit_dir: &str) -> Vec<ChecklistStatus>;

    /// The format's own command-line tool and how to install it, for
    /// `doctor` (`None`: the format has none).
    fn tool(&self) -> Option<(&'static str, &'static str)> {
        None
    }

    /// The unit's document that defines its requirements (`spec.md` in
    /// Spec Kit), relative to the unit's directory; `None` when the format
    /// has no requirement identifiers.
    fn requirements_file(&self) -> Option<&'static str> {
        None
    }

    /// The requirements `text` (the requirements file) defines, each with
    /// the line that defines it, in the document's order.
    fn requirements_in(&self, _text: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Records a quick change: `request` (with acceptance `checks`)
    /// becomes a story, a requirement and a task in the format's standing
    /// unit for such changes. `Err`: the format has none.
    fn record_change(
        &self,
        _root: &Path,
        _request: &str,
        _checks: &[String],
    ) -> Result<Recorded, String> {
        Err("this spec format has no place for quick changes: write a spec for it".to_owned())
    }

    /// The format's installed extensions, from its own inventory (`None`:
    /// the format has none, or none could be read). The only reader of the
    /// format's extension files.
    fn extensions(&self, _root: &Path) -> Option<Inventory> {
        None
    }

    /// Where a bug lives (D-123): the format's own bug structure when it
    /// has one, else its standing bug-fix unit. `Err`: neither
    /// (`BUG-NO-UNIT`).
    ///
    /// # Errors
    ///
    /// The format has no place for a bug.
    fn bug_unit(&self, _root: &Path) -> Result<speccoder_core::bug::BugHome, BugRefusal> {
        Err(BugRefusal::new(
            "BUG-NO-UNIT",
            "this spec format has no place for a bug: write a spec for the fix, or `speccoder do` for a change without a reproduction",
        ))
    }

    /// Records `triad` as a story, an approved-to-be requirement and a task
    /// in the standing bug-fix unit `home` names (appends only, verified at
    /// write time).
    ///
    /// # Errors
    ///
    /// The home is not a standing unit, or the files changed meanwhile.
    fn record_bug(
        &self,
        _root: &Path,
        _home: &speccoder_core::bug::BugHome,
        _triad: &speccoder_core::bug::Triad,
    ) -> Result<RecordedBug, BugRefusal> {
        Err(BugRefusal::new(
            "BUG-NO-UNIT",
            "this spec format has no standing unit for bugs",
        ))
    }

    /// The format's own bug step, for a native home.
    fn bug_step(&self) -> Option<BugStep> {
        None
    }

    /// A native home's directory for `slug`, project-relative, and its
    /// unit name.
    fn bug_dir(
        &self,
        _home: &speccoder_core::bug::BugHome,
        _slug: &str,
    ) -> Option<(String, String)> {
        None
    }

    /// A new slug for `title` that no bug of `home` has yet.
    fn bug_slug(&self, _root: &Path, _home: &speccoder_core::bug::BugHome, _title: &str) -> String {
        String::new()
    }

    /// The triad as the home holds it: the bug `key` (a native home's slug,
    /// a standing unit's requirement). Pure: no model, no tool.
    ///
    /// # Errors
    ///
    /// The file is missing or not in a shape the adapter reads.
    fn read_bug(
        &self,
        _root: &Path,
        _home: &speccoder_core::bug::BugHome,
        _key: &str,
    ) -> Result<ReadBug, BugRefusal> {
        Err(BugRefusal::new("BUG-NO-UNIT", "no bug home"))
    }

    /// The selected unit's requirements with their scenarios, in document
    /// order: what a review writes tests for. None when the format yields
    /// no requirement with scenarios.
    fn oracle_requirements(&self, _snapshot: &ProjectSnapshot) -> Vec<OracleRequirement> {
        Vec::new()
    }

    /// The project's standing units and their rungs (a quick change's, a
    /// bug's): what `speccoder_core::ladder::rung_of` decides by.
    fn standing_units(&self, _root: &Path) -> Vec<(String, speccoder_core::ladder::Rung)> {
        Vec::new()
    }

    /// The unit's requirement identifiers, each with the hash of the line
    /// that defines it (`FR-001`, `SC-002` in Spec Kit): the rows of the
    /// trace matrix. None when the format has no such identifiers.
    fn requirement_ids(&self, root: &Path, unit_dir: &str) -> Vec<(String, String)> {
        let Some(file) = self.requirements_file() else {
            return Vec::new();
        };
        let text = std::fs::read_to_string(root.join(unit_dir).join(file)).unwrap_or_default();
        self.requirements_in(&text)
            .into_iter()
            .map(|(id, line)| (id, crate::integrity::sha256_hex(line.as_bytes())))
            .collect()
    }

    /// The format's per-checkout pointer to the current unit
    /// (repository-relative), if it has one.
    fn current_pointer(&self) -> Option<&'static str> {
        None
    }

    /// Records `unit_dir` (repository-relative) as the current unit in the
    /// format's own per-checkout pointer, or clears it (`None`); returns the
    /// pointer's path. `Err`: the format has no current unit.
    fn set_current(&self, _root: &Path, _unit_dir: Option<&str>) -> Result<String, String> {
        Err(self.messages().no_current.to_owned())
    }

    /// Ticks `task` (one byte, under the line-hash guard).
    fn tick(&self, tasks_file: &Path, task: &str, line_hash: &str) -> Result<(), String>;

    /// Appends convergence tasks (prefix-guarded); returns their IDs.
    fn append(
        &self,
        tasks_file: &Path,
        file_hash: &str,
        items: &[String],
    ) -> Result<Vec<String>, String>;

    /// What must be committed before a worktree is made; paths are
    /// repository-relative, `join` maps a project-relative path to one.
    fn commit_precondition(
        &self,
        root: &Path,
        join: &dyn Fn(&str) -> String,
        unit_dir: &str,
    ) -> CommitPrecondition;
}

/// A format's ruling on a command (`argv`, whether a planning skill asks):
/// `Some(Ok)` allowed, `Some(Err(why))` refused, `None` no opinion.
pub type CommandRule = fn(&[String], bool) -> Option<Result<(), String>>;

/// Which paths are the format's intent, and what a hosted planning skill may
/// touch.
#[derive(Debug)]
pub struct Rules {
    /// Globs of the intent: never written during implementation.
    pub intent: &'static [&'static str],
    /// Why an intent write is refused.
    pub intent_reason: &'static str,
    /// What a planning skill writes without asking.
    pub authoring_path: fn(&str) -> bool,
    /// The format's own commands: `Some(Ok)` runs without asking,
    /// `Some(Err(why))` is refused, `None` is classified like any command.
    /// The flag is whether a hosted planning skill asks.
    pub command: CommandRule,
    /// Environment variables every command of the project gets.
    pub env: &'static [(&'static str, &'static str)],
    /// Why a planning skill's other write needs approval (`path` given).
    pub authoring_refusal: fn(&str) -> String,
}

/// Model-facing fragments; the prompts around them are the same for every
/// format.
#[derive(Debug)]
pub struct Wording {
    /// The format as the model is told it, e.g. `GitHub Spec Kit`.
    pub product: &'static str,
    /// The format's short name, e.g. `Spec Kit`.
    pub name: &'static str,
    /// What one unit of work is called, e.g. `feature`.
    pub unit: &'static str,
    /// The intent paths, e.g. `.specify/, specs/`.
    pub intent_paths: &'static str,
    /// The same as code spans.
    pub intent_code: &'static str,
    /// The documents convergence reads, e.g. `spec.md, plan.md and tasks.md`.
    pub documents: &'static str,
    /// What a planning skill may do, e.g. `write Spec Kit artifacts (…) and
    /// run Spec Kit's own scripts under …`.
    pub authoring: &'static str,
    /// What a planning skill may leave a commit to, e.g. `a Spec Kit script`.
    pub authoring_tool: &'static str,
    /// The heading of the requirement text in a bundle.
    pub story: &'static str,
    /// The heading of the project-wide rules (`Project constitution`).
    pub constitution: &'static str,
    /// The heading of the unit's plan in the attempt prompt (`Feature plan`).
    pub plan: &'static str,
    /// The same in the external agent's prompt (`Plan`).
    pub plan_short: &'static str,
    /// What acceptance tests are derived from.
    pub examples: &'static str,
    /// A task closing a gap: `A task in a Convergence phase`.
    pub convergence_task: &'static str,
    /// The spec, plan and task list headings, as convergence shows them.
    pub files: [&'static str; 3],
    /// How convergence is told what to assess.
    pub assess: Assess,
}

/// The format's words in the convergence assessment.
#[derive(Debug)]
pub struct Assess {
    /// The governing constraints, e.g. `the constitution`.
    pub rules: &'static str,
    /// What each finding is checked against.
    pub targets: &'static str,
    /// A rule whose violation is `contradicts`, e.g. `a constitution MUST`.
    pub must: &'static str,
    /// What `unrequested` code is not called for by.
    pub sources: &'static str,
    /// What makes a finding CRITICAL.
    pub critical: &'static str,
    /// Examples of `source_ref`.
    pub refs: &'static str,
}

/// What a person is told; texts name the format's own files and commands.
#[derive(Debug)]
pub struct Messages {
    /// The task list's file name.
    pub tasks_file: &'static str,
    /// How to make the task list.
    pub make_tasks: &'static str,
    /// No project at or above the path (path given).
    pub not_a_project: fn(&Path) -> String,
    /// The short form, for commands that only need the root.
    pub no_project: &'static str,
    /// A worktree lost its project.
    pub no_project_in_worktree: &'static str,
    /// The project is not in a git repository.
    pub not_in_git: &'static str,
    /// No skills are installed.
    pub no_skills: &'static str,
    /// Unchecked checklists stop a run (the list given).
    pub checklists: fn(&str) -> String,
    /// What a unit is called: `feature`, `change`.
    pub unit: &'static str,
    /// Why there is no current unit to set.
    pub no_current: &'static str,
    /// Who else reads the pointer, said when it changes.
    pub current_shared: &'static str,
    /// The command that makes a project of this format here, and why.
    pub make_project: (&'static str, &'static str),
}

/// Hosted planning skills.
#[derive(Debug)]
pub struct Planning {
    /// The prefix installed skills carry (`plan` runs `<prefix>plan`).
    pub prefix: &'static str,
    /// Skills whose `before_<name>` / `after_<name>` hooks run.
    pub core: &'static [&'static str],
    /// Why a skill is not hosted, and what to use instead.
    pub refused: fn(&str) -> Option<String>,
    /// Core skills that create a new unit (Spec Kit: `specify`): they get
    /// no current-unit variable, and the unit they create becomes current.
    pub creates: &'static [&'static str],
    /// The variable the format's own scripts read the current unit from
    /// (Spec Kit: `SPECIFY_FEATURE_DIRECTORY`), passed to planning commands.
    pub current_env: Option<&'static str>,
    /// The skills that lead from what exists to a task list, given whether
    /// a plan exists (Spec Kit: `plan`, `tasks`).
    pub toward_tasks: fn(bool) -> &'static [&'static str],
    /// The skill that revises a written spec from a person's comments
    /// (Spec Kit: `clarify`), if the format has one.
    pub revises: Option<&'static str>,
    /// The skill that checks the planning files for consistency (Spec
    /// Kit: `analyze`), if the format has one.
    pub checks: Option<&'static str>,
}

impl Planning {
    /// The part the skill `short` (without the prefix) plays: it creates a
    /// unit, leads toward the task list, revises a spec, checks the
    /// planning, or something else — in that order of precedence.
    #[must_use]
    pub fn role_of(&self, short: &str) -> speccoder_core::actions::SkillRole {
        use speccoder_core::actions::SkillRole;
        let toward = |plan| (self.toward_tasks)(plan).contains(&short);
        if self.creates.contains(&short) {
            SkillRole::Create
        } else if toward(false) || toward(true) {
            SkillRole::TowardTasks
        } else if self.revises == Some(short) {
            SkillRole::Revise
        } else if self.checks == Some(short) {
            SkillRole::Check
        } else {
            SkillRole::Other
        }
    }
}

/// What a worktree needs committed: a worktree contains only commits.
#[derive(Debug, Clone)]
pub struct CommitPrecondition {
    /// Nothing untracked, ignored or modified under these.
    pub strict: Vec<String>,
    /// Nothing untracked or modified under these (the format ignores its
    /// own per-checkout files), except runtime state.
    pub tracked: Vec<String>,
    /// Runtime state under `tracked` that is not intent.
    pub runtime: fn(&str) -> bool,
    /// Files that must be tracked at all.
    pub essentials: Vec<String>,
    /// The refusal.
    pub message: &'static str,
    /// How to fix it.
    pub hint: &'static str,
    /// How to fix a repository without commits.
    pub no_commits_hint: &'static str,
}

impl CommitPrecondition {
    /// Every path whose change after the fork means the spec moved.
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        self.tracked.iter().chain(&self.strict).cloned().collect()
    }
}

/// The formats this build knows, in detection order.
fn all() -> [&'static dyn Detect; 3] {
    [&speckit::SPECKIT, &openspec::OPENSPEC, &stubs::KIRO]
}

/// What every known format implements: finding its marker.
pub trait Detect: Send + Sync {
    /// Which format.
    fn kind(&self) -> FormatKind;
    /// The root containing `start`, by the format's marker.
    fn root(&self, start: &Path) -> Option<PathBuf>;
    /// The adapter, when this build runs the format.
    fn adapter(&self) -> Option<&'static dyn Adapter>;
}

/// Every adapter this build runs, in detection order.
#[must_use]
pub fn adapters() -> Vec<&'static dyn Adapter> {
    all().into_iter().filter_map(|d| d.adapter()).collect()
}

/// The adapter of `kind`, if this build runs it.
#[must_use]
pub fn adapter(kind: FormatKind) -> Option<&'static dyn Adapter> {
    all()
        .into_iter()
        .find(|d| d.kind() == kind)
        .and_then(|d| d.adapter())
}

/// The reference format, the default where no project decides.
#[must_use]
pub fn reference() -> &'static dyn Adapter {
    &speckit::SPECKIT
}

static REQUEST: OnceLock<FormatKind> = OnceLock::new();

/// `--format` / `SPECCODER_FORMAT`: chosen once, by the binary, before any
/// command runs.
pub fn request(kind: FormatKind) {
    let _ = REQUEST.set(kind);
}

/// The project's format, resolved and opened.
pub struct Opened {
    /// The format's adapter.
    pub adapter: &'static dyn Adapter,
    /// What it sees.
    pub snapshot: ProjectSnapshot,
}

/// Why no project was opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// How it ends.
    pub status: CommandStatus,
    /// Why.
    pub message: String,
    /// No format's marker was found at all.
    pub unmarked: bool,
}

/// Opens the project containing `start` in the requested, configured or
/// detected format.
pub fn open(start: &Path, feature: Option<&str>) -> Result<Opened, Refused> {
    open_with(start, feature, REQUEST.get().copied())
}

/// [`open`] with an explicit choice instead of the process-wide request.
pub fn open_with(
    start: &Path,
    feature: Option<&str>,
    explicit: Option<FormatKind>,
) -> Result<Opened, Refused> {
    let configured = match explicit {
        Some(_) => None,
        None => configured(start)?,
    };
    let (adapter, mut warnings) = resolve(start, explicit.or(configured), &all())?;
    let mut snapshot = adapter.open(start, feature).ok_or_else(|| Refused {
        status: CommandStatus::NotAProject,
        message: (adapter.messages().not_a_project)(start),
        unmarked: true,
    })?;
    snapshot.diagnostics.append(&mut warnings);
    Ok(Opened { adapter, snapshot })
}

/// The adapter of a project already opened at `root` (a worktree's project,
/// a snapshot's root): resolved the way [`open`] resolved it, the reference
/// format where nothing decides.
#[must_use]
pub fn at(root: &Path) -> &'static dyn Adapter {
    let chosen = REQUEST
        .get()
        .copied()
        .or_else(|| configured(root).ok().flatten());
    resolve(root, chosen, &all()).map_or_else(|_| reference(), |(a, _)| a)
}

/// The adapter of an inspected project.
#[must_use]
pub fn of(snapshot: &ProjectSnapshot) -> &'static dyn Adapter {
    at(Path::new(&snapshot.root))
}

/// The format of the project at `start`: `chosen` when given, else the one
/// supported format whose marker is nearest. Markers of formats this build
/// does not run, at the same root, become warnings.
fn resolve(
    start: &Path,
    chosen: Option<FormatKind>,
    formats: &[&'static dyn Detect],
) -> Result<(&'static dyn Adapter, Vec<Diagnostic>), Refused> {
    let refuse = |message: String| Refused {
        status: CommandStatus::NotAProject,
        message,
        unmarked: false,
    };
    if let Some(kind) = chosen {
        return formats
            .iter()
            .find(|d| d.kind() == kind)
            .and_then(|d| d.adapter())
            .map(|a| (a, Vec::new()))
            .ok_or_else(|| refuse(not_yet(kind)));
    }
    // The nearest project decides: one nested in another repository's
    // project is its own. Only markers at the same root compete.
    let mut found: Vec<(&'static dyn Detect, PathBuf)> = formats
        .iter()
        .filter_map(|d| d.root(start).map(|r| (*d, r)))
        .collect();
    let nearest = found.iter().map(|(_, r)| r.components().count()).max();
    found.retain(|(_, r)| Some(r.components().count()) == nearest);
    let supported: Vec<&'static dyn Adapter> =
        found.iter().filter_map(|(d, _)| d.adapter()).collect();
    let others = || found.iter().filter(|(d, _)| d.adapter().is_none());
    match supported.as_slice() {
        [one] => {
            let warnings = others()
                .map(|(d, root)| {
                    Diagnostic::new(
                        Severity::Warning,
                        "format.unsupported-marker",
                        format!(
                            "{} is also present at {}; {}",
                            d.kind().title(),
                            root.display(),
                            not_yet(d.kind())
                        ),
                    )
                })
                .collect();
            Ok((*one, warnings))
        }
        [] => match others().next() {
            Some((d, root)) => Err(refuse(format!(
                "found {} project at {}; {}",
                article(d.kind()),
                root.display(),
                not_yet(d.kind())
            ))),
            // No marker at all: the reference format's own answer.
            None => Ok((reference(), Vec::new())),
        },
        many => Err(Refused {
            unmarked: false,
            status: CommandStatus::DiagnosticsError,
            message: format!(
                "this project is in more than one format ({}); choose one with --format",
                many.iter()
                    .map(|a| a.kind().name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

fn not_yet(kind: FormatKind) -> String {
    match kind {
        FormatKind::Kiro => {
            "speccoder runs Spec Kit and OpenSpec projects, not Kiro specs".to_owned()
        }
        k => format!("speccoder does not run {} projects", k.title()),
    }
}

fn article(kind: FormatKind) -> String {
    match kind {
        FormatKind::Openspec => "an OpenSpec".to_owned(),
        k => format!("a {}", k.title()),
    }
}

/// `[spec] format` of the nearest `speccoder.toml` at or above `start`.
fn configured(start: &Path) -> Result<Option<FormatKind>, Refused> {
    let Some(dir) = crate::verify::find_config(start) else {
        return Ok(None);
    };
    let Ok(text) = std::fs::read_to_string(dir.join(crate::verify::config::FILE)) else {
        return Ok(None);
    };
    parse_config(&text).map_err(|e| Refused {
        unmarked: false,
        status: CommandStatus::ConfigError,
        message: format!("speccoder.toml: {e}"),
    })
}

/// `[spec] format` from a `speccoder.toml`'s text.
pub fn parse_config(text: &str) -> Result<Option<FormatKind>, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Spec {
        format: Option<String>,
    }
    // An unreadable file is the configuration loader's to report.
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        return Ok(None);
    };
    let Some(spec) = table.get("spec") else {
        return Ok(None);
    };
    let spec: Spec = spec
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| format!("[spec]: {}", e.message()))?;
    let [a, b, c] = FormatKind::ALL.map(FormatKind::name);
    spec.format
        .map(|f| {
            f.parse()
                .map_err(|_| format!("[spec]: format = \"{f}\" ({a}, {b} or {c})"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(FormatKind, bool);

    impl Detect for Fake {
        fn kind(&self) -> FormatKind {
            self.0
        }
        fn root(&self, _: &Path) -> Option<PathBuf> {
            Some(PathBuf::from("/p"))
        }
        fn adapter(&self) -> Option<&'static dyn Adapter> {
            self.1.then(reference)
        }
    }

    struct At(FormatKind, &'static str);

    impl Detect for At {
        fn kind(&self) -> FormatKind {
            self.0
        }
        fn root(&self, _: &Path) -> Option<PathBuf> {
            Some(PathBuf::from(self.1))
        }
        fn adapter(&self) -> Option<&'static dyn Adapter> {
            Some(reference())
        }
    }

    #[test]
    fn the_nearest_project_decides() {
        static OUTER: At = At(FormatKind::Speckit, "/repo");
        static INNER: At = At(FormatKind::Openspec, "/repo/sub/project");
        let (_, w) = resolve(Path::new("/repo/sub/project/x"), None, &[&OUTER, &INNER]).unwrap();
        assert!(w.is_empty());
        static TWIN: At = At(FormatKind::Speckit, "/repo/sub/project");
        assert!(resolve(Path::new("/x"), None, &[&TWIN, &INNER]).is_err());
    }

    static SK: Fake = Fake(FormatKind::Speckit, true);
    static OS: Fake = Fake(FormatKind::Openspec, false);
    static OS_RUNS: Fake = Fake(FormatKind::Openspec, true);
    static KIRO: Fake = Fake(FormatKind::Kiro, false);

    #[test]
    fn a_choice_wins_and_an_unsupported_one_is_refused() {
        let p = Path::new("/p");
        let (a, w) = resolve(p, Some(FormatKind::Speckit), &[&SK, &OS]).unwrap();
        assert_eq!((a.kind(), w.len()), (FormatKind::Speckit, 0));
        let r = resolve(p, Some(FormatKind::Openspec), &[&SK, &OS])
            .err()
            .unwrap();
        assert_eq!(r.status, CommandStatus::NotAProject);
        assert_eq!(r.message, "speccoder does not run OpenSpec projects");
        let r = resolve(p, Some(FormatKind::Kiro), &[&SK, &KIRO])
            .err()
            .unwrap();
        assert!(r.message.ends_with("not Kiro specs"));
    }

    #[test]
    fn detection_warns_about_other_markers_and_refuses_ambiguity() {
        let p = Path::new("/p");
        let (a, w) = resolve(p, None, &[&SK, &OS]).unwrap();
        assert_eq!(a.kind(), FormatKind::Speckit);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].code, "format.unsupported-marker");
        let r = resolve(p, None, &[&OS]).err().unwrap();
        assert_eq!(
            r.message,
            "found an OpenSpec project at /p; speccoder does not run OpenSpec projects"
        );
        let r = resolve(p, None, &[&SK, &OS_RUNS]).err().unwrap();
        assert_eq!(r.status, CommandStatus::DiagnosticsError);
        assert!(
            r.message.contains("choose one with --format"),
            "{}",
            r.message
        );
    }

    #[test]
    fn the_spec_section_names_a_format() {
        assert_eq!(parse_config(""), Ok(None));
        assert_eq!(
            parse_config("[spec]\nformat = \"openspec\"\n"),
            Ok(Some(FormatKind::Openspec))
        );
        assert_eq!(
            parse_config("[spec]\nformat = \"x\"\n"),
            Err("[spec]: format = \"x\" (speckit, openspec or kiro)".to_owned())
        );
        assert!(
            parse_config("[spec]\nkind = 1\n")
                .unwrap_err()
                .starts_with("[spec]: unknown field")
        );
        assert_eq!(parse_config("[format]\nargv = [\"x\"]\n"), Ok(None));
    }
}
