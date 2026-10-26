//! Bug fixes, the L2 rung: a person's triad — what happens (*Current*),
//! what should (*Expected*), what must stay as it is (*Unchanged*) — one
//! reproduction per *Expected* statement that fails on the untouched code
//! by an assertion, shown to the person once and then frozen, and a fix
//! judged like any task plus three bug rules: every reproduction goes from
//! red to green, the pass-to-pass set stays green, and undoing the fix
//! (the revert) is rejected by at least one reproduction.
//!
//! The rules live here, where they are property-tested; running tests and
//! model calls is the runtime's.

use serde::{Deserialize, Serialize};

use crate::actions::Via;
use crate::next::NextStep;
use crate::verify::{HeldOut, RedClass, Verdict};

/// Reproduce tries when `[bug] reproduce_tries` is not set.
pub const DEFAULT_TRIES: u32 = 3;
/// The most `[bug] reproduce_tries` allows.
pub const MAX_TRIES: u32 = 5;

/// One statement of the triad, in the person's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Statement {
    /// `C` (current), `E1`, `E2`, … (expected), `U1`, … (unchanged):
    /// positional, stable within the requirement.
    pub id: String,
    /// The words, whitespace-normalised.
    pub text: String,
}

/// The person's three statements about a bug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Triad {
    /// What happens now (the symptom).
    pub current: Statement,
    /// What should happen instead; at least one.
    pub expected: Vec<Statement>,
    /// What must keep working as it does.
    pub unchanged: Vec<Statement>,
    /// `sha256:<hex>` over the normalised statements: a different digest
    /// lapses a confirmation.
    pub digest: String,
}

/// Why words are not a triad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriadError {
    /// The symptom or a statement is empty (`BUG-EMPTY`).
    Empty,
    /// No *Expected* statement (`BUG-NO-EXPECTED`).
    NoExpected,
}

impl TriadError {
    /// The catalog code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "BUG-EMPTY",
            Self::NoExpected => "BUG-NO-EXPECTED",
        }
    }

    /// What the person is told.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Empty => {
                "the symptom and every statement need words: `speccoder bug \"<what happens>\" --expected \"<what should>\"`"
            }
            Self::NoExpected => {
                "say what should happen instead: `--expected \"…\"` (one per behaviour)"
            }
        }
    }
}

/// `text` with every run of whitespace one space, trimmed: how statements
/// are compared and hashed.
#[must_use]
pub fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Triad {
    /// The triad of the person's words; `hash` gives the hex SHA-256 of
    /// bytes (the core does no hashing itself).
    ///
    /// # Errors
    ///
    /// An empty symptom or statement, or no *Expected* statement.
    pub fn new(
        current: &str,
        expected: &[String],
        unchanged: &[String],
        hash: impl Fn(&[u8]) -> String,
    ) -> Result<Self, TriadError> {
        let current = normalise(current);
        if current.is_empty() {
            return Err(TriadError::Empty);
        }
        let expected: Vec<String> = expected.iter().map(|e| normalise(e)).collect();
        let unchanged: Vec<String> = unchanged.iter().map(|u| normalise(u)).collect();
        if expected.iter().chain(&unchanged).any(String::is_empty) {
            return Err(TriadError::Empty);
        }
        if expected.is_empty() {
            return Err(TriadError::NoExpected);
        }
        let statements = |prefix: char, texts: Vec<String>| -> Vec<Statement> {
            texts
                .into_iter()
                .enumerate()
                .map(|(i, text)| Statement {
                    id: format!("{prefix}{}", i + 1),
                    text,
                })
                .collect()
        };
        let mut t = Self {
            current: Statement {
                id: "C".to_owned(),
                text: current,
            },
            expected: statements('E', expected),
            unchanged: statements('U', unchanged),
            digest: String::new(),
        };
        t.digest = format!("sha256:{}", hash(t.canonical().as_bytes()));
        Ok(t)
    }

    /// The text the digest is over: one `id<TAB>words` line per statement.
    #[must_use]
    pub fn canonical(&self) -> String {
        self.statements()
            .map(|s| format!("{}\t{}\n", s.id, s.text))
            .collect()
    }

    /// Every statement: current, expected, unchanged.
    pub fn statements(&self) -> impl Iterator<Item = &Statement> {
        std::iter::once(&self.current)
            .chain(&self.expected)
            .chain(&self.unchanged)
    }

    /// The statement `id`.
    #[must_use]
    pub fn statement(&self, id: &str) -> Option<&Statement> {
        self.statements().find(|s| s.id == id)
    }

    /// The *Expected* and *Unchanged* ids, the ones tests cite.
    #[must_use]
    pub fn cited(&self) -> Vec<String> {
        self.expected
            .iter()
            .chain(&self.unchanged)
            .map(|s| s.id.clone())
            .collect()
    }

    /// The triad as labelled lines, the person's words verbatim: what a
    /// format's own bug step is given, and what is looked for on read-back.
    #[must_use]
    pub fn labelled(&self) -> String {
        let mut out = format!("Current: {}\n", self.current.text);
        for e in &self.expected {
            out.push_str(&format!("Expected: {}\n", e.text));
        }
        for u in &self.unchanged {
            out.push_str(&format!("Unchanged: {}\n", u.text));
        }
        out
    }

    /// A short title: the symptom's first sentence, at most eight words.
    #[must_use]
    pub fn title(&self) -> String {
        let first = self
            .current
            .text
            .split(['.', '!', '?'])
            .find(|s| !s.trim().is_empty())
            .unwrap_or(&self.current.text)
            .trim();
        let words: Vec<&str> = first.split_whitespace().take(8).collect();
        let mut t = words.join(" ");
        if first.split_whitespace().count() > 8 {
            t.push('…');
        }
        t
    }
}

