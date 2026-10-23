//! A format this version detects but does not run, so a project in one gets
//! "not yet supported" instead of "not a project".

use std::path::{Path, PathBuf};

use speccoder_core::spec::FormatKind;

use crate::format::{Adapter, Detect};

/// Kiro and cc-sdd: `.kiro/specs/<name>/tasks.md` beside `requirements.md`
/// or `bugfix.md`.
pub struct Kiro;

/// The Kiro detector.
pub static KIRO: Kiro = Kiro;

fn upward(start: &Path, marks: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let start = std::fs::canonicalize(start).ok()?;
    start.ancestors().find(|d| marks(d)).map(Path::to_path_buf)
}

impl Detect for Kiro {
    fn kind(&self) -> FormatKind {
        FormatKind::Kiro
    }

    fn root(&self, start: &Path) -> Option<PathBuf> {
        upward(start, |d| {
            std::fs::read_dir(d.join(".kiro/specs")).is_ok_and(|entries| {
                entries.flatten().any(|e| {
                    let s = e.path();
                    s.join("tasks.md").is_file()
                        && (s.join("requirements.md").is_file() || s.join("bugfix.md").is_file())
                })
            })
        })
    }

    fn adapter(&self) -> Option<&'static dyn Adapter> {
        None
    }
}
