//! GitHub, built in: its REST and GraphQL APIs, over TLS, as the
//! person — a token from the environment or the one `speccoder auth login`
//! kept in the credential store. No `gh` needed. Reading is free (issues, a
//! branch's pull request); writing is `pr`, which a person confirms.
//! github.com and Enterprise hosts alike: the host is the remote's.

pub mod auth;
pub mod http;

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};
use speccoder_core::{
    AuthReport, CommandData, CommandResult, CommandStatus, IssueEntry, IssuesReport,
};

use http::Api;

/// How speccoder reaches GitHub: where its API answers and whose token it
/// uses. Commands take it from the environment ([`Ctx::from_env`]); tests
/// point it at a stand-in ([`Ctx::fixed`]).
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    base: Option<String>,
    fixed: Option<Option<auth::Secret>>,
}

impl Ctx {
    /// `SPECCODER_GITHUB_API` (a proxy) if set; tokens from the
    /// environment, else the keychain.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            base: std::env::var("SPECCODER_GITHUB_API")
                .ok()
                .filter(|b| !b.trim().is_empty()),
            fixed: None,
        }
    }

    /// The API at `base` with `token` (none: signed out); never the
    /// environment or the keychain.
    #[must_use]
    pub fn fixed(base: &str, token: Option<&str>) -> Self {
        Self {
            base: Some(base.to_owned()),
            fixed: Some(token.map(auth::Secret::new)),
        }
    }

    /// The token for `host`, and where it came from.
    #[must_use]
    pub fn token(&self, host: &str) -> Option<(auth::Secret, auth::Source)> {
        match &self.fixed {
            Some(t) => t.clone().map(|t| (t, auth::Source::Env("GH_TOKEN"))),
            None => auth::token(host),
        }
    }

    fn api(&self, host: &str, token: auth::Secret) -> Api {
        Api::new(host, token, self.base.as_deref())
    }

    fn signed_in(&self, host: &str) -> Result<Api, String> {
        self.token(host)
            .map(|(t, _)| self.api(host, t))
            .ok_or_else(|| {
                format!(
                    "not signed in to {host}: `{}` (or set GH_TOKEN)",
                    login_command(host)
                )
            })
    }

    /// Whether a token is kept in the keychain (not for a fixed context).
    fn keeps(&self) -> bool {
        self.fixed.is_none()
    }
}

/// A repository on a GitHub host, as a git remote names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// `github.com` or an Enterprise hostname.
    pub host: String,
    /// The owner (user or organisation).
    pub owner: String,
    /// The repository's name.
    pub name: String,
}

impl Repo {
    /// `owner/name`.
    #[must_use]
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Its address on the web.
    #[must_use]
    pub fn url(&self) -> String {
        format!("https://{}/{}/{}", self.host, self.owner, self.name)
    }

