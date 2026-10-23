//! Installed Agent Skills: `SKILL.md` files under `.claude/skills/`
//! and `.agents/skills/`, as spec formats install their commands.
//!
//! speccoder hosts the planning skills (`speckit-specify`, `-clarify`,
//! `-plan`, `-tasks`, `-analyze`, `-checklist`, `-constitution`, and others)
//! with its own model and policy. It refuses the ones it does itself, with
//! evidence (`speckit-implement` → `speccoder run`, `speckit-converge` →
//! `speccoder converge`), and the ones that reach outside the repository.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::format::Planning;

/// Where skills are installed, relative to the project root.
pub const DIRS: [&str; 2] = [".claude/skills", ".agents/skills"];

/// One installed skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skill {
    /// `name` from the frontmatter (else the directory name).
    pub name: String,
    /// `description` from the frontmatter.
    pub description: String,
    /// The `SKILL.md`, relative to the root.
    pub path: String,
    /// `user-invocable` is not `false`.
    pub invocable: bool,
    /// Why speccoder will not host it, if it will not.
    pub refused: Option<String>,
}

/// Skills speccoder does not host in a project of the reference format, and
/// what to use instead.
#[must_use]
pub fn refusal(name: &str) -> Option<String> {
    (crate::format::reference().planning().refused)(name)
}

/// The frontmatter's simple `key: value` lines and the body after it.
#[must_use]
pub fn split(text: &str) -> (Vec<(String, String)>, String) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (Vec::new(), text.to_owned());
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), text.to_owned());
    };
    let fields = rest[..end]
        .lines()
        .filter(|l| !l.starts_with(' ') && !l.starts_with('\t'))
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| {
            let v = v.trim().trim_matches('"').trim_matches('\'').to_owned();
            (k.trim().to_owned(), v)
        })
        .collect();
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']).to_owned();
    (fields, body)
}

/// Every installed skill under `root`, by name, for a project of the
/// reference format.
#[must_use]
pub fn discover(root: &Path) -> Vec<Skill> {
    discover_with(root, crate::format::reference().planning())
}

/// [`discover`] for a project whose format hosts `planning`.
#[must_use]
pub fn discover_with(root: &Path, planning: &Planning) -> Vec<Skill> {
    let mut out: Vec<Skill> = Vec::new();
    for dir in DIRS {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for e in entries.flatten() {
            let file = e.path().join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let (fields, _) = split(&text);
            let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
            let name = get("name").unwrap_or_else(|| e.file_name().to_string_lossy().into_owned());
            if out.iter().any(|s| s.name == name) {
                continue; // the first directory wins
            }
            out.push(Skill {
                refused: (planning.refused)(&name),
                description: get("description").unwrap_or_default(),
                path: format!("{dir}/{}/SKILL.md", e.file_name().to_string_lossy()),
                invocable: get("user-invocable").is_none_or(|v| v != "false"),
                name,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The skill's instruction with `$ARGUMENTS` replaced (Spec Kit's convention).
pub fn instruction(root: &Path, skill: &Skill, arguments: &str) -> Result<String, String> {
    let path: PathBuf = root.join(&skill.path);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (_, body) = split(&text);
    Ok(body.replace("$ARGUMENTS", arguments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_and_body_split_and_nested_keys_are_ignored() {
        let text = "---\nname: \"speckit-plan\"\ndescription: \"Plan it.\"\nmetadata:\n  author: \"x\"\nuser-invocable: true\n---\n\n## User Input\n\n$ARGUMENTS\n";
        let (fields, body) = split(text);
        assert_eq!(
            fields,
            [
                ("name".to_owned(), "speckit-plan".to_owned()),
                ("description".to_owned(), "Plan it.".to_owned()),
                ("metadata".to_owned(), String::new()),
                ("user-invocable".to_owned(), "true".to_owned()),
            ]
        );
        assert!(body.starts_with("## User Input"));
        assert!(refusal("speckit-implement").is_some() && refusal("speckit-plan").is_none());
    }
}
