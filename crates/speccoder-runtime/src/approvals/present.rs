//! What a person sees when a call needs them: built live from the
//! journaled [`Request`] and what only the live run knows — the file as it
//! is now, the sandbox — and never journaled itself, so replay does not
//! depend on the worktree. The terminal prompt, editors (ACP), the
//! `approvals` listing and the dashboard all render this one value.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Request;
use crate::tools::policy::Object;

/// Lines of a body shown before "v to view all".
pub const HEAD: usize = 20;

/// Files larger than this are described, not shown.
const MAX_BYTES: usize = 1024 * 1024;

/// What only the live run knows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentContext {
    /// The worktree, to read the file a write or edit changes.
    pub root: Option<PathBuf>,
    /// The sandbox mechanism (`Seatbelt`, `Landlock+seccomp`); `None`: off.
    pub sandbox: Option<String>,
    /// What a command's network is without asking (`none`, `loopback`).
    pub network: String,
    /// The file's text, given instead of read from `root` (a saved attempt's).
    pub file_text: Option<String>,
}

/// One choice: `once`, `task`, `run`, `always`, `reject` or `stop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// Its name.
    pub choice: String,
    /// Its key (`y`, `t`, `r`, `a`, `n`, `s`).
    pub key: char,
    /// Its number, fixed whether or not the others are offered.
    pub number: u8,
    /// What it does, the scope included.
    pub label: String,
}

/// The call as a person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    /// The question.
    pub request: Request,
    /// `Write this file?`, `Edit this file?`, `Run this command?`,
    /// `Use the network?`, `Call MCP tool?`.
    pub title: String,
    /// The path, the whole command, or the MCP tool — never cut.
    pub subject: String,
    /// The object a decision covers, as lines: the file (and the name as
    /// asked when it differs), the argv with its hash and the executable,
    /// the tool with its digest; and what changed since an approval of the
    /// same name, when something did.
    pub object: Vec<String>,
    /// Where arguments of the call were copied from: `contains text read
    /// from README.md (turn 3): "…"`, then `and N more`.
    pub sources: Vec<String>,
    /// Said once, on the prompt that uses the run's asking budget.
    pub notice: Option<String>,
    /// The body as shown: a diff, a file's head, the command, arguments.
    pub body: Vec<String>,
    /// The whole body (`v`).
    pub full: Vec<String>,
    /// What the call can reach.
    pub sandbox: String,
    /// The choices, in order.
    pub options: Vec<Choice>,
    /// What an approval for the task or run covers (`this path, any content`).
    pub scope: String,
    /// The file's text before and after, for editors that show a diff.
    pub old_text: Option<String>,
    /// The file's text after the call.
    pub new_text: Option<String>,
    /// The file's absolute path, for a write or edit in a known worktree.
    pub file: Option<String>,
}

/// Model text made safe for a terminal: escape (`\x1b`) as `␛`, other
/// control characters as `^X`; newlines and tabs kept.
#[must_use]
pub fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\x1b' => "␛".to_owned(),
            '\n' | '\t' => c.to_string(),
            c if (c as u32) < 0x20 => format!("^{}", char::from(b'@' + c as u8)),
            '\x7f' => "^?".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// A word an argv element needs no quotes for.
fn plain(a: &str) -> bool {
    !a.is_empty()
        && a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c))
}

/// The argv as a person would type it.
#[must_use]
pub fn shell_quote(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if plain(a) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What "for the task" or "for this run" covers, by kind: always this
/// exact object.
#[must_use]
pub fn scope_text(kind: &str) -> &'static str {
    match kind {
        "run" => "exactly this command in this directory",
        "network" => "exactly this command in this directory, with the network",
        "mcp" => "this tool as defined now, any arguments",
        "agent" => "this one call",
        _ => "this file, any content",
    }
}

fn first12(hex: &str) -> &str {
    hex.get(..12).unwrap_or(hex)
}

