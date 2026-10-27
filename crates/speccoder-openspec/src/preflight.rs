//! What `openspec archive` would refuse, found before any model call:
//! `validate --strict` passes a MODIFIED requirement that does
//! not exist, and only `archive` fails on it — after the work is done.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use speccoder_core::snapshot::{Diagnostic, Severity};

use crate::Change;
use crate::delta::{Operation, normalise};
use crate::discover::{Config, DIR, SCHEMA};

/// Problems of the project as a whole.
pub fn project(root: &Path, config: &Config, diags: &mut Vec<Diagnostic>) {
    let dir = root.join(DIR);
    if !config.present && (dir.join("project.md").is_file() || dir.join("AGENTS.md").is_file()) {
        diags.push(
            Diagnostic::new(
                Severity::Error,
                "legacy_root",
                "openspec/ has the pre-1.0 layout (project.md, no config.yaml)",
            )
            .at("openspec")
            .hint("run `openspec update` to migrate the project"),
        );
    }
    if config.store {
        diags.push(
            Diagnostic::new(
                Severity::Error,
                "store_root",
                "openspec/config.yaml uses a shared store, which speccoder does not run",
            )
            .at("openspec/config.yaml")
            .hint("keep the change in the repository's own openspec/ directory"),
        );
    }
}

/// Problems of one change, given the main specs' requirement names.
pub fn change(c: &Change, main: &BTreeMap<String, BTreeSet<String>>, diags: &mut Vec<Diagnostic>) {
    if c.schema != SCHEMA {
        diags.push(
            Diagnostic::new(
                Severity::Error,
                "unsupported_schema",
                format!(
                    "change {} uses schema `{}`; speccoder runs `{SCHEMA}`",
                    c.id, c.schema
                ),
            )
            .at(&c.dir)
            .hint("use the default schema for changes speccoder implements"),
        );
    }
    if c.deltas.is_empty() {
        diags.push(
            Diagnostic::new(
                Severity::Error,
                "no_deltas",
                format!(
                    "change {} has no spec deltas: nothing to derive tests from",
                    c.id
                ),
            )
            .at(&c.dir)
            .hint("add specs/<capability>/spec.md with ADDED or MODIFIED requirements"),
        );
    }
    for d in &c.deltas {
        let existing = main.get(&d.capability);
        let exists = |name: &str| existing.is_some_and(|e| e.contains(&normalise(name)));
        let renamed_from: BTreeSet<String> = d.renames.iter().map(|r| normalise(&r.from)).collect();
        let renamed_to: BTreeSet<String> = d.renames.iter().map(|r| normalise(&r.to)).collect();
        let at = |line: u32| (d.path.clone(), line);
        let mut push =
            |severity, code: &str, (path, line): (String, u32), message: String, hint: &str| {
                diags.push(
                    Diagnostic::new(severity, code, message)
                        .at(path)
                        .line(line)
                        .hint(hint),
                );
            };
        for r in &d.requirements {
            match r.operation {
                Operation::Modified | Operation::Removed => {
                    let name = normalise(&r.name);
                    if renamed_from.contains(&name) {
                        push(
                            Severity::Error,
                            "renamed_conflict",
                            at(r.line),
                            format!(
                                "{} {} `{}`, which this change also renames",
                                d.capability,
                                r.operation.name().to_uppercase(),
                                r.name
                            ),
                            "refer to the requirement by its new name",
                        );
                    } else if !exists(&r.name) && !renamed_to.contains(&name) {
                        push(
                            Severity::Error,
                            "missing_target",
                            at(r.line),
                            format!(
                                "{} {} `{}`: no such requirement in openspec/specs/{}/spec.md; archive would refuse this delta",
                                d.capability,
                                r.operation.name().to_uppercase(),
                                r.name,
                                d.capability
                            ),
                            "use the requirement's exact name, or ADD it instead",
                        );
                    }
                }
                Operation::Added | Operation::Renamed => {}
            }
            if matches!(r.operation, Operation::Added | Operation::Modified)
                && r.scenarios.is_empty()
            {
                push(
                    Severity::Error,
                    "missing_scenario",
                    at(r.line),
                    format!("requirement `{}` has no `#### Scenario:`", r.name),
                    "add at least one scenario: it is what the tests are derived from",
                );
            }
            if r.operation == Operation::Removed && !(r.reason && r.migration) {
                push(
                    Severity::Warning,
                    "removed_without_reason",
                    at(r.line),
                    format!("REMOVED `{}` has no **Reason** or **Migration**", r.name),
                    "say why it goes and what replaces it",
                );
            }
        }
        for n in &d.renames {
            if !exists(&n.from) {
                push(
                    Severity::Error,
                    "missing_target",
                    at(n.line),
                    format!(
                        "{} RENAMED `{}`: no such requirement in openspec/specs/{}/spec.md; archive would refuse this delta",
                        d.capability, n.from, d.capability
                    ),
                    "use the requirement's exact current name after FROM:",
                );
            }
        }
        if existing.is_none() && !d.purpose {
            push(
                Severity::Warning,
                "purpose_missing",
                (d.path.clone(), 1),
                format!(
                    "new capability `{}` has no `## Purpose`; archive will write a TBD placeholder",
                    d.capability
                ),
                "add a `## Purpose` section to the delta",
            );
        }
    }
}
