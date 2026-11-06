//! The engine's writes to a spec's `tasks.md`: a tick (with its parent
//! when it was the last required sub-task) and the convergence append.
//! Both re-read the file and refuse when it changed since speccoder
//! read it.

use std::path::Path;

use crate::hash::sha256_hex;
use crate::parse;

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn replace(path: &Path, bytes: &[u8], suffix: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!("md.{suffix}"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Where `x` goes in the box of the line at `range`.
fn box_at(text: &str, range: [usize; 2]) -> Option<usize> {
    let line = &text[range[0]..range[1]];
    let open = line.find("[ ]")?;
    Some(range[0] + open + 1)
}

/// Ticks `task_id`: `x` into its `[ ]`; and into its parent's when every
/// required sibling is ticked now. Nothing else changes.
///
/// # Errors
///
/// The task is not open, its line changed since it was read, or the file
/// cannot be written.
pub fn tick(path: &Path, task_id: &str, expected_line_hash: &str) -> Result<(), String> {
    let text = read(path)?;
    let (t, _) = parse::tasks("tasks.md", &text);
    let task = t
        .parsed
        .tasks
        .iter()
        .find(|x| x.id == task_id)
        .ok_or_else(|| format!("{task_id} is not in {}", path.display()))?;
    if task.done {
        return Err(format!("{task_id} is already ticked"));
    }
    if task.line_hash != expected_line_hash {
        return Err(format!(
            "{task_id}'s line changed since the attempt began; not ticked"
        ));
    }
    let at = box_at(&text, task.byte_range)
        .ok_or_else(|| format!("{task_id}'s box is not `[ ]`; speccoder ticks only an open box"))?;
    let mut bytes = text.clone().into_bytes();
    bytes[at] = b'x';
    let done = |id: &str| id == task_id || t.parsed.tasks.iter().any(|x| x.id == id && x.done);
    if let Some(parent) = task_id
        .rsplit_once('.')
        .and_then(|(p, _)| t.parents.iter().find(|x| x.id == p))
        && !parent.done
        && parent.children.iter().all(|c| done(c))
        && let Some((range, _)) = parent.line
        && let Some(p_at) = box_at(&text, range)
    {
        bytes[p_at] = b'x';
    }
    replace(path, &bytes, "speccoder-tick")
}

/// Ticks every open required task and every open parent (never an
/// optional one): a bugfix spec's task list, closed by the bug's verified
/// fix. Returns the ids ticked, in file order; nothing else changes.
///
/// # Errors
///
/// The file cannot be read or written.
pub fn close_all(path: &Path) -> Result<Vec<String>, String> {
    let text = read(path)?;
    let (t, _) = parse::tasks("tasks.md", &text);
    let mut boxes: Vec<(usize, String)> = t
        .parsed
        .tasks
        .iter()
        .filter(|x| !x.done)
        .filter_map(|x| Some((box_at(&text, x.byte_range)?, x.id.clone())))
        .chain(
            t.parents
                .iter()
                .filter(|p| !p.done)
                .filter_map(|p| Some((box_at(&text, p.line.as_ref()?.0)?, p.id.clone()))),
        )
        .collect();
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    boxes.sort();
    let mut bytes = text.into_bytes();
    for (at, _) in &boxes {
        bytes[*at] = b'x';
    }
    replace(path, &bytes, "speccoder-tick")?;
    Ok(boxes.into_iter().map(|(_, id)| id).collect())
}

/// Appends `- [ ] N. Convergence` with one open sub-task `N.k` per item,
/// N after the highest top-level number. The file must still hash to
/// `expected_file_hash`; the old bytes stay an exact prefix.
///
/// # Errors
///
/// The file changed since convergence began, the append would change an
/// existing task, or the file cannot be written.
pub fn append_convergence(
    path: &Path,
    expected_file_hash: &str,
    items: &[String],
) -> Result<Vec<String>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let text = read(path)?;
    if sha256_hex(text.as_bytes()) != expected_file_hash {
        return Err(format!(
            "{} changed since convergence began; nothing appended",
            path.display()
        ));
    }
    let (t, _) = parse::tasks("tasks.md", &text);
    let top = t
        .parsed
        .tasks
        .iter()
        .map(|x| x.id.as_str())
        .chain(t.parents.iter().map(|p| p.id.as_str()))
        .chain(t.optional.iter().map(String::as_str))
        .filter_map(|id| id.split('.').next()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let mut section = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        section.push('\n');
    }
    section.push_str(&format!("\n- [ ] {top}. Convergence\n"));
    let mut ids = Vec::new();
    for (k, item) in items.iter().enumerate() {
        let id = format!("{top}.{}", k + 1);
        // One line each; a model's text never becomes structure.
        let line = item.split_whitespace().collect::<Vec<_>>().join(" ");
        let line = line.trim_start_matches(['-', '*', ' ']);
        let line = line.strip_prefix("[ ]").unwrap_or(line).trim();
        section.push_str(&format!("  - [ ] {id} {line}\n"));
        ids.push(id);
    }
    let new = format!("{text}{section}");
    let (again, _) = parse::tasks("tasks.md", &new);
    let same = t.parsed.tasks.iter().all(|old| {
        again
            .parsed
            .tasks
            .iter()
            .any(|x| x.id == old.id && x.line_hash == old.line_hash && x.done == old.done)
    });
    let added = ids
        .iter()
        .all(|id| again.parsed.tasks.iter().any(|x| &x.id == id && !x.done));
    if !new.starts_with(&text) || !same || !added {
        return Err(
            "the convergence group would change existing tasks; nothing appended".to_owned(),
        );
    }
    replace(path, new.as_bytes(), "speccoder-append")?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASKS: &str =
        "- [ ] 2. Rules\n  - [ ] 2.1 One\n  - [x] 2.2 Two\n  - [ ]* 2.3 Optional\n- [ ] 3. Three\n";

    fn file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("tasks.md");
        std::fs::write(&p, text).unwrap();
        (dir, p)
    }

    #[test]
    fn the_last_required_sub_task_closes_its_parent() {
        let (_d, p) = file(TASKS);
        let (t, _) = parse::tasks("tasks.md", TASKS);
        let hash = t
            .parsed
            .tasks
            .iter()
            .find(|x| x.id == "2.1")
            .unwrap()
            .line_hash
            .clone();
        tick(&p, "2.1", &hash).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "- [x] 2. Rules\n  - [x] 2.1 One\n  - [x] 2.2 Two\n  - [ ]* 2.3 Optional\n- [ ] 3. Three\n"
        );
        assert!(tick(&p, "2.1", &hash).is_err(), "never twice");
    }

    #[test]
    fn a_closed_bugfix_ticks_every_required_box_and_no_optional_one() {
        let (_d, p) = file(TASKS);
        assert_eq!(close_all(&p).unwrap(), ["2", "2.1", "3"]);
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "- [x] 2. Rules\n  - [x] 2.1 One\n  - [x] 2.2 Two\n  - [ ]* 2.3 Optional\n- [x] 3. Three\n"
        );
        assert!(close_all(&p).unwrap().is_empty(), "nothing left to close");
    }

    #[test]
    fn a_changed_line_is_not_ticked() {
        let (_d, p) = file(TASKS);
        assert!(
            tick(&p, "3", "not-the-hash")
                .unwrap_err()
                .contains("changed")
        );
    }

    #[test]
    fn convergence_is_the_next_number() {
        let (_d, p) = file(TASKS);
        let ids = append_convergence(
            &p,
            &sha256_hex(TASKS.as_bytes()),
            &["Handle an empty title".to_owned()],
        )
        .unwrap();
        assert_eq!(ids, ["4.1"]);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.starts_with(TASKS));
        assert!(
            text.ends_with("\n- [ ] 4. Convergence\n  - [ ] 4.1 Handle an empty title\n"),
            "{text}"
        );
    }
}
