//! An attempt's thread, built from what was recorded — the journal's model
//! turns and calls, git's ticks and saved attempts, the approvals record —
//! the same while the attempt runs and afterwards. And Explain: a waiting
//! call's facts, with no model.

use std::path::Path;

use serde_json::Value;
use speccoder_core::thread::{
    self as core, AttemptRef, Explanation, MAX_ROWS, Row, RowKind, Thread,
};
use speccoder_core::{ApprovalsReport, CommandData, StatusReport};

use crate::tools::policy::Object;

/// The tool servers speccoder's own tools answer on.
const OWN_SERVERS: &[&str] = &["fs", "proc", "git", "workspace"];

/// The thread of `task`'s attempt `run` (the newest when `None`) in the
/// project at `path`.
///
/// # Errors
///
/// No project or state directory, or no such task.
pub fn of(
    path: &Path,
    state: &Path,
    feature: &str,
    task: &str,
    run: Option<&str>,
    all: bool,
) -> Result<Thread, String> {
    let root = crate::format::at(path)
        .detect(path)
        .ok_or_else(|| "not a spec project".to_owned())?;
    let plane = crate::worktree::project_state_dir(state, &root);
    let name = format!("{feature}/{task}");
    let status = status_of(&root, feature);
    let ticked = status
        .as_ref()
        .and_then(|s| s.tasks.iter().find(|t| t.id == task).cloned());
    let saved: Vec<crate::attempts::Saved> = crate::attempts::list(&root, feature)
        .into_iter()
        .filter(|s| s.task == task)
        .collect();
    let live = crate::substrate::live_run(&name);
    let pending = pending_of(&root, feature, state, &name);

    let mut attempts: Vec<AttemptRef> = Vec::new();
    let mut push = |r: AttemptRef| {
        if !attempts.iter().any(|a| a.run == r.run) {
            attempts.push(r);
        }
    };
    if let Some(r) = &live {
        push(AttemptRef {
            run: r.clone(),
            outcome: "running".into(),
            at: None,
        });
    }
    for p in &pending {
        if let Some(r) = &p.attempt {
            push(AttemptRef {
                run: r.clone(),
                outcome: "waiting".into(),
                at: None,
            });
        }
    }
    if let Some(t) = &ticked
        && let Some(r) = &t.run_id
        && t.commit.is_some()
    {
        push(AttemptRef {
            run: r.clone(),
            outcome: t.state.clone(),
            at: None,
        });
    }
    for s in &saved {
        if let Some(r) = &s.run_id {
            push(AttemptRef {
                run: r.clone(),
                outcome: s.outcome.clone(),
                at: None,
            });
        }
    }
    // Only one of this task's own attempts: never another task's run.
    let chosen = run
        .filter(|r| attempts.iter().any(|a| a.run == *r))
        .map(str::to_owned)
        .or_else(|| attempts.first().map(|a| a.run.clone()));
    let attempt = chosen
        .as_ref()
        .and_then(|r| attempts.iter().find(|a| &a.run == r).cloned());

    let mut rows = Vec::new();
    if let Some(run) = &chosen {
        if run.starts_with("external_") {
            let agent = run
                .trim_start_matches("external_")
                .split('_')
                .next()
                .unwrap_or("the agent");
            rows.push(Row::divider(
                format!("This attempt was {agent}'s: its words and calls are in its own record"),
                "info",
            ));
        } else {
            match crate::pipeline::tokio_runtime().and_then(|rt| {
                rt.block_on(crate::substrate::records(&plane, run))
                    .map_err(|e| e.0)
            }) {
                Ok(history) => {
                    let decided = decisions(&plane, &name);
                    rows.extend(rows_of(&history, &decided));
                }
                Err(e) => rows.push(Row::divider(
                    format!("Its record cannot be read: {e}"),
                    "info",
                )),
            }
        }
        // A call that waits for a person, in place.
        rows.extend(
            pending
                .iter()
                .filter(|p| p.attempt.as_deref() == Some(run.as_str()))
                .map(waiting_row),
        );
        rows.extend(dividers(
            &root,
            run,
            ticked
                .as_ref()
                .filter(|t| t.run_id.as_deref() == Some(run.as_str())),
            saved
                .iter()
                .find(|s| s.run_id.as_deref() == Some(run.as_str())),
        ));
    }
    let mut rows = core::fold(rows);
    rows.iter_mut().for_each(clean_row);
    let more = if all {
        0
    } else {
        rows.len().saturating_sub(MAX_ROWS)
    };
    if more > 0 {
        rows.drain(..more);
    }
    Ok(Thread {
        feature: feature.to_owned(),
        task: task.to_owned(),
        live: live.is_some() && live == chosen,
        attempt,
        attempts,
        rows,
        more: u32::try_from(more).unwrap_or(u32::MAX),
    })
}

