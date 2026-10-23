//! OpenSpec adapter for speccoder.
//!
//! Finds an OpenSpec project and its active changes, parses a change's
//! `tasks.md` (as OpenSpec counts tasks) and spec deltas (scenarios included),
//! and checks before any model call what `openspec archive` would refuse
//! later. Read-only except [`write::tick`] and [`write::append_convergence`];
//! it never runs OpenSpec's CLI ([`crosscheck`] only reads its JSON).

pub mod crosscheck;
pub mod delta;
pub mod discover;
mod hash;
pub mod preflight;
pub mod tasks;
pub mod write;

use std::fs;
use std::path::{Path, PathBuf};

use speccoder_core::snapshot::{
    Artifacts, ConstitutionState, Diagnostic, FeatureRef, FeatureSnapshot, ProjectSnapshot,
    Selection, SelectionReason, Severity, SpeckitMeta,
};
use speccoder_core::spec::{DeltaRequirement, FormatKind, OpenSpecMeta};

pub use hash::sha256_hex;

/// No OpenSpec directory was found from the start path upward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAProject;

/// A change's files, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Its id (directory name).
    pub id: String,
    /// `openspec/changes/<id>`, relative to the root.
    pub dir: String,
    /// The change's schema (its `.openspec.yaml`, else the project's).
    pub schema: String,
    /// `proposal.md`, if present.
    pub proposal: Option<String>,
    /// `design.md`, if present.
    pub design: Option<String>,
    /// Its spec deltas, by capability.
    pub deltas: Vec<delta::Delta>,
}

/// The change directory of `id`, relative to the root.
#[must_use]
pub fn change_dir(id: &str) -> String {
    format!("{}/changes/{id}", discover::DIR)
}

/// Reads change `id` of the project at `root`, with the deltas' problems.
#[must_use]
pub fn load_change(root: &Path, id: &str, project_schema: &str) -> (Change, Vec<Diagnostic>) {
    let dir = change_dir(id);
    let abs = root.join(&dir);
    let read = |name: &str| fs::read_to_string(abs.join(name)).ok();
    let mut diags = Vec::new();
    let deltas = discover::spec_files(&abs.join("specs"))
        .into_iter()
        .filter_map(|(cap, path)| {
            let text = fs::read_to_string(&path).ok()?;
            let rel = format!("{dir}/specs/{cap}/spec.md");
            let (d, mut found) = delta::parse(&cap, &rel, &text);
            diags.append(&mut found);
            Some(d)
        })
        .collect();
    (
        Change {
            id: id.to_owned(),
            schema: discover::change_schema(&abs).unwrap_or_else(|| project_schema.to_owned()),
            proposal: read("proposal.md"),
            design: read("design.md"),
            dir,
            deltas,
        },
        diags,
    )
}

/// Requirement names per capability of the main specs of `root`.
#[must_use]
pub fn main_specs(
    root: &Path,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let files: Vec<(String, String)> =
        discover::spec_files(&root.join(discover::DIR).join("specs"))
            .into_iter()
            .filter_map(|(cap, p)| fs::read_to_string(p).ok().map(|t| (cap, t)))
            .collect();
    delta::main_requirements(&files)
}

