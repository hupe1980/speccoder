//! The settings' tests: the catalogue against the parsers, reading, editing.
use serde_json::json;

use super::catalogue::*;
use super::edit::*;
use super::*;

/// A value of `kind` the parsers accept, for the drift test.
fn sample(k: &Key) -> Value {
    if let Some(d) = k.default
        && let Ok(v) = serde_json::from_str::<Value>(d)
    {
        return v;
    }
    if k.key == "cap" {
        return json!("2M");
    }
    match k.kind {
        Kind::Bool => json!(true),
        Kind::Int(a, _) => json!(a.max(1)),
        Kind::Num(a, b) => json!(f64::midpoint(a, b).min(1.0)),
        Kind::Str => json!("x"),
        Kind::Choice(c) => json!(c[0]),
        Kind::Duration => json!("7d"),
        Kind::Argv => json!(["true"]),
        Kind::ArgvList => json!([["cargo", "check"]]),
        Kind::StrList => json!(["x"]),
    }
}

/// The smallest configuration that holds `section` with `key` set.
fn with(section: &Section, key: &Key) -> Vec<ConfigChange> {
    let prefix = if section.named {
        format!("{}.t", section.name)
    } else {
        section.name.to_owned()
    };
    let mut changes: Vec<ConfigChange> = section
        .keys
        .iter()
        .filter(|r| r.required && r.key != key.key)
        .map(|r| ConfigChange {
            key: format!("{prefix}.{}", r.key),
            value: Some(sample(r)),
            append: None,
        })
        .collect();
    changes.push(ConfigChange {
        key: format!("{prefix}.{}", key.key),
        value: Some(sample(key)),
        append: None,
    });
    // `argv`, `report` and `acceptance_argv` belong to a command tool,
    // which needs its program and report; the rest to cargo-mutants.
    if section.name == "mutation" {
        let set = |key: &str, value: Value| ConfigChange {
            key: key.into(),
            value: Some(value),
            append: None,
        };
        if ["argv", "report", "acceptance_argv"].contains(&key.key) {
            changes.retain(|c| c.key != "mutation.tool");
            changes.insert(0, set("mutation.tool", json!("command")));
            for (k, v) in [
                ("mutation.argv", json!(["mutants", "{out}"])),
                ("mutation.report", json!("report.json")),
            ] {
                if !changes.iter().any(|c| c.key == k) {
                    changes.insert(1, set(k, v));
                }
            }
        } else {
            changes.retain(|c| c.key != "mutation.tool" || key.key == "tool");
            if key.key != "tool" {
                changes.insert(0, set("mutation.tool", json!("cargo-mutants")));
            }
        }
    }
    // An executor names its agent; `kind` and `argv` stand without one.
    if section.name == "executor" && !["agent", "kind", "argv"].contains(&key.key) {
        changes.insert(
            0,
            ConfigChange {
                key: "executor.agent".into(),
                value: Some(json!("copilot")),
                append: None,
            },
        );
    }
    if section.name == "executor" && key.key == "argv" {
        changes.retain(|c| c.key != "executor.kind");
        changes.insert(
            0,
            ConfigChange {
                key: "executor.kind".into(),
                value: Some(json!("command")),
                append: None,
            },
        );
    }
    if section.name == "model" && key.key == "base_url" {
        changes.push(ConfigChange {
            key: "model.provider".into(),
            value: Some(json!("openai-compatible")),
            append: None,
        });
    }
    if section.name == "model" && key.key == "region" {
        changes.push(ConfigChange {
            key: "model.provider".into(),
            value: Some(json!("bedrock")),
            append: None,
        });
    }
    changes
}

#[test]
fn every_catalogued_key_is_accepted_by_the_parsers() {
    for s in CATALOGUE {
        for k in s.keys {
            let changes = with(s, k);
            edit("", &changes).unwrap_or_else(|e| panic!("[{}] {}: {e}", s.name, k.key));
        }
    }
}

