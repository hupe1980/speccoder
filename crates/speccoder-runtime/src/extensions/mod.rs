//! A project's extensions: Agent Plugins 1.0 folders in
//! `.speccoder/extensions/<name>/` and Spec Kit's installed extensions,
//! each with speccoder's part in `dev.speccoder/extension.toml`. Loaded as
//! contributions speccoder already runs — `[hooks]`, hosted skills, MCP
//! servers — plus views. One digest covers every file of an extension;
//! nothing of it runs, is offered or reaches a model before a person
//! trusted that digest ([`trust`]).

pub mod add;
pub mod report;
pub mod trust;
pub mod views;

use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use speccoder_core::extensions::{
    self as core, AddedFrom, Contribution, ContributionKind, ExtensionSource, ViewDef, ViewSource,
    hook_contribution, view_contribution,
};
use speccoder_core::hooks::{HookDef, HookFile, TrustState};

/// Where Agent Plugins folders live, relative to the project.
pub const DIR: &str = ".speccoder/extensions";
/// speccoder's namespace in an Agent Plugins folder.
pub const NAMESPACE: &str = "dev.speccoder";

const PLUGIN_KEYS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];

/// An `mcp.json` stdio server, its placeholders not yet expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// `<ext>_<server>`.
    pub name: String,
    /// `command` then `args`.
    pub command: Vec<String>,
    /// Literal variables.
    pub env: Vec<(String, String)>,
    /// `cwd`, if given.
    pub cwd: Option<String>,
}

/// A Spec Kit hook an extension declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// `after_specify`.
    pub event: String,
    /// `speckit.threatspec.model`.
    pub command: String,
    /// What it asks.
    pub prompt: Option<String>,
    /// A mandatory hook.
    pub mandatory: bool,
}

/// One extension as loaded.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Its name.
    pub name: String,
    /// Agent Plugins or Spec Kit.
    pub source: ExtensionSource,
    /// Its folder.
    pub folder: PathBuf,
    /// Its folder, relative to the project.
    pub rel: String,
    /// Its version.
    pub version: Option<String>,
    /// What it says it is.
    pub description: Option<String>,
    /// Where it was added from.
    pub origin: Option<AddedFrom>,
    /// `sha256:<hex>` over every file.
    pub digest: String,
    /// Every file and its digest.
    pub files: Vec<HookFile>,
    /// Its `[hooks]`, named `<ext>.<hook>`.
    pub hooks: Vec<HookDef>,
    /// Its `[views]`.
    pub views: Vec<ViewDef>,
    /// Its skills: `(<ext>:<skill>, SKILL.md)`.
    pub skills: Vec<(String, PathBuf)>,
    /// Its stdio MCP servers.
    pub servers: Vec<Server>,
    /// Spec Kit: its commands.
    pub commands: Vec<String>,
    /// Spec Kit: its hooks.
    pub offers: Vec<Offer>,
    /// Why it cannot be loaded.
    pub problems: Vec<String>,
}

impl Loaded {
    /// Whether nothing keeps it from loading.
    #[must_use]
    pub fn usable(&self) -> bool {
        self.problems.is_empty()
    }

