//! The person's editor, one click away: which editors this
//! machine has, which one is chosen, and opening a project file at a line
//! in it — by the editor's own launcher, an argument list, never a shell.
//!
//! A path opens in the feature's worktree when it has the file there (the
//! version the person looked at), else in the checkout; it must be
//! relative, stay inside, and exist.

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use speccoder_core::editor::{EditorEntry, EditorsReport, Opened, OpenedIn};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::git::Git;
use crate::user_config::EditorChoice;

/// How an editor's launcher takes a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// `code -g file:line:col`.
    Goto,
    /// `zed file:line:col`.
    Colon,
    /// `idea --line L --column C file`.
    JetBrains,
    /// `vim +L file`.
    Plus,
}

struct Known {
    id: &'static str,
    name: &'static str,
    launchers: &'static [&'static str],
    /// macOS app bundles' launchers, under `/Applications` or
    /// `~/Applications`.
    bundles: &'static [&'static str],
    style: Style,
    terminal: bool,
}

const fn gui(
    id: &'static str,
    name: &'static str,
    launchers: &'static [&'static str],
    bundles: &'static [&'static str],
    style: Style,
) -> Known {
    Known {
        id,
        name,
        launchers,
        bundles,
        style,
        terminal: false,
    }
}

const fn tty(id: &'static str, name: &'static str, style: Style) -> Known {
    Known {
        id,
        name,
        launchers: &[],
        bundles: &[],
        style,
        terminal: true,
    }
}

/// Every editor speccoder knows, window editors first.
const KNOWN: &[Known] = &[
    gui(
        "code",
        "Visual Studio Code",
        &["code"],
        &["Visual Studio Code.app/Contents/Resources/app/bin/code"],
        Style::Goto,
    ),
    gui(
        "cursor",
        "Cursor",
        &["cursor"],
        &["Cursor.app/Contents/Resources/app/bin/cursor"],
        Style::Goto,
    ),
    gui(
        "zed",
        "Zed",
        &["zed", "zeditor"],
        &["Zed.app/Contents/MacOS/cli"],
        Style::Colon,
    ),
    gui(
        "windsurf",
        "Windsurf",
        &["windsurf"],
        &["Windsurf.app/Contents/Resources/app/bin/windsurf"],
        Style::Goto,
    ),
    gui(
        "code-insiders",
        "Visual Studio Code Insiders",
        &["code-insiders"],
        &["Visual Studio Code - Insiders.app/Contents/Resources/app/bin/code"],
        Style::Goto,
    ),
    gui(
        "subl",
        "Sublime Text",
        &["subl"],
        &["Sublime Text.app/Contents/SharedSupport/bin/subl"],
        Style::Colon,
    ),
    gui(
        "idea",
        "IntelliJ IDEA",
        &["idea"],
        &[
            "IntelliJ IDEA.app/Contents/MacOS/idea",
            "IntelliJ IDEA CE.app/Contents/MacOS/idea",
        ],
        Style::JetBrains,
    ),
    gui(
        "rustrover",
        "RustRover",
        &["rustrover"],
        &["RustRover.app/Contents/MacOS/rustrover"],
        Style::JetBrains,
    ),
    gui(
        "pycharm",
        "PyCharm",
        &["pycharm", "charm"],
        &[
            "PyCharm.app/Contents/MacOS/pycharm",
            "PyCharm CE.app/Contents/MacOS/pycharm",
        ],
        Style::JetBrains,
    ),
    gui(
        "webstorm",
        "WebStorm",
        &["webstorm"],
        &["WebStorm.app/Contents/MacOS/webstorm"],
        Style::JetBrains,
    ),
    gui(
        "goland",
        "GoLand",
        &["goland"],
        &["GoLand.app/Contents/MacOS/goland"],
        Style::JetBrains,
    ),
    gui(
        "clion",
        "CLion",
        &["clion"],
        &["CLion.app/Contents/MacOS/clion"],
        Style::JetBrains,
    ),
    gui(
        "phpstorm",
        "PhpStorm",
        &["phpstorm"],
        &["PhpStorm.app/Contents/MacOS/phpstorm"],
        Style::JetBrains,
    ),
    gui(
        "rubymine",
        "RubyMine",
        &["rubymine"],
        &["RubyMine.app/Contents/MacOS/rubymine"],
        Style::JetBrains,
    ),
    tty("nvim", "Neovim", Style::Plus),
    tty("vim", "Vim", Style::Plus),
    tty("vi", "vi", Style::Plus),
    tty("hx", "Helix", Style::Colon),
    tty("micro", "micro", Style::Plus),
    tty("nano", "nano", Style::Plus),
    tty("emacs", "Emacs", Style::Plus),
];

