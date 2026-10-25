//! Acceptance strength (F-060, D-063, D-085): how well a task's acceptance
//! set tells right from wrong.
//!
//! A red → green acceptance test shows the task's work made it pass; it does
//! not show the test would have failed a wrong implementation. After a
//! `verified` verdict, wrong candidates — mutants of the changed code and
//! model-written near misses — are run against the set: the share it rejects
//! is its strength. Spec-permitted alternatives are run too: one the set
//! rejects is a test that encodes a choice the spec did not make, and only a
//! person can rule on it.
//!
//! These are the rules; running candidates is the runtime's.

use serde::{Deserialize, Serialize};

/// What a candidate run showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The acceptance run failed on an assertion (or a symbol it needs is gone).
    Rejected,
    /// The acceptance run passed: the set accepts this wrong implementation.
    Survived,
    /// The run hit its deadline: a detectable failure, counted as rejected.
    Timeout,
    /// It did not build or the tests could not be collected: not judged.
    Unviable,
}

impl Outcome {
    /// Counted as rejected.
    #[must_use]
    pub const fn rejects(self) -> bool {
        matches!(self, Self::Rejected | Self::Timeout)
    }

    /// Judged at all.
    #[must_use]
    pub const fn judged(self) -> bool {
        !matches!(self, Self::Unviable)
    }
}

/// A model-written near miss: plausible, and wrong about one scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NearMiss {
    /// `near-miss-1`, …
    pub id: String,
    /// The scenario or requirement it gets wrong.
    pub scenario: String,
    /// What it gets wrong, in one line (the author's claim).
    pub rationale: String,
    /// Against the visible set.
    pub visible: Outcome,
    /// Against the visible and the held-out set together.
    pub combined: Outcome,
}

/// Mutants of the changed code, run against the acceptance command only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Mutants {
    /// The tool.
    pub tool: String,
    /// Caught by an assertion.
    pub caught: u32,
    /// Not caught.
    pub missed: u32,
    /// Caught by the deadline.
    pub timeout: u32,
    /// Did not build.
    pub unviable: u32,
    /// The missed ones, as the tool names them.
    pub survivors: Vec<String>,
}

/// What a spec-permitted alternative showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermittedOutcome {
    /// The set accepts it, as it should.
    Accepted,
    /// The set rejects it on an assertion: probably over-strict.
    Rejected,
    /// Rejected, and a person ruled the tests right.
    Ruled,
    /// Timed out: no finding (a hang is not an assertion).
    Timeout,
    /// Did not build: no finding.
    Unviable,
}

/// An alternative the author says the spec allows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Permitted {
    /// `permitted-1`, …
    pub id: String,
    /// The scenario whose open choice it takes differently.
    pub scenario: String,
    /// The freedom it uses (the author's claim).
    pub rationale: String,
    /// `sha256:<hex>` of its files: what a ruling is bound to.
    pub digest: String,
    /// Against the visible and held-out set together.
    pub outcome: PermittedOutcome,
    /// The failing run's last lines, when rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failing: Option<String>,
    /// Where its patch is kept for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

/// How far a measurement got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Everything planned was run.
    Measured,
    /// The budget ran out: what ran is recorded.
    Partial,
    /// Nothing could be judged (no tool, no variant, an error).
    NotMeasured,
    /// The change touches no source file.
    NotApplicable,
}

/// Rejected of judged, for one source.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct Counts {
    /// Rejected (timeouts included).
    pub rejected: u32,
    /// Judged (unviable excluded).
    pub judged: u32,
}

/// One set's strength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Strength {
    /// How far it got.
    pub state: State,
    /// Rejected / judged over every source; `None` when nothing was judged.
    pub share: Option<f64>,
    /// Mutants; `None` when not run.
    pub mutants: Option<Counts>,
    /// Near misses.
    pub near_misses: Counts,
    /// Candidates that did not build.
    pub unviable: u32,
    /// What got through: mutant names and near-miss ids with their scenario.
    pub survivors: Vec<String>,
}

/// The `min_strength` gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// No `min_strength`.
    Off,
    /// Met at once.
    Met,
    /// Met after one regeneration of the acceptance tests.
    RegeneratedMet,
    /// Not met, or not measurable.
    Failed,
}

