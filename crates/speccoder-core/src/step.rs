//! How a fix line or a next step is taken from an interface: speccoder
//! performs it, opens a view, runs the person's own command, or it belongs
//! in the person's shell. One parser ([`step_of`]) for every line speccoder
//! prints, so no interface copies a command speccoder can take itself.

use serde::{Deserialize, Serialize};

use crate::actions::{PlanFrom, Runnable, Through};

/// A read-only place an interface opens instead of running a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// `doctor`: every check of the setup.
    Doctor,
    /// `doctor --bug-report`: the setup as a bug report, to copy.
    BugReport,
    /// `report`, `status`: every task and its evidence.
    Report,
    /// `report --live`, `tui`: the run as it goes.
    Live,
    /// `report --approvals`, `task approve|reject`: the calls waiting,
    /// shown in full before a decision.
    Approvals,
    /// `explain CODE`: what a code means and how to resolve it.
    Explain,
    /// `review`: the spec's review.
    Review,
    /// `config`: the settings.
    Settings,
    /// `init`: the setup assistant.
    Setup,
    /// `bug`: the bug sheet.
    Bug,
    /// `do`: the quick-change sheet.
    QuickChange,
    /// `feature similar`: the new-feature sheet.
    NewFeature,
    /// `new`: the new-project sheet.
    NewProject,
    /// `auth`: the GitHub sign-in.
    #[serde(rename = "github")]
    GitHub,
    /// A task's drawer (`subject`: its id).
    Task,
}

/// How an interface takes a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum How {
    /// speccoder performs it (`actions::perform`).
    Perform {
        /// What runs.
        run: Box<Runnable>,
        /// What to ask for first (`Why it is skipped`); `None`: nothing.
        input: Option<String>,
        /// The words the line gives already (a `--reason "…"` filled in).
        text: Option<String>,
        /// What to say in a confirmation first (it leaves the machine or
        /// cannot be undone).
        confirm: Option<String>,
        /// The feature the line names (`--feature F`); `None`: the current.
        feature: Option<String>,
    },
    /// A view to open.
    View {
        /// Which.
        view: View,
        /// What it is about: a code, a task.
        subject: Option<String>,
    },
    /// A command speccoder printed as a fix and can run for the person, in
    /// the project folder and the OS sandbox (see `outside`) — never one
    /// composed elsewhere. Several argvs run in order, each only after the last
    /// one succeeded (`a && b`).
    Command {
        /// The argvs, exactly.
        argvs: Vec<Vec<String>>,
        /// It reads the keyboard (a login): a terminal of the person's
        /// own, not a panel.
        interactive: bool,
        /// What it changes outside the project, for a command that runs
        /// outside the sandbox by its nature (`changes your global git
        /// config`); `None`: it runs in the sandbox, writes confined to
        /// the project and the package caches.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outside: Option<String>,
        /// The host's proof that it issued exactly these argvs; only a
        /// host that runs commands sets it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
    /// It must run in the shell speccoder starts from (`export NAME=…`):
    /// nothing run elsewhere reaches that shell.
    Shell {
        /// Why it cannot run here.
        why: String,
        /// The setting that stands for it (`model`), if there is one.
        setting: Option<String>,
    },
    /// Words for a person (`→ …`), or a command only they can complete.
    Words {
        /// Why it is not taken here; `None` for words.
        why: Option<String>,
    },
}

/// A line with how it is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Step {
    /// The line, as printed.
    pub command: String,
    /// How it is taken.
    #[serde(flatten)]
    pub how: How,
}

/// What a line is about when it does not say: the result's feature and task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context<'a> {
    /// The feature.
    pub feature: Option<&'a str>,
    /// The task.
    pub task: Option<&'a str>,
}

/// The person's own programs a fix may name: anything else is words to
/// copy (a line of prose is never run by accident).
const PROGRAMS: &[&str] = &[
    "aws",
    "brew",
    "bun",
    "bundle",
    "cargo",
    "claude",
    "codex",
    "composer",
    "copilot",
    "corepack",
    "deno",
    "dotnet",
    "gem",
    "gemini",
    "gh",
    "git",
    "go",
    "gpg",
    "gradle",
    "just",
    "make",
    "mise",
    "mkdir",
    "mvn",
    "npm",
    "npx",
    "opencode",
    "openspec",
    "pip",
    "pip3",
    "pipx",
    "pnpm",
    "poetry",
    "python",
    "python3",
    "rustup",
    "specify",
    "ssh-keygen",
    "uv",
    "uvx",
    "xcode-select",
    "yarn",
];

/// The commands of speccoder no interface takes: developer tools.
const DEVELOPER: &[&str] = &["eval", "replay", "bench", "completion", "acp", "workflow"];

/// speccoder's subcommands: a line that starts `speccoder` and goes on
/// with anything else is words (`speccoder did not create …`).
pub const SUBCOMMANDS: &[&str] = &[
    "acp",
    "actions",
    "approvals",
    "attest",
    "auth",
    "bench",
    "bug",
    "completion",
    "config",
    "converge",
    "do",
    "doctor",
    "eval",
    "explain",
    "feature",
    "fix",
    "help",
    "inbox",
    "init",
    "inspect",
    "issue",
    "new",
    "pr",
    "preset",
    "propose",
    "replay",
    "report",
    "review",
    "run",
    "sample",
    "schema",
    "spec",
    "status",
    "suggest",
    "task",
    "tui",
    "ui",
    "verify",
    "workflow",
    "worktree",
];

