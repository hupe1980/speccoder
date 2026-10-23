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
