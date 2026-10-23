//! Fixture and temp-dir tests for `inspect`.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use speccoder_core::snapshot::{
    ConstitutionState, DependencyKind, Diagnostic, ProjectSnapshot, SelectionReason, Severity,
    TaskState,
};
use speccoder_speckit::{NotAProject, inspect};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Every file under `dir` with its contents, to prove nothing changed.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                out.insert(p.clone(), fs::read(&p).unwrap());
            } else {
                out.insert(
                    p.clone(),
                    fs::read_link(&p)
                        .unwrap()
                        .into_os_string()
                        .into_encoded_bytes(),
                );
            }
        }
    }
    out
}

fn inspect_unchanged(dir: &Path, feature: Option<&str>) -> ProjectSnapshot {
    let before = tree(dir);
    let s = inspect(dir, feature).unwrap();
    assert_eq!(before, tree(dir), "inspect modified {}", dir.display());
    s
}

fn codes(s: &ProjectSnapshot) -> Vec<&str> {
    s.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn find<'s>(s: &'s ProjectSnapshot, code: &str) -> &'s Diagnostic {
    s.diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("no {code} in {:?}", codes(s)))
}

/// A minimal project in a temp dir with one feature whose tasks.md is `tasks`.
fn temp_project(tasks: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join(".specify/memory")).unwrap();
    fs::write(
        r.join(".specify/init-options.json"),
        r#"{"speckit_version":"1.1.3","integration":"claude","script":"sh"}"#,
    )
    .unwrap();
    fs::write(
        r.join(".specify/integration.json"),
        r#"{"integration":"claude"}"#,
    )
    .unwrap();
    fs::write(
        r.join(".specify/memory/constitution.md"),
        "# C\n\nFilled.\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("specs/001-x")).unwrap();
    fs::write(r.join("specs/001-x/tasks.md"), tasks).unwrap();
    dir
}

// ── US1 ────────────────────────────────────────────────────────────────────

#[test]
fn valid_project_snapshot() {
    let s = inspect_unchanged(&fixture("valid"), None);

    assert_eq!(s.speckit.version.as_deref(), Some("1.1.3"));
    assert_eq!(s.speckit.integration.as_deref(), Some("claude"));
    assert_eq!(s.speckit.script.as_deref(), Some("sh"));
    assert_eq!(s.speckit.feature_numbering.as_deref(), Some("sequential"));
    assert_eq!(s.constitution, ConstitutionState::Present);
    assert_eq!(s.selection.reason, SelectionReason::Recorded);
    assert_eq!(s.selection.feature.as_deref(), Some("001-user-auth"));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);

    let f = s.feature.as_ref().unwrap();
    let a = f.artifacts;
    assert!(a.spec && a.plan && a.tasks && a.data_model && a.contracts);
    assert!(!a.research && !a.quickstart && !a.checklists);

    let t = f.tasks.as_ref().unwrap();
    assert_eq!(t.path, "specs/001-user-auth/tasks.md");
    let phases: Vec<_> = t.phases.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(
        phases,
        [
            "Format: `[ID] [P?] [Story] Description`",
            "Phase 1: Setup",
            "Phase 2: Foundational",
            "Phase 3: User Story 1 - Log in with email (Priority: P1) 🎯 MVP",
            "Dependencies & Execution Order",
        ]
    );
    assert!(t.phases.iter().all(|p| p.section_hash.len() == 64));

    let rows: Vec<_> = t
        .tasks
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                t.done,
                t.parallel,
                t.story.as_deref(),
                t.phase.as_deref().unwrap_or(""),
                t.subsection.as_deref(),
                t.line,
            )
        })
        .collect();
    let us1 = "Phase 3: User Story 1 - Log in with email (Priority: P1) 🎯 MVP";
    assert_eq!(
        rows,
        [
            ("T001", true, false, None, "Phase 1: Setup", None, 22),
            ("T002", true, false, None, "Phase 1: Setup", None, 23),
            ("T003", false, true, None, "Phase 1: Setup", None, 24),
            (
                "T004",
                false,
                false,
                None,
                "Phase 2: Foundational",
                None,
                28
            ),
            (
                "T005",
                false,
                true,
                Some("US1"),
                us1,
                Some("Tests for User Story 1"),
                34
            ),
            (
                "T006",
                false,
                true,
                Some("US1"),
                us1,
                Some("Implementation for User Story 1"),
                38
            ),
            (
                "T007",
                false,
                false,
                Some("US1"),
                us1,
                Some("Implementation for User Story 1"),
                39
            ),
        ]
    );
    assert_eq!(
        t.tasks[6].title,
        "Implement AuthService in src/services/auth.rs (depends on T006)"
    );
    assert_eq!((t.totals.total, t.totals.done, t.totals.open), (7, 2, 5));

    // Byte ranges point at the item in the file.
    let text = fs::read_to_string(fixture("valid/specs/001-user-auth/tasks.md")).unwrap();
    let [start, _] = t.tasks[0].byte_range;
    assert!(text[start..].starts_with("- [x] T001 Create project structure"));
}

