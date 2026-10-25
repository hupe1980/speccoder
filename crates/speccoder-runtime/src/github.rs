//! GitHub through the person's own `gh` (F-083): its login, its host, its
//! enterprise configuration — speccoder stores no token and asks for none.
//! Reading is free (issues, a branch's pull request); writing is `pr`,
//! which a person confirms. Every call runs without stdin, without prompts,
//! within a time limit, and its error is kept as the reason.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use speccoder_core::{CommandData, CommandResult, CommandStatus, IssueEntry, IssuesReport};

/// How long one `gh` call may take.
const LIMIT: Duration = Duration::from_secs(20);

/// The GitHub CLI: `SPECCODER_GH` (tests, a wrapper), else `gh`.
#[must_use]
pub fn gh() -> String {
    std::env::var("SPECCODER_GH")
        .ok()
        .filter(|g| !g.trim().is_empty())
        .unwrap_or_else(|| "gh".to_owned())
}

/// The repository has a remote: checked before `gh` (and its login) is
/// asked anything, so a missing remote is said first, not after a login.
fn remote(root: &Path) -> Result<(), String> {
    let remotes = crate::git::Git::at(root)
        .line(&["remote"])
        .unwrap_or_default();
    if remotes.trim().is_empty() {
        Err("this repository has no remote: `git remote add origin <url>` (GitHub's address for it)".to_owned())
    } else {
        Ok(())
    }
}

/// Runs `gh args…` in `root`: its stdout, or why not.
fn run(program: &str, root: &Path, args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                format!("`{program}` is not installed (https://cli.github.com)")
            }
            _ => format!("`{program}`: {e}"),
        })?;
    // Read both pipes while it runs: a full pipe would stall it.
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "`{program} {}` took longer than {}s",
                    args.join(" "),
                    LIMIT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        let why = stderr.trim();
        Err(if why.is_empty() {
            format!("`{program} {}` failed", args.join(" "))
        } else {
            why.to_owned()
        })
    }
}

/// What `gh` can do here.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    /// `gh` runs.
    pub installed: bool,
    /// It is logged in and sees this repository.
    pub ready: bool,
    /// `owner/name`.
    pub repo: Option<String>,
    /// The repository's address.
    pub url: Option<String>,
    /// Why not, and what to do.
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    name_with_owner: String,
    url: String,
}