/// Where the format keeps the bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomeKind {
    /// Spec Kit's first-party `bug` extension: its own per-bug directory.
    NativeSpeckitBug,
    /// Kiro's `bugfix.md` spec.
    NativeKiroBugfix,
    /// Spec Kit without the extension: the standing bug-fix unit.
    StandingSpeckit,
    /// OpenSpec: the standing bug-fix change.
    StandingOpenspec,
}

impl HomeKind {
    /// The format's own bug structure (no task list: the task is
    /// `bug:<slug>`, ticked nowhere).
    #[must_use]
    pub const fn native(self) -> bool {
        matches!(self, Self::NativeSpeckitBug | Self::NativeKiroBugfix)
    }

    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeSpeckitBug => "native_speckit_bug",
            Self::NativeKiroBugfix => "native_kiro_bugfix",
            Self::StandingSpeckit => "standing_speckit",
            Self::StandingOpenspec => "standing_openspec",
        }
    }
}

/// The bug's home, as the format adapter chose it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BugHome {
    /// Which structure.
    pub kind: HomeKind,
    /// Project-relative directory (or file).
    pub location: String,
    /// Why this one, as shown and recorded.
    pub reason: String,
}

/// Where the bug is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "BugStage")]
pub enum Stage {
    /// The triad is being recorded.
    Record,
    /// Reproductions are being written and shown to fail.
    Reproduce,
    /// The reproductions wait for the person's confirmation.
    Confirm,
    /// The pass-to-pass set is chosen and run.
    Unchanged,
    /// The fix is attempted and judged.
    Fix,
    /// The verdict commit.
    Ship,
    /// Nothing more happens without the person.
    Done,
}

/// The bug's outcome; distinct from a task's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "BugOutcome")]
pub enum Outcome {
    /// The triad is recorded; nothing reproduced yet.
    Recorded,
    /// Every statement is reproduced; the person confirms.
    AwaitingConfirmation,
    /// The tries ran out without a reproduction of every statement.
    CannotReproduce,
    /// Every reproduction passed on the untouched code, on every try.
    AlreadySatisfied,
    /// An *Unchanged* statement does not hold today.
    UnchangedNotHolding,
    /// The fix was judged; see the verdict.
    Fixed,
    /// The fix did not pass.
    Failed,
}

impl Outcome {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::AwaitingConfirmation => "awaiting_confirmation",
            Self::CannotReproduce => "cannot_reproduce",
            Self::AlreadySatisfied => "already_satisfied",
            Self::UnchangedNotHolding => "unchanged_not_holding",
            Self::Fixed => "fixed",
            Self::Failed => "failed",
        }
    }
}

/// Who wrote a reproduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "ReproductionSource")]
pub enum Source {
    /// The test-blind reproduction author.
    Author,
    /// The person (`--test`).
    Person,
}

/// A reproduction on the final tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "FinalState")]
pub enum Final {
    /// It passed.
    Passed,
    /// It failed.
    Failed,
    /// Not run (no attempt yet).
    NotRun,
}

