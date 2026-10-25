//! GitHub Spec Kit adapter for speccoder.
//!
//! Discovers a Spec Kit project's artifacts and parses `tasks.md` into the
//! snapshot types of `speccoder-core`. Strictly read-only: it never writes,
//! never runs project scripts, and never reads outside the project root.

pub mod checklists;
mod features;
mod hash;
mod metadata;
pub mod stories;
pub mod tasks;
pub mod workspace;

use std::path::Path;

pub use hash::sha256_hex;

use speccoder_core::snapshot::{
    Artifacts, Diagnostic, FeatureRef, FeatureSnapshot, ProjectSnapshot, Severity,
};

use crate::workspace::{ReadError, Workspace};

/// No `.specify/` directory was found from the start path upward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAProject;

/// Inspects the Spec Kit project containing `start`, selecting `feature`
/// (full directory name or numeric prefix) if given.
pub fn inspect(start: &Path, feature: Option<&str>) -> Result<ProjectSnapshot, NotAProject> {
    inspect_with(start, feature, None)
}

/// [`inspect`] with the value of `SPECIFY_FEATURE_DIRECTORY`, which the caller
/// reads, so this crate never consults the process environment itself.
pub fn inspect_with(
    start: &Path,
    feature: Option<&str>,
    feature_directory_env: Option<&str>,
) -> Result<ProjectSnapshot, NotAProject> {
    let ws = workspace::find_root(start).ok_or(NotAProject)?;
    let mut diags = Vec::new();

    let speckit = metadata::read(&ws, &mut diags);
    let constitution = metadata::constitution(&ws, &mut diags);
    let features = list_features(&ws, &mut diags);
    let selection = features::select(
        &features,
        feature,
        feature_directory_env,
        speckit.current_feature.as_deref(),
        &mut diags,
    );
    let feature = selection
        .feature
        .as_deref()
        .and_then(|name| features.iter().find(|f| f.name == name))
        .map(|f| feature_snapshot(&ws, f, &mut diags));

    Ok(ProjectSnapshot {
        root: ws.root().display().to_string(),
        speckit,
        constitution,
        features,
        selection,
        feature,
        diagnostics: diags,
        format: speccoder_core::spec::FormatKind::Speckit,
        openspec: None,
    })
}

fn list_features(ws: &Workspace, diags: &mut Vec<Diagnostic>) -> Vec<FeatureRef> {
    match ws.list_dirs(features::SPECS) {
        Ok(listing) => {
            for name in &listing.escaping {
                diags.push(metadata::read_error(
                    &format!("{}/{name}", features::SPECS),
                    &ReadError::Escape,
                ));
            }
            features::collect(&listing.inside, diags)
        }
        Err(ReadError::Missing) => {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "discover.no-specs-dir",
                    "no specs/ directory",
                )
                .hint("speccoder spec specify \"…\" creates the first feature (/speckit-specify in your editor)"),
            );
            Vec::new()
        }
        Err(e) => {
            diags.push(metadata::read_error(features::SPECS, &e));
            Vec::new()
        }
    }
}

fn feature_snapshot(
    ws: &Workspace,
    f: &FeatureRef,
    diags: &mut Vec<Diagnostic>,
) -> FeatureSnapshot {
    let p = |name: &str| format!("{}/{name}", f.dir);
    let tasks_path = p("tasks.md");
    let artifacts = Artifacts {
        spec: ws.is_file(&p("spec.md")),
        plan: ws.is_file(&p("plan.md")),
        tasks: ws.is_file(&tasks_path),
        research: ws.is_file(&p("research.md")),
        data_model: ws.is_file(&p("data-model.md")),
        contracts: ws.is_dir(&p("contracts")),
        quickstart: ws.is_file(&p("quickstart.md")),
        checklists: ws.is_dir(&p("checklists")),
    };
    let tasks = match ws.read_text(&tasks_path) {
        Ok(text) => {
            let (parsed, mut found) = tasks::parse(&tasks_path, &text);
            diags.append(&mut found);
            Some(parsed)
        }
        Err(ReadError::Missing) => {
            diags.push(
                Diagnostic::new(Severity::Warning, "tasks.missing", "no tasks.md yet")
                    .at(&tasks_path)
                    .hint("speccoder spec plan, then speccoder spec tasks (/speckit-plan, /speckit-tasks in your editor)"),
            );
            None
        }
        Err(e) => {
            diags.push(metadata::read_error(&tasks_path, &e));
            None
        }
    };
    let schedule = tasks.as_ref().map(|t| {
        let (schedule, mut found) = speccoder_core::graph::schedule(t);
        diags.append(&mut found);
        schedule
    });
    FeatureSnapshot {
        feature: f.clone(),
        artifacts,
        tasks,
        schedule,
    }
}

