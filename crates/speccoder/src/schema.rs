//! The JSON Schemas of everything speccoder prints (F-056), generated from
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
    v.extend(core::PAYLOADS.iter().map(|p| (*p, describe(p))));
    v
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
        _ => "",
    }
}

/// The schema `name`, if speccoder publishes one by that name.
#[must_use]
pub fn get(name: &str) -> Option<Value> {
    let title = names().into_iter().find(|(n, _)| *n == name)?.1;
    Some(match name {
        "result" => generate::<core::CommandResult>(name, title),
        "event" => generate::<crate::events::Line>(name, title),
        "inspect" => generate::<core::snapshot::ProjectSnapshot>(name, title),
        "verify" => generate::<core::verify::VerifyReport>(name, title),
        "worktree" => generate::<core::WorktreeReport>(name, title),
        "propose" => generate::<core::ProposeReport>(name, title),
        "replay" => generate::<core::ReplayReport>(name, title),
        "run" => generate::<core::RunReport>(name, title),
        "eval" => generate::<core::EvalReport>(name, title),
        "feature" => generate::<core::FeatureReport>(name, title),
        "converge" => generate::<core::ConvergeReport>(name, title),
        "report" => generate::<core::StatusReport>(name, title),
        "approvals" => generate::<core::ApprovalsReport>(name, title),
        "control" => generate::<core::ControlReport>(name, title),
        "cost" => generate::<core::CostReport>(name, title),
        "pr" => generate::<core::PrReport>(name, title),
        "bench" => generate::<core::BenchReport>(name, title),
        "init" => generate::<core::InitReport>(name, title),
        "skills" => generate::<core::SkillsReport>(name, title),
        "skill" => generate::<core::SkillRunReport>(name, title),
        "checklists" => generate::<Vec<core::ChecklistStatus>>(name, title),
        "archive" => generate::<core::ArchiveReport>(name, title),
        "in-place" => generate::<core::InPlaceReport>(name, title),
        "attest" => generate::<core::AttestReport>(name, title),
        "explain" => generate::<core::ExplainReport>(name, title),
        "schema" => generate::<core::SchemaReport>(name, title),
        "doctor" => generate::<core::DoctorReport>(name, title),
        "features" => generate::<core::FeaturesReport>(name, title),
        "ui" => generate::<core::UiReport>(name, title),
        "config" => generate::<core::ConfigReport>(name, title),
        "issues" => generate::<core::IssuesReport>(name, title),
        "inbox" => generate::<core::InboxReport>(name, title),
        "review" => generate::<core::ReviewReport>(name, title),
        "auth" => generate::<core::AuthReport>(name, title),
        _ => return None,
    })
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
