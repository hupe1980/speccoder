//! The Python Spec Kit's scripts run. `.specify/scripts/bash/common.sh`
//! resolves preset templates with a Python that imports PyYAML — its own
//! override `SPECKIT_PYTHON_EXECUTABLE`, else `python3` — and the system
//! `python3` usually has no PyYAML, so with a preset installed
//! `setup-tasks.sh` finds no tasks template. The installed `specify` runs
//! on a Python that has it (Spec Kit depends on PyYAML): speccoder finds
//! that interpreter, checks that it imports `yaml`, and gives it to every
//! command of a Spec Kit project — unless the person set the override.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Spec Kit's override, read by `common.sh`.
pub const VAR: &str = "SPECKIT_PYTHON_EXECUTABLE";
/// Its deprecated predecessor, still read when the override is unset.
const LEGACY: &str = "SPECKIT_PYTHON";

/// How long the probe of an interpreter may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What `common.sh` itself requires of the interpreter: Python 3, PyYAML.
const PROBE: &str = "import sys, yaml; raise SystemExit(sys.version_info.major != 3)";

/// The fix, when no Python with PyYAML is found.
pub const FIX: &[&str] = &[
    "python3 -m pip install pyyaml",
    "export SPECKIT_PYTHON_EXECUTABLE=/path/to/a/python-with-pyyaml",
];

/// The variables a command of a Spec Kit project gets: the override, set
/// to the installed `specify`'s own interpreter when it imports PyYAML.
/// `env` looks up the command's environment (its `PATH` finds `specify`);
/// nothing when the person set the override or its predecessor, or when
/// no such interpreter is found.
#[must_use]
pub fn command_env(env: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    if set(VAR) || set(LEGACY) {
        return Vec::new();
    }
    specify_interpreter(env("PATH").as_deref())
        .map(|p| vec![(VAR.to_owned(), p.display().to_string())])
        .unwrap_or_default()
}

/// The interpreter of the `specify` that `path` finds, when it imports
/// PyYAML and a sandboxed command can read it (remembered per `specify`
/// for the process: every command asks).
#[must_use]
pub fn specify_interpreter(path: Option<&str>) -> Option<PathBuf> {
    static FOUND: OnceLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = OnceLock::new();
    let lookup = path.map(|p| vec![("PATH".to_owned(), p.to_owned())]);
    let specify = crate::sandbox::resolve_executable(
        "specify",
        Path::new("/"),
        lookup.as_deref().unwrap_or_default(),
    )?;
    let cache = FOUND.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(found) = cache.lock()
        && let Some(hit) = found.get(&specify)
    {
        return hit.clone();
    }
    let hit = discover(&specify, path);
    if let Ok(mut found) = cache.lock() {
        found.insert(specify, hit.clone());
    }
    hit
}

/// The first interpreter of the `specify` at `specify` (canonical) that
/// imports PyYAML: its shebang's (`#!/…/python`, or `#!/usr/bin/env
/// python3` on `path`), then the interpreters beside it (a virtual
/// environment's `bin/`, as `uv tool` and `pipx` install it, whose
/// launcher may be a `/bin/sh` trampoline).
#[must_use]
pub fn discover(specify: &Path, path: Option<&str>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(p) = shebang(specify, path) {
        candidates.push(p);
    }
    if let Some(dir) = specify.parent() {
        candidates.extend(["python3", "python"].map(|n| dir.join(n)));
    }
    let hidden = hidden();
    candidates
        .into_iter()
        .filter(|p| !hidden.iter().any(|h| p.starts_with(h)))
        .find(|p| imports_yaml(p))
}

/// The Python a script's `#!` line names, if it names one.
fn shebang(script: &Path, path: Option<&str>) -> Option<PathBuf> {
    use std::io::Read as _;
    let mut head = [0u8; 512];
    let n = std::fs::File::open(script)
        .and_then(|mut f| f.read(&mut head))
        .ok()?;
    let text = String::from_utf8_lossy(&head[..n]);
    let line = text.lines().next()?.strip_prefix("#!")?;
    let mut words = line.split_whitespace();
    let program = PathBuf::from(words.next()?);
    let python = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("python"))
    };
    if program.file_name().is_some_and(|n| n == "env") {
        let name = words.find(|w| !w.starts_with('-'))?;
        let lookup = path.map(|p| vec![("PATH".to_owned(), p.to_owned())]);
        return python(Path::new(name))
            .then(|| {
                crate::sandbox::resolve_executable(
                    name,
                    Path::new("/"),
                    lookup.as_deref().unwrap_or_default(),
                )
            })
            .flatten();
    }
    python(&program).then_some(program)
}

/// The trees a sandboxed command cannot read.
fn hidden() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            crate::sandbox::HIDDEN
                .iter()
                .map(|h| home.join(h))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `python` is a Python 3 that imports `yaml`, as `common.sh`
/// asks it (run outside any project: a `yaml.py` there is not PyYAML).
#[must_use]
pub fn imports_yaml(python: &Path) -> bool {
    let Ok(mut child) = Command::new(python)
        .args(["-c", PROBE])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if began.elapsed() > PROBE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
}

