//! `--test PATH::NAME`: the person's own reproduction. It must exist, be
//! collected and fail on the untouched code by an assertion, twice — checked
//! before anything is written. Naming it is the confirmation.

use std::path::Path;

use speccoder_core::bug::bug_class;
use speccoder_core::verify::{RedClass, classify_red};

use crate::format::BugRefusal;

/// A named test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// Project-relative file.
    pub file: String,
    /// The test's name.
    pub name: String,
    /// The command that runs only it.
    pub argv: Vec<String>,
    /// The file's content.
    pub content: String,
}

/// `PATH::NAME` split at its last `::`.
#[must_use]
pub fn split(spec: &str) -> Option<(String, String)> {
    let (file, name) = spec.rsplit_once("::")?;
    let file = file.trim().trim_start_matches("./");
    let name = name.trim();
    (!file.is_empty() && !name.is_empty()).then(|| (file.to_owned(), name.to_owned()))
}

/// The command that runs only `name` in `file`: `template` (`[bug] test`,
/// `{file}` and `{name}` filled) or the toolchain's own runner.
#[must_use]
pub fn argv(
    root: &Path,
    file: &str,
    name: &str,
    template: Option<&[String]>,
) -> Option<Vec<String>> {
    if let Some(t) = template {
        return Some(
            t.iter()
                .map(|a| a.replace("{file}", file).replace("{name}", name))
                .collect(),
        );
    }
    let p = Path::new(file);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match ext {
        "rs" if root.join("Cargo.toml").is_file() => {
            Some(if file.starts_with("tests/") && !file[6..].contains('/') {
                words(&["cargo", "test", "--test", stem, "--", "--exact", name])
            } else {
                words(&["cargo", "test", "--lib", "--", name])
            })
        }
        "py" => Some(vec![
            "python".to_owned(),
            "-m".to_owned(),
            "pytest".to_owned(),
            format!("{file}::{name}"),
        ]),
        "ts" | "js" | "tsx" | "jsx" | "mjs" | "cjs" if root.join("package.json").is_file() => {
            Some(words(&["npx", "vitest", "run", file, "-t", name]))
        }
        "go" => {
            let dir = p
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            Some(vec![
                "go".to_owned(),
                "test".to_owned(),
                if dir.is_empty() {
                    "./".to_owned()
                } else {
                    format!("./{dir}")
                },
                "-run".to_owned(),
                format!("^{name}$"),
            ])
        }
        _ => None,
    }
}

/// Resolves `spec` in the project at `root` (the file and the test name
/// must be there) without running it.
///
/// # Errors
///
/// `BUG-TEST-NOT-FOUND`.
pub fn resolve(root: &Path, spec: &str, template: Option<&[String]>) -> Result<Named, BugRefusal> {
    let not_found = |why: String| BugRefusal::new("BUG-TEST-NOT-FOUND", why);
    let (file, name) =
        split(spec).ok_or_else(|| not_found(format!("`{spec}` is not PATH::NAME")))?;
    if file.contains("..") || Path::new(&file).is_absolute() {
        return Err(not_found(format!("`{file}` is not a path in the project")));
    }
    let content = std::fs::read_to_string(root.join(&file))
        .map_err(|_| not_found(format!("there is no test file `{file}`")))?;
    if !content.contains(name.rsplit("::").next().unwrap_or(&name)) {
        return Err(not_found(format!("`{file}` has no test `{name}`")));
    }
    let argv = argv(root, &file, &name, template).ok_or_else(|| {
        not_found(format!(
            "speccoder has no rule to run one test of `{file}`; set `[bug] test` in speccoder.toml"
        ))
    })?;
    Ok(Named {
        file,
        name,
        argv,
        content,
    })
}

/// Its class on the untouched code: run twice, red only by assertion.
#[must_use]
pub fn baseline(
    root: &Path,
    argv: &[String],
    runner: &crate::sandbox::Runner,
) -> (RedClass, String) {
    let (first, summary) = crate::pipeline::acceptance_run(root, argv, runner);
    let one = classify_red(&first, summary.as_ref());
    let excerpt = format!("{}\n{}", first.stdout_tail, first.stderr_tail);
    if one != RedClass::Assertion {
        return (one, excerpt);
    }
    let (second, summary) = crate::pipeline::acceptance_run(root, argv, runner);
    (
        bug_class(one, Some(classify_red(&second, summary.as_ref()))),
        excerpt,
    )
}

/// The refusal for a named test that is not red.
#[must_use]
pub fn not_red(spec: &str, class: RedClass) -> BugRefusal {
    if class == RedClass::Passed {
        return BugRefusal::new(
            "BUG-TEST-PASSES",
            format!(
                "`{spec}` passes on the untouched code: it cannot show the bug (perhaps it is fixed already)"
            ),
        );
    }
    if class == RedClass::NoTests {
        return BugRefusal::new(
            "BUG-TEST-NOT-FOUND",
            format!("`{spec}` ran no test: the runner did not collect it"),
        );
    }
    BugRefusal::new(
        "BUG-TEST-NOT-RED",
        format!(
            "`{spec}` does not fail on an assertion ({}): a reproduction must",
            crate::pipeline::class_word(class)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_test_runs_by_its_toolchain_or_the_configured_template() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let r = dir.path();
        std::fs::write(r.join("Cargo.toml"), "[package]\nname = \"t\"\n")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            argv(r, "tests/title.rs", "empty_title_is_refused", None),
            Some(
                [
                    "cargo",
                    "test",
                    "--test",
                    "title",
                    "--",
                    "--exact",
                    "empty_title_is_refused"
                ]
                .map(String::from)
                .to_vec()
            )
        );
        assert_eq!(
            argv(r, "pkg/test_title.py", "test_empty", None).map(|a| a[3].clone()),
            Some("pkg/test_title.py::test_empty".to_owned())
        );
        assert_eq!(
            argv(r, "title/title_test.go", "TestEmpty", None),
            Some(
                ["go", "test", "./title", "-run", "^TestEmpty$"]
                    .map(String::from)
                    .to_vec()
            )
        );
        let t = [
            "sh".to_owned(),
            "t.sh".to_owned(),
            "{file}".to_owned(),
            "{name}".to_owned(),
        ];
        assert_eq!(
            argv(r, "tests/a.sh", "b", Some(&t)),
            Some(["sh", "t.sh", "tests/a.sh", "b"].map(String::from).to_vec())
        );
        assert_eq!(argv(r, "tests/a.sh", "b", None), None);
        assert_eq!(
            split("tests/title.rs::empty"),
            Some(("tests/title.rs".to_owned(), "empty".to_owned()))
        );
        assert_eq!(split("tests/title.rs"), None);
        assert_eq!(
            resolve(r, "tests/none.rs::x", None).err().map(|e| e.code),
            Some("BUG-TEST-NOT-FOUND")
        );
        assert_eq!(not_red("x", RedClass::Passed).code, "BUG-TEST-PASSES");
        assert_eq!(not_red("x", RedClass::BuildError).code, "BUG-TEST-NOT-RED");
    }
}
