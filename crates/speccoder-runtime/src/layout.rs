//! The project's test layout, detected by code: the runners of
//! the checkout, each runner's collection rule as configured, where tests
//! live today, the naming habit, and where speccoder puts a story's
//! acceptance file. A fact handed to the implementer and every test author,
//! never a judge input; undeterminable parts are listed as unknown, never
//! guessed. Same checkout, same bytes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use speccoder_core::collection::{
    CollectionRule, JsRunner, Toolchain, defines_tests, go_rule, rust_reachable, rust_rule,
    rust_targets, strip_comments, toolchain_of,
};

use crate::collection::static_cfg;

/// Where tests live today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    /// `inline_modules`, `tests_dir`, `beside_sources`, `package`.
    pub kind: String,
    /// The directory (or `*` for inline modules across the sources).
    pub path: String,
    /// How many test files.
    pub files: usize,
}

/// One runner's layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunnerLayout {
    /// The toolchain.
    pub toolchain: Toolchain,
    /// The member or package root (`""` at the checkout's root).
    pub root: String,
    /// Its collection rule as configured.
    pub rule: CollectionRule,
    /// Where tests live today.
    pub locations: Vec<Location>,
    /// Test files present that nothing runs.
    pub not_run: Vec<String>,
    /// The project's naming habit.
    pub naming: Option<String>,
    /// Where speccoder puts story files; `None`: no place the runner collects.
    pub story_dir: Option<String>,
    /// The file extension story files take (JS/TS: the project's majority).
    pub extension: String,
    /// What could not be determined.
    pub unknown: Vec<String>,
}

/// The checkout's test layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestLayout {
    /// Every runner found, in toolchain order.
    pub runners: Vec<RunnerLayout>,
    /// SHA-256 of the runners' canonical JSON.
    pub digest: String,
}

fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn at(root: &str, rel: &str) -> String {
    if root.is_empty() {
        rel.to_owned()
    } else {
        format!("{root}/{rel}")
    }
}

