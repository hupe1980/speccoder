//! Tolerant parser for Spec Kit's `tasks.md`.
//!
//! A task is a Markdown task-list item (`- [ ]`, `- [x]`, `- [X]`) whose text
//! starts with an ID `T` + three or more digits. Items inside code blocks and
//! HTML comments are not list items to a CommonMark parser, so they are never
//! tasks. Level-2 headings are phases, level-3 headings subsections. No
//! dependencies are inferred here.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use speccoder_core::snapshot::{
    DependencyMode, Diagnostic, ParsedTasks, Phase, Severity, Task, Totals,
};

use crate::hash::sha256_hex;

/// Parses `text` (the contents of the workspace-relative `path`).
#[must_use]
pub fn parse(path: &str, text: &str) -> (ParsedTasks, Vec<Diagnostic>) {
    let lines = LineIndex::new(text);
    let mut diags = Vec::new();
    let mut phases: Vec<(Phase, usize)> = Vec::new(); // with section start offset
    let mut tasks = Vec::new();
    let mut subsection: Option<String> = None;
    let mut pending_item: Option<Range<usize>> = None;
    let mut dependency_mode = DependencyMode::Structural;

    // Spec Kit's tasks.md opens with YAML front matter; without metadata-block
    // support its closing `---` would turn the line above it into a heading.
    let options = Options::ENABLE_TASKLISTS | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                pending_item = None;
                let line_no = lines.line_of(range.start);
                let title = heading_title(lines.text(text, line_no));
                if level == HeadingLevel::H2 && declares_dependencies(&title) {
                    dependency_mode = DependencyMode::Declared;
                }
                match level {
                    HeadingLevel::H2 => {
                        phases.push((
                            Phase {
                                title,
                                line: to_u32(line_no + 1),
                                section_hash: String::new(),
                            },
                            lines.start(line_no),
                        ));
                        subsection = None;
                    }
                    HeadingLevel::H3 => subsection = Some(title),
                    _ => {}
                }
            }
            Event::Start(Tag::Item) => pending_item = Some(range),
            Event::Start(Tag::Paragraph) => {}
            Event::TaskListMarker(checked) => {
                if let Some(item) = pending_item.take() {
                    let line_no = lines.line_of(item.start);
                    let line = lines.text(text, line_no);
                    let display_line = to_u32(line_no + 1);
                    match parse_task_line(line) {
                        Some(parsed) => tasks.push(Task {
                            id: parsed.id,
                            done: checked,
                            parallel: parsed.parallel,
                            story: parsed.story,
                            yours: parsed.yours,
                            depends_on: explicit_deps(&parsed.title),
                            title: parsed.title,
                            phase: phases.last().map(|(p, _)| p.title.clone()),
                            subsection: if phases.is_empty() {
                                None
                            } else {
                                subsection.clone()
                            },
                            line: display_line,
                            byte_range: [item.start, item.end],
                            line_hash: sha256_hex(line.as_bytes()),
                        }),
                        None => diags.push(
                            Diagnostic::new(
                                Severity::Warning,
                                "tasks.missing-id",
                                "checklist item without a task ID; not treated as a task",
                            )
                            .at(path)
                            .line(display_line)
                            .hint("start the item with an ID such as T001"),
                        ),
                    }
                }
            }
            _ => pending_item = None,
        }
    }

    // Section hashes: heading line start through the next phase (or EOF).
    let starts: Vec<usize> = phases.iter().map(|(_, s)| *s).collect();
    for (i, (phase, start)) in phases.iter_mut().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(text.len());
        phase.section_hash = sha256_hex(&text.as_bytes()[*start..end]);
    }

    check(path, &tasks, &mut diags);
    let done = tasks.iter().filter(|t| t.done).count();
    let totals = Totals {
        total: tasks.len(),
        done,
        open: tasks.len() - done,
    };
    let parsed = ParsedTasks {
        path: path.to_owned(),
        phases: phases.into_iter().map(|(p, _)| p).collect(),
        tasks,
        totals,
        dependency_mode,
    };
    (parsed, diags)
}

/// A level-2 `Format…` heading that documents `depends on`, as the Explicit
/// Task Dependencies preset writes it.
fn declares_dependencies(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.starts_with("format") && lower.contains("depends on")
}

fn check(path: &str, tasks: &[Task], diags: &mut Vec<Diagnostic>) {
    if tasks.is_empty() {
        diags.push(
            Diagnostic::new(
                Severity::Warning,
                "tasks.empty",
                "tasks.md contains no tasks",
            )
            .at(path)
            .hint("speccoder spec tasks generates them (/speckit-tasks in your editor)"),
        );
        return;
    }
    let mut first_line: HashMap<&str, u32> = HashMap::new();
    let mut previous: Option<(u64, &str)> = None;
    for t in tasks {
        if let Some(first) = first_line.get(t.id.as_str()) {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "tasks.duplicate-id",
                    format!("{} appears on lines {first} and {}", t.id, t.line),
                )
                .at(path)
                .line(t.line)
                .hint("give every task a unique ID"),
            );
        } else {
            first_line.insert(&t.id, t.line);
        }
        let n = id_number(&t.id);
        if let Some((prev_n, prev_id)) = previous
            && n < prev_n
        {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "tasks.out-of-order",
                    format!("{} follows {prev_id}", t.id),
                )
                .at(path)
                .line(t.line),
            );
        }
        previous = Some((n, &t.id));
        if t.phase.is_none() {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "tasks.before-phase",
                    format!("{} comes before the first phase heading", t.id),
                )
                .at(path)
                .line(t.line),
            );
        }
    }
}

