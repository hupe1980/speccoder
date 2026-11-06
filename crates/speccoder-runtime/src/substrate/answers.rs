//! What the model answers, one type per answer. Each schema it is held to
//! is derived from its type (`agentplane::tools::strict_schema`), so a field
//! and its description live in one place; the provider enforces the schema
//! during generation and agentplane validates the answer against it.
//!
//! An enum's variants are explained in the enum's doc, not their own:
//! documented variants become a `oneOf` that strict decoding refuses.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

/// The strict schema of the answer `T`.
#[must_use]
pub fn schema<T: JsonSchema>() -> Value {
    agentplane::tools::strict_schema::<T>()
}

/// Whether an attempt or a hosted skill finished its work.
///
/// - `done`: The work is finished.
/// - `incomplete`: Part of it is left undone.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Done,
    Incomplete,
}

/// An attempt's closing report: the model's claim, never its verdict.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttemptReport {
    /// What was done, in a sentence or two.
    pub summary: String,
    /// done: this task's work is finished — even while tests that later
    /// tasks make pass still fail. incomplete: part of this task is left
    /// undone.
    pub status: Status,
    /// The files changed.
    pub files_changed: Vec<String>,
    /// The checks run, as commands.
    pub checks_run: Vec<String>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
}

/// How a task would be done; recorded, never executed.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Proposal {
    /// The approach in a sentence or two.
    pub summary: String,
    /// The steps, in order.
    pub steps: Vec<String>,
    /// The files expected to change.
    pub files: Vec<String>,
    /// The checks that would show it works.
    pub checks: Vec<String>,
    /// What is unclear or risky.
    pub risks: Vec<String>,
}

/// What kind of gap between the spec and the code.
///
/// - `missing`: Not built.
/// - `partial`: Built in part.
/// - `contradicts`: Built otherwise than the spec says.
/// - `unrequested`: Built, not asked for.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GapType {
    Missing,
    Partial,
    Contradicts,
    Unrequested,
}

/// How much a gap matters.
///
/// - `CRITICAL`: Breaks what the spec promises.
/// - `HIGH`: A requirement unmet.
/// - `MEDIUM`: A part of one unmet.
/// - `LOW`: Cosmetic.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

/// One gap convergence found.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Finding {
    /// The requirement or scenario it is about (`FR-003`, `US1`).
    pub source_ref: String,
    /// What kind of gap.
    pub gap_type: GapType,
    /// How much it matters.
    pub severity: Severity,
    /// The gap, in a sentence.
    pub description: String,
    /// Where in the code it shows.
    pub evidence: String,
    /// The task that would close it, as a task line.
    pub task: String,
}

/// Convergence's assessment of the code against the spec.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Assessment {
    /// The gaps found.
    pub findings: Vec<Finding>,
    /// What was checked.
    pub checked: Vec<String>,
}

/// A kind of shortcut the monitor looks for.
///
/// - `hardcoded_output`: Returns the expected output instead of computing it.
/// - `special_cased_input`: Special-cases the tests' inputs.
/// - `weakened_test`: Weakens or skips a test.
/// - `test_environment_detection`: Behaves differently under test.
/// - `swallowed_error`: Swallows an error the spec wants seen.
/// - `contradicts_spec`: Does what the spec rules out.
/// - `other`: Anything else.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    HardcodedOutput,
    SpecialCasedInput,
    WeakenedTest,
    TestEnvironmentDetection,
    SwallowedError,
    ContradictsSpec,
    Other,
}

/// One suspicion of the monitor.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Suspicion {
    /// What kind of shortcut.
    pub kind: Shortcut,
    /// The lines that show it.
    pub evidence: String,
    /// Why it is one.
    pub explanation: String,
}

/// The monitor's review of an attempt's diff.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Monitoring {
    /// Whether the attempt took a shortcut.
    pub suspected: bool,
    /// What shows it.
    pub findings: Vec<Suspicion>,
}

/// Where the person's words came from, and the question that settles it.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Assumption {
    /// The words assumed.
    pub words: String,
    /// What they were assumed from.
    pub from: String,
    /// The question that would settle them.
    pub question: String,
}

/// The person's words shaped into the spec's language.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Shaped {
    /// The text proposed for the field.
    pub proposal: String,
    /// What was assumed.
    pub assumptions: Vec<Assumption>,
    /// What to ask the person.
    pub questions: Vec<String>,
    /// The language written in.
    pub language: String,
    /// The proposal back in the person's own words.
    pub back_translation: String,
}

/// A near-miss variant is a wrong implementation the tests must fail, or a
/// permitted one they must pass.
///
/// - `wrong`: Wrong: the tests must fail on it.
/// - `permitted`: Permitted by the spec: the tests must pass on it.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NearMissKind {
    Wrong,
    Permitted,
}

