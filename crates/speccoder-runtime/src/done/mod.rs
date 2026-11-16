//! `speccoder done`: the definition of done for a checkout or a commit.
//!
//! [`Facts`] are read once — the files, what changed since the base, the
//! docs, the changelog, the version, the declared screenshots — and each
//! line in [`lines`] is a pure function of them. Nothing is written.

pub mod ci;
pub mod lines;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use speccoder_core::done::DoneReport;
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::git::Git;
use lines::{Page, Shot};

/// The docs pages when `[done] docs` is not set.
pub const DEFAULT_DOCS: &[&str] = &[
    "README.md",
    "**/README.md",
    "docs/**/*.md",
    "site/content/**/*.md",
];

/// The changelog read.
pub const CHANGELOG: &str = "CHANGELOG.md";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDone {
    docs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawShot {
    shot: String,
    sources: Vec<String>,
}

/// `[done]` and `[screenshots.<name>]` in `speccoder.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Globs of the docs pages; none: [`DEFAULT_DOCS`].
    pub docs: Option<Vec<String>>,
    /// Each declared screenshot: its glob and its sources' globs.
    pub screenshots: BTreeMap<String, (String, Vec<String>)>,
}

/// Reads `[done]` and `[screenshots.<name>]`; absent sections give the
/// defaults.
///
/// # Errors
///
/// An unknown key, a bad glob, or a screenshot without sources.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let raw: RawDone = match table.get("done") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[done]: {e}"))?,
        None => RawDone::default(),
    };
    for g in raw.docs.iter().flatten() {
        Glob::new(g).map_err(|e| format!("[done]: docs: {e}"))?;
    }
    let mut screenshots = BTreeMap::new();
    if let Some(v) = table.get("screenshots") {
        let shots = v
            .as_table()
            .ok_or("`screenshots` must be a table of [screenshots.<name>] tables")?;
        for (name, v) in shots {
            let s: RawShot = v
                .clone()
                .try_into()
                .map_err(|e| format!("[screenshots.{name}]: {e}"))?;
            if s.sources.is_empty() {
                return Err(format!(
                    "[screenshots.{name}]: `sources` names what the shot shows"
                ));
            }
            for g in std::iter::once(&s.shot).chain(&s.sources) {
                Glob::new(g).map_err(|e| format!("[screenshots.{name}]: {e}"))?;
            }
            screenshots.insert(name.clone(), (s.shot, s.sources));
        }
    }
    Ok(Config {
        docs: raw.docs,
        screenshots,
    })
}

/// What is judged: the files as they are on disk, or as a commit has them.
#[derive(Debug, Clone)]
pub enum Source {
    /// The working tree, uncommitted changes included.
    WorkingTree,
    /// A commit (a branch tip at `pr`).
    Commit(String),
}

/// Everything the lines judge, read once.
#[derive(Debug, Default)]
pub struct Facts {
    /// The base as named, and the commit compared with.
    pub base: Option<(String, String)>,
    /// Every file (tracked, and untracked but not ignored).
    pub files: BTreeSet<String>,
    /// Files changed since the base.
    pub changed: Vec<String>,
    /// The docs pages, read.
    pub pages: Vec<Page>,
    /// The changelog's text, when there is one.
    pub changelog: Option<String>,
    /// The project's version.
    pub version: Option<String>,
    /// The last release tag's version.
    pub last: Option<String>,
    /// The declared screenshots, with their history.
    pub shots: Vec<Shot>,
}

/// Reads files from the working tree, or from a commit — in one
/// `git cat-file --batch` for the paths asked for together.
struct Reader<'a> {
    git: &'a Git,
    root: &'a Path,
    source: &'a Source,
    cache: RefCell<HashMap<String, Option<String>>>,
}

