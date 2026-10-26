//! `speccoder pr`: hands a feature's work to review as a **draft**
//! pull request whose body is the evidence behind every tick.
//!
//! The one place speccoder talks to a git remote, and only when a person
//! runs it and confirms (or passes `--yes`); without that it is a dry run
//! that prints the body and the commands. It never merges, never marks the
//! pull request ready, and pushes only `speccoder/<feature>`, the
//! attestation notes (`refs/notes/speccoder`) and the held-out tests of
//! the tasks closed on the pushed head (`refs/speccoder/held-out/<unit>`),
//! none of them forced.

use std::fmt::Write as _;
use std::path::Path;

use speccoder_core::{CommandData, CommandResult, CommandStatus, PrReport, StatusReport};

use crate::git::Git;

/// How to hand off.
#[derive(Debug, Clone)]
pub struct PrOptions<'a> {
    /// The git remote (`origin`).
    pub remote: &'a str,
    /// The target branch; default: the branch the checkout is on.
    pub base: Option<&'a str>,
    /// Push and open (`false`: a dry run).
    pub open: bool,
    /// Hand off although the branch's full-tier checks failed (said in the body).
    pub force: bool,
    /// How GitHub is reached ([`crate::github::Ctx::from_env`]).
    pub github: &'a crate::github::Ctx,
}

/// Commit trailers on the branch: (task, `Mutation:` value, `Usage:` value).
fn trailers(repo: &Git, base: &str, branch: &str) -> Vec<(String, String, String)> {
    const FS: char = '\u{1f}';
    const RS: char = '\u{1e}';
    let format = format!(
        "--format=%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Mutation,valueonly){FS}%(trailers:key=Usage,valueonly){RS}"
    );
    let log = repo
        .run(&["log", &format, &format!("{base}..{branch}")])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    log.split(RS)
        .filter_map(|r| {
            let f: Vec<&str> = r.trim_start_matches('\n').split(FS).collect();
            let [task, mutation, usage] = f[..] else {
                return None;
            };
            let task = task.trim().rsplit('/').next()?.to_owned();
            (!task.is_empty()).then(|| (task, mutation.trim().to_owned(), usage.trim().to_owned()))
        })
        .collect()
}

/// The bug fixes on the branch: (task, `Bug:` value, `Fixes:` value) of
/// every commit carrying a `Fixes` trailer.
fn bug_trailers(repo: &Git, base: &str, branch: &str) -> Vec<(String, String, String)> {
    const FS: char = '\u{1f}';
    const RS: char = '\u{1e}';
    let format = format!(
        "--format=%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Bug,valueonly){FS}%(trailers:key=Fixes,valueonly){RS}"
    );
    let log = repo
        .run(&["log", &format, &format!("{base}..{branch}")])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    log.split(RS)
        .filter_map(|r| {
            let f: Vec<&str> = r.trim_start_matches('\n').split(FS).collect();
            let [task, bug, fixes] = f[..] else {
                return None;
            };
            let task = task.trim().rsplit('/').next()?.to_owned();
            (!task.is_empty() && !fixes.trim().is_empty())
                .then(|| (task, bug.trim().to_owned(), fixes.trim().to_owned()))
        })
        .collect()
}