fn known(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// What detection reads: the environment's editor variables, the
/// directories on `PATH`, and where app bundles live.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `$VISUAL`.
    pub visual: Option<String>,
    /// `$EDITOR`.
    pub editor: Option<String>,
    /// `PATH`'s directories.
    pub path: Vec<PathBuf>,
    /// Folders holding app bundles (`/Applications`, `~/Applications`).
    pub apps: Vec<PathBuf>,
}

impl Env {
    /// This process's.
    #[must_use]
    pub fn current() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let mut apps = Vec::new();
        if cfg!(target_os = "macos") {
            apps.push(PathBuf::from("/Applications"));
            if let Some(h) = std::env::var_os("HOME") {
                apps.push(PathBuf::from(h).join("Applications"));
            }
        }
        Self {
            visual: var("VISUAL"),
            editor: var("EDITOR"),
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            apps,
        }
    }

    /// A launcher of `k` on this machine.
    fn find(&self, k: &Known) -> Option<PathBuf> {
        let launchers: Vec<&str> = if k.terminal {
            vec![k.id]
        } else {
            k.launchers.to_vec()
        };
        launchers.iter().find_map(|l| self.on_path(l)).or_else(|| {
            k.bundles
                .iter()
                .flat_map(|b| self.apps.iter().map(move |d| d.join(b)))
                .find(|p| executable(p))
        })
    }

    /// `program` on `PATH`: absolute directories only (an empty or `.`
    /// entry would find a program planted in the repository), executable.
    fn on_path(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            let p = PathBuf::from(program);
            return (p.is_absolute() && executable(&p)).then_some(p);
        }
        self.path
            .iter()
            .filter(|d| d.is_absolute())
            .map(|d| d.join(program))
            .find(|p| executable(p))
    }

    /// The known editor `$VISUAL`, then `$EDITOR`, names (`code --wait`
    /// is `code`).
    fn named(&self) -> Option<&'static Known> {
        [&self.visual, &self.editor]
            .into_iter()
            .flatten()
            .find_map(|v| {
                let program = v.split_whitespace().next()?;
                let base = Path::new(program).file_name()?.to_str()?;
                KNOWN
                    .iter()
                    .find(|k| k.id == base || k.launchers.contains(&base))
            })
    }
}

/// A file that can be run.
fn executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Shells: a command of the person's own may not be one, or a file named
/// `$(…)` in a repository would become shell text.
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "nu",
    "pwsh",
    "powershell",
    "cmd",
    "env",
];

/// The editors, the person's choice and the detected one.
#[must_use]
pub fn editors(choice: Option<&EditorChoice>, env: &Env) -> EditorsReport {
    let mut list: Vec<EditorEntry> = KNOWN
        .iter()
        .map(|k| EditorEntry {
            id: k.id.to_owned(),
            name: k.name.to_owned(),
            terminal: k.terminal,
            found: env.find(k).map(|p| p.display().to_string()),
        })
        .collect();
    // Window editors first, found ones first; the table's order otherwise.
    list.sort_by_key(|e| (e.terminal, e.found.is_none()));
    EditorsReport {
        chosen: choice.map(|c| match c {
            EditorChoice::Named(id) => id.clone(),
            EditorChoice::Command(_) => "custom".to_owned(),
        }),
        detected: env.named().map(|k| k.id.to_owned()),
        editors: list,
    }
}

/// The editor a file opens in: its name, launcher and how it takes a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    name: String,
    program: PathBuf,
    how: How,
    terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum How {
    Known(Style),
    /// The person's own argv after the program, with placeholders.
    Template(Vec<String>),
}

