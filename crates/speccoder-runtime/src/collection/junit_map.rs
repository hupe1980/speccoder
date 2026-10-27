//! Test ids (a JUnit report's `classname::name`, a listing's lines) mapped
//! back to the file they come from, per toolchain.

use std::collections::BTreeMap;

use speccoder_core::collection::{go_id_matches, python_id_matches, rust_tests_by_file};

/// Where a judged file sits, for matching ids to it.
#[derive(Debug, Clone)]
pub enum Place {
    /// A Rust file: the reachability map of its package.
    Rust(BTreeMap<String, Vec<(String, String)>>),
    /// A pytest file.
    Python,
    /// A JS/TS file, relative to its package root.
    Js(String),
    /// A Go file: its package's import path and the tests it declares.
    Go(String, Vec<String>),
}

/// The ids of `ids` that come from `path`.
#[must_use]
pub fn tests_of(place: &Place, path: &str, ids: &[String]) -> Vec<String> {
    match place {
        Place::Rust(reach) => rust_tests_by_file(reach, ids)
            .remove(path)
            .unwrap_or_default(),
        Place::Python => ids
            .iter()
            .filter(|id| python_id_matches(path, id))
            .cloned()
            .collect(),
        Place::Js(rel) => ids
            .iter()
            .filter(|id| {
                [path, rel.as_str()].iter().any(|p| {
                    *id == p
                        || id.starts_with(&format!("{p}::"))
                        || id.starts_with(&format!("{p} "))
                })
            })
            .cloned()
            .collect(),
        Place::Go(import, names) => ids
            .iter()
            .filter(|id| go_id_matches(import, names, id))
            .cloned()
            .collect(),
    }
}

/// A configured listing's lines that name `path` (as a file, an id
/// prefixed by it, or by the toolchain's own id form).
#[must_use]
pub fn listed(place: &Place, path: &str, lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = lines
        .iter()
        .filter(|l| *l == path || l.starts_with(&format!("{path}::")))
        .cloned()
        .collect();
    if out.is_empty() {
        out = tests_of(place, path, lines);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_map_per_toolchain() {
        let ids = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            tests_of(
                &Place::Js("src/a.test.ts".into()),
                "web/src/a.test.ts",
                &ids(&["src/a.test.ts::greets", "src/b.test.ts::x"])
            ),
            ["src/a.test.ts::greets"]
        );
        assert_eq!(
            tests_of(
                &Place::Go("ex.com/m/g".into(), vec!["TestA".into()]),
                "g/a_test.go",
                &ids(&["ex.com/m/g::TestA", "ex.com/m/g::TestB"])
            ),
            ["ex.com/m/g::TestA"]
        );
        assert_eq!(
            listed(
                &Place::Python,
                "tests/test_a.py",
                &ids(&["tests/test_a.py", "tests/test_b.py"])
            ),
            ["tests/test_a.py"]
        );
    }
}
