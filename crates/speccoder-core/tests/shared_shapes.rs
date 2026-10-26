//! The shared pieces this build introduces serialise to the field
//! and name sets their owners' data models define, so the owners extend
//! them rather than replace them.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use serde_json::Value;

fn fields(v: &Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn pre_frozen_acceptance_is_spec_093s_record() {
    use speccoder_core::acceptance::{
        Decided, PreFrozenAcceptance, Producer, SealedRef, VisibleTest,
    };
    let r = PreFrozenAcceptance {
        unit: "u".to_owned(),
        requirement: "FR-001".to_owned(),
        wording_digest: "w".to_owned(),
        approval_digest: "a".to_owned(),
        visible: vec![VisibleTest {
            path: "p".to_owned(),
            sha256: "s".to_owned(),
            cites: vec!["E1".to_owned()],
        }],
        held_out_sealed: Some(SealedRef {
            key: "FR-001".to_owned(),
            manifest_sha256: "m".to_owned(),
            count: 1,
        }),
        producer: Producer::Review,
        decided: Decided {
            by: "Ada".to_owned(),
            via: speccoder_core::actions::Via::Web,
            at: 0,
        },
    };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(
        fields(&v),
        set(&[
            "unit",
            "requirement",
            "wording_digest",
            "approval_digest",
            "visible",
            "held_out_sealed",
            "producer",
            "decided"
        ])
    );
    assert_eq!(fields(&v["visible"][0]), set(&["path", "sha256", "cites"]));
    assert_eq!(
        fields(&v["held_out_sealed"]),
        set(&["key", "manifest_sha256", "count"])
    );
    assert_eq!(fields(&v["decided"]), set(&["by", "via", "at"]));
    for (p, w) in [
        (Producer::Review, "review"),
        (Producer::BugConfirmation, "bug_confirmation"),
        (Producer::PersonTest, "person_test"),
    ] {
        assert_eq!(serde_json::to_value(p).unwrap(), w);
    }
}

#[test]
fn context_entries_are_spec_098s() {
    use speccoder_core::context::{ContextEntry, ContextKind, ContextManifest};
    let m = ContextManifest::of(
        vec![ContextEntry {
            kind: ContextKind::Issue,
            source: "github:o/r#1".to_owned(),
            reference: "#1".to_owned(),
            sha256: "s".to_owned(),
            bytes: 1,
            cut: false,
        }],
        |b| format!("{}", b.len()),
    );
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(fields(&v), set(&["entries", "digest"]));
    assert_eq!(
        fields(&v["entries"][0]),
        set(&["kind", "source", "reference", "sha256", "bytes", "cut"])
    );
    assert_eq!(v["entries"][0]["kind"], "issue");
}

#[test]
fn origins_are_spec_092s_variants() {
    use speccoder_core::ladder::Origin;
    let bug = serde_json::to_value(Origin::Bug {
        unit: "u".to_owned(),
        requirement: "r".to_owned(),
        task: "t".to_owned(),
        triad_sha256: "d".to_owned(),
        by: "b".to_owned(),
        via: speccoder_core::actions::Via::Terminal,
        at: 0,
    })
    .unwrap();
    assert_eq!(
        fields(&bug),
        set(&[
            "kind",
            "unit",
            "requirement",
            "task",
            "triad_sha256",
            "by",
            "via",
            "at"
        ])
    );
    let issue = serde_json::to_value(Origin::Issue {
        number: 1,
        repo: None,
        context_sha256: None,
    })
    .unwrap();
    assert_eq!(
        fields(&issue),
        set(&["kind", "number", "repo", "context_sha256"])
    );
}

#[test]
fn inbox_kinds_and_event_names_follow_their_registries() {
    use speccoder_core::event::{Class, REGISTRY, well_formed};
    use speccoder_core::inbox::{Group, Kind};
    for (k, g) in [
        ("bug_confirm", Group::BlockedOnYou),
        ("bug_cannot_reproduce", Group::BlockedOnYou),
        ("bug_unchanged_not_holding", Group::BlockedOnYou),
        ("bug_repro_weak", Group::BlockedOnYou),
        ("bug_already_satisfied", Group::ReadyToReview),
    ] {
        assert_eq!(Kind::named(k).map(Kind::group), Some(g), "{k}");
    }
    let bug: Vec<_> = REGISTRY
        .iter()
        .filter(|n| n.name.starts_with("bug."))
        .collect();
    assert_eq!(bug.len(), 9);
    assert!(bug.iter().all(|n| well_formed(n.name)));
    let domain: BTreeSet<&str> = bug
        .iter()
        .filter(|n| n.class == Class::Domain)
        .map(|n| n.name)
        .collect();
    assert_eq!(
        domain,
        BTreeSet::from([
            "bug.recorded",
            "bug.confirmed",
            "bug.confirm_rejected",
            "bug.outcome"
        ])
    );
}