/// Why no editor: `EDITOR-NOT-FOUND`'s words.
fn missing(why: impl Into<String>) -> CommandResult {
    CommandResult::new("open", CommandStatus::NeedsHuman, &why.into()).with_code("EDITOR-NOT-FOUND")
}

/// The editor to use: `[ui] editor`, else the one `$VISUAL`/`$EDITOR`
/// names; `terminal`: a terminal editor may be used.
///
/// # Errors
///
/// Nothing chosen or detected, a chosen editor not found, or a terminal
/// editor where none may run.
pub fn resolve_editor(
    choice: Option<&EditorChoice>,
    env: &Env,
    terminal: bool,
) -> Result<Resolved, Box<CommandResult>> {
    let k = match choice {
        Some(EditorChoice::Command(argv)) => {
            let base = Path::new(&argv[0])
                .file_name()
                .and_then(|b| b.to_str())
                .unwrap_or("");
            if SHELLS.contains(&base) {
                return Err(Box::new(missing(format!(
                    "[ui] editor runs {base}: name the editor itself, with {{file}} as its own argument"
                ))));
            }
            let program = env.on_path(&argv[0]).ok_or_else(|| {
                Box::new(missing(format!("[ui] editor: `{}` is not found", argv[0])))
            })?;
            return Ok(Resolved {
                name: base.to_owned(),
                program,
                how: How::Template(argv[1..].to_vec()),
                terminal: false,
            });
        }
        Some(EditorChoice::Named(id)) => known(id).ok_or_else(|| {
            Box::new(missing(format!(
                "[ui] editor = \"{id}\" is not an editor speccoder knows"
            )))
        })?,
        None => match env.named() {
            Some(k) => k,
            // In a terminal, the editor's convention: `$VISUAL`, `$EDITOR`
            // as they are, else the first window editor found.
            None if terminal => {
                if let Some(v) = env.visual.as_ref().or(env.editor.as_ref())
                    && let Some(program) = v.split_whitespace().next().and_then(|p| env.on_path(p))
                {
                    let mut argv: Vec<String> =
                        v.split_whitespace().skip(1).map(str::to_owned).collect();
                    argv.push("{file}".to_owned());
                    return Ok(Resolved {
                        name: program
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                        program,
                        how: How::Template(argv),
                        terminal: true,
                    });
                }
                KNOWN
                    .iter()
                    .find(|k| !k.terminal && env.find(k).is_some())
                    .ok_or_else(|| Box::new(missing("no editor is chosen and none was found: `speccoder open --choose <editor>`, or set VISUAL")))?
            }
            None => {
                return Err(Box::new(missing(
                    "no editor is chosen: choose one, or set VISUAL",
                )));
            }
        },
    };
    if k.terminal && !terminal {
        return Err(Box::new(missing(format!(
            "{} needs a terminal: choose a window editor here, or use `speccoder open` in a terminal",
            k.name
        ))));
    }
    let program = env.find(k).ok_or_else(|| {
        Box::new(missing(format!(
            "{} is not installed here (no `{}` found)",
            k.name,
            k.launchers.first().copied().unwrap_or(k.id)
        )))
    })?;
    Ok(Resolved {
        name: k.name.to_owned(),
        program,
        how: How::Known(k.style),
        terminal: k.terminal,
    })
}

impl Resolved {
    /// The launcher's arguments for `file` at `line`, `column`.
    fn args(&self, file: &Path, line: Option<u32>, column: Option<u32>) -> Vec<String> {
        let f = file.display().to_string();
        let at = |sep: &str| match (line, column) {
            (Some(l), Some(c)) => format!("{f}{sep}{l}{sep}{c}"),
            (Some(l), None) => format!("{f}{sep}{l}"),
            _ => f.clone(),
        };
        match &self.how {
            How::Known(Style::Goto) => vec!["-g".to_owned(), at(":")],
            How::Known(Style::Colon) => vec![at(":")],
            How::Known(Style::JetBrains) => {
                let mut a = Vec::new();
                if let Some(l) = line {
                    a.extend(["--line".to_owned(), l.to_string()]);
                    if let Some(c) = column {
                        a.extend(["--column".to_owned(), c.to_string()]);
                    }
                }
                a.push(f);
                a
            }
            How::Known(Style::Plus) => match line {
                Some(l) => vec![format!("+{l}"), f],
                None => vec![f],
            },
            How::Template(t) => t
                .iter()
                .map(|a| fill(a, &f, line.unwrap_or(1), column.unwrap_or(1)))
                .collect(),
        }
    }
}

