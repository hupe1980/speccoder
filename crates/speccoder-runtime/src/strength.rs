//! Measuring the acceptance set's strength (F-060, D-085): wrong and
//! permitted candidates of a verified task's change, run against its
//! acceptance tests.
//!
//! Every candidate is written into the task's worktree under a checkpoint —
//! as held-out tests are overlaid — run with the visible acceptance command
//! (and, for the combined set, each held-out file overlaid), and the tree is
//! put back. Afterwards the tree and the judge inputs must be exactly as
//! before, or the measurement is void. The rules are
//! `speccoder_core::strength`'s; this module only runs things.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use speccoder_core::strength::{
    self as rules, AcceptanceStrength, Gate, Mutants, NearMiss, Outcome, Permitted,
    PermittedOutcome, Ruling, Set, State,
};
use speccoder_core::verify::{CheckResult, CheckStatus, RedClass, RunnerSummary, classify_red};

use crate::held_out::HeldOutFile;
use crate::integrity::{JudgeSnapshot, sha256_hex};
use crate::substrate::run_config::StrengthConfig;
use crate::tools::policy::Policy;
use crate::{mutation, sandbox, worktree};

/// Where a task's variants, findings and rulings are kept.
#[must_use]
pub fn dir(plane_dir: &Path, feature: &str, task: &str) -> PathBuf {
    plane_dir.join("strength").join(feature).join(task)
}

/// One candidate: files that replace changed source files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// `wrong` or `permitted`.
    pub kind: String,
    /// The scenario it concerns.
    pub scenario: String,
    /// What it gets wrong, or the freedom it uses.
    pub rationale: String,
    /// `(path relative to the worktree, content)`.
    pub files: Vec<(String, String)>,
}

impl Variant {
    /// `sha256:<hex>` over its files: what a ruling is bound to.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut text = String::new();
        for (p, c) in &self.files {
            text.push_str(&format!("{p}\n{}\n{c}\n", c.len()));
        }
        format!("sha256:{}", sha256_hex(text.as_bytes()))
    }
}

/// The author's answer, validated: only the changed source files, each a
/// real change, no duplicates, at most `near_miss` wrong and `permitted`
/// permitted. What was dropped and why goes to `notes`.
#[must_use]
pub fn validate(
    answer: &Value,
    changed: &[(String, String)],
    config: &StrengthConfig,
    notes: &mut Vec<String>,
) -> (Vec<Variant>, Vec<Variant>) {
    let mut wrong = Vec::new();
    let mut permitted = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (i, v) in answer["variants"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let kind = v["kind"].as_str().unwrap_or_default().to_owned();
        let mut files = Vec::new();
        let mut bad = None;
        for f in v["files"].as_array().into_iter().flatten() {
            let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
                bad = Some("a file without path or content".to_owned());
                break;
            };
            let path = match crate::tools::paths::normalise(path) {
                Ok(p) => p,
                Err(e) => {
                    bad = Some(format!("`{path}`: {}", e.0));
                    break;
                }
            };
            match changed.iter().find(|(p, _)| *p == path) {
                None => {
                    bad = Some(format!("`{path}` is not a changed source file"));
                    break;
                }
                Some((_, now)) if now == content => {}
                Some(_) => files.push((path, content.to_owned())),
            }
        }
        if bad.is_none() && files.is_empty() {
            bad = Some("identical to the change".to_owned());
        }
        let variant = Variant {
            kind: kind.clone(),
            scenario: v["scenario"].as_str().unwrap_or_default().to_owned(),
            rationale: v["rationale"].as_str().unwrap_or_default().to_owned(),
            files,
        };
        if bad.is_none() && !seen.insert(variant.digest()) {
            bad = Some("the same as an earlier variant".to_owned());
        }
        if let Some(why) = bad {
            notes.push(format!("variant {} dropped: {why}", i + 1));
            continue;
        }
        match kind.as_str() {
            "wrong" if wrong.len() < config.near_miss as usize => wrong.push(variant),
            "permitted" if permitted.len() < config.permitted as usize => permitted.push(variant),
            _ => notes.push(format!(
                "variant {} dropped: more {kind} variants than asked for",
                i + 1
            )),
        }
    }
    (wrong, permitted)
}

