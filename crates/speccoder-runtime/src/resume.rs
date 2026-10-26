//! Resuming an attempt a killed process left behind.
//!
//! Before an attempt, a marker records its admission key, the checkpoint it
//! started from and the acceptance tests it is judged by. A run that ends in
//! any ordinary way removes it ([`Guard`]); a killed process cannot, so a
//! marker on disk means an attempt was interrupted. The next run then keeps
//! the worktree as the attempt left it and admits the attempt under the same
//! key: agentplane finds the interrupted run and resumes it from its journal
//! — model calls and tool calls that finished are read back, not repeated —
//! and a run that had already finished is read back whole and judged again
//! (a task that passed just before the kill is ticked now).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// An attempt in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// The agentplane admission key.
    pub key: String,
    /// The checkpoint the attempt started from.
    pub before: String,
    /// The worktree's HEAD then; a different HEAD means the marker is stale.
    pub head: String,
    /// Which attempt of the task run.
    pub attempt: u32,
    /// Which best-of-n sample (0-based).
    pub sample: usize,
    /// The acceptance tests prepared for the task run.
    pub acceptance: Option<serde_json::Value>,
    /// The run the attempt belongs to: an invocation resuming it adopts it.
    #[serde(default)]
    pub run_id: Option<String>,
    /// The run's prompts when the attempt started.
    #[serde(default)]
    pub asked: u32,
    /// The run's grants when it started.
    #[serde(default)]
    pub run_grants: Vec<crate::tools::policy::Object>,
}

/// Where the marker of `feature/task` lives.
#[must_use]
pub fn path(plane_dir: &Path, feature: &str, task: &str) -> PathBuf {
    plane_dir
        .join("resume")
        .join(feature)
        .join(format!("{task}.json"))
}

/// The marker, if an attempt was interrupted.
#[must_use]
pub fn load(path: &Path) -> Option<Marker> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Records `marker` (written whole, then renamed: never half a marker).
pub fn save(path: &Path, marker: &Marker) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec(marker).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(tmp, path)
}

/// A fresh admission key: unique per attempt, so an ordinary rerun never
/// meets an old attempt's run.
#[must_use]
pub fn new_key(feature: &str, task: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let digest = Sha256::digest(format!(
        "{feature}/{task}/{nanos}/{}/{n}",
        std::process::id()
    ));
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("attempt-{hex}")
}

/// Removes the marker when a task run that started an attempt ends in any
/// ordinary way. A run refused before its attempt starts (a session check,
/// a worktree held by a waiting call) leaves an interrupted or waiting
/// attempt's marker alone: that attempt still resumes.
#[derive(Debug)]
pub struct Guard {
    path: PathBuf,
    armed: std::cell::Cell<bool>,
}

impl Guard {
    /// A guard for the marker at `path`, not yet armed.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            armed: std::cell::Cell::new(false),
        }
    }

    /// An attempt is starting: the marker goes when the run ends.
    pub fn arm(&self) {
        self.armed.set(true);
    }

    /// Leaves the marker in place: the attempt waits for a person and the
    /// next run resumes it.
    pub fn keep(self) {
        self.armed.set(false);
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.armed.get() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