/// `a` with `{file}`, `{line}`, `{column}` filled in one pass: a file
/// named `{line}.rs` stays its name.
fn fill(a: &str, file: &str, line: u32, column: u32) -> String {
    let mut out = String::new();
    let mut rest = a;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let (put, skip) = if tail.starts_with("{file}") {
            (file.to_owned(), 6)
        } else if tail.starts_with("{line}") {
            (line.to_string(), 6)
        } else if tail.starts_with("{column}") {
            (column.to_string(), 8)
        } else {
            ("{".to_owned(), 1)
        };
        out.push_str(&put);
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out
}

/// A project-relative path that stays inside: no root, no `..`, no
/// prefix. `src/./a.rs` is `src/a.rs`.
fn relative(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in Path::new(path.trim()).components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// The worktree holding `speccoder/<feature>`, when there is one.
fn worktree_of(top: &Path, feature: &str) -> Option<PathBuf> {
    let out = Git::at(top)
        .run(&["worktree", "list", "--porcelain"])
        .ok()?;
    let want = format!("branch refs/heads/speccoder/{feature}");
    let text = String::from_utf8_lossy(&out);
    let mut current: Option<&str> = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            current = Some(p);
        } else if line == want {
            return current.map(PathBuf::from);
        }
    }
    None
}

/// Where `path` opens: the feature's worktree when it has the file, else
/// the checkout at `top`. The file exists and is inside, links resolved.
///
/// # Errors
///
/// `OPEN-REFUSED` with why.
pub fn locate(
    top: &Path,
    feature: Option<&str>,
    path: &str,
) -> Result<(PathBuf, OpenedIn, String), Box<CommandResult>> {
    let refuse = |why: String| {
        Box::new(
            CommandResult::new("open", CommandStatus::UsageError, &why).with_code("OPEN-REFUSED"),
        )
    };
    let mut places = Vec::new();
    if let Some(w) = feature.and_then(|f| worktree_of(top, f)) {
        places.push((w, OpenedIn::Worktree));
    }
    places.push((top.to_path_buf(), OpenedIn::Checkout));
    // An absolute path (a thread's read) inside one of them is taken as
    // relative to it.
    let given = Path::new(path.trim());
    let path_rel = if given.is_absolute() {
        let real = std::fs::canonicalize(given).ok();
        places
            .iter()
            .find_map(|(root, _)| {
                let base = std::fs::canonicalize(root).ok()?;
                real.as_ref()?
                    .strip_prefix(&base)
                    .ok()
                    .map(|r| r.display().to_string())
            })
            .ok_or_else(|| refuse(format!("{path} is not inside the project")))?
    } else {
        path.to_owned()
    };
    let rel = relative(&path_rel)
        .ok_or_else(|| refuse(format!("{path} is not a path inside the project")))?;
    if rel
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == ".git")
    {
        return Err(refuse(format!("{path} is git's own, not a project file")));
    }
    for (root, place) in places {
        let Ok(base) = std::fs::canonicalize(&root) else {
            continue;
        };
        let Ok(file) = std::fs::canonicalize(base.join(&rel)) else {
            continue;
        };
        if !file.starts_with(&base) {
            return Err(refuse(format!("{path} leads outside the project")));
        }
        if file.is_file() {
            return Ok((file, place, rel.display().to_string()));
        }
    }
    Err(refuse(match feature {
        Some(f) => format!("{path} is not in speccoder/{f} or in your checkout"),
        None => format!("{path} does not exist"),
    }))
}

/// The line, kept within the file: past its end is its last line, 0 is
/// none.
fn clamp(file: &Path, line: Option<u32>) -> Option<u32> {
    use std::io::Read;
    let line = line.filter(|l| *l > 0)?;
    // Counted as far as the line asked for, never the whole of a huge file.
    let Ok(f) = std::fs::File::open(file) else {
        return Some(line);
    };
    let (mut count, mut last, mut buf) = (0u32, b'\n', [0u8; 64 * 1024]);
    let mut r = std::io::BufReader::new(f);
    while count < line {
        let Ok(n) = r.read(&mut buf) else { break };
        if n == 0 {
            break;
        }
        count = count.saturating_add(
            u32::try_from(buf[..n].iter().filter(|c| **c == b'\n').count()).unwrap_or(u32::MAX),
        );
        last = buf[n - 1];
    }
    if count < line && last != b'\n' {
        count = count.saturating_add(1);
    }
    Some(line.min(count.max(1)))
}

