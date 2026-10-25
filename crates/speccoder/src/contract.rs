//! The machine contract checked against what is published (F-056): the
//! schemas under `site/static/schemas/v2/` and the codes page are what the
//! code generates (`SPECCODER_BLESS=1` writes them), the CLI reference
//! lists every exit status, and results and events validate.

use std::path::PathBuf;

use serde_json::Value;
use speccoder_core::codes::CATALOG;
use speccoder_core::{CommandData, CommandResult, CommandStatus};

fn site() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../site")
}

fn bless() -> bool {
    std::env::var_os("SPECCODER_BLESS").is_some()
}

fn published(name: &str) -> Value {
    let file = site().join(format!("static/schemas/v2/{name}.json"));
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|_| panic!("{} missing; run with SPECCODER_BLESS=1", file.display()));
    serde_json::from_str(&text).unwrap()
}

/// `value` against the published schema `name`; the errors, if any.
pub fn errors(name: &str, value: &Value) -> Vec<String> {
    let schema = published(name);
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

/// A `--json` result: the envelope against `result`, `data` against its
/// `payload`'s schema.
pub fn check_result(value: &Value) {
    let e = errors("result", value);
    assert!(e.is_empty(), "result: {e:?}\n{value}");
    if let Some(payload) = value["payload"].as_str() {
        let e = errors(payload, &value["data"]);
        assert!(e.is_empty(), "{payload}: {e:?}\n{}", value["data"]);
    }
    if value["status"] != "ok" {
        assert!(
            value["code"].is_string() && value["docs"].is_string(),
            "{value}"
        );
        assert!(!value["fix"].as_array().unwrap().is_empty(), "{value}");
    }
}

#[test]
fn the_published_schemas_are_what_the_code_generates() {
    let dir = site().join("static/schemas/v2");
    let mut stale = Vec::new();
    for (name, _) in crate::schema::names() {
        let schema = crate::schema::get(name).unwrap();
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{name}"
        );
        let text = format!("{}\n", serde_json::to_string_pretty(&schema).unwrap());
        let file = dir.join(format!("{name}.json"));
        if bless() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&file, text).unwrap();
        } else if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
            stale.push(name);
        }
    }
    assert!(
        stale.is_empty(),
        "schemas differ from the code: {stale:?}; run with SPECCODER_BLESS=1"
    );
}

#[test]
fn the_codes_page_is_the_catalog_and_the_cli_reference_lists_every_status() {
    let file = site().join("content/docs/codes.md");
    let page = crate::explain::page();
    if bless() {
        std::fs::write(&file, &page).unwrap();
    } else {
        assert_eq!(
            std::fs::read_to_string(&file).unwrap_or_default(),
            page,
            "codes.md differs from the catalog; run with SPECCODER_BLESS=1"
        );
    }
    let cli = std::fs::read_to_string(site().join("content/docs/cli.md")).unwrap();
    let mut statuses: Vec<CommandStatus> = CATALOG.iter().map(|e| e.status).collect();
    statuses.dedup();
    for s in statuses {
        let word = serde_json::to_value(s).unwrap();
        assert!(
            cli.contains(&format!("| `{}` |", word.as_str().unwrap())),
            "cli.md has no row for {word}"
        );
    }
}

#[test]
fn every_status_with_its_default_code_validates() {
    let mut seen = std::collections::BTreeSet::new();
    for e in CATALOG {
        if !seen.insert(format!("{:?}", e.status)) {
            continue;
        }
        let r = CommandResult::new("run", e.status, "x");
        check_result(&serde_json::to_value(&r).unwrap());
    }
    let explained = crate::explain::command(None, true);
    check_result(&serde_json::to_value(&explained).unwrap());
    let schemas = crate::schema::command(Some("event"));
    check_result(&serde_json::to_value(&schemas).unwrap());
    assert!(matches!(schemas.data(), Some(CommandData::Schema(_))));
}