/// What one acceptance run says about a candidate.
#[must_use]
pub fn classify(result: &CheckResult, summary: Option<&RunnerSummary>) -> Outcome {
    if result.status == CheckStatus::Passed {
        return Outcome::Survived;
    }
    match classify_red(result, summary) {
        RedClass::Assertion | RedClass::Absent => Outcome::Rejected,
        RedClass::Timeout => Outcome::Timeout,
        // A build or collection error, nothing collected, or output not
        // recognised: not judged (never counted as strong).
        _ => Outcome::Unviable,
    }
}

/// How a candidate fared against the visible and the combined set, and the
/// failing run's tail.
#[derive(Debug, Clone)]
struct Ran {
    visible: Outcome,
    combined: Outcome,
    failing: Option<String>,
}

fn tail(r: &CheckResult) -> String {
    let text = if r.stdout_tail.trim().is_empty() {
        &r.stderr_tail
    } else {
        &r.stdout_tail
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

/// What measuring needs.
pub struct Setup<'a> {
    /// The task's worktree.
    pub wt_root: &'a Path,
    /// The project inside it.
    pub project: &'a Path,
    /// Scratch for checkpoints.
    pub tmp: &'a Path,
    /// Sandboxed commands.
    pub runner: &'a sandbox::Runner,
    /// The judge's inputs, checked unchanged afterwards.
    pub policy: &'a Policy,
    /// The visible acceptance commands (the first is the original set's;
    /// mutants run with it).
    pub acceptance: &'a [Vec<String>],
    /// The held-out files, verified, with their contents.
    pub held_out: &'a [(HeldOutFile, String)],
    /// `[mutation]`, when configured.
    pub mutation: Option<&'a mutation::Config>,
    /// The attempt's change.
    pub patch: &'a str,
    /// `[acceptance]` strength settings.
    pub config: &'a StrengthConfig,
    /// Where variants and findings are kept.
    pub keep: &'a Path,
    /// Rulings a person made on this task.
    pub rulings: &'a [Ruling],
}

/// Writes `files` into the worktree.
fn apply(root: &Path, files: &[(String, String)]) -> Result<(), String> {
    for (p, c) in files {
        std::fs::write(root.join(p), c).map_err(|e| format!("{p}: {e}"))?;
    }
    Ok(())
}

/// Runs the acceptance set against the tree as it is now.
fn judge_tree(s: &Setup<'_>, assertion_only: bool) -> Result<Ran, String> {
    // Every visible command; the gravest outcome stands.
    let rank = |o: Outcome| match o {
        Outcome::Rejected => 3,
        Outcome::Timeout => 2,
        Outcome::Unviable => 1,
        Outcome::Survived => 0,
    };
    let mut visible = Outcome::Survived;
    let mut failing = None;
    for argv in s.acceptance {
        let (r, summary) = crate::pipeline::acceptance_run(s.project, argv, s.runner);
        let o = classify(&r, summary.as_ref());
        if rank(o) > rank(visible) {
            visible = o;
            failing = Some(tail(&r));
        }
    }
    let mut combined = visible;
    // The held-out set only matters when the visible one let it through.
    if visible == Outcome::Survived && !s.held_out.is_empty() {
        let runs = crate::held_out::overlay_run(
            s.wt_root, s.project, s.tmp, s.held_out, s.runner, s.policy,
        )?;
        for ran in runs.iter().filter_map(|r| r.result.as_ref()) {
            let o = classify(&ran.0, ran.1.as_ref());
            if o == Outcome::Rejected || (o == Outcome::Timeout && !assertion_only) {
                combined = o;
                failing = Some(tail(&ran.0));
                break;
            }
        }
    }
    Ok(Ran {
        visible,
        combined,
        failing,
    })
}

/// A unified diff of a variant against the change, for a person.
fn variant_patch(changed: &[(String, String)], v: &Variant) -> String {
    let mut out = format!("# {} — {}: {}\n", v.kind, v.scenario, v.rationale);
    for (path, content) in &v.files {
        let now = changed
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, c)| c.as_str())
            .unwrap_or_default();
        out.push_str(
            &similar::TextDiff::from_lines(now, content)
                .unified_diff()
                .header(&format!("a/{path}"), &format!("b/{path}"))
                .to_string(),
        );
    }
    out
}

