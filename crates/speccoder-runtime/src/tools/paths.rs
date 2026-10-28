//! Path rules for tools: normalisation, containment,
//! secrets, and the protected set the model may never write.

use crate::format::Rules;
use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

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

/// A glob set matched regardless of letter case, on every OS: on a
/// case-insensitive volume (macOS and Windows by default) `Speccoder.toml`
/// and `.CLAUDE/settings.json` are the protected files, and a set that
/// folds case everywhere is the stricter answer on the others.
fn set(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        if let Ok(g) = GlobBuilder::new(p).case_insensitive(true).build() {
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
    // Agent configuration: hooks, settings, skills, commands and MCP servers
    // an agent runs or loads. Spec Kit's runtime events render extension
    // hooks into these files; a change here runs code in a later session,
    // outside speccoder's sandbox.
    ".claude/**",
    ".agents/**",
    ".codex/**",
    ".gemini/**",
    ".opencode/**",
    ".cursor/**",
    ".kiro/**",
    ".windsurf/**",
    ".roo/**",
    ".qwen/**",
    ".kilocode/**",
    ".augment/**",
    ".amazonq/**",
    ".codebuddy/**",
    ".qoder/**",
    ".mcp.json",
    "**/.mcp.json",
    "opencode.json",
    "opencode.jsonc",
];

/// Agent context files: instructions every agent session reads. Editing them
/// changes what later attempts are told, so it needs a person.
const CONTEXT: &[&str] = &[
    "AGENTS.md",
    "**/AGENTS.md",
    "CLAUDE.md",
    "**/CLAUDE.md",
    "GEMINI.md",
    "**/GEMINI.md",
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

/// The file a person's spec review is recorded in, in a unit's directory
///. The model never writes it — not while implementing, not while
/// running a planning skill: an agent must not approve its own spec.
pub const REVIEW_FILE: &str = "review.json";

/// Why a path is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The spec format's intent: the spec the work is judged against.
    Intent,
    /// Verification inputs.
    Judge,
    /// Git internals.
    Git,
    /// Credentials.
    Secret,
}

impl Class {
    /// What the model is told, for a project in the reference format
    /// ([`Protected::reason`] for the project's own).
    #[must_use]
    pub fn reason(self) -> &'static str {
        self.reason_with(crate::format::reference().rules())
    }

    fn reason_with(self, rules: &Rules) -> &'static str {
        match self {
            Self::Intent => rules.intent_reason,
            Self::Judge => {
                "it decides verification or configures an agent (speccoder.toml, CI and test-runner configuration, agent hooks, settings, skills and MCP servers)"
            }
            Self::Git => "it is git's own data",
            Self::Secret => "it looks like a credential",
        }
    }
}

/// The protected set, compiled once.
#[derive(Debug, Clone)]
pub struct Protected {
    rules: &'static Rules,
    secrets: GlobSet,
    intent: GlobSet,
    judge: GlobSet,
    context: GlobSet,
    manifests: GlobSet,
}

impl Default for Protected {
    fn default() -> Self {
        Self::new(crate::format::reference().rules())
    }
}

impl Protected {
    /// The protected set of a project whose format has `rules`.
    #[must_use]
    pub fn new(rules: &'static Rules) -> Self {
        Self {
            rules,
            secrets: set(SECRETS),
            intent: set(rules.intent),
            judge: set(JUDGE),
            context: set(CONTEXT),
            manifests: set(MANIFESTS),
        }
    }