    /// Reads a remote URL: `https://host/o/r(.git)`, `git@host:o/r(.git)`,
    /// `ssh://git@host[:port]/o/r(.git)`. `None`: it names no `o/r`.
    #[must_use]
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (host, path) = if let Some(rest) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .or_else(|| url.strip_prefix("ssh://"))
            .or_else(|| url.strip_prefix("git://"))
        {
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?.split(':').next()?;
            (host, path)
        } else {
            // scp-like: user@host:owner/name
            let (left, path) = url.split_once(':')?;
            (left.rsplit('@').next()?, path)
        };
        let path = path.trim_matches('/').trim_end_matches(".git");
        let mut parts = path.split('/');
        let (owner, name) = (parts.next()?, parts.next()?);
        if parts.next().is_some() || owner.is_empty() || name.is_empty() || host.is_empty() {
            return None;
        }
        Some(Self {
            host: host.to_ascii_lowercase(),
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

/// The repository `remote` (default `origin`, else the first) names.
///
/// # Errors
///
/// No remote, or one that names no GitHub repository.
pub fn repo(root: &Path, remote: Option<&str>) -> Result<Repo, String> {
    let git = crate::git::Git::at(root);
    let remotes = git.line(&["remote"]).unwrap_or_default();
    let names: Vec<&str> = remotes
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    let chosen = match remote {
        Some(r) => r,
        None if names.contains(&"origin") => "origin",
        None => match names.first() {
            Some(r) => r,
            None => {
                return Err("this repository has no remote: `git remote add origin <url>` (GitHub's address for it)".to_owned());
            }
        },
    };
    let url = git
        .line(&["remote", "get-url", chosen])
        .map_err(|_| format!("no git remote `{chosen}`"))?;
    Repo::parse(&url)
        .ok_or_else(|| format!("the remote `{chosen}` ({url}) names no GitHub repository"))
}

/// What GitHub is to this repository.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    /// Signed in and the repository readable.
    pub ready: bool,
    /// `github.com` or the Enterprise host.
    pub host: Option<String>,
    /// Who speccoder acts as.
    pub login: Option<String>,
    /// Where the token came from (`GH_TOKEN`, `the keychain`).
    pub source: Option<String>,
    /// `owner/name`.
    pub repo: Option<String>,
    /// The repository's address.
    pub url: Option<String>,
    /// Why not, and what to do.
    pub reason: Option<String>,
    /// The command that fixes it, when one does.
    pub fix: Option<String>,
}

/// Whether speccoder can reach this repository on GitHub, and as whom.
#[must_use]
pub fn status(ctx: &Ctx, root: &Path) -> Status {
    let r = match repo(root, None) {
        Ok(r) => r,
        Err(e) => {
            return Status {
                fix: Some("git remote add origin <url>".to_owned()),
                reason: Some(e),
                ..Status::default()
            };
        }
    };
    let base = Status {
        host: Some(r.host.clone()),
        repo: Some(r.slug()),
        url: Some(r.url()),
        ..Status::default()
    };
    let Some((token, source)) = ctx.token(&r.host) else {
        return Status {
            reason: Some(format!("not signed in to {}", r.host)),
            fix: Some(login_command(&r.host)),
            ..base
        };
    };
    let api = ctx.api(&r.host, token);
    let login = match api.get("/user") {
        Ok(u) => u["login"].as_str().map(str::to_owned),
        Err(e) => {
            return Status {
                source: Some(source.word().to_owned()),
                fix: Some(login_command(&r.host)),
                reason: Some(e),
                ..base
            };
        }
    };
    match api.get(&format!("/repos/{}", r.slug())) {
        Ok(_) => Status {
            ready: true,
            login,
            source: Some(source.word().to_owned()),
            ..base
        },
        Err(e) => Status {
            login,
            source: Some(source.word().to_owned()),
            reason: Some(e),
            ..base
        },
    }
}

/// `speccoder auth login` for `host` (the host named when not github.com).
fn login_command(host: &str) -> String {
    if host == "github.com" {
        "speccoder auth login".to_owned()
    } else {
        format!("speccoder auth login --host {host}")
    }
}

/// One issue, read in full.
#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    /// `#n`.
    pub number: u64,
    /// Its title.
    pub title: String,
    /// Its text.
    pub body: String,
    /// Its address.
    pub url: String,
    /// Label names.
    pub labels: Vec<String>,
}

fn names(v: &Value, key: &str) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x[key].as_str().map(str::to_owned))
        .collect()
}

/// The repository's open issues (pull requests aside), assigned to the
/// person first, then the most recently updated; `features` maps an issue
/// URL to the feature whose spec names it.
///
/// # Errors
///
/// No GitHub remote, not signed in, or GitHub's refusal.
pub fn issues(
    ctx: &Ctx,
    root: &Path,
    features: &std::collections::BTreeMap<String, String>,
) -> Result<IssuesReport, String> {
    let r = repo(root, None)?;
    let api = ctx.signed_in(&r.host)?;
    let raw = api.get(&format!(
        "/repos/{}/issues?state=open&per_page=100&sort=updated",
        r.slug()
    ))?;
    let me = api
        .get("/user")
        .ok()
        .and_then(|u| u["login"].as_str().map(str::to_owned));
    let mut issues: Vec<IssueEntry> = raw
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i.get("pull_request").is_none())
        .map(|i| {
            let url = i["html_url"].as_str().unwrap_or_default().to_owned();
            let assignees = names(&i["assignees"], "login");
            IssueEntry {
                number: i["number"].as_u64().unwrap_or_default(),
                title: i["title"].as_str().unwrap_or_default().to_owned(),
                labels: names(&i["labels"], "name"),
                mine: me.as_ref().is_some_and(|m| assignees.contains(m)),
                assignees,
                updated: i["updated_at"].as_str().map(str::to_owned),
                feature: features.get(&url).cloned(),
                url,
            }
        })
        .collect();
    issues.sort_by(|a, b| b.mine.cmp(&a.mine).then_with(|| b.updated.cmp(&a.updated)));
    Ok(IssuesReport {
        repo: Some(r.slug()),
        issues,
    })
}

