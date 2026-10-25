//! OpenSpec's paths, rules and words (D-060): intent is all of `openspec/`;
//! planning writes active changes only; archiving is the person's.

use std::path::Path;

use crate::format::{Assess, Messages, Planning, Rules, Wording};

/// OpenSpec's intent and planning rules.
pub static RULES: Rules = Rules {
    intent: &["openspec", "openspec/**"],
    intent_reason: "it is OpenSpec intent (openspec/): the spec is not edited during implementation",
    authoring_path: is_authoring_path,
    command,
    // OpenSpec's CLI asks no questions of a sandboxed child and phones home
    // unless told not to.
    env: &[
        ("OPENSPEC_TELEMETRY", "0"),
        ("DO_NOT_TRACK", "1"),
        ("OPENSPEC_NO_UPDATE_CHECK", "1"),
    ],
    authoring_refusal,
};

/// What a hosted planning skill writes: active changes, never the archive.
fn is_authoring_path(p: &str) -> bool {
    p.starts_with("openspec/changes/") && !p.starts_with("openspec/changes/archive/")
}

fn authoring_refusal(p: &str) -> String {
    format!(
        "a planning skill writes OpenSpec changes (openspec/changes/, not its archive); `{p}` is not one"
    )
}

/// The OpenSpec subcommand of `argv`: `openspec …`, `npx openspec …` or
/// `npx @fission-ai/openspec[@version] …`.
fn subcommand(argv: &[String]) -> Option<&[String]> {
    let program = argv.first()?;
    let name = program.rsplit('/').next().unwrap_or(program);
    if name == "openspec" {
        return Some(&argv[1..]);
    }
    if name == "npx" {
        let rest: Vec<&String> = argv[1..].iter().filter(|a| !a.starts_with('-')).collect();
        let package = rest.first()?;
        let bare = package
            .strip_prefix("@fission-ai/")
            .unwrap_or(package)
            .split('@')
            .next()
            .unwrap_or_default();
        if bare == "openspec" {
            let at = argv.iter().position(|a| a == *package)?;
            return Some(&argv[at + 1..]);
        }
    }
    None
}

/// OpenSpec's CLI (contracts/policy.md): reading is free, creating a change
/// is planning, everything that rewrites intent or the installation is
/// refused — archiving above all, which is the person's.
fn command(argv: &[String], authoring: bool) -> Option<Result<(), String>> {
    let sub = subcommand(argv)?;
    let first = sub.first().map_or("", String::as_str);
    match first {
        "" | "--version" | "-V" | "--help" | "-h" | "list" | "show" | "status"
        | "instructions" | "validate" | "schemas" | "templates" | "context" | "view" => {
            Some(Ok(()))
        }
        "new" if authoring => Some(Ok(())),
        "new" => Some(Err(
            "creating an OpenSpec change is planning, not implementation".to_owned(),
        )),
        "archive" => Some(Err(
            "archiving rewrites the main specs and is the person's: they run `speccoder spec archive`"
                .to_owned(),
        )),
        "sync" | "init" | "update" | "config" | "store" | "schema" => Some(Err(format!(
            "`openspec {first}` changes OpenSpec's intent or installation; speccoder never runs it"
        ))),
        _ => None,
    }
}

/// What the model is told about OpenSpec.
pub static WORDING: Wording = Wording {
    product: "OpenSpec",
    name: "OpenSpec",
    unit: "change",
    intent_paths: "openspec/",
    intent_code: "`openspec/`",
    documents: "proposal, design, spec deltas and tasks.md",
    authoring: "write OpenSpec changes (openspec/changes/, never its archive) and run OpenSpec's read commands and `openspec new change`",
    authoring_tool: "an OpenSpec command",
    story: "Spec deltas",
    constitution: "Project context",
    plan: "Change proposal and design",
    plan_short: "Proposal and design",
    examples: "the scenarios of the change's spec deltas that this task concerns (name the capability, requirement and scenario each test covers in a comment above it)",
    convergence_task: "A task in a Convergence group",
    files: ["spec deltas", "proposal and design", "tasks.md"],
    assess: Assess {
        rules: "the project context",
        targets: "ADDED or MODIFIED requirement and each of its scenarios, design decision and project context constraint",
        must: "a SHALL or MUST of the deltas",
        sources: "proposal, design, deltas and tasks",
        critical: "a SHALL or MUST not met, or missing/contradicts on a MODIFIED requirement",
        refs: "greeting: Farewell / Farewell with name, design: argv parsing",
    },
};

/// What a person is told about an OpenSpec project.
pub static MESSAGES: Messages = Messages {
    tasks_file: "tasks.md",
    make_tasks: "create it with `speccoder spec propose`",
    not_a_project,
    no_project: "not an OpenSpec project (no openspec/ found)",
    no_project_in_worktree: "the worktree has no OpenSpec project",
    not_in_git: "the OpenSpec project is not inside a git repository",
    no_skills: "no skills installed (OpenSpec installs them with `openspec init`, under .claude/skills/)",
    checklists,
    unit: "change",
    no_current: "OpenSpec has no current change: name one with --change",
    current_shared: "",
    make_project: ("openspec init", "make it an OpenSpec project"),
};

fn not_a_project(path: &Path) -> String {
    format!(
        "no OpenSpec project (openspec/) at or above {}; run `openspec init` first",
        path.display()
    )
}

fn checklists(open: &str) -> String {
    format!("unfinished checklists: {open}")
}

/// OpenSpec's planning skills.
pub static PLANNING: Planning = Planning {
    prefix: "openspec-",
    core: &[],
    refused,
    creates: &[],
    current_env: None,
    toward_tasks,
};

fn toward_tasks(_has_plan: bool) -> &'static [&'static str] {
    &["propose"]
}

/// Skills speccoder does not host, and what to use instead.
fn refused(name: &str) -> Option<String> {
    match name {
        "openspec-apply-change" => {
            Some("speccoder is the apply step, with evidence: use `speccoder run`".to_owned())
        }
        "openspec-sync-specs" | "openspec-archive-change" => Some(
            "merging a change into the main specs is the person's: use `speccoder spec archive`"
                .to_owned(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn openspec_commands_by_subcommand() {
        for (line, implementing, planning) in [
            ("openspec list --json", Some(true), Some(true)),
            (
                "openspec validate x --strict --json",
                Some(true),
                Some(true),
            ),
            (
                "npx @fission-ai/openspec@1.14.3 show x",
                Some(true),
                Some(true),
            ),
            ("npx -y openspec status", Some(true), Some(true)),
            ("openspec new change add-x", Some(false), Some(true)),
            ("openspec archive x --yes", Some(false), Some(false)),
            ("/usr/local/bin/openspec sync", Some(false), Some(false)),
            ("openspec init", Some(false), Some(false)),
            ("openspec frobnicate", None, None),
            ("cargo test", None, None),
        ] {
            let a = argv(line);
            assert_eq!(
                command(&a, false).map(|r| r.is_ok()),
                implementing,
                "{line}"
            );
            assert_eq!(command(&a, true).map(|r| r.is_ok()), planning, "{line}");
        }
        let why = command(&argv("openspec archive x"), false)
            .unwrap()
            .unwrap_err();
        assert!(why.contains("speccoder spec archive"), "{why}");
        assert!(is_authoring_path("openspec/changes/add-x/proposal.md"));
        assert!(!is_authoring_path(
            "openspec/changes/archive/2026-x/tasks.md"
        ));
        assert!(!is_authoring_path("openspec/specs/greeting/spec.md"));
    }
}
