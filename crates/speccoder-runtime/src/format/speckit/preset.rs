//! The `speccoder` Spec Kit preset (F-134): a tasks template that plans
//! for evidence, bundled in the binary and installed through Spec Kit's own
//! `specify preset add --dev` — on a person's confirmed command, never from
//! the network. Spec 101's New Project passes [`extract`]'s directory to
//! `specify init --preset`.

use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_core::{CommandData, CommandResult, CommandStatus, PresetReport, PresetState};

/// The preset's id.
pub const ID: &str = "speccoder";

const MANIFEST: &str = include_str!("../../../assets/preset/speccoder/preset.yml");
const TEMPLATE: &str = include_str!("../../../assets/preset/speccoder/templates/tasks-template.md");

/// The Spec Kit version the preset needs (`requires.speckit_version`).
pub const NEEDS: (u64, u64, u64) = (1, 2, 0);

/// How long a `specify` command may take.
const SPECIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// This build's preset version: speccoder's.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Where [`extract`] puts the bundled preset: `<state>/presets/speccoder-<version>/`
/// (read-only: a plan names it before anything is written).
#[must_use]
pub fn dir(state: &Path) -> PathBuf {
    state.join("presets").join(format!("{ID}-{}", version()))
}

/// Writes the bundled preset to `<state>/presets/speccoder-<version>/`
/// (idempotent: files already as bundled are kept) and returns that
/// directory — what `specify preset add --dev` and `specify init --preset`
/// are given (FR-044).
///
/// # Errors
///
/// The directory cannot be written.
pub fn extract(state: &Path) -> Result<PathBuf, String> {
    let dir = dir(state);
    let files = [
        ("preset.yml", MANIFEST.replace("{version}", version())),
        ("templates/tasks-template.md", TEMPLATE.to_owned()),
    ];
    for (rel, content) in &files {
        let f = dir.join(rel);
        if std::fs::read_to_string(&f).ok().as_deref() == Some(content.as_str()) {
            continue;
        }
        if let Some(p) = f.parent() {
            std::fs::create_dir_all(p)
                .map_err(|e| format!("cannot create {}: {e}", p.display()))?;
        }
        std::fs::write(&f, content).map_err(|e| format!("cannot write {}: {e}", f.display()))?;
    }
    Ok(dir)
}

/// Runs `specify` in `root` with a deadline; `None` when it cannot run or
/// does not finish.
fn specify(root: &Path, args: &[&str]) -> Option<std::process::Output> {
    let mut child = Command::new("specify")
        .args(args)
        .envs(super::python::command_env(&|k| std::env::var(k).ok()))
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let began = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if began.elapsed() > SPECIFY_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(_) => return None,
        }
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let v = text
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let mut n = v.split('.').map(|p| {
        p.trim_matches(|c: char| !c.is_ascii_digit())
            .parse::<u64>()
            .ok()
    });
    Some((
        n.next()??,
        n.next().flatten().unwrap_or(0),
        n.next().flatten().unwrap_or(0),
    ))
}

/// The installed `specify`'s version, when it runs.
#[must_use]
pub fn speckit_version() -> Option<(u64, u64, u64)> {
    let out = specify(Path::new("."), &["--version"])?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().rev().find_map(parse_version)
}

/// The preset as Spec Kit reports it in the project at `root`.
#[must_use]
pub fn state(root: &Path) -> PresetState {
    // Not in the project: nothing to ask Spec Kit (doctor stays fast).
    let dir = root.join(".specify/presets").join(ID);
    if !dir.is_dir() {
        return PresetState {
            installed: None,
            active_for_tasks_template: false,
            bundled: version().to_owned(),
        };
    }
    let listed = specify(root, &["preset", "list"])
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let from_manifest = std::fs::read_to_string(dir.join("preset.yml"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.trim().strip_prefix("version:"))
                .map(|v| v.trim().trim_matches('"').to_owned())
        });
    let installed = from_manifest.or_else(|| {
        listed
            .lines()
            .find(|l| l.contains(ID) && !l.contains("No presets"))
            .and_then(|l| {
                l.split_whitespace().find_map(|w| {
                    parse_version(w).map(|_| {
                        w.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                            .to_owned()
                    })
                })
            })
            .or_else(|| Some("unknown".to_owned()))
    });
    let resolved = specify(root, &["preset", "resolve", "tasks-template"])
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    PresetState {
        installed,
        active_for_tasks_template: resolved.contains(&format!("presets/{ID}"))
            || resolved.contains(&format!("from: {ID} ")),
        bundled: version().to_owned(),
    }
}

fn refuse(code: &str, status: CommandStatus, message: String) -> CommandResult {
    CommandResult::new("preset", status, &message).with_code(code)
}