#[test]
fn inspect_works_from_a_subdirectory() {
    let s = inspect(&fixture("valid/specs/001-user-auth"), None).unwrap();
    assert_eq!(s.selection.feature.as_deref(), Some("001-user-auth"));
}

/// (id, state, blockers) of every task of the selected feature.
fn states(s: &ProjectSnapshot) -> Vec<(String, TaskState, Vec<String>)> {
    s.feature
        .as_ref()
        .unwrap()
        .schedule
        .as_ref()
        .unwrap()
        .tasks
        .iter()
        .map(|t| (t.id.clone(), t.state, t.blocked_by.clone()))
        .collect()
}

fn row(id: &str, state: TaskState, blockers: &[&str]) -> (String, TaskState, Vec<String>) {
    (
        id.to_owned(),
        state,
        blockers.iter().map(|b| (*b).to_owned()).collect(),
    )
}

#[test]
fn valid_project_schedule() {
    use TaskState::{Blocked, Done, Ready};
    let s = inspect(&fixture("valid"), None).unwrap();
    assert_eq!(
        states(&s),
        [
            row("T001", Done, &[]),
            row("T002", Done, &[]),
            row("T003", Ready, &[]),
            row("T004", Blocked, &["T003"]),
            row("T005", Blocked, &["T003", "T004"]),
            row("T006", Blocked, &["T003", "T004", "T005"]),
            row("T007", Blocked, &["T003", "T004", "T005", "T006"]),
        ]
    );
    let t7 = &s.feature.as_ref().unwrap().tasks.as_ref().unwrap().tasks[6];
    assert_eq!(t7.depends_on, ["T006"]);
}

#[test]
fn graph_fixture_ready_set_and_blockers() {
    use TaskState::{Blocked, Done, Ready};
    let s = inspect_unchanged(&fixture("graph"), None);
    assert!(s.diagnostics.is_empty(), "{:?}", codes(&s));
    assert_eq!(
        states(&s),
        [
            row("T001", Done, &[]),
            row("T002", Done, &[]),
            row("T003", Done, &[]),
            row("T004", Done, &[]),
            // Both stories start as soon as setup and foundational are done.
            row("T005", Ready, &[]),
            row("T006", Blocked, &["T005"]),
            row("T007", Blocked, &["T005"]),
            row("T008", Blocked, &["T005", "T006", "T007"]),
            row("T009", Ready, &[]),
            // Explicit cross-story dependency on T008, plus the [P] join on T009.
            row("T010", Blocked, &["T008", "T009"]),
            row(
                "T011",
                Blocked,
                &["T005", "T006", "T007", "T008", "T009", "T010"]
            ),
        ]
    );
    let sched = s.feature.as_ref().unwrap().schedule.as_ref().unwrap();
    assert_eq!(
        (sched.totals.done, sched.totals.ready, sched.totals.blocked),
        (4, 2, 5)
    );
    let t10 = &sched.tasks[9];
    let explicit: Vec<_> = t10
        .depends_on
        .iter()
        .filter(|d| d.kind == DependencyKind::Explicit)
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(explicit, ["T008"]);
}

#[test]
fn graph_errors_are_reported_and_block() {
    use TaskState::Blocked;
    let s = inspect_unchanged(&fixture("graph-errors"), None);
    let graph: Vec<_> = s
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("graph."))
        .map(|d| (d.code.as_str(), d.line, d.message.as_str()))
        .collect();
    assert_eq!(
        graph,
        [
            (
                "graph.dangling",
                Some(5),
                "T001 depends on T099, which does not exist"
            ),
            ("graph.cycle", Some(6), "dependency cycle: T002 → T003"),
            ("graph.cycle", Some(8), "dependency cycle: T004"),
        ]
    );
    assert!(s.has_errors());
    assert!(states(&s).iter().all(|(_, state, _)| *state == Blocked));
    assert_eq!(states(&s)[0], row("T001", Blocked, &["T099"]));
}

// ── US2 ────────────────────────────────────────────────────────────────────

#[test]
fn malformed_tasks_are_diagnosed() {
    let s = inspect_unchanged(&fixture("malformed"), None);
    assert_eq!(s.selection.reason, SelectionReason::OnlyFeature);
    let dup = find(&s, "tasks.duplicate-id");
    assert_eq!(dup.severity, Severity::Error);
    assert_eq!(dup.message, "T002 appears on lines 7 and 11");
    assert_eq!(find(&s, "tasks.missing-id").line, Some(8));
    assert_eq!(find(&s, "tasks.out-of-order").message, "T003 follows T004");
    assert_eq!(find(&s, "tasks.before-phase").line, Some(3));
    assert!(s.has_errors());
    assert_eq!(s.feature.unwrap().tasks.unwrap().totals.total, 5);
}

