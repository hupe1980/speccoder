//! The pass-to-pass set, chosen deterministically once the attempt's diff
//! is known: baseline-green tests in test files that reference a changed
//! source file (per toolchain), plus each *Unchanged* statement's test. No
//! model decides what counts as "nearby" (principle IV).

use speccoder_core::bug::{Member, MemberFinal, PassToPass, PassToPassState};
use speccoder_core::verify::{TestOutcome, TestResult};

/// Why `test_file` (its `content`) is selected for `changed`, if it is.
#[must_use]
pub fn rule(
    changed: &str,
    test_file: &str,
    content: &str,
    crate_name: Option<&str>,
) -> Option<String> {
    let path = std::path::Path::new(changed);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if test_file == changed {
        return content
            .contains("#[cfg(test)]")
            .then(|| "same_file".to_owned());
    }
    match ext {
        "rs" => {
            let rel = changed.strip_prefix("src/")?;
            let module: Vec<&str> = rel
                .trim_end_matches(".rs")
                .split('/')
                .filter(|m| !matches!(*m, "lib" | "main" | "mod"))
                .collect();
            let name = crate_name.map(|c| c.replace('-', "_"));
            let path = module.join("::");
            let named = |root: &str| {
                if path.is_empty() {
                    format!("{root}::")
                } else {
                    format!("{root}::{path}")
                }
            };
            let hit = name.as_deref().is_some_and(|n| content.contains(&named(n)))
                || (test_file.starts_with("src/") && content.contains(&named("crate")));
            hit.then(|| format!("references:crate::{path}"))
        }
        "py" => {
            let module = changed.trim_end_matches(".py").replace('/', ".");
            let module = module.trim_end_matches(".__init__");
            let (parent, leaf) = module.rsplit_once('.').unwrap_or(("", module));
            let hit = content.lines().any(|l| {
                let l = l.trim();
                l.starts_with(&format!("import {module}"))
                    || l.starts_with(&format!("from {module} import"))
                    || (!parent.is_empty()
                        && l.starts_with(&format!("from {parent} import"))
                        && l.split(['(', ',', ' ']).any(|w| w.trim() == leaf))
            });
            hit.then(|| format!("imports:{module}"))
        }
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            let target = changed.rsplit_once('.').map_or(changed, |(a, _)| a);
            let base = std::path::Path::new(test_file)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let hit = specifiers(content).iter().any(|s| {
                s.starts_with('.')
                    && normalise(&format!("{base}/{s}"))
                        .trim_end_matches(".js")
                        .trim_end_matches(".ts")
                        == target
            });
            hit.then(|| format!("imports:{changed}"))
        }
        "go" => {
            let dir = path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let tdir = std::path::Path::new(test_file)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            (test_file.ends_with("_test.go") && dir == tdir).then(|| format!("package:{dir}"))
        }
        _ => None,
    }
}

/// The module specifiers a JS/TS file imports.
fn specifiers(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in content.lines() {
        for q in ['"', '\''] {
            if (l.contains("import") || l.contains("require("))
                && let Some(i) = l
                    .find(" from ")
                    .map(|i| i + 6)
                    .or_else(|| l.find("require(").map(|i| i + 8))
            {
                let rest = l[i..].trim_start();
                if let Some(r) = rest.strip_prefix(q)
                    && let Some(end) = r.find(q)
                {
                    out.push(r[..end].to_owned());
                }
            }
        }
    }
    out
}

