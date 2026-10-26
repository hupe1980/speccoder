#![allow(dead_code)] // shared by several test binaries; each uses part of it
//! Every `--json` the CLI tests read is checked against the published
//! schemas: the envelope against `result`, `data` against
//! the schema its `payload` names, and a non-ok result has a code, a fix
//! and a docs link.

use serde_json::Value;

/// A state directory of this test process's own, for every binary a test
/// spawns without naming one: the developer's real state directory is
/// never written. Under nextest it is the one its wrapper made for this
/// test (`scripts/test-state.sh`), removed when the test ends. Under plain
/// `cargo test` it is this process's own under the target directory's
/// `tmp` (removed by `cargo clean`); those of processes that ended are
/// removed as the next one starts.
pub fn isolated_state() -> &'static std::path::Path {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if std::env::var_os("NEXTEST").is_some()
            && let Some(dir) = std::env::var_os("SPECCODER_STATE_DIR").filter(|d| !d.is_empty())
        {
            return dir.into();
        }
        let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("speccoder-test-state");
        std::fs::create_dir_all(&root).unwrap();
        for entry in std::fs::read_dir(&root).unwrap().flatten() {
            let ended = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|pid| {
                    !std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .stderr(std::process::Stdio::null())
                        .status()
                        .is_ok_and(|s| s.success())
                });
            if ended {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        let dir = root.join(std::process::id().to_string());
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    })
}

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
