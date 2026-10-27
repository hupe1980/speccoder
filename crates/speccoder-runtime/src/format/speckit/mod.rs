//! GitHub Spec Kit, the reference format: `.specify/` and `specs/NNN-name/`
//! with `spec.md`, `plan.md` and `tasks.md`. Parsing is the
//! `speccoder-speckit` crate's; this adapter is the only runtime code that
//! knows Spec Kit's paths and words.

pub mod bench;
pub mod bug;
pub mod extensions;
pub mod hooks;
pub mod preset;
mod quick;
mod texts;
pub mod workflow;

use std::path::{Path, PathBuf};

use speccoder_core::ChecklistStatus;
use speccoder_core::snapshot::{Diagnostic, FeatureSnapshot, ParsedTasks, ProjectSnapshot};
use speccoder_core::spec::{FormatKind, ScenarioRef};

pub use speccoder_speckit::stories::story_section;
pub use speccoder_speckit::workspace::Workspace;
pub use texts::{MESSAGES, PLANNING, RULES, WORDING};

use crate::context::{Bundle, BundleError, DependencyContext, TaskContext, cap};
use crate::format::{Adapter, CommitPrecondition, Detect, Messages, Planning, Rules, Wording};

/// The Spec Kit adapter.
#[derive(Debug)]
pub struct SpecKit;

/// The one instance.
pub static SPECKIT: SpecKit = SpecKit;

impl Detect for SpecKit {
    fn kind(&self) -> FormatKind {
        FormatKind::Speckit
    }

    fn root(&self, start: &Path) -> Option<PathBuf> {
        speccoder_speckit::workspace::find_root(start).map(|ws| ws.root().to_path_buf())
    }

    fn adapter(&self) -> Option<&'static dyn Adapter> {
        Some(&SPECKIT)
    }
}

impl Adapter for SpecKit {
    fn plan_structure(&self, snapshot: &ProjectSnapshot) -> Option<crate::format::PlanStructure> {
        use crate::format::{PhaseKind, PlannedTask, code_spans};
        let feature = snapshot.feature.as_ref()?;
        let tasks = feature.tasks.as_ref()?;
        let kind = |phase: &str, story: bool| {
            let p = phase.to_ascii_lowercase();
            if p.contains("setup") {
                PhaseKind::Setup
            } else if p.contains("foundational") {
                PhaseKind::Foundational
            } else if story || p.contains("user story") {
                PhaseKind::Story
            } else if p.contains("polish") || p.contains("cross-cutting") {
                PhaseKind::Polish
            } else {
                PhaseKind::Other
            }
        };
        let mut stories = std::collections::BTreeMap::new();
        let planned = tasks
            .tasks
            .iter()
            .map(|t| {
                if let Some(st) = &t.story
                    && !stories.contains_key(st)
                    && let Ok(b) = self.bundle(snapshot, &t.id)
                {
                    stories.insert(st.clone(), b.story);
                }
                PlannedTask {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    phase: kind(t.phase.as_deref().unwrap_or_default(), t.story.is_some()),
                    story: t.story.clone(),
                    spans: code_spans(&t.title),
                    done: t.done,
                }
            })
            .collect();
        Some(crate::format::PlanStructure {
            tasks: planned,
            stories,
        })
    }

    fn init_steps(&self, choices: &crate::format::FormatInit) -> Vec<crate::format::InitStep> {
        use speccoder_core::new_project::ExtensionKind;
        let script = if cfg!(windows) { "ps" } else { "sh" };
        let mut argv: Vec<String> = [
            "specify",
            "init",
            "--here",
            "--force",
            "--non-interactive",
            "--integration",
            "claude",
            "--script",
            script,
            "--ignore-agent-tools",
        ]
        .map(str::to_owned)
        .to_vec();
        if choices.preset {
            argv.push("--preset".to_owned());
            argv.push(preset::dir(&choices.state).display().to_string());
        }
        for e in &choices.extensions {
            argv.push("--extension".to_owned());
            argv.push(e.source.clone());
        }
        let hosts: Vec<String> = choices.extensions.iter().filter_map(|e| e.host()).collect();
        if choices
            .extensions
            .iter()
            .any(|e| e.kind == ExtensionKind::Url)
        {
            argv.push("--trust-extension-urls".to_owned());
        }
        vec![crate::format::InitStep {
            argv,
            hosts,
            writes: ".specify/, .claude/skills/speckit-*/".to_owned(),
            env: Vec::new(),
            marker: ".specify",
            note: choices
                .preset
                .then(|| "with the speccoder preset: tasks that plan for evidence".to_owned()),
            preset: choices.preset,
        }]
    }

