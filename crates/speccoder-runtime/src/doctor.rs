//! `speccoder doctor`: everything a run depends on, checked
//! without writing anything — git, the repository, the spec project and
//! its tool, the configuration, the model key, the sandbox, the state
//! directory, the attestation signing key and the repository's
//! instructions. A failure is a run that would fail or judge wrongly; a
//! warning is less than it could be (unsigned evidence, no spec tool).

use std::path::Path;
use std::process::Command;

use speccoder_core::{
    CommandData, CommandResult, CommandStatus, DoctorCheck, DoctorLevel, DoctorReport,
};

use crate::git::Git;

fn check(name: &str, level: DoctorLevel, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        check: name.to_owned(),
        level,
        detail: detail.into(),
        code: None,
        fix: Vec::new(),
        remedy: None,
        tasks: Vec::new(),
        commands: Vec::new(),
    }
}

fn failed(name: &str, code: &str, detail: impl Into<String>, fix: &[&str]) -> DoctorCheck {
    DoctorCheck {
        code: Some(code.to_owned()),
        fix: fix.iter().map(|f| (*f).to_owned()).collect(),
        ..check(name, DoctorLevel::Fail, detail)
    }
}

fn warned(name: &str, detail: impl Into<String>, fix: &[&str]) -> DoctorCheck {
    DoctorCheck {
        fix: fix.iter().map(|f| (*f).to_owned()).collect(),
        ..check(name, DoctorLevel::Warn, detail)
    }
}

/// The plan checks as doctor rows: `check: "plan"`, level `warn`, the
/// code's catalog entry, the tasks, the fixes and, for behaviour planned
/// early, the decision speccoder records as the remedy.
#[must_use]
pub fn plan_checks(
    adapter: &dyn crate::format::Adapter,
    snapshot: &speccoder_core::snapshot::ProjectSnapshot,
    root: &Path,
) -> Vec<DoctorCheck> {
    let feature = snapshot
        .feature
        .as_ref()
        .map(|f| f.feature.name.clone())
        .unwrap_or_default();
    crate::plan_lint::lints(adapter, snapshot, root)
        .into_iter()
        .map(|l| DoctorCheck {
            code: Some(l.code.to_owned()),
            fix: l.fix.clone(),
            tasks: l.tasks.clone(),
            remedy: l
                .acceptance_at
                .as_ref()
                .map(|(task, story)| speccoder_core::Remedy {
                    label: format!("Write {story}'s acceptance at {task}"),
                    effect: format!("{task}'s tick can be verified on {story}'s scenarios"),
                    command: format!("speccoder task acceptance-at {task} --story {story}"),
                    undo: Some(format!(
                        "speccoder task acceptance-at {task} --story {story} --revoke"
                    )),
                    history: false,
                    run: speccoder_core::actions::Runnable::AcceptanceAt {
                        feature: feature.clone(),
                        story: story.clone(),
                        task: task.clone(),
                        revoke: false,
                    },
                }),
            ..check("plan", DoctorLevel::Warn, l.detail)
        })
        .collect()
}

/// Whether the speccoder preset is active for the tasks template.
fn preset_check(root: &Path) -> DoctorCheck {
    let state = crate::preset::state(root);
    let offer = |update: bool| speccoder_core::Remedy {
        label: if update {
            "Update the speccoder preset".to_owned()
        } else {
            "Add the speccoder preset".to_owned()
        },
        effect: "future task lists plan for evidence; no spec file changes".to_owned(),
        command: "speccoder preset add --yes".to_owned(),
        undo: Some("specify preset remove speccoder".to_owned()),
        history: false,
        run: speccoder_core::actions::Runnable::PresetAdd { update },
    };
    match (&state.installed, state.active_for_tasks_template) {
        (Some(v), true) if v == &state.bundled || v == "unknown" => check(
            "preset",
            DoctorLevel::Pass,
            format!("speccoder preset {v} active for tasks-template"),
        ),
        (Some(v), active) => DoctorCheck {
            remedy: Some(offer(true)),
            fix: vec!["speccoder preset add --yes".to_owned()],
            ..check(
                "preset",
                DoctorLevel::Info,
                format!(
                    "speccoder preset {v} installed{} — this speccoder bundles {}",
                    if active {
                        ""
                    } else {
                        ", not active for tasks-template"
                    },
                    state.bundled
                ),
            )
        },
        (None, _) => DoctorCheck {
            remedy: Some(offer(false)),
            fix: vec!["speccoder preset add --yes".to_owned()],
            ..check(
                "preset",
                DoctorLevel::Info,
                "speccoder preset not installed (tasks-template from Spec Kit's core)",
            )
        },
    }
}

