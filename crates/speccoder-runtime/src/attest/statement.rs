//! in-toto Statements v1 for a ticked task: `test-result` per test phase, a
//! SLSA verification summary (`vsa`), and speccoder's `task-verification`.

use serde_json::{Map, Value, json};

/// Statement v1's `_type`.
pub const STATEMENT: &str = "https://in-toto.io/Statement/v1";
/// in-toto's test-result predicate.
pub const TEST_RESULT: &str = "https://in-toto.io/attestation/test-result/v0.1";
/// SLSA's verification summary.
pub const VSA: &str = "https://slsa.dev/verification_summary/v1";
/// speccoder's own: this task was verified against this spec.
pub const TASK_VERIFICATION: &str = "https://speccoder.dev/task-verification/v0";
/// Who verifies, in a VSA.
pub const VERIFIER: &str = "https://speccoder.dev/verifier/v0";
/// The policy a VSA names: the judge's configuration.
pub const POLICY: &str = "https://speccoder.dev/policy/judge-config/v0";

/// What every statement of one tick is about: the task commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// `<feature>/<task>`.
    pub name: String,
    /// The commit's object id (40 or 64 hex).
    pub commit: String,
}

impl Subject {
    fn value(&self) -> Value {
        json!([{ "name": self.name, "digest": { "gitCommit": self.commit } }])
    }
}

/// A statement about `subject`.
#[must_use]
pub fn statement(subject: &Subject, predicate_type: &str, predicate: Value) -> Value {
    json!({
        "_type": STATEMENT,
        "subject": subject.value(),
        "predicateType": predicate_type,
        "predicate": predicate,
    })
}

/// The canonical bytes of `v`: keys sorted at every level, no whitespace.
/// What is signed, so it must not depend on how a map was built.
#[must_use]
pub fn canonical(v: &Value) -> Vec<u8> {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for k in keys {
                    out.insert(k.clone(), sorted(&m[k]));
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_vec(&sorted(v)).unwrap_or_default()
}

/// A test phase of one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The checks before the attempt (pre-existing failures are warnings).
    Baseline,
    /// The acceptance tests before the attempt: red is expected.
    Red,
    /// The acceptance tests and the checks after the attempt.
    Green,
    /// The tests the implementer never saw (F-059).
    HeldOut,
}

impl Phase {
    /// The phase's name in `configuration[0].name`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Red => "red",
            Self::Green => "green",
            Self::HeldOut => "held_out",
        }
    }
}

/// One phase's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseResult {
    /// Which.
    pub phase: Phase,
    /// `PASSED`, `WARNED` or `FAILED`.
    pub result: &'static str,
    /// Tests that passed.
    pub passed: Vec<String>,
    /// Tests that failed before and still do (baseline).
    pub warned: Vec<String>,
    /// Tests that failed.
    pub failed: Vec<String>,
    /// sha256 of what ran (the checks' or acceptance tests' argv).
    pub config_sha256: String,
}

/// The `test-result` predicate of `r`.
#[must_use]
pub fn test_result(r: &PhaseResult) -> Value {
    let mut p = json!({
        "result": r.result,
        "configuration": [{
            "name": format!("speccoder:phase/{}", r.phase.name()),
            "digest": { "sha256": r.config_sha256 },
        }],
        "passedTests": r.passed,
        "failedTests": r.failed,
    });
    if !r.warned.is_empty() {
        p["warnedTests"] = json!(r.warned);
    }
    p
}

/// What a VSA summarises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// speccoder's version.
    pub speccoder: String,
    /// agentplane's version.
    pub agentplane: String,
    /// RFC 3339 UTC.
    pub time: String,
    /// `git+<origin>@<commit>` or `git:<commit>`.
    pub resource_uri: String,
    /// The judge-config hash.
    pub policy_sha256: String,
    /// `(name, sha256 of that line's payload)` of every other line of the tick.
    pub inputs: Vec<(String, String)>,
    /// `verified` or `regression_only`.
    pub verdict: String,
}

/// The VSA predicate of `s`.
#[must_use]
pub fn vsa(s: &Summary) -> Value {
    let level = if s.verdict == "verified" {
        "SPECCODER_VERIFIED"
    } else {
        "SPECCODER_REGRESSION_ONLY"
    };
    json!({
        "verifier": {
            "id": VERIFIER,
            "version": { "speccoder": s.speccoder, "agentplane": s.agentplane },
        },
        "timeVerified": s.time,
        "resourceUri": s.resource_uri,
        "policy": { "uri": POLICY, "digest": { "sha256": s.policy_sha256 } },
        "inputAttestations": s
            .inputs
            .iter()
            .map(|(name, sha)| json!({ "name": name, "digest": { "sha256": sha } }))
            .collect::<Vec<_>>(),
        "verificationResult": "PASSED",
        "verifiedLevels": [level],
    })
}