/// The marker of text a person writes.
const BLANK: &str = "…";

/// Argvs run one after the other (`a && b`).
pub type Argvs = Vec<Vec<String>>;

/// How `line` is taken, with `ctx` filling what it leaves open.
///
/// A line is text, and text can carry values spliced into it (a file's
/// name, a feature's): so a line becomes the person's own command
/// ([`How::Command`]) only when it is one of speccoder's constant lines —
/// a catalog fix or a builder's line, its `{task}`/`{feature}` slots
/// filled with plain identifiers. A source that wants a command with
/// other values gives its argvs ([`person`], [`steps_with`]). A speccoder
/// command that speccoder performs or opens is read from any line: what it
/// becomes is a typed runnable, never an argv.
#[must_use]
pub fn step_of(line: &str, ctx: Context<'_>) -> Step {
    let filled = fill(line, ctx);
    Step {
        command: line.to_owned(),
        how: how(&filled),
    }
}

/// The person's own command a source built as argvs: shown as the line a
/// shell would read ([`render`]), run exactly as given.
#[must_use]
pub fn person(argvs: Argvs) -> Step {
    Step {
        command: render(&argvs),
        how: command(argvs),
    }
}

/// Every line of `fix`, as steps; a line a source gave as argvs
/// (`typed`, rendered to that line) is those argvs.
#[must_use]
pub fn steps_with(fix: &[String], typed: &[Argvs], ctx: Context<'_>) -> Vec<Step> {
    fix.iter()
        .map(|f| match typed.iter().find(|t| render(t) == *f) {
            Some(t) => person(t.clone()),
            None => step_of(f, ctx),
        })
        .collect()
}

/// Argvs as a shell reads them: a word with anything but plain characters
/// single-quoted; `&&` between them.
#[must_use]
pub fn render(argvs: &[Vec<String>]) -> String {
    let word = |w: &String| {
        let plain = !w.is_empty()
            && w.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | ',' | '@' | '+')
            });
        if plain {
            w.clone()
        } else {
            format!("'{}'", w.replace('\'', "'\\''"))
        }
    };
    argvs
        .iter()
        .map(|a| a.iter().map(word).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" && ")
}

fn fill(line: &str, ctx: Context<'_>) -> String {
    let mut s = line.to_owned();
    if let Some(t) = ctx.task {
        s = s.replace("{task}", t).replace("<task>", t);
    }
    if let Some(f) = ctx.feature {
        s = s.replace("{feature}", f).replace("<feature>", f);
    }
    s
}

fn words(why: Option<&str>) -> How {
    How::Words {
        why: why.map(str::to_owned),
    }
}

/// Why a line with values of the project's is not run from here.
const NAMES_VALUES: &str = "it names files or values of this project: run it in your shell";

/// A trailing "(why)" is said, not run.
fn said(line: &str) -> &str {
    let line = line.trim();
    match line.rfind(" (") {
        Some(at) if line.ends_with(')') => line[..at].trim_end(),
        _ => line,
    }
}

fn how(line: &str) -> How {
    let line = said(line);
    if line.starts_with('→') || line.is_empty() {
        return words(None);
    }
    if let Some(rest) = line.strip_prefix("export ") {
        let name = rest.split('=').next().unwrap_or_default().trim();
        let model = name.ends_with("_API_KEY") || name.starts_with("AWS_");
        return How::Shell {
            why: format!(
                "`{name}` must be set where speccoder starts: a command run from here ends with its own shell, and the variable with it"
            ),
            setting: model.then(|| "model".to_owned()),
        };
    }
    let Some(chain) = argvs(line) else {
        return words(Some("a shell line: run it in your shell"));
    };
    let Some(first) = chain.first() else {
        return words(None);
    };
    let constant = constant(&chain);
    if first.first().map(String::as_str) == Some("speccoder") {
        let known = first
            .get(1)
            .is_some_and(|w| SUBCOMMANDS.contains(&w.as_str()));
        return if known && chain.len() == 1 {
            speccoder(first, constant)
        } else {
            words(None)
        };
    }
    if chain
        .iter()
        .any(|a| a.first().map(String::as_str) == Some("sudo"))
    {
        return words(Some("it asks for your password: run it in your shell"));
    }
    if chain.iter().any(|a| {
        a.first()
            .is_none_or(|p| p == "speccoder" || !PROGRAMS.contains(&p.as_str()))
    }) {
        return words(None);
    }
    if chain.iter().flatten().any(|w| has_blank(w)) {
        return words(Some("it has a part only you can fill in"));
    }
    if constant {
        command(chain)
    } else {
        words(Some(NAMES_VALUES))
    }
}

