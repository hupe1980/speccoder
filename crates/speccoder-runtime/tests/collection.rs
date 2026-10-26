//! Which changed test files a run collected, per toolchain: each fixture
//! project is written into a temporary directory from the case table
//! below (no nested manifests in the crate), and every case is decided as
//! the table says — by the static rule, the JUnit report or a listing.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use speccoder_core::collection::{DecidedBy, FileState};
use speccoder_core::verify::{CheckResult, CheckStatus, TestOutcome, TestResult};
use speccoder_runtime::collection::{Input, judge};
use speccoder_runtime::verify::config::{Check, Tier};
use speccoder_runtime::verify::process::LocalRunner;

/// One case: a path the attempt wrote, and what the run must be found to
/// have done with it (`None`: not judged at all — a helper).
struct Case {
    path: &'static str,
    text: &'static str,
    expect: Option<(FileState, Option<DecidedBy>)>,
}

const C: Option<DecidedBy> = None;

fn case(path: &'static str, text: &'static str, state: FileState, by: Option<DecidedBy>) -> Case {
    Case {
        path,
        text,
        expect: Some((state, by)),
    }
}

fn helper(path: &'static str, text: &'static str) -> Case {
    Case {
        path,
        text,
        expect: None,
    }
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (p, t) in files {
        let f = root.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    }
}

fn check(argv: &[&str]) -> Check {
    Check {
        name: "test".to_owned(),
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        cwd: ".".to_owned(),
        when_exists: None,
        tier: Tier::Fast,
        timeout: Duration::from_secs(120),
        junit: None,
        list: None,
    }
}

fn result(argv: &[&str], ids: &[&str]) -> CheckResult {
    CheckResult {
        name: "test".to_owned(),
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        status: CheckStatus::Passed,
        exit_code: Some(0),
        duration_ms: 1,
        junit: true,
        tests: Some(
            ids.iter()
                .map(|i| TestResult {
                    id: (*i).to_owned(),
                    outcome: if i.ends_with("skipped") {
                        TestOutcome::Skipped
                    } else {
                        TestOutcome::Passed
                    },
                })
                .collect(),
        ),
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        truncated: false,
        note: None,
        flaky: false,
    }
}

/// Runs `cases` in a project of `base` files and checks each verdict.
fn run(
    base: &[(&str, &str)],
    cases: &[Case],
    checks: &[Check],
    results: &[CheckResult],
    listings: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), base);
    write(
        dir.path(),
        &cases.iter().map(|c| (c.path, c.text)).collect::<Vec<_>>(),
    );
    let changes: Vec<(String, String)> = cases
        .iter()
        .map(|c| (c.path.to_owned(), "added".to_owned()))
        .collect();
    let got = judge(&Input {
        root: dir.path(),
        changes: &changes,
        checks,
        results,
        runner: &LocalRunner,
        listings,
    })
    .unwrap_or_default();
    for c in cases {
        let f = got.files.iter().find(|f| f.path == c.path);
        match (&c.expect, f) {
            (None, None) => {}
            (None, Some(f)) => panic!("{} is a helper, judged as {f:?}", c.path),
            (Some(_), None) => panic!("{} not judged: {got:?}", c.path),
            (Some((state, by)), Some(f)) => {
                assert_eq!((f.state, f.decided_by), (*state, *by), "{}: {f:?}", c.path);
                if f.state == FileState::NotCollected {
                    assert!(f.rule.is_some() && !f.fixes.is_empty(), "{f:?}");
                }
            }
        }
    }
}

const NEXTEST: &[&str] = &["cargo", "nextest", "run", "--profile", "speccoder"];

