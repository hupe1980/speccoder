//! The `speccoder / evidence` check run, rendered from a branch report:
//! the verdict bar and the trust it used (summary), the requirement trace,
//! every tick and its tour (text), and one annotation per tick that is not
//! plainly verified, on the line it is about. The same, as workflow
//! commands and a step summary, where the check cannot be published.
//!
//! Rendering only: the report decides, these words never soften it.

use serde::Serialize;
use serde_json::{Value, json};
use speccoder_core::verify_branch::{
    Anchor, BranchReport, Conclusion, D121_REGRESSIONS, TRAILER_ONLY_TASKS, TickOutcome,
    TickReport, Unverifiable,
};
use speccoder_core::workflow::{ANNOTATIONS_PER_KIND, data, property};

use crate::evidence::banner;

/// The check run's name.
pub const NAME: &str = "speccoder / evidence";

/// Characters GitHub takes in a check run's `summary`, and in its `text`.
pub const TEXT_BUDGET: usize = 65_535;

/// Annotations GitHub takes per check-run request.
pub const ANNOTATIONS_PER_REQUEST: usize = 50;

/// One annotation, in GitHub's check-run shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Annotation {
    /// Repository-relative path.
    pub path: String,
    /// First line.
    pub start_line: u32,
    /// Last line.
    pub end_line: u32,
    /// `failure`, `warning` or `notice`.
    pub annotation_level: &'static str,
    /// One line.
    pub title: String,
    /// What it means and what to do.
    pub message: String,
}

fn at(anchor: &Anchor, level: &'static str, title: String, message: String) -> Annotation {
    Annotation {
        path: anchor.path.clone(),
        start_line: anchor.line.max(1),
        end_line: anchor.line.max(1),
        annotation_level: level,
        title,
        message,
    }
}

/// What to do about an unverifiable tick.
fn fix(r: &TickReport, reason: Unverifiable) -> String {
    let task = &r.tick.task;
    match reason {
        Unverifiable::TickedWithoutEvidence => format!(
            "No commit carries this task's evidence. Re-run the task with speccoder, or `speccoder task accept {task}` (attested)."
        ),
        Unverifiable::Missing => "The task commit has no attestation note: push them (`git push origin refs/notes/speccoder`).".to_owned(),
        Unverifiable::CommitUnreachable => "The commit with the evidence is not on this branch: a squash or rebase merge drops it; merge with a merge commit.".to_owned(),
        Unverifiable::UntrustedSigner => "Signed by a key the maintainer's trust does not list: add the signer to the base branch's allowed signers, or re-sign.".to_owned(),
        Unverifiable::Unsigned => "The evidence is not signed: `speccoder attest sign`, then push the notes.".to_owned(),
        Unverifiable::ToolMissing => "The signature's tool is not installed on this runner; install it and re-run.".to_owned(),
        Unverifiable::BadSignature | Unverifiable::Mismatch | Unverifiable::Malformed => {
            "The evidence does not hold for this commit (edited or tampered). Re-run the task with speccoder.".to_owned()
        }
    }
}

