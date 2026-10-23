//! JUnit XML → per-test results (research R2): the common report format of
//! nextest, pytest `--junitxml`, jest-junit and Surefire.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use speccoder_core::verify::{TestOutcome, TestResult};

/// Parses a JUnit report.
pub fn parse(xml: &str) -> Result<Vec<TestResult>, String> {
    let mut reader = Reader::from_str(xml);
    let mut tests = Vec::new();
    // The open <testcase>, if inside one.
    let mut open: Option<TestResult> = None;
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("invalid JUnit XML: {e}"))?
        {
            Event::Start(e) if e.name().as_ref() == "testcase" => {
                open = Some(TestResult {
                    id: test_id(&e)?,
                    outcome: TestOutcome::Passed,
                });
            }
            Event::Empty(e) if e.name().as_ref() == "testcase" => tests.push(TestResult {
                id: test_id(&e)?,
                outcome: TestOutcome::Passed,
            }),
            Event::Start(e) | Event::Empty(e) => {
                if let Some(t) = open.as_mut() {
                    match e.name().as_ref() {
                        "failure" => t.outcome = TestOutcome::Failed,
                        // An assertion failure says more than an error beside it.
                        "error" if t.outcome != TestOutcome::Failed => {
                            t.outcome = TestOutcome::Error;
                        }
                        "skipped" if !t.outcome.is_failing() => {
                            t.outcome = TestOutcome::Skipped;
                        }
                        _ => {}
                    }
                }
            }
            Event::End(e) if e.name().as_ref() == "testcase" => {
                tests.extend(open.take());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(tests)
}

fn test_id(e: &BytesStart<'_>) -> Result<String, String> {
    let attr = |key: &str| -> Result<Option<String>, String> {
        match e
            .try_get_attribute(key)
            .map_err(|err| format!("invalid JUnit attribute: {err}"))?
        {
            Some(a) => a
                .normalized_value(XmlVersion::default())
                .map(|v| Some(v.into_owned()))
                .map_err(|err| format!("invalid JUnit attribute: {err}")),
            None => Ok(None),
        }
    };
    let name = attr("name")?.ok_or("a <testcase> has no name")?;
    Ok(match attr("classname")? {
        Some(class) if !class.is_empty() => format!("{class}::{name}"),
        _ => name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nextest_style_reports() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" failures="1" errors="0">
  <testsuite name="speccoder-core" tests="3" failures="1">
    <testcase name="graph::tests::ok" classname="speccoder-core" timestamp="x" time="0.01"/>
    <testcase name="graph::tests::bad" classname="speccoder-core" time="0.01">
      <failure type="test failure">assertion failed &amp; more</failure>
      <system-out>out</system-out>
    </testcase>
    <testcase name="graph::tests::later" classname="speccoder-core"><skipped/></testcase>
  </testsuite>
</testsuites>"#;
        let tests = parse(xml).unwrap();
        let got: Vec<_> = tests.iter().map(|t| (t.id.as_str(), t.outcome)).collect();
        assert_eq!(
            got,
            [
                ("speccoder-core::graph::tests::ok", TestOutcome::Passed),
                ("speccoder-core::graph::tests::bad", TestOutcome::Failed),
                ("speccoder-core::graph::tests::later", TestOutcome::Skipped),
            ]
        );
    }

    #[test]
    fn reads_pytest_style_reports_with_errors() {
        let xml = r#"<testsuites><testsuite name="pytest"><testcase classname="tests.test_api" name="test_login"><error message="boom"/></testcase><testcase name="test_bare"/></testsuite></testsuites>"#;
        let tests = parse(xml).unwrap();
        assert_eq!(tests[0].id, "tests.test_api::test_login");
        assert_eq!(tests[0].outcome, TestOutcome::Error);
        assert_eq!(tests[1].id, "test_bare");
    }

    #[test]
    fn a_failure_beside_an_error_is_a_failure() {
        let xml = r#"<testsuite><testcase name="a"><error/><failure/></testcase><testcase name="b"><failure/><error/></testcase></testsuite>"#;
        let tests = parse(xml).unwrap();
        assert!(tests.iter().all(|t| t.outcome == TestOutcome::Failed));
    }

    #[test]
    fn rejects_malformed_xml() {
        assert!(parse("<testsuite><testcase name=\"a\"></testsuite>").is_err());
    }
}
