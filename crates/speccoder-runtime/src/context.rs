//! The context bundle for one task.
//!
//! Built from files *before* a run and passed as the run's input, so the
//! journal holds exactly what the model saw and replay needs no files.
//! Deterministic: same files, same bundle, same bytes.

use serde::Serialize;
use speccoder_core::snapshot::{FeatureSnapshot, ProjectSnapshot};
use speccoder_speckit::workspace::Workspace;

/// Largest share of the prompt one document may take.
pub const PART_LIMIT: usize = 32 * 1024;

/// What the model is given about a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    /// Feature directory name.
    pub feature: String,
    /// The project constitution (may be empty).
    pub constitution: String,
    /// `plan.md` (may be empty).
    pub plan: String,
    /// The task's user-story section of `spec.md`; the whole spec for a
    /// Convergence task; or empty.
    pub story: String,
    /// The task.
    pub task: TaskContext,
    /// Its direct dependencies, in document order.
    pub dependencies: Vec<DependencyContext>,
}

/// The task as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskContext {
    /// Task ID.
    pub id: String,
    /// Title after markers.
    pub title: String,
    /// Enclosing phase heading.
    pub phase: String,
    /// Enclosing subsection, if any.
    pub subsection: String,
    /// Story label, if any.
    pub story: String,
    /// Carries `[P]`.
    pub parallel: bool,
}

/// A dependency of the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyContext {
    /// Task ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Ticked in `tasks.md`.
    pub done: bool,
}

/// Why no bundle could be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// No feature selected.
    NoFeature,
    /// The feature has no task list.
    NoTasks,
    /// The task ID is not in the list.
    NotFound(String),
}

/// Builds the bundle for `task_id` of the selected feature.
pub fn bundle(snapshot: &ProjectSnapshot, task_id: &str) -> Result<Bundle, BundleError> {
    let feature: &FeatureSnapshot = snapshot.feature.as_ref().ok_or(BundleError::NoFeature)?;
    let tasks = feature.tasks.as_ref().ok_or(BundleError::NoTasks)?;
    let task = tasks
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| BundleError::NotFound(task_id.to_owned()))?;
    let ws = Workspace::at(std::path::Path::new(&snapshot.root));
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
        constitution: read(".specify/memory/constitution.md"),
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
            None if is_convergence(task.phase.as_deref().unwrap_or_default()) => spec.clone(),
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

/// Whether a phase heading is a Convergence phase (appended by converge).
#[must_use]
pub fn is_convergence(phase: &str) -> bool {
    phase.to_lowercase().contains("convergence")
}

/// The `### User Story N` section for label `USN`, up to the next heading of
/// the same or a higher level.
#[must_use]
pub fn story_section(spec: &str, label: &str) -> String {
    let Some(n) = label.strip_prefix("US") else {
        return String::new();
    };
    let wanted = format!("user story {n}");
    let mut out = Vec::new();
    let mut level = None;
    for line in spec.lines() {
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let is_heading = hashes > 0 && line[hashes..].starts_with(' ');
        if let Some(l) = level {
            if is_heading && hashes <= l {
                break;
            }
            out.push(line);
        } else if is_heading {
            let text = line[hashes..].trim().to_lowercase();
            let names_it = text
                .strip_prefix(&wanted)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()));
            if names_it {
                level = Some(hashes);
                out.push(line);
            }
        }
    }
    out.join("\n")
}

/// Cuts text at `PART_LIMIT` bytes on a char boundary, with a marker.
fn cap(text: &str) -> String {
    if text.len() <= PART_LIMIT {
        return text.to_owned();
    }
    let mut end = PART_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n[… cut at {PART_LIMIT} bytes of {} …]",
        &text[..end],
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nAs a user…\n\n**Acceptance**: works\n\n### User Story 10 - Other\n\nno\n\n### User Story 2 - Profile\n\nprofile text\n\n## Requirements\n\n- FR-001\n";

    #[test]
    fn story_section_stops_at_the_next_story_and_does_not_confuse_10_with_1() {
        let s = story_section(SPEC, "US1");
        assert!(s.starts_with("### User Story 1 - Login"));
        assert!(s.contains("**Acceptance**: works"));
        assert!(!s.contains("Other"));
        let s2 = story_section(SPEC, "US2");
        assert!(s2.contains("profile text") && !s2.contains("FR-001"));
        assert_eq!(story_section(SPEC, "US3"), "");
    }

    #[test]
    fn cap_marks_the_cut() {
        let long = "é".repeat(PART_LIMIT);
        let capped = cap(&long);
        assert!(capped.len() < long.len());
        assert!(
            capped.contains("[… cut at 32768 bytes of 65536 …]"),
            "{}",
            &capped[capped.len() - 40..]
        );
    }
}