/// The first line a program prints for `--version`, if it runs.
fn version(program: &str) -> Option<String> {
    let out = Command::new(program).arg("--version").output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    })
}

/// Bedrock's `model key` row: the region and where the AWS chain will take
/// credentials from, named offline (never a value, never a call); and a
/// `model region` warning when `AWS_REGION` names another region than
/// the file, which wins.
fn bedrock_key(
    m: &crate::substrate::model_config::ModelConfig,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<DoctorCheck> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    let home = std::path::PathBuf::from(set("HOME").unwrap_or_default());
    let region = m.region.clone().unwrap_or_default();
    let mut out = vec![match crate::aws::detect(&set, &home) {
        Some(source) => {
            let mut detail = format!("bedrock {} · {region} · {}", m.model, source.describe());
            if source.kind == crate::aws::Kind::ApiKey
                && let Some(p) = crate::aws::profile(&set, &home)
            {
                detail.push_str(&format!(
                    " (the API key is used; profile {} is not)",
                    p.name
                ));
            }
            check("model key", DoctorLevel::Pass, detail)
        }
        None => DoctorCheck {
            code: Some("KEY-MISSING".to_owned()),
            fix: crate::substrate::model_config::bedrock_fixes(),
            ..check(
                "model key",
                DoctorLevel::Fail,
                format!(
                    "{} is not set and no AWS profile was found",
                    crate::aws::API_KEY
                ),
            )
        },
    }];
    if let Some(other) = set("AWS_REGION").filter(|r| r.trim() != region) {
        out.push(check(
            "model region",
            DoctorLevel::Warn,
            format!("{region} in speccoder.toml, AWS_REGION is {other} — the file is used"),
        ));
    }
    out
}

