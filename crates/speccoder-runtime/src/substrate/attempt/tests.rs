//! The attempt's tests.

use super::*;

#[test]
fn labels_are_read_from_the_parts_and_refusals_carry_the_call() {
    use agentplane::core::{Label, SourceId};
    let model = Label::untrusted(SourceId::new("model:x/y"));
    // A refusal quoting the call is the call's; a constant is trusted.
    assert!(
        refusal("denied: `src/x` is protected".to_owned(), &model)
            .label()
            .is_untrusted()
    );
    // Parts labelled when admitted: the spec trusted, the digest the verifier's.
    let input = Tainted::object([
        ("bundle", Tainted::trusted(json!({}))),
        (
            "digest",
            Tainted::with_label(json!({}), provenance::from(provenance::VERIFIER)),
        ),
        (
            "settings",
            Tainted::object([("nudges", Tainted::trusted(json!([])))]),
        ),
    ]);
    let label = conversation_label(&input);
    assert!(
        label
            .provenance
            .contains(&SourceId::new(provenance::VERIFIER))
    );
    // No labels of its own: every part takes the whole input's (fails closed).
    let flat = Tainted::with_label(json!({ "bundle": {} }), model.clone());
    assert_eq!(conversation_label(&flat).provenance, model.provenance);
}

#[test]
fn absolute_paths_inside_the_root_become_relative_and_others_stay() {
    let mut a = json!({ "path": "/work/repo/specs/001-x/plan.md" });
    relativise(&mut a, Some("/work/repo"));
    assert_eq!(a["path"], "specs/001-x/plan.md");
    let mut outside = json!({ "path": "/work/repository-two/x" });
    relativise(&mut outside, Some("/work/repo"));
    assert_eq!(outside["path"], "/work/repository-two/x");
    let mut none = json!({ "path": "/etc/passwd" });
    relativise(&mut none, None);
    assert_eq!(none["path"], "/etc/passwd");
}

/// A stop is its evidence: what speccoder cannot check goes on, the
/// rest is asked once, a refused call stops at once.
#[test]
fn a_stop_stands_on_evidence_speccoder_checks() {
    let gate = StopGate {
        spec: "Rows are sorted\n  ascending by name.".to_owned(),
        frozen: vec!["tests/test_us3.py".to_owned()],
        rejected: false,
    };
    let call =
        |kind: &str, evidence: &str| json!({ "kind": kind, "evidence": evidence, "reason": "why" });
    let judge =
        |args: &Value, refused: bool, asked: bool| judge_stop(&gate, args, refused, asked, 5);
    let unproven = |s: Stop| matches!(s, Stop::Refused(_));

    // The T018 stop: scope is no kind, an entry point is no evidence.
    assert!(unproven(judge(
        &json!({ "reason": "beyond the task's module scope" }),
        false,
        false
    )));
    assert!(unproven(judge(&call("scope", "src/cli"), false, false)));
    assert!(unproven(judge(
        &call("wrong_test", "tests/new_test.py"),
        false,
        false
    )));
    assert!(unproven(judge(
        &call("spec_conflict", "sorted descending by name"),
        false,
        false
    )));
    assert!(unproven(judge(
        &call("spec_conflict", "sorted"),
        false,
        false
    )));
    assert!(unproven(judge(
        &call("refused", "cargo add x"),
        false,
        false
    )));

    // Checked evidence is asked about once, with the turns left; the
    // second call stops, its reason first.
    let quote = call("spec_conflict", "sorted ascending by name");
    assert!(
        matches!(judge(&quote, false, false), Stop::Asked(t) if t.contains("5 turns are left"))
    );
    assert_eq!(
        judge(&quote, false, true),
        Stop::Stopped("why — the spec says “sorted ascending by name”".to_owned())
    );
    assert_eq!(
        judge(&call("wrong_test", "./tests/test_us3.py"), false, true),
        Stop::Stopped("why — frozen test `./tests/test_us3.py`".to_owned())
    );
    // A refused call needs no second word, nor does the last turn.
    assert!(matches!(
        judge(&call("refused", "cargo add x"), true, false),
        Stop::Stopped(_)
    ));
    let q = call("needs_information", "which locale?");
    assert!(matches!(
        judge_stop(&gate, &q, false, false, 0),
        Stop::Stopped(_)
    ));
}

/// The md2csv example (2026-11-04): after each answer the attempt's
/// whole turn log was printed again. What a replayed step says is not.
#[test]
fn a_replayed_step_says_nothing() {
    #[derive(Debug, Default)]
    struct Seen(std::sync::Mutex<Vec<u32>>);
    impl crate::progress::Progress for Seen {
        fn event(&self, e: &crate::progress::Event) {
            if let crate::progress::Event::ToolCall { turn, .. } = e {
                self.0.lock().unwrap().push(*turn);
            }
        }
    }
    let seen = std::sync::Arc::new(Seen::default());
    let replaying = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let quiet = Quiet {
        replaying: std::sync::Arc::clone(&replaying),
        next: seen.clone(),
    };
    let call = |turn| crate::progress::Event::ToolCall {
        task: "T012".to_owned(),
        turn,
        tool: "run_command".to_owned(),
        target: String::new(),
        decision: "allow".to_owned(),
        ok: Some(true),
        tail: Vec::new(),
        contained: false,
    };
    crate::progress::Progress::event(&quiet, &call(1));
    replaying.store(false, std::sync::atomic::Ordering::SeqCst);
    crate::progress::Progress::event(&quiet, &call(2));
    assert_eq!(*seen.0.lock().unwrap(), [2]);
}
