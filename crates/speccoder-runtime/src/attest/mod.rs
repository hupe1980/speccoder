//! Signed, portable evidence per ticked task.
//!
//! After a tick commit, the engine writes an in-toto Bundle as the commit's
//! git note (`refs/notes/speccoder`): a `test-result` statement per test
//! phase that ran, one SLSA verification summary and speccoder's
//! `task-verification`, each a DSSE envelope signed with the person's git
//! signing key. `accept` and `skip` write a `task-verification` alone. The
//! model never sees, writes or signs any of it.

pub mod dsse;
pub mod notes;
pub mod policy;
pub mod sign;
pub mod statement;
pub mod verify;

use std::path::Path;

use serde_json::{Value, json};
use speccoder_core::RunReport;

use self::dsse::Envelope;
use self::statement::{Phase, PhaseResult, Subject};
use crate::substrate::run_config::AttestConfig;

/// What a tick's attestation came to, for the run report.
#[must_use]
pub fn outcome(signed: bool, signer: Option<&str>, reason: Option<&str>) -> Value {
    json!({
        "state": if signed { "signed" } else { "unsigned" },
        "signer": signer,
        "reason": reason,
    })
}

/// Everything a tick's statements are made of besides the run report.
#[derive(Debug)]
pub struct Tick<'a> {
    /// The repository (a worktree or the checkout: refs are shared).
    pub repo: &'a Path,
    /// `speccoder.toml` as the attempt read it.
    pub config: &'a str,
    /// `[attest]`.
    pub attest: &'a AttestConfig,
    /// The spec format, e.g. `speckit`.
    pub format: &'a str,
    /// The unit (feature) and its directory.
    pub feature: &'a str,
    /// The unit's directory, relative to the project.
    pub feature_dir: &'a str,
    /// Task ID.
    pub task: &'a str,
    /// Task title.
    pub title: &'a str,
    /// The task line's hash before the tick.
    pub line_hash: &'a str,
    /// The requirement the task belongs to, e.g. `US1`.
    pub section: Option<&'a str>,
    /// That requirement's text, as the attempt was given it.
    pub section_text: &'a str,
    /// The unit's requirement identifiers with their line hashes (the
    /// format's; the trace names those the title cites).
    pub requirements: &'a [(String, String)],
    /// The tick commit.
    pub commit: &'a str,
    /// The verdict word.
    pub verdict: &'a str,
    /// `{ mode, backend, network }`.
    pub sandbox: Value,
    /// `{ kind, agent? }`.
    pub executor: Value,
    /// `{ provider, model }`, when one judged.
    pub model: Value,
}

/// Whether a task title cites requirement `id`, as a word of its own.
#[must_use]
pub fn cites(title: &str, id: &str) -> bool {
    title
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .any(|word| word == id)
}

/// The `trace[]` rows of a task's statement: each requirement its title
/// cites, with the defining line's hash; then each acceptance test with
/// the scenario it checks (none: the author cited none). All with the
/// verdict.
fn trace(t: &Tick<'_>, report: &RunReport) -> Vec<Value> {
    let mut rows: Vec<Value> = t
        .requirements
        .iter()
        .filter(|(id, _)| cites(t.title, id))
        .map(|(id, hash)| {
            json!({ "requirement": id, "requirement_hash": hash, "phase": "green", "result": t.verdict })
        })
        .collect();
    let tests = report
        .acceptance
        .as_ref()
        .and_then(|a| a["tests"].as_array())
        .into_iter()
        .flatten();
    for test in tests {
        let path = test["path"].as_str().unwrap_or_default();
        let cited: Vec<&str> = test["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let row = |scenario: Option<&str>| {
            json!({
                "requirement": t.section,
                "scenario": scenario,
                "test": path,
                "phase": "green",
                "result": t.verdict,
            })
        };
        if cited.is_empty() {
            rows.push(row(None));
        }
        rows.extend(cited.into_iter().map(|s| row(Some(s))));
    }
    rows
}

