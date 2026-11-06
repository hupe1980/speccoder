//! Kiro adapter for speccoder.
//!
//! Finds a Kiro (or cc-sdd) project and its specs in `.kiro/specs/<name>/`,
//! reads a spec's `requirements.md` (each `### Requirement N` with its
//! numbered EARS criteria `N.M`) and its `tasks.md` line by line (`[ ]*` is
//! not a Markdown task), and the steering every task reads. Read-only
//! except [`write::tick`] and [`write::append_convergence`].

mod hash;
pub mod parse;
pub mod write;

use std::path::{Path, PathBuf};

use speccoder_core::snapshot::{
    Artifacts, ConstitutionState, Diagnostic, FeatureRef, FeatureSnapshot, ProjectSnapshot,
    Selection, SelectionReason, Severity, SpeckitMeta,
};
use speccoder_core::spec::FormatKind;

pub use hash::sha256_hex;

/// Where Kiro keeps its specs, relative to the project root.
pub const SPECS: &str = ".kiro/specs";

/// No Kiro spec was found from the start path upward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAProject;

/// A spec directory speccoder can read: `tasks.md` beside its requirements.
fn is_spec(dir: &Path) -> bool {
    dir.join("tasks.md").is_file()
        && (dir.join("requirements.md").is_file() || dir.join("bugfix.md").is_file())
}

/// The specs under `root`, by name.
#[must_use]
pub fn specs(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(SPECS))
        .map(|rd| {
            rd.flatten()
                .filter(|e| is_spec(&e.path()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The nearest directory at or above `start` holding a Kiro spec.
#[must_use]
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(start).ok()?;
    start
        .ancestors()
        .find(|d| !specs(d).is_empty())
        .map(Path::to_path_buf)
}

/// A spec's files, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// Its requirements, in order (none for a bugfix spec).
    pub requirements: Vec<parse::Requirement>,
    /// `design.md`, if present.
    pub design: Option<String>,
    /// A bugfix spec: `bugfix.md` and no `requirements.md`.
    pub bugfix: bool,
}

/// Reads spec `name` of the project at `root`.
#[must_use]
pub fn load(root: &Path, name: &str) -> Spec {
    let dir = root.join(SPECS).join(name);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
    Spec {
        requirements: read("requirements.md")
            .map(|t| parse::requirements(&t))
            .unwrap_or_default(),
        design: read("design.md"),
        bugfix: !dir.join("requirements.md").is_file() && dir.join("bugfix.md").is_file(),
    }
}

/// The spec's `tasks.md` read, a whole requirement (`3`) cited as its criteria.
#[must_use]
pub fn tasks_of(root: &Path, name: &str, spec: &Spec) -> Option<(parse::Tasks, Vec<Diagnostic>)> {
    let rel = format!("{SPECS}/{name}/tasks.md");
    let text = std::fs::read_to_string(root.join(&rel)).ok()?;
    let (mut t, diags) = parse::tasks(&rel, &text);
    for task in &mut t.parsed.tasks {
        task.scenarios = task
            .scenarios
            .iter()
            .flat_map(
                |c| match spec.requirements.iter().find(|r| &r.number == c) {
                    Some(r) => r.criteria.iter().map(|x| x.id.clone()).collect(),
                    None => vec![c.clone()],
                },
            )
            .collect();
    }
    Some((t, diags))
}

/// The steering every task reads, as one text.
#[must_use]
pub fn steering_text(root: &Path) -> String {
    parse::steering(&root.join(".kiro/steering"))
        .into_iter()
        .map(|(name, text)| format!("## .kiro/steering/{name}\n\n{}", text.trim_end()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn select(names: &[String], wanted: Option<&str>, diags: &mut Vec<Diagnostic>) -> Selection {
    let none = Selection {
        reason: SelectionReason::None,
        feature: None,
    };
    match (wanted, names) {
        (Some(w), _) if names.iter().any(|n| n == w) => Selection {
            reason: SelectionReason::Argument,
            feature: Some(w.to_owned()),
        },
        (Some(w), _) => {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "select.not-found",
                    format!("no Kiro spec `{w}`"),
                )
                .hint(format!("specs: {}", names.join(", "))),
            );
            none
        }
        (None, [only]) => Selection {
            reason: SelectionReason::OnlyFeature,
            feature: Some(only.clone()),
        },
        (None, []) => none,
        (None, many) => {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "select.ambiguous",
                    format!("{} Kiro specs: {}", many.len(), many.join(", ")),
                )
                .hint("choose one with --feature <name>"),
            );
            none
        }
    }
}

/// Inspects the Kiro project containing `start`, selecting spec `feature`
/// if given, else the only spec. A bugfix spec and an unreadable task list
/// are among the diagnostics.
///
/// # Errors
///
/// No Kiro spec at or above `start`.
pub fn inspect(start: &Path, feature: Option<&str>) -> Result<ProjectSnapshot, NotAProject> {
    let root = find_root(start).ok_or(NotAProject)?;
    let names = specs(&root);
    let mut diags = Vec::new();
    let selection = select(&names, feature, &mut diags);
    let feature = selection.feature.as_deref().map(|name| {
        let spec = load(&root, name);
        let dir = format!("{SPECS}/{name}");
        if spec.bugfix {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "kiro.bugfix",
                    format!("{name} is a bugfix spec: speccoder runs Kiro feature specs (requirements.md), not bugfix specs yet"),
                )
                .at(format!("{dir}/bugfix.md"))
                .hint("fix it in Kiro, or write the fix as a feature spec"),
            );
        }
        let (tasks, optional) = match tasks_of(&root, name, &spec) {
            Some((t, mut found)) => {
                diags.append(&mut found);
                (Some(t.parsed), t.optional)
            }
            None => (None, Vec::new()),
        };
        if !optional.is_empty() {
            diags.push(
                Diagnostic::new(
                    Severity::Info,
                    "kiro.optional",
                    format!(
                        "optional task{} {} {} not run",
                        if optional.len() == 1 { "" } else { "s" },
                        optional.join(", "),
                        if optional.len() == 1 { "is" } else { "are" }
                    ),
                )
                .at(format!("{dir}/tasks.md")),
            );
        }
        let schedule = tasks.as_ref().map(|t| {
            let (s, mut found) = speccoder_core::graph::schedule(t);
            diags.append(&mut found);
            s
        });
        FeatureSnapshot {
            feature: FeatureRef {
                dir: dir.clone(),
                name: name.to_owned(),
                prefix: String::new(),
            },
            artifacts: Artifacts {
                spec: !spec.requirements.is_empty() || spec.bugfix,
                plan: spec.design.is_some(),
                tasks: tasks.is_some(),
                ..Artifacts::default()
            },
            tasks,
            schedule,
        }
    });
    Ok(ProjectSnapshot {
        constitution: if steering_text(&root).is_empty() {
            ConstitutionState::Absent
        } else {
            ConstitutionState::Present
        },
        root: root.display().to_string(),
        speckit: SpeckitMeta::default(),
        features: names
            .iter()
            .map(|n| FeatureRef {
                dir: format!("{SPECS}/{n}"),
                name: n.clone(),
                prefix: String::new(),
            })
            .collect(),
        selection,
        feature,
        diagnostics: diags,
        format: FormatKind::Kiro,
        openspec: None,
    })
}
