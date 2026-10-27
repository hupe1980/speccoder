//! Why a tick is regression-only: one reason from a closed set, decided by
//! code from the run's facts, worded once, and carried in the commit's
//! `Regression-Only:` trailer and its signed statement.
//!
//! The reason sits beside [`crate::verify::Verdict`], not inside it: signed
//! statements and schemas of the verdict stay as they are, and a tick made
//! before reasons existed reads [`RegressionReason::Unrecorded`].

use serde::{Deserialize, Serialize};

use crate::verify::RedClass;

/// Which tick made already-passing acceptance tests pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ImplementedBy {
    /// A tick on the feature branch.
    Task {
        /// Its task.
        task: String,
        /// Its commit (short).
        commit: String,
    },
    /// They passed before the feature's first tick.
    BeforeFeature {
        /// Always `true`.
        before_feature: bool,
    },
}

/// Why a tick is regression-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum RegressionReason {
    /// The task belongs to no user story.
    NoStory,
    /// Its story states no acceptance scenario.
    NoScenarios {
        /// The story.
        story: String,
    },
    /// `[acceptance] generate = false`.
    AcceptanceOff,
    /// The test author returned no file.
    AuthorWroteNone,
    /// The author's answer could not be used.
    AuthorFailed {
        /// What speccoder refused (its own words).
        detail: String,
    },
    /// The acceptance tests passed before the attempt.
    AlreadyPassing {
        /// The tick that made them pass; `null` when it could not be determined.
        implemented_by: Option<ImplementedBy>,
    },
    /// They did not fail on an assertion or an absent symbol.
    NotRed {
        /// How the red run ended.
        red_class: RedClass,
    },
    /// The acceptance command collected no test.
    TestsNotCollected,
    /// A tick made before reasons were recorded (read side only).
    Unrecorded,
}

/// A regression-only tick's reason with its one-line wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "RegressionOnly")]
pub struct Regression {
    /// The reason with its typed details.
    pub reason: RegressionReason,
    /// speccoder's sentence for it, the same in every interface.
    pub text: String,
}

impl Regression {
    /// The reason, worded.
    #[must_use]
    pub fn of(reason: RegressionReason) -> Self {
        let text = wording(&reason);
        Self { reason, text }
    }
}

impl RegressionReason {
    /// The reason's recorded name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NoStory => "no_story",
            Self::NoScenarios { .. } => "no_scenarios",
            Self::AcceptanceOff => "acceptance_off",
            Self::AuthorWroteNone => "author_wrote_none",
            Self::AuthorFailed { .. } => "author_failed",
            Self::AlreadyPassing { .. } => "already_passing",
            Self::NotRed { .. } => "not_red",
            Self::TestsNotCollected => "tests_not_collected",
            Self::Unrecorded => "unrecorded",
        }
    }
}

/// Why the acceptance author's answer was not used: typed, so the reason
/// is decided by code, never by reading a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "cause", rename_all = "snake_case")]
pub enum SkipCause {
    /// The author returned no file.
    NoFiles,
    /// A file was refused.
    Refused {
        /// The path, as the author gave it.
        path: String,
        /// speccoder's refusal (`is not a new test file`).
        why: String,
    },
    /// The command is not one the policy allows.
    CommandNotAllowed {
        /// The command.
        argv: String,
    },
    /// Writing a file failed.
    WriteFailed {
        /// The path.
        path: String,
    },
    /// The layout offers no place the runner collects.
    NoPlace,
}

/// The facts a reason is decided from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegressionFacts {
    /// `[acceptance] generate`.
    pub acceptance_on: bool,
    /// The task's story (`US1`), when it has one.
    pub story: Option<String>,
    /// The scenarios its story states.
    pub scenarios: usize,
    /// Why no acceptance test was kept, when none was.
    pub skip: Option<SkipCause>,
    /// How the acceptance tests' red run ended.
    pub red_class: Option<RedClass>,
}