/// A variant that special-cases the tests' inputs.
///
/// - `special_case`: Passes the tests' inputs only.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpecialCaseKind {
    SpecialCase,
}

/// A file as written.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct File {
    /// Project-relative path.
    pub path: String,
    /// The whole content.
    pub content: String,
}

/// One variant of the code under test.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Variant<K> {
    /// What kind of variant.
    pub kind: K,
    /// The scenario it is about.
    pub scenario: String,
    /// Why it is wrong, or permitted.
    pub rationale: String,
    /// The files it replaces.
    pub files: Vec<File>,
}

/// Variants of the code that measure what the tests tell apart.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Variants<K> {
    /// The variants.
    pub variants: Vec<Variant<K>>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
}

/// A hosted planning skill's closing report.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Hosted {
    /// What was written, in a sentence or two.
    pub summary: String,
    /// Whether the skill finished.
    pub status: Status,
    /// The files written.
    pub files_written: Vec<String>,
    /// What is left to ask the person.
    pub questions: Vec<String>,
}

/// One way to build an idea.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Approach {
    /// Its name.
    pub title: String,
    /// What it is.
    pub summary: String,
    /// What it gives and costs.
    pub tradeoffs: Vec<String>,
    /// What could go wrong.
    pub risks: Vec<String>,
}

/// An exploration's closing report: approaches for the person to choose.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Explored {
    /// The idea, as understood.
    pub summary: String,
    /// Whether exploring finished.
    pub status: Status,
    /// Approaches that differ in kind, not degree.
    pub approaches: Vec<Approach>,
    /// What any approach must respect.
    pub constraints: Vec<String>,
    /// What is still open.
    pub open_questions: Vec<String>,
}

/// An acceptance test file and the scenarios it checks.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestFile {
    /// Project-relative path.
    pub path: String,
    /// The whole content.
    pub content: String,
    /// The scenarios it checks.
    pub scenarios: Vec<String>,
}

/// The acceptance tests written before the code.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Acceptance {
    /// The test files.
    pub files: Vec<TestFile>,
    /// The command that runs them.
    pub argv: Vec<String>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
}

/// A reproduction of one statement of a bug.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Reproduction {
    /// Project-relative path.
    pub path: String,
    /// The whole content.
    pub content: String,
    /// The statement it reproduces (`E1`) or guards (`U1`).
    pub statement: String,
    /// The test's name.
    pub test: String,
    /// The command that runs it.
    pub argv: Vec<String>,
}

/// A bug's reproductions.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Reproductions {
    /// One file per statement.
    pub files: Vec<Reproduction>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
}

/// A held-out test file, never shown to the implementer.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HeldOutFile {
    /// Project-relative path.
    pub path: String,
    /// The whole content.
    pub content: String,
    /// The scenarios it checks.
    pub scenarios: Vec<String>,
    /// The command that runs it.
    pub argv: Vec<String>,
    /// Whether it checks the story's scenarios together.
    pub composition: bool,
}

/// The held-out tests.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HeldOut {
    /// The test files.
    pub files: Vec<HeldOutFile>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
}

/// A test file the person reviews before any code.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReviewFile {
    /// Project-relative path.
    pub path: String,
    /// The whole content.
    pub content: String,
    /// The scenarios it checks.
    pub scenarios: Vec<String>,
    /// The command that runs it.
    pub argv: Vec<String>,
}

/// What a scenario's test expects, in the person's words.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Expectation {
    /// The scenario.
    pub scenario: String,
    /// The situation.
    pub given: String,
    /// What must hold.
    pub expect: String,
}

/// A scenario no test can check, and why.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Untestable {
    /// The scenario.
    pub scenario: String,
    /// Why no test can check it.
    pub why: String,
}

/// The tests for the person's review, with what each expects.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Review {
    /// The test files.
    pub files: Vec<ReviewFile>,
    /// The command that runs them.
    pub argv: Vec<String>,
    /// Anything the reviewer should know.
    pub notes: Vec<String>,
    /// What each scenario's test expects.
    pub expectations: Vec<Expectation>,
    /// Scenarios no test can check.
    pub untestable: Vec<Untestable>,
}

/// Why an attempt cannot go on, as its evidence shows.
///
/// - `refused`: a call it needs was refused.
/// - `spec_conflict`: the spec contradicts itself or the code.
/// - `wrong_test`: a frozen test contradicts the spec.
/// - `needs_information`: only the person can say.
#[allow(missing_docs)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlockedKind {
    Refused,
    SpecConflict,
    WrongTest,
    NeedsInformation,
}

/// A `report_blocked` call: the honest exit, checked against its evidence.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Blocked {
    /// Why it cannot go on.
    pub kind: BlockedKind,
    /// What shows it: the spec text, the test, the refused call.
    pub evidence: String,
    /// The reason, for the person.
    pub reason: String,
}

/// An `ask_person` call.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Question {
    /// One question.
    pub question: String,
}
