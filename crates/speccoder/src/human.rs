//! Human-readable reports (contracts/inspect.md).

use std::io::{self, Write};
use std::path::Path;

use speccoder_core::verify::{CheckStatus, VerifyReport};
use speccoder_core::{
    ApprovalsReport, ConvergeReport, CostReport, EvalReport, ProposeReport, RunReport,
    StatusReport, WorktreeReport,
};

use speccoder_core::snapshot::{
    Artifacts, ConstitutionState, DependencyMode, Diagnostic, ProjectSnapshot, ScheduledTask,
    SelectionReason, Severity, Task, TaskState,
};

/// The full `inspect` report.
pub fn inspect(w: &mut impl Write, s: &ProjectSnapshot) -> io::Result<()> {
    let name = Path::new(&s.root)
        .file_name()
        .map_or_else(|| s.root.clone(), |n| n.to_string_lossy().into_owned());
    let mut kit = vec![s.speckit.integration.clone(), s.speckit.script.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    if !kit.is_empty() {
        kit = format!(" ({kit})");
    }
    let version = s.speckit.version.as_deref().unwrap_or("version unknown");
    match &s.openspec {
        Some(o) => writeln!(w, "Project    {name:<24} OpenSpec (schema {})", o.schema)?,
        None => writeln!(w, "Project    {name:<24} Spec Kit {version}{kit}")?,
    }
    writeln!(w, "Root       {}", s.root)?;
    let unit = if s.openspec.is_some() {
        "Change "
    } else {
        "Feature"
    };

    match &s.feature {
        Some(f) => {
            let why = match (s.selection.reason, s.openspec.is_some()) {
                (SelectionReason::Argument, false) => "from --feature",
                (SelectionReason::Argument, true) => "from --change",
                (SelectionReason::Environment, _) => "from SPECIFY_FEATURE_DIRECTORY",
                (SelectionReason::Recorded, _) => "recorded in .specify/feature.json",
                (SelectionReason::OnlyFeature, false) => "the only feature",
                (SelectionReason::OnlyFeature, true) => "the only active change",
                (SelectionReason::None, _) => "",
            };
            writeln!(w, "{unit}    {:<24} ({why})", f.feature.name)?;
            match &s.openspec {
                Some(o) => {
                    let mark = |b: bool| if b { "✓" } else { "–" };
                    writeln!(
                        w,
                        "Artifacts  context {}  proposal {}  design {}  deltas {}  tasks {}",
                        mark(o.context.is_some()),
                        mark(f.artifacts.research),
                        mark(f.artifacts.plan),
                        mark(f.artifacts.spec),
                        mark(f.artifacts.tasks),
                    )?;
                    if !o.requirements.is_empty() {
                        writeln!(w, "Requirements")?;
                        for r in &o.requirements {
                            let n = r.scenarios.len();
                            let scenarios = match (r.operation.as_str(), n) {
                                ("renamed" | "removed", _) => String::new(),
                                (_, 1) => " (1 scenario)".to_owned(),
                                (_, n) => format!(" ({n} scenarios)"),
                            };
                            writeln!(
                                w,
                                "  {:<9}{}: {}{scenarios}",
                                r.operation, r.capability, r.name
                            )?;
                        }
                    }
                }
                None => writeln!(w, "Artifacts  {}", artifacts(s.constitution, &f.artifacts))?,
            }
            match (&f.tasks, &f.schedule) {
                (Some(t), Some(sched)) => {
                    writeln!(
                        w,
                        "Tasks      {} total · {} done · {} ready · {} blocked",
                        t.totals.total, sched.totals.done, sched.totals.ready, sched.totals.blocked
                    )?;
                    if t.dependency_mode == DependencyMode::Declared {
                        writeln!(
                            w,
                            "           dependencies as declared per task (Format line)"
                        )?;
                    }
                    if t.dependency_mode == DependencyMode::Chain {
                        writeln!(w, "           a chain: groups in order, tasks in order")?;
                    }
                    for (task, st) in t.tasks.iter().zip(&sched.tasks) {
                        writeln!(w, "  {}", task_line(task, st))?;
                    }
                }
                (Some(t), None) => {
                    writeln!(
                        w,
                        "Tasks      {} total · {} done · {} open",
                        t.totals.total, t.totals.done, t.totals.open
                    )?;
                }
                (None, _) => writeln!(w, "Tasks      none (no tasks.md)")?,
            }
        }
        None => writeln!(w, "{unit}    none")?,
    }

    if !s.diagnostics.is_empty() {
        writeln!(w, "Diagnostics")?;
        for d in &s.diagnostics {
            writeln!(w, "  {}", diagnostic(d))?;
        }
    }
    writeln!(w, "No files modified.")
}

/// Why no feature could be selected, with the available features.
pub fn selection_needed(w: &mut impl Write, s: &ProjectSnapshot) -> io::Result<()> {
    for d in s
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("select."))
    {
        writeln!(w, "speccoder inspect: {}", d.message)?;
    }
    let (units, how) = if s.openspec.is_some() {
        ("Changes:", "Select one with --change <id>.")
    } else {
        ("Features:", "Select one with --feature <name or number>.")
    };
    writeln!(w, "{units}")?;
    for f in &s.features {
        writeln!(w, "  {}", f.name)?;
    }
    writeln!(w, "{how}")
}

fn artifacts(constitution: ConstitutionState, a: &Artifacts) -> String {
    let mark = |b: bool| if b { "✓" } else { "–" };
    let c = match constitution {
        ConstitutionState::Present => "✓",
        ConstitutionState::Unfilled => "template",
        ConstitutionState::Absent => "–",
    };
    format!(
        "constitution {c}  spec {}  plan {}  tasks {}  research {}  data-model {}  contracts {}  quickstart {}  checklists {}",
        mark(a.spec),
        mark(a.plan),
        mark(a.tasks),
        mark(a.research),
        mark(a.data_model),
        mark(a.contracts),
        mark(a.quickstart),
        mark(a.checklists),
    )
}