#[test]
fn rust_files_are_decided_by_cargo_s_rule_then_the_report() {
    use DecidedBy::{Junit, Static};
    use FileState::{Collected, NotCollected};
    let base = [
        (
            "Cargo.toml",
            "[package]\nname = \"greet\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nslow = []\n",
        ),
        (
            "src/lib.rs",
            "pub fn greet() {}\n#[cfg(test)]\nmod tests { #[test] fn it_works() {} }\n",
        ),
        (
            "tests/suite/main.rs",
            "mod case;\n#[path = \"../shared/pathed.rs\"]\nmod pathed;\n",
        ),
        (
            "tests/gated/main.rs",
            "#[cfg(feature = \"slow\")]\nmod slow;\n",
        ),
    ];
    let cases = [
        case(
            "tests/unit/greet_name.rs",
            "#[test]\nfn a() {}\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "tests/deep/a/b.rs",
            "#[test]\nfn b() {}\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "tests/gated/slow.rs",
            "#[test]\nfn s() {}\n",
            NotCollected,
            Some(Junit),
        ),
        case(
            "tests/greet.rs",
            "#[test]\nfn greets() {}\n",
            Collected,
            Some(Junit),
        ),
        case(
            "tests/suite/case.rs",
            "#[test]\nfn works() {}\n",
            Collected,
            Some(Junit),
        ),
        case(
            "tests/shared/pathed.rs",
            "#[test]\nfn p() {}\n",
            Collected,
            Some(Junit),
        ),
        helper("tests/common/mod.rs", "pub fn setup() {}\n"),
    ];
    let ids = [
        "greet::tests::it_works",
        "greet::greet::greets",
        "greet::suite::case::works",
        "greet::suite::pathed::p",
        "greet::greet::ignored_one_skipped",
    ];
    run(
        &base,
        &cases,
        &[check(NEXTEST)],
        &[result(NEXTEST, &ids)],
        false,
    );
}

#[test]
fn the_poc_s_file_reads_cargo_s_rule_and_both_ways_out() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"greet\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub fn greet() {}\n"),
            (
                "tests/unit/greet_name.rs",
                "use greet::greet;\n#[test]\nfn greets() {}\n",
            ),
        ],
    );
    let changes = [("tests/unit/greet_name.rs".to_owned(), "added".to_owned())];
    let c = judge(&Input {
        root: dir.path(),
        changes: &changes,
        checks: &[check(NEXTEST)],
        results: &[result(NEXTEST, &["greet::tests::it_works"])],
        runner: &LocalRunner,
        listings: true,
    })
    .unwrap();
    assert_eq!(
        c.reasons(),
        [
            "tests not collected: `tests/unit/greet_name.rs` — Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare); this file is neither"
        ]
    );
    assert_eq!(
        c.retry_lines(),
        [
            "`tests/unit/greet_name.rs` never ran: Cargo builds `tests/*.rs` and `tests/<dir>/main.rs`. Move the tests to `tests/greet_name.rs`, or add `tests/unit/main.rs` with `mod greet_name;`."
        ]
    );
    assert!(
        c.listing.is_empty(),
        "the static rule decided it: no listing runs"
    );
}

#[test]
fn autotests_off_leaves_tests_rs_uncollected() {
    let base = [
        (
            "Cargo.toml",
            "[package]\nname = \"g\"\nversion = \"0.1.0\"\nedition = \"2024\"\nautotests = false\n\n[[test]]\nname = \"it\"\npath = \"tests/it.rs\"\n",
        ),
        ("src/lib.rs", ""),
    ];
    let cases = [
        case(
            "tests/other.rs",
            "#[test]\nfn o() {}\n",
            FileState::NotCollected,
            Some(DecidedBy::Static),
        ),
        case(
            "tests/it.rs",
            "#[test]\nfn i() {}\n",
            FileState::Collected,
            Some(DecidedBy::Junit),
        ),
    ];
    run(
        &base,
        &cases,
        &[check(NEXTEST)],
        &[result(NEXTEST, &["g::it::i"])],
        false,
    );
}