/// `a/./b/../c` → `a/c`.
fn normalise(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Whether a runner's test id is a test of `file`.
#[must_use]
pub fn belongs(id: &str, file: &str) -> bool {
    let stem = std::path::Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if stem.is_empty() {
        return false;
    }
    let dotted = file
        .rsplit_once('.')
        .map_or(file, |(a, _)| a)
        .replace('/', ".");
    id.contains(file)
        || id.contains(&dotted)
        || id.split([':', '.', '/', ' ']).any(|seg| seg == stem)
}

/// The members for `changed` source files among `tests` (path and content),
/// those green on the baseline run (`baseline`), plus `unchanged` (each
/// `U<n>`'s test file and name).
#[must_use]
pub fn select(
    changed: &[String],
    tests: &[(String, String)],
    baseline: &[TestResult],
    crate_name: Option<&str>,
    unchanged: &[(String, String, String)],
) -> PassToPass {
    let mut members: Vec<Member> = Vec::new();
    for (file, content) in tests {
        let Some(why) = changed
            .iter()
            .find_map(|c| rule(c, file, content, crate_name))
        else {
            continue;
        };
        for t in baseline
            .iter()
            .filter(|t| t.outcome == TestOutcome::Passed && belongs(&t.id, file))
        {
            if members.iter().any(|m| m.test == t.id) {
                continue;
            }
            members.push(Member {
                test: t.id.clone(),
                file: file.clone(),
                rule: why.clone(),
                final_: MemberFinal::NotRun,
            });
        }
    }
    for (statement, file, test) in unchanged {
        members.push(Member {
            test: test.clone(),
            file: file.clone(),
            rule: format!("unchanged:{statement}"),
            final_: MemberFinal::NotRun,
        });
    }
    PassToPass {
        state: if members.is_empty() {
            PassToPassState::NoneSelected
        } else {
            PassToPassState::Selected
        },
        members,
    }
}

/// Each selected member's result in the final run (by name; one the run
/// does not name is missing). *Unchanged* members are settled by their own
/// runs (`unchanged_passed`).
pub fn settle(
    p2p: &mut PassToPass,
    finals: &[TestResult],
    unchanged_passed: &dyn Fn(&str) -> bool,
) {
    for m in &mut p2p.members {
        if let Some(statement) = m.rule.strip_prefix("unchanged:") {
            m.final_ = if unchanged_passed(statement) {
                MemberFinal::Passed
            } else {
                MemberFinal::Failed
            };
            continue;
        }
        m.final_ = match finals.iter().find(|t| t.id == m.test).map(|t| t.outcome) {
            Some(TestOutcome::Passed) => MemberFinal::Passed,
            Some(TestOutcome::Failed | TestOutcome::Error | TestOutcome::Skipped) => {
                MemberFinal::Failed
            }
            None => MemberFinal::Missing,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passed(id: &str) -> TestResult {
        TestResult {
            id: id.to_owned(),
            outcome: TestOutcome::Passed,
        }
    }

    #[test]
    fn rust_tests_naming_the_changed_module_are_selected() {
        assert_eq!(
            rule(
                "src/title.rs",
                "tests/save.rs",
                "use notes::title::save;",
                Some("notes")
            ),
            Some("references:crate::title".to_owned())
        );
        assert_eq!(
            rule(
                "src/title.rs",
                "tests/other.rs",
                "use notes::body;",
                Some("notes")
            ),
            None
        );
        assert_eq!(
            rule(
                "src/title.rs",
                "src/title.rs",
                "#[cfg(test)]\nmod tests {}",
                Some("notes")
            ),
            Some("same_file".to_owned())
        );
        assert_eq!(
            rule(
                "src/a/mod.rs",
                "tests/a.rs",
                "use my_crate::a::f;",
                Some("my-crate")
            ),
            Some("references:crate::a".to_owned())
        );
    }

    #[test]
    fn python_js_and_go_rules() {
        assert_eq!(
            rule(
                "app/title.py",
                "tests/test_title.py",
                "from app.title import save\n",
                None
            ),
            Some("imports:app.title".to_owned())
        );
        assert_eq!(
            rule(
                "app/title.py",
                "tests/test_x.py",
                "from app import body, title\n",
                None
            ),
            Some("imports:app.title".to_owned())
        );
        assert_eq!(
            rule(
                "src/title.ts",
                "src/title.test.ts",
                "import { save } from './title';\n",
                None
            ),
            Some("imports:src/title.ts".to_owned())
        );
        assert_eq!(
            rule(
                "src/title.ts",
                "test/title.test.ts",
                "import { save } from \"../src/title.js\";\n",
                None
            ),
            Some("imports:src/title.ts".to_owned())
        );
        assert_eq!(
            rule(
                "title/title.go",
                "title/title_test.go",
                "package title",
                None
            ),
            Some("package:title".to_owned())
        );
        assert_eq!(rule("title/title.go", "other/x_test.go", "", None), None);
    }

    #[test]
    fn only_baseline_green_tests_of_selected_files_and_none_is_recorded_as_none() {
        let tests = vec![(
            "tests/save.rs".to_owned(),
            "use notes::title::save;".to_owned(),
        )];
        let baseline = vec![
            passed("save::saves_a_title"),
            TestResult {
                id: "save::broken".to_owned(),
                outcome: TestOutcome::Failed,
            },
            passed("other::x"),
        ];
        let mut p = select(
            &["src/title.rs".to_owned()],
            &tests,
            &baseline,
            Some("notes"),
            &[],
        );
        assert_eq!(p.state, PassToPassState::Selected);
        assert_eq!(p.members.len(), 1);
        settle(&mut p, &[], &|_| true);
        assert_eq!(
            p.members[0].final_,
            MemberFinal::Missing,
            "a missing test fails"
        );
        let none = select(
            &["src/title.rs".to_owned()],
            &[],
            &baseline,
            Some("notes"),
            &[],
        );
        assert_eq!(none.state, PassToPassState::NoneSelected);
        let with_u = select(
            &[],
            &[],
            &[],
            None,
            &[(
                "U1".to_owned(),
                "tests/u1.rs".to_owned(),
                "keeps".to_owned(),
            )],
        );
        assert_eq!(with_u.members[0].rule, "unchanged:U1");
        assert!(belongs("tests.test_title.test_x", "tests/test_title.py"));
        assert!(belongs("save::saves", "tests/save.rs"));
    }
}