/// The presets installed in the project at `root` (`.specify/presets/<id>/`).
#[must_use]
pub fn presets(root: &Path) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(root.join(".specify/presets"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    ids.sort();
    ids
}

/// The Python Spec Kit's scripts would resolve preset templates with,
/// given the person's environment `env`: the override when it imports
/// PyYAML, the installed `specify`'s interpreter (what speccoder passes),
/// else `python3` on `PATH`. `Err`: none imports PyYAML — the reason.
///
/// # Errors
///
/// No Python with PyYAML is found.
pub fn resolver(env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let path = env("PATH");
    for var in [VAR, LEGACY] {
        if let Some(v) = env(var).filter(|v| !v.is_empty()) {
            let lookup = path.as_ref().map(|p| vec![("PATH".to_owned(), p.clone())]);
            let found = crate::sandbox::resolve_executable(
                &v,
                Path::new("/"),
                lookup.as_deref().unwrap_or_default(),
            );
            if found.as_deref().is_some_and(imports_yaml) {
                return Ok(format!("{var}={v}"));
            }
            return Err(format!(
                "{var}={v} is not a Python 3 that imports PyYAML, and an override set by you is kept"
            ));
        }
    }
    if let Some(p) = specify_interpreter(path.as_deref()) {
        return Ok(format!("{} (specify's own Python)", p.display()));
    }
    let lookup = path.as_ref().map(|p| vec![("PATH".to_owned(), p.clone())]);
    let python3 = crate::sandbox::resolve_executable(
        "python3",
        Path::new("/"),
        lookup.as_deref().unwrap_or_default(),
    );
    match python3 {
        Some(p) if imports_yaml(&p) => Ok(format!("{} (python3 on PATH)", p.display())),
        _ => Err(
            "no Python with PyYAML found: neither specify's own interpreter nor python3 on PATH imports yaml"
                .to_owned(),
        ),
    }
}

/// The warning for the project at `root`: presets are installed and no
/// Python Spec Kit's scripts would use imports PyYAML (`None`: no preset,
/// or one does).
#[must_use]
pub fn warning(root: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let presets = presets(root);
    if presets.is_empty() {
        return None;
    }
    resolver(env).err().map(|why| {
        format!(
            "Spec Kit preset{} {} installed, but {why}: Spec Kit's scripts cannot resolve preset templates, so `speccoder spec tasks` stops (\"Template 'tasks-template' was not found\")",
            if presets.len() == 1 { "" } else { "s" },
            presets.join(", ")
        )
    })
}

/// The same before a preset is installed into the project at `root`.
#[must_use]
pub fn warning_before_add(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    resolver(env).err().map(|why| {
        format!(
            "{why}: once a preset is installed, Spec Kit's scripts cannot resolve its templates, so `speccoder spec tasks` stops (\"Template 'tasks-template' was not found\")"
        )
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    /// A fake `specify` in `<dir>/bin` whose shebang names `<dir>/venv/python`,
    /// a script that "imports yaml" when `<dir>/venv/yaml.py` exists.
    fn fake(dir: &Path, yaml: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = dir.join("bin");
        let venv = dir.join("venv");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&venv).unwrap();
        let python = venv.join("python");
        std::fs::write(
            &python,
            "#!/bin/sh\ncase \"$2\" in *'import sys, yaml'*) [ -f \"$(dirname \"$0\")/yaml.py\" ] && exit 0; exit 1;; esac\nexit 2\n",
        )
        .unwrap();
        if yaml {
            std::fs::write(venv.join("yaml.py"), "").unwrap();
        }
        let specify = bin.join("specify");
        std::fs::write(
            &specify,
            format!("#!{}\nprint('specify')\n", python.display()),
        )
        .unwrap();
        for f in [&python, &specify] {
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        python
    }

    #[test]
    fn the_shebang_interpreter_is_passed_when_it_imports_yaml() {
        let d = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(d.path()).unwrap();
        let python = fake(&dir, true);
        let path = format!("{}:/usr/bin:/bin", dir.join("bin").display());
        let env = |k: &str| (k == "PATH").then(|| path.clone());
        assert_eq!(
            command_env(&env),
            vec![(VAR.to_owned(), python.display().to_string())]
        );
        // The person's own value is never overridden.
        for var in [VAR, LEGACY] {
            let mine = |k: &str| match k {
                "PATH" => Some(path.clone()),
                k if k == var => Some("/mine/python".to_owned()),
                _ => None,
            };
            assert!(command_env(&mine).is_empty(), "{var}");
        }
    }

    #[test]
    fn an_interpreter_without_yaml_is_not_passed_and_warns_with_a_preset() {
        let d = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(d.path()).unwrap();
        fake(&dir, false);
        let path = format!("{}", dir.join("bin").display());
        let env = |k: &str| (k == "PATH").then(|| path.clone());
        assert!(command_env(&env).is_empty());
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(warning(&root, &env), None, "no preset, no warning");
        std::fs::create_dir_all(root.join(".specify/presets/speccoder")).unwrap();
        let w = warning(&root, &env).unwrap_or_default();
        assert!(w.contains("speccoder") && w.contains("PyYAML"), "{w}");
        assert!(warning_before_add(&env).is_some());
    }

    #[test]
    fn a_uv_tool_trampoline_finds_the_interpreter_beside_it() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(d.path()).unwrap();
        let python = fake(&dir, true);
        // uv's launcher for a long path: a /bin/sh trampoline in the venv.
        let specify = dir.join("venv/specify");
        std::fs::write(
            &specify,
            "#!/bin/sh\n'''exec' \"$(dirname -- \"$(realpath -- \"$0\")\")\"/'python' \"$0\" \"$@\"\n' '''\n",
        )
        .unwrap();
        std::fs::set_permissions(&specify, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(discover(&specify, None), Some(python));
    }
}