#[test]
fn a_real_cargo_listing_decides_a_reachable_file_without_a_report() {
    // `cargo test -- --list` on a scratch crate: what the runner would run.
    let base = [
        (
            "Cargo.toml",
            "[package]\nname = \"lister\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nslow = []\n\n[workspace]\n",
        ),
        ("src/lib.rs", ""),
        (
            "tests/gated/main.rs",
            "#[cfg(feature = \"slow\")]\nmod slow;\n",
        ),
    ];
    let cases = [
        case(
            "tests/listed.rs",
            "#[test]\nfn listed() {}\n",
            FileState::Collected,
            Some(DecidedBy::Listing),
        ),
        case(
            "tests/gated/slow.rs",
            "#[test]\nfn s() {}\n",
            FileState::NotCollected,
            Some(DecidedBy::Listing),
        ),
    ];
    let target = tempfile::tempdir().unwrap();
    // The listing builds the scratch crate; its build goes to a directory of its own.
    let cargo = ["cargo", "test"];
    let mut c = check(&cargo);
    c.timeout = Duration::from_secs(300);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), &base);
    write(
        dir.path(),
        &cases.iter().map(|c| (c.path, c.text)).collect::<Vec<_>>(),
    );
    write(
        dir.path(),
        &[(
            ".cargo/config.toml",
            &format!("[build]\ntarget-dir = \"{}\"\n", target.path().display()),
        )],
    );
    let changes: Vec<(String, String)> = cases
        .iter()
        .map(|c| (c.path.to_owned(), "added".to_owned()))
        .collect();
    let got = judge(&Input {
        root: dir.path(),
        changes: &changes,
        checks: &[c],
        results: &[],
        runner: &LocalRunner,
        listings: true,
    })
    .unwrap();
    for c in &cases {
        let f = got.files.iter().find(|f| f.path == c.path).unwrap();
        assert_eq!(Some((f.state, f.decided_by)), c.expect, "{f:?} {got:?}");
    }
    assert!(!got.listing.is_empty());
}

#[test]
fn pytest_files_follow_python_files_testpaths_and_norecursedirs() {
    use DecidedBy::{Junit, Static};
    use FileState::{Collected, NotCollected};
    let argv = ["python3", "-m", "pytest", "--junitxml=r.xml"];
    let base = [("pytest.ini", "[pytest]\ntestpaths = tests\n")];
    let cases = [
        case(
            "tests/check_greet.py",
            "def test_a():\n    pass\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "src/test_inside.py",
            "def test_a():\n    pass\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "tests/.cache/test_x.py",
            "def test_a():\n    pass\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "tests/test_greet.py",
            "def test_a():\n    pass\n",
            Collected,
            Some(Junit),
        ),
        case(
            "tests/sub/test_more.py",
            "class TestC:\n    def test_b(self):\n        pass\n",
            Collected,
            Some(Junit),
        ),
        case(
            "tests/greet_test.py",
            "def test_c():\n    pass\n",
            Collected,
            Some(Junit),
        ),
        helper("tests/conftest.py", "import pytest\n"),
    ];
    let ids = [
        "tests.test_greet::test_a",
        "tests.sub.test_more.TestC::test_b",
        "tests.greet_test::test_c",
    ];
    run(
        &base,
        &cases,
        &[check(&argv)],
        &[result(&argv, &ids)],
        false,
    );
}

#[test]
fn js_files_follow_the_runner_s_include() {
    use DecidedBy::{Junit, Static};
    use FileState::{Collected, NotCollected};
    let argv = ["npx", "vitest", "run", "--reporter=junit"];
    let base = [
        (
            "package.json",
            "{ \"devDependencies\": { \"vitest\": \"3\" } }",
        ),
        (
            "web/package.json",
            "{ \"jest\": { \"testMatch\": [\"**/*.jest.js\"] } }",
        ),
    ];
    let cases = [
        case(
            "src/greet.tests.ts",
            "it('greets', () => {})\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "src/greet_test.ts",
            "test('x', () => {})\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "web/x.test.js",
            "test('x', () => {})\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "src/greet.test.ts",
            "it('greets', () => {})\n",
            Collected,
            Some(Junit),
        ),
        case(
            "src/a.spec.js",
            "describe('a', () => {})\n",
            Collected,
            Some(Junit),
        ),
        case(
            "src/b.test.mjs",
            "test('b', () => {})\n",
            Collected,
            Some(Junit),
        ),
        helper("src/__fixtures__/data.test.ts", "export const data = 1\n"),
    ];
    let ids = [
        "src/greet.test.ts::greets",
        "src/a.spec.js::a",
        "src/b.test.mjs::b",
    ];
    run(
        &base,
        &cases,
        &[check(&argv)],
        &[result(&argv, &ids)],
        false,
    );
}