/// The issues the branch's bug fixes close: a `Fixes: #N` trailer on a
/// commit whose `Bug:` outcome is `fixed`, for a task that is still
/// verified or attested (a task the status does not list — a trailer-only
/// bug task — stands on its trailers). Each issue once, in order.
#[must_use]
pub fn bug_fixes(status: &StatusReport, trailers: &[(String, String, String)]) -> Vec<u64> {
    let mut out = Vec::new();
    for (task, bug, fixes) in trailers {
        let holds = status
            .tasks
            .iter()
            .find(|t| t.id == *task)
            .is_none_or(|t| matches!(t.state.as_str(), "verified" | "attested"));
        if !bug.starts_with("fixed ") || !holds {
            continue;
        }
        for n in fixes
            .lines()
            .filter_map(|l| l.trim().trim_start_matches('#').parse::<u64>().ok())
        {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// A note's state for the table: `ssh SHA256:…` (signer and key; who the
/// key belongs to is `attest verify`'s answer), `unsigned` or `missing`.
fn attestation(state: &str) -> String {
    match state.strip_prefix("signed ") {
        Some(signer) => format!("`{signer}`"),
        None => state.to_owned(),
    }
}

/// How a reviewer checks the attestations themselves, when there are any.
/// `sigstore` is the identity and issuer the project says sign: printed
/// for the reviewer to confirm, never trusted on the project's word.
#[must_use]
pub fn verify_section(
    status: &StatusReport,
    remote: &str,
    sigstore: Option<(&str, &str)>,
) -> Option<String> {
    let attested = status
        .tasks
        .iter()
        .filter(|t| t.attestation.as_deref().is_some_and(|a| a != "missing"))
        .count();
    let trust = sigstore.map_or_else(
        || "--allowed-signers <file>".to_owned(),
        |(identity, issuer)| {
            format!("--certificate-identity {identity} --certificate-oidc-issuer {issuer}")
        },
    );
    (attested > 0).then(|| {
        format!(
            "\n**Attestations.** {attested} tick(s) carry in-toto statements (DSSE, SLSA VSA) in the git note `{REF}` on their commit. To check them yourself:\n\n```sh\ngit fetch {remote} {REF}:{REF}\nspeccoder attest verify --feature {} {trust}\n```\n",
            status.feature,
            REF = crate::attest::notes::REF,
        )
    })
}

/// The pull request body: totals, a table of every task's evidence, what
/// is open, mutation results and tokens, and what `verified` does not mean.
#[must_use]
pub fn body(status: &StatusReport, trailers: &[(String, String, String)]) -> String {
    let t = &status.totals;
    let ticked = t.verified + t.regression_only + t.attested + t.ticked_without_evidence;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "`{}`, implemented by speccoder. **{ticked} of {} tasks ticked**: {} verified (spec-derived acceptance tests went from red to green), {} regression only (the checks stayed green), {} attested by a person, {} ticked without evidence; {} skipped, {} open.\n",
        status.feature,
        status.tasks.len(),
        t.verified,
        t.regression_only,
        t.attested,
        t.ticked_without_evidence,
        t.skipped,
        t.ready + t.blocked
    );
    let _ = writeln!(
        out,
        "| Task | Evidence | Mutation | Attestation | Commit |\n|---|---|---|---|---|"
    );
    for task in &status.tasks {
        let mutation = trailers
            .iter()
            .find(|(id, m, _)| *id == task.id && !m.is_empty())
            .map(|(_, m, _)| m.replace(' ', ", "))
            .unwrap_or_default();
        let mut evidence = task.state.replace('_', " ");
        if let Some(n) = &task.note {
            let _ = write!(evidence, " — {n}");
        }
        if let Some(a) = &task.last_attempt {
            let _ = write!(
                evidence,
                " (last attempt: {}, `{}`)",
                a.outcome.replace('_', " "),
                a.reference
            );
        }
        let title: String = task.title.chars().take(70).collect();
        let _ = writeln!(
            out,
            "| {} {} | {} | {} | {} | {} |",
            task.id,
            title.replace('|', "\\|"),
            evidence.replace('|', "\\|"),
            mutation,
            task.attestation
                .as_deref()
                .map(attestation)
                .unwrap_or_default(),
            task.commit
                .as_deref()
                .map(|c| &c[..c.len().min(9)])
                .unwrap_or("")
        );
    }
    let (mut input, mut output) = (0u64, 0u64);
    for (_, _, usage) in trailers {
        if let Some(u) = crate::attempts::parse_usage(usage) {
            input += u["input_tokens"].as_u64().unwrap_or(0);
            output += u["output_tokens"].as_u64().unwrap_or(0);
        }
    }
    if input + output > 0 {
        let _ = writeln!(
            out,
            "\nTokens: {input} in, {output} out (`speccoder report --cost` has the breakdown)."
        );
    }
    if !status.convergence.is_empty() {
        let _ = writeln!(
            out,
            "\nConvergence appended {} round(s) of tasks after assessing the feature against its spec.",
            status.convergence.len()
        );
    }
    let _ = writeln!(
        out,
        "\n> `verified` means the spec-derived acceptance tests went from red to green and no check regressed. It does not mean the change is correct, complete or safe: review this like any contribution. Opened as a draft by `speccoder pr`; speccoder never merges."
    );
    out
}

/// Where the guidance to add the evidence check is.
pub const EVIDENCE_CHECK_DOCS: &str =
    "https://hupe1980.github.io/speccoder/docs/github/#evidence-check";

/// GitHub's cap on a pull request body, in characters.
pub const BODY_LIMIT: usize = 65_536;

/// The workflow under `.github/workflows/` that runs `speccoder-verify`,
/// if one does (repository-relative).
#[must_use]
pub fn evidence_workflow(repo_root: &Path) -> Option<String> {
    let dir = repo_root.join(".github/workflows");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .collect();
    files.sort();
    files.into_iter().find_map(|p| {
        let text = std::fs::read_to_string(&p).ok()?;
        text.contains("speccoder-verify").then(|| {
            format!(
                ".github/workflows/{}",
                p.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            )
        })
    })
}

/// `### Evidence tour`: every tick's tour, collapsed, as many as fit in
/// `room` characters (dropped last-first, counted).
#[must_use]
pub fn tours(tours: &[String], room: usize) -> String {
    if tours.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n### Evidence tour\n\n");
    let mut shown = 0;
    for t in tours {
        if out.chars().count() + t.chars().count() + 200 > room {
            break;
        }
        out.push_str(t);
        shown += 1;
    }
    if shown < tours.len() {
        out.push_str(&format!(
            "\n{} tour(s) not shown (GitHub's body limit) — `speccoder-verify tour --feature <unit> --task <id>`\n",
            tours.len() - shown
        ));
    }
    if shown == 0 && out.chars().count() > room {
        return String::new();
    }
    out
}

/// `speccoder pr`.
#[must_use]
pub fn pr(path: &Path, feature: Option<&str>, options: &PrOptions<'_>) -> CommandResult {
    let refuse = |status: CommandStatus, m: &str| CommandResult::new("pr", status, m);
    let status = crate::status::report(path, feature);
    let Some(CommandData::Report(report)) = status.data() else {
        return refuse(status.status(), status.message());
    };
    let Some(branch) = report.branch.clone() else {
        return refuse(
            CommandStatus::GitPrecondition,
            "speccoder has not worked on this feature yet (no speccoder branch)",
        )
        .with_code("NOTHING-TO-HAND-OFF");
    };
    let repo_root = match Git::at(path).toplevel() {
        Ok(Some(r)) => r,
        _ => {
            return refuse(CommandStatus::GitPrecondition, "not a git repository")
                .with_code("NO-REPOSITORY");
        }
    };
    let repo = Git::at(&repo_root);
    let base = match options.base {
        Some(b) => b.to_owned(),
        None => match repo.line(&["rev-parse", "--abbrev-ref", "HEAD"]) {
            Ok(b) if b != "HEAD" => b,
            _ => {
                return refuse(
                    CommandStatus::GitPrecondition,
                    "the checkout is not on a branch; pass --base",
                );
            }
        },
    };
    if base == branch {
        return refuse(
            CommandStatus::GitPrecondition,
            "the base is the speccoder branch itself",
        );
    }
    let ahead = repo
        .line(&["rev-list", "--count", &format!("{base}..{branch}")])
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    if ahead == 0 {
        return refuse(
            CommandStatus::GitPrecondition,
            &format!("{branch} has nothing that {base} does not have"),
        )
        .with_code("NOTHING-TO-HAND-OFF");
    }
    if !repo
        .succeeds(&["remote", "get-url", options.remote])
        .unwrap_or(false)
    {
        return refuse(
            CommandStatus::GitPrecondition,
            &format!("no git remote `{}`", options.remote),
        )
        .with_code("NO-REMOTE");
    }
    let t = &report.totals;
    let title = format!(
        "{}: {} verified, {} regression only, {} open (speccoder)",
        report.feature,
        t.verified,
        t.regression_only,
        t.ready + t.blocked
    );
    // A branch whose full-tier checks failed is not handed off unless forced.
    let full =
        crate::pipeline::full::read(&repo_root, &report.feature, &branch).filter(|t| !t.passed);
    if let Some(t) = full.as_ref().filter(|_| !options.force) {
        return refuse(
            CommandStatus::VerificationFailed,
            &format!(
                "the full-tier checks failed on {branch}: {}; fix them, or pass --force",
                t.failed.join(", ")
            ),
        );
    }
    let mut body = body(report, &trailers(&repo, &base, &branch));
    if let Some(t) = &full {
        body.push_str(&format!(
            "\n> **Full-tier checks failed** on this branch: {}. Handed off with `--force`.\n",
            t.failed.join(", ")
        ));
    }
    let attest = crate::verify::find_config(path)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|c| crate::substrate::run_config::parse(&c).ok())
        .map(|c| c.attest);
    let sigstore = attest
        .as_ref()
        .filter(|a| a.signer == crate::substrate::run_config::Signer::Sigstore)
        .and_then(|a| {
            a.sigstore_identity
                .as_deref()
                .zip(a.sigstore_issuer.as_deref())
        });
    // The issues the spec names: merged, the pull request closes them.
    if !report.issues.is_empty() {
        body.push('\n');
        for url in &report.issues {
            body.push_str(&format!("Closes {url}\n"));
        }
    }
    // The issues the bug fixes on the branch close (`speccoder bug --issue N`).
    let fixes = bug_fixes(report, &bug_trailers(&repo, &base, &branch));
    if !fixes.is_empty() {
        body.push('\n');
        for n in fixes {
            body.push_str(&format!("Fixes #{n}\n"));
        }
    }
    if let Some(section) = verify_section(report, options.remote, sigstore) {
        body.push_str(&section);
    }
    // Each closed task's tour, from its signed statements and commit.
    let tip = repo
        .line(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")])
        .unwrap_or_default();
    if let Ok(recorded) = crate::attest::branch::recorded(&repo_root, &tip, &report.feature) {
        let room = BODY_LIMIT.saturating_sub(body.chars().count());
        body.push_str(&tours(&crate::tour::markdown_all(&recorded), room));
    }
    // The held-out sets of exactly the tasks closed on the pushed head.
    let project_state = crate::format::at(path)
        .detect(path)
        .zip(crate::worktree::state_dir())
        .map(|(root, state)| crate::worktree::project_state_dir(&state, &root));
    let selection = project_state
        .as_ref()
        .map(|s| {
            let closed = crate::held_out::closed_on(&repo_root, &tip, &report.feature);
            crate::held_out::select_closed(s, &report.feature, &closed)
        })
        .unwrap_or_default();
    let held_out_ref = format!("{}/{}", crate::held_out::REF_PREFIX, report.feature);
    let evidence = evidence_workflow(&repo_root);
    let refspec = format!("{branch}:refs/heads/{branch}");
    let notes = crate::attest::notes::REF;
    let push_notes = repo
        .succeeds(&["rev-parse", "--verify", "-q", notes])
        .unwrap_or(false);
    let mut commands = vec![format!("git push {} {refspec}", options.remote)];
    if push_notes {
        commands.push(format!("git push {} {notes}", options.remote));
    }
    if !selection.sets.is_empty() {
        commands.push(format!("git push {} {held_out_ref}", options.remote));
    }
    // The repository the remote names on GitHub: where the draft opens.
    let github = crate::github::repo(&repo_root, Some(options.remote));
    commands.push(match &github {
        Ok(r) => format!(
            "open (or update) a draft pull request on {}: {branch} → {base} (GitHub API, as you)",
            r.slug()
        ),
        Err(e) => format!("no pull request can be opened: {e}"),
    });
    let mut out = PrReport {
        feature: report.feature.clone(),
        branch: branch.clone(),
        base: base.clone(),
        remote: options.remote.to_owned(),
        title: title.clone(),
        body: body.clone(),
        commands,
        opened: false,
        url: None,
        held_out_ref: (!selection.sets.is_empty()).then(|| held_out_ref.clone()),
        held_out_tasks: selection.tasks.clone(),
        held_out_withheld: selection.withheld,
        evidence_check: if evidence.is_some() {
            "present"
        } else {
            "absent"
        }
        .to_owned(),
    };
    if !options.open {
        let mut message =
            "dry run: nothing was pushed (confirm, or pass --yes, to push and open the draft)"
                .to_owned();
        message.push_str(&match &evidence {
            Some(w) => format!("\nevidence check: {w} runs speccoder-verify"),
            None => format!("\nevidence check: none — see {EVIDENCE_CHECK_DOCS}"),
        });
        if !selection.tasks.is_empty() {
            message.push_str(&format!(
                "\nheld-out: sets of {}",
                selection.tasks.join(", ")
            ));
        }
        if selection.withheld > 0 {
            message.push_str(&format!(
                "\nheld-out tests of {} open task(s) stay local",
                selection.withheld
            ));
        }
        if !selection.tampered.is_empty() {
            message.push_str(&format!(
                "\nheld-out sets changed since they judged, not published: {}",
                selection.tampered.join(", ")
            ));
        }
        return CommandResult::new("pr", CommandStatus::Ok, &message)
            .with_data(CommandData::Pr(Box::new(out)));
    }
    // Nothing leaves the machine unless the pull request can be opened:
    // a GitHub repository the remote names, and someone signed in there.
    let github = match github {
        Ok(r) => r,
        Err(e) => {
            return crate::github::refused("pr", &format!("{e}; nothing was pushed"))
                .with_data(CommandData::Pr(Box::new(out)));
        }
    };
    if options.github.token(&github.host).is_none() {
        return crate::github::refused(
            "pr",
            &format!(
                "not signed in to {}: `speccoder auth login` (or set GH_TOKEN); nothing was pushed",
                github.host
            ),
        )
        .with_data(CommandData::Pr(Box::new(out)));
    }
    if let Err(e) = repo.run(&["push", "--quiet", options.remote, &refspec]) {
        return CommandResult::new(
            "pr",
            CommandStatus::GitPrecondition,
            &format!("push failed: {}", e.0),
        )
        .with_data(CommandData::Pr(Box::new(out)));
    }
    // The notes are pushed as they are: a remote that has diverged is told
    // how to merge, never overwritten; the pull request opens regardless.
    let notes_warning = push_notes
        .then(|| repo.run(&["push", "--quiet", options.remote, &format!("{notes}:{notes}")]).err())
        .flatten()
        .map(|e| {
            format!(
                "; the attestation notes were not pushed ({}): `git fetch {r} {notes}:refs/notes/speccoder-remote && git notes --ref=speccoder merge refs/notes/speccoder-remote`, then `git push {r} {notes}`",
                e.0.lines().last().unwrap_or_default().trim(),
                r = options.remote,
            )
        })
        .unwrap_or_default();
    // The held-out ref, built on the pushed tip and its previous
    // publication: a fast-forward, never forced.
    let held_out_warning = if selection.sets.is_empty() {
        String::new()
    } else {
        let published = crate::held_out::publish_closed(
            &repo_root,
            &report.feature,
            &tip,
            &selection,
            &project_state
                .as_ref()
                .map_or_else(std::env::temp_dir, |s| s.join("tmp")),
        );
        let pushed = published.and_then(|_| {
            repo.run(&[
                "push",
                "--quiet",
                options.remote,
                &format!("{held_out_ref}:{held_out_ref}"),
            ])
            .map(|_| ())
            .map_err(|e| e.0)
        });
        match pushed {
            Ok(()) => String::new(),
            Err(e) => format!(
                "; the held-out tests were not pushed ({}): git fetch {r} {held_out_ref} and compare",
                e.lines().last().unwrap_or_default().trim(),
                r = options.remote,
            ),
        }
    };
    let notes_warning = format!("{notes_warning}{held_out_warning}");
    let failed = |e: &str, out: PrReport| {
        let r = crate::github::refused("pr", e);
        let status = r.status();
        r.reworded(status, &format!("pushed {branch}, but {e}"))
            .with_data(CommandData::Pr(Box::new(out)))
    };
    // A pull request for this branch already: the push updated its code;
    // its title and description are brought up to date with the evidence.
    // A second one is never opened.
    // The pull request in the repository this remote names (not `origin`'s).
    match crate::github::pr_in(options.github, &github, &branch) {
        Ok(Some(existing)) if existing.state == "OPEN" => {
            out.opened = true;
            out.url = Some(existing.url.clone());
            if let Err(e) =
                crate::github::update(options.github, &github, existing.number, &title, &body)
            {
                return failed(&e, out);
            }
            return CommandResult::new(
                "pr",
                CommandStatus::Ok,
                &format!(
                    "pushed {branch}: updated pull request #{} and its evidence ({}){notes_warning}",
                    existing.number, existing.url
                ),
            )
            .with_data(CommandData::Pr(Box::new(out)));
        }
        Ok(_) => {}
        Err(e) => return failed(&e, out),
    }
    match crate::github::open_draft(options.github, &github, &base, &branch, &title, &body) {
        Ok((number, url)) => {
            out.opened = true;
            out.url = Some(url.clone());
            CommandResult::new(
                "pr",
                CommandStatus::Ok,
                &format!("draft pull request #{number} opened: {url}{notes_warning}"),
            )
            .with_data(CommandData::Pr(Box::new(out)))
        }
        Err(e) => failed(&e, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workflow_running_the_verifier_is_found() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(evidence_workflow(d.path()), None);
        let w = d.path().join(".github/workflows");
        std::fs::create_dir_all(&w).unwrap();
        std::fs::write(w.join("ci.yml"), "jobs: {}\n").unwrap();
        std::fs::write(
            w.join("evidence.yaml"),
            "steps:\n  - uses: hupe1980/speccoder-verify@v0.2.0\n",
        )
        .unwrap();
        assert_eq!(
            evidence_workflow(d.path()).as_deref(),
            Some(".github/workflows/evidence.yaml")
        );
    }

    #[test]
    fn tours_fit_the_body_dropping_the_last_first() {
        let all: Vec<String> = (0..10)
            .map(|i| format!("<details>{i}{}</details>\n", "x".repeat(100)))
            .collect();
        let fitted = tours(&all, 700);
        assert!(fitted.chars().count() <= 700, "{}", fitted.chars().count());
        assert!(fitted.contains("<details>0"));
        assert!(!fitted.contains("<details>9"));
        assert!(fitted.contains("tour(s) not shown"));
        assert_eq!(tours(&[], 100), "");
        assert!(tours(&all, 100_000).contains("<details>9"));
    }
}
