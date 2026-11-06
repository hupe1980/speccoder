//! Kiro adapter for speccoder.
//!
//! Finds a Kiro (or cc-sdd) project and its specs in `.kiro/specs/<name>/`,
//! reads a spec's `requirements.md` (each `### Requirement N` with its
//! numbered EARS criteria `N.M`) or a bugfix spec's `bugfix.md`
//! ([`bugfix`]), its `tasks.md` line by line (`[ ]*` is not a Markdown
//! task), cc-sdd's approvals and the steering as Kiro includes it.
//! Read-only except [`write::tick`], [`write::close_all`] and
//! [`write::append_convergence`].

pub mod bugfix;
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

/// A spec directory speccoder can read: `tasks.md` beside its
/// requirements, or a bugfix spec's `bugfix.md` (with or without tasks).
fn is_spec(dir: &Path) -> bool {
    (dir.join("tasks.md").is_file() && dir.join("requirements.md").is_file())
        || dir.join("bugfix.md").is_file()
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

/// Bugfix spec `name`'s `bugfix.md`, read.
///
/// # Errors
///
/// The file is missing, or holds no Current or no Expected statement.
pub fn read_bugfix(root: &Path, name: &str) -> Result<bugfix::Bugfix, String> {
    let rel = format!("{SPECS}/{name}/bugfix.md");
    let text = std::fs::read_to_string(root.join(&rel)).map_err(|_| format!("{rel} is missing"))?;
    bugfix::read(&text).map_err(|e| format!("{rel}: {e}"))
}

/// cc-sdd's approval of spec `name`'s tasks (`spec.json`): `Some` with
/// why it does not allow a run — the tasks are written not approved, or
/// the file cannot be read. Kiro writes no `spec.json`: approved.
#[must_use]
pub fn unapproved(root: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(root.join(SPECS).join(name).join("spec.json")).ok()?;
    match serde_json::from_str::<serde_json::Value>(&text) {
        Err(e) => Some(format!(
            "spec.json cannot be read ({e}), so its approval cannot be either"
        )),
        Ok(v) => (v.pointer("/approvals/tasks/approved") == Some(&serde_json::Value::Bool(false)))
            .then(|| "cc-sdd has not approved this spec's tasks (spec.json)".to_owned()),
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

/// The file `rel` names, when it resolves inside `root`.
fn inside(root: &Path, rel: &str) -> Option<PathBuf> {
    let r = std::fs::canonicalize(root).ok()?;
    let p = std::fs::canonicalize(root.join(rel)).ok()?;
    p.starts_with(&r).then_some(p)
}

/// What `file:path`, `file:path:7`, `file:path:3-9` or `folder:path`
/// stands for; `None` when it does not resolve inside `root`.
fn reference(root: &Path, inner: &str) -> Option<String> {
    if let Some(dir) = inner.strip_prefix("folder:") {
        let mut names: Vec<String> = std::fs::read_dir(inside(root, dir.trim())?)
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        return Some(names.join("\n"));
    }
    let spec = inner.strip_prefix("file:")?.trim();
    let lines = |s: &str| -> Option<(usize, usize)> {
        let (a, b) = s.split_once('-').unwrap_or((s, s));
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let (path, range) = match spec.rsplit_once(':') {
        Some((p, r)) if lines(r).is_some() => (p, lines(r)),
        _ => (spec, None),
    };
    let text = std::fs::read_to_string(inside(root, path)?).ok()?;
    Some(match range {
        None => text.trim_end().to_owned(),
        Some((a, b)) => text
            .lines()
            .skip(a.saturating_sub(1))
            .take(b.saturating_sub(a) + 1)
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

/// `text` with each `#[[file:…]]` / `#[[folder:…]]` inlined from `root`,
/// once (an inlined file's own references stay as written); the ones that
/// do not resolve are kept visible and returned.
fn inline(root: &Path, text: &str) -> (String, Vec<String>) {
    let (mut out, mut unresolved, mut rest) = (String::new(), Vec::new(), text);
    while let Some(i) = rest.find("#[[") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 3..];
        let Some(end) = after.find("]]") else {
            break;
        };
        let whole = &rest[i..i + 3 + end + 2];
        match reference(root, &after[..end]) {
            Some(s) => out.push_str(&s),
            None => {
                unresolved.push(whole.to_owned());
                out.push_str(whole);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    (out, unresolved)
}

/// The steering an attempt reads, as one text — each file under a heading
/// that says when it applies — and what inspect says about it.
#[must_use]
pub fn steering_read(root: &Path) -> (String, Vec<Diagnostic>) {
    let s = parse::steering(&root.join(".kiro/steering"));
    let mut diags = Vec::new();
    let quoted = |ps: &[String]| {
        ps.iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(" or ")
    };
    let text = s
        .rules
        .iter()
        .map(|r| {
            let when = match &r.when {
                parse::When::Always => String::new(),
                parse::When::Files(ps) => {
                    format!(" — when you change files matching {}", quoted(ps))
                }
                parse::When::About(d) => format!(" — when the work is about: {d}"),
            };
            let (body, unresolved) = inline(root, &r.text);
            if !unresolved.is_empty() {
                diags.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "kiro.steering",
                        format!(
                            "{} does not resolve inside the project; the model reads it as written",
                            unresolved.join(", ")
                        ),
                    )
                    .at(format!(".kiro/steering/{}", r.name)),
                );
            }
            format!("## .kiro/steering/{}{when}\n\n{}", r.name, body.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    for (name, why) in &s.left_out {
        diags.push(
            Diagnostic::new(
                Severity::Info,
                "kiro.steering",
                format!("{name} is not read ({why})"),
            )
            .at(format!(".kiro/steering/{name}")),
        );
    }
    (text, diags)
}

/// The steering every task reads, as one text.
#[must_use]
pub fn steering_text(root: &Path) -> String {
    steering_read(root).0
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
            diags.push(match read_bugfix(&root, name) {
                Ok(b) => {
                    let ids = |letter: char, s: &[bugfix::Statement]| match s.len() {
                        0 => String::new(),
                        1 => format!(" ({letter}1)"),
                        n => format!(" ({letter}1–{letter}{n})"),
                    };
                    let unchanged = if b.unchanged.is_empty() {
                        "no Unchanged statement".to_owned()
                    } else {
                        format!(
                            "Unchanged {}{}",
                            bugfix::span(&b.unchanged),
                            ids('U', &b.unchanged)
                        )
                    };
                    Diagnostic::new(
                        Severity::Info,
                        "kiro.bugfix",
                        format!(
                            "a bugfix spec — Current {}, Expected {}{}, {unchanged}; it runs as a bug: reproduced, confirmed by you, fixed, the revert rejected",
                            bugfix::span(&b.current),
                            bugfix::span(&b.expected),
                            ids('E', &b.expected),
                        ),
                    )
                    .at(format!("{dir}/bugfix.md"))
                    .hint(format!("speccoder run --feature {name}"))
                }
                Err(e) => Diagnostic::new(Severity::Error, "kiro.bugfix-unreadable", e)
                    .at(format!("{dir}/bugfix.md"))
                    .hint("write each statement under ### Current Behavior, ### Expected Behavior and ### Unchanged Behavior as `2.1 WHEN … THEN the system SHALL …`"),
            });
        }
        if let Some(why) = unapproved(&root, name) {
            diags.push(
                Diagnostic::new(Severity::Error, "kiro.unapproved", why)
                    .at(format!("{dir}/spec.json"))
                    .hint(format!("approve them in cc-sdd (/kiro:spec-tasks {name} -y), then run again")),
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
    let (steering, mut found) = steering_read(&root);
    diags.append(&mut found);
    Ok(ProjectSnapshot {
        constitution: if steering.is_empty() {
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
