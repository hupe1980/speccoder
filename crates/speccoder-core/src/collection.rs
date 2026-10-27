//! Which test files a run actually collected: a test that never ran is
//! evidence of nothing.
//!
//! Pure: the runtime reads manifests and configuration, runs listings, and
//! hands their facts here. Per toolchain this module knows the static
//! collection rule (what proves a file is *not* collected), how a test id in
//! a JUnit report or a listing maps back to a file, the markers that make a
//! file define tests, and the one-sentence rule and fixes a person and the
//! retry read.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::verify::CheckStatus;

/// A test toolchain speccoder knows how to ask.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Toolchain {
    /// Cargo (`cargo test`, `cargo nextest`).
    Rust,
    /// pytest.
    Python,
    /// vitest or jest.
    #[serde(rename = "javascript")]
    JavaScript,
    /// `go test`.
    Go,
}

impl Toolchain {
    /// As people read it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript/TypeScript",
            Self::Go => "Go",
        }
    }
}

/// A runner's collection rule: one sentence a person reads, and where it
/// came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionRule {
    /// The toolchain.
    pub toolchain: Toolchain,
    /// The rule (``Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare)``).
    pub text: String,
    /// Where it came from (`Cargo.toml (default)`, `pyproject.toml [tool.pytest.ini_options]`).
    pub source: String,
}

/// Whether the run collected a test from a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    /// At least one test from it ran (passed or failed).
    Collected,
    /// None did.
    NotCollected,
    /// speccoder cannot tell.
    Unknown,
}

/// What decided a file's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    /// The toolchain's static rule.
    Static,
    /// The check's fresh JUnit report.
    Junit,
    /// The runner's own listing.
    Listing,
}

/// One judged test file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FileCollection {
    /// Workspace-relative path.
    pub path: String,
    /// Collected, not collected, or unknown.
    pub state: FileState,
    /// What decided it; `null` only when unknown.
    pub decided_by: Option<DecidedBy>,
    /// Tests from it that ran (skipped and ignored ones excluded).
    pub tests: usize,
    /// The first 20 of their ids.
    pub test_ids: Vec<String>,
    /// The runner's rule (not collected).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<CollectionRule>,
    /// Not collected: which part of the rule the file breaks; unknown: why
    /// speccoder cannot tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The ways out, for this toolchain and file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
}

/// A listing speccoder ran because the earlier sources left a file undecided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListingRun {
    /// The command.
    pub argv: Vec<String>,
    /// How long it took.
    pub duration_ms: u64,
    /// How it ended.
    pub status: CheckStatus,
}

/// The attempt's changed test files and whether the run collected them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Collection {
    /// Every judged file, by path.
    pub files: Vec<FileCollection>,
    /// The listings run, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub listing: Vec<ListingRun>,
}

impl Collection {
    /// The files no test ran from.
    pub fn not_collected(&self) -> impl Iterator<Item = &FileCollection> {
        self.files
            .iter()
            .filter(|f| f.state == FileState::NotCollected)
    }

    /// The files speccoder cannot tell about.
    pub fn unknown(&self) -> impl Iterator<Item = &FileCollection> {
        self.files.iter().filter(|f| f.state == FileState::Unknown)
    }

    /// The verdict reasons: one per file that never ran.
    #[must_use]
    pub fn reasons(&self) -> Vec<String> {
        self.not_collected().map(reason).collect()
    }

    /// The retry lines: one per file that never ran.
    #[must_use]
    pub fn retry_lines(&self) -> Vec<String> {
        self.not_collected().map(retry_line).collect()
    }

    /// The sorted paths that never ran (the retry fingerprint's part).
    #[must_use]
    pub fn uncollected_paths(&self) -> Vec<String> {
        let mut p: Vec<String> = self.not_collected().map(|f| f.path.clone()).collect();
        p.sort();
        p
    }
}

/// The prefix of every verdict reason a never-run file gives.
pub const REASON_PREFIX: &str = "tests not collected: ";

/// ``tests not collected: `<path>` — <rule>; <why>``.
#[must_use]
pub fn reason(f: &FileCollection) -> String {
    let rule = f.rule.as_ref().map_or("", |r| r.text.as_str());
    match f.why.as_deref() {
        Some(why) => format!("{REASON_PREFIX}`{}` — {rule}; {why}", f.path),
        None => format!("{REASON_PREFIX}`{}` — {rule}", f.path),
    }
}

/// `` `<path>` never ran: <short rule>. <Fixes>.`` — the retry digest's line.
#[must_use]
pub fn retry_line(f: &FileCollection) -> String {
    let short = f.rule.as_ref().map_or_else(String::new, short_rule);
    let fixes = capitalise(&f.fixes.join(", or "));
    let mut line = format!("`{}` never ran", f.path);
    if !short.is_empty() {
        line.push_str(": ");
        line.push_str(&short);
    }
    line.push('.');
    if !fixes.is_empty() {
        line.push(' ');
        line.push_str(&fixes);
        line.push('.');
    }
    line
}

fn short_rule(r: &CollectionRule) -> String {
    r.text
        .split(" (and the modules")
        .next()
        .unwrap_or(&r.text)
        .to_owned()
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The toolchain a file's extension belongs to.
#[must_use]
pub fn toolchain_of(path: &str) -> Option<Toolchain> {
    let ext = path.rsplit_once('.').map(|(_, e)| e)?;
    match ext {
        "rs" => Some(Toolchain::Rust),
        "py" => Some(Toolchain::Python),
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" => Some(Toolchain::JavaScript),
        "go" => Some(Toolchain::Go),
        _ => None,
    }
}

/// Directories nothing judges: build output and dependencies.
#[must_use]
pub fn is_build_or_dependency(path: &str) -> bool {
    path.split('/').any(|c| {
        matches!(
            c,
            "target"
                | "node_modules"
                | "vendor"
                | ".venv"
                | "venv"
                | "dist"
                | "build"
                | "__pycache__"
        )
    })
}

/// Whether `source` defines at least one test by the toolchain's markers:
/// a helper without test definitions is never judged.
#[must_use]
pub fn defines_tests(toolchain: Toolchain, source: &str) -> bool {
    match toolchain {
        Toolchain::Rust => {
            let code: String = strip_comments(source).split_whitespace().collect();
            // `#[test]`, and any `#[<crate>::test]` / `#[<crate>::test(…)]`
            // attribute (tokio, async-std, test-log, sqlx, googletest, …).
            code.contains("#[test]")
                || ["#[rstest", "#[test_case", "#[quickcheck", "#[proptest"]
                    .iter()
                    .any(|m| code.contains(m))
                || code.match_indices("#[").any(|(i, _)| {
                    let end = code[i..].find(']').map_or(code.len(), |e| i + e + 1);
                    let attr = &code[i..end];
                    attr.contains("::test]") || attr.contains("::test(")
                })
        }
        Toolchain::Python => source.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("def test")
                || t.starts_with("async def test")
                || t.starts_with("class Test")
        }),
        Toolchain::JavaScript => {
            let code = strip_comments(source);
            [
                "it(",
                "test(",
                "describe(",
                "it.each",
                "test.each",
                "it.only(",
                "test.only(",
            ]
            .iter()
            .any(|m| {
                code.match_indices(m).any(|(i, _)| {
                    i == 0
                        || !code[..i]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
                })
            })
        }
        Toolchain::Go => !go_test_like(source).is_empty(),
    }
}