    fn prepare_init(&self, choices: &crate::format::FormatInit) -> Result<(), String> {
        if choices.preset {
            preset::extract(&choices.state)?;
        }
        Ok(())
    }

    fn tool(&self) -> Option<(&'static str, &'static str)> {
        Some((
            "specify",
            "uv tool install specify-cli --from git+https://github.com/github/spec-kit.git",
        ))
    }

    fn kind(&self) -> FormatKind {
        FormatKind::Speckit
    }

    fn detect(&self, start: &Path) -> Option<PathBuf> {
        Detect::root(self, start)
    }

    fn inspect(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot> {
        speccoder_speckit::inspect(start, feature).ok()
    }

    fn open(&self, start: &Path, feature: Option<&str>) -> Option<ProjectSnapshot> {
        let env = std::env::var(texts::FEATURE_ENV)
            .ok()
            .filter(|v| !v.is_empty());
        speccoder_speckit::inspect_with(start, feature, env.as_deref()).ok()
    }

    fn parse_tasks(&self, path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>) {
        speccoder_speckit::tasks::parse(path, text)
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

    fn current_pointer(&self) -> Option<&'static str> {
        Some(texts::POINTER)
    }

    fn set_current(&self, root: &Path, unit_dir: Option<&str>) -> Result<String, String> {
        // Spec Kit's own per-checkout pointer (git-ignored by `.specify/.gitignore`).
        let file = root.join(texts::POINTER);
        match unit_dir {
            Some(dir) => {
                let text =
                    serde_json::to_string_pretty(&serde_json::json!({ "feature_directory": dir }))
                        .map_err(|e| e.to_string())?;
                std::fs::write(&file, format!("{text}\n"))
                    .map_err(|e| format!("{}: {e}", file.display()))?;
            }
            None => match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", file.display())),
            },
        }
        Ok(texts::POINTER.to_owned())
    }

