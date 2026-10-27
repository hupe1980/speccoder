//! The similarity hint: deterministic, symmetric, and right on a table
//! of 30 labelled descriptions against one project's requirements — the
//! table that fixes the thresholds.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use speccoder_core::similar::{CLOSE, Requirement, SimilarLevel, matches, shared_terms};

fn req(unit: &str, id: &str, text: &str) -> Requirement {
    Requirement {
        unit: unit.to_owned(),
        id: id.to_owned(),
        text: text.to_owned(),
    }
}

/// A project of four features.
fn corpus() -> Vec<Requirement> {
    vec![
        req(
            "001-login",
            "FR-001",
            "Users MUST sign in with an email address and a password.",
        ),
        req(
            "001-login",
            "FR-002",
            "The system MUST lock an account after five failed sign-in attempts.",
        ),
        req(
            "001-login",
            "FR-003",
            "Users MUST be able to reset a forgotten password by email.",
        ),
        req(
            "002-billing",
            "FR-001",
            "The system MUST charge the card on file at the start of each billing period.",
        ),
        req(
            "002-billing",
            "FR-002",
            "Users MUST be able to download every invoice as a PDF.",
        ),
        req(
            "002-billing",
            "FR-003",
            "The system MUST send a receipt by email after each payment.",
        ),
        req(
            "003-reports",
            "FR-001",
            "Users MUST be able to create a report from a saved filter.",
        ),
        req(
            "003-reports",
            "FR-002",
            "Reports MUST show totals per month.",
        ),
        req(
            "003-reports",
            "FR-003",
            "Users MUST be able to schedule a report to run every week.",
        ),
        req("003-reports", "FR-004", "Users can export a report as PDF."),
        req(
            "004-search",
            "FR-001",
            "Search MUST match titles and tags, ignoring case.",
        ),
        req(
            "004-search",
            "FR-002",
            "Search results MUST be paginated, twenty per page.",
        ),
        req(
            "004-search",
            "FR-003",
            "Archived items MUST be excluded from search unless asked for.",
        ),
    ]
}

/// Descriptions and the requirement they duplicate (or `None`).
const TABLE: &[(&str, Option<(&str, &str)>)] = &[
    ("export a report as CSV", Some(("003-reports", "FR-004"))),
    (
        "Users can export a report as CSV, with the columns they chose",
        Some(("003-reports", "FR-004")),
    ),
    ("export reports to PDF", Some(("003-reports", "FR-004"))),
    ("reset a forgotten password", Some(("001-login", "FR-003"))),
    ("password reset by email", Some(("001-login", "FR-003"))),
    (
        "lock accounts after failed sign-in attempts",
        Some(("001-login", "FR-002")),
    ),
    (
        "sign in with email and password",
        Some(("001-login", "FR-001")),
    ),
    ("download invoices as PDF", Some(("002-billing", "FR-002"))),
    (
        "email a receipt after payment",
        Some(("002-billing", "FR-003")),
    ),
    (
        "charge the card each billing period",
        Some(("002-billing", "FR-001")),
    ),
    ("schedule a weekly report", Some(("003-reports", "FR-003"))),
    ("monthly totals in reports", Some(("003-reports", "FR-002"))),
    (
        "create a report from a saved filter",
        Some(("003-reports", "FR-001")),
    ),
    ("paginate search results", Some(("004-search", "FR-002"))),
    (
        "search titles and tags ignoring case",
        Some(("004-search", "FR-001")),
    ),
    (
        "exclude archived items from search",
        Some(("004-search", "FR-003")),
    ),
    ("search by tag", Some(("004-search", "FR-001"))),
    ("invoice download", Some(("002-billing", "FR-002"))),
    (
        "weekly scheduled reports by email",
        Some(("003-reports", "FR-003")),
    ),
    (
        "show search results twenty per page",
        Some(("004-search", "FR-002")),
    ),
    // Unrelated to anything there.
    ("dark mode for the settings page", None),
    ("a command-line flag for verbose logging", None),
    ("upload a profile picture", None),
    ("translate the interface into German", None),
    ("rate limit the public API", None),
    ("keyboard shortcuts for the editor", None),
    ("a health check endpoint for the load balancer", None),
    ("drag and drop to reorder columns", None),
    ("two-factor authentication with TOTP apps", None),
    ("offline caching of the last documents", None),
];

#[test]
fn the_labelled_table_holds_at_the_fixed_thresholds() {
    assert_eq!(TABLE.len(), 30);
    let corpus = corpus();
    let mut wrong = Vec::new();
    for (query, want) in TABLE {
        let got = matches(query, &corpus);
        let first = got
            .first()
            .map(|m| (m.unit.as_str(), m.requirement.as_str()));
        if first != *want {
            wrong.push(format!(
                "{query:?}: {:?} (score {:?}), want {want:?}",
                first,
                got.first().map(|m| m.score)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_reports_example_names_its_shared_terms() {
    let got = matches("export a report as CSV", &corpus());
    let m = got.first().unwrap();
    assert_eq!(
        (m.unit.as_str(), m.requirement.as_str()),
        ("003-reports", "FR-004")
    );
    assert_eq!(m.text, "Users can export a report as PDF.");
    assert!(m.score >= CLOSE);
    assert!(matches!(
        m.level,
        SimilarLevel::Close | SimilarLevel::VeryClose
    ));
    assert_eq!(m.shared, vec!["export".to_owned(), "report".to_owned()]);
    assert!(got.len() <= 3);
}

#[test]
fn an_exact_copy_is_very_close() {
    let got = matches(
        "Users MUST be able to download every invoice as a PDF.",
        &corpus(),
    );
    assert_eq!(got[0].level, SimilarLevel::VeryClose);
    assert!((got[0].score - 1.0).abs() < 0.01, "{}", got[0].score);
}

#[test]
fn a_hundred_runs_give_the_same_matches() {
    let corpus = corpus();
    let first = matches("export a report as CSV", &corpus);
    for _ in 0..100 {
        assert_eq!(matches("export a report as CSV", &corpus), first);
    }
}

#[test]
fn ties_go_to_the_lower_unit_number_then_the_identifier() {
    let corpus = vec![
        req("010-b", "FR-002", "export a ledger"),
        req("002-a", "FR-009", "export a ledger"),
        req("002-a", "FR-001", "export a ledger"),
    ];
    let got = matches("export a ledger", &corpus);
    let order: Vec<(&str, &str)> = got
        .iter()
        .map(|m| (m.unit.as_str(), m.requirement.as_str()))
        .collect();
    assert_eq!(
        order,
        vec![
            ("002-a", "FR-001"),
            ("002-a", "FR-009"),
            ("010-b", "FR-002")
        ]
    );
}

proptest! {
    #[test]
    fn empty_queries_match_nothing_and_shared_terms_are_symmetric(
        a in "[a-z ]{0,60}", b in "[a-z ]{0,60}"
    ) {
        prop_assert!(matches("", &corpus()).is_empty());
        prop_assert!(matches("   the of a", &corpus()).is_empty());
        prop_assert_eq!(shared_terms(&a, &b), shared_terms(&b, &a));
        let once = matches(&a, &corpus());
        prop_assert_eq!(matches(&a, &corpus()), once);
    }
}
