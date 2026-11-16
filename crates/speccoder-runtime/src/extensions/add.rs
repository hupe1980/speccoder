//! `speccoder extensions add` and `remove`: an extension's folder copied
//! into the project, or deleted from it. Adding runs nothing of the
//! extension — no repository hooks, no submodules, no install scripts —
//! and a repository is taken only at a named commit.

use std::path::{Path, PathBuf};

use super::{DIR, Loaded};

/// Copies the extension at `source` — a folder, or `URL@<commit>` — into
/// the project at `root`, and returns it as loaded.
///
/// # Errors
///
/// The source cannot be read or fetched, is not loadable (the problems
/// named), or its name is taken.
pub fn add(root: &Path, source: &str) -> Result<Loaded, String> {
    let project = super::project(root);
    let fetched;
    let (from, origin) = if Path::new(source).is_dir() {
        (PathBuf::from(source), None)
    } else {
        if source.starts_with('-') {
            return Err(format!("`{source}` is not a folder or a repository URL"));
        }
        let (url, commit) = source.rsplit_once('@').ok_or_else(|| {
            format!("{source} is neither a folder nor URL@<commit> (a full 40-character commit)")
        })?;
        if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "`{commit}` is not a full commit: give URL@<40 hex characters>, so what you trust is what was fetched"
            ));
        }
        fetched = fetch(url, commit)?;
        (
            fetched.path().join("tree"),
            Some((url.to_owned(), commit.to_owned())),
        )
    };
    // Bounded before anything is copied: files only, within the limits.
    let mut bytes: u64 = 0;
    let listed = super::walk(&from, true)?;
    if listed.len() > speccoder_core::extensions::MAX_FILES {
        return Err(format!(
            "more than {} files: an extension holds at most that many",
            speccoder_core::extensions::MAX_FILES
        ));
    }
    for rel in &listed {
        let meta = std::fs::symlink_metadata(from.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        if !meta.file_type().is_file() {
            return Err(format!("{rel} is not a regular file"));
        }
        bytes += meta.len();
        if bytes > speccoder_core::extensions::MAX_BYTES {
            return Err("more than 20 MB: an extension holds at most that much".to_owned());
        }
    }
    let staged = tempfile::tempdir().map_err(|e| e.to_string())?;
    let name_dir = staged.path().join("x");
    copy(&from, &name_dir)?;
    // Where it came from is speccoder's to say, never the folder's own claim.
    let _ = std::fs::remove_file(name_dir.join(super::NAMESPACE).join("source.toml"));
    if let Some((url, commit)) = &origin {
        let part = name_dir.join(super::NAMESPACE);
        std::fs::create_dir_all(&part).map_err(|e| e.to_string())?;
        let text = format!(
            "# Where `speccoder extensions add` took this extension from.\nurl = {url:?}\ncommit = {commit:?}\nadded = {:?}\n",
            crate::attest::now_rfc3339()
        );
        std::fs::write(part.join("source.toml"), text).map_err(|e| e.to_string())?;
    }
    // The name is plugin.json's: read it before choosing the folder.
    let name = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(name_dir.join("plugin.json")).map_err(|_| {
            format!("{source} has no plugin.json: an extension is an Agent Plugins 1.0 folder")
        })?,
    )
    .ok()
    .and_then(|v| v["name"].as_str().map(str::to_owned))
    .ok_or_else(|| format!("{source}: plugin.json names no extension"))?;
    if !speccoder_core::extensions::valid_name(&name) {
        return Err(format!("`{name}` is not a valid extension name"));
    }
    let target = project.join(DIR).join(&name);
    if target.exists() || super::load(&project).iter().any(|e| e.name == name) {
        return Err(format!(
            "`{name}` is already here: `speccoder extensions remove {name}` first"
        ));
    }
    std::fs::create_dir_all(project.join(DIR)).map_err(|e| e.to_string())?;
    let renamed = staged.path().join(&name);
    std::fs::rename(&name_dir, &renamed).map_err(|e| e.to_string())?;
    let loaded = super::one(
        &project,
        &renamed,
        speccoder_core::extensions::ExtensionSource::AgentPlugins,
    );
    if !loaded.usable() {
        return Err(format!(
            "{name} cannot be loaded: {}",
            loaded.problems.join("; ")
        ));
    }
    copy(&renamed, &target)?;
    super::load(&project)
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("{name} was copied but does not load"))
}

/// Deletes the extension `name` and its trust record.
///
/// # Errors
///
/// No such extension, or it is Spec Kit's (Spec Kit removes those).
pub fn remove(root: &Path, state: &Path, name: &str) -> Result<(), String> {
    let all = super::load(root);
    let Some(e) = all.iter().find(|e| e.name == name) else {
        return Err(format!("there is no extension `{name}`"));
    };
    if e.source == speccoder_core::extensions::ExtensionSource::SpecKit {
        return Err(format!(
            "{name} is a Spec Kit extension: `specify extension remove {name}` removes it"
        ));
    }
    std::fs::remove_dir_all(&e.folder).map_err(|err| format!("{}: {err}", e.rel))?;
    crate::hooks::trust::forget(&crate::hooks::dir(state, root), &super::trust::key(name));
    Ok(())
}

/// `url` at `commit`, checked out in a temporary folder (`tree/`), with no
/// hook of the repository and no submodule run.
fn fetch(url: &str, commit: &str) -> Result<tempfile::TempDir, String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let tree = tmp.path().join("tree");
    let no_hooks = tmp.path().join("no-hooks");
    std::fs::create_dir_all(&no_hooks).map_err(|e| e.to_string())?;
    let local = !url.contains("://") && !url.contains('@');
    let git = |args: &[&str]| -> Result<(), String> {
        let hooks = format!("core.hooksPath={}", no_hooks.display());
        let mut all = vec!["-c", hooks.as_str(), "-c", "submodule.recurse=false"];
        if !local {
            all.extend(["-c", "protocol.file.allow=never"]);
        }
        all.extend_from_slice(args);
        crate::git::Git::at(&tree)
            .quiet(&all, &[("GIT_CONFIG_NOSYSTEM", std::ffi::OsStr::new("1"))])
            .map(|_| ())
            .map_err(|e| {
                if e.0.starts_with("cannot run git") {
                    format!("git is needed to add from a repository ({e}); add a folder instead")
                } else {
                    e.0
                }
            })
    };
    std::fs::create_dir_all(&tree).map_err(|e| e.to_string())?;
    git(&["init", "--quiet"])?;
    git(&["fetch", "--quiet", "--depth", "1", "--", url, commit])
        .map_err(|e| format!("could not fetch {commit} from {url}: {e}"))?;
    git(&["checkout", "--quiet", "FETCH_HEAD"])?;
    Ok(tmp)
}

/// Copies regular files and folders from `from` to `to`, leaving `.git`
/// out and refusing links.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let dest = to.join(entry.file_name());
        if kind.is_symlink() {
            return Err(format!(
                "{} is a link: an extension holds files only",
                path.display()
            ));
        } else if kind.is_dir() {
            copy(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}