struct TaskLine {
    id: String,
    parallel: bool,
    story: Option<String>,
    yours: bool,
    title: String,
}

/// Parses `- [ ] T014 [P] [US1] [YOU] Title` (the checkbox has already been seen by
/// the Markdown parser; here it is only skipped).
fn parse_task_line(line: &str) -> Option<TaskLine> {
    let rest = line.trim_start();
    let rest = rest
        .strip_prefix(['-', '*', '+'])
        .or_else(|| {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0)
                .then(|| rest[digits..].strip_prefix(['.', ')']))
                .flatten()
        })?
        .trim_start();
    let rest = rest
        .get(3..)
        .filter(|_| rest.starts_with('['))?
        .trim_start();

    let digits = rest
        .strip_prefix('T')?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let id_end = 1 + digits;
    let boundary_ok = rest[id_end..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric());
    if digits < 3 || !boundary_ok {
        return None;
    }
    let id = rest[..id_end].to_owned();

    let mut rest = rest[id_end..].trim_start();
    let mut parallel = false;
    let mut story = None;
    let mut yours = false;
    loop {
        if let Some(r) = rest.strip_prefix("[P]") {
            parallel = true;
            rest = r.trim_start();
        } else if let Some(r) = rest.strip_prefix("[YOU]") {
            yours = true;
            rest = r.trim_start();
        } else if let Some(label) = story_label(rest) {
            rest = rest[label.len() + 2..].trim_start();
            story = Some(label.to_owned());
        } else {
            break;
        }
    }
    Some(TaskLine {
        id,
        parallel,
        story,
        yours,
        title: rest.trim_end().to_owned(),
    })
}

/// Task IDs named after `depends on`, `depend on` or `after` (any case):
/// lists separated by whitespace, `,`, `and`, `&`, and ranges `T012-T015` /
/// `T012–T015`. Nothing else in the text creates a dependency.
#[must_use]
pub fn explicit_deps(title: &str) -> Vec<String> {
    // Inline code is quoted text (`depends on T099` as an example), not a reference.
    let title = &without_code_spans(title);
    let lower = title.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for phrase in ["depends on", "depend on", "after"] {
        for (i, _) in lower.match_indices(phrase) {
            let word_start = i == 0 || !lower[..i].ends_with(|c: char| c.is_alphanumeric());
            if word_start {
                collect_ids(&title[i + phrase.len()..], &mut out);
            }
        }
    }
    out
}

/// `s` with every inline code span (`` `…` ``) replaced by spaces of the same
/// byte length, so offsets and case-folding stay aligned.
fn without_code_spans(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        } else {
            out.push(c);
        }
    }
    out
}

/// Reads a list of IDs from the start of `s`.
fn collect_ids(mut s: &str, out: &mut Vec<String>) {
    let mut push = |id: String| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == ',' || c == '&');
        if let Some(rest) = s.strip_prefix("and ") {
            s = rest;
            continue;
        }
        let Some((id, rest)) = take_id(s) else {
            return;
        };
        let range_end = rest
            .strip_prefix('-')
            .or_else(|| rest.strip_prefix('–'))
            .and_then(take_id);
        match range_end {
            Some((end, after)) => {
                let width = id.len() - 1;
                let (a, b) = (id_number(&id), id_number(&end));
                if a <= b && b - a <= 1000 {
                    for n in a..=b {
                        push(format!("T{n:0width$}"));
                    }
                } else {
                    push(id);
                    push(end);
                }
                s = after;
            }
            None => {
                push(id);
                s = rest;
            }
        }
    }
}

