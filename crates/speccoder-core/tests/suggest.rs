//! The composer's rung suggestion: deterministic, and right on a table of
//! labelled texts (at least 90 % of 40).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use speccoder_core::ladder::{RungCueRule, RungPick, command_for, suggest};

/// Texts a person types into the composer, and the rung they meant.
const TABLE: &[(&str, RungPick)] = &[
    // Bugs: a wrong result of existing behaviour.
    ("the slug of \"Ünïcödé\" is empty", RungPick::Bug),
    ("saving an empty title crashes the app", RungPick::Bug),
    ("login is broken on Safari", RungPick::Bug),
    ("the total is wrong when a discount applies", RungPick::Bug),
    ("export fails when the report has no rows", RungPick::Bug),
    ("the search no longer finds archived items", RungPick::Bug),
    (
        "dates show in UTC instead of the user's time zone",
        RungPick::Bug,
    ),
    ("the CLI panics on a missing config file", RungPick::Bug),
    ("pagination doesn't stop at the last page", RungPick::Bug),
    ("the upload hangs at 99 percent", RungPick::Bug),
    (
        "When I press save twice, two records are created and the second one has an error",
        RungPick::Bug,
    ),
    (
        "regression: the dark theme lost its contrast",
        RungPick::Bug,
    ),
    ("the avatar is missing on the profile page", RungPick::Bug),
    // Questions.
    ("why does the build take ten minutes?", RungPick::Ask),
    ("how is the session token stored", RungPick::Ask),
    ("what happens when the cache is full?", RungPick::Ask),
    ("which module parses the config", RungPick::Ask),
    ("does the API support pagination", RungPick::Ask),
    ("where are the migrations kept?", RungPick::Ask),
    ("can we drop Node 18", RungPick::Ask),
    // Features: several capabilities, or long.
    (
        "Users can export a report as CSV, with the columns they chose",
        RungPick::Feature,
    ),
    (
        "add a settings page and let users change their password",
        RungPick::Feature,
    ),
    (
        "teams can invite members; members can leave a team",
        RungPick::Feature,
    ),
    (
        "a dashboard of open tasks:\n- filter by owner\n- sort by due date\n- export the view",
        RungPick::Feature,
    ),
    (
        "Build a notification centre where people see mentions from every project they belong to, mark each one read, mute a project for a week and get a daily digest by email",
        RungPick::Feature,
    ),
    (
        "support SSO with Okta and provision accounts from the directory",
        RungPick::Feature,
    ),
    (
        "offline mode: cache the last 50 documents, sync edits when back online",
        RungPick::Feature,
    ),
    (
        "a public API with tokens and rate limits",
        RungPick::Feature,
    ),
    // Quick changes: one requirement.
    ("add a --verbose flag", RungPick::Quick),
    ("rename the Save button to Apply", RungPick::Quick),
    ("make the default page size 50", RungPick::Quick),
    ("log the request id with every line", RungPick::Quick),
    ("show the version in the footer", RungPick::Quick),
    ("sort tags alphabetically", RungPick::Quick),
    ("trim whitespace from user names", RungPick::Quick),
    ("bump the timeout to 30 seconds", RungPick::Quick),
    ("use the system font on macOS", RungPick::Quick),
    ("hide archived projects from the sidebar", RungPick::Quick),
    ("allow emoji in titles", RungPick::Quick),
    ("print a summary line at the end of a run", RungPick::Quick),
];

#[test]
fn the_labelled_table_is_suggested_right_at_least_ninety_percent() {
    assert_eq!(TABLE.len(), 40);
    let wrong: Vec<String> = TABLE
        .iter()
        .filter_map(|(text, want)| {
            let got = suggest(text).rung;
            (got != *want).then(|| format!("{text:?}: {got:?}, want {want:?}"))
        })
        .collect();
    assert!(
        wrong.len() * 10 <= TABLE.len(),
        "{} of {} wrong:\n{}",
        wrong.len(),
        TABLE.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_bug_names_its_cue_verbatim() {
    let s = suggest("the slug of \"Ünïcödé\" is empty");
    assert_eq!(s.rung, RungPick::Bug);
    assert_eq!(s.cue.rule, RungCueRule::WrongResult);
    assert_eq!(s.cue.matched, vec!["is empty".to_owned()]);
    assert!(s.reason.contains("(\"is empty\")"), "{}", s.reason);
    assert!(s.available);
    assert_eq!(
        s.alternatives,
        vec![RungPick::Quick, RungPick::Feature, RungPick::Ask]
    );
}

#[test]
fn a_question_is_ask_and_not_available_yet() {
    let s = suggest("why does the build take ten minutes?");
    assert_eq!(s.rung, RungPick::Ask);
    assert!(!s.available);
    assert_eq!(s.cue.rule, RungCueRule::Question);
    assert!(s.reason.contains("pick a rung"), "{}", s.reason);
    assert_eq!(command_for(s.rung, "x", Some("specify")), None);
}

#[test]
fn empty_text_suggests_nothing() {
    for t in ["", "   ", "\n\t"] {
        let s = suggest(t);
        assert_eq!(s.rung, RungPick::None);
        assert!(!s.available);
        assert_eq!(s.reason, "Describe the change first.");
    }
}

#[test]
fn long_text_is_a_feature_and_says_how_long() {
    let text = (0..30)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let s = suggest(&text);
    assert_eq!(s.rung, RungPick::Feature);
    assert_eq!(s.cue.rule, RungCueRule::Long);
    assert!(s.reason.contains("30 words"), "{}", s.reason);
}

#[test]
fn commands_quote_the_text_and_name_the_format_skill() {
    assert_eq!(
        command_for(RungPick::Bug, "say \"hi\"", None).as_deref(),
        Some("speccoder bug \"say \\\"hi\\\"\"")
    );
    assert_eq!(
        command_for(RungPick::Quick, "a\n b", None).as_deref(),
        Some("speccoder do \"a b\"")
    );
    assert_eq!(
        command_for(RungPick::Feature, "x", Some("propose")).as_deref(),
        Some("speccoder spec propose \"x\"")
    );
    assert_eq!(command_for(RungPick::Feature, "x", None), None);
}

proptest! {
    #[test]
    fn the_same_text_gives_the_same_suggestion(text in ".{0,200}") {
        let a = suggest(&text);
        for _ in 0..3 {
            prop_assert_eq!(&suggest(&text), &a);
        }
        prop_assert!(a.cue.matched.iter().all(|m| m == "?" || text.contains(m.as_str())));
        prop_assert_eq!(a.alternatives.len(), if a.rung == RungPick::None { 4 } else { 3 });
    }
}
