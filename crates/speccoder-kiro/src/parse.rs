//! Kiro's files, read as written: `requirements.md` (numbered EARS
//! criteria under `### Requirement N`), `tasks.md` line by line (`[ ]*`
//! is not a Markdown task) and the steering with when it applies.

use speccoder_core::snapshot::{
    DependencyMode, Diagnostic, ParsedTasks, Phase, Severity, Task, Totals,
};

use crate::hash::sha256_hex;

/// One acceptance criterion: `N.M` and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    /// `1.2`: the requirement's number and the criterion's list number.
    pub id: String,
    /// The criterion as written (an EARS sentence).
    pub text: String,
}

/// One `### Requirement N` with its criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// `N`.
    pub number: String,
    /// The heading after `Requirement N:`, else `Requirement N`.
    pub title: String,
    /// The whole section, heading included.
    pub text: String,
    /// Its numbered acceptance criteria.
    pub criteria: Vec<Criterion>,
}

impl Requirement {
    /// `Requirement 1: Automatic labels`: how a person names it.
    #[must_use]
    pub fn key(&self) -> String {
        if self.title == format!("Requirement {}", self.number) {
            self.title.clone()
        } else {
            format!("Requirement {}: {}", self.number, self.title)
        }
    }
}

/// The requirements of a `requirements.md`, in order.
#[must_use]
pub fn requirements(text: &str) -> Vec<Requirement> {
    let mut out: Vec<Requirement> = Vec::new();
    let mut in_criteria = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            in_criteria = false;
            let rest = rest.trim();
            let Some(after) = rest.strip_prefix("Requirement ") else {
                continue;
            };
            let number: String = after.chars().take_while(char::is_ascii_digit).collect();
            if number.is_empty() {
                continue;
            }
            let title = after[number.len()..]
                .trim_start_matches([':', ' ', '-', '–'])
                .trim();
            out.push(Requirement {
                title: if title.is_empty() {
                    format!("Requirement {number}")
                } else {
                    title.to_owned()
                },
                number,
                text: format!("{line}\n"),
                criteria: Vec::new(),
            });
            continue;
        }
        if line.starts_with("## ") {
            in_criteria = false;
            continue;
        }
        let Some(r) = out.last_mut() else { continue };
        r.text.push_str(line);
        r.text.push('\n');
        if line.starts_with("#### ") {
            in_criteria = line.to_lowercase().contains("acceptance criteria");
            continue;
        }
        if !in_criteria {
            continue;
        }
        let trimmed = line.trim_start();
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && trimmed[digits.len()..].starts_with(". ") {
            r.criteria.push(Criterion {
                id: format!("{}.{digits}", r.number),
                text: trimmed[digits.len() + 2..].trim().to_owned(),
            });
        } else if !trimmed.is_empty()
            && line.starts_with([' ', '\t'])
            && let Some(c) = r.criteria.last_mut()
        {
            // A criterion's continuation line.
            c.text.push(' ');
            c.text.push_str(trimmed);
        }
    }
    out
}

/// A parent task: the engine ticks it with its last required sub-task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parent {
    /// `2`.
    pub id: String,
    /// Its line's byte range and hash (a `## 1. Group` heading has none).
    pub line: Option<([usize; 2], String)>,
    /// Ticked.
    pub done: bool,
    /// Its required sub-tasks.
    pub children: Vec<String>,
}

/// `tasks.md` read: the tasks speccoder runs, the parents it closes and
/// the optional tasks it leaves alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tasks {
    /// The runnable tasks, as every format's.
    pub parsed: ParsedTasks,
    /// Parent tasks, by id.
    pub parents: Vec<Parent>,
    /// Optional (`[ ]*`) tasks, by id.
    pub optional: Vec<String>,
}

/// One checkbox line: `- [ ]`, `[x]`, `[X]`, an optional `*` (after the
/// box or the number), a number.
struct Line {
    done: bool,
    open: bool,
    optional: bool,
    id: String,
    title: String,
}

