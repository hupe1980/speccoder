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
    let line = format!(
        "{check} {:<5} {p:<3} {story:<4} {:<width$}",
        t.id,
        shorten(&t.title, TITLE_WIDTH),
        width = TITLE_WIDTH
    );
    let state = match st.state {
        TaskState::Done => String::new(),
        TaskState::Ready => "ready".to_owned(),
        TaskState::Blocked => blocked_by(&st.blocked_by),
    };
    format!("{line}  {state}").trim_end().to_owned()
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

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut.trim_end())
    }
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
pub fn worktree_refused(w: &mut impl Write, message: &str, r: &WorktreeReport) -> io::Result<()> {
    writeln!(w, "speccoder worktree: {message}")?;
    for p in &r.refused_paths {
        writeln!(w, "  {:<10} {}", p.reason, p.path)?;
    }
    if let Some(h) = &r.hint {
        writeln!(w, "{h}")?;
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
        writeln!(w, "  ? {} {} — {}", p.task, p.covers, p.reason)?;
        writeln!(
            w,
            "      speccoder approve --task {0}   |   speccoder reject --task {0} --reason \"…\"",
            p.task
        )?;
    }
    let by = |b: &Option<String>| b.as_ref().map(|b| format!(" — by {b}")).unwrap_or_default();
    for d in &a.decided {
        match &d.reason {
            Some(r) => writeln!(w, "  ✗ {} {} rejected: {r}{}", d.task, d.covers, by(&d.by))?,
            None => writeln!(w, "  ✓ {} {} approved{}", d.task, d.covers, by(&d.by))?,
        }
    }
    writeln!(w, "{message}")
}

/// A feature's tasks and their evidence.
pub fn status(w: &mut impl Write, r: &StatusReport, message: &str) -> io::Result<()> {
    writeln!(
        w,
        "Feature    {} (from {})",
        r.feature,
        r.branch.as_deref().unwrap_or("the checkout")
    )?;
    let mut phase = "";
    for t in &r.tasks {
        if t.phase != phase {
            phase = &t.phase;
            writeln!(w, "{phase}")?;
        }
        let (mark, note) = match t.state.as_str() {
            "verified" => ("✓", "verified".to_owned()),
            "regression_only" => ("~", "regression only".to_owned()),
            "ticked_without_evidence" => ("!", "ticked, no speccoder evidence".to_owned()),
            "attested" => ("A", "attested by a person (not verified)".to_owned()),
            "skipped" => ("–", "skipped".to_owned()),
            "ready" => ("·", "ready".to_owned()),
            _ => ("⧗", format!("blocked by {}", t.blocked_by.join(", "))),
        };
        writeln!(w, "  {mark} {} {:<52} {note}", t.id, shorten(&t.title, 52))?;
        if let Some(n) = &t.note {
            writeln!(w, "      reason: {}", shorten(n, 90))?;
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
                .filter(|c| matches!(c.classification.as_str(), "correct" | "honest_stop"))
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
    if let Some(t) = e.summary.tokens_per_correct {
        writeln!(w, "Tokens     {t} per correct case")?;
    }
    writeln!(w, "{message}")
}

/// The `Next:` block: each command padded to the longest (at most 44), then
/// why; nothing when there is no step.
pub fn next(w: &mut dyn Write, steps: &[speccoder_core::next::NextStep]) -> std::io::Result<()> {
    if steps.is_empty() {
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
        writeln!(w, "{label}{:<width$}  {}", s.command, s.why)?;
    }
    Ok(())
}
