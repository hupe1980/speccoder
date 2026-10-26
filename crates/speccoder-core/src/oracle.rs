//! The oracle at review (D-117, spec 093): per requirement, the visible
//! acceptance tests the run would use — written on the person's request,
//! red-checked on the current tree, shaped by the person and frozen with
//! the approval — and the review comments anchored to what the person
//! reads. These are the shapes every interface shows; none of them has a
//! field that could hold a held-out test's path or content.

use serde::{Deserialize, Serialize};

use crate::actions::Via;

/// How a run started from the review asks for a person.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// *Ask on Risky Calls*: a call the policy asks about is asked inline,
    /// as a run from the interface does.
    #[default]
    Ask,
    /// *Autopilot*: nothing asks; a call the policy would ask about and no
    /// ticked grant covers is refused for the run and listed afterwards.
    Autopilot,
}

impl RunMode {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Autopilot => "autopilot",
        }
    }
}

/// What a change to a requirement's tests does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OracleOp {
    /// Write the tests (the author calls, the comparison, the red check,
    /// the held-out set).
    Write,
    /// Drop one test (the input is the reason, required).
    Reject,
    /// Write one test again (the input is the person's comment).
    Again,
    /// Replace one test's content with the person's (the input).
    Edit,
    /// Say which reading of an ambiguous scenario is meant.
    Reading,
}

/// One of the two readings of an ambiguous scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Reading {
    /// The first author's.
    A,
    /// The second author's.
    B,
}

impl Reading {
    /// `A` or `B`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

/// What an author states it asserts for one scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Expectation {
    /// The scenario's id (`US1-2`).
    pub scenario: String,
    /// The situation, in the spec's terms.
    pub given: String,
    /// The expected outcome it asserts.
    pub expect: String,
}

/// A scenario an author found untestable, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Untestable {
    /// The scenario's id.
    pub scenario: String,
    /// Why.
    pub why: String,
}

/// One reading of a scenario: what one author expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReadingText {
    /// The situation.
    pub given: String,
    /// The outcome (`untestable: …` when the author found none).
    pub expect: String,
}

/// The person's choice on an ambiguous scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Resolution {
    /// The reading meant.
    pub reading: Reading,
    /// Who chose.
    pub by: String,
    /// Unix seconds.
    pub at: u64,
}

/// Two authors read a scenario differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Ambiguity {
    /// The scenario.
    pub scenario: String,
    /// The first author's reading (its tests are the ones kept).
    pub a: ReadingText,
    /// The second author's.
    pub b: ReadingText,
    /// The person's choice; `None` while it is open.
    pub resolution: Option<Resolution>,
}

impl Ambiguity {
    /// Still open: nothing approves or freezes past it.
    #[must_use]
    pub const fn open(&self) -> bool {
        self.resolution.is_none()
    }
}

/// Who wrote a shown test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthoredBy {
    /// The acceptance author.
    Model,
    /// The person (an edit).
    Person,
}

/// One visible test as the review shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleTest {
    /// Project-relative path.
    pub path: String,
    /// Its content (a visible test: the implementer reads it too).
    pub content: String,
    /// The scenarios it cites (never empty).
    pub scenarios: Vec<String>,
    /// Its red class on the current tree (`assertion`, `absent_symbol`,
    /// `passed`, `build_error`, `flaky`, …).
    pub red_class: String,
    /// Red: it can be frozen.
    pub red: bool,
    /// Who wrote it.
    pub authored_by: AuthoredBy,
    /// The person who edited it.
    pub edited_by: Option<String>,
}

/// A test the author wrote that is not part of the set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleDiscarded {
    /// Its path as given.
    pub path: String,
    /// `cites_no_scenario`, `not_a_new_test_file`, `exists`.
    pub why: String,
}

/// A test the person rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleRejected {
    /// Its path.
    pub path: String,
    /// The person's reason (told to every later author call).
    pub reason: String,
    /// Who.
    pub by: String,
    /// Unix seconds.
    pub at: u64,
}