    /// What it contributes, in words.
    #[must_use]
    pub fn contributions(&self) -> Vec<Contribution> {
        let mut out: Vec<Contribution> = self
            .hooks
            .iter()
            .map(|h| hook_contribution(&h.name, h))
            .collect();
        out.extend(
            self.views
                .iter()
                .map(|v| view_contribution(&format!("{}.{}", self.name, v.name), v)),
        );
        out.extend(self.skills.iter().map(|(n, _)| Contribution {
            kind: ContributionKind::Skill,
            name: format!("/{n}"),
            line: format!("skill /{n}: a prompt you run from the composer"),
            reaches: Vec::new(),
        }));
        out.extend(self.servers.iter().map(|s| Contribution {
            kind: ContributionKind::McpServer,
            name: s.name.clone(),
            line: format!(
                "MCP server {}: starts {} in the sandbox; each call asks unless allowed",
                s.name,
                core::shown(&s.command)
            ),
            reaches: vec![format!("server:{}", s.name)],
        }));
        out.extend(self.commands.iter().map(|c| Contribution {
            kind: ContributionKind::Command,
            name: c.clone(),
            line: format!("command {c}: a prompt you run from the palette"),
            reaches: Vec::new(),
        }));
        // One line per command and kind: the events it stands at.
        let mut seen: Vec<(String, bool)> = Vec::new();
        for o in &self.offers {
            if seen.contains(&(o.command.clone(), o.mandatory)) {
                continue;
            }
            seen.push((o.command.clone(), o.mandatory));
            let events: Vec<&str> = self
                .offers
                .iter()
                .filter(|x| x.command == o.command && x.mandatory == o.mandatory)
                .map(|x| x.event.as_str())
                .collect();
            out.push(Contribution {
                kind: ContributionKind::HookOffer,
                name: format!(
                    "{} {}",
                    if o.mandatory { "runs" } else { "offers" },
                    o.command
                ),
                line: if o.mandatory {
                    format!(
                        "runs {} at {} (Spec Kit hook)",
                        o.command,
                        events.join(", ")
                    )
                } else {
                    format!("offers {} at {}", o.command, events.join(", "))
                },
                reaches: if o.mandatory {
                    events.iter().map(|e| format!("hook:{e}")).collect()
                } else {
                    Vec::new()
                },
            });
        }
        out
    }
}

/// The project folder of `root`: where `speccoder.toml` is, else `root`.
#[must_use]
pub fn project(root: &Path) -> PathBuf {
    crate::verify::find_config(root).unwrap_or_else(|| root.to_path_buf())
}

