//! The plan: computed read-only from the folder as it is, the person's
//! choices, the installed tools and the format's own init.

use std::path::Path;

use speccoder_core::new_project::{
    ExtensionChoice, FormatOption, GitIdentity, LOCAL_LIMIT_S, NETWORK_LIMIT_S, NewChoices,
    ObservedFolder, PlanStep, ProjectMode, ProjectPlan, SandboxNote, Stack, StackOption, StackPick,
    StepAccess,
};
use speccoder_core::spec::FormatKind;

use super::{Creation, digest, record_dir, refusal, resolve, shown, stacks, steps};
use crate::git::Git;

/// A step and what runs it.
#[derive(Debug, Clone)]
pub(crate) struct Spec {
    pub step: PlanStep,
    pub env: Vec<(String, String)>,
    /// Done when this exists in the folder; empty: never detected.
    pub marker: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Command,
    Init,
    Commit,
}

pub(crate) fn format_kind(id: &str) -> Option<FormatKind> {
    FormatKind::ALL.into_iter().find(|k| k.name() == id)
}

/// A creation of the folder that ran and did not finish: a re-run resumes it.
pub(crate) fn unfinished(state: &Path, target: &Path) -> Option<Creation> {
    let text = std::fs::read_to_string(record_dir(state, target).join("creation.json")).ok()?;
    let c: Creation = serde_json::from_str(&text).ok()?;
    (c.commit.is_none() && c.undone_at.is_none()).then_some(c)
}

/// The git configuration value `key` as git in `dir` sees it.
fn git_config(dir: &Path, key: &str) -> Option<String> {
    Git::at(dir)
        .line(&["config", "--get", key])
        .ok()
        .filter(|v| !v.is_empty())
}