/// A person's ruling that an over-strict finding was wrong: the tests were
/// right, the alternative is not what the spec allows (D-074).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Ruling {
    /// The alternative's digest.
    pub digest: String,
    /// Its scenario.
    pub scenario: String,
    /// Who ruled.
    pub by: String,
    /// Why.
    pub reason: String,
}

/// The acceptance set's strength, as recorded with a verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AcceptanceStrength {
    /// The visible set alone.
    pub visible: Strength,
    /// Visible and held-out together: what the verdict rests on (equal to
    /// `visible` without held-out tests). Mutants run with the visible
    /// command only, so its mutant counts are a lower bound.
    pub combined: Strength,
    /// A held-out set was overlaid.
    pub held_out: bool,
    /// The mutants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutants: Option<Mutants>,
    /// The near misses.
    pub near_misses: Vec<NearMiss>,
    /// The permitted alternatives.
    pub permitted: Vec<Permitted>,
    /// Rulings that applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rulings: Vec<Ruling>,
    /// The gate.
    pub gate: Gate,
    /// Why not (fully) measured, or what was dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The near-miss call's run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// The near-miss call's tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
    /// How long the stage took.
    pub duration_ms: u64,
}

/// Which set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set {
    /// The visible tests.
    Visible,
    /// Visible and held-out.
    Combined,
}

/// One set's strength from what ran. `partial`: the budget ran out.
#[must_use]
pub fn strength(
    mutants: Option<&Mutants>,
    near_misses: &[NearMiss],
    set: Set,
    partial: bool,
) -> Strength {
    let mut unviable = 0;
    let mut survivors = Vec::new();
    let mutant_counts = mutants.map(|m| {
        unviable += m.unviable;
        survivors.extend(m.survivors.iter().cloned());
        Counts {
            rejected: m.caught + m.timeout,
            judged: m.caught + m.timeout + m.missed,
        }
    });
    let mut near = Counts::default();
    for n in near_misses {
        let o = match set {
            Set::Visible => n.visible,
            Set::Combined => n.combined,
        };
        if !o.judged() {
            unviable += 1;
            continue;
        }
        near.judged += 1;
        if o.rejects() {
            near.rejected += 1;
        } else {
            survivors.push(format!("{} ({}: {})", n.id, n.scenario, n.rationale));
        }
    }
    let rejected = mutant_counts.map_or(0, |c| c.rejected) + near.rejected;
    let judged = mutant_counts.map_or(0, |c| c.judged) + near.judged;
    let share = (judged > 0).then(|| f64::from(rejected) / f64::from(judged));
    let state = match (share, partial) {
        (None, _) => State::NotMeasured,
        (Some(_), true) => State::Partial,
        (Some(_), false) => State::Measured,
    };
    Strength {
        state,
        share,
        mutants: mutant_counts,
        near_misses: near,
        unviable,
        survivors,
    }
}

/// A strength that measured nothing, with its state.
#[must_use]
pub fn unmeasured(state: State) -> Strength {
    Strength {
        state,
        share: None,
        mutants: None,
        near_misses: Counts::default(),
        unviable: 0,
        survivors: Vec::new(),
    }
}

/// The alternatives the set rejected on an assertion and nobody ruled on:
/// probably over-strict tests.
#[must_use]
pub fn over_strict(permitted: &[Permitted]) -> Vec<&Permitted> {
    permitted
        .iter()
        .filter(|p| p.outcome == PermittedOutcome::Rejected)
        .collect()
}

/// The gate for `min`. Only a full measurement can meet it: no evidence is
/// not good evidence, and a partial one never satisfies it.
#[must_use]
pub fn gate(min: Option<f64>, s: &Strength, regenerated: bool) -> Gate {
    let Some(min) = min else {
        return Gate::Off;
    };
    match (s.state, s.share) {
        (State::Measured, Some(share)) if share + f64::EPSILON >= min => {
            if regenerated {
                Gate::RegeneratedMet
            } else {
                Gate::Met
            }
        }
        _ => Gate::Failed,
    }
}

