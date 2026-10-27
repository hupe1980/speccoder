//! A branch's ticks and what their evidence comes to: the payload of
//! `speccoder-verify check` and `speccoder attest verify --branch`
//! (`branch_verify`), the rule that turns per-tick outcomes into the
//! branch's conclusion, and the badge.
//!
//! The rules are here, pure, so every interface (the binary, the check
//! run, the badge) concludes the same from the same ticks.

use serde::Serialize;

use crate::{ObjectVerification, RequirementDrift, SignerView, TraceRow, VerifyOutcome};

/// `not_evaluated`: earlier-task regressions have no signed record
/// this verifier could read yet.
pub const EARLIER_TASK_REGRESSIONS: &str = "earlier_task_regressions";
/// `not_evaluated`: a trailer-only bug task (`Spec-Task: <unit>/bug:<slug>`)
/// is in the checked range, and this version cannot check it.
pub const TRAILER_ONLY_TASKS: &str = "trailer_only_tasks";

/// Where an annotation lands: a file and a 1-based line, at the head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Anchor {
    /// Repository-relative path.
    pub path: String,
    /// 1-based line.
    pub line: u32,
}

/// What a checked task is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TickKind {
    /// A `[X]` line in a task list.
    Tick,
    /// A closed task of a format home without a task list: its verdict
    /// commit is the claim (a bug's `bug:<slug>`).
    TrailerOnly,
}

/// A ticked task at the checked head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Tick {
    /// The unit, as the format adapter names it.
    pub feature: String,
    /// Task ID (`T012`, `1.3`, `bug:<slug>`).
    pub task: String,
    /// The task's title.
    pub title: String,
    /// A tick or a trailer-only task.
    pub kind: TickKind,
    /// The task line (a tick) or the home's assessment (trailer-only).
    pub anchor: Anchor,
    /// sha256 of the task line with its box read as open: what the
    /// predicate's `task.line_hash` must equal. `None` for trailer-only.
    pub line_sha256: Option<String>,
    /// For trailer-only: the claim's digest at the head (the bug's triad).
    pub claim_sha256: Option<String>,
}

/// Why a tick's evidence does not verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unverifiable {
    /// No commit carries the task's verdict (ticked by hand).
    TickedWithoutEvidence,
    /// The task commit has no attestation note.
    Missing,
    /// A line of the note does not parse, or the predicate is not v0.
    Malformed,
    /// A signature does not verify.
    BadSignature,
    /// A statement disagrees with the commit, its trailers, the task line
    /// or the bundle.
    Mismatch,
    /// Signed by a key the verifier does not trust.
    UntrustedSigner,
    /// Not signed (unverifiable unless the caller accepts unsigned ticks).
    Unsigned,
    /// The commit the evidence names is not reachable from the head (a
    /// squash or rebase merge).
    CommitUnreachable,
    /// The signature's tool (`ssh-keygen`, `gpg`, `cosign`) cannot run.
    ToolMissing,
}

impl Unverifiable {
    /// The reason an `attest verify` outcome is, `None` for `ok`.
    #[must_use]
    pub const fn of(outcome: VerifyOutcome) -> Option<Self> {
        match outcome {
            VerifyOutcome::Ok => None,
            VerifyOutcome::Unsigned => Some(Self::Unsigned),
            VerifyOutcome::Missing => Some(Self::Missing),
            VerifyOutcome::BadSignature => Some(Self::BadSignature),
            VerifyOutcome::UntrustedSigner => Some(Self::UntrustedSigner),
            VerifyOutcome::Mismatch => Some(Self::Mismatch),
            VerifyOutcome::Malformed => Some(Self::Malformed),
            VerifyOutcome::ToolMissing => Some(Self::ToolMissing),
        }
    }

    /// The word reports use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::TickedWithoutEvidence => "ticked_without_evidence",
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::BadSignature => "bad_signature",
            Self::Mismatch => "mismatch",
            Self::UntrustedSigner => "untrusted_signer",
            Self::Unsigned => "unsigned",
            Self::CommitUnreachable => "commit_unreachable",
            Self::ToolMissing => "tool_missing",
        }
    }
}

/// A tick's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TickOutcome {
    /// The evidence verifies; `verdict` says what it shows.
    Evidenced,
    /// The evidence verifies, but a requirement the task cites changed
    /// since the tick: it is for the earlier wording.
    Stale,
    /// The evidence does not verify; `reason` says why.
    Unverifiable,
    /// Reserved: a later signed checkpoint marks this earlier task
    /// regressed. No producer yet; it fails like `unverifiable`.
    Regressed,
}