/// How many tests `source` declares by the toolchain's markers (a lower
/// bound for what a run of it must collect).
#[must_use]
pub fn declared_tests(toolchain: Toolchain, source: &str) -> usize {
    let code = strip_comments(source);
    match toolchain {
        Toolchain::Rust => {
            let flat: String = code.split_whitespace().collect();
            flat.match_indices("#[")
                .filter(|(i, _)| {
                    let end = flat[*i..].find(']').map_or(flat.len(), |e| i + e + 1);
                    let attr = &flat[*i..end];
                    attr == "#[test]"
                        || attr.contains("::test]")
                        || attr.contains("::test(")
                        || attr.starts_with("#[rstest")
                        || attr.starts_with("#[test_case")
                })
                .count()
        }
        Toolchain::Python => code
            .lines()
            .map(str::trim_start)
            .filter(|l| l.starts_with("def test") || l.starts_with("async def test"))
            .count(),
        Toolchain::JavaScript => ["it(", "test("]
            .iter()
            .map(|m| {
                code.match_indices(m)
                    .filter(|(i, _)| {
                        *i == 0
                            || !code[..*i]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
                    })
                    .count()
            })
            .sum(),
        Toolchain::Go => go_declared_tests(source).len(),
    }
}

/// Strips `//` and `/* */` comments, keeping line breaks (strings are left
/// as they are: a comment marker inside one is rare and only hides text).
#[must_use]
pub fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b = src.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
        } else if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if b[i] == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
        } else {
            // Keep multi-byte characters whole.
            let ch = src[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Joins `dir` and `rel` and normalises `.`/`..`; `None` when the result
/// leaves the workspace root.
#[must_use]
pub fn join(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for c in dir.split('/').chain(rel.split('/')) {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, f)| f)
}

fn stem(path: &str) -> &str {
    let f = file_name(path);
    f.split_once('.').map_or(f, |(s, _)| s)
}

// ---------------------------------------------------------------- Rust

/// One Cargo package, as its manifest says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RustPackage {
    /// The package's directory, workspace-relative (`""` for the root).
    pub root: String,
    /// `[package] name`.
    pub name: String,
    /// `autotests` (default `true`).
    pub autotests: bool,
    /// `[[test]]` targets: name and path (package-relative).
    pub tests: Vec<(String, String)>,
    /// `[lib] path`, when set.
    pub lib: Option<String>,
    /// `[[bin]]` targets with a `path`: name and path (package-relative).
    pub bins: Vec<(String, String)>,
}

/// A compiled test binary and its crate root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustTarget {
    /// nextest's binary id: `greet`, `greet::greet_name`, `greet::bin/cli`.
    pub binary_id: String,
    /// The crate root, workspace-relative.
    pub root_file: String,
}

/// The test-carrying targets of `pkg`, from its manifest and the files
/// present (`files`: workspace-relative paths).
#[must_use]
pub fn rust_targets(pkg: &RustPackage, files: &BTreeSet<String>) -> Vec<RustTarget> {
    let at = |rel: &str| join(&pkg.root, rel).unwrap_or_default();
    let mut out = Vec::new();
    let lib = at(pkg.lib.as_deref().unwrap_or("src/lib.rs"));
    if files.contains(&lib) {
        out.push(RustTarget {
            binary_id: pkg.name.clone(),
            root_file: lib,
        });
    }
    let main = at("src/main.rs");
    if files.contains(&main) {
        out.push(RustTarget {
            binary_id: format!("{}::bin/{}", pkg.name, pkg.name),
            root_file: main,
        });
    }
    for (name, path) in &pkg.bins {
        let root_file = at(path);
        if files.contains(&root_file) && !out.iter().any(|t| t.root_file == root_file) {
            out.push(RustTarget {
                binary_id: format!("{}::bin/{name}", pkg.name),
                root_file,
            });
        }
    }
    let bin_dir = at("src/bin");
    for f in files {
        let Some(rest) = f.strip_prefix(&format!("{bin_dir}/")) else {
            continue;
        };
        let name = match rest.split('/').collect::<Vec<_>>()[..] {
            [one] if one.ends_with(".rs") => one.trim_end_matches(".rs"),
            [dir, "main.rs"] => dir,
            _ => continue,
        };
        out.push(RustTarget {
            binary_id: format!("{}::bin/{name}", pkg.name),
            root_file: f.clone(),
        });
    }
    let mut explicit: BTreeSet<String> = BTreeSet::new();
    for (name, path) in &pkg.tests {
        let root_file = at(path);
        explicit.insert(root_file.clone());
        out.push(RustTarget {
            binary_id: format!("{}::{name}", pkg.name),
            root_file,
        });
    }
    if pkg.autotests {
        let tests_dir = at("tests");
        for f in files {
            let Some(rest) = f.strip_prefix(&format!("{tests_dir}/")) else {
                continue;
            };
            let name = match rest.split('/').collect::<Vec<_>>()[..] {
                [one] if one.ends_with(".rs") => one.trim_end_matches(".rs"),
                [dir, "main.rs"] => dir,
                _ => continue,
            };
            if explicit.contains(f) || pkg.tests.iter().any(|(n, _)| n == name) {
                continue;
            }
            out.push(RustTarget {
                binary_id: format!("{}::{name}", pkg.name),
                root_file: f.clone(),
            });
        }
    }
    out
}

/// A `mod` declaration found in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDecl {
    /// The module's name.
    pub name: String,
    /// Its `#[path = "…"]`, when set.
    pub path: Option<String>,
    /// The inline modules (`mod a { … }`) it sits in, outermost first, each
    /// with its own `#[path]` when set.
    pub inline: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
}