/// `0.83 mutants=9/11 near-miss=1/2 unviable=2`, or `not measured (…)`.
#[must_use]
pub fn summary(s: &Strength, why: Option<&str>) -> String {
    let why = why.map(|w| format!(" ({w})")).unwrap_or_default();
    match (s.state, s.share) {
        (State::NotApplicable, _) => format!("not applicable{why}"),
        (_, None) => format!("not measured{why}"),
        (state, Some(share)) => {
            let mut out = format!("{share:.2}");
            if let Some(m) = s.mutants {
                out.push_str(&format!(" mutants={}/{}", m.rejected, m.judged));
            }
            out.push_str(&format!(
                " near-miss={}/{}",
                s.near_misses.rejected, s.near_misses.judged
            ));
            if s.unviable > 0 {
                out.push_str(&format!(" unviable={}", s.unviable));
            }
            if state == State::Partial {
                out.push_str(" partial");
            }
            out
        }
    }
}

/// The trailers of a verified tick: `Acceptance-Strength:` (the combined
/// set) and, with held-out tests, `Acceptance-Strength-Visible:`.
#[must_use]
pub fn trailers(a: &AcceptanceStrength) -> String {
    let why = a.notes.first().map(String::as_str);
    let mut out = format!("Acceptance-Strength: {}\n", summary(&a.combined, why));
    if a.held_out {
        out.push_str(&format!(
            "Acceptance-Strength-Visible: {}\n",
            summary(&a.visible, None)
        ));
    }
    for r in &a.rulings {
        out.push_str(&format!(
            "Ruling: tests-right {} {} by {} — {}\n",
            r.scenario, r.digest, r.by, r.reason
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn near(id: &str, visible: Outcome, combined: Outcome) -> NearMiss {
        NearMiss {
            id: id.to_owned(),
            scenario: "US1-2".to_owned(),
            rationale: "ignores the second example".to_owned(),
            visible,
            combined,
        }
    }

    fn mutants(caught: u32, missed: u32) -> Mutants {
        Mutants {
            tool: "cargo-mutants".to_owned(),
            caught,
            missed,
            timeout: 1,
            unviable: 2,
            survivors: (0..missed).map(|i| format!("src/lib.rs:{i}")).collect(),
        }
    }

    #[test]
    fn the_share_pools_sources_and_names_survivors() {
        let m = mutants(8, 2);
        let n = [
            near("near-miss-1", Outcome::Rejected, Outcome::Rejected),
            near("near-miss-2", Outcome::Survived, Outcome::Rejected),
            near("near-miss-3", Outcome::Unviable, Outcome::Unviable),
        ];
        let v = strength(Some(&m), &n, Set::Visible, false);
        // 9 of 11 mutants (timeout counts), 1 of 2 near misses.
        assert_eq!(
            v.mutants,
            Some(Counts {
                rejected: 9,
                judged: 11
            })
        );
        assert_eq!(
            v.near_misses,
            Counts {
                rejected: 1,
                judged: 2
            }
        );
        assert_eq!(v.unviable, 3);
        assert!((v.share.unwrap() - 10.0 / 13.0).abs() < 1e-9);
        assert!(
            v.survivors
                .iter()
                .any(|s| s.starts_with("near-miss-2 (US1-2"))
        );
        let c = strength(Some(&m), &n, Set::Combined, false);
        assert!(c.share > v.share);
        assert_eq!(
            summary(&v, None),
            "0.77 mutants=9/11 near-miss=1/2 unviable=3"
        );
    }

    #[test]
    fn nothing_judged_is_not_measured_and_meets_no_gate() {
        let s = strength(
            None,
            &[near("n", Outcome::Unviable, Outcome::Unviable)],
            Set::Visible,
            false,
        );
        assert_eq!(s.state, State::NotMeasured);
        assert_eq!(gate(Some(0.0), &s, false), Gate::Failed);
        assert_eq!(gate(None, &s, false), Gate::Off);
        assert_eq!(
            summary(&s, Some("no mutation tool")),
            "not measured (no mutation tool)"
        );
        assert_eq!(
            summary(&unmeasured(State::NotApplicable), Some("no source change")),
            "not applicable (no source change)"
        );
    }

    #[test]
    fn the_gate_takes_only_a_full_measurement() {
        let n = [near("n", Outcome::Rejected, Outcome::Rejected)];
        let full = strength(None, &n, Set::Combined, false);
        let partial = strength(None, &n, Set::Combined, true);
        assert_eq!(gate(Some(0.8), &full, false), Gate::Met);
        assert_eq!(gate(Some(0.8), &full, true), Gate::RegeneratedMet);
        assert_eq!(gate(Some(0.8), &partial, false), Gate::Failed);
        assert!(summary(&partial, None).ends_with(" partial"));
    }

    #[test]
    fn only_an_unruled_assertion_rejection_is_over_strict() {
        let p = |id: &str, outcome| Permitted {
            id: id.to_owned(),
            scenario: "US2-1".to_owned(),
            rationale: "another order".to_owned(),
            digest: "sha256:ab".to_owned(),
            outcome,
            failing: None,
            patch: None,
        };
        let all = [
            p("a", PermittedOutcome::Accepted),
            p("b", PermittedOutcome::Rejected),
            p("c", PermittedOutcome::Ruled),
            p("d", PermittedOutcome::Timeout),
            p("e", PermittedOutcome::Unviable),
        ];
        let found: Vec<&str> = over_strict(&all).iter().map(|p| p.id.as_str()).collect();
        assert_eq!(found, ["b"]);
    }

    #[test]
    fn trailers_carry_both_sets_and_rulings() {
        let n = [near("n", Outcome::Survived, Outcome::Rejected)];
        let a = AcceptanceStrength {
            visible: strength(None, &n, Set::Visible, false),
            combined: strength(None, &n, Set::Combined, false),
            held_out: true,
            mutants: None,
            near_misses: n.to_vec(),
            permitted: Vec::new(),
            rulings: vec![Ruling {
                digest: "sha256:ab".to_owned(),
                scenario: "US2-1".to_owned(),
                by: "Dana".to_owned(),
                reason: "order is specified in the plan".to_owned(),
            }],
            gate: Gate::Off,
            notes: Vec::new(),
            run_id: None,
            usage: None,
            duration_ms: 1,
        };
        assert_eq!(
            trailers(&a),
            "Acceptance-Strength: 1.00 near-miss=1/1\nAcceptance-Strength-Visible: 0.00 near-miss=0/1\nRuling: tests-right US2-1 sha256:ab by Dana — order is specified in the plan\n"
        );
    }

    fn outcome() -> impl Strategy<Value = Outcome> {
        prop_oneof![
            Just(Outcome::Rejected),
            Just(Outcome::Survived),
            Just(Outcome::Timeout),
            Just(Outcome::Unviable)
        ]
    }

    proptest! {
        #[test]
        fn share_is_a_share_and_moves_the_right_way(
            outcomes in proptest::collection::vec(outcome(), 0..12),
            caught in 0u32..20, missed in 0u32..20,
        ) {
            let m = Mutants { caught, missed, ..Mutants::default() };
            let n: Vec<NearMiss> = outcomes.iter().enumerate()
                .map(|(i, o)| near(&format!("n{i}"), *o, *o)).collect();
            let s = strength(Some(&m), &n, Set::Visible, false);
            if let Some(share) = s.share {
                prop_assert!((0.0..=1.0).contains(&share));
            }
            // An unviable candidate changes nothing; a rejected one never lowers it.
            let mut more = n.clone();
            more.push(near("u", Outcome::Unviable, Outcome::Unviable));
            prop_assert_eq!(strength(Some(&m), &more, Set::Visible, false).share, s.share);
            let mut more = n.clone();
            more.push(near("r", Outcome::Rejected, Outcome::Rejected));
            let after = strength(Some(&m), &more, Set::Visible, false).share.unwrap_or(0.0);
            prop_assert!(after + 1e-12 >= s.share.unwrap_or(0.0));
            // A partial measurement meets no gate.
            prop_assert_eq!(gate(Some(0.0), &strength(Some(&m), &n, Set::Visible, true), false), Gate::Failed);
        }
    }
}