impl<'a> Reader<'a> {
    fn new(git: &'a Git, root: &'a Path, source: &'a Source) -> Self {
        Self {
            git,
            root,
            source,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Reads `paths` of the commit at once (a no-op on the working tree).
    fn prefetch<'p>(&self, paths: impl IntoIterator<Item = &'p String>) {
        let Source::Commit(c) = self.source else {
            return;
        };
        let paths: Vec<&String> = paths.into_iter().collect();
        if paths.is_empty() {
            return;
        }
        let input: String = paths.iter().map(|p| format!("{c}:{p}\n")).collect();
        let Ok(out) = self
            .git
            .run_with_stdin(&["cat-file", "--batch"], input.as_bytes())
        else {
            return;
        };
        let mut cache = self.cache.borrow_mut();
        let mut rest = out.as_slice();
        for p in paths {
            // `<oid> blob <size>\n<bytes>\n`, or `<name> missing\n`.
            let Some(nl) = rest.iter().position(|b| *b == b'\n') else {
                break;
            };
            let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
            rest = &rest[nl + 1..];
            let size = header
                .rsplit(' ')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|_| header.contains(" blob "));
            let text = size.and_then(|n| {
                let body = rest.get(..n)?;
                rest = rest.get(n + 1..).unwrap_or_default();
                String::from_utf8(body.to_vec()).ok()
            });
            cache.insert(p.clone(), text);
        }
    }

    fn read(&self, path: &str) -> Option<String> {
        if let Some(hit) = self.cache.borrow().get(path) {
            return hit.clone();
        }
        let text = match self.source {
            Source::WorkingTree => std::fs::read_to_string(self.root.join(path)).ok(),
            Source::Commit(c) => self
                .git
                .run(&["show", &format!("{c}:{path}")])
                .ok()
                .and_then(|b| String::from_utf8(b).ok()),
        };
        self.cache
            .borrow_mut()
            .insert(path.to_owned(), text.clone());
        text
    }
}

/// Paths from `-z` output: never quoted, whatever they hold.
fn names(out: &[u8]) -> Vec<String> {
    out.split(|b| *b == 0 || *b == b'\n')
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Path globs: `*` stays within a folder, `**` crosses folders.
fn globs(patterns: &[impl AsRef<str>]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        if let Ok(g) = GlobBuilder::new(p.as_ref()).literal_separator(true).build() {
            b.add(g);
        }
    }
    b.build().unwrap_or_else(|_| GlobSet::empty())
}

/// Whether a path lies in tests, fixtures, vendored code or dated posts:
/// never a docs page by default.
fn testish(path: &str) -> bool {
    path.split('/').any(|c| {
        matches!(
            c,
            "tests" | "test" | "fixtures" | "node_modules" | "vendor" | "blog" | "news" | "posts"
        ) || c.starts_with('.')
    })
}

