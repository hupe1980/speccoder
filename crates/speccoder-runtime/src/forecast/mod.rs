//! `speccoder forecast`: the feature's branch paired with each
//! other open branch, never the others with each other. Per pair, the merge
//! is made without touching any checkout (`git merge-tree`): conflicting
//! files, or a merged tree on which the frozen oracles run — mine always,
//! the sibling's when this project's state holds it. A scenario breaks only
//! when a test citing it passes on its own branch and fails on the merge
//! twice; its red→green evidence stands in for a run on the base.

mod oracles;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::forecast::{
    BrokenScenario, FetchState, ForecastReport, LIMIT, OracleFacts, PairForecast, PairVerdict,
    order, verdict,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use self::oracles::{Bench, Cache, Failed, Oracle, Outcome, Tally};
use crate::git::Git;
use crate::worktree;

const COMMAND: &str = "forecast";

/// What a forecast is asked.
#[derive(Default)]
pub struct Options<'a> {
    /// The unit (`None`: the current one).
    pub feature: Option<&'a str>,
    /// FetchState every remote first.
    pub fetch: bool,
    /// Siblings named by the person: checked whatever their age.
    pub named: &'a [String],
    /// speccoder's state directory (`None`: the default).
    pub state: Option<&'a Path>,
    /// Told each sibling as it is checked.
    pub progress: Option<&'a (dyn Fn(&str) + Sync)>,
}

/// The stored last forecast and the results it can reuse.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    report: Option<ForecastReport>,
    #[serde(default, flatten)]
    cache: Cache,
}

