//! The listing a check's own run implies: built from the argv of
//! a fast check that runs the toolchain — its selectors, filters and
//! packages kept — so a test counts only if a check the verdict rests on
//! would run it. No check runs the toolchain: there is no listing, and the
//! file is unknown.

/// Whether a check's argv runs `toolchain`'s tests.
#[must_use]
pub fn runs(toolchain: speccoder_core::collection::Toolchain, argv: &[String]) -> bool {
    use speccoder_core::collection::Toolchain;
    let has = |w: &str| argv.iter().any(|a| a == w || a.ends_with(&format!("/{w}")));
    let word = |i: usize| argv.get(i).map(String::as_str);
    match toolchain {
        Toolchain::Rust => {
            // `cargo test` or `cargo nextest run`: a cargo that runs tests.
            let cargo = argv
                .iter()
                .position(|a| a == "cargo" || a.ends_with("/cargo"));
            match cargo {
                Some(i) => {
                    matches!(word(i + 1), Some("test"))
                        || matches!((word(i + 1), word(i + 2)), (Some("nextest"), Some("run")))
                }
                None => has("cargo-nextest"),
            }
        }
        Toolchain::Python => argv.iter().any(|a| a.contains("pytest")),
        Toolchain::JavaScript => argv
            .iter()
            .any(|a| a.contains("vitest") || a.contains("jest")),
        Toolchain::Go => {
            has("gotestsum")
                || argv
                    .iter()
                    .position(|a| a == "go" || a.ends_with("/go"))
                    .is_some_and(|i| word(i + 1) == Some("test"))
        }
    }
}

/// `cargo nextest run …` as `cargo nextest list … --message-format json`,
/// its filters and package selection kept.
#[must_use]
pub fn nextest_list(argv: &[String]) -> Option<Vec<String>> {
    let i = argv.iter().position(|a| a == "nextest")?;
    if argv.get(i + 1).map(String::as_str) != Some("run") {
        return None;
    }
    // Options only `run` takes: dropped, never changing the selection.
    const RUN_ONLY: &[&str] = &[
        "--no-fail-fast",
        "--fail-fast",
        "--no-capture",
        "--nocapture",
        "--retries",
        "--success-output",
        "--failure-output",
        "--status-level",
        "--final-status-level",
        "--no-tests",
        "--test-threads",
        "-j",
        "--profile",
        "-P",
        "--max-fail",
    ];
    let mut out: Vec<String> = argv[..=i].to_vec();
    out.push("list".to_owned());
    let mut k = i + 2;
    while k < argv.len() {
        let a = &argv[k];
        let flag = a.split('=').next().unwrap_or(a);
        if RUN_ONLY.contains(&flag) {
            // A separate value goes with it.
            if !a.contains('=')
                && !matches!(
                    flag,
                    "--no-fail-fast" | "--fail-fast" | "--no-capture" | "--nocapture"
                )
            {
                k += 1;
            }
            k += 1;
            continue;
        }
        out.push(a.clone());
        k += 1;
    }
    out.extend(["--message-format", "json"].map(str::to_owned));
    Some(out)
}

/// A `cargo test` check's parts: options before `--`, and the test
/// binary's arguments after it.
fn split_cargo(argv: &[String]) -> (Vec<String>, Vec<String>) {
    match argv.iter().position(|a| a == "--") {
        Some(i) => (argv[..i].to_vec(), argv[i + 1..].to_vec()),
        None => (argv.to_vec(), Vec::new()),
    }
}