/// Detects the layout of the checkout at `root`; `check_argvs` are the
/// checks' commands (they name the runners).
#[must_use]
pub fn detect(root: &Path, check_argvs: &[Vec<String>]) -> TestLayout {
    let files = static_cfg::files(root);
    let read = |p: &str| std::fs::read_to_string(root.join(p)).ok();
    let tests_in = |pred: &dyn Fn(&str) -> bool| -> Vec<&String> {
        files
            .iter()
            .filter(|f| pred(f))
            .filter(|f| {
                toolchain_of(f).is_some_and(|t| read(f).is_some_and(|s| defines_tests(t, &s)))
            })
            .collect()
    };
    let mut runners = Vec::new();

    for pkg in static_cfg::rust_packages(root, &files) {
        let targets = rust_targets(&pkg, &files);
        let reach = rust_reachable(&targets, &|p| read(p));
        let inside = |f: &str| {
            f.ends_with(".rs")
                && static_cfg::owner(std::slice::from_ref(&pkg), f).is_some()
                && !f.starts_with(&at(&pkg.root, "target/"))
        };
        let tests_dir = at(&pkg.root, "tests");
        let inline = tests_in(&|f| {
            inside(f)
                && !f.starts_with(&format!("{tests_dir}/"))
                && read(f).is_some_and(|s| strip_comments(&s).contains("#[cfg(test)]"))
        });
        let top = tests_in(&|f| {
            inside(f)
                && f.strip_prefix(&format!("{tests_dir}/"))
                    .is_some_and(|r| !r.contains('/'))
        });
        let not_run: Vec<String> =
            tests_in(&|f| inside(f) && f.starts_with(&format!("{tests_dir}/")))
                .into_iter()
                .filter(|f| !reach.contains_key(*f))
                .cloned()
                .collect();
        let mut locations = vec![
            Location {
                kind: "inline_modules".to_owned(),
                path: at(&pkg.root, "src"),
                files: inline.len(),
            },
            Location {
                kind: "tests_dir".to_owned(),
                path: format!("{tests_dir}/*.rs"),
                files: top.len(),
            },
        ];
        let mains = files
            .iter()
            .filter(|f| {
                f.strip_prefix(&format!("{tests_dir}/"))
                    .is_some_and(|r| r.matches('/').count() == 1 && r.ends_with("/main.rs"))
            })
            .count();
        if mains > 0 {
            locations.push(Location {
                kind: "tests_dir".to_owned(),
                path: format!("{tests_dir}/<dir>/main.rs"),
                files: mains,
            });
        }
        let mut unknown = Vec::new();
        let story_dir = if pkg.autotests {
            Some(tests_dir.clone())
        } else {
            unknown.push(format!(
                "{}: `autotests = false` — a new integration test needs a `[[test]]` entry, which speccoder does not add",
                at(&pkg.root, "Cargo.toml")
            ));
            None
        };
        runners.push(RunnerLayout {
            toolchain: Toolchain::Rust,
            root: pkg.root.clone(),
            rule: rust_rule(&pkg),
            locations,
            not_run,
            naming: Some(format!("{tests_dir}/<area>.rs")),
            story_dir,
            extension: "rs".to_owned(),
            unknown,
        });
    }

    if static_cfg::uses_pytest(root, &files, check_argvs) {
        let rule = static_cfg::pytest_rule(root);
        let py = tests_in(&|f| f.ends_with(".py"));
        let mut by_dir: BTreeMap<String, usize> = BTreeMap::new();
        for f in &py {
            *by_dir.entry(parent(f).to_owned()).or_default() += 1;
        }
        let exists = |d: &str| d.is_empty() || root.join(d).is_dir();
        let mut unknown = Vec::new();
        let story_dir = match rule.testpaths.first() {
            Some(t) if exists(t.trim_end_matches('/')) => Some(t.trim_end_matches('/').to_owned()),
            Some(t) => {
                unknown.push(format!(
                    "testpaths names `{t}`, which does not exist: no place pytest collects"
                ));
                None
            }
            None if exists("tests") => Some("tests".to_owned()),
            None => Some(String::new()),
        };
        let prefix = py
            .iter()
            .filter(|f| f.rsplit('/').next().is_some_and(|n| n.starts_with("test_")))
            .count();
        runners.push(RunnerLayout {
            toolchain: Toolchain::Python,
            root: String::new(),
            rule: rule.rule(),
            locations: by_dir
                .into_iter()
                .map(|(path, files)| Location {
                    kind: "tests_dir".to_owned(),
                    path,
                    files,
                })
                .collect(),
            not_run: py
                .iter()
                .filter(|f| rule.breaks(f).is_some())
                .map(|f| (*f).clone())
                .collect(),
            naming: Some(if prefix * 2 >= py.len() {
                "test_<name>.py".to_owned()
            } else {
                "<name>_test.py".to_owned()
            }),
            story_dir,
            extension: "py".to_owned(),
            unknown,
        });
    }

    for pkg in static_cfg::js_packages(root, &files, check_argvs) {
        let inside = |f: &str| pkg.root.is_empty() || f.starts_with(&format!("{}/", pkg.root));
        let js = tests_in(&|f| inside(f) && toolchain_of(f) == Some(Toolchain::JavaScript));
        let rel = |f: &str| {
            if pkg.root.is_empty() {
                f.to_owned()
            } else {
                f.trim_start_matches(&format!("{}/", pkg.root)).to_owned()
            }
        };
        let collected: Vec<&String> = js
            .iter()
            .copied()
            .filter(|f| {
                pkg.rule
                    .as_ref()
                    .map_or(true, |r| r.breaks(&rel(f)).is_none())
            })
            .collect();
        let mut by_dir: BTreeMap<String, usize> = BTreeMap::new();
        for f in &collected {
            *by_dir.entry(parent(f).to_owned()).or_default() += 1;
        }
        let ts = collected.iter().filter(|f| f.contains(".ts")).count();
        let extension = if collected.is_empty() {
            if files.iter().any(|f| inside(f) && f.ends_with(".ts")) {
                "ts"
            } else {
                "js"
            }
        } else if ts * 2 >= collected.len() {
            "ts"
        } else {
            "js"
        }
        .to_owned();
        let habit = by_dir
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(d, _)| d.clone());
        let src = at(&pkg.root, "src");
        let story_dir = habit.clone().or_else(|| {
            Some(if root.join(&src).is_dir() {
                src.clone()
            } else {
                pkg.root.clone()
            })
        });
        let mut unknown = Vec::new();
        let rule = match &pkg.rule {
            Ok(r) => r.rule(),
            Err(why) => {
                unknown.push(why.clone());
                CollectionRule {
                    toolchain: Toolchain::JavaScript,
                    text: format!(
                        "{} collects the files its configuration includes (not read: {why})",
                        pkg.runner.name()
                    ),
                    source: at(&pkg.root, "package.json"),
                }
            }
        };
        runners.push(RunnerLayout {
            toolchain: Toolchain::JavaScript,
            root: pkg.root.clone(),
            rule,
            locations: by_dir
                .into_iter()
                .map(|(path, files)| Location {
                    kind: if path.contains("__tests__") {
                        "tests_dir"
                    } else {
                        "beside_sources"
                    }
                    .to_owned(),
                    path,
                    files,
                })
                .collect(),
            not_run: js
                .iter()
                .filter(|f| !collected.contains(f))
                .map(|f| (*f).clone())
                .collect(),
            naming: Some(match pkg.runner {
                JsRunner::Vitest | JsRunner::Jest => format!("<name>.test.{extension}"),
            }),
            story_dir,
            extension,
            unknown,
        });
    }

    for m in static_cfg::go_modules(root, &files) {
        let inside = |f: &str| m.root.is_empty() || f.starts_with(&format!("{}/", m.root));
        let go = tests_in(&|f| inside(f) && f.ends_with("_test.go"));
        let mut by_dir: BTreeMap<String, usize> = BTreeMap::new();
        for f in &go {
            *by_dir.entry(parent(f).to_owned()).or_default() += 1;
        }
        let tags = static_cfg::go_tags(check_argvs);
        runners.push(RunnerLayout {
            toolchain: Toolchain::Go,
            root: m.root.clone(),
            rule: go_rule(),
            locations: by_dir
                .into_iter()
                .map(|(path, files)| Location {
                    kind: "package".to_owned(),
                    path,
                    files,
                })
                .collect(),
            not_run: go
                .iter()
                .filter(|f| {
                    speccoder_core::collection::go_breaks(f, &read(f).unwrap_or_default(), &tags)
                        .is_some()
                })
                .map(|f| (*f).clone())
                .collect(),
            naming: Some("<name>_test.go beside the package's sources".to_owned()),
            story_dir: Some(m.root.clone()),
            extension: "go".to_owned(),
            unknown: Vec::new(),
        });
    }

    let canonical = serde_json::to_string(&runners).unwrap_or_default();
    TestLayout {
        digest: crate::integrity::sha256_hex(canonical.as_bytes()),
        runners,
    }
}

