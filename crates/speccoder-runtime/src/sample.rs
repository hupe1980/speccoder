//! Try the sample (F-131): a small project and a run of it recorded with
//! a real model, bundled into release builds, copied into speccoder's own
//! state directory (never the person's workspace) and verified by strict
//! replay — no model, no key, no network. A run this build cannot
//! reproduce is reported as such, never shown as if it had worked; a
//! build without the recording says so (`SAMPLE-MISSING`), never shows an
//! empty playback.
//!
//! The recording is made by the maintainers with `just record-sample`
//! (`scripts/record-sample.sh`), never in CI.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, SampleRecording, SampleReport, SampleRun,
};

const COMMAND: &str = "sample";

/// The recording's manifest (`manifest.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Manifest {
    /// The project's name (`wordcount`).
    pub case: String,
    /// The recorded runs, in order.
    pub run_ids: Vec<String>,
    /// The provider it was recorded with.
    pub provider: String,
    /// The model.
    pub model: String,
    /// The agentplane version that recorded it.
    pub agentplane: String,
    /// The speccoder version that recorded it.
    pub speccoder: String,
    /// When (RFC 3339).
    pub recorded_at: String,
    /// `reproduced`.
    pub expect: String,
}

/// A sample: the project's history as a git bundle, the journal, the manifest.
#[derive(Debug, Clone)]
pub struct Asset {
    /// `git bundle create --all` of the project.
    pub bundle: Vec<u8>,
    /// The agentplane journal of the recorded runs.
    pub journal: Vec<u8>,
    /// `manifest.toml`.
    pub manifest: String,
}

/// The sample bundled into this build, if it has one.
#[must_use]
pub fn bundled() -> Option<Asset> {
    bundled_asset()
}

#[cfg(speccoder_sample)]
fn bundled_asset() -> Option<Asset> {
    Some(Asset {
        bundle: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/sample/sample.bundle"
        ))
        .to_vec(),
        // The journal is mostly preallocated space: bundled compressed.
        journal: gunzip(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/sample/journal.redb.gz"
        )))?,
        manifest: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/sample/manifest.toml"
        ))
        .to_owned(),
    })
}

/// A gzip stream's contents; `None` when it is not one.
#[cfg_attr(not(speccoder_sample), allow(dead_code))]
fn gunzip(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

#[cfg(not(speccoder_sample))]
const fn bundled_asset() -> Option<Asset> {
    None
}

/// Whether this build has the sample.
#[must_use]
pub fn available() -> bool {
    cfg!(speccoder_sample)
}

/// Where the sample's copies live.
#[must_use]
pub fn home(state: &Path) -> PathBuf {
    state.join("sample").join(env!("CARGO_PKG_VERSION"))
}

/// The copy of the project, if one was made.
#[must_use]
pub fn copy(state: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home(state).join("sample.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let p = PathBuf::from(v["path"].as_str()?);
    p.is_dir().then_some(p)
}

/// The sample is missing from this build.
#[must_use]
pub fn missing() -> CommandResult {
    CommandResult::new(
        COMMAND,
        CommandStatus::NotFound,
        "this build has no sample: its recorded run is bundled into release builds once it is recorded",
    )
    .with_code("SAMPLE-MISSING")
}

/// `speccoder sample [--fresh]`: the bundled sample, copied and replayed.
#[must_use]
pub fn open(state: &Path, fresh: bool) -> CommandResult {
    match bundled() {
        Some(a) => open_from(&a, state, fresh),
        None => missing(),
    }
}

fn failed(m: &str) -> CommandResult {
    CommandResult::new(COMMAND, CommandStatus::Error, m)
}

/// [`open`] for `asset`: a copy under `<state>/sample/<version>/` (an
/// existing one is reopened unless `fresh`), its journal placed in the
/// copy's project state, every recorded run replayed strictly.
#[must_use]
pub fn open_from(asset: &Asset, state: &Path, fresh: bool) -> CommandResult {
    let manifest: Manifest = match toml::from_str(&asset.manifest) {
        Ok(m) => m,
        Err(e) => return failed(&format!("the sample's manifest is unreadable: {e}")),
    };
    let base = home(state);
    let dir = base.join(&manifest.case);
    let made = fresh || !dir.join(".git").exists();
    if made {
        if dir.exists()
            && let Err(e) = std::fs::remove_dir_all(&dir)
        {
            return failed(&format!("cannot replace the sample's copy: {e}"));
        }
        if let Err(e) = std::fs::create_dir_all(&base) {
            return failed(&format!("{}: {e}", base.display()));
        }
        let bundle = base.join("sample.bundle");
        if let Err(e) = std::fs::write(&bundle, &asset.bundle) {
            return failed(&format!("{}: {e}", bundle.display()));
        }
        let cloned = crate::git::Git::at(&base).run(&[
            "clone",
            "--quiet",
            "--",
            &bundle.display().to_string(),
            &dir.display().to_string(),
        ]);
        let _ = std::fs::remove_file(&bundle);
        if let Err(e) = cloned {
            return failed(&format!(
                "the sample's history could not be unpacked: {}",
                e.0
            ));
        }
        let plane = crate::worktree::project_state_dir(state, &dir);
        if let Err(e) = std::fs::create_dir_all(&plane)
            .and_then(|()| std::fs::write(plane.join(crate::substrate::JOURNAL), &asset.journal))
        {
            return failed(&format!("the sample's journal could not be placed: {e}"));
        }
    }
    let runs: Vec<SampleRun> = manifest
        .run_ids
        .iter()
        .map(|id| {
            let r = crate::replay_with(&dir, id, state);
            let result = match r.data() {
                Some(CommandData::Replay(rep)) => rep.result.clone(),
                _ => "not_replayable".to_owned(),
            };
            SampleRun {
                id: id.clone(),
                finding: (result != "reproduced").then(|| r.message().to_owned()),
                result,
            }
        })
        .collect();
    let report = SampleReport {
        path: dir.display().to_string(),
        fresh: made,
        recorded: SampleRecording {
            model: manifest.model.clone(),
            recorded_at: manifest.recorded_at.clone(),
            agentplane: manifest.agentplane.clone(),
        },
        runs,
    };
    if let Err(e) = std::fs::write(
        base.join("sample.json"),
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "path": report.path,
            "recorded": report.recorded,
            "runs": report.runs,
        })
        .to_string(),
    ) {
        return failed(&format!(
            "the sample's record (sample.json) could not be written, so the app cannot show it: {e}"
        ));
    }
    let diverged = report.runs.iter().find(|r| r.result != "reproduced");
    let result = match diverged {
        Some(r) => CommandResult::new(
            COMMAND,
            CommandStatus::ReplayDiverged,
            &format!(
                "this build cannot reproduce the sample's recorded run — please report it: {}",
                r.finding.clone().unwrap_or_default()
            ),
        )
        .with_code("SAMPLE-DIVERGED"),
        None => CommandResult::new(
            COMMAND,
            CommandStatus::Ok,
            &format!(
                "sample ready at {} — {} recorded run{} reproduced (no model called)",
                crate::welcome::tilde(&dir),
                report.runs.len(),
                if report.runs.len() == 1 { "" } else { "s" }
            ),
        )
        .with_next(vec![
            speccoder_core::next::NextStep {
                command: format!("speccoder ui -C {}", crate::welcome::tilde(&dir)),
                why: "watch the recorded run play back".to_owned(),
            },
            speccoder_core::next::NextStep {
                command: "speccoder new \"…\"".to_owned(),
                why: "start your own".to_owned(),
            },
        ]),
    };
    result.with_data(CommandData::Sample(Box::new(report)))
}

