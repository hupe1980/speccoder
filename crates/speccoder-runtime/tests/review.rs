//! The spec review: each requirement approved, or changes
//! requested, on its exact wording; a reworded requirement's decision
//! lapses; the model never writes the record.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use speccoder_core::{CommandData, CommandStatus, ReviewReport};
use speccoder_runtime::review;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
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
}

const SPEC: &str = "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST log out.\n- **SC-001**: Login takes under a second.\n";

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for (p, t) in [
        (".specify/memory/constitution.md", "# Constitution\n"),
        (".specify/init-options.json", "{}\n"),
        ("specs/001-login/spec.md", SPEC),
    ] {
        fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
        fs::write(r.join(p), t).unwrap();
    }
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["config", "user.name", "Ada"]);
    git(r, &["config", "user.email", "ada@example.com"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    dir
}

fn data(res: &speccoder_core::CommandResult) -> ReviewReport {
    let Some(CommandData::Review(r)) = res.data() else {
        panic!("{}", res.message())
    };
    (**r).clone()
}

fn states(r: &ReviewReport) -> Vec<(&str, &str)> {
    r.requirements
        .iter()
        .map(|i| (i.id.as_str(), i.state.as_str()))
        .collect()
}

#[test]
fn every_requirement_is_a_card_to_review() {
    let r = repo();
    let res = review::report(r.path(), None);
    let rv = data(&res);
    assert_eq!(
        states(&rv),
        [("FR-001", "open"), ("FR-002", "open"), ("SC-001", "open")]
    );
    assert_eq!(rv.requirements[0].text, "Users MUST log in.");
    assert_eq!(rv.file, "specs/001-login/review.json");
    assert_eq!(res.message(), "0/3 requirements approved; 3 to review");
}

#[test]
fn decisions_name_who_and_bind_to_the_wording() {
    let r = repo();
    let res = review::decide(
        r.path(),
        None,
        &["FR-001".to_owned()],
        "approve",
        None,
        "cli",
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(
        res.message().contains("commit specs/001-login/review.json"),
        "{}",
        res.message()
    );
    // A change request needs its comment.
    let bare = review::decide(
        r.path(),
        None,
        &["FR-002".to_owned()],
        "request",
        None,
        "cli",
    );
    assert_eq!(bare.status(), CommandStatus::UsageError);
    let asked = review::decide(
        r.path(),
        None,
        &["FR-002".to_owned()],
        "request",
        Some("say what happens to open sessions"),
        "web",
    );
    let rv = data(&asked);
    assert_eq!(
        states(&rv),
        [
            ("FR-001", "approved"),
            ("FR-002", "changes_requested"),
            ("SC-001", "open")
        ]
    );
    assert_eq!(rv.requirements[0].by.as_deref(), Some("Ada"));
    assert!(
        rv.revision
            .as_deref()
            .unwrap()
            .contains("FR-002 (\"Users MUST log out.\"): say what happens")
    );

    // Reworded: the approval lapses and says what was approved.
    fs::write(
        r.path().join("specs/001-login/spec.md"),
        SPEC.replace("Users MUST log in.", "Users MUST log in with a passkey."),
    )
    .unwrap();
    let rv = data(&review::report(r.path(), None));
    let fr1 = &rv.requirements[0];
    assert_eq!(fr1.state, "lapsed");
    assert_eq!(fr1.decided_on.as_deref(), Some("Users MUST log in."));
    assert_eq!(rv.totals.lapsed, 1);

    // Approving "the rest" takes what is not approved on its wording now.
    let all = data(&review::decide(r.path(), None, &[], "approve", None, "cli"));
    assert!(all.totals.complete(), "{:?}", all.totals);
    assert!(all.revision.is_none());
}

#[test]
fn an_unknown_requirement_is_refused_and_a_removed_one_forgotten() {
    let r = repo();
    let res = review::decide(
        r.path(),
        None,
        &["FR-009".to_owned()],
        "approve",
        None,
        "cli",
    );
    assert_eq!(res.status(), CommandStatus::NotFound);
    assert!(
        res.message().contains("FR-001, FR-002, SC-001"),
        "{}",
        res.message()
    );
    let _ = review::decide(r.path(), None, &[], "approve", None, "cli");
    fs::write(
        r.path().join("specs/001-login/spec.md"),
        "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n",
    )
    .unwrap();
    let _ = review::decide(r.path(), None, &["FR-001".to_owned()], "clear", None, "cli");
    let text = fs::read_to_string(r.path().join("specs/001-login/review.json")).unwrap();
    assert!(
        !text.contains("FR-002") && !text.contains("FR-001"),
        "{text}"
    );
}

#[test]
fn a_skill_cannot_change_the_review() {
    let r = repo();
    let _ = review::decide(r.path(), None, &[], "approve", None, "cli");
    let file = r.path().join("specs/001-login/review.json");
    let approved = fs::read(&file).unwrap();
    let before = review::snapshot(r.path());
    // What a planning skill's command might do.
    fs::write(&file, "{\"requirements\":{}}\n").unwrap();
    let restored = review::restore(&before);
    assert_eq!(restored.len(), 1, "{restored:?}");
    assert_eq!(fs::read(&file).unwrap(), approved);
    // A record the skill created is removed again — in a unit that existed,
    let other = r.path().join("specs/002-x");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("spec.md"), SPEC).unwrap();
    let before = review::snapshot(r.path());
    fs::write(other.join("review.json"), "{}").unwrap();
    assert_eq!(review::restore(&before).len(), 1);
    assert!(!other.join("review.json").exists());
    // and in one the skill created (Spec Kit's specify makes a unit).
    let before = review::snapshot(r.path());
    let made = r.path().join("specs/003-new");
    fs::create_dir_all(&made).unwrap();
    fs::write(made.join("spec.md"), SPEC).unwrap();
    fs::write(made.join("review.json"), "{\"requirements\":{}}").unwrap();
    assert_eq!(review::restore(&before).len(), 1);
    assert!(!made.join("review.json").exists());
    assert!(
        made.join("spec.md").exists(),
        "the spec the skill wrote stays"
    );
}

#[test]
fn the_report_and_the_payload_carry_the_review() {
    let r = repo();
    let _ = review::decide(
        r.path(),
        None,
        &["FR-001".to_owned()],
        "approve",
        None,
        "cli",
    );
    let res = speccoder_runtime::status::report(r.path(), None);
    let Some(CommandData::Report(rep)) = res.data() else {
        panic!("{}", res.message())
    };
    let t = rep.review.expect("review totals");
    assert_eq!((t.approved, t.open), (1, 2));
}

const ASKING: &str = "# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST log out [NEEDS CLARIFICATION: what happens to open sessions?].\n\n## Edge Cases\n\n- A locked account [NEEDS CLARIFICATION: unlock how?].\n";

#[test]
fn open_questions_are_answered_in_place() {
    let r = repo();
    fs::write(r.path().join("specs/001-login/spec.md"), ASKING).unwrap();
    let rv = data(&review::report(r.path(), None));
    let q: Vec<(&str, &str, Option<&str>)> = rv
        .questions
        .iter()
        .map(|q| (q.id.as_str(), q.text.as_str(), q.near.as_deref()))
        .collect();
    assert_eq!(
        q,
        [
            ("Q1", "what happens to open sessions?", Some("FR-002")),
            ("Q2", "unlock how?", Some("Edge Cases"))
        ]
    );
    assert_eq!(rv.totals.questions, 2);
    assert!(!rv.totals.complete());
    // A wording that still asks is not signed: refused by name, passed by in bulk.
    let refused = review::decide(
        r.path(),
        None,
        &["FR-002".to_owned()],
        "approve",
        None,
        "cli",
    );
    assert_eq!(
        refused.status(),
        CommandStatus::UsageError,
        "{}",
        refused.message()
    );
    let bulk = review::decide(r.path(), None, &[], "approve", None, "cli");
    assert!(bulk.message().contains("not FR-002"), "{}", bulk.message());
    assert_eq!(
        states(&data(&bulk)),
        [("FR-001", "approved"), ("FR-002", "open")]
    );
    // The answer takes the question's place; nothing else changes.
    let a = review::answer(r.path(), None, "q1", "they end at once");
    assert_eq!(a.status(), CommandStatus::Ok, "{}", a.message());
    let spec = fs::read_to_string(r.path().join("specs/001-login/spec.md")).unwrap();
    assert!(
        spec.contains("- **FR-002**: Users MUST log out they end at once."),
        "{spec}"
    );
    assert!(spec.contains("unlock how?"), "{spec}");
    assert_eq!(data(&review::report(r.path(), None)).questions.len(), 1);
    assert_eq!(
        review::answer(r.path(), None, "Q9", "x").status(),
        CommandStatus::NotFound
    );
}

#[test]
fn the_person_rewords_a_requirement_and_its_history_shows() {
    let r = repo();
    let _ = review::decide(r.path(), None, &[], "approve", None, "cli");
    let e = review::edit(
        r.path(),
        None,
        "FR-002",
        "Users MUST log out of every device.",
        false,
        "cli",
    );
    assert_eq!(e.status(), CommandStatus::Ok, "{}", e.message());
    let rv = data(&e);
    // The line keeps its identifier; the approval lapsed with the words.
    let fr2 = rv.requirements.iter().find(|i| i.id == "FR-002").unwrap();
    assert_eq!(
        (fr2.text.as_str(), fr2.state.as_str()),
        ("Users MUST log out of every device.", "lapsed")
    );
    // Since the last commit: what a commit would record.
    assert_eq!(rv.changes.len(), 1);
    assert_eq!(rv.changes[0].id, "FR-002");
    // Reworded and approved in one step.
    let e = review::edit(
        r.path(),
        None,
        "FR-002",
        "Users MUST log out everywhere.",
        true,
        "cli",
    );
    let rv = data(&e);
    assert_eq!(
        rv.requirements
            .iter()
            .find(|i| i.id == "FR-002")
            .unwrap()
            .state,
        "approved"
    );
    // Committed: the history names what each version changed.
    git(r.path(), &["commit", "-qam", "spec: log out everywhere"]);
    let rv = data(&review::report(r.path(), None));
    assert!(rv.changes.is_empty());
    let v: Vec<(&str, Vec<String>, Vec<String>)> = rv
        .history
        .iter()
        .map(|v| (v.subject.as_str(), v.added.clone(), v.reworded.clone()))
        .collect();
    assert_eq!(
        v[0],
        (
            "spec: log out everywhere",
            vec![],
            vec!["FR-002".to_owned()]
        )
    );
    assert_eq!(v[1].0, "init");
    assert_eq!(v[1].1, ["FR-001", "FR-002", "SC-001"]);
    // One line of words; an identifier the spec has.
    assert_eq!(
        review::edit(r.path(), None, "FR-002", "a\nb", false, "cli").status(),
        CommandStatus::UsageError
    );
    assert_eq!(
        review::edit(r.path(), None, "FR-099", "x", false, "cli").status(),
        CommandStatus::NotFound
    );
}
