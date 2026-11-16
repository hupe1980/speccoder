//! Story claims: refs on the shared remote, changed only by
//! compare-and-swap — one holder per story, however many race for it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use speccoder_core::claims::{Claim, ClaimState, ClaimVerb, ClaimsReport};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::claims::{Options, claim, local};

const FEATURE: &str = "001-graph";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
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
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn who(dir: &Path, name: &str) {
    git(dir, &["config", "user.name", name]);
    git(
        dir,
        &[
            "config",
            "user.email",
            &format!("{}@example.com", name.to_lowercase()),
        ],
    );
}

/// The `graph` fixture committed in a new repository.
fn project(dir: &Path) {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(dir)
            .status()
            .unwrap()
            .success()
    );
    git(dir, &["init", "-q"]);
    who(dir, "Owner");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// A bare remote holding the project, and `n` clones of it.
fn team(n: usize) -> (tempfile::TempDir, Vec<PathBuf>) {
    let root = tempfile::tempdir().unwrap();
    let seed = root.path().join("seed");
    fs::create_dir_all(&seed).unwrap();
    project(&seed);
    let bare = root.path().join("remote.git");
    git(
        root.path(),
        &["clone", "-q", "--bare", "seed", "remote.git"],
    );
    let clones = (0..n)
        .map(|i| {
            let c = root.path().join(format!("c{i}"));
            git(
                root.path(),
                &["clone", "-q", bare.to_str().unwrap(), c.to_str().unwrap()],
            );
            c
        })
        .collect();
    (root, clones)
}

fn ask(dir: &Path, stories: &[&str], release: bool, force: bool) -> (CommandResult, ClaimsReport) {
    let stories: Vec<String> = stories.iter().map(|s| (*s).to_owned()).collect();
    let r = claim(
        dir,
        &Options {
            feature: Some(FEATURE),
            stories: &stories,
            release,
            force,
        },
    );
    let Some(CommandData::Claims(c)) = r.data() else {
        panic!("{:?}: {}", r.status(), r.message())
    };
    let c = (**c).clone();
    (r, c)
}

fn state_of(r: &ClaimsReport, story: &str) -> Option<ClaimState> {
    r.claims.iter().find(|c| c.story == story).map(|c| c.state)
}

fn remote_refs(dir: &Path) -> String {
    git(dir, &["ls-remote", "origin", "refs/speccoder/*"])
}

#[test]
fn us1_a_story_is_taken_once_and_taken_over_only_on_purpose() {
    let (_root, c) = team(2);
    who(&c[0], "Dana");
    who(&c[1], "Sam");

    let (r, a) = ask(&c[0], &["us1"], false, false);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert!(
        r.message().starts_with("US1 is yours for 24h"),
        "{}",
        r.message()
    );
    assert_eq!(a.changed[0].verb, ClaimVerb::Claimed);
    assert!(
        a.notices.iter().any(|n| n.contains("git fetch")),
        "{:?}",
        a.notices
    );
    assert!(remote_refs(&c[0]).contains("refs/speccoder/claims/001-graph/US1"));
    // The fetch refspec was added once: a plain fetch brings claims.
    assert!(
        git(&c[0], &["config", "--get-all", "remote.origin.fetch"])
            .contains("refs/speccoder/claims/*")
    );

    // Sam is refused, told whose it is and how to take it over.
    let (r, b) = ask(&c[1], &["US1"], false, false);
    assert_eq!(r.status(), CommandStatus::NeedsHuman);
    assert_eq!(r.code().map(|e| e.name), Some("CLAIMED"));
    assert!(
        r.message().contains("US1 is claimed by Dana"),
        "{}",
        r.message()
    );
    assert!(
        r.fix().iter().any(|f| f.contains("--force")),
        "{:?}",
        r.fix()
    );
    assert_eq!(state_of(&b, "US1"), Some(ClaimState::Theirs));

    // Dana claiming again renews and keeps since.
    let (_, again) = ask(&c[0], &["US1"], false, false);
    assert_eq!(again.changed[0].verb, ClaimVerb::Renewed);

    // Sam takes it over on purpose.
    let (r, b) = ask(&c[1], &["US1"], false, true);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(b.changed[0].verb, ClaimVerb::Took);
    assert_eq!(b.changed[0].from.as_deref(), Some("Dana"));
    assert!(
        r.message().contains("took US1 over from Dana"),
        "{}",
        r.message()
    );

    // Dana sees it; releasing Sam's needs --force.
    let (_, a) = ask(&c[0], &[], false, false);
    assert_eq!(state_of(&a, "US1"), Some(ClaimState::Theirs));
    let (r, _) = ask(&c[0], &["US1"], true, false);
    assert_eq!(r.code().map(|e| e.name), Some("CLAIMED"));
    assert!(
        r.message().contains("Sam's, not yours to release"),
        "{}",
        r.message()
    );

    // Sam releases: gone on the remote.
    let (r, b) = ask(&c[1], &["US1"], true, false);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(b.changed[0].verb, ClaimVerb::Released);
    assert!(remote_refs(&c[0]).is_empty());
    // The person's copy follows on the next listing.
    let (_, a) = ask(&c[0], &[], false, false);
    assert!(a.claims.is_empty(), "{:?}", a.claims);
}