/// One tick's report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TickReport {
    /// The tick.
    pub tick: Tick,
    /// The task commit checked (the latest reachable with the task's
    /// verdict trailers), when one exists.
    pub commit: Option<String>,
    /// The outcome.
    pub outcome: TickOutcome,
    /// `verified`, `regression_only` or `attested`, as the evidence claims.
    pub verdict: Option<String>,
    /// For `unverifiable`.
    pub reason: Option<Unverifiable>,
    /// More on the reason, in one line.
    pub detail: Option<String>,
    /// Who signed.
    pub signer: Option<SignerView>,
    /// Who accepted an `attested` tick.
    pub decider: Option<String>,
    /// For `stale`: the cited requirements that changed since the tick.
    pub stale: Vec<RequirementDrift>,
    /// For `stale`: the first changed requirement's line at the head.
    pub stale_anchor: Option<Anchor>,
    /// A regression-only tick: why, in speccoder's wording (signed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression: Option<String>,
    /// Predicate fields or revisions this verifier does not know (kept,
    /// never deciding).
    pub newer_fields: Vec<String>,
    /// `attest verify`'s object for the commit, unchanged.
    pub verification: Option<ObjectVerification>,
}

impl TickReport {
    /// Whether the tick fails the branch.
    #[must_use]
    pub fn fails(&self, require_verified: bool) -> bool {
        match self.outcome {
            TickOutcome::Evidenced => {
                require_verified && self.verdict.as_deref() != Some("verified")
            }
            TickOutcome::Stale | TickOutcome::Unverifiable | TickOutcome::Regressed => true,
        }
    }
}

/// A task skipped by a person's decision: no tick, not counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SkippedTask {
    /// The unit.
    pub feature: String,
    /// Task ID.
    pub task: String,
}

/// Which units a check covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Every unit whose task list or intent differs between base and head.
    Touched,
    /// Every unit (the badge, the default branch).
    All,
}

/// The branch's conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Conclusion {
    /// Every tick carries verifying evidence.
    Success,
    /// A tick is unverifiable, stale or regressed (or not verified while
    /// required), or a claim in range cannot be checked.
    Failure,
    /// Nothing to check: never success.
    Neutral,
}

impl Conclusion {
    /// The word reports and check runs use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Neutral => "neutral",
        }
    }

    /// The verifier's exit status: 0, 1, or 78 (neutral).
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::Neutral => 78,
        }
    }

    /// The rule (R7): any failing tick, or a claim this version cannot
    /// check, is failure; nothing to check is neutral; else success.
    #[must_use]
    pub fn of(ticks: &[TickReport], require_verified: bool, not_evaluated: &[String]) -> Self {
        if ticks.iter().any(|t| t.fails(require_verified))
            || not_evaluated.iter().any(|n| n == TRAILER_ONLY_TASKS)
        {
            Self::Failure
        } else if ticks.is_empty() {
            Self::Neutral
        } else {
            Self::Success
        }
    }
}

/// Counts by outcome, in the verdict bar's order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct BranchTotals {
    /// Evidenced `verified`.
    pub verified: usize,
    /// Evidenced `regression_only`.
    pub regression_only: usize,
    /// Evidenced `attested`.
    pub attested: usize,
    /// Stale.
    pub stale: usize,
    /// Unverifiable.
    pub unverifiable: usize,
    /// Regressed (reserved).
    pub regressed: usize,
    /// Skipped by a person (not ticks).
    pub skipped: usize,
}

impl BranchTotals {
    /// The counts of `ticks` (and `skipped` skips).
    #[must_use]
    pub fn of(ticks: &[TickReport], skipped: usize) -> Self {
        let mut t = Self {
            skipped,
            ..Self::default()
        };
        for r in ticks {
            match (r.outcome, r.verdict.as_deref()) {
                (TickOutcome::Evidenced, Some("verified")) => t.verified += 1,
                (TickOutcome::Evidenced, Some("attested")) => t.attested += 1,
                (TickOutcome::Evidenced, _) => t.regression_only += 1,
                (TickOutcome::Stale, _) => t.stale += 1,
                (TickOutcome::Unverifiable, _) => t.unverifiable += 1,
                (TickOutcome::Regressed, _) => t.regressed += 1,
            }
        }
        t
    }
}