/// `speccoder preset add`: installs (or, when `update`, replaces) the
/// bundled preset in the Spec Kit project at `root` with `specify preset
/// add --dev <dir> --priority 10` — run only when `confirmed`; otherwise
/// the command and what it writes come back for a person to confirm.
#[must_use]
pub fn add(root: &Path, update: bool, confirmed: bool, state_dir: &Path) -> CommandResult {
    let opened = crate::format::open(root, None);
    let speckit = opened
        .as_ref()
        .is_ok_and(|o| o.adapter.kind() == speccoder_core::spec::FormatKind::Speckit);
    if !speckit {
        return refuse(
            "PRESET-NOT-SPECKIT",
            CommandStatus::UsageError,
            "presets are a Spec Kit mechanism: this is not a Spec Kit project".to_owned(),
        );
    }
    let project = opened
        .as_ref()
        .map(|o| PathBuf::from(&o.snapshot.root))
        .unwrap_or_else(|_| root.to_path_buf());
    match speckit_version() {
        Some(v) if v >= NEEDS => {}
        found => {
            return refuse(
                "PRESET-SPECKIT-MISSING",
                CommandStatus::ConfigError,
                format!(
                    "the preset needs Spec Kit's `specify` {}.{}.{} or newer{}",
                    NEEDS.0,
                    NEEDS.1,
                    NEEDS.2,
                    found.map_or_else(
                        || " (not installed)".to_owned(),
                        |(a, b, c)| format!(" (found {a}.{b}.{c})")
                    )
                ),
            );
        }
    }
    let dir = match extract(state_dir) {
        Ok(d) => d,
        Err(e) => return refuse("PRESET-FAILED", CommandStatus::Error, e),
    };
    let dir_text = dir.display().to_string();
    let before = state(&project);
    // Already this version and active: nothing to do, nothing run.
    if before.installed.as_deref() == Some(version()) && before.active_for_tasks_template {
        return CommandResult::new(
            "preset",
            CommandStatus::Ok,
            &format!(
                "the speccoder preset {} is already active for tasks-template",
                version()
            ),
        )
        .with_data(CommandData::Preset(Box::new(PresetReport {
            preset: ID.to_owned(),
            version: version().to_owned(),
            command: Vec::new(),
            output: String::new(),
            state: before,
        })));
    }
    let update = update || before.installed.is_some();
    let command: Vec<String> = if update {
        vec![
            "specify",
            "preset",
            "update",
            ID,
            "--dev",
            &dir_text,
            "--priority",
            "10",
        ]
    } else {
        vec![
            "specify",
            "preset",
            "add",
            "--dev",
            &dir_text,
            "--priority",
            "10",
        ]
    }
    .into_iter()
    .map(str::to_owned)
    .collect();
    let report = |output: String, state: PresetState| PresetReport {
        preset: ID.to_owned(),
        version: version().to_owned(),
        command: command.clone(),
        output,
        state,
    };
    // Spec Kit's scripts resolve a preset's templates with PyYAML.
    let python = super::python::warning_before_add(&|k| std::env::var(k).ok());
    let warn = |message: String| match &python {
        Some(w) => format!("{message}. Warning: {w}"),
        None => message,
    };
    let fix: Vec<String> = python
        .iter()
        .flat_map(|_| super::python::FIX.iter().map(|f| (*f).to_owned()))
        .collect();
    if !confirmed {
        let asked = CommandResult::new(
            "preset",
            CommandStatus::NeedsHuman,
            &warn(format!(
                "Run `{}`? It writes .specify/presets/{ID}/ and Spec Kit's preset registry; nothing in specs/ changes",
                command.join(" ")
            )),
        )
        .with_code("NEEDS-HUMAN")
        .with_next(vec![speccoder_core::next::NextStep {
            command: "speccoder preset add --yes".to_owned(),
            why: "confirm: Spec Kit installs the preset".to_owned(),
        }])
        .with_data(CommandData::Preset(Box::new(report(String::new(), before))));
        return if fix.is_empty() {
            asked
        } else {
            asked.with_fix(fix)
        };
    }
    let args: Vec<&str> = command.iter().skip(1).map(String::as_str).collect();
    let Some(out) = specify(&project, &args) else {
        return refuse(
            "PRESET-SPECKIT-MISSING",
            CommandStatus::ConfigError,
            "`specify` could not be run".to_owned(),
        );
    };
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let excerpt: String = output
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    if !out.status.success() {
        return refuse(
            "PRESET-FAILED",
            CommandStatus::Error,
            format!("`{}` failed: {excerpt}", command.join(" ")),
        )
        .with_data(CommandData::Preset(Box::new(report(excerpt, before))));
    }
    let after = state(&project);
    let added = CommandResult::new(
        "preset",
        CommandStatus::Ok,
        &warn(format!(
            "the speccoder preset {} is {} for tasks-template",
            version(),
            if after.active_for_tasks_template {
                "active"
            } else {
                "installed (not active)"
            }
        )),
    )
    .with_data(CommandData::Preset(Box::new(report(excerpt, after))));
    if fix.is_empty() {
        added
    } else {
        added.with_fix(fix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_is_idempotent_and_names_this_version() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let a = extract(d.path()).unwrap_or_else(|e| panic!("{e}"));
        let b = extract(d.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a, b);
        let manifest = std::fs::read_to_string(a.join("preset.yml")).unwrap_or_default();
        assert!(
            manifest.contains(&format!("version: \"{}\"", version())),
            "{manifest}"
        );
        assert!(a.join("templates/tasks-template.md").is_file());
        assert_eq!(parse_version("specify 1.2.3"), Some((1, 2, 3)));
    }
}