/// Lines builders print that are no catalog fix: constant, or with
/// `{task}`/`{feature}` slots.
const BUILT: &[&str] = &[
    "speccoder run --in-place",
    "speccoder run --in-place --return",
    "speccoder run --ignore-checklists",
    "speccoder run --allow-failing-baseline",
    "speccoder run --parallel 2",
    "speccoder run --task {task} --fresh-acceptance",
    "speccoder attest verify",
    "speccoder attest verify --feature {feature} --json",
    "speccoder feature --clear",
    "speccoder explain --list",
    "speccoder fix --dry-run",
    "speccoder inspect",
    "speccoder worktree",
    "speccoder verify",
    "speccoder schema",
    "speccoder sample",
    "speccoder spec archive",
    "speccoder spec archive --yes",
    "git stash -u",
    "git status",
    "speccoder spec archive {change} --yes",
    "speccoder config commit",
    "speccoder help",
    "speccoder help advanced",
    "speccoder sample --fresh --yes",
    "speccoder spec",
    "ssh-keygen -t ed25519",
    "git config --global gpg.format ssh",
];

/// The constant lines, parsed: the catalog's fixes and [`BUILT`].
fn templates() -> &'static [Argvs] {
    static PARSED: std::sync::OnceLock<Vec<Argvs>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        crate::codes::CATALOG
            .iter()
            .flat_map(|e| e.fix.iter())
            // A line about another command (`speccoder {command} --help`)
            // is said, never signed: its slot names any command.
            .filter(|l| !l.contains("{command}"))
            .chain(BUILT.iter())
            .filter_map(|l| argvs(said(l)))
            .collect()
    })
}

/// A value a slot may take: a plain identifier (`001-x`, `T004`), never a
/// flag, never more than one word.
fn plain(v: &str) -> bool {
    v.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `w` is the template word `t` with its slots filled by plain values.
fn fills(t: &str, w: &str) -> bool {
    let Some(open) = t.find('{') else {
        return t == w;
    };
    let Some(close) = t[open..].find('}').map(|c| open + c) else {
        return t == w;
    };
    let (head, rest) = (&t[..open], &t[close + 1..]);
    let Some(w) = w.strip_prefix(head) else {
        return false;
    };
    // The slot takes the shortest plain run after which the rest fits.
    (1..=w.len())
        .filter(|n| w.is_char_boundary(*n))
        .any(|n| plain(&w[..n]) && fills(rest, &w[n..]))
}

/// `chain` is one of speccoder's constant lines (with `--feature F`
/// appended, as next steps are).
fn constant(chain: &[Vec<String>]) -> bool {
    let matches = |t: &[String], a: &[String]| {
        let a = match a {
            [rest @ .., f, v]
                if f == "--feature" && plain(v) && !t.iter().any(|w| w == "--feature") =>
            {
                rest
            }
            a => a,
        };
        t.len() == a.len() && t.iter().zip(a).all(|(t, w)| fills(t, w))
    };
    templates()
        .iter()
        .any(|t| t.len() == chain.len() && t.iter().zip(chain).all(|(t, a)| matches(t, a)))
}

fn has_blank(w: &str) -> bool {
    w.contains(BLANK) || (w.contains('<') && w.contains('>')) || w.contains('{')
}

type Signer = std::rc::Rc<dyn Fn(&[Vec<String>], bool) -> String>;

thread_local! {
    static SIGNER: std::cell::RefCell<Option<Signer>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `signer` signing every command step built meanwhile on
/// this thread (its `token`): a host that runs commands signs only the
/// steps speccoder itself builds, never JSON from elsewhere.
pub fn signing<T>(signer: Signer, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Signer>);
    impl Drop for Reset {
        fn drop(&mut self) {
            let before = self.0.take();
            SIGNER.with(|s| *s.borrow_mut() = before);
        }
    }
    let before = SIGNER.with(|s| s.borrow_mut().replace(signer));
    let _reset = Reset(before);
    f()
}

fn command(argvs: Argvs) -> How {
    let interactive = argvs.iter().any(|a| interactive(a));
    let token = SIGNER.with(|s| s.borrow().as_ref().map(|f| f(&argvs, interactive)));
    How::Command {
        interactive,
        outside: argvs.iter().find_map(|a| outside(a)),
        argvs,
        token,
    }
}

/// What `argv` changes outside the project when it is one of the few
/// fixes that, by their nature, cannot run confined to it: those run
/// outside the sandbox, and say so. Every other runs in it.
#[must_use]
pub fn outside(argv: &[String]) -> Option<String> {
    match argv
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["git", "config", "--global", ..] => Some("changes your global git config".to_owned()),
        ["git", "worktree", "remove", path] => Some(format!(
            "removes the worktree at {path}, outside the project"
        )),
        ["brew", "install", .., package] => {
            Some(format!("installs {package} on this machine with Homebrew"))
        }
        ["npm", "install", "-g" | "--global", package] => {
            Some(format!("installs {package} on this machine with npm"))
        }
        _ => None,
    }
}