/// Ticks `task_id` in the `tasks.md` at `tasks_path` — the engine's one
/// sanctioned write to an artifact.
///
/// Re-parses the file, requires the task's line to still hash to
/// `expected_line_hash` (else the file changed since the attempt began), and
/// replaces the space inside the task's `[ ]` with `X`: one byte, nothing
/// else touched. Written to a sibling temporary file and renamed into place.
pub fn mark_task_complete(
    tasks_path: &Path,
    task_id: &str,
    expected_line_hash: &str,
) -> Result<(), String> {
    let text = std::fs::read_to_string(tasks_path)
        .map_err(|e| format!("{}: {e}", tasks_path.display()))?;
    let (parsed, _) = tasks::parse("tasks.md", &text);
    let task = parsed
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("{task_id} is not in {}", tasks_path.display()))?;
    if task.done {
        return Err(format!("{task_id} is already ticked"));
    }
    if task.line_hash != expected_line_hash {
        return Err(format!(
            "{task_id}'s line changed since the attempt began; not ticked"
        ));
    }
    let [start, end] = task.byte_range;
    let item = &text[start..end.min(text.len())];
    let open = item
        .find("[ ]")
        .ok_or_else(|| format!("{task_id} has no `[ ]` checkbox"))?;
    let at = start + open + 1;
    let mut bytes = text.into_bytes();
    bytes[at] = b'X';
    let tmp = tasks_path.with_extension("md.speccoder-tick");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, tasks_path).map_err(|e| format!("{}: {e}", tasks_path.display()))
}

/// Appends a `## Phase N: Convergence` section with one open task per item
/// to the `tasks.md` at `tasks_path` — the engine's other sanctioned write
/// (Spec Kit's converge append contract).
///
/// The file must still hash to `expected_file_hash`; IDs continue after the
/// highest existing one, the phase number after the highest existing phase.
/// The old bytes stay an exact prefix and every existing task re-parses
/// unchanged. No items → the file is not touched. Returns the new IDs.
pub fn append_convergence(
    tasks_path: &Path,
    expected_file_hash: &str,
    items: &[String],
) -> Result<Vec<String>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(tasks_path)
        .map_err(|e| format!("{}: {e}", tasks_path.display()))?;
    if hash::sha256_hex(text.as_bytes()) != expected_file_hash {
        return Err(format!(
            "{} changed since convergence began; nothing appended",
            tasks_path.display()
        ));
    }
    let (parsed, _) = tasks::parse("tasks.md", &text);
    let max_id = parsed
        .tasks
        .iter()
        .filter_map(|t| t.id.strip_prefix('T').and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    let phase = parsed
        .phases
        .iter()
        .filter_map(|p| {
            p.title
                .strip_prefix("Phase ")
                .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let mut section = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        section.push('\n');
    }
    section.push_str(&format!("\n## Phase {phase}: Convergence\n\n"));
    let mut ids = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let id = format!(
            "T{:03}",
            max_id + 1 + u32::try_from(i).unwrap_or(u32::MAX - max_id - 1)
        );
        // One line each; a model's text never becomes structure.
        let line = item.split_whitespace().collect::<Vec<_>>().join(" ");
        let line = line.trim_start_matches(['-', '*', ' ']);
        let line = line.strip_prefix("[ ]").unwrap_or(line).trim();
        section.push_str(&format!("- [ ] {id} {line}\n"));
        ids.push(id);
    }
    let new = format!("{text}{section}");
    // Prefix guard: the old bytes are untouched and every old task parses the same.
    let (reparsed, _) = tasks::parse("tasks.md", &new);
    let same = parsed.tasks.iter().all(|old| {
        reparsed
            .tasks
            .iter()
            .any(|t| t.id == old.id && t.line_hash == old.line_hash && t.done == old.done)
    });
    let added = ids
        .iter()
        .all(|id| reparsed.tasks.iter().any(|t| &t.id == id && !t.done));
    if !new.starts_with(&text) || !same || !added {
        return Err(
            "the convergence section would change existing tasks; nothing appended".to_owned(),
        );
    }
    let tmp = tasks_path.with_extension("md.speccoder-append");
    std::fs::write(&tmp, new.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, tasks_path).map_err(|e| format!("{}: {e}", tasks_path.display()))?;
    Ok(ids)
}
