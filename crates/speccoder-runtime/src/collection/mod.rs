//! Which of an attempt's changed test files the run actually collected
//! (FR-001…FR-010): a test that never ran is evidence of nothing.
//!
//! Each test file the attempt added or changed — a test path by
//! convention that defines tests by its toolchain's markers — is decided,
//! in order, by the toolchain's static rule (it alone can prove a file is
//! never collected), the check's fresh JUnit report, and the runner's own
//! listing (or the check's `list` command). What none of them decides is
//! `unknown`: it never passes.

pub mod junit_map;
pub mod listing;
pub mod static_cfg;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use speccoder_core::collection::{
    COMPILED_OUT, Collection, CollectionRule, DecidedBy, FileCollection, JsRunner, Toolchain,
    collected, defines_tests, go_breaks, go_declared_tests, go_rule, is_build_or_dependency,
    not_collected, rust_fixes, rust_reachable, rust_rule, rust_targets, toolchain_of, unknown,
};
use speccoder_core::verify::{CheckResult, TestOutcome};

use crate::verify::config::Check;
use crate::verify::process::CommandRunner;
use junit_map::Place;

/// What the judge hands the collection check.
pub struct Input<'a> {
    /// The worktree, after the attempt.
    pub root: &'a Path,
    /// The attempt's changes: path and `added`/`modified`/`deleted`.
    pub changes: &'a [(String, String)],
    /// The fast checks as configured.
    pub checks: &'a [Check],
    /// Their results after the attempt.
    pub results: &'a [CheckResult],
    /// Runs listings (the sandbox).
    pub runner: &'a dyn CommandRunner,
    /// Whether listings may run (only for an otherwise passing attempt).
    pub listings: bool,
}