#[test]
fn minimal_project_reports_absent_artifacts_and_template_constitution() {
    let s = inspect_unchanged(&fixture("minimal"), None);
    assert_eq!(s.constitution, ConstitutionState::Unfilled);
    assert_eq!(
        find(&s, "discover.metadata-missing").path.as_deref(),
        Some(".specify/integration.json")
    );
    find(&s, "tasks.missing");
    assert!(!s.has_errors(), "{:?}", codes(&s));
    let a = s.feature.unwrap().artifacts;
    assert!(a.spec && !a.plan && !a.tasks && !a.contracts);
}

#[test]
fn feature_without_tasks_suggests_the_tasks_step() {
    let s = inspect_unchanged(&fixture("no-tasks"), None);
    let d = find(&s, "tasks.missing");
    assert_eq!(d.severity, Severity::Warning);
    assert!(d.hint.as_deref().unwrap().contains("/speckit-tasks"));
    assert!(s.feature.unwrap().tasks.is_none());
}

#[test]
fn empty_non_utf8_and_oversized_task_lists() {
    let empty = temp_project(b"# Tasks\n\nNothing yet.\n");
    assert!(codes(&inspect(empty.path(), None).unwrap()).contains(&"tasks.empty"));

    let latin1 = temp_project(b"## Phase 1\n- [ ] T001 caf\xe9\n");
    let s = inspect(latin1.path(), None).unwrap();
    assert_eq!(find(&s, "discover.not-utf8").severity, Severity::Error);
    assert!(s.feature.unwrap().tasks.is_none());

    let big = temp_project(&vec![b'x'; 5 * 1024 * 1024]);
    let s = inspect(big.path(), None).unwrap();
    assert_eq!(find(&s, "discover.too-large").severity, Severity::Error);
}

#[test]
fn crlf_task_list_parses_with_exact_offsets() {
    let text = "## Phase 1: Setup\r\n\r\n- [ ] T001 First\r\n- [x] T002 [P] Second\r\n";
    let dir = temp_project(text.as_bytes());
    let s = inspect(dir.path(), None).unwrap();
    let t = s.feature.unwrap().tasks.unwrap();
    assert_eq!(t.tasks[1].title, "Second");
    assert_eq!(t.tasks[1].line, 4);
    let [start, _] = t.tasks[1].byte_range;
    assert!(text[start..].starts_with("- [x] T002"));
}

// ── US3 ────────────────────────────────────────────────────────────────────

#[test]
fn several_features_are_ordered_and_never_guessed() {
    let s = inspect_unchanged(&fixture("multi"), None);
    let names: Vec<_> = s.features.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["001-a", "002-b", "010-c"]);
    assert_eq!(s.selection.reason, SelectionReason::None);
    assert!(s.feature.is_none());
    assert_eq!(find(&s, "select.ambiguous").severity, Severity::Error);
    assert_eq!(find(&s, "discover.not-a-feature").severity, Severity::Info);
}

#[test]
fn feature_argument_by_name_prefix_or_number() {
    for arg in ["002-b", "002", "2"] {
        let s = inspect(&fixture("multi"), Some(arg)).unwrap();
        assert_eq!(s.selection.reason, SelectionReason::Argument, "{arg}");
        assert_eq!(s.selection.feature.as_deref(), Some("002-b"), "{arg}");
    }
    let s = inspect(&fixture("multi"), Some("999")).unwrap();
    assert!(
        find(&s, "select.not-found")
            .hint
            .as_deref()
            .unwrap()
            .contains("001-a, 002-b, 010-c")
    );
}

// ── US4 ────────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn symlinks_out_of_the_root_are_refused_and_never_read() {
    use std::os::unix::fs::symlink;

    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("tasks.md"),
        "## P\n- [ ] T666 SENTINEL\n",
    )
    .unwrap();
    fs::create_dir(outside.path().join("002-evil")).unwrap();
    fs::write(
        outside.path().join("002-evil/tasks.md"),
        "## P\n- [ ] T667 SENTINEL\n",
    )
    .unwrap();

    // tasks.md of the only real feature points outside.
    let dir = temp_project(b"");
    let tasks = dir.path().join("specs/001-x/tasks.md");
    fs::remove_file(&tasks).unwrap();
    symlink(outside.path().join("tasks.md"), &tasks).unwrap();
    // A whole feature directory points outside.
    symlink(
        outside.path().join("002-evil"),
        dir.path().join("specs/002-evil"),
    )
    .unwrap();

    let s = inspect_unchanged(dir.path(), Some("001"));
    let escapes: Vec<_> = s
        .diagnostics
        .iter()
        .filter(|d| d.code == "discover.escape")
        .map(|d| d.path.as_deref().unwrap())
        .collect();
    assert_eq!(escapes, ["specs/002-evil", "specs/001-x/tasks.md"]);
    assert_eq!(s.features.len(), 1);
    assert!(s.feature.as_ref().unwrap().tasks.is_none());
    let json = format!("{s:?}");
    assert!(!json.contains("SENTINEL"));
}