/// One issue by number, `#n` or address.
///
/// # Errors
///
/// Not a reference, or GitHub's reason.
pub fn issue(ctx: &Ctx, root: &Path, reference: &str) -> Result<Issue, String> {
    let r = repo(root, None)?;
    let reference = reference.trim().trim_start_matches('#');
    let number = if reference.chars().all(|c| c.is_ascii_digit()) && !reference.is_empty() {
        reference.to_owned()
    } else {
        match reference
            .strip_prefix("https://")
            .and_then(|rest| rest.split_once("/issues/"))
            .map(|(_, n)| n.trim_end_matches('/'))
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        {
            Some(n) => n.to_owned(),
            None => {
                return Err(format!(
                    "`{reference}` is no issue: a number (42, #42) or its address"
                ));
            }
        }
    };
    let api = ctx.signed_in(&r.host)?;
    let i = api.get(&format!("/repos/{}/issues/{number}", r.slug()))?;
    if i.get("pull_request").is_some() {
        return Err(format!("#{number} is a pull request, not an issue"));
    }
    Ok(Issue {
        number: i["number"].as_u64().unwrap_or_default(),
        title: i["title"].as_str().unwrap_or_default().to_owned(),
        body: i["body"].as_str().unwrap_or_default().to_owned(),
        url: i["html_url"].as_str().unwrap_or_default().to_owned(),
        labels: names(&i["labels"], "name"),
    })
}

/// A pull request's diff against its merge base and its head commit,
/// fetched read-only (no checkout of the contributor's branch, no CI
/// token): what `speccoder bug --candidate pr:N` judges.
///
/// # Errors
///
/// No repository, not signed in, or GitHub's reason.
pub fn pull_diff(ctx: &Ctx, root: &Path, number: u64) -> Result<(String, String), String> {
    let r = repo(root, None)?;
    let api = ctx.signed_in(&r.host)?;
    let pull = api.get(&format!("/repos/{}/pulls/{number}", r.slug()))?;
    let head = pull["head"]["sha"].as_str().unwrap_or_default().to_owned();
    let diff = api.get_diff(&format!("/repos/{}/pulls/{number}", r.slug()))?;
    Ok((diff, head))
}

/// The repository's `owner/repo`, if it has a GitHub remote.
#[must_use]
pub fn slug_of(root: &Path) -> Option<String> {
    repo(root, None).ok().map(|r| r.slug())
}

/// What a feature started from `issue` is asked to specify: its title, its
/// text, and its address — which the spec keeps, so `pr` can close it.
#[must_use]
pub fn description(issue: &Issue) -> String {
    let body = issue.body.trim();
    if body.is_empty() {
        format!("{} (GitHub issue {})", issue.title, issue.url)
    } else {
        format!("{}\n\n{body}\n\nGitHub issue: {}", issue.title, issue.url)
    }
}

/// GitHub issue addresses in `text` (a spec), in order, once each.
#[must_use]
pub fn issue_urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in text.match_indices("https://") {
        let rest = &text[i..];
        let end = rest
            .find(|c: char| {
                c.is_whitespace() || matches!(c, ')' | ']' | '>' | '"' | '\'' | ',' | '`')
            })
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ';', ':']);
        // https://host/owner/repo/issues/123 — on github.com or an enterprise host.
        let parts: Vec<&str> = url.trim_start_matches("https://").split('/').collect();
        let issue = parts.len() == 5
            && parts[3] == "issues"
            && !parts[4].is_empty()
            && parts[4].chars().all(|c| c.is_ascii_digit());
        if issue && !out.iter().any(|u| u == url) {
            out.push(url.to_owned());
        }
    }
    out
}