fn lex(src: &str) -> Vec<Tok> {
    let code = strip_comments(src);
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[s..i].iter().collect();
            // A raw string: r"…" or r#"…"#.
            if word == "r" && i < chars.len() && (chars[i] == '"' || chars[i] == '#') {
                let mut hashes = 0;
                while i < chars.len() && chars[i] == '#' {
                    hashes += 1;
                    i += 1;
                }
                if i < chars.len() && chars[i] == '"' {
                    i += 1;
                    let s = i;
                    loop {
                        if i >= chars.len() {
                            break;
                        }
                        if chars[i] == '"'
                            && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#'))
                        {
                            break;
                        }
                        i += 1;
                    }
                    out.push(Tok::Str(chars[s..i.min(chars.len())].iter().collect()));
                    i += 1 + hashes;
                }
                continue;
            }
            out.push(Tok::Ident(word));
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    s.push(chars[i + 1]);
                    i += 2;
                } else {
                    s.push(chars[i]);
                    i += 1;
                }
            }
            out.push(Tok::Str(s));
            i += 1;
        } else if c == '\'' {
            // A char literal ('x', '\n') or a lifetime ('a).
            if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else if chars.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else {
                i += 1;
            }
        } else {
            out.push(Tok::Punct(c));
            i += 1;
        }
    }
    out
}

