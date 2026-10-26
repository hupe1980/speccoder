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
        include_str!("../../../assets/workflow/step.yml"),
    ),
    (
        ".specify/workflows/steps/speccoder/__init__.py",
        include_str!("../../../assets/workflow/__init__.py"),
    ),
    (
        ".specify/workflows/speccoder/workflow.yml",
        include_str!("../../../assets/workflow/workflow.yml"),
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
                "not a Spec Kit project (no .specify/ found); run `specify init --here --integration claude` first",
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

/// An overlay of one of Spec Kit's workflows: one `replace` edit of one
/// step with a `speccoder` step. The table is the general mechanism; each
/// entry is one overlay.
#[derive(Debug, Clone, Copy)]
pub struct Overlay {
    /// The name `--overlay` takes.
    pub name: &'static str,
    /// The workflow it extends.
    pub extends: &'static str,
    /// The step it replaces.
    pub replace: &'static str,
    /// An extension it needs installed and enabled.
    pub needs: Option<&'static str>,
    /// The replacing step's configuration (YAML lines under `step:`).
    pub step: &'static str,
}

/// Every overlay speccoder writes.
pub const OVERLAYS: &[Overlay] = &[Overlay {
    name: "bugfix",
    extends: "bugfix",
    replace: "test",
    needs: Some("bug"),
    step: "        id: test\n        type: speccoder\n        bug: \"{{ inputs.slug }}\"\n",
}];

/// Where an overlay's file goes.
#[must_use]
pub fn overlay_file(o: &Overlay) -> String {
    format!(".specify/workflows/overlays/{}/speccoder.yml", o.extends)
}

/// The overlay's YAML.
#[must_use]
pub fn overlay_text(o: &Overlay) -> String {
    format!(
        "# Written by `speccoder workflow install --overlay {name}`; removed by `speccoder workflow uninstall --overlay {name}`.\nschema_version: \"1.0\"\nid: speccoder-{name}\nextends: {extends}\nedits:\n  - replace: {replace}\n    step:\n{step}",
        name = o.name,
        extends = o.extends,
        replace = o.replace,
        step = o.step,
    )
}

fn project_root(path: &Path) -> Result<PathBuf, Box<CommandResult>> {
    match crate::inspect(path, None).data() {
        Some(CommandData::Inspect(s)) => Ok(PathBuf::from(&s.root)),
        _ => Err(Box::new(CommandResult::new(
            "workflow",
            CommandStatus::NotAProject,
            "not a Spec Kit project (no .specify/ found); run `specify init --here --integration claude` first",
        ))),
    }
}

fn named(name: &str) -> Result<&'static Overlay, Box<CommandResult>> {
    OVERLAYS.iter().find(|o| o.name == name).ok_or_else(|| {
        Box::new(
            CommandResult::new(
                "workflow",
                CommandStatus::UsageError,
                &format!(
                    "no overlay `{name}` (known: {})",
                    OVERLAYS
                        .iter()
                        .map(|o| o.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_code("USAGE"),
        )
    })
}

/// `speccoder workflow install --overlay NAME`: the overlay, and the
/// `speccoder` step type it uses. Refused when the workflow it extends, the
/// step it replaces or the extension it needs is absent.
#[must_use]
pub fn install_overlay(path: &Path, name: &str, force: bool) -> CommandResult {
    let root = match project_root(path) {
        Ok(r) => r,
        Err(r) => return *r,
    };
    let o = match named(name) {
        Ok(o) => o,
        Err(r) => return *r,
    };
    let refuse = |m: String| CommandResult::new("workflow", CommandStatus::ConfigError, &m);
    if let Some(ext) = o.needs
        && super::extensions::inventory(&root).is_none_or(|i| i.enabled(ext).is_none())
    {
        return refuse(format!(
            "the `{}` overlay needs Spec Kit's `{ext}` extension installed and enabled: `specify extension add {ext}`",
            o.name
        ));
    }
    let workflow = root
        .join(".specify/workflows")
        .join(o.extends)
        .join("workflow.yml");
    let Ok(text) = std::fs::read_to_string(&workflow) else {
        return refuse(format!(
            "there is no `{}` workflow to overlay: `specify workflow add {}`",
            o.extends, o.extends
        ));
    };
    if !text.lines().any(|l| {
        l.trim() == format!("- id: {}", o.replace) || l.trim() == format!("- id: \"{}\"", o.replace)
    }) {
        return refuse(format!(
            "the `{}` workflow has no step `{}` to replace",
            o.extends, o.replace
        ));
    }
    let file = root.join(overlay_file(o));
    if file.exists() && !force {
        return refuse(format!(
            "{} exists; pass --force to replace",
            overlay_file(o)
        ));
    }
    // The step type it uses, unless installed already.
    for (rel, body) in &FILES[..2] {
        let f = root.join(rel);
        if f.exists() && !force {
            continue;
        }
        if let Some(dir) = f.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            return CommandResult::new(
                "workflow",
                CommandStatus::Error,
                &format!("{}: {e}", dir.display()),
            );
        }
        if let Err(e) = std::fs::write(&f, body) {
            return CommandResult::new(
                "workflow",
                CommandStatus::Error,
                &format!("{}: {e}", f.display()),
            );
        }
    }
    if let Some(dir) = file.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return CommandResult::new(
            "workflow",
            CommandStatus::Error,
            &format!("{}: {e}", dir.display()),
        );
    }
    if let Err(e) = std::fs::write(&file, overlay_text(o)) {
        return CommandResult::new(
            "workflow",
            CommandStatus::Error,
            &format!("{}: {e}", file.display()),
        );
    }
    CommandResult::new(
        "workflow",
        CommandStatus::Ok,
        &format!(
            "installed the `{}` overlay: `specify workflow run {}` now runs speccoder for its `{}` step (it pauses for your confirmation); `speccoder workflow uninstall --overlay {}` restores the original",
            o.name, o.extends, o.replace, o.name
        ),
    )
}

/// `speccoder workflow uninstall --overlay NAME`.
#[must_use]
pub fn uninstall_overlay(path: &Path, name: &str) -> CommandResult {
    let root = match project_root(path) {
        Ok(r) => r,
        Err(r) => return *r,
    };
    let o = match named(name) {
        Ok(o) => o,
        Err(r) => return *r,
    };
    let file = root.join(overlay_file(o));
    if !file.exists() {
        return CommandResult::new(
            "workflow",
            CommandStatus::NotFound,
            &format!("the `{}` overlay is not installed", o.name),
        );
    }
    if let Err(e) = std::fs::remove_file(&file) {
        return CommandResult::new(
            "workflow",
            CommandStatus::Error,
            &format!("{}: {e}", file.display()),
        );
    }
    if let Some(dir) = file.parent() {
        let _ = std::fs::remove_dir(dir);
    }
    CommandResult::new(
        "workflow",
        CommandStatus::Ok,
        &format!(
            "removed the `{}` overlay: the `{}` workflow runs its own `{}` step again",
            o.name, o.extends, o.replace
        ),
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