/// The layout's text for the project at `root`, its checks read from its
/// `speccoder.toml`.
#[must_use]
pub fn text_at(root: &Path) -> String {
    detect(root, &crate::verify::check_argvs(root)).text()
}

impl TestLayout {
    /// The runner speccoder's acceptance tests go to: the first with a
    /// collected place, else the first at all.
    #[must_use]
    pub fn primary(&self) -> Option<&RunnerLayout> {
        self.runners
            .iter()
            .find(|r| r.story_dir.is_some())
            .or_else(|| self.runners.first())
    }

    /// The story file for `story` (`US1`) — for Go, in `package` (a
    /// directory). `None`: no place the runner collects.
    #[must_use]
    pub fn story_file(&self, story: &str, package: Option<&str>) -> Option<String> {
        let r = self.primary()?;
        let dir = r.story_dir.as_ref()?;
        let s = story.to_ascii_lowercase();
        Some(match r.toolchain {
            Toolchain::Rust => at(dir, &format!("acceptance_{s}.rs")),
            Toolchain::Python => at(dir, &format!("test_acceptance_{s}.py")),
            Toolchain::JavaScript => at(dir, &format!("acceptance-{s}.test.{}", r.extension)),
            Toolchain::Go => at(package.unwrap_or(dir), &format!("acceptance_{s}_test.go")),
        })
    }

    /// The command that runs only `file`, for the author's prompt.
    #[must_use]
    pub fn command_for(&self, file: &str) -> Option<String> {
        let r = self.primary()?;
        let stem = file.rsplit('/').next()?.split('.').next()?.to_owned();
        Some(match r.toolchain {
            Toolchain::Rust => format!("cargo test --test {stem}"),
            Toolchain::Python => format!("python3 -m pytest {file}"),
            Toolchain::JavaScript => format!("npx vitest run {file}"),
            Toolchain::Go => format!("go test ./{}", parent(file)),
        })
    }

    /// Whether `argv` is the command that runs only one of `files` — the
    /// one the author's prompt names, which an exact `[commands] allow`
    /// (`cargo test`) would otherwise refuse.
    #[must_use]
    pub fn runs_only(&self, argv: &[String], files: &[&str]) -> bool {
        files.iter().any(|f| {
            self.command_for(f)
                .is_some_and(|c| c.split_whitespace().eq(argv.iter().map(String::as_str)))
        })
    }

