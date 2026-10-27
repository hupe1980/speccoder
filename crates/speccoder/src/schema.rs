//! The JSON Schemas of everything speccoder prints, generated from
//! the very types that print it: the result envelope, one schema per
//! payload (`payload` in the envelope names it) and the event line.
//! Published under `site/static/schemas/v2/`; a test keeps them in step.

use schemars::{JsonSchema, generate::SchemaSettings};
use serde_json::Value;
use speccoder_core as core;

/// Where the schemas are published.
pub const BASE: &str = "https://hupe1980.github.io/speccoder/schemas/v2/";

fn generate<T: JsonSchema>(name: &str, title: &str) -> Value {
    let generator = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    let mut schema =
        serde_json::to_value(generator.into_root_schema_for::<T>()).unwrap_or_default();
    if let Some(o) = schema.as_object_mut() {
        o.insert(
            "$id".to_owned(),
            Value::String(format!("{BASE}{name}.json")),
        );
        o.insert("title".to_owned(), Value::String(title.to_owned()));
    }
    schema
}

/// Every schema: (name, what it describes).
#[must_use]
pub fn names() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        (
            "result",
            "the result envelope: what --json prints and the result event holds",
        ),
        ("event", "one line of --events"),
    ];
    // The public projections of domain events, one per name the event
    // registry marks as a domain event.
    v.extend(
        core::event::REGISTRY
            .iter()
            .filter(|n| n.class == core::event::Class::Domain)
            .map(|n| {
                (
                    projection(n.name),
                    "a domain event's public projection: ids, digests and counts",
                )
            }),
    );
    v.extend(core::PAYLOADS.iter().map(|p| (*p, describe(p))));
    v
}

/// The schema name of a domain event's public projection.
fn projection(name: &str) -> &'static str {
    match name {
        "bug.recorded" => "events/bug.recorded",
        "bug.confirmed" => "events/bug.confirmed",
        "bug.confirm_rejected" => "events/bug.confirm_rejected",
        "bug.outcome" => "events/bug.outcome",
        "oracle.frozen" => "events/oracle.frozen",
        "oracle.lapsed" => "events/oracle.lapsed",
        "collection.checked" => "events/collection.checked",
        "project.created" => "events/project.created",
        "project.undone" => "events/project.undone",
        _ => "",
    }
}

const fn describe(payload: &str) -> &'static str {
    match payload.as_bytes() {
        b"inspect" => "inspect: the project snapshot",
        b"verify" => "verify: the checks' results",
        b"worktree" => "worktree: the feature's worktree",
        b"propose" => "propose: what the model proposes for a task",
        b"replay" => "replay: the replayed run",
        b"run" => "run --task: one task's attempt and verdict",
        b"eval" => "eval: the evaluation",
        b"feature" => "run: a feature run's attempts",
        b"converge" => "converge: the convergence round",
        b"report" => "report: every task and its evidence",
        b"approvals" => "approvals: calls waiting and decisions made",
        b"control" => "approve, reject, accept, skip, nudge: the decision",
        b"cost" => "cost: tokens and money per task",
        b"pr" => "pr: the draft pull request",
        b"bench" => "bench import: the imported cases",
        b"init" => "init: the configuration written",
        b"skills" => "spec: the format's planning skills",
        b"skill" => "spec SKILL: the hosted skill's run",
        b"checklists" => "run: the unfinished checklists that stopped it",
        b"archive" => "spec archive: the OpenSpec archive",
        b"in-place" => "an in-place run's refusal or stop: the paths involved",
        b"attest" => "attest: each task's attestation",
        b"explain" => "explain: code entries",
        b"schema" => "schema: the published schemas",
        b"doctor" => "doctor: every check of the machine and the project",
        b"features" => "feature: the features and which one is current",
        b"ui" => "ui: where the web interface is served",
        b"config" => "config: every key of speccoder.toml, its value and its default",
        b"issues" => "issue: the repository's open GitHub issues",
        b"inbox" => "inbox: every decision waiting for a person, across features",
        b"review" => "review: the spec's requirements and the person's decision on each",
        b"auth" => "auth: who speccoder is on a GitHub host, and where its token comes from",
        b"actions" => "actions: what a person can do now, and the feature's journey",
        b"bug" => "bug: a bug, its triad, reproductions, confirmation and how far it got",
        b"preset" => "preset add: the speccoder Spec Kit preset, installed or offered",
        b"new_project" => "new: a new project's plan, its steps, its commit and what Undo would do",
        b"sample" => "sample: the bundled sample's copy and each recorded run, replayed strictly",
        b"similar" => {
            "feature similar: the requirements close to a description, and the terms they share"
        }
        b"rung_suggestion" => {
            "suggest: the rung a description suggests and the cue that decided it"
        }
        b"planning_chain" => {
            "spec SKILL --through: the skills plan through ran, where it stopped, and the unit's readiness"
        }
        b"policy_coverage" => "config coverage: what a prefix entry would newly allow",
        b"comments" => "review comments: the comment tray and the last revision turn",
        b"branch_verify" => {
            "attest verify --branch, attest badge: every tick of a branch and what its evidence comes to"
        }
        b"evidence_tour" => "attest tour, /api/tour: one task's evidence, step by step",
        _ => "",
    }
}

