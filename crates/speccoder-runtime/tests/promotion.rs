//! Promotion: a quick change that grew is offered as a spec of its own,
//! on recorded facts only; a decline holds until another signal holds.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::ladder;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=ada@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const UNIT: &str = "002-quick-changes";

/// A project with one quick change, recorded and committed.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for (p, t) in [
        (".specify/memory/constitution.md", "# Constitution\n"),
        (".specify/init-options.json", "{}\n"),
        (
            "specs/001-login/spec.md",
            "# Spec\n\n- **FR-001**: Users MUST log in.\n",
        ),
        (
            "specs/001-login/tasks.md",
            "# Tasks\n\n## Phase 1: Core\n\n- [ ] T001 Log in per FR-001\n",
        ),
    ] {
        fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
        fs::write(r.join(p), t).unwrap();
    }
    git(r, &["init", "-q", "-b", "main"]);
    // speccoder commits as the repository's person: one is configured
    // here, not left to the machine's global settings.
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    let res = speccoder_runtime::quick::change(
        r,
        "Log every sign-in",
        &[],
        false,
        speccoder_core::actions::Via::Terminal,
        &speccoder_runtime::RunOptions::default(),
        None,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    dir
}

/// The task's verdict commit on the unit's branch, touching `files`.
fn verdict(r: &Path, files: &[&str]) {
    git(r, &["checkout", "-q", "-B", &format!("speccoder/{UNIT}")]);
    for f in files {
        fs::create_dir_all(r.join(f).parent().unwrap()).unwrap();
        fs::write(r.join(f), "x\n").unwrap();
    }
    let tasks = r.join(format!("specs/{UNIT}/tasks.md"));
    let t = fs::read_to_string(&tasks)
        .unwrap()
        .replace("- [ ] T001", "- [x] T001");
    fs::write(&tasks, t).unwrap();
    git(r, &["add", "-A"]);
    git(
        r,
        &[
            "commit",
            "-qm",
            &format!("speccoder: T001\n\nSpec-Task: {UNIT}/T001\nVerdict: verified\n"),
        ],
    );
    git(r, &["checkout", "-q", "main"]);
}

fn many(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("src/m{i}.rs")).collect()
}

#[test]
fn a_change_that_grew_is_offered_once_on_its_signals() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    let files = many(11);
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    verdict(r.path(), &refs);

    let offers = ladder::offers(r.path(), &plane, UNIT);
    assert_eq!(offers.len(), 1, "{offers:?}");
    let o = &offers[0];
    assert_eq!(o.task, "T001");
    assert_eq!(
        o.requirement,
        Some(("FR-001".to_owned(), "Log every sign-in".to_owned()))
    );
    let s1 = &o.signals[0];
    assert_eq!((s1.id.as_str(), s1.value, s1.threshold), ("S1", 11, 8));

    // Needs You lists it, ready to review, with Promote and Not now.
    let inbox = speccoder_runtime::inbox::inbox_with(r.path(), Some(UNIT), Some(state.path()));
    let Some(CommandData::Inbox(i)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    let item = i
        .items
        .iter()
        .find(|i| i.kind == "promote")
        .expect("an offer");
    assert_eq!(item.group, "ready_to_review");
    assert!(
        item.detail.contains("11 files changed (more than 8)"),
        "{}",
        item.detail
    );
    let commands: Vec<&str> = item.actions.iter().map(|a| a.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            "speccoder spec promote 002-quick-changes/T001",
            "speccoder spec promote 002-quick-changes/T001 --decline"
        ]
    );

    // Declined: gone, until a signal that did not hold holds.
    let d = speccoder_runtime::actions::perform_in(
        r.path(),
        None,
        speccoder_runtime::actions::Dispatch::new(
            &speccoder_core::actions::Runnable::DeclinePromotion {
                task: format!("{UNIT}/T001"),
            },
            speccoder_core::actions::Via::Terminal,
        ),
        Some(state.path()),
    );
    assert_eq!(d.status(), CommandStatus::Ok, "{}", d.message());
    assert!(ladder::offers(r.path(), &plane, UNIT).is_empty());
    let mut more = files.clone();
    more.push("speccoder.toml".to_owned());
    let refs: Vec<&str> = more.iter().map(String::as_str).collect();
    verdict(r.path(), &refs);
    let back = ladder::offers(r.path(), &plane, UNIT);
    let ids: Vec<&str> = back[0].signals.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["S1", "S4"]);
}

#[test]
fn a_small_change_is_offered_nothing() {
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let plane = speccoder_runtime::worktree::project_state_dir(state.path(), r.path());
    verdict(r.path(), &["src/one.rs", "src/two.rs"]);
    assert!(ladder::offers(r.path(), &plane, UNIT).is_empty());
    // A feature is never offered promotion, whatever its size.
    assert!(ladder::offers(r.path(), &plane, "001-login").is_empty());
}

