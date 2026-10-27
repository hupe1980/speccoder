//! `speccoder init`: detect a project's checks and write
//! `speccoder.toml`.
//!
//! Detection looks only at the project root's manifests and lockfiles (and
//! whether `cargo nextest` is installed); it runs nothing of the project's.
//! What it finds becomes a commented file the person reviews and commits.
//! The result is parsed back with speccoder's own parsers before it is
//! written, so `init` never leaves a file `run` would refuse.

use std::path::{Path, PathBuf};

use serde::Serialize;
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::git::Git;
use crate::verify::config::FILE;

/// What `init` saw at the project root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// `Cargo.toml` exists.
    pub cargo: bool,
    /// `cargo nextest` is installed.
    pub nextest: bool,
    /// `package.json`'s scripts, by name.
    pub npm_scripts: Vec<String>,
    /// `npm`, `pnpm`, `yarn` or `bun`, from the lockfile; `None` without `package.json`.
    pub node_manager: Option<String>,
    /// `pyproject.toml`, `setup.cfg`, `setup.py` or `pytest.ini` exists.
    pub python: bool,
    /// The project's own interpreter (`.venv/bin/python`), when it has one:
    /// checks use it, so they work without an activated environment.
    pub python_bin: Option<String>,
    /// pytest runs with that interpreter (or `python3`).
    pub pytest: bool,
    /// `[tool.ruff]` (or `ruff.toml`) is configured.
    pub ruff: bool,
    /// `[tool.mypy]` (or `mypy.ini`) is configured.
    pub mypy: bool,
    /// `go.mod` exists.
    pub go: bool,
    /// A `Makefile` with a `test:` target exists.
    pub make_test: bool,
}

/// One check `init` proposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proposed {
    /// `[checks.<name>]`.
    pub name: String,
    /// The command.
    pub argv: Vec<String>,
    /// `fast` or `full`.
    pub tier: String,
    /// A JUnit report the check writes.
    pub junit: Option<String>,
    /// Why this check.
    pub why: String,
}

fn check(name: &str, argv: &[&str], tier: &str, junit: Option<&str>, why: &str) -> Proposed {
    Proposed {
        name: name.to_owned(),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        tier: tier.to_owned(),
        junit: junit.map(str::to_owned),
        why: why.to_owned(),
    }
}