/// Every published schema: its name, the name its root type has in the
/// web app's generated types, and the type that prints it. One table, so
/// the schemas and the names generated from them cannot diverge.
macro_rules! published {
    ($($name:literal, $ts:literal => $ty:ty,)*) => {
        /// The schema `name`, if speccoder publishes one by that name.
        #[must_use]
        pub fn get(name: &str) -> Option<Value> {
            if name == PREDICATE_NAME {
                return Some(predicate());
            }
            let title = names().into_iter().find(|(n, _)| *n == name)?.1;
            Some(match name {
                $($name => generate::<$ty>(name, title),)*
                _ => return None,
            })
        }

        /// Each schema's name and its root type's name in the web app's
        /// generated types (`result` → `Envelope`).
        #[cfg(test)]
        pub const ROOTS: &[(&str, &str)] = &[$(($name, $ts),)*];
    };
}

published! {
    "result", "Envelope" => core::CommandResult,
    "event", "EventLine" => crate::events::Line,
    "inspect", "ProjectSnapshot" => core::snapshot::ProjectSnapshot,
    "verify", "VerifyReport" => core::verify::VerifyReport,
    "worktree", "WorktreeReport" => core::WorktreeReport,
    "propose", "ProposeReport" => core::ProposeReport,
    "replay", "ReplayReport" => core::ReplayReport,
    "run", "RunReport" => core::RunReport,
    "eval", "EvalReport" => core::EvalReport,
    "feature", "FeatureReport" => core::FeatureReport,
    "converge", "ConvergeReport" => core::ConvergeReport,
    "report", "StatusReport" => core::StatusReport,
    "approvals", "ApprovalsReport" => core::ApprovalsReport,
    "control", "ControlReport" => core::ControlReport,
    "cost", "CostReport" => core::CostReport,
    "pr", "PrReport" => core::PrReport,
    "bench", "BenchReport" => core::BenchReport,
    "init", "InitReport" => core::InitReport,
    "skills", "SkillsReport" => core::SkillsReport,
    "skill", "SkillRunReport" => core::SkillRunReport,
    "checklists", "Checklists" => Vec<core::ChecklistStatus>,
    "archive", "ArchiveReport" => core::ArchiveReport,
    "in-place", "InPlaceReport" => core::InPlaceReport,
    "attest", "AttestReport" => core::AttestReport,
    "explain", "ExplainReport" => core::ExplainReport,
    "schema", "SchemaReport" => core::SchemaReport,
    "doctor", "DoctorReport" => core::DoctorReport,
    "features", "FeaturesReport" => core::FeaturesReport,
    "ui", "UiReport" => core::UiReport,
    "config", "ConfigReport" => core::ConfigReport,
    "issues", "IssuesReport" => core::IssuesReport,
    "inbox", "InboxReport" => core::InboxReport,
    "review", "ReviewReport" => core::ReviewReport,
    "auth", "AuthReport" => core::AuthReport,
    "actions", "ActionsReport" => core::ActionsReport,
    "policy_coverage", "PolicyCoverage" => core::PolicyCoverage,
    "branch_verify", "BranchReport" => core::verify_branch::BranchReport,
    "evidence_tour", "EvidenceTour" => core::tour::EvidenceTour,
    "bug", "BugReport" => core::bug::BugReport,
    "events/bug.recorded", "PublicBugRecorded" => crate::events::PublicBugRecorded,
    "events/bug.confirmed", "PublicBugConfirmed" => crate::events::PublicBugConfirmed,
    "events/bug.confirm_rejected", "PublicBugConfirmRejected" => crate::events::PublicBugConfirmRejected,
    "events/bug.outcome", "PublicBugOutcome" => crate::events::PublicBugOutcome,
    "events/oracle.frozen", "PublicOracleFrozen" => crate::events::PublicOracleFrozen,
    "events/oracle.lapsed", "PublicOracleLapsed" => crate::events::PublicOracleLapsed,
    "events/collection.checked", "PublicCollectionChecked" => crate::events::PublicCollectionChecked,
    "comments", "CommentsView" => core::oracle::CommentsView,
    "preset", "PresetReport" => core::PresetReport,
    "events/project.created", "PublicProjectCreated" => crate::events::PublicProjectCreated,
    "events/project.undone", "PublicProjectUndone" => crate::events::PublicProjectUndone,
    "new_project", "NewProjectReport" => core::new_project::NewProjectReport,
    "sample", "SampleReport" => core::SampleReport,
    "similar", "SimilarReport" => core::similar::SimilarReport,
    "rung_suggestion", "RungSuggestion" => core::ladder::RungSuggestion,
    "planning_chain", "PlanningChainReport" => core::PlanningChainReport,
}

