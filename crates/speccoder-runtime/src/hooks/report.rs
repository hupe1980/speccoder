//! `speccoder hooks`: every hook with its trust and recent runs, and the
//! three things a person does with one — trust it, test it, deliver what
//! waits.

use std::path::Path;

use speccoder_core::hooks::{HookEntry, HookTested, HooksReport, Role, Runner, TrustState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::{Digested, HookDef};

fn toml_of(name: &str, definition: &serde_json::Value) -> String {
    let body = toml::to_string(definition).unwrap_or_default();
    format!("[hooks.{name}]\n{body}")
}

/// What keeps `def` from running here, with the fix.
fn problem(def: &HookDef, root: &Path) -> Option<String> {
    match &def.runner {
        Runner::Url {
            secret_env: Some(var),
            ..
        } if std::env::var_os(var).is_none() => Some(format!(
            "${var} is not set: export the signing key, or drop `secret_env`"
        )),
        Runner::Argv(argv) => {
            let program = argv.first()?;
            let found = if program.contains('/') {
                root.join(program).is_file() || Path::new(program).is_file()
            } else {
                std::env::var_os("PATH")
                    .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
            };
            (!found).then(|| format!("`{program}` is not found: install it, or fix `argv`"))
        }
        Runner::Url { .. } => None,
    }
}

/// One hook as the list shows it.
#[must_use]
pub fn entry(d: &Digested, root: &Path, dir: &Path) -> HookEntry {
    let def = &d.def;
    let state = super::trust::state(dir, &def.name, &d.digest);
    let (role, on) = match &def.role {
        Role::Gate { .. } => ("gate", Vec::new()),
        Role::Observer { on } => ("observer", on.clone()),
    };
    HookEntry {
        name: def.name.clone(),
        role: role.to_owned(),
        listens: def.listens(),
        at: def.at(),
        on,
        runs: match &def.runner {
            Runner::Argv(a) => a.join(" "),
            Runner::Url { url, .. } => url.clone(),
        },
        state,
        digest: d.digest.clone(),
        files: d.files.clone(),
        outside: d.outside.clone(),
        definition: toml_of(&def.name, &def.definition),
        trusted_definition: (state == TrustState::Changed)
            .then(|| super::trust::last_trusted(dir, &def.name))
            .flatten()
            .map(|t| toml_of(&def.name, &t.definition)),
        recent: super::runs(dir).remove(&def.name).unwrap_or_default(),
        trust_flag: format!("--trust-hook {}={}", def.name, d.digest),
        problem: problem(def, root),
    }
}

fn result(message: &str, report: HooksReport, status: CommandStatus) -> CommandResult {
    CommandResult::new("hooks", status, message).with_data(CommandData::Hooks(Box::new(report)))
}

fn read(
    root: &Path,
    state: &Path,
) -> Result<(Vec<Digested>, std::path::PathBuf), Box<CommandResult>> {
    match super::load(root) {
        Ok(defs) => Ok((
            defs.iter().map(|d| super::digest(d, root)).collect(),
            super::dir(state, root),
        )),
        Err(e) => Err(Box::new(
            result(
                &e,
                HooksReport {
                    hooks: Vec::new(),
                    problem: Some(e.clone()),
                    trusted: Vec::new(),
                    tested: None,
                    delivered: None,
                    pending: None,
                },
                CommandStatus::ConfigError,
            )
            .with_code("HOOK-MISCONFIGURED"),
        )),
    }
}

fn report_of(hooks: &[Digested], root: &Path, dir: &Path) -> HooksReport {
    HooksReport {
        hooks: hooks.iter().map(|d| entry(d, root, dir)).collect(),
        problem: None,
        trusted: Vec::new(),
        tested: None,
        delivered: None,
        pending: None,
    }
}

fn summary(r: &HooksReport) -> String {
    let waiting = r
        .hooks
        .iter()
        .filter(|h| h.state != TrustState::Trusted)
        .count();
    match (r.hooks.len(), waiting) {
        (0, _) => "no hooks: add a [hooks.<name>] to speccoder.toml (docs: hooks)".to_owned(),
        (1, 0) => "1 hook, trusted".to_owned(),
        (n, 0) => format!("{n} hooks, all trusted"),
        (n, w) => format!(
            "{n} hook{} — {w} wait{} for your trust",
            if n == 1 { "" } else { "s" },
            if w == 1 { "s" } else { "" }
        ),
    }
}

/// `speccoder hooks`.
#[must_use]
pub fn list(root: &Path, state: &Path) -> CommandResult {
    let (hooks, dir) = match read(root, state) {
        Ok(h) => h,
        Err(r) => return *r,
    };
    let report = report_of(&hooks, root, &dir);
    let message = summary(&report);
    result(&message, report, CommandStatus::Ok)
}

/// `speccoder hooks trust [<name>]`: each named hook (every one not
/// trusted, without a name) that `confirm` agrees to, after it saw the
/// entry. A newly trusted observer hears what happens from now on.
pub fn trust(
    root: &Path,
    state: &Path,
    name: Option<&str>,
    via: &str,
    confirm: &mut dyn FnMut(&HookEntry) -> bool,
) -> CommandResult {
    let (hooks, dir) = match read(root, state) {
        Ok(h) => h,
        Err(r) => return *r,
    };
    if let Some(n) = name
        && !hooks.iter().any(|h| h.def.name == n)
    {
        return result(
            &format!("there is no hook `{n}`"),
            report_of(&hooks, root, &dir),
            CommandStatus::NotFound,
        );
    }
    let by = crate::approvals::who_in(root);
    let mut trusted = Vec::new();
    for d in hooks
        .iter()
        .filter(|h| name.is_none_or(|n| h.def.name == n))
    {
        let e = entry(d, root, &dir);
        if e.state == TrustState::Trusted || !confirm(&e) {
            continue;
        }
        super::trust::record(&dir, d, &by, via);
        // A new observer hears what happens from now on; one trusted again
        // after a change goes on where it was, missing nothing.
        if d.def.at().is_none() && !super::deliver::has_cursor(&dir, &d.def.name) {
            super::deliver::start_here(&dir, root, &d.def.name);
        }
        trusted.push(d.def.name.clone());
    }
    let mut report = report_of(&hooks, root, &dir);
    let message = match trusted.len() {
        0 => summary(&report),
        _ => format!("trusted {}: it runs from now on", trusted.join(", ")),
    };
    report.trusted = trusted;
    result(&message, report, CommandStatus::Ok)
}

/// Trusts `name` from an interface that showed `digest`: refused when the
/// hook changed since.
#[must_use]
pub fn trust_shown(
    root: &Path,
    state: &Path,
    name: &str,
    digest: &str,
    via: &str,
) -> CommandResult {
    let current = super::load(root)
        .ok()
        .and_then(|h| h.into_iter().find(|d| d.name == name))
        .map(|d| super::digest(&d, root).digest);
    if current.as_deref() != Some(digest) {
        return CommandResult::new(
            "hooks",
            CommandStatus::UsageError,
            &format!("{name} changed since it was shown: look at it again before trusting"),
        )
        .with_code("CALL-CHANGED");
    }
    // Only the bytes that were shown: changed in between, nothing is trusted.
    trust(root, state, Some(name), via, &mut |e| e.digest == digest)
}

/// `speccoder hooks test <name>`: a gate alone on the checkout, or a sample
/// event to an observer. Only a trusted hook runs.
#[must_use]
pub fn test(root: &Path, state: &Path, name: &str) -> CommandResult {
    let (hooks, dir) = match read(root, state) {
        Ok(h) => h,
        Err(r) => return *r,
    };
    let Some(d) = hooks.iter().find(|h| h.def.name == name) else {
        return result(
            &format!("there is no hook `{name}`"),
            report_of(&hooks, root, &dir),
            CommandStatus::NotFound,
        );
    };
    if super::trust::state(&dir, name, &d.digest) != TrustState::Trusted {
        return result(
            &format!("{name} waits for your trust: it runs only after you trusted it"),
            report_of(&hooks, root, &dir),
            CommandStatus::NeedsHuman,
        )
        .with_code("HOOK-UNTRUSTED")
        .with_fix(vec![format!("speccoder hooks trust {name}")]);
    }
    let began = std::time::Instant::now();
    let tested = if d.def.at().is_some() {
        let g = super::gate::test(&d.def, root, state);
        HookTested {
            name: name.to_owned(),
            outcome: g.outcome.as_str().to_owned(),
            detail: g.reason,
            duration_ms: g.duration_ms,
        }
    } else {
        let sample = super::events::sample(&super::project_id(state, root));
        let sent = super::deliver::send(&d.def, &sample, root, state);
        let duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        super::record_run(
            &dir,
            name,
            super::HookRun {
                at: crate::attest::now_rfc3339(),
                what: "test".to_owned(),
                outcome: if sent.is_ok() { "delivered" } else { "failed" }.to_owned(),
                duration_ms,
                error: sent.as_ref().err().cloned(),
            },
        );
        HookTested {
            name: name.to_owned(),
            outcome: if sent.is_ok() { "delivered" } else { "failed" }.to_owned(),
            detail: Some(sent.unwrap_or_else(|e| e)),
            duration_ms,
        }
    };
    let words = match (tested.outcome.as_str(), tested.detail.as_deref()) {
        ("passed", _) => format!("{name} would pass"),
        ("blocked", Some(why)) => format!("{name} would block — {why}"),
        ("delivered", Some(answer)) => format!("{name} took the sample event ({answer})"),
        ("failed", Some(why)) => format!("{name} did not take the sample event — {why}"),
        (o, _) => format!("{name}: {o}"),
    };
    let mut report = report_of(&hooks, root, &dir);
    report.tested = Some(tested);
    result(&words, report, CommandStatus::Ok)
}

/// `speccoder hooks deliver`: what waits for every trusted observer.
#[must_use]
pub fn deliver(root: &Path, state: &Path) -> CommandResult {
    let (hooks, dir) = match read(root, state) {
        Ok(h) => h,
        Err(r) => return *r,
    };
    let done = super::deliver::pass(root, state, None);
    let mut report = report_of(&hooks, root, &dir);
    report.delivered = Some(done.delivered);
    report.pending = Some(done.pending);
    let mut message = if done.busy {
        "another speccoder process is delivering; it takes what waits".to_owned()
    } else {
        format!(
            "delivered {} event{}; {} pending",
            done.delivered,
            if done.delivered == 1 { "" } else { "s" },
            done.pending
        )
    };
    for (name, why) in &done.failed {
        message.push_str(&format!("\n{name} failed: {why}"));
    }
    if !done.untrusted.is_empty() {
        message.push_str(&format!(
            "\nskipped (not trusted): {}",
            done.untrusted.join(", ")
        ));
    }
    let status = if done.failed.is_empty() {
        CommandStatus::Ok
    } else {
        CommandStatus::Error
    };
    result(&message, report, status)
}

/// `--trust-hook name=digest`…: trusts each for its current digest only
/// (`via = ci`).
///
/// # Errors
///
/// A refusal naming the flag that does not hold.
pub fn apply_flags(root: &Path, state: &Path, flags: &[String]) -> Result<(), Box<CommandResult>> {
    if flags.is_empty() {
        return Ok(());
    }
    let (hooks, dir) = read(root, state)?;
    let parsed = flags
        .iter()
        .map(|f| super::trust::parse_flag(f))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            CommandResult::new("hooks", CommandStatus::UsageError, &e).with_code("USAGE")
        })?;
    super::trust::apply_flags(&dir, &hooks, &parsed, &crate::approvals::who_in(root)).map_err(
        |e| CommandResult::new("hooks", CommandStatus::NeedsHuman, &e).with_code("HOOK-UNTRUSTED"),
    )?;
    for (name, _) in &parsed {
        if hooks
            .iter()
            .any(|h| &h.def.name == name && h.def.at().is_none())
            && !super::deliver::has_cursor(&dir, name)
        {
            super::deliver::start_here(&dir, root, name);
        }
    }
    Ok(())
}

/// The notice a run gives once when observers are not trusted: they hear
/// nothing until a person trusts them.
#[must_use]
pub fn untrusted_notice(root: &Path, state: &Path) -> Option<String> {
    let (hooks, dir) = read(root, state).ok()?;
    let waiting: Vec<&str> = hooks
        .iter()
        .filter(|h| h.def.at().is_none())
        .filter(|h| super::trust::state(&dir, &h.def.name, &h.digest) != TrustState::Trusted)
        .map(|h| h.def.name.as_str())
        .collect();
    (!waiting.is_empty()).then(|| {
        format!(
            "{} not trusted: {} nothing until you trust {} (speccoder hooks trust)",
            waiting.join(", "),
            if waiting.len() == 1 {
                "it hears"
            } else {
                "they hear"
            },
            if waiting.len() == 1 { "it" } else { "them" }
        )
    })
}
