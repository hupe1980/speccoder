//! `speccoder done` on a real repository: each line not done with its fix,
//! then done after the fixes; nothing written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use speccoder_core::done::{DonePart, DoneReport, DoneState};
use speccoder_core::{CommandData, CommandStatus};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env(
            "GIT_COMMITTER_DATE",
            std::env::var("DATE").unwrap_or_default(),
        )
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn commit_at(dir: &Path, when: &str, msg: &str) {
    git(dir, &["add", "-A"]);
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(["commit", "-qm", msg])
        .current_dir(dir)
        .env("GIT_COMMITTER_DATE", when)
        .env("GIT_AUTHOR_DATE", when)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

fn report(dir: &Path, base: Option<&str>) -> (CommandStatus, DoneReport, Vec<String>) {
    let res = speccoder_runtime::done::report(dir, base, false);
    let fix = res.fix();
    let Some(CommandData::Done(r)) = res.data() else {
        panic!("{}", res.message())
    };
    (res.status(), (**r).clone(), fix)
}

fn line(r: &DoneReport, part: DonePart) -> &speccoder_core::done::DoneLine {
    r.lines
        .iter()
        .find(|l| l.part == part)
        .unwrap_or_else(|| panic!("no {part:?} line: {r:?}"))
}

/// A project at its 0.1.0 release: a site with two pages, a README, a
/// changelog, one declared screenshot.
fn released() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    git(p, &["init", "-q", "-b", "main"]);
    write(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    );
    write(p, "src/lib.rs", "pub fn x() {}\n");
    write(
        p,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n\n- first\n",
    );
    write(
        p,
        "README.md",
        "# X\n\nSee the [guide](site/content/docs/guide.md).\n",
    );
    write(p, "site/zola.toml", "title = \"x\"\n");
    write(
        p,
        "site/content/_index.md",
        "+++\ntitle = \"x\"\n+++\nStart with the [guide](@/docs/guide.md#install).\n",
    );
    write(
        p,
        "site/content/docs/guide.md",
        "+++\ntitle = \"Guide\"\n+++\n## Install\n\nInstall 0.1.0 with cargo.\n\n![shot](../../shots/main.png)\n",
    );
    write(p, "site/static/shots/main.png", "png");
    write(p, "ui/Main.svelte", "<p>x</p>\n");
    write(
        p,
        "speccoder.toml",
        "[screenshots.main]\nshot = \"site/static/shots/main.png\"\nsources = [\"ui/Main.svelte\"]\n",
    );
    commit_at(p, "2026-01-01T00:00:00Z", "0.1.0");
    git(p, &["tag", "v0.1.0"]);
    d
}

#[test]
fn a_released_project_is_done_and_says_what_it_checked() {
    let d = released();
    let (status, r, _) = report(d.path(), None);
    assert_eq!(status, CommandStatus::Ok, "{r:?}");
    assert_eq!(r.base.as_deref(), Some("v0.1.0"));
    let parts: Vec<DonePart> = r.lines.iter().map(|l| l.part).collect();
    // No versions line: the version is the release's.
    assert_eq!(
        parts,
        [
            DonePart::Changelog,
            DonePart::Links,
            DonePart::Screenshots,
            DonePart::Tone
        ]
    );
    assert!(
        line(&r, DonePart::Links)
            .summary
            .starts_with("3 links in 3 pages"),
        "{r:?}"
    );
}