/// Result of `speccoder-verify check` / `speccoder attest verify --branch`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BranchReport {
    /// The commit checked.
    pub head: String,
    /// The merge base with the given base, when one was given.
    pub base: Option<String>,
    /// Which units were covered.
    pub mode: Mode,
    /// The units checked, in order.
    pub features: Vec<String>,
    /// Every tick (and trailer-only task), in document order per unit.
    pub ticks: Vec<TickReport>,
    /// Tasks skipped by a person's decision (informational).
    pub skipped: Vec<SkippedTask>,
    /// Counts by outcome.
    pub totals: BranchTotals,
    /// The conclusion.
    pub conclusion: Conclusion,
    /// The trust used: the allowed-signers file and the revision it was
    /// read at, the Sigstore identity and issuer.
    pub trust: serde_json::Value,
    /// Unsigned evidence was accepted (`--require-signed=false`).
    pub accepted_unsigned: bool,
    /// `regression_only` and `attested` fail too.
    pub require_verified: bool,
    /// Requirement → task → test → verdict, from the signed `trace[]`,
    /// with a row (no task) for each requirement no tick cites.
    pub trace: Vec<TraceRow>,
    /// Evidenced (not stale) over all ticks; `None` without ticks.
    pub share: Option<f64>,
    /// Bug homes touched without a verdict commit (informational).
    pub open_no_claim: Vec<String>,
    /// Rules this verifier version does not evaluate yet.
    pub not_evaluated: Vec<String>,
    /// Predicate fields or revisions seen but unknown here (tolerated).
    pub newer_fields: Vec<String>,
}

impl BranchReport {
    /// The check run's title: `failure: 7 verified, 1 regression only, …`;
    /// a neutral outcome says there was nothing to check.
    #[must_use]
    pub fn title(&self) -> String {
        if self.conclusion == Conclusion::Neutral {
            return "neutral: no ticks checked".to_owned();
        }
        let t = &self.totals;
        let mut title = format!(
            "{}: {} verified, {} regression only, {} attested, {} stale, {} unverifiable",
            self.conclusion.word(),
            t.verified,
            t.regression_only,
            t.attested,
            t.stale,
            t.unverifiable
        );
        if t.regressed > 0 {
            title.push_str(&format!(", {} regressed", t.regressed));
        }
        title
    }
}

/// Evidenced (not stale) over all ticks; `None` with none.
#[must_use]
pub fn share(ticks: &[TickReport]) -> Option<f64> {
    if ticks.is_empty() {
        return None;
    }
    let evidenced = ticks
        .iter()
        .filter(|t| t.outcome == TickOutcome::Evidenced)
        .count();
    #[allow(clippy::cast_precision_loss)] // counts of ticks
    Some(evidenced as f64 / ticks.len() as f64)
}

/// The badge: a shields.io endpoint document, and the same as an SVG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Badge {
    /// The endpoint schema's version (1).
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// `evidence`.
    pub label: String,
    /// `80%`, or `none`.
    pub message: String,
    /// `brightgreen`, `yellow`, `red`, `lightgrey`.
    pub color: String,
}

impl Badge {
    /// The badge of a report: ≥ 95 % green, ≥ 75 % yellow, else red; never
    /// green with an unverifiable tick; `none` without ticks.
    #[must_use]
    pub fn of(report: &BranchReport) -> Self {
        let (message, color) = match report.share {
            None => ("none".to_owned(), "lightgrey"),
            Some(s) => {
                let percent = (s * 100.0).floor();
                let color = if percent >= 95.0 && report.totals.unverifiable == 0 {
                    "brightgreen"
                } else if percent >= 75.0 {
                    "yellow"
                } else {
                    "red"
                };
                (format!("{percent:.0}%"), color)
            }
        };
        Self {
            schema_version: 1,
            label: "evidence".to_owned(),
            message,
            color: color.to_owned(),
        }
    }