fn checkbox(line: &str) -> Option<Line> {
    let indent = line.len() - line.trim_start().len();
    let rest = line[indent..].strip_prefix(['-', '*'])?.strip_prefix(' ')?;
    let body = rest.strip_prefix('[')?;
    let close = body.find(']')?;
    let inner = &body[..close];
    if inner.chars().count() > 1 {
        return None;
    }
    let after = &body[close + 1..];
    let (optional, after) = match after.strip_prefix('*') {
        Some(a) => (true, a),
        None => (false, after),
    };
    let after = after.trim_start();
    let id: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    // cc-sdd also writes the star after the number: `- [ ] 7.9* (P) …`.
    let optional = optional || after[id.len()..].starts_with('*');
    let id = id.trim_end_matches('.').to_owned();
    if id.is_empty() || !id.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let title = after[after.find(char::is_whitespace).unwrap_or(after.len())..]
        .trim()
        .to_owned();
    Some(Line {
        done: matches!(inner, "x" | "X"),
        open: inner == " ",
        optional,
        id,
        title,
    })
}

/// `_Requirements: 1.2, 3_` → `["1.2", "3"]`; free text cites nothing.
fn citations(detail: &str) -> Option<Vec<String>> {
    let d = detail.trim().trim_start_matches(['-', '*', ' ']);
    let inner = d
        .strip_prefix("_Requirements:")
        .and_then(|r| r.strip_suffix('_'))
        .or_else(|| {
            // `**Validates: Requirements 1.2**`: its leading `**` went with the marker.
            d.strip_prefix("Validates: Requirements")
                .map(|r| r.trim_end().trim_end_matches('*'))
        })?;
    Some(
        inner
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .map(str::to_owned)
            .collect(),
    )
}

/// `_Depends: 1.1, 1.2_` → the tasks named.
fn depends(detail: &str) -> Option<Vec<String>> {
    let d = detail.trim().trim_start_matches(['-', '*', ' ']);
    let inner = d.strip_prefix("_Depends:")?.strip_suffix('_')?;
    Some(
        inner
            .split(',')
            .map(|w| w.trim().trim_end_matches('.').to_owned())
            .filter(|w| !w.is_empty())
            .collect(),
    )
}

/// The parent's id of `id` (`2.1` → `2`), if it has one.
fn parent_of(id: &str) -> Option<&str> {
    id.rsplit_once('.').map(|(p, _)| p)
}