/// A branch's pull request.
#[derive(Debug, Clone, Serialize)]
pub struct Pull {
    /// `#n`.
    pub number: u64,
    /// Its title.
    pub title: String,
    /// Its address.
    pub url: String,
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// A draft.
    pub draft: bool,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or none.
    pub review: Option<String>,
    /// Checks that passed, failed, and are still running.
    pub checks: (u32, u32, u32),
}

/// One query: the newest pull request whose head is the branch, its review
/// decision and its checks' rollup.
const PR_FOR: &str = "query($owner:String!,$name:String!,$head:String!){repository(owner:$owner,name:$name){pullRequests(headRefName:$head,first:1,orderBy:{field:CREATED_AT,direction:DESC}){nodes{number title url state isDraft reviewDecision commits(last:1){nodes{commit{statusCheckRollup{contexts(first:100){nodes{__typename ... on CheckRun{status conclusion} ... on StatusContext{state}}}}}}}}}}}";

/// The pull request whose head is `branch`, if there is one.
///
/// # Errors
///
/// No GitHub remote, not signed in, or GitHub's refusal.
pub fn pr_for(ctx: &Ctx, root: &Path, branch: &str) -> Result<Option<Pull>, String> {
    pr_in(ctx, &repo(root, None)?, branch)
}

/// [`pr_for`] in `r` (the repository a given remote names).
///
/// # Errors
///
/// Not signed in, or GitHub's refusal.
pub fn pr_in(ctx: &Ctx, r: &Repo, branch: &str) -> Result<Option<Pull>, String> {
    let api = ctx.signed_in(&r.host)?;
    let data = api.graphql(
        PR_FOR,
        json!({ "owner": r.owner, "name": r.name, "head": branch }),
    )?;
    let Some(p) = data["repository"]["pullRequests"]["nodes"]
        .as_array()
        .and_then(|n| n.first())
    else {
        return Ok(None);
    };
    let (mut ok, mut bad, mut running) = (0, 0, 0);
    let contexts = &p["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"];
    for c in contexts.as_array().into_iter().flatten() {
        // Check runs have `status`/`conclusion`; status contexts have `state`.
        let conclusion = c["conclusion"]
            .as_str()
            .or_else(|| c["state"].as_str())
            .unwrap_or("");
        let status = c["status"].as_str().unwrap_or("COMPLETED");
        match (status, conclusion) {
            (s, _) if s != "COMPLETED" => running += 1,
            (_, "SUCCESS" | "NEUTRAL" | "SKIPPED") => ok += 1,
            (_, "PENDING" | "EXPECTED") => running += 1,
            _ => bad += 1,
        }
    }
    Ok(Some(Pull {
        number: p["number"].as_u64().unwrap_or_default(),
        title: p["title"].as_str().unwrap_or_default().to_owned(),
        url: p["url"].as_str().unwrap_or_default().to_owned(),
        state: p["state"].as_str().unwrap_or_default().to_owned(),
        draft: p["isDraft"].as_bool().unwrap_or(false),
        review: p["reviewDecision"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        checks: (ok, bad, running),
    }))
}

/// Opens a draft pull request from `head` into `base`: its number and
/// address.
///
/// # Errors
///
/// Not signed in, or GitHub's refusal.
pub fn open_draft(
    ctx: &Ctx,
    repo: &Repo,
    base: &str,
    head: &str,
    title: &str,
    body: &str,
) -> Result<(u64, String), String> {
    let api = ctx.signed_in(&repo.host)?;
    let p = api.rest(
        reqwest::Method::POST,
        &format!("/repos/{}/pulls", repo.slug()),
        Some(&json!({ "title": title, "head": head, "base": base, "body": body, "draft": true })),
    )?;
    Ok((
        p["number"].as_u64().unwrap_or_default(),
        p["html_url"].as_str().unwrap_or_default().to_owned(),
    ))
}

/// The comment that tells an issue a feature is being specified from it;
/// the hidden marker keeps it said once per feature.
#[must_use]
pub fn announcement(feature: &str, spec: &str, branch: &str) -> String {
    format!(
        "<!-- speccoder:feature {feature} -->\nBeing specified as **`{feature}`** with [speccoder](https://hupe1980.github.io/speccoder/): the spec is `{spec}`, the work goes to `{branch}`, and its draft pull request will close this issue — every task with the evidence behind it."
    )
}

/// What [`announce`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Announced {
    /// The comment's address.
    Commented(String),
    /// The feature had said so already.
    Already,
}