/// The base: `given`, else the remote's default branch when this one has
/// diverged from it, else the last release tag, else that branch.
fn base(git: &Git, given: Option<&str>, head: &str) -> Result<Option<(String, String)>, String> {
    let commit = |r: &str| {
        git.line(&["rev-parse", "--verify", "-q", &format!("{r}^{{commit}}")])
            .ok()
    };
    if let Some(b) = given {
        let c = commit(b).ok_or_else(|| format!("no such branch, tag or commit: {b}"))?;
        // What this branch changed: since it left the base, not what the
        // base did meanwhile.
        let mb = git.line(&["merge-base", head, &c]).unwrap_or(c);
        return Ok(Some((b.to_owned(), mb)));
    }
    let default = git
        .line(&["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .filter(|s| !s.is_empty());
    if let Some(d) = &default
        && let Ok(mb) = git.line(&["merge-base", head, d])
        && commit(head).as_deref() != Some(mb.as_str())
    {
        return Ok(Some((d.clone(), mb)));
    }
    if let Ok(tag) = git.line(&[
        "describe",
        "--tags",
        "--abbrev=0",
        "--match",
        "v[0-9]*",
        head,
    ]) && let Some(c) = commit(&tag)
    {
        return Ok(Some((tag, c)));
    }
    Ok(default.and_then(|d| commit(&d).map(|c| (d, c))))
}

/// The project's version from its manifest.
fn version(read: impl Fn(&str) -> Option<String>) -> Option<String> {
    let toml_at = |file: &str, paths: &[&[&str]]| {
        let t: toml::Table = toml::from_str(&read(file)?).ok()?;
        paths.iter().find_map(|p| {
            let mut v = t.get(p[0])?;
            for k in &p[1..] {
                v = v.get(k)?;
            }
            v.as_str().map(str::to_owned)
        })
    };
    toml_at(
        "Cargo.toml",
        &[
            &["workspace", "package", "version"],
            &["package", "version"],
        ],
    )
    .or_else(|| {
        let v: serde_json::Value = serde_json::from_str(&read("package.json")?).ok()?;
        v["version"].as_str().map(str::to_owned)
    })
    .or_else(|| {
        toml_at(
            "pyproject.toml",
            &[&["project", "version"], &["tool", "poetry", "version"]],
        )
    })
}

impl Facts {
    /// Reads the facts at `root` (the repository's top level), judging
    /// `source` against `base` (resolved when `None`).
    ///
    /// # Errors
    ///
    /// A base that names nothing.
    pub fn read(
        root: &Path,
        config: &Config,
        source: &Source,
        given: Option<&str>,
    ) -> Result<Self, String> {
        let git = Git::at(root);
        let head = match source {
            Source::WorkingTree => "HEAD".to_owned(),
            Source::Commit(c) => c.clone(),
        };
        let r = Reader::new(&git, root, source);
        let base = base(&git, given, &head)?;
        let (files, untracked): (BTreeSet<String>, BTreeSet<String>) = match source {
            Source::WorkingTree => {
                let tracked = git.run(&["ls-files", "-z"]).unwrap_or_default();
                let others = git
                    .run(&["ls-files", "-z", "--others", "--exclude-standard"])
                    .unwrap_or_default();
                let split = |b: &[u8]| {
                    b.split(|c| *c == 0)
                        .filter(|s| !s.is_empty())
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .collect::<BTreeSet<_>>()
                };
                let others = split(&others);
                // A tracked file deleted on disk is gone.
                let mut all: BTreeSet<String> = split(&tracked)
                    .into_iter()
                    .filter(|p| root.join(p).exists())
                    .collect();
                all.extend(others.iter().cloned());
                (all, others)
            }
            Source::Commit(c) => (
                names(
                    &git.run(&["ls-tree", "-r", "-z", "--name-only", c])
                        .unwrap_or_default(),
                )
                .into_iter()
                .collect(),
                BTreeSet::new(),
            ),
        };
        let changed = match (&base, source) {
            (None, _) => Vec::new(),
            (Some((_, b)), Source::WorkingTree) => {
                let mut c = names(
                    &git.run(&["diff", "-z", "--name-only", b])
                        .unwrap_or_default(),
                );
                c.extend(untracked.iter().cloned());
                c.sort();
                c.dedup();
                c
            }
            (Some((_, b)), Source::Commit(h)) => names(
                &git.run(&["diff", "-z", "--name-only", b, h])
                    .unwrap_or_default(),
            ),
        };
        let docs_globs = match &config.docs {
            Some(d) => globs(d),
            None => globs(DEFAULT_DOCS),
        };
        let wanted: Vec<&String> = files
            .iter()
            .filter(|p| docs_globs.is_match(p.as_str()) && (config.docs.is_some() || !testish(p)))
            .collect();
        let manifests =
            [CHANGELOG, "Cargo.toml", "package.json", "pyproject.toml"].map(String::from);
        r.prefetch(
            wanted
                .iter()
                .copied()
                .chain(manifests.iter().filter(|m| files.contains(*m))),
        );
        let pages = wanted
            .into_iter()
            .filter_map(|p| {
                r.read(p).map(|text| Page {
                    path: p.clone(),
                    text,
                })
            })
            .collect();
        let changelog = files
            .contains(CHANGELOG)
            .then(|| r.read(CHANGELOG))
            .flatten();
        let version = version(|f| files.contains(f).then(|| r.read(f)).flatten());
        let last = git
            .line(&[
                "describe",
                "--tags",
                "--abbrev=0",
                "--match",
                "v[0-9]*",
                &head,
            ])
            .ok()
            .map(|t| t.trim_start_matches('v').to_owned());
        let dirty: BTreeSet<String> = match source {
            Source::WorkingTree => {
                let mut d: BTreeSet<String> = names(
                    &git.run(&["diff", "-z", "--name-only", "HEAD"])
                        .unwrap_or_default(),
                )
                .into_iter()
                .collect();
                d.extend(untracked);
                d
            }
            Source::Commit(_) => BTreeSet::new(),
        };
        let shots = config
            .screenshots
            .iter()
            .map(|(name, (shot, sources))| {
                shot_facts(&git, &head, &files, &dirty, name, shot, sources)
            })
            .collect();
        Ok(Self {
            base,
            files,
            changed,
            pages,
            changelog,
            version,
            last,
            shots,
        })
    }
}

fn shot_facts(
    git: &Git,
    head: &str,
    files: &BTreeSet<String>,
    dirty: &BTreeSet<String>,
    name: &str,
    shot: &str,
    sources: &[String],
) -> Shot {
    let shot_set = globs(&[shot]);
    let source_set = globs(sources);
    let shots: Vec<String> = files
        .iter()
        .filter(|p| shot_set.is_match(p.as_str()))
        .cloned()
        .collect();
    let srcs: Vec<String> = files
        .iter()
        .filter(|p| source_set.is_match(p.as_str()))
        .cloned()
        .collect();
    // The newest commit touching any of `paths`: its time and the first
    // of them it touched.
    let newest = |paths: &[String]| -> Option<(String, i64)> {
        if paths.is_empty() {
            return None;
        }
        let mut args = vec!["log", "-1", "-z", "--format=%ct", "--name-only", head, "--"];
        args.extend(paths.iter().map(String::as_str));
        let out = names(&git.run(&args).ok()?);
        let t = out.first()?.parse().ok()?;
        let path = out[1..]
            .iter()
            .find(|p| paths.contains(p))
            .cloned()
            .unwrap_or_default();
        Some((path, t))
    };
    Shot {
        name: name.to_owned(),
        glob: shot.to_owned(),
        shot_time: newest(&shots).map(|(_, t)| t),
        shot_dirty: shots.iter().any(|s| dirty.contains(s)),
        newest_source: newest(&srcs),
        dirty_sources: srcs
            .iter()
            .filter(|s| dirty.contains(*s))
            .cloned()
            .collect(),
        files: shots,
    }
}

/// `%20` and the like decoded; anything malformed kept as written.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match s
            .get(i + 1..i + 3)
            .filter(|_| b[i] == b'%')
            .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            Some(n) => {
                out.push(n);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// Lexically normalised: `.` dropped, `..` pops; `None` above the root.
fn normal(path: &Path) -> Option<String> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(n) => out.push(n),
        }
    }
    Some(out.to_string_lossy().replace('\\', "/"))
}