/// The promoted unit's trace shows the quick change's tick as inherited —
/// proven against an earlier wording — and counts nothing verified.
#[test]
fn a_promoted_unit_inherits_the_tick_never_as_verified() {
    let r = repo();
    // The isolated state directory every test process gets (nextest's wrapper).
    let state = speccoder_runtime::worktree::state_dir().expect("a state directory");
    let plane = speccoder_runtime::worktree::project_state_dir(&state, r.path());
    verdict(r.path(), &["src/a.rs"]);
    for (p, t) in [
        (
            "specs/003-sign-in-log/spec.md",
            "# Spec\n\n- **FR-001**: Every sign-in MUST be logged.\n",
        ),
        (
            "specs/003-sign-in-log/tasks.md",
            "# Tasks\n\n## Phase 1: Core\n\n- [ ] T001 Log sign-ins per FR-001\n",
        ),
    ] {
        fs::create_dir_all(r.path().join(p).parent().unwrap()).unwrap();
        fs::write(r.path().join(p), t).unwrap();
    }
    git(r.path(), &["add", "-A"]);
    git(r.path(), &["commit", "-qm", "the promoted spec"]);
    ladder::record_origin(
        &plane,
        "003-sign-in-log",
        vec![speccoder_core::ladder::Origin::PromotedFrom {
            from_unit: UNIT.to_owned(),
            requirement: "FR-001".to_owned(),
            task: "T001".to_owned(),
            requirement_sha256: "d".to_owned(),
            conversations: Vec::new(),
            signals: vec!["S1".to_owned()],
            by: "Ada".to_owned(),
            via: speccoder_core::actions::Via::Terminal,
            at: 1,
        }],
    )
    .unwrap();
    let report = speccoder_runtime::status::report(r.path(), Some("003-sign-in-log"));
    let Some(CommandData::Report(s)) = report.data() else {
        panic!("{}", report.message())
    };
    let inherited: Vec<_> = s.trace.iter().filter(|t| t.inherited.is_some()).collect();
    assert_eq!(inherited.len(), 1, "{:?}", s.trace);
    assert_eq!(inherited[0].task.as_deref(), Some("002-quick-changes/T001"));
    assert!(
        inherited[0].changed,
        "inherited is proven against an earlier wording"
    );
    assert_eq!(s.totals.verified, 0);
}

#[derive(Debug, Default)]
struct Heard(std::sync::Mutex<Vec<speccoder_runtime::progress::Event>>);

impl speccoder_runtime::progress::Progress for Heard {
    fn event(&self, event: &speccoder_runtime::progress::Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

/// Once plan through made the unit: the origin is written once and
/// `unit.promoted` says so with ids and digests, never the words.
#[test]
fn a_promotion_is_said_once_with_digests_only() {
    use speccoder_runtime::progress::Event;
    let r = repo();
    let state = tempfile::tempdir().unwrap();
    let files = many(11);
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    verdict(r.path(), &refs);
    let made = speccoder_core::CommandResult::new("plan", CommandStatus::Ok, "planned").with_data(
        CommandData::PlanningChain(Box::new(speccoder_core::PlanningChainReport {
            unit: Some("003-sign-in-log".to_owned()),
            through: speccoder_core::actions::Through::Tasks,
            ran: Vec::new(),
            stop: speccoder_core::ChainStop::Done {
                plan_checks: Vec::new(),
            },
            readiness: None,
            similar: Vec::new(),
            source: speccoder_core::actions::FeatureSource::Description,
            next_skill: None,
        })),
    );
    let heard = std::sync::Arc::new(Heard::default());
    let sink: std::sync::Arc<dyn speccoder_runtime::progress::Progress> = heard.clone();
    let promote = || {
        ladder::promoted(
            r.path(),
            state.path(),
            &made,
            &format!("{UNIT}/T001"),
            &[],
            speccoder_core::actions::Via::Terminal,
            Some(&sink),
        );
    };
    promote();
    promote();
    let said = heard.0.lock().unwrap();
    assert_eq!(said.len(), 1, "written once, said once: {said:?}");
    let Event::UnitPromoted {
        unit,
        from_unit,
        requirement,
        task,
        requirement_sha256,
        signals,
        ..
    } = &said[0]
    else {
        panic!("{said:?}")
    };
    assert_eq!(
        (
            unit.as_str(),
            from_unit.as_str(),
            requirement.as_str(),
            task.as_str()
        ),
        ("003-sign-in-log", UNIT, "FR-001", "T001")
    );
    assert_eq!(requirement_sha256.len(), 64);
    assert_eq!(signals, &["S1"]);
}

/// Promoted while still open: skipping it in its old unit is offered first,
/// saying where it goes on — and only as the person's explicit step.
#[test]
fn a_promoted_open_task_offers_its_skip_first() {
    let r = repo();
    let state = speccoder_runtime::worktree::state_dir().expect("a state directory");
    let plane = speccoder_runtime::worktree::project_state_dir(&state, r.path());
    let before = speccoder_runtime::actions::available(r.path(), Some(UNIT), Some("T001"));
    let first = |a: &[speccoder_core::actions::Action]| {
        a.iter()
            .find(|a| a.group == "task")
            .map(|a| (a.label.clone(), a.why.clone()))
    };
    assert_eq!(
        first(&before).map(|(l, _)| l),
        Some("accept T001".to_owned())
    );
    ladder::record_origin(
        &plane,
        "003-sign-in-log",
        vec![speccoder_core::ladder::Origin::PromotedFrom {
            from_unit: UNIT.to_owned(),
            requirement: "FR-001".to_owned(),
            task: "T001".to_owned(),
            requirement_sha256: "d".to_owned(),
            conversations: Vec::new(),
            signals: vec!["S2".to_owned()],
            by: "Ada".to_owned(),
            via: speccoder_core::actions::Via::Web,
            at: 1,
        }],
    )
    .unwrap();
    let after = speccoder_runtime::actions::available(r.path(), Some(UNIT), Some("T001"));
    let (label, why) = first(&after).unwrap();
    assert_eq!(label, "skip T001");
    assert!(why.contains("goes on as 003-sign-in-log"), "{why}");
    // Nothing skips it on its own.
    let report = speccoder_runtime::status::report(r.path(), Some(UNIT));
    let Some(CommandData::Report(s)) = report.data() else {
        panic!("{}", report.message())
    };
    assert_ne!(s.tasks[0].state, "skipped");
}
