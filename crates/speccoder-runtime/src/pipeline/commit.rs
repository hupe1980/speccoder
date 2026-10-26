//! Commit messages, trailers and the records a finished attempt leaves.

use std::path::Path;

use crate::{approvals, git, tools};

/// The monitor's findings in one line: `kind: evidence; …`.
pub(crate) fn monitor_summary(monitor: Option<&serde_json::Value>) -> String {
    monitor
        .and_then(|m| m["findings"].as_array())
        .map(|f| {
            f.iter()
                .map(|x| {
                    format!(
                        "{}: {}",
                        x["kind"].as_str().unwrap_or("other"),
                        x["evidence"]
                            .as_str()
                            .unwrap_or_default()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "no details given".to_owned())
}

/// Inline "for this task" approvals become task decisions of this run, as
/// if made with `speccoder task approve`, so a retry or a later attempt of
/// the run is not asked again — and a later run is.
///
/// # Errors
///
/// The decisions file cannot be read or written: the attempt's decisions
/// are not recorded (later attempts ask again), and the caller says so.
pub(crate) fn record_inline_approvals(
    dir: &Path,
    feature: &str,
    task: &str,
    calls: &[serde_json::Value],
    run_grants: &std::sync::Mutex<Vec<tools::policy::Object>>,
    run: &str,
) -> Result<(), String> {
    // Every decision a person made in this attempt, with who made it and how
    // far it reaches; calls an earlier decision allowed (`*-grant`) are not
    // new decisions.
    let decided: Vec<approvals::Decided> = calls
        .iter()
        .filter(|c| c["approved_by"] == "person" || c["rejected_by"] == "person")
        .filter(|c| !c["scope"].as_str().is_some_and(|s| s.ends_with("-grant")))
        .filter_map(|c| {
            let object: tools::policy::Object = serde_json::from_value(c["object"].clone()).ok()?;
            let by = serde_json::from_value::<approvals::Who>(c["approver"].clone())
                .unwrap_or_else(|_| approvals::Who {
                    name: "unknown".to_owned(),
                    via: "inline".to_owned(),
                    client: None,
                    basis: "asserted".to_owned(),
                });
            let approved = c["approved_by"] == "person";
            Some(approvals::Decided {
                verb: if approved { "approve" } else { "reject" }.to_owned(),
                object: Some(object),
                run: Some(run.to_owned()),
                reason: c["rejection"].as_str().map(str::to_owned),
                scope: c["scope"].as_str().unwrap_or("once").to_owned(),
                note: c["note"].as_str().map(str::to_owned),
                by,
                at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
            })
        })
        .collect();
    if decided.is_empty() {
        return Ok(());
    }
    let mut d = approvals::Decisions::load(dir)?;
    for decision in decided {
        match (decision.verb.as_str(), decision.scope.as_str()) {
            // For the task: it joins the task's approvals, for later attempts.
            ("approve", "task") => {
                let Some(object) = decision.object.clone() else {
                    continue;
                };
                d.approve(feature, task, object, Some(run), &decision.by);
                if let Some(last) = d
                    .tasks
                    .get_mut(&format!("{feature}/{task}"))
                    .and_then(|t| t.decided.last_mut())
                {
                    last.note.clone_from(&decision.note);
                }
            }
            // For this run: every later task of the run; the history only.
            ("approve", "run") => {
                if let (Ok(mut g), Some(object)) = (run_grants.lock(), &decision.object)
                    && !g.contains(object)
                {
                    g.push(object.clone());
                }
                d.note(feature, task, decision);
            }
            _ => d.note(feature, task, decision),
        }
    }
    d.save(dir)
}

/// Sums two token usage objects key by key.
pub(crate) fn add_usage(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match (a, b) {
        (None, b) => b.cloned(),
        (a, None) => a.cloned(),
        (Some(a), Some(b)) => {
            let mut sum = serde_json::Map::new();
            for key in [
                "input_tokens",
                "output_tokens",
                "cache_read_tokens",
                "cache_write_tokens",
            ] {
                let n = a[key].as_u64().unwrap_or(0) + b[key].as_u64().unwrap_or(0);
                sum.insert(key.to_owned(), serde_json::json!(n));
            }
            Some(serde_json::Value::Object(sum))
        }
    }
}

/// Commits everything in the worktree on its branch with speccoder's
/// identity, without hooks; returns the commit id. Never pushes.
pub(crate) fn commit_attempt(worktree: &Path, message: &str) -> Result<String, String> {
    commit_attempt_as(worktree, message, false)
}

/// [`commit_attempt`]; `allow_empty` records a verdict whose work touched no
/// tracked file (in place, the tick lives in a git-ignored `tasks.md`).
pub(crate) fn commit_attempt_as(
    worktree: &Path,
    message: &str,
    allow_empty: bool,
) -> Result<String, String> {
    let g = git::Git::at(worktree);
    let base = [
        "-c",
        "user.name=speccoder",
        "-c",
        "user.email=speccoder@users.noreply.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    let mut add = base.to_vec();
    add.extend(["add", "-A"]);
    g.run(&add).map_err(|e| e.0)?;
    let mut commit = base.to_vec();
    commit.extend(["commit", "-q", "--no-verify", "-m", message]);
    if allow_empty {
        commit.push("--allow-empty");
    }
    g.run(&commit).map_err(|e| e.0)?;
    g.line(&["rev-parse", "HEAD"]).map_err(|e| e.0)
}

/// `speccoder: T004 <title>` cut to 72 characters on a char boundary.
pub(crate) fn commit_subject(task: &str, title: &str) -> String {
    let full = format!("speccoder: {task} {title}");
    if full.chars().count() <= 72 {
        return full;
    }
    let cut: String = full.chars().take(71).collect();
    format!("{}…", cut.trim_end())
}

/// What a tick's commit message is made of.
pub(crate) struct TickFacts<'a> {
    pub(crate) feature: &'a str,
    pub(crate) task: &'a str,
    pub(crate) title: &'a str,
    pub(crate) report: &'a speccoder_core::RunReport,
    /// The kind of red the acceptance tests showed, when they did.
    pub(crate) red: Option<speccoder_core::verify::RedClass>,
    pub(crate) checklists_overridden: bool,
    /// More trailers (a bug task's `Bug:`, `Acceptance:`, `Fixes`).
    pub(crate) extra: String,
}

/// The commit message of a ticked task: subject, title, and the trailers
/// `report` reads back (`Spec-Task`, `Verdict`, `Red`, `Flaky`,
/// `Scope-Outside`, `Checklists`, `Held-Out`, `Acceptance-Strength`,
/// `Ruling`, `Run`, `Usage`, `Mutation`).
pub(crate) fn tick_message(f: &TickFacts<'_>) -> String {
    let report = f.report;
    let verdict_word = report
        .verdict
        .as_ref()
        .map(super::outcome::verdict_word)
        .unwrap_or_default();
    // Attempts and the judge-side acceptance calls together.
    let usage = add_usage(
        add_usage(
            report.usage.as_ref(),
            report.acceptance.as_ref().and_then(|a| a.get("usage")),
        )
        .as_ref(),
        report
            .acceptance
            .as_ref()
            .and_then(|a| a.get("held_out"))
            .and_then(|h| h.get("usage")),
    );
    let mut evidence = f
        .red
        .map(|c| format!("Red: {}\n", super::acceptance::class_word(c)))
        .unwrap_or_default();
    evidence.push_str(&held_out_trailer(report.acceptance.as_ref()));
    evidence.push_str(&flaky_trailer(report.verification.as_ref()));
    evidence.push_str(&scope_trailer(report.scope.as_ref()));
    if let Some(strength) = &report.strength {
        evidence.push_str(&speccoder_core::strength::trailers(strength));
    }
    if f.checklists_overridden {
        evidence.push_str("Checklists: overridden\n");
    }
    evidence.push_str(&f.extra);
    format!(
        "{}\n\n{}\n\nSpec-Task: {}/{}\nVerdict: {verdict_word}\n{evidence}Run: {}\n{}{}",
        commit_subject(f.task, f.title),
        f.title,
        f.feature,
        f.task,
        report.run_id,
        crate::attempts::usage_trailer(usage.as_ref()),
        report
            .mutation
            .as_ref()
            .and_then(crate::mutation::trailer)
            .unwrap_or_default()
    )
}

/// `Held-Out: passed=<k>/<k> manifest=sha256:<hex>`, `absent=<reason>` or
/// `off`; nothing for a task without acceptance tests.
fn held_out_trailer(acceptance: Option<&serde_json::Value>) -> String {
    let Some(h) = acceptance.and_then(|a| a.get("held_out")) else {
        return String::new();
    };
    match h["state"].as_str() {
        Some("on") => {
            let kept = h["kept"].as_u64().unwrap_or(0);
            format!(
                "Held-Out: passed={kept}/{kept} manifest={}\n",
                h["manifest"].as_str().unwrap_or_default()
            )
        }
        Some("off") => "Held-Out: off\n".to_owned(),
        Some("absent") => format!(
            "Held-Out: absent={}\n",
            h["reason"].as_str().unwrap_or("none_generated")
        ),
        _ => String::new(),
    }
}

/// `Flaky: <checks>` for checks that passed only when run again.
fn flaky_trailer(verification: Option<&speccoder_core::verify::VerifyReport>) -> String {
    let names: Vec<&str> = verification
        .into_iter()
        .flat_map(|v| &v.checks)
        .filter(|c| c.flaky)
        .map(|c| c.name.as_str())
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!("Flaky: {}\n", names.join(", "))
    }
}

/// `Scope-Outside: <paths>` (the first ten, then a count).
fn scope_trailer(scope: Option<&speccoder_core::scope::ScopeFinding>) -> String {
    let Some(outside) = scope.map(|s| &s.outside).filter(|o| !o.is_empty()) else {
        return String::new();
    };
    let mut shown: Vec<&str> = outside.iter().take(10).map(String::as_str).collect();
    let more = outside.len().saturating_sub(10);
    let more = format!("… (+{more})");
    if outside.len() > 10 {
        shown.push(&more);
    }
    format!("Scope-Outside: {}\n", shown.join(", "))
}

/// Leftovers of an interrupted run (or edits by hand) saved under an attempt
/// ref and set aside, so a run starts from the branch tip; the ref.
pub(crate) fn set_aside_leftovers(
    wt_root: &Path,
    tmp: &Path,
    feature: &str,
    task: &str,
) -> Result<String, String> {
    crate::attempts::save(
        wt_root,
        tmp,
        feature,
        task,
        &crate::attempts::Record {
            run: None,
            outcome: crate::controls::SET_ASIDE,
            reason: "the worktree had uncommitted changes when a run started (an interrupted run, or edits by hand); saved and set aside — `speccoder task restore` keeps them, `task drop` discards them",
            pending: None,
            usage: None,
        },
    )
    .and_then(|saved| crate::attempts::restore_head(wt_root, tmp).map(|()| saved.reference))
    .map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    /// The value of trailer `key` in `message`, by exact key (`Acceptance:` is
    /// not `Acceptance-Strength:`).
    fn trailer<'m>(message: &'m str, key: &str) -> Option<&'m str> {
        message.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k == key).then(|| v.trim())
        })
    }

    #[test]
    fn trailers_are_read_by_their_exact_key() {
        let m = "speccoder: T001 fix\n\nSpec-Task: 007-bugfixes/T001\nVerdict: verified\nAcceptance-Strength: 1.00 near-miss=1/1\nAcceptance: bug_confirmation manifest=sha256:m approval=sha256:a wording=sha256:w by=Ada\nBug: fixed (standing_speckit) triad=sha256:t\nFixes: #42\n";
        assert_eq!(
            trailer(m, "Acceptance"),
            Some("bug_confirmation manifest=sha256:m approval=sha256:a wording=sha256:w by=Ada")
        );
        assert_eq!(
            trailer(m, "Acceptance-Strength"),
            Some("1.00 near-miss=1/1")
        );
        assert_eq!(
            trailer(m, "Bug"),
            Some("fixed (standing_speckit) triad=sha256:t")
        );
        assert_eq!(trailer(m, "Spec-Task"), Some("007-bugfixes/T001"));
        assert_eq!(trailer(m, "Fixes"), Some("#42"));
        assert_eq!(trailer(m, "Acceptance-Str"), None);
    }
}