/// The site a page belongs to: the folder holding `content/` and
/// `zola.toml` (or `config.toml`), and the page's path inside `content/`.
fn site_of<'a>(page: &'a str, files: &BTreeSet<String>) -> Option<(String, &'a str)> {
    let mut at = 0;
    while let Some(i) = page[at..].find("content/") {
        let start = at + i;
        if start == 0 || page.as_bytes()[start - 1] == b'/' {
            let site = &page[..start];
            if files.contains(&format!("{site}zola.toml"))
                || files.contains(&format!("{site}config.toml"))
            {
                return Some((site.to_owned(), &page[start + "content/".len()..]));
            }
        }
        at = start + 1;
    }
    None
}

/// Resolves the links of the pages against the files.
struct Links<'a> {
    files: &'a BTreeSet<String>,
    dirs: BTreeSet<String>,
    read: &'a dyn Fn(&str) -> Option<String>,
    anchors: RefCell<HashMap<String, Option<BTreeSet<String>>>>,
}

impl<'a> Links<'a> {
    fn new(files: &'a BTreeSet<String>, read: &'a dyn Fn(&str) -> Option<String>) -> Self {
        let mut dirs = BTreeSet::new();
        for f in files {
            let mut p = f.as_str();
            while let Some(i) = p.rfind('/') {
                p = &p[..i];
                if !dirs.insert(p.to_owned()) {
                    break;
                }
            }
        }
        Self {
            files,
            dirs,
            read,
            anchors: RefCell::new(HashMap::new()),
        }
    }

