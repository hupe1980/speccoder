//! `speccoder sample`: the bundled sample, copied into speccoder's state
//! directory and replayed strictly — no model, no key, no network.

use std::io::Write;

use speccoder_core::{CommandResult, CommandStatus, SampleReport};

/// `speccoder sample [--fresh] [--yes]`.
#[must_use]
pub fn run(fresh: bool, yes: bool, interactive: bool) -> CommandResult {
    let Some(state) = speccoder_runtime::worktree::state_dir() else {
        return CommandResult::new(
            "sample",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR");
    };
    if !speccoder_runtime::sample::available() {
        return speccoder_runtime::sample::missing();
    }
    // Starting over discards the person's changes to the copy: asked first.
    let exists = speccoder_runtime::sample::copy(&state).is_some();
    if fresh && exists && !yes {
        if !interactive {
            return CommandResult::new(
                "sample",
                CommandStatus::NeedsHuman,
                "starting over discards your changes to the sample; nothing was replaced",
            )
            .with_code("NEW-CONFIRM")
            .with_fix(vec!["speccoder sample --fresh --yes".to_owned()]);
        }
        eprint!("Start over? Your changes to the sample are discarded. [y/N] ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return CommandResult::new("sample", CommandStatus::Ok, "the sample was kept as it is");
        }
    }
    speccoder_runtime::sample::open(&state, fresh)
}

/// The human rendering of a `sample` payload.
pub fn render(
    out: &mut dyn Write,
    r: &SampleReport,
    result: &CommandResult,
) -> std::io::Result<()> {
    writeln!(
        out,
        "A recorded run of {}, recorded {} with {} (agentplane {})",
        std::path::Path::new(&r.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        r.recorded.recorded_at,
        r.recorded.model,
        r.recorded.agentplane
    )?;
    for run in &r.runs {
        writeln!(
            out,
            "  {} {}{}",
            run.id,
            run.result,
            run.finding
                .as_deref()
                .map(|f| format!(": {f}"))
                .unwrap_or_default()
        )?;
    }
    if result.status() == CommandStatus::Ok {
        writeln!(out, "ok: {}", result.message())?;
    }
    Ok(())
}