fn status_of(root: &Path, feature: &str) -> Option<StatusReport> {
    match crate::status::report(root, Some(feature)).data() {
        Some(CommandData::Report(r)) => Some((**r).clone()),
        _ => None,
    }
}

/// The person's decisions on `task`, oldest first: `(verb, object, reason)`.
/// A call reads the latest on its very object.
fn decisions(plane: &Path, task: &str) -> Vec<(String, Option<Object>, Option<String>)> {
    crate::approvals::Decisions::load(plane)
        .ok()
        .and_then(|d| d.tasks.get(task).cloned())
        .map(|t| {
            t.decided
                .into_iter()
                .map(|d| (d.verb, d.object, d.reason))
                .collect()
        })
        .unwrap_or_default()
}

/// The rows of a run's records: each model turn's thought and words, then
/// its calls, in the order they were recorded.
fn rows_of(
    history: &[agentplane::journal::Record],
    decided: &[(String, Option<Object>, Option<String>)],
) -> Vec<Row> {
    let escape = |t: &str| crate::approvals::present::escape(t.trim());
    let mut items: Vec<(u64, Row)> = Vec::new();
    let turns = agentplane::model::turns(history);
    for (n, t) in turns.iter().enumerate() {
        let turn = u32::try_from(n + 1).ok();
        let seq = t.seq;
        let Some(c) = &t.completion else { continue };
        if let Some(r) = c.reasoning.as_deref().filter(|r| !r.trim().is_empty()) {
            let text = escape(r);
            items.push((
                seq,
                Row {
                    turn,
                    conclusion: core::conclusion(&text),
                    text,
                    ..Row::of(RowKind::Thought)
                },
            ));
        }
        if let Some(w) = c
            .commentary
            .as_deref()
            .filter(|w| !w.trim().is_empty() && !c.tool_calls.is_empty())
        {
            items.push((
                seq,
                Row {
                    turn,
                    text: escape(w),
                    ..Row::of(RowKind::Words)
                },
            ));
        }
    }
    let turn_at = |seq: u64| u32::try_from(turns.iter().filter(|t| t.seq <= seq).count()).ok();
    for call in agentplane::journal::calls(history) {
        let d = call.descriptor();
        if d.kind != agentplane::tools::TOOL_CALL_KIND {
            continue;
        }
        let server = d.args["server"].as_str().unwrap_or_default();
        let tool = d.args["tool"].as_str().unwrap_or_default();
        // Resolving the object a decision would cover is speccoder's step,
        // not the model's call.
        if server == "fs" && tool == "resolve" {
            continue;
        }
        let name = if OWN_SERVERS.contains(&server) {
            tool.to_owned()
        } else {
            format!("mcp__{server}__{tool}")
        };
        let args = &d.args["arguments"];
        let seq = call.seq();
        items.push((
            seq,
            call_row(
                &name,
                args,
                call.output(),
                call.outcome().is_some(),
                decided,
                turn_at(seq),
            ),
        ));
    }
    items.sort_by_key(|(s, _)| *s);
    items.into_iter().map(|(_, r)| r).collect()
}