/// The reason, by precedence (FR-013, highest first): `acceptance_off`,
/// `no_story`, `no_scenarios`, `author_failed`, `author_wrote_none`,
/// `tests_not_collected`, `not_red`, `already_passing`. `already_passing`
/// comes without `implemented_by`; the bisection fills it.
#[must_use]
pub fn decide(f: &RegressionFacts) -> RegressionReason {
    if !f.acceptance_on {
        return RegressionReason::AcceptanceOff;
    }
    let Some(story) = f.story.as_ref() else {
        return RegressionReason::NoStory;
    };
    if f.scenarios == 0 {
        return RegressionReason::NoScenarios {
            story: story.clone(),
        };
    }
    match &f.skip {
        Some(SkipCause::NoFiles) => return RegressionReason::AuthorWroteNone,
        Some(cause) => {
            return RegressionReason::AuthorFailed {
                detail: detail(cause),
            };
        }
        None => {}
    }
    match f.red_class {
        None => RegressionReason::AuthorWroteNone,
        Some(RedClass::NoTests) => RegressionReason::TestsNotCollected,
        Some(RedClass::Passed) => RegressionReason::AlreadyPassing {
            implemented_by: None,
        },
        Some(c) => RegressionReason::NotRed { red_class: c },
    }
}

/// A path an author chose, as speccoder shows it: one line, no backticks,
/// at most 120 characters, inside a code span.
#[must_use]
pub fn code_path(p: &str) -> String {
    let clean: String = p
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if c == '`' { '\'' } else { c })
        .collect();
    let clean = clean.trim_start_matches("./");
    let cut: String = if clean.chars().count() > 120 {
        clean.chars().take(119).chain(['…']).collect()
    } else {
        clean.to_owned()
    };
    format!("`{cut}`")
}

/// speccoder's words for a skip cause.
#[must_use]
pub fn detail(cause: &SkipCause) -> String {
    match cause {
        SkipCause::NoFiles => "the test author wrote no file".to_owned(),
        SkipCause::Refused { path, why } if why.starts_with("changes existing lines") => {
            format!("{why} of {}", code_path(path))
        }
        SkipCause::Refused { path, why } => format!("{} {why}", code_path(path)),
        SkipCause::CommandNotAllowed { argv } => {
            format!(
                "the command {} is not allowed by the policy",
                code_path(argv)
            )
        }
        SkipCause::WriteFailed { path } => format!("writing {} failed", code_path(path)),
        SkipCause::NoPlace => "no place the runner collects".to_owned(),
    }
}

fn class_words(c: RedClass) -> &'static str {
    match c {
        RedClass::Assertion => "assertion",
        RedClass::Absent => "absent symbol",
        RedClass::BuildError => "build error",
        RedClass::CollectionError => "collection error",
        RedClass::Timeout => "timeout",
        RedClass::NoTests => "no tests",
        RedClass::Flaky => "flaky",
        RedClass::Unclassified => "unrecognised output",
        RedClass::Passed => "passed",
    }
}

fn class_name(c: RedClass) -> &'static str {
    match c {
        RedClass::Assertion => "assertion",
        RedClass::Absent => "absent",
        RedClass::BuildError => "build_error",
        RedClass::CollectionError => "collection_error",
        RedClass::Timeout => "timeout",
        RedClass::NoTests => "no_tests",
        RedClass::Flaky => "flaky",
        RedClass::Unclassified => "unclassified",
        RedClass::Passed => "passed",
    }
}

fn class_named(s: &str) -> Option<RedClass> {
    [
        RedClass::Assertion,
        RedClass::Absent,
        RedClass::BuildError,
        RedClass::CollectionError,
        RedClass::Timeout,
        RedClass::NoTests,
        RedClass::Flaky,
        RedClass::Unclassified,
        RedClass::Passed,
    ]
    .into_iter()
    .find(|c| class_name(*c) == s)
}