/// The target a `cargo test` check runs: whether it runs `binary` of
/// package `pkg` (`kind`: `lib`, `bin <name>`, `test <name>`).
#[must_use]
pub fn cargo_runs(argv: &[String], pkg: &str, root_package: bool, kind: &(String, String)) -> bool {
    let (opts, _) = split_cargo(argv);
    let value = |flag: &str| -> Vec<String> {
        let mut v = Vec::new();
        for (i, a) in opts.iter().enumerate() {
            if a == flag {
                if let Some(n) = opts.get(i + 1) {
                    v.push(n.clone());
                }
            } else if let Some(x) = a.strip_prefix(&format!("{flag}=")) {
                v.push(x.to_owned());
            }
        }
        v
    };
    let has = |f: &str| opts.iter().any(|a| a == f);
    let packages: Vec<String> = value("-p").into_iter().chain(value("--package")).collect();
    let package_ok = if !packages.is_empty() {
        packages.iter().any(|p| p == pkg)
    } else {
        root_package || has("--workspace") || has("--all")
    };
    if !package_ok {
        return false;
    }
    let tests = value("--test");
    let bins = value("--bin");
    let selected = has("--lib")
        || has("--tests")
        || has("--bins")
        || has("--doc")
        || has("--all-targets")
        || !tests.is_empty()
        || !bins.is_empty()
        || has("--examples")
        || has("--benches");
    if !selected || has("--all-targets") {
        return kind.0 != "bench" && kind.0 != "example";
    }
    match kind.0.as_str() {
        "lib" => has("--lib"),
        "bin" => has("--bins") || bins.contains(&kind.1),
        "test" => has("--tests") || tests.contains(&kind.1),
        _ => false,
    }
}

/// The listing of one target as the `cargo test` check would run it: its
/// package and feature options and its test filters kept, the target
/// selected, `--list` added. The second argv lists the ignored tests
/// (subtracted), unless the check runs them too.
#[must_use]
pub fn cargo_list(
    argv: &[String],
    pkg: &str,
    kind: &(String, String),
) -> (Vec<String>, Option<Vec<String>>) {
    let (opts, test_args) = split_cargo(argv);
    // Drop the check's own target selection; keep everything else.
    let mut kept = Vec::new();
    let mut i = 0;
    while i < opts.len() {
        let a = &opts[i];
        let flag = a.split('=').next().unwrap_or(a);
        match flag {
            "--lib" | "--tests" | "--bins" | "--doc" | "--all-targets" | "--examples"
            | "--benches" | "--no-fail-fast" => {}
            "--test" | "--bin" | "-p" | "--package" => {
                if !a.contains('=') {
                    i += 1;
                }
            }
            _ => kept.push(a.clone()),
        }
        i += 1;
    }
    let mut base = kept;
    base.extend(["-p".to_owned(), pkg.to_owned()]);
    match kind.0.as_str() {
        "lib" => base.push("--lib".to_owned()),
        "bin" => base.extend(["--bin".to_owned(), kind.1.clone()]),
        _ => base.extend(["--test".to_owned(), kind.1.clone()]),
    }
    base.push("--".to_owned());
    // Output-only flags of the test binary are dropped; filters stay.
    let filters: Vec<String> = test_args
        .iter()
        .filter(|a| {
            !matches!(
                a.as_str(),
                "--nocapture" | "--no-capture" | "--show-output" | "-q" | "--quiet"
            )
        })
        .cloned()
        .collect();
    let runs_ignored = filters
        .iter()
        .any(|a| a == "--ignored" || a == "--include-ignored");
    let mut list = base.clone();
    list.extend(
        filters
            .iter()
            .filter(|a| *a != "--include-ignored")
            .cloned(),
    );
    list.extend(["--list", "--format", "terse"].map(str::to_owned));
    let ignored = (!runs_ignored).then(|| {
        let mut l = base;
        l.extend(filters.iter().cloned());
        l.extend(["--list", "--format", "terse", "--ignored"].map(str::to_owned));
        l
    });
    (list, ignored)
}

