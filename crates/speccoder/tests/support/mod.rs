#![allow(dead_code)] // shared by several test binaries; each uses part of it
//! Every `--json` the CLI tests read is checked against the published
//! schemas (F-056, SC-001): the envelope against `result`, `data` against
//! the schema its `payload` names, and a non-ok result has a code, a fix
//! and a docs link.

use serde_json::Value;

fn schema(name: &str) -> Value {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../site/static/schemas/v2/{name}.json"));
    serde_json::from_str(
        &std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display())),
    )
    .unwrap()
}

fn check(name: &str, value: &Value) {
    let s = schema(name);
    let v = jsonschema::validator_for(&s).unwrap();
    let errors: Vec<String> = v
        .iter_errors(value)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{name}: {errors:?}\n{value}");
}

/// Parses stdout as one result and checks it.
pub fn result(stdout: &[u8]) -> Value {
    let text = String::from_utf8_lossy(stdout);
    let v: Value = serde_json::from_str(text.trim()).unwrap_or_else(|e| panic!("{e}: {text}"));
    checked(v)
}

/// Checks a parsed result.
pub fn checked(v: Value) -> Value {
    // The payload first: its errors name the field, the envelope's only
    // say "not valid under any of the payloads".
    if let Some(p) = v["payload"].as_str() {
        check(p, &v["data"]);
    }
    check("result", &v);
    if v["status"] != "ok" {
        assert!(v["code"].is_string(), "no code: {v}");
        assert!(
            v["fix"].as_array().is_some_and(|f| !f.is_empty()),
            "no fix: {v}"
        );
        assert!(
            v["docs"]
                .as_str()
                .is_some_and(|d| d.starts_with("https://")),
            "no docs: {v}"
        );
    }
    v
}

/// Checks one `--events` line.
pub fn event(line: &str) -> Value {
    let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    check("event", &v);
    if v["type"] == "result" {
        checked(v["result"].clone());
    }
    v
}
