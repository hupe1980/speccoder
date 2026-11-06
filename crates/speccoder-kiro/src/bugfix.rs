//! A bugfix spec's `bugfix.md`: its Current, Expected and Unchanged
//! statements, numbered as the file numbers them (`2.1 WHEN … SHALL …`).

/// One statement: its number in the file (`2.1`, else its position) and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// `2.1`.
    pub id: String,
    /// The statement, wrapped lines joined.
    pub text: String,
}

/// The triad of a `bugfix.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bugfix {
    /// Current Behavior (Defect).
    pub current: Vec<Statement>,
    /// Expected Behavior (Correct).
    pub expected: Vec<Statement>,
    /// Unchanged Behavior (Regression Prevention).
    pub unchanged: Vec<Statement>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Current,
    Expected,
    Unchanged,
}

fn section(heading: &str) -> Option<Section> {
    let h = heading.to_lowercase();
    if h.contains("current behavio") {
        Some(Section::Current)
    } else if h.contains("expected behavio") {
        Some(Section::Expected)
    } else if h.contains("unchanged behavio") {
        Some(Section::Unchanged)
    } else {
        None
    }
}

/// `1.1 WHEN …`, `1. WHEN …`, `- WHEN …`: the item's number (if any) and text.
fn item(line: &str) -> Option<(Option<String>, &str)> {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return Some((None, rest.trim()));
    }
    let t = t.trim_start_matches("**");
    let number: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if number.is_empty() || !number.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let rest = &t[number.len()..];
    let rest = rest.trim_start_matches("**");
    // `1.1 WHEN`, `1. WHEN`: a number then a space; never `1.5x faster`.
    if !rest.starts_with(' ') {
        return None;
    }
    let number = number.trim_end_matches('.');
    Some((Some(number.to_owned()), rest.trim()))
}

/// Reads `bugfix.md`.
///
/// # Errors
///
/// No Current or no Expected statement: the triad is not there to run.
pub fn read(text: &str) -> Result<Bugfix, String> {
    let mut b = Bugfix {
        current: Vec::new(),
        expected: Vec::new(),
        unchanged: Vec::new(),
    };
    let mut at: Option<Section> = None;
    let mut open = false;
    let mut fence = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = !fence;
            open = false;
            continue;
        }
        if fence {
            continue;
        }
        if trimmed.starts_with('#') {
            at = section(trimmed);
            open = false;
            continue;
        }
        let Some(s) = at else { continue };
        let list = match s {
            Section::Current => &mut b.current,
            Section::Expected => &mut b.expected,
            Section::Unchanged => &mut b.unchanged,
        };
        if trimmed.is_empty() {
            open = false;
            continue;
        }
        if let Some((number, rest)) = item(line) {
            // `2.1` as written; a plain `1.` or `-` is numbered in its section.
            let n = match s {
                Section::Current => 1,
                Section::Expected => 2,
                Section::Unchanged => 3,
            };
            let id = match number {
                Some(x) if x.contains('.') => x,
                Some(x) => format!("{n}.{x}"),
                None => format!("{n}.{}", list.len() + 1),
            };
            list.push(Statement {
                id,
                text: rest.to_owned(),
            });
            open = true;
        } else if open && let Some(last) = list.last_mut() {
            last.text.push(' ');
            last.text.push_str(trimmed);
        }
    }
    if b.current.is_empty() {
        return Err("no Current Behavior statement".to_owned());
    }
    if b.expected.is_empty() {
        return Err("no Expected Behavior statement".to_owned());
    }
    Ok(b)
}

/// `2.1–2.3`, `3.1`: a statement list's numbers as a range.
#[must_use]
pub fn span(statements: &[Statement]) -> String {
    match statements {
        [] => String::new(),
        [only] => only.id.clone(),
        [first, .., last] => format!("{}–{}", first.id, last.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiros_numbered_wrapped_statements_are_read() {
        let text = "# Bugfix Requirements Document\n\n## Introduction\n\n1. Not a statement.\n\n## Bug Analysis\n\n### Current Behavior (Defect)\n\n1.1 WHEN a run has more than 100 jobs THEN the system fetches only the\nfirst 100 jobs\n\n1.2 WHEN called THEN it uses per_page 100\n\n### Expected Behavior (Correct)\n\n2.1 WHEN a run has more than 100 jobs THEN the system SHALL fetch all\njobs\n\n### Unchanged Behavior (Regression Prevention)\n\n3.1 WHEN a run has 100 or fewer jobs THEN the system SHALL CONTINUE TO\nfetch all jobs\n\n---\n\n## Bug Condition (Formal)\n\n```pascal\n1.1 FUNCTION isBugCondition(X)\n```\n";
        let b = read(text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(b.current.len(), 2);
        assert_eq!(
            b.current[0].text,
            "WHEN a run has more than 100 jobs THEN the system fetches only the first 100 jobs"
        );
        assert_eq!(b.expected[0].id, "2.1");
        assert!(b.expected[0].text.ends_with("SHALL fetch all jobs"));
        assert_eq!(b.unchanged.len(), 1);
        assert_eq!(span(&b.current), "1.1–1.2");
    }

    #[test]
    fn list_markers_and_numbered_lists_are_read() {
        let text = "## Current Behavior (Defect)\n- WHEN idle THEN logged out\n## Expected Behavior (Correct)\n1. WHEN idle THEN the system SHALL keep the session\n2. WHEN active THEN the system SHALL keep it\n## Unchanged Behavior\n";
        let b = read(text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(b.current[0].id, "1.1");
        assert_eq!(
            b.expected.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["2.1", "2.2"]
        );
        assert!(b.unchanged.is_empty());
    }

    #[test]
    fn a_triad_without_current_or_expected_is_refused() {
        assert!(read("## Expected Behavior\n2.1 WHEN x THEN SHALL y\n").is_err());
        assert!(read("## Current Behavior\n1.1 WHEN x THEN y\n").is_err());
    }
}
