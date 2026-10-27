//! Running one step of a new project's plan: exactly its argv, in the
//! folder, with stdin closed, a time limit, the person's environment
//! without anything that looks like a credential, tool caches in speccoder's private
//! cache, and — where the OS offers one — in the sandbox with writes
//! confined to the folder and that cache, and the network only for a step
//! the plan labels as reaching it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use speccoder_core::new_project::{PlanStepResult, PlanStepState, StepAccess};

use crate::sandbox::{self, Network};
use crate::verify::process::{self, Invocation};

/// The lines of output a failed step shows.
const TAIL: usize = 20;

/// speccoder's private cache for a tool's steps.
pub(crate) fn cache_dir(state: &Path, tool: &str) -> PathBuf {
    state.join("cache").join("new").join(tool)
}

/// The variables that send a tool's caches into `cache`.
fn cache_env(cache: &Path) -> Vec<(String, String)> {
    let c = |sub: &str| cache.join(sub).display().to_string();
    vec![
        ("UV_CACHE_DIR".to_owned(), c("uv")),
        ("UV_PYTHON_INSTALL_DIR".to_owned(), c("uv-python")),
        ("npm_config_cache".to_owned(), c("npm")),
        ("npm_config_update_notifier".to_owned(), "false".to_owned()),
        ("GOMODCACHE".to_owned(), c("gomod")),
        ("GOCACHE".to_owned(), c("gobuild")),
        ("GOTELEMETRY".to_owned(), "off".to_owned()),
    ]
}

/// The cache `tool`'s steps filled, when there is one, and the variables
/// that point the tool at it: where a worktree's offline install of a
/// project `speccoder new` made finds its packages.
pub(crate) fn filled_cache(state: &Path, tool: &str) -> Option<(PathBuf, Vec<(String, String)>)> {
    let cache = cache_dir(state, tool);
    cache.is_dir().then(|| {
        let env = cache_env(&cache);
        (cache, env)
    })
}

/// A step's environment: `vars` without anything that looks like a
/// credential (the rule checks' test — model keys, tokens, every `AWS_*`),
/// then the cache redirections and the step's own variables.
fn step_env(
    vars: impl IntoIterator<Item = (String, String)>,
    cache: &Path,
    own: &[(String, String)],
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vars
        .into_iter()
        .filter(|(k, _)| !process::is_secret(k))
        .collect();
    let mut extra = cache_env(cache);
    extra.extend(own.iter().cloned());
    for (k, v) in extra {
        env.retain(|(key, _)| *key != k);
        env.push((k, v));
    }
    env
}

/// One step to run.
pub(crate) struct Run<'a> {
    pub id: &'a str,
    pub argv: &'a [String],
    pub access: StepAccess,
    pub limit_s: u32,
    pub env: &'a [(String, String)],
}

/// The last `TAIL` lines of a step's output.
pub(crate) fn tail(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let skip = lines.len().saturating_sub(TAIL);
    lines.into_iter().skip(skip).collect()
}

/// Runs `step` in `target`.
pub(crate) fn run(target: &Path, state: &Path, step: &Run<'_>) -> PlanStepResult {
    let tool = step.argv.first().map_or("tool", String::as_str);
    let cache = cache_dir(state, tool);
    let failed = |message: String| PlanStepResult {
        id: step.id.to_owned(),
        state: PlanStepState::Failed,
        exit: None,
        duration_ms: 0,
        output_tail: vec![message],
    };
    if let Err(e) = std::fs::create_dir_all(&cache) {
        return failed(format!("speccoder's cache {}: {e}", cache.display()));
    }
    let env = step_env(std::env::vars(), &cache, step.env);
    let config = sandbox::Config {
        network: Network::Off,
        writable: vec![cache.display().to_string()],
        ..sandbox::Config::default()
    };
    let runner = match sandbox::runner(target, &config) {
        Ok((r, _)) => r,
        Err(e) => return failed(e),
    };
    // Once the repository exists, its own files are read-only to a step: a
    // driver planted in `.git/config` would run in speccoder's commit,
    // outside the sandbox. (Where the OS cannot carve it, the run compares
    // `.git` before and after the step instead.)
    let git = target.join(".git");
    let runner = if git.is_dir() {
        runner.carve(&[git], &[]).0
    } else {
        runner
    };
    let inv = Invocation {
        argv: step.argv.to_vec(),
        cwd: target.to_path_buf(),
        env,
        timeout: Duration::from_secs(u64::from(step.limit_s)),
        output_cap: 256 * 1024,
    };
    let out = runner.run_with(&inv, step.access == StepAccess::Network);
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout.kept),
        String::from_utf8_lossy(&out.stderr.kept)
    );
    let mut output_tail = tail(&text);
    let ok = out.exit_code == Some(0) && !out.timed_out && out.spawn_error.is_none();
    if let Some(e) = &out.spawn_error {
        output_tail.push(e.clone());
    }
    if out.timed_out {
        output_tail.push(format!(
            "timed out after {}s; it may be waiting for input — run it yourself: {}",
            step.limit_s,
            step.argv.join(" ")
        ));
    }
    PlanStepResult {
        id: step.id.to_owned(),
        state: if ok {
            PlanStepState::Done
        } else {
            PlanStepState::Failed
        },
        exit: out.exit_code,
        duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
        output_tail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_keeps_the_last_twenty_lines_that_say_something() {
        let text: String = (1..=30).map(|i| format!("line {i}\n\n")).collect();
        let t = tail(&text);
        assert_eq!(t.len(), 20);
        assert_eq!(t.first().map(String::as_str), Some("line 11"));
        assert_eq!(t.last().map(String::as_str), Some("line 30"));
    }

    #[test]
    fn a_step_never_sees_a_credential() {
        let vars = [
            ("PATH", "/bin"),
            ("HOME", "/h"),
            ("AWS_BEARER_TOKEN_BEDROCK", "b"),
            ("AWS_PROFILE", "p"),
            ("AWS_SECRET_ACCESS_KEY", "s"),
            ("ANTHROPIC_API_KEY", "k"),
            ("GITHUB_TOKEN", "t"),
            ("NPM_TOKEN", "t"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        let own = [("FOO".to_owned(), "1".to_owned())];
        let env = step_env(vars, Path::new("/c"), &own);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        for k in [
            "AWS_BEARER_TOKEN_BEDROCK",
            "AWS_PROFILE",
            "AWS_SECRET_ACCESS_KEY",
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "NPM_TOKEN",
        ] {
            assert!(!keys.contains(&k), "{k} reached the step");
        }
        for k in ["PATH", "HOME", "FOO", "UV_CACHE_DIR"] {
            assert!(keys.contains(&k), "{k} is missing");
        }
    }
}