/// Reads a Kiro or cc-sdd `tasks.md` at `path` (for diagnostics).
/// Citations stay as written: a whole requirement (`3`) is expanded by the
/// adapter, which has the requirements.
#[must_use]
pub fn tasks(path: &str, text: &str) -> (Tasks, Vec<Diagnostic>) {
    struct Entry {
        line: Line,
        no: usize,
        range: [usize; 2],
        hash: String,
        cites: Vec<String>,
        depends: Vec<String>,
    }
    let mut diags = Vec::new();
    let diag = |severity, code: &str, no: usize, message: String| {
        Diagnostic::new(severity, code, message)
            .at(path)
            .line(u32::try_from(no + 1).unwrap_or(u32::MAX))
    };
    let mut entries: Vec<Entry> = Vec::new();
    // `## 1. Group` headings (cc-sdd): a parent without a box.
    let mut groups: Vec<(String, String, usize)> = Vec::new();
    let mut fence = false;
    let mut offset = 0;
    for (no, full) in text.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += full.len();
        let line = full.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            let number: String = h.chars().take_while(char::is_ascii_digit).collect();
            if !number.is_empty() && h[number.len()..].starts_with('.') {
                groups.push((number.clone(), h.trim().to_owned(), no));
            }
            continue;
        }
        if let Some(l) = checkbox(line) {
            entries.push(Entry {
                no,
                range: [start, start + line.len()],
                hash: sha256_hex(line.as_bytes()),
                line: l,
                cites: Vec::new(),
                depends: Vec::new(),
            });
            continue;
        }
        // A detail line belongs to the task above it.
        if let Some(e) = entries.last_mut()
            && line.len() > line.trim_start().len()
        {
            if let Some(c) = citations(line) {
                e.cites.extend(c);
            } else if let Some(d) = depends(line) {
                e.depends.extend(d);
            }
        }
    }
    let known =
        |id: &str| entries.iter().any(|e| e.line.id == id) || groups.iter().any(|g| g.0 == id);
    let mut seen = std::collections::BTreeSet::new();
    for e in &entries {
        if !seen.insert(e.line.id.clone()) {
            diags.push(diag(
                Severity::Error,
                "tasks.duplicate-id",
                e.no,
                format!("task {} appears twice", e.line.id),
            ));
        }
        if let Some(p) = parent_of(&e.line.id)
            && !known(p)
        {
            diags.push(diag(
                Severity::Error,
                "tasks.orphan",
                e.no,
                format!("task {} has no task {p} above it", e.line.id),
            ));
        }
        if !e.line.done && !e.line.open {
            diags.push(diag(
                Severity::Warning,
                "tasks.marker",
                e.no,
                format!(
                    "task {}'s box is neither open nor done: speccoder cannot tick it",
                    e.line.id
                ),
            ));
        }
    }
    let has_children = |id: &str| entries.iter().any(|c| parent_of(&c.line.id) == Some(id));
    let required_children = |id: &str| -> Vec<String> {
        entries
            .iter()
            .filter(|c| parent_of(&c.line.id) == Some(id) && !c.line.optional)
            .map(|c| c.line.id.clone())
            .collect()
    };
    let mut parents: Vec<Parent> = entries
        .iter()
        .filter(|e| has_children(&e.line.id))
        .map(|e| Parent {
            id: e.line.id.clone(),
            line: Some((e.range, e.hash.clone())),
            done: e.line.done,
            children: required_children(&e.line.id),
        })
        .collect();
    parents.extend(
        groups
            .iter()
            .filter(|g| !entries.iter().any(|e| e.line.id == g.0))
            .map(|g| Parent {
                id: g.0.clone(),
                line: None,
                done: false,
                children: required_children(&g.0),
            }),
    );
    let optional: Vec<String> = entries
        .iter()
        .filter(|e| e.line.optional)
        .map(|e| e.line.id.clone())
        .collect();
    // A group per top-level task: its title is the phase its sub-tasks show under.
    let top = |id: &str| id.split('.').next().unwrap_or(id).to_owned();
    let phase_title = |id: &str| -> String {
        let t = top(id);
        entries
            .iter()
            .find(|e| e.line.id == t)
            .map(|e| format!("{t}. {}", e.line.title))
            .or_else(|| groups.iter().find(|g| g.0 == t).map(|g| g.1.clone()))
            .unwrap_or_else(|| format!("{t}."))
    };
    let mut phases: Vec<Phase> = Vec::new();
    let mut tasks_out: Vec<Task> = Vec::new();
    for e in &entries {
        if e.line.optional || has_children(&e.line.id) {
            continue;
        }
        let phase = phase_title(&e.line.id);
        if phases.last().is_none_or(|p| p.title != phase) {
            phases.push(Phase {
                title: phase.clone(),
                line: u32::try_from(e.no + 1).unwrap_or(u32::MAX),
                section_hash: String::new(),
            });
        }
        let (parallel, title) = match e.line.title.strip_prefix("(P)") {
            Some(t) => (true, t.trim().to_owned()),
            None => (false, e.line.title.clone()),
        };
        for d in &e.depends {
            if !known(d) {
                diags.push(diag(
                    Severity::Error,
                    "tasks.unknown-dependency",
                    e.no,
                    format!(
                        "task {} depends on {d}, which is not in the list",
                        e.line.id
                    ),
                ));
            }
        }
        tasks_out.push(Task {
            id: e.line.id.clone(),
            done: e.line.done,
            parallel,
            story: None,
            yours: false,
            title,
            depends_on: e.depends.clone(),
            scenarios: e.cites.clone(),
            phase: Some(phase),
            subsection: None,
            line: u32::try_from(e.no + 1).unwrap_or(u32::MAX),
            byte_range: e.range,
            line_hash: e.hash.clone(),
        });
    }
    // Each group's section: from its first line to the next group's.
    let starts: Vec<usize> = phases
        .iter()
        .map(|p| {
            text.split_inclusive('\n')
                .take(p.line as usize - 1)
                .map(str::len)
                .sum()
        })
        .collect();
    for (i, p) in phases.iter_mut().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(text.len());
        p.section_hash = sha256_hex(&text.as_bytes()[starts[i]..end]);
    }
    let done = tasks_out.iter().filter(|t| t.done).count();
    let total = tasks_out.len();
    (
        Tasks {
            parsed: ParsedTasks {
                path: path.to_owned(),
                phases,
                tasks: tasks_out,
                totals: Totals {
                    total,
                    done,
                    open: total - done,
                },
                dependency_mode: DependencyMode::Chain,
            },
            parents,
            optional,
        },
        diags,
    )
}

/// When a steering file applies, as Kiro includes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    /// `inclusion: always`, or no front matter.
    Always,
    /// `inclusion: fileMatch`: work on files matching these patterns.
    Files(Vec<String>),
    /// `inclusion: auto`: work its description is about.
    About(String),
}

/// One steering file an attempt reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Its file name (`tech.md`).
    pub name: String,
    /// When it applies.
    pub when: When,
    /// Its text, front matter removed.
    pub text: String,
}