/// The changed source files with their final contents.
#[must_use]
pub fn changed_files(wt_root: &Path, patch: &str) -> Vec<(String, String)> {
    mutation::changed_sources(patch)
        .into_iter()
        .filter_map(|p| {
            std::fs::read_to_string(wt_root.join(&p))
                .ok()
                .map(|c| (p, c))
        })
        .collect()
}

/// What [`measure`] needs from the author.
pub struct Authored {
    /// The answer (`{variants, notes}`), or why there is none.
    pub answer: Result<Value, String>,
    /// The author's run.
    pub run_id: Option<String>,
    /// Its tokens.
    pub usage: Option<Value>,
}

/// Measures the set. `author` is asked for variants (after the mutants, so a
/// slow tool leaves the call for last only when there is budget). `Err`: the
/// tree or the judge inputs did not come back as they were, and the
/// measurement is void.
pub fn measure(
    s: &Setup<'_>,
    author: impl FnOnce(&[(String, String)]) -> Authored,
) -> Result<AcceptanceStrength, String> {
    let began = Instant::now();
    let deadline = began + s.config.timeout;
    let changed = changed_files(s.wt_root, s.patch);
    let held = !s.held_out.is_empty();
    let mut notes = Vec::new();
    let done =
        |visible, combined, mutants, near_misses, permitted, rulings, notes, run_id, usage| {
            AcceptanceStrength {
                visible,
                combined,
                held_out: held,
                mutants,
                near_misses,
                permitted,
                rulings,
                gate: Gate::Off,
                notes,
                run_id,
                usage,
                duration_ms: u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
            }
        };
    if changed.is_empty() {
        return Ok(done(
            rules::unmeasured(State::NotApplicable),
            rules::unmeasured(State::NotApplicable),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec!["no source change".to_owned()],
            None,
            None,
        ));
    }
    let base = worktree::checkpoint(s.wt_root, s.tmp).map_err(|e| e.0)?;
    let judged = JudgeSnapshot::take(s.wt_root, s.policy);
    let mut partial = false;

    // 1. Mutants of the changed code, judged by the acceptance command alone.
    let mutants = match s.mutation {
        None => {
            notes.push("mutants not measured: no [mutation] tool".to_owned());
            None
        }
        Some(m) => {
            let mut m = m.clone();
            m.timeout = m
                .timeout
                .min(deadline.saturating_duration_since(Instant::now()));
            let scratch = s.runner.scratch().unwrap_or_else(|| s.tmp.to_path_buf());
            let first = s.acceptance.first().map(Vec::as_slice).unwrap_or_default();
            match mutation::run_against(&m, s.project, s.patch, &scratch, s.runner, first) {
                Some(r) if r.get("error").is_none() => {
                    let n = |k: &str| u32::try_from(r[k].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    Some(Mutants {
                        tool: r["tool"].as_str().unwrap_or_default().to_owned(),
                        caught: n("caught"),
                        missed: n("missed"),
                        timeout: n("timeout"),
                        unviable: n("unviable"),
                        survivors: r["survivors"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect(),
                    })
                }
                Some(r) => {
                    notes.push(format!(
                        "mutants not measured: {}",
                        r["error"].as_str().unwrap_or("the tool failed")
                    ));
                    None
                }
                None => None,
            }
        }
    };

    // 2. The author's variants.
    let (mut run_id, mut usage) = (None, None);
    let (wrong, allowed) = if s.config.near_miss + s.config.permitted == 0 {
        (Vec::new(), Vec::new())
    } else if Instant::now() >= deadline {
        partial = true;
        notes.push("near misses not measured: the strength budget ran out".to_owned());
        (Vec::new(), Vec::new())
    } else {
        let a = author(&changed);
        run_id = a.run_id;
        usage = a.usage;
        match a.answer {
            Ok(answer) => validate(&answer, &changed, s.config, &mut notes),
            Err(e) => {
                notes.push(format!("near misses not measured: {e}"));
                (Vec::new(), Vec::new())
            }
        }
    };
    let _ = std::fs::create_dir_all(s.keep);

    // 3. Each variant, under the checkpoint.
    let mut run_one = |v: &Variant, assertion_only: bool| -> Result<Option<Ran>, String> {
        if Instant::now() >= deadline {
            partial = true;
            return Ok(None);
        }
        let ran = apply(s.wt_root, &v.files).and_then(|()| judge_tree(s, assertion_only));
        worktree::restore(s.wt_root, s.tmp, &base).map_err(|e| e.0)?;
        ran.map(Some)
    };
    let mut near_misses = Vec::new();
    for (i, v) in wrong.iter().enumerate() {
        let Some(ran) = run_one(v, false)? else { break };
        near_misses.push(NearMiss {
            id: format!("near-miss-{}", i + 1),
            scenario: v.scenario.clone(),
            rationale: v.rationale.clone(),
            visible: ran.visible,
            combined: ran.combined,
        });
        if ran.combined == Outcome::Survived {
            let _ = std::fs::write(
                s.keep.join(format!("near-miss-{}.patch", i + 1)),
                variant_patch(&changed, v),
            );
        }
    }
    let mut permitted = Vec::new();
    let mut rulings = Vec::new();
    for (i, v) in allowed.iter().enumerate() {
        let Some(ran) = run_one(v, true)? else { break };
        let digest = v.digest();
        let id = format!("permitted-{}", i + 1);
        let mut outcome = match ran.combined {
            Outcome::Survived => PermittedOutcome::Accepted,
            Outcome::Rejected => PermittedOutcome::Rejected,
            Outcome::Timeout => PermittedOutcome::Timeout,
            Outcome::Unviable => PermittedOutcome::Unviable,
        };
        let mut patch = None;
        if outcome == PermittedOutcome::Rejected {
            if let Some(r) = s.rulings.iter().find(|r| r.digest == digest) {
                outcome = PermittedOutcome::Ruled;
                rulings.push(r.clone());
            } else {
                let path = s.keep.join(format!("{id}.patch"));
                let _ = std::fs::write(&path, variant_patch(&changed, v));
                patch = Some(path.to_string_lossy().into_owned());
            }
        }
        permitted.push(Permitted {
            id,
            scenario: v.scenario.clone(),
            rationale: v.rationale.clone(),
            digest,
            outcome,
            failing: matches!(
                outcome,
                PermittedOutcome::Rejected | PermittedOutcome::Ruled
            )
            .then_some(ran.failing)
            .flatten(),
            patch,
        });
    }
    if partial {
        notes.push("the strength budget ran out: partial".to_owned());
    }

    // 4. The tree and the judge's inputs are exactly as before.
    let after = worktree::checkpoint(s.wt_root, s.tmp).map_err(|e| e.0)?;
    if after != base {
        return Err("measuring acceptance strength left changes in the worktree".to_owned());
    }
    if let Some(d) = judged
        .drift(
            &JudgeSnapshot::take(s.wt_root, s.policy),
            s.policy,
            &std::collections::BTreeSet::new(),
        )
        .first()
    {
        return Err(format!(
            "measuring acceptance strength changed judge input {}",
            d.path
        ));
    }
    let visible = rules::strength(mutants.as_ref(), &near_misses, Set::Visible, partial);
    let combined = rules::strength(mutants.as_ref(), &near_misses, Set::Combined, partial);
    Ok(done(
        visible,
        combined,
        mutants,
        near_misses,
        permitted,
        rulings,
        notes,
        run_id,
        usage,
    ))
}

/// The input part `weak` for a regeneration: what got through, described
/// to the test author (never to the implementer).
#[must_use]
pub fn weak(a: &AcceptanceStrength) -> Value {
    let mut out: Vec<Value> = a
        .near_misses
        .iter()
        .filter(|n| n.combined == Outcome::Survived)
        .map(|n| json!({ "source": "near_miss", "scenario": n.scenario, "wrong": n.rationale }))
        .collect();
    if let Some(m) = &a.mutants {
        out.extend(
            m.survivors
                .iter()
                .take(12)
                .map(|s| json!({ "source": "mutant", "wrong": s })),
        );
    }
    json!(out)
}

/// A ruling a person made (`task rule --tests-right`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rulings {
    /// Every ruling, oldest first.
    pub rulings: Vec<Ruling>,
}

const RULINGS: &str = "rulings.json";
const HELD: &str = "held.json";

/// The task's rulings.
#[must_use]
pub fn rulings(keep: &Path) -> Vec<Ruling> {
    std::fs::read_to_string(keep.join(RULINGS))
        .ok()
        .and_then(|t| serde_json::from_str::<Rulings>(&t).ok())
        .map(|r| r.rulings)
        .unwrap_or_default()
}

/// Records `ruling`.
pub fn add_ruling(keep: &Path, ruling: Ruling) -> std::io::Result<()> {
    let mut all = rulings(keep);
    all.retain(|r| r.digest != ruling.digest);
    all.push(ruling);
    std::fs::create_dir_all(keep)?;
    std::fs::write(
        keep.join(RULINGS),
        serde_json::to_vec_pretty(&Rulings { rulings: all }).map_err(std::io::Error::other)?,
    )
}

/// An attempt held for a ruling: what a person decides on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    /// The flagged alternatives.
    pub findings: Vec<Permitted>,
}