/// Comments on issue `number` that `feature` is being specified from it
/// (once per feature), and assigns it to the person when `assign`.
///
/// # Errors
///
/// No GitHub remote, not signed in, or GitHub's refusal.
pub fn announce(
    ctx: &Ctx,
    root: &Path,
    number: u64,
    feature: &str,
    spec: &str,
    assign: bool,
) -> Result<Announced, String> {
    let r = repo(root, None)?;
    let api = ctx.signed_in(&r.host)?;
    let marker = format!("<!-- speccoder:feature {feature} -->");
    let comments = api.get(&format!(
        "/repos/{}/issues/{number}/comments?per_page=100",
        r.slug()
    ))?;
    let said = comments
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["body"].as_str().is_some_and(|b| b.contains(&marker)));
    let out = if said {
        Announced::Already
    } else {
        let branch = format!("speccoder/{feature}");
        let c = api.rest(
            reqwest::Method::POST,
            &format!("/repos/{}/issues/{number}/comments", r.slug()),
            Some(&json!({ "body": announcement(feature, spec, &branch) })),
        )?;
        Announced::Commented(c["html_url"].as_str().unwrap_or_default().to_owned())
    };
    if assign {
        let me = api.get("/user")?;
        let login = me["login"].as_str().unwrap_or_default();
        api.rest(
            reqwest::Method::POST,
            &format!("/repos/{}/issues/{number}/assignees", r.slug()),
            Some(&json!({ "assignees": [login] })),
        )?;
    }
    Ok(out)
}

/// The issue number in `reference` (`42`, `#42`, its address).
#[must_use]
pub fn issue_number(reference: &str) -> Option<u64> {
    reference
        .trim()
        .trim_end_matches('/')
        .trim_start_matches('#')
        .rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
}

/// `speccoder issue comment N [--assign]`: tells issue `reference` that the
/// current feature (or `feature`) is being specified from it.
#[must_use]
pub fn comment(
    ctx: &Ctx,
    root: &Path,
    feature: Option<&str>,
    reference: &str,
    assign: bool,
) -> CommandResult {
    let Some(number) = issue_number(reference) else {
        return CommandResult::new(
            "issue",
            CommandStatus::UsageError,
            &format!("`{reference}` is no issue: a number (42, #42) or its address"),
        )
        .with_code("USAGE");
    };
    let inspected = crate::inspect(root, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return CommandResult::carried("issue", &inspected);
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return CommandResult::new("issue", CommandStatus::SelectionNeeded, inspected.message());
    };
    let format = crate::format::of(snapshot);
    let spec = format.requirements_file().map_or_else(
        || f.feature.dir.clone(),
        |file| format!("{}/{file}", f.feature.dir.trim_end_matches('/')),
    );
    let assigned = if assign {
        ", and assigned it to you"
    } else {
        ""
    };
    match announce(ctx, root, number, &f.feature.name, &spec, assign) {
        Ok(Announced::Commented(url)) => CommandResult::ok(
            "issue",
            &format!(
                "commented on #{number} that it is being specified as {}{assigned}: {url}",
                f.feature.name
            ),
        ),
        Ok(Announced::Already) => CommandResult::ok(
            "issue",
            &format!(
                "#{number} already says it is being specified as {}{assigned}",
                f.feature.name
            ),
        ),
        Err(e) => refused("issue", &e),
    }
}

/// Whether the signed-in account can see the repository at `root` — after a
/// sign-in through the app, whether the app is installed there. `None`: it
/// can; else what to do.
#[must_use]
pub fn reach(ctx: &Ctx, root: &Path) -> Option<String> {
    let r = repo(root, None).ok()?;
    let api = ctx.signed_in(&r.host).ok()?;
    match api.get(&format!("/repos/{}", r.slug())) {
        Ok(_) => None,
        Err(e) if e.contains("was not found") && r.host == "github.com" => Some(format!(
            "{} cannot be reached yet: install the {} app on it — {}",
            r.slug(),
            auth::APP_SLUG,
            auth::install_url()
        )),
        Err(e) => Some(e),
    }
}

