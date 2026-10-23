//! Spec Kit checklists: `<feature>/checklists/*.md`, whose unchecked items
//! `/speckit-implement` treats as a stop-and-ask.

use std::path::Path;

use speccoder_core::ChecklistStatus;

/// Each checklist of the feature directory `feature_dir` (relative paths
/// from `root`), sorted by file; empty when there is no `checklists/`.
#[must_use]
pub fn status(root: &Path, feature_dir: &str) -> Vec<ChecklistStatus> {
    let dir = root.join(feature_dir).join("checklists");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let (mut done, mut open) = (0, 0);
            for line in text.lines() {
                let t = line.trim_start();
                let t = t
                    .strip_prefix("- ")
                    .or_else(|| t.strip_prefix("* "))
                    .unwrap_or("");
                if t.starts_with("[ ]") {
                    open += 1;
                } else if t.starts_with("[x]") || t.starts_with("[X]") {
                    done += 1;
                }
            }
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some(ChecklistStatus {
                file: format!("{feature_dir}/checklists/{name}"),
                total: done + open,
                done,
                open,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_checked_and_open_items() {
        let dir = tempfile::tempdir().unwrap();
        assert!(status(dir.path(), "specs/001-x").is_empty());
        let c = dir.path().join("specs/001-x/checklists");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(
            c.join("ux.md"),
            "# UX\n- [x] a\n- [X] b\n  - [ ] c\n* [ ] d\ntext [ ] e\n",
        )
        .unwrap();
        std::fs::write(c.join("api.md"), "- [x] a\n").unwrap();
        std::fs::write(c.join("notes.txt"), "- [ ] ignored\n").unwrap();
        let s = status(dir.path(), "specs/001-x");
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].file.as_str(), s[0].open),
            ("specs/001-x/checklists/api.md", 0)
        );
        assert_eq!((s[1].total, s[1].done, s[1].open), (4, 2, 2));
    }
}
