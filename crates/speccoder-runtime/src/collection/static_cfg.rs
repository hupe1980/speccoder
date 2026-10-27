//! The collection rules as the project configures them, read from static
//! files only: Cargo manifests, pytest's ini/toml sections, `package.json`
//! and JSON runner configs, `go.mod`. A configuration that is code (a
//! `vitest.config.ts`) is not evaluated: the rule is unknown there and the
//! runner's listing decides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use speccoder_core::collection::{JsRule, JsRunner, PytestRule, RustPackage};

/// The files of the checkout (workspace-relative, `/`-separated), build
/// output and dependency directories left out.
#[must_use]
pub fn files(root: &Path) -> BTreeSet<String> {
    walk(root).0
}

/// The most files walked: beyond it the list is incomplete.
const MAX_FILES: usize = 50_000;

/// [`files`], and whether the walk stopped at its limit (the list is then
/// incomplete: nothing can be proved missing from it).
#[must_use]
pub fn walk(root: &Path) -> (BTreeSet<String>, bool) {
    let mut out = BTreeSet::new();
    let walk = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !matches!(
                n.as_ref(),
                ".git" | "target" | "node_modules" | ".venv" | "vendor"
            )
        })
        .build();
    let mut seen = 0usize;
    let mut truncated = false;
    for e in walk.flatten() {
        seen += 1;
        if seen > MAX_FILES {
            truncated = true;
            break;
        }
        // A symlinked file counts as the file it names.
        if e.path().is_file()
            && let Ok(rel) = e.path().strip_prefix(root)
        {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    (out, truncated)
}

fn read(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}

/// Every Cargo package of the checkout (a manifest with `[package]`).
#[must_use]
pub fn rust_packages(root: &Path, files: &BTreeSet<String>) -> Vec<RustPackage> {
    files
        .iter()
        .filter(|f| *f == "Cargo.toml" || f.ends_with("/Cargo.toml"))
        .filter_map(|f| {
            let table: toml::Table = toml::from_str(&read(root, f)?).ok()?;
            let package = table.get("package")?.as_table()?;
            let dir = f.trim_end_matches("Cargo.toml").trim_end_matches('/');
            let at = |rel: &str| {
                if dir.is_empty() {
                    rel.to_owned()
                } else {
                    format!("{dir}/{rel}")
                }
            };
            Some(RustPackage {
                root: dir.to_owned(),
                name: package.get("name")?.as_str()?.to_owned(),
                autotests: package
                    .get("autotests")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(true),
                tests: table
                    .get("test")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?.to_owned();
                        // Without `path`: `tests/<name>.rs`, else `tests/<name>/main.rs`.
                        let path = t.get("path").and_then(toml::Value::as_str).map_or_else(
                            || {
                                let main = format!("tests/{name}/main.rs");
                                if files.contains(&at(&main)) {
                                    main
                                } else {
                                    format!("tests/{name}.rs")
                                }
                            },
                            str::to_owned,
                        );
                        Some((name, path))
                    })
                    .collect(),
                bins: table
                    .get("bin")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|b| {
                        Some((
                            b.get("name")?.as_str()?.to_owned(),
                            b.get("path")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect(),
                lib: table
                    .get("lib")
                    .and_then(|l| l.get("path"))
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// The package a path belongs to: the one with the longest root above it.
#[must_use]
pub fn owner<'a>(packages: &'a [RustPackage], path: &str) -> Option<&'a RustPackage> {
    packages
        .iter()
        .filter(|p| p.root.is_empty() || path.starts_with(&format!("{}/", p.root)))
        .max_by_key(|p| p.root.len())
}

/// Splits an ini value list (spaces or new lines).
fn words(v: &str) -> Vec<String> {
    v.split_whitespace().map(str::to_owned).collect()
}

/// `key = value` lines of an ini `[section]`, continuation lines joined.
fn ini_section(text: &str, section: &str) -> Option<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut inside = false;
    let mut found = false;
    let mut last: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            inside = &t[1..t.len() - 1] == section;
            found |= inside;
            last = None;
            continue;
        }
        if !inside || t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if line.starts_with(char::is_whitespace)
            && let Some(k) = &last
        {
            if let Some(v) = out.get_mut(k) {
                let v: &mut String = v;
                v.push(' ');
                v.push_str(t);
            }
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let k = k.trim().to_owned();
            out.insert(k.clone(), v.trim().to_owned());
            last = Some(k);
        }
    }
    found.then_some(out)
}

/// Whether the project is a pytest project: a pytest configuration, a
/// `conftest.py`, or a check that runs pytest.
#[must_use]
pub fn uses_pytest(root: &Path, files: &BTreeSet<String>, check_argvs: &[Vec<String>]) -> bool {
    check_argvs
        .iter()
        .any(|a| a.iter().any(|w| w.contains("pytest")))
        || files.contains("pytest.ini")
        || files.iter().any(|f| f.ends_with("conftest.py"))
        || read(root, "pyproject.toml").is_some_and(|t| t.contains("[tool.pytest"))
        || read(root, "setup.cfg").is_some_and(|t| t.contains("[tool:pytest]"))
        || read(root, "tox.ini").is_some_and(|t| t.contains("[pytest]"))
}

/// pytest's rule, from the first configuration file that has a section.
#[must_use]
pub fn pytest_rule(root: &Path) -> PytestRule {
    let mut rule = PytestRule::default();
    let from_ini = |file: &str, section: &str| {
        read(root, file)
            .and_then(|t| ini_section(&t, section))
            .map(|m| (m, format!("{file} [{section}]")))
    };
    let found = from_ini("pytest.ini", "pytest")
        .or_else(|| {
            let text = read(root, "pyproject.toml")?;
            let table: toml::Table = toml::from_str(&text).ok()?;
            let opts = table
                .get("tool")?
                .get("pytest")?
                .get("ini_options")?
                .as_table()?;
            let mut m = BTreeMap::new();
            for (k, v) in opts {
                let s = match v {
                    toml::Value::String(s) => s.clone(),
                    toml::Value::Array(a) => a
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => continue,
                };
                m.insert(k.clone(), s);
            }
            Some((m, "pyproject.toml [tool.pytest.ini_options]".to_owned()))
        })
        .or_else(|| from_ini("tox.ini", "pytest"))
        .or_else(|| from_ini("setup.cfg", "tool:pytest"));
    if let Some((m, source)) = found {
        if let Some(v) = m.get("python_files") {
            rule.python_files = words(v);
        }
        if let Some(v) = m.get("testpaths") {
            rule.testpaths = words(v);
        }
        if let Some(v) = m.get("norecursedirs") {
            rule.norecursedirs = words(v);
        }
        rule.source = source;
    }
    rule
}

/// A JS package: its directory, its runner, and its rule when static.
#[derive(Debug, Clone)]
pub struct JsPackage {
    /// The directory of its `package.json` (`""` at the root).
    pub root: String,
    /// vitest or jest.
    pub runner: JsRunner,
    /// The rule, or why it is not known statically.
    pub rule: Result<JsRule, String>,
}

/// Every `package.json` that uses vitest or jest (by its scripts or
/// dependencies, or a check naming the runner).
#[must_use]
pub fn js_packages(
    root: &Path,
    files: &BTreeSet<String>,
    check_argvs: &[Vec<String>],
) -> Vec<JsPackage> {
    let named = |r: &str| check_argvs.iter().any(|a| a.iter().any(|w| w.contains(r)));
    files
        .iter()
        .filter(|f| *f == "package.json" || f.ends_with("/package.json"))
        .filter_map(|f| {
            let text = read(root, f)?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let dir = f
                .trim_end_matches("package.json")
                .trim_end_matches('/')
                .to_owned();
            let mentions = |r: &str| {
                ["dependencies", "devDependencies"]
                    .iter()
                    .any(|k| json[k].get(r).is_some())
                    || json["scripts"].as_object().is_some_and(|s| {
                        s.values()
                            .any(|v| v.as_str().is_some_and(|v| v.contains(r)))
                    })
                    || json.get(r).is_some()
            };
            let at = |name: &str| {
                if dir.is_empty() {
                    name.to_owned()
                } else {
                    format!("{dir}/{name}")
                }
            };
            let runner = if mentions("vitest") || (named("vitest") && !mentions("jest")) {
                JsRunner::Vitest
            } else if mentions("jest") || named("jest") {
                JsRunner::Jest
            } else {
                return None;
            };
            let rule = match runner {
                JsRunner::Vitest => {
                    let code = ["vitest.config", "vite.config", "vitest.workspace"]
                        .iter()
                        .flat_map(|b| {
                            ["ts", "js", "mjs", "cjs", "mts", "cts"]
                                .map(|e| at(&format!("{b}.{e}")))
                        })
                        .find(|c| files.contains(c));
                    match code {
                        Some(c) => Err(format!("`{c}` is code: its include is not read")),
                        None => Ok(JsRule {
                            runner,
                            include: runner.defaults(),
                            exclude: Vec::new(),
                            source: "vitest (default)".to_owned(),
                        }),
                    }
                }
                JsRunner::Jest => {
                    let code = ["ts", "js", "mjs", "cjs"]
                        .map(|e| at(&format!("jest.config.{e}")))
                        .into_iter()
                        .find(|c| files.contains(c));
                    let (cfg, source) = if let Some(c) = code {
                        return Some(JsPackage {
                            root: dir,
                            runner,
                            rule: Err(format!("`{c}` is code: its testMatch is not read")),
                        });
                    } else if let Some(j) = read(root, &at("jest.config.json"))
                        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    {
                        (j, at("jest.config.json"))
                    } else {
                        (json["jest"].clone(), format!("{f} jest"))
                    };
                    if cfg.get("testRegex").is_some() {
                        Err(format!(
                            "{source} sets testRegex: speccoder reads testMatch only"
                        ))
                    } else {
                        let list = |k: &str| {
                            cfg[k].as_array().map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_owned))
                                    .collect::<Vec<_>>()
                            })
                        };
                        let include = list("testMatch");
                        let roots = list("roots").unwrap_or_default();
                        let mut include = include.clone().unwrap_or_else(|| runner.defaults());
                        if !roots.is_empty() {
                            // `<rootDir>/src` roots narrow every pattern to them.
                            let roots: Vec<String> = roots
                                .iter()
                                .map(|r| {
                                    r.trim_start_matches("<rootDir>")
                                        .trim_matches('/')
                                        .to_owned()
                                })
                                .collect();
                            include = roots
                                .iter()
                                .flat_map(|r| {
                                    include.iter().map(move |p| {
                                        let p = p
                                            .trim_start_matches("<rootDir>/")
                                            .trim_start_matches("**/");
                                        if r.is_empty() {
                                            format!("**/{p}")
                                        } else {
                                            format!("{r}/**/{p}")
                                        }
                                    })
                                })
                                .collect();
                        }
                        let ignore = list("testPathIgnorePatterns").unwrap_or_default();
                        Ok(JsRule {
                            runner,
                            include,
                            exclude: ignore
                                .iter()
                                .map(|p| format!("**/{}/**", p.trim_matches('/')))
                                .collect(),
                            source: if cfg.get("testMatch").is_some() {
                                source
                            } else {
                                "jest (default)".to_owned()
                            },
                        })
                    }
                }
            };
            Some(JsPackage {
                root: dir,
                runner,
                rule,
            })
        })
        .collect()
}