/// The object's lines: what a decision covers, shown in full.
#[must_use]
pub fn object_lines(object: Option<&Object>, changed: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    match object {
        Some(Object::Path { canonical, asked }) => {
            out.push(format!("file  {}", escape(canonical)));
            if asked != canonical {
                out.push(format!(
                    "asked as  {} (resolves to the file above)",
                    escape(asked)
                ));
            }
        }
        Some(Object::Command {
            argv,
            argv_sha256,
            cwd,
            executable,
            network,
        }) => {
            let json = serde_json::to_string(argv).unwrap_or_default();
            out.push(format!(
                "argv  {}  #{}",
                escape(&json),
                first12(argv_sha256)
            ));
            out.push(format!("runs  {}  in  {}", escape(executable), escape(cwd)));
            if *network {
                out.push("network  yes".to_owned());
            }
        }
        Some(Object::Tool { name, digest }) => {
            out.push(format!("tool  {}  @{}", escape(name), first12(digest)));
        }
        Some(Object::Agent {
            agent,
            call,
            argvs,
            paths,
            ..
        }) => {
            out.push(format!(
                "{} itself ({}), outside speccoder's sandbox",
                escape(agent),
                escape(call)
            ));
            for a in argvs {
                out.push(format!("runs  {}", escape(&shell_quote(a))));
            }
            for p in paths {
                out.push(format!("file  {}", escape(p)));
            }
        }
        None => {}
    }
    if let Some(c) = changed {
        out.push(format!("changed since approved: {}", escape(c)));
    }
    out
}

/// The source lines of a request.
#[must_use]
pub fn source_lines(request: &Request) -> Vec<String> {
    let mut out: Vec<String> = request
        .sources
        .iter()
        .map(|s| {
            let when = if s.turn == 0 {
                "the attempt's prompt".to_owned()
            } else {
                format!("turn {}", s.turn)
            };
            format!(
                "contains text read from {} ({when}): \"{}\"",
                escape(&s.from),
                escape(&s.argument)
            )
        })
        .collect();
    if request.more > 0 {
        out.push(format!("and {} more", request.more));
    }
    out
}

/// The notice on the prompt that uses the run's asking budget.
#[must_use]
pub fn budget_notice(request: &Request) -> Option<String> {
    (request.budget > 0 && request.asked == request.budget && !request.batched).then(|| {
        format!(
            "This is prompt {0} of {0} for this run; further calls will wait in Needs You.",
            request.budget
        )
    })
}

fn head(lines: &[String]) -> (Vec<String>, usize) {
    let shown = lines.iter().take(HEAD).cloned().collect();
    (shown, lines.len().saturating_sub(HEAD))
}

/// Numbered lines of a new file.
fn numbered(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .map(|(i, l)| format!("{:>5} │ {}", i + 1, escape(l)))
        .collect()
}

/// A unified diff with three lines of context and old/new line numbers.
#[must_use]
pub fn diff(old: &str, new: &str) -> Vec<String> {
    let d = similar::TextDiff::from_lines(old, new);
    let mut out = Vec::new();
    for (i, group) in d.grouped_ops(3).iter().enumerate() {
        if i > 0 {
            out.push("          ┈".to_owned());
        }
        for op in group {
            for change in d.iter_changes(op) {
                let (sign, old_no, new_no) = match change.tag() {
                    similar::ChangeTag::Equal => (' ', change.old_index(), change.new_index()),
                    similar::ChangeTag::Delete => ('-', change.old_index(), None),
                    similar::ChangeTag::Insert => ('+', None, change.new_index()),
                };
                let no = |n: Option<usize>| n.map_or(String::new(), |n| (n + 1).to_string());
                let line = format!(
                    "{:>5} {:>5} {sign} {}",
                    no(old_no),
                    no(new_no),
                    escape(change.value().trim_end_matches(['\n', '\r']))
                );
                out.push(line.trim_end().to_owned());
            }
        }
    }
    out
}

/// The text of `path` under `root`, or why it cannot be shown.
fn read(context: &PresentContext, path: &str) -> Result<Option<String>, String> {
    if let Some(text) = &context.file_text {
        return Ok(Some(text.clone()));
    }
    let Some(root) = context.root.as_deref() else {
        return Ok(None);
    };
    let file = root.join(path);
    let Ok(bytes) = std::fs::read(&file) else {
        return Ok(None);
    };
    if bytes.len() > MAX_BYTES {
        return Err(format!("{} bytes — too large to show", bytes.len()));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "a binary file — not shown".to_owned())
}

