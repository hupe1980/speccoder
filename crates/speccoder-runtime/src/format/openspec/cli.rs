//! OpenSpec's own CLI as a second opinion: when `openspec` is
//! on PATH, `validate --strict --json` and `instructions apply --json` run
//! read-only, sandboxed, telemetry off, and speccoder's parse is compared
//! with theirs. Absent or unusable, the cross-check is skipped and says why.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use speccoder_core::snapshot::{Diagnostic, ProjectSnapshot};
use speccoder_openspec::crosscheck;

use crate::verify::process::{CommandRunner, Invocation, scrubbed_env};

/// `openspec` on `PATH`, if any.
#[must_use]
pub fn find() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("openspec"))
        .find(|p| p.is_file())
}

/// Runs `openspec <args>` in `cwd`; its stdout, or why not.
pub fn run(
    runner: &dyn CommandRunner,
    program: &Path,
    cwd: &Path,
    args: &[&str],
) -> Result<String, String> {
    let mut env = scrubbed_env(std::env::vars());
    for (k, v) in super::RULES.env {
        env.retain(|(name, _)| name != k);
        env.push(((*k).to_owned(), (*v).to_owned()));
    }
    let mut argv = vec![program.display().to_string()];
    argv.extend(args.iter().map(|a| (*a).to_owned()));
    let out = runner.run(&Invocation {
        argv,
        cwd: cwd.to_path_buf(),
        env,
        timeout: Duration::from_secs(60),
        output_cap: 4 * 1024 * 1024,
    });
    if let Some(e) = out.spawn_error {
        return Err(e);
    }
    if out.timed_out {
        return Err("timed out after 60 s".to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout.kept).into_owned())
}

/// The first JSON object in `text` (OpenSpec may print a notice first).
fn json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    serde_json::from_str(&text[start..]).ok()
}

/// Cross-check diagnostics for the selected change of `snapshot`.
#[must_use]
pub fn crosscheck(snapshot: &ProjectSnapshot, runner: &dyn CommandRunner) -> Vec<Diagnostic> {
    crosscheck_with(snapshot, runner, find())
}

/// [`crosscheck()`] with the `openspec` program given (`None`: not installed).
#[must_use]
pub fn crosscheck_with(
    snapshot: &ProjectSnapshot,
    runner: &dyn CommandRunner,
    program: Option<PathBuf>,
) -> Vec<Diagnostic> {
    let Some(f) = snapshot.feature.as_ref() else {
        return Vec::new();
    };
    let Some(program) = program else {
        return vec![crosscheck::skipped("`openspec` is not on PATH")];
    };
    let root = Path::new(&snapshot.root);
    match run(runner, &program, root, &["--version"]) {
        Ok(v)
            if v.trim()
                .split('.')
                .next()
                .and_then(|m| m.trim().parse::<u32>().ok())
                .is_some_and(|m| m >= 1) => {}
        Ok(v) => {
            return vec![crosscheck::skipped(&format!(
                "`openspec --version` said `{}`; 1.0 or newer is needed",
                v.trim()
            ))];
        }
        Err(e) => return vec![crosscheck::skipped(&format!("`openspec --version`: {e}"))],
    }
    let id = f.feature.name.as_str();
    let validate = run(
        runner,
        &program,
        root,
        &["validate", id, "--strict", "--json"],
    )
    .ok()
    .and_then(|t| json(&t));
    let apply = run(
        runner,
        &program,
        root,
        &["instructions", "apply", "--change", id, "--json"],
    )
    .ok()
    .and_then(|t| json(&t));
    if validate.is_none() && apply.is_none() {
        return vec![crosscheck::skipped("OpenSpec printed no JSON")];
    }
    crosscheck::compare(
        &f.feature.dir,
        f.tasks.as_ref(),
        validate.as_ref(),
        apply.as_ref(),
    )
}
