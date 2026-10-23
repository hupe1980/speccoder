//! Read-only access confined to a project root.
//!
//! Every path is resolved against the canonical root; absolute paths, `..`
//! components and anything that canonicalizes outside the root (symlinks
//! included) are refused before the file is opened.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Largest artifact that is read and parsed.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// The Spec Kit marker directory.
pub const MARKER: &str = ".specify";

/// Why a workspace path could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Nothing exists at the path.
    Missing,
    /// The path is absolute, uses `..`, or resolves outside the root.
    Escape,
    /// The file is larger than [`MAX_FILE_BYTES`].
    TooLarge(u64),
    /// The file is not UTF-8.
    NotUtf8,
    /// Any other IO failure.
    Io(String),
}

/// A project root that all reads are confined to.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

/// Finds the nearest ancestor of `start` (inclusive) that contains `.specify/`.
#[must_use]
pub fn find_root(start: &Path) -> Option<Workspace> {
    let start = fs::canonicalize(start).ok()?;
    start
        .ancestors()
        .find(|dir| dir.join(MARKER).is_dir())
        .map(|root| Workspace {
            root: root.to_path_buf(),
        })
}

impl Workspace {
    /// A workspace rooted at `root` (which must exist).
    #[must_use]
    pub fn at(root: &Path) -> Option<Self> {
        let root = fs::canonicalize(root).ok()?;
        root.is_dir().then_some(Self { root })
    }

    /// The canonical root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a workspace-relative path to an existing path inside the root.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, ReadError> {
        let rel_path = Path::new(rel);
        if rel_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(ReadError::Escape);
        }
        let joined = self.root.join(rel_path);
        if fs::symlink_metadata(&joined).is_err() {
            return Err(ReadError::Missing);
        }
        let canonical = fs::canonicalize(&joined).map_err(|_| ReadError::Escape)?;
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(ReadError::Escape)
        }
    }

    /// Whether `rel` is a file inside the root.
    #[must_use]
    pub fn is_file(&self, rel: &str) -> bool {
        self.resolve(rel).is_ok_and(|p| p.is_file())
    }

    /// Whether `rel` is a directory inside the root.
    #[must_use]
    pub fn is_dir(&self, rel: &str) -> bool {
        self.resolve(rel).is_ok_and(|p| p.is_dir())
    }

    /// Reads a UTF-8 file inside the root, refusing files above the size limit.
    pub fn read_text(&self, rel: &str) -> Result<String, ReadError> {
        self.read_text_limited(rel, MAX_FILE_BYTES)
    }

    /// [`Self::read_text`] with an explicit size limit (e.g. large test reports).
    pub fn read_text_limited(&self, rel: &str, limit: u64) -> Result<String, ReadError> {
        let path = self.resolve(rel)?;
        let meta = fs::metadata(&path).map_err(|e| ReadError::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(ReadError::Missing);
        }
        if meta.len() > limit {
            return Err(ReadError::TooLarge(meta.len()));
        }
        let bytes = fs::read(&path).map_err(|e| ReadError::Io(e.to_string()))?;
        String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)
    }

    /// Names of the directory entries of `rel` that are directories, split into
    /// those inside the root and those resolving outside it.
    pub fn list_dirs(&self, rel: &str) -> Result<DirListing, ReadError> {
        let dir = self.resolve(rel)?;
        let entries = fs::read_dir(&dir).map_err(|e| ReadError::Io(e.to_string()))?;
        let mut listing = DirListing::default();
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            match self.resolve(&format!("{rel}/{name}")) {
                Ok(p) if p.is_dir() => listing.inside.push(name),
                Ok(_) | Err(ReadError::Missing) => {}
                Err(_) => listing.escaping.push(name),
            }
        }
        listing.inside.sort();
        listing.escaping.sort();
        Ok(listing)
    }
}

/// Result of [`Workspace::list_dirs`].
#[derive(Debug, Default)]
pub struct DirListing {
    /// Subdirectories inside the root.
    pub inside: Vec<String>,
    /// Entries that resolve outside the root.
    pub escaping: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(MARKER)).unwrap();
        fs::write(dir.path().join("a.md"), "hello").unwrap();
        let ws = find_root(dir.path()).unwrap();
        (dir, ws)
    }

    #[test]
    fn finds_root_from_a_subdirectory() {
        let (dir, ws) = workspace();
        fs::create_dir_all(dir.path().join("x/y")).unwrap();
        let found = find_root(&dir.path().join("x/y")).unwrap();
        assert_eq!(found.root(), ws.root());
    }

    #[test]
    fn refuses_parent_and_absolute_paths() {
        let (_dir, ws) = workspace();
        assert_eq!(ws.resolve("../a.md"), Err(ReadError::Escape));
        assert_eq!(ws.resolve("/etc/passwd"), Err(ReadError::Escape));
        assert_eq!(ws.read_text("a.md").as_deref(), Ok("hello"));
        assert_eq!(ws.resolve("missing.md"), Err(ReadError::Missing));
    }
}