    /// The format's rules this set was built from.
    #[must_use]
    pub fn rules(&self) -> &'static Rules {
        self.rules
    }

    /// What the model is told about a write to a path of `class`.
    #[must_use]
    pub fn reason(&self, class: Class) -> &'static str {
        class.reason_with(self.rules)
    }

    /// Whether a normalised path is a review record: an intent path named
    /// [`REVIEW_FILE`]. A planning skill writes intent; never this.
    #[must_use]
    pub fn is_review(&self, rel: &str) -> bool {
        self.intent.is_match(rel)
            && rel
                .rsplit('/')
                .next()
                .is_some_and(|f| f.eq_ignore_ascii_case(REVIEW_FILE))
    }

    /// Whether a normalised path is a secret (refused for reading too).
    #[must_use]
    pub fn is_secret(&self, rel: &str) -> bool {
        self.secrets.is_match(rel)
    }

    /// The protected class a normalised path belongs to, for writing.
    #[must_use]
    pub fn write_class(&self, rel: &str) -> Option<Class> {
        let folded = rel.to_lowercase();
        if folded == ".git"
            || folded.starts_with(".git/")
            || folded.contains("/.git/")
            || folded.ends_with("/.git")
        {
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

    /// Whether a normalised path is an agent context file (`AGENTS.md`,
    /// `CLAUDE.md`, `GEMINI.md`): writable only with a person's approval.
    #[must_use]
    pub fn is_context(&self, rel: &str) -> bool {
        self.write_class(rel).is_none() && self.context.is_match(rel)
    }

    /// Whether a normalised path is a judge or intent input: what a command
    /// must not change while an attempt runs.
    #[must_use]
    pub fn is_judged(&self, rel: &str) -> bool {
        matches!(self.write_class(rel), Some(Class::Judge | Class::Intent))
    }

    /// Whether a normalised path is a design document of the intent: an
    /// intent path that is no judge input, agent context file, manifest,
    /// credential or hidden file — what a planning task may write.
    #[must_use]
    pub fn is_design_doc(&self, rel: &str) -> bool {
        self.write_class(rel) == Some(Class::Intent)
            && !self.judge.is_match(rel)
            && !self.context.is_match(rel)
            && !self.is_manifest(rel)
            && !rel.split('/').any(|s| s.starts_with('.'))
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

/// The file a write of `rel` reaches, workspace-relative: every existing
/// ancestor's symlinks resolved (they must stay inside `root`, which is
/// canonical), the components that do not exist yet appended as written,
/// and the target itself never a symlink. Every component that exists is
/// named as the volume stores it, so on a case-insensitive volume
/// `Speccoder.toml` answers `speccoder.toml`. Two spellings that reach the
/// same file give the same answer; the same spelling after a directory was
/// swapped for a symlink gives another.
pub fn canonical_for_write(root: &Path, rel: &str) -> Result<String, Refused> {
    let rel = normalise(rel)?;
    let parts: Vec<&str> = rel.split('/').collect();
    let mut at = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        let next = at.join(part);
        let Ok(meta) = fs::symlink_metadata(&next) else {
            // Not there yet: the rest is appended as written.
            for rest in &parts[i..] {
                at.push(rest);
            }
            break;
        };
        if i + 1 == parts.len() {
            if meta.file_type().is_symlink() {
                return Err(Refused(format!(
                    "`{rel}` is a symlink; not written through"
                )));
            }
            // Not a symlink: canonicalizing only names it as stored.
            let canonical =
                fs::canonicalize(&next).map_err(|e| Refused(format!("`{rel}`: {e}")))?;
            if !canonical.starts_with(root) {
                return Err(Refused(format!("`{rel}` resolves outside the workspace")));
            }
            at = canonical;
            break;
        }
        let canonical = fs::canonicalize(&next).map_err(|e| Refused(format!("`{rel}`: {e}")))?;
        if !canonical.starts_with(root) {
            return Err(Refused(format!("`{rel}` resolves outside the workspace")));
        }
        at = canonical;
    }
    let inside = at
        .strip_prefix(root)
        .map_err(|_| Refused(format!("`{rel}` resolves outside the workspace")))?;
    let out: Vec<String> = inside
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if out.is_empty() {
        return Err(Refused("empty path".to_owned()));
    }
    Ok(out.join("/"))
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
        for agent in [
            ".claude/settings.json",
            ".claude/settings.local.json",
            ".claude/skills/x/SKILL.md",
            ".agents/skills/y/SKILL.md",
            ".codex/config.toml",
            ".kiro/hooks/a.json",
            ".mcp.json",
            "web/.mcp.json",
            "opencode.json",
        ] {
            assert_eq!(p.write_class(agent), Some(Class::Judge), "{agent}");
        }
        for context in ["AGENTS.md", "web/AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
            assert_eq!(p.write_class(context), None, "{context}");
            assert!(p.is_context(context), "{context}");
        }
        assert!(!p.is_context("README.md") && !p.is_context(".specify/memory/constitution.md"));
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

    /// Letter case never takes a path out of the protected set: on a
    /// case-insensitive volume each of these is the protected file itself.
    #[test]
    fn letter_case_does_not_leave_the_protected_set() {
        let p = Protected::default();
        for (path, class) in [
            ("Speccoder.toml", Class::Judge),
            ("SPECCODER.TOML", Class::Judge),
            ("makefile", Class::Judge),
            ("JUSTFILE", Class::Judge),
            (".CURSOR/rules.json", Class::Judge),
            (".CLAUDE/settings.json", Class::Judge),
            (".Github/workflows/ci.yml", Class::Judge),
            ("Specs/001-x/tasks.md", Class::Intent),
            (".SPECIFY/memory/constitution.md", Class::Intent),
            (".GIT/config", Class::Git),
            ("sub/.Git", Class::Git),
            (".ENV", Class::Secret),
            ("certs/server.KEY", Class::Secret),
        ] {
            assert_eq!(p.write_class(path), Some(class), "{path}");
        }
        assert!(p.is_review("specs/001-x/Review.JSON"));
        assert!(p.is_context("agents.md") && p.is_context("web/Claude.md"));
        assert!(p.is_manifest("crates/x/cargo.TOML"));
        assert!(is_test_path("Tests/api.rs"));
    }

    /// An existing file is named as the volume stores it: on a
    /// case-insensitive volume another spelling reaches the same file and
    /// gives the same answer.
    #[test]
    fn an_existing_file_is_named_as_stored() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("speccoder.toml"), "").unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        let insensitive = root.join("SPECCODER.toml").exists();
        let got = canonical_for_write(&root, "Speccoder.toml").unwrap();
        let lib = canonical_for_write(&root, "SRC/Lib.rs").unwrap();
        if insensitive {
            assert_eq!(got, "speccoder.toml");
            assert_eq!(lib, "src/lib.rs");
        } else {
            assert_eq!(got, "Speccoder.toml");
            assert_eq!(lib, "SRC/Lib.rs");
        }
    }

    #[test]
    fn a_write_reaches_its_canonical_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("src/b")).unwrap();
        fs::create_dir_all(root.join("src/a")).unwrap();
        assert_eq!(
            canonical_for_write(&root, "./src/a/x.rs").unwrap(),
            "src/a/x.rs"
        );
        // Not there yet: appended as written.
        assert_eq!(
            canonical_for_write(&root, "src/new/deep/x.rs").unwrap(),
            "src/new/deep/x.rs"
        );
        // A directory swapped for a symlink inside the root: another file.
        fs::remove_dir(root.join("src/a")).unwrap();
        std::os::unix::fs::symlink(root.join("src/b"), root.join("src/a")).unwrap();
        assert_eq!(
            canonical_for_write(&root, "src/a/x.rs").unwrap(),
            "src/b/x.rs"
        );
        // An ancestor that leaves the root, and a symlinked target, are refused.
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("out")).unwrap();
        assert!(canonical_for_write(&root, "out/x.rs").is_err());
        fs::write(root.join("src/b/real.rs"), "").unwrap();
        std::os::unix::fs::symlink(root.join("src/b/real.rs"), root.join("src/b/link.rs")).unwrap();
        assert!(canonical_for_write(&root, "src/b/link.rs").is_err());
        assert!(canonical_for_write(&root, "../x").is_err());
    }
}
