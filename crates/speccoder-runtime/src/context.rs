//! The context bundle for one task.
//!
//! Built from files *before* a run and passed as the run's input, so the
//! journal holds exactly what the model saw and replay needs no files.
//! Deterministic: same files, same bundle, same bytes.

use serde::Serialize;
use speccoder_core::snapshot::ProjectSnapshot;

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

/// Builds the bundle for `task_id` of the selected feature of a project in
/// the reference format.
pub fn bundle(snapshot: &ProjectSnapshot, task_id: &str) -> Result<Bundle, BundleError> {
    crate::format::reference().bundle(snapshot, task_id)
}

/// Cuts text at `PART_LIMIT` bytes on a char boundary, with a marker.
pub(crate) fn cap(text: &str) -> String {
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