/// The checks for what was found (pure: no IO).
#[must_use]
pub fn propose(found: &Found) -> Vec<Proposed> {
    let mut out = Vec::new();
    if found.cargo {
        out.push(if found.nextest {
            check(
                "test",
                // A new project has no tests yet: warn, do not fail the baseline.
                &[
                    "cargo",
                    "nextest",
                    "run",
                    "--workspace",
                    "--profile",
                    "speccoder",
                    "--no-tests=warn",
                ],
                "fast",
                Some("target/nextest/speccoder/junit.xml"),
                "Cargo.toml; cargo-nextest is installed (its JUnit report names every test)",
            )
        } else {
            check(
                "test",
                &["cargo", "test", "--workspace"],
                "fast",
                None,
                "Cargo.toml (install cargo-nextest for per-test results)",
            )
        });
        out.push(check(
            "fmt",
            &["cargo", "fmt", "--all", "--check"],
            "fast",
            None,
            "Cargo.toml",
        ));
        out.push(check(
            "clippy",
            &[
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            "full",
            None,
            "Cargo.toml",
        ));
    }
    if let Some(pm) = &found.node_manager {
        let has = |s: &str| found.npm_scripts.iter().any(|x| x == s);
        // `<pm> run <script>`: the allowed prefix is that one script.
        let run = |script: &str| -> Vec<String> { vec![pm.clone(), "run".into(), script.into()] };
        let mut add = |name: &str, script: &str, tier: &str| {
            out.push(Proposed {
                name: name.to_owned(),
                argv: run(script),
                tier: tier.to_owned(),
                junit: None,
                why: format!("package.json script `{script}` ({pm})"),
            });
        };
        if has("test") {
            add(
                if found.cargo { "node-test" } else { "test" },
                "test",
                "fast",
            );
        }
        if has("typecheck") {
            add("typecheck", "typecheck", "fast");
        }
        if has("lint") {
            add("lint", "lint", "full");
        }
    }
    if found.python {
        let name = if found.cargo || found.node_manager.is_some() {
            "pytest"
        } else {
            "test"
        };
        let python = found.python_bin.as_deref().unwrap_or("python3");
        let why = if found.pytest {
            format!("a Python project (pytest, {python})")
        } else {
            format!(
                "a Python project — pytest is not installed for {python}: `{python} -m pip install pytest`"
            )
        };
        out.push(check(
            name,
            &[
                python,
                "-m",
                "pytest",
                "-q",
                "--junitxml=.speccoder/junit.xml",
            ],
            "fast",
            Some(".speccoder/junit.xml"),
            &why,
        ));
        if found.ruff {
            out.push(check(
                "ruff",
                &["ruff", "check", "."],
                "fast",
                None,
                "ruff is configured",
            ));
        }
        if found.mypy {
            out.push(check(
                "mypy",
                &["mypy", "."],
                "full",
                None,
                "mypy is configured",
            ));
        }
    }
    if found.go {
        let name = if out.iter().any(|c| c.name == "test") {
            "go-test"
        } else {
            "test"
        };
        out.push(check(
            name,
            &["go", "test", "./..."],
            "fast",
            None,
            "go.mod",
        ));
        out.push(check(
            "vet",
            &["go", "vet", "./..."],
            "full",
            None,
            "go.mod",
        ));
    }
    if out.is_empty() && found.make_test {
        out.push(check(
            "test",
            &["make", "test"],
            "fast",
            None,
            "a Makefile `test` target",
        ));
    }
    out
}

/// Commands the model (and the spec-derived acceptance tests) need besides
/// the checks: narrow prefixes per toolchain, written to `[commands] allow`.
#[must_use]
pub fn allow(found: &Found) -> Vec<Vec<String>> {
    let v = |a: &[&str]| a.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let mut out = Vec::new();
    if found.cargo {
        out.extend([
            v(&["cargo", "test"]),
            v(&["cargo", "check"]),
            v(&["cargo", "build"]),
        ]);
    }
    if let Some(pm) = &found.node_manager {
        out.extend([v(&[pm, "test"]), v(&[pm, "run", "test"])]);
    }
    if found.python {
        out.push(v(&[
            found.python_bin.as_deref().unwrap_or("python3"),
            "-m",
            "pytest",
        ]));
    }
    if found.go {
        out.extend([v(&["go", "test"]), v(&["go", "build"]), v(&["go", "vet"])]);
    }
    out
}

/// The formatter speccoder runs after each attempt, when the toolchain has one.
#[must_use]
pub fn formatter(found: &Found) -> Option<Vec<String>> {
    let v = |a: &[&str]| Some(a.iter().map(|x| (*x).to_owned()).collect());
    if found.cargo {
        v(&["cargo", "fmt", "--all"])
    } else if found.go {
        v(&["gofmt", "-w", "."])
    } else if found.python && found.ruff {
        v(&["ruff", "format", "."])
    } else {
        None
    }
}

/// Looks at `root` (and whether `cargo nextest` runs).
#[must_use]
pub fn detect(root: &Path) -> Found {
    let exists = |p: &str| root.join(p).exists();
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_default();
    let cargo = exists("Cargo.toml");
    let nextest = cargo
        && std::process::Command::new("cargo")
            .args(["nextest", "--version"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
    let (node_manager, npm_scripts) = if exists("package.json") {
        let manager = if exists("pnpm-lock.yaml") {
            "pnpm"
        } else if exists("yarn.lock") {
            "yarn"
        } else if exists("bun.lockb") || exists("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        let scripts = serde_json::from_str::<serde_json::Value>(&read("package.json"))
            .ok()
            .and_then(|v| {
                v["scripts"]
                    .as_object()
                    .map(|o| o.keys().cloned().collect())
            })
            .unwrap_or_default();
        (Some(manager.to_owned()), scripts)
    } else {
        (None, Vec::new())
    };
    let pyproject = read("pyproject.toml");
    let makefile = read("Makefile");
    let python = ["pyproject.toml", "setup.cfg", "setup.py", "pytest.ini"]
        .iter()
        .any(|p| exists(p));
    // A project-local virtualenv: its interpreter, by the path checks run it.
    let python_bin = [".venv/bin/python", "venv/bin/python"]
        .iter()
        .find(|p| python && exists(p))
        .map(|p| (*p).to_owned());
    Found {
        cargo,
        nextest,
        npm_scripts,
        node_manager,
        python,
        pytest: python && {
            let bin = python_bin.as_deref().unwrap_or("python3");
            std::process::Command::new(if root.join(bin).is_file() {
                root.join(bin)
            } else {
                bin.into()
            })
            .args(["-m", "pytest", "--version"])
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        },
        python_bin,
        ruff: pyproject.contains("[tool.ruff") || exists("ruff.toml") || exists(".ruff.toml"),
        mypy: pyproject.contains("[tool.mypy") || exists("mypy.ini"),
        go: exists("go.mod"),
        make_test: makefile.lines().any(|l| l.starts_with("test:")),
    }
}

/// A TOML basic string (JSON's escapes are valid TOML).
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The provider and model `init` writes: the flag, else the provider whose
/// key variable is set (presence only — the value is never read).
fn model_choice(
    provider: Option<&str>,
    model: Option<&str>,
    env: &dyn Fn(&str) -> bool,
) -> (String, String) {
    let provider = provider.map_or_else(
        || {
            if env("ANTHROPIC_API_KEY") || !env("OPENAI_API_KEY") {
                "anthropic".to_owned()
            } else {
                "openai".to_owned()
            }
        },
        str::to_owned,
    );
    let model = model.map_or_else(
        || match provider.as_str() {
            "openai" => "gpt-5.4-mini".to_owned(),
            _ => "claude-sonnet-5-5".to_owned(),
        },
        str::to_owned,
    );
    (provider, model)
}

/// The file `init` writes.
#[must_use]
pub fn render(
    checks: &[Proposed],
    allow: &[Vec<String>],
    format: Option<&[String]>,
    provider: &str,
    model: &str,
) -> String {
    let mut out = String::from(
        "# speccoder configuration — written by `speccoder init`; review it, then commit it\n\
         # (runs read the committed file in their worktree).\n\n\
         # Checks: speccoder runs them before and after every attempt and decides the\n\
         # verdict from them. `fast` runs per task; `full` at phase ends and `verify --tier full`.\n",
    );
    if checks.is_empty() {
        out.push_str(
            "# No checks were detected. Add at least one, e.g.:\n\
             # [checks.test]\n# argv = [\"make\", \"test\"]\n",
        );
    }
    for c in checks {
        out.push_str(&format!("\n# {}\n[checks.{}]\nargv = [", c.why, c.name));
        out.push_str(
            &c.argv
                .iter()
                .map(|a| quote(a))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("]\n");
        if c.tier != "fast" {
            out.push_str(&format!("tier = {}\n", quote(&c.tier)));
        }
        if let Some(j) = &c.junit {
            out.push_str(&format!("junit = {}\n", quote(j)));
        }
    }
    out.push_str(&format!(
        "\n# The model. The API key comes only from the environment ({}).\n\
         [model]\nprovider = {}\nmodel = {}\n\
         # price_input_per_mtok = 0.0       # USD per million tokens, for `speccoder report --cost`\n\
         # price_output_per_mtok = 0.0\n\
         # price_cache_read_per_mtok = 0.0\n\
         \n# Limits per attempt, and attempts per task (a failed verdict is retried once).\n\
         # [budget]\n# max_turns = 30\n# max_tool_calls = 100\n# max_attempts = 2\n\
         \n# Test files that existed before an attempt: \"ask\" before any change (default), or\n\
         # \"additions\" to let new tests be added without asking (changed lines still ask).\n\
         # [tests]\n# frozen = \"additions\"\n\
         \n# Commands run in an OS sandbox: writes only to the worktree, credentials unreadable,\n\
         # no network beyond loopback. Give commands that fetch the network, or switch it off.\n\
         # [sandbox]\n# network_allow = [[\"cargo\", \"fetch\"]]\n# mode = \"auto\"           # \"required\" refuses to run unsandboxed; \"off\"\n\
         \n# Commands the model may run besides the checks (argv prefixes); others need approval.\n",
        if provider == "openai" {
            "OPENAI_API_KEY"
        } else {
            "ANTHROPIC_API_KEY"
        },
        quote(provider),
        quote(model)
    ));
    if let Some(argv) = format {
        out.push_str(&format!(
            "\n# The formatter speccoder runs after each attempt, before the checks.\n[format]\nargv = [{}]\n\n",
            argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(", ")
        ));
    }
    if allow.is_empty() {
        out.push_str("# [commands]\n# allow = [[\"make\", \"check\"]]\n");
    } else {
        let lists: Vec<String> = allow
            .iter()
            .map(|a| {
                format!(
                    "[{}]",
                    a.iter().map(|x| quote(x)).collect::<Vec<_>>().join(", ")
                )
            })
            .collect();
        out.push_str(&format!("[commands]\nallow = [{}]\n", lists.join(", ")));
    }
    out
}

/// The nextest profile speccoder's JUnit report path expects.
pub const NEXTEST_PROFILE: &str = "\n# Written by `speccoder init`: speccoder reads this JUnit report.\n[profile.speccoder]\nfail-fast = false\n\n[profile.speccoder.junit]\npath = \"junit.xml\"\n";

/// `speccoder init [--provider] [--model] [--force]` with an
/// explicit view of the environment and of what is installed.
pub fn init_with(
    path: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    force: bool,
    found: Option<Found>,
    env: &dyn Fn(&str) -> bool,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("init", status, &m);
    // The Spec Kit root when there is one; else the path itself.
    let root = match crate::inspect(path, None).data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => path.to_path_buf(),
    };
    let file = root.join(FILE);
    if file.exists() && !force {
        // It exists: change it, or start it over — never `init` again.
        return refuse(
            CommandStatus::ConfigError,
            format!("{} exists already", file.display()),
        )
        .with_code("CONFIG-EXISTS")
        .with_fix(vec![
            "speccoder config".to_owned(),
            "speccoder init --force".to_owned(),
        ]);
    }
    if let Some(p) = provider.filter(|p| !matches!(*p, "anthropic" | "openai")) {
        return refuse(
            CommandStatus::UsageError,
            format!("--provider {p}: anthropic or openai"),
        );
    }
    let found = found.unwrap_or_else(|| detect(&root));
    let checks = propose(&found);
    let (provider, model) = model_choice(provider, model, env);
    let mut text = render(
        &checks,
        &allow(&found),
        formatter(&found).as_deref(),
        &provider,
        &model,
    );
    // The repository's instructions for agents: AGENTS.md is read by
    // default; a repository that only has CLAUDE.md gets it named too.
    let instructions: Vec<String> = ["AGENTS.md", "CLAUDE.md"]
        .iter()
        .filter(|n| root.join(n).is_file())
        .map(|n| (*n).to_owned())
        .collect();
    if instructions == ["CLAUDE.md"] {
        text.push_str(
            "\n# The repository's instructions for agents, given to every attempt: CLAUDE.md\n# here, AGENTS.md wherever a directory has one.\n[context]\ninstructions = [\"AGENTS.md\", \"CLAUDE.md\"]\n",
        );
    }
    let variable = if provider == "openai" {
        "OPENAI_API_KEY"
    } else {
        "ANTHROPIC_API_KEY"
    }
    .to_owned();
    let key = speccoder_core::InitKey {
        set: env(&variable),
        variable,
    };
    // Never write what speccoder itself would refuse.
    if let Err(e) = crate::verify::config::parse(&text)
        .and_then(|_| crate::substrate::model_config::parse(&text).map(|_| ()))
        .and_then(|()| crate::substrate::run_config::parse(&text).map(|_| ()))
    {
        return refuse(
            CommandStatus::Error,
            format!("generated configuration is invalid: {e}"),
        );
    }
    let mut wrote = Vec::new();
    if let Err(e) = std::fs::write(&file, &text) {
        return refuse(CommandStatus::Error, format!("{}: {e}", file.display()));
    }
    wrote.push(FILE.to_owned());
    // The nextest profile the `test` check names, unless one is there.
    if checks.iter().any(|c| c.argv.iter().any(|a| a == "nextest")) {
        let nextest = root.join(".config/nextest.toml");
        let existing = std::fs::read_to_string(&nextest).unwrap_or_default();
        if !existing.contains("[profile.speccoder]") {
            let _ = std::fs::create_dir_all(root.join(".config"));
            if std::fs::write(&nextest, format!("{existing}{NEXTEST_PROFILE}")).is_ok() {
                wrote.push(".config/nextest.toml".to_owned());
            }
        }
    }
    // What checks write — speccoder's reports under .speccoder/, the
    // toolchains' build output — is output, not source: kept out of git, or
    // every run would find it in the worktree and set it aside. Added to
    // .gitignore only when that file is new or has no other uncommitted
    // change (so committing it commits nothing else).
    let mut output: Vec<&str> = Vec::new();
    if checks
        .iter()
        .any(|c| c.argv.iter().any(|a| a.contains(".speccoder/")))
    {
        output.push(".speccoder/");
    }
    for c in &checks {
        for dir in crate::worktree::build_output(&c.argv) {
            if !output.contains(dir) {
                output.push(dir);
            }
        }
    }
    let git = Git::at(&root);
    let missing: Vec<&str> = output
        .into_iter()
        .filter(|d| {
            !git.succeeds(&["check-ignore", "-q", "--no-index", d])
                .unwrap_or(false)
        })
        .collect();
    if !missing.is_empty() {
        let ignore = root.join(".gitignore");
        let existing = std::fs::read_to_string(&ignore).unwrap_or_default();
        let clean = !ignore.exists()
            || git
                .line(&["status", "--porcelain", "--", ".gitignore"])
                .is_ok_and(|l| l.trim().is_empty());
        if clean {
            let sep = if existing.is_empty() || existing.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            if std::fs::write(
                &ignore,
                format!(
                    "{existing}{sep}# what checks write (speccoder init)\n{}\n",
                    missing.join("\n")
                ),
            )
            .is_ok()
            {
                wrote.push(".gitignore".to_owned());
            }
        }
    }
    let message = if checks.is_empty() {
        format!("wrote {FILE} without checks — add one before `speccoder run`")
    } else {
        format!(
            "wrote {} with {} check(s) and {provider}/{model}; review it, then commit it",
            speccoder_core::words::listed(&wrote),
            checks.len()
        )
    };
    CommandResult::new("init", CommandStatus::Ok, &message).with_data(CommandData::Init(Box::new(
        speccoder_core::InitReport {
            path: file.display().to_string(),
            checks: checks
                .iter()
                .map(|c| speccoder_core::InitCheck {
                    name: c.name.clone(),
                    argv: c.argv.clone(),
                    tier: c.tier.clone(),
                    why: c.why.clone(),
                })
                .collect(),
            model: format!("{provider}/{model}"),
            wrote,
            ran: Vec::new(),
            instructions,
            key,
            committed: None,
        },
    )))
}

/// Whether a test check found no tests to run — an empty suite on a fresh
/// scaffold (FR-027): pytest's exit 5, `go test` with only packages
/// without test files, vitest, jest or nextest reporting nothing to run.
/// Neither passing nor failing: *no tests yet*.
#[must_use]
pub fn no_tests(argv: &[String], exit: Option<i32>, output: &str) -> bool {
    let has = |w: &str| argv.iter().any(|a| a == w || a.ends_with(&format!("/{w}")));
    let lower = output.to_lowercase();
    if has("pytest") {
        return exit == Some(5);
    }
    if has("go") && argv.iter().any(|a| a == "test") {
        let said: Vec<&str> = output
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        return exit == Some(0)
            && !said.is_empty()
            && said
                .iter()
                .all(|l| l.contains("no test files") || l.contains("matched no packages"));
    }
    // Only a run that ended as an empty suite ends: a failure that also
    // says "no tests" somewhere stays a failure.
    if has("vitest") || has("jest") || argv.iter().any(|a| a == "test") && has("npm") {
        return exit == Some(0)
            && (lower.contains("no test files found") || lower.contains("no tests found"));
    }
    if has("nextest") {
        // nextest's own exit status for "no tests to run".
        return exit == Some(4) && lower.contains("no tests to run");
    }
    false
}

/// Runs the fast-tier checks of the configuration `init` just wrote on the
/// untouched code, confined like in a run.
#[must_use]
pub fn verify_written(root: &Path) -> Vec<speccoder_core::InitRun> {
    use speccoder_core::verify::CheckStatus;
    let Ok((runner, _)) = crate::sandbox::runner_at(root) else {
        return Vec::new();
    };
    let Ok(report) =
        crate::verify::run_checks(root, Some(crate::verify::config::Tier::Fast), &runner)
    else {
        return Vec::new();
    };
    report
        .checks
        .iter()
        .map(|c| speccoder_core::InitRun {
            name: c.name.clone(),
            passed: matches!(c.status, CheckStatus::Passed | CheckStatus::Skipped),
            status: if no_tests(
                &c.argv,
                c.exit_code,
                &format!("{}\n{}", c.stdout_tail, c.stderr_tail),
            ) {
                "no_tests".to_owned()
            } else {
                serde_json::to_value(c.status)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            },
            duration_ms: c.duration_ms,
            reason: (!matches!(c.status, CheckStatus::Passed | CheckStatus::Skipped))
                .then(|| {
                    let said = |t: &str| {
                        t.lines()
                            .rev()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(str::to_owned)
                    };
                    said(&c.stderr_tail)
                        .or_else(|| said(&c.stdout_tail))
                        .or_else(|| c.note.clone())
                        .unwrap_or_else(|| {
                            format!(
                                "exit {}",
                                c.exit_code
                                    .map_or_else(|| "?".to_owned(), |e| e.to_string())
                            )
                        })
                })
                .map(|r| r.chars().take(200).collect()),
        })
        .collect()
}

/// [`init_with`] for this machine.
#[must_use]
pub fn init(
    path: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    force: bool,
) -> CommandResult {
    init_with(path, provider, model, force, None, &|v| {
        std::env::var_os(v).is_some_and(|x| !x.is_empty())
    })
}

/// `init` written and its checks run on the untouched code: what a person
/// then decides to commit.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// `init`'s own result.
    pub written: CommandResult,
    /// Its report, with the checks' runs.
    pub report: speccoder_core::InitReport,
    /// The project root the file was written in.
    pub root: PathBuf,
    /// Everything the checks need committed: the file, and the nextest
    /// profile it names when that is not committed yet.
    pub to_commit: Vec<String>,
    /// Checks that fail on the untouched code.
    pub failed: Vec<String>,
}

/// The files `init` may commit: nothing else, whatever a caller asks.
pub const COMMITTABLE: &[&str] = &[
    FILE,
    ".config/nextest.toml",
    ".gitignore",
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "go.sum",
];

/// Writes the configuration and (unless `no_verify`) runs its checks on the
/// untouched code. A refusal comes back as `Err`.
///
/// # Errors
///
/// `init`'s refusal (a file exists, no project, …).
pub fn prepare(
    path: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    force: bool,
    no_verify: bool,
) -> Result<Prepared, Box<CommandResult>> {
    let written = init(path, provider, model, force);
    let Some(CommandData::Init(report)) = written.data() else {
        return Err(Box::new(written));
    };
    let mut report = (**report).clone();
    let root = PathBuf::from(&report.path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    if !no_verify && !report.checks.is_empty() {
        report.ran = verify_written(&root);
    }
    let failed = report
        .ran
        .iter()
        .filter(|r| !r.passed && r.status != "no_tests")
        .map(|r| r.name.clone())
        .collect();
    let mut to_commit: Vec<String> = report.wrote.clone();
    let profile = ".config/nextest.toml";
    if report
        .checks
        .iter()
        .any(|c| c.argv.iter().any(|a| a == "nextest"))
        && root.join(profile).exists()
        && !to_commit.iter().any(|p| p == profile)
        && Git::at(&root)
            .line(&["status", "--porcelain", "--", profile])
            .is_ok_and(|l| !l.trim().is_empty())
    {
        to_commit.push(profile.to_owned());
    }
    // A lockfile the checks just wrote, never committed: every run would
    // find it in the worktree, so it goes with the configuration.
    for lock in crate::worktree::LOCKFILES {
        if root.join(lock).exists()
            && !to_commit.iter().any(|p| p == lock)
            && Git::at(&root)
                .line(&["status", "--porcelain", "--", lock])
                .is_ok_and(|l| l.starts_with("??"))
        {
            to_commit.push((*lock).to_owned());
        }
    }
    Ok(Prepared {
        written,
        report,
        root,
        to_commit,
        failed,
    })
}

/// Commits exactly `paths` — each one of [`COMMITTABLE`] — as
/// `speccoder: config`; the short id.
///
/// # Errors
///
/// A path `init` does not commit, or git's complaint.
pub fn commit_written(root: &Path, paths: &[String]) -> Result<String, String> {
    commit_written_as(root, paths, "speccoder: config")
}

/// [`commit_written`] with its own subject.
///
/// # Errors
///
/// A path `init` does not commit, or git's complaint.
pub fn commit_written_as(root: &Path, paths: &[String], subject: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to commit".to_owned());
    }
    if let Some(p) = paths.iter().find(|p| !COMMITTABLE.contains(&p.as_str())) {
        return Err(format!("`{p}` is not a file init commits"));
    }
    let g = Git::at(root);
    let names: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut add = vec!["add", "--"];
    add.extend(&names);
    let mut commit = vec!["commit", "-q", "-m", subject, "--"];
    commit.extend(&names);
    g.run(&add).map_err(|e| e.0)?;
    // Nothing differs from the last commit: say that, not an empty failure.
    let mut unchanged = vec!["diff", "--cached", "--quiet", "--"];
    unchanged.extend(&names);
    if g.succeeds(&unchanged).unwrap_or(false) {
        return Err(format!(
            "nothing to commit: {} is committed as it is",
            speccoder_core::words::listed(&names)
        ));
    }
    g.run(&commit)
        .and_then(|_| g.line(&["rev-parse", "--short", "HEAD"]))
        .map_err(|e| {
            if e.0.trim().is_empty() {
                "git commit failed without saying why".to_owned()
            } else {
                e.0
            }
        })
}

/// The result of an `init` the person has decided about: the message, the
/// report, and what comes next.
#[must_use]
pub fn finish(p: Prepared) -> CommandResult {
    let Prepared {
        written,
        report,
        to_commit,
        failed,
        ..
    } = p;
    let mut message = match &report.committed {
        Some(id) => written.message().replace(
            "; review it, then commit it",
            &format!(" and committed it ({id})"),
        ),
        None => written.message().to_owned(),
    };
    if !failed.is_empty() {
        message = format!(
            "{message}; the check(s) {} fail on the untouched code: every task would be judged against a red baseline",
            failed.join(", ")
        );
    }
    let step = |command: String, why: &str| speccoder_core::next::NextStep {
        command,
        why: why.to_owned(),
    };
    let mut next = Vec::new();
    // No spec project yet: the format's tool makes one — first, before the
    // commit and the key matter.
    let root = PathBuf::from(&report.path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let format = crate::format::at(&root);
    if format.detect(&root).is_none() {
        let (command, why) = format.messages().make_project;
        message = format!("{message}; there is no spec project here yet");
        next.push(step(command.to_owned(), why));
    }
    if report.committed.is_none() {
        let listed = to_commit.join(" ");
        next.push(step(
            format!("git add {listed} && git commit -m \"speccoder: config\" -- {listed}"),
            "speccoder reads committed files only",
        ));
    }
    if !report.key.set {
        next.push(step(
            format!("export {}=…", report.key.variable),
            "the model key, from the environment only",
        ));
    }
    // Something to run, or something to write first.
    let features = match crate::current::list(&root, None).data() {
        Some(CommandData::Features(f)) => f.features.len(),
        _ => 0,
    };
    if features == 0 {
        next.push(step(
            "speccoder spec specify \"…\"".to_owned(),
            "describe the first feature; its spec, plan and tasks follow",
        ));
    } else {
        next.push(step(
            "speccoder run".to_owned(),
            "work through the ready tasks",
        ));
    }
    CommandResult::new("init", CommandStatus::Ok, &message)
        .with_data(CommandData::Init(Box::new(report)))
        .with_next(next)
}

/// What `init` would write here, without writing: for the setup assistant.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    /// Whether the project already has a `speccoder.toml`.
    pub configured: bool,
    /// The checks it would declare.
    pub checks: Vec<Proposed>,
    /// Commands the model could run without asking.
    pub allow: Vec<Vec<String>>,
    /// The formatter it would configure.
    pub formatter: Option<Vec<String>>,
    /// The model key variables, and whether each is set.
    pub keys: Vec<(String, bool)>,
}

/// Where `init` writes for `path`: the spec project's root, else `path`.
#[must_use]
pub fn root_of(path: &Path) -> PathBuf {
    match crate::inspect(path, None).data() {
        Some(CommandData::Inspect(s)) => PathBuf::from(&s.root),
        _ => path.to_path_buf(),
    }
}

/// [`Preview`] of `path`.
#[must_use]
pub fn preview(path: &Path) -> Preview {
    let root = root_of(path);
    let found = detect(&root);
    let set = |v: &str| std::env::var_os(v).is_some_and(|x| !x.is_empty());
    Preview {
        configured: root.join(FILE).is_file(),
        checks: propose(&found),
        allow: allow(&found),
        formatter: formatter(&found),
        keys: ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
            .iter()
            .map(|v| ((*v).to_owned(), set(v)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_follow_what_was_found() {
        let rust = propose(&Found {
            cargo: true,
            nextest: true,
            ..Found::default()
        });
        let names: Vec<&str> = rust.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["test", "fmt", "clippy"]);
        assert_eq!(
            rust[0].junit.as_deref(),
            Some("target/nextest/speccoder/junit.xml")
        );
        assert_eq!(rust[2].tier, "full");
        let plain = propose(&Found {
            cargo: true,
            ..Found::default()
        });
        assert_eq!(plain[0].argv, ["cargo", "test", "--workspace"]);

        let node = propose(&Found {
            node_manager: Some("pnpm".to_owned()),
            npm_scripts: vec!["test".into(), "lint".into(), "build".into()],
            ..Found::default()
        });
        let shown: Vec<(String, String)> = node
            .iter()
            .map(|c| (c.name.clone(), c.argv.join(" ")))
            .collect();
        assert_eq!(
            shown,
            [
                ("test".into(), "pnpm run test".into()),
                ("lint".into(), "pnpm run lint".into())
            ]
        );
        let npm = propose(&Found {
            node_manager: Some("npm".to_owned()),
            npm_scripts: vec!["test".into()],
            ..Found::default()
        });
        assert_eq!(npm[0].argv, ["npm", "run", "test"]);

        let py = propose(&Found {
            python: true,
            ruff: true,
            ..Found::default()
        });
        assert_eq!(
            py.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["test", "ruff"]
        );
        let go = propose(&Found {
            go: true,
            ..Found::default()
        });
        assert_eq!(go[0].argv, ["go", "test", "./..."]);
        let make = propose(&Found {
            make_test: true,
            ..Found::default()
        });
        assert_eq!(make[0].argv, ["make", "test"]);
        assert!(propose(&Found::default()).is_empty());
    }

    #[test]
    fn the_model_follows_the_key_that_is_set() {
        let none = |_: &str| false;
        let openai = |v: &str| v == "OPENAI_API_KEY";
        assert_eq!(model_choice(None, None, &none).0, "anthropic");
        assert_eq!(
            model_choice(None, None, &openai),
            ("openai".to_owned(), "gpt-5.4-mini".to_owned())
        );
        assert_eq!(
            model_choice(Some("anthropic"), Some("m"), &openai),
            ("anthropic".to_owned(), "m".to_owned())
        );
    }

    #[test]
    fn every_rendered_file_parses() {
        for found in [
            Found {
                cargo: true,
                nextest: true,
                ..Found::default()
            },
            Found {
                python: true,
                ruff: true,
                mypy: true,
                go: true,
                ..Found::default()
            },
            Found::default(),
        ] {
            let text = render(
                &propose(&found),
                &allow(&found),
                formatter(&found).as_deref(),
                "openai",
                "gpt-5.4-mini",
            );
            if found.cargo {
                assert!(
                    text.contains("[format]\nargv = [\"cargo\", \"fmt\", \"--all\"]"),
                    "{text}"
                );
                assert_eq!(
                    crate::substrate::run_config::parse(&text)
                        .ok()
                        .and_then(|c| c.format),
                    Some(vec![
                        "cargo".to_owned(),
                        "fmt".to_owned(),
                        "--all".to_owned()
                    ])
                );
            }
            let checks =
                crate::verify::config::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert_eq!(checks.len(), propose(&found).len());
            assert!(crate::substrate::model_config::parse(&text).is_ok_and(|m| m.is_some()));
            assert!(crate::substrate::run_config::parse(&text).is_ok());
        }
    }

    #[test]
    fn an_empty_suite_on_a_fresh_scaffold_is_no_tests_yet() {
        let argv = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(no_tests(
            &argv(&["uv", "run", "pytest"]),
            Some(5),
            "no tests ran"
        ));
        assert!(!no_tests(
            &argv(&["uv", "run", "pytest"]),
            Some(1),
            "1 failed"
        ));
        assert!(no_tests(
            &argv(&["go", "test", "./..."]),
            Some(0),
            "go: warning: \"./...\" matched no packages\n"
        ));
        assert!(no_tests(
            &argv(&["go", "test", "./..."]),
            Some(0),
            "?   \tx\t[no test files]\n"
        ));
        assert!(!no_tests(
            &argv(&["go", "test", "./..."]),
            Some(0),
            "ok  \tx\t0.1s\n"
        ));
        assert!(no_tests(
            &argv(&["npm", "test"]),
            Some(0),
            "No test files found, exiting with code 0"
        ));
        assert!(no_tests(
            &argv(&["npx", "vitest", "run"]),
            Some(0),
            "No test files found"
        ));
        assert!(no_tests(
            &argv(&["cargo", "nextest", "run"]),
            Some(4),
            "error: no tests to run"
        ));
        assert!(!no_tests(
            &argv(&["cargo", "test"]),
            Some(0),
            "test result: ok. 1 passed"
        ));
    }
}