    fn has_anchor(&self, file: &str, anchor: &str, zola: bool) -> bool {
        let mut cache = self.anchors.borrow_mut();
        let a = cache
            .entry(file.to_owned())
            .or_insert_with(|| (self.read)(file).map(|t| lines::anchors(&t, zola)));
        a.as_ref().is_none_or(|a| a.contains(anchor))
    }

    /// A site URL path (`/docs/web/`) to the file it serves.
    fn site_file(&self, site: &str, url: &str) -> Option<String> {
        let path = url.trim_matches('/');
        if path.is_empty() {
            let index = format!("{site}content/_index.md");
            return self.files.contains(&index).then_some(index);
        }
        [
            format!("{site}static/{path}"),
            format!("{site}content/{path}"),
            format!("{site}content/{path}.md"),
            format!("{site}content/{path}/_index.md"),
            format!("{site}content/{path}/index.md"),
        ]
        .into_iter()
        .find(|c| self.files.contains(c))
    }

    fn resolve(&self, page: &Page, target: &str) -> Result<Option<String>, ()> {
        if lines::is_external(target) {
            return Err(());
        }
        let target = percent_decode(target.split('?').next().unwrap_or(target));
        let target = target.as_str();
        let (path, anchor) = target.split_once('#').unwrap_or((target, ""));
        let site = site_of(&page.path, self.files);
        let zola = site.is_some();
        let file = if path.is_empty() {
            page.path.clone()
        } else if let Some((site, inside)) = &site {
            if let Some(rest) = path.strip_prefix("@/") {
                let f = format!("{site}content/{rest}");
                if !self.files.contains(&f) {
                    return Ok(Some("no such page".to_owned()));
                }
                f
            } else {
                // Relative to the page's address: `docs/web.md` is served
                // at `/docs/web/`, `docs/_index.md` at `/docs/`.
                let url_dir = inside
                    .strip_suffix("_index.md")
                    // `post/index.md` is a page bundle, served at `/post/`.
                    .or_else(|| {
                        inside
                            .strip_suffix("index.md")
                            .filter(|d| d.is_empty() || d.ends_with('/'))
                    })
                    .or_else(|| inside.strip_suffix(".md"))
                    .unwrap_or(inside);
                let url = if path.starts_with('/') {
                    path.to_owned()
                } else {
                    format!("/{url_dir}/{path}")
                };
                let Some(url) = normal(Path::new(&url)) else {
                    return Ok(Some("above the site".to_owned()));
                };
                match self.site_file(site, &url) {
                    Some(f) => f,
                    None => return Ok(Some("nothing is served there".to_owned())),
                }
            }
        } else {
            let dir = Path::new(&page.path).parent().unwrap_or(Path::new(""));
            let joined = if let Some(abs) = path.strip_prefix('/') {
                PathBuf::from(abs)
            } else {
                dir.join(path)
            };
            let Some(f) = normal(&joined) else {
                return Ok(Some("above the repository".to_owned()));
            };
            if !self.files.contains(&f) && !self.dirs.contains(f.trim_end_matches('/')) {
                return Ok(Some("no such file".to_owned()));
            }
            f
        };
        if anchor.is_empty() || !Path::new(&file).extension().is_some_and(|e| e == "md") {
            return Ok(None);
        }
        Ok((!self.has_anchor(&file, anchor, zola)).then(|| format!("no heading #{anchor}")))
    }
}

/// The report of `facts`; `ci`: the CI line for the commit, if any.
#[must_use]
pub fn judge(
    facts: &Facts,
    read: &dyn Fn(&str) -> Option<String>,
    ci: Option<speccoder_core::done::DoneLine>,
) -> DoneReport {
    let docs: BTreeSet<String> = facts.pages.iter().map(|p| p.path.clone()).collect();
    let base = facts.base.as_ref().map(|(b, _)| b.as_str());
    let links = Links::new(&facts.files, read);
    let mut out = Vec::new();
    out.extend(lines::changelog(
        CHANGELOG,
        facts.changelog.as_deref(),
        &facts.changed,
        &docs,
        base,
        facts
            .version
            .as_deref()
            .filter(|v| facts.last.as_deref() != Some(*v)),
    ));
    if !facts.pages.is_empty() {
        out.push(lines::links(&facts.pages, |p, t| links.resolve(p, t)));
    }
    out.extend(lines::screenshots(&facts.shots));
    if !facts.pages.is_empty() {
        out.push(lines::tone(&facts.pages));
    }
    out.extend(lines::versions(
        &facts.pages,
        facts.version.as_deref(),
        facts.last.as_deref(),
    ));
    out.extend(ci);
    DoneReport::new(base.map(str::to_owned), out)
}