/// One annotation per tick that is not plainly verified: unverifiable and
/// regressed as `failure`, regression-only and stale as `warning`
/// (stale on the changed requirement's line), attested as a `notice`
/// naming who accepted it.
#[must_use]
pub fn annotations(report: &BranchReport) -> Vec<Annotation> {
    let mut out = Vec::new();
    for r in &report.ticks {
        let task = &r.tick.task;
        let detail = r
            .detail
            .as_deref()
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        match (r.outcome, r.verdict.as_deref()) {
            (TickOutcome::Unverifiable, _) => {
                let reason = r.reason.unwrap_or(Unverifiable::Malformed);
                out.push(at(
                    &r.tick.anchor,
                    "failure",
                    format!("{task} unverifiable: {}", reason.word()),
                    format!("{}{detail}", fix(r, reason)),
                ));
            }
            (TickOutcome::Regressed, _) => out.push(at(
                &r.tick.anchor,
                "failure",
                format!("{task} regressed"),
                "A later task's signed checkpoint marks this task regressed.".to_owned(),
            )),
            (TickOutcome::Stale, _) => out.push(at(
                r.stale_anchor.as_ref().unwrap_or(&r.tick.anchor),
                "warning",
                format!(
                    "{task} stale: {} changed",
                    r.stale
                        .iter()
                        .map(|d| d.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                format!(
                    "{task}'s evidence is for the earlier wording of this requirement. Re-prove it (speccoder converge) or revert the edit."
                ),
            )),
            (TickOutcome::Evidenced, Some("regression_only")) => out.push(at(
                &r.tick.anchor,
                "warning",
                format!("{task} regression only"),
                banner("regression_only").1.to_owned(),
            )),
            (TickOutcome::Evidenced, Some("attested")) => out.push(at(
                &r.tick.anchor,
                "notice",
                format!(
                    "{task} attested by {}",
                    r.decider.as_deref().unwrap_or("a person")
                ),
                banner("attested").1.to_owned(),
            )),
            (TickOutcome::Evidenced, _) => {}
        }
    }
    out
}

/// The conclusion as a sentence.
fn sentence(report: &BranchReport) -> String {
    let failing = report
        .ticks
        .iter()
        .filter(|t| t.fails(report.require_verified))
        .count();
    match report.conclusion {
        Conclusion::Success => "every tick carries evidence that verifies.".to_owned(),
        Conclusion::Neutral => "neutral: no ticks checked — nothing here is proof.".to_owned(),
        Conclusion::Failure if failing == 0 => {
            "a claim in range cannot be checked by this verifier version.".to_owned()
        }
        Conclusion::Failure => format!("{failing} tick(s) without verifying evidence."),
    }
}

/// The verdict bar, its first line: the counts in the verdict words.
#[must_use]
pub fn bar(report: &BranchReport) -> String {
    let t = &report.totals;
    let mut bar = format!(
        "**verified {} · regression only {} · attested {} · stale {} · unverifiable {}",
        t.verified, t.regression_only, t.attested, t.stale, t.unverifiable
    );
    if t.regressed > 0 {
        bar.push_str(&format!(" · regressed {}", t.regressed));
    }
    format!("{bar}** — {}", sentence(report))
}

fn trust_line(report: &BranchReport) -> String {
    let ssh = &report.trust["ssh_allowed_signers"];
    let ssh = match (
        ssh["rev"].as_str(),
        ssh["path"].as_str(),
        ssh["file"].as_str(),
    ) {
        (Some(rev), Some(path), _) => format!("allowed signers `{path}` at `{rev}`"),
        (_, _, Some(file)) => format!("allowed signers `{file}`"),
        _ => "no SSH allowed signers".to_owned(),
    };
    let sigstore = report.trust["sigstore"]["identity"]
        .as_str()
        .map(|i| {
            format!(
                ", Sigstore identity `{i}` ({})",
                report.trust["sigstore"]["issuer"].as_str().unwrap_or("?")
            )
        })
        .unwrap_or_default();
    format!("Trust: {ssh}{sigstore}; the verifier's, never the branch's.")
}

/// The check run's `summary`: the bar, the trust, and what this verifier
/// did not evaluate or show.
#[must_use]
pub fn summary(report: &BranchReport, omitted_annotations: usize) -> String {
    let mut out = format!("{}\n\n{}\n", bar(report), trust_line(report));
    if report.accepted_unsigned {
        out.push_str(
            "\n> **Unsigned ticks were accepted** (`require-signed: false`): their statements could have been written by anyone who can push.\n",
        );
    }
    if report.require_verified {
        out.push_str("\nEvery tick must be `verified` (`require-verified: true`).\n");
    }
    let mut notes = Vec::new();
    for n in &report.not_evaluated {
        notes.push(match n.as_str() {
            D121_REGRESSIONS => {
                "earlier-task regressions (D-121) not checked by this verifier version".to_owned()
            }
            TRAILER_ONLY_TASKS => "a trailer-only bug task (`Spec-Task: <unit>/bug:<slug>`) is in range; this verifier version cannot check it, so the branch fails".to_owned(),
            other => format!("{other}: not checked by this verifier version"),
        });
    }
    if !report.newer_fields.is_empty() {
        notes.push(format!(
            "fields from a newer predicate revision not shown: {}",
            report.newer_fields.join(", ")
        ));
    }
    if !report.open_no_claim.is_empty() {
        notes.push(format!(
            "open, no claim: {}",
            report.open_no_claim.join(", ")
        ));
    }
    if !report.skipped.is_empty() {
        notes.push(format!(
            "{} task(s) skipped by a person (not ticks)",
            report.skipped.len()
        ));
    }
    if omitted_annotations > 0 {
        notes.push(format!(
            "{omitted_annotations} annotation(s) not shown — every tick is in the table"
        ));
    }
    for n in notes {
        out.push_str(&format!("\n- {n}"));
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    cut(&out, TEXT_BUDGET, "the summary")
}

/// A table cell: one line, no pipes.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn verdict_words(r: &TickReport) -> String {
    match r.outcome {
        TickOutcome::Evidenced => banner(r.verdict.as_deref().unwrap_or_default())
            .1
            .to_owned(),
        TickOutcome::Stale => "STALE — evidence for an earlier wording".to_owned(),
        TickOutcome::Unverifiable => format!(
            "UNVERIFIABLE — {}",
            r.reason.map_or("no evidence", Unverifiable::word)
        ),
        TickOutcome::Regressed => "REGRESSED".to_owned(),
    }
}

/// The per-tick table and the trace, without tours.
fn tables(report: &BranchReport) -> String {
    let mut out = String::from(
        "### Ticks\n\n| Unit | Task | Outcome | Commit | Signer |\n|---|---|---|---|---|\n",
    );
    for r in &report.ticks {
        out.push_str(&format!(
            "| {} | {} | {}{} | {} | {} |\n",
            cell(&r.tick.feature),
            cell(&r.tick.task),
            cell(&verdict_words(r)),
            r.detail
                .as_deref()
                .map(|d| format!(" — {}", cell(d)))
                .unwrap_or_default(),
            r.commit.as_deref().map_or_else(
                || "—".to_owned(),
                |c| format!("`{}`", &c[..c.len().min(12)])
            ),
            r.signer.as_ref().map_or_else(
                || "—".to_owned(),
                |s| cell(s.principal.as_deref().unwrap_or(&s.kind))
            ),
        ));
    }
    if report.ticks.is_empty() {
        out.push_str("| — | — | no ticks | — | — |\n");
    }
    if !report.trace.is_empty() {
        out.push_str("\n### Requirement trace\n\n| Requirement | Scenario | Test | Task | Verdict |\n|---|---|---|---|---|\n");
        for t in &report.trace {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                cell(&t.requirement),
                cell(t.scenario.as_deref().unwrap_or("—")),
                cell(t.test.as_deref().unwrap_or("—")),
                cell(t.task.as_deref().unwrap_or("not cited")),
                cell(&match (&t.verdict, t.changed) {
                    (Some(v), true) => format!("{v} (changed since)"),
                    (Some(v), false) => v.clone(),
                    (None, _) => "—".to_owned(),
                }),
            ));
        }
    }
    out
}