/// Whether `gh` is there, logged in, and sees this repository.
#[must_use]
pub fn status(gh: &str, root: &Path) -> Status {
    let version = run(gh, root, &["--version"]);
    if let Err(e) = remote(root) {
        return Status {
            installed: version.is_ok(),
            reason: Some(e),
            ..Status::default()
        };
    }
    if let Err(e) = version {
        return Status {
            reason: Some(e),
            ..Status::default()
        };
    }
    match run(gh, root, &["repo", "view", "--json", "nameWithOwner,url"])
        .and_then(|t| serde_json::from_str::<Repo>(&t).map_err(|e| e.to_string()))
    {
        Ok(r) => Status {
            installed: true,
            ready: true,
            repo: Some(r.name_with_owner),
            url: Some(r.url),
            reason: None,
        },
        Err(e) => Status {
            installed: true,
            reason: Some(if e.contains("auth login") || e.contains("not logged") {
                "not logged in: `gh auth login`".to_owned()
            } else {
                e
            }),
            ..Status::default()
        },
    }
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
    #[serde(default)]
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssue {
    number: u64,
    title: String,
    #[serde(default)]
    body: String,
    url: String,
    #[serde(default)]
    labels: Vec<Named>,
    #[serde(default)]
    assignees: Vec<Named>,
    #[serde(default)]
    updated_at: Option<String>,
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

/// The login `gh` acts as, if it says.
fn me(gh: &str, root: &Path) -> Option<String> {
    run(gh, root, &["api", "user", "--jq", ".login"])
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The repository's open issues, assigned to the person first, then the
/// most recently updated; `features` maps an issue URL to the feature whose
/// spec names it.
///
/// # Errors
///
/// `gh`'s reason (not installed, not logged in, no GitHub remote).
pub fn issues(
    gh: &str,
    root: &Path,
    features: &std::collections::BTreeMap<String, String>,
) -> Result<IssuesReport, String> {
    remote(root)?;
    let text = run(
        gh,
        root,
        &[
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,url,labels,assignees,updatedAt",
        ],
    )?;
    let raw: Vec<RawIssue> =
        serde_json::from_str(&text).map_err(|e| format!("`gh issue list`: {e}"))?;
    let login = me(gh, root);
    let mut issues: Vec<IssueEntry> = raw
        .into_iter()
        .map(|i| {
            let assignees: Vec<String> = i.assignees.into_iter().map(|a| a.login).collect();
            IssueEntry {
                number: i.number,
                mine: login.as_ref().is_some_and(|l| assignees.contains(l)),
                feature: features.get(&i.url).cloned(),
                title: i.title,
                labels: i.labels.into_iter().map(|l| l.name).collect(),
                assignees,
                updated: i.updated_at,
                url: i.url,
            }
        })
        .collect();
    issues.sort_by(|a, b| b.mine.cmp(&a.mine).then_with(|| b.updated.cmp(&a.updated)));
    let repo = run(gh, root, &["repo", "view", "--json", "nameWithOwner"])
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["nameWithOwner"].as_str().map(str::to_owned));
    Ok(IssuesReport { repo, issues })
}

/// One issue by number, `#n` or address.
///
/// # Errors
///
/// Not a reference, or `gh`'s reason.
pub fn issue(gh: &str, root: &Path, reference: &str) -> Result<Issue, String> {
    remote(root)?;
    let r = reference.trim().trim_start_matches('#');
    let ok = r.chars().all(|c| c.is_ascii_digit()) && !r.is_empty()
        || (r.starts_with("https://")
            && r.contains("/issues/")
            && !r.contains(char::is_whitespace));
    if !ok {
        return Err(format!(
            "`{reference}` is no issue: a number (42, #42) or its address"
        ));
    }
    let text = run(
        gh,
        root,
        &["issue", "view", r, "--json", "number,title,body,url,labels"],
    )?;
    let i: RawIssue = serde_json::from_str(&text).map_err(|e| format!("`gh issue view`: {e}"))?;
    Ok(Issue {
        number: i.number,
        title: i.title,
        body: i.body,
        url: i.url,
        labels: i.labels.into_iter().map(|l| l.name).collect(),
    })
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

/// A branch's pull request, as `gh` sees it.
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

/// The pull request whose head is `branch`, if there is one.
///
/// # Errors
///
/// `gh`'s reason, other than "no pull request".
pub fn pr_for(gh: &str, root: &Path, branch: &str) -> Result<Option<Pull>, String> {
    let text = match run(
        gh,
        root,
        &[
            "pr",
            "view",
            branch,
            "--json",
            "number,title,url,state,isDraft,reviewDecision,statusCheckRollup",
        ],
    ) {
        Ok(t) => t,
        Err(e)
            if e.contains("no pull requests found")
                || e.contains("no open pull requests")
                || e.contains("Could not resolve") =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("`gh pr view`: {e}"))?;
    let (mut ok, mut bad, mut running) = (0, 0, 0);
    for c in v["statusCheckRollup"].as_array().into_iter().flatten() {
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
        number: v["number"].as_u64().unwrap_or_default(),
        title: v["title"].as_str().unwrap_or_default().to_owned(),
        url: v["url"].as_str().unwrap_or_default().to_owned(),
        state: v["state"].as_str().unwrap_or_default().to_owned(),
        draft: v["isDraft"].as_bool().unwrap_or(false),
        review: v["reviewDecision"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        checks: (ok, bad, running),
    }))
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
pub fn list(gh: &str, root: &Path) -> CommandResult {
    match issues(gh, root, &started(root)) {
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
        Err(e) => refused(&e),
    }
}

/// A `gh` failure as a result: its code and fix.
#[must_use]
pub fn refused(reason: &str) -> CommandResult {
    let code = if reason.contains("has no remote") {
        "NO-REMOTE"
    } else if reason.contains("not installed") {
        "GH-MISSING"
    } else if reason.contains("auth login") || reason.contains("not logged") {
        "GH-AUTH"
    } else {
        "GH-FAILED"
    };
    // The code's own status: a missing remote is a git precondition.
    let status = if code == "NO-REMOTE" {
        CommandStatus::GitPrecondition
    } else {
        CommandStatus::Error
    };
    CommandResult::new("issue", status, &format!("GitHub: {reason}")).with_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

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