#[test]
fn recorded_feature_outside_the_root_is_refused() {
    let dir = temp_project(b"## P\n- [ ] T001 a\n");
    fs::write(
        dir.path().join(".specify/feature.json"),
        r#"{"feature_directory":"../../outside"}"#,
    )
    .unwrap();
    let s = inspect_unchanged(dir.path(), None);
    assert_eq!(find(&s, "select.not-found").severity, Severity::Error);
    assert!(s.feature.is_none());
}

#[test]
fn a_directory_without_spec_kit_is_not_a_project() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(inspect(dir.path(), None), Err(NotAProject));
}

// ── Polish ─────────────────────────────────────────────────────────────────

#[test]
fn a_thousand_tasks_inspect_in_under_a_second() {
    let mut text = String::from("# Tasks\n\n## Phase 1: Setup\n\n");
    for i in 1..=1000 {
        text.push_str(&format!(
            "- [ ] T{i:04} [P] [US1] Task number {i} in src/module_{i}.rs\n"
        ));
    }
    let dir = temp_project(text.as_bytes());
    let start = Instant::now();
    let s = inspect(dir.path(), None).unwrap();
    let elapsed = start.elapsed();
    let f = s.feature.unwrap();
    assert_eq!(f.tasks.unwrap().totals.total, 1000);
    // One [P] run behind no serial task: all 1,000 are ready at once.
    assert_eq!(f.schedule.unwrap().totals.ready, 1000);
    assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
}

// ── the engine's tick ───────────────────────────────────────────────

#[test]
fn ticking_changes_exactly_one_byte_and_checks_the_line() {
    let text = "## Phase 1\n- [x] T001 a\n- [ ] T002 [P] b\n- [ ] T003 c\n";
    let dir = temp_project(text.as_bytes());
    let path = dir.path().join("specs/001-x/tasks.md");
    let s = inspect(dir.path(), None).unwrap();
    let t2 = s.feature.unwrap().tasks.unwrap().tasks[1].clone();

    assert!(
        speccoder_speckit::mark_task_complete(&path, "T002", "wrong-hash")
            .unwrap_err()
            .contains("changed")
    );
    speccoder_speckit::mark_task_complete(&path, "T002", &t2.line_hash).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        "## Phase 1\n- [x] T001 a\n- [X] T002 [P] b\n- [ ] T003 c\n"
    );
    let diff = text
        .bytes()
        .zip(after.bytes())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(diff, 1);
    assert!(
        speccoder_speckit::mark_task_complete(&path, "T002", &t2.line_hash)
            .unwrap_err()
            .contains("already")
    );
    assert!(speccoder_speckit::mark_task_complete(&path, "T009", "x").is_err());
}

// ── the engine's convergence append ─────────────────────────────────

#[test]
fn convergence_appends_a_numbered_phase_and_keeps_every_old_byte() {
    let text = "## Phase 1: Setup\n- [x] T001 a\n\n## Phase 2: User Story 1 - Login\n- [X] T009 b";
    let dir = temp_project(text.as_bytes());
    let path = dir.path().join("specs/001-x/tasks.md");
    let hash = speccoder_speckit::sha256_hex(text.as_bytes());

    assert!(
        speccoder_speckit::append_convergence(&path, "stale", &["x".to_owned()])
            .unwrap_err()
            .contains("changed")
    );
    // Nothing to append: not a byte changes, no empty header.
    assert!(
        speccoder_speckit::append_convergence(&path, &hash, &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), text);

    let ids = speccoder_speckit::append_convergence(
        &path,
        &hash,
        &[
            "Reject empty input per FR-002 (missing)".to_owned(),
            "- [ ] Handle\nnewlines per US1/AC2 (partial)".to_owned(),
        ],
    )
    .unwrap();
    assert_eq!(ids, ["T010", "T011"]);
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.starts_with(text));
    assert_eq!(
        &after[text.len()..],
        "\n\n## Phase 3: Convergence\n\n- [ ] T010 Reject empty input per FR-002 (missing)\n- [ ] T011 Handle newlines per US1/AC2 (partial)\n"
    );
    // The appended tasks wait for every story phase and are ready now.
    let s = inspect(dir.path(), None).unwrap();
    let sched = s.feature.unwrap().schedule.unwrap();
    let t10 = sched.tasks.iter().find(|t| t.id == "T010").unwrap();
    assert_eq!(t10.state, speccoder_core::snapshot::TaskState::Ready);
}
