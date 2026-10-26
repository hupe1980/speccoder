// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use speccoder_core::{PendingCall, StatusTotals, TaskStatus};
use speccoder_runtime::approvals::{Decision, PresentContext, Request, Scope, options, present};

use crate::style::{Look, Theme};

fn task(id: &str, state: &str) -> TaskStatus {
    TaskStatus {
        id: id.to_owned(),
        title: format!("title of {id}"),
        phase: "Phase 2".to_owned(),
        state: state.to_owned(),
        drift: None,
        blocked_by: Vec::new(),
        commit: None,
        run_id: None,
        last_attempt: None,
        set_aside: None,
        note: None,
        attestation: None,
        strength: None,
        bug: None,
    }
}

fn snapshot() -> Snapshot {
    let mut verified = task("T001", "verified");
    verified.commit = Some("0123456789abcdef".to_owned());
    let mut blocked = task("T003", "blocked");
    blocked.blocked_by = vec!["T002".to_owned()];
    Snapshot {
        report: Some(StatusReport {
            review: None,
            issues: Vec::new(),
            feature: "001-x".to_owned(),
            branch: Some("speccoder/001-x".to_owned()),
            source: "branch".to_owned(),
            selected_by: String::new(),
            artifacts: Vec::new(),
            planning: Vec::new(),
            uncommitted: Vec::new(),
            journey: Vec::new(),
            unmerged_commits: 1,
            tasks: vec![verified, task("T002", "ready"), blocked],
            convergence: Vec::new(),
            trace: vec![speccoder_core::TraceRow {
                requirement: "FR-009".to_owned(),
                scenario: None,
                test: None,
                task: None,
                verdict: None,
                changed: false,
            }],
            totals: StatusTotals {
                verified: 1,
                ready: 1,
                blocked: 1,
                ..StatusTotals::default()
            },
        }),
        live: Some((
            LiveState {
                feature: "001-x".to_owned(),
                task: "T002".to_owned(),
                stage: Some(Stage::Attempt),
                stage_detail: Some("1/2".to_owned()),
                recent: vec!["turn 1  read_file src/lib.rs (allow)".to_owned()],
                input_tokens: 1200,
                ..LiveState::default()
            },
            true,
        )),
        approvals: Some(ApprovalsReport {
            feature: "001-x".to_owned(),
            pending: vec![PendingCall {
                task: "T002".to_owned(),
                covers: "write `package.json`".to_owned(),
                tool: "write_file".to_owned(),
                arguments: json!({}),
                reason: "a dependency manifest".to_owned(),
                reference: "refs/x".to_owned(),
                presentation: Some(speccoder_core::ApprovalView {
                    kind: "write".to_owned(),
                    title: "Write this file?".to_owned(),
                    subject: "package.json".to_owned(),
                    body_lines: vec!["    1 │ {}".to_owned()],
                    sandbox: "a file write — nothing is executed".to_owned(),
                    claim: None,
                    risk: "medium".to_owned(),
                    scope: "for the rest of T002: this file, any content".to_owned(),
                    object: vec!["file  package.json".to_owned()],
                    sources: Vec::new(),
                    options: Vec::new(),
                }),
                ..PendingCall::default()
            }],
            decided: Vec::new(),
            unreadable: None,
            batch: Vec::new(),
        }),
        problem: None,
        next: Some(speccoder_core::next::NextStep {
            command: "speccoder task approve T002".to_owned(),
            why: "allow it; the attempt resumes on the next run".to_owned(),
        }),
        in_place: Some("in-place · speccoder/001-x (returns to main)".to_owned()),
        primary: None,
    }
}

/// A call to edit a manifest, as the terminal prompt would show it.
fn call() -> Presentation {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    present(
        &Request {
            task: "001-x/T002".to_owned(),
            task_title: "Handle Unicode input".to_owned(),
            turn: 8,
            tool: "edit_file".to_owned(),
            arguments: json!({ "path": "Cargo.toml", "old_string": "serde = \"1\"\n", "new_string": "serde = \"1\"\nunicode-normalization = \"0.1\"\n" }),
            kind: "edit".to_owned(),
            reason: "`Cargo.toml` is a dependency manifest".to_owned(),
            object: Some(speccoder_runtime::tools::policy::Object::Path {
                canonical: "Cargo.toml".to_owned(),
                asked: "Cargo.toml".to_owned(),
            }),
            risk: "medium".to_owned(),
            claim: Some("NFKD is needed".to_owned()),
            options: options("medium", true, false, "edit", "medium"),
            ..Request::default()
        },
        &PresentContext {
            root: Some(dir.path().to_path_buf()),
            sandbox: Some("Seatbelt".to_owned()),
            network: "none".to_owned(),
            file_text: None,
        },
    )
}

