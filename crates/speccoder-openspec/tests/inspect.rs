//! A change of the OpenSpec 1.14.3 fixtures, as speccoder sees it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use speccoder_core::snapshot::{DependencyKind, Severity, TaskState};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/1.14.3")
        .join(name)
}

fn codes(s: &speccoder_core::snapshot::ProjectSnapshot) -> Vec<(Severity, &str)> {
    s.diagnostics
        .iter()
        .map(|d| (d.severity, d.code.as_str()))
        .collect()
}

#[test]
fn the_sample_change_is_selected_with_its_chain_and_requirements() {
    let s = speccoder_openspec::inspect(&fixture("sample"), None).unwrap();
    assert!(
        s.diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{:?}",
        codes(&s)
    );
    assert_eq!(s.features.len(), 1, "the archive is not a change");
    let f = s.feature.as_ref().unwrap();
    assert_eq!(f.feature.name, "add-farewell-and-locale");
    let tasks = f.tasks.as_ref().unwrap();
    let ids: Vec<_> = tasks
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.done))
        .collect();
    assert_eq!(
        ids,
        [
            ("1.1", true),
            ("1.2", false),
            ("2.1", false),
            ("2.2", false)
        ]
    );
    assert_eq!(tasks.tasks[2].phase.as_deref(), Some("2. Farewell"));
    let sched = f.schedule.as_ref().unwrap();
    let st = |id: &str| sched.tasks.iter().find(|t| t.id == id).unwrap();
    assert_eq!(st("1.2").state, TaskState::Ready);
    assert_eq!(st("2.1").state, TaskState::Blocked);
    let deps: Vec<_> = st("2.2")
        .depends_on
        .iter()
        .map(|d| (d.id.as_str(), d.kind))
        .collect();
    assert!(
        deps.contains(&("2.1", DependencyKind::Structural)),
        "{deps:?}"
    );
    assert!(
        deps.contains(&("1.2", DependencyKind::Structural)),
        "{deps:?}"
    );

    let meta = s.openspec.as_ref().unwrap();
    assert_eq!(meta.schema, "spec-driven");
    let reqs: Vec<_> = meta
        .requirements
        .iter()
        .map(|r| (r.operation.as_str(), r.name.as_str(), r.scenarios.len()))
        .collect();
    assert_eq!(
        reqs,
        [
            ("added", "Farewell", 1),
            ("modified", "Default greeting", 2),
            ("renamed", "Greet by name", 0)
        ]
    );
}

#[test]
fn scenarios_carry_given_when_then() {
    let root = fixture("sample");
    let (c, diags) =
        speccoder_openspec::load_change(&root, "add-farewell-and-locale", "spec-driven");
    assert!(diags.is_empty(), "{diags:?}");
    let d = &c.deltas[0];
    assert_eq!(d.capability, "greeting");
    let configured = &d.requirements[1].scenarios[1];
    assert_eq!(configured.name, "Configured default");
    assert_eq!(configured.given, ["`GREET_DEFAULT=Grace`"]);
    assert_eq!(configured.then, ["the output is `Hello, Grace!`"]);
    assert_eq!(
        &d.text[configured.range[0]..configured.range[1]],
        configured.raw
    );
    assert!(
        configured
            .raw
            .starts_with("#### Scenario: Configured default")
    );
    assert_eq!(d.renames[0].from, "Greet a named person");
    assert_eq!(d.renames[0].to, "Greet by name");
}

#[test]
fn the_broken_example_is_refused_before_any_model_call() {
    let s = speccoder_openspec::inspect(&fixture("broken-example"), None).unwrap();
    let c = codes(&s);
    assert!(c.contains(&(Severity::Error, "missing_target")), "{c:?}");
    assert!(
        c.contains(&(Severity::Warning, "removed_without_reason")),
        "{c:?}"
    );
    assert!(c.contains(&(Severity::Warning, "purpose_missing")), "{c:?}");
    let missing = s
        .diagnostics
        .iter()
        .find(|d| d.code == "missing_target")
        .unwrap();
    assert!(
        missing
            .message
            .contains("greeting MODIFIED `Does not exist`"),
        "{}",
        missing.message
    );
    assert_eq!(missing.line, Some(19));
    assert!(c.contains(&(Severity::Error, "no_tasks")), "{c:?}");
}