/// A reproduction against the revert candidate (the fix undone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "RevertOutcome")]
pub enum Revert {
    /// It failed on an assertion: it tells the fix from no fix.
    Rejected,
    /// It passed without the fix.
    Survived,
    /// The revert did not build: no evidence either way.
    Unviable,
    /// Not run.
    NotRun,
}

/// One reproduction test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Reproduction {
    /// The statement it reproduces (`E1`) — or, for a test of an
    /// *Unchanged* statement, that one (`U1`).
    pub statement: String,
    /// Project-relative test file.
    pub file: String,
    /// The test's name.
    pub test: String,
    /// Who wrote it.
    pub source: Source,
    /// Its class on the untouched code, run twice.
    pub baseline: Vec<RedClass>,
    /// `sha256:<hex>` of its content.
    pub digest: String,
    /// The command that runs only it.
    pub argv: Vec<String>,
    /// On the final tree.
    #[serde(rename = "final")]
    pub final_: Final,
    /// Against the revert candidate.
    pub revert: Revert,
}

impl Reproduction {
    /// A reproduction of an *Expected* statement (not an *Unchanged* test).
    #[must_use]
    pub fn reproduces(&self) -> bool {
        self.statement.starts_with('E')
    }

    /// Red by assertion on both baseline runs: the bug red rule (an absent
    /// symbol is not a bug's red).
    #[must_use]
    pub fn red(&self) -> bool {
        self.baseline.len() >= 2 && self.baseline.iter().all(|c| *c == RedClass::Assertion)
    }
}

/// A held-out variant, as the payload shows it: never its content or path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HeldOutVariant {
    /// The statement it covers.
    pub statement: String,
    /// On the untouched code.
    pub baseline: RedClass,
    /// On the final tree.
    #[serde(rename = "final")]
    pub final_: Final,
}

/// One statement's class in one try.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TryStatement {
    /// `E1`, `U1`.
    pub statement: String,
    /// What its test did on the untouched code (`assertion` only when both
    /// runs failed on one).
    pub class: RedClass,
    /// Up to 20 lines of the run, when it was not red.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

/// One try of the reproduction author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReproduceTry {
    /// 1, 2, …
    pub n: u32,
    /// Per statement.
    pub per_statement: Vec<TryStatement>,
    /// Why the whole try was discarded (`wrote outside tests`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discarded: Option<String>,
    /// The author's run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Its tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
    /// The person's reason for rejecting the reproductions before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection: Option<String>,
}

/// What reproduce decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reproduce {
    /// Every *Expected* statement is red by assertion in one try.
    Reproduced {
        /// The try (1-based).
        n: u32,
    },
    /// Every statement passed on every try.
    AlreadySatisfied,
    /// Another try is allowed.
    Retry,
    /// The tries ran out.
    CannotReproduce {
        /// Red by assertion in some try.
        reproduced: Vec<String>,
        /// Passed in every try that ran it.
        satisfied: Vec<String>,
        /// Neither.
        not_demonstrated: Vec<String>,
    },
}

/// The class of `statement` in `t`, if it ran there.
fn class_in<'t>(t: &'t ReproduceTry, statement: &str) -> Option<&'t RedClass> {
    t.per_statement
        .iter()
        .find(|s| s.statement == statement)
        .map(|s| &s.class)
}

/// What the tries so far decide, for the *Expected* statements `expected`
/// and at most `max` tries. Pure; never reads the author's words.
#[must_use]
pub fn decide_reproduce(tries: &[ReproduceTry], expected: &[String], max: u32) -> Reproduce {
    if let Some(t) = tries.iter().find(|t| {
        t.discarded.is_none()
            && !expected.is_empty()
            && expected
                .iter()
                .all(|e| class_in(t, e) == Some(&RedClass::Assertion))
    }) {
        return Reproduce::Reproduced { n: t.n };
    }
    let made = u32::try_from(tries.len()).unwrap_or(u32::MAX);
    if made < max.max(1) {
        return Reproduce::Retry;
    }
    let all_passed = !tries.is_empty()
        && tries.iter().all(|t| {
            t.discarded.is_none()
                && expected
                    .iter()
                    .all(|e| class_in(t, e) == Some(&RedClass::Passed))
        });
    if all_passed {
        return Reproduce::AlreadySatisfied;
    }
    let mut reproduced = Vec::new();
    let mut satisfied = Vec::new();
    let mut not_demonstrated = Vec::new();
    for e in expected {
        let classes: Vec<&RedClass> = tries.iter().filter_map(|t| class_in(t, e)).collect();
        if classes.contains(&&RedClass::Assertion) {
            reproduced.push(e.clone());
        } else if !classes.is_empty() && classes.iter().all(|c| **c == RedClass::Passed) {
            satisfied.push(e.clone());
        } else {
            not_demonstrated.push(e.clone());
        }
    }
    Reproduce::CannotReproduce {
        reproduced,
        satisfied,
        not_demonstrated,
    }
}

