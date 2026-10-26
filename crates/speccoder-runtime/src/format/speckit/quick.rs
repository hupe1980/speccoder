//! Quick changes in Spec Kit's layout: one standing feature,
//! `NNN-quick-changes`, where each request becomes a user story with its
//! acceptance scenarios, a requirement and a task — so it is specified,
//! reviewed, run and judged like any other.

use std::path::Path;

use super::super::Recorded;

/// The standing feature's name, after its number.
const NAME: &str = "quick-changes";

/// A request's title: its first sentence, at most eight words.
fn title(request: &str) -> String {
    let first = request
        .split(['.', '\n', '!', '?'])
        .find(|s| !s.trim().is_empty())
        .unwrap_or(request)
        .trim();
    let words: Vec<&str> = first.split_whitespace().take(8).collect();
    let mut t = words.join(" ");
    if first.split_whitespace().count() > 8 {
        t.push('…');
    }
    t
}

/// The highest number after `prefix` in `text` (`FR-`, `T`, `User Story `).
pub(super) fn highest(text: &str, prefix: &str) -> u32 {
    text.match_indices(prefix)
        .filter_map(|(i, _)| {
            let digits: String = text[i + prefix.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .max()
        .unwrap_or(0)
}

/// Records `request` (and its `checks`) in the standing feature under
/// `root`, creating it the first time.
pub(super) fn record(root: &Path, request: &str, checks: &[String]) -> Result<Recorded, String> {
    let request = request.trim();
    if request.is_empty() {
        return Err("describe the change: `speccoder do \"…\"`".to_owned());
    }
    let specs = root.join(speccoder_speckit::features::SPECS);
    let existing = std::fs::read_dir(&specs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| {
            n.ends_with(&format!("-{NAME}")) && speccoder_speckit::features::prefix(n).is_some()
        });
    let name = match existing {
        Some(n) => n,
        None => {
            let next = std::fs::read_dir(&specs)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let p = speccoder_speckit::features::prefix(&n)?.to_owned();
                    (p.len() <= 4).then(|| p.parse::<u32>().ok()).flatten()
                })
                .max()
                .unwrap_or(0)
                + 1;
            format!("{next:03}-{NAME}")
        }
    };
    let dir = specs.join(&name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let spec_path = dir.join("spec.md");
    let tasks_path = dir.join("tasks.md");
    let mut spec = std::fs::read_to_string(&spec_path).unwrap_or_else(|_| {
        format!(
            "# Feature Specification: Quick Changes\n\n**Feature Branch**: `{name}`\n**Status**: Ongoing\n**Input**: Small changes asked for one at a time (`speccoder do`). Each is a user story with its own requirement and task, specified, reviewed, run and judged like any other.\n\n## User Scenarios & Testing\n"
        )
    });
    let mut tasks = std::fs::read_to_string(&tasks_path)
        .unwrap_or_else(|_| "# Tasks: Quick Changes\n".to_owned());
    let story = highest(&spec, "### User Story ") + 1;
    let fr = highest(&spec, "**FR-") + 1;
    let task = highest(&tasks, "- [ ] T")
        .max(highest(&tasks, "- [x] T"))
        .max(highest(&tasks, "- [X] T"))
        + 1;
    let t = title(request);
    let one_line = request.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut block = format!("\n### User Story {story} - {t} (Priority: P1)\n\n{request}\n");
    if !checks.is_empty() {
        block.push_str("\n**Acceptance Scenarios**:\n\n");
        for (i, c) in checks.iter().enumerate() {
            block.push_str(&format!("{}. {}\n", i + 1, c.trim()));
        }
    }
    let requirement = format!("FR-{fr:03}");
    block.push_str(&format!("\n- **{requirement}**: {one_line}\n"));
    if !spec.ends_with('\n') {
        spec.push('\n');
    }
    spec.push_str(&block);
    let task_id = format!("T{task:03}");
    if !tasks.ends_with('\n') {
        tasks.push('\n');
    }
    tasks.push_str(&format!(
        "\n## Phase {story}: User Story {story} - {t} (Priority: P1)\n\n- [ ] {task_id} [US{story}] {t} per {requirement}\n"
    ));
    // Appends only: what was there is an exact prefix of what is written,
    // checked again right before writing (principle I).
    for (path, new) in [(&spec_path, &spec), (&tasks_path, &tasks)] {
        if let Ok(now) = std::fs::read_to_string(path)
            && !new.starts_with(now.trim_end_matches('\n'))
        {
            return Err(format!(
                "{} changed while the change was being recorded; try again",
                path.display()
            ));
        }
    }
    std::fs::write(&spec_path, spec).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    std::fs::write(&tasks_path, tasks).map_err(|e| format!("{}: {e}", tasks_path.display()))?;
    Ok(Recorded {
        feature: name.clone(),
        dir: format!("{}/{name}", speccoder_speckit::features::SPECS),
        task: task_id,
        requirement,
        title: t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_first_sentence_in_eight_words() {
        assert_eq!(
            title("Make the login button blue. It is grey now."),
            "Make the login button blue"
        );
        assert_eq!(
            title("Show the version number in the footer of every page please"),
            "Show the version number in the footer of…"
        );
        assert_eq!(highest("- **FR-002**: a\n- **FR-010**: b", "**FR-"), 10);
    }
}