/// The definition of done at `root` (the repository's top) for `source`;
/// the configuration is the nearest `speccoder.toml` from `at` inside the
/// repository.
///
/// # Errors
///
/// The configuration does not parse (`ConfigError`), or the base names
/// nothing (`UsageError`).
pub fn evaluate(
    root: &Path,
    at: &Path,
    source: &Source,
    base: Option<&str>,
    ci: bool,
) -> Result<DoneReport, (CommandStatus, String)> {
    let top = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let config = crate::verify::find_config(at)
        .filter(|d| std::fs::canonicalize(d).is_ok_and(|d| d.starts_with(&top)))
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .map(|t| parse(&t))
        .transpose()
        .map_err(|e| (CommandStatus::ConfigError, format!("speccoder.toml: {e}")))?
        .unwrap_or_default();
    let facts =
        Facts::read(root, &config, source, base).map_err(|e| (CommandStatus::UsageError, e))?;
    let git = Git::at(root);
    let r = Reader::new(&git, root, source);
    // A link's target is mostly a page already read.
    let read = |p: &str| {
        facts
            .pages
            .iter()
            .find(|page| page.path == p)
            .map(|page| page.text.clone())
            .or_else(|| r.read(p))
    };
    let ci = ci.then(|| ci::line(root));
    Ok(judge(&facts, &read, ci))
}

/// `speccoder done [--base <ref>]`: the checkout as it is on disk.
#[must_use]
pub fn report(path: &Path, base: Option<&str>, ci: bool) -> CommandResult {
    let Ok(Some(root)) = Git::at(path).toplevel() else {
        return CommandResult::new(
            "done",
            CommandStatus::GitPrecondition,
            "not a git repository",
        )
        .with_code("NO-REPOSITORY");
    };
    match evaluate(&root, path, &Source::WorkingTree, base, ci) {
        Err((status, e)) => CommandResult::new("done", status, &e),
        Ok(r) => {
            let open: Vec<_> = r.open().collect();
            if open.is_empty() {
                return CommandResult::new("done", CommandStatus::Ok, "done: every line holds")
                    .with_data(CommandData::Done(Box::new(r)));
            }
            let message = format!(
                "{} of {} {} not done",
                open.len(),
                r.lines.len(),
                if r.lines.len() == 1 { "line" } else { "lines" }
            );
            let fix = open.iter().flat_map(|l| l.fixes.iter().cloned()).collect();
            CommandResult::new("done", CommandStatus::NeedsHuman, &message)
                .with_code("DONE-NOT-DONE")
                .with_fix(fix)
                .with_data(CommandData::Done(Box::new(r)))
        }
    }
}