fn call_row(
    tool: &str,
    args: &Value,
    output: Option<&Value>,
    settled: bool,
    decided: &[(String, Option<Object>, Option<String>)],
    turn: Option<u32>,
) -> Row {
    let (verb, object) = core::verb_of(tool, args);
    let object = crate::approvals::present::escape(&object);
    let mut row = Row {
        turn,
        verb: Some(verb.clone()),
        object: Some(object),
        ..Row::of(RowKind::Call)
    };
    if verb == "Edited" {
        let (a, r) = core::edit_lines(
            args["old_string"].as_str().unwrap_or_default(),
            args["new_string"].as_str().unwrap_or_default(),
        );
        row.added = Some(a);
        row.removed = Some(r);
        row.detail = Some(mini_diff(
            args["old_string"].as_str().unwrap_or_default(),
            args["new_string"].as_str().unwrap_or_default(),
        ));
    } else if verb == "Wrote" {
        row.added =
            u32::try_from(args["content"].as_str().unwrap_or_default().lines().count()).ok();
    }
    let error = output.and_then(|o| o["error"].as_str()).map(str::to_owned);
    row.outcome = Some(
        match (settled, output) {
            (false, _) => "waiting",
            (true, None) => "failed",
            (true, Some(o)) if o["ok"] == Value::Bool(false) => {
                if error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("denied") || e.contains("rejected"))
                {
                    "refused"
                } else {
                    "failed"
                }
            }
            (true, Some(_)) => "ok",
        }
        .to_owned(),
    );
    if let Some(o) = output {
        if verb == "Ran" {
            let secs = o["duration_ms"]
                .as_u64()
                .map(|ms| format!("{:.1} s", ms as f64 / 1000.0));
            let code = o["exit_code"].as_i64();
            let tail: Vec<&str> = o["stderr"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.trim().is_empty())
                .collect();
            let tail = tail[tail.len().saturating_sub(5)..].join("\n");
            row.detail = Some(match (code, secs) {
                (Some(0), Some(s)) => s,
                (Some(c), Some(s)) => format!("exit {c} · {s}\n{tail}").trim().to_owned(),
                _ => tail,
            });
            if o["contained"] == true {
                row.decided = Some("contained".into());
            }
        }
        if row.outcome.as_deref() != Some("ok") && row.detail.is_none() {
            row.detail = error.map(|e| crate::approvals::present::escape(&e));
        }
    }
    // A person's decision on this very object, in this run.
    let matches = |o: &Object| match o {
        Object::Command { argv, .. } => args["argv"].as_array().is_some_and(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .eq(argv.iter().map(String::as_str))
        }),
        Object::Path { asked, .. } => args["path"].as_str() == Some(asked.as_str()),
        _ => false,
    };
    if let Some((verb, _, reason)) = decided
        .iter()
        .rev()
        .find(|(_, o, _)| o.as_ref().is_some_and(matches))
    {
        if verb == "approve" {
            row.decided = Some("you".into());
        } else if verb == "reject" {
            row.decided = Some("refused".into());
            if let Some(r) = reason {
                row.detail = Some(crate::approvals::present::escape(r));
            }
        }
    }
    row
}

/// An edit as a few `-`/`+` lines (at most 20).
fn mini_diff(old: &str, new: &str) -> String {
    // An edit's text need not end its last line: a line is a line either way.
    let line = |t: &str| {
        if t.is_empty() || t.ends_with('\n') {
            t.to_owned()
        } else {
            format!("{t}\n")
        }
    };
    let lines: Vec<String> = crate::approvals::present::diff(&line(old), &line(new));
    let n = lines.len();
    let mut out: Vec<String> = lines.into_iter().take(20).collect();
    if n > 20 {
        out.push(format!("… {} more lines", n - 20));
    }
    out.join("\n")
}

/// The calls of `task` (`feature/task`) that wait for a person.
fn pending_of(
    root: &Path,
    feature: &str,
    state: &Path,
    task: &str,
) -> Vec<speccoder_core::PendingCall> {
    let Some(CommandData::Approvals(a)) = crate::approvals::list_with(root, Some(feature), state)
        .data()
        .cloned()
    else {
        return Vec::new();
    };
    let a: ApprovalsReport = *a;
    a.pending
        .into_iter()
        .filter(|p| format!("{feature}/{}", p.task) == task || p.task == task)
        .collect()
}

