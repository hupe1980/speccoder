//! GitHub, built in, against a stand-in API: readiness and its
//! reasons, issues (assigned first, then the freshest), one issue read in
//! full, a pull request's state and checks — no `gh` anywhere.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fake_github;

use std::process::Command;

use speccoder_runtime::github::{self, Ctx, Repo};

/// A git repository whose remote names o/r on github.com.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &["remote", "add", "origin", "git@github.com:o/r.git"][..],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

fn issues() -> String {
    r#"[
 {"number":7,"title":"Old","html_url":"https://github.com/o/r/issues/7","labels":[],"assignees":[],"updated_at":"2026-10-01T00:00:00Z"},
 {"number":42,"title":"Greet people","html_url":"https://github.com/o/r/issues/42","labels":[{"name":"feature"}],"assignees":[{"login":"me"}],"updated_at":"2026-09-01T00:00:00Z"},
 {"number":5,"title":"A pull request","html_url":"https://github.com/o/r/pull/5","labels":[],"assignees":[],"updated_at":"2026-10-21T00:00:00Z","pull_request":{}},
 {"number":9,"title":"Fresh","html_url":"https://github.com/o/r/issues/9","labels":[],"assignees":[{"login":"other"}],"updated_at":"2026-10-20T00:00:00Z"}
]"#
    .to_owned()
}

#[test]
fn issues_come_assigned_first_then_freshest_and_one_reads_in_full() {
    let dir = repo();
    let api = fake_github::serve(vec![
        ("GET /user", 200, r#"{"login":"me"}"#.to_owned()),
        ("GET /repos/o/r", 200, r#"{"full_name":"o/r"}"#.to_owned()),
        ("GET /repos/o/r/issues", 200, issues()),
        (
            "GET /repos/o/r/issues/42",
            200,
            r#"{"number":42,"title":"Greet people","body":"By name, politely.","html_url":"https://github.com/o/r/issues/42","labels":[{"name":"feature"}]}"#.to_owned(),
        ),
    ]);
    let ctx = Ctx::fixed(&api.url, Some("t0ken"));
    let s = github::status(&ctx, dir.path());
    assert!(s.ready && s.repo.as_deref() == Some("o/r"), "{s:?}");
    assert_eq!(s.login.as_deref(), Some("me"));
    // The token goes as a bearer, to its own host only.
    assert!(
        api.requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.authorization == "Bearer t0ken")
    );

    let started = std::collections::BTreeMap::from([(
        "https://github.com/o/r/issues/9".to_owned(),
        "003-fresh".to_owned(),
    )]);
    let r = github::issues(&ctx, dir.path(), &started).unwrap();
    let order: Vec<u64> = r.issues.iter().map(|i| i.number).collect();
    assert_eq!(
        order,
        [42, 9, 7],
        "assigned first, then the freshest; pull requests aside"
    );
    assert!(r.issues[0].mine && r.issues[0].labels == ["feature"]);
    assert_eq!(r.issues[1].feature.as_deref(), Some("003-fresh"));
    assert_eq!(r.repo.as_deref(), Some("o/r"));

    for reference in ["42", "#42", "https://github.com/o/r/issues/42"] {
        let i = github::issue(&ctx, dir.path(), reference).unwrap();
        let d = github::description(&i);
        assert!(d.starts_with("Greet people\n\nBy name, politely."), "{d}");
        assert!(
            d.ends_with("GitHub issue: https://github.com/o/r/issues/42"),
            "{d}"
        );
    }
    // Not a reference: refused before GitHub is asked.
    let asked = api.calls().len();
    for bad in [
        "",
        "abc",
        "--web",
        "42; rm -rf /",
        "https://github.com/o/r/pull/3",
    ] {
        assert!(github::issue(&ctx, dir.path(), bad).is_err(), "{bad}");
    }
    assert_eq!(api.calls().len(), asked);
}

#[test]
fn signed_out_or_refused_is_explained_with_the_fix() {
    let dir = repo();
    let api = fake_github::serve(vec![(
        "GET /user",
        401,
        r#"{"message":"Bad credentials"}"#.to_owned(),
    )]);
    // No token at all: nothing is asked.
    let s = github::status(&Ctx::fixed(&api.url, None), dir.path());
    assert!(!s.ready, "{s:?}");
    assert_eq!(s.reason.as_deref(), Some("not signed in to github.com"));
    assert_eq!(s.fix.as_deref(), Some("speccoder auth login"));
    assert!(api.calls().is_empty());
    let e =
        github::issues(&Ctx::fixed(&api.url, None), dir.path(), &Default::default()).unwrap_err();
    let refused = github::refused("issue", &e);
    assert_eq!(refused.code().map(|c| c.name), Some("GITHUB-AUTH"), "{e}");
    assert_eq!(refused.fix()[0], "speccoder auth login --with-token");
    // A token GitHub refuses.
    let s = github::status(&Ctx::fixed(&api.url, Some("old")), dir.path());
    assert!(
        !s.ready && s.reason.as_deref().unwrap().contains("refused the token"),
        "{s:?}"
    );
    // Signing in with it is refused, and nothing is kept.
    let r = github::login_with_token(&Ctx::fixed(&api.url, None), "github.com", "old");
    assert_eq!(
        r.code().map(|c| c.name),
        Some("GITHUB-AUTH"),
        "{}",
        r.message()
    );
}

#[test]
fn a_missing_remote_is_said_before_any_login() {
    let bare = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(bare.path())
            .status()
            .unwrap()
            .success()
    );
    let api = fake_github::serve(vec![]);
    let ctx = Ctx::fixed(&api.url, None);
    let s = github::status(&ctx, bare.path());
    let reason = s.reason.unwrap();
    assert!(reason.contains("has no remote"), "{reason}");
    assert_eq!(
        github::refused("issue", &reason).code().map(|c| c.name),
        Some("NO-REMOTE")
    );
    assert!(
        github::issues(&ctx, bare.path(), &Default::default())
            .unwrap_err()
            .contains("has no remote")
    );
    assert!(api.calls().is_empty());
}

