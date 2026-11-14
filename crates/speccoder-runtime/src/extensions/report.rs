//! What `speccoder extensions`, Settings › Extensions, Needs You and
//! `doctor` show: each extension with what it contributes in words, its
//! trust, and — when it changed — what changed, widened first.

use std::path::Path;

use speccoder_core::extensions::{Extension, ExtensionsReport};
use speccoder_core::hooks::TrustState;

use super::{Loaded, trust};

/// `e` as shown, against the trust records in `dir`.
#[must_use]
pub fn entry(e: &Loaded, dir: &Path) -> Extension {
    let state = if e.usable() {
        trust::state(dir, e)
    } else {
        TrustState::Untrusted
    };
    Extension {
        name: e.name.clone(),
        source: e.source,
        version: e.version.clone(),
        description: e.description.clone(),
        folder: e.rel.clone(),
        origin: e.origin.clone(),
        digest: e.digest.clone(),
        files: e.files.clone(),
        contributions: e.contributions(),
        trust: state,
        changes: (state == TrustState::Changed)
            .then(|| trust::changes(dir, e))
            .flatten(),
        trust_flag: format!("--trust-extension {}={}", e.name, e.digest),
        problems: e.problems.clone(),
    }
}

/// Every extension of the project at `root`. A trust record outlives an
/// extension that is not here now (another branch has it); only `remove`
/// forgets one.
#[must_use]
pub fn list(root: &Path, state: &Path) -> ExtensionsReport {
    let dir = crate::hooks::dir(state, root);
    let all = super::load(root);
    ExtensionsReport {
        extensions: all.iter().map(|e| entry(e, &dir)).collect(),
        ..ExtensionsReport::default()
    }
}

/// Trusts `name` exactly as shown to the person (`digest`), through `via`.
///
/// # Errors
///
/// No such extension, it cannot be loaded, or it changed since it was shown.
pub fn trust_shown(
    root: &Path,
    state: &Path,
    name: &str,
    digest: &str,
    via: &str,
) -> Result<(), String> {
    let all = super::load(root);
    let Some(e) = all.iter().find(|e| e.name == name) else {
        return Err(format!("there is no extension `{name}`"));
    };
    if !e.usable() {
        return Err(format!(
            "{name} cannot be loaded: {}",
            e.problems.join("; ")
        ));
    }
    if e.digest != digest {
        return Err(format!(
            "{name} changed since it was shown ({} now): review it again",
            e.digest
        ));
    }
    trust::record(
        &crate::hooks::dir(state, root),
        e,
        &crate::approvals::who_in(root),
        via,
    );
    Ok(())
}

/// The extensions waiting for trust, by name: what a run says once.
#[must_use]
pub fn waiting(root: &Path, state: &Path) -> Vec<String> {
    let dir = crate::hooks::dir(state, root);
    super::load(root)
        .iter()
        .filter(|e| e.usable() && trust::state(&dir, e) != TrustState::Trusted)
        .map(|e| e.name.clone())
        .collect()
}

/// What a run says once about extensions that wait for trust.
#[must_use]
pub fn run_notice(root: &Path, state: &Path) -> Option<String> {
    let names = waiting(root, state);
    (!names.is_empty()).then(|| {
        format!(
            "{} {} for your trust: nothing of {} runs (speccoder extensions trust {})",
            names.join(", "),
            if names.len() == 1 { "waits" } else { "wait" },
            if names.len() == 1 { "it" } else { "them" },
            names[0]
        )
    })
}

/// `doctor`'s lines: an extension that cannot be loaded, or waits for trust.
#[must_use]
pub fn doctor_lines(root: &Path, state: &Path) -> Vec<(bool, String)> {
    let dir = crate::hooks::dir(state, root);
    super::load(root)
        .iter()
        .filter_map(|e| {
            if !e.usable() {
                return Some((
                    false,
                    format!(
                        "extension {} cannot be loaded: {}",
                        e.name,
                        e.problems.join("; ")
                    ),
                ));
            }
            match trust::state(&dir, e) {
                TrustState::Trusted => None,
                TrustState::Untrusted => Some((
                    false,
                    format!(
                        "extension {} waits for your trust: speccoder extensions trust {}",
                        e.name, e.name
                    ),
                )),
                TrustState::Changed => Some((
                    false,
                    format!(
                        "extension {} changed since you trusted it: speccoder extensions trust {}",
                        e.name, e.name
                    ),
                )),
            }
        })
        .collect()
}

// ── commands ───────────────────────────────────────────────────────────

use speccoder_core::{CommandData, CommandResult, CommandStatus};

fn result(message: &str, report: ExtensionsReport, status: CommandStatus) -> CommandResult {
    CommandResult::new("extensions", status, message)
        .with_data(CommandData::Extensions(Box::new(report)))
}

fn summary(r: &ExtensionsReport) -> String {
    let waiting = r
        .extensions
        .iter()
        .filter(|e| e.problems.is_empty() && e.trust != TrustState::Trusted)
        .count();
    let broken = r
        .extensions
        .iter()
        .filter(|e| !e.problems.is_empty())
        .count();
    let mut s = match (r.extensions.len(), waiting) {
        (0, _) => {
            return "no extensions: `speccoder extensions add <folder | URL@commit>` (docs: extensions)"
                .to_owned();
        }
        (1, 0) => "1 extension, trusted".to_owned(),
        (n, 0) => format!("{n} extensions, all trusted"),
        (n, w) => format!(
            "{n} extension{} — {w} wait{} for your trust",
            if n == 1 { "" } else { "s" },
            if w == 1 { "s" } else { "" }
        ),
    };
    if broken > 0 {
        s.push_str(&format!(", {broken} cannot be loaded"));
    }
    s
}

