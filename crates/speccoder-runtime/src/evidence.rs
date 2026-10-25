//! A task's evidence as rows (F-058, D-087): its verdict and what
//! established it, read from the signed statements on its commit (F-061)
//! and the report — never re-derived, so what an interface shows is what a
//! reviewer can verify with `attest verify`. Every interface renders these
//! rows; none computes its own.

use serde_json::Value;
use speccoder_core::{RunReport, TaskStatus};

pub use speccoder_core::evidence::Row;

/// The banner: the verdict's mark and its words, which never blur.
#[must_use]
pub fn banner(state: &str) -> (&'static str, &'static str) {
    match state {
        "verified" => ("verified", "VERIFIED"),
        "regression_only" => (
            "regression_only",
            "REGRESSION ONLY — nothing task-specific was shown",
        ),
        "attested" => ("attested", "ATTESTED — a person's word, not evidence"),
        "ticked_without_evidence" => (
            "ticked_without_evidence",
            "TICKED WITHOUT SPECCODER EVIDENCE",
        ),
        "evidence_without_tick" => (
            "evidence_without_tick",
            "COMMITTED — its local tick is missing",
        ),
        "skipped" => ("skipped", "SKIPPED"),
        "ready" => ("ready", "READY"),
        _ => ("blocked", "BLOCKED"),
    }
}

/// The statements of a commit's note, decoded.
#[must_use]
pub fn statements(repo: &std::path::Path, commit: &str) -> Vec<Value> {
    let Some(note) = crate::attest::notes::read(repo, commit) else {
        return Vec::new();
    };
    crate::attest::dsse::decode(&note)
        .0
        .iter()
        .filter_map(crate::attest::dsse::Envelope::statement)
        .collect()
}

fn phase<'a>(statements: &'a [Value], name: &str) -> Option<&'a Value> {
    statements.iter().find_map(|s| {
        let p = &s["predicate"];
        (p["configuration"][0]["name"] == format!("speccoder:phase/{name}")).then_some(p)
    })
}

fn names(v: &Value) -> usize {
    v.as_array().map_or(0, Vec::len)
}

