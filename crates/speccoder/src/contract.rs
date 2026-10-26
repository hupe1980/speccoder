//! The machine contract checked against what is published: the
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
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
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

/// Names given more than once.
fn duplicates<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut twice: Vec<&str> = names.into_iter().filter(|n| !seen.insert(*n)).collect();
    twice.dedup();
    twice
}

/// The names `CommandData::kind` gives, read from its source: one per
/// variant, as the compiler keeps the match exhaustive.
fn payload_kinds() -> Vec<String> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-core/src/command.rs");
    let text = std::fs::read_to_string(file).unwrap();
    let body = text
        .split("pub const fn kind(&self) -> &'static str {")
        .nth(1)
        .and_then(|b| b.split("\n    }\n").next())
        .unwrap();
    body.lines()
        .filter_map(|l| l.split("=> \"").nth(1))
        .filter_map(|r| r.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_payload_has_its_own_name() {
    let kinds = payload_kinds();
    assert!(!kinds.is_empty());
    assert_eq!(
        duplicates(speccoder_core::PAYLOADS.iter().copied()),
        Vec::<&str>::new()
    );
    assert_eq!(
        duplicates(kinds.iter().map(String::as_str)),
        Vec::<&str>::new()
    );
    let mut a: Vec<&str> = kinds.iter().map(String::as_str).collect();
    let mut b = speccoder_core::PAYLOADS.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(
        a, b,
        "CommandData::kind and PAYLOADS name different payloads"
    );
    // A second `verify` is caught as a name.
    assert_eq!(duplicates(["verify", "run", "verify"]), ["verify"]);
}

fn ui_lib() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-web/ui/src/lib")
}

/// The UI's types, generated from the published schemas (not the code:
/// the page is held to what is published).
fn ui_types() -> String {
    let schemas: Vec<(&str, &str, Value)> = crate::schema::ROOTS
        .iter()
        .map(|(name, root)| (*name, *root, published(name)))
        .collect();
    crate::typescript::render(&schemas).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_ui_types_are_what_the_schemas_generate() {
    let file = ui_lib().join("schema.gen.ts");
    let text = ui_types();
    if bless() {
        std::fs::write(&file, text).unwrap();
    } else {
        assert!(
            std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()),
            "UI types differ from the schemas: crates/speccoder-web/ui/src/lib/schema.gen.ts; run SPECCODER_BLESS=1 cargo test -p speccoder ui_types"
        );
    }
}

/// Names `text` declares (`interface X`, `type X =`).
fn declared(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start().trim_start_matches("export ");
            l.strip_prefix("interface ")
                .or_else(|| l.strip_prefix("type "))
                .map(|r| {
                    r.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>()
                })
        })
        .filter(|n| !n.is_empty())
        .collect()
}

#[test]
fn the_hand_written_ui_types_declare_nothing_the_schemas_generate() {
    let generated = declared(&ui_types());
    let hand = std::fs::read_to_string(ui_lib().join("types.ts")).unwrap();
    let twice: Vec<String> = declared(&hand)
        .into_iter()
        .filter(|n| generated.contains(n))
        .collect();
    assert!(
        twice.is_empty(),
        "types.ts declares what schema.gen.ts generates: {twice:?}; import it from ./schema.gen instead"
    );
    assert_eq!(
        declared("export interface A {\n}\ntype B = string;\n// type C\n"),
        ["A", "B"]
    );
}

#[test]
fn the_published_predicate_schema_is_what_the_code_generates() {
    let schema = crate::schema::predicate();
    let text = format!("{}\n", serde_json::to_string_pretty(&schema).unwrap());
    let file = site().join("static/schemas/task-verification-v0.json");
    if bless() {
        std::fs::write(&file, text).unwrap();
    } else {
        assert_eq!(
            std::fs::read_to_string(&file).ok().as_deref(),
            Some(text.as_str()),
            "task-verification-v0.json differs from the code; run with SPECCODER_BLESS=1"
        );
    }
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
            class: "command".to_owned(),
            object: "argv#1a2b3c4d5e6f".to_owned(),
        },
        Event::ApprovalDecided {
            task: "T001".to_owned(),
            call: "c1".to_owned(),
            decision: "approve".to_owned(),
            scope: Some("once".to_owned()),
            by: "Ada".to_owned(),
            how: "asserted".to_owned(),
            class: "command".to_owned(),
            object: "argv#1a2b3c4d5e6f".to_owned(),
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

/// Every speccoder command a person is told to run parses.
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
        Outcome::Planning {
            skills: vec!["plan".to_owned(), "tasks".to_owned()],
        },
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
            stale: 0,
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
}