/// The `mod` declarations of a Rust source file (comments ignored), file
/// modules (`mod x;`) only, each with the inline modules around it.
#[must_use]
pub fn rust_mod_decls(src: &str) -> Vec<ModDecl> {
    let toks = lex(src);
    let mut out = Vec::new();
    // Inline modules open: (name, path, brace depth inside it).
    let mut stack: Vec<(String, Option<String>, usize)> = Vec::new();
    let mut depth = 0usize;
    let mut pending_path: Option<String> = None;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Punct('#') if toks.get(i + 1) == Some(&Tok::Punct('[')) => {
                // An attribute: #[path = "…"] or anything else, skipped.
                let mut j = i + 2;
                let mut nest = 1;
                let is_path = toks.get(j) == Some(&Tok::Ident("path".to_owned()))
                    && toks.get(j + 1) == Some(&Tok::Punct('='));
                if is_path && let Some(Tok::Str(p)) = toks.get(j + 2) {
                    pending_path = Some(p.clone());
                }
                while j < toks.len() && nest > 0 {
                    match toks[j] {
                        Tok::Punct('[') => nest += 1,
                        Tok::Punct(']') => nest -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            Tok::Ident(w) if w == "mod" => {
                if let Some(Tok::Ident(name)) = toks.get(i + 1) {
                    match toks.get(i + 2) {
                        Some(Tok::Punct(';')) => {
                            out.push(ModDecl {
                                name: name.clone(),
                                path: pending_path.take(),
                                inline: stack
                                    .iter()
                                    .map(|(n, p, _)| (n.clone(), p.clone()))
                                    .collect(),
                            });
                            i += 3;
                            continue;
                        }
                        Some(Tok::Punct('{')) => {
                            depth += 1;
                            stack.push((name.clone(), pending_path.take(), depth));
                            i += 3;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            Tok::Ident(w) if matches!(w.as_str(), "pub" | "crate" | "super" | "self" | "in") => {}
            Tok::Punct('(' | ')') => {}
            Tok::Punct('{') => {
                depth += 1;
                pending_path = None;
            }
            Tok::Punct('}') => {
                if stack.last().is_some_and(|(_, _, d)| *d == depth) {
                    stack.pop();
                }
                depth = depth.saturating_sub(1);
                pending_path = None;
            }
            _ => pending_path = None,
        }
        i += 1;
    }
    out
}

/// Where each file reachable from `targets` sits: the targets' binary ids
/// with the file's module path (`""` for a crate root). `read` gives a
/// file's text (workspace-relative path); a file it cannot read ends the
/// walk there. `#[path]` is followed; a path leaving the root is ignored.
#[must_use]
pub fn rust_reachable(
    targets: &[RustTarget],
    read: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for t in targets {
        // (file, module path, mod-rs-like: children live beside it)
        let mut queue: Vec<(String, String, bool)> =
            vec![(t.root_file.clone(), String::new(), true)];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some((file, module, mod_rs)) = queue.pop() {
            if !seen.insert(file.clone()) {
                continue;
            }
            let Some(src) = read(&file) else { continue };
            out.entry(file.clone())
                .or_default()
                .push((t.binary_id.clone(), module.clone()));
            let dir = parent(&file).to_owned();
            // A non-mod-rs file `a/x.rs` holds its children in `a/x/`.
            let base = if mod_rs {
                dir.clone()
            } else {
                join(&dir, stem(&file)).unwrap_or_default()
            };
            for d in rust_mod_decls(&src) {
                let mut inline_dir = base.clone();
                let mut module_path: Vec<String> = module
                    .split("::")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                for (n, p) in &d.inline {
                    inline_dir = join(&inline_dir, p.as_deref().unwrap_or(n)).unwrap_or_default();
                    module_path.push(n.clone());
                }
                module_path.push(d.name.clone());
                let child_module = module_path.join("::");
                let candidates: Vec<(Option<String>, bool)> = match &d.path {
                    // Outside inline blocks, #[path] is relative to the file's directory.
                    Some(p) if d.inline.is_empty() => vec![(join(&dir, p), true)],
                    Some(p) => vec![(join(&inline_dir, p), true)],
                    None => vec![
                        (join(&inline_dir, &format!("{}.rs", d.name)), false),
                        (join(&inline_dir, &format!("{}/mod.rs", d.name)), true),
                    ],
                };
                for (c, rs) in candidates {
                    let Some(c) = c else { continue };
                    if read(&c).is_some() {
                        queue.push((c, child_module.clone(), rs));
                        break;
                    }
                }
            }
        }
    }
    out
}

/// The Rust rule sentence.
pub const RUST_RULE: &str =
    "Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare)";

/// The Rust rule as configured: `autotests = false` leaves only `[[test]]` targets.
#[must_use]
pub fn rust_rule(pkg: &RustPackage) -> CollectionRule {
    let manifest = join(&pkg.root, "Cargo.toml").unwrap_or_else(|| "Cargo.toml".to_owned());
    if pkg.autotests {
        CollectionRule {
            toolchain: Toolchain::Rust,
            text: RUST_RULE.to_owned(),
            source: if pkg.tests.is_empty() {
                format!("{manifest} (default)")
            } else {
                format!("{manifest} [[test]]")
            },
        }
    } else {
        let listed: Vec<String> = pkg.tests.iter().map(|(_, p)| format!("`{p}`")).collect();
        CollectionRule {
            toolchain: Toolchain::Rust,
            text: if listed.is_empty() {
                "Cargo builds no integration test (`autotests = false` and no `[[test]]`), only inline test modules (and the modules they declare)".to_owned()
            } else {
                format!(
                    "Cargo builds only the `[[test]]` targets {} (`autotests = false`) and inline test modules (and the modules they declare)",
                    listed.join(", ")
                )
            },
            source: format!("{manifest} autotests"),
        }
    }
}

/// The ways out for an unreachable Rust file.
#[must_use]
pub fn rust_fixes(pkg_root: &str, path: &str) -> Vec<String> {
    let s = stem(path);
    let tests = join(pkg_root, "tests").unwrap_or_else(|| "tests".to_owned());
    match path.strip_prefix(&format!("{tests}/")) {
        Some(rest) if rest.contains('/') => {
            let dir = rest.split('/').next().unwrap_or_default();
            vec![
                format!("move the tests to `{tests}/{s}.rs`"),
                format!("add `{tests}/{dir}/main.rs` with `mod {s};`"),
            ]
        }
        _ => vec![
            format!("declare it with `mod {s};` from its parent module"),
            format!("move the tests to `{tests}/{s}.rs`"),
        ],
    }
}

/// Which file each test id belongs to: the longest binary id that prefixes
/// it, then the longest module path of a file of that binary. `files`:
/// path → (binary id, module path) as [`rust_reachable`] gives them.
#[must_use]
pub fn rust_tests_by_file(
    files: &BTreeMap<String, Vec<(String, String)>>,
    ids: &[String],
) -> BTreeMap<String, Vec<String>> {
    let mut by_binary: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (path, places) in files {
        for (b, m) in places {
            by_binary
                .entry(b.as_str())
                .or_default()
                .push((m.as_str(), path.as_str()));
        }
    }
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in ids {
        let Some((binary, rest)) = by_binary
            .keys()
            .filter_map(|b| {
                id.strip_prefix(*b)
                    .and_then(|r| r.strip_prefix("::"))
                    .map(|r| (*b, r))
            })
            .max_by_key(|(b, _)| b.len())
        else {
            continue;
        };
        let owner = by_binary[binary]
            .iter()
            .filter(|(m, _)| m.is_empty() || rest.starts_with(&format!("{m}::")))
            .max_by_key(|(m, _)| m.len());
        if let Some((_, path)) = owner {
            out.entry((*path).to_owned()).or_default().push(id.clone());
        }
    }
    out
}

// ---------------------------------------------------------------- globs

/// Whether `path` matches a glob: `**`, `*`, `?`, `[…]`, `{a,b}` and the
/// extglobs `?(…)`, `@(…)`, `+(…)`, `*(…)` (vitest and jest defaults use them).
#[must_use]
pub fn glob_match(pattern: &str, path: &str) -> bool {
    expand(pattern).iter().any(|p| {
        match_segments(
            &p.split('/').collect::<Vec<_>>(),
            &path.split('/').collect::<Vec<_>>(),
        )
    })
}

/// Brace and extglob alternatives expanded to plain globs.
fn expand(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    // The first group: `{` or an extglob `X(`.
    let mut i = 0;
    while i < chars.len() {
        let ext = matches!(chars[i], '?' | '@' | '+' | '*' | '!') && chars.get(i + 1) == Some(&'(');
        if chars[i] == '{' || ext {
            let open = if ext { i + 1 } else { i };
            let close_ch = if ext { ')' } else { '}' };
            let open_ch = chars[open];
            let mut depth = 0;
            let mut j = open;
            let mut parts = Vec::new();
            let mut start = open + 1;
            while j < chars.len() {
                let c = chars[j];
                if c == open_ch || (c == '{' && open_ch != '{') || (c == '(' && open_ch != '(') {
                    depth += 1;
                } else if c == close_ch
                    || (c == '}' && close_ch != '}')
                    || (c == ')' && close_ch != ')')
                {
                    depth -= 1;
                    if depth == 0 {
                        parts.push(chars[start..j].iter().collect::<String>());
                        break;
                    }
                } else if depth == 1 && (c == ',' && !ext || c == '|' && ext) {
                    parts.push(chars[start..j].iter().collect::<String>());
                    start = j + 1;
                }
                j += 1;
            }
            if j >= chars.len() {
                break;
            }
            let head: String = chars[..i].iter().collect();
            let tail: String = chars[j + 1..].iter().collect();
            let mut alts = parts;
            if ext && matches!(chars[i], '?' | '*') {
                alts.insert(0, String::new());
            }
            if ext && chars[i] == '!' {
                // A negation is not expanded: treat as any.
                alts = vec!["*".to_owned()];
            }
            return alts
                .into_iter()
                .flat_map(|a| expand(&format!("{head}{a}{tail}")))
                .collect();
        }
        i += 1;
    }
    vec![pattern.to_owned()]
}

fn match_segments(p: &[&str], s: &[&str]) -> bool {
    match (p.first(), s.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            match_segments(&p[1..], s) || (!s.is_empty() && match_segments(p, &s[1..]))
        }
        (Some(ps), Some(ss)) => match_name(ps, ss) && match_segments(&p[1..], &s[1..]),
        _ => false,
    }
}

/// fnmatch over one path segment: `*`, `?`, `[…]`.
#[must_use]
pub fn match_name(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    fn go(p: &[char], n: &[char]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some('*') => (0..=n.len()).any(|k| go(&p[1..], &n[k..])),
            Some('?') => !n.is_empty() && go(&p[1..], &n[1..]),
            Some('[') => {
                let Some(end) = p.iter().position(|c| *c == ']') else {
                    return n.first() == Some(&'[') && go(&p[1..], &n[1..]);
                };
                let Some(c) = n.first() else { return false };
                let class = &p[1..end];
                let (neg, class) = match class.first() {
                    Some('!' | '^') => (true, &class[1..]),
                    _ => (false, class),
                };
                let mut hit = false;
                let mut k = 0;
                while k < class.len() {
                    if k + 2 < class.len() && class[k + 1] == '-' {
                        hit |= class[k] <= *c && *c <= class[k + 2];
                        k += 3;
                    } else {
                        hit |= class[k] == *c;
                        k += 1;
                    }
                }
                hit != neg && go(&p[end + 1..], &n[1..])
            }
            Some(c) => n.first() == Some(c) && go(&p[1..], &n[1..]),
        }
    }
    go(&p, &n)
}

// ---------------------------------------------------------------- Python

/// pytest's collection settings, as configured or by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PytestRule {
    /// `python_files` (default `test_*.py`, `*_test.py`).
    pub python_files: Vec<String>,
    /// `testpaths` (default: none — the rootdir).
    pub testpaths: Vec<String>,
    /// `norecursedirs` (default pytest's).
    pub norecursedirs: Vec<String>,
    /// Where the settings came from.
    pub source: String,
}

impl Default for PytestRule {
    fn default() -> Self {
        Self {
            python_files: vec!["test_*.py".to_owned(), "*_test.py".to_owned()],
            testpaths: Vec::new(),
            norecursedirs: [
                "*.egg",
                ".*",
                "_darcs",
                "build",
                "CVS",
                "dist",
                "node_modules",
                "venv",
                "{arch}",
            ]
            .map(str::to_owned)
            .to_vec(),
            source: "pytest defaults".to_owned(),
        }
    }
}

impl PytestRule {
    /// The rule sentence.
    #[must_use]
    pub fn rule(&self) -> CollectionRule {
        let files: Vec<String> = self.python_files.iter().map(|p| format!("`{p}`")).collect();
        let under = if self.testpaths.is_empty() {
            "the rootdir".to_owned()
        } else {
            self.testpaths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        CollectionRule {
            toolchain: Toolchain::Python,
            text: format!(
                "pytest collects {} (python_files) under {} (testpaths)",
                files.join(" and "),
                under
            ),
            source: self.source.clone(),
        }
    }

    /// Why `path` is never collected by the static rule, if it is not.
    #[must_use]
    pub fn breaks(&self, path: &str) -> Option<String> {
        let name = file_name(path);
        if !self.python_files.iter().any(|p| match_name(p, name)) {
            return Some("this file matches neither".to_owned());
        }
        if !self.testpaths.is_empty()
            && !self.testpaths.iter().any(|t| {
                let t = t.trim_end_matches('/');
                t == "." || path.starts_with(&format!("{t}/"))
            })
        {
            return Some("this file is outside testpaths".to_owned());
        }
        let dir = parent(path);
        if let Some(d) = dir
            .split('/')
            .filter(|d| !d.is_empty())
            .find(|d| self.norecursedirs.iter().any(|p| match_name(p, d)))
        {
            return Some(format!("it is under `{d}/`, which norecursedirs skips"));
        }
        None
    }

    /// The ways out.
    #[must_use]
    pub fn fixes(&self, path: &str) -> Vec<String> {
        let s = stem(path);
        let s = s.strip_prefix("test_").unwrap_or(s);
        let mut out = vec![format!("rename to `test_{s}.py`")];
        if let Some(t) = self.testpaths.first() {
            out.push(format!(
                "move under `{}/` (testpaths)",
                t.trim_end_matches('/')
            ));
        }
        out
    }
}

/// A pytest id's file: `tests/test_x.py::test_a` (listing) or the JUnit
/// `classname::name` form `tests.test_x::test_a` / `tests.test_x.TestC::t`.
#[must_use]
pub fn python_id_matches(path: &str, id: &str) -> bool {
    if id.starts_with(&format!("{path}::")) || id == path {
        return true;
    }
    let dotted = path.trim_end_matches(".py").replace('/', ".");
    id.starts_with(&format!("{dotted}::")) || id.starts_with(&format!("{dotted}."))
}

// ---------------------------------------------------------------- JS / TS

/// The JS runners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsRunner {
    /// vitest.
    Vitest,
    /// jest.
    Jest,
}

impl JsRunner {
    /// Its command name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::Jest => "jest",
        }
    }

    /// The setting that selects test files.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Vitest => "include",
            Self::Jest => "testMatch",
        }
    }

    /// Its default patterns.
    #[must_use]
    pub fn defaults(self) -> Vec<String> {
        match self {
            Self::Vitest => vec!["**/*.{test,spec}.?(c|m)[jt]s?(x)".to_owned()],
            Self::Jest => vec![
                "**/__tests__/**/*.?([mc])[jt]s?(x)".to_owned(),
                "**/?(*.)+(spec|test).?([mc])[jt]s?(x)".to_owned(),
            ],
        }
    }
}