    /// The part a model reads (contracts/prompts.md).
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::from("# Test layout (detected by speccoder from this checkout)\n");
        if self.runners.is_empty() {
            out.push_str("\nNo test runner speccoder knows (Cargo, pytest, vitest/jest, go test) was found.\n");
        }
        for r in &self.runners {
            let name = match r.toolchain {
                Toolchain::Rust => "cargo (Rust)",
                Toolchain::Python => "pytest (Python)",
                Toolchain::JavaScript => "vitest/jest (JavaScript/TypeScript)",
                Toolchain::Go => "go test (Go)",
            };
            out.push_str(&format!(
                "\nRunner: {name}{}\nRule: {} — from {}\n",
                if r.root.is_empty() {
                    String::new()
                } else {
                    format!(", in `{}`", r.root)
                },
                r.rule.text,
                r.rule.source
            ));
            let today: Vec<String> = r
                .locations
                .iter()
                .map(|l| match l.kind.as_str() {
                    "inline_modules" => format!(
                        "inline `#[cfg(test)]` modules in {} file(s) under {}",
                        l.files, l.path
                    ),
                    _ => format!("{}: {} file(s)", l.path, l.files),
                })
                .collect();
            if !today.is_empty() {
                out.push_str(&format!("Tests today: {}\n", today.join("; ")));
            }
            if !r.not_run.is_empty() {
                out.push_str(&format!("Not run by anything: {}\n", r.not_run.join(", ")));
            }
            if let Some(n) = &r.naming {
                out.push_str(&format!("Naming: {n}\n"));
            }
            for u in &r.unknown {
                out.push_str(&format!("Not determined: {u}\n"));
            }
        }
        out.push_str(
            "\nPut new tests where the runner collects them. A path the task names that the runner would not\ncollect is a mistake in the plan — use the nearest collected place and say so in your report.\nA test file that never runs fails the verdict.\n",
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (p, t) in files {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap_or(root)).ok();
            std::fs::write(f, t).ok();
        }
    }

    #[test]
    fn a_rust_crate_s_layout_names_its_rule_its_tests_and_its_dead_files() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            d.path(),
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"greet\"\nversion = \"0.1.0\"\n",
                ),
                (
                    "src/lib.rs",
                    "pub fn greet() {}\n#[cfg(test)]\nmod tests { #[test] fn t() {} }\n",
                ),
                ("tests/unit/greet_name.rs", "#[test]\nfn a() {}\n"),
            ],
        );
        let l = detect(d.path(), &[]);
        assert_eq!(
            l.story_file("US1", None).as_deref(),
            Some("tests/acceptance_us1.rs")
        );
        assert_eq!(
            l.command_for("tests/acceptance_us1.rs").as_deref(),
            Some("cargo test --test acceptance_us1")
        );
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let file = ["tests/acceptance_us1.rs"];
        assert!(l.runs_only(&argv("cargo test --test acceptance_us1"), &file));
        assert!(!l.runs_only(&argv("cargo test --test acceptance_us2"), &file));
        assert!(!l.runs_only(&argv("cargo test"), &file));
        let text = l.text();
        assert!(text.contains("Rule: Cargo builds `tests/*.rs`"), "{text}");
        assert!(
            text.contains("inline `#[cfg(test)]` modules in 1 file(s)"),
            "{text}"
        );
        assert!(
            text.contains("Not run by anything: tests/unit/greet_name.rs"),
            "{text}"
        );
        // Same checkout, same bytes.
        assert_eq!(detect(d.path(), &[]), l);
    }

    #[test]
    fn no_collected_place_is_said_never_guessed() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            d.path(),
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"g\"\nversion = \"0.1.0\"\nautotests = false\n",
                ),
                ("src/lib.rs", ""),
            ],
        );
        let l = detect(d.path(), &[]);
        assert_eq!(l.story_file("US1", None), None);
        assert!(l.runners[0].unknown[0].contains("autotests = false"));
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(d.path(), &[("pytest.ini", "[pytest]\ntestpaths = spec\n")]);
        assert_eq!(detect(d.path(), &[]).story_file("US1", None), None);
    }

    #[test]
    fn python_js_and_go_story_files_follow_the_layout() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            d.path(),
            &[
                ("pytest.ini", "[pytest]\n"),
                ("tests/test_a.py", "def test_a():\n    pass\n"),
            ],
        );
        assert_eq!(
            detect(d.path(), &[]).story_file("US2", None).as_deref(),
            Some("tests/test_acceptance_us2.py")
        );
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            d.path(),
            &[
                (
                    "package.json",
                    "{ \"devDependencies\": { \"vitest\": \"3\" } }",
                ),
                ("src/a.test.ts", "it('a', () => {})\n"),
            ],
        );
        assert_eq!(
            detect(d.path(), &[]).story_file("US1", None).as_deref(),
            Some("src/acceptance-us1.test.ts")
        );
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            d.path(),
            &[("go.mod", "module ex.com/m\n"), ("g/g.go", "package g\n")],
        );
        assert_eq!(
            detect(d.path(), &[])
                .story_file("US1", Some("g"))
                .as_deref(),
            Some("g/acceptance_us1_test.go")
        );
    }
}
