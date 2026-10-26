//! The four renderers of a call needing a person — the terminal prompt,
//! the dashboard overlay, the editor (ACP) and the web sheet — hold to one
//! presentation: the same object lines, the same source lines and the same
//! choices in the same order. An option that exists in
//! one interface and not in another is a hole.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::Value;
use speccoder_runtime::approvals::{PresentContext, Presentation, Request, present};

fn fixtures() -> Vec<(String, Presentation)> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-runtime/tests/fixtures/object-grants/presentations.json");
    let all: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    all.into_iter()
        .map(|f| {
            let request: Request = serde_json::from_value(f["request"].clone()).unwrap();
            let p = present(
                &request,
                &PresentContext {
                    root: None,
                    sandbox: Some("Seatbelt".to_owned()),
                    network: "none".to_owned(),
                    file_text: None,
                },
            );
            (f["name"].as_str().unwrap().to_owned(), p)
        })
        .collect()
}

/// What the terminal prompt shows: its object, source and choice lines.
fn terminal(p: &Presentation) -> (Vec<String>, Vec<String>, Vec<String>) {
    let shown = crate::style::asciify(&crate::prompt::render(p));
    let pick = |tag: &str| -> Vec<String> {
        shown
            .lines()
            .filter_map(|l| l.strip_prefix(tag).map(str::to_owned))
            .collect()
    };
    let sources: Vec<String> = pick("  Copied       ")
        .into_iter()
        .filter(|l| !l.starts_with("arguments came from"))
        .collect();
    let choices = p
        .options
        .iter()
        .filter(|o| {
            shown.contains(&format!("({})", o.key)) && shown.contains(&format!("{}. ", o.number))
        })
        .map(|o| o.choice.clone())
        .collect();
    (pick("  Object       "), sources, choices)
}

/// What an editor is sent.
fn acp(p: &Presentation) -> (Vec<String>, Vec<String>, Vec<String>) {
    let text = crate::acp::approval_text(p);
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let object = lines.iter().take(p.object.len()).cloned().collect();
    let sources = lines
        .iter()
        .skip(p.object.len())
        .take(p.sources.len())
        .cloned()
        .collect();
    let choices = crate::acp::permission_options(p)
        .iter()
        .filter_map(|o| o["optionId"].as_str().map(str::to_owned))
        .collect();
    (object, sources, choices)
}

#[test]
fn every_renderer_offers_the_presentations_options_and_lines() {
    for (name, p) in fixtures() {
        let want = (
            p.object
                .iter()
                .map(|l| crate::style::asciify(l))
                .collect::<Vec<_>>(),
            p.sources
                .iter()
                .map(|l| crate::style::asciify(l))
                .collect::<Vec<_>>(),
            p.request.options.clone(),
        );
        assert_eq!(terminal(&p), want, "{name}: terminal");
        let (o, s, c) = acp(&p);
        assert_eq!(
            (o, s, c),
            (
                p.object.clone(),
                p.sources.clone(),
                p.request.options.clone()
            ),
            "{name}: acp"
        );
        // The choices' keys are the same everywhere keys exist.
        let keys: String = p.options.iter().map(|o| o.key).collect();
        let expected: String = p
            .request
            .options
            .iter()
            .map(|o| match o.as_str() {
                "once" => 'y',
                "task" => 't',
                "run" => 'r',
                "always" => 'a',
                "reject" => 'n',
                _ => 's',
            })
            .collect();
        assert_eq!(keys, expected, "{name}");
    }
    // The fixtures cover each case the spec names.
    let names: Vec<String> = fixtures().into_iter().map(|(n, _)| n).collect();
    for n in [
        "write",
        "write via symlink",
        "command",
        "network",
        "mcp changed digest",
        "derived",
        "high risk",
        "budget-reaching",
    ] {
        assert!(names.iter().any(|x| x == n), "{n}");
    }
}

#[test]
fn the_dashboard_and_the_web_sheet_render_the_presentation_not_their_own() {
    // The dashboard overlay draws the presentation's lines and choices.
    let draw = include_str!("tui/draw.rs");
    for field in ["&p.object", "&p.sources", "&p.options", "&p.notice"] {
        assert!(draw.contains(field), "the overlay does not draw {field}");
    }
    // The web sheet offers only what the request offers, and Always Allow
    // is an answer to the runtime, never a policy write of its own.
    let sheet = include_str!("../../speccoder-web/ui/src/components/AskSheet.svelte");
    assert!(sheet.contains("new Set(p?.request.options"));
    assert!(sheet.contains("offered.has(\"always\")"));
    assert!(
        !sheet.contains("/api/config"),
        "the sheet writes the policy itself"
    );
    assert!(sheet.contains("p.object") && sheet.contains("p.sources"));
}