/// The rows for `task`, from its commit's `statements` and, in a hosted
/// run, the run's own report.
#[must_use]
pub fn rows(task: &TaskStatus, statements: &[Value], run: Option<&RunReport>) -> Vec<Row> {
    let mut out = Vec::new();
    let row = |label: &str, mark: &str, text: String| Row {
        label: label.to_owned(),
        mark: mark.to_owned(),
        text,
    };
    let tv = statements
        .iter()
        .find(|s| s["predicateType"] == "https://speccoder.dev/task-verification/v0")
        .map(|s| &s["predicate"]);
    // Acceptance: red, then green.
    match (phase(statements, "red"), phase(statements, "green")) {
        (Some(red), Some(green)) => out.push(row(
            "Acceptance",
            "verified",
            format!(
                "red → green: {} failing before, {} passing after (spec-derived, frozen)",
                names(&red["failedTests"]),
                names(&green["passedTests"])
            ),
        )),
        (None, Some(green)) => out.push(row(
            "Acceptance",
            "regression_only",
            format!(
                "none red first — the checks alone ({} passing)",
                names(&green["passedTests"])
            ),
        )),
        _ => out.push(row("Acceptance", "ready", "not run".to_owned())),
    }
    out.push(match phase(statements, "held_out") {
        Some(h) if h["result"] == "PASSED" => row(
            "Held-out",
            "verified",
            format!(
                "{} passed (judge only, never shown to the implementer)",
                names(&h["passedTests"])
            ),
        ),
        Some(h) => row(
            "Held-out",
            "failed",
            format!("{} failed", names(&h["failedTests"])),
        ),
        None => row("Held-out", "ready", "not run".to_owned()),
    });
    out.push(match phase(statements, "baseline") {
        Some(b) => {
            let warned = names(&b["warnedTests"]);
            row(
                "Checks",
                if warned == 0 {
                    "verified"
                } else {
                    "regression_only"
                },
                if warned == 0 {
                    "baseline green, nothing regressed".to_owned()
                } else {
                    format!(
                        "{warned} failing before the attempt (not blamed on it), nothing regressed"
                    )
                },
            )
        }
        None => row("Checks", "ready", "no record".to_owned()),
    });
    if let Some(tv) = tv {
        let findings = names(&tv["integrity"]["findings"]);
        out.push(row(
            "Diff audit",
            if findings == 0 { "verified" } else { "failed" },
            if findings == 0 {
                "no tests weakened, no skips, no config changes".to_owned()
            } else {
                format!("{findings} finding(s)")
            },
        ));
        let monitor = tv["monitor"]["state"]
            .as_str()
            .or_else(|| tv["monitor"].as_str())
            .unwrap_or("off");
        out.push(row(
            "Gaming monitor",
            match monitor {
                "clean" => "verified",
                "suspected" => "needs_approval",
                _ => "ready",
            },
            monitor.to_owned(),
        ));
        let mutation = &tv["mutation"];
        out.push(if mutation.is_null() {
            row("Mutation", "ready", "not run".to_owned())
        } else {
            let caught = mutation["caught"].as_u64().unwrap_or(0);
            let missed = mutation["missed"].as_u64().unwrap_or(0);
            row(
                "Mutation",
                if missed == 0 {
                    "verified"
                } else {
                    "regression_only"
                },
                format!("{caught} caught, {missed} survived"),
            )
        });
        let decisions: Vec<String> = tv["decisions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| {
                format!(
                    "{} {} — {}{}",
                    d["verb"].as_str().unwrap_or("decided"),
                    d["grant"]
                        .as_str()
                        .or_else(|| d["tool"].as_str())
                        .unwrap_or_default(),
                    d["by"].as_str().unwrap_or("?"),
                    d["via"]
                        .as_str()
                        .map(|v| format!(" via {v}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        out.push(row(
            "Approvals",
            if decisions.is_empty() {
                "ready"
            } else {
                "verified"
            },
            if decisions.is_empty() {
                "none needed".to_owned()
            } else {
                decisions.join("; ")
            },
        ));
        let instructions: Vec<&str> = tv["instructions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i["path"].as_str())
            .collect();
        out.push(row(
            "Instructions",
            "ready",
            if instructions.is_empty() {
                "none".to_owned()
            } else {
                instructions.join(", ")
            },
        ));
        if let Some(by) = tv["decider"]["by"].as_str() {
            out.push(row(
                "Decided by",
                "attested",
                format!(
                    "{by} — {}",
                    tv["decider"]["reason"].as_str().unwrap_or_default()
                ),
            ));
        }
    }
    // Acceptance strength (F-060): how well the set tells right from wrong.
    if let Some(st) = tv
        .and_then(|tv| tv.get("strength"))
        .filter(|v| !v.is_null())
    {
        let share = st["combined"]["share"].as_f64();
        let rejected = st["permitted"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| p["outcome"] == "rejected");
        out.push(row(
            "Strength",
            match share {
                _ if rejected => "needs_approval",
                Some(x) if x >= 0.999 => "verified",
                Some(_) => "regression_only",
                None => "ready",
            },
            task.strength.clone().unwrap_or_else(|| {
                share.map_or_else(|| "not measured".to_owned(), |x| format!("{x:.2}"))
            }),
        ));
    } else if let Some(st) = &task.strength {
        out.push(row("Strength", "ready", st.clone()));
    }
    out.push(match task.attestation.as_deref() {
        Some(a) if a.starts_with("signed") => row("Attestation", "verified", a.to_owned()),
        Some(a) => row("Attestation", "regression_only", a.to_owned()),
        None => row(
            "Attestation",
            "ready",
            "none (not ticked by speccoder)".to_owned(),
        ),
    });
    if let Some(a) = &task.last_attempt {
        out.push(row(
            "Last attempt",
            a.outcome.as_str(),
            format!(
                "{} — {} ({})",
                a.outcome.replace('_', " "),
                a.reason,
                a.reference
            ),
        ));
    }
    if let Some(r) = run {
        for e in &r.earlier {
            out.push(row(
                "Earlier attempt",
                "failed",
                format!("{} — run {}", e.attempt_status.replace('_', " "), e.run_id),
            ));
        }
        if let Some(claim) = r.model_report.as_ref().and_then(|m| m["summary"].as_str()) {
            out.push(row(
                "Model says",
                "ready",
                format!("\"{claim}\" (a claim, not evidence)"),
            ));
        }
    }
    out
}

/// What the model said in each turn of `run` (F-095), read from the
/// project's journal: its account of the attempt — a claim, shown beside
/// the evidence, never as it.
///
/// # Errors
///
/// No project or state directory, or the journal cannot be read.
pub fn narration_of(path: &std::path::Path, run: &str) -> Result<Vec<String>, String> {
    let root = crate::format::at(path)
        .detect(path)
        .ok_or_else(|| "not a spec project".to_owned())?;
    let state = crate::worktree::state_dir().ok_or_else(|| "no state directory".to_owned())?;
    let plane = crate::worktree::project_state_dir(&state, &root);
    crate::pipeline::tokio_runtime()?
        .block_on(crate::substrate::narration(&plane, run))
        .map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(state: &str) -> TaskStatus {
        TaskStatus {
            id: "T002".to_owned(),
            title: "t".to_owned(),
            phase: "P".to_owned(),
            state: state.to_owned(),
            drift: None,
            blocked_by: Vec::new(),
            commit: Some("abc".to_owned()),
            run_id: None,
            last_attempt: None,
            note: None,
            attestation: Some("signed ssh SHA256:x".to_owned()),
            strength: None,
        }
    }

    fn phase_stmt(name: &str, result: &str, passed: &[&str], failed: &[&str]) -> Value {
        json!({ "predicateType": "https://in-toto.io/attestation/test-result/v0.1", "predicate": {
            "result": result, "configuration": [{ "name": format!("speccoder:phase/{name}") }],
            "passedTests": passed, "failedTests": failed, "warnedTests": [] } })
    }

    #[test]
    fn rows_come_from_the_statements() {
        let statements = vec![
            phase_stmt("baseline", "PASSED", &["a"], &[]),
            phase_stmt("red", "FAILED", &[], &["accept::login"]),
            phase_stmt("green", "PASSED", &["accept::login", "a"], &[]),
            json!({ "predicateType": "https://speccoder.dev/task-verification/v0", "predicate": {
                "integrity": { "findings": [] }, "monitor": { "state": "clean" },
                "mutation": { "caught": 26, "missed": 3 },
                "decisions": [{ "verb": "approve", "grant": "run `cargo add x`", "by": "Ada", "via": "dashboard" }],
                "instructions": [{ "path": "AGENTS.md" }] } }),
        ];
        let rows = rows(&task("verified"), &statements, None);
        let text = |label: &str| {
            rows.iter()
                .find(|r| r.label == label)
                .map(|r| r.text.clone())
                .unwrap_or_default()
        };
        assert!(
            text("Acceptance").contains("1 failing before, 2 passing after"),
            "{rows:?}"
        );
        assert_eq!(text("Held-out"), "not run");
        assert_eq!(text("Mutation"), "26 caught, 3 survived");
        assert!(text("Approvals").contains("Ada via dashboard"));
        assert_eq!(text("Instructions"), "AGENTS.md");
        assert!(text("Attestation").starts_with("signed"));
    }

    #[test]
    fn banners_never_blur() {
        assert_eq!(banner("verified").1, "VERIFIED");
        assert!(
            banner("regression_only")
                .1
                .contains("nothing task-specific")
        );
        assert!(banner("attested").1.contains("not evidence"));
        assert_ne!(banner("attested").0, banner("verified").0);
    }
}