/// A waiting call, as a row.
fn waiting_row(p: &speccoder_core::PendingCall) -> Row {
    Row {
        verb: Some(
            p.presentation
                .as_ref()
                .map_or_else(|| "Asks".to_owned(), |v| v.title.clone()),
        ),
        object: Some(
            p.presentation
                .as_ref()
                .map_or_else(|| p.covers.clone(), |v| v.subject.clone()),
        ),
        outcome: Some("waiting".into()),
        detail: Some(p.reason.clone()),
        call: Some(p.id()),
        ..Row::of(RowKind::Approval)
    }
}

/// The evidence of the attempt's end: its tick's trailers, or its saved
/// record's outcome.
fn dividers(
    root: &Path,
    run: &str,
    ticked: Option<&speccoder_core::TaskStatus>,
    saved: Option<&crate::attempts::Saved>,
) -> Vec<Row> {
    let mut out = Vec::new();
    if let Some(t) = ticked
        && let Some(commit) = &t.commit
    {
        let message = crate::git::Git::at(root)
            .line(&["log", "-1", "--format=%B", commit])
            .unwrap_or_default();
        let trailer = |k: &str| {
            message
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{k}: ")))
                .map(str::trim)
                .map(str::to_owned)
        };
        let scenarios = trailer("Scenarios");
        let ids: Vec<String> = scenarios
            .iter()
            .flat_map(|s| s.split(", "))
            .map(|x| x.split('@').next().unwrap_or(x).to_owned())
            .collect();
        if let Some(red) = trailer("Red") {
            let n = match ids.len() {
                0 => String::new(),
                1 => " — 1 scenario".to_owned(),
                n => format!(" — {n} scenarios"),
            };
            out.push(Row::divider(
                format!("Acceptance written{n}, failing first ({red})"),
                "evidence",
            ));
        }
        if !ids.is_empty() {
            out.push(Row::divider(
                format!("{} red → green", ids.join(", ")),
                "verified",
            ));
        }
        if let Some(h) = trailer("Held-Out")
            && let Some(p) = h.strip_prefix("passed=")
        {
            let p = p.split_whitespace().next().unwrap_or(p);
            out.push(Row::divider(
                format!("Held-out tests passed {p}"),
                "verified",
            ));
        }
        let verdict = trailer("Verdict").unwrap_or_else(|| t.state.clone());
        // Only a verified (or attested) tick is drawn as proof.
        let tone = if matches!(verdict.as_str(), "verified" | "attested") {
            "verified"
        } else {
            "regression"
        };
        out.push(Row::divider(
            format!("Ticked {} — {}", t.id, verdict.replace('_', " ")),
            tone,
        ));
        return out;
    }
    if let Some(s) = saved {
        let (tone, words) = match s.outcome.as_str() {
            "stopped" => ("stopped", s.reason.clone()),
            "blocked_by_gate" => ("blocked", s.reason.clone()),
            o => ("failed", format!("{} — {}", o.replace('_', " "), s.reason)),
        };
        let words = words.trim().trim_end_matches(" —").to_owned();
        let first = words
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + &words[c.len_utf8()..]);
        out.push(Row::divider(first.unwrap_or(words), tone));
    }
    let _ = run;
    out
}

// ── Explain ────────────────────────────────────────────────────────────