/// Opens `path` at `line`/`column` with `editor`; a window editor is
/// started and left running, a terminal editor runs in the foreground.
#[must_use]
pub fn open_with(
    editor: &Resolved,
    top: &Path,
    feature: Option<&str>,
    path: &str,
    line: Option<u32>,
    column: Option<u32>,
) -> CommandResult {
    let (file, place, rel) = match locate(top, feature, path) {
        Ok(found) => found,
        Err(r) => return *r,
    };
    let line = clamp(&file, line);
    let column = column.filter(|c| *c > 0 && line.is_some());
    let mut cmd = Command::new(&editor.program);
    cmd.args(editor.args(&file, line, column))
        .current_dir(file.parent().unwrap_or(top));
    let failed = |why: String| missing(format!("{} did not open {rel}: {why}", editor.name));
    if editor.terminal {
        match cmd.status() {
            Ok(s) if s.success() => {}
            Ok(s) => return failed(format!("it exited with {s}")),
            Err(e) => return failed(e.to_string()),
        }
    } else {
        // Its own process group: a ctrl-c of speccoder's host is not the
        // editor's. Its error output to a file, read only on failure.
        let err_file = tempfile::tempfile().ok();
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(
            err_file
                .as_ref()
                .and_then(|f| f.try_clone().ok())
                .map_or_else(Stdio::null, Stdio::from),
        );
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return failed(e.to_string()),
        };
        // A launcher hands the file to the running editor and exits; one
        // that fails says so at once. Still running after a moment: the
        // editor itself, left to run.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            match child.try_wait() {
                Ok(Some(s)) if s.success() => break,
                Ok(Some(s)) => {
                    let mut err = String::new();
                    if let Some(mut f) = err_file {
                        use std::io::{Read, Seek};
                        let _ = f.rewind();
                        let _ = f.take(4096).read_to_string(&mut err);
                    }
                    let why = err
                        .lines()
                        .next()
                        .map_or_else(|| format!("it exited with {s}"), str::to_owned);
                    return failed(why);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(40)),
                Err(e) => return failed(e.to_string()),
            }
        }
        // Reaped whenever it ends, so it never lingers as a zombie.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    let at = line.map_or_else(String::new, |l| format!(":{l}"));
    CommandResult::new(
        "open",
        CommandStatus::Ok,
        &format!("opened {rel}{at} in {}", editor.name),
    )
    .with_data(CommandData::Opened(Box::new(Opened {
        editor: editor.name.clone(),
        path: rel,
        line,
        column,
        location: place,
    })))
}

/// Opens `path` in the person's editor; `terminal`: from a terminal, where
/// a terminal editor may run.
#[must_use]
pub fn open(
    at: &Path,
    feature: Option<&str>,
    path: &str,
    line: Option<u32>,
    column: Option<u32>,
    terminal: bool,
) -> CommandResult {
    let Ok(Some(top)) = Git::at(at).toplevel() else {
        return CommandResult::new(
            "open",
            CommandStatus::GitPrecondition,
            "not a git repository",
        )
        .with_code("NO-REPOSITORY");
    };
    let (settings, _) = crate::user_config::load();
    match resolve_editor(settings.ui.editor.as_ref(), &Env::current(), terminal) {
        Ok(e) => open_with(&e, &top, feature, path, line, column),
        Err(r) => *r,
    }
}

/// The editors listing for this person on this machine.
#[must_use]
pub fn report() -> CommandResult {
    let (settings, _) = crate::user_config::load();
    let r = editors(settings.ui.editor.as_ref(), &Env::current());
    let message = match (&r.chosen, &r.detected) {
        (Some(c), _) => format!("files open in {c}"),
        (None, Some(d)) => format!("files open in {d} (from VISUAL or EDITOR)"),
        (None, None) => "no editor chosen".to_owned(),
    };
    CommandResult::new("open", CommandStatus::Ok, &message)
        .with_data(CommandData::Editors(Box::new(r)))
}

