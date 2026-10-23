//! `speccoder workflow install`: speccoder as a Spec Kit workflow
//! step type, and a workflow that uses it.
//!
//! Spec Kit owns the phase level (`specify workflow run`, gates, resume);
//! speccoder owns the task level. The step type `speccoder` maps speccoder's
//! exit code onto Spec Kit's step status — 23 (a person is needed) becomes
//! `paused`, so `specify workflow resume` continues after the decision.

use std::path::{Path, PathBuf};

use speccoder_core::{CommandData, CommandResult, CommandStatus};

/// The files, by project-relative path.
pub const FILES: [(&str, &str); 3] = [
    (
        ".specify/workflows/steps/speccoder/step.yml",
        include_str!("../assets/workflow/step.yml"),
    ),
    (
        ".specify/workflows/steps/speccoder/__init__.py",
        include_str!("../assets/workflow/__init__.py"),
    ),
    (
        ".specify/workflows/speccoder/workflow.yml",
        include_str!("../assets/workflow/workflow.yml"),
    ),
];

/// Writes the step type and the workflow into the Spec Kit project at `path`.
#[must_use]
pub fn install(path: &Path, force: bool) -> CommandResult {
    let root = match crate::inspect(path, None).data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => {
            return CommandResult::new(
                "workflow",
                CommandStatus::NotAProject,
                "not a Spec Kit project (no .specify/ found); run `specify init` first",
            );
        }
    };
    let existing: Vec<&str> = FILES
        .iter()
        .filter(|(p, _)| root.join(p).exists())
        .map(|(p, _)| *p)
        .collect();
    if !existing.is_empty() && !force {
        return CommandResult::new(
            "workflow",
            CommandStatus::ConfigError,
            &format!("{} exist(s); pass --force to replace", existing.join(", ")),
        );
    }
    for (rel, text) in FILES {
        let file = root.join(rel);
        if let Some(dir) = file.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            return CommandResult::new(
                "workflow",
                CommandStatus::Error,
                &format!("{}: {e}", dir.display()),
            );
        }
        if let Err(e) = std::fs::write(&file, text) {
            return CommandResult::new(
                "workflow",
                CommandStatus::Error,
                &format!("{}: {e}", file.display()),
            );
        }
    }
    CommandResult::new(
        "workflow",
        CommandStatus::Ok,
        "installed the `speccoder` step type and workflow; run it with `specify workflow run .specify/workflows/speccoder/workflow.yml --input spec=\"…\"` (resume after a pause with `specify workflow resume <run>`)",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assets_declare_the_step_type_and_use_it() {
        assert!(FILES[0].1.contains("type_key: \"speccoder\""));
        assert!(
            FILES[1].1.contains("type_key = \"speccoder\"")
                && FILES[1].1.contains("NEEDS_HUMAN = 23")
        );
        assert!(FILES[2].1.contains("type: speccoder") && FILES[2].1.contains("converge: true"));
    }
}