/// The plan for `path` with `choices`: read-only. `fresh`: tool versions
/// probed now (the perform), else reused for a minute (the sheet).
pub(crate) fn plan_with(
    path: &Path,
    choices: &NewChoices,
    state: &Path,
    fresh: bool,
) -> (ProjectPlan, Vec<Spec>) {
    let target = resolve(path);
    let (target, mut refused) = match target {
        Ok(t) => (t, None),
        Err(r) => (path.to_path_buf(), Some(r)),
    };
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned());
    let mut observed = ObservedFolder {
        exists: target.is_dir(),
        ..ObservedFolder::default()
    };
    if target.exists() && !target.is_dir() && refused.is_none() {
        refused = Some(refusal(
            "NEW-TARGET-NOT-EMPTY",
            &format!("{} is a file, not a folder", shown(&target)),
            "→ choose a folder",
        ));
    }
    if observed.exists {
        let mut entries: Vec<String> = std::fs::read_dir(&target)
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        entries.sort();
        observed.entries = entries;
    }
    // The repository the folder (or its nearest existing parent) is in.
    let mut probe_dir = target.clone();
    while !probe_dir.is_dir() {
        if !probe_dir.pop() {
            break;
        }
    }
    let top = Git::at(&probe_dir)
        .toplevel()
        .ok()
        .flatten()
        .map(|t| std::fs::canonicalize(&t).unwrap_or(t));
    let at_top = top.as_deref() == Some(target.as_path());
    if at_top {
        observed.head = Git::at(&target).head().ok().flatten();
        observed.porcelain = Git::at(&target)
            .line(&["status", "--porcelain", "--untracked-files=all"])
            .map(|l| l.lines().map(str::to_owned).collect())
            .unwrap_or_default();
    }
    let resuming = unfinished(state, &target);
    let meaningful = observed.entries.iter().any(|e| e != ".DS_Store");
    let mode = match &resuming {
        Some(c) => c.mode,
        None if at_top => ProjectMode::ExistingRepository,
        None => ProjectMode::NewFolder,
    };
    if refused.is_none() && resuming.is_none() {
        refused = match (mode, &top) {
            (ProjectMode::NewFolder, Some(t)) => Some(refusal(
                "NEW-NESTED-REPOSITORY",
                &format!("{} is inside the repository {}", shown(&target), shown(t)),
                &format!("→ choose a folder outside it, or use {} itself", shown(t)),
            )),
            (ProjectMode::NewFolder, None) if meaningful => Some(refusal(
                "NEW-TARGET-NOT-EMPTY",
                &format!("{} is not empty and not a git repository", shown(&target)),
                "→ choose an empty folder, or run `git init` there and commit what it holds first",
            )),
            (ProjectMode::ExistingRepository, _) if !observed.porcelain.is_empty() => {
                Some(refusal(
                    "NEW-DIRTY",
                    &format!("{} has uncommitted changes", shown(&target)),
                    "→ commit or stash them first",
                ))
            }
            (ProjectMode::ExistingRepository, _) => crate::format::adapters()
                .into_iter()
                .find(|a| {
                    a.detect(&target)
                        .is_some_and(|r| std::fs::canonicalize(&r).unwrap_or(r) == target)
                })
                .map(|a| {
                    refusal(
                        "NEW-ALREADY-PROJECT",
                        &format!(
                            "{} is already {} {} project",
                            shown(&target),
                            if a.wording().name.starts_with(['A', 'E', 'I', 'O', 'U']) {
                                "an"
                            } else {
                                "a"
                            },
                            a.wording().name
                        ),
                        &format!("→ open it instead: speccoder ui -C {}", shown(&target)),
                    )
                }),
            _ => None,
        };
    }
    // The stack: the repository's own, or the person's choice, else the
    // suggestion among the installed tools.
    let installed: Vec<(Stack, String)> = Stack::ALL
        .into_iter()
        .filter_map(|s| {
            s.tool()
                .and_then(|t| stacks::version(t, fresh))
                .map(|v| (s, v))
        })
        .collect();
    let stack = match mode {
        ProjectMode::ExistingRepository => {
            let (s, file) = stacks::detect(&target);
            StackPick {
                id: s,
                detected: true,
                reason: file.map_or_else(
                    || "no manifest found: add code and a test runner yourself".to_owned(),
                    |f| format!("found {f}"),
                ),
            }
        }
        ProjectMode::NewFolder => match choices.stack {
            Some(s) => StackPick {
                id: s,
                detected: false,
                reason: "chosen".to_owned(),
            },
            None => speccoder_core::new_project::suggest_stack(&choices.idea, &installed),
        },
    };
    let stacks_shown: Vec<StackOption> = Stack::ALL
        .into_iter()
        .map(|s| {
            let version = installed
                .iter()
                .find(|(k, _)| *k == s)
                .map(|(_, v)| v.clone());
            StackOption {
                id: s,
                label: s.label().to_owned(),
                disabled: (s != Stack::None && version.is_none()).then(|| {
                    format!(
                        "{} is not installed — install it: {}",
                        s.tool().unwrap_or_default(),
                        s.install_hint()
                    )
                }),
                version,
            }
        })
        .collect();
    let format_id = choices
        .format
        .clone()
        .unwrap_or_else(|| FormatKind::Speckit.name().to_owned());
    let adapter = format_kind(&format_id).and_then(crate::format::adapter);
    let formats: Vec<FormatOption> = [FormatKind::Speckit, FormatKind::Openspec]
        .into_iter()
        .map(|k| {
            let tool = crate::format::adapter(k).and_then(|a| a.tool());
            let version = tool.and_then(|(t, _)| stacks::version(t, fresh));
            FormatOption {
                id: k.name().to_owned(),
                label: k.title().to_owned(),
                disabled: match (tool, &version) {
                    (Some((t, hint)), None) => Some(format!("{t} is not installed — {hint}")),
                    _ => None,
                },
                version,
            }
        })
        .collect();
    let extensions: Vec<ExtensionChoice> = if format_id == FormatKind::Speckit.name() {
        choices
            .extensions
            .iter()
            .filter(|e| !e.trim().is_empty())
            .map(|e| ExtensionChoice::of(e))
            .collect()
    } else {
        Vec::new()
    };
    // Tools: git, the stack's, the format's.
    let git_version = stacks::version("git", fresh);
    if let Some(v) = &git_version {
        observed.tools.insert("git".to_owned(), v.clone());
    }
    let stack_tool = if mode == ProjectMode::NewFolder {
        stack.id.tool()
    } else {
        None
    };
    if let Some(t) = stack_tool {
        match installed.iter().find(|(s, _)| *s == stack.id) {
            Some((_, v)) => {
                observed.tools.insert(t.to_owned(), v.clone());
            }
            None if refused.is_none() => {
                refused = Some(refusal(
                    "NEW-TOOL-MISSING",
                    &format!("{t} is not installed"),
                    &format!("→ install it: {}", stack.id.install_hint()),
                ));
            }
            None => {}
        }
    }
    let format_tool = adapter.and_then(|a| a.tool());
    match (adapter, format_tool) {
        (None, _) if refused.is_none() => {
            refused = Some(refusal(
                "USAGE",
                &format!("`{format_id}` is not a spec format speccoder can start a project in"),
                &format!(
                    "→ choose {} or {}",
                    FormatKind::Speckit.name(),
                    FormatKind::Openspec.name()
                ),
            ));
        }
        (Some(_), Some((t, hint))) => match stacks::version(t, fresh) {
            Some(v) => {
                observed.tools.insert(t.to_owned(), v);
            }
            None if refused.is_none() => {
                refused = Some(refusal(
                    "NEW-TOOL-MISSING",
                    &format!("{t} is not installed"),
                    &format!("→ install it: {hint}"),
                ));
            }
            None => {}
        },
        _ => {}
    }
    if git_version.is_none() && refused.is_none() {
        refused = Some(refusal(
            "NEW-TOOL-MISSING",
            "git is not installed",
            "→ install it: https://git-scm.com/downloads",
        ));
    }
    let identity = match (
        git_config(&probe_dir, "user.name"),
        git_config(&probe_dir, "user.email"),
    ) {
        (Some(name), Some(email)) => Some(GitIdentity { name, email }),
        _ => None,
    };
    if identity.is_none() && refused.is_none() {
        refused = Some(refusal(
            "NEW-NO-IDENTITY",
            "git does not know who you are: user.name and user.email are not set",
            "→ git config --global user.name \"Your Name\" and git config --global user.email you@example.com",
        ));
    }
    // The steps.
    let cwd = target.display().to_string();
    let mut specs: Vec<Spec> = Vec::new();
    let mut push = |id: String,
                    argv: Vec<String>,
                    hosts: Vec<String>,
                    writes: String,
                    env: Vec<(String, String)>,
                    marker: &str,
                    note: Option<String>,
                    kind: Kind| {
        let access = if hosts.is_empty() {
            StepAccess::Local
        } else {
            StepAccess::Network
        };
        specs.push(Spec {
            step: PlanStep {
                id,
                argv,
                cwd: cwd.clone(),
                access,
                hosts,
                writes,
                limit_s: if access == StepAccess::Network {
                    NETWORK_LIMIT_S
                } else {
                    LOCAL_LIMIT_S
                },
                note,
            },
            env,
            marker: marker.to_owned(),
            kind,
        });
    };
    if mode == ProjectMode::NewFolder {
        push(
            "git_init".to_owned(),
            vec!["git".to_owned(), "init".to_owned()],
            Vec::new(),
            ".git/".to_owned(),
            Vec::new(),
            ".git",
            None,
            Kind::Command,
        );
        for (i, r) in stacks::recipe(stack.id, &name).into_iter().enumerate() {
            push(
                format!("scaffold.{}", i + 1),
                r.argv,
                r.hosts,
                r.writes.to_owned(),
                Vec::new(),
                r.marker,
                r.note.map(str::to_owned),
                Kind::Command,
            );
        }
    }
    let mut preset = false;
    if let Some(a) = adapter {
        let tool = a.tool().map_or("format", |(t, _)| t);
        let init = crate::format::FormatInit {
            extensions: extensions.clone(),
            preset: !choices.no_preset,
            state: state.to_path_buf(),
            cache: steps::cache_dir(state, tool),
        };
        let found = a.init_steps(&init);
        if found.is_empty() && refused.is_none() {
            refused = Some(refusal(
                "USAGE",
                &format!("speccoder cannot initialise {} projects", a.wording().name),
                &format!(
                    "→ {}: {}",
                    a.messages().make_project.1,
                    a.messages().make_project.0
                ),
            ));
        }
        for s in found {
            preset |= s.preset;
            push(
                "format_init".to_owned(),
                s.argv,
                s.hosts,
                s.writes,
                s.env,
                s.marker,
                s.note,
                Kind::Command,
            );
        }
    }
    push(
        "speccoder_init".to_owned(),
        vec![
            "speccoder".to_owned(),
            "init".to_owned(),
            "--yes".to_owned(),
        ],
        Vec::new(),
        "speccoder.toml (its checks run on the untouched code)".to_owned(),
        Vec::new(),
        crate::verify::config::FILE,
        None,
        Kind::Init,
    );
    let subject = format!("new project: {name}");
    push(
        "commit".to_owned(),
        vec![
            "git".to_owned(),
            "commit".to_owned(),
            "-m".to_owned(),
            subject,
        ],
        Vec::new(),
        identity.as_ref().map_or_else(
            || "one commit".to_owned(),
            |i| format!("one commit as {} <{}>", i.name, i.email),
        ),
        Vec::new(),
        "",
        (mode == ProjectMode::ExistingRepository).then(|| {
            "existing files: none of yours will be committed — the tree is clean".to_owned()
        }),
        Kind::Commit,
    );
    let sandbox = SandboxNote {
        backend: match crate::sandbox::backend() {
            Ok(crate::sandbox::Backend::Seatbelt) => Some("seatbelt".to_owned()),
            Ok(crate::sandbox::Backend::Landlock(_)) => Some("landlock".to_owned()),
            Err(_) => None,
        },
    };
    let mut plan = ProjectPlan {
        target: target.display().to_string(),
        name,
        mode,
        stack,
        stacks: stacks_shown,
        format: format_id,
        formats,
        extensions,
        preset,
        steps: specs.iter().map(|s| s.step.clone()).collect(),
        sandbox,
        identity,
        refusal: refused,
        observed,
        digest: String::new(),
    };
    plan.digest = digest(&plan);
    (plan, specs)
}
