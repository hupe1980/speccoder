//! Path rules for tools: normalisation, containment,
//! secrets, and the protected set the model may never write.

use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

/// A path refused, with the reason the model is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub String);

/// `a/./b/` → `a/b`; refuses absolute paths, `..`, NUL and the empty path.
pub fn normalise(rel: &str) -> Result<String, Refused> {
    if rel.contains('\0') {
        return Err(Refused("path contains NUL".to_owned()));
    }
    let mut parts = Vec::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => {
                return Err(Refused(format!(
                    "`{rel}` must be relative to the workspace and must not use `..`"
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(Refused("empty path".to_owned()));
    }
    Ok(parts.join("/"))
}

fn set(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        if let Ok(g) = Glob::new(p) {
            b.add(g);
        }
    }
    b.build().unwrap_or_else(|_| GlobSet::empty())
}

/// Files the model may neither read nor write.
const SECRETS: &[&str] = &[
    ".env",
    "**/.env",
    ".env.*",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/id_rsa*",
    "**/id_ed25519*",
    ".ssh/**",
    "**/.ssh/**",
    "**/.netrc",
    "**/.npmrc",
    "**/.pypirc",
];

/// Spec Kit intent: the spec the work is judged against.
const INTENT: &[&str] = &[".specify", ".specify/**", "specs/**", "**/.specify/**"];

/// Inputs that decide verification.
const JUDGE: &[&str] = &[
    "speccoder.toml",
    "**/speccoder.toml",
    ".github/**",
    ".gitlab-ci.yml",
    ".cargo/config.toml",
    ".cargo/config",
    "nextest.toml",
    ".config/nextest.toml",
    "pytest.ini",
    "tox.ini",
    "**/jest.config.*",
    "**/vitest.config.*",
    "deny.toml",
    "justfile",
    "Makefile",
];

/// Dependency manifests: editable only with approval.
const MANIFESTS: &[&str] = &[
    "**/Cargo.toml",
    "**/package.json",
    "**/pyproject.toml",
    "**/requirements*.txt",
    "**/go.mod",
    "**/Gemfile",
    "**/pom.xml",
    "**/build.gradle",
    "**/build.gradle.kts",
];

/// Test files by convention.
const TESTS: &[&str] = &[
    "tests/**",
    "**/tests/**",
    "test/**",
    "**/test/**",
    "**/__tests__/**",
    "**/*_test.*",
    "**/test_*.py",
    "**/*.test.*",
    "**/*.spec.*",
    "**/*Test.java",
    "**/*_spec.rb",
];

/// Whether a normalised path is a test file by convention.
#[must_use]
pub fn is_test_path(rel: &str) -> bool {
    static SET: std::sync::OnceLock<GlobSet> = std::sync::OnceLock::new();
    SET.get_or_init(|| set(TESTS)).is_match(rel)
}

/// Why a path is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Spec Kit intent.
    Intent,
    /// Verification inputs.
    Judge,
    /// Git internals.
    Git,
    /// Credentials.
    Secret,
}

impl Class {
    /// What the model is told.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Intent => {
                "it is Spec Kit intent (.specify/, specs/): the spec is not edited during implementation"
            }
            Self::Judge => {
                "it decides verification (speccoder.toml, CI and test-runner configuration)"
            }
            Self::Git => "it is git's own data",
            Self::Secret => "it looks like a credential",
        }
    }
}

/// The protected set, compiled once.
#[derive(Debug, Clone)]
pub struct Protected {
    secrets: GlobSet,
    intent: GlobSet,
    judge: GlobSet,
    manifests: GlobSet,
}

impl Default for Protected {
    fn default() -> Self {
        Self {
            secrets: set(SECRETS),
            intent: set(INTENT),
            judge: set(JUDGE),
            manifests: set(MANIFESTS),
        }
    }
}

impl Protected {
    /// Whether a normalised path is a secret (refused for reading too).
    #[must_use]
    pub fn is_secret(&self, rel: &str) -> bool {
        self.secrets.is_match(rel)
    }

