//! `speccoder pr`: hands a feature's work to review as a **draft**
//! pull request whose body is the evidence behind every tick.
//!
//! The one place speccoder talks to a git remote, and only when a person
//! runs it and confirms (or passes `--yes`); without that it is a dry run
//! that prints the body and the commands. It never merges, never marks the
//! pull request ready, and pushes only `speccoder/<feature>` and the
//! attestation notes (`refs/notes/speccoder`, never forced).

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
    /// The GitHub CLI (`gh`).
    pub gh: &'a str,
    /// Push and open (`false`: a dry run).
    pub open: bool,
    /// Hand off although the branch's full-tier checks failed (said in the body).
    pub force: bool,
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
        "Spec Kit feature `{}`, implemented by speccoder. **{ticked} of {} tasks ticked**: {} verified (spec-derived acceptance tests went from red to green), {} regression only (the checks stayed green), {} attested by a person, {} ticked without evidence; {} skipped, {} open.\n",
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
            "\nTokens: {input} in, {output} out (`speccoder cost` has the breakdown)."
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
        );
    };
    let repo_root = match Git::at(path).toplevel() {
        Ok(Some(r)) => r,
        _ => return refuse(CommandStatus::GitPrecondition, "not a git repository"),
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
        );
    }
    if !repo
        .succeeds(&["remote", "get-url", options.remote])
        .unwrap_or(false)
    {
        return refuse(
            CommandStatus::GitPrecondition,
            &format!("no git remote `{}`", options.remote),
        );
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
    if let Some(section) = verify_section(report, options.remote, sigstore) {
        body.push_str(&section);
    }
    let refspec = format!("{branch}:refs/heads/{branch}");
    let notes = crate::attest::notes::REF;
    let push_notes = repo
        .succeeds(&["rev-parse", "--verify", "-q", notes])
        .unwrap_or(false);
    let mut commands = vec![format!("git push {} {refspec}", options.remote)];
    if push_notes {
        commands.push(format!("git push {} {notes}", options.remote));
    }
    commands.extend([format!(
        "{} pr create --draft --base {base} --head {branch} --title {title:?} --body-file <body>",
        options.gh
    )]);
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
    };
    if !options.open {
        return CommandResult::new(
            "pr",
            CommandStatus::Ok,
            "dry run: nothing was pushed (confirm, or pass --yes, to push and open the draft)",
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
    let body_file = std::env::temp_dir().join(format!("speccoder-pr-{}.md", std::process::id()));
    if let Err(e) = std::fs::write(&body_file, &body) {
        return refuse(CommandStatus::Error, &e.to_string());
    }
    let created = std::process::Command::new(options.gh)
        .args([
            "pr",
            "create",
            "--draft",
            "--base",
            &base,
            "--head",
            &branch,
            "--title",
            &title,
            "--body-file",
        ])
        .arg(&body_file)
        .current_dir(&repo_root)
        .stdin(std::process::Stdio::null())
        .output();
    let _ = std::fs::remove_file(&body_file);
    match created {
        Ok(o) if o.status.success() => {
            out.opened = true;
            out.url = String::from_utf8_lossy(&o.stdout)
                .lines()
                .rev()
                .find(|l| l.starts_with("http"))
                .map(str::to_owned);
            let msg = format!(
                "draft pull request opened{}{notes_warning}",
                out.url
                    .as_deref()
                    .map(|u| format!(": {u}"))
                    .unwrap_or_default()
            );
            CommandResult::new("pr", CommandStatus::Ok, &msg)
                .with_data(CommandData::Pr(Box::new(out)))
        }
        Ok(o) => CommandResult::new(
            "pr",
            CommandStatus::Error,
            &format!(
                "pushed {branch}, but `{} pr create` failed: {}",
                options.gh,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
        )
        .with_data(CommandData::Pr(Box::new(out))),
        Err(e) => CommandResult::new(
            "pr",
            CommandStatus::Error,
            &format!("pushed {branch}, but `{}` could not run: {e}", options.gh),
        )
        .with_data(CommandData::Pr(Box::new(out))),
    }
}