/// The facts of the call `call` of `task` (`feature/task`) that waits for a
/// person: what the program is and can reach, why it asks, where its
/// arguments came from, the model's last words, the task's scenarios. No
/// model is asked.
///
/// # Errors
///
/// No such waiting call.
pub fn explain(
    path: &Path,
    state: &Path,
    task: &str,
    call: Option<&str>,
) -> Result<Explanation, String> {
    let (feature, id) = task
        .split_once('/')
        .ok_or_else(|| format!("{task}: give feature/task"))?;
    let root = crate::format::at(path)
        .detect(path)
        .ok_or_else(|| "not a spec project".to_owned())?;
    let Some(CommandData::Approvals(a)) = crate::approvals::list_with(&root, Some(feature), state)
        .data()
        .cloned()
    else {
        return Err("the waiting calls cannot be read".to_owned());
    };
    let p = a
        .pending
        .iter()
        .find(|p| call.is_none_or(|c| p.id() == c) && (p.task == id || p.task == task))
        .ok_or_else(|| match call {
            Some(c) => format!("{c} does not wait for you"),
            None => format!("{task} has no call waiting for you"),
        })?;
    let worktrees = crate::worktree::project_state_dir(state, &root).join("worktrees");
    let (argv, hosts, outside) = reach_of(&p.arguments, &[root.as_path(), worktrees.as_path()]);
    let (program, path_of) = match argv.first() {
        Some(prog) => (
            Some(prog.clone()),
            executable(&p.object).or_else(|| which(prog)),
        ),
        None => (None, None),
    };
    let view = p.presentation.as_ref();
    let reasoning = p.attempt.as_deref().and_then(|run| {
        crate::pipeline::tokio_runtime()
            .ok()?
            .block_on(crate::substrate::account(
                &crate::worktree::project_state_dir(state, &root),
                run,
            ))
            .ok()?
            .into_iter()
            .rev()
            .find_map(|t| t.reasoning)
    });
    let scenarios = scenarios_of(&root, feature, id);
    Ok(clean(Explanation {
        task: format!("{feature}/{id}"),
        call: p.id(),
        title: status_of(&root, feature)
            .and_then(|s| s.tasks.into_iter().find(|t| t.id == id).map(|t| t.title))
            .unwrap_or_default(),
        question: view.map_or_else(|| "Allow this call?".to_owned(), |v| v.title.clone()),
        program,
        path: path_of,
        hosts,
        outside,
        reach: view.map_or_else(String::new, |v| v.sandbox.clone()),
        rule: p.reason.clone(),
        provenance: p.sources.clone(),
        said: view.and_then(|v| v.claim.clone()),
        reasoning: reasoning.map(|r| core::conclusion(&r).unwrap_or(r)),
        scenarios,
    }))
}

/// The facts of the call a prompt presents, from the presentation alone:
/// what the terminal's `?` prints.
#[must_use]
pub fn explain_presented(p: &crate::approvals::present::Presentation) -> Explanation {
    let r = &p.request;
    // The prompt knows no project root: the working directory and
    // speccoder's state (its worktrees) are inside.
    let cwd = std::env::current_dir().unwrap_or_default();
    let state = crate::worktree::state_dir().unwrap_or_default();
    let (argv, hosts, outside) = reach_of(&r.arguments, &[cwd.as_path(), state.as_path()]);
    let path = match &r.object {
        Some(Object::Command { executable, .. }) if executable != "not found" => {
            Some(executable.clone())
        }
        _ => argv.first().and_then(|p| which(p)),
    };
    clean(Explanation {
        task: r.task.clone(),
        call: String::new(),
        title: r.task_title.clone(),
        question: p.title.clone(),
        program: argv.first().cloned(),
        path,
        hosts,
        outside,
        reach: p.sandbox.clone(),
        rule: r.reason.clone(),
        provenance: p.sources.clone(),
        said: r.claim.clone(),
        reasoning: None,
        scenarios: Vec::new(),
    })
}

