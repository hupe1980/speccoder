//! The merge forecast: a feature's branch paired with each other open
//! branch — textual conflicts from the merge, and the frozen oracles run on
//! the merged tree. A scenario breaks only when its test passes on its own
//! branch and fails on the merge; nothing here ever says *safe*.

use serde::{Deserialize, Serialize};

/// Siblings checked unless named.
pub const LIMIT: usize = 20;

/// What became of the fetch before the forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum FetchState {
    /// Every remote was fetched.
    Fetched,
    /// `--no-fetch`: branches as last fetched.
    Skipped,
    /// The repository has no remote.
    NoRemote,
    /// The fetch failed: branches as last fetched.
    Failed {
        /// git's reason, one line.
        why: String,
    },
}

/// How a pair comes out.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PairVerdict {
    /// The merge cannot be made: files edited on both sides.
    Conflicts,
    /// The merge is made and breaks a scenario, or rewrites an oracle's tests.
    Breaks,
    /// Not judged by the oracles (`why` says why); the textual check passed.
    NotJudged,
    /// The merge is made and every scenario that held still holds.
    NoConflictObserved,
}

impl PairVerdict {
    /// A person must look.
    #[must_use]
    pub const fn is_problem(self) -> bool {
        matches!(self, Self::Conflicts | Self::Breaks)
    }
}

/// A scenario that holds on its own branch and fails on the merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrokenScenario {
    /// Whose oracle: the unit.
    pub unit: String,
    /// The forecast's own unit's oracle.
    pub mine: bool,
    /// The scenario (`US1-2`).
    pub scenario: String,
    /// `given → must hold`, as the oracle commit states it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// The failing test's file.
    pub test: String,
    /// The end of its output on the merge.
    pub output: String,
}

/// One sibling paired with the forecast's branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PairForecast {
    /// The sibling as git names it (`origin/dana/csv`).
    pub branch: String,
    /// Its tip.
    pub commit: String,
    /// Its last commit's author.
    pub author: String,
    /// Its last commit's time (Unix seconds).
    pub updated: u64,
    /// How the pair comes out.
    pub verdict: PairVerdict,
    /// Conflicting files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// BrokenScenario scenarios.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub broken: Vec<BrokenScenario>,
    /// Frozen oracle tests the merge changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewritten: Vec<String>,
    /// The merge does not set up here (its dependencies do not install, its
    /// lockfile disagrees): what the install said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// Tests that failed once on the merge, then passed: named, not counted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flaky: Vec<String>,
    /// Scenarios that passed on the merge.
    pub held: usize,
    /// Why the oracles did not judge it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The sibling's own oracle ran too.
    pub their_oracle: bool,
    /// One per problem.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
}

/// The oracles the forecast judged with, on the forecast's branch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleFacts {
    /// Stories with a frozen oracle here.
    pub stories: Vec<String>,
    /// Their scenarios.
    pub scenarios: usize,
    /// Their tests.
    pub tests: usize,
    /// Scenarios passing on the forecast's branch: the ones a merge can break.
    pub green: usize,
}

/// `speccoder forecast`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForecastReport {
    /// The unit.
    pub feature: String,
    /// Its branch.
    pub branch: String,
    /// Its tip.
    pub commit: String,
    /// What siblings are measured from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// The fetch.
    pub fetch: FetchState,
    /// The oracles on this branch.
    pub oracle: OracleFacts,
    /// Problems first.
    pub pairs: Vec<PairForecast>,
    /// Siblings past the limit, newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unchecked: Vec<String>,
    /// What limited the forecast (no sandbox, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    /// Test commands run this time.
    pub ran: usize,
    /// Test results reused from earlier forecasts.
    pub reused: usize,
    /// When (Unix seconds).
    pub at: u64,
}

impl ForecastReport {
    /// The pairs a person must look at.
    pub fn problems(&self) -> impl Iterator<Item = &PairForecast> {
        self.pairs.iter().filter(|p| p.verdict.is_problem())
    }

    /// One line: how many siblings conflict with or break this branch.
    #[must_use]
    pub fn headline(&self) -> String {
        let n = self.pairs.len();
        let bad = self.problems().count();
        let clean = self
            .pairs
            .iter()
            .filter(|p| p.verdict == PairVerdict::NoConflictObserved)
            .count();
        let of = if n == 1 { "branch" } else { "branches" };
        if n == 0 {
            "no other branch to check".to_owned()
        } else if bad == 0 && clean == n {
            format!("no conflict observed with {n} {of}")
        } else if bad == 0 {
            format!("no conflict observed with {clean} of {n} {of}; the rest not judged")
        } else {
            format!(
                "{bad} of {n} {of} {} with yours",
                if bad == 1 { "conflicts" } else { "conflict" }
            )
        }
    }
}