/// Every check, in order, for the project at `path`.
#[must_use]
pub fn checks(
    path: &Path,
    feature: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<DoctorCheck> {
    let mut out = Vec::new();
    // git and the repository.
    let git = version("git");
    out.push(match &git {
        Some(v) => check("git", DoctorLevel::Pass, v.clone()),
        None => failed(
            "git",
            "GIT",
            "git is not installed or does not run",
            &["→ install git"],
        ),
    });
    let top = Git::at(path).toplevel().ok().flatten();
    let has_commit = top.as_ref().is_some_and(|t| {
        Git::at(t)
            .line(&["rev-parse", "--verify", "-q", "HEAD"])
            .is_ok()
    });
    out.push(match (&top, has_commit) {
        (Some(t), true) => check("repository", DoctorLevel::Pass, t.display().to_string()),
        (Some(_), false) => failed(
            "repository",
            "NO-REPOSITORY",
            "the repository has no commit yet",
            &["git add -A && git commit -m init"],
        ),
        (None, _) => failed(
            "repository",
            "NO-REPOSITORY",
            "not a git repository",
            &["git init && git add -A && git commit -m init"],
        ),
    });
    // The spec project, and its own tool.
    let opened = crate::format::open(path, feature);
    let root = match &opened {
        Ok(o) => {
            let kind = o.adapter.wording().name;
            let feature = o
                .snapshot
                .feature
                .as_ref()
                .map(|f| format!(", feature {}", f.feature.name))
                .unwrap_or_default();
            out.push(check(
                "project",
                DoctorLevel::Pass,
                format!("{kind} at {}{feature}", o.snapshot.root),
            ));
            out.push(current_check(o.adapter, &o.snapshot));
            let name = o.snapshot.feature.as_ref().map(|f| f.feature.name.as_str());
            if let Some(c) = untracked_check(Path::new(&o.snapshot.root), name) {
                out.push(c);
            }
            if let Some((tool, install)) = o.adapter.tool() {
                out.push(match version(tool) {
                    Some(v) => check("spec tool", DoctorLevel::Pass, v),
                    None => DoctorCheck {
                        fix: vec![format!("→ {install}")],
                        ..check(
                            "spec tool",
                            DoctorLevel::Warn,
                            format!(
                                "`{tool}` is not on PATH: planning skills and cross-checks need it"
                            ),
                        )
                    },
                });
            }
            // The speccoder preset, as Spec Kit reports it.
            if o.adapter.kind() == speccoder_core::spec::FormatKind::Speckit
                && version("specify").is_some()
            {
                out.push(preset_check(Path::new(&o.snapshot.root)));
            }
            out.extend(o.adapter.doctor_checks(Path::new(&o.snapshot.root), env));
            // The plan checks of the selected feature: warnings.
            if let Some(f) = o.snapshot.feature.as_ref() {
                out.extend(plan_checks(
                    o.adapter,
                    &o.snapshot,
                    Path::new(&o.snapshot.root),
                ));
                // Who does what: information, never a warning.
                if let Some(l) =
                    crate::status::lanes(Path::new(&o.snapshot.root), Some(&f.feature.name))
                {
                    out.push(DoctorCheck {
                        tasks: l.planning.iter().chain(&l.yours).cloned().collect(),
                        ..check("lanes", DoctorLevel::Pass, l.line())
                    });
                }
            }
            // Where speccoder's acceptance tests would go.
            let root = std::path::PathBuf::from(&o.snapshot.root);
            let layout = crate::layout::detect(&root, &crate::verify::check_argvs(&root));
            if let Some(r) = layout.primary().filter(|r| r.story_dir.is_none()) {
                out.push(warned(
                    "tests",
                    format!(
                        "no place the runner collects new test files, so no acceptance test is written: {}",
                        r.unknown.join("; ")
                    ),
                    &["→ let the runner collect a tests directory (Cargo: drop `autotests = false`; pytest: a `testpaths` entry that exists)"],
                ));
            }
            root
        }
        Err(r) => {
            let fix = speccoder_core::codes::lookup("NO-PROJECT")
                .map(|e| speccoder_core::codes::fixes(e, "doctor", None, None))
                .unwrap_or_default();
            out.push(DoctorCheck {
                code: Some("NO-PROJECT".to_owned()),
                fix,
                ..check("project", DoctorLevel::Fail, r.message.clone())
            });
            path.to_path_buf()
        }
    };
    // The configuration: present, valid, committed.
    let config_dir = crate::verify::find_config(&root);
    let text = config_dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok());
    let mut run_config = None;
    let mut model = None;
    match (&config_dir, &text) {
        (Some(dir), Some(text)) => {
            let parsed = crate::verify::config::parse(text)
                .map(|_| ())
                .and_then(|()| crate::substrate::model_config::parse(text).map(|m| model = m))
                .and_then(|()| {
                    crate::substrate::run_config::parse(text).map(|c| run_config = Some(c))
                });
            match parsed {
                Err(e) => out.push(failed(
                    "config",
                    "CONFIG-INVALID",
                    e,
                    &["→ fix the line the message names in speccoder.toml"],
                )),
                Ok(()) => {
                    let file = dir.join(crate::verify::config::FILE);
                    let g = Git::at(dir);
                    let tracked = g
                        .succeeds(&[
                            "ls-files",
                            "--error-unmatch",
                            "--",
                            crate::verify::config::FILE,
                        ])
                        .unwrap_or(false);
                    let clean = g
                        .succeeds(&["diff", "--quiet", "HEAD", "--", crate::verify::config::FILE])
                        .unwrap_or(false);
                    out.push(if tracked && clean {
                        check("config", DoctorLevel::Pass, file.display().to_string())
                    } else {
                        failed(
                            "config",
                            "CONFIG-NOT-TRACKED",
                            format!("{} is not committed as it is: speccoder reads committed files only", file.display()),
                            &["git add speccoder.toml && git commit -m \"speccoder: config\""],
                        )
                    });
                }
            }
        }
        _ => out.push(failed(
            "config",
            "CONFIG-MISSING",
            "no speccoder.toml",
            &["speccoder init"],
        )),
    }
    // The model, then its key.
    if let Some(m) = &model {
        out.push(check(
            "model",
            DoctorLevel::Pass,
            format!("{}/{}", m.provider, m.model),
        ));
        let variable = m.key_variable().to_owned();
        if m.provider == "bedrock" {
            out.extend(bedrock_key(m, env));
        } else {
            out.push(
                if variable.is_empty() || env(&variable).is_some_and(|v| !v.is_empty()) {
                    check(
                        "model key",
                        DoctorLevel::Pass,
                        format!("{} {}", m.provider, m.model),
                    )
                } else {
                    DoctorCheck {
                        code: Some("KEY-MISSING".to_owned()),
                        fix: vec![format!("export {variable}=…")],
                        ..check(
                            "model key",
                            DoctorLevel::Fail,
                            format!("{variable} is not set"),
                        )
                    }
                },
            );
        }
    }
    // The sandbox.
    let sandbox = text
        .as_deref()
        .and_then(|t| crate::sandbox::parse(t).ok())
        .unwrap_or_default();
    out.push(match (crate::sandbox::backend(), sandbox.mode) {
        (Ok(b), _) => check(
            "sandbox",
            DoctorLevel::Pass,
            format!("{b:?}").to_lowercase(),
        ),
        (Err(e), crate::sandbox::Mode::Required) => failed(
            "sandbox",
            "SANDBOX-UNAVAILABLE",
            e,
            &["→ run where the OS sandbox exists, or set [sandbox] mode = \"auto\""],
        ),
        (Err(e), crate::sandbox::Mode::Off) => {
            check("sandbox", DoctorLevel::Warn, format!("off ({e})"))
        }
        (Err(e), crate::sandbox::Mode::Auto) => warned(
            "sandbox",
            format!("not available, commands run unsandboxed: {e}"),
            &["→ prefer a container or VM without your credentials"],
        ),
    });
    // `[worktree] include` and `.worktreeinclude`: what a run's worktree
    // gets copied — names only, nothing copied.
    let worktree = text
        .as_deref()
        .and_then(|t| crate::prepare::parse(t).ok())
        .unwrap_or_default();
    if let Some(dir) = crate::verify::find_config(path)
        && let Some(row) = include_check(&dir, &worktree.include)
    {
        out.push(row);
    }
    // The state directory.
    out.push(match crate::worktree::state_dir() {
        Some(dir) => {
            // Probed without creating it: a check writes nothing of its
            // own — the directory, or the nearest folder it would be made
            // in. The nearest path that exists must be a folder: a file
            // there (the directory itself, or a parent) blocks creating it.
            let existing = dir.ancestors().find(|a| a.exists());
            let writable = existing
                .filter(|a| a.is_dir())
                .is_some_and(|a| tempfile::tempfile_in(a).is_ok());
            if writable {
                check("state", DoctorLevel::Pass, dir.display().to_string())
            } else {
                failed(
                    "state",
                    "NO-STATE-DIR",
                    format!("{} is not writable", dir.display()),
                    &["export SPECCODER_STATE_DIR=\"$PWD/.speccoder-state\""],
                )
            }
        }
        None => failed(
            "state",
            "NO-STATE-DIR",
            "no state directory (SPECCODER_STATE_DIR, XDG_STATE_HOME and HOME are unset)",
            &["export SPECCODER_STATE_DIR=\"$PWD/.speccoder-state\""],
        ),
    });
    // A person's decisions: a file from before decisions named objects is
    // refused by every run (never read as "no decisions").
    if let Some(dir) = crate::worktree::state_dir() {
        let project = crate::worktree::project_state_dir(&dir, &root);
        if let Err(e) = crate::approvals::Decisions::load(&project) {
            out.push(failed(
                "decisions",
                "CONFIG-INVALID",
                e,
                &[&format!(
                    "rm {}   (it predates this version; its decisions are not carried over)",
                    project.join(crate::approvals::FILE).display()
                )],
            ));
        }
    }
    // The attestation signing key.
    let attest = run_config
        .as_ref()
        .map(|c| c.attest.clone())
        .unwrap_or_default();
    if attest.enabled {
        let repo = top.clone().unwrap_or_else(|| root.clone());
        out.push(match crate::attest::sign::signer(&repo, &attest) {
            Ok(s) => check(
                "signing",
                DoctorLevel::Pass,
                format!("attestations signed with {}", s.kind()),
            ),
            Err(e) => {
                let fix = signing_fix(
                    std::env::var_os("HOME")
                        .map(std::path::PathBuf::from)
                        .as_deref(),
                );
                // Optional: every tick is attested either way; a key signs it.
                DoctorCheck {
                    fix,
                    ..check(
                        "signing",
                        DoctorLevel::Info,
                        format!("attestations are not signed (optional): {e}"),
                    )
                }
            }
        });
    }
    // GitHub (issues, pull requests): offline — the host the remote names
    // and where a token would come from; optional either way.
    if let Ok(r) = crate::github::repo(&root, None) {
        let ctx = crate::github::Ctx::from_env();
        out.push(match ctx.token(&r.host) {
            Some((_, source)) => check(
                "github",
                DoctorLevel::Pass,
                format!("{} on {} (token from {})", r.slug(), r.host, source.word()),
            ),
            None => DoctorCheck {
                fix: vec!["speccoder auth login --with-token".to_owned()],
                ..check(
                    "github",
                    DoctorLevel::Info,
                    format!(
                        "not signed in to {} (optional: for issues and pull requests)",
                        r.host
                    ),
                )
            },
        });
    }
    // The repository's instructions for agents.
    let names = run_config
        .as_ref()
        .map(|c| c.instructions.clone())
        .unwrap_or_else(|| vec!["AGENTS.md".to_owned()]);
    let repo = top.unwrap_or_else(|| root.clone());
    let given = crate::instructions::collect(&repo, &root, &[], &names);
    out.push(if !given.is_empty() {
        check(
            "instructions",
            DoctorLevel::Pass,
            given
                .iter()
                .map(|i| i.path.clone())
                .collect::<Vec<_>>()
                .join(", "),
        )
    } else if repo.join("CLAUDE.md").is_file() && !names.iter().any(|n| n == "CLAUDE.md") {
        warned(
            "instructions",
            "CLAUDE.md is here but not read: attempts get no repository instructions",
            &["→ add [context] instructions = [\"AGENTS.md\", \"CLAUDE.md\"] to speccoder.toml"],
        )
    } else {
        check("instructions", DoctorLevel::Pass, "none (no AGENTS.md)")
    });
    out
}

