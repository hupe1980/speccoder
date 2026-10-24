//! Task scope as a reported finding: which changed paths a task's line did
//! not name. Never a gate — a task line is prose, and a list of paths in
//! it is a hint, not a contract (D-058; architecture §6.3).

use serde::{Deserialize, Serialize};

/// What an attempt changed beyond the paths its task names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeFinding {
    /// The task names at least one path.
    pub stated: bool,
    /// The paths the task names.
    pub named: Vec<String>,
    /// Changed paths outside them.
    pub outside: Vec<String>,
}

/// Paths a task line names: backticked spans, and bare tokens with a `/`
/// or a file extension (`src/lib.rs`, `README.md`, `tests/`).
#[must_use]
pub fn named_paths(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let t = raw
            .trim_matches(|c: char| matches!(c, '(' | ')' | '"' | '\'' | '[' | ']' | '<' | '>'))
            .trim_end_matches(['.', ',', ';', ':', '!', '?']);
        let t = t.strip_prefix("./").unwrap_or(t);
        if looks_like_path(t) && !out.iter().any(|p| p == t) {
            out.push(t.to_owned());
        }
    };
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let Some(len) = rest[start + 1..].find('`') else {
            break;
        };
        push(&rest[start + 1..start + 1 + len]);
        rest = &rest[start + 2 + len..];
    }
    for token in text.split_whitespace().filter(|t| !t.contains('`')) {
        push(token);
    }
    out
}

fn looks_like_path(t: &str) -> bool {
    if t.is_empty() || t.contains("://") || t.contains(' ') || t.starts_with('/') {
        return false;
    }
    if t.contains('/') {
        return t.chars().any(char::is_alphanumeric);
    }
    // `name.ext`: an extension of letters, as files have (`v1.2` is not one).
    t.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=5).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphabetic())
    })
}

/// Changed paths not covered by `named` (exact path, a directory prefix, or
/// a `*` glob), except `exempt` ones (speccoder's own acceptance tests).
#[must_use]
pub fn finding(named: &[String], changed: &[String], exempt: &[String]) -> ScopeFinding {
    let covers = |n: &str, p: &str| {
        let dir = n.trim_end_matches('/');
        p == n || p.starts_with(&format!("{dir}/")) || (n.contains('*') && glob(n, p))
    };
    ScopeFinding {
        stated: !named.is_empty(),
        named: named.to_vec(),
        outside: if named.is_empty() {
            Vec::new()
        } else {
            changed
                .iter()
                .filter(|p| !exempt.contains(p) && !named.iter().any(|n| covers(n, p)))
                .cloned()
                .collect()
        },
    }
}

/// `*` matches within a path segment, `**` across segments.
fn glob(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.split_first() {
            None => s.is_empty(),
            Some((b'*', rest)) if rest.first() == Some(&b'*') => {
                let rest = rest.get(1..).unwrap_or_default();
                let rest = rest.strip_prefix(b"/").unwrap_or(rest);
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            Some((b'*', rest)) => (0..=s.len())
                .take_while(|&i| i == 0 || s[i - 1] != b'/')
                .any(|i| go(rest, &s[i..])),
            Some((c, rest)) => s.first() == Some(c) && go(rest, &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_paths_from_a_task_line() {
        assert_eq!(
            named_paths("[US1] Implement Service in src/services/x.py (depends on T012)"),
            ["src/services/x.py"]
        );
        assert_eq!(
            named_paths("Document `--bye` in README.md, and update `docs/`."),
            ["docs/", "README.md"]
        );
        assert!(named_paths("Bump to v1.2 and say hi").is_empty());
        assert!(named_paths("See https://example.com/x for details").is_empty());
    }

    #[test]
    fn lists_changes_outside_the_named_paths() {
        let named = vec![
            "src/lib.rs".to_owned(),
            "docs/".to_owned(),
            "src/**/*.snap".to_owned(),
        ];
        let changed: Vec<String> = [
            "src/lib.rs",
            "docs/a/b.md",
            "src/util.rs",
            "src/x/y.snap",
            "tests/acceptance_t1.rs",
        ]
        .map(str::to_owned)
        .to_vec();
        let f = finding(&named, &changed, &["tests/acceptance_t1.rs".to_owned()]);
        assert!(f.stated);
        assert_eq!(f.outside, ["src/util.rs"]);
        let none = finding(&[], &changed, &[]);
        assert!(!none.stated && none.outside.is_empty());
    }
}