/// The predicate schema's name (`speccoder schema task-verification-v0`).
pub const PREDICATE_NAME: &str = "task-verification-v0";

/// Where the task-verification predicate's schema is published (beside
/// the payloads, not under `v2/`: it is the predicate's own version).
pub const PREDICATE: &str =
    "https://hupe1980.github.io/speccoder/schemas/task-verification-v0.json";

/// The schema of the `task-verification` predicate body (v0), published as
/// part of its open specification.
#[must_use]
pub fn predicate() -> Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    let mut schema = serde_json::to_value(
        generator.into_root_schema_for::<core::predicate::TaskVerificationV0>(),
    )
    .unwrap_or_default();
    if let Some(o) = schema.as_object_mut() {
        o.insert("$id".to_owned(), Value::String(PREDICATE.to_owned()));
        o.insert(
            "title".to_owned(),
            Value::String(format!(
                "{} (revision {})",
                core::predicate::TYPE,
                core::predicate::REVISION
            )),
        );
    }
    schema
}

/// `speccoder schema [NAME]`.
#[must_use]
pub fn command(name: Option<&str>) -> core::CommandResult {
    let listed: Vec<core::SchemaName> = names()
        .into_iter()
        .map(|(n, d)| core::SchemaName {
            name: n.to_owned(),
            describes: d.to_owned(),
            url: format!("{BASE}{n}.json"),
        })
        .collect();
    match name {
        None => core::CommandResult::ok("schema", &format!("{} schemas", listed.len())).with_data(
            core::CommandData::Schema(Box::new(core::SchemaReport {
                names: listed,
                schema: None,
            })),
        ),
        Some(n) => match get(n) {
            Some(schema) => core::CommandResult::ok("schema", n).with_data(
                core::CommandData::Schema(Box::new(core::SchemaReport {
                    names: listed,
                    schema: Some(schema),
                })),
            ),
            None => core::CommandResult::new(
                "schema",
                core::CommandStatus::NotFound,
                &format!("no schema `{n}`"),
            )
            .with_code("NOT-FOUND-SCHEMA"),
        },
    }
}