/// `speccoder doctor`: the checks; the status of the first failure (ok
/// when nothing fails).
#[must_use]
pub fn run(path: &Path, feature: Option<&str>, look: Option<&str>) -> CommandResult {
    report(path, feature, look, None)
}

/// `speccoder doctor --agents`: the checks and every agent's card; with
/// `probe`, each installed one is asked for a session (never a prompt).
#[must_use]
pub fn agents(path: &Path, feature: Option<&str>, probe: bool) -> CommandResult {
    report(path, feature, None, Some(probe))
}

/// The configured `[executor]` agent, if it is an ACP one.
fn configured_agent(path: &Path) -> Option<crate::acp::agent::Agent> {
    let text = crate::verify::find_config(path)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())?;
    match crate::external::parse(&text).ok()??.kind {
        crate::external::Kind::Acp(a) => Some(a),
        crate::external::Kind::Command(_) => None,
    }
}

/// The `executor` row: the configured agent's card, as known without
/// starting it.
fn executor_check(agent: &crate::acp::agent::Agent) -> DoctorCheck {
    let on_path = |x: &str| crate::acp::agent::on_path(x);
    let Some(p) = agent.preset() else {
        let argv = agent.command(on_path);
        let runs = argv.first().is_some_and(|a| on_path(a));
        return if runs {
            check(
                "executor",
                DoctorLevel::Pass,
                format!(
                    "{} over ACP · not known whether it asks before it acts",
                    argv.join(" ")
                ),
            )
        } else {
            warned(
                "executor",
                format!(
                    "`{}` is not on PATH",
                    argv.first().map_or("", String::as_str)
                ),
                &[],
            )
        };
    };
    let c = crate::acp::ready::card(p, Some(agent), &on_path, false);
    let word = |t: &speccoder_core::AgentTick| match t.state {
        speccoder_core::TickState::Yes => "yes",
        speccoder_core::TickState::No => "no",
        speccoder_core::TickState::Unknown => "not known",
    };
    let detail = format!(
        "{} · installed: {} ({}) · signed in: {} ({}) · asks first: {} ({})",
        c.name,
        word(&c.installed),
        c.installed.detail,
        word(&c.signed_in),
        c.signed_in.detail,
        word(&c.asks_first),
        c.asks_first.detail
    );
    let bad = [&c.installed, &c.asks_first]
        .into_iter()
        .find(|t| t.state == speccoder_core::TickState::No);
    match bad {
        None => check("executor", DoctorLevel::Pass, detail),
        Some(t) => DoctorCheck {
            fix: t.fix.clone(),
            commands: t.commands.clone(),
            ..warned("executor", detail, &[])
        },
    }
}

