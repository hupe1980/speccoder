//! Kiro's paths, rules and words: intent is `.kiro/specs/`; the specs are
//! written in Kiro or with cc-sdd, never by a skill speccoder hosts.

use std::path::Path;

use crate::format::{Assess, Messages, Planning, Rules, Wording};

/// Kiro's intent and planning rules.
pub static RULES: Rules = Rules {
    intent: &[".kiro/specs", ".kiro/specs/**"],
    intent_reason: "it is Kiro intent (.kiro/specs/): the spec is not edited during implementation",
    authoring_path: crate::format::writes_nothing,
    command: |_, _| None,
    env: &[],
    found_env: crate::format::no_found_env,
    authoring_refusal,
    judged,
    plan: "design.md",
};

/// A spec's judged files: its requirements and its task list.
fn judged(rel: &str) -> bool {
    matches!(rel, "requirements.md" | "bugfix.md" | "tasks.md")
}

fn authoring_refusal(p: &str) -> String {
    format!("Kiro specs are written in Kiro or with cc-sdd; `{p}` is not written here")
}

/// What the model is told about Kiro.
pub static WORDING: Wording = Wording {
    product: "Kiro",
    name: "Kiro",
    unit: "spec",
    intent_paths: ".kiro/specs/",
    intent_code: "`.kiro/specs/`",
    documents: "requirements.md, design.md and tasks.md",
    authoring: "write nothing: Kiro specs are written in Kiro or with cc-sdd",
    authoring_tool: "Kiro",
    story: "Requirements",
    constitution: "Steering",
    plan: "Design",
    plan_short: "Design",
    examples: "the acceptance criteria this task cites (name the criterion, e.g. `1.2`, each test covers in a comment above it)",
    convergence_task: "A task in a Convergence group",
    files: ["requirements.md", "design.md", "tasks.md"],
    assess: Assess {
        rules: "the steering",
        targets: "requirement and each of its acceptance criteria, design decision and steering constraint",
        must: "a SHALL of the acceptance criteria",
        sources: "requirements, design and tasks",
        critical: "a SHALL not met",
        refs: "1.2, Requirement 3, design: Data Models",
    },
};

/// What a person is told about a Kiro project.
pub static MESSAGES: Messages = Messages {
    tasks_file: "tasks.md",
    make_tasks: "write it in Kiro (or with cc-sdd's /kiro:spec-tasks)",
    not_a_project,
    no_project: "not a Kiro project (no .kiro/specs/ found)",
    no_project_in_worktree: "the worktree has no Kiro spec",
    not_in_git: "the Kiro project is not inside a git repository",
    no_skills: "Kiro specs are written in Kiro or with cc-sdd; speccoder runs them (`speccoder run`)",
    checklists,
    unit: "spec",
    no_current: "Kiro has no current spec: name one with --feature",
    current_shared: "",
    make_project: ("kiro", "write a spec in Kiro"),
};

fn not_a_project(path: &Path) -> String {
    format!(
        "no Kiro spec (.kiro/specs/<name>/tasks.md) at or above {}",
        path.display()
    )
}

fn checklists(open: &str) -> String {
    format!("unfinished checklists: {open}")
}

/// Kiro hosts no planning skills here: its specs come from Kiro or cc-sdd.
pub static PLANNING: Planning = Planning {
    prefix: "kiro-",
    core: &[],
    refused,
    creates: &[],
    current_env: None,
    toward_tasks,
    revises: None,
    checks: None,
    explores: None,
    explore_paths: crate::format::writes_nothing,
    explore_commands: &[],
};

fn toward_tasks(_has_plan: bool) -> &'static [&'static str] {
    &[]
}

fn refused(_name: &str) -> Option<String> {
    Some("Kiro specs are written in Kiro or with cc-sdd; speccoder runs them".to_owned())
}
