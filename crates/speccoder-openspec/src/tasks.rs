//! `tasks.md` of an OpenSpec change.
//!
//! Lines are matched the way OpenSpec counts them (any list marker, any
//! indent, checkboxes inside code fences included), so progress agrees with
//! `openspec list`. A task's ID is the number it starts with (`1.2`,
//! `2.3a`, `1.4.1`), else `#<ordinal>`; groups are `## N.` headings; the
//! order is a chain: nested tasks come before their parent.

use speccoder_core::snapshot::{
    DependencyMode, Diagnostic, ParsedTasks, Phase, Severity, Task, Totals,
};

use crate::hash::sha256_hex;

/// A checkbox line as OpenSpec matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkbox<'a> {
    /// Leading whitespace, in bytes.
    pub indent: usize,
    /// Byte offset of what is between the brackets, within the line.
    pub inner_start: usize,
    /// What is between the brackets.
    pub inner: &'a str,
    /// The text after the brackets.
    pub text: &'a str,
}

impl Checkbox<'_> {
    /// `[x]` or `[X]`.
    #[must_use]
    pub fn done(&self) -> bool {
        matches!(self.inner.trim(), "x" | "X")
    }

    /// Exactly `[ ]`: the only box the engine ticks.
    #[must_use]
    pub fn tickable(&self) -> bool {
        self.inner == " "
    }
}

/// Matches OpenSpec's task pattern
/// `^\s*(?:[-*+]|\d{1,9}[.)])\s*\[(?:\s*([^\]\s]?)\s*\](?![(\[])|\s+\])\s*(.*)`.
#[must_use]
pub fn checkbox(line: &str) -> Option<Checkbox<'_>> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let after_marker = if let Some(r) = rest.strip_prefix(['-', '*', '+']) {
        r
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&digits) {
            return None;
        }
        rest[digits..].strip_prefix(['.', ')'])?
    };
    let open = after_marker.trim_start();
    let body = open.strip_prefix('[')?;
    let close = body.find(']')?;
    let inner = &body[..close];
    let after = &body[close + 1..];
    let blank = inner.chars().all(char::is_whitespace);
    let one_char = inner.trim().chars().count() == 1;
    if !(blank || one_char) {
        return None;
    }
    // `[x](link)` is a link, not a checkbox — unless the box holds only spaces.
    if after.starts_with(['(', '[']) && (inner.is_empty() || !blank) {
        return None;
    }
    let inner_start = line.len() - body.len();
    Some(Checkbox {
        indent,
        inner_start,
        inner,
        text: after.trim(),
    })
}

/// The leading task number of `text` (`1.2`, `2.3a`, `1.4.1`), if any.
#[must_use]
pub fn leading_id(text: &str) -> Option<&str> {
    let mut end = 0;
    let bytes = text.as_bytes();
    let mut parts = 0;
    loop {
        let digits = bytes[end..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 {
            break;
        }
        end += digits;
        parts += 1;
        if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
            end += 1;
        } else {
            break;
        }
    }
    if parts < 2 {
        return None;
    }
    end += bytes[end..]
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    let next = text[end..].chars().next();
    (next.is_none_or(char::is_whitespace)).then_some(&text[..end])
}

/// `## 2. Farewell` → 2.
fn group_number(title: &str) -> Option<u32> {
    let digits: String = title.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !title[digits.len()..].starts_with('.') {
        return None;
    }
    digits.parse().ok()
}