/// The held-out set of a requirement as anyone may see it: a count, a
/// state and a digest — never a path or a line of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HeldOutSummary {
    /// `kept`, `absent` or `off` (`[acceptance] held_out = false`).
    pub state: String,
    /// Tests kept.
    pub count: usize,
    /// The sealed manifest's digest, when kept.
    pub manifest_sha256: Option<String>,
    /// Why none was kept (`not_red`, `none_generated`, …).
    pub reason: Option<String>,
}

/// Where a requirement's oracle is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OracleState {
    /// No tests written yet.
    None,
    /// Written; some are red.
    Written,
    /// Written; none is red, so none can be frozen.
    NotRed,
    /// The author call failed.
    Failed,
    /// Two readings disagree: the person chooses or rewords.
    Ambiguous,
    /// Frozen with the approval, on the current wording.
    Frozen,
    /// Frozen on an earlier wording: the run writes tests anew.
    Lapsed,
    /// `review.json` names a frozen set this machine does not have.
    FrozenElsewhere,
}

impl OracleState {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Written => "written",
            Self::NotRed => "not_red",
            Self::Failed => "failed",
            Self::Ambiguous => "ambiguous",
            Self::Frozen => "frozen",
            Self::Lapsed => "lapsed",
            Self::FrozenElsewhere => "frozen_elsewhere",
        }
    }
}

/// One requirement's oracle in the review: everything the Tests card shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OracleView {
    /// The requirement (`US1`).
    pub req: String,
    /// Its title.
    pub title: String,
    /// Its scenarios: id and words.
    pub scenarios: Vec<crate::spec::ScenarioRef>,
    /// Where it is.
    pub state: OracleState,
    /// For `lapsed`: the wording it was frozen on.
    pub frozen_on: Option<String>,
    /// Who froze it, when frozen.
    pub frozen_by: Option<String>,
    /// When (Unix seconds).
    pub frozen_at: Option<u64>,
    /// The kept tests.
    pub tests: Vec<OracleTest>,
    /// The tests not kept, and why.
    pub discarded: Vec<OracleDiscarded>,
    /// The tests the person rejected.
    pub rejected: Vec<OracleRejected>,
    /// Scenarios no red kept test covers.
    pub uncovered: Vec<String>,
    /// Scenarios an author found untestable, with why.
    pub untestable: Vec<Untestable>,
    /// Readings that disagree.
    pub ambiguities: Vec<Ambiguity>,
    /// The held-out set: count and digest only.
    pub held_out: HeldOutSummary,
    /// `[acceptance] min_strength` (0–100), `None` when none is set.
    pub strength_target: Option<u8>,
    /// The author calls *Write Tests* makes (`[review] oracle_samples`).
    pub calls: u8,
    /// With one call there is nothing to compare.
    pub ambiguity_check: bool,
    /// The visible set's manifest digest, when tests are written.
    pub manifest_sha256: Option<String>,
    /// Why writing failed, or why it cannot be written here.
    pub error: Option<String>,
}

impl OracleView {
    /// The open ambiguities.
    pub fn open_ambiguities(&self) -> impl Iterator<Item = &Ambiguity> {
        self.ambiguities.iter().filter(|a| a.open())
    }

    /// What approval freezes for it, as the sheet says it.
    #[must_use]
    pub fn freezes(&self) -> String {
        let red = self.tests.iter().filter(|t| t.red).count();
        if let Some(a) = self.open_ambiguities().next() {
            return format!("blocked: two readings of {}", a.scenario);
        }
        match self.state {
            OracleState::Frozen => format!("{red} tests frozen"),
            _ if red == 0 => "tests written at run time".to_owned(),
            _ => {
                let mut parts = vec![format!("{red} {}", if red == 1 { "test" } else { "tests" })];
                if self.held_out.state == "kept" {
                    parts.push(format!("{} held out", self.held_out.count));
                }
                if let Some(t) = self.strength_target {
                    parts.push(format!("target {t}%"));
                }
                parts.join(" · ")
            }
        }
    }
}