/// `argv` reads the keyboard — a sign-in's prompt or device flow, an
/// editor, a question — so it needs a terminal of the person's own.
#[must_use]
pub fn interactive(argv: &[String]) -> bool {
    let w: Vec<&str> = argv.iter().map(String::as_str).collect();
    let has = |f: &[&str]| {
        w.iter()
            .any(|x| f.contains(x) || f.iter().any(|p| p.ends_with('=') && x.starts_with(p)))
    };
    match w.as_slice() {
        ["copilot", "login", ..]
        | ["opencode", "auth", "login", ..]
        | ["gh", "auth", "login" | "refresh", ..]
        | ["aws", "login" | "configure", ..]
        | ["aws", "sso", "login", ..]
        | ["claude", ..]
        | ["codex", "login", ..]
        | ["codex"]
        | ["gemini", ..]
        | ["npm", "login" | "adduser", ..]
        | ["specify", "init", ..]
        | ["openspec", "init", ..]
        | [
            "gpg",
            "--full-generate-key" | "--gen-key" | "--edit-key",
            ..,
        ]
        | ["speccoder", "tui" | "ui", ..]
        | ["speccoder", "auth", "login", ..] => true,
        ["speccoder", "spec", "archive", ..] => !has(&["--yes"]),
        ["ssh-keygen", ..] => !(has(&["-N"]) && has(&["-f"])),
        ["git", "commit", ..] => !has(&["-m", "-F", "--file", "--no-edit", "-C", "--message="]),
        ["git", "merge", ..] => !has(&["--no-edit", "--ff-only", "--abort"]),
        ["git", "rebase", ..] => has(&["-i", "--interactive"]),
        ["git", "add", ..] => has(&["-p", "-i", "--patch", "--interactive"]),
        _ => false,
    }
}

/// The words of a line, split as a shell would for the plain cases;
/// `None` for anything a shell would expand or redirect. `a && b` is two.
fn argvs(line: &str) -> Option<Vec<Vec<String>>> {
    let mut out = vec![Vec::new()];
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => word.push(chars.next()?),
            (Some('"'), '$' | '`') => return None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            // `&&` separates only as a word of its own (`a && b`).
            (None, '&') if !in_word && chars.peek() == Some(&'&') => {
                chars.next();
                if !chars.peek().is_none_or(|c| c.is_whitespace()) {
                    return None;
                }
                out.push(Vec::new());
            }
            // A comment ends the line.
            (None, '#') if !in_word => break,
            // `<name>` is a blank to fill in, never a redirection.
            (None, '<') => {
                word.push('<');
                loop {
                    let c = chars.next()?;
                    word.push(c);
                    if c == '>' {
                        break;
                    }
                }
                in_word = true;
            }
            (None, '|' | ';' | '>' | '$' | '`' | '&' | '(' | ')' | '*' | '~') => return None,
            (None, c) if c.is_whitespace() => {
                if in_word {
                    out.last_mut()?.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, '\\') => return None,
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        out.last_mut()?.push(word);
    }
    (!out.iter().any(Vec::is_empty)).then_some(out)
}

/// The value after `flag` (`--feature F` → `F`).
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// The first word after the subcommand's words that is no flag.
fn positional(args: &[String], skip: usize) -> Option<&str> {
    let mut i = skip;
    while let Some(a) = args.get(i) {
        if a.starts_with('-') {
            // A flag with a value: the flags speccoder's fixes use.
            let valued = matches!(
                a.as_str(),
                "--feature"
                    | "--reason"
                    | "--note"
                    | "--story"
                    | "--task"
                    | "--remote"
                    | "--through"
            );
            i += if valued { 2 } else { 1 };
            continue;
        }
        return Some(a);
    }
    None
}

/// The words given for an input, or the prompt to ask with when the line
/// leaves them blank (`"…"`).
fn input(given: Option<&str>, prompt: &str) -> (Option<String>, Option<String>) {
    match given {
        Some(t) if !t.contains(BLANK) && !t.trim().is_empty() => (None, Some(t.to_owned())),
        _ => (Some(prompt.to_owned()), None),
    }
}

fn view(view: View, subject: Option<&str>) -> How {
    How::View {
        view,
        subject: subject.map(str::to_owned),
    }
}

/// What a line about a feature names, when it names one fully.
fn named_feature(args: &[String]) -> Option<String> {
    flag(args, "--feature")
        .filter(|f| !has_blank(f))
        .map(str::to_owned)
}

fn perform(run: Runnable, args: &[String]) -> How {
    How::Perform {
        run: Box::new(run),
        input: None,
        text: None,
        confirm: None,
        feature: named_feature(args),
    }
}

fn asking(run: Runnable, args: &[String], given: Option<&str>, prompt: &str) -> How {
    let (input, text) = input(given, prompt);
    How::Perform {
        run: Box::new(run),
        input,
        text,
        confirm: None,
        feature: named_feature(args),
    }
}

fn confirmed(run: Runnable, args: &[String], confirm: &str) -> How {
    How::Perform {
        run: Box::new(run),
        input: None,
        text: None,
        confirm: Some(confirm.to_owned()),
        feature: named_feature(args),
    }
}

/// The confirmation a pull request asks for (it leaves the machine).
pub const PR_CONFIRM: &str = "Push the feature's speccoder branch and its attestation notes to `origin`, and open (or update) its draft pull request on GitHub, as you (your token).";

