//! Spec deltas of a change (`openspec/changes/<id>/specs/<capability>/spec.md`)
//! and the requirement names of the main specs (`openspec/specs/**/spec.md`).
//!
//! OpenSpec returns scenarios only as raw text; speccoder parses them itself
//! because they are the acceptance examples (D-060).

use std::collections::{BTreeMap, BTreeSet};

use speccoder_core::snapshot::{Diagnostic, Severity};

/// What a delta section does to the main spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// `## ADDED Requirements`.
    Added,
    /// `## MODIFIED Requirements`: replaces the whole requirement.
    Modified,
    /// `## REMOVED Requirements`.
    Removed,
    /// `## RENAMED Requirements`: `FROM:` / `TO:` pairs.
    Renamed,
}

impl Operation {
    /// Its lowercase name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Removed => "removed",
            Self::Renamed => "renamed",
        }
    }
}

/// A `#### Scenario:` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    /// Its name.
    pub name: String,
    /// `GIVEN` lines (an `AND` after one is added to it).
    pub given: Vec<String>,
    /// `WHEN` lines.
    pub when: Vec<String>,
    /// `THEN` lines.
    pub then: Vec<String>,
    /// The block as written.
    pub raw: String,
    /// 1-based line of its heading.
    pub line: u32,
    /// Byte range in the delta file.
    pub range: [usize; 2],
}

/// A requirement of an ADDED, MODIFIED or REMOVED section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The section it is in.
    pub operation: Operation,
    /// Its name as written.
    pub name: String,
    /// The block as written (heading through the last line before the next
    /// heading of the same or a higher level).
    pub text: String,
    /// Its scenarios.
    pub scenarios: Vec<Scenario>,
    /// REMOVED: a `**Reason**` line is present.
    pub reason: bool,
    /// REMOVED: a `**Migration**` line is present.
    pub migration: bool,
    /// 1-based line of its heading.
    pub line: u32,
    /// Byte range in the delta file.
    pub range: [usize; 2],
}

/// A RENAMED pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    /// The old name.
    pub from: String,
    /// The new name.
    pub to: String,
    /// 1-based line of `FROM:`.
    pub line: u32,
}

/// One delta file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    /// The capability, from the path between `specs/` and `/spec.md`.
    pub capability: String,
    /// The file, relative to the project root.
    pub path: String,
    /// The whole file.
    pub text: String,
    /// It has a `## Purpose` section.
    pub purpose: bool,
    /// ADDED, MODIFIED and REMOVED requirements, in file order.
    pub requirements: Vec<Requirement>,
    /// RENAMED pairs.
    pub renames: Vec<Rename>,
}

/// OpenSpec's name matching: case-insensitive, whitespace collapsed.
#[must_use]
pub fn normalise(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n + 1).unwrap_or(u32::MAX)
}

/// `## MODIFIED Requirements` → Modified.
fn section(title: &str) -> Option<Operation> {
    let lower = normalise(title);
    let op = lower.strip_suffix(" requirements")?;
    match op {
        "added" => Some(Operation::Added),
        "modified" => Some(Operation::Modified),
        "removed" => Some(Operation::Removed),
        "renamed" => Some(Operation::Renamed),
        _ => None,
    }
}

/// `### Requirement: Name` → Name (case-insensitive keyword).
fn requirement_name(rest: &str) -> Option<&str> {
    let head = rest.get(..12)?;
    head.eq_ignore_ascii_case("requirement:")
        .then(|| rest[12..].trim())
}

/// `- **WHEN** x` / `- WHEN x` → ("when", "x").
fn step(line: &str) -> Option<(String, String)> {
    let item = line.trim_start().strip_prefix(['-', '*'])?.trim_start();
    let item = item.strip_prefix("**").unwrap_or(item);
    let word: String = item.chars().take_while(char::is_ascii_alphabetic).collect();
    let kind = word.to_lowercase();
    if !matches!(kind.as_str(), "given" | "when" | "then" | "and") {
        return None;
    }
    let rest = item[word.len()..].trim_start_matches('*').trim();
    Some((kind, rest.to_owned()))
}

/// `- FROM: \`### Requirement: Old\`` → ("from", "Old").
fn rename_line(line: &str) -> Option<(&'static str, String)> {
    let item = line
        .trim_start()
        .strip_prefix(['-', '*'])
        .unwrap_or(line)
        .trim();
    let (key, rest) = item.split_once(':')?;
    let key = match key.trim().trim_matches('*').to_lowercase().as_str() {
        "from" => "from",
        "to" => "to",
        _ => return None,
    };
    let rest = rest.trim().trim_matches('`').trim();
    let name = rest
        .strip_prefix("###")
        .map(str::trim)
        .and_then(requirement_name)
        .unwrap_or(rest);
    Some((key, name.to_owned()))
}

