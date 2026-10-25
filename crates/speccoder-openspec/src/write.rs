//! The engine's two writes to a change's `tasks.md`: a one-byte tick and the
//! convergence append. Both re-read the file and refuse when it
//! changed since speccoder read it.

use std::path::Path;

use crate::hash::sha256_hex;
use crate::tasks::{self, checkbox};

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn replace(path: &Path, bytes: &[u8], suffix: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!("md.{suffix}"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Ticks `task_id`: writes `x` into its `[ ]`, one byte, nothing else.
///
/// Refused when the task's line no longer hashes to `expected_line_hash`,
/// when its box is anything but a single space (`[~]`, `[]`), and when it
/// is a parent with an open nested task.
pub fn tick(tasks_path: &Path, task_id: &str, expected_line_hash: &str) -> Result<(), String> {
    let text = read(tasks_path)?;
    let (parsed, _) = tasks::parse("tasks.md", &text);
    let task = parsed
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("{task_id} is not in {}", tasks_path.display()))?;
    if task.done {
        return Err(format!("{task_id} is already ticked"));
    }
    if task.line_hash != expected_line_hash {
        return Err(format!(
            "{task_id}'s line changed since the attempt began; not ticked"
        ));
    }
    let [start, end] = task.byte_range;
    let line = &text[start..end];
    let cb = checkbox(line).ok_or_else(|| format!("{task_id} has no checkbox"))?;
    if !cb.tickable() {
        return Err(format!(
            "{task_id}'s box is `[{}]`, not `[ ]`; speccoder ticks only an open box",
            cb.inner
        ));
    }
    // The checkboxes indented below it are its nested tasks.
    let mut open_child = false;
    for l in text[end..].lines().skip(1) {
        if l.trim().is_empty() {
            continue;
        }
        match checkbox(l) {
            Some(child) if child.indent > cb.indent => open_child |= !child.done(),
            _ => break,
        }
    }
    if open_child {
        return Err(format!("{task_id} has an open nested task; tick it first"));
    }
    let at = start + cb.inner_start;
    let mut bytes = text.into_bytes();
    bytes[at] = b'x';
    replace(tasks_path, &bytes, "speccoder-tick")
}

/// Appends `## N. Convergence` with one open task `N.k` per item, N after
/// the highest numbered group. The file must still hash to
/// `expected_file_hash`; the old bytes stay an exact prefix. No items → the
/// file is not touched. Returns the new IDs.
pub fn append_convergence(
    tasks_path: &Path,
    expected_file_hash: &str,
    items: &[String],
) -> Result<Vec<String>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let text = read(tasks_path)?;
    if sha256_hex(text.as_bytes()) != expected_file_hash {
        return Err(format!(
            "{} changed since convergence began; nothing appended",
            tasks_path.display()
        ));
    }
    let (parsed, _) = tasks::parse("tasks.md", &text);
    let group = parsed
        .phases
        .iter()
        .filter_map(|p| {
            let digits: String = p.title.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0)
        + 1;
    let mut section = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        section.push('\n');
    }
    section.push_str(&format!("\n## {group}. Convergence\n\n"));
    let mut ids = Vec::new();
    for (k, item) in items.iter().enumerate() {
        let id = format!("{group}.{}", k + 1);
        // One line each; a model's text never becomes structure.
        let line = item.split_whitespace().collect::<Vec<_>>().join(" ");
        let line = line.trim_start_matches(['-', '*', ' ']);
        let line = line.strip_prefix("[ ]").unwrap_or(line).trim();
        section.push_str(&format!("- [ ] {id} {line}\n"));
        ids.push(id);
    }
    let new = format!("{text}{section}");
    let (reparsed, _) = tasks::parse("tasks.md", &new);
    let same = parsed.tasks.iter().all(|old| {
        reparsed
            .tasks
            .iter()
            .any(|t| t.id == old.id && t.line_hash == old.line_hash && t.done == old.done)
    });
    let added = ids
        .iter()
        .all(|id| reparsed.tasks.iter().any(|t| &t.id == id && !t.done));
    if !new.starts_with(&text) || !same || !added {
        return Err(
            "the convergence group would change existing tasks; nothing appended".to_owned(),
        );
    }
    replace(tasks_path, new.as_bytes(), "speccoder-append")?;
    Ok(ids)
}