/// Brings pull request `number`'s title and description up to date.
///
/// # Errors
///
/// Not signed in, or GitHub's refusal.
pub fn update(ctx: &Ctx, repo: &Repo, number: u64, title: &str, body: &str) -> Result<(), String> {
    let api = ctx.signed_in(&repo.host)?;
    api.rest(
        reqwest::Method::PATCH,
        &format!("/repos/{}/pulls/{number}", repo.slug()),
        Some(&json!({ "title": title, "body": body })),
    )
    .map(|_| ())
}

/// The issue URLs each feature's spec names: issue URL → feature.
#[must_use]
pub fn started(root: &Path) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let listed = crate::current::list(root, None);
    let Some(CommandData::Features(f)) = listed.data() else {
        return out;
    };
    for feature in &f.features {
        for (_, _, text) in crate::documents::sources(root, Some(&feature.name)) {
            for url in issue_urls(&text) {
                out.entry(url).or_insert_with(|| feature.name.clone());
            }
        }
    }
    out
}

/// `speccoder issue`: the open issues.
#[must_use]
pub fn list(ctx: &Ctx, root: &Path) -> CommandResult {
    match issues(ctx, root, &started(root)) {
        Ok(report) => {
            let n = report.issues.len();
            let msg = format!(
                "{n} open issue{}{}",
                if n == 1 { "" } else { "s" },
                report
                    .repo
                    .as_deref()
                    .map(|r| format!(" in {r}"))
                    .unwrap_or_default()
            );
            CommandResult::new("issue", CommandStatus::Ok, &msg)
                .with_data(CommandData::Issues(Box::new(report)))
        }
        Err(e) => refused("issue", &e),
    }
}

/// A repository of the person's, for the welcome's Clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoEntry {
    /// `owner/name`.
    pub slug: String,
    /// Its description.
    pub description: Option<String>,
    /// Private.
    pub private: bool,
    /// Last pushed (RFC 3339).
    pub pushed: Option<String>,
}

/// The repositories the person signed in as can reach on github.com,
/// the most recently updated first (read-only); and who they are.
///
/// # Errors
///
/// Not signed in, or GitHub's refusal.
pub fn repositories(ctx: &Ctx) -> Result<(String, Vec<RepoEntry>), String> {
    let api = ctx.signed_in("github.com")?;
    let me = api
        .get("/user")?
        .get("login")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let raw = api.get("/user/repos?per_page=100&sort=updated")?;
    let list = raw
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(RepoEntry {
                slug: r["full_name"].as_str()?.to_owned(),
                description: r["description"].as_str().map(str::to_owned),
                private: r["private"].as_bool().unwrap_or(false),
                pushed: r["pushed_at"].as_str().map(str::to_owned),
            })
        })
        .collect();
    Ok((me, list))
}

/// Whether `slug` names a repository (`owner/name`, GitHub's characters).
#[must_use]
pub fn valid_slug(slug: &str) -> bool {
    let ok = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    matches!(slug.split_once('/'), Some((o, n)) if ok(o) && ok(n) && !n.contains('/'))
}