#[test]
fn a_vitest_config_in_code_is_not_read_and_unknown_never_passes() {
    let base = [
        (
            "package.json",
            "{ \"devDependencies\": { \"vitest\": \"3\" } }",
        ),
        (
            "vitest.config.ts",
            "export default { test: { include: ['**/*.tests.ts'] } }\n",
        ),
    ];
    let cases = [case(
        "src/greet.tests.ts",
        "it('a', () => {})\n",
        FileState::Unknown,
        C,
    )];
    run(
        &base,
        &cases,
        &[check(&["npx", "vitest", "run"])],
        &[],
        false,
    );
}

#[test]
fn go_files_follow_go_s_rules() {
    use DecidedBy::{Junit, Static};
    use FileState::{Collected, NotCollected};
    let argv = ["gotestsum", "--junitfile", "r.xml"];
    let base = [
        ("go.mod", "module ex.com/m\n\ngo 1.22\n"),
        ("g/greet.go", "package g\n"),
    ];
    let cases = [
        case(
            "g/testdata/x_test.go",
            "package g\nfunc TestX(t *testing.T) {}\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "g/tagged_test.go",
            "//go:build integration\n\npackage g\nfunc TestT(t *testing.T) {}\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "g/lower_test.go",
            "package g\nfunc testLower(t *testing.T) {}\n",
            NotCollected,
            Some(Static),
        ),
        case(
            "g/greet_test.go",
            "package g\nfunc TestGreet(t *testing.T) {}\n",
            Collected,
            Some(Junit),
        ),
        case(
            "g/more_test.go",
            "package g\nfunc TestMore(t *testing.T) {}\n",
            Collected,
            Some(Junit),
        ),
        case(
            "h/h_test.go",
            "package h\nfunc TestH(t *testing.T) {}\n",
            Collected,
            Some(Junit),
        ),
    ];
    let ids = [
        "ex.com/m/g::TestGreet",
        "ex.com/m/g::TestMore/sub",
        "ex.com/m/h::TestH",
    ];
    run(
        &base,
        &cases,
        &[check(&argv)],
        &[result(&argv, &ids)],
        false,
    );
}

#[test]
fn a_configured_list_decides_and_a_listing_that_times_out_is_unknown() {
    let base = [("pytest.ini", "[pytest]\n")];
    let mut listed = check(&["python3", "-m", "pytest"]);
    listed.list = Some(
        ["sh", "-c", "echo tests/test_a.py::test_a"]
            .map(str::to_owned)
            .to_vec(),
    );
    let cases = [
        case(
            "tests/test_a.py",
            "def test_a():\n    pass\n",
            FileState::Collected,
            Some(DecidedBy::Listing),
        ),
        case(
            "tests/test_b.py",
            "def test_b():\n    pass\n",
            FileState::NotCollected,
            Some(DecidedBy::Listing),
        ),
    ];
    run(&base, &cases, &[listed], &[], true);

    let mut slow = check(&["python3", "-m", "pytest"]);
    slow.list = Some(["sleep", "5"].map(str::to_owned).to_vec());
    slow.timeout = Duration::from_millis(300);
    let cases = [case(
        "tests/test_a.py",
        "def test_a():\n    pass\n",
        FileState::Unknown,
        C,
    )];
    run(&base, &cases, &[slow], &[], true);
}

#[test]
fn an_unrecognised_project_is_unknown() {
    let cases = [
        case("tests/x.rs", "#[test]\nfn a() {}\n", FileState::Unknown, C),
        case(
            "tests/test_x.py",
            "def test_a():\n    pass\n",
            FileState::Unknown,
            C,
        ),
    ];
    run(&[], &cases, &[check(&["make", "test"])], &[], true);
}