#[test]
fn us1_eight_racing_claims_end_with_one_holder() {
    let (_root, c) = team(8);
    for (i, dir) in c.iter().enumerate() {
        who(dir, &format!("P{i}"));
    }
    let results: Vec<(CommandStatus, String)> = std::thread::scope(|s| {
        let handles: Vec<_> = c
            .iter()
            .map(|dir| {
                s.spawn(move || {
                    let (r, _) = ask(dir, &["US2"], false, false);
                    (r.status(), r.message().to_owned())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let won: Vec<_> = results
        .iter()
        .filter(|(s, _)| *s == CommandStatus::Ok)
        .collect();
    assert_eq!(won.len(), 1, "{results:?}");
    // Every loser is told who holds it.
    for (s, m) in results.iter().filter(|(s, _)| *s != CommandStatus::Ok) {
        assert_eq!(*s, CommandStatus::NeedsHuman, "{m}");
        assert!(m.contains("US2 is claimed by P"), "{m}");
    }
}

/// Writes a claim straight onto the remote, as another machine would.
fn plant(dir: &Path, claim: &Claim) {
    let tree = git(dir, &["hash-object", "-t", "tree", "-w", "/dev/null"]);
    let msg = dir.join(".claim-msg");
    fs::write(&msg, claim.message()).unwrap();
    let id = git(dir, &["commit-tree", &tree, "-F", msg.to_str().unwrap()]);
    fs::remove_file(msg).unwrap();
    let name = speccoder_core::claims::ref_of(&claim.feature, &claim.story);
    git(dir, &["push", "-q", "origin", &format!("{id}:{name}")]);
}

fn lee(story: &str, until: u64, done: Option<String>) -> Claim {
    Claim {
        feature: FEATURE.into(),
        story: story.into(),
        holder: "Lee".into(),
        email: "lee@example.com".into(),
        since: until.saturating_sub(100_000),
        until,
        branch: "speccoder/001-graph".into(),
        done,
    }
}

#[test]
fn us2_a_stale_claim_is_taken_without_force_and_a_merged_one_goes() {
    let (_root, c) = team(1);
    who(&c[0], "Sam");
    // US1: Lee's, ran out an hour ago.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    plant(&c[0], &lee("US1", now - 3600, None));
    // US3 (planted by an older plan): done at a commit already on main → merged.
    let merged = git(&c[0], &["rev-parse", "HEAD"]);
    plant(&c[0], &lee("US3", now + 3600, Some(merged)));
    // US2: done on a branch nobody merged.
    git(&c[0], &["checkout", "-q", "-b", "speccoder/001-graph"]);
    git(&c[0], &["commit", "-q", "--allow-empty", "-m", "work"]);
    let open = git(&c[0], &["rev-parse", "HEAD"]);
    git(&c[0], &["checkout", "-q", "main"]);
    plant(&c[0], &lee("US2", now + 3600, Some(open)));

    let (r, list) = ask(&c[0], &[], false, false);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(state_of(&list, "US1"), Some(ClaimState::Stale));
    assert_eq!(state_of(&list, "US3"), None, "merged: removed");
    assert!(
        list.notices.iter().any(|n| n.contains("US3 merged")),
        "{:?}",
        list.notices
    );
    assert!(!remote_refs(&c[0]).contains("/US3"));
    // A done claim holds while unmerged.
    assert_eq!(state_of(&list, "US2"), Some(ClaimState::Done));

    let (r, took) = ask(&c[0], &["US1"], false, false);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(took.changed[0].verb, ClaimVerb::Took);
    assert!(took.changed[0].stale);
    assert!(
        r.message().contains("after Lee's claim ran out"),
        "{}",
        r.message()
    );

    let (r, _) = ask(&c[0], &["US2"], false, false);
    assert_eq!(r.code().map(|e| e.name), Some("CLAIMED"));
    assert!(
        r.message()
            .contains("US2 is done on Lee's speccoder/001-graph"),
        "{}",
        r.message()
    );

    // The web app's view reads this repository's copy, no network.
    let l = local(&c[0], Some(FEATURE));
    let Some(CommandData::Claims(l)) = l.data() else {
        panic!()
    };
    assert!(!l.live);
    assert_eq!(state_of(l, "US1"), Some(ClaimState::Mine));
}

#[test]
fn only_a_story_the_feature_has_is_claimed() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    for bad in ["US9", "T005", "login"] {
        let r = claim(
            dir.path(),
            &Options {
                feature: Some(FEATURE),
                stories: &[bad.to_owned()],
                ..Options::default()
            },
        );
        assert_eq!(
            r.status(),
            CommandStatus::UsageError,
            "{bad}: {}",
            r.message()
        );
        assert!(
            r.message().contains("its stories: US1, US2"),
            "{}",
            r.message()
        );
    }
}

#[test]
fn without_a_remote_claims_live_in_the_repository() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    who(dir.path(), "Dana");
    let (r, a) = ask(dir.path(), &["US1"], false, false);
    assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
    assert_eq!(a.remote, None);
    assert!(
        a.notices.is_empty(),
        "no refspec without a remote: {:?}",
        a.notices
    );
    who(dir.path(), "Sam");
    let (r, _) = ask(dir.path(), &["US1"], false, false);
    assert_eq!(r.code().map(|e| e.name), Some("CLAIMED"));
}

#[test]
fn claiming_needs_a_git_identity_and_a_reachable_remote() {
    let (root, c) = team(1);
    git(&c[0], &["config", "user.email", ""]);
    let (r, _) = (
        claim(
            &c[0],
            &Options {
                feature: Some(FEATURE),
                stories: &["US1".to_owned()],
                ..Options::default()
            },
        ),
        (),
    );
    assert_eq!(
        r.code().map(|e| e.name),
        Some("CLAIM-NO-IDENTITY"),
        "{}",
        r.message()
    );
    who(&c[0], "Dana");
    fs::remove_dir_all(root.path().join("remote.git")).unwrap();
    let r = claim(
        &c[0],
        &Options {
            feature: Some(FEATURE),
            stories: &["US1".to_owned()],
            ..Options::default()
        },
    );
    assert_eq!(
        r.code().map(|e| e.name),
        Some("CLAIM-UNREACHABLE"),
        "{}",
        r.message()
    );
}

#[test]
fn a_run_stops_before_a_story_another_person_holds() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    project(r);
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n[checks.ok]\nargv = [\"true\"]\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "config"]);
    who(r, "Dana");
    let (res, _) = ask(r, &["US1"], false, false);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    who(r, "Sam");
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    let d: Arc<dyn ModelProvider> = f.clone();
    let res = speccoder_runtime::run_feature_with(
        r,
        Some(FEATURE),
        state.path(),
        &d,
        None,
        &speccoder_runtime::RunOptions::default(),
    );
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(
        res.code().map(|e| e.name),
        Some("CLAIMED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().contains("T005: US1 is claimed by Dana"),
        "{}",
        res.message()
    );
    assert!(
        f.asked().is_empty(),
        "no model call for a story another holds"
    );

    // A task run on its own stops the same way.
    let res = speccoder_runtime::run_with(r, Some(FEATURE), "T006", state.path(), d.clone(), None);
    assert_eq!(
        res.code().map(|e| e.name),
        Some("CLAIMED"),
        "{}",
        res.message()
    );
    assert!(
        res.message().starts_with("T006: US1 is claimed by Dana"),
        "{}",
        res.message()
    );
    assert!(f.asked().is_empty());

    // Needs You says whose it is, with Take It Over behind a confirm.
    let inbox = speccoder_runtime::inbox::inbox_with(r, Some(FEATURE), Some(state.path()));
    let Some(CommandData::Inbox(inbox)) = inbox.data() else {
        panic!("{}", inbox.message())
    };
    let item = inbox
        .items
        .iter()
        .find(|i| i.kind == "claimed")
        .expect("a claimed item");
    assert_eq!(item.title, "US1 is Dana's");
    assert_eq!(item.task.as_deref(), Some("T005"));
    let take = &item.actions[0];
    assert!(
        take.confirm
            .as_deref()
            .is_some_and(|c| c.contains("Dana is not told"))
    );
    assert_eq!(
        take.run,
        Some(speccoder_core::actions::Runnable::Claim {
            story: "US1".into(),
            release: false,
            force: true
        })
    );
    // Taken over: the item is gone.
    let (res, _) = ask(r, &["US1"], false, true);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let inbox = speccoder_runtime::inbox::inbox_with(r, Some(FEATURE), Some(state.path()));
    let Some(CommandData::Inbox(inbox)) = inbox.data() else {
        panic!()
    };
    assert!(!inbox.items.iter().any(|i| i.kind == "claimed"));
}