/// Clones the person's repository `slug` from github.com into `target`
/// with git, as the person (their credentials, never a token of
/// speccoder's in the command). The interface has confirmed that this
/// reaches github.com. `SPECCODER_GIT_BASE` points at another place to
/// clone from (tests).
#[must_use]
pub fn clone(_ctx: &Ctx, slug: &str, target: &Path) -> CommandResult {
    let refuse = |status, m: &str| CommandResult::new("clone", status, m);
    // `~/x` as the person typed it.
    let expanded = match (target.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
        _ => target.to_path_buf(),
    };
    let target = expanded.as_path();
    if !target.is_absolute() {
        return refuse(
            CommandStatus::UsageError,
            &format!(
                "give the destination as an absolute path or ~/…, not {}",
                target.display()
            ),
        )
        .with_code("USAGE");
    }
    // Never inside another repository's worktree.
    let mut probe = target.to_path_buf();
    while !probe.is_dir() {
        if !probe.pop() {
            break;
        }
    }
    if let Ok(Some(top)) = crate::git::Git::at(&probe).toplevel() {
        return refuse(
            CommandStatus::GitPrecondition,
            &format!(
                "{} is inside the repository {}",
                target.display(),
                top.display()
            ),
        )
        .with_code("NEW-NESTED-REPOSITORY")
        .with_fix(vec!["→ choose a folder outside it".to_owned()]);
    }
    if !valid_slug(slug) {
        return refuse(
            CommandStatus::UsageError,
            &format!("`{slug}` is not a repository: give it as owner/name"),
        )
        .with_code("USAGE");
    }
    if target.exists() && std::fs::read_dir(target).map_or(true, |mut d| d.next().is_some()) {
        return refuse(
            CommandStatus::UsageError,
            &format!("{} is not empty", target.display()),
        )
        .with_code("NEW-TARGET-NOT-EMPTY")
        .with_fix(vec!["→ choose an empty folder or a new one".to_owned()]);
    }
    let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return refuse(
            CommandStatus::UsageError,
            "the destination needs a parent folder",
        );
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        return refuse(
            CommandStatus::Error,
            &format!("cannot create {}: {e}", parent.display()),
        );
    }
    // Tests point it elsewhere; a release build clones from github.com only.
    let base = std::env::var("SPECCODER_GIT_BASE")
        .ok()
        .filter(|b| cfg!(debug_assertions) && !b.trim().is_empty())
        .unwrap_or_else(|| "https://github.com".to_owned());
    let url = format!("{}/{slug}.git", base.trim_end_matches('/'));
    let dest = target.display().to_string();
    match crate::git::Git::at(parent).run(&["clone", "--quiet", "--", &url, &dest]) {
        Ok(_) => CommandResult::new(
            "clone",
            CommandStatus::Ok,
            &format!("cloned {slug} into {dest}"),
        )
        .with_next(vec![speccoder_core::next::NextStep {
            command: format!("speccoder init -C {dest}"),
            why: "set speccoder up there".to_owned(),
        }]),
        // The clone is git's, with the person's git credentials — not
        // speccoder's sign-in, which only lists the repositories.
        Err(e) => {
            let auth = [
                "Authentication failed",
                "could not read Username",
                "terminal prompts disabled",
                "Repository not found",
            ]
            .iter()
            .any(|m| e.0.contains(m));
            let why = if auth {
                format!(
                    "git could not sign in to clone {slug}: a private repository needs your git credentials (a credential helper, or an SSH remote) — speccoder's sign-in only lists repositories: {}",
                    e.0
                )
            } else {
                format!("git clone failed: {}", e.0)
            };
            refuse(CommandStatus::Error, &why).with_code("GITHUB-FAILED")
        }
    }
}

/// A GitHub failure as a result: its code and fix.
#[must_use]
pub fn refused(command: &str, reason: &str) -> CommandResult {
    let code = if reason.contains("has no remote") || reason.contains("no git remote") {
        "NO-REMOTE"
    } else if reason.contains("not signed in") || reason.contains("refused the token") {
        "GITHUB-AUTH"
    } else {
        "GITHUB-FAILED"
    };
    // The code's own status: a missing remote is a git precondition.
    let status = if code == "NO-REMOTE" {
        CommandStatus::GitPrecondition
    } else {
        CommandStatus::Error
    };
    CommandResult::new(command, status, &format!("GitHub: {reason}")).with_code(code)
}

/// `speccoder auth status`: who speccoder is on `host` (default: the
/// repository's, else github.com).
#[must_use]
pub fn auth_status(ctx: &Ctx, root: &Path, host: Option<&str>) -> CommandResult {
    let host = host.map_or_else(
        || repo(root, None).map_or_else(|_| "github.com".to_owned(), |r| r.host),
        str::to_owned,
    );
    let Some((token, source)) = ctx.token(&host) else {
        return CommandResult::new(
            "auth",
            CommandStatus::Ok,
            &format!("not signed in to {host}: `{}`", login_command(&host)),
        )
        .with_data(CommandData::Auth(Box::new(AuthReport {
            host,
            login: None,
            source: None,
        })));
    };
    match ctx.api(&host, token).get("/user") {
        Ok(u) => {
            let login = u["login"].as_str().map(str::to_owned);
            CommandResult::new(
                "auth",
                CommandStatus::Ok,
                &format!(
                    "signed in to {host} as {} (the token from {})",
                    login.as_deref().unwrap_or("?"),
                    source.word()
                ),
            )
            .with_data(CommandData::Auth(Box::new(AuthReport {
                host,
                login,
                source: Some(source.word().to_owned()),
            })))
        }
        Err(e) => refused("auth", &e),
    }
}