/// `T012…` → (`T012`, rest) when followed by a non-alphanumeric boundary.
fn take_id(s: &str) -> Option<(String, &str)> {
    let digits = s
        .strip_prefix('T')?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let end = 1 + digits;
    let boundary = s[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
    (digits >= 3 && boundary).then(|| (s[..end].to_owned(), &s[end..]))
}

/// `[US12] …` → `US12`.
fn story_label(s: &str) -> Option<&str> {
    let inner = s.strip_prefix('[')?;
    let end = inner.find(']')?;
    let label = &inner[..end];
    let n = label.strip_prefix("US")?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(label)
}

fn id_number(id: &str) -> u64 {
    id[1..].parse().unwrap_or(u64::MAX)
}

fn heading_title(line: &str) -> String {
    line.trim()
        .trim_start_matches('#')
        .trim_end_matches('#')
        .trim()
        .to_owned()
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Byte offsets of line starts, for offset → line and line → text.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

    /// 0-based line containing `offset`.
    fn line_of(&self, offset: usize) -> usize {
        self.starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1)
    }

    fn start(&self, line: usize) -> usize {
        self.starts[line]
    }

    /// The line's text without its line ending (`\n` or `\r\n`).
    fn text<'t>(&self, text: &'t str, line: usize) -> &'t str {
        let start = self.starts[line];
        let end = self.starts.get(line + 1).map_or(text.len(), |&n| n - 1);
        let s = &text[start..end.max(start)];
        s.strip_suffix('\r').unwrap_or(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markers_and_title() {
        let t =
            parse_task_line("- [ ] T014 [P] [US1] Implement Service in src/x.py (depends on T012)")
                .unwrap();
        assert_eq!(t.id, "T014");
        assert!(t.parallel);
        assert_eq!(t.story.as_deref(), Some("US1"));
        assert_eq!(t.title, "Implement Service in src/x.py (depends on T012)");
        assert!(!t.yours);
    }

    #[test]
    fn a_you_label_marks_a_step_for_a_person() {
        let t = parse_task_line("- [ ] T019 [US2] [YOU] Create the GitHub repository").unwrap();
        assert!(t.yours);
        assert_eq!(t.story.as_deref(), Some("US2"));
        assert_eq!(t.title, "Create the GitHub repository");
        let t = parse_task_line("- [ ] T020 [YOU] [P] Publish the package").unwrap();
        assert!(t.yours && t.parallel);
        assert_eq!(t.title, "Publish the package");
    }

    #[test]
    fn rejects_items_without_a_proper_id() {
        assert!(parse_task_line("- [ ] Create model").is_none());
        assert!(parse_task_line("- [ ] T01 too short").is_none());
        assert!(parse_task_line("- [ ] T001x glued").is_none());
        assert!(parse_task_line("- [x] T001").is_some());
    }

    #[test]
    fn ignores_code_blocks_and_html_comments() {
        let text = "## Phase 1\n<!--\n- [ ] T900 in a comment\n-->\n```\n- [ ] T901 in a fence\n```\n- [ ] T001 real\n";
        let (parsed, diags) = parse("tasks.md", text);
        let ids: Vec<_> = parsed.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["T001"]);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn extracts_explicit_dependencies() {
        assert_eq!(
            explicit_deps("Implement X (depends on T012, T013)"),
            ["T012", "T013"]
        );
        assert_eq!(
            explicit_deps("Do Y after T005 and T006 & T007"),
            ["T005", "T006", "T007"]
        );
        assert_eq!(
            explicit_deps("Depends On T010-T012"),
            ["T010", "T011", "T012"]
        );
        assert_eq!(explicit_deps("depend on T008–T009 first"), ["T008", "T009"]);
        assert!(explicit_deps("Run after deployment, see T004").is_empty());
        assert!(explicit_deps("Hereafter T004 matters").is_empty());
        assert!(explicit_deps("Write T001 docs").is_empty());
        assert!(explicit_deps("Fixture with a dangling `depends on T099` line").is_empty());
        assert_eq!(explicit_deps("Ä `x` after T002"), ["T002"]);
    }

    #[test]
    fn format_line_sets_declared_mode() {
        let declared = "## Format: `[ID] [P?] [Story?] Description [(depends on ...)]`\n## Phase 1\n- [ ] T001 a\n";
        assert_eq!(
            parse("t", declared).0.dependency_mode,
            DependencyMode::Declared
        );
        let plain = "## Format: `[ID] [P?] [Story] Description`\n## Phase 1\n- [ ] T001 a\n";
        assert_eq!(
            parse("t", plain).0.dependency_mode,
            DependencyMode::Structural
        );
        let fenced = "```\n## Format: (depends on ...)\n```\n## Phase 1\n- [ ] T001 a\n";
        assert_eq!(
            parse("t", fenced).0.dependency_mode,
            DependencyMode::Structural
        );
    }

    #[test]
    fn front_matter_is_not_a_phase() {
        let text = "---\ndescription: \"Task list\"\n---\n\n# Tasks\n\n## Phase 1\n- [ ] T001 a\n";
        let (parsed, _) = parse("t", text);
        let phases: Vec<_> = parsed.phases.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(phases, ["Phase 1"]);
    }

    #[test]
    fn crlf_line_hash_excludes_line_ending() {
        let (lf, _) = parse("t", "## P\n- [ ] T001 a\n");
        let (crlf, _) = parse("t", "## P\r\n- [ ] T001 a\r\n");
        assert_eq!(lf.tasks[0].line_hash, crlf.tasks[0].line_hash);
        assert_eq!(crlf.tasks[0].line, 2);
        let r = crlf.tasks[0].byte_range;
        assert!("## P\r\n- [ ] T001 a\r\n"[r[0]..r[1]].starts_with("- [ ] T001 a"));
    }
}