/// RFC 3339 UTC, to the second.
#[must_use]
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn sha(text: &str) -> String {
    crate::integrity::sha256_hex(text.as_bytes())
}

/// The test phases a run report shows.
#[must_use]
pub fn phases(report: &RunReport) -> Vec<PhaseResult> {
    let mut out = Vec::new();
    let checks_config = report
        .verification
        .as_ref()
        .map(|v| {
            v.checks
                .iter()
                .map(|c| c.argv.join(" "))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if let Some(cmp) = report
        .verification
        .as_ref()
        .and_then(|v| v.comparison.as_ref())
    {
        out.push(PhaseResult {
            phase: Phase::Baseline,
            result: if cmp.pre_existing.is_empty() {
                "PASSED"
            } else {
                "WARNED"
            },
            passed: Vec::new(),
            warned: cmp.pre_existing.clone(),
            failed: Vec::new(),
            config_sha256: sha(&checks_config),
        });
    }
    let acceptance = report.acceptance.as_ref();
    let acceptance_config = acceptance
        .map(|a| a["argv"].to_string())
        .unwrap_or_default();
    let acceptance_files: Vec<String> = acceptance
        .and_then(|a| a["files"].as_array())
        .map(|f| {
            f.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if acceptance.is_some_and(|a| a["red"] == true) {
        out.push(PhaseResult {
            phase: Phase::Red,
            result: "FAILED",
            passed: Vec::new(),
            warned: Vec::new(),
            failed: acceptance_files.clone(),
            config_sha256: sha(&acceptance_config),
        });
    }
    if let Some(v) = &report.verification {
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        for c in &v.checks {
            match &c.tests {
                Some(tests) => {
                    for t in tests {
                        let id = format!("{}::{}", c.name, t.id);
                        match t.outcome {
                            speccoder_core::verify::TestOutcome::Passed => passed.push(id),
                            speccoder_core::verify::TestOutcome::Failed
                            | speccoder_core::verify::TestOutcome::Error => failed.push(id),
                            speccoder_core::verify::TestOutcome::Skipped => {}
                        }
                    }
                }
                None if c.status == speccoder_core::verify::CheckStatus::Passed => {
                    passed.push(c.name.clone());
                }
                None => failed.push(c.name.clone()),
            }
        }
        passed.extend(acceptance_files);
        out.push(PhaseResult {
            phase: Phase::Green,
            result: if v.passed { "PASSED" } else { "FAILED" },
            passed,
            warned: Vec::new(),
            failed,
            config_sha256: sha(&format!("{checks_config}\n{acceptance_config}")),
        });
    }
    if let Some(h) = acceptance
        .and_then(|a| a.get("held_out"))
        .filter(|h| h["state"] == "on")
    {
        out.push(PhaseResult {
            phase: Phase::HeldOut,
            result: "PASSED",
            passed: Vec::new(),
            warned: Vec::new(),
            failed: Vec::new(),
            config_sha256: h["manifest"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches("sha256:")
                .to_owned(),
        });
    }
    out
}

/// The decisions people made on the attempt's calls.
fn decisions(tool_calls: &[Value]) -> Vec<Value> {
    tool_calls
        .iter()
        .filter(|c| c["by"] == "person" || c["by"] == "expiry")
        .map(|c| {
            json!({
                "verb": if c["approved_by"] == "person" { "approve" } else if c["expired"] == true { "expired" } else { "reject" },
                "grant": serde_json::from_value::<crate::tools::policy::Object>(c["object"].clone()).ok().map(|o| o.describe()),
                "scope": c["scope"],
                "reason": c["rejection"],
                "by": c["operator"]["actor"],
                "basis": c["operator"]["basis"],
                // Through what: the decision's own record (`approver`).
                "via": if c["via"].is_null() { &c["approver"]["via"] } else { &c["via"] },
            })
        })
        .collect()
}

/// The acceptance set's strength as evidence: shares, counts,
/// what got through, the permitted alternatives and the rulings — never the
/// local patch paths or the variants' code.
fn strength(report: &RunReport) -> Value {
    let Some(a) = &report.strength else {
        return Value::Null;
    };
    json!({
        "visible": a.visible,
        "combined": a.combined,
        "held_out": a.held_out,
        "gate": a.gate,
        "near_misses": a.near_misses.iter().map(|n| json!({ "id": n.id, "scenario": n.scenario, "visible": n.visible, "combined": n.combined })).collect::<Vec<_>>(),
        "permitted": a.permitted.iter().map(|p| json!({ "id": p.id, "scenario": p.scenario, "digest": p.digest, "outcome": p.outcome })).collect::<Vec<_>>(),
        "rulings": a.rulings,
    })
}

/// The monitor's result, `unreviewed` when it did not finish, `off` when
/// not configured.
fn monitor(report: &RunReport) -> Value {
    match &report.monitor {
        None => json!({ "state": "off", "findings": [] }),
        Some(m) if m.get("error").is_some() => json!({ "state": "unreviewed", "findings": [] }),
        Some(m) => json!({
            "state": if m["suspected"] == true { "suspected" } else { "clean" },
            "findings": m.get("findings").cloned().unwrap_or_else(|| json!([])),
        }),
    }
}

/// `git+<origin without credentials>@<commit>`, or `git:<commit>`.
fn resource_uri(repo: &Path, commit: &str) -> String {
    let origin = crate::git::Git::at(repo)
        .line(&["remote", "get-url", "origin"])
        .ok()
        .filter(|u| !u.is_empty());
    match origin {
        Some(url) => {
            // Credentials in a remote URL never leave the machine.
            let clean = match url.split_once("://") {
                Some((scheme, rest)) => {
                    let host_end = rest.find('/').unwrap_or(rest.len());
                    match rest[..host_end].rfind('@') {
                        Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
                        None => url.clone(),
                    }
                }
                None => url.clone(),
            };
            format!("git+{clean}@{commit}")
        }
        None => format!("git:{commit}"),
    }
}

/// What `object`'s note says, signatures unchecked: `signed <kind>
/// <keyid>`, `unsigned` or `missing`.
#[must_use]
pub fn note_state(repo: &Path, object: &str) -> String {
    use base64::Engine as _;
    let Some(note) = notes::read(repo, object) else {
        return "missing".to_owned();
    };
    let (lines, _) = dsse::decode(&note);
    if lines.is_empty() {
        return "missing".to_owned();
    }
    let Some(sig) = lines
        .iter()
        .find_map(|l| l.signatures.first().map(|s| (s, l)))
    else {
        return "unsigned".to_owned();
    };
    let armored = base64::engine::general_purpose::STANDARD
        .decode(&sig.0.sig)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let kind = if sig.1.sigstore.is_some() {
        "sigstore"
    } else if armored.contains("SSH SIGNATURE") {
        "ssh"
    } else {
        "openpgp"
    };
    format!("signed {kind} {}", sig.0.keyid)
}

/// Signs every envelope that is not signed yet; the first failure's reason
/// when some stay unsigned.
pub fn sign_all(lines: &mut [Envelope], signer: &sign::Signer) -> Option<String> {
    let mut failure = None;
    // Keyless Sigstore signs the summary alone — it names every other line
    // by digest — so a tick costs one identity login, not one per line.
    let summary_only = matches!(signer, sign::Signer::Sigstore)
        && lines.iter().any(|l| {
            l.statement()
                .is_some_and(|s| s["predicateType"] == statement::VSA)
        });
    for line in lines.iter_mut().filter(|l| l.signatures.is_empty()) {
        if summary_only
            && !line
                .statement()
                .is_some_and(|s| s["predicateType"] == statement::VSA)
        {
            continue;
        }
        let Some(bytes) = line.signed_bytes() else {
            continue;
        };
        match sign::sign(signer, &bytes) {
            Ok((sig, material)) => {
                line.signatures.push(sig);
                line.sigstore = material;
            }
            Err(e) => {
                failure.get_or_insert(e);
            }
        }
    }
    failure
}

/// Writes `lines` as `commit`'s note, signed as `[attest]` says; what it
/// came to (`signed`/`unsigned`, signer, reason).
fn write(repo: &Path, commit: &str, attest: &AttestConfig, mut lines: Vec<Envelope>) -> Value {
    let (signed, signer, reason) = if attest.sign_later {
        (
            false,
            None,
            Some("[attest] sign_at = \"later\": `speccoder attest sign`".to_owned()),
        )
    } else {
        match sign::signer(repo, attest) {
            Ok(s) => match sign_all(&mut lines, &s) {
                None => (true, Some(s.kind()), None),
                Some(e) => (false, Some(s.kind()), Some(e)),
            },
            Err(e) => (false, None, Some(e)),
        }
    };
    if let Err(e) = notes::write(repo, commit, &dsse::encode(&lines)) {
        return json!({ "state": "missing", "reason": format!("writing the note failed: {e}") });
    }
    outcome(signed, signer, reason.as_deref())
}

/// Records a passed tick: `test-result` per phase, `task-verification`,
/// then the `vsa` naming every other line. Never fails the tick.
#[must_use]
pub fn record_tick(t: &Tick<'_>, report: &RunReport) -> Value {
    let subject = Subject {
        name: format!("{}/{}", t.feature, t.task),
        commit: t.commit.to_owned(),
    };
    let tree = crate::git::Git::at(t.repo)
        .line(&["rev-parse", &format!("{}^{{tree}}", t.commit)])
        .unwrap_or_default();
    let policy = policy::judge_config_sha256(t.config).unwrap_or_default();
    let time = now_rfc3339();
    let mut named: Vec<(String, Value)> = phases(report)
        .iter()
        .map(|p| {
            (
                format!("test-result/{}", p.phase.name()),
                statement::statement(&subject, statement::TEST_RESULT, statement::test_result(p)),
            )
        })
        .collect();
    let verification = json!({
        "verdict": t.verdict,
        "unit": { "format": t.format, "feature": t.feature, "dir": t.feature_dir },
        "task": { "id": t.task, "line_hash": t.line_hash, "title": t.title },
        "tree": tree,
        "spec": { "section": t.section, "section_hash": sha(t.section_text) },
        "trace": trace(t, report),
        "integrity": { "findings": report.integrity },
        "monitor": monitor(report),
        "mutation": report.mutation,
        "sandbox": t.sandbox,
        "executor": t.executor,
        "model": t.model,
        "run": report.run_id,
        "decisions": decisions(&report.tool_calls),
        "strength": strength(report),
        "instructions": report.instructions,
        "policy_sha256": policy,
        "time": time,
    });
    named.push((
        "task-verification".to_owned(),
        statement::statement(&subject, statement::TASK_VERIFICATION, verification),
    ));
    let mut lines = Vec::new();
    let mut inputs = Vec::new();
    for (name, s) in &named {
        if let Err(why) = statement::check_clean(s) {
            return json!({ "state": "missing", "reason": format!("a statement would carry {why}; none was written") });
        }
        let bytes = statement::canonical(s);
        inputs.push((name.clone(), crate::integrity::sha256_hex(&bytes)));
        lines.push(Envelope::unsigned(&bytes));
    }
    let summary = statement::statement(
        &subject,
        statement::VSA,
        statement::vsa(&statement::Summary {
            speccoder: env!("CARGO_PKG_VERSION").to_owned(),
            agentplane: crate::substrate::AGENTPLANE_VERSION.to_owned(),
            time,
            resource_uri: resource_uri(t.repo, t.commit),
            policy_sha256: policy,
            inputs,
            verdict: t.verdict.to_owned(),
        }),
    );
    if let Err(why) = statement::check_clean(&summary) {
        return json!({ "state": "missing", "reason": format!("a statement would carry {why}; none was written") });
    }
    lines.push(Envelope::unsigned(&statement::canonical(&summary)));
    write(t.repo, t.commit, t.attest, lines)
}

/// Moves `from`'s statements to `to`, the commit that carries the same
/// task's work onto the feature branch (a parallel lane's commit,
/// cherry-picked): [`reissue`], then `from` loses its note.
#[must_use]
pub fn carry(repo: &Path, from: &str, to: &str, attest: &AttestConfig) -> Value {
    if from == to {
        // The lane started at the branch tip: the pick is the same commit.
        return match note_state(repo, to).split_whitespace().collect::<Vec<_>>()[..] {
            ["signed", kind, ..] => outcome(true, Some(kind), None),
            [state, ..] => json!({ "state": state }),
            [] => json!({ "state": "missing" }),
        };
    }
    let out = reissue(repo, from, to, attest);
    if out["state"] != "missing" {
        notes::remove(repo, from);
    }
    out
}

/// Re-issues `from`'s statements for `to`, a commit with the same task's
/// work: the subject, tree and resource move to `to`, the summary names
/// the new lines, and everything is signed afresh. What the evidence says
/// is unchanged.
#[must_use]
pub fn reissue(repo: &Path, from: &str, to: &str, attest: &AttestConfig) -> Value {
    let Some(note) = notes::read(repo, from) else {
        return json!({ "state": "missing", "reason": "the lane's commit had no attestation" });
    };
    let (envelopes, unreadable) = dsse::decode(&note);
    let statements: Option<Vec<Value>> = envelopes.iter().map(Envelope::statement).collect();
    let Some(mut statements) = statements.filter(|_| unreadable == 0) else {
        return json!({ "state": "missing", "reason": "the lane's attestation was unreadable" });
    };
    let tree = crate::git::Git::at(repo)
        .line(&["rev-parse", &format!("{to}^{{tree}}")])
        .unwrap_or_default();
    let mut renamed = std::collections::BTreeMap::new();
    let mut lines = Vec::new();
    for s in statements
        .iter_mut()
        .filter(|s| s["predicateType"] != statement::VSA)
    {
        let old = crate::integrity::sha256_hex(&statement::canonical(s));
        s["subject"][0]["digest"]["gitCommit"] = json!(to);
        if s["predicateType"] == statement::TASK_VERIFICATION {
            s["predicate"]["tree"] = json!(tree);
        }
        let bytes = statement::canonical(s);
        renamed.insert(old, crate::integrity::sha256_hex(&bytes));
        lines.push(Envelope::unsigned(&bytes));
    }
    for s in statements
        .iter_mut()
        .filter(|s| s["predicateType"] == statement::VSA)
    {
        s["subject"][0]["digest"]["gitCommit"] = json!(to);
        s["predicate"]["resourceUri"] = json!(resource_uri(repo, to));
        if let Some(inputs) = s["predicate"]["inputAttestations"].as_array_mut() {
            for input in inputs {
                let old = input["digest"]["sha256"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if let Some(new) = renamed.get(&old) {
                    input["digest"]["sha256"] = json!(new);
                }
            }
        }
        lines.push(Envelope::unsigned(&statement::canonical(s)));
    }
    write(repo, to, attest, lines)
}

/// Records a person's decision on a task — `attested` (accept) or
/// `skipped` (skip) — as one `task-verification`, no summary: a decision
/// is not a verification.
#[must_use]
pub fn record_decision(t: &Tick<'_>, by: &str, reason: &str) -> Value {
    let subject = Subject {
        name: format!("{}/{}", t.feature, t.task),
        commit: t.commit.to_owned(),
    };
    let tree = crate::git::Git::at(t.repo)
        .line(&["rev-parse", &format!("{}^{{tree}}", t.commit)])
        .unwrap_or_default();
    let s = statement::statement(
        &subject,
        statement::TASK_VERIFICATION,
        json!({
            "verdict": t.verdict,
            "unit": { "format": t.format, "feature": t.feature, "dir": t.feature_dir },
            "task": { "id": t.task, "line_hash": t.line_hash, "title": t.title },
            "tree": tree,
            "decider": { "by": by, "reason": reason },
            "policy_sha256": policy::judge_config_sha256(t.config).unwrap_or_default(),
            "time": now_rfc3339(),
        }),
    );
    if let Err(why) = statement::check_clean(&s) {
        return json!({ "state": "missing", "reason": format!("the statement would carry {why}; none was written") });
    }
    write(
        t.repo,
        t.commit,
        t.attest,
        vec![Envelope::unsigned(&statement::canonical(&s))],
    )
}

/// `speccoder attest sign`: signs the unsigned speccoder statements of the
/// selected task objects with the configured signer, payloads unchanged.
#[must_use]
pub fn sign_at(
    path: &Path,
    feature: Option<&str>,
    rev: Option<&str>,
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus};
    let inspected = crate::inspect(path, feature);
    let Some(speccoder_core::CommandData::Inspect(snap)) = inspected.data() else {
        return CommandResult::carried("attest", &inspected);
    };
    let Some(f) = snap.feature.as_ref() else {
        return CommandResult::new(
            "attest",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        );
    };
    let root = std::path::PathBuf::from(&snap.root);
    let config = crate::verify::find_config(&root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let attest = match crate::substrate::run_config::parse(&config) {
        Ok(c) => c.attest,
        Err(e) => return CommandResult::new("attest", CommandStatus::ConfigError, &e),
    };
    let signer = match sign::signer(&root, &attest) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::new(
                "attest",
                CommandStatus::ConfigError,
                &format!("cannot sign: {e}"),
            );
        }
    };
    let targets = match verify::targets(&root, &f.feature.name, rev) {
        Ok(t) => t,
        Err(e) => return CommandResult::new("attest", CommandStatus::GitPrecondition, &e),
    };
    let (mut signed, mut objects, mut failures) = (0usize, 0usize, Vec::new());
    for t in &targets {
        let Some(note) = notes::read(&root, &t.object) else {
            continue;
        };
        let (mut lines, unreadable) = dsse::decode(&note);
        let before = lines.iter().filter(|l| l.signatures.is_empty()).count();
        if before == 0 || unreadable > 0 {
            continue;
        }
        if let Some(e) = sign_all(&mut lines, &signer) {
            failures.push(format!("{}: {e}", t.task));
        }
        let after = lines.iter().filter(|l| l.signatures.is_empty()).count();
        if after < before {
            if let Err(e) = notes::write(&root, &t.object, &dsse::encode(&lines)) {
                failures.push(format!("{}: {e}", t.task));
                continue;
            }
            signed += before - after;
            objects += 1;
        }
    }
    let mut message = format!(
        "signed {signed} statement(s) on {objects} task object(s) with {}",
        signer.kind()
    );
    if !failures.is_empty() {
        message.push_str(&format!("; not signed: {}", failures.join("; ")));
    }
    CommandResult::new(
        "attest",
        if failures.is_empty() {
            CommandStatus::Ok
        } else {
            CommandStatus::Error
        },
        &message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_rfc3339_utc() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T', "{t}");
        assert!(t.starts_with("20"), "{t}");
    }

    #[test]
    fn a_decision_names_who_and_through_what() {
        let calls = vec![json!({
            "tool": "write_file",
            "arguments": { "path": "package.json", "content": "{}" },
            "by": "person",
            "approved_by": "person",
            "scope": "once",
            "approver": { "name": "Dana", "via": "dashboard", "basis": "asserted" },
            "operator": { "actor": "Dana", "basis": "asserted" },
        })];
        let d = &decisions(&calls)[0];
        assert_eq!(d["verb"], "approve");
        assert_eq!(d["by"], "Dana");
        assert_eq!(d["via"], "dashboard");
    }
}