fn report(
    path: &Path,
    feature: Option<&str>,
    look: Option<&str>,
    cards: Option<bool>,
) -> CommandResult {
    let mut checks = checks(path, feature, &|k| std::env::var(k).ok());
    let chosen = configured_agent(path);
    if let Some(a) = &chosen {
        checks.push(executor_check(a));
    }
    if let Some(look) = look {
        checks.push(check("look", DoctorLevel::Pass, look));
    }
    let failures: Vec<&DoctorCheck> = checks
        .iter()
        .filter(|c| c.level == DoctorLevel::Fail)
        .collect();
    let warnings = checks
        .iter()
        .filter(|c| c.level == DoctorLevel::Warn)
        .count();
    let optional = checks
        .iter()
        .filter(|c| c.level == DoctorLevel::Info)
        .count();
    let report = DoctorReport {
        speccoder: env!("CARGO_PKG_VERSION").to_owned(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        checks: checks.clone(),
        agents: cards.map_or_else(Vec::new, |probe| {
            crate::acp::ready::cards(chosen.as_ref(), probe)
        }),
    };
    let result = match failures.first() {
        None => CommandResult::ok(
            "doctor",
            &format!(
                "{} checks passed, {warnings} warning(s){}",
                checks.len() - warnings - optional,
                if optional > 0 {
                    format!(", {optional} optional")
                } else {
                    String::new()
                }
            ),
        ),
        Some(first) => {
            let entry = first
                .code
                .as_deref()
                .and_then(speccoder_core::codes::lookup);
            let status = entry.map_or(CommandStatus::Error, |e| e.status);
            let message = format!(
                "{} problem(s): {}",
                failures.len(),
                failures
                    .iter()
                    .map(|f| f.check.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let mut r = CommandResult::new("doctor", status, &message).with_fix(first.fix.clone());
            if let Some(code) = &first.code {
                r = r.with_code(code);
            }
            r
        }
    };
    result.with_data(CommandData::Doctor(Box::new(report)))
}

/// The `worktree include` row: the git-ignored files a run's worktree gets
/// copied from this checkout, by name; none when nothing names any.
fn include_check(project: &Path, patterns: &[String]) -> Option<DoctorCheck> {
    if patterns.is_empty() && !project.join(crate::include::FILE).is_file() {
        return None;
    }
    let files = crate::include::matched(project, patterns);
    let protected = crate::tools::paths::Protected::default();
    let masked = files.iter().filter(|f| protected.is_secret(f)).count();
    Some(if files.is_empty() {
        warned(
            "worktree include",
            "nothing to copy: no git-ignored file matches [worktree] include or .worktreeinclude",
            &[],
        )
    } else {
        check(
            "worktree include",
            DoctorLevel::Pass,
            format!(
                "copied into each run's worktree: {}{}",
                files.join(", "),
                if masked == 0 {
                    String::new()
                } else {
                    format!(
                        " ({masked} secret file(s): never committed, secret-named values masked)"
                    )
                }
            ),
        )
    })
}

/// Untracked configuration a worktree will not have: speccoder works on
/// commits, so a check that reads `.config/nextest.toml` or an untracked
/// `*.toml` passes here and cannot run there.
fn untracked_check(root: &Path, feature: Option<&str>) -> Option<DoctorCheck> {
    let out = Git::at(root)
        // NUL-separated: a name is never quoted or split.
        .run(&[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=normal",
            ".",
        ])
        .ok()?;
    // Output and caches are no configuration: a worktree makes its own.
    const OUTPUT: &[&str] = &[
        ".DS_Store",
        ".speccoder",
        ".venv",
        "venv",
        "node_modules",
        "target",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        ".ruff_cache",
        ".tox",
        ".nox",
        ".coverage",
        "dist",
        "build",
        ".next",
        ".turbo",
        ".gradle",
        ".idea",
        ".vscode",
    ];
    let config = |p: &str| {
        let name = p.trim_end_matches('/').rsplit('/').next().unwrap_or(p);
        if OUTPUT.contains(&name) {
            return false;
        }
        name.starts_with('.')
            || [".toml", ".json", ".yaml", ".yml", ".cfg", ".ini"]
                .iter()
                .any(|e| name.ends_with(e))
    };
    let found: Vec<String> = String::from_utf8_lossy(&out)
        .split('\0')
        .filter_map(|l| l.strip_prefix("?? "))
        .filter(|p| config(p))
        .map(str::to_owned)
        .collect();
    if found.is_empty() {
        return None;
    }
    let list = found.join(" ");
    // The paths are the repository's: argvs, never a line read back.
    // Literal pathspecs: a name like `*.json` or `:(top)x` is that file.
    let mut add = ["git", "--literal-pathspecs", "add", "--"]
        .map(str::to_owned)
        .to_vec();
    add.extend(found.iter().cloned());
    let commit = ["git", "commit", "-m", "Track project config"]
        .map(str::to_owned)
        .to_vec();
    let commands = vec![vec![add, commit]];
    Some(DoctorCheck {
        fix: commands
            .iter()
            .map(|c| speccoder_core::step::render(c))
            .collect(),
        commands,
        // What of it a run needs, speccoder commits itself.
        remedy: crate::code::setup_remedy(root, feature),
        ..check(
            "untracked",
            DoctorLevel::Warn,
            format!(
                "{list} — not committed, so a worktree does not have it; a check that needs it cannot run there"
            ),
        )
    })
}

/// Which unit commands work on when none is named, and why — or what
/// stops them from knowing (several units, a pointer to a gone one).
fn current_check(
    format: &'static dyn crate::format::Adapter,
    snap: &speccoder_core::snapshot::ProjectSnapshot,
) -> DoctorCheck {
    let unit = format.messages().unit;
    let problem = snap
        .diagnostics
        .iter()
        .find(|d| d.code == "select.ambiguous" || d.code == "select.not-found");
    match (&snap.feature, problem) {
        (Some(f), _) => check(
            "feature",
            DoctorLevel::Pass,
            format!(
                "{} ({})",
                f.feature.name,
                crate::current::reason_text(snap.selection.reason, format)
            ),
        ),
        (None, Some(d)) => DoctorCheck {
            fix: vec!["→ speccoder feature".to_owned()]
                .into_iter()
                .chain(d.hint.iter().map(|h| format!("→ {h}")))
                .collect(),
            ..check("feature", DoctorLevel::Warn, d.message.clone())
        },
        (None, None) if snap.features.is_empty() => check(
            "feature",
            DoctorLevel::Pass,
            format!("no {unit}s yet — `speccoder spec specify \"…\"` creates one"),
        ),
        (None, None) => DoctorCheck {
            fix: vec!["→ speccoder feature".to_owned()],
            ..check("feature", DoctorLevel::Warn, format!("no current {unit}"))
        },
    }
}

/// Replaces values that look like secrets and the home directory.
#[must_use]
pub fn redact(text: &str) -> String {
    let home = std::env::var("HOME").ok().filter(|h| h.len() > 1);
    let mut out = String::with_capacity(text.len());
    for word in
        text.split_inclusive(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '=')
    {
        let core =
            word.trim_end_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '=');
        let secret = [
            "sk-ant-",
            "sk-proj-",
            "sk-",
            "ghp_",
            "github_pat_",
            "AKIA",
            "xoxb-",
            "glpat-",
        ]
        .iter()
        .any(|p| core.starts_with(p) && core.len() > p.len() + 8);
        if secret {
            out.push_str("[redacted]");
            out.push_str(&word[core.len()..]);
        } else {
            out.push_str(word);
        }
    }
    let out = crate::secrets::mask(&out);
    match home {
        Some(h) => out.replace(&h, "~"),
        None => out,
    }
}

/// The bug report of `doctor`'s `result` for the project at `root`:
/// versions, every check, and the configuration — in Markdown, ready for
/// an issue, with secrets and the home directory taken out.
#[must_use]
pub fn bug_report(result: &CommandResult, root: &Path) -> String {
    let Some(CommandData::Doctor(d)) = result.data() else {
        return redact(result.message());
    };
    let mut out = format!(
        "### speccoder doctor\n\n- speccoder {}\n- {}\n\n| Check | Result | Detail |\n|---|---|---|\n",
        d.speccoder, d.os
    );
    for c in &d.checks {
        let level = match c.level {
            DoctorLevel::Pass => "ok",
            DoctorLevel::Info => "optional",
            DoctorLevel::Warn => "warning",
            DoctorLevel::Fail => "**fail**",
        };
        out.push_str(&format!(
            "| {} | {level}{} | {} |\n",
            c.check,
            c.code
                .as_deref()
                .map(|c| format!(" `{c}`"))
                .unwrap_or_default(),
            c.detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    let config = std::fs::read_to_string(root.join("speccoder.toml")).ok();
    if let Some(text) = config {
        out.push_str("\n<details><summary>speccoder.toml</summary>\n\n```toml\n");
        out.push_str(text.trim_end());
        out.push_str("\n```\n\n</details>\n");
    }
    redact(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_home_are_redacted() {
        let home = std::env::var("HOME").unwrap_or_default();
        let text = format!(
            "key sk-ant-api03-abcdefghijklmnop at {home}/x ghp_0123456789abcdefghij \"sk-proj-aaaaaaaaaaaaaaaa\""
        );
        let r = redact(&text);
        assert!(
            !r.contains("abcdefghijklmnop") && !r.contains("0123456789abcdef"),
            "{r}"
        );
        assert!(!r.contains("sk-proj-aaaa"), "{r}");
        if home.len() > 1 {
            assert!(!r.contains(&home) && r.contains("~/x"), "{r}");
        }
        assert!(r.contains("[redacted]"));
        assert_eq!(redact("sk-short"), "sk-short");
    }

    #[test]
    fn the_files_a_worktree_gets_copied_are_named_and_nothing_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args([
                        "-c",
                        "user.name=t",
                        "-c",
                        "user.email=t@e",
                        "-c",
                        "commit.gpgsign=false"
                    ])
                    .args(args)
                    .current_dir(r)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        std::fs::write(r.join(".gitignore"), ".env\n*.local\n").unwrap();
        std::fs::write(r.join("app.local"), "x").unwrap();
        std::fs::write(r.join("tracked.local"), "x").unwrap();
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&["add", "-f", "tracked.local"]);
        git(&["commit", "-qm", "init"]);
        std::fs::write(r.join(".env"), "TOKEN=abcdefgh123\n").unwrap();
        assert!(include_check(r, &[]).is_none());
        let row = include_check(r, &[".env".to_owned()]).unwrap();
        assert_eq!(row.level, DoctorLevel::Pass);
        assert_eq!(
            row.detail,
            "copied into each run's worktree: .env (1 secret file(s): never committed, secret-named values masked)"
        );
        assert!(!row.detail.contains("abcdefgh123"));
        // `.worktreeinclude` too; a tracked file never.
        std::fs::write(r.join(".worktreeinclude"), "*.local\n").unwrap();
        let row = include_check(r, &[]).unwrap();
        assert_eq!(row.detail, "copied into each run's worktree: app.local");
        let row = include_check(r, &["nothing-here".to_owned()]).unwrap();
        assert_eq!(row.detail, "copied into each run's worktree: app.local");
    }

    #[test]
    fn outside_any_repository_every_problem_is_named_with_its_fix() {
        let dir = tempfile::tempdir().unwrap();
        let checks = checks(dir.path(), None, &|_| None);
        let failing: Vec<(&str, Option<&str>)> = checks
            .iter()
            .filter(|c| c.level == DoctorLevel::Fail)
            .map(|c| (c.check.as_str(), c.code.as_deref()))
            .collect();
        assert!(
            failing.contains(&("repository", Some("NO-REPOSITORY"))),
            "{failing:?}"
        );
        assert!(
            failing.contains(&("project", Some("NO-PROJECT"))),
            "{failing:?}"
        );
        assert!(
            failing.contains(&("config", Some("CONFIG-MISSING"))),
            "{failing:?}"
        );
        for c in checks.iter().filter(|c| c.level != DoctorLevel::Pass) {
            assert!(!c.fix.is_empty(), "{} has no fix", c.check);
        }
    }
}

/// How to sign with SSH: with a public key the person has, the newest kind
/// first; without one, make one first. Never a key that does not exist.
fn signing_fix(home: Option<&Path>) -> Vec<String> {
    let found = home.and_then(|h| {
        [
            "id_ed25519.pub",
            "id_ecdsa.pub",
            "id_ed25519_sk.pub",
            "id_ecdsa_sk.pub",
            "id_rsa.pub",
        ]
        .iter()
        .find(|k| h.join(".ssh").join(k).is_file())
    });
    let mut fix = Vec::new();
    let key = match found {
        Some(k) => format!("~/.ssh/{k}"),
        None => {
            fix.push("ssh-keygen -t ed25519".to_owned());
            "~/.ssh/id_ed25519.pub".to_owned()
        }
    };
    fix.push("git config --global gpg.format ssh".to_owned());
    fix.push(format!("git config --global user.signingKey {key}"));
    fix
}

#[cfg(test)]
mod signing_tests {
    use super::*;

    #[test]
    fn the_signing_fix_names_a_key_that_exists() {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let none = signing_fix(Some(home.path()));
        assert_eq!(none[0], "ssh-keygen -t ed25519");
        std::fs::create_dir_all(home.path().join(".ssh")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(home.path().join(".ssh/id_rsa.pub"), "ssh-rsa AAAA")
            .unwrap_or_else(|e| panic!("{e}"));
        let rsa = signing_fix(Some(home.path()));
        assert_eq!(
            rsa.last().map(String::as_str),
            Some("git config --global user.signingKey ~/.ssh/id_rsa.pub")
        );
        assert!(!rsa.iter().any(|f| f.starts_with("ssh-keygen")));
    }
}