/// A speccoder command line, as an interface takes it.
#[allow(clippy::too_many_lines)] // one table, read top to bottom
fn speccoder(argv: &[String], constant: bool) -> How {
    // A feature not named yet is the one the interface has selected.
    let mut args: Vec<String> = argv.get(1..).unwrap_or_default().to_vec();
    if let Some(i) = args.iter().position(|a| a == "--feature")
        && args.get(i + 1).is_some_and(|f| has_blank(f))
    {
        args.drain(i..=i + 1);
    }
    let a = args.as_slice();
    let word = |i: usize| a.get(i).map(String::as_str);
    let task = |i: usize| {
        positional(a, i)
            .filter(|t| !has_blank(t))
            .map(str::to_owned)
    };
    // As written: only one of speccoder's constant lines.
    let run_cmd = || {
        if a.iter().any(|w| has_blank(w)) {
            words(Some("it has a part only you can fill in"))
        } else if constant {
            command(vec![argv.to_vec()])
        } else {
            words(Some(NAMES_VALUES))
        }
    };
    match (word(0), word(1)) {
        (Some("doctor"), _) if has(a, "--bug-report") => view(View::BugReport, None),
        (Some("doctor"), _) => view(View::Doctor, None),
        (Some("status"), _) => view(View::Report, None),
        (Some("report"), _) if has(a, "--approvals") => view(View::Approvals, None),
        (Some("report"), _) if has(a, "--live") => view(View::Live, None),
        (Some("report"), _) => view(View::Report, None),
        (Some("tui"), _) => view(View::Live, None),
        (Some("ui" | "inbox" | "approvals"), _) if !has(a, "-C") => view(View::Approvals, None),
        (Some("explain"), Some(code)) if !code.starts_with('-') => view(View::Explain, Some(code)),
        (Some("run"), _) if has(a, "--converge") && !a.iter().any(|w| w == "--task") => {
            perform(Runnable::Converge, a)
        }
        (Some("converge"), _) => perform(Runnable::Converge, a),
        (Some("run"), _) => {
            // Only what a run from an interface does: one task or the
            // feature, nothing else changed. Other flags run as written.
            let mut rest = a.iter().skip(1);
            let mut plain = true;
            while let Some(w) = rest.next() {
                match w.as_str() {
                    "--task" | "--feature" | "--agent" => plain &= rest.next().is_some(),
                    "-v" => {}
                    _ => plain = false,
                }
            }
            match flag(a, "--task") {
                _ if !plain => run_cmd(),
                Some(t) if has_blank(t) => view(View::Report, None),
                Some(t) => perform(
                    Runnable::Run {
                        task: Some(t.to_owned()),
                        agent: flag(a, "--agent").map(str::to_owned),
                    },
                    a,
                ),
                None => perform(
                    Runnable::Run {
                        task: None,
                        agent: flag(a, "--agent").map(str::to_owned),
                    },
                    a,
                ),
            }
        }
        (Some("task"), Some("approve" | "reject")) => view(View::Approvals, None),
        (Some("task"), Some(verb)) => {
            let Some(t) = task(2) else {
                return view(View::Report, None);
            };
            match verb {
                "accept" => asking(
                    Runnable::Accept { task: t },
                    a,
                    flag(a, "--reason"),
                    "Why it is done",
                ),
                "skip" => asking(
                    Runnable::Skip { task: t },
                    a,
                    flag(a, "--reason"),
                    "Why it is skipped",
                ),
                "nudge" => asking(
                    Runnable::Nudge { task: t },
                    a,
                    flag(a, "--note"),
                    "The note",
                ),
                "hint" => asking(Runnable::Hint { task: t }, a, positional(a, 3), "The hint"),
                "rule" => asking(
                    Runnable::Rule { task: t },
                    a,
                    flag(a, "--reason"),
                    "Why the tests are right",
                ),
                "done" => perform(Runnable::Done { task: t }, a),
                "restore" => perform(Runnable::KeepSetAside { task: t }, a),
                "drop" => confirmed(
                    Runnable::DropSetAside { task: t.clone() },
                    a,
                    &format!(
                        "Discard the changes set aside before {t}? They are deleted, not kept anywhere."
                    ),
                ),
                _ => view(View::Task, Some(&t)),
            }
        }
        (Some("review"), Some("approve")) if a.len() == 2 => perform(
            Runnable::Review {
                ids: Vec::new(),
                verdict: "approve".to_owned(),
            },
            a,
        ),
        (Some("review"), Some("answer")) => match task(2) {
            Some(q) => asking(
                Runnable::Review {
                    ids: vec![q],
                    verdict: "answer".to_owned(),
                },
                a,
                positional(a, 3),
                "Your answer",
            ),
            None => view(View::Review, None),
        },
        (Some("review"), _) => view(View::Review, None),
        // Another project's: run there.
        (Some(_), _) if has(a, "-C") => words(Some("it is another project's: run it there")),
        (Some("spec"), Some("archive")) => run_cmd(),
        (Some("spec"), Some(name)) if !has_blank(name) && !name.starts_with('-') => {
            let given = positional(a, 2);
            if let Some(through) = flag(a, "--through").and_then(Through::parse) {
                // Words to plan from: the new-feature sheet asks for them.
                return match given {
                    Some(_) => view(View::NewFeature, None),
                    None => perform(
                        Runnable::PlanThrough {
                            from: PlanFrom::Continue,
                            through,
                        },
                        a,
                    ),
                };
            }
            if a.iter()
                .skip(2)
                .any(|w| w.starts_with('-') && w != "--feature")
            {
                // A flag only the command takes.
                return run_cmd();
            }
            let skill = |arguments: &str| Runnable::Skill {
                name: name.to_owned(),
                arguments: arguments.to_owned(),
            };
            match given {
                None => perform(skill(""), a),
                Some(w) if has_blank(w) => asking(skill(""), a, None, "Describe it"),
                Some(w) => perform(skill(w), a),
            }
        }
        (Some("bug"), Some("keep")) => match task(2) {
            Some(t) => asking(
                Runnable::BugKeep { task: t },
                a,
                flag(a, "--reason"),
                "Why it is kept",
            ),
            None => view(View::Bug, None),
        },
        (Some("bug"), Some("run")) => match flag(a, "--task").filter(|t| !has_blank(t)) {
            Some(t) => perform(
                Runnable::BugRun {
                    slug: None,
                    task: Some(t.to_owned()),
                },
                a,
            ),
            None => view(View::Bug, None),
        },
        (Some("bug"), _) => view(View::Bug, None),
        (Some("do"), _) => view(View::QuickChange, None),
        (Some("pr"), _) if flag(a, "--remote").is_some() => {
            words(Some("name the remote it pushes to"))
        }
        (Some("pr"), _) => confirmed(Runnable::PullRequest, a, PR_CONFIRM),
        (Some("fix"), _) if a.len() == 1 => perform(Runnable::FixSetup, a),
        (Some("preset"), w) if w != Some("add") => words(None),
        (Some("preset"), _) => confirmed(
            Runnable::PresetAdd { update: false },
            a,
            "Install the speccoder preset with the spec tool's own preset command? It writes the spec tool's preset directory and registry; no spec file changes.",
        ),
        (Some("feature"), Some("similar")) => view(View::NewFeature, None),
        (Some("feature"), None) => asking(Runnable::Feature, a, None, "The feature"),
        (Some("feature"), Some(name)) if !name.starts_with('-') => asking(
            Runnable::Feature,
            a,
            Some(name).filter(|n| !has_blank(n)),
            "The feature",
        ),
        (Some("init"), _) => view(View::Setup, None),
        (Some("config"), None | Some("get" | "unset")) => view(View::Settings, None),
        (Some("config"), Some("set")) if a.iter().any(|w| has_blank(w)) => {
            view(View::Settings, None)
        }
        (Some("new"), _) => view(View::NewProject, None),
        (Some("auth"), _) => view(View::GitHub, None),
        (Some(dev), _) if DEVELOPER.contains(&dev) => {
            words(Some("a developer tool: run it in your shell"))
        }
        _ => run_cmd(),
    }
}

