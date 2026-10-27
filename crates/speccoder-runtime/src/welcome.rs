//! What a host started outside a project shows: the recent projects
//! (speccoder's own state, written when a project is opened or created)
//! and folder names to choose from — read-only, under the person's home.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most recent projects kept.
pub const MAX_RECENT: usize = 20;
/// The most folder names one listing returns.
pub const MAX_FOLDERS: usize = 200;

/// A recent project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    /// Its folder, canonical.
    pub path: String,
    /// Its name.
    pub name: String,
    /// Unix seconds it was last opened.
    pub opened_at: u64,
    /// speccoder created it.
    #[serde(default)]
    pub created: bool,
}

/// A recent project as the welcome shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentView {
    /// The record.
    #[serde(flatten)]
    pub recent: Recent,
    /// The folder, `~`-abbreviated.
    pub shown: String,
    /// The folder is still there.
    pub exists: bool,
    /// Undo is offered on it (created within the last day, nothing else happened).
    pub undo: Option<speccoder_core::new_project::UndoOffer>,
}

fn file(state: &Path) -> PathBuf {
    state.join("recent.json")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The recent projects, newest first.
#[must_use]
pub fn recent(state: &Path) -> Vec<Recent> {
    std::fs::read_to_string(file(state))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(state: &Path, list: &[Recent]) {
    let f = file(state);
    if let Some(p) = f.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Ok(text) = serde_json::to_string_pretty(list) {
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }
}

/// Whether a host started at `path` serves it right away: a spec project,
/// or a git repository (the setup assistant takes over there). Anywhere
/// else the host serves the welcome.
#[must_use]
pub fn is_project(path: &Path) -> bool {
    crate::format::open(path, None).is_ok()
        || crate::git::Git::at(path)
            .toplevel()
            .ok()
            .flatten()
            .is_some()
}

/// Records that the project at `root` was opened (or created) now.
pub fn remember(state: &Path, root: &Path, name: &str, created: bool) {
    let path = std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let mut list = recent(state);
    let was_created = list.iter().any(|r| r.path == path && r.created);
    list.retain(|r| r.path != path);
    list.insert(
        0,
        Recent {
            path,
            name: name.to_owned(),
            opened_at: now(),
            created: created || was_created,
        },
    );
    list.truncate(MAX_RECENT);
    save(state, &list);
}

/// Removes the project at `root` from the recent ones.
pub fn forget(state: &Path, root: &Path) {
    let path = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut list = recent(state);
    list.retain(|r| Path::new(&r.path) != path && r.path != root.display().to_string());
    save(state, &list);
}

/// `~/x` for a path under the home directory.
#[must_use]
pub fn tilde(p: &Path) -> String {
    match home() {
        Some(h) if p.starts_with(&h) => match p.strip_prefix(&h) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        },
        _ => p.display().to_string(),
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| std::fs::canonicalize(&h).unwrap_or(h))
}

/// The recent projects with whether each folder is still there (nothing
/// of a missing one is read) and Undo on a project created in the last day.
#[must_use]
pub fn views(state: &Path) -> Vec<RecentView> {
    let day = 24 * 60 * 60;
    recent(state)
        .into_iter()
        .map(|r| {
            let p = PathBuf::from(&r.path);
            let exists = p.is_dir();
            // A day from the creation, not from the last time it was opened.
            let made_at = crate::new_project::creation(state, &p).map(|c| c.at);
            let undo = (exists && made_at.is_some_and(|at| now().saturating_sub(at) < day))
                .then(|| crate::new_project::undo_offer(state, &p))
                .filter(|u| u.available);
            RecentView {
                shown: tilde(&p),
                exists,
                undo,
                recent: r,
            }
        })
        .collect()
}

/// Where New Project puts a folder by default: beside the last project
/// speccoder created, else `~/Workspaces` when it exists, else home.
#[must_use]
pub fn default_parent(state: &Path) -> PathBuf {
    if let Some(parent) = recent(state)
        .into_iter()
        .filter(|r| r.created)
        .find_map(|r| Path::new(&r.path).parent().map(Path::to_path_buf))
        .filter(|p| p.is_dir())
    {
        return parent;
    }
    match home() {
        Some(h) if h.join("Workspaces").is_dir() => h.join("Workspaces"),
        Some(h) => h,
        None => PathBuf::from("."),
    }
}

/// Folder names under `under` (which must lie in the person's home):
/// directories only, hidden ones only when `under`'s last part asks for
/// them, at most [`MAX_FOLDERS`]. `under` may end in a partial name: the
/// folders of its parent that start with it.
///
/// # Errors
///
/// `under` lies outside the home directory, or there is no home.
pub fn folders(under: &str) -> Result<Vec<String>, String> {
    let home = home().ok_or_else(|| "no home directory".to_owned())?;
    let expanded = match under.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None if under.is_empty() => home.clone(),
        None => PathBuf::from(under),
    };
    if !expanded.is_absolute() {
        return Err("give a path under your home folder".to_owned());
    }
    let (dir, prefix) = if under.ends_with('/') || expanded.is_dir() {
        (expanded.clone(), String::new())
    } else {
        (
            expanded
                .parent()
                .map_or_else(|| expanded.clone(), Path::to_path_buf),
            expanded
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    let dir = std::fs::canonicalize(&dir).map_err(|_| "not a folder".to_owned())?;
    if !dir.starts_with(&home) {
        return Err("only folders under your home folder are listed".to_owned());
    }
    let hidden = prefix.starts_with('.');
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|_| "not readable".to_owned())?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| hidden || !n.starts_with('.'))
        .filter(|n| n.starts_with(&prefix))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.truncate(MAX_FOLDERS);
    Ok(names.into_iter().map(|n| tilde(&dir.join(n))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_are_newest_first_once_each_and_forgotten_on_request() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let a = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let b = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        remember(state.path(), a.path(), "a", true);
        remember(state.path(), b.path(), "b", false);
        remember(state.path(), a.path(), "a", false);
        let r = recent(state.path());
        assert_eq!(
            r.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(r[0].created, "created stays");
        forget(state.path(), a.path());
        assert_eq!(recent(state.path()).len(), 1);
    }

    #[test]
    fn undo_is_offered_a_day_from_the_creation_not_from_the_last_open() {
        let run = |dir: &Path, args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(dir)
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok, "git {args:?}");
        };
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = std::fs::canonicalize(repo.path()).unwrap_or_else(|e| panic!("{e}"));
        run(&root, &["init", "-q"]);
        std::fs::write(root.join("a"), "a\n").unwrap_or_else(|e| panic!("{e}"));
        run(&root, &["add", "-A"]);
        run(&root, &["commit", "-qm", "new project: x"]);
        let head = crate::git::Git::at(&root).head().ok().flatten();
        let record = crate::new_project::Creation {
            path: root.display().to_string(),
            mode: speccoder_core::new_project::ProjectMode::ExistingRepository,
            commit: head,
            plan_digest: String::new(),
            name: "x".to_owned(),
            idea: String::new(),
            stack: speccoder_core::new_project::Stack::None,
            format: "speckit".to_owned(),
            by: None,
            via: speccoder_core::actions::Via::Terminal,
            at: now() - 2 * 24 * 60 * 60,
            first_feature_offered: false,
            undone_at: None,
            before: crate::new_project::Before::default(),
            left: None,
            git: None,
        };
        crate::new_project::write_creation(state.path(), &record).unwrap_or_else(|e| panic!("{e}"));
        // Opened again just now: still two days after it was made.
        remember(state.path(), &root, "x", true);
        let v = views(state.path());
        assert_eq!(v.len(), 1);
        assert!(v[0].undo.is_none(), "{:?}", v[0].undo);
    }
}
