//! Bugs in OpenSpec: one standing change, `openspec/changes/bugfixes/`,
//! appended like any change — each bug a requirement of the `bugfixes`
//! capability whose scenarios are the person's Expected and Unchanged
//! statements, a line of the proposal and a task group. Appends only.

use std::path::Path;

use speccoder_core::bug::{BugHome, HomeKind, Triad, normalise};

use crate::format::{BugRefusal, ReadBug, RecordedBug};

/// The standing change.
const CHANGE: &str = "bugfixes";
/// Where it lives.
const DIR: &str = "openspec/changes/bugfixes";

/// The home: the standing change.
pub(super) fn unit() -> BugHome {
    BugHome {
        kind: HomeKind::StandingOpenspec,
        location: DIR.to_owned(),
        reason: "OpenSpec: the standing bug-fix change".to_owned(),
    }
}

/// The highest `prefix<n>` in `text`.
fn highest(text: &str, prefix: &str) -> u32 {
    text.match_indices(prefix)
        .filter_map(|(i, _)| {
            text[i + prefix.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

/// Records `triad` in the standing change, creating it the first time.
pub(super) fn record(root: &Path, triad: &Triad) -> Result<RecordedBug, BugRefusal> {
    let io = |e: String| BugRefusal::new("INTERNAL", e);
    let dir = root.join(DIR);
    let spec_dir = dir.join("specs").join(CHANGE);
    std::fs::create_dir_all(&spec_dir).map_err(|e| io(format!("{}: {e}", spec_dir.display())))?;
    let meta = dir.join(".openspec.yaml");
    let proposal_path = dir.join("proposal.md");
    let tasks_path = dir.join("tasks.md");
    let spec_path = spec_dir.join("spec.md");
    let mut proposal = std::fs::read_to_string(&proposal_path).unwrap_or_else(|_| {
        "# Proposal\n\n## Why\n\nBugs reported one at a time (`speccoder bug`): each is a requirement in the reporter's words — Current, Expected, Unchanged — reproduced before it is fixed.\n\n## What Changes\n".to_owned()
    });
    let mut spec = std::fs::read_to_string(&spec_path)
        .unwrap_or_else(|_| "## ADDED Requirements\n".to_owned());
    let mut tasks = std::fs::read_to_string(&tasks_path).unwrap_or_else(|_| "# Tasks\n".to_owned());
    let n = highest(&spec, "### Requirement: B") + 1;
    let group = highest(&tasks, "\n## ") + 1;
    let title = triad.title();
    let requirement = format!("B{n}");
    let mut block = format!(
        "\n### Requirement: {requirement} {title}\nA bug, in the reporter's words.\n\nCurrent: {}\n",
        triad.current.text
    );
    for e in &triad.expected {
        block.push_str(&format!("Expected: {}\n", e.text));
    }
    for u in &triad.unchanged {
        block.push_str(&format!("Unchanged: {}\n", u.text));
    }
    for s in triad.expected.iter().chain(&triad.unchanged) {
        block.push_str(&format!(
            "\n#### Scenario: {}\n- **THEN** {}\n",
            s.id, s.text
        ));
    }
    if !spec.ends_with('\n') {
        spec.push('\n');
    }
    spec.push_str(&block);
    // The person's words read back from what is about to be written,
    // exactly, or nothing is written.
    let faithful = parse(&spec, &requirement, String::new()).is_ok_and(|back| {
        back.current == [triad.current.text.clone()]
            && back.expected
                == triad
                    .expected
                    .iter()
                    .map(|s| s.text.clone())
                    .collect::<Vec<_>>()
            && back.unchanged
                == triad
                    .unchanged
                    .iter()
                    .map(|s| s.text.clone())
                    .collect::<Vec<_>>()
    });
    if !faithful {
        return Err(BugRefusal::new(
            "BUG-ASSESSMENT-DRIFT",
            format!(
                "{requirement} would not read back exactly your words; nothing is written — reword it"
            ),
        ));
    }
    if !proposal.ends_with('\n') {
        proposal.push('\n');
    }
    proposal.push_str(&format!("- {requirement}: {title}\n"));
    if !tasks.ends_with('\n') {
        tasks.push('\n');
    }
    let task = format!("{group}.1");
    tasks.push_str(&format!(
        "\n## {group}. {title}\n\n- [ ] {task} Fix: {title} ({requirement})\n"
    ));
    for (path, new) in [
        (&proposal_path, &proposal),
        (&spec_path, &spec),
        (&tasks_path, &tasks),
    ] {
        if let Ok(now) = std::fs::read_to_string(path)
            && !new.starts_with(now.trim_end_matches('\n'))
        {
            return Err(BugRefusal::new(
                "INTERNAL",
                format!(
                    "{} changed while the bug was being recorded; try again",
                    path.display()
                ),
            ));
        }
    }
    if !meta.exists() {
        std::fs::write(&meta, "schema: spec-driven\n").map_err(|e| io(e.to_string()))?;
    }
    std::fs::write(&proposal_path, proposal).map_err(|e| io(e.to_string()))?;
    std::fs::write(&spec_path, spec).map_err(|e| io(e.to_string()))?;
    std::fs::write(&tasks_path, tasks).map_err(|e| io(e.to_string()))?;
    Ok(RecordedBug {
        unit: CHANGE.to_owned(),
        dir: DIR.to_owned(),
        requirement,
        task,
        title,
        files: vec![
            format!("{DIR}/proposal.md"),
            format!("{DIR}/specs/{CHANGE}/spec.md"),
            format!("{DIR}/tasks.md"),
        ],
    })
}

/// The triad of requirement `key` (`B3`).
pub(super) fn read(root: &Path, key: &str) -> Result<ReadBug, BugRefusal> {
    let file = format!("{DIR}/specs/{CHANGE}/spec.md");
    let text = std::fs::read_to_string(root.join(&file))
        .map_err(|_| BugRefusal::new("BUG-NO-UNIT", format!("{file} is missing")))?;
    parse(&text, key, file)
}

/// [`read`] of the spec's `text`.
fn parse(text: &str, key: &str, file: String) -> Result<ReadBug, BugRefusal> {
    let head = format!("### Requirement: {key} ");
    let mut r = ReadBug {
        current: Vec::new(),
        expected: Vec::new(),
        unchanged: Vec::new(),
        title: None,
        file,
    };
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("### Requirement: ") {
            inside = line.starts_with(&head);
            if inside {
                r.title = Some(line[head.len()..].trim().to_owned());
            }
            continue;
        }
        if !inside || line.starts_with("#### ") {
            if line.starts_with("#### ") && inside {
                inside = false;
            }
            continue;
        }
        if let Some((label, rest)) = line.split_once(':') {
            let words = normalise(rest);
            match label.trim() {
                "Current" => r.current.push(words),
                "Expected" => r.expected.push(words),
                "Unchanged" => r.unchanged.push(words),
                _ => {}
            }
        }
    }
    if r.current.is_empty() || r.expected.is_empty() {
        return Err(BugRefusal::new(
            "BUG-ASSESSMENT-UNREADABLE",
            format!("{key} in {} has no Current and Expected lines", r.file),
        ));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bug_is_a_requirement_a_line_of_the_proposal_and_a_task_group() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let t = |c: &str, e: &str| {
            Triad::new(
                c,
                &[e.to_owned()],
                &["a title is saved".to_owned()],
                crate::integrity::sha256_hex,
            )
            .unwrap_or_else(|e| panic!("{e:?}"))
        };
        let a = record(
            dir.path(),
            &t("saving an empty title crashes", "an empty title is refused"),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((a.requirement.as_str(), a.task.as_str()), ("B1", "1.1"));
        let b = record(
            dir.path(),
            &t("long titles are cut", "a long title is kept whole"),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((b.requirement.as_str(), b.task.as_str()), ("B2", "2.1"));
        let read = read(dir.path(), "B2").unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(read.expected, ["a long title is kept whole"]);
        assert_eq!(read.unchanged, ["a title is saved"]);
        let tasks =
            std::fs::read_to_string(dir.path().join(DIR).join("tasks.md")).unwrap_or_default();
        assert!(
            tasks.contains("- [ ] 2.1 Fix: long titles are cut (B2)"),
            "{tasks}"
        );
        let spec = std::fs::read_to_string(dir.path().join(DIR).join("specs/bugfixes/spec.md"))
            .unwrap_or_default();
        assert!(
            spec.contains("#### Scenario: E1\n- **THEN** an empty title is refused"),
            "{spec}"
        );
    }
}