    /// The badge as a flat SVG, its text the same as the endpoint's.
    #[must_use]
    pub fn svg(&self) -> String {
        let fill = match self.color.as_str() {
            "brightgreen" => "#4c1",
            "yellow" => "#dfb317",
            "red" => "#e05d44",
            _ => "#9f9f9f",
        };
        // Roughly 7 px per character of 11 px Verdana, plus padding.
        let width = |s: &str| u32::try_from(s.chars().count()).unwrap_or(8) * 7 + 10;
        let (lw, mw) = (width(&self.label), width(&self.message));
        let total = lw + mw;
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total}\" height=\"20\" role=\"img\" aria-label=\"{label}: {message}\">\
<title>{label}: {message}</title>\
<rect width=\"{lw}\" height=\"20\" fill=\"#555\"/>\
<rect x=\"{lw}\" width=\"{mw}\" height=\"20\" fill=\"{fill}\"/>\
<g fill=\"#fff\" text-anchor=\"middle\" font-family=\"Verdana,Geneva,DejaVu Sans,sans-serif\" font-size=\"11\">\
<text x=\"{lx}\" y=\"14\">{label}</text><text x=\"{mx}\" y=\"14\">{message}</text></g></svg>\n",
            label = self.label,
            message = self.message,
            lx = lw / 2,
            mx = lw + mw / 2,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(task: &str, outcome: TickOutcome, verdict: Option<&str>) -> TickReport {
        TickReport {
            tick: Tick {
                feature: "001-x".to_owned(),
                task: task.to_owned(),
                title: String::new(),
                kind: TickKind::Tick,
                anchor: Anchor {
                    path: "specs/001-x/tasks.md".to_owned(),
                    line: 1,
                },
                line_sha256: None,
                claim_sha256: None,
            },
            commit: None,
            outcome,
            verdict: verdict.map(str::to_owned),
            reason: (outcome == TickOutcome::Unverifiable)
                .then_some(Unverifiable::TickedWithoutEvidence),
            detail: None,
            signer: None,
            decider: None,
            stale: Vec::new(),
            stale_anchor: None,
            regression: None,
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
            trust: serde_json::Value::Null,
            accepted_unsigned: false,
            require_verified: false,
            trace: Vec::new(),
            open_no_claim: Vec::new(),
            not_evaluated: Vec::new(),
            newer_fields: Vec::new(),
        }
    }

    #[test]
    fn the_conclusion_is_failure_on_any_failing_tick_and_never_success_without_ticks() {
        use TickOutcome::*;
        let ok = tick("T1", Evidenced, Some("verified"));
        let ro = tick("T2", Evidenced, Some("regression_only"));
        assert_eq!(Conclusion::of(&[], false, &[]), Conclusion::Neutral);
        assert_eq!(
            Conclusion::of(&[ok.clone(), ro.clone()], false, &[]),
            Conclusion::Success
        );
        assert_eq!(
            Conclusion::of(&[ok.clone(), ro], true, &[]),
            Conclusion::Failure
        );
        for bad in [Stale, Unverifiable, Regressed] {
            assert_eq!(
                Conclusion::of(&[ok.clone(), tick("T3", bad, None)], false, &[]),
                Conclusion::Failure,
                "{bad:?}"
            );
        }
        // A claim this version cannot check is not proof.
        assert_eq!(
            Conclusion::of(&[], false, &[TRAILER_ONLY_TASKS.to_owned()]),
            Conclusion::Failure
        );
        assert_eq!(
            Conclusion::of(&[ok], false, &[EARLIER_TASK_REGRESSIONS.to_owned()]),
            Conclusion::Success
        );
    }

    #[test]
    fn the_title_counts_in_the_bars_order() {
        use TickOutcome::*;
        let r = report(vec![
            tick("T1", Evidenced, Some("verified")),
            tick("T2", Evidenced, Some("attested")),
            tick("T3", Unverifiable, None),
        ]);
        assert_eq!(
            r.title(),
            "failure: 1 verified, 0 regression only, 1 attested, 0 stale, 1 unverifiable"
        );
        assert_eq!(report(Vec::new()).title(), "neutral: no ticks checked");
    }

    #[test]
    fn the_badge_reads_eight_of_ten_as_eighty_percent_and_never_green_with_an_unverifiable_tick() {
        use TickOutcome::*;
        let mut ticks: Vec<TickReport> = (0..8)
            .map(|i| tick(&format!("T{i}"), Evidenced, Some("verified")))
            .collect();
        ticks.push(tick("T8", Stale, Some("verified")));
        ticks.push(tick("T9", Unverifiable, None));
        let b = Badge::of(&report(ticks));
        assert_eq!((b.message.as_str(), b.color.as_str()), ("80%", "yellow"));
        assert!(b.svg().contains(">80%<"));
        let mut all: Vec<TickReport> = (0..40)
            .map(|i| tick(&format!("T{i}"), Evidenced, Some("verified")))
            .collect();
        all.push(tick("T99", Unverifiable, None));
        let b = Badge::of(&report(all));
        assert_eq!((b.message.as_str(), b.color.as_str()), ("97%", "yellow"));
        assert_eq!(Badge::of(&report(Vec::new())).message, "none");
    }
}