/// The definition of done for `feature`'s branch against the base `pr`
/// would use (the checkout's branch), CI aside: what the Pull Request card
/// shows before Open Draft PR. No branch yet: no report.
#[must_use]
pub fn for_feature(path: &Path, feature: Option<&str>) -> CommandResult {
    let status = crate::status::report(path, feature);
    let Some(CommandData::Report(report)) = status.data() else {
        return status;
    };
    let Some(branch) = report.branch.clone() else {
        return CommandResult::new("done", CommandStatus::Ok, "no speccoder branch yet");
    };
    let Ok(Some(root)) = Git::at(path).toplevel() else {
        return CommandResult::new(
            "done",
            CommandStatus::GitPrecondition,
            "not a git repository",
        )
        .with_code("NO-REPOSITORY");
    };
    let git = Git::at(&root);
    let base = git.line(&["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    let tip = git.line(&["rev-parse", "--verify", &format!("{branch}^{{commit}}")]);
    let (Some(base), Ok(tip)) = (base.filter(|b| b != "HEAD" && *b != branch), tip) else {
        return CommandResult::new("done", CommandStatus::Ok, "no base to compare with");
    };
    match evaluate(&root, path, &Source::Commit(tip), Some(&base), false) {
        Err((status, e)) => CommandResult::new("done", status, &e),
        Ok(r) => {
            let open = r.open().count();
            let message = if open == 0 {
                "done: every line holds".to_owned()
            } else {
                format!("{open} of {} lines not done", r.lines.len())
            };
            CommandResult::new("done", CommandStatus::Ok, &message)
                .with_data(CommandData::Done(Box::new(r)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_takes_docs_and_named_screenshots_and_refuses_strays() {
        let c = parse(
            "[done]\ndocs = [\"site/**/*.md\"]\n[screenshots.trust]\nshot = \"s/trust-*.webp\"\nsources = [\"ui/T.svelte\"]\n",
        )
        .unwrap();
        assert_eq!(c.docs.as_deref(), Some(&["site/**/*.md".to_owned()][..]));
        assert_eq!(c.screenshots["trust"].1, ["ui/T.svelte"]);
        assert!(parse("[done]\nother = 1\n").is_err());
        assert!(parse("[screenshots.x]\nshot = \"a\"\nsources = []\n").is_err());
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_site_page_is_found_with_its_path_inside_content() {
        let files: BTreeSet<String> = ["site/zola.toml", "site/content/docs/web.md"]
            .map(str::to_owned)
            .into();
        assert_eq!(
            site_of("site/content/docs/web.md", &files),
            Some(("site/".to_owned(), "docs/web.md"))
        );
        assert_eq!(site_of("docs/content/x.md", &files), None);
    }

    #[test]
    fn site_links_resolve_by_address_and_readme_links_by_path() {
        let files: BTreeSet<String> = [
            "site/zola.toml",
            "site/content/_index.md",
            "site/content/docs/_index.md",
            "site/content/docs/web.md",
            "site/content/docs/security.md",
            "site/static/screenshots/a.webp",
            "site/content/blog/post/index.md",
            "site/content/blog/post/img.png",
            "README.md",
            "crates/x/README.md",
        ]
        .map(str::to_owned)
        .into();
        let read = |p: &str| -> Option<String> {
            Some(match p {
                "site/content/docs/web.md" => "# Web\n## The `Pull Request` card\n".to_owned(),
                "README.md" => "# 🖥️ The app\n".to_owned(),
                _ => String::new(),
            })
        };
        let l = Links::new(&files, &read);
        let sec = Page {
            path: "site/content/docs/security.md".into(),
            text: String::new(),
        };
        let readme = Page {
            path: "crates/x/README.md".into(),
            text: String::new(),
        };
        assert_eq!(
            l.resolve(&sec, "@/docs/web.md#the-pull-request-card"),
            Ok(None)
        );
        assert!(l.resolve(&sec, "@/docs/web.md#nope").unwrap().is_some());
        assert!(l.resolve(&sec, "@/docs/gone.md").unwrap().is_some());
        assert_eq!(l.resolve(&sec, "../../screenshots/a.webp"), Ok(None));
        assert_eq!(l.resolve(&sec, "../web/"), Ok(None));
        assert_eq!(l.resolve(&sec, "/screenshots/a.webp"), Ok(None));
        let bundle = Page {
            path: "site/content/blog/post/index.md".into(),
            text: String::new(),
        };
        assert_eq!(l.resolve(&bundle, "img.png"), Ok(None));
        assert!(l.resolve(&sec, "/screenshots/b.webp").unwrap().is_some());
        assert_eq!(l.resolve(&readme, "../../README.md#-the-app"), Ok(None));
        assert!(l.resolve(&readme, "../../NOPE.md").unwrap().is_some());
        assert_eq!(l.resolve(&readme, "https://x.y"), Err(()));
        assert_eq!(l.resolve(&readme, "../../READ%4DE.md"), Ok(None));
    }
}