/// Every action an interface lists stands for a command that parses: on
/// a project with no feature yet, one with planning to commit, and one
/// mid-run with a task selected — the list and Needs You's.
#[test]
fn every_listed_action_is_a_command_that_parses() {
    let git = |dir: &std::path::Path, args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git {args:?}");
    };
    let none = speccoder_core::ApprovalsReport {
        feature: "001-graph".to_owned(),
        pending: Vec::new(),
        decided: Vec::new(),
        unreadable: None,
        batch: Vec::new(),
    };
    let graph = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-speckit/tests/fixtures/graph/specs");
    let mut seen = 0;
    for step in ["no feature", "uncommitted", "committed"] {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        std::fs::create_dir_all(r.join(".specify/memory")).unwrap();
        std::fs::write(r.join(".specify/memory/constitution.md"), "# C\n").unwrap();
        git(r, &["init", "-q"]);
        git(r, &["add", "-A"]);
        git(r, &["commit", "-q", "-m", "init"]);
        if step != "no feature" {
            let ok = std::process::Command::new("cp")
                .arg("-R")
                .arg(&graph)
                .arg(r)
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok);
        }
        if step == "committed" {
            git(r, &["add", "-A"]);
            git(r, &["commit", "-q", "-m", "plan"]);
        }
        let mut commands: Vec<String> =
            speccoder_runtime::actions::available_with(r, None, Some("T005"), Some(&none))
                .into_iter()
                .map(|a| a.command)
                .collect();
        if let Some(CommandData::Inbox(i)) =
            speccoder_runtime::inbox::inbox_with(r, None, None).data()
        {
            commands.extend(
                i.items
                    .iter()
                    .flat_map(|i| i.actions.iter().map(|a| a.command.clone())),
            );
        }
        for c in &commands {
            parses(c);
        }
        seen += commands.len();
    }
    assert!(seen > 10, "{seen} actions listed");
}

/// The Spec Kit workflow step `speccoder workflow install` writes builds
/// speccoder commands that parse: its own `argv()`, run against stand-ins
/// for Spec Kit's modules.
#[test]
fn the_workflow_step_builds_commands_that_parse() {
    let Ok(dir) = tempfile::tempdir() else { return };
    let pkg = dir.path().join("specify_cli/workflows");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(dir.path().join("specify_cli/__init__.py"), "").unwrap();
    std::fs::write(pkg.join("__init__.py"), "").unwrap();
    std::fs::write(
        pkg.join("base.py"),
        "class StepBase: pass\nclass StepContext:\n    def __init__(self, root): self.project_root = root\nclass StepResult: pass\nclass StepStatus: pass\n",
    )
    .unwrap();
    std::fs::write(
        pkg.join("expressions.py"),
        "def evaluate_expression(raw, context): return raw\n",
    )
    .unwrap();
    let step = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-runtime/assets/workflow/__init__.py");
    let script = format!(
        "import importlib.util, json, sys\nsys.path.insert(0, {dir:?})\nspec = importlib.util.spec_from_file_location('step', {step:?})\nm = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)\nfrom specify_cli.workflows.base import StepContext\nc = StepContext('/p')\ns = m.SpeccoderStep()\nprint(json.dumps([s.argv(cfg, c) for cfg in [{{'skill': 'plan', 'args': 'more detail'}}, {{}}, {{'converge': True, 'feature': '001-x', 'max_tasks': 2}}]]))\n",
        dir = dir.path(),
        step = step,
    );
    let Ok(out) = std::process::Command::new("python3")
        .args(["-I", "-B", "-c", &script])
        .output()
    else {
        return; // no Python here: Spec Kit cannot run either
    };
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let argvs: Vec<Vec<String>> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(argvs.len(), 3);
    for mut argv in argvs {
        argv[0] = "speccoder".to_owned();
        let parsed = crate::help::command()
            .try_get_matches_from(&argv)
            .and_then(|m| <crate::cli::Cli as clap::FromArgMatches>::from_arg_matches(&m));
        assert!(
            parsed.is_ok(),
            "{argv:?}: {}",
            parsed.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }
}
