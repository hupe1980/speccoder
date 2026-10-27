//! Durable attempt outcomes.
//!
//! A passed task is a commit on `speccoder/<feature>`. Every other outcome
//! — a failed verdict, a stop, an undemonstrated gap, an interrupted run —
//! is saved as a commit object whose tree is the worktree as the attempt
//! left it, parented on the branch tip, under
//! `refs/speccoder/attempts/<feature>/<task>/<run>`, with its outcome in
//! trailers. The worktree then goes back to where the attempt started, so
//! the next attempt never builds on (or freezes tests from) unfinished work,
//! and nothing is lost: `git diff speccoder/<feature> <ref>` shows it.
//! Git is the durable record; the journal can be replaced across upgrades.

use std::path::Path;

use crate::git::{Git, GitError};
use crate::worktree::{self, Checkpoint};

/// Where attempt refs live.
pub const PREFIX: &str = "refs/speccoder/attempts";

/// Field and record separators for `git for-each-ref` output.
const FS: char = '\u{1f}';
const RS: char = '\u{1e}';

/// An attempt saved under [`PREFIX`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// Task ID.
    pub task: String,
    /// `blocked`, `verification_failed`, `not_demonstrated`, `interrupted`, …
    pub outcome: String,
    /// Why, in one or a few lines.
    pub reason: String,
    /// The agentplane run, when there was one.
    pub run_id: Option<String>,
    /// The call that awaited approval (`{tool, arguments, reason}`), if any.
    pub pending: Option<serde_json::Value>,
    /// Model tokens the attempt used.
    pub usage: Option<serde_json::Value>,
    /// The full ref name.
    pub reference: String,
    /// The commit it points at.
    pub commit: String,
}

pub(crate) const IDENTITY: [&str; 6] = [
    "-c",
    "user.name=speccoder",
    "-c",
    "user.email=speccoder@users.noreply.invalid",
    "-c",
    "commit.gpgsign=false",
];

/// What an attempt ended with.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    /// The agentplane run, when there was one (it names the ref).
    pub run: Option<&'a str>,
    /// `blocked`, `verification_failed`, …
    pub outcome: &'a str,
    /// Why.
    pub reason: &'a str,
    /// The call awaiting approval, if the attempt stopped for one.
    pub pending: Option<&'a serde_json::Value>,
    /// Model tokens the attempt used (`cost`).
    pub usage: Option<&'a serde_json::Value>,
}

/// The `Usage:` trailer speccoder writes on its commits, or nothing.
#[must_use]
pub fn usage_trailer(usage: Option<&serde_json::Value>) -> String {
    let Some(u) = usage.filter(|u| u.is_object()) else {
        return String::new();
    };
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    // An external agent that reports its own cost (Claude Code) keeps it.
    let usd = u["cost_usd"]
        .as_f64()
        .map(|c| format!(" usd={c}"))
        .unwrap_or_default();
    format!(
        "Usage: input={} output={} cache_read={} cache_write={}{usd}\n",
        n("input_tokens"),
        n("output_tokens"),
        n("cache_read_tokens"),
        n("cache_write_tokens")
    )
}

/// Reads a `Usage:` trailer value back into a usage object.
#[must_use]
pub fn parse_usage(value: &str) -> Option<serde_json::Value> {
    let mut out = serde_json::Map::new();
    for part in value.split_whitespace() {
        let (k, v) = part.split_once('=')?;
        let key = match k {
            "input" => "input_tokens",
            "output" => "output_tokens",
            "cache_read" => "cache_read_tokens",
            "cache_write" => "cache_write_tokens",
            "usd" => {
                out.insert(
                    "cost_usd".to_owned(),
                    serde_json::json!(v.parse::<f64>().ok()?),
                );
                continue;
            }
            _ => continue,
        };
        out.insert(key.to_owned(), serde_json::json!(v.parse::<u64>().ok()?));
    }
    (!out.is_empty()).then_some(serde_json::Value::Object(out))
}