/// The tool the sample's checks need that is not on the `PATH`, with how
/// to get it (`None`: they can run here).
#[must_use]
pub fn missing_tool(copy: &Path) -> Option<String> {
    let text = std::fs::read_to_string(copy.join(crate::verify::config::FILE)).ok()?;
    let checks = crate::verify::config::parse(&text).ok()?;
    let env: Vec<(String, String)> = std::env::vars().collect();
    checks
        .iter()
        .filter_map(|c| c.argv.first())
        .find(|p| crate::sandbox::resolve_executable(p, copy, &env).is_none())
        .map(|p| match p.as_str() {
            "cargo" => "cargo is not installed — install it: https://rustup.rs".to_owned(),
            other => format!("{other} is not installed"),
        })
}

/// *Run the checks yourself*: the sample's own fast checks, in its copy,
/// confined like a run's (no network). Refused anywhere but the sample.
#[must_use]
pub fn run_checks(state: &Path, root: &Path) -> CommandResult {
    let copy = copy(state).and_then(|c| std::fs::canonicalize(c).ok());
    if copy.is_none() || copy != std::fs::canonicalize(root).ok() {
        return CommandResult::new(
            COMMAND,
            CommandStatus::UsageError,
            "these are the sample's checks: run them in its copy (speccoder sample)",
        )
        .with_code("USAGE");
    }
    if let Some(why) = missing_tool(root) {
        return CommandResult::new(COMMAND, CommandStatus::ConfigError, &why).with_code("CONFIG");
    }
    let runner = match crate::sandbox::runner_at(root) {
        Ok((r, _)) => r,
        Err(e) => return CommandResult::new(COMMAND, CommandStatus::ConfigError, &e),
    };
    match crate::verify::run_checks(root, Some(crate::verify::config::Tier::Fast), &runner) {
        Ok(report) => {
            let (status, message) = if report.passed {
                (CommandStatus::Ok, "the sample's checks pass here, offline")
            } else {
                (
                    CommandStatus::VerificationFailed,
                    "a check of the sample fails here",
                )
            };
            CommandResult::new(COMMAND, status, message)
                .with_data(CommandData::Verify(Box::new(report)))
        }
        Err(e) => CommandResult::new(COMMAND, CommandStatus::Error, &e),
    }
}

#[cfg(test)]
mod gzip_tests {
    #[test]
    fn a_compressed_journal_unpacks_and_anything_else_is_refused() {
        use std::io::Write as _;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        gz.write_all(b"journal bytes").unwrap();
        let packed = gz.finish().unwrap();
        assert_eq!(
            super::gunzip(&packed).as_deref(),
            Some(&b"journal bytes"[..])
        );
        assert_eq!(super::gunzip(b"not gzip"), None);
    }
}