/// How a pair comes out from what was found: conflicting files first,
/// then broken scenarios or rewritten tests, then a reason not to judge (a
/// merge that does not set up here is one: a dependency missing on this
/// machine looks the same as a lockfile the merge broke).
#[must_use]
pub fn verdict(p: &PairForecast) -> PairVerdict {
    if !p.files.is_empty() {
        PairVerdict::Conflicts
    } else if !p.broken.is_empty() || !p.rewritten.is_empty() {
        PairVerdict::Breaks
    } else if p.why.is_some() {
        PairVerdict::NotJudged
    } else {
        PairVerdict::NoConflictObserved
    }
}

/// Problems first, then not judged, then clean; newest first within each.
pub fn order(pairs: &mut [PairForecast]) {
    pairs.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then(b.updated.cmp(&a.updated))
            .then(a.branch.cmp(&b.branch))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(branch: &str, verdict: PairVerdict, updated: u64) -> PairForecast {
        PairForecast {
            branch: branch.into(),
            commit: "c".into(),
            author: "a".into(),
            updated,
            verdict,
            files: Vec::new(),
            broken: Vec::new(),
            rewritten: Vec::new(),
            setup: None,
            flaky: Vec::new(),
            held: 0,
            why: None,
            their_oracle: false,
            fixes: Vec::new(),
        }
    }

    #[test]
    fn a_conflict_outranks_a_break_and_a_reason_never_hides_a_problem() {
        let b = BrokenScenario {
            unit: "001".into(),
            mine: true,
            scenario: "US1-1".into(),
            contract: None,
            test: "t".into(),
            output: String::new(),
        };
        let mut p = pair("x", PairVerdict::NotJudged, 0);
        assert_eq!(verdict(&p), PairVerdict::NoConflictObserved);
        p.why = Some("docs only".into());
        assert_eq!(verdict(&p), PairVerdict::NotJudged);
        p.setup = Some("lockfile".into());
        assert_eq!(verdict(&p), PairVerdict::NotJudged);
        p.setup = None;
        p.rewritten = vec!["t".into()];
        assert_eq!(verdict(&p), PairVerdict::Breaks);
        p.rewritten.clear();
        p.broken = vec![b];
        assert_eq!(verdict(&p), PairVerdict::Breaks);
        p.files = vec!["a.rs".into()];
        assert_eq!(verdict(&p), PairVerdict::Conflicts);
    }

    #[test]
    fn problems_lead_newest_first() {
        let mut p = vec![
            pair("clean", PairVerdict::NoConflictObserved, 9),
            pair("docs", PairVerdict::NotJudged, 9),
            pair("old-clash", PairVerdict::Conflicts, 1),
            pair("break", PairVerdict::Breaks, 5),
            pair("new-clash", PairVerdict::Conflicts, 7),
        ];
        order(&mut p);
        let names: Vec<_> = p.iter().map(|p| p.branch.as_str()).collect();
        assert_eq!(names, ["new-clash", "old-clash", "break", "docs", "clean"]);
    }

    #[test]
    fn the_headline_never_says_safe() {
        let mut r = ForecastReport {
            feature: "f".into(),
            branch: "b".into(),
            commit: "c".into(),
            base: None,
            fetch: FetchState::Skipped,
            oracle: OracleFacts::default(),
            pairs: Vec::new(),
            unchecked: Vec::new(),
            notices: Vec::new(),
            ran: 0,
            reused: 0,
            at: 0,
        };
        assert_eq!(r.headline(), "no other branch to check");
        r.pairs = vec![pair("a", PairVerdict::NoConflictObserved, 1)];
        assert_eq!(r.headline(), "no conflict observed with 1 branch");
        r.pairs.push(pair("c", PairVerdict::NotJudged, 1));
        assert_eq!(
            r.headline(),
            "no conflict observed with 1 of 2 branches; the rest not judged"
        );
        r.pairs.pop();
        r.pairs.push(pair("b", PairVerdict::Breaks, 1));
        assert_eq!(r.headline(), "1 of 2 branches conflicts with yours");
        assert!(!r.headline().contains("safe"));
    }
}