/// `speccoder extensions`.
#[must_use]
pub fn list_cmd(root: &Path, state: &Path) -> CommandResult {
    let report = list(root, state);
    let message = summary(&report);
    result(&message, report, CommandStatus::Ok)
}

/// `speccoder extensions add <folder | URL@commit>`.
#[must_use]
pub fn add_cmd(root: &Path, state: &Path, source: &str) -> CommandResult {
    match super::add::add(root, source) {
        Ok(e) => {
            let mut report = list(root, state);
            report.added = Some(e.name.clone());
            result(
                &format!(
                    "added {} to {}: commit it, then trust it (speccoder extensions trust {}) — nothing of it runs until then",
                    e.name, e.rel, e.name
                ),
                report,
                CommandStatus::Ok,
            )
        }
        Err(err) => {
            let status = if err.contains("cannot be loaded") || err.contains("plugin.json") {
                CommandStatus::ConfigError
            } else {
                CommandStatus::UsageError
            };
            let r = result(&err, list(root, state), status);
            if status == CommandStatus::ConfigError {
                r.with_code("EXTENSION-UNLOADABLE")
            } else {
                r
            }
        }
    }
}

/// `speccoder extensions remove <name>`.
#[must_use]
pub fn remove_cmd(root: &Path, state: &Path, name: &str) -> CommandResult {
    match super::add::remove(root, state, name) {
        Ok(()) => {
            let mut report = list(root, state);
            report.removed = Some(name.to_owned());
            result(
                &format!("removed {name}: commit the change to remove it for everyone"),
                report,
                CommandStatus::Ok,
            )
        }
        Err(err) => result(&err, list(root, state), CommandStatus::UsageError),
    }
}

/// `speccoder extensions trust <name>`: trusted when `confirm` agrees,
/// having seen the entry. Its observers hear what happens from now on.
pub fn trust_cmd(
    root: &Path,
    state: &Path,
    name: &str,
    via: &str,
    confirm: &mut dyn FnMut(&Extension) -> bool,
) -> CommandResult {
    let dir = crate::hooks::dir(state, root);
    let all = super::load(root);
    let Some(e) = all.iter().find(|e| e.name == name) else {
        return result(
            &format!("there is no extension `{name}`"),
            list(root, state),
            CommandStatus::NotFound,
        );
    };
    let shown = entry(e, &dir);
    if !e.usable() {
        return result(
            &format!("{name} cannot be loaded: {}", e.problems.join("; ")),
            list(root, state),
            CommandStatus::ConfigError,
        )
        .with_code("EXTENSION-UNLOADABLE");
    }
    if shown.trust == TrustState::Trusted {
        return result(
            &format!("{name} is trusted as it is"),
            list(root, state),
            CommandStatus::Ok,
        );
    }
    if !confirm(&shown) {
        return result(
            &format!("{name} not trusted: nothing of it runs"),
            list(root, state),
            CommandStatus::NeedsHuman,
        )
        .with_code("EXTENSION-UNTRUSTED");
    }
    trust::record(&dir, e, &crate::approvals::who_in(root), via);
    after_trust(root, &dir, e);
    let mut report = list(root, state);
    report.trusted = vec![name.to_owned()];
    result(
        &format!("trusted {name}: it runs from now on"),
        report,
        CommandStatus::Ok,
    )
}

/// A newly trusted extension's observers hear what happens from now on;
/// one trusted again after a change goes on where it was.
fn after_trust(root: &Path, dir: &Path, e: &Loaded) {
    for h in e.hooks.iter().filter(|h| h.at().is_none()) {
        if !crate::hooks::deliver::has_cursor(dir, &h.name) {
            crate::hooks::deliver::start_here(dir, root, &h.name);
        }
    }
}

/// Trusts from an interface that showed `digest` (the web's sheet).
///
/// # Errors
///
/// As [`trust_shown`].
pub fn trust_from_sheet(root: &Path, state: &Path, name: &str, digest: &str) -> Result<(), String> {
    trust_shown(root, state, name, digest, "web")?;
    let dir = crate::hooks::dir(state, root);
    if let Some(e) = super::load(root).iter().find(|e| e.name == name) {
        after_trust(root, &dir, e);
    }
    Ok(())
}

/// `--trust-extension name=sha256:…` for this command.
///
/// # Errors
///
/// A flag is malformed, names no extension, or a digest that is not current.
pub fn apply_flags(root: &Path, state: &Path, flags: &[String]) -> Result<(), Box<CommandResult>> {
    if flags.is_empty() {
        return Ok(());
    }
    let parsed = flags
        .iter()
        .map(|f| trust::parse_flag(f))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            Box::new(
                CommandResult::new("extensions", CommandStatus::UsageError, &e).with_code("USAGE"),
            )
        })?;
    let dir = crate::hooks::dir(state, root);
    let all = super::load(root);
    trust::apply_flags(&dir, &all, &parsed, &crate::approvals::who_in(root)).map_err(|e| {
        Box::new(
            CommandResult::new("extensions", CommandStatus::NeedsHuman, &e)
                .with_code("EXTENSION-UNTRUSTED"),
        )
    })?;
    for (name, _) in &parsed {
        if let Some(e) = all.iter().find(|e| &e.name == name) {
            after_trust(root, &dir, e);
        }
    }
    Ok(())
}