/// Saves `id` as the person's editor.
#[must_use]
pub fn choose(id: &str) -> CommandResult {
    if known(id).is_none() {
        return CommandResult::new(
            "open",
            CommandStatus::UsageError,
            &format!(
                "{id} is not an editor speccoder knows (one of {})",
                KNOWN.iter().map(|k| k.id).collect::<Vec<_>>().join(", ")
            ),
        );
    }
    match crate::user_config::set_editor(id) {
        Ok(_) => report(),
        Err(e) => CommandResult::new("open", CommandStatus::Error, &e),
    }
}

/// `src/lib.rs:42:7` → the path, line and column.
#[must_use]
pub fn split_location(spec: &str) -> (String, Option<u32>, Option<u32>) {
    let mut parts: Vec<&str> = spec.rsplitn(3, ':').collect();
    parts.reverse();
    let num = |s: &str| s.parse::<u32>().ok();
    match parts.as_slice() {
        [p, l, c] if num(l).is_some() && num(c).is_some() => ((*p).to_owned(), num(l), num(c)),
        [p, l, c] if num(c).is_some() => (format!("{p}:{l}"), num(c), None),
        [p, l] if num(l).is_some() => ((*p).to_owned(), num(l), None),
        _ => (spec.to_owned(), None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_executable(dir: &Path, names: &[&str]) {
        #[cfg(unix)]
        for n in names {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(n), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn env_with(dir: &Path, names: &[&str]) -> Env {
        for n in names {
            std::fs::write(dir.join(n), "").unwrap();
        }
        Env {
            path: vec![dir.to_path_buf()],
            ..Env::default()
        }
    }

    #[test]
    fn found_editors_come_first_and_visual_names_one() {
        let d = tempfile::tempdir().unwrap();
        let mut env = env_with(d.path(), &["cursor", "nvim"]);
        make_executable(d.path(), &["cursor", "nvim"]);
        env.visual = Some("/usr/local/bin/code --wait".into());
        let r = editors(None, &env);
        assert_eq!(r.editors[0].id, "cursor");
        assert!(r.editors[0].found.is_some());
        assert_eq!(r.detected.as_deref(), Some("code"));
        // Terminal editors after every window editor.
        let first_tty = r.editors.iter().position(|e| e.terminal).unwrap();
        assert!(r.editors[..first_tty].iter().all(|e| !e.terminal));
    }

    #[test]
    fn each_style_puts_the_line_where_its_launcher_wants_it() {
        let f = Path::new("/p/a b.rs");
        let r = |how| Resolved {
            name: "x".into(),
            program: "x".into(),
            how,
            terminal: false,
        };
        assert_eq!(
            r(How::Known(Style::Goto)).args(f, Some(4), Some(2)),
            ["-g", "/p/a b.rs:4:2"]
        );
        assert_eq!(
            r(How::Known(Style::Colon)).args(f, Some(4), None),
            ["/p/a b.rs:4"]
        );
        assert_eq!(
            r(How::Known(Style::JetBrains)).args(f, Some(4), Some(2)),
            ["--line", "4", "--column", "2", "/p/a b.rs"]
        );
        assert_eq!(
            r(How::Known(Style::Plus)).args(f, Some(4), None),
            ["+4", "/p/a b.rs"]
        );
        assert_eq!(
            r(How::Known(Style::Plus)).args(f, None, None),
            ["/p/a b.rs"]
        );
        assert_eq!(
            r(How::Template(vec!["{file}@{line}".into()])).args(f, None, None),
            ["/p/a b.rs@1"]
        );
    }

    #[test]
    fn the_choice_wins_and_a_terminal_editor_needs_a_terminal() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with(d.path(), &["zed", "nvim"]);
        make_executable(d.path(), &["zed", "nvim"]);
        let zed = resolve_editor(Some(&EditorChoice::Named("zed".into())), &env, false).unwrap();
        assert_eq!(zed.name, "Zed");
        let nvim = Some(EditorChoice::Named("nvim".into()));
        let refused = resolve_editor(nvim.as_ref(), &env, false).unwrap_err();
        assert_eq!(refused.code().map(|c| c.name), Some("EDITOR-NOT-FOUND"));
        assert!(resolve_editor(nvim.as_ref(), &env, true).is_ok());
        let none = resolve_editor(None, &env, false).unwrap_err();
        assert!(none.message().contains("no editor is chosen"));
        // A terminal takes the first window editor found, or VISUAL as is.
        assert_eq!(resolve_editor(None, &env, true).unwrap().name, "Zed");
        std::fs::write(d.path().join("myed"), "").unwrap();
        make_executable(d.path(), &["myed"]);
        let mut own = env.clone();
        own.visual = Some("myed -w".into());
        let r = resolve_editor(None, &own, true).unwrap();
        assert_eq!(r.args(Path::new("/a"), Some(2), None), ["-w", "/a"]);
        let gone =
            resolve_editor(Some(&EditorChoice::Named("cursor".into())), &env, false).unwrap_err();
        assert!(
            gone.message().contains("not installed"),
            "{}",
            gone.message()
        );
    }

    #[test]
    fn only_a_file_inside_the_project_is_located() {
        let d = tempfile::tempdir().unwrap();
        let top = d.path();
        std::fs::create_dir_all(top.join("src")).unwrap();
        std::fs::write(top.join("src/a.rs"), "1\n2\n3\n").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret"), top.join("link")).unwrap();
        let (file, place, rel) = locate(top, None, "./src/a.rs").unwrap();
        assert!(file.ends_with("src/a.rs") && place == OpenedIn::Checkout && rel == "src/a.rs");
        for bad in ["../x", "/etc/passwd", "", "src/../../x", "missing.rs"] {
            let e = locate(top, None, bad).unwrap_err();
            assert_eq!(e.code().map(|c| c.name), Some("OPEN-REFUSED"), "{bad}");
        }
        #[cfg(unix)]
        assert!(
            locate(top, None, "link")
                .unwrap_err()
                .message()
                .contains("outside")
        );
        assert_eq!(clamp(&file, Some(9)), Some(3));
        assert_eq!(clamp(&file, Some(0)), None);
    }

    #[test]
    fn a_location_splits_into_path_line_and_column() {
        assert_eq!(
            split_location("a.rs:4:2"),
            ("a.rs".into(), Some(4), Some(2))
        );
        assert_eq!(split_location("a.rs:4"), ("a.rs".into(), Some(4), None));
        assert_eq!(split_location("a.rs"), ("a.rs".into(), None, None));
        assert_eq!(
            split_location("dir:x/a.rs:7"),
            ("dir:x/a.rs".into(), Some(7), None)
        );
    }

    #[test]
    fn a_command_of_ones_own_is_no_shell_and_fills_once() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with(d.path(), &["myed"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                d.path().join("myed"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        let sh = EditorChoice::Command(vec!["/bin/sh".into(), "-c".into(), "vim {file}".into()]);
        assert!(
            resolve_editor(Some(&sh), &env, false)
                .unwrap_err()
                .message()
                .contains("runs sh")
        );
        let own = EditorChoice::Command(vec!["myed".into(), "{file}:{line}".into()]);
        let r = resolve_editor(Some(&own), &env, false).unwrap();
        assert!(r.program.is_absolute());
        assert_eq!(
            r.args(Path::new("/p/{line}.rs"), Some(3), None),
            ["/p/{line}.rs:3"]
        );
        // A relative PATH entry finds nothing.
        let rel = Env {
            path: vec![PathBuf::from(".")],
            ..Env::default()
        };
        assert!(rel.on_path("myed").is_none());
    }

    #[test]
    fn an_absolute_path_inside_is_taken_and_gits_own_files_are_not() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".git")).unwrap();
        std::fs::write(d.path().join(".git/config"), "x").unwrap();
        std::fs::write(d.path().join("a.rs"), "x").unwrap();
        let abs = d.path().join("a.rs").display().to_string();
        assert_eq!(locate(d.path(), None, &abs).unwrap().2, "a.rs");
        assert!(
            locate(d.path(), None, ".git/config")
                .unwrap_err()
                .message()
                .contains("git's own")
        );
    }
}