#[test]
fn a_branch_s_pull_request_reads_with_its_review_and_checks() {
    let dir = repo();
    let api = fake_github::serve(vec![(
        "POST /graphql",
        200,
        r#"{"data":{"repository":{"pullRequests":{"nodes":[{"number":12,"title":"001-x","url":"https://github.com/o/r/pull/12","state":"OPEN","isDraft":true,"reviewDecision":"CHANGES_REQUESTED","commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SUCCESS"},{"__typename":"CheckRun","status":"COMPLETED","conclusion":"FAILURE"},{"__typename":"CheckRun","status":"IN_PROGRESS","conclusion":null},{"__typename":"StatusContext","state":"SUCCESS"}]}}}}]}}]}}}}"#.to_owned(),
    )]);
    let ctx = Ctx::fixed(&api.url, Some("t"));
    let p = github::pr_for(&ctx, dir.path(), "speccoder/001-x")
        .unwrap()
        .unwrap();
    assert_eq!((p.number, p.state.as_str(), p.draft), (12, "OPEN", true));
    assert_eq!(p.review.as_deref(), Some("CHANGES_REQUESTED"));
    assert_eq!(p.checks, (2, 1, 1));
    let sent = api.requests.lock().unwrap()[0].body.clone();
    assert!(
        sent.contains("\"head\":\"speccoder/001-x\"") && sent.contains("\"owner\":\"o\""),
        "{sent}"
    );
}

#[test]
fn an_enterprise_remote_speaks_to_its_own_host() {
    let r = Repo::parse("https://ghe.corp.example/team/svc.git").unwrap();
    assert_eq!(r.host, "ghe.corp.example");
    let (rest, graphql) = github::http::endpoints(&r.host, None);
    assert_eq!(rest, "https://ghe.corp.example/api/v3");
    assert_eq!(graphql, "https://ghe.corp.example/api/graphql");
    assert_eq!(
        github::http::endpoints("github.com", None).0,
        "https://api.github.com"
    );
}

#[test]
fn the_device_flow_waits_for_the_code_then_has_the_token() {
    let pending = fake_github::serve(vec![
        (
            "POST /login/device/code",
            200,
            r#"{"device_code":"dc","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","interval":1,"expires_in":30}"#.to_owned(),
        ),
        ("POST /login/oauth/access_token", 200, r#"{"access_token":"gho_new","token_type":"bearer"}"#.to_owned()),
    ]);
    let p = github::auth::start(&pending.url, "client").unwrap();
    assert_eq!(
        (p.user_code.as_str(), p.verification_uri.as_str()),
        ("ABCD-1234", "https://github.com/login/device")
    );
    let token = github::auth::wait(&pending.url, "client", &p).unwrap();
    assert_eq!(token.expose(), "gho_new");
    let sent = pending.requests.lock().unwrap()[1].body.clone();
    assert!(sent.contains("grant_type=urn"), "{sent}");
    assert!(
        sent.contains("client_id=client") && sent.contains("device_code=dc"),
        "{sent}"
    );

    let denied = fake_github::serve(vec![
        (
            "POST /login/device/code",
            200,
            r#"{"device_code":"dc","user_code":"X","verification_uri":"u","interval":1,"expires_in":30}"#.to_owned(),
        ),
        ("POST /login/oauth/access_token", 200, r#"{"error":"access_denied"}"#.to_owned()),
    ]);
    let p = github::auth::start(&denied.url, "client").unwrap();
    assert_eq!(
        github::auth::wait(&denied.url, "client", &p).unwrap_err(),
        "the sign-in was denied"
    );
}

#[test]
fn a_pull_request_is_looked_up_where_the_remote_points() {
    let dir = repo();
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "upstream",
                "https://github.com/up/stream.git"
            ])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let up = github::repo(dir.path(), Some("upstream")).unwrap();
    assert_eq!(up.slug(), "up/stream");
    let api = fake_github::serve(vec![(
        "POST /graphql",
        200,
        r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#.to_owned(),
    )]);
    let ctx = Ctx::fixed(&api.url, Some("t"));
    assert!(
        github::pr_in(&ctx, &up, "speccoder/001-x")
            .unwrap()
            .is_none()
    );
    let sent = api.requests.lock().unwrap()[0].body.clone();
    assert!(
        sent.contains("\"owner\":\"up\"") && sent.contains("\"name\":\"stream\""),
        "{sent}"
    );
}