#[test]
fn each_line_is_not_done_with_its_fix_then_done_after_the_fixes() {
    let d = released();
    let p = d.path();
    // After the release: a source change, a bump, a dead anchor, a stale
    // shot, and a page that promises.
    write(p, "src/lib.rs", "pub fn x() {}\npub fn y() {}\n");
    write(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.2.0\"\n",
    );
    write(p, "ui/Main.svelte", "<p>y</p>\n");
    write(
        p,
        "site/content/_index.md",
        "+++\ntitle = \"x\"\n+++\nStart with the [guide](@/docs/guide.md#setup).\n\nExport is coming soon.\n",
    );
    commit_at(p, "2026-02-01T00:00:00Z", "work");
    let before = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(p)
        .output()
        .unwrap()
        .stdout;

    let (status, r, fix) = report(p, None);
    assert_eq!(status, CommandStatus::NeedsHuman);
    for part in [
        DonePart::Changelog,
        DonePart::Links,
        DonePart::Screenshots,
        DonePart::Tone,
        DonePart::Versions,
    ] {
        let l = line(&r, part);
        assert_eq!(l.state, DoneState::NotDone, "{l:?}");
        // SC-003: every open line says what to do.
        assert!(!l.fixes.is_empty() && !l.findings.is_empty(), "{l:?}");
    }
    assert_eq!(line(&r, DonePart::Changelog).findings[0].path, "Cargo.toml");
    assert!(
        line(&r, DonePart::Links).findings[0]
            .what
            .contains("#setup"),
        "{r:?}"
    );
    assert_eq!(
        line(&r, DonePart::Versions).findings[0].path,
        "site/content/docs/guide.md"
    );
    assert!(fix.iter().any(|f| f.contains("retake main")), "{fix:?}");

    // FR-013: nothing was written.
    let after = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(p)
        .output()
        .unwrap()
        .stdout;
    assert_eq!(before, after);

    // The fixes; the shot retaken but not yet committed counts as current.
    write(
        p,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n- y\n\n## [0.1.0] - 2026-01-01\n\n- first\n",
    );
    write(
        p,
        "site/content/_index.md",
        "+++\ntitle = \"x\"\n+++\nStart with the [guide](@/docs/guide.md#install).\n",
    );
    write(
        p,
        "site/content/docs/guide.md",
        "+++\ntitle = \"Guide\"\n+++\n## Install\n\nInstall 0.2.0 with cargo.\n\n![shot](../../shots/main.png)\n",
    );
    write(p, "site/static/shots/main.png", "png2");
    let (status, r, _) = report(p, None);
    assert_eq!(status, CommandStatus::Ok, "{r:#?}");
    assert!(r.done);
}

#[test]
fn an_uncommitted_source_change_makes_its_shot_stale_and_a_bad_base_is_said() {
    let d = released();
    write(d.path(), "ui/Main.svelte", "<p>z</p>\n");
    let (_, r, _) = report(d.path(), None);
    let l = line(&r, DonePart::Screenshots);
    assert_eq!(l.state, DoneState::NotDone);
    assert!(l.findings[0].what.contains("not committed"), "{l:?}");
    let res = speccoder_runtime::done::report(d.path(), Some("no-such-ref"), false);
    assert_eq!(res.status(), CommandStatus::UsageError);
}

#[test]
fn a_branch_is_judged_against_the_remotes_default_branch() {
    let d = released();
    let p = d.path();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        p,
        &[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    git(p, &["push", "-q", "origin", "main"]);
    git(p, &["remote", "set-head", "origin", "main"]);
    git(p, &["switch", "-q", "-c", "feature"]);
    write(p, "tests/t.rs", "#[test] fn t() {}\n");
    commit_at(p, "2026-02-01T00:00:00Z", "a test");
    let (_, r, _) = report(p, None);
    assert_eq!(r.base.as_deref(), Some("origin/main"));
    // Only a test changed: no entry needed.
    assert_eq!(line(&r, DonePart::Changelog).state, DoneState::Done);
}

#[test]
fn a_given_base_counts_only_what_the_branch_changed() {
    let d = released();
    let p = d.path();
    git(p, &["switch", "-q", "-c", "feature"]);
    write(p, "tests/t.rs", "#[test] fn t() {}\n");
    commit_at(p, "2026-02-01T00:00:00Z", "a test");
    git(p, &["switch", "-q", "main"]);
    write(p, "src/other.rs", "pub fn o() {}\n");
    commit_at(p, "2026-02-02T00:00:00Z", "main moves on");
    git(p, &["switch", "-q", "feature"]);
    let (_, r, _) = report(p, Some("main"));
    // main's own change is not this branch's.
    assert_eq!(
        line(&r, DonePart::Changelog).state,
        DoneState::Done,
        "{r:?}"
    );
}

#[test]
fn a_commit_reads_pages_with_any_name() {
    let d = released();
    let p = d.path();
    write(
        p,
        "site/content/docs/über.md",
        "+++\ntitle = \"x\"\n+++\n## Ärger\n\nSee [it](#arger) and [guide](@/docs/guide.md).\n",
    );
    commit_at(p, "2026-02-01T00:00:00Z", "a page");
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(p)
        .output()
        .unwrap();
    let head = String::from_utf8(head.stdout).unwrap().trim().to_owned();
    let r = speccoder_runtime::done::evaluate(
        p,
        p,
        &speccoder_runtime::done::Source::Commit(head),
        Some("v0.1.0"),
        false,
    )
    .unwrap();
    let links = line(&r, DonePart::Links);
    assert_eq!(links.state, DoneState::Done, "{links:?}");
    assert!(links.summary.contains("in 4 pages"), "{links:?}");
}