/// The JS package a path belongs to.
#[must_use]
pub fn js_owner<'a>(packages: &'a [JsPackage], path: &str) -> Option<&'a JsPackage> {
    packages
        .iter()
        .filter(|p| p.root.is_empty() || path.starts_with(&format!("{}/", p.root)))
        .max_by_key(|p| p.root.len())
}

/// A Go module: its directory and module path.
#[derive(Debug, Clone)]
pub struct GoModule {
    /// The directory of its `go.mod`.
    pub root: String,
    /// `module …`.
    pub path: String,
}

/// Every `go.mod` of the checkout.
#[must_use]
pub fn go_modules(root: &Path, files: &BTreeSet<String>) -> Vec<GoModule> {
    files
        .iter()
        .filter(|f| *f == "go.mod" || f.ends_with("/go.mod"))
        .filter_map(|f| {
            let text = read(root, f)?;
            let path = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("module "))?
                .trim()
                .trim_matches('"')
                .to_owned();
            Some(GoModule {
                root: f
                    .trim_end_matches("go.mod")
                    .trim_end_matches('/')
                    .to_owned(),
                path,
            })
        })
        .collect()
}

/// The build tags a check sets (`-tags a,b` / `-tags=a,b`).
#[must_use]
pub fn go_tags(check_argvs: &[Vec<String>]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for a in check_argvs {
        for (i, w) in a.iter().enumerate() {
            let v = w
                .strip_prefix("-tags=")
                .or_else(|| w.strip_prefix("--tags="))
                .map(str::to_owned)
                .or_else(|| {
                    (w == "-tags" || w == "--tags")
                        .then(|| a.get(i + 1).cloned())
                        .flatten()
                });
            for t in v.iter().flat_map(|v| v.split([',', ' '])) {
                if !t.is_empty() {
                    out.insert(t.to_owned());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_sections_join_continuation_lines() {
        let m = ini_section(
            "[other]\nx = 1\n[pytest]\npython_files = check_*.py\n    test_*.py\ntestpaths = tests\n",
            "pytest",
        )
        .unwrap();
        assert_eq!(words(&m["python_files"]), ["check_*.py", "test_*.py"]);
        assert_eq!(m["testpaths"], "tests");
        assert!(ini_section("[a]\n", "pytest").is_none());
    }

    #[test]
    fn go_tags_are_read_in_both_forms() {
        let t = go_tags(&[
            vec!["go".into(), "test".into(), "-tags".into(), "a,b".into()],
            vec!["go".into(), "test".into(), "-tags=c".into()],
        ]);
        assert_eq!(t.into_iter().collect::<Vec<_>>(), ["a", "b", "c"]);
    }
}