/// The attempt's changed files that are judged: test paths by convention
/// that define tests, not deleted, not build output or dependencies, not
/// benchmarks or examples (FR-007).
#[must_use]
pub fn judged_files(root: &Path, changes: &[(String, String)]) -> Vec<(String, Toolchain)> {
    let mut out: Vec<(String, Toolchain)> = changes
        .iter()
        .filter(|(_, kind)| kind != "deleted")
        .filter(|(p, _)| {
            (crate::tools::paths::is_test_path(p) || test_named(p)) && !is_build_or_dependency(p)
        })
        .filter(|(p, _)| !p.split('/').any(|c| c == "benches" || c == "examples"))
        .filter_map(|(p, _)| {
            let t = toolchain_of(p)?;
            let text = std::fs::read_to_string(root.join(p)).ok()?;
            defines_tests(t, &text).then(|| (p.clone(), t))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A file whose name says it holds tests (`greet.tests.ts`, `x-spec.js`)
/// even where speccoder's path conventions do not list it.
fn test_named(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    stem.split(['.', '_', '-']).any(|w| {
        matches!(
            w.to_ascii_lowercase().trim_end_matches('s'),
            "test" | "spec"
        )
    })
}

/// The listing a file needs, keyed so it runs once per command.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Need {
    argv: Vec<String>,
    cwd: String,
    kind: ListKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ListKind {
    Nextest,
    Libtest,
    Pytest,
    Vitest,
    Jest,
    Go,
    Custom,
}

/// One file waiting for a listing.
struct Pending {
    path: String,
    place: Place,
    rule: Option<CollectionRule>,
    needs: Vec<(Need, Option<String>)>,
}

/// Decides every judged file of the attempt; `None` when it changed none.
#[must_use]
pub fn judge(input: &Input<'_>) -> Option<Collection> {
    let judged = judged_files(input.root, input.changes);
    if judged.is_empty() {
        return None;
    }
    let files = static_cfg::files(input.root);
    let argvs: Vec<Vec<String>> = input.checks.iter().map(|c| c.argv.clone()).collect();
    let timeout = input
        .checks
        .iter()
        .map(|c| c.timeout)
        .max()
        .unwrap_or(Duration::from_secs(600));
    let custom = input.checks.iter().find_map(|c| c.list.clone());
    // A configured listing runs under its own check's deadline.
    let custom_timeout = input
        .checks
        .iter()
        .find(|c| c.list.is_some())
        .map_or(timeout, |c| c.timeout);
    let packages = static_cfg::rust_packages(input.root, &files);
    let js = static_cfg::js_packages(input.root, &files, &argvs);
    let go = static_cfg::go_modules(input.root, &files);
    let pytest = static_cfg::uses_pytest(input.root, &files, &argvs)
        .then(|| static_cfg::pytest_rule(input.root));
    let read = |p: &str| std::fs::read_to_string(input.root.join(p)).ok();

    let mut out: Vec<FileCollection> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    for (path, toolchain) in &judged {
        let source = read(path).unwrap_or_default();
        // 1. The static rule; what it cannot decide waits for a report.
        let (place, rule, needs) = match toolchain {
            Toolchain::Rust => {
                let Some(pkg) = static_cfg::owner(&packages, path) else {
                    unrecognised(
                        &mut out,
                        &mut pending,
                        custom.as_ref(),
                        path,
                        Place::Rust(BTreeMap::new()),
                        "no Cargo.toml governs it",
                    );
                    continue;
                };
                let targets = rust_targets(pkg, &files);
                let reach = rust_reachable(&targets, &|p| read(p));
                let rule = rust_rule(pkg);
                let Some(places) = reach.get(path) else {
                    let tests =
                        speccoder_core::collection::join(&pkg.root, "tests").unwrap_or_default();
                    let why = if pkg.autotests && path.starts_with(&format!("{tests}/")) {
                        "this file is neither"
                    } else {
                        "no target or `mod` declaration reaches it"
                    };
                    out.push(not_collected(
                        path,
                        DecidedBy::Static,
                        rule,
                        why,
                        rust_fixes(&pkg.root, path),
                    ));
                    continue;
                };
                let nextest = argvs.iter().any(|a| a.iter().any(|w| w == "nextest"));
                let needs: Vec<(Need, Option<String>)> = if nextest {
                    vec![(
                        Need {
                            argv: [
                                "cargo",
                                "nextest",
                                "list",
                                "--message-format",
                                "json",
                                "--workspace",
                            ]
                            .map(str::to_owned)
                            .to_vec(),
                            cwd: String::new(),
                            kind: ListKind::Nextest,
                        },
                        None,
                    )]
                } else {
                    places
                        .iter()
                        .filter_map(|(binary, _)| {
                            targets.iter().find(|t| &t.binary_id == binary)?;
                            let sel: Vec<String> =
                                match binary.strip_prefix(&format!("{}::", pkg.name)) {
                                    None => vec!["--lib".to_owned()],
                                    Some(b) => match b.strip_prefix("bin/") {
                                        Some(bin) => vec!["--bin".to_owned(), bin.to_owned()],
                                        None => vec!["--test".to_owned(), b.to_owned()],
                                    },
                                };
                            let mut argv = vec![
                                "cargo".to_owned(),
                                "test".to_owned(),
                                "-p".to_owned(),
                                pkg.name.clone(),
                            ];
                            argv.extend(sel);
                            argv.extend(["--", "--list", "--format", "terse"].map(str::to_owned));
                            Some((
                                Need {
                                    argv,
                                    cwd: String::new(),
                                    kind: ListKind::Libtest,
                                },
                                Some(binary.clone()),
                            ))
                        })
                        .collect()
                };
                (Place::Rust(reach), rule, needs)
            }
            Toolchain::Python => {
                let Some(rule) = pytest.as_ref() else {
                    unrecognised(
                        &mut out,
                        &mut pending,
                        custom.as_ref(),
                        path,
                        Place::Python,
                        "no pytest configuration or check: speccoder knows pytest's collection only",
                    );
                    continue;
                };
                if let Some(why) = rule.breaks(path) {
                    out.push(not_collected(
                        path,
                        DecidedBy::Static,
                        rule.rule(),
                        &why,
                        rule.fixes(path),
                    ));
                    continue;
                }
                let mut argv = prefix(&argvs, "pytest")
                    .unwrap_or_else(|| ["python3", "-m", "pytest"].map(str::to_owned).to_vec());
                argv.extend(["--collect-only", "-q"].map(str::to_owned));
                (
                    Place::Python,
                    rule.rule(),
                    vec![(
                        Need {
                            argv,
                            cwd: String::new(),
                            kind: ListKind::Pytest,
                        },
                        None,
                    )],
                )
            }
            Toolchain::JavaScript => {
                let Some(pkg) = static_cfg::js_owner(&js, path) else {
                    unrecognised(
                        &mut out,
                        &mut pending,
                        custom.as_ref(),
                        path,
                        Place::Js(path.clone()),
                        "no package.json uses vitest or jest: speccoder knows their collection only",
                    );
                    continue;
                };
                let rel = if pkg.root.is_empty() {
                    path.clone()
                } else {
                    path.trim_start_matches(&format!("{}/", pkg.root))
                        .to_owned()
                };
                let rule = match &pkg.rule {
                    Ok(r) => {
                        if let Some(why) = r.breaks(&rel) {
                            out.push(not_collected(
                                path,
                                DecidedBy::Static,
                                r.rule(),
                                &why,
                                r.fixes(&rel),
                            ));
                            continue;
                        }
                        r.rule()
                    }
                    Err(_) => CollectionRule {
                        toolchain: Toolchain::JavaScript,
                        text: format!(
                            "{} collects the files its configuration includes",
                            pkg.runner.name()
                        ),
                        source: format!("{}/package.json", pkg.root)
                            .trim_start_matches('/')
                            .to_owned(),
                    },
                };
                let (name, extra, kind) = match pkg.runner {
                    JsRunner::Vitest => ("vitest", vec!["list", "--json"], ListKind::Vitest),
                    JsRunner::Jest => ("jest", vec!["--listTests"], ListKind::Jest),
                };
                let mut argv =
                    prefix(&argvs, name).unwrap_or_else(|| vec!["npx".to_owned(), name.to_owned()]);
                argv.extend(extra.into_iter().map(str::to_owned));
                (
                    Place::Js(rel),
                    rule,
                    vec![(
                        Need {
                            argv,
                            cwd: pkg.root.clone(),
                            kind,
                        },
                        None,
                    )],
                )
            }
            Toolchain::Go => {
                let Some(module) = go
                    .iter()
                    .filter(|m| m.root.is_empty() || path.starts_with(&format!("{}/", m.root)))
                    .max_by_key(|m| m.root.len())
                else {
                    unrecognised(
                        &mut out,
                        &mut pending,
                        custom.as_ref(),
                        path,
                        Place::Go(String::new(), Vec::new()),
                        "no go.mod governs it",
                    );
                    continue;
                };
                let tags = static_cfg::go_tags(&argvs);
                if let Some((why, fixes)) = go_breaks(path, &source, &tags) {
                    out.push(not_collected(
                        path,
                        DecidedBy::Static,
                        go_rule(),
                        &why,
                        fixes,
                    ));
                    continue;
                }
                let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
                let rel_dir = dir
                    .strip_prefix(module.root.as_str())
                    .unwrap_or(dir)
                    .trim_start_matches('/')
                    .to_owned();
                let import = if rel_dir.is_empty() {
                    module.path.clone()
                } else {
                    format!("{}/{rel_dir}", module.path)
                };
                let mut argv = vec!["go".to_owned(), "test".to_owned()];
                if !tags.is_empty() {
                    argv.push(format!(
                        "-tags={}",
                        tags.iter().cloned().collect::<Vec<_>>().join(",")
                    ));
                }
                argv.extend(["-list", "."].map(str::to_owned));
                argv.push(if rel_dir.is_empty() {
                    ".".to_owned()
                } else {
                    format!("./{rel_dir}")
                });
                (
                    Place::Go(import, go_declared_tests(&source)),
                    go_rule(),
                    vec![(
                        Need {
                            argv,
                            cwd: module.root.clone(),
                            kind: ListKind::Go,
                        },
                        None,
                    )],
                )
            }
        };
        // 2. The check's fresh JUnit report, from a check that runs this toolchain.
        let reports: Vec<&CheckResult> = input
            .results
            .iter()
            .filter(|r| r.tests.is_some() && runs(toolchain, &r.argv))
            .collect();
        if !reports.is_empty() {
            let ids: Vec<String> = reports
                .iter()
                .flat_map(|r| r.tests.iter().flatten())
                .filter(|t| !matches!(t.outcome, TestOutcome::Skipped))
                .map(|t| t.id.clone())
                .collect();
            let mine = junit_map::tests_of(&place, path, &ids);
            out.push(if mine.is_empty() {
                not_collected(
                    path,
                    DecidedBy::Junit,
                    rule,
                    COMPILED_OUT,
                    compiled_out_fixes(*toolchain),
                )
            } else {
                collected(path, DecidedBy::Junit, &mine)
            });
            continue;
        }
        // 3. A listing: the check's own `list`, else the runner's.
        let needs = match &custom {
            Some(argv) => vec![(
                Need {
                    argv: argv.clone(),
                    cwd: String::new(),
                    kind: ListKind::Custom,
                },
                None,
            )],
            None => needs,
        };
        pending.push(Pending {
            path: path.clone(),
            place,
            rule: Some(rule),
            needs,
        });
    }

    let mut runs = Vec::new();
    if !pending.is_empty() && input.listings {
        let mut cache: BTreeMap<Need, Result<String, String>> = BTreeMap::new();
        for p in &pending {
            let mut ids: Vec<String> = Vec::new();
            let mut failed: Option<String> = None;
            for (need, binary) in &p.needs {
                let got = cache.entry(need.clone()).or_insert_with(|| {
                    let cwd = input.root.join(&need.cwd);
                    let deadline = if need.kind == ListKind::Custom {
                        custom_timeout
                    } else {
                        timeout
                    };
                    let (r, run) = listing::run(input.runner, &cwd, &need.argv, deadline);
                    runs.push(run);
                    r.map(|l| l.stdout)
                });
                match got {
                    Err(why) => failed = Some(why.clone()),
                    Ok(stdout) => {
                        let root = input.root.join(&need.cwd);
                        let listed: Option<Vec<String>> = match need.kind {
                            ListKind::Nextest => listing::nextest_ids(stdout),
                            ListKind::Libtest => Some(
                                listing::libtest_names(stdout)
                                    .into_iter()
                                    .map(|n| {
                                        format!("{}::{n}", binary.as_deref().unwrap_or_default())
                                    })
                                    .collect(),
                            ),
                            ListKind::Pytest => Some(listing::pytest_ids(stdout)),
                            ListKind::Vitest => listing::vitest_files(stdout, &root),
                            ListKind::Jest => Some(listing::jest_files(stdout, &root)),
                            ListKind::Go => Some(listing::go_names(stdout)),
                            ListKind::Custom => Some(listing::lines(stdout, input.root)),
                        };
                        match listed {
                            Some(l) => ids.extend(l),
                            None => {
                                failed = Some(format!(
                                    "the output of `{}` could not be read",
                                    need.argv.join(" ")
                                ))
                            }
                        }
                    }
                }
            }
            if let Some(why) = failed {
                out.push(unknown(&p.path, &why));
                continue;
            }
            let custom = p.needs.iter().any(|(n, _)| n.kind == ListKind::Custom);
            let mine = if custom {
                junit_map::listed(&p.place, &p.path, &ids)
            } else {
                match &p.place {
                    Place::Js(rel) => ids
                        .iter()
                        .filter(|f| *f == rel || *f == &p.path)
                        .cloned()
                        .collect(),
                    place => junit_map::tests_of(place, &p.path, &ids),
                }
            };
            let toolchain = p
                .rule
                .as_ref()
                .map(|r| r.toolchain)
                .or_else(|| toolchain_of(&p.path))
                .unwrap_or(Toolchain::Rust);
            out.push(match (mine.is_empty(), &p.rule) {
                (false, _) => collected(&p.path, DecidedBy::Listing, &mine),
                (true, Some(rule)) => not_collected(
                    &p.path,
                    DecidedBy::Listing,
                    rule.clone(),
                    COMPILED_OUT,
                    compiled_out_fixes(toolchain),
                ),
                (true, None) => not_collected(
                    &p.path,
                    DecidedBy::Listing,
                    CollectionRule {
                        toolchain,
                        text: "the check's `list` command names the tests it runs".to_owned(),
                        source: "speccoder.toml [checks] list".to_owned(),
                    },
                    "it names none from this file",
                    vec!["put the tests where the check runs them".to_owned()],
                ),
            });
        }
    } else {
        for p in &pending {
            out.push(unknown(
                &p.path,
                "no JUnit report names its tests and no listing ran",
            ));
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Some(Collection {
        files: out,
        listing: runs,
    })
}

/// Why the runner would never collect a new file at `path` with `content`,
/// by the static rule alone (`None`: it may be collected, or the rule is
/// not known statically). For placing new test files (FR-029, FR-031).
#[must_use]
pub fn statically_refused(root: &Path, path: &str, content: &str) -> Option<String> {
    let mut files = static_cfg::files(root);
    files.insert(path.to_owned());
    let read = |p: &str| {
        if p == path {
            Some(content.to_owned())
        } else {
            std::fs::read_to_string(root.join(p)).ok()
        }
    };
    match toolchain_of(path)? {
        Toolchain::Rust => {
            let packages = static_cfg::rust_packages(root, &files);
            let pkg = static_cfg::owner(&packages, path)?;
            let reach = rust_reachable(&rust_targets(pkg, &files), &read);
            (!reach.contains_key(path)).then(|| rust_rule(pkg).text)
        }
        Toolchain::Python => {
            let rule = static_cfg::pytest_rule(root);
            rule.breaks(path).map(|_| rule.rule().text)
        }
        Toolchain::JavaScript => {
            let js = static_cfg::js_packages(root, &files, &[]);
            let pkg = static_cfg::js_owner(&js, path)?;
            let r = pkg.rule.as_ref().ok()?;
            let rel = if pkg.root.is_empty() {
                path.to_owned()
            } else {
                path.trim_start_matches(&format!("{}/", pkg.root))
                    .to_owned()
            };
            r.breaks(&rel).map(|_| r.rule().text)
        }
        Toolchain::Go => {
            go_breaks(path, content, &std::collections::BTreeSet::new()).map(|_| go_rule().text)
        }
    }
}

/// A file whose runner speccoder does not recognise: the check's own
/// `list` decides it when there is one, else it is unknown.
fn unrecognised(
    out: &mut Vec<FileCollection>,
    pending: &mut Vec<Pending>,
    custom: Option<&Vec<String>>,
    path: &str,
    place: Place,
    why: &str,
) {
    match custom {
        Some(argv) => pending.push(Pending {
            path: path.to_owned(),
            place,
            rule: None,
            needs: vec![(
                Need {
                    argv: argv.clone(),
                    cwd: String::new(),
                    kind: ListKind::Custom,
                },
                None,
            )],
        }),
        None => out.push(unknown(path, why)),
    }
}

/// Whether a check's argv runs `toolchain`'s tests.
fn runs(toolchain: &Toolchain, argv: &[String]) -> bool {
    let has = |w: &str| argv.iter().any(|a| a == w || a.ends_with(&format!("/{w}")));
    match toolchain {
        Toolchain::Rust => has("cargo") || has("cargo-nextest"),
        Toolchain::Python => argv.iter().any(|a| a.contains("pytest")),
        Toolchain::JavaScript => argv
            .iter()
            .any(|a| a.contains("vitest") || a.contains("jest")),
        Toolchain::Go => has("go") || has("gotestsum"),
    }
}

/// The checks' argv up to and including the word naming `runner`.
fn prefix(argvs: &[Vec<String>], runner: &str) -> Option<Vec<String>> {
    argvs.iter().find_map(|a| {
        let i = a
            .iter()
            .position(|w| w == runner || w.ends_with(&format!("/{runner}")))?;
        Some(a[..=i].to_vec())
    })
}

fn compiled_out_fixes(t: Toolchain) -> Vec<String> {
    vec![
        match t {
            Toolchain::Rust => {
                "make the check run them (a `cfg` feature they need, an `#[ignore]` they carry)"
            }
            Toolchain::Python => {
                "make the check collect them (a marker or `-k` that deselects them, a skip)"
            }
            Toolchain::JavaScript => {
                "make the check include them (a project or filter that leaves them out, a `.skip`)"
            }
            Toolchain::Go => {
                "make the check run them (a `-run` filter or build tag that leaves them out)"
            }
        }
        .to_owned(),
    ]
}