/// Parses the delta at `path` (relative to the project root) for
/// `capability`, with its problems.
#[must_use]
pub fn parse(capability: &str, path: &str, text: &str) -> (Delta, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut lines = Vec::new();
    let mut at = 0;
    for l in text.split_inclusive('\n') {
        lines.push((at, l.trim_end_matches(['\n', '\r'])));
        at += l.len();
    }
    let mut delta = Delta {
        capability: capability.to_owned(),
        path: path.to_owned(),
        text: text.to_owned(),
        purpose: false,
        requirements: Vec::new(),
        renames: Vec::new(),
    };
    let mut op: Option<Operation> = None;
    let mut pending_from: Option<(String, u32)> = None;
    // Where the open requirement and scenario started (line index).
    let mut req_start: Option<usize> = None;
    let mut sc_start: Option<usize> = None;
    let mut last_kind = String::new();

    let close_scenario = |delta: &mut Delta, from: usize, to: usize| {
        let (start, _) = lines[from];
        let end = lines.get(to).map_or(text.len(), |(s, _)| *s);
        if let Some(sc) = delta
            .requirements
            .last_mut()
            .and_then(|r| r.scenarios.last_mut())
        {
            sc.raw = text[start..end].trim_end().to_owned();
            sc.range = [start, start + sc.raw.len()];
        }
    };
    let close_requirement = |delta: &mut Delta, from: usize, to: usize| {
        let (start, _) = lines[from];
        let end = lines.get(to).map_or(text.len(), |(s, _)| *s);
        if let Some(r) = delta.requirements.last_mut() {
            r.text = text[start..end].trim_end().to_owned();
            r.range = [start, start + r.text.len()];
        }
    };

    for (i, (_, line)) in lines.iter().enumerate() {
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let heading = hashes > 0 && line[hashes..].starts_with(' ');
        let rest = if heading { line[hashes..].trim() } else { "" };
        if heading
            && hashes <= 4
            && let Some(s) = sc_start.take()
        {
            close_scenario(&mut delta, s, i);
        }
        if heading
            && hashes <= 3
            && let Some(s) = req_start.take()
        {
            close_requirement(&mut delta, s, i);
        }
        if heading && hashes <= 2 {
            op = section(rest);
            if normalise(rest) == "purpose" {
                delta.purpose = true;
            }
            continue;
        }
        if heading && hashes == 3 {
            if let Some(name) = requirement_name(rest) {
                match op {
                    Some(Operation::Renamed) | None => {}
                    Some(operation) => {
                        delta.requirements.push(Requirement {
                            operation,
                            name: name.to_owned(),
                            text: String::new(),
                            scenarios: Vec::new(),
                            reason: false,
                            migration: false,
                            line: line_no(i),
                            range: [0, 0],
                        });
                        req_start = Some(i);
                    }
                }
            } else if rest.to_lowercase().starts_with("scenario:") {
                diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "scenario_wrong_level",
                        "a scenario heading needs four hashes (`#### Scenario:`)",
                    )
                    .at(path)
                    .line(line_no(i))
                    .hint("write `#### Scenario: …`; OpenSpec ignores it otherwise"),
                );
            }
            continue;
        }
        if heading && hashes == 4 {
            if let Some(name) = rest.strip_prefix("Scenario:")
                && let Some(r) = delta
                    .requirements
                    .last_mut()
                    .filter(|_| req_start.is_some())
            {
                r.scenarios.push(Scenario {
                    name: name.trim().to_owned(),
                    given: Vec::new(),
                    when: Vec::new(),
                    then: Vec::new(),
                    raw: String::new(),
                    line: line_no(i),
                    range: [0, 0],
                });
                sc_start = Some(i);
                last_kind.clear();
            }
            continue;
        }
        if op == Some(Operation::Renamed) {
            match rename_line(line) {
                Some(("from", name)) => pending_from = Some((name, line_no(i))),
                Some((_, to)) => {
                    if let Some((from, line)) = pending_from.take() {
                        delta.renames.push(Rename { from, to, line });
                    }
                }
                None => {}
            }
            continue;
        }
        if req_start.is_some()
            && let Some(r) = delta.requirements.last_mut()
        {
            let lower = line.to_lowercase();
            if lower.contains("**reason**") {
                r.reason = true;
            }
            if lower.contains("**migration**") {
                r.migration = true;
            }
        }
        if sc_start.is_some()
            && let Some((kind, text)) = step(line)
        {
            let kind = if kind == "and" {
                last_kind.clone()
            } else {
                kind
            };
            if let Some(sc) = delta
                .requirements
                .last_mut()
                .and_then(|r| r.scenarios.last_mut())
            {
                match kind.as_str() {
                    "given" => sc.given.push(text),
                    "when" => sc.when.push(text),
                    "then" => sc.then.push(text),
                    _ => {}
                }
            }
            last_kind = kind;
        }
    }
    if let Some(s) = sc_start {
        close_scenario(&mut delta, s, lines.len());
    }
    if let Some(s) = req_start {
        close_requirement(&mut delta, s, lines.len());
    }
    (delta, diags)
}

/// Requirement names (normalised) per capability of the main specs under
/// `specs_dir` (`openspec/specs`), each file's text given by `read`.
#[must_use]
pub fn main_requirements(files: &[(String, String)]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (capability, text) in files {
        let names = out.entry(capability.clone()).or_default();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("### ")
                && let Some(name) = requirement_name(rest.trim())
            {
                names.insert(normalise(name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_renames() {
        assert_eq!(
            step("- **GIVEN** `X=1`"),
            Some(("given".to_owned(), "`X=1`".to_owned()))
        );
        assert_eq!(
            step("- WHEN it runs"),
            Some(("when".to_owned(), "it runs".to_owned()))
        );
        assert_eq!(step("- something else"), None);
        assert_eq!(
            rename_line("- FROM: `### Requirement: Greet a named person`"),
            Some(("from", "Greet a named person".to_owned()))
        );
        assert_eq!(normalise("  Greet   By name "), "greet by name");
    }
}
