//! Spec Kit's paths, rules and words: what the runtime says and protects
//! for a Spec Kit project, and nothing the judge decides.

use std::path::Path;

use crate::format::{Assess, Messages, Planning, Rules, Wording};

/// The marker directory of a Spec Kit project.
pub const MARKER: &str = ".specify";
/// Spec Kit's pointer to the current feature (per checkout, git-ignored).
pub const POINTER: &str = ".specify/feature.json";

/// Spec Kit's selection override, read by the caller of the parser.
pub const FEATURE_ENV: &str = "SPECIFY_FEATURE_DIRECTORY";

/// The constitution, relative to the project root.
pub const CONSTITUTION: &str = ".specify/memory/constitution.md";

/// Spec Kit's intent and planning rules.
pub static RULES: Rules = Rules {
    intent: &[".specify", ".specify/**", "specs/**", "**/.specify/**"],
    intent_reason: "it is Spec Kit intent (.specify/, specs/): the spec is not edited during implementation",
    authoring_path: is_authoring_path,
    command,
    env: &[],
    // The Python Spec Kit's scripts resolve preset templates with.
    found_env: super::python::command_env,
    authoring_refusal,
    judged,
};

/// A feature's judged files: its spec, its task list, the person's review
/// and the checklists.
pub(crate) fn judged(rel: &str) -> bool {
    matches!(rel, "spec.md" | "tasks.md" | "review.json")
        || rel == "checklists"
        || rel.starts_with("checklists/")
}

/// What a hosted planning skill writes: the feature's artifacts, the
/// feature selection and the constitution — Spec Kit's own outputs.
fn is_authoring_path(p: &str) -> bool {
    p.starts_with("specs/") || p == ".specify/feature.json" || p.starts_with(".specify/memory/")
}

fn authoring_refusal(p: &str) -> String {
    format!(
        "a planning skill writes Spec Kit artifacts (specs/, .specify/feature.json, .specify/memory/); `{p}` is not one"
    )
}

/// A planning skill runs Spec Kit's own scripts without asking.
fn command(argv: &[String], authoring: bool) -> Option<Result<(), String>> {
    (authoring && is_script(argv)).then_some(Ok(()))
}

/// One of Spec Kit's own scripts, run directly or by `bash`/`sh`.
fn is_script(argv: &[String]) -> bool {
    let script = |a: &str| {
        let a = a.strip_prefix("./").unwrap_or(a);
        // Spec Kit's scripts and those of installed extensions
        // (`.specify/extensions/<ext>/scripts/…`): the person installed them,
        // and the model can write neither.
        let ext_script = a
            .strip_prefix(".specify/extensions/")
            .is_some_and(|rest| rest.split('/').nth(1) == Some("scripts"));
        !a.contains("..")
            && (a.starts_with(".specify/scripts/bash/") && a.ends_with(".sh")
                || a.starts_with(".specify/scripts/powershell/") && a.ends_with(".ps1")
                || ext_script && (a.ends_with(".sh") || a.ends_with(".ps1")))
    };
    match argv {
        [first, ..] if script(first) => true,
        [shell, second, ..] if matches!(shell.as_str(), "bash" | "sh") => script(second),
        _ => false,
    }
}

/// What the model is told about Spec Kit.
pub static WORDING: Wording = Wording {
    product: "GitHub Spec Kit",
    name: "Spec Kit",
    unit: "feature",
    intent_paths: ".specify/, specs/",
    intent_code: "`.specify/`, `specs/`",
    documents: "spec.md, plan.md and tasks.md",
    authoring: "write Spec Kit artifacts (specs/, .specify/feature.json, .specify/memory/) and run Spec Kit's own scripts under .specify/scripts/",
    authoring_tool: "a Spec Kit script",
    story: "User story",
    constitution: "Project constitution",
    plan: "Feature plan",
    plan_short: "Plan",
    examples: "the acceptance examples of the task's user story",
    convergence_task: "A task in a Convergence phase",
    files: ["spec.md", "plan.md", "tasks.md"],
    assess: Assess {
        rules: "the constitution",
        targets: "functional requirement (FR-###), success criterion that needs buildable work (SC-###), user story acceptance scenario (USn/ACm), plan decision and constitution MUST principle",
        must: "a constitution MUST",
        sources: "spec, plan and tasks",
        critical: "constitution MUST violation, or missing/contradicts that blocks a P1 story",
        refs: "FR-003, US1/AC2, plan: storage, Constitution II",
    },
};

/// What a person is told about a Spec Kit project.
pub static MESSAGES: Messages = Messages {
    tasks_file: "tasks.md",
    make_tasks: "`speccoder spec tasks` writes it",
    not_a_project,
    no_project: "not a Spec Kit project (no .specify/ found)",
    no_project_in_worktree: "the worktree has no Spec Kit project",
    not_in_git: "the Spec Kit project is not inside a git repository",
    no_skills: "no skills installed (Spec Kit installs them with `specify init --here --integration claude`, under .claude/skills/)",
    checklists,
    unit: "feature",
    no_current: "",
    current_shared: "Spec Kit's scripts and the /speckit-* commands in your editor read it too",
    make_project: (
        "specify init --here --integration claude",
        "make it a Spec Kit project (the skills speccoder runs come with it)",
    ),
};

fn not_a_project(path: &Path) -> String {
    format!(
        "no Spec Kit project (.specify/) at or above {}; run `specify init --here --integration claude` first (speccoder runs Spec Kit's skills from .claude/skills/)",
        path.display()
    )
}

fn checklists(open: &str) -> String {
    format!(
        "unfinished checklists: {open}; Spec Kit treats an unchecked item as a stop — finish them, or run with --ignore-checklists"
    )
}

/// Spec Kit's planning skills.
pub static PLANNING: Planning = Planning {
    prefix: "speckit-",
    // Spec Kit's core skills, whose `before_<name>` / `after_<name>` hooks
    // speccoder runs (a hook's own skill gets none: no recursion).
    core: &[
        "specify",
        "clarify",
        "plan",
        "tasks",
        "analyze",
        "checklist",
        "constitution",
    ],
    refused,
    creates: &["specify"],
    current_env: Some(FEATURE_ENV),
    toward_tasks,
    revises: Some("clarify"),
    checks: Some("analyze"),
};

fn toward_tasks(has_plan: bool) -> &'static [&'static str] {
    if has_plan {
        &["tasks"]
    } else {
        &["plan", "tasks"]
    }
}

/// Skills speccoder does not host, and what to use instead.
fn refused(name: &str) -> Option<String> {
    match name {
        "speckit-implement" => Some(
            "speccoder implements tasks itself, with evidence: use `speccoder run`".to_owned(),
        ),
        "speckit-converge" => Some(
            "speccoder converges with a read-only assessment and a guarded append: use `speccoder converge`"
                .to_owned(),
        ),
        "speckit-taskstoissues" => Some(
            "it turns tasks into GitHub issues; speccoder goes the other way — `speccoder issue start N` makes a feature from an issue, and `speccoder pr` closes it".to_owned(),
        ),
        _ => None,
    }
}