#[test]
fn every_event_type_validates() {
    use speccoder_core::stage::{Stage, StageState};
    use speccoder_runtime::progress::{Event, PlannedTask};
    let events = [
        Event::RunStarted {
            feature: "001-x".to_owned(),
            tasks: vec![PlannedTask {
                id: "T001".to_owned(),
                title: "t".to_owned(),
                state: "ready".to_owned(),
            }],
        },
        Event::TaskStarted {
            feature: "001-x".to_owned(),
            task: "T001".to_owned(),
            title: "t".to_owned(),
        },
        Event::Stage {
            task: "T001".to_owned(),
            stage: Stage::Baseline,
            state: StageState::Started,
            detail: None,
            elapsed_ms: None,
            tail: Vec::new(),
        },
        Event::Stage {
            task: "T001".to_owned(),
            stage: Stage::Baseline,
            state: StageState::Failed,
            detail: Some("x".to_owned()),
            elapsed_ms: Some(3),
            tail: vec!["boom".to_owned()],
        },
        Event::Resumed {
            task: "T001".to_owned(),
            stage: Some(Stage::Attempt),
            last_call: None,
        },
        Event::ToolCall {
            task: "T001".to_owned(),
            turn: 1,
            tool: "run_command".to_owned(),
            target: "cargo test".to_owned(),
            decision: "allow".to_owned(),
            ok: Some(true),
            tail: Vec::new(),
        },
        Event::ApprovalRequested {
            task: "T001".to_owned(),
            call: "c1".to_owned(),
            tool: "run_command".to_owned(),
            what: "cargo add x".to_owned(),
            class: "run `cargo add x`".to_owned(),
        },
        Event::ApprovalDecided {
            task: "T001".to_owned(),
            call: "c1".to_owned(),
            decision: "approve".to_owned(),
            scope: Some("once".to_owned()),
            by: "Ada".to_owned(),
            how: "asserted".to_owned(),
        },
        Event::Usage {
            task: "T001".to_owned(),
            input_tokens: 1,
            output_tokens: 2,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: Some(0.1),
        },
        Event::TaskFinished {
            task: "T001".to_owned(),
            outcome: "verified".to_owned(),
            message: "m".to_owned(),
            run: Some("run_1".to_owned()),
            input_tokens: 1,
            output_tokens: 2,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: None,
        },
        Event::Notice {
            text: "n".to_owned(),
        },
    ];
    for (i, ev) in events.iter().enumerate() {
        let line = crate::events::Line {
            v: 2,
            seq: i as u64 + 1,
            ts: crate::events::now(),
            body: crate::events::body(ev),
        };
        let value = serde_json::to_value(&line).unwrap();
        let e = errors("event", &value);
        assert!(e.is_empty(), "{e:?}\n{value}");
    }
    let result = crate::events::Line {
        v: 2,
        seq: 99,
        ts: crate::events::now(),
        body: crate::events::Body::Result {
            result: CommandResult::new("run", CommandStatus::NeedsHuman, "x"),
        },
    };
    let value = serde_json::to_value(&result).unwrap();
    assert!(errors("event", &value).is_empty(), "{value}");
    // A line of the wrong shape is refused: the schema is not "anything".
    let bad =
        serde_json::json!({ "v": 2, "seq": 1, "ts": "x", "type": "task.finished", "task": 3 });
    assert!(!errors("event", &bad).is_empty());
}

/// A command line as a shell would split it (double quotes only).
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Every speccoder command a person is told to run parses (F-057, SC-002).
fn parses(line: &str) {
    // `<task>`-style blanks a person fills in.
    let filled = line
        .replace("<task>", "T004")
        .replace("<dependency>", "T003")
        .replace("<feature>", "001-x")
        .replace("<name>", "001-x")
        .replace("<run>", "run_1")
        .replace("<dir>", "keep")
        .replace("<SKILL>", "plan")
        .replace("<TASK>", "T004")
        .replace("<command>", "run");
    for part in filled.split("&&") {
        let w = words(part.trim());
        if w.first().map(String::as_str) != Some("speccoder") {
            continue;
        }
        let parsed = crate::help::command()
            .try_get_matches_from(&w)
            .and_then(|m| <crate::cli::Cli as clap::FromArgMatches>::from_arg_matches(&m));
        let help = parsed
            .as_ref()
            .err()
            .is_some_and(|e| e.kind() == clap::error::ErrorKind::DisplayHelp);
        assert!(
            parsed.is_ok() || help,
            "`{line}` does not parse: {}",
            parsed.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }
}

#[test]
fn every_next_step_and_fix_is_a_command_that_parses() {
    use speccoder_core::next::{Outcome, next};
    let t = || "T004".to_owned();
    let outcomes = [
        Outcome::TaskPassed {
            task: t(),
            ready: 1,
        },
        Outcome::TaskPassed {
            task: t(),
            ready: 0,
        },
        Outcome::FeatureDone,
        Outcome::NeedsApproval { task: t() },
        Outcome::Blocked { task: t() },
        Outcome::BudgetExhausted { task: t() },
        Outcome::VerificationFailed { task: t() },
        Outcome::SuspectedGaming { task: t() },
        Outcome::TestsOverStrict { task: t() },
        Outcome::AcceptanceWeak { task: t() },
        Outcome::NotDemonstrated { task: t() },
        Outcome::RunFailed { task: Some(t()) },
        Outcome::RunFailed { task: None },
        Outcome::Interrupted { task: t() },
        Outcome::Running,
        Outcome::NothingReady,
        Outcome::ReportShown {
            ready: 1,
            open_approvals: 1,
        },
        Outcome::ApprovalsListed { first: Some(t()) },
        Outcome::Decided,
        Outcome::ChecklistsOpen,
        Outcome::ConfigError,
        Outcome::GitPrecondition,
        Outcome::ArchiveAwaiting {
            change: "add-x".to_owned(),
        },
    ];
    for o in &outcomes {
        for feature in [None, Some("001-x")] {
            for step in next(o, feature) {
                parses(&step.command);
            }
        }
    }
    for e in CATALOG {
        for f in speccoder_core::codes::fixes(e, "run", Some("T004"), Some("001-x")) {
            parses(&f);
        }
    }
    for (_, new) in crate::help::FORMER {
        parses(new);
    }
}