    fn bundle(&self, snapshot: &ProjectSnapshot, task_id: &str) -> Result<Bundle, BundleError> {
        let feature: &FeatureSnapshot = snapshot.feature.as_ref().ok_or(BundleError::NoFeature)?;
        let tasks = feature.tasks.as_ref().ok_or(BundleError::NoTasks)?;
        let task = tasks
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| BundleError::NotFound(task_id.to_owned()))?;
        let ws = Workspace::at(Path::new(&snapshot.root));
        let read = |rel: &str| {
            ws.as_ref()
                .and_then(|ws| ws.read_text(rel).ok())
                .map(|t| cap(&t))
                .unwrap_or_default()
        };
        let dir = &feature.feature.dir;
        let spec = read(&format!("{dir}/spec.md"));

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
            constitution: read(texts::CONSTITUTION),
            plan: read(&format!("{dir}/plan.md")),
            // A Convergence task closes a gap found against the whole spec; it
            // gets the whole spec, so its acceptance tests have a source.
            story: match task.story.as_deref() {
                // A labelled task whose `### User Story N` section is missing
                // (a spec that left the template's headings) still has the spec
                // as its intent: acceptance tests are derived from all of it.
                Some(label) => {
                    let section = story_section(&spec, label);
                    if section.trim().is_empty() {
                        spec.clone()
                    } else {
                        section
                    }
                }
                None if self.is_convergence(task.phase.as_deref().unwrap_or_default()) => {
                    spec.clone()
                }
                None => String::new(),
            },
            task: TaskContext {
                id: task.id.clone(),
                title: task.title.clone(),
                phase: task.phase.clone().unwrap_or_default(),
                subsection: task.subsection.clone().unwrap_or_default(),
                story: task.story.clone().unwrap_or_default(),
                parallel: task.parallel,
            },
            dependencies,
        })
    }

    fn scenarios(&self, snapshot: &ProjectSnapshot, task: &str) -> Vec<ScenarioRef> {
        let Some(f) = snapshot.feature.as_ref() else {
            return Vec::new();
        };
        let label = f
            .tasks
            .as_ref()
            .and_then(|t| t.tasks.iter().find(|t| t.id == task))
            .and_then(|t| t.story.clone());
        let spec = Workspace::at(Path::new(&snapshot.root))
            .and_then(|ws| ws.read_text(&format!("{}/spec.md", f.feature.dir)).ok())
            .unwrap_or_default();
        // A labelled task whose story is missing has the whole spec as its
        // intent, as in its bundle.
        let own = speccoder_speckit::stories::scenario_refs(&spec, label.as_deref());
        if own.is_empty() && label.is_some() {
            speccoder_speckit::stories::scenario_refs(&spec, None)
        } else {
            own
        }
    }

    fn acceptance_testable(&self, bundle: &Bundle) -> bool {
        bundle.story.to_lowercase().contains("acceptance")
    }

    /// Each user story with its scenarios, keyed `USn`, citing `USn-k`.
    fn oracle_requirements(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Vec<crate::format::OracleRequirement> {
        let Some(f) = snapshot.feature.as_ref() else {
            return Vec::new();
        };
        let spec = Workspace::at(Path::new(&snapshot.root))
            .and_then(|ws| ws.read_text(&format!("{}/spec.md", f.feature.dir)).ok())
            .unwrap_or_default();
        let (stories, _) = speccoder_speckit::stories::requirements("spec.md", &spec);
        stories
            .into_iter()
            .map(|r| crate::format::OracleRequirement {
                scenarios: speccoder_speckit::stories::scenario_refs(&spec, Some(&r.key)),
                key: r.key,
                title: r.title,
                text: r.text,
            })
            .collect()
    }

    /// `NNN-quick-changes` and `NNN-bugfixes`.
    fn standing_units(&self, root: &Path) -> Vec<(String, speccoder_core::ladder::Rung)> {
        use speccoder_core::ladder::Rung;
        let mut out: Vec<(String, Rung)> =
            std::fs::read_dir(root.join(speccoder_speckit::features::SPECS))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| speccoder_speckit::features::prefix(n).is_some())
                .filter_map(|n| {
                    let rung = if n.ends_with("-quick-changes") {
                        Rung::Quick
                    } else if n.ends_with("-bugfixes") {
                        Rung::Bug
                    } else {
                        return None;
                    };
                    Some((n, rung))
                })
                .collect();
        out.push((".specify/bugs".to_owned(), Rung::Bug));
        out
    }

    /// A Convergence phase, appended by converge.
    fn is_convergence(&self, phase: &str) -> bool {
        phase.to_lowercase().contains("convergence")
    }

    fn convergence_texts(&self, root: &Path, unit_dir: &Path) -> [String; 4] {
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
        [
            read(root.join(texts::CONSTITUTION)),
            read(unit_dir.join("spec.md")),
            read(unit_dir.join("plan.md")),
            read(unit_dir.join("tasks.md")),
        ]
    }

    fn checklists(&self, root: &Path, unit_dir: &str) -> Vec<ChecklistStatus> {
        speccoder_speckit::checklists::status(root, unit_dir)
    }

    fn requirements_file(&self) -> Option<&'static str> {
        Some("spec.md")
    }

    fn record_change(
        &self,
        root: &Path,
        request: &str,
        checks: &[String],
    ) -> Result<super::Recorded, String> {
        quick::record(root, request, checks)
    }

    fn extensions(&self, root: &Path) -> Option<crate::format::Inventory> {
        extensions::inventory(root)
    }

    fn bug_unit(
        &self,
        root: &Path,
    ) -> Result<speccoder_core::bug::BugHome, crate::format::BugRefusal> {
        bug::unit(root)
    }

    fn record_bug(
        &self,
        root: &Path,
        home: &speccoder_core::bug::BugHome,
        triad: &speccoder_core::bug::Triad,
    ) -> Result<crate::format::RecordedBug, crate::format::BugRefusal> {
        bug::record(root, home, triad)
    }

    fn bug_step(&self) -> Option<crate::format::BugStep> {
        Some(bug::STEP)
    }

    fn bug_dir(&self, home: &speccoder_core::bug::BugHome, slug: &str) -> Option<(String, String)> {
        bug::dir(home, slug)
    }

    fn bug_slug(&self, root: &Path, _home: &speccoder_core::bug::BugHome, title: &str) -> String {
        bug::slug(root, title)
    }

    fn read_bug(
        &self,
        root: &Path,
        home: &speccoder_core::bug::BugHome,
        key: &str,
    ) -> Result<crate::format::ReadBug, crate::format::BugRefusal> {
        bug::read(root, home, key)
    }

    fn requirements_in(&self, text: &str) -> Vec<(String, String)> {
        // `- **FR-001**: …` and `- **SC-001**: …` define the identifiers.
        text.lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix("- **")?;
                let id = rest.split("**").next()?;
                let (kind, number) = id.split_once('-')?;
                (matches!(kind, "FR" | "SC")
                    && !number.is_empty()
                    && number.chars().all(|c| c.is_ascii_digit()))
                .then(|| (id.to_owned(), line.trim_end().to_owned()))
            })
            .collect()
    }

    fn tick(&self, tasks_file: &Path, task: &str, line_hash: &str) -> Result<(), String> {
        speccoder_speckit::mark_task_complete(tasks_file, task, line_hash)
    }

    fn append(
        &self,
        tasks_file: &Path,
        file_hash: &str,
        items: &[String],
    ) -> Result<Vec<String>, String> {
        speccoder_speckit::append_convergence(tasks_file, file_hash, items)
    }

    fn commit_precondition(
        &self,
        _root: &Path,
        join: &dyn Fn(&str) -> String,
        unit_dir: &str,
    ) -> CommitPrecondition {
        precondition(&join(texts::MARKER), &join(unit_dir))
    }
}

