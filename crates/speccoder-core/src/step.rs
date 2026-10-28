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
    /// A command speccoder printed as a fix and can run as the person, in
    /// the project folder, outside the sandbox — never one composed
    /// elsewhere. Several argvs run in order, each only after the last
    /// one succeeded (`a && b`).
    Command {
        /// The argvs, exactly.
        argvs: Vec<Vec<String>>,
        /// It reads the keyboard (a login): a terminal of the person's
        /// own, not a panel.
        interactive: bool,
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

/// Commands that read the keyboard: a login's prompt or device flow.
const INTERACTIVE: &[&[&str]] = &[
    &["copilot", "login"],
    &["opencode", "auth", "login"],
    &["gh", "auth", "login"],
    &["aws", "login"],
    &["aws", "sso", "login"],
    &["aws", "configure"],
    &["claude", "login"],
    &["claude", "/login"],
    &["codex", "login"],
    &["gemini"],
    &["speccoder", "tui"],
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

/// How `line` is taken, with `ctx` filling what it leaves open.
#[must_use]
pub fn step_of(line: &str, ctx: Context<'_>) -> Step {
    let filled = fill(line, ctx);
    Step {
        command: line.to_owned(),
        how: how(&filled),
    }
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

fn how(line: &str) -> How {
    let line = line.trim();
    if line.starts_with('→') || line.is_empty() {
        return words(None);
    }
    // A trailing "(why)" is said, not run.
    let line = match line.rfind(" (") {
        Some(at) if line.ends_with(')') => line[..at].trim_end(),
        _ => line,
    };
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
    if first.first().map(String::as_str) == Some("speccoder") {
        let known = first
            .get(1)
            .is_some_and(|w| SUBCOMMANDS.contains(&w.as_str()) || w.starts_with('{'));
        return if known && chain.len() == 1 {
            speccoder(first)
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
            .is_none_or(|p| p != "speccoder" && !PROGRAMS.contains(&p.as_str()))
    }) {
        return words(None);
    }
    if chain.iter().flatten().any(|w| has_blank(w)) {
        return words(Some("it has a part only you can fill in"));
    }
    command(chain)
}

fn has_blank(w: &str) -> bool {
    w.contains(BLANK) || (w.contains('<') && w.contains('>')) || w.contains('{')
}

fn command(argvs: Vec<Vec<String>>) -> How {
    let interactive = argvs.iter().any(|a| {
        INTERACTIVE
            .iter()
            .any(|i| a.len() >= i.len() && a.iter().zip(i.iter()).all(|(w, p)| w == p))
    });
    How::Command {
        argvs,
        interactive,
        token: None,
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
            (None, '&') if chars.peek() == Some(&'&') => {
                chars.next();
                if in_word {
                    out.last_mut()?.push(std::mem::take(&mut word));
                    in_word = false;
                }
                out.push(Vec::new());
            }
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
fn speccoder(argv: &[String]) -> How {
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
    let run_cmd = || {
        if a.iter().any(|w| has_blank(w)) {
            words(Some("it has a part only you can fill in"))
        } else {
            command(vec![argv.to_vec()])
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
                    "--task" | "--feature" => plain &= rest.next().is_some(),
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
                    },
                    a,
                ),
                None => perform(Runnable::Run { task: None }, a),
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
        // Another project's: as written, at a terminal of the person's own
        // (a host serves on until it is stopped).
        (Some(_), _) if has(a, "-C") => How::Command {
            argvs: vec![argv.to_vec()],
            interactive: true,
            token: None,
        },
        (Some("spec"), Some("archive")) => How::Command {
            argvs: vec![argv.to_vec()],
            // Without `--yes` it asks first.
            interactive: !has(a, "--yes"),
            token: None,
        },
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
        (Some("config"), None) => view(View::Settings, None),
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
    fix.iter().map(|f| step_of(f, ctx)).collect()
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
        assert_eq!(run_of("speccoder run").0, Runnable::Run { task: None });
        assert_eq!(
            run_of("speccoder run --task {task}").0,
            Runnable::Run { task: Some(t()) }
        );
        assert_eq!(
            run_of("speccoder run --feature {feature} -v").0,
            Runnable::Run { task: None }
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
    fn what_has_no_runnable_runs_as_written() {
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
        assert_eq!(
            argvs_of("npm install"),
            (vec![s(&["npm", "install"])], false)
        );
        assert_eq!(
            argvs_of("git add speccoder.toml && git commit -m \"speccoder: config\"").0,
            vec![
                s(&["git", "add", "speccoder.toml"]),
                s(&["git", "commit", "-m", "speccoder: config"])
            ]
        );
        assert_eq!(
            argvs_of("uv sync (the lockfile's own)").0,
            vec![s(&["uv", "sync"])]
        );
        assert!(argvs_of("copilot login").1);
        assert!(argvs_of("opencode auth login").1);
        assert!(argvs_of("gh auth login").1);
        assert!(argvs_of("aws sso login --profile dev").1);
        assert!(!argvs_of("brew install gh").1);
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