/// `speccoder auth login --with-token`: checks `token` against `host`, then
/// keeps it in the credential store.
#[must_use]
pub fn login_with_token(ctx: &Ctx, host: &str, token: &str) -> CommandResult {
    let token = auth::Secret::new(token);
    if token.expose().is_empty() {
        return CommandResult::new(
            "auth",
            CommandStatus::UsageError,
            "no token on stdin: `echo $TOKEN | speccoder auth login --with-token`",
        )
        .with_code("USAGE");
    }
    let login = match ctx.api(host, token.clone()).get("/user") {
        Ok(u) => u["login"].as_str().unwrap_or_default().to_owned(),
        Err(e) => return refused("auth", &e),
    };
    if ctx.keeps()
        && let Err(e) = auth::store(host, &token)
    {
        return CommandResult::new("auth", CommandStatus::ConfigError, &e).with_code("GITHUB-AUTH");
    }
    CommandResult::new(
        "auth",
        CommandStatus::Ok,
        &format!("signed in to {host} as {login}; the token is in the keychain"),
    )
    .with_data(CommandData::Auth(Box::new(AuthReport {
        host: host.to_owned(),
        login: Some(login),
        source: Some(auth::Source::Keychain.word().to_owned()),
    })))
}

/// `speccoder auth logout`: forgets `host`'s token.
#[must_use]
pub fn logout(host: &str) -> CommandResult {
    match auth::forget(host) {
        Ok(true) => CommandResult::ok(
            "auth",
            &format!("signed out of {host}: the token is gone from the keychain"),
        ),
        Ok(false) => CommandResult::ok("auth", &format!("no token for {host} in the keychain")),
        Err(e) => CommandResult::new("auth", CommandStatus::ConfigError, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_of_every_shape_name_their_repository() {
        for (url, slug, host) in [
            ("https://github.com/o/r.git", "o/r", "github.com"),
            ("https://github.com/o/r", "o/r", "github.com"),
            ("git@github.com:o/r.git", "o/r", "github.com"),
            ("ssh://git@github.com/o/r.git", "o/r", "github.com"),
            ("ssh://git@GHE.corp:2222/team/svc", "team/svc", "ghe.corp"),
            ("https://token@github.com/o/r.git", "o/r", "github.com"),
        ] {
            let r = Repo::parse(url).unwrap_or_else(|| panic!("{url}"));
            assert_eq!((r.slug().as_str(), r.host.as_str()), (slug, host), "{url}");
        }
        assert!(Repo::parse("/local/path").is_none());
        assert!(Repo::parse("https://github.com/o").is_none());
        assert!(Repo::parse("https://github.com/o/r/extra").is_none());
    }

    #[test]
    fn issue_urls_are_found_once_in_order() {
        let spec = "**Input**: Greet (GitHub issue https://github.com/o/r/issues/42).\n\
                    See https://github.com/o/r/issues/42 and https://ghe.corp/o/r/issues/7;\n\
                    not https://github.com/o/r/pull/3 nor https://github.com/o/r/issues/ nor https://github.com/o/r/issues/4x.";
        assert_eq!(
            issue_urls(spec),
            [
                "https://github.com/o/r/issues/42",
                "https://ghe.corp/o/r/issues/7"
            ]
        );
    }

    #[test]
    fn a_description_keeps_the_issue_s_address() {
        let i = Issue {
            number: 42,
            title: "Greet people".into(),
            body: "By name.".into(),
            url: "https://github.com/o/r/issues/42".into(),
            labels: vec![],
        };
        let d = description(&i);
        assert!(
            d.starts_with("Greet people\n\nBy name.")
                && d.ends_with("GitHub issue: https://github.com/o/r/issues/42"),
            "{d}"
        );
        assert_eq!(issue_urls(&d), ["https://github.com/o/r/issues/42"]);
    }
}