#[test]
fn every_key_in_the_reference_is_catalogued() {
    let doc = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/content/docs/configuration.md"),
    )
    .unwrap_or_default();
    assert!(!doc.is_empty(), "the configuration reference is missing");
    let mut section: Option<String> = None;
    let mut missing = Vec::new();
    for line in doc.lines() {
        if line.starts_with("## ") {
            // `## \`[model]\`` sets the section; a heading naming several
            // (or none) leaves rows to name their own.
            let names: Vec<&str> = line.split('`').skip(1).step_by(2).collect();
            section = (names.len() == 1).then(|| {
                names[0]
                    .trim_matches(['[', ']'])
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_owned()
            });
            if line.contains("Your settings") {
                break;
            }
            continue;
        }
        let Some(cell) = line.strip_prefix("| ").and_then(|l| l.split(" |").next()) else {
            continue;
        };
        if cell.starts_with("Key") || cell.starts_with("---") {
            continue;
        }
        let mut prefix = section.clone().map(|s| vec![s]).unwrap_or_default();
        for token in cell.split('`').skip(1).step_by(2) {
            let key = match token.strip_prefix('[').and_then(|t| t.split_once("] ")) {
                Some((table, key)) => {
                    prefix = table.split('.').map(str::to_owned).collect();
                    key.to_owned()
                }
                None => token.to_owned(),
            };
            let mut path = prefix.clone();
            if CATALOGUE
                .iter()
                .any(|c| c.named && Some(&c.name.to_owned()) == prefix.first())
            {
                path.insert(1, "t".to_owned());
            }
            path.push(key);
            let dotted = path.join(".");
            if place(&dotted).is_err() {
                missing.push(dotted);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "documented but not catalogued: {missing:?}"
    );
}

#[test]
fn bedrock_is_set_provider_first_then_region() {
    let set = |key: &str, v: Value| ConfigChange {
        key: key.into(),
        value: Some(v),
        append: None,
    };
    let text = "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n";
    // A region before the provider is refused: it is Bedrock's.
    assert!(edit(text, &[set("model.region", json!("us-west-2"))]).is_err());
    let step = edit(text, &[set("model.provider", json!("bedrock"))]).unwrap();
    assert!(validate(&step).is_err(), "not in force without a region");
    let done = edit(&step, &[set("model.region", json!("us-west-2"))]).unwrap();
    let m = crate::substrate::model_config::parse(&done)
        .unwrap()
        .unwrap();
    assert_eq!(
        (m.provider.as_str(), m.region.as_deref()),
        ("bedrock", Some("us-west-2"))
    );
    // Anything else wrong is still refused on the way.
    assert!(edit(&step, &[set("model.max_output_tokens", json!(0))]).is_err());
    let p = place("model.provider").unwrap();
    assert!(matches!(p.key.kind, Kind::Choice(c) if c.contains(&"bedrock")));
}

#[test]
fn the_step_to_bedrock_is_let_through_only_when_the_region_is_all_that_is_missing() {
    let set = |key: &str, v: Value| ConfigChange {
        key: key.into(),
        value: Some(v),
        append: None,
    };
    // A key Bedrock does not use, left from the provider before.
    let compat = "[model]\nprovider = \"openai-compatible\"\nmodel = \"m.x\"\nbase_url = \"http://localhost:11434/v1\"\n";
    let e = edit(compat, &[set("model.provider", json!("bedrock"))]).unwrap_err();
    assert_eq!(
        e,
        "[model]: `base_url` does not apply to provider `bedrock` (its credentials and endpoint come from AWS) — speccoder config unset model.base_url"
    );
    // Its fix is the unset, from `config set` and from every command.
    assert_eq!(
        crate::substrate::model_config::config_fix(&e).as_deref(),
        Some("speccoder config unset model.base_url")
    );
    assert_eq!(
        crate::substrate::model_config::refused("run", &e).fix(),
        ["speccoder config unset model.base_url"]
    );
    // Any other `[model]` error on the way is still refused.
    let step = "[model]\nprovider = \"bedrock\"\nmodel = \"m.x\"\n";
    assert!(edit(step, &[set("model.model", json!(""))]).is_err());
    assert!(edit(step, &[set("model.structured", json!("tool"))]).is_err());
    assert!(edit(step, &[set("model.reasoning_summary", json!(false))]).is_ok());
}

#[test]
fn switching_to_bedrock_says_to_set_the_region_and_warns_of_a_model_id_bedrock_does_not_use() {
    let dir = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(ok.success());
    std::fs::write(
        dir.path().join(FILE),
        "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n",
    )
    .unwrap();
    let r = apply(
        dir.path(),
        &[ConfigChange {
            key: "model.provider".into(),
            value: Some(json!("bedrock")),
            append: None,
        }],
    );
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert!(
        r.message().contains(
            "warning: `claude-sonnet-5-5` is not a Bedrock model id; Bedrock names models like global.anthropic.claude-sonnet-5-5"
        ),
        "{}",
        r.message()
    );
    assert_eq!(
        r.next().first().map(|n| n.command.as_str()),
        Some("speccoder config set model.region <region>")
    );
    // The listing marks the region required for this provider.
    let region = read(dir.path())
        .sections
        .into_iter()
        .find(|s| s.name == "model")
        .and_then(|s| s.keys.into_iter().find(|k| k.key == "region"))
        .unwrap();
    assert!(region.required);
    // With the region set: nothing left to say about it.
    let r = apply(
        dir.path(),
        &[ConfigChange {
            key: "model.region".into(),
            value: Some(json!("eu-central-1")),
            append: None,
        }],
    );
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert!(!r.message().contains("set it next"), "{}", r.message());
    assert_eq!(
        r.next().first().map(|n| n.command.as_str()),
        Some("speccoder config commit")
    );
    // An id Bedrock uses: no warning.
    assert_eq!(
        crate::substrate::model_config::bedrock_model_warning("global.anthropic.claude-sonnet-5-5"),
        None
    );
    assert_eq!(
        crate::substrate::model_config::bedrock_model_warning(
            "arn:aws:bedrock:eu-central-1:123456789012:inference-profile/x"
        ),
        None
    );
}

#[test]
fn an_edit_keeps_comments_and_refuses_what_a_parser_refuses() {
    let text = "# The project's checks.\n[checks.test]\nargv = [\"cargo\", \"test\"] # all of them\n\n# How much an attempt may do.\n[budget]\nmax_turns = 30 # plenty\n";
    let out = edit(
        text,
        &[ConfigChange {
            key: "budget.max_turns".into(),
            value: Some(json!(40)),
            append: None,
        }],
    )
    .unwrap();
    assert!(out.contains("# The project's checks."), "{out}");
    assert!(out.contains("# How much an attempt may do."), "{out}");
    assert!(out.contains("max_turns = 40 # plenty"), "{out}");
    assert!(
        out.contains("argv = [\"cargo\", \"test\"] # all of them"),
        "{out}"
    );
    // Out of range, unknown, the wrong kind: nothing is written.
    for (key, value) in [
        ("budget.max_turns", json!(0)),
        ("budget.max_turn", json!(3)),
        ("sandbox.mode", json!("maybe")),
        ("ui.theme", json!("dark")),
    ] {
        assert!(
            edit(
                text,
                &[ConfigChange {
                    key: key.into(),
                    value: Some(value),
                    append: None
                }]
            )
            .is_err(),
            "{key}"
        );
    }
    // Unset removes the key and an emptied table.
    let out = edit(
        text,
        &[ConfigChange {
            key: "budget.max_turns".into(),
            value: None,
            append: None,
        }],
    )
    .unwrap();
    assert!(!out.contains("[budget]"), "{out}");
    // Append adds once.
    let allow = ConfigChange {
        key: "commands.allow".into(),
        value: None,
        append: Some(json!(["cargo", "fetch"])),
    };
    let once = edit(text, std::slice::from_ref(&allow)).unwrap();
    let twice = edit(&once, &[allow]).unwrap();
    assert_eq!(once, twice);
    assert!(once.contains("allow = [[\"cargo\", \"fetch\"]]"), "{once}");
    // A named table is made under an implicit container.
    let out = edit(
        "",
        &[ConfigChange {
            key: "checks.lint.argv".into(),
            value: Some(json!(["cargo", "clippy"])),
            append: None,
        }],
    )
    .unwrap();
    assert!(
        out.contains("[checks.lint]") && !out.contains("[checks]\n"),
        "{out}"
    );
}

/// A unit of more than one byte is refused, never a panic (it aborted
/// the web host).
#[test]
fn a_multibyte_unit_is_refused_not_a_panic() {
    let w = |s: &str| vec![s.to_owned()];
    for bad in ["7д", "1€", "д", "€"] {
        assert!(value_of("approvals.wait", &w(bad)).is_err(), "{bad}");
    }
    assert!(crate::substrate::run_config::parse("[approvals]\nwait = \"7д\"\n").is_err());
}

/// Inline tables are tables: a key in one is set and unset like any.
#[test]
fn inline_tables_are_edited_not_skipped() {
    let text = "[attest]\nsigstore = { identity = \"a@b\", issuer = \"https://x\" }\n\n[checks]\nlint = { argv = [\"cargo\", \"clippy\"] }\n";
    let unset = |k: &str| ConfigChange {
        key: k.into(),
        value: None,
        append: None,
    };
    let set = |k: &str, v: Value| ConfigChange {
        key: k.into(),
        value: Some(v),
        append: None,
    };
    let out = edit(text, &[unset("attest.sigstore.identity")]).unwrap_or_else(|e| panic!("{e}"));
    assert!(!out.contains("a@b") && out.contains("issuer"), "{out}");
    let out =
        edit(text, &[set("checks.lint.tier", json!("full"))]).unwrap_or_else(|e| panic!("{e}"));
    assert!(out.contains("tier = \"full\""), "{out}");
    let out = edit(
        text,
        &[
            unset("attest.sigstore.identity"),
            unset("attest.sigstore.issuer"),
        ],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(!out.contains("sigstore"), "{out}");
}

#[test]
fn words_become_values_of_the_key_s_kind() {
    let w = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(value_of("budget.max_turns", &w("40")).unwrap(), json!(40));
    assert_eq!(value_of("monitor.enabled", &w("on")).unwrap(), json!(true));
    assert_eq!(
        value_of("format.argv", &w("cargo fmt")).unwrap(),
        json!(["cargo", "fmt"])
    );
    assert_eq!(
        value_of("commands.allow", &w("cargo check")).unwrap(),
        json!([["cargo", "check"]])
    );
    assert_eq!(value_of("approvals.wait", &w("2d")).unwrap(), json!("2d"));
    assert!(value_of("budget.max_turns", &w("lots")).is_err());
}