/// `.kiro/steering/` read: the rules, and the files left out with why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Steering {
    /// What an attempt reads, by file name.
    pub rules: Vec<Rule>,
    /// `(file, why)`: `manual` files, a `fileMatch` without a pattern.
    pub left_out: Vec<(String, String)>,
}

/// A front-matter value without its quotes.
fn unquote(v: &str) -> String {
    v.trim().trim_matches(['"', '\'']).to_owned()
}

/// `"a"`, `["a", "b"]`, or a `- a` list on the lines after the key.
fn patterns(front: &str) -> Vec<String> {
    let mut lines = front.lines();
    while let Some(l) = lines.next() {
        let Some(v) = l.trim_start().strip_prefix("fileMatchPattern:") else {
            continue;
        };
        let v = v.trim();
        if let Some(inner) = v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            return inner
                .split(',')
                .map(unquote)
                .filter(|p| !p.is_empty())
                .collect();
        }
        if !v.is_empty() {
            return vec![unquote(v)];
        }
        return lines
            .map_while(|l| l.trim_start().strip_prefix("- "))
            .map(unquote)
            .filter(|p| !p.is_empty())
            .collect();
    }
    Vec::new()
}

/// The value of `key:` in `front`.
fn value(front: &str, key: &str) -> Option<String> {
    front
        .lines()
        .find_map(|l| l.trim_start().strip_prefix(key)?.strip_prefix(':'))
        .map(unquote)
}