/// The class a reproduction counts as from its two baseline runs: red only
/// by an assertion in both; red once, then not, is flaky; an absent symbol
/// is not a bug's red.
#[must_use]
pub fn bug_class(first: RedClass, second: Option<RedClass>) -> RedClass {
    match (first, second) {
        (RedClass::Assertion, Some(RedClass::Assertion)) => RedClass::Assertion,
        (RedClass::Assertion, _) => RedClass::Flaky,
        (c, _) => c,
    }
}

/// How the person decided the reproductions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// They saw the reproductions and confirmed them.
    Shown,
    /// They named the tests themselves (`--test`).
    NamedTest,
}

/// The person's one confirmation, bound to what they saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Confirmation {
    /// `sha256:<hex>` over the reproductions shown (their files and
    /// statement ids).
    pub digest: String,
    /// The triad's digest when confirmed: a different one lapses this.
    pub triad_digest: String,
    /// Who.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// Seen or named.
    pub basis: Basis,
    /// Unix seconds.
    pub at: u64,
}

impl Confirmation {
    /// Still valid for the triad as it reads now.
    #[must_use]
    pub fn current(&self, triad_digest: &str) -> bool {
        self.triad_digest == triad_digest
    }
}

/// The person rejected the reproductions shown; the reason goes to the
/// author's next try.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Rejection {
    /// The digest rejected.
    pub digest: String,
    /// Why.
    pub reason: String,
    /// Who.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// Unix seconds.
    pub at: u64,
}

/// Whether anything was selected for pass-to-pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PassToPassState {
    /// Members were selected.
    Selected,
    /// Nothing was: never counted as passed.
    NoneSelected,
}

/// A pass-to-pass member on the final tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemberFinal {
    /// Passed, by name.
    Passed,
    /// Failed.
    Failed,
    /// Not found in the final run: a failure.
    Missing,
    /// Not run yet.
    NotRun,
}

/// One test that must stay green.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "PassToPassMember")]
pub struct Member {
    /// The test id, as the runner reports it.
    pub test: String,
    /// Its file.
    pub file: String,
    /// How it was selected: `same_file`, `references:<module>`,
    /// `imports:<path>`, `package:<pkg>`, `unchanged:U<n>`.
    pub rule: String,
    /// On the final tree.
    #[serde(rename = "final")]
    pub final_: MemberFinal,
}

/// The tests that passed before the fix and must pass after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PassToPass {
    /// Selected, or nothing was.
    pub state: PassToPassState,
    /// The members.
    pub members: Vec<Member>,
}

impl PassToPass {
    /// Every member passed by name (vacuously true when none was selected,
    /// which is recorded as such, never as passed).
    #[must_use]
    pub fn held(&self) -> bool {
        self.members.iter().all(|m| m.final_ == MemberFinal::Passed)
    }
}

/// The revert candidate's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevertResult {
    /// The gravest outcome over the reproductions.
    pub outcome: Revert,
    /// The statements whose reproduction failed on an assertion.
    pub rejected_by: Vec<String>,
}

/// One special-case candidate: a fix that recognises a reproduction's
/// literal inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpecialCase {
    /// The statement whose reproduction it targets.
    pub statement: String,
    /// `sha256:<hex>` of its files: what a ruling is bound to.
    pub digest: String,
    /// The combined set (reproductions and held-out variants) rejected it.
    pub rejected: bool,
    /// Its statement's held-out variant was written again once, told it.
    pub regenerated: bool,
    /// A person ruled the tests right about it.
    #[serde(default)]
    pub ruled: bool,
}

/// Everything that shows the reproductions tell a fixed tree from a
/// buggy one.
#[derive(Debug, Clone, Copy)]
pub struct Discrimination<'a> {
    /// The reproductions (with `final` and `revert`).
    pub reproductions: &'a [Reproduction],
    /// The revert candidate.
    pub revert: &'a RevertResult,
    /// The special-case candidates.
    pub special_case: &'a [SpecialCase],
}