/// Builds what the person sees.
#[must_use]
pub fn present(request: &Request, context: &PresentContext) -> Presentation {
    let args = &request.arguments;
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    let path = s("path");
    let kind = request.kind.as_str();
    let mut old_text = None;
    let mut new_text = None;
    let (title, subject, full) = match kind {
        "run" | "network" => {
            let argv: Vec<String> = args["argv"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let cmd = escape(&shell_quote(&argv));
            let cwd = args["cwd"]
                .as_str()
                .filter(|c| !c.is_empty())
                .unwrap_or(".");
            let net = if kind == "network" {
                "this call".to_owned()
            } else {
                context.network.clone()
            };
            (
                if kind == "network" {
                    "Use the network?"
                } else {
                    "Run this command?"
                },
                cmd.clone(),
                vec![format!("$ {cmd}"), format!("cwd: {cwd} · network: {net}")],
            )
        }
        "mcp" => {
            let pretty = serde_json::to_string_pretty(args).unwrap_or_default();
            (
                "Call MCP tool?",
                request.tool.clone(),
                pretty.lines().map(escape).collect(),
            )
        }
        // An external agent's own call speccoder cannot classify: what the
        // agent said about it, as it said it.
        "agent" => {
            let pretty = serde_json::to_string_pretty(&args["rawInput"]).unwrap_or_default();
            let mut lines = vec![format!(
                "kind: {}",
                escape(args["kind"].as_str().unwrap_or("not said"))
            )];
            lines.extend(pretty.lines().map(escape));
            (
                "Let the agent make this call?",
                escape(args["title"].as_str().unwrap_or("an untitled call")),
                lines,
            )
        }
        _ => {
            let current = read(context, &path);
            // A write over a file that exists now is an edit of it.
            let title = if kind == "write" && matches!(current, Ok(None)) {
                "Write this file?"
            } else {
                "Edit this file?"
            };
            let lines = match current {
                Err(note) => vec![note],
                Ok(current) => match (request.tool.as_str(), current) {
                    ("write_file", None) => {
                        let content = s("content");
                        new_text = Some(content.clone());
                        numbered(&content)
                    }
                    ("write_file", Some(old)) => {
                        let content = s("content");
                        let d = diff(&old, &content);
                        old_text = Some(old);
                        new_text = Some(content);
                        d
                    }
                    (_, Some(old)) => {
                        let (from, to) = (s("old_string"), s("new_string"));
                        let all = args["replace_all"] == true;
                        if !from.is_empty() && old.contains(&from) {
                            let new = if all {
                                old.replace(&from, &to)
                            } else {
                                old.replacen(&from, &to, 1)
                            };
                            let d = diff(&old, &new);
                            old_text = Some(old);
                            new_text = Some(new);
                            d
                        } else {
                            let mut l = vec![
                                "the text to replace is not in the file as it is now:".to_owned(),
                            ];
                            l.extend(from.lines().map(|x| format!("  - {}", escape(x))));
                            l.extend(to.lines().map(|x| format!("  + {}", escape(x))));
                            l
                        }
                    }
                    (_, None) => {
                        let (from, to) = (s("old_string"), s("new_string"));
                        let mut l: Vec<String> =
                            from.lines().map(|x| format!("  - {}", escape(x))).collect();
                        l.extend(to.lines().map(|x| format!("  + {}", escape(x))));
                        l
                    }
                },
            };
            (title, escape(&path), lines)
        }
    };
    let (mut body, more) = head(&full);
    if more > 0 {
        body.push(format!("… {more} more lines (v to view all)"));
    }
    let sandbox = match kind {
        "run" | "network" => match &context.sandbox {
            Some(m) => format!(
                "sandbox on ({m}) · network: {} · writes: worktree",
                if kind == "network" {
                    "this call"
                } else {
                    context.network.as_str()
                }
            ),
            None => "sandbox off — runs unconfined".to_owned(),
        },
        "mcp" => "the MCP server runs outside speccoder's sandbox".to_owned(),
        "agent" => "the agent's own call — it runs outside speccoder's sandbox".to_owned(),
        _ => "a file write — nothing is executed".to_owned(),
    };
    let scope = scope_text(kind).to_owned();
    let task = request.task.rsplit('/').next().unwrap_or(&request.task);
    let options = request
        .options
        .iter()
        .filter_map(|o| choice(o, task, &scope))
        .collect();
    Presentation {
        request: request.clone(),
        title: title.to_owned(),
        subject,
        object: object_lines(request.object.as_ref(), request.changed.as_deref()),
        sources: source_lines(request),
        notice: budget_notice(request),
        body,
        full,
        sandbox,
        options,
        scope,
        old_text,
        new_text,
        file: context
            .root
            .as_ref()
            .filter(|_| !path.is_empty() && matches!(kind, "write" | "edit"))
            .map(|r| r.join(&path).display().to_string()),
    }
}

/// The text a terminal shows for a bug's reproductions: each with its
/// statement, file, test and how it failed, then the digest the answer is
/// bound to.
#[must_use]
pub fn confirm_text(r: &super::ConfirmRequest) -> String {
    let mut out = format!(
        "Bug {} — {}: {} reproduction(s) fail on the untouched code by an assertion.\n",
        r.bug,
        r.title,
        r.reproductions
            .iter()
            .filter(|x| x.statement.starts_with('E'))
            .count()
    );
    for x in &r.reproductions {
        out.push_str(&format!(
            "\n{} — {}\n  {} :: {}\n  $ {}\n",
            x.statement,
            x.words,
            x.file,
            x.test,
            x.argv.join(" ")
        ));
        for l in x.excerpt.lines().take(8) {
            out.push_str(&format!("    | {l}\n"));
        }
    }
    if let Some(w) = &r.warning {
        out.push_str(&format!("\n! {w}\n"));
    }
    out.push_str(&format!("\ndigest {}\n", r.digest));
    out
}

/// The choice `o` as every interface offers it: its key, its fixed number
/// and its label.
#[must_use]
pub fn choice(o: &str, task: &str, scope: &str) -> Option<Choice> {
    let (key, number, label) = match o {
        "once" => ('y', 1, "Yes, once".to_owned()),
        "task" => ('t', 2, format!("Yes, for the rest of {task}: {scope}")),
        "run" => ('r', 3, format!("Yes, for this run: {scope}, in any task")),
        "always" => (
            'a',
            4,
            "Always allow exactly this command (adds it to speccoder.toml)".to_owned(),
        ),
        "reject" => ('n', 5, "No, and tell the model why …".to_owned()),
        "stop" => (
            's',
            6,
            "Stop this task here (it waits; decide later)".to_owned(),
        ),
        _ => return None,
    };
    Some(Choice {
        choice: o.to_owned(),
        key,
        number,
        label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    fn request(tool: &str, arguments: serde_json::Value, kind: &str, risk: &str) -> Request {
        Request {
            task: "001-slugify/T004".to_owned(),
            task_title: "Handle Unicode input".to_owned(),
            turn: 8,
            tool: tool.to_owned(),
            arguments,
            kind: kind.to_owned(),
            reason: "why".to_owned(),
            risk: risk.to_owned(),
            options: crate::approvals::options(risk, true, false, kind, risk),
            ..Request::default()
        }
    }

    fn ctx(root: &Path) -> PresentContext {
        PresentContext {
            root: Some(root.to_path_buf()),
            sandbox: Some("Seatbelt".to_owned()),
            network: "none".to_owned(),
            file_text: None,
        }
    }

    #[test]
    fn a_new_file_shows_its_head() {
        let d = tempfile::tempdir().unwrap();
        let content: String = (1..=25).map(|i| format!("line {i}\n")).collect();
        let p = present(
            &request(
                "write_file",
                json!({ "path": "src/new.rs", "content": content }),
                "write",
                "low",
            ),
            &ctx(d.path()),
        );
        assert_eq!(
            (p.title.as_str(), p.subject.as_str()),
            ("Write this file?", "src/new.rs")
        );
        assert_eq!(p.body[0], "    1 │ line 1");
        assert_eq!(p.body.last().unwrap(), "… 5 more lines (v to view all)");
        assert_eq!(p.full.len(), 25);
        assert_eq!(p.sandbox, "a file write — nothing is executed");
        assert!(p.old_text.is_none() && p.new_text.is_some());
    }

    #[test]
    fn an_edit_is_a_numbered_diff() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        let p = present(
            &request(
                "edit_file",
                json!({ "path": "Cargo.toml", "old_string": "serde = \"1\"\n", "new_string": "serde = \"1\"\nunicode-normalization = \"0.1\"\n" }),
                "edit",
                "medium",
            ),
            &ctx(d.path()),
        );
        assert_eq!(p.title, "Edit this file?");
        assert!(
            p.body
                .contains(&"          6 + unicode-normalization = \"0.1\"".to_owned()),
            "{:#?}",
            p.body
        );
        assert!(
            p.body.contains(&"    5     5   serde = \"1\"".to_owned()),
            "{:#?}",
            p.body
        );
        let labels: Vec<&str> = p.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(
            labels[1],
            "Yes, for the rest of T004: this file, any content"
        );
        assert_eq!(
            labels[2],
            "Yes, for this run: this file, any content, in any task"
        );
    }

    #[test]
    fn a_missing_old_string_is_said_and_shown() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.rs"), "fn a() {}\n").unwrap();
        let p = present(
            &request(
                "edit_file",
                json!({ "path": "a.rs", "old_string": "fn b() {}", "new_string": "fn c() {}" }),
                "edit",
                "low",
            ),
            &ctx(d.path()),
        );
        assert_eq!(
            p.body[0],
            "the text to replace is not in the file as it is now:"
        );
        assert_eq!(
            p.body[1..],
            ["  - fn b() {}".to_owned(), "  + fn c() {}".to_owned()]
        );
    }

    #[test]
    fn commands_network_and_mcp() {
        let d = tempfile::tempdir().unwrap();
        let p = present(
            &request(
                "run_command",
                json!({ "argv": ["sh", "-c", "echo 'hi' > x"] }),
                "run",
                "high",
            ),
            &ctx(d.path()),
        );
        assert_eq!(p.title, "Run this command?");
        assert_eq!(
            p.body,
            ["$ sh -c 'echo '\\''hi'\\'' > x'", "cwd: . · network: none"]
        );
        assert_eq!(
            p.sandbox,
            "sandbox on (Seatbelt) · network: none · writes: worktree"
        );
        // High risk: no "for this run", no "always"; the numbers stay fixed.
        let numbers: Vec<u8> = p.options.iter().map(|o| o.number).collect();
        assert_eq!(numbers, [1, 2, 5, 6]);
        let p = present(
            &request(
                "run_command",
                json!({ "argv": ["curl", "x"], "network": true }),
                "network",
                "high",
            ),
            &PresentContext {
                sandbox: None,
                ..ctx(d.path())
            },
        );
        assert_eq!(p.title, "Use the network?");
        assert_eq!(p.body[1], "cwd: . · network: this call");
        assert_eq!(p.sandbox, "sandbox off — runs unconfined");
        let p = present(
            &request(
                "mcp__gh__create_issue",
                json!({ "title": "x" }),
                "mcp",
                "medium",
            ),
            &ctx(d.path()),
        );
        assert_eq!(
            (p.title.as_str(), p.subject.as_str()),
            ("Call MCP tool?", "mcp__gh__create_issue")
        );
        assert_eq!(p.sandbox, "the MCP server runs outside speccoder's sandbox");
        assert!(
            p.options[1]
                .label
                .ends_with("this tool as defined now, any arguments")
        );
    }

    #[test]
    fn model_text_cannot_drive_the_terminal() {
        assert_eq!(escape("a\x1b[2Jb\x07c\nd"), "a␛[2Jb^Gc\nd");
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("bin"), [0u8, 159, 146, 150]).unwrap();
        let p = present(
            &request(
                "write_file",
                json!({ "path": "bin", "content": "x" }),
                "edit",
                "low",
            ),
            &ctx(d.path()),
        );
        assert_eq!(p.body, ["a binary file — not shown"]);
    }

    #[test]
    fn the_object_sources_and_budget_are_shown() {
        let d = tempfile::tempdir().unwrap();
        let mut r = request(
            "run_command",
            json!({ "argv": ["make", "check"] }),
            "run",
            "low",
        );
        r.object = Some(Object::command(
            vec!["make".to_owned(), "check".to_owned()],
            ".".to_owned(),
            "/w/bin/make".to_owned(),
            false,
        ));
        r.changed = Some("executable /usr/bin/make → /w/bin/make".to_owned());
        r.sources = vec![crate::approvals::Source {
            argument: "make check".to_owned(),
            from: "README.md".to_owned(),
            turn: 2,
        }];
        r.more = 2;
        r.asked = 12;
        r.budget = 12;
        r.options = crate::approvals::options("low", true, false, "run", "low");
        let p = present(&r, &ctx(d.path()));
        assert_eq!(
            p.object[0],
            format!(
                "argv  [\"make\",\"check\"]  #{}",
                &crate::tools::policy::argv_sha256(&["make".to_owned(), "check".to_owned()])[..12]
            )
        );
        assert_eq!(p.object[1], "runs  /w/bin/make  in  .");
        assert_eq!(
            p.object[2],
            "changed since approved: executable /usr/bin/make → /w/bin/make"
        );
        assert_eq!(
            p.sources,
            [
                "contains text read from README.md (turn 2): \"make check\"",
                "and 2 more"
            ]
        );
        assert!(
            p.notice
                .as_deref()
                .is_some_and(|n| n.starts_with("This is prompt 12 of 12"))
        );
        let keys: Vec<char> = p.options.iter().map(|o| o.key).collect();
        assert_eq!(keys, ['y', 't', 'r', 'a', 'n', 's']);
        // A path asked by another name shows both.
        let mut w = request(
            "write_file",
            json!({ "path": "src/a/x.rs", "content": "" }),
            "write",
            "low",
        );
        w.object = Some(Object::Path {
            canonical: "src/b/x.rs".to_owned(),
            asked: "src/a/x.rs".to_owned(),
        });
        let p = present(&w, &ctx(d.path()));
        assert_eq!(
            p.object,
            [
                "file  src/b/x.rs",
                "asked as  src/a/x.rs (resolves to the file above)"
            ]
        );
    }
}