/// Saves the worktree at `dir` as an attempt of `feature/task`.
pub fn save(
    dir: &Path,
    tmp: &Path,
    feature: &str,
    task: &str,
    record: &Record<'_>,
) -> Result<Saved, GitError> {
    let Record {
        run,
        outcome,
        reason,
        pending,
        usage,
    } = *record;
    let git = Git::at(dir);
    let tree = worktree::checkpoint(dir, tmp)?;
    let head = git.line(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    // A file whose change holds a value copied from the person's secret
    // files is left out: a saved attempt is a ref anyone with the
    // repository sees.
    let (tree, left_out) = without_secrets(&git, tmp, &head, tree)?;
    let reason = if left_out.is_empty() {
        reason.to_owned()
    } else {
        format!(
            "{reason} ({} left out: it held a value copied from your checkout)",
            left_out.join(", ")
        )
    };
    let reason = reason.as_str();
    let message = format!(
        "speccoder attempt: {task} {outcome}\n\n{reason}\n\nSpec-Task: {feature}/{task}\nOutcome: {outcome}\n{}{}{}",
        run.map(|r| format!("Run: {r}\n")).unwrap_or_default(),
        usage_trailer(usage),
        // Compact JSON is one line: a trailer value.
        pending
            .map(|p| format!("Pending: {p}\n"))
            .unwrap_or_default()
    );
    let mut args = IDENTITY.to_vec();
    args.extend(["commit-tree", &tree.0, "-p", &head, "-m", &message]);
    let commit = git.line(&args)?;
    let name = run
        .filter(|r| {
            r.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map_or_else(
            || format!("{outcome}-{}", commit.get(..12).unwrap_or(&commit)),
            str::to_owned,
        );
    let reference = format!("{PREFIX}/{feature}/{task}/{name}");
    git.run(&["update-ref", &reference, &commit])?;
    Ok(Saved {
        task: task.to_owned(),
        outcome: outcome.to_owned(),
        reason: reason.to_owned(),
        run_id: run.map(str::to_owned),
        pending: pending.cloned(),
        usage: usage.cloned(),
        reference,
        commit,
    })
}

/// `tree` without each file whose change from `head` adds a value
/// [`crate::secrets`] masks, and those files.
fn without_secrets(
    git: &Git,
    tmp: &Path,
    head: &str,
    tree: Checkpoint,
) -> Result<(Checkpoint, Vec<String>), GitError> {
    let changed = crate::git::nul_fields(&git.run(&[
        "diff",
        "--name-only",
        "-z",
        "--no-renames",
        head,
        &tree.0,
    ])?);
    let held: Vec<String> = changed
        .into_iter()
        .filter(|f| {
            git.run(&["diff", "-U0", "--no-color", head, &tree.0, "--", f])
                .is_ok_and(|p| crate::include::added_secret(&String::from_utf8_lossy(&p)).is_some())
        })
        .collect();
    if held.is_empty() {
        return Ok((tree, held));
    }
    let index = tmp.join(format!("attempt-index-{}", std::process::id()));
    let _ = std::fs::create_dir_all(tmp);
    git.run_with_index(&index, &["read-tree", &tree.0], None)?;
    let mut rm = vec!["update-index", "--force-remove", "--"];
    rm.extend(held.iter().map(String::as_str));
    git.run_with_index(&index, &rm, None)?;
    let out = git.run_with_index(&index, &["write-tree"], None);
    let _ = std::fs::remove_file(&index);
    let kept = String::from_utf8_lossy(&out?).trim().to_owned();
    Ok((Checkpoint(kept), held))
}

/// Makes the worktree at `dir` equal to its HEAD commit's tree.
pub fn restore_head(dir: &Path, tmp: &Path) -> Result<(), GitError> {
    let tree = Git::at(dir).line(&["rev-parse", "HEAD^{tree}"])?;
    worktree::restore(dir, tmp, &Checkpoint(tree)).map(|_| ())
}

/// The saved attempts of `feature`, newest first.
pub fn list(repo: &Path, feature: &str) -> Vec<Saved> {
    let format = format!(
        "--format=%(refname){FS}%(objectname){FS}%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Outcome,valueonly){FS}%(trailers:key=Run,valueonly){FS}%(trailers:key=Pending,valueonly){FS}%(trailers:key=Usage,valueonly){FS}%(contents:body){RS}"
    );
    let out = Git::at(repo)
        .run(&[
            "for-each-ref",
            "--sort=-committerdate",
            &format,
            &format!("{PREFIX}/{feature}/"),
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    out.split(RS)
        .filter_map(|record| {
            let f: Vec<&str> = record.trim_start_matches('\n').split(FS).collect();
            let [reference, commit, task, outcome, run, pending, usage, body] = f[..] else {
                return None;
            };
            let task = task.trim().strip_prefix(&format!("{feature}/"))?.to_owned();
            // The reason is the body before the trailer block.
            let reason = body
                .split("\n\nSpec-Task:")
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            let run = run.trim();
            Some(Saved {
                task,
                outcome: outcome.trim().to_owned(),
                reason,
                run_id: (!run.is_empty()).then(|| run.to_owned()),
                pending: serde_json::from_str(pending.trim()).ok(),
                usage: parse_usage(usage.trim()),
                reference: reference.to_owned(),
                commit: commit.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_holding_a_copied_value_is_left_out_of_a_saved_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args([
                        "-c",
                        "user.name=t",
                        "-c",
                        "user.email=t@e",
                        "-c",
                        "commit.gpgsign=false"
                    ])
                    .args(args)
                    .current_dir(r)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        std::fs::write(r.join("a.txt"), "a\n").unwrap();
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "init"]);
        crate::secrets::register("SNAP_TOKEN", "attempt-snap-Vb3 value");
        std::fs::write(r.join("a.txt"), "a\nb\n").unwrap();
        std::fs::write(r.join("snap.txt"), "token: attempt-snap-Vb3 value\n").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let saved = save(
            r,
            tmp.path(),
            "001-x",
            "T001",
            &Record {
                run: None,
                outcome: "failed",
                reason: "red",
                pending: None,
                usage: None,
            },
        )
        .unwrap();
        let g = Git::at(r);
        let files = g
            .run(&["ls-tree", "-r", "--name-only", &saved.commit])
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&files), "a.txt\n");
        assert!(
            saved.reason.contains("snap.txt left out"),
            "{}",
            saved.reason
        );
    }

    #[test]
    fn usage_trailers_round_trip_with_a_reported_cost() {
        let u = serde_json::json!({ "input_tokens": 100, "output_tokens": 5, "cache_read_tokens": 90, "cache_write_tokens": 3, "cost_usd": 0.0125 });
        let t = usage_trailer(Some(&u));
        assert_eq!(
            t,
            "Usage: input=100 output=5 cache_read=90 cache_write=3 usd=0.0125\n"
        );
        let back = parse_usage(t.trim().trim_start_matches("Usage: ")).unwrap_or_default();
        assert_eq!(back, u);
        let native = parse_usage("input=1 output=2 cache_read=0 cache_write=0").unwrap_or_default();
        assert!(native.get("cost_usd").is_none());
    }
}