/// The one sentence per reason, the same in every interface (FR-017): the
/// only place a reason is worded.
#[must_use]
pub fn wording(r: &RegressionReason) -> String {
    match r {
        RegressionReason::NoStory => "no user story — nothing to derive a test from".to_owned(),
        RegressionReason::NoScenarios { story } => {
            format!("its story {story} states no acceptance scenario")
        }
        RegressionReason::AcceptanceOff => "acceptance tests are off in speccoder.toml".to_owned(),
        RegressionReason::AuthorWroteNone => "the test author wrote no test for it".to_owned(),
        RegressionReason::AuthorFailed { detail } => {
            format!("its acceptance tests could not be used: {detail}")
        }
        RegressionReason::AlreadyPassing { implemented_by } => match implemented_by {
            Some(ImplementedBy::Task { task, .. }) => {
                format!("already passing before the attempt — implemented by {task}")
            }
            Some(ImplementedBy::BeforeFeature { .. }) => {
                "already passing before this feature".to_owned()
            }
            None => "already passing before the attempt; which task did it could not be determined"
                .to_owned(),
        },
        RegressionReason::NotRed { red_class } => format!(
            "its acceptance tests did not fail first ({})",
            class_words(*red_class)
        ),
        RegressionReason::TestsNotCollected => {
            "its acceptance tests never ran: the command collected 0 tests".to_owned()
        }
        RegressionReason::Unrecorded => {
            "why was not recorded (ticked by an earlier speccoder)".to_owned()
        }
    }
}

/// The trailer's name.
pub const TRAILER: &str = "Regression-Only";

/// The trailer's value: `<reason>[ <key>=<value>]…` (contracts/cli-json.md).
#[must_use]
pub fn trailer(r: &RegressionReason) -> String {
    let name = r.name();
    match r {
        RegressionReason::NoScenarios { story } => format!("{name} story={}", escape(story)),
        RegressionReason::AuthorFailed { detail } => format!("{name} detail={}", escape(detail)),
        RegressionReason::AlreadyPassing { implemented_by } => match implemented_by {
            Some(ImplementedBy::Task { task, commit }) => format!(
                "{name} implemented_by={} commit={}",
                escape(task),
                escape(commit)
            ),
            Some(ImplementedBy::BeforeFeature { .. }) => format!("{name} before_feature=true"),
            None => format!("{name} implemented_by=unknown"),
        },
        RegressionReason::NotRed { red_class } => {
            format!("{name} red_class={}", class_name(*red_class))
        }
        _ => name.to_owned(),
    }
}

