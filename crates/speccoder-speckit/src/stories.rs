//! User stories of `spec.md`: the section a task's `[USn]` label points at,
//! and the stories as format-neutral requirements with their Given/When/Then
//! acceptance scenarios.

use speccoder_core::snapshot::{Diagnostic, Severity};
use speccoder_core::spec::{Requirement, Scenario, ScenarioRef, SourceRange};

/// The `### User Story N` section for label `USN`, up to the next heading of
/// the same or a higher level.
#[must_use]
pub fn story_section(spec: &str, label: &str) -> String {
    let Some(n) = label.strip_prefix("US") else {
        return String::new();
    };
    let wanted = format!("user story {n}");
    let mut out = Vec::new();
    let mut level = None;
    for line in spec.lines() {
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let is_heading = hashes > 0 && line[hashes..].starts_with(' ');
        if let Some(l) = level {
            if is_heading && hashes <= l {
                break;
            }
            out.push(line);
        } else if is_heading {
            let text = line[hashes..].trim().to_lowercase();
            let names_it = text
                .strip_prefix(&wanted)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()));
            if names_it {
                level = Some(hashes);
                out.push(line);
            }
        }
    }
    out.join("\n")
}

/// Each `### User Story N - Title` of the spec at `path` as a requirement
/// keyed `USN`, with the numbered `**Given** … **When** … **Then** …` items
/// of its section as scenarios keyed `USN/ACm`. A key that appears twice is
/// reported and the later story skipped.
#[must_use]
pub fn requirements(path: &str, text: &str) -> (Vec<Requirement>, Vec<Diagnostic>) {
    let mut lines = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        lines.push((at, line));
        at += line.len();
    }
    let heading = |line: &str| {
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        (hashes > 0 && line[hashes..].starts_with(' ')).then_some(hashes)
    };
    let range = |start: usize, end: usize| SourceRange {
        path: path.to_owned(),
        start,
        end,
    };

    let mut found: Vec<Requirement> = Vec::new();
    let mut diags = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (start, line) = lines[i];
        let Some(level) = heading(line) else {
            i += 1;
            continue;
        };
        let Some((n, title)) = story_heading(line[level..].trim()) else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < lines.len() && heading(lines[j].1).is_none_or(|l| l > level) {
            j += 1;
        }
        let end = lines.get(j).map_or(text.len(), |(s, _)| *s);
        let key = format!("US{n}");
        let scenarios = lines[i + 1..j]
            .iter()
            .filter(|(_, l)| is_scenario(l))
            .enumerate()
            .map(|(m, (s, l))| Scenario {
                key: format!("{key}/AC{}", m + 1),
                raw: l.trim_end().to_owned(),
                source: range(*s, s + l.trim_end().len()),
            })
            .collect();
        if found.iter().any(|r| r.key == key) {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "spec.duplicate-requirement",
                    format!("{key} appears more than once; the first is used"),
                )
                .at(path),
            );
        } else {
            found.push(Requirement {
                key,
                title,
                text: text[start..end].trim_end().to_owned(),
                scenarios,
                source: range(start, end),
            });
        }
        i = j;
    }
    (found, diags)
}

/// The acceptance scenarios a task's tests cite, as `US<n>-<k>` with the
/// spec's own words: the scenarios of the task's story (`label`), or of
/// every story for a convergence task (`label` = `None`). A story without
/// numbered scenarios but with an `**Acceptance**` line has that line as
/// `US<n>-1`.
#[must_use]
pub fn scenario_refs(spec: &str, label: Option<&str>) -> Vec<ScenarioRef> {
    let (stories, _) = requirements("spec.md", spec);
    let mut out = Vec::new();
    for story in stories.iter().filter(|r| label.is_none_or(|l| l == r.key)) {
        if story.scenarios.is_empty() {
            if let Some(line) = story
                .text
                .lines()
                .find(|l| l.to_lowercase().contains("**acceptance"))
            {
                out.push(ScenarioRef {
                    id: format!("{}-1", story.key),
                    text: line.trim().to_owned(),
                });
            }
            continue;
        }
        for (k, sc) in story.scenarios.iter().enumerate() {
            let text = sc.raw.trim_start();
            let digits = text.bytes().take_while(u8::is_ascii_digit).count();
            out.push(ScenarioRef {
                id: format!("{}-{}", story.key, k + 1),
                text: text[digits..].trim_start_matches('.').trim().to_owned(),
            });
        }
    }
    out
}

/// `User Story 3 - Title (Priority: P2)` → (3, "Title").
fn story_heading(text: &str) -> Option<(u32, String)> {
    let rest = text
        .get(..11)
        .filter(|p| p.eq_ignore_ascii_case("user story "))
        .map(|_| &text[11..])?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n = digits.parse().ok()?;
    let rest = rest[digits.len()..].trim_start();
    let title = rest
        .strip_prefix(['-', '–', '—', ':'])
        .unwrap_or(rest)
        .trim();
    let title = match title.rfind("(Priority") {
        Some(p) => title[..p].trim_end(),
        None => title,
    };
    Some((n, title.to_owned()))
}

/// A numbered list item stating a Given/When/Then scenario.
fn is_scenario(line: &str) -> bool {
    let l = line.trim_start();
    let digits = l.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && l[digits..].starts_with(". ") && l.to_lowercase().contains("**given**")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nAs a user…\n\n**Acceptance**: works\n\n### User Story 10 - Other\n\nno\n\n### User Story 2 - Profile\n\nprofile text\n\n## Requirements\n\n- FR-001\n";

    #[test]
    fn story_section_stops_at_the_next_story_and_does_not_confuse_10_with_1() {
        let s = story_section(SPEC, "US1");
        assert!(s.starts_with("### User Story 1 - Login"));
        assert!(s.contains("**Acceptance**: works"));
        assert!(!s.contains("Other"));
        let s2 = story_section(SPEC, "US2");
        assert!(s2.contains("profile text") && !s2.contains("FR-001"));
        assert_eq!(story_section(SPEC, "US3"), "");
    }

    #[test]
    fn stories_become_requirements_with_their_scenarios() {
        let text = "# S\n\n### User Story 1 - Search by name (Priority: P1)\n\nWhy.\n\n**Acceptance Scenarios**:\n\n1. **Given** a list, **When** I search, **Then** I see matches\n2. **Given** nothing, **When** I search, **Then** I see none\n\n### User Story 2 - Export\n\n1. Not a scenario\n\n## Requirements\n\n### User Story 1 - Again\n";
        let (reqs, diags) = requirements("specs/001-x/spec.md", text);
        assert_eq!(reqs.len(), 2);
        let us1 = &reqs[0];
        assert_eq!(
            (us1.key.as_str(), us1.title.as_str()),
            ("US1", "Search by name")
        );
        assert_eq!(us1.text, story_section(text, "US1").trim_end());
        assert_eq!(us1.scenarios.len(), 2);
        assert_eq!(us1.scenarios[1].key, "US1/AC2");
        let s = &us1.scenarios[0].source;
        assert_eq!(&text[s.start..s.end], us1.scenarios[0].raw);
        assert!(us1.scenarios[0].raw.starts_with("1. **Given** a list"));
        assert_eq!(
            &text[us1.source.start..us1.source.end],
            format!("{}\n\n", us1.text)
        );
        assert!(reqs[1].scenarios.is_empty() && reqs[1].title == "Export");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "spec.duplicate-requirement");
    }
}