fn drop_flags(argv: &[String], flags: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        let flag = a.split('=').next().unwrap_or(a);
        if flags.contains(&flag) {
            if !a.contains('=') && argv.get(i + 1).is_some_and(|n| !n.starts_with('-')) {
                i += 1;
            }
            i += 1;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

/// The pytest check with `--collect-only -q`: its paths, `-k`, `-m` kept.
#[must_use]
pub fn pytest_list(argv: &[String]) -> Vec<String> {
    let mut out = drop_flags(
        argv,
        &[
            "--junitxml",
            "--junit-xml",
            "--cov",
            "--cov-report",
            "-x",
            "--exitfirst",
            "-v",
            "--verbose",
        ],
    );
    out.extend(["--collect-only", "-q"].map(str::to_owned));
    out
}

/// The vitest check as `vitest list --json`, its filters and projects kept.
#[must_use]
pub fn vitest_list(argv: &[String]) -> Option<Vec<String>> {
    let i = argv.iter().position(|a| a.ends_with("vitest"))?;
    let mut out: Vec<String> = argv[..=i].to_vec();
    out.push("list".to_owned());
    let rest = &argv[i + 1..];
    let rest = match rest.first().map(String::as_str) {
        Some("run" | "watch" | "dev") => &rest[1..],
        _ => rest,
    };
    out.extend(drop_flags(
        rest,
        &[
            "--reporter",
            "--outputFile",
            "--coverage",
            "--run",
            "--silent",
        ],
    ));
    out.push("--json".to_owned());
    Some(out)
}

/// The jest check with `--listTests`, its path patterns and projects kept.
#[must_use]
pub fn jest_list(argv: &[String]) -> Vec<String> {
    let mut out = drop_flags(
        argv,
        &[
            "--reporters",
            "--ci",
            "--coverage",
            "--silent",
            "--json",
            "--outputFile",
        ],
    );
    out.push("--listTests".to_owned());
    out
}

/// The go check as `go test -list <its -run, else .> <its packages>`.
#[must_use]
pub fn go_list(argv: &[String]) -> Vec<String> {
    // gotestsum: the go arguments follow `--`.
    let go_args: Vec<String> = if argv.iter().any(|a| a.ends_with("gotestsum")) {
        argv.iter()
            .skip_while(|a| *a != "--")
            .skip(1)
            .cloned()
            .collect()
    } else {
        argv.iter()
            .skip_while(|a| !(a.as_str() == "go" || a.ends_with("/go")))
            .skip(2)
            .cloned()
            .collect()
    };
    let mut pattern = ".".to_owned();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < go_args.len() {
        let a = &go_args[i];
        if a == "-run" {
            if let Some(p) = go_args.get(i + 1) {
                pattern = p.clone();
            }
            i += 2;
            continue;
        }
        if let Some(p) = a.strip_prefix("-run=") {
            pattern = p.to_owned();
        } else if matches!(a.as_str(), "-v" | "-json" | "-race" | "-short")
            || a.starts_with("-count")
            || a.starts_with("-cover")
            || a.starts_with("-timeout")
            || a.starts_with("-coverprofile")
        {
        } else {
            rest.push(a.clone());
        }
        i += 1;
    }
    let mut out = vec![
        "go".to_owned(),
        "test".to_owned(),
        "-list".to_owned(),
        pattern,
    ];
    if rest.iter().all(|a| a.starts_with('-')) {
        rest.push("./...".to_owned());
    }
    out.extend(rest);
    out
}

/// A skip that a listing cannot see (the listing names the test, the run
/// skips it): pytest, JS and Go files with one are decided by a report only.
#[must_use]
pub fn skips_unseen_by_listing(
    toolchain: speccoder_core::collection::Toolchain,
    source: &str,
) -> bool {
    use speccoder_core::collection::Toolchain;
    let code = speccoder_core::collection::strip_comments(source);
    match toolchain {
        Toolchain::Python => [
            "pytest.mark.skip",
            "pytest.skip(",
            "pytest.mark.xfail",
            "unittest.skip",
            "pytest.importorskip",
        ]
        .iter()
        .any(|m| code.contains(m)),
        Toolchain::JavaScript => [
            ".skip(",
            ".todo(",
            "xit(",
            "xdescribe(",
            "xtest(",
            ".skipIf(",
            ".runIf(",
        ]
        .iter()
        .any(|m| code.contains(m)),
        Toolchain::Go => ["t.Skip", "testing.Short()"]
            .iter()
            .any(|m| code.contains(m)),
        // Ignored tests are listed separately (`--ignored`); `cfg` is compiled out.
        Toolchain::Rust => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use speccoder_core::collection::Toolchain;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn only_a_check_that_runs_the_tests_counts() {
        assert!(runs(Toolchain::Rust, &v(&["cargo", "test", "--lib"])));
        assert!(runs(Toolchain::Rust, &v(&["cargo", "nextest", "run"])));
        assert!(!runs(Toolchain::Rust, &v(&["cargo", "fmt", "--check"])));
        assert!(!runs(Toolchain::Rust, &v(&["cargo", "clippy"])));
        assert!(!runs(Toolchain::Go, &v(&["go", "vet", "./..."])));
        assert!(runs(Toolchain::Go, &v(&["go", "test", "./..."])));
    }

    #[test]
    fn a_cargo_check_runs_only_what_it_selects() {
        let lib = ("lib".to_owned(), String::new());
        let greet = ("test".to_owned(), "greet".to_owned());
        assert!(cargo_runs(&v(&["cargo", "test", "--lib"]), "g", true, &lib));
        assert!(!cargo_runs(
            &v(&["cargo", "test", "--lib"]),
            "g",
            true,
            &greet
        ));
        assert!(cargo_runs(&v(&["cargo", "test"]), "g", true, &greet));
        assert!(!cargo_runs(
            &v(&["cargo", "test", "-p", "other"]),
            "g",
            true,
            &greet
        ));
        assert!(cargo_runs(
            &v(&["cargo", "test", "--test", "greet"]),
            "g",
            true,
            &greet
        ));
        assert!(
            !cargo_runs(&v(&["cargo", "test"]), "g", false, &greet),
            "not the root package, no -p"
        );
    }

    #[test]
    fn listings_keep_the_check_s_selection() {
        let (l, ignored) = cargo_list(
            &v(&["cargo", "test", "--offline", "--", "greet", "--nocapture"]),
            "g",
            &("test".to_owned(), "greet".to_owned()),
        );
        assert_eq!(
            l,
            v(&[
                "cargo",
                "test",
                "--offline",
                "-p",
                "g",
                "--test",
                "greet",
                "--",
                "greet",
                "--list",
                "--format",
                "terse"
            ])
        );
        assert!(ignored.unwrap().ends_with(&v(&["--ignored"])));
        assert_eq!(
            nextest_list(&v(&[
                "cargo",
                "nextest",
                "run",
                "--profile",
                "speccoder",
                "--no-tests=warn",
                "-E",
                "test(a)"
            ]))
            .unwrap(),
            v(&[
                "cargo",
                "nextest",
                "list",
                "-E",
                "test(a)",
                "--message-format",
                "json"
            ])
        );
        assert_eq!(
            pytest_list(&v(&[
                "python3",
                "-m",
                "pytest",
                "tests/unit",
                "-k",
                "greet",
                "--junitxml=r.xml"
            ])),
            v(&[
                "python3",
                "-m",
                "pytest",
                "tests/unit",
                "-k",
                "greet",
                "--collect-only",
                "-q"
            ])
        );
        assert_eq!(
            vitest_list(&v(&["npx", "vitest", "run", "src/a", "--reporter=junit"])).unwrap(),
            v(&["npx", "vitest", "list", "src/a", "--json"])
        );
        assert_eq!(
            go_list(&v(&["go", "test", "-run", "TestA", "./pkg/..."])),
            v(&["go", "test", "-list", "TestA", "./pkg/..."])
        );
        assert_eq!(
            go_list(&v(&[
                "gotestsum",
                "--junitfile",
                "r.xml",
                "--",
                "-tags=it",
                "./..."
            ])),
            v(&["go", "test", "-list", ".", "-tags=it", "./..."])
        );
    }

    #[test]
    fn skips_a_listing_cannot_see_are_found() {
        assert!(skips_unseen_by_listing(
            Toolchain::Python,
            "@pytest.mark.skip\ndef test_a(): ..."
        ));
        assert!(skips_unseen_by_listing(
            Toolchain::JavaScript,
            "it.skip('a', () => {})"
        ));
        assert!(skips_unseen_by_listing(
            Toolchain::Go,
            "func TestA(t *testing.T) { t.Skip() }"
        ));
        assert!(!skips_unseen_by_listing(
            Toolchain::Python,
            "def test_a(): ..."
        ));
    }
}