/// The steering in `dir` (`.kiro/steering`), by file name: `always` and
/// unmarked files as rules; `fileMatch` and `auto` with their condition;
/// `manual` left out, as Kiro leaves it out until a person names it.
#[must_use]
pub fn steering(dir: &std::path::Path) -> Steering {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Steering::default();
    };
    let mut files: Vec<_> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    let mut out = Steering::default();
    for p in files {
        let (Ok(text), Some(name)) = (std::fs::read_to_string(&p), p.file_name()) else {
            continue;
        };
        let name = name.to_string_lossy().into_owned();
        let Some((front, body)) = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
        else {
            out.rules.push(Rule {
                name,
                when: When::Always,
                text,
            });
            continue;
        };
        let body = body.trim_start().to_owned();
        let when = match value(front, "inclusion").as_deref() {
            None | Some("always") => When::Always,
            Some("fileMatch") => match patterns(front) {
                p if p.is_empty() => {
                    out.left_out
                        .push((name, "`fileMatch` without a `fileMatchPattern`".to_owned()));
                    continue;
                }
                p => When::Files(p),
            },
            Some("auto") => match value(front, "description").filter(|d| !d.is_empty()) {
                Some(d) => When::About(d),
                None => {
                    out.left_out
                        .push((name, "`auto` without a `description`".to_owned()));
                    continue;
                }
            },
            Some("manual") => {
                out.left_out
                    .push((name, "manual: read only when a person names it".to_owned()));
                continue;
            }
            Some(other) => {
                out.left_out
                    .push((name, format!("unknown inclusion `{other}`")));
                continue;
            }
        };
        out.rules.push(Rule {
            name,
            when,
            text: body,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(project: &str, file: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(project)
            .join(".kiro/specs")
            .join(project)
            .join(file);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    #[test]
    fn kiro_requirements_are_numbered_criteria() {
        let reqs = requirements(&fixture("issue-triage", "requirements.md"));
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].key(), "Requirement 1: Automatic labels");
        let ids: Vec<&str> = reqs[0].criteria.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["1.1", "1.2", "1.3"]);
        assert!(
            reqs[0].criteria[0]
                .text
                .starts_with("WHEN a new issue mentions \"crash\", THE Triage_Bot SHALL")
        );
        assert!(reqs[0].text.contains("**User Story:**"));
        assert_eq!(reqs[1].criteria.len(), 1);
    }

    #[test]
    fn cc_sdd_requirements_read_the_same() {
        let reqs = requirements(&fixture("photo-albums", "requirements.md"));
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].text.contains("**Objective:**"));
        assert!(
            reqs[0].criteria[1]
                .text
                .starts_with("IF an album contains no photos THEN")
        );
    }

    #[test]
    fn kiro_tasks_leave_parents_and_optional_tasks_to_the_engine() {
        let (t, diags) = tasks("tasks.md", &fixture("issue-triage", "tasks.md"));
        assert!(diags.is_empty(), "{diags:?}");
        let ids: Vec<&str> = t.parsed.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1", "2.1", "2.2", "3", "4"]);
        assert_eq!(t.optional, ["2.3"]);
        assert_eq!(t.parents.len(), 1);
        assert_eq!(t.parents[0].id, "2");
        assert_eq!(t.parents[0].children, ["2.1", "2.2"]);
        let task = |id: &str| t.parsed.tasks.iter().find(|t| t.id == id).unwrap();
        assert!(task("1").done, "ticked before speccoder ran");
        assert!(task("1").scenarios.is_empty(), "free text cites nothing");
        assert_eq!(task("2.2").scenarios, ["1.2", "1.3"]);
        assert_eq!(
            task("2.1").phase.as_deref(),
            Some("2. Implement the labelling rules")
        );
        assert_eq!(
            task("4").scenarios,
            ["2"],
            "a whole requirement, expanded by the adapter"
        );
        assert_eq!(task("3").title, "Checkpoint - Rules working");
        assert_eq!(t.parsed.dependency_mode, DependencyMode::Chain);
    }

    #[test]
    fn cc_sdd_tasks_find_parents_by_number_and_read_depends() {
        let (t, diags) = tasks("tasks.md", &fixture("photo-albums", "tasks.md"));
        assert!(diags.is_empty(), "{diags:?}");
        let ids: Vec<&str> = t.parsed.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1.1", "1.2", "2.1", "3"]);
        let task = |id: &str| t.parsed.tasks.iter().find(|t| t.id == id).unwrap();
        assert!(task("1.1").parallel);
        assert_eq!(task("1.1").title, "Create the album model in `src/lib.rs`");
        assert_eq!(task("2.1").depends_on, ["1.1"]);
        assert_eq!(
            t.parents.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    #[test]
    fn a_star_after_the_number_is_optional_too() {
        let (t, _) = tasks(
            "tasks.md",
            "- [ ] 7.8 E2E tests\n- [ ] 7.9* (P) Performance tests\n",
        );
        let ids: Vec<&str> = t.parsed.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["7.8"]);
        assert_eq!(t.optional, ["7.9"]);
    }

    #[test]
    fn a_validates_line_cites_its_criteria() {
        let (t, _) = tasks(
            "tasks.md",
            "- [ ] 1. Property test\n  - **Property 1: One label**\n  - **Validates: Requirements 1.2, 1.3**\n",
        );
        assert_eq!(t.parsed.tasks[0].scenarios, ["1.2", "1.3"]);
    }

    #[test]
    fn broken_lists_are_named() {
        let (_, diags) = tasks(
            "tasks.md",
            "- [ ] 1. A\n- [ ] 1. B\n- [ ] 3.1 C\n  - _Depends: 9_\n",
        );
        let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(
            codes,
            [
                "tasks.duplicate-id",
                "tasks.orphan",
                "tasks.unknown-dependency"
            ]
        );
    }

    #[test]
    fn steering_is_read_with_its_conditions() {
        let dir = tempfile::tempdir().unwrap();
        let w = |n: &str, t: &str| std::fs::write(dir.path().join(n), t).unwrap();
        w("tech.md", "# Tech\n");
        w("always.md", "---\ninclusion: always\n---\n\nAlways.\n");
        w(
            "ui.md",
            "---\ninclusion: fileMatch\nfileMatchPattern: [\"**/*.ts\", '**/*.tsx']\n---\nUI.\n",
        );
        w(
            "web.md",
            "---\ninclusion: fileMatch\nfileMatchPattern:\n  - \"web/**\"\n---\nWeb.\n",
        );
        w(
            "api.md",
            "---\ninclusion: auto\nname: api\ndescription: REST handlers\n---\nAPI.\n",
        );
        w("notes.md", "---\ninclusion: manual\n---\nNotes.\n");
        w("broken.md", "---\ninclusion: fileMatch\n---\nNo pattern.\n");
        let s = steering(dir.path());
        let rules: Vec<(&str, &When)> =
            s.rules.iter().map(|r| (r.name.as_str(), &r.when)).collect();
        assert_eq!(
            rules,
            [
                ("always.md", &When::Always),
                ("api.md", &When::About("REST handlers".to_owned())),
                ("tech.md", &When::Always),
                (
                    "ui.md",
                    &When::Files(vec!["**/*.ts".to_owned(), "**/*.tsx".to_owned()])
                ),
                ("web.md", &When::Files(vec!["web/**".to_owned()])),
            ]
        );
        assert_eq!(s.rules[0].text, "Always.\n");
        let left: Vec<&str> = s.left_out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(left, ["broken.md", "notes.md"]);
    }
}