/// Task IDs named after `depends on` / `after` in the text.
fn references(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for key in ["depends on ", "after "] {
        for (i, _) in lower.match_indices(key) {
            if i > 0 && lower.as_bytes()[i - 1].is_ascii_alphanumeric() {
                continue;
            }
            let word: String = text[i + key.len()..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
                .collect();
            let word = word.trim_end_matches('.');
            if leading_id(word) == Some(word) && !out.iter().any(|o| o == word) {
                out.push(word.to_owned());
            }
        }
    }
    out
}

struct Raw {
    task: Task,
    indent: usize,
    group: usize,
}

/// Parses an OpenSpec `tasks.md` at `path` (for diagnostics).
#[must_use]
pub fn parse(path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let diag = |severity, code: &str, line: usize, message: String, hint: &str| {
        let d = Diagnostic::new(severity, code, message)
            .at(path)
            .line(u32::try_from(line + 1).unwrap_or(u32::MAX));
        if hint.is_empty() { d } else { d.hint(hint) }
    };
    let mut phases: Vec<(Phase, usize, Option<u32>)> = Vec::new();
    let mut raws: Vec<Raw> = Vec::new();
    let mut fence: Option<&str> = None;
    let mut unnumbered_warned: Option<usize> = None;
    let mut offset = 0;
    let mut ordinal = 0;
    for (no, full) in text.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += full.len();
        let line = full.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        if let Some(f) = fence {
            if trimmed.starts_with(f) {
                fence = None;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            continue;
        } else if let Some(title) = line.strip_prefix("## ") {
            let title = title.trim().to_owned();
            let number = group_number(&title);
            phases.push((
                Phase {
                    title,
                    line: u32::try_from(no + 1).unwrap_or(u32::MAX),
                    section_hash: String::new(),
                },
                start,
                number,
            ));
            continue;
        }
        let Some(cb) = checkbox(line) else { continue };
        ordinal += 1;
        if fence.is_some() {
            diags.push(diag(
                Severity::Warning,
                "in_fence",
                no,
                "a checkbox inside a code block: OpenSpec counts it as a task".to_owned(),
                "move it out of the code block, or remove its brackets",
            ));
        }
        if cb.text.is_empty() {
            diags.push(diag(
                Severity::Error,
                "textless",
                no,
                "a checkbox without text".to_owned(),
                "describe the task, or remove the line",
            ));
            continue;
        }
        if cb.inner.is_empty() {
            diags.push(diag(
                Severity::Warning,
                "empty_brackets",
                no,
                "`[]` has no space: speccoder cannot tick it".to_owned(),
                "write `[ ]`",
            ));
        } else if !cb.done() && !cb.tickable() {
            diags.push(diag(
                Severity::Warning,
                "marker_not_space",
                no,
                format!(
                    "`[{}]` is neither open nor done: speccoder cannot tick it",
                    cb.inner
                ),
                "write `[ ]` (open) or `[x]` (done)",
            ));
        }
        let (id, title) = match leading_id(cb.text) {
            Some(id) => (id.to_owned(), cb.text[id.len()..].trim().to_owned()),
            None => (format!("#{ordinal}"), cb.text.to_owned()),
        };
        let group = phases.len();
        let (phase, number) = match phases.last() {
            Some((p, _, n)) => (Some(p.title.clone()), *n),
            None => (None, None),
        };
        if let (Some(n), Some(first)) = (number, id.split('.').next()) {
            if first.parse::<u32>().ok().is_some_and(|f| f != n) && !id.starts_with('#') {
                diags.push(diag(
                    Severity::Warning,
                    "misplaced_id",
                    no,
                    format!("{id} is under group {n}"),
                    "renumber the task or move it to its group",
                ));
            }
        } else if phase.is_some() && number.is_none() && unnumbered_warned != Some(group) {
            unnumbered_warned = Some(group);
            diags.push(diag(
                Severity::Warning,
                "unnumbered_section",
                no,
                format!(
                    "`## {}` is not a numbered group; its tasks run after everything before them",
                    phase.as_deref().unwrap_or_default()
                ),
                "number the heading (`## 3. …`)",
            ));
        }
        let depends_on = references(&title);
        if !depends_on.is_empty() {
            diags.push(diag(
                Severity::Warning,
                "extension_dependency",
                no,
                format!(
                    "{id} names its dependencies ({}): a speccoder extension OpenSpec ignores",
                    depends_on.join(", ")
                ),
                "",
            ));
        }
        raws.push(Raw {
            task: Task {
                id,
                done: cb.done(),
                parallel: false,
                story: None,
                title,
                depends_on,
                phase,
                subsection: None,
                line: u32::try_from(no + 1).unwrap_or(u32::MAX),
                byte_range: [start, start + line.len()],
                line_hash: sha256_hex(line.as_bytes()),
            },
            indent: cb.indent,
            group,
        });
    }

    // Duplicates.
    for (i, r) in raws.iter().enumerate() {
        if raws[..i].iter().any(|e| e.task.id == r.task.id) {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "duplicate_id",
                    format!("task {} appears more than once", r.task.id),
                )
                .at(path)
                .line(r.task.line)
                .hint("give each task its own number"),
            );
        }
    }
    if raws.is_empty() {
        diags.push(
            Diagnostic::new(Severity::Error, "no_tasks", "tasks.md has no tasks")
                .at(path)
                .hint("add `- [ ] 1.1 …` lines under `## 1. …`"),
        );
    }

    // Chain order: a nested task before its parent.
    let mut tasks = Vec::with_capacity(raws.len());
    let mut stack: Vec<Raw> = Vec::new();
    for r in raws {
        while stack
            .last()
            .is_some_and(|top| top.group != r.group || top.indent >= r.indent)
        {
            tasks.extend(stack.pop().map(|t| t.task));
        }
        stack.push(r);
    }
    while let Some(t) = stack.pop() {
        tasks.push(t.task);
    }

    let starts: Vec<usize> = phases.iter().map(|(_, s, _)| *s).collect();
    let phases = phases
        .into_iter()
        .enumerate()
        .map(|(i, (mut p, start, _))| {
            let end = starts.get(i + 1).copied().unwrap_or(text.len());
            p.section_hash = sha256_hex(&text.as_bytes()[start..end]);
            p
        })
        .collect();
    let done = tasks.iter().filter(|t| t.done).count();
    let totals = Totals {
        total: tasks.len(),
        done,
        open: tasks.len() - done,
    };
    (
        ParsedTasks {
            path: path.to_owned(),
            phases,
            tasks,
            totals,
            dependency_mode: DependencyMode::Chain,
        },
        diags,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkboxes_match_openspecs_pattern() {
        for (line, inner, text) in [
            ("- [ ] 1.1 a", " ", "1.1 a"),
            ("  * [x] b", "x", "b"),
            ("1) [X] c", "X", "c"),
            ("- [~] d", "~", "d"),
            ("- [] e", "", "e"),
            ("- [  ]", "  ", ""),
            ("+[ ]f", " ", "f"),
        ] {
            let cb = checkbox(line).unwrap_or_else(|| panic!("{line}"));
            assert_eq!((cb.inner, cb.text), (inner, text), "{line}");
            assert_eq!(
                &line[cb.inner_start..cb.inner_start + cb.inner.len()],
                inner
            );
        }
        for line in [
            "[ ] no marker",
            "- [ab] two",
            "- [x](link)",
            "text",
            "1234567890. [ ] x",
        ] {
            assert!(checkbox(line).is_none(), "{line}");
        }
        assert!(checkbox("- [ ](x)").is_some());
    }

    #[test]
    fn ids_and_references() {
        assert_eq!(leading_id("1.2 x"), Some("1.2"));
        assert_eq!(leading_id("2.3a y"), Some("2.3a"));
        assert_eq!(leading_id("1.4.1"), Some("1.4.1"));
        assert_eq!(leading_id("1 x"), None);
        assert_eq!(leading_id("1.2x3"), None);
        assert_eq!(leading_id("Read 1.2"), None);
        assert_eq!(references("do it, depends on 2.2"), ["2.2"]);
        assert_eq!(references("After 9.9, which"), ["9.9"]);
        assert!(references("hereafter 1.2").is_empty());
    }
}