impl How {
    /// It is a copy and nothing else: words, or a shell line.
    #[must_use]
    pub const fn copy_only(&self) -> bool {
        matches!(self, Self::Words { .. } | Self::Shell { .. })
    }
}

/// Every line of `fix`, as steps.
#[must_use]
pub fn steps(fix: &[String], ctx: Context<'_>) -> Vec<Step> {
    steps_with(fix, &[], ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(line: &str) -> How {
        step_of(
            line,
            Context {
                feature: Some("001-x"),
                task: Some("T001"),
            },
        )
        .how
    }

    fn run_of(line: &str) -> (Runnable, Option<String>, Option<String>) {
        match h(line) {
            How::Perform {
                run, input, text, ..
            } => (*run, input, text),
            other => panic!("{line}: {other:?}"),
        }
    }

    fn view_of(line: &str) -> (View, Option<String>) {
        match h(line) {
            How::View { view, subject } => (view, subject),
            other => panic!("{line}: {other:?}"),
        }
    }

    fn argvs_of(line: &str) -> (Vec<Vec<String>>, bool) {
        match h(line) {
            How::Command {
                argvs, interactive, ..
            } => (argvs, interactive),
            other => panic!("{line}: {other:?}"),
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn read_only_commands_open_their_view() {
        assert_eq!(view_of("speccoder doctor").0, View::Doctor);
        assert_eq!(
            view_of("speccoder doctor --feature {feature}").0,
            View::Doctor
        );
        assert_eq!(view_of("speccoder doctor --bug-report").0, View::BugReport);
        assert_eq!(view_of("speccoder report").0, View::Report);
        assert_eq!(view_of("speccoder status").0, View::Report);
        assert_eq!(view_of("speccoder report --approvals").0, View::Approvals);
        assert_eq!(view_of("speccoder report --live").0, View::Live);
        assert_eq!(view_of("speccoder report --feature <name>").0, View::Report);
        assert_eq!(
            view_of("speccoder explain NEEDS-APPROVAL"),
            (View::Explain, Some("NEEDS-APPROVAL".to_owned()))
        );
        assert_eq!(view_of("speccoder task approve {task}").0, View::Approvals);
        assert_eq!(view_of("speccoder init --force").0, View::Setup);
        assert_eq!(view_of("speccoder config").0, View::Settings);
        assert_eq!(
            view_of("speccoder config set checks.test.argv -- <your test command>").0,
            View::Settings
        );
        assert_eq!(view_of("speccoder do \"…\"").0, View::QuickChange);
        assert_eq!(
            view_of("speccoder bug \"<what happens>\" --expected \"<what should>\"").0,
            View::Bug
        );
        assert_eq!(
            view_of("speccoder review tests <REQ> --write").0,
            View::Review
        );
        assert_eq!(
            view_of("speccoder task acceptance-at {task} --story <US>"),
            (View::Task, Some("T001".to_owned()))
        );
    }

    #[test]
    fn speccoder_commands_with_a_runnable_perform() {
        let t = || "T001".to_owned();
        assert_eq!(
            run_of("speccoder run").0,
            Runnable::Run {
                task: None,
                agent: None
            }
        );
        assert_eq!(
            run_of("speccoder run --task T004 --agent opencode").0,
            Runnable::Run {
                task: Some("T004".to_owned()),
                agent: Some("opencode".to_owned())
            }
        );
        assert_eq!(
            run_of("speccoder run --task {task}").0,
            Runnable::Run {
                task: Some(t()),
                agent: None
            }
        );
        assert_eq!(
            run_of("speccoder run --feature {feature} -v").0,
            Runnable::Run {
                task: None,
                agent: None
            }
        );
        assert_eq!(run_of("speccoder run --converge").0, Runnable::Converge);
        assert_eq!(
            run_of("speccoder converge --feature {feature}").0,
            Runnable::Converge
        );
        assert_eq!(
            run_of("speccoder task skip {task} --reason \"…\""),
            (
                Runnable::Skip { task: t() },
                Some("Why it is skipped".to_owned()),
                None
            )
        );
        assert_eq!(
            run_of("speccoder task hint {task} \"…\"").1.as_deref(),
            Some("The hint")
        );
        assert_eq!(
            run_of("speccoder task done {task}").0,
            Runnable::Done { task: t() }
        );
        assert_eq!(
            run_of("speccoder task rule {task} --tests-right --reason \"…\"").0,
            Runnable::Rule { task: t() }
        );
        assert_eq!(
            run_of("speccoder bug keep {feature}/{task} --reason \"kept as a regression test\""),
            (
                Runnable::BugKeep {
                    task: "001-x/T001".to_owned()
                },
                None,
                Some("kept as a regression test".to_owned())
            )
        );
        assert_eq!(
            run_of("speccoder spec tasks"),
            (
                Runnable::Skill {
                    name: "tasks".to_owned(),
                    arguments: String::new()
                },
                None,
                None
            )
        );
        assert_eq!(
            run_of("speccoder spec plan --through tasks").0,
            Runnable::PlanThrough {
                from: PlanFrom::Continue,
                through: Through::Tasks
            }
        );
        assert_eq!(
            run_of("speccoder review answer Q1 \"…\"").1.as_deref(),
            Some("Your answer")
        );
        assert_eq!(run_of("speccoder fix").0, Runnable::FixSetup);
        assert!(matches!(
            h("speccoder pr --yes"),
            How::Perform { run, confirm: Some(_), .. } if *run == Runnable::PullRequest
        ));
        assert!(matches!(
            h("speccoder task drop {task}"),
            How::Perform { run, confirm: Some(_), .. }
                if matches!(*run, Runnable::DropSetAside { .. })
        ));
        assert_eq!(
            run_of("speccoder feature <name>"),
            (Runnable::Feature, Some("The feature".to_owned()), None)
        );
        // The feature a line names travels with it.
        assert!(matches!(
            h("speccoder run --feature 002-y"),
            How::Perform { feature: Some(f), .. } if f == "002-y"
        ));
    }

    #[test]
    fn what_has_no_runnable_runs_as_written_when_speccoder_wrote_it() {
        assert_eq!(
            argvs_of("speccoder run --ignore-checklists").0,
            vec![s(&["speccoder", "run", "--ignore-checklists"])]
        );
        assert_eq!(
            argvs_of("speccoder attest verify --feature {feature} --json").0,
            vec![s(&[
                "speccoder",
                "attest",
                "verify",
                "--feature",
                "001-x",
                "--json"
            ])]
        );
        assert_eq!(
            argvs_of("speccoder spec archive --yes").0,
            vec![s(&["speccoder", "spec", "archive", "--yes"])]
        );
    }

    #[test]
    fn the_persons_own_commands_are_exact_argvs() {
        // A constant line of speccoder's: run as it reads.
        assert_eq!(
            argvs_of("git add speccoder.toml && git commit -m \"speccoder: config\"").0,
            vec![
                s(&["git", "add", "speccoder.toml"]),
                s(&["git", "commit", "-m", "speccoder: config"])
            ]
        );
        assert_eq!(argvs_of("git stash -u").0, vec![s(&["git", "stash", "-u"])]);
        // Built as argvs by its source: run as given, shown as a shell reads it.
        let built = person(vec![s(&["git", "add", "--", "my file.toml", "it's"])]);
        assert_eq!(built.command, "git add -- 'my file.toml' 'it'\\''s'");
        assert!(matches!(&built.how, How::Command { argvs, .. } if argvs[0][3] == "my file.toml"));
        let typed = vec![vec![s(&["npm", "install"])]];
        let steps = steps_with(
            &["npm install".to_owned(), "npm ci".to_owned()],
            &typed,
            Context::default(),
        );
        assert!(matches!(&steps[0].how, How::Command { argvs, .. } if argvs == &typed[0]));
        // A line nobody built as argvs is never run from its text.
        assert!(steps[1].how.copy_only(), "{:?}", steps[1]);
    }

    #[test]
    fn spliced_values_never_become_a_command() {
        // A file named `.x&&make`, spliced into a line, and a comment.
        for line in [
            "git add .x&&make && git commit -m \"Track project config\"",
            "git add .x && make && git commit -m init",
            "git switch B && git merge b   # resolve the conflicts",
            "git worktree remove /tmp/wt --force",
            "npm install",
            "speccoder run --feature 001-a --yes",
            "speccoder attest verify --feature 001-a --yes",
        ] {
            let how = h(line);
            assert!(!matches!(&how, How::Command { .. }), "{line}: {how:?}");
        }
        // `&&` separates only as a word; a word with it is no line at all.
        assert_eq!(argvs("a .x&&make"), None);
        assert_eq!(
            argvs("git merge b   # resolve"),
            Some(vec![s(&["git", "merge", "b"])])
        );
        // A slot takes a plain identifier, never a flag or two words.
        assert!(matches!(
            h("speccoder run --task T004 --fresh-acceptance"),
            How::Command { .. }
        ));
        assert!(!matches!(
            h("speccoder run --task --yes --fresh-acceptance"),
            How::Command { .. }
        ));
        assert!(matches!(
            h("speccoder run --ignore-checklists --feature 001-x"),
            How::Command { .. }
        ));
    }

    #[test]
    fn free_slots_and_lines_about_other_commands_are_never_signed() {
        // `speccoder {command} --help` names any command: said, not run.
        for line in [
            "speccoder run --help",
            "speccoder run --json",
            "speccoder doctor --events",
        ] {
            assert!(!matches!(h(line), How::Command { .. }), "{line}");
        }
        // A key is the settings' to show, never a free slot.
        assert_eq!(
            view_of("speccoder config get model.region").0,
            View::Settings
        );
        assert_eq!(
            view_of("speccoder config unset model.base_url").0,
            View::Settings
        );
    }

    #[test]
    fn what_writes_outside_the_project_says_so() {
        let global = person(vec![s(&["git", "config", "--global", "gpg.format", "ssh"])]);
        assert!(
            matches!(global.how, How::Command { outside: Some(o), .. } if o == "changes your global git config")
        );
        let local = person(vec![s(&["npm", "install"])]);
        assert!(matches!(local.how, How::Command { outside: None, .. }));
    }

    #[test]
    fn what_reads_the_keyboard_is_known() {
        for argv in [
            &["copilot", "login"][..],
            &["opencode", "auth", "login"],
            &["gh", "auth", "login"],
            &["aws", "sso", "login", "--profile", "dev"],
            &["ssh-keygen", "-t", "ed25519"],
            &["git", "commit"],
            &["git", "merge", "b"],
            &["specify", "init", "--here"],
            &["speccoder", "spec", "archive"],
        ] {
            assert!(interactive(&s(argv)), "{argv:?}");
        }
        for argv in [
            &["brew", "install", "gh"][..],
            &["git", "commit", "-m", "x"],
            &["ssh-keygen", "-t", "ed25519", "-N", "", "-f", "k"],
            &["speccoder", "spec", "archive", "--yes"],
        ] {
            assert!(!interactive(&s(argv)), "{argv:?}");
        }
    }

    #[test]
    fn only_a_signer_on_this_thread_signs_and_only_what_is_built() {
        let unsigned = person(vec![s(&["npm", "install"])]);
        assert!(matches!(unsigned.how, How::Command { token: None, .. }));
        let signed = signing(
            std::rc::Rc::new(|a: &[Vec<String>], i: bool| format!("{}:{i}", a.len())),
            || person(vec![s(&["npm", "install"])]),
        );
        assert!(matches!(signed.how, How::Command { token: Some(t), .. } if t == "1:false"));
        // Reset once done.
        assert!(matches!(
            person(vec![s(&["x"])]).how,
            How::Command { token: None, .. }
        ));
    }

    #[test]
    fn what_cannot_run_here_says_why() {
        assert!(matches!(
            h("export ANTHROPIC_API_KEY=…"),
            How::Shell { setting: Some(s), .. } if s == "model"
        ));
        assert!(matches!(
            h("export SPECCODER_STATE_DIR=\"$PWD/.speccoder-state\""),
            How::Shell { setting: None, .. }
        ));
        for line in [
            "→ re-run with -v and report the output",
            "set [sandbox] mode = \"off\"",
            "curl https://example.com | sh",
            "sudo apt-get install bubblewrap",
            "git remote add origin <url>",
            "git diff -- <unit>/review.json",
            "echo $HOME",
            "speccoder eval --keep <dir>",
            "speccoder pr --remote <name>",
        ] {
            assert!(h(line).copy_only(), "{line}: {:?}", h(line));
        }
    }
}
