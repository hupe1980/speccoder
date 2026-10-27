//! The evidence tour of one task, built from the signed statements on its
//! commit and the commit itself — nothing else, so it never says more than
//! `attest verify` can check. Six steps in a fixed order; a step without
//! evidence says so in the verdict's own words instead of being left out.

use std::path::Path;

use serde_json::{Value, json};
use speccoder_core::predicate::TaskVerificationV0;
use speccoder_core::tour::{EvidenceTour, Source, StepKind, TourStep};
use speccoder_core::verify_branch::{TickOutcome, TickReport, Unverifiable};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::attest::statement;
use crate::evidence::banner;
use crate::git::Git;

/// The most of a requirement passage a tour quotes.
const PASSAGE_MAX: usize = 1500;

fn step(kind: StepKind, source: Source, body: String, facts: Value) -> TourStep {
    TourStep {
        kind,
        source,
        title: kind.title().to_owned(),
        body_markdown: body,
        facts,
    }
}

fn phase<'a>(statements: &'a [Value], name: &str) -> Option<&'a Value> {
    statements.iter().find_map(|s| {
        let p = &s["predicate"];
        (s["predicateType"] == statement::TEST_RESULT
            && p["configuration"][0]["name"] == format!("speccoder:phase/{name}"))
        .then_some(p)
    })
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// Why a step has nothing, in the verdict's words.
fn nothing(verdict: Option<&str>, what: &str) -> String {
    match verdict {
        Some("attested") => format!("{what} — {}", banner("attested").1),
        Some("regression_only") => format!("{what} — {}", banner("regression_only").1),
        _ => what.to_owned(),
    }
}

/// `> ` before every line, at most [`PASSAGE_MAX`] characters.
fn quote(text: &str) -> String {
    let mut out = String::new();
    for line in text.trim().lines() {
        if out.chars().count() > PASSAGE_MAX {
            out.push_str("> …\n");
            break;
        }
        out.push_str(&format!("> {line}\n"));
    }
    out
}

/// Step 1: the requirement as the commit's tree has it, through the
/// format adapter — the task's story and the requirements its title
/// cites; `signed` when the story hashes to the signed one.
fn requirement(
    repo: &Path,
    prefix: &str,
    feature: &str,
    task: &str,
    title: &str,
    commit: &str,
    tv: Option<&TaskVerificationV0>,
) -> TourStep {
    let read = crate::attest::tree::intent_tops(repo, commit)
        .and_then(|tops| crate::attest::tree::materialize(repo, commit, Some(&tops)))
        .ok()
        .and_then(|dir| {
            let at = dir.path().join(prefix);
            let format = crate::format::at(&at);
            let snap = format.inspect(&at, Some(feature))?;
            let story = format
                .bundle(&snap, task)
                .ok()
                .map(|b| b.story)
                .unwrap_or_default();
            let unit = snap.feature.as_ref().map(|f| f.feature.dir.clone())?;
            let cited: Vec<(String, String)> = format
                .requirements_file()
                .and_then(|f| {
                    std::fs::read_to_string(Path::new(&snap.root).join(&unit).join(f)).ok()
                })
                .map(|text| {
                    format
                        .requirements_in(&text)
                        .into_iter()
                        .filter(|(id, _)| crate::attest::cites(title, id))
                        .collect()
                })
                .unwrap_or_default();
            Some((story, cited))
        });
    let Some((story, cited)) = read.filter(|(s, c)| !s.trim().is_empty() || !c.is_empty()) else {
        return step(
            StepKind::Requirement,
            Source::NotRecorded,
            format!("no requirement passage found at `{}`", short(commit)),
            Value::Null,
        );
    };
    let hash = crate::integrity::sha256_hex(story.as_bytes());
    let section = tv.and_then(|tv| tv.spec.as_ref());
    let signed = !story.trim().is_empty() && section.is_some_and(|s| s.section_hash == hash);
    let ids: Vec<&str> = cited.iter().map(|(id, _)| id.as_str()).collect();
    let name = match (section.and_then(|s| s.section.clone()), ids.is_empty()) {
        (Some(s), true) => s,
        (Some(s), false) => format!("{s}: {}", ids.join(", ")),
        (None, false) => ids.join(", "),
        (None, true) => "the task's requirement".to_owned(),
    };
    let mut passage = String::new();
    for (_, line) in &cited {
        passage.push_str(line.trim());
        passage.push('\n');
    }
    if !story.trim().is_empty() {
        if !passage.is_empty() {
            passage.push('\n');
        }
        passage.push_str(&story);
    }
    step(
        StepKind::Requirement,
        if signed {
            Source::Signed
        } else {
            Source::Committed
        },
        format!("{name} (at `{}`)\n{}", short(commit), quote(&passage)),
        json!({ "section": section.and_then(|s| s.section.clone()), "cited": ids, "story_sha256": hash, "commit": commit }),
    )
}