/// A call's argv, the hosts its arguments name and the absolute paths
/// outside `inside` they name. Only argv and path-like keys are read —
/// never a file's content or an edit's text.
fn reach_of(args: &serde_json::Value, inside: &[&Path]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let argv: Vec<String> = args["argv"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut texts: Vec<&str> = argv.iter().skip(1).map(String::as_str).collect();
    for k in ["path", "cwd", "url", "uri", "dest", "destination", "target"] {
        if let Some(v) = args[k].as_str() {
            texts.push(v);
        }
    }
    let mut hosts: Vec<String> = texts.iter().filter_map(|t| host_in(t)).collect();
    hosts.sort();
    hosts.dedup();
    let mut outside: Vec<String> = texts
        .iter()
        .filter(|t| {
            t.starts_with('/')
                && !inside
                    .iter()
                    .any(|i| !i.as_os_str().is_empty() && Path::new(t).starts_with(i))
        })
        .map(|t| (*t).to_owned())
        .collect();
    outside.sort();
    outside.dedup();
    (argv, hosts, outside)
}

/// A row's texts, masked and made safe for a terminal.
fn clean_row(r: &mut Row) {
    let safe = |t: &str| crate::approvals::present::escape(&crate::secrets::mask(t));
    r.text = safe(&r.text);
    for t in [&mut r.conclusion, &mut r.object, &mut r.detail]
        .into_iter()
        .flatten()
    {
        *t = safe(t);
    }
    r.items.iter_mut().for_each(clean_row);
}

/// Every text an explanation shows, masked and made safe for a terminal.
fn clean(mut x: Explanation) -> Explanation {
    let safe = |t: &str| crate::approvals::present::escape(&crate::secrets::mask(t));
    let each = |v: &mut Vec<String>| v.iter_mut().for_each(|t| *t = safe(t));
    for t in [&mut x.title, &mut x.question, &mut x.reach, &mut x.rule] {
        *t = safe(t);
    }
    for t in [&mut x.program, &mut x.path, &mut x.said, &mut x.reasoning]
        .into_iter()
        .flatten()
    {
        *t = safe(t);
    }
    each(&mut x.hosts);
    each(&mut x.outside);
    each(&mut x.provenance);
    x
}

fn executable(object: &Option<speccoder_core::ObjectView>) -> Option<String> {
    object.as_ref()?.lines.iter().find_map(|l| {
        l.split_once("executable ")
            .map(|(_, e)| e.split_whitespace().next().unwrap_or(e).to_owned())
    })
}

fn which(program: &str) -> Option<String> {
    if program.contains('/') {
        return Some(program.to_owned());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// The host an argument names: a URL's, `user@host:path`'s, or none.
fn host_in(text: &str) -> Option<String> {
    if let Some((_, rest)) = text.split_once("://") {
        let host = rest.split(['/', '?', '#']).next()?;
        let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
        let host = host.split(':').next()?;
        return (!host.is_empty()).then(|| host.to_owned());
    }
    // git@github.com:org/repo
    let (user_host, _) = text.split_once(':')?;
    let (_, host) = user_host.split_once('@')?;
    (host.contains('.') && !host.contains('/')).then(|| host.to_owned())
}

/// The scenarios the task is judged by: `US1-2 — text`.
fn scenarios_of(root: &Path, feature: &str, task: &str) -> Vec<String> {
    let Ok(opened) = crate::format::open(root, Some(feature)) else {
        return Vec::new();
    };
    opened
        .adapter
        .scenarios(&opened.snapshot, task)
        .into_iter()
        .map(|s| format!("{} — {}", s.id, s.text))
        .take(8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_reads_argv_and_paths_never_file_content() {
        let root = std::path::Path::new("/p");
        let (_, hosts, outside) = super::reach_of(
            &serde_json::json!({ "path": "src/lib.rs", "content": "//! see https://docs.rs/x\n/* c */" }),
            &[root],
        );
        assert!(
            hosts.is_empty() && outside.is_empty(),
            "{hosts:?} {outside:?}"
        );
        let (argv, hosts, outside) = super::reach_of(
            &serde_json::json!({ "argv": ["/usr/bin/env", "curl", "https://crates.io/x", "-o", "/etc/x", "/p/out"] }),
            &[root],
        );
        assert_eq!(argv[0], "/usr/bin/env");
        assert_eq!(hosts, ["crates.io"]);
        assert_eq!(outside, ["/etc/x"]);
    }

    #[test]
    fn hosts_are_read_from_urls_and_scp_forms() {
        assert_eq!(
            host_in("https://crates.io/api/v1/crates/serde").as_deref(),
            Some("crates.io")
        );
        assert_eq!(
            host_in("git@github.com:org/repo.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(host_in("src/lib.rs"), None);
        assert_eq!(host_in("a:b"), None);
    }
}