/// Reads a trailer value back; `None` when it is not one speccoder writes.
#[must_use]
pub fn parse_trailer(value: &str) -> Option<RegressionReason> {
    let mut parts = value.split_whitespace();
    let name = parts.next()?;
    let mut kv = std::collections::BTreeMap::new();
    for p in parts {
        let (k, v) = p.split_once('=')?;
        kv.insert(k, unescape(v)?);
    }
    let get = |k: &str| kv.get(k).cloned();
    Some(match name {
        "no_story" => RegressionReason::NoStory,
        "no_scenarios" => RegressionReason::NoScenarios {
            story: get("story")?,
        },
        "acceptance_off" => RegressionReason::AcceptanceOff,
        "author_wrote_none" => RegressionReason::AuthorWroteNone,
        "author_failed" => RegressionReason::AuthorFailed {
            detail: get("detail")?,
        },
        "already_passing" => RegressionReason::AlreadyPassing {
            implemented_by: if get("before_feature").as_deref() == Some("true") {
                Some(ImplementedBy::BeforeFeature {
                    before_feature: true,
                })
            } else {
                match get("implemented_by") {
                    Some(t) if t != "unknown" => Some(ImplementedBy::Task {
                        task: t,
                        commit: get("commit").unwrap_or_default(),
                    }),
                    _ => None,
                }
            },
        },
        "not_red" => RegressionReason::NotRed {
            red_class: class_named(&get("red_class")?)?,
        },
        "tests_not_collected" => RegressionReason::TestsNotCollected,
        _ => return None,
    })
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn class() -> impl Strategy<Value = RedClass> {
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

    fn reason() -> impl Strategy<Value = RegressionReason> {
        prop_oneof![
            Just(RegressionReason::NoStory),
            "[A-Z]{2}[0-9]{1,2}".prop_map(|story| RegressionReason::NoScenarios { story }),
            Just(RegressionReason::AcceptanceOff),
            Just(RegressionReason::AuthorWroteNone),
            ".{0,40}".prop_map(|detail| RegressionReason::AuthorFailed { detail }),
            Just(RegressionReason::AlreadyPassing {
                implemented_by: None
            }),
            Just(RegressionReason::AlreadyPassing {
                implemented_by: Some(ImplementedBy::BeforeFeature {
                    before_feature: true
                })
            }),
            ("T[0-9]{3}", "[0-9a-f]{7}").prop_map(|(task, commit)| {
                RegressionReason::AlreadyPassing {
                    implemented_by: Some(ImplementedBy::Task { task, commit }),
                }
            }),
            class().prop_map(|red_class| RegressionReason::NotRed { red_class }),
            Just(RegressionReason::TestsNotCollected),
        ]
    }

    proptest! {
        #[test]
        fn trailers_round_trip(r in reason()) {
            prop_assert_eq!(parse_trailer(&trailer(&r)), Some(r));
        }

        #[test]
        fn decide_is_total_and_follows_precedence(
            on in any::<bool>(),
            story in proptest::option::of("US[0-9]"),
            scenarios in 0usize..3,
            skip in proptest::option::of(prop_oneof![
                Just(SkipCause::NoFiles),
                Just(SkipCause::NoPlace),
                Just(SkipCause::WriteFailed { path: "a".to_owned() }),
            ]),
            red in proptest::option::of(class()),
        ) {
            let f = RegressionFacts { acceptance_on: on, story: story.clone(), scenarios, skip: skip.clone(), red_class: red };
            let r = decide(&f);
            let expected = if !on { "acceptance_off" }
                else if story.is_none() { "no_story" }
                else if scenarios == 0 { "no_scenarios" }
                else if matches!(skip, Some(SkipCause::NoPlace | SkipCause::WriteFailed { .. })) { "author_failed" }
                else if skip.is_some() { "author_wrote_none" }
                else if red == Some(RedClass::NoTests) { "tests_not_collected" }
                else if red.is_none() { "author_wrote_none" }
                else if red == Some(RedClass::Passed) { "already_passing" }
                else { "not_red" };
            prop_assert_eq!(r.name(), expected);
            prop_assert!(!wording(&r).is_empty());
        }
    }

    #[test]
    fn the_wording_is_the_spec_s() {
        let r = RegressionReason::AlreadyPassing {
            implemented_by: Some(ImplementedBy::Task {
                task: "T003".to_owned(),
                commit: "d433d77".to_owned(),
            }),
        };
        assert_eq!(
            wording(&r),
            "already passing before the attempt — implemented by T003"
        );
        assert_eq!(
            trailer(&r),
            "already_passing implemented_by=T003 commit=d433d77"
        );
        assert_eq!(
            wording(&RegressionReason::NotRed {
                red_class: RedClass::BuildError
            }),
            "its acceptance tests did not fail first (build error)"
        );
        assert_eq!(
            trailer(&RegressionReason::NotRed {
                red_class: RedClass::BuildError
            }),
            "not_red red_class=build_error"
        );
        let d = detail(&SkipCause::Refused {
            path: "tests/unit/x.rs".to_owned(),
            why: "is not where the runner collects tests".to_owned(),
        });
        assert_eq!(
            d,
            "`tests/unit/x.rs` is not where the runner collects tests"
        );
        assert_eq!(
            serde_json::to_value(&r).unwrap_or_default(),
            serde_json::json!({ "reason": "already_passing", "implemented_by": { "task": "T003", "commit": "d433d77" } })
        );
        assert!(code_path(&"a".repeat(300)).chars().count() <= 122);
    }
}