/// A JS runner's include rule, when it is known statically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsRule {
    /// Which runner.
    pub runner: JsRunner,
    /// Include patterns.
    pub include: Vec<String>,
    /// Exclude patterns (`node_modules` always).
    pub exclude: Vec<String>,
    /// Where they came from.
    pub source: String,
}

impl JsRule {
    /// The rule sentence.
    #[must_use]
    pub fn rule(&self) -> CollectionRule {
        let pats: Vec<String> = self.include.iter().map(|p| format!("`{p}`")).collect();
        CollectionRule {
            toolchain: Toolchain::JavaScript,
            text: format!(
                "{} collects files matching {} ({})",
                self.runner.name(),
                pats.join(" or "),
                self.runner.key()
            ),
            source: self.source.clone(),
        }
    }

    /// Why `path` (relative to the runner's root) is never collected, if it is not.
    #[must_use]
    pub fn breaks(&self, path: &str) -> Option<String> {
        if path.split('/').any(|c| c == "node_modules")
            || self.exclude.iter().any(|p| glob_match(p, path))
        {
            return Some("this file is excluded".to_owned());
        }
        (!self.include.iter().any(|p| glob_match(p, path)))
            .then(|| "this file does not match".to_owned())
    }

    /// The ways out.
    #[must_use]
    pub fn fixes(&self, path: &str) -> Vec<String> {
        let name = file_name(path);
        let (s, ext) = name.split_once('.').unwrap_or((name, "ts"));
        let ext = ext.rsplit('.').next().unwrap_or("ts");
        vec![format!(
            "rename to `{s}.test.{ext}` ({}: `{}`)",
            self.runner.key(),
            self.include.first().map_or("", String::as_str)
        )]
    }
}

// ---------------------------------------------------------------- Go

/// The Go rule sentence.
pub const GO_RULE: &str = "go test runs `*_test.go` files in a package directory (not `testdata/` or a `_`/`.`-prefixed directory) whose functions are named `Test…`, `Example…` or `Fuzz…` followed by a non-lowercase character";

/// The Go rule.
#[must_use]
pub fn go_rule() -> CollectionRule {
    CollectionRule {
        toolchain: Toolchain::Go,
        text: GO_RULE.to_owned(),
        source: "go test (default)".to_owned(),
    }
}