fn file(plane: &Path, feature: &str) -> PathBuf {
    plane.join("forecast").join(format!("{feature}.json"))
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new(COMMAND, status, message).with_code(code)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The project, its feature and where its state lives.
struct Place {
    root: PathBuf,
    top: PathBuf,
    /// The project's directory inside the repository, with a trailing `/`
    /// (empty at the top).
    prefix: String,
    feature: String,
    plane: PathBuf,
    state: PathBuf,
}

fn place(
    path: &Path,
    feature: Option<&str>,
    state: Option<&Path>,
) -> Result<Place, Box<CommandResult>> {
    let Some(state) = state.map(Path::to_path_buf).or_else(worktree::state_dir) else {
        return Err(Box::new(refuse(
            CommandStatus::ConfigError,
            "NO-STATE-DIR",
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )));
    };
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(COMMAND, &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            COMMAND,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = PathBuf::from(&snapshot.root);
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let Ok(Some(top)) = Git::at(&root).toplevel() else {
        return Err(Box::new(refuse(
            CommandStatus::GitPrecondition,
            "NO-REPOSITORY",
            "not a git repository",
        )));
    };
    let top = canonical(&top);
    let prefix = canonical(&root)
        .strip_prefix(&top)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .filter(|r| !r.is_empty())
        .map(|r| format!("{r}/"))
        .unwrap_or_default();
    Ok(Place {
        plane: worktree::project_state_dir(&state, &root),
        feature: f.feature.name.clone(),
        root,
        top,
        prefix,
        state,
    })
}

/// The last forecast of the feature, as stored (none yet: no data).
#[must_use]
pub fn last(path: &Path, feature: Option<&str>, state: Option<&Path>) -> CommandResult {
    let p = match place(path, feature, state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match load(&p.plane, &p.feature).report {
        Some(r) => CommandResult::new(COMMAND, CommandStatus::Ok, &r.headline())
            .with_data(CommandData::Forecast(Box::new(r))),
        None => CommandResult::new(COMMAND, CommandStatus::Ok, "no forecast yet"),
    }
}

fn load(plane: &Path, feature: &str) -> Stored {
    std::fs::read_to_string(file(plane, feature))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(plane: &Path, feature: &str, stored: &Stored) {
    let path = file(plane, feature);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(stored) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// One forecast of a feature at a time on this machine: the lock holds
/// its process id; a lock whose process is gone is a crashed forecast's.
struct Lock(PathBuf, String);

fn alive(pid: &str) -> bool {
    !pid.is_empty()
        && std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

impl Lock {
    fn take(plane: &Path, feature: &str) -> Option<Self> {
        let path = plane.join("forecast").join(format!("{feature}.lock"));
        std::fs::create_dir_all(path.parent()?).ok()?;
        let me = std::process::id().to_string();
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    f.write_all(me.as_bytes()).ok()?;
                    return Some(Self(path, me));
                }
                Err(_) => {
                    let holder = std::fs::read_to_string(&path).unwrap_or_default();
                    if alive(holder.trim()) {
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        None
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Only its own: never a lock another forecast took since.
        if std::fs::read_to_string(&self.0).is_ok_and(|h| h.trim() == self.1) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

fn fetch(git: &Git) -> FetchState {
    let remotes = git.line(&["remote"]).unwrap_or_default();
    if remotes.is_empty() {
        return FetchState::NoRemote;
    }
    // Never a prompt: a key with a passphrase or an unknown host fails.
    let ssh = std::env::var_os("GIT_SSH_COMMAND").unwrap_or_else(|| "ssh -o BatchMode=yes".into());
    match git.quiet(
        &["fetch", "--all", "--prune", "--quiet"],
        &[("GIT_SSH_COMMAND", ssh.as_os_str() as &OsStr)],
    ) {
        Ok(_) => FetchState::Fetched,
        Err(e) => FetchState::Failed {
            why: e.0.lines().last().unwrap_or("git fetch failed").to_owned(),
        },
    }
}

/// The ref siblings are measured from: the remote's default branch,
/// else the local default branch (`init.defaultBranch`, `main`,
/// `master`) — never a feature's branch, never merely what is checked out.
pub(crate) fn base_of(git: &Git) -> Option<String> {
    if let Some(d) = git
        .line(&["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .filter(|s| !s.is_empty())
    {
        return Some(d);
    }
    let configured = git
        .line(&["config", "--get", "init.defaultBranch"])
        .ok()
        .filter(|s| !s.is_empty());
    configured
        .into_iter()
        .chain(["main".to_owned(), "master".to_owned()])
        .filter(|b| !b.starts_with("speccoder/"))
        .find(|b| {
            git.succeeds(&["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")])
                .unwrap_or(false)
        })
}

/// A branch to pair with.
#[derive(Debug, Clone)]
struct Sibling {
    name: String,
    commit: String,
    updated: u64,
    author: String,
}

fn about(git: &Git, rev: &str) -> Option<(String, u64, String)> {
    let line = git
        .line(&["log", "-n1", "--format=%H%x1f%ct%x1f%an", rev, "--"])
        .ok()?;
    let mut p = line.split('\u{1f}');
    Some((
        p.next()?.to_owned(),
        p.next()?.parse().ok()?,
        p.next().unwrap_or_default().to_owned(),
    ))
}

/// Other local speccoder branches and every remote-tracking branch with
/// work beyond the base that mine does not have, one per commit, newest
/// first; past `LIMIT`, named only.
fn siblings(
    git: &Git,
    mine: &str,
    mine_commit: &str,
    base: Option<&str>,
) -> (Vec<Sibling>, Vec<String>) {
    let Ok(out) = git.line(&[
        "for-each-ref",
        "--sort=-committerdate",
        "--format=%(refname)%1f%(refname:short)%1f%(objectname)%1f%(committerdate:unix)%1f%(authorname)",
        "refs/heads/speccoder/",
        "refs/remotes/",
    ]) else {
        return (Vec::new(), Vec::new());
    };
    let twin = format!("/{mine}");
    let base_commit = base.and_then(|b| git.line(&["rev-parse", "--verify", "-q", b]).ok());
    let mut seen = BTreeSet::from([mine_commit.to_owned()]);
    let mut found: Vec<Sibling> = Vec::new();
    // A branch by its name without the remote: a local branch and its
    // remote copy are one branch, the newer kept.
    let named = |full: &str| -> String {
        full.strip_prefix("refs/heads/")
            .map(str::to_owned)
            .or_else(|| {
                full.strip_prefix("refs/remotes/")
                    .and_then(|r| r.split_once('/'))
                    .map(|(_, b)| b.to_owned())
            })
            .unwrap_or_else(|| full.to_owned())
    };
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();
    // Local names first, so a branch and its remote twin keep the local one.
    let mut rows: Vec<Vec<&str>> = out.lines().map(|l| l.split('\u{1f}').collect()).collect();
    rows.sort_by_key(|r| !r[0].starts_with("refs/heads/"));
    rows.sort_by_key(|r| std::cmp::Reverse(r.get(3).and_then(|t| t.parse::<u64>().ok())));
    for r in rows {
        let [full, short, commit, at, author] = r[..] else {
            continue;
        };
        if full.ends_with("/HEAD")
            || named(full).starts_with("speccoder-lane/")
            || short == mine
            || short.ends_with(&twin)
            || Some(short) == base
            || base_commit.as_deref() == Some(commit)
            || !seen.insert(commit.to_owned())
        {
            continue;
        }
        if let Some(b) = &base_commit
            && git
                .line(&["rev-list", "--count", &format!("{b}..{commit}")])
                .is_ok_and(|n| n == "0")
        {
            continue;
        }
        if git
            .succeeds(&["merge-base", "--is-ancestor", commit, mine_commit])
            .unwrap_or(false)
        {
            continue;
        }
        let name = named(full);
        if let Some(newer) = by_name.get(&name)
            && git
                .succeeds(&["merge-base", "--is-ancestor", commit, newer])
                .unwrap_or(false)
        {
            continue;
        }
        by_name.insert(name, commit.to_owned());
        found.push(Sibling {
            name: short.to_owned(),
            commit: commit.to_owned(),
            updated: at.parse().unwrap_or(0),
            author: author.to_owned(),
        });
    }
    let rest = found.split_off(found.len().min(LIMIT));
    (found, rest.into_iter().map(|s| s.name).collect())
}

/// The merge of two tips, made in git's object store only.
enum Merge {
    /// The merged tree.
    Clean(String),
    /// The files edited on both sides.
    Conflicts(Vec<String>),
    /// git could not tell.
    Failed(String),
}

fn merge(git: &Git, mine: &str, theirs: &str) -> Merge {
    match git.outcome(&[
        "merge-tree",
        "--write-tree",
        "--name-only",
        "--no-messages",
        "-z",
        mine,
        theirs,
    ]) {
        Ok((Some(0), out, _)) => Merge::Clean(fields(&out).into_iter().next().unwrap_or_default()),
        Ok((Some(1), out, _)) => {
            let mut files: Vec<String> = fields(&out).into_iter().skip(1).collect();
            files.sort();
            files.dedup();
            Merge::Conflicts(files)
        }
        Ok((_, _, err)) => Merge::Failed(
            err.lines()
                .last()
                .unwrap_or("git merge-tree failed")
                .to_owned(),
        ),
        Err(e) => Merge::Failed(e.0),
    }
}

fn fields(out: &[u8]) -> Vec<String> {
    out.split(|b| *b == 0)
        .map(|s| String::from_utf8_lossy(s).trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Prose docs only (Markdown, reStructuredText, AsciiDoc, or under
/// `docs/`): text, data and images can be what a test reads.
fn docs_only(files: &[String]) -> bool {
    const EXT: &[&str] = &["md", "mdx", "markdown", "rst", "adoc"];
    !files.is_empty()
        && files.iter().all(|f| {
            f.split('/').next() == Some("docs")
                || Path::new(f)
                    .extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|e| EXT.contains(&e.to_ascii_lowercase().as_str()))
        })
}

/// What the sibling changed since it left mine.
fn changed(git: &Git, mine: &str, theirs: &str) -> Vec<String> {
    let Ok(mb) = git.line(&["merge-base", mine, theirs]) else {
        return Vec::new();
    };
    git.run(&["diff", "--name-only", "-z", &mb, theirs])
        .map(|o| fields(&o))
        .unwrap_or_default()
}

/// The unit a sibling speccoder branch is for (`speccoder/003-x`,
/// `origin/speccoder/003-x`).
fn unit_of(branch: &str) -> Option<&str> {
    let parts: Vec<&str> = branch.split('/').collect();
    match parts[..] {
        ["speccoder", u] | [_, "speccoder", u] if !u.is_empty() => Some(u),
        _ => None,
    }
}

/// `[sandbox]` and `[worktree]`, and a digest of the file they came from.
fn config(root: &Path) -> Result<(crate::sandbox::Config, crate::prepare::Config, String), String> {
    let text = crate::verify::find_config(root)
        .and_then(|c| std::fs::read_to_string(c.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    Ok((
        crate::sandbox::parse(&text).map_err(|e| format!("speccoder.toml: {e}"))?,
        crate::prepare::parse(&text).map_err(|e| format!("speccoder.toml: {e}"))?,
        crate::integrity::sha256_hex(text.as_bytes())
            .get(..16)
            .unwrap_or_default()
            .to_owned(),
    ))
}

/// `speccoder forecast`.
#[must_use]
pub fn run(path: &Path, opts: &Options<'_>) -> CommandResult {
    let p = match place(path, opts.feature, opts.state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let git = Git::at(&p.top);
    let mine = format!("speccoder/{}", p.feature);
    let Ok(mine_commit) = git.line(&[
        "rev-parse",
        "--verify",
        "-q",
        &format!("refs/heads/{mine}^{{commit}}"),
    ]) else {
        return refuse(
            CommandStatus::NotFound,
            "FORECAST-NO-BRANCH",
            &format!("nothing to forecast: {mine} does not exist yet — run the feature first"),
        )
        .with_fix(vec!["speccoder run".to_owned()]);
    };
    let Some(_lock) = Lock::take(&p.plane, &p.feature) else {
        return refuse(
            CommandStatus::UsageError,
            "FORECAST-BUSY",
            &format!("a forecast of {} is already running here", p.feature),
        );
    };
    let (sandbox, worktree_config, salt) = match config(&p.root) {
        Ok(c) => c,
        Err(e) => return refuse(CommandStatus::ConfigError, "CONFIG-INVALID", &e),
    };
    let fetched = if opts.fetch {
        fetch(&git)
    } else {
        FetchState::Skipped
    };
    let base = base_of(&git);
    let base_commit = base
        .as_deref()
        .and_then(|b| git.line(&["rev-parse", "--verify", "-q", b]).ok());
    // Named siblings, after the fetch: an unknown name is the person's
    // mistake; mine, the base and a name given twice are not paired.
    let mut named = Vec::new();
    let mut seen = BTreeSet::from([mine_commit.clone()]);
    seen.extend(base_commit);
    for n in opts.named {
        let Ok(commit) = git.line(&["rev-parse", "--verify", "-q", &format!("{n}^{{commit}}")])
        else {
            return refuse(
                CommandStatus::UsageError,
                "USAGE",
                &format!("no such branch: {n}"),
            );
        };
        if !seen.insert(commit.clone()) {
            continue;
        }
        let (_, updated, author) = about(&git, &commit).unwrap_or_default();
        named.push(Sibling {
            name: n.clone(),
            commit,
            updated,
            author,
        });
    }
    let (siblings, unchecked) = if opts.named.is_empty() {
        siblings(&git, &mine, &mine_commit, base.as_deref())
    } else {
        (named, Vec::new())
    };

    let mut notices = Vec::new();
    let mine_oracles = oracles::of_unit(
        &p.plane,
        &p.top,
        &p.prefix,
        &p.feature,
        &mine_commit,
        true,
        &mut notices,
    );
    let mut stored = load(&p.plane, &p.feature);
    let mut tally = Tally::default();
    let bench = Bench {
        top: &p.top,
        prefix: &p.prefix,
        salt,
        tmp: p.plane.join("tmp"),
        state: &p.state,
        sandbox: &sandbox,
        worktree: &worktree_config,
    };
    // Tests run only in the OS sandbox: they are code from other branches.
    let unsandboxed = match crate::sandbox::runner(&p.root, &sandbox) {
        Ok((r, _)) if r.is_sandboxed() => None,
        Ok((_, notice)) => {
            Some(notice.unwrap_or_else(|| "commands would run unsandboxed".to_owned()))
        }
        Err(e) => Some(e),
    };
    if let Some(why) = &unsandboxed
        && !mine_oracles.is_empty()
    {
        notices.push(format!(
            "no test ran — the forecast runs other branches' code only in the OS sandbox: {why}"
        ));
    }
    let tree_of = |rev: &str| {
        git.line(&["rev-parse", &format!("{rev}^{{tree}}")])
            .unwrap_or_default()
    };
    let mine_tree = tree_of(&mine_commit);

    // My scenarios that hold on my branch: the ones a merge can break.
    let mut mine_green: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut facts = OracleFacts::default();
    for o in &mine_oracles {
        facts.stories.push(o.story.clone());
        facts.scenarios += o.scenarios().len();
        facts.tests += o.tests.len();
    }
    if unsandboxed.is_none() && !mine_oracles.is_empty() {
        let tests: Vec<_> = mine_oracles.iter().flat_map(|o| o.tests.iter()).collect();
        match bench.run(
            &mine_commit,
            &mine_tree,
            &tests,
            &mut stored.cache,
            &mut tally,
        ) {
            Ok(runs) => {
                for o in &mine_oracles {
                    let g = oracles::green(o, &runs);
                    facts.green += g.len();
                    mine_green.entry(o.unit.clone()).or_default().extend(g);
                }
            }
            Err(e) => notices.push(format!("your oracle could not run on your branch: {e}")),
        }
    }

    let total = siblings.len();
    let mut pairs = Vec::new();
    for (i, s) in siblings.iter().enumerate() {
        if let Some(say) = opts.progress {
            say(&format!("checking {} ({}/{total})", s.name, i + 1));
        }
        pairs.push(pair(
            &git,
            &bench,
            &p,
            s,
            (&mine_commit, &mine_oracles, &mine_green),
            unsandboxed.as_deref(),
            &mut stored.cache,
            &mut tally,
        ));
    }
    order(&mut pairs);
    let report = ForecastReport {
        feature: p.feature.clone(),
        branch: mine,
        commit: mine_commit,
        base,
        fetch: fetched,
        oracle: facts,
        pairs,
        unchecked,
        notices,
        ran: tally.ran,
        reused: tally.reused,
        at: now(),
    };
    stored.report = Some(report.clone());
    save(&p.plane, &p.feature, &stored);
    result(report)
}

/// The report as a command's result: a person is needed when a pair
/// conflicts or breaks.
fn result(report: ForecastReport) -> CommandResult {
    let headline = report.headline();
    let fixes: Vec<String> = report
        .problems()
        .flat_map(|p| p.fixes.iter().cloned())
        .collect();
    let res = if fixes.is_empty() {
        CommandResult::new(COMMAND, CommandStatus::Ok, &headline)
    } else {
        CommandResult::new(COMMAND, CommandStatus::NeedsHuman, &headline)
            .with_code("FORECAST-CONFLICT")
            .with_fix(fixes)
    };
    res.with_data(CommandData::Forecast(Box::new(report)))
}

/// One sibling with mine.
#[allow(clippy::too_many_arguments)]
fn pair(
    git: &Git,
    bench: &Bench<'_>,
    p: &Place,
    s: &Sibling,
    mine: (&str, &[Oracle], &BTreeMap<String, BTreeSet<String>>),
    unsandboxed: Option<&str>,
    cache: &mut Cache,
    tally: &mut Tally,
) -> PairForecast {
    let (mine_commit, mine_oracles, mine_green) = mine;
    let mut out = PairForecast {
        branch: s.name.clone(),
        commit: s.commit.clone(),
        author: s.author.clone(),
        updated: s.updated,
        verdict: PairVerdict::NotJudged,
        files: Vec::new(),
        broken: Vec::new(),
        rewritten: Vec::new(),
        setup: None,
        flaky: Vec::new(),
        held: 0,
        why: None,
        their_oracle: false,
        fixes: Vec::new(),
    };
    let who = if s.author.is_empty() {
        "its author"
    } else {
        s.author.as_str()
    };
    let tree = match merge(git, mine_commit, &s.commit) {
        Merge::Conflicts(files) => {
            // As the project names them (the repository's path outside it).
            let files: Vec<String> = files
                .into_iter()
                .map(|f| f.strip_prefix(&p.prefix).map_or(f.clone(), str::to_owned))
                .collect();
            out.fixes.push(format!(
                "`{}` edits {} too: merge it now while it is small (`git merge {}`), or talk to {who}",
                s.name,
                list(&files),
                s.name
            ));
            out.files = files;
            out.verdict = PairVerdict::Conflicts;
            return out;
        }
        Merge::Failed(e) => {
            out.why = Some(format!("the merge could not be made: {e}"));
            return out;
        }
        Merge::Clean(tree) => tree,
    };
    // Their oracle, when this machine holds it.
    let mut skipped = Vec::new();
    let theirs: Vec<Oracle> = unit_of(&s.name)
        .filter(|u| *u != p.feature)
        .map(|u| {
            oracles::of_unit(
                &p.plane,
                &p.top,
                &p.prefix,
                u,
                &s.commit,
                false,
                &mut skipped,
            )
        })
        .unwrap_or_default();
    let why = if docs_only(&changed(git, mine_commit, &s.commit)) {
        Some("docs only".to_owned())
    } else if mine_oracles.is_empty() && theirs.is_empty() {
        Some("no frozen oracle here to judge it".to_owned())
    } else if unsandboxed.is_some() {
        Some("no sandbox".to_owned())
    } else if theirs.is_empty() && mine_green.values().all(BTreeSet::is_empty) {
        Some("no oracle green on yours".to_owned())
    } else {
        None
    };
    if let Some(why) = why {
        out.why = Some(why);
        return out;
    }

    // What holds on each side's own branch.
    let mut green: BTreeMap<String, BTreeSet<String>> = mine_green.clone();
    if !theirs.is_empty() {
        let tests: Vec<_> = theirs.iter().flat_map(|o| o.tests.iter()).collect();
        let their_tree = git
            .line(&["rev-parse", &format!("{}^{{tree}}", s.commit)])
            .unwrap_or_default();
        match bench.run(&s.commit, &their_tree, &tests, cache, tally) {
            Ok(runs) => {
                out.their_oracle = true;
                for o in &theirs {
                    green
                        .entry(o.unit.clone())
                        .or_default()
                        .extend(oracles::green(o, &runs));
                }
            }
            // Mine still judges; with nothing of mine to judge, say why.
            Err(e) if mine_green.values().all(BTreeSet::is_empty) => {
                out.why = Some(format!("their oracle could not run on their branch: {e}"));
                return out;
            }
            Err(_) => {}
        }
    }
    let all: Vec<&Oracle> = mine_oracles.iter().chain(theirs.iter()).collect();
    // A merge that changes a frozen test rewrites what judges the work.
    let mut rewritten = BTreeSet::new();
    for o in &all {
        for t in &o.tests {
            if oracles::blob_sha(&p.top, &tree, &p.prefix, &t.path).as_deref()
                != Some(t.sha256.as_str())
            {
                rewritten.insert(t.path.clone());
            }
        }
    }
    let tests: Vec<_> = all
        .iter()
        .flat_map(|o| o.tests.iter())
        .filter(|t| !rewritten.contains(&t.path))
        .filter(|t| {
            // Only tests of scenarios that hold somewhere decide anything.
            all.iter().any(|o| {
                o.tests.iter().any(|x| x.path == t.path)
                    && green
                        .get(&o.unit)
                        .is_some_and(|g| t.scenarios.iter().any(|s| g.contains(s)))
            })
        })
        .collect();
    let runs = if tests.is_empty() {
        BTreeMap::new()
    } else {
        match bench.run(mine_commit, &tree, &tests, cache, tally) {
            Ok(r) => r,
            // A dependency of theirs not on this machine looks the same as
            // a lockfile the merge broke: shown, never called a break.
            Err(Failed::Setup(e)) => {
                out.why = Some("the merge does not set up here".to_owned());
                out.setup = Some(e);
                return out;
            }
            Err(Failed::Other(e)) => {
                out.why = Some(format!("the oracles could not run on the merge: {e}"));
                return out;
            }
        }
    };
    let mut undecided: Vec<String> = Vec::new();
    for o in &all {
        let Some(g) = green.get(&o.unit) else {
            continue;
        };
        for scenario in o.scenarios().into_iter().filter(|sc| g.contains(sc)) {
            let citing: Vec<_> = o
                .tests
                .iter()
                .filter(|t| t.scenarios.contains(&scenario))
                .collect();
            if citing.iter().any(|t| rewritten.contains(&t.path)) {
                continue;
            }
            if let Some((t, why)) = citing.iter().find_map(|t| {
                runs.get(&t.path)
                    .and_then(|r: &Outcome| r.undecided.as_deref())
                    .map(|w| (t, w))
            }) {
                undecided.push(format!("{} {why} on the merge", t.path));
                continue;
            }
            let failed = citing
                .iter()
                .find(|t| runs.get(&t.path).is_some_and(|r: &Outcome| !r.passed));
            match failed {
                Some(t) => {
                    let r = &runs[&t.path];
                    let whose = if o.mine {
                        String::new()
                    } else {
                        format!("{}'s ", o.unit)
                    };
                    let contract = o.contracts.get(&scenario).cloned();
                    out.fixes.push(format!(
                        "`{}` {} {whose}{scenario}{}: see it with `git merge --no-commit --no-ff {}` then `{}`, and talk to {who} before either merges",
                        s.name,
                        if o.mine { "breaks" } else { "is broken by yours —" },
                        contract.as_deref().map(|c| format!(" ({c})")).unwrap_or_default(),
                        s.name,
                        if p.prefix.is_empty() {
                            t.argv.join(" ")
                        } else {
                            format!("cd {} && {}", p.prefix.trim_end_matches('/'), t.argv.join(" "))
                        }
                    ));
                    out.broken.push(BrokenScenario {
                        unit: o.unit.clone(),
                        mine: o.mine,
                        scenario,
                        contract,
                        test: t.path.clone(),
                        output: r.output.clone(),
                    });
                }
                None => out.held += 1,
            }
        }
        for t in &o.tests {
            if runs.get(&t.path).is_some_and(|r| r.flaky) && !out.flaky.contains(&t.path) {
                out.flaky.push(t.path.clone());
            }
        }
    }
    undecided.dedup();
    if out.broken.is_empty() && !undecided.is_empty() {
        out.why = Some(undecided.join("; "));
    }
    out.rewritten = rewritten.into_iter().collect();
    if !out.rewritten.is_empty() {
        out.fixes.push(format!(
            "`{}` changes {}, frozen oracle tests: the merge rewrites what judges the work — ask {who} why",
            s.name,
            list(&out.rewritten)
        ));
    }
    out.verdict = verdict(&out);
    out
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_only_means_nothing_a_test_executes() {
        let v = |x: &[&str]| x.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(docs_only(&v(&["README.md", "docs/a.svg", "guide/x.MD"])));
        assert!(!docs_only(&v(&["README.md", "src/lib.rs"])));
        assert!(!docs_only(&v(&["fixtures/expected.txt"])));
        assert!(!docs_only(&v(&[])));
    }

    #[test]
    fn a_speccoder_branch_names_its_unit() {
        assert_eq!(unit_of("speccoder/003-x"), Some("003-x"));
        assert_eq!(unit_of("origin/speccoder/003-x"), Some("003-x"));
        assert_eq!(unit_of("origin/dana/csv"), None);
        assert_eq!(unit_of("speccoder/a/b"), None);
    }

    #[test]
    fn lists_stay_short() {
        let v = |n: usize| (0..n).map(|i| format!("f{i}")).collect::<Vec<_>>();
        assert_eq!(list(&v(1)), "f0");
        assert_eq!(list(&v(2)), "f0 and f1");
        assert_eq!(list(&v(4)), "f0, f1 and 2 more");
    }
}