fn task_line(t: &Task, st: &ScheduledTask) -> String {
    let check = if t.done { "[x]" } else { "[ ]" };
    let p = if t.parallel { "[P]" } else { "" };
    let story = t.story.as_deref().unwrap_or("");
    let look = crate::style::out();
    let state = match st.state {
        TaskState::Done => String::new(),
        TaskState::Ready => "ready".to_owned(),
        TaskState::Blocked => blocked_by(&st.blocked_by),
    };
    if look.screen_reader {
        let state = if state.is_empty() {
            "done"
        } else {
            state.as_str()
        };
        return format!("task: {} {} — {state}", t.id, t.title);
    }
    // The title takes what the line leaves beside the fixed columns and a
    // state column of the same width on every row (so states line up), at
    // most TITLE_WIDTH.
    let width = look
        .width
        .saturating_sub(4 + 6 + 4 + 5 + 2 + 32)
        .clamp(16, TITLE_WIDTH);
    let line = format!(
        "{check} {:<5} {p:<3} {story:<4} {}",
        t.id,
        pad(&shorten(&t.title, width), width),
    );
    let token = match st.state {
        TaskState::Done => crate::style::Token::Verified,
        TaskState::Ready => crate::style::Token::Ready,
        TaskState::Blocked => crate::style::Token::Blocked,
    };
    format!("{line}  {}", look.paint(token, &state))
        .trim_end()
        .to_owned()
}

/// `blocked by T003, T004, T005 (+2)`
fn blocked_by(ids: &[String]) -> String {
    const SHOWN: usize = 3;
    let mut s = format!(
        "blocked by {}",
        ids.iter()
            .take(SHOWN)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
    if ids.len() > SHOWN {
        s.push_str(&format!(" (+{})", ids.len() - SHOWN));
    }
    s
}

/// Task titles are cut to one terminal line; `--json` carries them in full.
const TITLE_WIDTH: usize = 72;

/// `s` cut to `max` display columns (never mid-character), with `…`.
fn shorten(s: &str, max: usize) -> String {
    crate::style::out().fit(s, max)
}

/// `s` padded with spaces to `cols` display columns.
fn pad(s: &str, cols: usize) -> String {
    let w = crate::style::width(s);
    format!("{s}{}", " ".repeat(cols.saturating_sub(w)))
}

fn diagnostic(d: &Diagnostic) -> String {
    let sev = match d.severity {
        Severity::Error => "error  ",
        Severity::Warning => "warning",
        Severity::Info => "info   ",
    };
    let loc = match (&d.path, d.line) {
        (Some(p), Some(l)) => format!("{p}:{l}  "),
        (Some(p), None) => format!("{p}  "),
        _ => String::new(),
    };
    let hint = d
        .hint
        .as_deref()
        .map(|h| format!(" — {h}"))
        .unwrap_or_default();
    format!("{sev}  {}  {loc}{}{hint}", d.code, d.message)
}

/// Failing tests listed per check before eliding the rest.
const FAILING_SHOWN: usize = 10;

/// The `verify` report.
pub fn verify(w: &mut impl Write, r: &VerifyReport) -> io::Result<()> {
    writeln!(w, "Root       {}", r.root)?;
    writeln!(w, "Checks     tier {}", r.tier)?;
    for c in &r.checks {
        let mark = match c.status {
            CheckStatus::Passed => "✓",
            CheckStatus::Skipped => "–",
            _ => "✗",
        };
        let status = format!("{:?}", c.status).to_lowercase();
        let tests = c
            .test_counts()
            .map(|(p, f, s)| format!("  {p} passed · {f} failed · {s} skipped"))
            .unwrap_or_default();
        let secs = c.duration_ms as f64 / 1000.0;
        writeln!(
            w,
            "  {mark} {:<12} {status:<10} {secs:>7.1}s{tests}",
            c.name
        )?;
        if let Some(note) = &c.note {
            writeln!(w, "      {note}")?;
        }
        let failing: Vec<_> = c
            .tests
            .iter()
            .flatten()
            .filter(|t| t.outcome.is_failing())
            .collect();
        for t in failing.iter().take(FAILING_SHOWN) {
            writeln!(w, "      failed: {}", t.id)?;
        }
        if failing.len() > FAILING_SHOWN {
            writeln!(w, "      … and {} more", failing.len() - FAILING_SHOWN)?;
        }
        if c.status.is_failure() && c.tests.is_none() {
            let tail = if c.stderr_tail.trim().is_empty() {
                &c.stdout_tail
            } else {
                &c.stderr_tail
            };
            for line in tail
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                writeln!(w, "      | {line}")?;
            }
        }
    }
    if let Some(cmp) = &r.comparison {
        writeln!(
            w,
            "Baseline   {} → {} tests · {} regressions · {} failing before and now · {} new",
            cmp.count_before,
            cmp.count_after,
            cmp.regressions.len(),
            cmp.pre_existing.len(),
            cmp.new_tests.len()
        )?;
        for reg in &cmp.regressions {
            writeln!(w, "      regression: {} is {}", reg.id, reg.now)?;
        }
    }
    writeln!(
        w,
        "{}",
        if r.passed {
            "Verification passed."
        } else {
            "Verification failed."
        }
    )
}

/// Where speccoder works.
pub fn worktree(w: &mut impl Write, r: &WorktreeReport) -> io::Result<()> {
    let opt = |s: &Option<String>| s.clone().unwrap_or_default();
    writeln!(w, "Feature    {}", r.feature)?;
    writeln!(
        w,
        "Worktree   {}  ({})",
        opt(&r.path),
        if r.created { "created" } else { "reused" }
    )?;
    writeln!(w, "Branch     {}", opt(&r.branch))?;
    writeln!(w, "Base       {}", opt(&r.base))?;
    writeln!(w, "Your checkout was not modified.")
}

/// Why speccoder will not use a worktree here.
pub fn worktree_refused(w: &mut impl Write, r: &WorktreeReport) -> io::Result<()> {
    for p in &r.refused_paths {
        writeln!(w, "  {:<10} {}", p.reason, p.path)?;
    }
    Ok(())
}

/// A proposal: printed, recorded, never executed.
pub fn propose(w: &mut impl Write, p: &ProposeReport) -> io::Result<()> {
    let empty = serde_json::Value::Null;
    let prop = p.proposal.as_ref().unwrap_or(&empty);
    writeln!(w, "Task       {} ({})", p.task, p.feature)?;
    writeln!(
        w,
        "Summary    {}",
        prop["summary"].as_str().unwrap_or_default()
    )?;
    for (key, label) in [
        ("steps", "Steps"),
        ("files", "Files"),
        ("checks", "Checks"),
        ("risks", "Risks"),
    ] {
        let items = prop[key].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        writeln!(w, "{label}")?;
        for (i, item) in items.iter().enumerate() {
            let text = item.as_str().unwrap_or_default();
            if key == "steps" {
                writeln!(w, "  {}. {text}", i + 1)?;
            } else {
                writeln!(w, "  - {text}")?;
            }
        }
    }
    if let Some(u) = &p.usage {
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        writeln!(
            w,
            "Usage      {} in · {} out · {} cache read · {} cache write",
            n("input_tokens"),
            n("output_tokens"),
            n("cache_read_tokens"),
            n("cache_write_tokens")
        )?;
    }
    writeln!(
        w,
        "Run        {}  (speccoder replay {} reproduces it without the model)",
        p.run_id, p.run_id
    )?;
    writeln!(w, "Nothing was executed.")
}