/// The record of an approval that froze the oracle (in `review.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReviewApproval {
    /// SHA-256 over the approved decision lines and the frozen oracle
    /// entries: what a tick that reused a frozen set names.
    pub sha256: String,
    /// The run's mode.
    pub mode: RunMode,
    /// Who approved.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// Unix seconds.
    pub at: u64,
}

/// A grant an autopilot run may hold, offered on the approve sheet: one
/// object-bound grant (F-125) a person decided earlier for this feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OfferedGrant {
    /// Its grant id: what `--grant` and the sheet's tick name.
    pub id: String,
    /// `write`, `command`, `network` or `mcp`.
    pub class: String,
    /// The object in full (`run: cargo run --example demo · argv #3f…`).
    pub object: String,
}

/// What the approve sheet shows besides the requirements: who can approve
/// and run now, and what autopilot would hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApproveSheet {
    /// Decision lines the approval records.
    pub approves: Vec<String>,
    /// Why *Approve and Run* is disabled now (each a line naming its
    /// cause); empty: it can run.
    pub blocked: Vec<String>,
    /// Why autopilot is disabled (no enforced sandbox); `None`: it can run.
    pub autopilot_blocked: Option<String>,
    /// The sandbox line (`Seatbelt, network off`).
    pub sandbox: Option<String>,
    /// The grants autopilot may hold, none ticked by default.
    pub grants: Vec<OfferedGrant>,
}

/// One call an autopilot run refused instead of asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AutopilotDenial {
    /// The task.
    pub task: String,
    /// The model turn.
    pub turn: u32,
    /// `write`, `command`, `network`, `mcp` (empty: none resolved).
    pub class: String,
    /// What it reached (`src/a.rs`, `cargo add x`).
    pub object: String,
    /// `autopilot: not asked — listed for you`.
    pub reason: String,
}

/// What an anchored comment points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    /// A passage of a feature document.
    Passage {
        /// Project-relative path.
        path: String,
        /// First line (1-based).
        start_line: usize,
        /// Last line.
        end_line: usize,
        /// The words selected.
        quote: String,
        /// `sha256:<hex>` of the quote (filled when the draft is stored).
        #[serde(default)]
        quote_sha256: String,
        /// Its section's heading path (`Requirements › Functional`).
        #[serde(default)]
        section: String,
    },
    /// A line of the spec's changes since the last commit.
    DiffLine {
        /// Project-relative path.
        path: String,
        /// `old` or `new`.
        side: String,
        /// The line (1-based) on that side.
        line: usize,
        /// The line's text.
        quote: String,
    },
    /// A line of a shown visible test.
    TestLine {
        /// The requirement.
        req: String,
        /// The test's path.
        path: String,
        /// The line (1-based).
        line: usize,
        /// The line's text.
        quote: String,
    },
    /// A finding the review shows.
    Finding {
        /// `analyze` or `review`.
        source: String,
        /// Its id.
        id: String,
        /// Its text.
        text: String,
    },
}

impl Anchor {
    /// The quote the comment is about.
    #[must_use]
    pub fn quote(&self) -> &str {
        match self {
            Self::Passage { quote, .. }
            | Self::DiffLine { quote, .. }
            | Self::TestLine { quote, .. } => quote,
            Self::Finding { text, .. } => text,
        }
    }

    /// Where it is, in a few words (`spec.md:40-42`, `US1 tests/a.rs:3`).
    #[must_use]
    pub fn place(&self) -> String {
        match self {
            Self::Passage {
                path,
                start_line,
                end_line,
                ..
            } if start_line == end_line => format!("{path}:{start_line}"),
            Self::Passage {
                path,
                start_line,
                end_line,
                ..
            } => format!("{path}:{start_line}-{end_line}"),
            Self::DiffLine {
                path, side, line, ..
            } => format!("{path}:{line} ({side})"),
            Self::TestLine {
                req, path, line, ..
            } => format!("{req} {path}:{line}"),
            Self::Finding { source, id, .. } => format!("{source} {id}"),
        }
    }
}

/// Where a draft comment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    /// In the tray.
    Draft,
    /// Its quote is no longer where it was anchored: sent only if kept.
    Outdated,
    /// Sent in a revision turn.
    Sent,
}