/// Functions that look like tests (`func Test…`/`func test…`, …): the
/// markers that make a file judged.
fn go_test_like(source: &str) -> Vec<String> {
    let code = strip_comments(source);
    code.lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("func ")?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let lower = name.to_ascii_lowercase();
            (["test", "example", "fuzz"]
                .iter()
                .any(|p| lower.starts_with(p))
                && rest[name.len()..].trim_start().starts_with('('))
            .then_some(name)
        })
        .collect()
}

/// The test functions `go test` runs from a file: `Test`, `Example`,
/// `Fuzz` followed by end or a non-lowercase character.
#[must_use]
pub fn go_declared_tests(source: &str) -> Vec<String> {
    go_test_like(source)
        .into_iter()
        .filter(|n| {
            ["Test", "Example", "Fuzz"].iter().any(|p| {
                n.strip_prefix(p)
                    .is_some_and(|r| r.chars().next().is_none_or(|c| !c.is_lowercase()))
            })
        })
        .collect()
}

/// The file's `//go:build` expression, when it has one.
#[must_use]
pub fn go_build_constraint(source: &str) -> Option<String> {
    source
        .lines()
        .take_while(|l| !l.trim_start().starts_with("package "))
        .find_map(|l| {
            l.trim()
                .strip_prefix("//go:build ")
                .map(|e| e.trim().to_owned())
        })
}