/// The verdict on a bug task: `base` (the ordinary rules, held-out
/// included) stands only if every reproduction passed on the final tree,
/// the revert was rejected by an assertion, every special case was rejected
/// (or ruled), the pass-to-pass set passed by name and the held-out
/// variants passed. Reasons name statements, never a held-out test.
#[must_use]
pub fn bug_verdict(
    base: Verdict,
    d: &Discrimination<'_>,
    p2p: &PassToPass,
    held_out: &HeldOut,
) -> Verdict {
    let mut reasons = Vec::new();
    for r in d.reproductions {
        if r.final_ != Final::Passed {
            reasons.push(format!(
                "the reproduction of {} does not pass on the fixed tree",
                r.statement
            ));
        }
    }
    if d.revert.rejected_by.is_empty() {
        reasons.push(match d.revert.outcome {
            Revert::Unviable => "the reproduction does not discriminate the fix: the revert does not build, and no reproduction rejects it by an assertion".to_owned(),
            _ => "the reproduction does not discriminate the fix: with the change undone the reproductions still pass".to_owned(),
        });
    }
    for s in d.special_case {
        if !s.rejected && !s.ruled {
            reasons.push(format!(
                "reproduction too weak: a fix that special-cases {}'s input passes the reproductions and the held-out variants",
                s.statement
            ));
        }
    }
    for m in &p2p.members {
        match m.final_ {
            MemberFinal::Passed => {}
            MemberFinal::Missing => {
                reasons.push(format!("pass-to-pass: {} is missing", m.test));
            }
            MemberFinal::Failed | MemberFinal::NotRun => {
                reasons.push(format!("pass-to-pass: {} does not pass", m.test));
            }
        }
    }
    match held_out {
        HeldOut::Passed { .. } | HeldOut::Off => {}
        HeldOut::Absent { reason } => reasons.push(format!(
            "no held-out variant was kept ({reason}): the fix cannot be checked on other inputs"
        )),
        HeldOut::Failed { scenarios } => reasons.push(format!(
            "held-out variant failed for {}",
            scenarios.join(", ")
        )),
    }
    match base {
        Verdict::Failed {
            reasons: mut before,
        } => {
            for r in reasons {
                if !before.contains(&r) {
                    before.push(r);
                }
            }
            Verdict::Failed { reasons: before }
        }
        v if reasons.is_empty() => v,
        _ => Verdict::Failed { reasons },
    }
}

/// The author's own words, kept only as a labelled claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "AuthorClaim")]
pub struct Claim {
    /// The words.
    pub text: String,
    /// Always `author`.
    pub by: String,
}

/// Statements, by what the tries showed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "StatementsShown")]
pub struct Statements {
    /// Red by assertion in some try.
    pub reproduced: Vec<String>,
    /// Neither red nor passing.
    pub not_reproduced: Vec<String>,
    /// Passed on every try.
    pub satisfied: Vec<String>,
}

/// The hand-off of a stopped bug, composed by the engine from the journal
/// (never written by a model).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Handoff {
    /// The outcome it hands off.
    pub outcome: Outcome,
    /// The tries made.
    pub tries: Vec<ReproduceTry>,
    /// Per statement.
    pub statements: Statements,
    /// Tokens the tries spent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<serde_json::Value>,
    /// How long reproduce took.
    pub elapsed_ms: u64,
    /// What the person can do.
    pub next: Vec<NextStep>,
    /// The author's last words, labelled as its claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<Claim>,
}

/// Where a foreign fix came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOrigin {
    /// A patch file.
    Patch,
    /// A GitHub pull request.
    Pr,
}

/// Why a candidate was refused before it was applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CandidateRefusal {
    /// `BUG-CANDIDATE-JUDGE` or `BUG-CANDIDATE-APPLY`.
    pub code: String,
    /// The judge paths it touches, or the hunks that did not apply.
    pub paths: Vec<String>,
}

/// Someone else's fix, judged as the attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Candidate {
    /// A patch or a pull request.
    pub origin: CandidateOrigin,
    /// The patch file, as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The pull request's number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    /// `sha256:<hex>` of the diff.
    pub digest: String,
    /// The pull request's head commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// It was applied as the attempt.
    pub applied: bool,
    /// Why it was not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<CandidateRefusal>,
}

