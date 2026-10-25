//! What a person sees when a call needs them: built live from the
//! journaled [`Request`] and what only the live run knows — the file as it
//! is now, the sandbox — and never journaled itself, so replay does not
//! depend on the worktree. The terminal prompt, editors (ACP), the
//! `approvals` listing and the dashboard all render this one value.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Request;

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

/// One choice: `once`, `task`, `run`, `reject` or `stop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// Its name.
    pub choice: String,
    /// Its key (`y`, `t`, `r`, `n`, `s`).
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

/// What "for the task" or "for this run" covers, by kind.
#[must_use]
pub fn scope_text(kind: &str) -> &'static str {
    match kind {
        "run" => "this exact command",
        "network" => "this exact command with the network",
        "mcp" => "any call of this MCP tool, any arguments",
        _ => "this path, any content",
    }
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
        _ => "a file write — nothing is executed".to_owned(),
    };
    let scope = scope_text(kind).to_owned();
    let task = request.task.rsplit('/').next().unwrap_or(&request.task);
    let options = request
        .options
        .iter()
        .filter_map(|o| {
            let (key, number, label) = match o.as_str() {
                "once" => ('y', 1, "Yes, once".to_owned()),
                "task" => ('t', 2, format!("Yes, for the rest of {task}: {scope}")),
                "run" => ('r', 3, format!("Yes, for this run: {scope}, in any task")),
                "reject" => ('n', 4, "No, and tell the model why …".to_owned()),
                "stop" => (
                    's',
                    5,
                    "Stop this task here (it waits; decide later)".to_owned(),
                ),
                _ => return None,
            };
            Some(Choice {
                choice: o.clone(),
                key,
                number,
                label,
            })
        })
        .collect();
    Presentation {
        request: request.clone(),
        title: title.to_owned(),
        subject,
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
            covers: "c".to_owned(),
            risk: risk.to_owned(),
            claim: None,
            options: crate::approvals::options(risk, true),
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
            "Yes, for the rest of T004: this path, any content"
        );
        assert_eq!(
            labels[2],
            "Yes, for this run: this path, any content, in any task"
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
        // High risk: no "for this run"; the numbers stay fixed.
        let numbers: Vec<u8> = p.options.iter().map(|o| o.number).collect();
        assert_eq!(numbers, [1, 2, 4, 5]);
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
                .ends_with("any call of this MCP tool, any arguments")
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
}