/// Step 2: the acceptance tests the signed trace names, and where they
/// were decided (the predicate's `acceptance.source`, else not recorded).
fn frozen_tests(tv: Option<&TaskVerificationV0>, verdict: Option<&str>) -> TourStep {
    let tests: Vec<String> = tv
        .and_then(|tv| tv.trace.as_ref())
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r["test"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let source = tv
        .and_then(|tv| tv.acceptance.as_ref())
        .and_then(|a| a["source"].as_str())
        .map_or_else(
            || "where decided: not recorded".to_owned(),
            |s| format!("decided: {s}"),
        );
    if tests.is_empty() {
        return step(
            StepKind::FrozenTests,
            Source::NotRecorded,
            nothing(verdict, "no task-specific test"),
            json!({ "tests": [], "acceptance": tv.and_then(|tv| tv.acceptance.clone()) }),
        );
    }
    step(
        StepKind::FrozenTests,
        Source::Signed,
        format!(
            "{} ({source})",
            tests
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        json!({ "tests": tests, "acceptance": tv.and_then(|tv| tv.acceptance.clone()) }),
    )
}

/// Step 3: the tests failing first, how, and the signed excerpt.
fn red(statements: &[Value], tv: Option<&TaskVerificationV0>, verdict: Option<&str>) -> TourStep {
    let Some(red) = phase(statements, "red") else {
        return step(
            StepKind::Red,
            Source::NotRecorded,
            nothing(verdict, "no red phase"),
            Value::Null,
        );
    };
    let failed = names(&red["failedTests"]);
    let excerpt = tv.and_then(|tv| tv.red.as_ref());
    let kind = excerpt.map_or("kind not recorded", |r| match r.kind {
        speccoder_core::predicate::RedKind::Assertion => "assertion",
        speccoder_core::predicate::RedKind::MissingSymbol => "missing symbol",
    });
    let block = match excerpt {
        Some(r) if r.withheld => {
            "output excerpt withheld (it was not clean: a local path or a credential)".to_owned()
        }
        Some(r) => format!(
            "```text\n{}\n```",
            r.excerpt
                .as_deref()
                .unwrap_or_default()
                .replace("```", "'''")
        ),
        None => "no output excerpt recorded".to_owned(),
    };
    step(
        StepKind::Red,
        Source::Signed,
        format!(
            "failed: {} ({kind})\n{block}",
            if failed.is_empty() {
                "—".to_owned()
            } else {
                failed
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ),
        json!({ "failed": failed, "red": excerpt }),
    )
}

/// Step 4: the commit's diff, by its stat (`committed`: git objects).
fn diff(repo: &Path, commit: &str) -> TourStep {
    let stat = Git::at(repo)
        .run(&["show", "--stat=100", "--format=", commit])
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .unwrap_or_default();
    let numstat = Git::at(repo)
        .run(&["show", "--numstat", "--format=", commit])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let (mut files, mut added, mut removed) = (0usize, 0u64, 0u64);
    for line in numstat.lines() {
        let mut f = line.split('\t');
        if let (Some(a), Some(r), Some(_)) = (f.next(), f.next(), f.next()) {
            files += 1;
            added += a.parse::<u64>().unwrap_or(0);
            removed += r.parse::<u64>().unwrap_or(0);
        }
    }
    let lines: Vec<&str> = stat.lines().collect();
    let shown = lines.len().min(30);
    let mut block = lines[..shown].join("\n");
    if lines.len() > shown {
        block.push_str(&format!("\n… {} more", lines.len() - shown));
    }
    step(
        StepKind::Diff,
        Source::Committed,
        format!(
            "`{}`: {files} file(s), +{added} −{removed}\n```text\n{block}\n```",
            short(commit)
        ),
        json!({ "commit": commit, "files": files, "added": added, "removed": removed }),
    )
}

/// Step 5: what passed after — acceptance, held-out, the checks against
/// their baseline — and signed dependencies, shown, never failing.
fn green(statements: &[Value], tv: Option<&TaskVerificationV0>, verdict: Option<&str>) -> TourStep {
    let Some(green) = phase(statements, "green") else {
        return step(
            StepKind::Green,
            Source::NotRecorded,
            nothing(verdict, "no green phase"),
            Value::Null,
        );
    };
    let passed = names(&green["passedTests"]).len();
    let failed = names(&green["failedTests"]).len();
    let held_out = phase(statements, "held_out").map_or_else(
        || "held-out: not run".to_owned(),
        |h| {
            if h["result"] == "PASSED" {
                "held-out passed".to_owned()
            } else {
                "held-out failed".to_owned()
            }
        },
    );
    let warned = phase(statements, "baseline").map(|b| names(&b["warnedTests"]).len());
    let checks = match warned {
        Some(0) => "checks green, no regression (baseline warnings: 0)".to_owned(),
        Some(n) => format!("checks green, no regression (baseline warnings: {n})"),
        None => "no baseline recorded".to_owned(),
    };
    let mut body = format!("{passed} passed, {failed} failed; {held_out}; {checks}");
    let deps = tv.and_then(|tv| tv.dependencies.clone());
    if let Some(d) = deps.as_ref().and_then(Value::as_array) {
        body.push_str(&format!(
            "\ndependencies added (signed): {}",
            d.iter()
                .map(|x| format!(
                    "`{}@{}`",
                    x["name"].as_str().unwrap_or("?"),
                    x["version"].as_str().unwrap_or("?")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    // The dependency folders the checks ran against, linked from the checkout.
    let linked = tv
        .and_then(|tv| tv.linked_dependencies.clone())
        .unwrap_or_default();
    if !linked.is_empty() {
        body.push_str(&format!(
            "\nchecked against dependencies linked from the checkout (signed): {}",
            linked
                .iter()
                .map(|l| format!(
                    "`{}` ({} sha256:{})",
                    l.path,
                    l.lockfile,
                    l.sha256.chars().take(12).collect::<String>()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    step(
        StepKind::Green,
        Source::Signed,
        body,
        json!({ "passed": passed, "failed": failed, "held_out": phase(statements, "held_out").map(|h| &h["result"]), "baseline_warnings": warned, "dependencies": deps, "linked_dependencies": (!linked.is_empty()).then_some(linked) }),
    )
}

/// Step 6: the acceptance set's strength and mutation results.
fn strength(tv: Option<&TaskVerificationV0>, verdict: Option<&str>) -> TourStep {
    let st = tv
        .and_then(|tv| tv.strength.as_ref())
        .filter(|v| !v.is_null());
    let mutation = tv
        .and_then(|tv| tv.mutation.as_ref())
        .filter(|v| !v.is_null());
    if st.is_none() && mutation.is_none() {
        return step(
            StepKind::Strength,
            Source::NotRecorded,
            nothing(verdict, "not measured"),
            Value::Null,
        );
    }
    let mut parts = Vec::new();
    if let Some(st) = st {
        let share = |v: &Value| v["share"].as_f64().map(|x| format!("{x:.2}"));
        if let Some(v) = share(&st["visible"]) {
            parts.push(format!("visible {v}"));
        }
        if let Some(c) = share(&st["combined"]) {
            parts.push(format!("combined {c}"));
        }
        let near = st["near_misses"].as_array().map_or(0, Vec::len);
        parts.push(format!("{near} near miss(es) tried"));
        let rejected = st["permitted"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["outcome"] == "rejected")
            .count();
        if rejected > 0 {
            parts.push(format!(
                "{rejected} permitted alternative(s) rejected — over-strict"
            ));
        }
    }
    if let Some(m) = mutation {
        parts.push(format!(
            "{} of {} mutants caught",
            m["caught"].as_u64().unwrap_or(0),
            m["caught"].as_u64().unwrap_or(0) + m["missed"].as_u64().unwrap_or(0)
        ));
    }
    step(
        StepKind::Strength,
        Source::Signed,
        parts.join(", "),
        json!({ "strength": st, "mutation": mutation }),
    )
}

/// The tour of one checked tick. `prefix` is the project's path in the
/// repository.
#[must_use]
pub fn of(repo: &Path, prefix: &str, r: &TickReport) -> EvidenceTour {
    let mut tour = EvidenceTour {
        feature: r.tick.feature.clone(),
        task: r.tick.task.clone(),
        title: r.tick.title.clone(),
        verdict: r.verdict.clone(),
        commit: r.commit.clone(),
        steps: Vec::new(),
        unverifiable: None,
    };
    let Some(commit) = r.commit.as_deref() else {
        tour.steps.push(step(
            StepKind::Requirement,
            Source::NotRecorded,
            "no commit carries this task's evidence".to_owned(),
            Value::Null,
        ));
        tour.unverifiable = Some(
            r.detail
                .clone()
                .unwrap_or_else(|| Unverifiable::TickedWithoutEvidence.word().to_owned()),
        );
        return tour;
    };
    let statements = crate::evidence::statements(repo, commit);
    let tv: Option<TaskVerificationV0> = statements
        .iter()
        .find(|s| s["predicateType"] == statement::TASK_VERIFICATION)
        .and_then(|s| serde_json::from_value(s["predicate"].clone()).ok());
    let tv = tv.as_ref();
    tour.steps.push(requirement(
        repo,
        prefix,
        &r.tick.feature,
        &r.tick.task,
        &r.tick.title,
        commit,
        tv,
    ));
    if r.outcome == TickOutcome::Unverifiable {
        tour.unverifiable = Some(match (&r.reason, &r.detail) {
            (Some(reason), Some(d)) => format!("{} — {d}", reason.word()),
            (Some(reason), None) => reason.word().to_owned(),
            (None, _) => "the evidence does not verify".to_owned(),
        });
        return tour;
    }
    let verdict = r.verdict.as_deref();
    tour.steps.push(frozen_tests(tv, verdict));
    tour.steps.push(red(&statements, tv, verdict));
    tour.steps.push(diff(repo, commit));
    tour.steps.push(green(&statements, tv, verdict));
    tour.steps.push(strength(tv, verdict));
    // Reserved fields another spec owns: shown as the signed facts they
    // are, never interpreted here.
    if let Some(tv) = tv {
        for name in tv.reserved_present() {
            let value = match name {
                "bug" => tv.bug.as_ref(),
                "context" => tv.context.as_ref(),
                _ => None,
            };
            if let (Some(v), Some(last)) = (value, tour.steps.last_mut()) {
                last.body_markdown.push_str(&format!(
                    "\nsigned `{name}` (shown as recorded): `{}`",
                    v.to_string().chars().take(300).collect::<String>()
                ));
            }
        }
    }
    tour
}

/// The words a tour's summary line ends with.
#[must_use]
pub fn words(r: &TickReport) -> String {
    match r.outcome {
        TickOutcome::Evidenced => banner(r.verdict.as_deref().unwrap_or_default())
            .1
            .to_owned(),
        TickOutcome::Stale => "STALE — evidence for an earlier wording".to_owned(),
        TickOutcome::Unverifiable => "UNVERIFIABLE".to_owned(),
        TickOutcome::Regressed => "REGRESSED".to_owned(),
    }
}

/// Every tick's tour, as collapsed Markdown, in the report's order.
#[must_use]
pub fn markdown_all(checked: &crate::attest::branch::Checked) -> Vec<String> {
    checked
        .report
        .ticks
        .iter()
        .map(|r| of(&checked.repo, &checked.prefix, r).markdown(&words(r)))
        .collect()
}

/// `speccoder attest tour` and `/api/tour`: the tour of `feature`'s
/// `task` at `head`, with the check's trust when given (the outcome then
/// decides how far the tour goes), else from the statements as recorded.
#[must_use]
pub fn command(
    start: &Path,
    feature: &str,
    task: &str,
    opts: &crate::attest::branch::Options,
    command: &str,
) -> CommandResult {
    let checked = match crate::attest::branch::locate(start, opts, feature, task) {
        Ok(c) => c,
        Err(r) => return CommandResult::new(command, r.status, &r.message),
    };
    let Some(r) = checked.report.ticks.first() else {
        return CommandResult::new(
            command,
            CommandStatus::NotFound,
            &format!("{feature} has no ticked task {task} at {}", opts.head),
        );
    };
    let tour = of(&checked.repo, &checked.prefix, r);
    CommandResult::new(command, CommandStatus::Ok, &format!("{task}: {}", words(r)))
        .with_data(CommandData::EvidenceTour(Box::new(tour)))
}