/// Holds the attempt for a person.
pub fn hold(keep: &Path, findings: &[&Permitted]) -> std::io::Result<()> {
    std::fs::create_dir_all(keep)?;
    std::fs::write(
        keep.join(HELD),
        serde_json::to_vec_pretty(&Held {
            findings: findings.iter().map(|p| (*p).clone()).collect(),
        })
        .map_err(std::io::Error::other)?,
    )
}

/// What a held attempt waits on, if it is held.
#[must_use]
pub fn held(keep: &Path) -> Option<Held> {
    serde_json::from_str(&std::fs::read_to_string(keep.join(HELD)).ok()?).ok()
}

/// Releases a held attempt (ticked, set aside, or re-judged).
pub fn release(keep: &Path) {
    let _ = std::fs::remove_file(keep.join(HELD));
}

/// Another task of `feature` whose attempt is held for a ruling: it holds
/// the worktree, so `task` must wait.
#[must_use]
pub fn held_elsewhere(plane_dir: &Path, feature: &str, task: &str) -> Option<String> {
    std::fs::read_dir(plane_dir.join("strength").join(feature))
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|t| t != task)
        .find(|t| held(&dir(plane_dir, feature, t)).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed() -> Vec<(String, String)> {
        vec![(
            "src/lib.rs".to_owned(),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n".to_owned(),
        )]
    }

    #[test]
    fn only_real_changes_of_changed_files_are_kept() {
        let answer = json!({ "variants": [
            { "kind": "wrong", "scenario": "US1-1", "rationale": "off by one", "files": [{ "path": "src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { a + b + 1 }\n" }] },
            { "kind": "wrong", "scenario": "US1-1", "rationale": "same again", "files": [{ "path": "src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { a + b + 1 }\n" }] },
            { "kind": "wrong", "scenario": "US1-2", "rationale": "elsewhere", "files": [{ "path": "src/other.rs", "content": "x" }] },
            { "kind": "wrong", "scenario": "US1-2", "rationale": "no change", "files": [{ "path": "src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { a + b }\n" }] },
            { "kind": "permitted", "scenario": "US1-3", "rationale": "commutes", "files": [{ "path": "./src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { b + a }\n" }] },
            { "kind": "permitted", "scenario": "US1-3", "rationale": "too many", "files": [{ "path": "src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { b.wrapping_add(a) }\n" }] },
        ], "notes": [] });
        let mut notes = Vec::new();
        let (wrong, permitted) =
            validate(&answer, &changed(), &StrengthConfig::default(), &mut notes);
        assert_eq!(wrong.len(), 1);
        assert_eq!(permitted.len(), 1);
        assert_eq!(permitted[0].files[0].0, "src/lib.rs");
        let text = notes.join("\n");
        for why in [
            "same as an earlier",
            "not a changed source file",
            "identical to the change",
            "more permitted",
        ] {
            assert!(text.contains(why), "{why}: {text}");
        }
        assert!(wrong[0].digest().starts_with("sha256:"));
        assert_ne!(wrong[0].digest(), permitted[0].digest());
    }

    #[test]
    fn a_pass_survives_an_assertion_rejects_a_build_error_is_unviable() {
        let result = |status, stdout: &str| CheckResult {
            name: "acceptance".to_owned(),
            argv: vec!["cargo".to_owned(), "test".to_owned()],
            status,
            exit_code: Some(i32::from(status != CheckStatus::Passed)),
            duration_ms: 1,
            junit: false,
            tests: None,
            stdout_tail: stdout.to_owned(),
            stderr_tail: String::new(),
            truncated: false,
            note: None,
            flaky: false,
        };
        assert_eq!(
            classify(&result(CheckStatus::Passed, ""), None),
            Outcome::Survived
        );
        assert_eq!(
            classify(&result(CheckStatus::TimedOut, ""), None),
            Outcome::Timeout
        );
        let failed = RunnerSummary {
            passed: 1,
            failed: 1,
            errors: 0,
            build_failed: false,
            unresolved_only: false,
        };
        assert_eq!(
            classify(
                &result(CheckStatus::Failed, "test result: FAILED"),
                Some(&failed)
            ),
            Outcome::Rejected
        );
        assert_eq!(
            classify(
                &result(
                    CheckStatus::Failed,
                    "error[E0308]: mismatched types\nerror: could not compile `x`"
                ),
                None
            ),
            Outcome::Unviable
        );
    }

    #[test]
    fn rulings_and_holds_are_kept_per_task() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let keep = dir(state.path(), "001-x", "T005");
        let ruling = Ruling {
            digest: "sha256:ab".to_owned(),
            scenario: "US2-1".to_owned(),
            by: "Dana".to_owned(),
            reason: "the plan fixes the order".to_owned(),
        };
        add_ruling(&keep, ruling.clone()).unwrap_or_else(|e| panic!("{e}"));
        add_ruling(&keep, ruling.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rulings(&keep), [ruling]);
        let p = Permitted {
            id: "permitted-1".to_owned(),
            scenario: "US2-1".to_owned(),
            rationale: "another order".to_owned(),
            digest: "sha256:ab".to_owned(),
            outcome: PermittedOutcome::Rejected,
            failing: None,
            patch: None,
        };
        hold(&keep, &[&p]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            held_elsewhere(state.path(), "001-x", "T006").as_deref(),
            Some("T005")
        );
        assert!(held_elsewhere(state.path(), "001-x", "T005").is_none());
        release(&keep);
        assert!(held(&keep).is_none());
    }
}