fn who() -> Who {
    Who {
        name: "Dana".to_owned(),
        via: "terminal".to_owned(),
        client: None,
        basis: "asserted".to_owned(),
    }
}

fn screen_at(app: &App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw::draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn screen(app: &App) -> String {
    screen_at(app, 140, 30)
}

fn hosted() -> App {
    let mut app = App::hosting(snapshot(), who());
    app.apply(&Event::TaskStarted {
        feature: "001-x".to_owned(),
        task: "T002".to_owned(),
        title: "title of T002".to_owned(),
    });
    for (stage, state, ms) in [
        (Stage::Baseline, StageState::Passed, Some(1200)),
        (Stage::Red, StageState::Skipped, None),
        (Stage::Attempt, StageState::Started, None),
    ] {
        app.apply(&Event::Stage {
            task: "T002".to_owned(),
            stage,
            state,
            detail: (stage == Stage::Attempt).then(|| "1/2".to_owned()),
            elapsed_ms: ms,
            tail: Vec::new(),
        });
    }
    app.apply(&Event::ToolCall {
        task: "T002".to_owned(),
        turn: 1,
        tool: "read_file".to_owned(),
        target: "src/lib.rs".to_owned(),
        decision: "allow".to_owned(),
        ok: Some(true),
        tail: Vec::new(),
    });
    app.apply(&Event::Usage {
        task: "T002".to_owned(),
        input_tokens: 900,
        output_tokens: 40,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        usd: Some(0.0012),
    });
    app
}

#[test]
fn the_observer_shows_tasks_the_live_run_and_the_queue() {
    let s = screen(&App::new(snapshot()));
    for needle in [
        "✓ T001",
        "◐ T002",
        "⧗ T003",
        "running  001-x T002 — attempt 1/2",
        "◐ attempt",
        "turn 1  read_file src/lib.rs (allow)",
        "1200 in · 0 out",
        "Approvals (1 waiting, 0 decided)",
        "△ T002 write `package.json` — a dependency manifest",
        "Write this file?  package.json   risk ■■□ medium",
        "1 │ {}",
        "a → for the rest of T002: this file, any content",
        "1 verified",
        "Next: speccoder task approve T002 — allow it",
        "in-place · speccoder/001-x (returns to main)",
        "1 Run",
        "2 Evidence",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
}

#[test]
fn the_queue_approves_and_rejects_with_a_reason() {
    let mut app = App::new(snapshot());
    assert_eq!(app.key(KeyCode::Down), Action::None);
    assert_eq!(app.task, 1);
    // `a` only acts in the approval queue.
    assert_eq!(app.key(KeyCode::Char('a')), Action::None);
    app.key(KeyCode::Tab);
    assert_eq!(
        app.key(KeyCode::Char('a')),
        Action::Approve("T002".to_owned())
    );
    app.key(KeyCode::Char('r'));
    for c in "no deps".chars() {
        app.key(KeyCode::Char(c));
    }
    assert!(screen(&app).contains("reason (the model is told"));
    assert_eq!(
        app.key(KeyCode::Enter),
        Action::Reject("T002".to_owned(), "no deps".to_owned())
    );
    // An empty reason is refused; esc cancels typing.
    app.key(KeyCode::Char('r'));
    assert_eq!(app.key(KeyCode::Enter), Action::None);
    assert!(
        app.flash
            .as_deref()
            .unwrap_or_default()
            .contains("needs a reason")
    );
    app.key(KeyCode::Char('r'));
    app.key(KeyCode::Esc);
    assert!(app.typing.is_none());
    // Watching, nothing to stop: q leaves.
    assert_eq!(app.key(KeyCode::Char('q')), Action::Quit);
}

#[test]
fn a_hosted_run_shows_its_pipeline_calls_and_usage() {
    let s = screen(&hosted());
    for needle in [
        "T002 title of T002",
        "✓ baseline",
        "– red",
        "◐ attempt",
        "· verify",
        "baseline 1.2s · red · attempt 1/2…",
        "turn 1  read_file src/lib.rs (allow)",
        "900 in · 40 out · $0.0012",
        "◐ T002",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
}

#[test]
fn the_approval_overlay_answers_with_the_prompts_keys() {
    let mut app = hosted();
    app.ask(Ask::Call(Box::new(call())));
    let s = screen(&app);
    for needle in [
        "△ Approval needed — 001-x/T002 Handle Unicode input · turn 8",
        "Edit this file?  Cargo.toml",
        "risk ■■□ medium",
        "+ unicode-normalization",
        "Why stopped  `Cargo.toml` is a dependency manifest",
        "Sandbox",
        "\"NFKD is needed\"   (a claim)",
        "1. Yes, once",
        "(s)",
        "tab note · v view all · esc later",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
    // A note, then "for the task".
    app.key(KeyCode::Tab);
    for c in "keep it small".chars() {
        app.key(KeyCode::Char(c));
    }
    app.key(KeyCode::Enter);
    assert!(screen(&app).contains("note for the model: keep it small"));
    match app.key(KeyCode::Char('t')) {
        Action::Answer(a) => {
            assert_eq!(a.decision, Decision::Approve);
            assert_eq!(a.scope, Scope::Task);
            assert_eq!(a.note.as_deref(), Some("keep it small"));
            assert_eq!(a.by.as_ref().map(|w| w.via.as_str()), Some("terminal"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(app.overlay, Overlay::None);
    // Numbers mean the same: 1 is "yes, once".
    app.ask(Ask::Call(Box::new(call())));
    assert!(matches!(app.key(KeyCode::Char('1')), Action::Answer(a) if a.scope == Scope::Once));
    // n asks for a reason, which is required.
    app.ask(Ask::Call(Box::new(call())));
    assert_eq!(app.key(KeyCode::Char('n')), Action::None);
    assert_eq!(app.key(KeyCode::Enter), Action::None);
    assert!(
        app.flash
            .as_deref()
            .unwrap_or_default()
            .contains("needs a reason")
    );
    for c in "no new deps".chars() {
        app.key(KeyCode::Char(c));
    }
    match app.key(KeyCode::Enter) {
        Action::Answer(a) => {
            assert_eq!(a.decision, Decision::Reject);
            assert_eq!(a.reason.as_deref(), Some("no new deps"));
        }
        other => panic!("{other:?}"),
    }
    // esc is later: the call keeps waiting, enter brings it back, s stops.
    app.ask(Ask::Call(Box::new(call())));
    assert_eq!(app.key(KeyCode::Esc), Action::None);
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.hosted.as_ref().is_some_and(|h| h.ask.is_some()));
    assert!(screen(&app).contains("a call waits for you — enter to answer"));
    app.key(KeyCode::Enter);
    assert_eq!(app.overlay, Overlay::Ask);
    assert!(
        matches!(app.key(KeyCode::Char('s')), Action::Answer(a) if a.decision == Decision::Stop)
    );
}

#[test]
fn a_high_risk_call_offers_no_run_scope_and_v_shows_everything() {
    let mut app = hosted();
    let mut p = call();
    p.request.options = options("high", true, false, "edit", "high");
    app.ask(Ask::Call(Box::new(p)));
    assert_eq!(app.key(KeyCode::Char('r')), Action::None);
    assert!(
        app.flash
            .as_deref()
            .unwrap_or_default()
            .contains("not offered")
    );
    app.key(KeyCode::Char('v'));
    assert!(app.view_all);
}

#[test]
fn a_question_is_typed_and_sent() {
    let mut app = hosted();
    app.ask(Ask::Question("Which database?".to_owned()));
    assert!(screen(&app).contains("Which database?"));
    for c in "sqlite".chars() {
        app.key(KeyCode::Char(c));
    }
    assert_eq!(
        app.key(KeyCode::Enter),
        Action::Reply(Some("sqlite".to_owned()))
    );
    // An empty answer skips.
    app.ask(Ask::Question("Anything else?".to_owned()));
    assert_eq!(app.key(KeyCode::Enter), Action::Reply(None));
}

#[test]
fn quitting_a_working_run_asks_first() {
    let mut app = hosted();
    assert_eq!(app.key(KeyCode::Char('q')), Action::None);
    assert_eq!(app.overlay, Overlay::Quit);
    let s = screen(&app);
    assert!(s.contains("finish this task, then stop"), "{s}");
    assert!(s.contains("the next `speccoder run` resumes it"), "{s}");
    assert_eq!(app.key(KeyCode::Esc), Action::None);
    assert_eq!(app.overlay, Overlay::None);
    app.key(KeyCode::Char('q'));
    assert_eq!(app.key(KeyCode::Char('f')), Action::StopAfterTask);
    assert!(app.hosted.as_ref().is_some_and(|h| h.stopping));
    assert!(screen(&app).contains("(stopping after it)"));
    app.key(KeyCode::Char('q'));
    assert_eq!(app.key(KeyCode::Char('s')), Action::StopNow);
    // Once the run ended, q leaves.
    app.finish(CommandResult::new(
        "run",
        CommandStatus::Ok,
        "T002 verified",
    ));
    assert!(screen(&app).contains("run ended — T002 verified"));
    assert_eq!(app.key(KeyCode::Char('q')), Action::Quit);
}

#[test]
fn esc_never_quits_and_every_listed_key_works() {
    for screen_key in ['1', '2', '3'] {
        let mut app = App::new(snapshot());
        app.key(KeyCode::Char(screen_key));
        assert_eq!(app.key(KeyCode::Esc), Action::None);
        assert_eq!(app.screen, Screen::Run);
        app.key(KeyCode::Char('?'));
        assert_eq!(app.overlay, Overlay::Help);
        assert_eq!(app.key(KeyCode::Esc), Action::None);
        assert_eq!(app.overlay, Overlay::None);
    }
    let mut app = App::new(snapshot());
    // enter opens the selected task's evidence; d its diff.
    app.key(KeyCode::Enter);
    assert_eq!(app.screen, Screen::Evidence);
    assert_eq!(
        app.key(KeyCode::Char('d')),
        Action::Diff("0123456789abcdef".to_owned())
    );
    app.key(KeyCode::Char('j'));
    assert_eq!(app.task, 1);
    assert_eq!(app.key(KeyCode::Char('d')), Action::None);
    assert!(
        app.flash
            .as_deref()
            .unwrap_or_default()
            .contains("no change to show")
    );
    app.key(KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::Report);
    // The help lists the keys of the screen it is opened on.
    let keys: Vec<&str> = draw::keys(Screen::Run)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    for k in [
        "1 2 3",
        "?",
        "esc",
        "q",
        "enter",
        "tab",
        "d",
        "approval: v",
        "diff: ] [",
    ] {
        assert!(keys.contains(&k), "{k} not in {keys:?}");
    }
}

#[test]
fn the_diff_overlay_moves_by_hunk_and_file() {
    let mut app = App::new(snapshot());
    app.diff = Some(diff::Diff::parse(
        "abc T001\ndiff --git a/a.rs b/a.rs\n@@ -1 +1 @@\n-x\n+y\ndiff --git a/b.rs b/b.rs\n@@ -1 +1 @@\n-p\n+q\n",
    ));
    app.overlay = Overlay::Diff;
    app.key(KeyCode::Char('n'));
    assert!(screen(&app).contains("Diff a.rs"));
    app.key(KeyCode::Char('n'));
    assert!(screen(&app).contains("Diff b.rs"));
    app.key(KeyCode::Char('['));
    app.key(KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn the_evidence_screen_leads_with_the_verdict() {
    let mut app = App::new(snapshot());
    app.key(KeyCode::Char('2'));
    app.evidence = Some((
        "T001@0123456789abcdef".to_owned(),
        evidence::rows(&app.snap.report.as_ref().unwrap().tasks[0], &[], None),
    ));
    let s = screen(&app);
    for needle in [
        "✓ VERIFIED",
        "T001 title of T001",
        "Acceptance",
        "not run",
        "Attestation",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
    // regression only and attested never read like verified.
    app.snap.report.as_mut().unwrap().tasks[0].state = "regression_only".to_owned();
    assert!(screen(&app).contains("REGRESSION ONLY — nothing task-specific was shown"));
    app.snap.report.as_mut().unwrap().tasks[0].state = "attested".to_owned();
    assert!(screen(&app).contains("ATTESTED — a person's word, not evidence"));
    // A blocked task says what it waits for.
    app.key(KeyCode::Down);
    app.key(KeyCode::Down);
    assert!(screen(&app).contains("Waits for       T002"));
}

#[test]
fn the_report_screen_has_the_bar_the_waiting_and_the_uncited() {
    let mut app = App::new(snapshot());
    app.key(KeyCode::Char('3'));
    let s = screen(&app);
    for needle in [
        "[█··]",
        "Waiting for a person (1)",
        "Requirements no task cites (1)",
        "FR-009",
        "Convergence rounds (0)",
        "Next: speccoder task approve T002",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
    assert_eq!(
        draw::verdict_bar(&app.snap.report.as_ref().unwrap().tasks, true),
        "[#..]"
    );
}

#[test]
fn every_size_theme_and_ascii_draws() {
    let mut views = vec![App::new(snapshot()), hosted()];
    let mut asking = hosted();
    asking.ask(Ask::Call(Box::new(call())));
    views.push(asking);
    let mut help = App::new(snapshot());
    help.key(KeyCode::Char('?'));
    views.push(help);
    for screen_key in ['2', '3'] {
        let mut a = hosted();
        a.key(KeyCode::Char(screen_key));
        views.push(a);
    }
    for (w, h) in [(80, 24), (200, 60), (60, 15)] {
        for theme in Theme::ALL {
            for ascii in [false, true] {
                let look = Look {
                    color: true,
                    theme,
                    ascii,
                    width: usize::from(w),
                    screen_reader: false,
                };
                for app in &views {
                    let s = crate::style::with(look, || screen_at(app, w, h));
                    for line in s.lines() {
                        assert!(
                            crate::style::width(line) <= usize::from(w),
                            "{w}x{h}: {line}"
                        );
                    }
                    if ascii {
                        assert!(s.is_ascii(), "{theme:?} {w}x{h} not ASCII:\n{s}");
                    }
                }
            }
        }
    }
    // Too small: one line asking for room.
    let s = screen_at(&App::new(snapshot()), 40, 10);
    assert!(s.contains("make the terminal larger"), "{s}");
}

#[test]
fn a_hosted_run_is_driven_over_the_channel() {
    // The run's side: progress, a call that needs the person, the result.
    let (rx, cancel) = host::start(Path::new("."), |h| {
        h.progress.event(&Event::TaskStarted {
            feature: "001-x".to_owned(),
            task: "T002".to_owned(),
            title: "title of T002".to_owned(),
        });
        let answer = h.approver.ask(&call());
        let stopping = h.cancel.load(Ordering::SeqCst);
        CommandResult::new(
            "run",
            CommandStatus::Ok,
            &format!("{:?} {stopping}", answer.decision),
        )
    });
    let mut app = App::hosting(snapshot(), who());
    let (mut reply, mut question) = (None, None);
    let start = Instant::now();
    while app.overlay != Overlay::Ask && start.elapsed() < Duration::from_secs(10) {
        drain(&mut app, &rx, &mut reply, &mut question);
    }
    assert_eq!(
        app.hosted
            .as_ref()
            .and_then(|h| h.task.clone())
            .map(|t| t.0),
        Some("T002".to_owned())
    );
    // Later; asked to stop after the task; then the call approved with one key.
    app.key(KeyCode::Esc);
    app.key(KeyCode::Char('q'));
    assert_eq!(app.key(KeyCode::Char('f')), Action::StopAfterTask);
    cancel.store(true, Ordering::SeqCst);
    app.key(KeyCode::Enter);
    let Action::Answer(a) = app.key(KeyCode::Char('y')) else {
        panic!("no answer");
    };
    reply.take().unwrap().send(a).unwrap();
    while app.active() && start.elapsed() < Duration::from_secs(10) {
        drain(&mut app, &rx, &mut reply, &mut question);
    }
    let done = app.hosted.as_ref().and_then(|h| h.done.clone()).unwrap();
    assert_eq!(done.message(), "Approve true");
    assert_eq!(app.key(KeyCode::Char('q')), Action::Quit);
}

#[test]
fn an_approval_takes_precedence_over_help_and_the_diff() {
    for open in [Overlay::Help, Overlay::Diff] {
        let mut app = hosted();
        app.diff = Some(diff::Diff::parse("diff --git a/a b/a\n"));
        app.overlay = open;
        app.ask(Ask::Call(Box::new(call())));
        assert_eq!(app.overlay, Overlay::Ask);
        assert!(screen(&app).contains("Approval needed"));
    }
}

#[test]
fn f_picks_the_feature_and_a_working_run_keeps_its_own() {
    let report = speccoder_core::FeaturesReport {
        unit: "feature".to_owned(),
        current: Some("001-x".to_owned()),
        reason: "recorded".to_owned(),
        reason_text: "recorded in .specify/feature.json".to_owned(),
        action: "listed".to_owned(),
        features: vec![
            speccoder_core::FeatureEntry {
                title: None,
                number: 1,
                name: "001-x".to_owned(),
                dir: "specs/001-x".to_owned(),
                current: true,
                done: 1,
                tasks: 3,
                artifacts: vec!["spec".to_owned(), "plan".to_owned(), "tasks".to_owned()],
            },
            speccoder_core::FeatureEntry {
                title: None,
                number: 2,
                name: "002-y".to_owned(),
                dir: "specs/002-y".to_owned(),
                current: false,
                done: 0,
                tasks: 0,
                artifacts: vec!["spec".to_owned()],
            },
        ],
    };
    let mut app = App::new(snapshot());
    assert_eq!(app.key(KeyCode::Char('f')), Action::Features);
    app.show_features(report.clone());
    let s = screen(&app);
    for needle in [
        "Features · recorded in .specify/feature.json",
        "▸  1  001-x",
        "1/3 tasks",
        "2  002-y",
        "no tasks yet",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
    assert_eq!(app.pick, 0);
    app.key(KeyCode::Down);
    assert_eq!(app.key(KeyCode::Enter), Action::Switch("002-y".to_owned()));
    assert_eq!(app.overlay, Overlay::None);
    // Digits pick by number; esc closes.
    app.show_features(report);
    app.key(KeyCode::Char('2'));
    assert_eq!(app.pick, 1);
    app.key(KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::None);
    // While a hosted run works, its feature stays.
    let mut busy = hosted();
    assert_eq!(busy.key(KeyCode::Char('f')), Action::None);
    assert!(
        busy.flash
            .as_deref()
            .unwrap_or_default()
            .contains("switch once it has ended")
    );
}

#[test]
fn a_feature_being_planned_shows_what_writes_its_tasks() {
    let mut snap = snapshot();
    if let Some(r) = snap.report.as_mut() {
        r.tasks.clear();
        r.artifacts = vec!["spec".to_owned()];
        r.planning = vec!["plan".to_owned(), "tasks".to_owned()];
    }
    let s = screen(&App::new(snap));
    for needle in [
        "being planned: no task list yet",
        "✓ spec",
        "· plan",
        "next: speccoder spec plan",
    ] {
        assert!(s.contains(needle), "missing {needle:?} in\n{s}");
    }
}

#[test]
fn n_hands_the_next_step_to_the_terminal() {
    let mut snap = snapshot();
    snap.primary = Some(
        speccoder_core::actions::Action::new(
            "next",
            "spec tasks",
            "break the plan into tasks",
            "speccoder spec tasks",
        )
        .with_run(speccoder_core::actions::Runnable::Skill {
            name: "tasks".to_owned(),
            arguments: String::new(),
        }),
    );
    let mut app = App::new(snap);
    assert!(screen(&app).contains("n  spec tasks — break the plan into tasks (at the terminal)"));
    assert_eq!(app.key(KeyCode::Char('n')), Action::Handoff);
    // Nothing to take: said so, nothing happens.
    let mut idle = App::new(snapshot());
    assert_eq!(idle.key(KeyCode::Char('n')), Action::None);
    assert!(
        idle.flash
            .as_deref()
            .unwrap_or_default()
            .contains("nothing to take next")
    );
    // While a hosted run works, it keeps the terminal.
    let mut busy = hosted();
    assert_eq!(busy.key(KeyCode::Char('n')), Action::None);
    let _ = &mut app;
}