#[test]
fn edge_cases_of_tasks_md_are_diagnosed() {
    let text = std::fs::read_to_string(fixture("edge-tasks/tasks.md")).unwrap();
    let (parsed, diags) = speccoder_openspec::tasks::parse("tasks.md", &text);
    let found: Vec<_> = diags
        .iter()
        .map(|d| (d.code.as_str(), d.line.unwrap_or(0)))
        .collect();
    for want in [
        ("marker_not_space", 6),
        ("empty_brackets", 7),
        ("textless", 8),
        ("misplaced_id", 11),
        ("duplicate_id", 12),
        ("in_fence", 15),
        ("extension_dependency", 20),
        ("extension_dependency", 21),
        ("unnumbered_section", 26),
    ] {
        assert!(found.contains(&want), "{want:?} not in {found:?}");
    }
    // OpenSpec counts every checkbox with text, fenced ones included.
    assert_eq!(parsed.totals.total, 12);
    let ids: Vec<&str> = parsed.tasks.iter().map(|t| t.id.as_str()).collect();
    let child = ids.iter().position(|i| *i == "1.4.1").unwrap();
    let parent = ids.iter().position(|i| *i == "1.4").unwrap();
    assert!(
        child < parent,
        "a nested task runs before its parent: {ids:?}"
    );
    assert!(parsed.tasks.iter().any(|t| t.id == "2.3" && t.done));
    assert!(
        ids.contains(&"#13"),
        "an unnumbered task gets its ordinal: {ids:?}"
    );
    let (_, graph) = speccoder_core::graph::schedule(&parsed);
    let g: Vec<_> = graph.iter().map(|d| d.code.as_str()).collect();
    assert!(
        g.contains(&"graph.dangling") && g.contains(&"graph.cycle"),
        "{g:?}"
    );
}

#[test]
fn two_active_changes_need_a_choice() {
    let dir = tempfile::tempdir().unwrap();
    for id in ["b-change", "a-change"] {
        let c = dir.path().join("openspec/changes").join(id);
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(c.join("tasks.md"), "## 1. X\n\n- [ ] 1.1 x\n").unwrap();
    }
    let s = speccoder_openspec::inspect(dir.path(), None).unwrap();
    assert!(s.feature.is_none());
    let d = s
        .diagnostics
        .iter()
        .find(|d| d.code == "select.ambiguous")
        .unwrap();
    assert_eq!(d.message, "2 active changes: a-change, b-change");
    let s = speccoder_openspec::inspect(dir.path(), Some("b-change")).unwrap();
    assert_eq!(s.feature.unwrap().feature.name, "b-change");
    let s = speccoder_openspec::inspect(dir.path(), Some("nope")).unwrap();
    assert!(s.diagnostics.iter().any(|d| d.code == "select.not-found"));
    assert!(
        speccoder_openspec::inspect(&std::env::temp_dir().join("no-such-dir-xyz"), None).is_err()
    );
}

#[test]
fn stores_custom_schemas_and_legacy_roots_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let o = dir.path().join("openspec");
    std::fs::create_dir_all(o.join("changes/x/specs/c")).unwrap();
    std::fs::write(o.join("changes/x/tasks.md"), "## 1. X\n\n- [ ] 1.1 x\n").unwrap();
    std::fs::write(
        o.join("changes/x/specs/c/spec.md"),
        "## ADDED Requirements\n\n### Requirement: R\nThe system SHALL r.\n\n#### Scenario: s\n- **WHEN** a\n- **THEN** b\n",
    )
    .unwrap();
    std::fs::write(
        o.join("config.yaml"),
        "schema: custom\nstore:\n  name: shared\n",
    )
    .unwrap();
    let s = speccoder_openspec::inspect(dir.path(), None).unwrap();
    let c = codes(&s);
    assert!(
        c.contains(&(Severity::Error, "unsupported_schema")),
        "{c:?}"
    );
    assert!(c.contains(&(Severity::Error, "store_root")), "{c:?}");
    std::fs::remove_file(o.join("config.yaml")).unwrap();
    std::fs::write(o.join("project.md"), "# legacy\n").unwrap();
    let s = speccoder_openspec::inspect(dir.path(), None).unwrap();
    assert!(codes(&s).contains(&(Severity::Error, "legacy_root")));
}