/// Evaluates a `//go:build` expression with `tags` set (plus the OS,
/// architecture, `unix` and release tags `go1.N`, which are always on).
#[must_use]
pub fn go_build_satisfied(expr: &str, tags: &BTreeSet<String>) -> bool {
    let toks: Vec<String> = {
        let mut t = Vec::new();
        let mut cur = String::new();
        for c in expr.chars() {
            match c {
                '(' | ')' | '!' => {
                    if !cur.is_empty() {
                        t.push(std::mem::take(&mut cur));
                    }
                    t.push(c.to_string());
                }
                '&' | '|' => {
                    if !cur.is_empty() && !cur.starts_with(c) {
                        t.push(std::mem::take(&mut cur));
                    }
                    cur.push(c);
                    if cur.len() == 2 {
                        t.push(std::mem::take(&mut cur));
                    }
                }
                c if c.is_whitespace() => {
                    if !cur.is_empty() {
                        t.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() {
            t.push(cur);
        }
        t
    };
    let on = |tag: &str| {
        tags.contains(tag)
            || tag.starts_with("go1.")
            || matches!(
                tag,
                "unix" | "linux" | "darwin" | "amd64" | "arm64" | "gc" | "cgo"
            )
    };
    fn or(t: &[String], i: &mut usize, on: &dyn Fn(&str) -> bool) -> bool {
        let mut v = and(t, i, on);
        while t.get(*i).is_some_and(|x| x == "||") {
            *i += 1;
            v |= and(t, i, on);
        }
        v
    }
    fn and(t: &[String], i: &mut usize, on: &dyn Fn(&str) -> bool) -> bool {
        let mut v = not(t, i, on);
        while t.get(*i).is_some_and(|x| x == "&&") {
            *i += 1;
            v &= not(t, i, on);
        }
        v
    }
    fn not(t: &[String], i: &mut usize, on: &dyn Fn(&str) -> bool) -> bool {
        match t.get(*i).map(String::as_str) {
            Some("!") => {
                *i += 1;
                !not(t, i, on)
            }
            Some("(") => {
                *i += 1;
                let v = or(t, i, on);
                *i += 1;
                v
            }
            Some(x) => {
                *i += 1;
                on(x)
            }
            None => false,
        }
    }
    let mut i = 0;
    or(&toks, &mut i, &on)
}

/// Why a Go test file is never run by the static rule, if it is not, with
/// the fixes. `tags`: the build tags the check sets.
#[must_use]
pub fn go_breaks(
    path: &str,
    source: &str,
    tags: &BTreeSet<String>,
) -> Option<(String, Vec<String>)> {
    let s = stem(path);
    if let Some(d) = parent(path)
        .split('/')
        .find(|d| *d == "testdata" || d.starts_with('_') || d.starts_with('.'))
    {
        return Some((
            format!("it is under `{d}/`, which go ignores"),
            vec![format!("move it out of `{d}/`")],
        ));
    }
    if !file_name(path).ends_with("_test.go") {
        return Some((
            "its name does not end in `_test.go`".to_owned(),
            vec![format!("rename to `{s}_test.go`")],
        ));
    }
    if let Some(expr) = go_build_constraint(source)
        && !go_build_satisfied(&expr, tags)
    {
        return Some((
            format!("its build constraint `{expr}` is not set by the check"),
            vec![format!(
                "the file needs build tag `{expr}`, which the check does not set: add `-tags` to the check, or drop the constraint"
            )],
        ));
    }
    let like = go_test_like(source);
    if go_declared_tests(source).is_empty() && !like.is_empty() {
        return Some((
            format!(
                "no function is named `Test[A-Z_]…` ({})",
                like.iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            vec!["rename the functions to `Test<Name>` (capital after `Test`)".to_owned()],
        ));
    }
    None
}

/// A Go test id's match: JUnit `importpath::TestX` or a listing name, for
/// a file whose package is `import_path` and that declares `names`.
#[must_use]
pub fn go_id_matches(import_path: &str, names: &[String], id: &str) -> bool {
    let (pkg, name) = id.rsplit_once("::").unwrap_or(("", id));
    let base = name.split('/').next().unwrap_or(name);
    (pkg.is_empty() || pkg == import_path) && names.iter().any(|n| n == base)
}

// ---------------------------------------------------------------- decision

/// A decided file, built from parts.
#[must_use]
pub fn collected(path: &str, by: DecidedBy, ids: &[String]) -> FileCollection {
    FileCollection {
        path: path.to_owned(),
        state: FileState::Collected,
        decided_by: Some(by),
        tests: ids.len(),
        test_ids: ids.iter().take(20).cloned().collect(),
        rule: None,
        why: None,
        fixes: Vec::new(),
    }
}

/// A file no test ran from.
#[must_use]
pub fn not_collected(
    path: &str,
    by: DecidedBy,
    rule: CollectionRule,
    why: &str,
    fixes: Vec<String>,
) -> FileCollection {
    FileCollection {
        path: path.to_owned(),
        state: FileState::NotCollected,
        decided_by: Some(by),
        tests: 0,
        test_ids: Vec::new(),
        rule: Some(rule),
        why: Some(why.to_owned()),
        fixes,
    }
}

/// A file speccoder cannot tell about.
#[must_use]
pub fn unknown(path: &str, why: &str) -> FileCollection {
    FileCollection {
        path: path.to_owned(),
        state: FileState::Unknown,
        decided_by: None,
        tests: 0,
        test_ids: Vec::new(),
        rule: None,
        why: Some(why.to_owned()),
        fixes: Vec::new(),
    }
}

/// The reason for a file reachable by the rule whose tests did not run.
pub const COMPILED_OUT: &str =
    "the runner collected no test from it (its tests are compiled out or ignored)";

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn set(files: &[&str]) -> BTreeSet<String> {
        files.iter().map(|s| (*s).to_owned()).collect()
    }

    fn pkg() -> RustPackage {
        RustPackage {
            root: String::new(),
            name: "greet".to_owned(),
            autotests: true,
            ..RustPackage::default()
        }
    }

    fn reach(files: &[(&str, &str)]) -> BTreeMap<String, Vec<(String, String)>> {
        let map: BTreeMap<String, String> = files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
            .collect();
        let names: BTreeSet<String> = map.keys().cloned().collect();
        let targets = rust_targets(&pkg(), &names);
        rust_reachable(&targets, &|p| map.get(p).cloned())
    }

    #[test]
    fn the_poc_files_are_unreachable_and_tests_rs_files_are_targets() {
        let r = reach(&[
            (
                "src/lib.rs",
                "pub fn greet() {}\n#[cfg(test)]\nmod tests { #[test] fn it_works() {} }\n",
            ),
            ("tests/unit/greet_name.rs", "#[test] fn a() {}\n"),
            ("tests/greet_name.rs", "#[test] fn a() {}\n"),
        ]);
        assert!(!r.contains_key("tests/unit/greet_name.rs"));
        assert_eq!(
            r["tests/greet_name.rs"],
            [("greet::greet_name".to_owned(), String::new())]
        );
        assert_eq!(r["src/lib.rs"], [("greet".to_owned(), String::new())]);
    }

    #[test]
    fn a_main_rs_declares_its_modules_and_path_attributes_are_followed() {
        let r = reach(&[
            (
                "tests/unit/main.rs",
                "mod greet_name;\n// mod commented;\n#[path = \"../shared/x.rs\"]\nmod shared;\n",
            ),
            ("tests/unit/greet_name.rs", "#[test] fn a() {}\n"),
            ("tests/unit/commented.rs", "#[test] fn a() {}\n"),
            ("tests/shared/x.rs", "#[test] fn a() {}\n"),
            (
                "src/lib.rs",
                "#[cfg(feature = \"slow\")]\nmod slow;\nmod outer { mod inner; }\n#[path = \"../../escape.rs\"] mod escape;\n",
            ),
            ("src/slow.rs", "#[test] fn s() {}\n"),
            ("src/outer/inner.rs", "#[test] fn s() {}\n"),
        ]);
        assert_eq!(
            r["tests/unit/greet_name.rs"],
            [("greet::unit".to_owned(), "greet_name".to_owned())]
        );
        assert!(!r.contains_key("tests/unit/commented.rs"));
        assert_eq!(
            r["tests/shared/x.rs"],
            [("greet::unit".to_owned(), "shared".to_owned())]
        );
        assert_eq!(r["src/slow.rs"], [("greet".to_owned(), "slow".to_owned())]);
        assert_eq!(
            r["src/outer/inner.rs"],
            [("greet".to_owned(), "outer::inner".to_owned())]
        );
    }

    #[test]
    fn autotests_off_leaves_only_listed_targets() {
        let p = RustPackage {
            autotests: false,
            tests: vec![("it".to_owned(), "tests/it.rs".to_owned())],
            ..pkg()
        };
        let t = rust_targets(&p, &set(&["tests/it.rs", "tests/other.rs", "src/lib.rs"]));
        let roots: Vec<&str> = t.iter().map(|t| t.root_file.as_str()).collect();
        assert_eq!(roots, ["src/lib.rs", "tests/it.rs"]);
        assert!(rust_rule(&p).text.contains("`tests/it.rs`"));
    }

    #[test]
    fn test_ids_map_to_the_file_with_the_longest_module() {
        let r = reach(&[
            (
                "src/lib.rs",
                "mod a;\n#[cfg(test)] mod tests { #[test] fn t() {} }\n",
            ),
            ("src/a.rs", "#[test] fn x() {}\n"),
            ("tests/unit/main.rs", "mod greet_name;\n"),
            ("tests/unit/greet_name.rs", "#[test] fn a() {}\n"),
        ]);
        let ids = [
            "greet::tests::t",
            "greet::a::x",
            "greet::a::tests::y",
            "greet::unit::greet_name::a",
            "other::z",
        ]
        .map(str::to_owned);
        let m = rust_tests_by_file(&r, &ids);
        assert_eq!(m["src/lib.rs"], ["greet::tests::t"]);
        assert_eq!(m["src/a.rs"], ["greet::a::x", "greet::a::tests::y"]);
        assert_eq!(
            m["tests/unit/greet_name.rs"],
            ["greet::unit::greet_name::a"]
        );
        assert!(!m.contains_key("tests/unit/main.rs"));
    }

    #[test]
    fn markers_tell_tests_from_helpers() {
        assert!(defines_tests(Toolchain::Rust, "#[test]\nfn a() {}"));
        assert!(defines_tests(
            Toolchain::Rust,
            "#[tokio::test]\nasync fn a() {}"
        ));
        assert!(defines_tests(
            Toolchain::Rust,
            "#[test_log::test]\nfn a() {}"
        ));
        assert!(defines_tests(
            Toolchain::Rust,
            "#[sqlx::test(migrations = \"m\")]\nasync fn a() {}"
        ));
        assert!(defines_tests(Toolchain::Rust, "# [test]\nfn a() {}"));
        assert!(!defines_tests(
            Toolchain::Rust,
            "pub fn setup() {}\n// #[test]\n"
        ));
        assert!(defines_tests(
            Toolchain::Python,
            "def test_a():\n    pass\n"
        ));
        assert!(!defines_tests(
            Toolchain::Python,
            "import pytest\n@pytest.fixture\ndef x(): ...\n"
        ));
        assert!(defines_tests(
            Toolchain::JavaScript,
            "it('greets', () => {})"
        ));
        assert!(!defines_tests(
            Toolchain::JavaScript,
            "export const split = (s) => s.split(',')"
        ));
        assert!(defines_tests(
            Toolchain::Go,
            "package g\nfunc testLower(t *testing.T) {}\n"
        ));
        assert!(!defines_tests(
            Toolchain::Go,
            "package g\nfunc helper() {}\n"
        ));
    }

    #[test]
    fn the_retry_line_and_reason_read_as_the_contract_says() {
        let f = not_collected(
            "tests/unit/greet_name.rs",
            DecidedBy::Static,
            rust_rule(&pkg()),
            "this file is neither",
            rust_fixes("", "tests/unit/greet_name.rs"),
        );
        assert_eq!(
            reason(&f),
            "tests not collected: `tests/unit/greet_name.rs` — Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare); this file is neither"
        );
        assert_eq!(
            retry_line(&f),
            "`tests/unit/greet_name.rs` never ran: Cargo builds `tests/*.rs` and `tests/<dir>/main.rs`. Move the tests to `tests/greet_name.rs`, or add `tests/unit/main.rs` with `mod greet_name;`."
        );
    }

    #[test]
    fn pytest_rules() {
        let r = PytestRule::default();
        assert_eq!(
            r.breaks("tests/check_greet.py").as_deref(),
            Some("this file matches neither")
        );
        assert_eq!(r.breaks("tests/test_greet.py"), None);
        assert_eq!(r.breaks("greet_test.py"), None);
        assert!(
            r.breaks("build/test_x.py")
                .is_some_and(|w| w.contains("norecursedirs"))
        );
        let r = PytestRule {
            testpaths: vec!["tests".to_owned()],
            ..PytestRule::default()
        };
        assert_eq!(
            r.breaks("src/test_x.py").as_deref(),
            Some("this file is outside testpaths")
        );
        assert_eq!(
            r.rule().text,
            "pytest collects `test_*.py` and `*_test.py` (python_files) under `tests` (testpaths)"
        );
        assert!(python_id_matches(
            "tests/test_x.py",
            "tests/test_x.py::test_a"
        ));
        assert!(python_id_matches("tests/test_x.py", "tests.test_x::test_a"));
        assert!(python_id_matches(
            "tests/test_x.py",
            "tests.test_x.TestC::test_a"
        ));
        assert!(!python_id_matches(
            "tests/test_x.py",
            "tests.test_xy::test_a"
        ));
    }

    #[test]
    fn js_defaults_and_globs() {
        let v = JsRule {
            runner: JsRunner::Vitest,
            include: JsRunner::Vitest.defaults(),
            exclude: Vec::new(),
            source: "vitest (default)".to_owned(),
        };
        assert_eq!(v.breaks("src/greet.test.ts"), None);
        assert_eq!(v.breaks("src/greet.spec.mjs"), None);
        assert_eq!(
            v.breaks("src/greet.tests.ts").as_deref(),
            Some("this file does not match")
        );
        assert!(v.breaks("node_modules/x/a.test.ts").is_some());
        let j = JsRule {
            runner: JsRunner::Jest,
            include: JsRunner::Jest.defaults(),
            exclude: Vec::new(),
            source: "jest (default)".to_owned(),
        };
        assert_eq!(j.breaks("src/__tests__/greet.js"), None);
        assert_eq!(j.breaks("greet.test.tsx"), None);
        assert!(j.breaks("src/greet.check.js").is_some());
        assert!(glob_match("src/**/*.test.ts", "src/a/b/c.test.ts"));
        assert!(glob_match("src/**/*.test.ts", "src/c.test.ts"));
        assert!(!glob_match("src/*.test.ts", "src/a/c.test.ts"));
    }

    #[test]
    fn go_rules() {
        let none = BTreeSet::new();
        assert!(
            go_breaks(
                "pkg/greet/greet_test.go",
                "package greet\nfunc TestGreet(t *testing.T) {}\n",
                &none
            )
            .is_none()
        );
        assert!(
            go_breaks(
                "pkg/testdata/x_test.go",
                "func TestX(t *testing.T) {}",
                &none
            )
            .is_some_and(|(w, _)| w.contains("testdata"))
        );
        let tagged = "//go:build integration\n\npackage g\nfunc TestX(t *testing.T) {}\n";
        assert!(
            go_breaks("g/x_test.go", tagged, &none).is_some_and(|(w, _)| w.contains("integration"))
        );
        assert!(go_breaks("g/x_test.go", tagged, &set(&["integration"])).is_none());
        assert!(
            go_breaks(
                "g/x_test.go",
                "package g\nfunc testLower(t *testing.T) {}\n",
                &none
            )
            .is_some_and(|(w, _)| w.contains("testLower"))
        );
        assert_eq!(
            go_declared_tests(
                "func TestA(t *T) {}\nfunc Testb(t *T) {}\nfunc Example() {}\nfunc Test(t *T) {}\n"
            ),
            ["TestA", "Example", "Test"]
        );
        assert!(go_build_satisfied("linux || darwin", &none));
        assert!(!go_build_satisfied("!unix", &none));
        assert!(go_build_satisfied(
            "integration && !windows",
            &set(&["integration"])
        ));
        assert!(go_id_matches(
            "ex.com/m/g",
            &["TestA".to_owned()],
            "ex.com/m/g::TestA"
        ));
        assert!(go_id_matches(
            "ex.com/m/g",
            &["TestA".to_owned()],
            "TestA/sub"
        ));
        assert!(!go_id_matches(
            "ex.com/m/g",
            &["TestA".to_owned()],
            "ex.com/m/h::TestA"
        ));
    }

    proptest! {
        // Every file declared by construction from a crate root is reachable.
        #[test]
        fn declared_modules_are_reachable(names in proptest::collection::vec("[a-z]{1,6}", 1..6)) {
            let mut files: Vec<(String, String)> = Vec::new();
            let mut root = String::new();
            let mut dir = "tests/suite".to_owned();
            let mut prev: Option<String> = None;
            let mut uniq = BTreeSet::new();
            for n in names.iter().filter(|n| uniq.insert((*n).clone())) {
                let decl = format!("mod {n};\n");
                match &prev {
                    None => root.push_str(&decl),
                    Some(p) => {
                        if let Some(f) = files.iter_mut().find(|(f, _)| f == p) {
                            f.1.push_str(&decl);
                        }
                    }
                }
                let path = format!("{dir}/{n}.rs");
                files.push((path.clone(), "#[test] fn t() {}\n".to_owned()));
                dir = format!("{dir}/{n}");
                prev = Some(path);
            }
            files.push(("tests/suite/main.rs".to_owned(), root));
            let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            let r = reach(&refs);
            for (f, _) in &files {
                prop_assert!(r.contains_key(f), "{f} unreachable");
            }
        }
    }
}