/// Every extension of the project at `root`, by name.
#[must_use]
pub fn load(root: &Path) -> Vec<Loaded> {
    let project = project(root);
    let mut out = Vec::new();
    let mut folders: Vec<PathBuf> = std::fs::read_dir(project.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        // A link is listed too: it is refused by name, never followed.
        .filter(|p| {
            (p.is_dir() || std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()))
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    folders.sort();
    for folder in folders {
        out.push(one(&project, &folder, ExtensionSource::AgentPlugins));
    }
    // The format's own extensions, through its adapter.
    for folder in crate::format::speckit::extensions::folders(&project) {
        out.push(one(&project, &folder, ExtensionSource::SpecKit));
    }
    // An MCP server name taken twice — by another extension, or by a
    // `[mcp]` server of the project — makes the later one not loadable.
    let mut servers: Vec<String> = crate::verify::find_config(&project)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| crate::mcp::parse(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.name)
        .collect();
    for e in &mut out {
        for s in &e.servers {
            if servers.contains(&s.name) {
                e.problems.push(format!(
                    "mcp.json server {}: the name is taken by another MCP server",
                    s.name
                ));
            } else {
                servers.push(s.name.clone());
            }
        }
    }
    // A name taken twice: the second is not loadable.
    for i in 0..out.len() {
        if let Some(first) = out[..i].iter().find(|e| e.name == out[i].name) {
            let line = format!("the name `{}` is taken by {}", out[i].name, first.rel);
            out[i].problems.push(line);
        }
    }
    out
}

/// The usable extensions a person trusted, as they are now.
#[must_use]
pub fn trusted(root: &Path, state: &Path) -> Vec<Loaded> {
    let dir = crate::hooks::dir(state, root);
    load(root)
        .into_iter()
        .filter(|e| e.usable() && trust::state(&dir, e) == TrustState::Trusted)
        .collect()
}

pub(crate) fn one(project: &Path, folder: &Path, source: ExtensionSource) -> Loaded {
    let rel = folder
        .strip_prefix(project)
        .unwrap_or(folder)
        .to_string_lossy()
        .replace('\\', "/");
    let dir_name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut e = Loaded {
        name: dir_name.clone(),
        source,
        folder: folder.to_path_buf(),
        rel,
        version: None,
        description: None,
        origin: None,
        digest: String::new(),
        files: Vec::new(),
        hooks: Vec::new(),
        views: Vec::new(),
        skills: Vec::new(),
        servers: Vec::new(),
        commands: Vec::new(),
        offers: Vec::new(),
        problems: Vec::new(),
    };
    match digest(folder) {
        Ok((d, files)) => {
            e.digest = d;
            e.files = files;
        }
        Err(p) => {
            e.problems.push(p);
            return e;
        }
    }
    match source {
        ExtensionSource::AgentPlugins => plugin(&mut e, &dir_name),
        ExtensionSource::SpecKit => {
            spec_kit(&mut e);
            installed_skills(&mut e, project);
        }
    }
    speccoder_part(&mut e);
    check_paths(&mut e);
    e
}

/// Variables an `mcp.json` may not set: they decide which program runs or
/// what it loads before its own code.
const LOADER_VARS: &[&str] = &[
    "PATH",
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "RUBYOPT",
    "RUBYLIB",
    "PERL5OPT",
    "PERL5LIB",
    "BASH_ENV",
    "ENV",
    "JAVA_TOOL_OPTIONS",
];

/// Whether `arg` climbs out of where it starts or into `.git`.
fn escapes(arg: &str) -> bool {
    arg.split(['/', '\\']).any(|c| c == ".." || c == ".git")
}

/// What an extension runs is its own: a `./` program is one of its
/// digested files, no argument climbs out with `..` or into `.git`, and its
/// MCP servers set no loader variable.
fn check_paths(e: &mut Loaded) {
    let own = |p: &str| {
        let rel = p.trim_start_matches("./");
        e.files.iter().any(|f| f.path == rel)
    };
    let mut problems = Vec::new();
    let check = |what: &str, argv: &[String], problems: &mut Vec<String>| {
        if let Some(a) = argv.iter().find(|a| escapes(a)) {
            problems.push(format!("{what}: `{a}` leaves the extension's folder"));
        }
        if let Some(p) = argv.first().filter(|p| p.starts_with("./"))
            && !own(p)
        {
            problems.push(format!(
                "{what}: runs `{p}`, which is not one of its committed files"
            ));
        }
    };
    for h in &e.hooks {
        if let Some(argv) = h.argv() {
            check(&format!("[hooks.{}]", h.name), argv, &mut problems);
        }
    }
    for v in &e.views {
        if let ViewSource::Run(argv) = &v.source {
            check(&format!("[views.{}]", v.name), argv, &mut problems);
        }
    }
    for s in &e.servers {
        check(
            &format!("mcp.json server {}", s.name),
            &s.command,
            &mut problems,
        );
        if s.cwd.as_deref().is_some_and(escapes) {
            problems.push(format!(
                "mcp.json server {}: its cwd leaves the extension's folder",
                s.name
            ));
        }
        if let Some((k, _)) = s.env.iter().find(|(k, _)| {
            LOADER_VARS.contains(&k.as_str()) || k.starts_with("LD_") || k.starts_with("DYLD_")
        }) {
            problems.push(format!(
                "mcp.json server {}: it may not set {k} (it decides what runs)",
                s.name
            ));
        }
        if !crate::mcp::valid_server_name(&s.name) {
            problems.push(format!(
                "mcp.json server {}: not a valid server name (lowercase letters, digits, single `_`)",
                s.name
            ));
        }
    }
    e.problems.extend(problems);
}

/// `plugin.json`, `skills/`, `mcp.json`.
fn plugin(e: &mut Loaded, dir_name: &str) {
    let text = match std::fs::read_to_string(e.folder.join("plugin.json")) {
        Ok(t) => t,
        Err(_) => {
            e.problems
                .push("no plugin.json (an Agent Plugins 1.0 folder starts with one)".to_owned());
            return;
        }
    };
    let manifest: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(err) => {
            e.problems.push(format!("plugin.json: {err}"));
            return;
        }
    };
    let Some(m) = manifest.as_object() else {
        e.problems.push("plugin.json is not an object".to_owned());
        return;
    };
    let name = m
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !core::valid_name(name) {
        e.problems.push(format!(
            "plugin.json: `name` \"{name}\" is not a valid name (1–64 of a-z, 0-9, `-`, `.`)"
        ));
        return;
    }
    if name != dir_name {
        e.problems.push(format!(
            "plugin.json names `{name}`, but the folder is `{dir_name}`: they must match"
        ));
        return;
    }
    for (k, v) in m {
        let is_string = v.is_string();
        let bad = match k.as_str() {
            "version" | "description" | "homepage" | "repository" | "license" | "$schema" => {
                !is_string
            }
            "keywords" => !v
                .as_array()
                .is_some_and(|a| a.iter().all(serde_json::Value::is_string)),
            _ => false,
        };
        if bad {
            e.problems
                .push(format!("plugin.json: `{k}` has the wrong type"));
        }
    }
    // An unknown top-level field is ignored (Agent Plugins §5.2); a type
    // error in a known one is fatal (above).
    debug_assert!(PLUGIN_KEYS.contains(&"name"));
    e.name = name.to_owned();
    e.version = m.get("version").and_then(|v| v.as_str()).map(str::to_owned);
    e.description = m
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    // skills/<name>/SKILL.md, one level deep.
    let mut skills: Vec<(String, PathBuf)> = std::fs::read_dir(e.folder.join("skills"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|d| {
            let file = d.path().join("SKILL.md");
            let skill = d.file_name().to_str()?.to_owned();
            file.is_file().then(|| (format!("{name}:{skill}"), file))
        })
        .collect();
    skills.sort();
    e.skills = skills;
    if let Ok(text) = std::fs::read_to_string(e.folder.join("mcp.json")) {
        mcp(e, &text);
    }
}

/// `mcp.json`: stdio servers; remote ones are not supported yet.
fn mcp(e: &mut Loaded, text: &str) {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(err) => {
            e.problems.push(format!("mcp.json: {err}"));
            return;
        }
    };
    let Some(servers) = v.get("mcpServers").and_then(|s| s.as_object()) else {
        e.problems.push("mcp.json: no `mcpServers`".to_owned());
        return;
    };
    let placeholders_ok = |s: &str| {
        let mut rest = s;
        while let Some(i) = rest.find("${") {
            let after = &rest[i + 2..];
            let Some(end) = after.find('}') else {
                return false;
            };
            if !matches!(&after[..end], "PLUGIN_ROOT" | "PLUGIN_DATA") {
                return false;
            }
            rest = &after[end + 1..];
        }
        true
    };
    for (server, s) in servers {
        let at = |m: &str| format!("mcp.json server `{server}`: {m}");
        match s.get("type").and_then(|t| t.as_str()) {
            Some("stdio") => {}
            Some(t @ ("streamable-http" | "sse")) => {
                e.problems.push(at(&format!(
                    "`{t}` servers are not supported by this version (stdio only)"
                )));
                continue;
            }
            _ => {
                e.problems
                    .push(at("`type` is stdio, streamable-http or sse"));
                continue;
            }
        }
        let command = s
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or_default();
        let bare = !command.is_empty()
            && !command.contains("${")
            && (command.starts_with("./") || !command.contains(['/', '\\']));
        if !bare {
            e.problems.push(at(
                "`command` is a bare program name or a `./` path, without placeholders",
            ));
            continue;
        }
        let args: Vec<String> = s
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let env: Vec<(String, String)> = s
            .get("env")
            .and_then(|a| a.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = s.get("cwd").and_then(|c| c.as_str()).map(str::to_owned);
        let all_ok = args.iter().all(|a| placeholders_ok(a))
            && env
                .iter()
                .all(|(k, v)| placeholders_ok(v) && k != "PLUGIN_ROOT" && k != "PLUGIN_DATA")
            && cwd.as_deref().is_none_or(|c| {
                placeholders_ok(c)
                    && (c.starts_with("./")
                        || c.starts_with("${PLUGIN_ROOT}")
                        || c.starts_with("${PLUGIN_DATA}"))
            });
        if !all_ok {
            e.problems.push(at(
                "only ${PLUGIN_ROOT} and ${PLUGIN_DATA} may appear in args, env and cwd",
            ));
            continue;
        }
        let mut argv = vec![command.to_owned()];
        argv.extend(args);
        e.servers.push(Server {
            name: core::server_name(&e.name, server),
            command: argv,
            env,
            cwd,
        });
    }
}

/// A Spec Kit extension's manifest, read by its adapter.
fn spec_kit(e: &mut Loaded) {
    match crate::format::speckit::extensions::manifest(&e.folder) {
        Ok(m) => {
            if let Some(id) = m.id {
                e.name = id;
            }
            e.version = m.version;
            e.description = m.description;
            e.commands = m.commands;
            e.offers = m
                .hooks
                .into_iter()
                .map(|(event, command, prompt, mandatory)| Offer {
                    event,
                    command,
                    prompt,
                    mandatory,
                })
                .collect();
        }
        Err(err) => e.problems.push(err),
    }
}

/// A Spec Kit extension's commands run as the skills Spec Kit installed
/// for them: those files are part of what is trusted.
fn installed_skills(e: &mut Loaded, project: &Path) {
    let mut extra = Vec::new();
    for c in &e.commands {
        let skill = crate::format::speckit::extensions::skill_of(c);
        for base in crate::skills::DIRS {
            let rel = format!("{base}/{skill}/SKILL.md");
            let path = project.join(&rel);
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.file_type().is_file() || meta.len() > core::MAX_BYTES {
                e.problems
                    .push(format!("{rel} is not a regular file of a sane size"));
                continue;
            }
            let bytes = std::fs::read(&path).unwrap_or_default();
            extra.push(HookFile {
                path: rel,
                digest: format!("sha256:{}", sha256_hex(&bytes)),
            });
        }
    }
    if extra.is_empty() {
        return;
    }
    e.files.extend(extra);
    let mut all = Vec::new();
    for f in &e.files {
        all.extend_from_slice(f.path.as_bytes());
        all.push(0);
        all.extend_from_slice(f.digest.as_bytes());
        all.push(0);
    }
    e.digest = format!("sha256:{}", sha256_hex(&all));
}

/// `dev.speccoder/extension.toml` and `dev.speccoder/source.toml`.
fn speccoder_part(e: &mut Loaded) {
    let part = e.folder.join(NAMESPACE);
    if let Ok(text) = std::fs::read_to_string(part.join("source.toml"))
        && let Ok(origin) = toml::from_str::<AddedFrom>(&text)
    {
        e.origin = Some(origin);
    }
    let Ok(text) = std::fs::read_to_string(part.join("extension.toml")) else {
        return;
    };
    let table: toml::Table = match toml::from_str(&text) {
        Ok(t) => t,
        Err(err) => {
            e.problems
                .push(format!("{NAMESPACE}/extension.toml: {err}"));
            return;
        }
    };
    for (key, value) in &table {
        let Some(entries) = value.as_table() else {
            e.problems.push(format!(
                "{NAMESPACE}/extension.toml: `{key}` is not a table (known: hooks, views)"
            ));
            continue;
        };
        for (name, t) in entries {
            let Ok(json) = serde_json::to_value(t) else {
                continue;
            };
            match key.as_str() {
                "hooks" => match speccoder_core::hooks::parse(name, &json) {
                    Ok(mut def) => {
                        def.name = format!("{}.{name}", e.name);
                        def.extension =
                            Some((e.name.clone(), e.folder.to_string_lossy().into_owned()));
                        e.hooks.push(def);
                    }
                    Err(err) => e.problems.push(err),
                },
                "views" => match core::parse_view(name, &json) {
                    Ok(v) => e.views.push(v),
                    Err(err) => e.problems.push(err),
                },
                other => e.problems.push(format!(
                    "{NAMESPACE}/extension.toml: unknown table `[{other}]` (known: hooks, views)"
                )),
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The digest of the files of the extension in `folder`, and each file's
/// own, sorted. In a git checkout these are the files git tracks there —
/// what a run's worktree holds, so trust and runs see the same bytes;
/// elsewhere (a folder being added) every file.
///
/// # Errors
///
/// The folder or anything in it is a link, something in it is not a
/// regular file, it holds more than 2,000 files or 20 MB, or — in a git
/// checkout — files not committed or ignored by git sit in it (they would
/// run without being what was trusted).
pub fn digest(folder: &Path) -> Result<(String, Vec<HookFile>), String> {
    let meta =
        std::fs::symlink_metadata(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    if meta.file_type().is_symlink() {
        return Err(
            "the extension's folder is a link: an extension is a folder of its own".to_owned(),
        );
    }
    let listed = match git_files(folder)? {
        Some(files) => files,
        None => walk(folder, false)?,
    };
    if listed.len() > core::MAX_FILES {
        return Err(format!(
            "more than {} files: an extension holds at most that many",
            core::MAX_FILES
        ));
    }
    let mut files = Vec::new();
    let mut bytes_total: u64 = 0;
    for rel in listed {
        let path = folder.join(&rel);
        let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{rel}: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!("{rel} is a link: an extension holds files only"));
        }
        if !meta.file_type().is_file() {
            return Err(format!("{rel} is not a regular file"));
        }
        bytes_total += meta.len();
        if bytes_total > core::MAX_BYTES {
            return Err(format!(
                "more than {} MB: an extension holds at most that much",
                core::MAX_BYTES / (1024 * 1024)
            ));
        }
        let mut hasher = Sha256::new();
        let mut f = std::fs::File::open(&path).map_err(|e| format!("{rel}: {e}"))?;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = std::io::Read::read(&mut f, &mut chunk).map_err(|e| format!("{rel}: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&chunk[..n]);
        }
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        files.push(HookFile {
            path: rel,
            digest: format!("sha256:{hex}"),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut all = Vec::new();
    for f in &files {
        all.extend_from_slice(f.path.as_bytes());
        all.push(0);
        all.extend_from_slice(f.digest.as_bytes());
        all.push(0);
    }
    Ok((format!("sha256:{}", sha256_hex(&all)), files))
}

/// The files git tracks in `folder`, when it is in a git checkout; refused
/// while files there are not committed or are ignored by git.
fn git_files(folder: &Path) -> Result<Option<Vec<String>>, String> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(["-c", "core.quotePath=off"])
            .args(args)
            .current_dir(folder)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let Some(top) = git(&["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    if top.trim().is_empty() {
        return Ok(None);
    }
    let names = |text: String| -> Vec<String> {
        text.split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let loose = names(
        git(&[
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--",
            ".",
        ])
        .unwrap_or_default(),
    );
    let changed = names(
        git(&["diff", "--name-only", "-z", "--relative", "HEAD", "--", "."]).unwrap_or_default(),
    );
    let ignored = names(
        git(&[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "--",
            ".",
        ])
        .unwrap_or_default(),
    );
    let mut pending: Vec<String> = loose.into_iter().chain(changed).collect();
    pending.sort();
    pending.dedup();
    if !pending.is_empty() {
        return Err(format!(
            "has changes not committed ({}): commit them — a run uses what is committed",
            pending.join(", ")
        ));
    }
    if !ignored.is_empty() {
        return Err(format!(
            "holds files git ignores ({}): they would run without being trusted — delete them",
            ignored.join(", ")
        ));
    }
    let tracked = names(git(&["ls-files", "-z", "--", "."]).unwrap_or_default());
    if tracked.is_empty() {
        return Err("is not committed: commit it — a run uses what is committed".to_owned());
    }
    Ok(Some(tracked))
}

/// Every entry under `folder`, relative, for a folder outside a checkout;
/// a link anywhere is refused.
pub(crate) fn walk(folder: &Path, skip_git: bool) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    let mut entries = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .flatten()
        {
            entries += 1;
            if entries > core::MAX_FILES * 2 {
                return Err(format!(
                    "more than {} files: an extension holds at most that many",
                    core::MAX_FILES
                ));
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(folder)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().map_err(|e| format!("{rel}: {e}"))?;
            if kind.is_symlink() {
                return Err(format!("{rel} is a link: an extension holds files only"));
            }
            if kind.is_dir() {
                if path.file_name().is_some_and(|n| n == ".git") {
                    if skip_git {
                        continue;
                    }
                    return Err(
                        ".git inside an extension: add it from its repository instead".to_owned(),
                    );
                }
                stack.push(path);
            } else {
                out.push(rel);
            }
        }
    }
    Ok(out)
}

/// The folder of `feature` in the project at `root`, if the format lists it.
#[must_use]
pub fn feature_dir(root: &Path, feature: &str) -> Option<PathBuf> {
    let opened = crate::format::open(root, Some(feature)).ok()?;
    let f = opened
        .snapshot
        .features
        .iter()
        .find(|f| f.name == feature)?;
    Some(Path::new(&opened.snapshot.root).join(&f.dir))
}

/// `argv` with `./` paths inside `folder` (an extension's) and the
/// placeholders `vars` names expanded: `${NAME}` → value. Nothing else is
/// expanded.
#[must_use]
pub fn resolve(argv: &[String], folder: Option<&Path>, vars: &[(&str, String)]) -> Vec<String> {
    argv.iter()
        .enumerate()
        .map(|(i, a)| {
            let mut a = a.clone();
            for (k, v) in vars {
                a = a.replace(&format!("${{{k}}}"), v);
            }
            match folder {
                Some(f)
                    if a.starts_with("./")
                        && !escapes(&a)
                        && (i == 0 || f.join(&a[2..]).exists()) =>
                {
                    f.join(&a[2..]).to_string_lossy().into_owned()
                }
                _ => a,
            }
        })
        .collect()
}

/// A path a tool reported, relative to the project at `project`: as
/// given when relative and inside, stripped when absolute inside; `None`
/// when it leaves the project.
#[must_use]
pub fn relative_to(project: &Path, path: &str) -> Option<String> {
    let p = Path::new(path);
    let rel = if p.is_absolute() {
        let canonical = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        p.strip_prefix(&canonical)
            .or_else(|_| p.strip_prefix(project))
            .ok()?
            .to_path_buf()
    } else {
        p.to_path_buf()
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    (!text.is_empty() && !text.split('/').any(|s| s == "..")).then_some(text)
}

/// The stdio MCP servers of the trusted extensions of the project at
/// `root`, their placeholders expanded: `${PLUGIN_ROOT}` the extension's
/// folder, `${PLUGIN_DATA}` a folder of its own in speccoder's state, the
/// one place it may write besides the worktree.
#[must_use]
pub fn servers(root: &Path, state: &Path) -> Vec<crate::mcp::ServerConfig> {
    let base = crate::worktree::project_state_dir(state, root).join("extensions/data");
    let mut out = Vec::new();
    for e in trusted(root, state) {
        let data = base.join(&e.name);
        let _ = std::fs::create_dir_all(&data);
        let vars = [
            ("PLUGIN_ROOT", e.folder.to_string_lossy().into_owned()),
            ("PLUGIN_DATA", data.to_string_lossy().into_owned()),
        ];
        for s in &e.servers {
            let expand = |v: &str| {
                vars.iter()
                    .fold(v.to_owned(), |a, (k, x)| a.replace(&format!("${{{k}}}"), x))
            };
            let mut command = resolve(&s.command[..1], Some(&e.folder), &[]);
            command.extend(s.command[1..].iter().map(|a| expand(a)));
            out.push(crate::mcp::ServerConfig {
                name: s.name.clone(),
                command,
                allow: Vec::new(),
                deny: Vec::new(),
                env: s.env.iter().map(|(k, v)| (k.clone(), expand(v))).collect(),
                // In the worktree, as a `[mcp]` server, unless it names one.
                cwd: s.cwd.as_deref().map(|c| {
                    let c = expand(c);
                    c.strip_prefix("./")
                        .map_or_else(|| PathBuf::from(&c), |r| e.folder.join(r))
                }),
                data: Some(data.clone()),
            });
        }
    }
    out
}

/// The journey steps at which a Spec Kit hook on `event` is offered: what
/// it follows is done, what it precedes is next.
#[must_use]
pub fn offered_at(event: &str) -> &'static [&'static str] {
    match event {
        "after_specify" | "after_clarify" | "before_plan" => &["plan"],
        "after_plan" | "before_tasks" => &["tasks"],
        "after_tasks" | "before_implement" => &["commit", "run"],
        "after_implement" | "before_converge" | "after_converge" => &["review"],
        _ => &[],
    }
}