/// The issue a bug was recorded from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueRef {
    /// Its number.
    pub number: u64,
    /// `owner/repo`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The digest of its context-manifest entry (title and body as given
    /// to the reproduction author).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_digest: Option<String>,
}

/// Where the confirmed reproductions are kept, as recorded with the fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreFrozenRef {
    /// `sha256:<hex>` over the visible set.
    pub manifest: String,
    /// The confirmation's digest.
    pub approval: String,
    /// The triad's digest.
    pub wording: String,
}

/// What the fix's visible acceptance set came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AcceptanceRef {
    /// `bug_confirmation` or `person_test`.
    pub source: crate::acceptance::AcceptanceSource,
    /// The record it reused.
    pub pre_frozen: PreFrozenRef,
}

/// A bug: where it lives, the person's triad, the reproductions and how
/// far it got. The `bug` payload, and what speccoder keeps of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BugReport {
    /// Where the format keeps it.
    pub home: BugHome,
    /// The unit, as the adapter names it (`007-bugfixes`, `bugs/<slug>`).
    pub unit: String,
    /// The requirement (`FR-003`, or `bug:<slug>`).
    pub requirement: String,
    /// The task (`T003`, or `bug:<slug>`).
    pub task: String,
    /// A short title.
    pub title: String,
    /// The person's words.
    pub triad: Triad,
    /// Where it is.
    pub stage: Stage,
    /// How it stands.
    pub outcome: Outcome,
    /// The fix's verdict, once judged.
    #[serde(default)]
    pub verdict: Option<Verdict>,
    /// The code of the last stop or refusal.
    #[serde(default)]
    pub code: Option<String>,
    /// The reproductions (and *Unchanged* tests).
    #[serde(default)]
    pub reproductions: Vec<Reproduction>,
    /// The held-out variants (never their content or path).
    #[serde(default)]
    pub held_out: Vec<HeldOutVariant>,
    /// The sealed set holding them.
    #[serde(default)]
    pub held_out_sealed: Option<crate::acceptance::SealedRef>,
    /// The digest waiting for the person's confirmation.
    #[serde(default)]
    pub confirm_digest: Option<String>,
    /// The confirmation.
    #[serde(default)]
    pub confirmation: Option<Confirmation>,
    /// Rejections of earlier reproductions.
    #[serde(default)]
    pub rejections: Vec<Rejection>,
    /// What the fix's acceptance set came from.
    #[serde(default)]
    pub acceptance: Option<AcceptanceRef>,
    /// The tests selected to stay green.
    #[serde(default)]
    pub pass_to_pass: Option<PassToPass>,
    /// The revert candidate.
    #[serde(default)]
    pub revert: Option<RevertResult>,
    /// The special-case candidates.
    #[serde(default)]
    pub special_case: Vec<SpecialCase>,
    /// The reproduce tries.
    #[serde(default)]
    pub tries: Vec<ReproduceTry>,
    /// At most this many tries.
    pub max_tries: u32,
    /// The engine's hand-off, when it stopped.
    #[serde(default)]
    pub handoff: Option<Handoff>,
    /// The issue it came from.
    #[serde(default)]
    pub issue: Option<IssueRef>,
    /// Someone else's fix, judged.
    #[serde(default)]
    pub candidate: Option<Candidate>,
    /// The planning commit that recorded it.
    #[serde(default)]
    pub recorded_commit: Option<String>,
    /// The verdict commit.
    #[serde(default)]
    pub commit: Option<String>,
    /// The last run.
    #[serde(default)]
    pub run_id: Option<String>,
}

impl BugReport {
    /// The *Expected* ids.
    #[must_use]
    pub fn expected_ids(&self) -> Vec<String> {
        self.triad.expected.iter().map(|s| s.id.clone()).collect()
    }