/// One comment in the tray.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommentDraft {
    /// `c1`, `c2`, …
    pub id: String,
    /// What it is about.
    pub anchor: Anchor,
    /// The person's words.
    pub text: String,
    /// Who.
    pub by: String,
    /// Unix seconds.
    pub at: u64,
    /// Where it is.
    pub state: DraftState,
    /// The person kept it although its quote moved.
    #[serde(default)]
    pub kept: bool,
}

/// What a revision turn did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevisionView {
    /// `t1`, `t2`, …
    pub id: String,
    /// The comments it was sent with.
    pub comments: Vec<String>,
    /// The sections they touch.
    pub sections: Vec<String>,
    /// The format's revising skill.
    pub skill: String,
    /// Sections it changed.
    pub changed: Vec<String>,
    /// Of those, the ones no comment touched.
    pub outside: Vec<String>,
    /// `pending` (the person keeps or undoes it), `kept`, `undone`.
    pub outcome: String,
    /// Files it changed (project-relative).
    pub files: Vec<String>,
    /// Requirements whose tests were asked again from test comments.
    #[serde(default)]
    pub asked_again: Vec<String>,
}

/// Result of `speccoder review comments`: the tray and the last turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommentsView {
    /// The feature.
    pub feature: String,
    /// Every draft not yet sent, in order.
    pub drafts: Vec<CommentDraft>,
    /// The last revision turn.
    pub turn: Option<RevisionView>,
    /// The skill a send runs (`speckit-clarify`), when the format has one.
    pub revise_with: Option<String>,
}

/// One line comment of the Code view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeComment {
    /// Project-relative path.
    pub path: String,
    /// The line (1-based).
    pub line: usize,
    /// The code on that line.
    pub quote: String,
    /// The person's words.
    pub text: String,
}

impl CodeComment {
    /// The nudge line: `src/a.rs:12 "let x = …" — text`.
    #[must_use]
    pub fn nudge_line(&self) -> String {
        let quote: String = self.quote.trim().chars().take(80).collect();
        format!(
            "{}:{} \"{}\" — {}",
            self.path,
            self.line,
            quote,
            self.text.trim()
        )
    }
}

