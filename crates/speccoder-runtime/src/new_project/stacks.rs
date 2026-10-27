//! The stacks' own scaffolds and the tools they need: the exact commands
//! each ecosystem documents for starting a project, and how a tool's
//! version is read. speccoder writes none of a stack's files itself.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use speccoder_core::new_project::Stack;

/// A step of a stack's scaffold.
pub(crate) struct Recipe {
    pub argv: Vec<String>,
    pub hosts: Vec<String>,
    pub writes: &'static str,
    /// Done when this exists in the folder (resume).
    pub marker: &'static str,
    pub note: Option<&'static str>,
}

fn words(w: &[&str]) -> Vec<String> {
    w.iter().map(|s| (*s).to_owned()).collect()
}

/// The scaffold of `stack` for a project named `name`, after `git init`.
/// Cargo writes its own `.gitignore` inside a repository, so it runs with
/// its default version control (it does not init a second repository).
pub(crate) fn recipe(stack: Stack, name: &str) -> Vec<Recipe> {
    match stack {
        Stack::Rust => vec![
            Recipe {
                argv: words(&["cargo", "init", "--lib", "--name", name]),
                hosts: Vec::new(),
                writes: "Cargo.toml, src/lib.rs, .gitignore",
                marker: "Cargo.toml",
                note: None,
            },
            // Cargo's library template is a placeholder `add` with its test;
            // the first task would find that test frozen and could not
            // replace `add`. An empty library builds, and its tests start at
            // none.
            Recipe {
                argv: words(&["cp", "/dev/null", "src/lib.rs"]),
                hosts: Vec::new(),
                writes: "src/lib.rs (emptied of cargo's placeholder `add` and its test)",
                marker: "",
                note: None,
            },
            // An empty file is not rustfmt-clean: the project's `fmt` check
            // would fail before the first task.
            Recipe {
                argv: words(&["cargo", "fmt", "--all"]),
                hosts: Vec::new(),
                writes: "src/lib.rs (formatted)",
                marker: "",
                note: None,
            },
        ],
        Stack::Python => vec![
            Recipe {
                argv: words(&["uv", "init", "--package", "--vcs", "none", "--name", name]),
                hosts: Vec::new(),
                writes: "pyproject.toml, src/, README.md, .python-version",
                marker: "pyproject.toml",
                note: None,
            },
            Recipe {
                argv: words(&["uv", "add", "--dev", "pytest"]),
                hosts: words(&["pypi.org", "files.pythonhosted.org"]),
                writes: "pyproject.toml, uv.lock, .venv/",
                marker: "uv.lock",
                note: None,
            },
        ],
        Stack::Node => vec![
            Recipe {
                argv: words(&["npm", "init", "-y"]),
                hosts: Vec::new(),
                writes: "package.json",
                marker: "package.json",
                note: None,
            },
            Recipe {
                argv: words(&["npm", "install", "--save-dev", "vitest"]),
                hosts: words(&["registry.npmjs.org"]),
                writes: "package.json, package-lock.json, node_modules/",
                marker: "package-lock.json",
                note: Some("node_modules/ is not committed: add it to a .gitignore"),
            },
            Recipe {
                argv: words(&[
                    "npm",
                    "pkg",
                    "set",
                    "scripts.test=vitest run --passWithNoTests",
                ]),
                hosts: Vec::new(),
                writes: "package.json (scripts.test)",
                marker: "",
                note: None,
            },
        ],
        Stack::Go => vec![Recipe {
            argv: words(&["go", "mod", "init", name]),
            hosts: Vec::new(),
            writes: "go.mod",
            marker: "go.mod",
            note: None,
        }],
        Stack::None => Vec::new(),
    }
}

/// The stack an existing repository's manifests show, and the file that shows it.
pub(crate) fn detect(root: &Path) -> (Stack, Option<&'static str>) {
    for (file, stack) in [
        ("Cargo.toml", Stack::Rust),
        ("pyproject.toml", Stack::Python),
        ("package.json", Stack::Node),
        ("go.mod", Stack::Go),
    ] {
        if root.join(file).is_file() {
            return (stack, Some(file));
        }
    }
    (Stack::None, None)
}

/// How long a version probe may take.
const PROBE: Duration = Duration::from_secs(5);
/// How long a probe's answer is reused (the sheet recomputes the plan on
/// every change; the perform recomputes it fresh).
const REUSE: Duration = Duration::from_secs(60);

type Cache = BTreeMap<(String, String), (Instant, Option<String>)>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: std::sync::OnceLock<Mutex<Cache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// The version `tool` reports (`cargo 1.91.0 (…)` → `1.91.0`), or `None`
/// when it is not on the `PATH`. `fresh`: never a remembered answer.
pub(crate) fn version(tool: &str, fresh: bool) -> Option<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let key = (tool.to_owned(), path);
    if !fresh
        && let Ok(c) = cache().lock()
        && let Some((at, v)) = c.get(&key)
        && at.elapsed() < REUSE
    {
        return v.clone();
    }
    let v = probe(tool);
    if let Ok(mut c) = cache().lock() {
        c.insert(key, (Instant::now(), v.clone()));
    }
    v
}

fn probe(tool: &str) -> Option<String> {
    let env: Vec<(String, String)> = std::env::vars().collect();
    let cwd = std::env::temp_dir();
    crate::sandbox::resolve_executable(tool, &cwd, &env)?;
    // Go answers `go version`, the others `--version`.
    let args: &[&str] = if tool == "go" {
        &["version"]
    } else {
        &["--version"]
    };
    let mut child = std::process::Command::new(tool)
        .args(args)
        .current_dir(&cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if began.elapsed() > PROBE => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(String::new());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return Some(String::new()),
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(number(&text))
}

/// The first version-looking word of `text` (`go1.25.1` → `1.25.1`).
pub(crate) fn number(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(|w| w.trim_start_matches("go").trim_start_matches('v'))
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_the_tools_own_lines() {
        assert_eq!(number("cargo 1.91.0 (ea2d97820 2025-10-10)"), "1.91.0");
        assert_eq!(number("go version go1.25.1 darwin/arm64"), "1.25.1");
        assert_eq!(number("11.6.2\n"), "11.6.2");
        assert_eq!(number("specify 1.2.3"), "1.2.3");
        assert_eq!(number("uv 0.9.5 (Homebrew 2025-10-21)"), "0.9.5");
        assert_eq!(number("nothing here"), "");
    }

    #[test]
    fn every_stack_scaffolds_with_its_own_tool() {
        for stack in Stack::ALL {
            for r in recipe(stack, "md2csv") {
                // The stack's own tool — or, for Rust, emptying cargo's
                // placeholder library.
                let placeholder = r.argv == ["cp", "/dev/null", "src/lib.rs"];
                assert!(
                    placeholder || Some(r.argv[0].as_str()) == stack.tool(),
                    "{stack:?}: {:?}",
                    r.argv
                );
                assert_eq!(
                    r.hosts.is_empty(),
                    !r.argv.contains(&"add".to_owned()) && !r.argv.contains(&"install".to_owned())
                );
            }
        }
        assert!(recipe(Stack::None, "x").is_empty());
    }
}