/// What a Spec Kit worktree needs committed, given the repository-relative
/// `.specify` and feature directories.
#[must_use]
pub fn precondition(specify_dir: &str, feature_dir: &str) -> CommitPrecondition {
    CommitPrecondition {
        // The feature directory: nothing untracked, ignored or modified.
        strict: vec![feature_dir.to_owned()],
        // .specify: Spec Kit ignores its own per-checkout files (feature.json);
        // those are fine. Untracked or modified files are not.
        tracked: vec![specify_dir.to_owned()],
        runtime,
        essentials: vec![
            format!("{specify_dir}/init-options.json"),
            format!("{feature_dir}/spec.md"),
        ],
        message: "the feature's Spec Kit artifacts are not committed, so a worktree would not contain them",
        hint: "commit them (git add .specify specs/<feature> && git commit), or run in place: speccoder run --in-place",
        no_commits_hint: "commit the Spec Kit artifacts first (git add .specify specs && git commit)",
    }
}

/// Runtime state is not an artifact: Spec Kit's own workflow runs
/// (`.specify/workflows/runs/`, dirty while `specify workflow run` drives
/// speccoder) and Python bytecode caches of installed step types.
fn runtime(p: &str) -> bool {
    p.contains("/workflows/runs/")
        || p.starts_with(".specify/workflows/runs/")
        || p.split('/').any(|c| c == "__pycache__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_ids_are_the_bold_fr_and_sc_items() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("specs/001-x")).unwrap();
        std::fs::write(
            dir.path().join("specs/001-x/spec.md"),
            "## Requirements\n\n- **FR-001**: System MUST parse.\n- **FR-002**: System MUST render.\n  - **FR-X**: not one\n- **Key**: not one\n\n## Success\n\n- **SC-001**: Under a second.\n",
        )
        .unwrap();
        let ids: Vec<String> = SpecKit
            .requirement_ids(dir.path(), "specs/001-x")
            .into_iter()
            .map(|(id, hash)| {
                assert_eq!(hash.len(), 64);
                id
            })
            .collect();
        assert_eq!(ids, ["FR-001", "FR-002", "SC-001"]);
    }
}