/// A project with a feature, on a repository whose remote is o/r.
fn project() -> tempfile::TempDir {
    let dir = repo();
    for (p, t) in [
        (".specify/memory/constitution.md", "# Constitution\n"),
        (".specify/init-options.json", "{}\n"),
        (
            "specs/001-greet/spec.md",
            "# Spec\n\n- **FR-001**: Greet.\n",
        ),
    ] {
        std::fs::create_dir_all(dir.path().join(p).parent().unwrap()).unwrap();
        std::fs::write(dir.path().join(p), t).unwrap();
    }
    dir
}

#[test]
fn an_issue_is_told_once_that_it_is_being_specified() {
    let dir = project();
    let api = fake_github::serve(vec![
        ("GET /repos/o/r/issues/42/comments", 200, "[]".to_owned()),
        (
            "POST /repos/o/r/issues/42/comments",
            201,
            r#"{"html_url":"https://github.com/o/r/issues/42#issuecomment-1"}"#.to_owned(),
        ),
        ("GET /user", 200, r#"{"login":"me"}"#.to_owned()),
        ("POST /repos/o/r/issues/42/assignees", 201, "{}".to_owned()),
    ]);
    let ctx = Ctx::fixed(&api.url, Some("t"));
    let r = github::comment(&ctx, dir.path(), Some("001-greet"), "#42", true);
    assert_eq!(
        r.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        r.message()
    );
    assert!(
        r.message().contains("as 001-greet, and assigned it to you"),
        "{}",
        r.message()
    );
    let reqs = api.requests.lock().unwrap().clone();
    let posted = reqs
        .iter()
        .find(|q| q.method == "POST" && q.path.ends_with("/comments"))
        .unwrap();
    assert!(
        posted.body.contains("speccoder:feature 001-greet"),
        "{}",
        posted.body
    );
    assert!(
        posted.body.contains("specs/001-greet/spec.md")
            && posted.body.contains("speccoder/001-greet"),
        "{}",
        posted.body
    );
    let assigned = reqs
        .iter()
        .find(|q| q.path.ends_with("/assignees"))
        .unwrap();
    assert!(assigned.body.contains("\"me\""), "{}", assigned.body);

    // Said already: nothing is posted again.
    let again = fake_github::serve(vec![(
        "GET /repos/o/r/issues/42/comments",
        200,
        r#"[{"body":"<!-- speccoder:feature 001-greet -->\nBeing specified…"}]"#.to_owned(),
    )]);
    let r = github::comment(
        &Ctx::fixed(&again.url, Some("t")),
        dir.path(),
        Some("001-greet"),
        "42",
        false,
    );
    assert!(r.message().contains("already says"), "{}", r.message());
    assert!(
        !again.calls().iter().any(|c| c.starts_with("POST")),
        "{:?}",
        again.calls()
    );
}

#[test]
fn the_app_is_the_default_way_in_and_a_missing_install_is_said() {
    // speccoder's GitHub App on github.com; an Enterprise host needs its own.
    assert_eq!(
        github::auth::client_id("github.com").as_deref(),
        std::env::var("SPECCODER_GITHUB_CLIENT_ID")
            .ok()
            .as_deref()
            .or(Some(github::auth::APP_CLIENT_ID))
    );
    if std::env::var_os("SPECCODER_GITHUB_CLIENT_ID").is_none() {
        assert!(github::auth::client_id("ghe.corp").is_none());
    }
    assert_eq!(
        github::auth::install_url(),
        "https://github.com/apps/speccoder-app/installations/new"
    );
    // Signed in, but the app is not installed where the repository is.
    let dir = repo();
    let api = fake_github::serve(vec![]);
    let todo = github::reach(&Ctx::fixed(&api.url, Some("t")), dir.path()).unwrap();
    assert!(
        todo.contains("install the speccoder-app app") && todo.contains("installations/new"),
        "{todo}"
    );
    for (r, n) in [
        ("42", 42),
        ("#42", 42),
        ("https://github.com/o/r/issues/42", 42),
    ] {
        assert_eq!(github::issue_number(r), Some(n), "{r}");
    }
    assert_eq!(github::issue_number("abc"), None);
}
