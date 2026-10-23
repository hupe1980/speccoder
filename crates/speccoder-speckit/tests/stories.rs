//! User stories of real and template specs read as requirements.

#![allow(clippy::unwrap_used)]

use std::path::Path;

#[test]
fn every_story_of_the_corpus_specs_is_a_requirement() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (spec, stories, scenarios) in [
        ("compat/1.2.3/specs/001-sample/spec.md", 3, 4),
        ("real/speckit-1.2.2/specs/001-search/spec.md", 0, 0),
    ] {
        let text = std::fs::read_to_string(fixtures.join(spec)).unwrap();
        let (reqs, diags) = speccoder_speckit::stories::requirements(spec, &text);
        assert!(diags.is_empty(), "{spec}: {diags:?}");
        assert_eq!(reqs.len(), stories, "{spec}");
        let n: usize = reqs.iter().map(|r| r.scenarios.len()).sum();
        assert_eq!(n, scenarios, "{spec}");
        for r in &reqs {
            assert_eq!(
                r.text,
                speccoder_speckit::stories::story_section(&text, &r.key).trim_end()
            );
            for s in &r.scenarios {
                assert_eq!(&text[s.source.start..s.source.end], s.raw);
            }
        }
    }
}

#[test]
fn scenarios_are_numbered_per_story() {
    let spec = "### User Story 1 - Greet (Priority: P1)\n\n**Acceptance Scenarios**:\n\n1. **Given** a name, **When** greeted, **Then** hello\n2. **Given** none, **When** greeted, **Then** world\n\n### User Story 2 - Add\n\nA caller adds.\n\n**Acceptance**: `add(2, 3) == 5`.\n";
    let us1 = speccoder_speckit::stories::scenario_refs(spec, Some("US1"));
    let ids: Vec<&str> = us1.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["US1-1", "US1-2"]);
    assert_eq!(
        us1[1].text,
        "**Given** none, **When** greeted, **Then** world"
    );
    let us2 = speccoder_speckit::stories::scenario_refs(spec, Some("US2"));
    assert_eq!(us2.len(), 1);
    assert_eq!(
        (us2[0].id.as_str(), us2[0].text.as_str()),
        ("US2-1", "**Acceptance**: `add(2, 3) == 5`.")
    );
    assert_eq!(
        speccoder_speckit::stories::scenario_refs(spec, None).len(),
        3
    );
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/compat/1.2.3/specs/001-sample/spec.md"),
    )
    .unwrap();
    assert_eq!(
        speccoder_speckit::stories::scenario_refs(&text, Some("US1")).len(),
        2
    );
}