/// The deterministic ambiguity rule: words compared after trimming,
/// collapsing whitespace and lowercasing.
#[must_use]
pub fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Where two authors' stated expectations disagree: for a scenario, the
/// same given (normalised) with a different expected outcome; or one
/// covers it and the other declares it untestable. Different givens are
/// different examples, not a disagreement. In scenario order of `a`.
#[must_use]
pub fn compare(
    a: &[Expectation],
    a_untestable: &[Untestable],
    b: &[Expectation],
    b_untestable: &[Untestable],
) -> Vec<Ambiguity> {
    let mut out: Vec<Ambiguity> = Vec::new();
    let mut push = |scenario: &str, ra: ReadingText, rb: ReadingText| {
        if !out.iter().any(|x| x.scenario == scenario) {
            out.push(Ambiguity {
                scenario: scenario.to_owned(),
                a: ra,
                b: rb,
                resolution: None,
            });
        }
    };
    let reading = |e: &Expectation| ReadingText {
        given: e.given.clone(),
        expect: e.expect.clone(),
    };
    let untestable = |u: &Untestable| ReadingText {
        given: String::new(),
        expect: format!("untestable: {}", u.why),
    };
    for ea in a {
        for eb in b.iter().filter(|eb| eb.scenario == ea.scenario) {
            if normalise(&ea.given) == normalise(&eb.given)
                && normalise(&ea.expect) != normalise(&eb.expect)
            {
                push(&ea.scenario, reading(ea), reading(eb));
            }
        }
        if let Some(u) = b_untestable.iter().find(|u| u.scenario == ea.scenario) {
            push(&ea.scenario, reading(ea), untestable(u));
        }
    }
    for eb in b {
        if let Some(u) = a_untestable.iter().find(|u| u.scenario == eb.scenario) {
            push(&eb.scenario, untestable(u), reading(eb));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn e(s: &str, given: &str, expect: &str) -> Expectation {
        Expectation {
            scenario: s.to_owned(),
            given: given.to_owned(),
            expect: expect.to_owned(),
        }
    }

    #[test]
    fn the_same_given_with_another_outcome_is_ambiguous() {
        let a = [e("US1-1", "name Ada", "Hello, Ada!"), e("US1-2", "\"\"", "Hello!")];
        let b = [
            e("US1-1", " Name   ada ", "hello, ada!"),
            e("US1-2", "\"\"", "an error"),
        ];
        let found = compare(&a, &[], &b, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].scenario, "US1-2");
        assert_eq!(found[0].a.expect, "Hello!");
        assert_eq!(found[0].b.expect, "an error");
        assert!(found[0].open());
    }

    #[test]
    fn different_givens_are_different_examples() {
        let a = [e("US1-1", "name Ada", "Hello, Ada!")];
        let b = [e("US1-1", "name Bob", "Hello, Bob!")];
        assert!(compare(&a, &[], &b, &[]).is_empty());
    }

    #[test]
    fn covered_by_one_and_untestable_for_the_other_is_ambiguous() {
        let a = [e("US1-3", "offline", "an error")];
        let u = [Untestable {
            scenario: "US1-3".to_owned(),
            why: "needs a network".to_owned(),
        }];
        let found = compare(&a, &[], &[], &u);
        assert_eq!(found.len(), 1);
        assert!(found[0].b.expect.starts_with("untestable"));
        let back = compare(&[], &u, &a, &[]);
        assert_eq!(back.len(), 1);
        assert!(back[0].a.expect.starts_with("untestable"));
    }

    proptest! {
        #[test]
        fn agreeing_authors_raise_nothing(given in "[a-zA-Z ]{1,20}", expect in "[a-zA-Z ]{1,20}", pad in " {0,3}") {
            let a = [e("S", &given, &expect)];
            let b = [e("S", &format!("{pad}{}{pad}", given.to_uppercase()), &expect.to_lowercase())];
            prop_assert!(compare(&a, &[], &b, &[]).is_empty());
        }

        #[test]
        fn the_rule_is_symmetric_in_whether_it_fires(g in "[a-c]{1,3}", x in "[a-c]{1,3}", y in "[a-c]{1,3}") {
            let a = [e("S", &g, &x)];
            let b = [e("S", &g, &y)];
            prop_assert_eq!(compare(&a, &[], &b, &[]).len(), compare(&b, &[], &a, &[]).len());
            prop_assert_eq!(compare(&a, &[], &b, &[]).is_empty(), normalise(&x) == normalise(&y));
        }
    }

    #[test]
    fn the_sheet_says_what_approval_freezes() {
        let mut v = OracleView {
            req: "US1".to_owned(),
            title: "Greet".to_owned(),
            scenarios: Vec::new(),
            state: OracleState::Written,
            frozen_on: None,
            frozen_by: None,
            frozen_at: None,
            tests: vec![OracleTest {
                path: "tests/a.rs".to_owned(),
                content: String::new(),
                scenarios: vec!["US1-1".to_owned()],
                red_class: "assertion".to_owned(),
                red: true,
                authored_by: AuthoredBy::Model,
                edited_by: None,
            }],
            discarded: Vec::new(),
            rejected: Vec::new(),
            uncovered: Vec::new(),
            untestable: Vec::new(),
            ambiguities: Vec::new(),
            held_out: HeldOutSummary {
                state: "kept".to_owned(),
                count: 2,
                manifest_sha256: None,
                reason: None,
            },
            strength_target: Some(80),
            calls: 2,
            ambiguity_check: true,
            manifest_sha256: None,
            error: None,
        };
        assert_eq!(v.freezes(), "1 test · 2 held out · target 80%");
        v.tests[0].red = false;
        assert_eq!(v.freezes(), "tests written at run time");
        let line = CodeComment {
            path: "src/a.rs".to_owned(),
            line: 12,
            quote: "  let x = 1;".to_owned(),
            text: " use a constant ".to_owned(),
        };
        assert_eq!(line.nudge_line(), "src/a.rs:12 \"let x = 1;\" — use a constant");
    }
}