/// One attempt and its verdict. The model's report is shown as its claim.
pub fn run(w: &mut impl Write, r: &RunReport, message: &str) -> io::Result<()> {
    writeln!(w, "Task       {} ({})", r.task, r.feature)?;
    writeln!(w, "Worktree   {}", r.worktree)?;
    writeln!(w, "Attempt    {}", r.attempt_status)?;
    for c in &r.tool_calls {
        let tool = c["tool"].as_str().unwrap_or_default();
        let decision = c["decision"].as_str().unwrap_or_default();
        let what = c["arguments"]["path"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                c["arguments"]["argv"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            })
            .unwrap_or_default();
        let mut failed = if c["ok"].as_bool() == Some(false) {
            " (failed)".to_owned()
        } else {
            String::new()
        };
        // Who decided, when a person did.
        if let Some(name) = c["approver"]["name"].as_str() {
            let verb = if c["rejected_by"] == "person" {
                "rejected"
            } else {
                "approved"
            };
            failed.push_str(&format!(
                " — {verb} by {name} ({})",
                c["approver"]["via"].as_str().unwrap_or("?")
            ));
        }
        writeln!(
            w,
            "  {decision:<8} {tool:<12} {}{failed}",
            shorten(&what, 60)
        )?;
    }
    if let Some(reason) = &r.reason {
        writeln!(w, "Reason     {reason}")?;
    }
    if !r.instructions.is_empty() {
        let files: Vec<String> = r
            .instructions
            .iter()
            .map(|i| {
                if i.cut {
                    format!("{} (cut)", i.path)
                } else {
                    i.path.clone()
                }
            })
            .collect();
        writeln!(w, "Guided by  {}", files.join(", "))?;
    }
    if let Some(a) = &r.attestation {
        let detail = match (
            a["state"].as_str(),
            a["signer"].as_str(),
            a["reason"].as_str(),
        ) {
            (Some("signed"), Some(signer), _) => format!("signed ({signer})"),
            (Some(state), _, Some(why)) => format!("{state} — {why}"),
            (Some(state), ..) => state.to_owned(),
            _ => String::new(),
        };
        writeln!(w, "Attested   {detail}")?;
    }
    if let Some(p) = &r.pending {
        writeln!(
            w,
            "Pending    {} {} — {}",
            p["tool"].as_str().unwrap_or_default(),
            p["arguments"],
            p["reason"].as_str().unwrap_or_default()
        )?;
    }
    if let Some(m) = &r.model_report {
        writeln!(
            w,
            "Model says {} ({})",
            m["summary"].as_str().unwrap_or_default(),
            m["status"].as_str().unwrap_or_default()
        )?;
    }
    if r.changes.is_empty() {
        writeln!(w, "Changes    none")?;
    } else {
        writeln!(w, "Changes")?;
        for (path, kind) in &r.changes {
            writeln!(w, "  {kind:<9} {path}")?;
        }
    }
    if let Some(a) = &r.acceptance {
        if let Some(why) = a["skipped"].as_str() {
            writeln!(w, "Acceptance skipped: {why}")?;
        } else {
            let mark = |v: &serde_json::Value| match v.as_bool() {
                Some(true) => "✓",
                Some(false) => "✗",
                None => "–",
            };
            let files: Vec<&str> = a["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str())
                .collect();
            let class = a["red_class"].as_str().unwrap_or_default();
            let kind = match class {
                "absent" => " (absent: what the task adds did not exist)",
                _ => "",
            };
            writeln!(
                w,
                "Acceptance {}  red {}{kind} green {}  (spec-derived, frozen)",
                files.join(", "),
                mark(&a["red"]),
                mark(&a["green"])
            )?;
            if a["red"] == false && !class.is_empty() && class != "passed" {
                let first = a["excerpt"]
                    .as_str()
                    .and_then(|e| e.lines().next())
                    .unwrap_or_default();
                writeln!(
                    w,
                    "  acceptance: not demonstrated ({}){}",
                    class.replace('_', " "),
                    if first.is_empty() {
                        String::new()
                    } else {
                        format!(" — {first}")
                    }
                )?;
            }
        }
        if let Some(line) = held_out_line(&a["held_out"]) {
            writeln!(w, "Held-out   {line}")?;
        }
    }
    if let Some(st) = &r.strength {
        for line in strength_lines(st) {
            writeln!(w, "{line}")?;
        }
    }
    for f in &r.integrity {
        writeln!(w, "  ! {f}")?;
    }
    if let Some(v) = &r.verification {
        for c in &v.checks {
            let mark = if c.status.is_failure() {
                "✗"
            } else if c.status == CheckStatus::Skipped {
                "–"
            } else {
                "✓"
            };
            writeln!(w, "  {mark} {}", c.name)?;
        }
    }
    for (i, e) in r.earlier.iter().enumerate() {
        writeln!(
            w,
            "Attempt {} failed, undone and retried: {}",
            i + 1,
            shorten(&e.fingerprint, 70)
        )?;
    }
    if let Some(stop) = &r.retry_stop {
        writeln!(w, "No retry   {stop}")?;
    }
    if let Some(u) = &r.usage {
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        writeln!(
            w,
            "Usage      {} in · {} out · {} cache read",
            n("input_tokens"),
            n("output_tokens"),
            n("cache_read_tokens")
        )?;
    }
    if r.ticked {
        writeln!(w, "Ticked     {} in tasks.md (worktree)", r.task)?;
    }
    if let Some(c) = &r.commit {
        writeln!(w, "Commit     {c}")?;
    }
    if let Some(saved) = &r.saved_attempt {
        writeln!(w, "Saved      {saved} (undone in the worktree)")?;
    }
    if let Some(aside) = &r.set_aside {
        writeln!(w, "Set aside  earlier leftovers saved as {aside}")?;
    }
    for n in &r.notices {
        writeln!(w, "Note       {n}")?;
    }
    if let Some(m) = &r.monitor {
        if let Some(e) = m["error"].as_str() {
            writeln!(w, "Monitor    could not run: {e}")?;
        } else if m["suspected"] == true {
            writeln!(w, "Monitor    suspects gaming:")?;
            for f in m["findings"].as_array().into_iter().flatten() {
                writeln!(
                    w,
                    "  ? {} — {}",
                    f["kind"].as_str().unwrap_or("other"),
                    shorten(f["evidence"].as_str().unwrap_or_default(), 90)
                )?;
            }
        } else {
            writeln!(w, "Monitor    no gaming found")?;
        }
    }
    if let Some(m) = &r.mutation {
        if let Some(e) = m["error"].as_str() {
            writeln!(w, "Mutation   could not run: {}", shorten(e, 120))?;
        } else {
            writeln!(
                w,
                "Mutation   {} caught, {} survived{} ({})",
                m["caught"],
                m["missed"],
                match m["timeout"].as_u64() {
                    Some(t) if t > 0 => format!(", {t} timed out"),
                    _ => String::new(),
                },
                m["tool"].as_str().unwrap_or("mutation tool")
            )?;
            for s in m["survivors"].as_array().into_iter().flatten().take(5) {
                writeln!(w, "  ~ {}", shorten(s.as_str().unwrap_or_default(), 100))?;
            }
        }
    }
    if !r.samples.is_empty() {
        writeln!(w, "Samples    best of {}:", r.samples.len())?;
        for (i, s) in r.samples.iter().enumerate() {
            writeln!(
                w,
                "  {} {} {} · {} changed lines · {} tokens{}",
                if s["kept"] == true { "▸" } else { " " },
                i + 1,
                s["verdict"]["verdict"]
                    .as_str()
                    .unwrap_or(s["attempt_status"].as_str().unwrap_or("?")),
                s["changed_lines"],
                s["tokens"],
                s["survivors"]
                    .as_u64()
                    .map(|n| format!(" · {n} survivors"))
                    .unwrap_or_default()
            )?;
        }
    }
    writeln!(w, "Run        {}", r.run_id)?;
    if !message.is_empty() {
        writeln!(w, "{message}")?;
    }
    Ok(())
}

/// Tokens and money per task.
pub fn cost(w: &mut impl Write, c: &CostReport, message: &str) -> io::Result<()> {
    writeln!(w, "Feature    {}", c.feature)?;
    let line = |w: &mut dyn Write, name: &str, attempts: &str, u: &speccoder_core::Usage| {
        let usd = u.usd.map(|d| format!("  ${d:.4}")).unwrap_or_default();
        writeln!(
            w,
            "  {name:<12} {attempts:>9}  {:>9} in  {:>7} out  {:>9} cached{usd}",
            u.input_tokens, u.output_tokens, u.cache_read_tokens
        )
    };
    for t in &c.tasks {
        line(w, &t.task, &format!("{} att.", t.attempts), &t.usage)?;
    }
    if c.convergence.input_tokens > 0 {
        line(w, "convergence", "", &c.convergence)?;
    }
    line(w, "total", "", &c.total)?;
    if let Some(m) = &c.priced_model {
        writeln!(w, "Prices     {m}, from speccoder.toml")?;
    }
    writeln!(w, "{message}")
}

/// Calls awaiting a person, and decisions made.
pub fn approvals(w: &mut impl Write, a: &ApprovalsReport, message: &str) -> io::Result<()> {
    writeln!(w, "Feature    {}", a.feature)?;
    for p in &a.pending {
        match &p.presentation {
            Some(v) => {
                let head = format!("  △ {:<5} {}  {}", p.task, v.title, v.subject);
                let pad = 70usize.saturating_sub(head.chars().count()).max(3);
                writeln!(
                    w,
                    "{head}{}risk {}",
                    " ".repeat(pad),
                    crate::prompt::risk_bar(&v.risk)
                )?;
                for line in &v.body_lines {
                    writeln!(w, "      {line}")?;
                }
                writeln!(w, "      Why stopped  {}", p.reason)?;
                writeln!(w, "      Sandbox      {}", v.sandbox)?;
                if let Some(c) = &v.claim {
                    writeln!(w, "      Model says   \"{c}\"   (a claim)")?;
                }
                writeln!(w, "      approve → {}", v.scope)?;
            }
            None => writeln!(w, "  ? {} {} — {}", p.task, p.covers, p.reason)?,
        }
        writeln!(
            w,
            "      speccoder task approve {0} [--note \"…\"]   |   speccoder task reject {0} --reason \"…\"",
            p.task
        )?;
    }
    let by = |b: &Option<String>| b.as_ref().map(|b| format!(" — {b}")).unwrap_or_default();
    for d in &a.decided {
        let scope = match d.scope.as_deref() {
            Some("run") => " for this run",
            Some("once") => " once",
            _ => " for the task",
        };
        let note = d
            .note
            .as_ref()
            .map(|n| format!(" (note: {n})"))
            .unwrap_or_default();
        match &d.reason {
            Some(r) => writeln!(w, "  ✗ {} {} rejected: {r}{}", d.task, d.covers, by(&d.by))?,
            None => writeln!(
                w,
                "  ✓ {} {} approved{scope}{note}{}",
                d.task,
                d.covers,
                by(&d.by)
            )?,
        }
    }
    writeln!(w, "{message}")
}

/// A feature's tasks and their evidence.
pub fn status(w: &mut impl Write, r: &StatusReport, message: &str) -> io::Result<()> {
    writeln!(
        w,
        "Feature    {} (from {}){}",
        r.feature,
        r.branch.as_deref().unwrap_or("the checkout"),
        if r.selected_by.is_empty() {
            String::new()
        } else {
            format!(" · {}", r.selected_by)
        }
    )?;
    if !r.journey.is_empty() {
        writeln!(w, "Journey    {}", journey(&r.journey))?;
    }
    if !r.planning.is_empty() {
        // Being planned: the journey says where; the Next: block what follows.
        return writeln!(w, "{message}");
    }
    let look = crate::style::out();
    let id_width = r
        .tasks
        .iter()
        .map(|t| t.id.chars().count())
        .max()
        .unwrap_or(4);
    let mark_width = if look.ascii { 4 } else { 1 };
    // Mark, id and the spaces between the columns.
    let fixed = 2 + mark_width + 1 + id_width + 1 + 1;
    // The title takes what the terminal leaves beside the state (up to 30
    // columns of it), never less than 16; the state is cut last.
    let longest = r
        .tasks
        .iter()
        .map(|t| crate::style::width(&t.title))
        .max()
        .unwrap_or(16);
    let title_width = look
        .width
        .saturating_sub(fixed + 30)
        .max(16)
        .min(longest.max(16));
    let mut phase = "";
    for t in &r.tasks {
        if t.phase != phase {
            phase = &t.phase;
            if look.screen_reader {
                writeln!(w, "phase: {phase}")?;
            } else {
                writeln!(w, "{}", look.paint(crate::style::Token::Accent, phase))?;
            }
        }
        let note = match t.state.as_str() {
            "verified" => "verified".to_owned(),
            "regression_only" => "regression only".to_owned(),
            "ticked_without_evidence" => "ticked, no speccoder evidence".to_owned(),
            "evidence_without_tick" => {
                "committed, local tick missing — `run --in-place` ticks it".to_owned()
            }
            "attested" => "attested by a person (not verified)".to_owned(),
            "skipped" => "skipped".to_owned(),
            "ready" => "ready".to_owned(),
            _ => format!("blocked by {}", t.blocked_by.join(", ")),
        };
        let glyph = crate::style::Glyph::of_state(&t.state);
        if look.screen_reader {
            writeln!(w, "task: {} {} — {note}", t.id, t.title)?;
        } else {
            let note = shorten(&note, look.width.saturating_sub(fixed + title_width).max(8));
            writeln!(
                w,
                "  {} {} {} {}",
                look.cell(glyph),
                pad(&t.id, id_width),
                pad(&shorten(&t.title, title_width), title_width),
                look.paint(glyph.token(), &note)
            )?;
        }
        if let Some(n) = &t.note {
            writeln!(w, "      reason: {}", shorten(n, 90))?;
        }
        if let Some(d) = &t.drift {
            if d.stale() {
                for r in &d.requirements {
                    let how = match (&r.then, &r.now) {
                        (Some(_), Some(_)) => "reworded",
                        (Some(_), None) => "removed",
                        _ => "added",
                    };
                    writeln!(
                        w,
                        "      {}: {} {how} since the tick — its evidence is for the earlier spec",
                        look.paint(crate::style::Token::Attention, "spec changed"),
                        r.id
                    )?;
                }
            } else if d.files.is_empty() {
                writeln!(
                    w,
                    "      the spec, plan or constitution changed since the tick"
                )?;
            } else {
                writeln!(
                    w,
                    "      the spec changed since the tick ({}), not in what it cites",
                    d.files.join(", ")
                )?;
            }
        }
        if let Some(a) = &t.attestation {
            writeln!(w, "      attestation: {a}")?;
        }
        if let Some(st) = &t.strength {
            writeln!(w, "      acceptance strength: {st}")?;
        }
        if let Some(a) = &t.last_attempt {
            writeln!(
                w,
                "      last attempt: {} — {}",
                a.outcome,
                shorten(&a.reason, 90)
            )?;
        }
    }
    if !r.trace.is_empty() {
        // The rows are `--json`'s; here, how many are cited and which not.
        let mut requirements: Vec<&str> = r.trace.iter().map(|t| t.requirement.as_str()).collect();
        requirements.dedup();
        let uncited: Vec<&str> = r
            .trace
            .iter()
            .filter(|t| t.task.is_none())
            .map(|t| t.requirement.as_str())
            .collect();
        let mut line = format!(
            "Trace      {}/{} requirement(s) cited by a task",
            requirements.len() - uncited.len(),
            requirements.len()
        );
        if !uncited.is_empty() {
            line.push_str(&format!("; not cited: {}", uncited.join(", ")));
        }
        let mut changed: Vec<&str> = r
            .trace
            .iter()
            .filter(|t| t.changed)
            .map(|t| t.requirement.as_str())
            .collect();
        changed.dedup();
        if !changed.is_empty() {
            line.push_str(&format!(
                "; changed since their tick: {}",
                changed.join(", ")
            ));
        }
        writeln!(w, "{line}")?;
    }
    if let Some(t) = &r.review {
        let n = t.approved + t.open + t.changes_requested + t.lapsed;
        let mut line = format!("Review     {}/{n} requirement(s) approved", t.approved);
        for (k, word) in [
            (t.open, "to review"),
            (t.changes_requested, "with changes requested"),
            (t.lapsed, "changed since decided"),
        ] {
            if k > 0 {
                line.push_str(&format!("; {k} {word}"));
            }
        }
        writeln!(w, "{line}")?;
    }
    for c in &r.convergence {
        writeln!(
            w,
            "Converge   round {} ({}): {} task(s)",
            c.round,
            c.commit.get(..10).unwrap_or(&c.commit),
            c.tasks.len()
        )?;
    }
    writeln!(w, "{message}")
}

/// A convergence round.
/// `attest verify`: one line per task object, then the totals.
pub fn attest(
    w: &mut impl Write,
    a: &speccoder_core::AttestReport,
    message: &str,
) -> io::Result<()> {
    writeln!(w, "Feature    {}", a.feature)?;
    for o in &a.objects {
        let signer = o
            .signer
            .as_ref()
            .map(|s| {
                format!(
                    "{} {}{}",
                    s.kind,
                    s.keyid,
                    s.principal
                        .as_ref()
                        .map(|p| format!(" ({p})"))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default();
        writeln!(
            w,
            "  {:<6} {}  {:<16} {signer}",
            o.task,
            &o.object[..o.object.len().min(10)],
            o.outcome.word()
        )?;
        for r in &o.reasons {
            let detail = match (&r.field, &r.expected, &r.found) {
                (Some(f), Some(e), Some(x)) => format!("{f}: expected {e}, found {x}"),
                (_, _, Some(x)) => x.clone(),
                _ => String::new(),
            };
            writeln!(w, "      {} {detail}", r.code)?;
        }
        if let Some(same) = &o.same_tree_as {
            writeln!(w, "      same content as attested commit {same}")?;
        }
    }
    writeln!(w, "{message}")
}

pub fn converge(w: &mut impl Write, c: &ConvergeReport, message: &str) -> io::Result<()> {
    writeln!(
        w,
        "Converge   {} round {} → {}",
        c.feature, c.round, c.outcome
    )?;
    let field = |f: &serde_json::Value, k: &str| f[k].as_str().unwrap_or_default().to_owned();
    for f in &c.findings {
        writeln!(
            w,
            "  {:<8} {:<11} {:<12} {}",
            field(f, "severity"),
            field(f, "gap_type"),
            field(f, "source_ref"),
            shorten(&field(f, "description"), 70)
        )?;
    }
    for t in &c.appended {
        writeln!(w, "  + {t}")?;
    }
    for f in &c.unrequested {
        writeln!(
            w,
            "  ? for a person: {}",
            shorten(&field(f, "description"), 70)
        )?;
    }
    for f in &c.repeated {
        writeln!(w, "  = repeated: {}", shorten(&field(f, "task"), 70))?;
    }
    if let Some(commit) = &c.commit {
        writeln!(w, "Commit     {commit}")?;
    }
    for h in &c.hooks {
        writeln!(w, "Hook       {h}")?;
    }
    if let Some(run) = &c.run_id {
        writeln!(w, "Run        {run}")?;
    }
    if !message.is_empty() {
        writeln!(w, "{message}")?;
    }
    Ok(())
}

/// An evaluation suite's result.
pub fn eval(w: &mut impl Write, e: &EvalReport, message: &str) -> io::Result<()> {
    writeln!(w, "Model      {}", e.model)?;
    for c in &e.cases {
        let tokens = c.usage.as_ref().map_or(0, |u| {
            u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0)
        });
        let oracle = match c.oracle_passed {
            Some(true) => "oracle ✓",
            Some(false) => "oracle ✗",
            None => "",
        };
        writeln!(
            w,
            "  {:<18} {:<17} expect {:<8} attempt {:<15} ×{} {oracle:<9} {tokens} tokens{}",
            c.name,
            c.classification,
            c.expect,
            c.attempt_status.as_deref().unwrap_or("-"),
            c.attempts,
            if c.round > 1 || e.cases.iter().any(|x| x.round > 1) {
                format!("  (round {})", c.round)
            } else {
                String::new()
            },
        )?;
    }
    // Repeated rounds: how often each case came out as expected.
    if e.cases.iter().any(|c| c.round > 1) {
        let mut names: Vec<&str> = e.cases.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        writeln!(w, "Per case")?;
        for name in names {
            let runs: Vec<&speccoder_core::EvalCase> =
                e.cases.iter().filter(|c| c.name == name).collect();
            let good = runs
                .iter()
                .filter(|c| {
                    matches!(
                        c.classification.as_str(),
                        "correct" | "honest_stop" | "caught"
                    )
                })
                .count();
            let retried = runs.iter().filter(|c| c.attempts > 1).count();
            writeln!(
                w,
                "  {name:<18} {good}/{} as expected · {retried} retried",
                runs.len()
            )?;
        }
    }
    if e.summary.replay_checked > 0 {
        writeln!(
            w,
            "Replay     {} of {} recorded attempts replay strictly from the journal",
            e.summary.replayed, e.summary.replay_checked
        )?;
    }
    if let Some(m) = &e.summary.monitor {
        let pct = |x: Option<f64>| x.map_or("n/a".to_owned(), |v| format!("{v:.2}"));
        writeln!(
            w,
            "Monitor    precision {} · recall {} ({} judged: {} caught, {} false alarms, {} missed)",
            pct(m.precision),
            pct(m.recall),
            m.judged,
            m.true_positives,
            m.false_positives,
            m.false_negatives
        )?;
    }
    if let Some(g) = &e.summary.held_out_gap {
        writeln!(
            w,
            "Held-out   gap {:.2} ({}/{} visible passes failed the held-out tests) · {} tailoring case(s) caught",
            g.rate, g.held_out_failed, g.visible_passed, e.summary.caught
        )?;
    }
    if let Some(o) = &e.summary.held_out_oracle {
        writeln!(
            w,
            "           oracle agreed with {} held-out pass(es), disagreed with {}",
            o.agree, o.disagree
        )?;
    }
    match e.summary.median_acceptance_strength {
        Some(m) => writeln!(
            w,
            "Strength   median acceptance strength {m:.2} over verified cases · {} over-strict stop(s)",
            e.summary.over_strict_stops
        )?,
        None if e.summary.over_strict_stops > 0 => writeln!(
            w,
            "Strength   not measured · {} over-strict stop(s)",
            e.summary.over_strict_stops
        )?,
        None => {}
    }
    if let Some(t) = e.summary.tokens_per_correct {
        writeln!(w, "Tokens     {t} per correct case")?;
    }
    writeln!(w, "{message}")
}

/// `2/2 passed (gap on attempt 1: US1-2)`, `absent: not_red`, `off`.
fn held_out_line(h: &serde_json::Value) -> Option<String> {
    match h["state"].as_str()? {
        "off" => Some("off ([acceptance] held_out = false)".to_owned()),
        "absent" => Some(format!(
            "absent: {}",
            h["reason"]
                .as_str()
                .unwrap_or("none generated")
                .replace('_', " ")
        )),
        _ => {
            let kept = h["kept"].as_u64().unwrap_or(0);
            let attempts = h["attempts"].as_array().cloned().unwrap_or_default();
            let last = attempts
                .last()
                .and_then(|a| a["outcome"].as_str())
                .unwrap_or("not run");
            let gaps: Vec<String> = attempts
                .iter()
                .enumerate()
                .filter(|(_, a)| a["outcome"] == "failed")
                .map(|(i, a)| {
                    let ids: Vec<&str> = a["scenarios"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|s| s.as_str())
                        .collect();
                    format!("gap on attempt {}: {}", i + 1, ids.join(", "))
                })
                .collect();
            let state = match last {
                "passed" => format!("{kept}/{kept} passed"),
                other => format!("{} ({kept} file(s))", other.replace('_', " ")),
            };
            Some(if gaps.is_empty() {
                state
            } else {
                format!("{state} ({})", gaps.join("; "))
            })
        }
    }
}

/// The `Next:` block: each command padded to the longest (at most 44), then
/// why; nothing when there is no step.
pub fn next(w: &mut dyn Write, steps: &[speccoder_core::next::NextStep]) -> std::io::Result<()> {
    if steps.is_empty() {
        return Ok(());
    }
    let look = crate::style::out();
    if look.screen_reader {
        for s in steps {
            writeln!(w, "next: {} — {}", s.command, s.why)?;
        }
        return Ok(());
    }
    let width = steps
        .iter()
        .map(|s| s.command.chars().count())
        .max()
        .unwrap_or(0)
        .min(44);
    writeln!(w)?;
    for (i, s) in steps.iter().enumerate() {
        let label = if i == 0 { "Next:  " } else { "       " };
        writeln!(
            w,
            "{}{}  {}",
            look.paint(crate::style::Token::Accent, label),
            look.paint(crate::style::Token::Accent, &pad(&s.command, width)),
            s.why
        )?;
    }
    Ok(())
}

/// The acceptance set's strength: the share with its parts, what
/// got through, and any alternative the tests rejected.
fn strength_lines(a: &speccoder_core::strength::AcceptanceStrength) -> Vec<String> {
    use speccoder_core::strength::{PermittedOutcome, summary};
    let why = a.notes.first().map(String::as_str);
    let mut out = vec![format!("Strength   {}", summary(&a.combined, why))];
    if a.held_out {
        out.push(format!(
            "  visible tests alone: {}",
            summary(&a.visible, None)
        ));
    }
    for s in a.combined.survivors.iter().take(6) {
        out.push(format!("  survived: {s}"));
    }
    for p in &a.permitted {
        match p.outcome {
            PermittedOutcome::Rejected => out.push(format!(
                "  rejected a permitted alternative: {} — {} ({}){}",
                p.scenario,
                p.rationale,
                p.id,
                p.patch
                    .as_deref()
                    .map(|x| format!(", {x}"))
                    .unwrap_or_default()
            )),
            PermittedOutcome::Ruled => out.push(format!(
                "  ruled: the tests are right about {} ({})",
                p.scenario, p.id
            )),
            _ => {}
        }
    }
    out
}

/// `speccoder feature`: one line per feature — the current one marked, its
/// progress as a bar, its artifacts.
#[must_use]
pub fn features(r: &speccoder_core::FeaturesReport) -> Vec<String> {
    use crate::style::Token;
    let look = crate::style::out();
    let width = r
        .features
        .iter()
        .map(|f| f.name.chars().count())
        .max()
        .unwrap_or(0);
    r.features
        .iter()
        .map(|f| {
            let progress = if f.tasks == 0 {
                "no tasks yet".to_owned()
            } else {
                format!("{}/{} tasks", f.done, f.tasks)
            };
            if look.screen_reader {
                return format!(
                    "{} {}: {}{} — {progress} — {}",
                    r.unit,
                    f.number,
                    f.name,
                    if f.current { " (current)" } else { "" },
                    if f.artifacts.is_empty() {
                        "nothing written".to_owned()
                    } else {
                        f.artifacts.join(", ")
                    }
                );
            }
            let filled = (f.done * 10).checked_div(f.tasks).unwrap_or(0);
            let bar = format!("{}{}", "■".repeat(filled), "□".repeat(10 - filled));
            let bar = if look.ascii {
                crate::style::asciify(&bar)
            } else {
                bar
            };
            let mark = match (f.current, look.ascii) {
                (true, true) => ">",
                (true, false) => "▸",
                (false, _) => " ",
            };
            let line = format!(
                "{mark} {:>2}  {:<width$}  {}  {:<12} {}",
                f.number,
                f.name,
                look.paint(
                    if f.tasks > 0 && f.done == f.tasks {
                        Token::Verified
                    } else {
                        Token::Accent
                    },
                    &bar
                ),
                progress,
                look.paint(Token::Muted, &f.artifacts.join(" · "))
            );
            let line = if look.ascii {
                crate::style::asciify(&line)
            } else {
                line
            };
            if f.current {
                look.paint(Token::Accent, &line)
            } else {
                line
            }
        })
        .collect()
}

/// The journey as one line: `✓ spec ── ✓ plan ── ● tasks ── · commit ──
/// · run ── · review` — what is done, the step now, what is left; each in
/// its meaning's colour, ASCII and screen-reader words when asked.
#[must_use]
pub fn journey(steps: &[speccoder_core::actions::JourneyStep]) -> String {
    journey_in(&crate::style::out(), steps)
}

/// [`journey`] in `look` (a prompt's, on stderr).
#[must_use]
pub fn journey_in(
    look: &crate::style::Look,
    steps: &[speccoder_core::actions::JourneyStep],
) -> String {
    use crate::style::Token;
    use speccoder_core::actions::StepState;
    if look.screen_reader {
        let words: Vec<String> = steps
            .iter()
            .map(|s| {
                let w = match s.state {
                    StepState::Done => "done",
                    StepState::Now => "next",
                    StepState::Todo => "to do",
                    StepState::Skipped => "skipped",
                };
                format!("{} {w}", s.name)
            })
            .collect();
        return format!("journey: {}", words.join(", "));
    }
    let (done, now, todo, skipped, join) = if look.ascii {
        ("[x]", "[>]", "[ ]", "[-]", " - ")
    } else {
        ("✓", "●", "·", "–", " ── ")
    };
    let cells: Vec<String> = steps
        .iter()
        .map(|s| match s.state {
            StepState::Done => look.paint(Token::Verified, &format!("{done} {}", s.name)),
            StepState::Now => look.paint(Token::Accent, &format!("{now} {}", s.name)),
            StepState::Todo => look.paint(Token::Muted, &format!("{todo} {}", s.name)),
            StepState::Skipped => look.paint(Token::Muted, &format!("{skipped} {}", s.name)),
        })
        .collect();
    cells.join(&look.paint(Token::Muted, join))
}

/// A framed card in `look`: a title in the top border, the lines inside, padded to
/// the terminal's width (at most 76 columns).
#[must_use]
pub fn card_in(look: &crate::style::Look, title: &str, lines: &[String]) -> String {
    use crate::style::Token;
    let inner = look.width.clamp(40, 78).saturating_sub(4);
    let (tl, tr, bl, br, h, v) = if look.ascii {
        ("+", "+", "+", "+", "-", "|")
    } else {
        ("┌", "┐", "└", "┘", "─", "│")
    };
    let border = |t: &str| look.paint(Token::Border, t);
    let head = format!(" {title} ");
    let rest = inner.saturating_sub(crate::style::width(&head) + 1);
    let mut out = format!(
        "  {}{}{}{}\n",
        border(&format!("{tl}{h}")),
        look.paint(Token::Accent, &head),
        border(&h.repeat(rest)),
        border(tr)
    );
    for l in lines {
        let l = look.fit(l, inner - 2);
        let pad = (inner - 2).saturating_sub(crate::style::width(&l));
        out.push_str(&format!(
            "  {} {l}{} {}\n",
            border(v),
            " ".repeat(pad),
            border(v)
        ));
    }
    out.push_str(&format!(
        "  {}\n",
        border(&format!("{bl}{}{br}", h.repeat(inner)))
    ));
    out
}

/// `speccoder config`: per section, each key with its value — set values
/// plain, defaults marked — and the uncommitted change, if any.
#[must_use]
pub fn config(c: &speccoder_core::ConfigReport) -> Vec<String> {
    let shown = |k: &speccoder_core::ConfigKey| match (&k.value, &k.default) {
        (Some(v), _) => v.to_string(),
        (None, Some(d)) => format!("{d}  (default)"),
        (None, None) if k.required => "—  (required)".to_owned(),
        (None, None) => "—".to_owned(),
    };
    let mut out = Vec::new();
    for s in &c.sections {
        if s.named {
            out.push(format!("[{}.<name>]  {}", s.name, s.title));
            if s.tables.is_empty() {
                out.push("  (none)".to_owned());
            }
            for t in &s.tables {
                for k in t.keys.iter().filter(|k| k.value.is_some()) {
                    out.push(format!("  {}.{}.{} = {}", s.name, t.name, k.key, shown(k)));
                }
            }
        } else {
            out.push(format!("[{}]  {}", s.name, s.title));
            for k in &s.keys {
                out.push(format!("  {}.{} = {}", s.name, k.key, shown(k)));
            }
        }
    }
    if let Some(d) = &c.diff {
        out.push(String::new());
        out.push("Not committed yet:".to_owned());
        out.extend(
            d.lines()
                .filter(|l| l.starts_with('+') || l.starts_with('-'))
                .filter(|l| !l.starts_with("+++") && !l.starts_with("---"))
                .map(|l| format!("  {l}")),
        );
    }
    out
}

/// `inbox`: one block per decision — the kind, where, what to decide, what
/// it rests on, and the command that decides it (the first action's; the
/// others after it).
#[must_use]
pub fn inbox(r: &speccoder_core::InboxReport, look: &crate::style::Look) -> Vec<String> {
    use crate::style::{Glyph, Token};
    let mut out = Vec::new();
    for i in &r.items {
        let g = match i.kind.as_str() {
            "unknown" | "approval" | "ruling" | "gaming" | "stale" | "aside" => Glyph::Attention,
            "stuck" => Glyph::Failed,
            "handoff" => Glyph::Verified,
            _ => Glyph::Ready,
        };
        let place = match &i.task {
            Some(t) => format!("{} {t}", i.feature),
            None => i.feature.clone(),
        };
        out.push(format!(
            "{} {:<9} {}  {}",
            look.cell(g),
            i.kind,
            look.paint(Token::Muted, &place),
            i.title
        ));
        if !i.detail.is_empty() {
            out.push(format!("            {}", shorten(&i.detail, 100)));
        }
        if let Some((first, rest)) = i.actions.split_first() {
            out.push(format!(
                "            {}",
                look.paint(Token::Accent, &format!("→ {}", first.command))
            ));
            let others: Vec<&str> = rest
                .iter()
                .map(|a| a.label.split(' ').next().unwrap_or_default())
                .collect();
            if !others.is_empty() {
                out.push(format!("            or: {}", others.join(", ")));
            }
        }
    }
    out
}

/// `review`: one card per requirement — its state, its wording, who
/// decided and what they asked.
#[must_use]
pub fn review(r: &speccoder_core::ReviewReport, look: &crate::style::Look) -> Vec<String> {
    use crate::style::{Glyph, Token};
    let mut out = Vec::new();
    if !r.questions.is_empty() {
        out.push(look.paint(Token::Attention, "Open questions"));
        for q in &r.questions {
            out.push(format!(
                "{} {:<7} {}",
                look.cell(Glyph::Attention),
                q.id,
                shorten(&q.text, 90)
            ));
            out.push(format!(
                "            {}",
                look.paint(
                    Token::Muted,
                    &format!(
                        "{}:{}{}",
                        q.file,
                        q.line,
                        q.near
                            .as_deref()
                            .map(|n| format!(" · {n}"))
                            .unwrap_or_default()
                    )
                )
            ));
        }
        out.push(look.paint(
            Token::Accent,
            &format!(
                "            → speccoder review answer {} \"…\"",
                r.questions[0].id
            ),
        ));
        out.push(String::new());
    }
    for i in &r.requirements {
        let (g, word) = match i.state.as_str() {
            "approved" => (Glyph::Verified, "approved"),
            "changes_requested" => (Glyph::Attention, "changes requested"),
            "lapsed" => (Glyph::Attention, "changed since decided"),
            _ => (Glyph::Ready, "to review"),
        };
        out.push(format!(
            "{} {:<7} {}  {}",
            look.cell(g),
            i.id,
            shorten(&i.text, 90),
            look.paint(g.token(), word)
        ));
        if let Some(was) = &i.decided_on {
            out.push(format!("            was: {}", shorten(was, 90)));
        }
        match (&i.by, &i.comment) {
            (Some(by), Some(c)) => out.push(format!(
                "            {} — {}",
                look.paint(Token::Muted, by),
                shorten(c, 90)
            )),
            (Some(by), None) => out.push(format!("            {}", look.paint(Token::Muted, by))),
            _ => {}
        }
    }
    if !r.changes.is_empty() {
        out.push(String::new());
        out.push(look.paint(Token::Accent, "Since the last commit"));
        for c in &r.changes {
            let how = match (&c.then, &c.now) {
                (None, _) => "added",
                (_, None) => "removed",
                _ => "reworded",
            };
            out.push(format!("  {:<7} {how}", c.id));
        }
    }
    if r.history.len() > 1 {
        out.push(String::new());
        out.push(look.paint(Token::Muted, "History"));
        for v in r.history.iter().take(5) {
            let mut what: Vec<String> = Vec::new();
            for (n, w) in [
                (&v.added, "added"),
                (&v.reworded, "reworded"),
                (&v.removed, "removed"),
            ] {
                if !n.is_empty() {
                    what.push(format!("{} {w}", n.len()));
                }
            }
            out.push(format!(
                "  {} {}  {}  {}",
                v.date,
                v.commit.get(..7).unwrap_or(&v.commit),
                shorten(&v.subject, 50),
                look.paint(Token::Muted, &what.join(", "))
            ));
        }
    }
    out
}