    /// `<unit>/<task>`: how commands name it.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}/{}", self.unit, self.task)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn hash(b: &[u8]) -> String {
        // Not SHA-256: a stand-in; the runtime passes the real one.
        format!(
            "{:016x}",
            b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| {
                (h ^ u64::from(*x)).wrapping_mul(0x0100_0000_01b3)
            })
        )
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_triad_numbers_its_statements_and_refuses_empty_words() {
        let t = Triad::new(
            "  saving an empty   title crashes ",
            &strings(&["an empty title is refused", "the form keeps its text"]),
            &strings(&["a title is saved"]),
            hash,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(t.current.text, "saving an empty title crashes");
        assert_eq!(t.cited(), ["E1", "E2", "U1"]);
        assert!(t.digest.starts_with("sha256:"));
        assert_eq!(
            t.labelled(),
            "Current: saving an empty title crashes\nExpected: an empty title is refused\nExpected: the form keeps its text\nUnchanged: a title is saved\n"
        );
        assert_eq!(t.title(), "saving an empty title crashes");
        assert_eq!(
            Triad::new(" ", &strings(&["x"]), &[], hash),
            Err(TriadError::Empty)
        );
        assert_eq!(Triad::new("x", &[], &[], hash), Err(TriadError::NoExpected));
        assert_eq!(
            Triad::new("x", &strings(&["  "]), &[], hash),
            Err(TriadError::Empty)
        );
        assert_eq!(TriadError::NoExpected.code(), "BUG-NO-EXPECTED");
    }

    fn try_of(n: u32, classes: &[(&str, RedClass)]) -> ReproduceTry {
        ReproduceTry {
            n,
            per_statement: classes
                .iter()
                .map(|(s, c)| TryStatement {
                    statement: (*s).to_owned(),
                    class: *c,
                    excerpt: None,
                })
                .collect(),
            discarded: None,
            run_id: None,
            usage: None,
            rejection: None,
        }
    }

    fn red_class() -> impl Strategy<Value = RedClass> {
        prop_oneof![
            Just(RedClass::Assertion),
            Just(RedClass::Absent),
            Just(RedClass::BuildError),
            Just(RedClass::CollectionError),
            Just(RedClass::Timeout),
            Just(RedClass::NoTests),
            Just(RedClass::Flaky),
            Just(RedClass::Unclassified),
            Just(RedClass::Passed),
        ]
    }

    proptest! {
        #[test]
        fn reproduce_is_decided_by_classes_alone(
            classes in proptest::collection::vec(proptest::collection::vec(red_class(), 2), 1..6),
            max in 1u32..6,
        ) {
            let expected = strings(&["E1", "E2"]);
            let tries: Vec<ReproduceTry> = classes
                .iter()
                .enumerate()
                .map(|(i, c)| try_of(u32::try_from(i).unwrap_or(0) + 1, &[("E1", c[0]), ("E2", c[1])]))
                .collect();
            let all_red = tries.iter().any(|t| t.per_statement.iter().all(|s| s.class == RedClass::Assertion));
            match decide_reproduce(&tries, &expected, max) {
                Reproduce::Reproduced { n } => {
                    prop_assert!(all_red);
                    let t = &tries[(n - 1) as usize];
                    // Absent or flaky is never a bug's red.
                    prop_assert!(t.per_statement.iter().all(|s| s.class == RedClass::Assertion));
                }
                Reproduce::AlreadySatisfied => {
                    prop_assert!(!all_red);
                    prop_assert!(tries.len() >= max as usize);
                    prop_assert!(tries.iter().all(|t| t.per_statement.iter().all(|s| s.class == RedClass::Passed)));
                }
                Reproduce::Retry => {
                    prop_assert!(!all_red);
                    prop_assert!(tries.len() < max as usize);
                }
                Reproduce::CannotReproduce { reproduced, satisfied, not_demonstrated } => {
                    prop_assert!(!all_red);
                    prop_assert!(tries.len() >= max as usize);
                    let mut named: Vec<String> = reproduced.iter().chain(&satisfied).chain(&not_demonstrated).cloned().collect();
                    named.sort();
                    prop_assert_eq!(named, expected.clone());
                }
            }
        }

        #[test]
        fn a_bug_is_red_only_by_two_assertions(a in red_class(), b in red_class()) {
            let c = bug_class(a, Some(b));
            prop_assert_eq!(c == RedClass::Assertion, a == RedClass::Assertion && b == RedClass::Assertion);
        }

        #[test]
        fn verified_needs_every_bug_rule(
            finals in proptest::collection::vec(prop_oneof![Just(Final::Passed), Just(Final::Failed), Just(Final::NotRun)], 1..4),
            rejected_by in proptest::collection::vec("E[1-3]", 0..3),
            special in proptest::collection::vec((any::<bool>(), any::<bool>()), 0..3),
            members in proptest::collection::vec(prop_oneof![Just(MemberFinal::Passed), Just(MemberFinal::Failed), Just(MemberFinal::Missing)], 0..4),
            held in prop_oneof![
                Just(HeldOut::Passed { files: 1 }),
                Just(HeldOut::Failed { scenarios: vec!["E1".to_owned()] }),
                Just(HeldOut::Absent { reason: "all_discarded".to_owned() }),
                Just(HeldOut::Off),
            ],
        ) {
            let reproductions: Vec<Reproduction> = finals.iter().enumerate().map(|(i, f)| Reproduction {
                statement: format!("E{}", i + 1),
                file: format!("tests/repro_{i}.rs"),
                test: "t".to_owned(),
                source: Source::Author,
                baseline: vec![RedClass::Assertion, RedClass::Assertion],
                digest: "sha256:x".to_owned(),
                argv: Vec::new(),
                final_: *f,
                revert: Revert::NotRun,
            }).collect();
            let revert = RevertResult { outcome: if rejected_by.is_empty() { Revert::Survived } else { Revert::Rejected }, rejected_by: rejected_by.clone() };
            let special_case: Vec<SpecialCase> = special.iter().map(|(rejected, ruled)| SpecialCase {
                statement: "E1".to_owned(), digest: "sha256:s".to_owned(), rejected: *rejected, regenerated: false, ruled: *ruled,
            }).collect();
            let p2p = PassToPass {
                state: if members.is_empty() { PassToPassState::NoneSelected } else { PassToPassState::Selected },
                members: members.iter().map(|m| Member { test: "held_out_secret_name".to_owned(), file: "tests/a.rs".to_owned(), rule: "same_file".to_owned(), final_: *m }).collect(),
            };
            let d = Discrimination { reproductions: &reproductions, revert: &revert, special_case: &special_case };
            let v = bug_verdict(Verdict::Verified, &d, &p2p, &held);
            let ok = finals.iter().all(|f| *f == Final::Passed)
                && !rejected_by.is_empty()
                && special.iter().all(|(r, ruled)| *r || *ruled)
                && members.iter().all(|m| *m == MemberFinal::Passed)
                && matches!(held, HeldOut::Passed { .. } | HeldOut::Off);
            prop_assert_eq!(v == Verdict::Verified, ok);
            if let Verdict::Failed { reasons } = &v {
                // A held-out failure names statements only.
                prop_assert!(reasons.iter().all(|r| !r.contains("held_out_") || r.starts_with("pass-to-pass")));
            }
            // A failed base stays failed.
            let failed = bug_verdict(Verdict::Failed { reasons: vec!["check `x` failed".to_owned()] }, &d, &p2p, &held);
            prop_assert!(matches!(failed, Verdict::Failed { .. }), "a failed verdict stays failed");
        }
    }

    #[test]
    fn absent_and_flaky_are_not_a_bugs_red() {
        assert_eq!(
            bug_class(RedClass::Absent, Some(RedClass::Absent)),
            RedClass::Absent
        );
        assert_eq!(
            bug_class(RedClass::Assertion, Some(RedClass::Passed)),
            RedClass::Flaky
        );
        assert_eq!(bug_class(RedClass::Assertion, None), RedClass::Flaky);
        let tries = [try_of(1, &[("E1", RedClass::Absent)])];
        assert!(matches!(
            decide_reproduce(&tries, &strings(&["E1"]), 1),
            Reproduce::CannotReproduce { .. }
        ));
    }

    #[test]
    fn mixed_tries_name_what_was_satisfied() {
        let tries = [
            try_of(1, &[("E1", RedClass::Assertion), ("E2", RedClass::Passed)]),
            try_of(2, &[("E1", RedClass::BuildError), ("E2", RedClass::Passed)]),
        ];
        assert_eq!(
            decide_reproduce(&tries, &strings(&["E1", "E2"]), 2),
            Reproduce::CannotReproduce {
                reproduced: strings(&["E1"]),
                satisfied: strings(&["E2"]),
                not_demonstrated: Vec::new(),
            }
        );
        let passed = [
            try_of(1, &[("E1", RedClass::Passed)]),
            try_of(2, &[("E1", RedClass::Passed)]),
        ];
        assert_eq!(
            decide_reproduce(&passed, &strings(&["E1"]), 2),
            Reproduce::AlreadySatisfied
        );
        assert_eq!(
            decide_reproduce(&passed[..1], &strings(&["E1"]), 2),
            Reproduce::Retry
        );
    }
}