    /// The protected class a normalised path belongs to, for writing.
    #[must_use]
    pub fn write_class(&self, rel: &str) -> Option<Class> {
        if rel == ".git" || rel.starts_with(".git/") || rel.contains("/.git/") {
            Some(Class::Git)
        } else if self.is_secret(rel) {
            Some(Class::Secret)
        } else if self.intent.is_match(rel) {
            Some(Class::Intent)
        } else if self.judge.is_match(rel) {
            Some(Class::Judge)
        } else {
            None
        }
    }

    /// Whether writing this path changes dependencies (needs approval).
    #[must_use]
    pub fn is_manifest(&self, rel: &str) -> bool {
        self.manifests.is_match(rel)
    }
}

/// An existing path inside `root` (symlinks resolved), for reading.
pub fn resolve_existing(root: &Path, rel: &str) -> Result<PathBuf, Refused> {
    let rel = normalise(rel)?;
    let full = root.join(&rel);
    let canonical =
        fs::canonicalize(&full).map_err(|_| Refused(format!("`{rel}` does not exist")))?;
    if canonical.starts_with(root) {
        Ok(canonical)
    } else {
        Err(Refused(format!("`{rel}` resolves outside the workspace")))
    }
}

/// A path inside `root` to write: every existing ancestor must resolve
/// inside the root, and the target itself must not be a symlink.
pub fn resolve_for_write(root: &Path, rel: &str) -> Result<PathBuf, Refused> {
    let rel = normalise(rel)?;
    let full = root.join(&rel);
    if fs::symlink_metadata(&full).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Refused(format!(
            "`{rel}` is a symlink; not written through"
        )));
    }
    let mut ancestor = full.parent();
    while let Some(a) = ancestor {
        if a.exists() {
            let canonical = fs::canonicalize(a).map_err(|e| Refused(format!("`{rel}`: {e}")))?;
            if !canonical.starts_with(root) {
                return Err(Refused(format!("`{rel}` resolves outside the workspace")));
            }
            break;
        }
        ancestor = a.parent();
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_and_refuses_escapes() {
        assert_eq!(normalise("./src//lib.rs").unwrap(), "src/lib.rs");
        assert_eq!(normalise("src/").unwrap(), "src");
        assert!(normalise("../x").is_err());
        assert!(normalise("/etc/passwd").is_err());
        assert!(normalise("a/../../b").is_err());
        assert!(normalise("").is_err());
    }

    #[test]
    fn classifies_the_protected_set() {
        let p = Protected::default();
        for (path, class) in [
            ("specs/001-x/tasks.md", Class::Intent),
            (".specify/memory/constitution.md", Class::Intent),
            ("speccoder.toml", Class::Judge),
            (".github/workflows/ci.yml", Class::Judge),
            (".config/nextest.toml", Class::Judge),
            ("web/jest.config.ts", Class::Judge),
            (".git/config", Class::Git),
            (".env", Class::Secret),
            ("deploy/.env.production", Class::Secret),
            ("certs/server.key", Class::Secret),
        ] {
            assert_eq!(p.write_class(path), Some(class), "{path}");
        }
        for free in ["src/lib.rs", "tests/api.rs", "docs/specs.md", "README.md"] {
            assert_eq!(p.write_class(free), None, "{free}");
        }
        assert!(p.is_manifest("Cargo.toml") && p.is_manifest("crates/x/Cargo.toml"));
        assert!(p.is_manifest("web/package.json") && !p.is_manifest("src/main.rs"));
        for t in [
            "tests/api.rs",
            "crates/x/tests/it.rs",
            "app/test_login.py",
            "web/a.test.ts",
            "web/__tests__/b.js",
            "pkg/x_test.go",
        ] {
            assert!(is_test_path(t), "{t}");
        }
        for f in ["src/lib.rs", "src/testing.rs", "docs/tests.md"] {
            assert!(!is_test_path(f), "{f}");
        }
    }
}