/// Why `v` may not go into a statement: an absolute local path, a URL with
/// credentials, or something shaped like a secret (FR-013).
pub fn check_clean(v: &Value) -> Result<(), String> {
    let mut problem = None;
    walk(v, &mut |s| {
        if problem.is_none() {
            problem = unclean(s);
        }
    });
    problem.map_or(Ok(()), Err)
}

fn walk(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
        Value::Object(m) => m.values().for_each(|x| walk(x, f)),
        _ => {}
    }
}

fn unclean(s: &str) -> Option<String> {
    let absolute = [
        "/Users/",
        "/home/",
        "/private/",
        "/var/folders/",
        "/tmp/",
        "/root/",
    ];
    if s.split_whitespace().any(|w| {
        let w = w.trim_start_matches(['"', '\'', '(', '`']);
        absolute.iter().any(|p| w.starts_with(p))
            || (w.len() > 2 && w.as_bytes()[1] == b':' && w.as_bytes()[2] == b'\\')
    }) {
        return Some("an absolute local path".to_owned());
    }
    if let Some(i) = s.find("://") {
        let rest = &s[i + 3..];
        let host_end = rest.find(['/', ' ']).unwrap_or(rest.len());
        if rest[..host_end].contains('@') {
            return Some("a URL with credentials".to_owned());
        }
    }
    let secret = [
        "sk-ant-",
        "sk-proj-",
        "ghp_",
        "github_pat_",
        "AKIA",
        "xoxb-",
        "-----BEGIN",
    ];
    if secret.iter().any(|p| s.contains(p)) {
        return Some("something shaped like a secret".to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> Subject {
        Subject {
            name: "001-demo/T004".to_owned(),
            commit: "3f1c".repeat(10),
        }
    }

    #[test]
    fn statements_are_in_toto_v1_about_the_commit() {
        let s = statement(&subject(), TEST_RESULT, json!({}));
        assert_eq!(s["_type"], STATEMENT);
        assert_eq!(s["subject"][0]["name"], "001-demo/T004");
        assert_eq!(s["subject"][0]["digest"]["gitCommit"], "3f1c".repeat(10));
    }

    #[test]
    fn canonical_bytes_sort_keys_at_every_level() {
        let a = json!({ "b": 1, "a": { "d": 2, "c": [ { "z": 0, "y": 1 } ] } });
        assert_eq!(
            String::from_utf8(canonical(&a)).unwrap(),
            r#"{"a":{"c":[{"y":1,"z":0}],"d":2},"b":1}"#
        );
    }

    #[test]
    fn a_phase_names_itself_and_lists_its_tests() {
        let p = test_result(&PhaseResult {
            phase: Phase::Baseline,
            result: "WARNED",
            passed: vec!["a".to_owned()],
            warned: vec!["flaky::old".to_owned()],
            failed: Vec::new(),
            config_sha256: "00".repeat(32),
        });
        assert_eq!(p["configuration"][0]["name"], "speccoder:phase/baseline");
        assert_eq!(p["warnedTests"][0], "flaky::old");
        assert_eq!(p["result"], "WARNED");
    }

    #[test]
    fn a_summary_names_its_level_and_inputs() {
        let v = vsa(&Summary {
            speccoder: "0.1.0".to_owned(),
            agentplane: "0.52.0".to_owned(),
            time: "2026-10-24T10:00:00Z".to_owned(),
            resource_uri: "git:abc".to_owned(),
            policy_sha256: "11".repeat(32),
            inputs: vec![("test-result/green".to_owned(), "22".repeat(32))],
            verdict: "regression_only".to_owned(),
        });
        assert_eq!(v["verifiedLevels"][0], "SPECCODER_REGRESSION_ONLY");
        assert_eq!(v["inputAttestations"][0]["name"], "test-result/green");
        assert_eq!(v["policy"]["uri"], POLICY);
    }

    #[test]
    fn no_secret_or_local_path_goes_into_a_statement() {
        assert!(check_clean(&json!({ "t": ["cargo-test::parse::empty"] })).is_ok());
        for bad in [
            json!({ "p": "/Users/ada/src/x.rs" }),
            json!({ "p": ["see (/home/ada/x)"] }),
            json!({ "u": "https://ada:hunter2@github.com/o/r" }),
            json!({ "k": "token sk-ant-api03-xxxx" }),
            json!({ "w": r"C:\Users\ada" }),
        ] {
            assert!(check_clean(&bad).is_err(), "{bad}");
        }
        // A remote without credentials is fine.
        assert!(check_clean(&json!({ "u": "git+https://github.com/o/r@abc" })).is_ok());
    }
}