/// Inspects the OpenSpec project containing `start`, selecting `change` if
/// given, else the only active change. Preflight problems of the selected
/// change are among the diagnostics.
pub fn inspect(start: &Path, change: Option<&str>) -> Result<ProjectSnapshot, NotAProject> {
    let root: PathBuf = discover::find_root(start).ok_or(NotAProject)?;
    let mut diags = Vec::new();
    let config = discover::config(&root, &mut diags);
    preflight::project(&root, &config, &mut diags);
    let ids = discover::changes(&root);
    let features: Vec<FeatureRef> = ids
        .iter()
        .map(|id| FeatureRef {
            dir: change_dir(id),
            name: id.clone(),
            prefix: String::new(),
        })
        .collect();

    let selection = select(&ids, change, &mut diags);
    let mut requirements = Vec::new();
    let feature = selection.feature.as_deref().map(|id| {
        let (c, mut found) = load_change(&root, id, &config.schema);
        diags.append(&mut found);
        let tasks_rel = format!("{}/tasks.md", c.dir);
        let tasks = match fs::read_to_string(root.join(&tasks_rel)) {
            Ok(text) => {
                let (parsed, mut found) = tasks::parse(&tasks_rel, &text);
                diags.append(&mut found);
                Some(parsed)
            }
            Err(_) => {
                diags.push(
                    Diagnostic::new(Severity::Error, "no_tasks", "the change has no tasks.md")
                        .at(&tasks_rel)
                        .hint("create it with the change's propose skill"),
                );
                None
            }
        };
        let schedule = tasks.as_ref().map(|t| {
            let (s, mut found) = speccoder_core::graph::schedule(t);
            diags.append(&mut found);
            s
        });
        preflight::change(&c, &main_specs(&root), &mut diags);
        requirements = requirements_of(&c);
        let abs = root.join(&c.dir);
        FeatureSnapshot {
            feature: FeatureRef {
                dir: c.dir.clone(),
                name: c.id.clone(),
                prefix: String::new(),
            },
            artifacts: Artifacts {
                spec: !c.deltas.is_empty(),
                plan: abs.join("design.md").is_file(),
                tasks: tasks.is_some(),
                research: abs.join("proposal.md").is_file(),
                ..Artifacts::default()
            },
            tasks,
            schedule,
        }
    });

    Ok(ProjectSnapshot {
        root: root.display().to_string(),
        speckit: SpeckitMeta::default(),
        constitution: if config.context.is_some() {
            ConstitutionState::Present
        } else {
            ConstitutionState::Absent
        },
        features,
        selection,
        feature,
        diagnostics: diags,
        format: FormatKind::Openspec,
        openspec: Some(OpenSpecMeta {
            schema: config.schema,
            context: config.context,
            unit_kind: "change".to_owned(),
            requirements,
        }),
    })
}

fn select(ids: &[String], wanted: Option<&str>, diags: &mut Vec<Diagnostic>) -> Selection {
    let none = Selection {
        reason: SelectionReason::None,
        feature: None,
    };
    match wanted {
        Some(w) => {
            if ids.iter().any(|i| i == w) {
                Selection {
                    reason: SelectionReason::Argument,
                    feature: Some(w.to_owned()),
                }
            } else {
                diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "select.not-found",
                        format!("no active change `{w}`"),
                    )
                    .hint(if ids.is_empty() {
                        "there are no active changes under openspec/changes/".to_owned()
                    } else {
                        format!("active changes: {}", ids.join(", "))
                    }),
                );
                none
            }
        }
        None => match ids {
            [only] => Selection {
                reason: SelectionReason::OnlyFeature,
                feature: Some(only.clone()),
            },
            [] => {
                diags.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "select.no-features",
                        "no active changes under openspec/changes/",
                    )
                    .hint("propose one with `speccoder spec propose`"),
                );
                none
            }
            many => {
                diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "select.ambiguous",
                        format!("{} active changes: {}", many.len(), many.join(", ")),
                    )
                    .hint("choose one with --change <id>"),
                );
                none
            }
        },
    }
}

/// A change's delta requirements, renames as their new name.
fn requirements_of(c: &Change) -> Vec<DeltaRequirement> {
    let mut out = Vec::new();
    for d in &c.deltas {
        for r in &d.requirements {
            out.push(DeltaRequirement {
                capability: d.capability.clone(),
                operation: r.operation.name().to_owned(),
                name: r.name.clone(),
                scenarios: r.scenarios.iter().map(|s| s.name.clone()).collect(),
                path: d.path.clone(),
                line: r.line,
            });
        }
        for n in &d.renames {
            out.push(DeltaRequirement {
                capability: d.capability.clone(),
                operation: "renamed".to_owned(),
                name: n.to.clone(),
                scenarios: Vec::new(),
                path: d.path.clone(),
                line: n.line,
            });
        }
    }
    out
}