/// The check run's `text`: the tables, then each tour collapsed, tours
/// dropped last-first with a counted line when over budget. Returns how
/// many tours were not shown.
#[must_use]
pub fn text(report: &BranchReport, tours: &[String]) -> (String, usize) {
    let tables = cut(&tables(report), TEXT_BUDGET - 200, "the tables");
    let mut out = tables;
    let mut shown = 0;
    let budget = TEXT_BUDGET - 200;
    if !tours.is_empty() {
        out.push_str("\n### Evidence tours\n\n");
    }
    for tour in tours {
        if out.chars().count() + tour.chars().count() + 1 > budget {
            break;
        }
        out.push_str(tour);
        out.push('\n');
        shown += 1;
    }
    let omitted = tours.len() - shown;
    if omitted > 0 {
        out.push_str(&format!(
            "\n{omitted} tour(s) not shown — run `speccoder-verify tour --feature <unit> --task <id>`\n"
        ));
    }
    (out, omitted)
}

/// `text` within `budget` characters, cut at a line with a counted note.
fn cut(text: &str, budget: usize, what: &str) -> String {
    if text.chars().count() <= budget {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut lines = text.lines();
    let mut dropped = 0;
    for line in lines.by_ref() {
        if out.chars().count() + line.chars().count() + 120 > budget {
            dropped += 1;
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    dropped += lines.count();
    out.push_str(&format!(
        "\n… {dropped} line(s) of {what} not shown (GitHub's size cap); every tick is in the outcome.\n"
    ));
    out
}

/// The conclusion a check run is published with: GitHub counts a
/// `neutral` required check as passing, so neutral is `failure` unless the
/// maintainer allows it.
#[must_use]
pub fn published(conclusion: Conclusion, allow_neutral: bool) -> &'static str {
    match conclusion {
        Conclusion::Neutral if allow_neutral => "neutral",
        Conclusion::Neutral | Conclusion::Failure => "failure",
        Conclusion::Success => "success",
    }
}

/// The check-run request bodies, `{ "create": …, "updates": [ … ] }`:
/// at most 50 annotations in the create, the rest in updates of 50.
#[must_use]
pub fn request(report: &BranchReport, tours: &[String], name: &str, allow_neutral: bool) -> Value {
    let all = annotations(report);
    let summary = summary(report, 0);
    let (text, _) = text(report, tours);
    let title = report.title();
    let mut batches = all.chunks(ANNOTATIONS_PER_REQUEST);
    let first: Vec<&Annotation> = batches
        .next()
        .map(|b| b.iter().collect())
        .unwrap_or_default();
    let updates: Vec<Value> = batches
        .map(|b| {
            json!({
                "output": { "title": title, "summary": summary, "annotations": b }
            })
        })
        .collect();
    json!({
        "create": {
            "name": name,
            "head_sha": report.head,
            "status": "completed",
            "conclusion": published(report.conclusion, allow_neutral),
            "output": {
                "title": title,
                "summary": summary,
                "text": text,
                "annotations": first,
            }
        },
        "updates": updates,
    })
}

/// The fallback: the annotations as workflow commands, at most 10 of a
/// kind, then a notice counting the rest.
#[must_use]
pub fn workflow_commands(report: &BranchReport) -> Vec<String> {
    let mut by_kind: [(&str, Vec<String>); 3] = [
        ("error", Vec::new()),
        ("warning", Vec::new()),
        ("notice", Vec::new()),
    ];
    for a in annotations(report) {
        let kind = match a.annotation_level {
            "failure" => 0,
            "warning" => 1,
            _ => 2,
        };
        by_kind[kind].1.push(format!(
            "::{} file={},line={},title={}::{}",
            by_kind[kind].0,
            property(&a.path),
            a.start_line,
            property(&a.title),
            data(&a.message)
        ));
    }
    let hidden: usize = by_kind
        .iter()
        .map(|(_, v)| v.len().saturating_sub(ANNOTATIONS_PER_KIND))
        .sum();
    let mut out: Vec<String> = by_kind
        .into_iter()
        .flat_map(|(_, v)| v.into_iter().take(ANNOTATIONS_PER_KIND))
        .collect();
    if hidden > 0 {
        out.push(format!(
            "::notice title={}::{}",
            property(NAME),
            data(&format!(
                "{hidden} more annotation(s) not shown — every tick is in the job summary"
            ))
        ));
    }
    out
}

/// The whole check as Markdown: summary, then text (the job summary, the
/// `--markdown` file).
#[must_use]
pub fn markdown(report: &BranchReport, tours: &[String]) -> String {
    let (text, _) = text(report, tours);
    format!(
        "## {NAME}: {}\n\n{}\n{text}",
        report.title(),
        summary(report, 0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use speccoder_core::RequirementDrift;
    use speccoder_core::verify_branch::{BranchTotals, Mode, Tick, TickKind, share};

    fn tick(task: &str, line: u32, outcome: TickOutcome, verdict: Option<&str>) -> TickReport {
        TickReport {
            tick: Tick {
                feature: "001-x".to_owned(),
                task: task.to_owned(),
                title: format!("{task} title"),
                kind: TickKind::Tick,
                anchor: Anchor {
                    path: "specs/001-x/tasks.md".to_owned(),
                    line,
                },
                line_sha256: None,
                claim_sha256: None,
            },
            commit: Some("0123456789abcdef".to_owned()),
            outcome,
            verdict: verdict.map(str::to_owned),
            reason: (outcome == TickOutcome::Unverifiable)
                .then_some(Unverifiable::TickedWithoutEvidence),
            detail: None,
            signer: None,
            decider: (verdict == Some("attested")).then(|| "Ada".to_owned()),
            stale: if outcome == TickOutcome::Stale {
                vec![RequirementDrift {
                    id: "FR-002".to_owned(),
                    then: Some("old".to_owned()),
                    now: Some("new".to_owned()),
                }]
            } else {
                Vec::new()
            },
            stale_anchor: (outcome == TickOutcome::Stale).then(|| Anchor {
                path: "specs/001-x/spec.md".to_owned(),
                line: 9,
            }),
            newer_fields: Vec::new(),
            verification: None,
        }
    }

    fn report(ticks: Vec<TickReport>) -> BranchReport {
        let conclusion = Conclusion::of(&ticks, false, &[]);
        BranchReport {
            head: "h".to_owned(),
            base: None,
            mode: Mode::All,
            features: vec!["001-x".to_owned()],
            totals: BranchTotals::of(&ticks, 0),
            share: share(&ticks),
            ticks,
            skipped: Vec::new(),
            conclusion,
            trust: json!({ "ssh_allowed_signers": { "rev": "base", "path": ".github/speccoder/allowed_signers" } }),
            accepted_unsigned: false,
            require_verified: false,
            trace: Vec::new(),
            open_no_claim: Vec::new(),
            not_evaluated: vec![D121_REGRESSIONS.to_owned()],
            newer_fields: vec!["provenance".to_owned()],
        }
    }

    fn mixed() -> BranchReport {
        use TickOutcome::*;
        let mut r = report(vec![
            tick("T1", 3, Evidenced, Some("verified")),
            tick("T2", 4, Evidenced, Some("regression_only")),
            tick("T3", 5, Evidenced, Some("attested")),
            tick("T4", 6, Stale, Some("verified")),
            tick("T5", 7, Unverifiable, None),
            tick("T6", 8, Regressed, Some("verified")),
        ]);
        r.ticks[2].decider = Some("Ada".to_owned());
        r
    }

    #[test]
    fn one_annotation_per_tick_that_is_not_plainly_verified_at_its_level_and_line() {
        let r = mixed();
        let a = annotations(&r);
        let got: Vec<(&str, &str, u32)> = a
            .iter()
            .map(|a| (a.annotation_level, a.path.as_str(), a.start_line))
            .collect();
        assert_eq!(
            got,
            [
                ("warning", "specs/001-x/tasks.md", 4),
                ("notice", "specs/001-x/tasks.md", 5),
                ("warning", "specs/001-x/spec.md", 9),
                ("failure", "specs/001-x/tasks.md", 7),
                ("failure", "specs/001-x/tasks.md", 8),
            ]
        );
        assert_eq!(a[1].title, "T3 attested by Ada");
        assert_eq!(a[2].title, "T4 stale: FR-002 changed");
        assert_eq!(a[3].title, "T5 unverifiable: ticked_without_evidence");
    }

    #[test]
    fn the_summary_leads_with_the_bar_and_names_what_is_not_evaluated() {
        let r = mixed();
        let s = summary(&r, 0);
        assert!(
            s.starts_with("**verified 1 · regression only 1 · attested 1 · stale 1 · unverifiable 1 · regressed 1** — 3 tick(s) without verifying evidence."),
            "{s}"
        );
        assert!(
            s.contains("earlier-task regressions (D-121) not checked by this verifier version")
        );
        assert!(s.contains("fields from a newer predicate revision not shown: provenance"));
        assert!(s.contains("`.github/speccoder/allowed_signers` at `base`"));
        assert_eq!(
            r.title(),
            "failure: 1 verified, 1 regression only, 1 attested, 1 stale, 1 unverifiable, 1 regressed"
        );
    }

    #[test]
    fn sixty_annotations_go_fifty_in_the_create_and_ten_in_an_update() {
        let ticks: Vec<TickReport> = (1..=60)
            .map(|i| tick(&format!("T{i}"), i, TickOutcome::Unverifiable, None))
            .collect();
        let r = report(ticks);
        let req = request(&r, &[], NAME, false);
        assert_eq!(
            req["create"]["output"]["annotations"]
                .as_array()
                .unwrap()
                .len(),
            50
        );
        assert_eq!(req["updates"].as_array().unwrap().len(), 1);
        assert_eq!(
            req["updates"][0]["output"]["annotations"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        assert_eq!(req["create"]["head_sha"], "h");
        assert_eq!(req["create"]["conclusion"], "failure");
        // The fallback: ten per kind, the rest counted.
        let lines = workflow_commands(&r);
        assert_eq!(lines.len(), 11);
        assert!(
            lines[10].contains("50 more annotation(s) not shown"),
            "{}",
            lines[10]
        );
        assert!(lines[0].starts_with("::error file=specs/001-x/tasks.md,line=1,title=T1 unverifiable%3A ticked_without_evidence::"));
    }

    #[test]
    fn neutral_is_published_as_failure_unless_allowed() {
        let r = report(Vec::new());
        assert_eq!(r.conclusion, Conclusion::Neutral);
        assert_eq!(
            request(&r, &[], NAME, false)["create"]["conclusion"],
            "failure"
        );
        assert_eq!(
            request(&r, &[], NAME, true)["create"]["conclusion"],
            "neutral"
        );
        assert_eq!(
            request(&r, &[], NAME, false)["create"]["output"]["title"],
            "neutral: no ticks checked"
        );
    }

    #[test]
    fn summary_and_text_stay_within_the_cap_and_count_what_was_cut() {
        let ticks: Vec<TickReport> = (1..=400)
            .map(|i| tick(&format!("T{i}"), i, TickOutcome::Unverifiable, None))
            .collect();
        let r = report(ticks);
        let tours: Vec<String> = (0..400)
            .map(|i| format!("<details>{}</details>\n", "x".repeat(400 + i)))
            .collect();
        let (t, omitted) = text(&r, &tours);
        assert!(t.chars().count() <= TEXT_BUDGET);
        assert!(omitted > 0);
        assert!(t.contains(&format!("{omitted} tour(s) not shown")));
        let s = summary(&r, 350);
        assert!(s.chars().count() <= TEXT_BUDGET);
        assert!(s.contains("350 annotation(s) not shown"));
        let long = "line\n".repeat(20_000);
        let c = cut(&long, 1000, "x");
        assert!(c.chars().count() <= 1000, "{}", c.chars().count());
        assert!(c.contains("line(s) of x not shown"));
    }
}
