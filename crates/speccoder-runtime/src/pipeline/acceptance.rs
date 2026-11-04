//! Spec-derived acceptance tests: written before the attempt, red then green.

use std::path::Path;

use speccoder_core::verify::{RedClass, classify_red};

use crate::{sandbox, substrate, tools, verify, worktree};

pub(crate) struct AcceptanceSetup<'a> {
    pub(crate) rt: &'a tokio::runtime::Runtime,
    pub(crate) plane: &'a substrate::Plane,
    pub(crate) project: &'a Path,
    pub(crate) worktree: &'a Path,
    pub(crate) tmp: &'a Path,
    pub(crate) policy: &'a tools::policy::Policy,
    pub(crate) feature: &'a str,
    pub(crate) task: &'a str,
    pub(crate) bundle: serde_json::Value,
    /// The task's scenarios, which each test cites.
    pub(crate) scenarios: &'a [speccoder_core::spec::ScenarioRef],
    /// The repository's instructions (test conventions live there too).
    pub(crate) instructions: &'a [crate::instructions::Instruction],
    pub(crate) runner: &'a sandbox::Runner,
    /// The project's test layout: where acceptance files go (`None`: the
    /// author places them by the old naming rule).
    pub(crate) layout: Option<&'a crate::layout::TestLayout>,
    /// A person's rulings that the task's earlier tests were wrong: their
    /// words, told to the author verbatim.
    pub(crate) told: &'a [String],
    /// The tasks on the same requirements (or named by this one's line):
    /// their tests are shown, never contradicted.
    pub(crate) related: &'a [String],
    /// A story's oracle, written before its code: imports of what the
    /// story adds are expected, never checked against the code.
    pub(crate) oracle: bool,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedAcceptance {
    /// What the run report shows.
    pub(crate) report: serde_json::Value,
    /// The command, when the tests were written.
    pub(crate) argv: Option<Vec<String>>,
    /// Further commands of the set (a bug's reproductions run one test
    /// each); every one must pass.
    #[serde(default)]
    pub(crate) more_argv: Vec<Vec<String>>,
    /// They failed before the attempt, on an assertion or for an absent
    /// symbol.
    pub(crate) red: bool,
    /// What the red run showed.
    #[serde(default)]
    pub(crate) red_class: Option<speccoder_core::verify::RedClass>,
    /// The held-out manifest's hash, when a held-out set was kept: a resumed
    /// attempt reuses it, never re-authors it.
    #[serde(default)]
    pub(crate) held_out: Option<String>,
    /// Why the author call itself failed (the provider refused, the run
    /// failed) — not "the spec has no examples": the task stops for a person.
    #[serde(default)]
    pub(crate) generation_failed: Option<String>,
    /// Why no acceptance test was kept, typed: what a regression-only
    /// reason is decided from (`report["skipped"]` is its display).
    #[serde(default)]
    pub(crate) skip_cause: Option<speccoder_core::regression::SkipCause>,
}

pub(crate) const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
];

pub(crate) fn prepare_acceptance(a: &AcceptanceSetup<'_>) -> PreparedAcceptance {
    use serde_json::json;
    use speccoder_core::regression::SkipCause;
    let skipped_by = |why: String, cause: Option<SkipCause>, report: serde_json::Value| {
        let mut report = report;
        report["skipped"] = json!(why);
        PreparedAcceptance {
            report,
            argv: None,
            more_argv: Vec::new(),
            red: false,
            red_class: None,
            held_out: None,
            generation_failed: None,
            skip_cause: cause,
        }
    };
    let failed = |why: String, report: serde_json::Value| PreparedAcceptance {
        generation_failed: Some(why.clone()),
        ..skipped_by(why, None, report)
    };

    let files: Vec<String> = worktree::checkpoint(a.worktree, a.tmp)
        .and_then(|c| worktree::files(a.worktree, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    // Where the tests go: the task's own file in the project's layout, or
    // no place the runner collects.
    let placement = match placement(a) {
        Ok(p) => p,
        Err(()) => {
            return skipped_by(
                "no place the runner collects".to_owned(),
                Some(SkipCause::NoPlace),
                json!({}),
            );
        }
    };
    let title = a.bundle["task"]["title"].as_str().unwrap_or_default();
    let suite = suite(a.worktree, title, a.related);
    let interface = interface(a.worktree, title);
    let commands = task_commands(a);
    let shown: Vec<String> = commands.iter().map(|c| c.shown.clone()).collect();
    // A refused answer is asked for once more, told exactly why: what
    // speccoder refuses is checked, never left to the author's reading.
    let mut refusals: Vec<String> = Vec::new();
    // Both calls' tokens, when it is asked again.
    let mut spent: Option<serde_json::Value> = None;
    let (answer, base, planned, argv, ran) = loop {
        let input = substrate::acceptance::input(
            a.bundle.clone(),
            a.scenarios,
            &files,
            &manifest,
            a.instructions,
            placement.as_ref(),
            &substrate::acceptance::Known {
                told: a.told,
                refused: &refusals,
                suite: &suite,
                interface: &interface,
                commands: &shown,
            },
        );
        let outcome = a.rt.block_on(a.plane.acceptance(a.feature, a.task, input));
        let outcome = match outcome {
            Ok(o) => o,
            Err(e) => return failed(format!("generation failed: {}", e.0), json!({})),
        };
        let output = outcome
            .output
            .as_ref()
            .map(|o| o.peek().clone())
            .unwrap_or_default();
        let answer = output["answer"].clone();
        spent = super::commit::add_usage(spent.as_ref(), output.get("usage"));
        let mut base = json!({ "run_id": outcome.run_id.to_string(), "usage": spent, "notes": answer["notes"] });
        if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null()
        {
            let status = format!("{:?}", outcome.status);
            let why = substrate::provider_refusal(&status).map_or_else(
                || format!("generation failed: {}", substrate::why_not(&outcome.status)),
                |r| format!("the model provider refused the request: {r}"),
            );
            return failed(why, base);
        }
        if !refusals.is_empty() {
            base["refused_first"] = json!(refusals);
        }

        // Validate: new test files only, a command the policy allows, and
        // calls to code that is there (or that the task adds).
        let (valid, mut refused) =
            validate_answer(&answer, a.project, a.policy, a.scenarios, Some(title));
        // The task is about the command line: each test runs the command.
        for f in valid
            .iter()
            .filter(|f| !commands.iter().any(|c| c.run_by(&f.content)))
        {
            if let Some(c) = commands.first() {
                refused.push(RefusedFile {
                    path: f.path.clone(),
                    why: "no_command",
                    message: format!(
                        "`{}` never runs the project's command: run `{}` as a person would, in a subprocess",
                        f.path, c.shown
                    ),
                });
            }
        }
        // Valid: written and run red here, so a test red only by its own
        // error, or one that does not build or collect, is refused too.
        let planned: Vec<(String, String)> = valid
            .iter()
            .map(|f| (f.path.clone(), f.content.clone()))
            .collect();
        let argv: Vec<String> = argv_of(&answer["argv"]);
        let paths: Vec<&str> = planned.iter().map(|(p, _)| p.as_str()).collect();
        let mut ran = None;
        if refused.is_empty() && !planned.is_empty() {
            if !test_command_allowed(a.policy, a.layout, &argv, &paths) {
                if !refusals.is_empty() {
                    return skipped_by(
                        format!("`{}` is not an allowed command", argv.join(" ")),
                        Some(SkipCause::CommandNotAllowed {
                            argv: argv.join(" "),
                        }),
                        base,
                    );
                }
                refused.push(RefusedFile {
                    path: paths[0].to_owned(),
                    why: "command_not_allowed",
                    message: format!(
                        "`{}` is not an allowed command: give the command that runs only your files",
                        argv.join(" ")
                    ),
                });
            } else {
                for (rel, content) in &planned {
                    let full = a.project.join(rel);
                    if let Some(parent) = full.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&full, content) {
                        return skipped_by(
                            format!("`{rel}`: {e}"),
                            Some(SkipCause::WriteFailed { path: rel.clone() }),
                            base,
                        );
                    }
                }
                let (first, summary) = execute(a.project, &argv, a.runner);
                let class = classify_red(&first, summary.as_ref());
                // Red only on names the code does not have and the task does
                // not add (a module, a class, an attribute of the wrong shape)
                // is the test's own mistake, never the task's gap.
                let named = named(title);
                let theirs = |n: &str| {
                    let last = n.rsplit(['.', '/']).next().unwrap_or(n);
                    named.iter().any(|w| w == last) || title.contains(n)
                };
                let not_there = speccoder_core::verify::missing_names(&first)
                    .filter(|names| !names.is_empty() && !names.iter().any(|n| theirs(n)));
                let escape = crate::integrity::invalid_escape(
                    &format!("{}\n{}", first.stdout_tail, first.stderr_tail),
                    &paths,
                );
                let wrong = match class {
                    _ if escape.is_some() => escape.map(|w| {
                        format!("hold a string other than written (an invalid escape: {w})")
                    }),
                    RedClass::CollectionError | RedClass::BuildError => {
                        Some("do not build or collect".to_owned())
                    }
                    _ => not_there.map(|names| {
                        format!(
                            "fail only on what the code does not have ({}), never on an assertion",
                            names.join(", ")
                        )
                    }),
                };
                match wrong {
                    Some(how) if refusals.is_empty() => {
                        for rel in &paths {
                            let _ = std::fs::remove_file(a.project.join(rel));
                        }
                        refused.push(RefusedFile {
                            path: paths[0].to_owned(),
                            why: "errored",
                            message: format!(
                                "`{}` {how}: {}",
                                paths.join("`, `"),
                                first_failure(&first)
                            ),
                        });
                    }
                    _ => ran = Some((first, summary)),
                }
            }
        }
        if refused.is_empty() {
            break (answer, base, planned, argv, ran);
        }
        if refusals.is_empty() {
            refusals.extend(refused.iter().map(|r| r.message.clone()));
            continue;
        }
        let r = &refused[0];
        return skipped_by(
            r.message.clone(),
            Some(SkipCause::Refused {
                path: r.path.clone(),
                why: refusal_words(r.why).to_owned(),
            }),
            base,
        );
    };
    let tests: Vec<serde_json::Value> = planned
        .iter()
        .map(|(p, _)| {
            let cited = valid_cites(&answer, p, a.scenarios);
            json!({ "path": p, "scenarios": cited })
        })
        .collect();
    let Some((first, summary)) = ran else {
        return skipped_by(
            "no acceptance tests were generated".to_owned(),
            Some(SkipCause::NoFiles),
            base,
        );
    };
    let mut written: Vec<String> = planned.iter().map(|(p, _)| p.clone()).collect();

    // Red: they must fail before the task is done — on an assertion, or
    // because what the task introduces does not exist yet; confirmed by a
    // second run.
    let red_tests = speccoder_core::verify::collected_count(&first, summary.as_ref());
    let mut class = classify_red(&first, summary.as_ref());
    if class.is_red() {
        let (second, summary) = execute(a.project, &argv, a.runner);
        if !classify_red(&second, summary.as_ref()).is_red() {
            class = RedClass::Flaky;
        }
    }
    let red = class.is_red();
    // Tests that do not even build or collect would break every other test
    // of the project for the attempt: they are taken out again.
    let broken = matches!(class, RedClass::CollectionError | RedClass::BuildError);
    let removed = if broken {
        for rel in &written {
            let _ = std::fs::remove_file(a.project.join(rel));
        }
        std::mem::take(&mut written)
    } else {
        Vec::new()
    };
    let mut report = base;
    report["files"] = json!(written);
    if !removed.is_empty() {
        report["removed"] = json!(removed);
    }
    report["tests"] = json!(tests);
    report["argv"] = json!(argv);
    report["red"] = json!(red);
    report["red_class"] = json!(class);
    if let Some(n) = red_tests {
        report["red_tests"] = json!(n);
    }
    if red {
        // What the attestation's red excerpt is cut from (clean-checked there).
        report["red_output"] = json!(
            format!("{}\n{}", first.stdout_tail, first.stderr_tail)
                .trim()
                .to_owned()
        );
    } else {
        report["note"] = json!(match class {
            RedClass::Passed => "the acceptance tests already pass before the attempt, so they are no evidence for it".to_owned(),
            RedClass::Flaky => "the acceptance tests failed once and passed when run again: not demonstrated".to_owned(),
            other if broken => format!(
                "the acceptance tests did not fail on an assertion ({}): not demonstrated, and taken out so the project's other tests still run",
                class_word(other)
            ),
            other => format!(
                "the acceptance tests did not fail on an assertion ({}): not demonstrated",
                class_word(other)
            ),
        });
        report["excerpt"] = json!(excerpt(&first));
    }
    // Green runs what went red, and what already passed (a regression
    // guard); tests that never failed on an assertion — taken out, or that
    // do not collect or time out — would fail every attempt, whatever it
    // does.
    let kept = red || class == RedClass::Passed;
    PreparedAcceptance {
        report,
        argv: kept.then_some(argv),
        more_argv: Vec::new(),
        red,
        red_class: Some(class),
        held_out: None,
        generation_failed: None,
        skip_cause: None,
    }
}

/// Where the visible author writes, from the layout: `Ok(None)` when no
/// runner speccoder knows was found (the author names the files), `Err`
/// when the layout offers no place the runner collects.
fn placement(a: &AcceptanceSetup<'_>) -> Result<Option<substrate::acceptance::Placement>, ()> {
    let Some(layout) = a.layout.filter(|l| !l.runners.is_empty()) else {
        return Ok(None);
    };
    let task = a.task.to_owned();
    // Go: the package the task names, else the module's root.
    let package =
        speccoder_core::scope::named_paths(a.bundle["task"]["title"].as_str().unwrap_or_default())
            .into_iter()
            .find(|p| p.ends_with(".go") || !p.contains('.'))
            .map(|p| {
                if p.ends_with(".go") {
                    p.rsplit_once('/')
                        .map_or(String::new(), |(d, _)| d.to_owned())
                } else {
                    p.trim_end_matches('/').to_owned()
                }
            });
    // Named for the feature too (Spec Kit restarts at T001 in every
    // feature), and free: an old file there is never the task's.
    let number: String = a.feature.chars().take_while(char::is_ascii_digit).collect();
    let scoped = if number.is_empty() {
        task.clone()
    } else {
        format!("{number}_{task}")
    };
    let path = (1..)
        .map(|n| {
            let name = if n == 1 {
                scoped.clone()
            } else {
                format!("{scoped}_{n}")
            };
            layout.acceptance_file(&name, package.as_deref())
        })
        .take(9)
        .find(|p| p.as_ref().is_none_or(|p| !a.project.join(p).exists()))
        .flatten()
        .ok_or(())?;
    let comment = if path.ends_with(".py") { "#" } else { "//" }.to_owned();
    Ok(Some(substrate::acceptance::Placement {
        command: layout.command_for(&path).unwrap_or_default(),
        path,
        task,
        scenarios: a.scenarios.iter().map(|s| s.id.clone()).collect(),
        comment,
        layout: layout.text(),
    }))
}

/// The scenario ids a planned file of `answer` cites, of those given.
fn valid_cites(
    answer: &serde_json::Value,
    path: &str,
    scenarios: &[speccoder_core::spec::ScenarioRef],
) -> Vec<String> {
    answer["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| {
            f["path"]
                .as_str()
                .and_then(|p| tools::paths::normalise(p).ok())
                .is_some_and(|p| p == path)
        })
        .flat_map(|f| f["scenarios"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .filter(|id| scenarios.iter().any(|s| s.id == *id))
        .collect()
}

/// A failed run's first failure, as the runner wrote it (one line).
pub(crate) fn first_failure(r: &speccoder_core::verify::CheckResult) -> String {
    let text = format!("{}\n{}", r.stdout_tail, r.stderr_tail);
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| {
            l.starts_with("FAILED ")
                || l.starts_with("ERROR ")
                || l.starts_with("E ")
                || l.starts_with("error")
                || l.contains("Error:")
        })
        .or_else(|| text.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("no output");
    line.chars().take(240).collect()
}

/// speccoder's words for a refusal's kind (a regression-only reason's detail).
pub(crate) fn refusal_words(why: &str) -> &'static str {
    match why {
        "exists" => "already exists",
        "not_collected" => "is not where the runner collects tests",
        "skip_marker" => "skips a test",
        "placeholder" => "is a placeholder that can never pass",
        "invalid_escape" => "holds a string other than written (an invalid escape sequence)",
        "crlf_as_text" => "asserts `\\r\\n` in output it reads as text, which can never pass",
        "imagined" => "calls code that is not there",
        "errored" => "fails only on an error of its own, or does not build",
        "command_not_allowed" => "gives a command the policy does not allow",
        "no_command" => "never runs the project's command",
        _ => "is not a new test file",
    }
}

/// One test file of an author's answer that may be written.
pub(crate) struct ValidFile {
    /// Normalised, project-relative.
    pub(crate) path: String,
    pub(crate) content: String,
    /// The scenario ids it cites, of those given (may be empty).
    pub(crate) cited: Vec<String>,
    /// The command that runs only it, when the answer gives one.
    pub(crate) argv: Vec<String>,
}

/// A file of an answer that may not be written, and why.
pub(crate) struct RefusedFile {
    /// Its path as given.
    pub(crate) path: String,
    /// `not_a_new_test_file` or `exists`.
    pub(crate) why: &'static str,
    /// What a person is told.
    pub(crate) message: String,
}

/// The command an author's answer gives: its argv — or, given as one
/// string (`["cargo test --test acceptance_us1"]`, the line the prompt
/// shows), that line split into its words when it is a plain command
/// (anything a shell would read differently stays one word, and is
/// refused as the command it is not).
#[must_use]
pub(crate) fn argv_of(v: &serde_json::Value) -> Vec<String> {
    let argv: Vec<String> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    match argv.as_slice() {
        [line] if line.split_ascii_whitespace().count() > 1 => {
            let words = crate::acp::govern::argv_of_line(line);
            // A line a shell would read: left whole, never guessed at.
            if words.len() == 3 && words[0] == "sh" && words[1] == "-c" && words[2] == line.trim() {
                argv
            } else {
                words
            }
        }
        _ => argv,
    }
}

/// Whether the author's `argv` may run: the policy allows it, or it is
/// the layout's command that runs only one of `files` (what the prompt
/// names).
pub(crate) fn test_command_allowed(
    policy: &tools::policy::Policy,
    layout: Option<&crate::layout::TestLayout>,
    argv: &[String],
    files: &[&str],
) -> bool {
    layout.is_some_and(|l| l.runs_only(argv, files))
        || policy.classify("run_command", &serde_json::json!({ "argv": argv }), None)
            == tools::policy::Decision::Allow
}

/// The acceptance author's files, checked (the one validation run time and
/// review share): new test paths only, nothing protected, nothing that
/// exists in `project`; each with the scenarios it cites of `scenarios`.
/// `task`: the line of a task whose tests call code that is there (a gap,
/// held-out, strengthening): their imports must resolve. `None` for a
/// story's oracle, written before its code.
pub(crate) fn validate_answer(
    answer: &serde_json::Value,
    project: &Path,
    policy: &tools::policy::Policy,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    task: Option<&str>,
) -> (Vec<ValidFile>, Vec<RefusedFile>) {
    let protected = policy.protected();
    let mut valid = Vec::new();
    let mut refused = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
            continue;
        };
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(e) => {
                refused.push(RefusedFile {
                    path: path.to_owned(),
                    why: "not_a_new_test_file",
                    message: format!("`{path}`: {}", e.0),
                });
                continue;
            }
        };
        if !tools::paths::is_test_path(&rel) || protected.write_class(&rel).is_some() {
            refused.push(RefusedFile {
                message: format!("`{rel}` is not a new test file"),
                path: rel,
                why: "not_a_new_test_file",
            });
            continue;
        }
        if project.join(&rel).exists() {
            refused.push(RefusedFile {
                message: format!("`{rel}` already exists"),
                path: rel,
                why: "exists",
            });
            continue;
        }
        // A new file that skips a test (or expects it to fail) proves
        // nothing it skips.
        if crate::integrity::has_skip_marker(content) {
            refused.push(RefusedFile {
                message: format!("`{rel}` skips a test"),
                path: rel,
                why: "skip_marker",
            });
            continue;
        }
        if crate::integrity::asserts_crlf_as_text(&rel, content) {
            refused.push(RefusedFile {
                message: format!(
                    "`{rel}` asserts `\\r\\n` in output read with `text=True`, which turns it into `\\n`: it can never pass — compare `.splitlines()`, or read bytes"
                ),
                path: rel,
                why: "crlf_as_text",
            });
            continue;
        }
        // A placeholder fails whatever the code does: no evidence.
        if crate::integrity::has_placeholder(content) {
            refused.push(RefusedFile {
                message: format!(
                    "`{rel}` is a placeholder: it raises \"not implemented\" instead of checking the behaviour"
                ),
                path: rel,
                why: "placeholder",
            });
            continue;
        }
        // A new file the runner would never collect is no evidence.
        if crate::collection::statically_refused(project, &rel, content).is_some() {
            refused.push(RefusedFile {
                message: format!("`{rel}` is not where the runner collects tests"),
                path: rel,
                why: "not_collected",
            });
            continue;
        }
        // A test of code that is there calls it as it is.
        if let Some(message) = task.and_then(|t| imagined(project, &rel, content, t)) {
            refused.push(RefusedFile {
                message,
                path: rel,
                why: "imagined",
            });
            continue;
        }
        let cited: Vec<String> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter(|id| scenarios.iter().any(|s| s.id == *id))
            .map(str::to_owned)
            .collect();
        let argv: Vec<String> = argv_of(&f["argv"]);
        valid.push(ValidFile {
            path: rel,
            content: content.to_owned(),
            cited,
            argv,
        });
    }
    (valid, refused)
}

/// The class as written in reports.
pub(crate) fn class_word(class: RedClass) -> String {
    serde_json::to_value(class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Up to 20 lines from the first error line of a run's output (else its
/// last 20 lines).
fn excerpt(result: &speccoder_core::verify::CheckResult) -> String {
    let text = format!("{}\n{}", result.stdout_tail, result.stderr_tail);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let from = lines
        .iter()
        .position(|l| {
            let l = l.to_lowercase();
            l.contains("error") || l.contains("fail")
        })
        .unwrap_or_else(|| lines.len().saturating_sub(20));
    lines
        .iter()
        .skip(from)
        .take(20)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// One run of an acceptance command, with the runner's summary.
pub(crate) fn acceptance_run(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> (
    speccoder_core::verify::CheckResult,
    Option<speccoder_core::verify::RunnerSummary>,
) {
    execute(project, argv, runner)
}

pub(crate) fn run_acceptance(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> speccoder_core::verify::CheckResult {
    execute(project, argv, runner).0
}

/// Runs the acceptance tests once; the runner's summary is read from the
/// whole captured output, before it is cut to tails.
fn execute(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> (
    speccoder_core::verify::CheckResult,
    Option<speccoder_core::verify::RunnerSummary>,
) {
    use speccoder_core::verify::{CheckResult, CheckStatus};
    use verify::process::CommandRunner;
    let out = runner.run(&verify::process::Invocation {
        argv: argv.to_vec(),
        cwd: project.to_path_buf(),
        env: verify::process::scrubbed_env(std::env::vars()),
        timeout: std::time::Duration::from_secs(600),
        output_cap: verify::process::OUTPUT_CAP,
    });
    let status = if out.spawn_error.is_some() {
        CheckStatus::Error
    } else if out.timed_out {
        CheckStatus::TimedOut
    } else if out.exit_code == Some(0) {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };
    let summary = verify::summary::parse(
        &String::from_utf8_lossy(&out.stdout.kept),
        &String::from_utf8_lossy(&out.stderr.kept),
        out.exit_code,
    );
    let tail = |b: &[u8]| {
        let start = b.len().saturating_sub(4096);
        String::from_utf8_lossy(&b[start..]).into_owned()
    };
    let result = CheckResult {
        name: "acceptance".to_owned(),
        argv: argv.to_vec(),
        status,
        exit_code: out.exit_code,
        duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
        junit: false,
        tests: None,
        stdout_tail: tail(&out.stdout.kept),
        stderr_tail: tail(&out.stderr.kept),
        truncated: out.stdout.truncated() || out.stderr.truncated(),
        note: out.spawn_error,
        flaky: false,
        sandbox: out.sandbox,
    };
    (result, summary)
}

/// The held-out set's author, store and red check: after a visible
/// set that was red. Returns the report part (`acceptance.held_out`) and
/// the stored manifest's hash when files were kept.
pub(crate) fn prepare_held_out(
    a: &AcceptanceSetup<'_>,
    visible: &PreparedAcceptance,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    store: &Path,
    composition: bool,
) -> (serde_json::Value, Option<String>) {
    use serde_json::json;
    let absent = |reason: &str, mut report: serde_json::Value| {
        report["state"] = json!("absent");
        report["reason"] = json!(reason);
        (report, None)
    };
    let visible_files: Vec<(String, String)> = visible.report["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str())
        .map(|p| {
            (
                p.to_owned(),
                std::fs::read_to_string(a.project.join(p)).unwrap_or_default(),
            )
        })
        .collect();
    let files: Vec<String> = worktree::checkpoint(a.worktree, a.tmp)
        .and_then(|c| worktree::files(a.worktree, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest_text = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let title = a.bundle["task"]["title"].as_str().unwrap_or_default();
    let input = substrate::acceptance::held_out_input(
        a.bundle.clone(),
        &files,
        &manifest_text,
        scenarios,
        &visible_files,
        composition,
        a.instructions,
        a.layout.map(crate::layout::TestLayout::text).as_deref(),
        &substrate::acceptance::Known {
            told: a.told,
            refused: &[],
            commands: &task_commands(a)
                .into_iter()
                .map(|c| c.shown)
                .collect::<Vec<_>>(),
            suite: &suite(a.worktree, title, a.related),
            interface: &if a.oracle {
                Vec::new()
            } else {
                interface(a.worktree, title)
            },
        },
    );
    let outcome = match a.rt.block_on(a.plane.acceptance(a.feature, a.task, input)) {
        Ok(o) => o,
        Err(e) => return absent("generation_failed", json!({ "error": e.0 })),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    let base = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"], "notes": answer["notes"] });
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return absent("generation_failed", base);
    }

    // Validate: new test paths, known scenarios, an allowed
    // command each, at most `LIMIT`.
    let known: std::collections::BTreeSet<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
    let protected = a.policy.protected();
    let mut kept: Vec<(crate::held_out::HeldOutFile, String)> = Vec::new();
    let mut discarded: Vec<crate::held_out::Discarded> = Vec::new();
    let mut discard = |path: &str, reason: &str| {
        discarded.push(crate::held_out::Discarded {
            path: path.to_owned(),
            reason: reason.to_owned(),
        });
    };
    let given = answer["files"].as_array().cloned().unwrap_or_default();
    if given.is_empty() {
        return absent("none_generated", base);
    }
    for f in &given {
        let path = f["path"].as_str().unwrap_or_default();
        let content = f["content"].as_str().unwrap_or_default();
        let cited: Vec<String> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        let argv: Vec<String> = argv_of(&f["argv"]);
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(_) => {
                discard(path, "path");
                continue;
            }
        };
        let taken =
            visible_files.iter().any(|(p, _)| *p == rel) || kept.iter().any(|(k, _)| k.path == rel);
        if !tools::paths::is_test_path(&rel)
            || protected.write_class(&rel).is_some()
            || a.project.join(&rel).exists()
            || taken
        {
            discard(&rel, "path");
            continue;
        }
        // A held-out test the runner would never collect judges nothing.
        if crate::collection::statically_refused(a.project, &rel, content).is_some() {
            discard(&rel, "not_collected");
            continue;
        }
        // Nor one that skips, raises "not implemented", or calls code that
        // is not there: it fails whatever the code does.
        if crate::integrity::has_skip_marker(content) {
            discard(&rel, "skip_marker");
            continue;
        }
        if crate::integrity::asserts_crlf_as_text(&rel, content) {
            discard(&rel, "crlf_as_text");
            continue;
        }
        if crate::integrity::has_placeholder(content) {
            discard(&rel, "placeholder");
            continue;
        }
        if !a.oracle && imagined(a.project, &rel, content, title).is_some() {
            discard(&rel, "imagined");
            continue;
        }
        if cited.is_empty() || cited.iter().any(|c| !known.contains(c.as_str())) {
            discard(&rel, "unknown_scenario");
            continue;
        }
        let composed = f["composition"].as_bool().unwrap_or(false);
        if composed && (!composition || cited.len() < 2) {
            discard(&rel, "composition");
            continue;
        }
        if !test_command_allowed(a.policy, a.layout, &argv, &[rel.as_str()]) {
            discard(&rel, "command");
            continue;
        }
        if kept.len() >= crate::held_out::LIMIT {
            discard(&rel, "limit");
            continue;
        }
        kept.push((
            crate::held_out::HeldOutFile {
                path: rel,
                sha256: crate::integrity::sha256_hex(content.as_bytes()),
                scenarios: cited,
                argv,
                composition: composed,
                red_tests: None,
            },
            content.to_owned(),
        ));
    }

    // Red now, on the tree the attempt starts from.
    let checkpoint = worktree::checkpoint(a.worktree, a.tmp)
        .map(|c| c.0)
        .unwrap_or_default();
    // Red by what the file adds: a failure its command shows without it is
    // another test's.
    match crate::held_out::overlay_red(a.worktree, a.project, a.tmp, &kept, a.runner, a.policy) {
        Ok(runs) => {
            let mut red = Vec::new();
            for ((mut file, content), ran) in kept.into_iter().zip(runs) {
                if ran.result.is_none() {
                    discard(&file.path, "collision");
                } else if crate::held_out::red_class(&ran).is_red() {
                    file.red_tests = ran
                        .result
                        .as_ref()
                        .and_then(|(r, s)| speccoder_core::verify::collected_count(r, s.as_ref()));
                    red.push((file, content));
                } else {
                    discard(&file.path, "not_red");
                }
            }
            kept = red;
        }
        Err(e) => {
            return absent(
                "generation_failed",
                json!({ "error": e, "run_id": base["run_id"] }),
            );
        }
    }
    let mut report = base;
    report["discarded"] = json!(discarded);
    if kept.is_empty() {
        let only_uncollected =
            !discarded.is_empty() && discarded.iter().all(|d| d.reason == "not_collected");
        return absent(
            if only_uncollected {
                "not_collected"
            } else {
                "all_discarded"
            },
            report,
        );
    }
    let manifest = crate::held_out::Manifest {
        version: 1,
        feature: a.feature.to_owned(),
        task: a.task.to_owned(),
        author_run: report["run_id"].as_str().unwrap_or_default().to_owned(),
        checkpoint,
        visible_scenarios: scenarios.iter().map(|s| s.id.clone()).collect(),
        files: kept.iter().map(|(f, _)| f.clone()).collect(),
        discarded,
    };
    let contents: Vec<(String, String)> = kept
        .iter()
        .map(|(f, c)| (f.path.clone(), c.clone()))
        .collect();
    match crate::held_out::store(store, &manifest, &contents) {
        Ok(sha) => {
            report["state"] = json!("on");
            report["manifest"] = json!(format!("sha256:{sha}"));
            report["kept"] = json!(kept.len());
            (report, Some(sha))
        }
        Err(e) => absent("generation_failed", json!({ "error": e })),
    }
}

/// The held-out verdict on the tree an attempt left: `Ok` with what the
/// tests said, or `Err` with why a person must look (tampering, a leak, a
/// run that would not leave the tree as it was).
pub(crate) struct HeldOutJudged {
    /// The verdict input.
    pub(crate) held_out: speccoder_core::verify::HeldOut,
    /// The failed scenarios, with the spec's words, for the retry digest.
    pub(crate) failed: Vec<speccoder_core::spec::ScenarioRef>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn judge_held_out(
    store: &Path,
    manifest_sha: &str,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    known_text: &str,
    wt_root: &Path,
    project: &Path,
    tmp: &Path,
    start: &worktree::Checkpoint,
    patch: &str,
    runner: &sandbox::Runner,
    policy: &tools::policy::Policy,
) -> Result<HeldOutJudged, (&'static str, String)> {
    use speccoder_core::verify::{CheckStatus, HeldOut};
    let (manifest, contents) =
        crate::held_out::load_verified(store, manifest_sha).map_err(|t| ("tampered", t.0))?;
    let files: Vec<(crate::held_out::HeldOutFile, String)> = manifest
        .files
        .iter()
        .cloned()
        .zip(contents.into_iter().map(|(_, c)| c))
        .collect();
    let leaks = crate::held_out::leaked(&files, known_text, wt_root, start, patch);
    if !leaks.is_empty() {
        return Err((
            "leak_suspected",
            format!(
                "the attempt contains {} literal(s) only the held-out tests hold; a person reviews it",
                leaks.len()
            ),
        ));
    }
    let runs = crate::held_out::overlay_run(wt_root, project, tmp, &files, runner, policy)
        .map_err(|e| ("tampered", e))?;
    let mut failed_ids: Vec<String> = Vec::new();
    for ((file, _), ran) in files.iter().zip(&runs) {
        // Green is evidence only when it collected what red did.
        // Green collects what red did — red's count, else what the file
        // declares, at least one — and a count that cannot be read fails.
        let declared = speccoder_core::collection::toolchain_of(&file.path).map_or(0, |t| {
            files
                .iter()
                .find(|(f, _)| f.path == file.path)
                .map_or(0, |(_, c)| speccoder_core::collection::declared_tests(t, c))
        });
        let required = file.red_tests.unwrap_or(declared).max(1);
        let passed = ran.result.as_ref().is_some_and(|(r, sum)| {
            r.status == CheckStatus::Passed
                && speccoder_core::verify::collected_count(r, sum.as_ref())
                    .is_some_and(|green| green >= required)
        });
        if !passed {
            failed_ids.extend(file.scenarios.iter().cloned());
        }
    }
    failed_ids.sort();
    failed_ids.dedup();
    let held_out = if failed_ids.is_empty() {
        HeldOut::Passed { files: files.len() }
    } else {
        HeldOut::Failed {
            scenarios: failed_ids.clone(),
        }
    };
    let failed = failed_ids
        .iter()
        .map(|id| {
            scenarios
                .iter()
                .find(|s| &s.id == id)
                .cloned()
                .unwrap_or_else(|| speccoder_core::spec::ScenarioRef {
                    id: id.clone(),
                    text: String::new(),
                })
        })
        .collect();
    Ok(HeldOutJudged { held_out, failed })
}

/// Added acceptance tests that strengthen a weak set.
pub(crate) struct Strengthened {
    /// The new test files, written into the worktree.
    pub(crate) files: Vec<String>,
    /// The command that runs them.
    pub(crate) argv: Vec<String>,
    /// The author's run and tokens.
    pub(crate) report: serde_json::Value,
}

/// Asks the test author once more, telling it what got through (`weak`),
/// and keeps its new tests only if they fail on the attempt's starting tree
/// (`before`) and pass on its final tree (`after`), where they are left.
/// `Err`: why the set could not be strengthened (no worktree change stays).
pub(crate) fn strengthen(
    a: &AcceptanceSetup<'_>,
    visible: &[(String, String)],
    weak: serde_json::Value,
    before: &worktree::Checkpoint,
    after: &worktree::Checkpoint,
) -> Result<Strengthened, String> {
    use serde_json::json;
    let files: Vec<String> = worktree::files(a.worktree, after)
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let input = substrate::acceptance::strengthen_input(
        a.bundle.clone(),
        a.scenarios,
        &files,
        &manifest,
        visible,
        weak,
        a.instructions,
    );
    let outcome =
        a.rt.block_on(a.plane.acceptance(a.feature, a.task, input))
            .map_err(|e| format!("the test author failed: {}", e.0))?;
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let report = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"] });
    let answer = &output["answer"];
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return Err(format!(
            "the test author did not answer ({})",
            substrate::why_not(&outcome.status)
        ));
    }
    // The same checks as every author's files.
    let title = a.bundle["task"]["title"].as_str().unwrap_or_default();
    let (valid, refused) = validate_answer(
        answer,
        a.project,
        a.policy,
        a.scenarios,
        (!a.oracle).then_some(title),
    );
    if let Some(r) = refused.first() {
        return Err(r.message.clone());
    }
    let planned: Vec<(String, String)> = valid.into_iter().map(|f| (f.path, f.content)).collect();
    if planned.is_empty() {
        return Err("the test author added no tests".to_owned());
    }
    let argv: Vec<String> = argv_of(&answer["argv"]);
    let paths: Vec<&str> = planned
        .iter()
        .map(|(p, _): &(String, String)| p.as_str())
        .collect();
    if !test_command_allowed(a.policy, a.layout, &argv, &paths) {
        return Err(format!("`{}` is not an allowed command", argv.join(" ")));
    }
    let write = || -> Result<(), String> {
        for (rel, content) in &planned {
            let full = a.project.join(rel);
            if let Some(parent) = full.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&full, content).map_err(|e| format!("`{rel}`: {e}"))?;
        }
        Ok(())
    };
    let back =
        |to: &worktree::Checkpoint| worktree::restore(a.worktree, a.tmp, to).map_err(|e| e.0);
    // Red on the starting tree: they fail without the task's work.
    back(before)?;
    let red = write().map(|()| {
        let (r, summary) = execute(a.project, &argv, a.runner);
        classify_red(&r, summary.as_ref())
    });
    back(after)?;
    let red = red?;
    if !red.is_red() {
        return Err(format!(
            "the added tests do not fail without the change ({})",
            class_word(red)
        ));
    }
    // Green on the final tree: the implementation passes them.
    write()?;
    let (green, _) = execute(a.project, &argv, a.runner);
    if green.status != speccoder_core::verify::CheckStatus::Passed {
        back(after)?;
        let cited: Vec<String> = answer["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|f| f["scenarios"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        return Err(format!(
            "the added tests fail the implementation{}",
            if cited.is_empty() {
                String::new()
            } else {
                format!(" (scenarios {})", cited.join(", "))
            }
        ));
    }
    Ok(Strengthened {
        files: planned.into_iter().map(|(p, _)| p).collect(),
        argv,
        report,
    })
}

/// The names a task line (or a requirement's text) gives in backticks: a
/// function, a type, a file's stem.
fn named(title: &str) -> Vec<String> {
    let mut names: Vec<String> = title
        .split('`')
        .skip(1)
        .step_by(2)
        .flat_map(|span| {
            span.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '/' || c == '.'))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .map(|w| {
            // A path names its file's stem; a call its function.
            let file = w.rsplit('/').next().unwrap_or(&w).to_owned();
            file.split('.').next().unwrap_or(&file).to_owned()
        })
        .filter(|w| w.len() >= 4 && w.chars().any(|c| c.is_alphabetic()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Whether a project path is a test's.
fn is_test(p: &str) -> bool {
    p.split('/').any(|seg| {
        let s = seg.to_ascii_lowercase();
        s == "tests"
            || s == "test"
            || s == "spec"
            || s == "__tests__"
            || s.starts_with("test_")
            || s.contains("_test.")
            || s.contains(".test.")
            || s.contains(".spec.")
    })
}

/// The tasks whose tests a new test must agree with: those this task's
/// line names (`per FR-004, T029`) and those citing a requirement or
/// scenario it cites (`FR-004`, `US1/AC3`, `SC-002`).
pub(crate) fn related(
    this: &speccoder_core::snapshot::Task,
    all: &[speccoder_core::snapshot::Task],
) -> Vec<String> {
    let cites = |title: &str| -> Vec<String> {
        title
            .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '/'))
            .map(|w| w.trim_matches('-').to_ascii_uppercase())
            .filter(|w| {
                let (head, num) = w.split_once('-').unwrap_or_default();
                (matches!(head, "FR" | "SC")
                    && !num.is_empty()
                    && num.chars().all(|c| c.is_ascii_digit()))
                    || w.split_once("/AC").is_some_and(|(us, ac)| {
                        us.strip_prefix("US")
                            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                            && !ac.is_empty()
                            && ac.chars().all(|c| c.is_ascii_digit())
                    })
            })
            .collect()
    };
    let mine = cites(&this.title);
    let named = |id: &str| {
        this.title
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == id)
    };
    // The tasks the line names first: three files are shown at most.
    let others: Vec<&speccoder_core::snapshot::Task> =
        all.iter().filter(|t| t.id != this.id).collect();
    others
        .iter()
        .filter(|t| named(&t.id))
        .chain(
            others
                .iter()
                .filter(|t| !named(&t.id) && cites(&t.title).iter().any(|c| mine.contains(c))),
        )
        .map(|t| t.id.clone())
        .collect()
}

/// The project's tests a new test must not contradict, at most three files,
/// 6 KiB in all, each trimmed with a note: first the related tasks' own
/// tests (named after their ids, `test_acceptance_t029.py`), then those
/// that name what the task line names (a backticked function, type or
/// file's stem).
pub(crate) fn suite(
    worktree: &Path,
    title: &str,
    related: &[String],
) -> Vec<substrate::acceptance::SuiteTest> {
    const FILES: usize = 3;
    const BUDGET: usize = 6 * 1024;
    let git = crate::git::Git::at(worktree);
    let ids: Vec<String> = related.iter().map(|t| t.to_ascii_lowercase()).collect();
    let mut paths: Vec<String> = if ids.is_empty() {
        Vec::new()
    } else {
        git.line(&["ls-files", "--cached", "--others", "--exclude-standard"])
            .unwrap_or_default()
            .lines()
            .filter(|p| is_test(p))
            .filter_map(|p| {
                let file = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();
                let at = file
                    .split(|c: char| !c.is_alphanumeric())
                    .filter_map(|w| ids.iter().position(|id| id == w))
                    .min()?;
                Some((at, p.to_owned()))
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    };
    let names = named(title);
    if !names.is_empty() {
        let mut args = vec!["grep", "-l", "-w", "-I"];
        for n in &names {
            args.extend(["-e", n.as_str()]);
        }
        if let Ok(out) = git.line(&args) {
            paths.extend(out.lines().filter(|p| is_test(p)).map(str::to_owned));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    let mut left = BUDGET;
    paths
        .iter()
        .take(FILES)
        .filter_map(|p| {
            let text = std::fs::read_to_string(worktree.join(p)).ok()?;
            let room = left.min(BUDGET / FILES);
            let content = if text.len() > room {
                let cut = (0..=room)
                    .rev()
                    .find(|i| text.is_char_boundary(*i))
                    .unwrap_or(0);
                format!("{}\n… (trimmed)", &text[..cut])
            } else {
                text
            };
            left = left.saturating_sub(content.len());
            Some(substrate::acceptance::SuiteTest {
                path: p.clone(),
                content,
            })
        })
        .collect()
}

/// The code a new test calls, as it is: each source file's declarations
/// (a function, type, method, field or re-export line, as written), the
/// files the task names first, 8 KiB in all — so the author calls the code
/// by the modules, names and shapes it has, never ones it imagines.
/// The commands a task's tests run: the project's, when its title or a
/// scenario it cites is about a command line.
fn task_commands(a: &AcceptanceSetup<'_>) -> Vec<crate::layout::Command> {
    let title = a.bundle["task"]["title"].as_str().unwrap_or_default();
    if crate::layout::task_via_command(title, a.scenarios.iter().map(|s| s.text.as_str())) {
        crate::layout::commands(a.project)
    } else {
        Vec::new()
    }
}

pub(crate) fn interface(worktree: &Path, title: &str) -> Vec<substrate::acceptance::SuiteTest> {
    const BUDGET: usize = 8 * 1024;
    const SOURCE: &[&str] = &[
        "py", "rs", "ts", "tsx", "js", "mjs", "go", "java", "kt", "rb", "cs", "swift",
    ];
    let Ok(out) = crate::git::Git::at(worktree).line(&[
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
    ]) else {
        return Vec::new();
    };
    let names = named(title);
    let stem = |p: &str| {
        let f = p.rsplit('/').next().unwrap_or(p);
        f.split('.').next().unwrap_or(f).to_owned()
    };
    // The product's code: never docs, a site, vendored or built files.
    let elsewhere = |p: &str| {
        p.split('/').any(|seg| {
            matches!(
                seg,
                "docs"
                    | "doc"
                    | "site"
                    | "examples"
                    | "vendor"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | "target"
                    | "scripts"
            ) || seg.starts_with('.')
        })
    };
    let mut files: Vec<&str> = out
        .lines()
        .filter(|p| {
            !is_test(p)
                && !elsewhere(p)
                && p.rsplit_once('.')
                    .is_some_and(|(_, ext)| SOURCE.contains(&ext))
        })
        .collect();
    // The files the task names first, then the source tree's.
    files.sort_by_key(|p| (!names.contains(&stem(p)), !p.starts_with("src/")));
    let mut left = BUDGET;
    let mut shown = Vec::new();
    for p in files {
        let Ok(text) = std::fs::read_to_string(worktree.join(p)) else {
            continue;
        };
        let content = declarations(&text, p.ends_with("__init__.py"));
        if content.is_empty() {
            continue;
        }
        // A file over what is left gives way to smaller ones.
        if content.len() > left {
            continue;
        }
        left -= content.len();
        shown.push(substrate::acceptance::SuiteTest {
            path: p.to_owned(),
            content,
        });
    }
    shown
}

/// A source file's declaration lines, indented as written: a signature
/// spanning lines is joined until its brackets close; a Python name with a
/// leading underscore is private and left out.
fn declarations(text: &str, init: bool) -> String {
    const STARTS: &[&str] = &[
        "def ",
        "async def ",
        "class ",
        "pub ",
        "pub(",
        "export ",
        "func ",
        "type ",
        "public ",
        "fun ",
        "interface ",
        "struct ",
        "enum ",
        "trait ",
    ];
    let mut out = String::new();
    let mut in_class = false;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        let top = t.len() == line.len();
        if top && !t.is_empty() {
            in_class = t.starts_with("class ");
        }
        let private = ["def _", "async def _", "class _"]
            .iter()
            .any(|p| t.starts_with(p) && !t.starts_with("def __init__"));
        // A Python class body's field (`header: tuple[str, ...]`) and a
        // package's re-export (`from .x import Y`) are its shape too.
        let field = in_class
            && !top
            && line.len() - t.len() == 4
            && t.split_once(':').is_some_and(|(n, rest)| {
                !rest.trim().is_empty()
                    && !n.is_empty()
                    && n.chars().all(|c| c.is_alphanumeric() || c == '_')
            });
        let export = init && top && t.starts_with("from .");
        let decl = STARTS.iter().any(|s| t.starts_with(s)) || field || export;
        if !decl || private {
            continue;
        }
        let mut sig = line.trim_end().to_owned();
        let open = |s: &str| {
            s.chars().filter(|c| matches!(c, '(' | '[')).count() as isize
                - s.chars().filter(|c| matches!(c, ')' | ']')).count() as isize
        };
        let mut depth = open(&sig);
        while depth > 0 {
            let Some(next) = lines.next() else { break };
            sig.push(' ');
            sig.push_str(next.trim());
            depth += open(next);
        }
        let sig: String = sig.chars().take(200).collect();
        out.push_str(sig.trim_end_matches(['{', ' ']));
        out.push('\n');
    }
    out
}

/// Whether a new test calls project code that is not there and that the
/// task does not add: a module no file is (and the task line does not
/// name), or a name the module has only in other letter case — the test
/// would be red for its own mistake, never for the task's gap. Python
/// imports and relative JavaScript/TypeScript imports; other languages'
/// compilers say it.
pub(crate) fn imagined(project: &Path, path: &str, content: &str, title: &str) -> Option<String> {
    let named = named(title);
    let adds = |module_path: &str| {
        let stem = module_path
            .rsplit('/')
            .next()
            .unwrap_or(module_path)
            .split('.')
            .next()
            .unwrap_or_default()
            .to_owned();
        title.contains(module_path) || named.contains(&stem)
    };
    let missing = |shown: &str| {
        format!(
            "`{path}` imports `{shown}`, which is not in the code and which this task does not add — call the code by the modules it has (under \"The code as it is\")"
        )
    };
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    if path.ends_with(".py") {
        for (module, names) in python_imports(content) {
            let first = module.split('.').next().unwrap_or_default();
            // Only the project's own modules: at the root or under src/.
            let Some(base) = ["", "src/"].into_iter().find(|b| {
                project.join(format!("{b}{first}")).exists()
                    || project.join(format!("{b}{first}.py")).exists()
            }) else {
                continue;
            };
            let rel = format!("{base}{}", module.replace('.', "/"));
            let file = [
                format!("{rel}.py"),
                format!("{rel}.pyi"),
                format!("{rel}/__init__.py"),
                format!("{rel}/__init__.pyi"),
            ]
            .into_iter()
            .find(|f| project.join(f).is_file());
            let Some(file) = file else {
                // A namespace package (a directory without `__init__.py`).
                if project.join(&rel).is_dir() || adds(&format!("{rel}.py")) {
                    continue;
                }
                return Some(missing(&module));
            };
            let text = std::fs::read_to_string(project.join(&file)).unwrap_or_default();
            // Names it makes at run time are not read from its text.
            if text.contains("import *") || text.contains("__getattr__") {
                continue;
            }
            let declared: Vec<String> = declarations(&text, true)
                .lines()
                .flat_map(|l| {
                    l.split(|c: char| !ident(c))
                        .filter(|w| !w.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect();
            for n in names {
                let there = text.split(|c: char| !ident(c)).any(|t| t == n);
                if there || named.contains(&n) || project.join(format!("{rel}/{n}.py")).is_file() {
                    continue;
                }
                if let Some(real) = declared.iter().find(|t| t.eq_ignore_ascii_case(&n)) {
                    return Some(format!(
                        "`{path}` imports `{n}` from `{module}`, which has `{real}` — call the code by the names it has"
                    ));
                }
            }
        }
        return None;
    }
    if [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .any(|e| path.ends_with(e))
    {
        let dir = Path::new(path).parent().unwrap_or(Path::new(""));
        for spec in relative_imports(content) {
            let joined = dir.join(&spec);
            let rel = crate::tools::paths::normalise(&joined.to_string_lossy()).unwrap_or_default();
            // NodeNext writes `./x.js` for `x.ts`.
            let bare = [".js", ".mjs", ".cjs", ".jsx"]
                .iter()
                .find_map(|e| rel.strip_suffix(e))
                .unwrap_or(&rel);
            let found = [rel.as_str(), bare].iter().any(|r| {
                [
                    "",
                    ".ts",
                    ".tsx",
                    ".js",
                    ".jsx",
                    ".mjs",
                    ".svelte",
                    ".vue",
                    "/index.ts",
                    "/index.tsx",
                    "/index.js",
                ]
                .iter()
                .any(|e| project.join(format!("{r}{e}")).is_file())
            });
            if !found && !adds(&rel) {
                return Some(missing(&spec));
            }
        }
    }
    None
}

/// A Python file's absolute imports: each module, with the names taken
/// from it (`from m import a, b`; none for `import m`).
fn python_imports(content: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("from ") {
            let Some((module, names)) = rest.split_once(" import ") else {
                continue;
            };
            if module.starts_with('.') {
                continue;
            }
            let mut names = names.to_owned();
            if names.contains('(') && !names.contains(')') {
                for next in lines.by_ref() {
                    names.push_str(next);
                    if next.contains(')') {
                        break;
                    }
                }
            }
            let names = names
                .split([',', '(', ')'])
                .filter_map(|n| n.split_whitespace().next())
                .filter(|n| *n != "*" && !n.starts_with('#'))
                .map(str::to_owned)
                .collect();
            out.push((module.trim().to_owned(), names));
        } else if let Some(rest) = t.strip_prefix("import ") {
            for m in rest.split(',') {
                if let Some(m) = m.split_whitespace().next() {
                    out.push((m.to_owned(), Vec::new()));
                }
            }
        }
    }
    out
}

/// A JavaScript/TypeScript file's relative module specifiers.
fn relative_imports(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for quote in ['\'', '"'] {
        for (i, _) in content.match_indices(quote) {
            let rest = &content[i + 1..];
            if !(rest.starts_with("./") || rest.starts_with("../")) {
                continue;
            }
            let before = content[..i].trim_end();
            if !(before.ends_with("from")
                || before.ends_with("import")
                || before.ends_with("require(")
                || before.ends_with("import("))
            {
                continue;
            }
            if let Some(end) = rest.find(quote) {
                out.push(rest[..end].to_owned());
            }
        }
    }
    out
}

/// What judges a task outside any story (setup, polish, convergence):
/// exactly the scenarios it names, never the author's pick of the whole
/// spec — those the stories' oracles judge. `None` when it names none.
pub(crate) fn outside_story(
    t: &speccoder_core::snapshot::Task,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    unit_dir: &str,
    judged: &dyn Fn(&str) -> bool,
) -> Option<speccoder_core::story::Judged> {
    if checks_only(t, unit_dir, judged) {
        return None;
    }
    let links: Vec<String> = t
        .scenarios
        .iter()
        .filter(|l| scenarios.iter().any(|s| &s.id == *l))
        .cloned()
        .collect();
    (!links.is_empty()).then_some(speccoder_core::story::Judged::Linked { scenarios: links })
}

/// The story whose oracle judges a task outside any story: the one owning
/// every scenario its line cites (a convergence gap), with those scenarios.
/// `None` when it cites none, or scenarios of several stories.
pub(crate) fn linked_story(
    t: &speccoder_core::snapshot::Task,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    unit_dir: &str,
    judged: &dyn Fn(&str) -> bool,
    reqs: &[crate::format::OracleRequirement],
) -> Option<(crate::format::OracleRequirement, Vec<String>)> {
    let Some(speccoder_core::story::Judged::Linked { scenarios: links }) =
        outside_story(t, scenarios, unit_dir, judged)
    else {
        return None;
    };
    let owner = |id: &String| {
        reqs.iter()
            .position(|r| r.scenarios.iter().any(|s| &s.id == id))
    };
    let owners: std::collections::BTreeSet<usize> =
        links.iter().map(owner).collect::<Option<_>>()?;
    match owners.into_iter().collect::<Vec<_>>()[..] {
        [one] => Some((reqs[one].clone(), links)),
        _ => None,
    }
}

/// Judged by the checks alone: planning work, or a step only a person
/// takes (`[YOU]`) — never a story's checkpoint.
pub(crate) fn checks_only(
    t: &speccoder_core::snapshot::Task,
    unit_dir: &str,
    judged: &dyn Fn(&str) -> bool,
) -> bool {
    t.yours
        || speccoder_core::lane::by_paths(&t.title, unit_dir, judged)
            == speccoder_core::lane::Lane::Planning
}

#[cfg(test)]
mod interface_tests {
    use super::{declarations, imagined, related};

    /// The sample's T032 (2026-11-02) cites FR-004, FR-007 and T029, whose
    /// frozen test it then contradicted: T029 is related, by both.
    #[test]
    fn tasks_on_the_same_requirements_are_related() {
        let task = |id: &str, title: &str| speccoder_core::snapshot::Task {
            id: id.to_owned(),
            title: title.to_owned(),
            ..Default::default()
        };
        let all = [
            task(
                "T026",
                "Route both tables in `src/lib.rs` per US1/AC3 (partial)",
            ),
            task(
                "T029",
                "Add header-presence validation per FR-004, FR-007 (partial)",
            ),
            task("T030", "Propagate the source path per FR-010"),
            task("T031", "Escape pipes per FR-0041 and us1/ac3"),
            task(
                "T032",
                "Add explicit header-presence validation per FR-004, FR-007, T029 (partial)",
            ),
        ];
        assert_eq!(related(&all[4], &all), ["T029"]);
        assert_eq!(related(&all[0], &all), ["T031"]);
        let named = task("T033", "Finish what T030 began");
        assert_eq!(related(&named, &all), ["T030"]);
    }

    /// The sample's T032 test (2026-11-02), against its code: the module it
    /// imagined, the class it misspelled — and what passes.
    #[test]
    fn a_test_s_imports_are_checked_against_the_code() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let w = dir.path();
        for (p, t) in [
            ("src/__init__.py", ""),
            (
                "src/models/__init__.py",
                "from .csv_output import CsvOutput\nfrom .conversion_error import ConversionError\n",
            ),
            ("src/models/csv_output.py", "class CsvOutput:\n    pass\n"),
            (
                "src/services/table_parser.py",
                "def parse_markdown_table(content):\n    return ()\n",
            ),
            ("web/src/lib/csv.ts", "export function toCsv() {}\n"),
        ] {
            std::fs::create_dir_all(w.join(p).parent().unwrap_or(w))
                .unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(w.join(p), t).unwrap_or_else(|e| panic!("{e}"));
        }
        let task = "Add header-presence validation in the conversion flow per FR-004";
        let check = |content: &str| imagined(w, "tests/test_t032.py", content, task);
        // As the sample wrote it: the module is not there.
        let got = check(
            "def test():\n    from src.lib.convert_table_to_csv import convert_table_to_csv\n    from src.models.conversion_error import ConversionError\n",
        );
        assert!(
            got.as_deref()
                .is_some_and(|m| m.contains("`src.lib.convert_table_to_csv`")),
            "{got:?}"
        );
        // A task that names the file adds it.
        assert_eq!(
            imagined(
                w,
                "tests/test_t032.py",
                "from src.lib.convert_table_to_csv import convert\n",
                "Add `src/lib/convert_table_to_csv.py`"
            ),
            None
        );
        // A name the module has only in other letters.
        let got = check("from src.models import CSVOutput\n");
        assert!(
            got.as_deref()
                .is_some_and(|m| m.contains("which has `CsvOutput`")),
            "{got:?}"
        );
        // What is there, what the task may add, and what is not the project's.
        assert_eq!(
            check(
                "import pytest\nfrom dataclasses import dataclass\nfrom src.models import (\n    CsvOutput,\n    ConversionError,\n)\nfrom src.services.table_parser import parse_markdown_table, validate_header\n"
            ),
            None
        );
        // Relative JavaScript/TypeScript imports.
        let ts = |c: &str| imagined(w, "web/src/lib/csv.test.ts", c, task);
        assert_eq!(
            ts("import { toCsv } from \"./csv\";\nimport { it } from \"vitest\";\n"),
            None
        );
        assert!(ts("import { toCsv } from './convert';\n").is_some());
        assert_eq!(ts("import { toCsv } from './csv.js';\n"), None, "NodeNext");
        // Names made at run time, and namespace packages, are never refused.
        for (p, t) in [
            ("pkg/__init__.py", "from .parser import *\n"),
            ("pkg/parser.py", "class Parser:\n    pass\n"),
            ("ns/inner.py", "def f():\n    pass\n"),
        ] {
            std::fs::create_dir_all(w.join(p).parent().unwrap_or(w))
                .unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(w.join(p), t).unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(check("from pkg import Parser\nimport ns\n"), None);
    }

    /// Rust's `iter.skip(1)` is no skipped test; `it.skip(` and
    /// `pytest.skip(` are.
    #[test]
    fn only_a_test_framework_s_skip_is_a_skip() {
        assert!(!crate::integrity::has_skip_marker(
            "let rows = lines.iter().skip(1);"
        ));
        assert!(crate::integrity::has_skip_marker(
            "it.skip('later', () => {})"
        ));
        assert!(crate::integrity::has_skip_marker(
            "    pytest.skip(\"later\")"
        ));
    }

    /// The sample's code (2026-11-02): the test author imagined a module,
    /// a class's spelling and a return shape; these lines say them.
    #[test]
    fn a_file_s_declarations_are_its_names_and_shapes() {
        let py = "from dataclasses import dataclass\n\n\n@dataclass(frozen=True)\nclass MarkdownTable:\n    header: tuple[str, ...]\n    rows: tuple[tuple[str, ...], ...]\n\n    def width(self) -> int:\n        return len(self.header)\n\n    def _check(self) -> None:\n        pass\n\n\ndef parse_markdown_table(\n    content: str,\n) -> tuple[MarkdownTable, ...]:\n    x: int = 1\n    return ()\n\n\ndef _split_row(line: str) -> tuple[str, ...]:\n    return ()\n";
        assert_eq!(
            declarations(py, false),
            "class MarkdownTable:\n    header: tuple[str, ...]\n    rows: tuple[tuple[str, ...], ...]\n    def width(self) -> int:\ndef parse_markdown_table( content: str, ) -> tuple[MarkdownTable, ...]:\n"
        );
        assert_eq!(
            declarations("from .csv_output import CsvOutput\nimport os\n", true),
            "from .csv_output import CsvOutput\n"
        );
        assert_eq!(
            declarations(
                "pub struct Table {\n    pub header: Vec<String>,\n    rows: usize,\n}\nfn private() {}\npub fn parse(s: &str) -> Vec<Table> {\n",
                false
            ),
            "pub struct Table\n    pub header: Vec<String>,\npub fn parse(s: &str) -> Vec<Table>\n"
        );
        assert_eq!(
            declarations(
                "export function toCsv(t: Table): string {\nconst x = 1;\n",
                false
            ),
            "export function toCsv(t: Table): string\n"
        );
    }
}

#[cfg(test)]
mod argv_tests {
    use super::argv_of;
    use serde_json::json;

    /// The sample's real answer (2026-10-29): the command as one string.
    #[test]
    fn a_command_given_as_one_string_is_its_words() {
        assert_eq!(
            argv_of(&json!(["cargo test --test acceptance_us1"])),
            ["cargo", "test", "--test", "acceptance_us1"]
        );
        assert_eq!(
            argv_of(&json!(["cargo", "test", "--test", "acceptance_us1"])),
            ["cargo", "test", "--test", "acceptance_us1"]
        );
        // A shell line stays one word: refused as the command it is not.
        assert_eq!(
            argv_of(&json!(["cargo test; rm -rf ."])),
            ["cargo test; rm -rf ."]
        );
        assert_eq!(argv_of(&json!(["pytest"])), ["pytest"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pre-frozen record of `producer` naming `tests/a.txt`.
    fn record(
        producer: speccoder_core::acceptance::Producer,
        content: &str,
    ) -> speccoder_core::acceptance::PreFrozenAcceptance {
        speccoder_core::acceptance::PreFrozenAcceptance {
            unit: "001-x".to_owned(),
            requirement: "US1".to_owned(),
            wording_digest: "sha256:w".to_owned(),
            approval_digest: "sha256:a".to_owned(),
            visible: vec![speccoder_core::acceptance::VisibleTest {
                path: "tests/a.txt".to_owned(),
                sha256: format!(
                    "sha256:{}",
                    crate::integrity::sha256_hex(content.as_bytes())
                ),
                cites: vec!["US1-1".to_owned()],
            }],
            held_out_sealed: None,
            producer,
            decided: speccoder_core::acceptance::Decided {
                by: "Ada".to_owned(),
                via: speccoder_core::actions::Via::Terminal,
                at: 0,
            },
        }
    }

    #[test]
    fn a_review_s_and_a_bug_s_frozen_sets_go_through_the_one_reuse_each_with_its_red_rule() {
        use speccoder_core::acceptance::Producer;
        let summary = |passed: u8, failed: u8| {
            format!(
                "echo 'test result: {}. {passed} passed; {failed} failed; 0 ignored; 0 measured; 0 filtered out'; exit {}",
                if failed > 0 { "FAILED" } else { "ok" },
                if failed > 0 { 101 } else { 0 }
            )
        };
        let content = "checks\n";
        for (producer, rule) in [
            (Producer::Review, RedRule::RunTime),
            (Producer::BugConfirmation, RedRule::BugAssertionOnly),
        ] {
            let rec = record(producer, content);
            let files = vec![("tests/a.txt".to_owned(), content.to_owned())];
            let run = |script: String| {
                let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
                let tests = [ReusedTest {
                    path: "tests/a.txt".to_owned(),
                    cites: vec!["US1-1".to_owned()],
                    argv: vec!["sh".to_owned(), "-c".to_owned(), script],
                    green: false,
                }];
                let got = reuse_pre_frozen(
                    dir.path(),
                    &rec,
                    &files,
                    &tests,
                    &sandbox::Runner::Plain,
                    rule,
                    None,
                );
                assert!(
                    dir.path().join("tests/a.txt").is_file(),
                    "written at the checkpoint"
                );
                got
            };
            // Red by one assertion, twice: reused, the source the producer's.
            let red = run(summary(0, 1)).unwrap_or_else(|e| panic!("{producer:?}: {e}"));
            assert!(red.red);
            assert_eq!(red.report["source"], serde_json::json!(producer.source()));
            // Already passing: a review's set is not demonstrated, a bug's
            // reproduction no longer reproduces — the person looks.
            let passing = run(summary(1, 0));
            match rule {
                RedRule::RunTime => {
                    let p = passing.unwrap_or_else(|e| panic!("{e}"));
                    assert!(!p.red);
                    assert!(
                        p.report["note"]
                            .as_str()
                            .is_some_and(|n| n.contains("already passing"))
                    );
                }
                RedRule::BugAssertionOnly => assert!(passing.is_err()),
            }
            // A failure of more than one test is a bug's no reproduction.
            if rule == RedRule::BugAssertionOnly {
                assert!(run(summary(1, 1)).is_err(), "one test's assertion only");
            }
        }
    }
}

/// How a reused set must be red at the checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RedRule {
    /// A review's oracle: the run-time rule (an assertion, or an absent
    /// symbol), twice; a set that is no longer red is reported as at run
    /// time — not demonstrated — rather than refused.
    RunTime,
    /// A bug's reproductions: an assertion only, both runs.
    BugAssertionOnly,
}

/// One test of a reused set: its file, what it cites and how it runs.
#[derive(Debug)]
pub(crate) struct ReusedTest {
    /// Project-relative path.
    pub(crate) path: String,
    /// The statement or scenario ids.
    pub(crate) cites: Vec<String>,
    /// The command that runs only it.
    pub(crate) argv: Vec<String>,
    /// It must pass at the checkpoint instead (an *Unchanged* test).
    pub(crate) green: bool,
}

/// Where a reused set's sealed held-out tests are red-checked: overlaid on
/// the checkpoint, run and removed.
pub(crate) struct SealedCheck<'a> {
    /// The project's state directory.
    pub(crate) state: &'a Path,
    /// The unit the set is sealed under.
    pub(crate) unit: &'a str,
    /// The worktree's root.
    pub(crate) wt_root: &'a Path,
    /// Scratch for checkpoints.
    pub(crate) tmp: &'a Path,
    /// The task's policy.
    pub(crate) policy: &'a tools::policy::Policy,
}

/// Acceptance tests decided before the run, reused at the checkpoint:
/// written into the worktree (the one function every pre-frozen producer
/// uses), each red twice by `rule` — or green, for a test that guards what
/// must not change — with no acceptance author call. `Err`: why the set
/// cannot stand (the person looks).
pub(crate) fn reuse_pre_frozen(
    project: &Path,
    record: &speccoder_core::acceptance::PreFrozenAcceptance,
    files: &[(String, String)],
    tests: &[ReusedTest],
    runner: &sandbox::Runner,
    rule: RedRule,
    sealed: Option<&SealedCheck<'_>>,
) -> Result<PreparedAcceptance, String> {
    use serde_json::json;
    for (rel, content) in files {
        let full = project.join(rel);
        if let Ok(now) = std::fs::read_to_string(&full) {
            if now != *content {
                return Err(format!(
                    "`{rel}` exists and differs from the confirmed test; it is not overwritten"
                ));
            }
            continue;
        }
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&full, content).map_err(|e| format!("`{rel}`: {e}"))?;
    }
    // A bug's reproduction is red only by its own test: a run of more than
    // one test cannot say whose assertion failed.
    let class_of = |r: &speccoder_core::verify::CheckResult,
                    s: Option<&speccoder_core::verify::RunnerSummary>| match rule
    {
        RedRule::BugAssertionOnly => speccoder_core::verify::classify_red_one(r, s, ""),
        RedRule::RunTime => classify_red(r, s),
    };
    let mut classes = Vec::new();
    for t in tests {
        let (first, summary) = execute(project, &t.argv, runner);
        let one = class_of(&first, summary.as_ref());
        let class = if t.green {
            one
        } else {
            match rule {
                RedRule::BugAssertionOnly if one == RedClass::Assertion => {
                    let (second, summary) = execute(project, &t.argv, runner);
                    speccoder_core::bug::bug_class(one, Some(class_of(&second, summary.as_ref())))
                }
                RedRule::BugAssertionOnly => one,
                RedRule::RunTime if one.is_red() => {
                    let (second, summary) = execute(project, &t.argv, runner);
                    if classify_red(&second, summary.as_ref()).is_red() {
                        one
                    } else {
                        RedClass::Flaky
                    }
                }
                RedRule::RunTime => one,
            }
        };
        let holds = if t.green {
            class == RedClass::Passed
        } else {
            match rule {
                RedRule::BugAssertionOnly => class == RedClass::Assertion,
                RedRule::RunTime => class.is_red(),
            }
        };
        if !holds && rule == RedRule::RunTime && !t.green {
            return Ok(not_red_frozen(record, files, tests, t, class));
        }
        if !holds {
            return Err(if t.green {
                format!(
                    "the confirmed test `{}` ({}) no longer passes on the branch ({})",
                    t.path,
                    t.cites.join(", "),
                    class_word(class)
                )
            } else {
                format!(
                    "the confirmed test `{}` ({}) no longer fails on an assertion ({}): the behaviour may have changed on the branch",
                    t.path,
                    t.cites.join(", "),
                    class_word(class)
                )
            });
        }
        classes.push(class);
    }
    let red: Vec<&ReusedTest> = tests.iter().filter(|t| !t.green).collect();
    let Some(first) = red.first() else {
        return Err("the confirmed set has no test that must fail first".to_owned());
    };
    let more: Vec<Vec<String>> = tests
        .iter()
        .filter(|t| !std::ptr::eq(*t, *first))
        .map(|t| t.argv.clone())
        .collect();
    let mut held_sha = record
        .held_out_sealed
        .as_ref()
        .map(|h| h.manifest_sha256.clone());
    let mut held_out = match &record.held_out_sealed {
        Some(h) => {
            json!({ "state": "on", "manifest": format!("sha256:{}", h.manifest_sha256), "kept": h.count })
        }
        None => json!({ "state": "absent", "reason": "none_kept" }),
    };
    // The sealed held-out set, red on the same checkpoint (overlaid, run,
    // removed): a set that is not red decides nothing.
    if let (Some(check), Some(h)) = (sealed, &record.held_out_sealed) {
        let (manifest, contents) = crate::held_out::open_sealed(check.state, check.unit, h)
            .map_err(|t| format!("the frozen held-out set was tampered with: {}", t.0))?;
        let set: Vec<(crate::held_out::HeldOutFile, String)> = manifest
            .files
            .iter()
            .cloned()
            .zip(contents.into_iter().map(|(_, c)| c))
            .collect();
        let runs = crate::held_out::overlay_red(
            check.wt_root,
            project,
            check.tmp,
            &set,
            runner,
            check.policy,
        )?;
        let all_red = runs
            .iter()
            .all(|r| r.result.is_some() && crate::held_out::red_class(r).is_red());
        if !all_red {
            held_sha = None;
            held_out = json!({ "state": "absent", "reason": "not_red" });
        }
    }
    let class = classes.first().copied().unwrap_or(RedClass::Assertion);
    Ok(PreparedAcceptance {
        report: json!({
            "files": files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            "tests": tests.iter().map(|t| json!({ "path": t.path, "scenarios": t.cites })).collect::<Vec<_>>(),
            "argv": first.argv,
            "red": true,
            "red_class": class,
            "source": record.producer.source(),
            "pre_frozen": {
                "manifest": crate::pre_frozen::manifest_digest(record),
                "approval": record.approval_digest,
                "wording": record.wording_digest,
                "by": record.decided.by,
                "via": record.decided.via,
            },
            "held_out": held_out,
        }),
        argv: Some(first.argv.clone()),
        more_argv: more,
        red: true,
        red_class: Some(class),
        held_out: held_sha,
        generation_failed: None,
        skip_cause: None,
    })
}

/// A frozen set no longer red at the checkpoint: reported as a run-time
/// set that is not red — not demonstrated — with nothing re-authored.
fn not_red_frozen(
    record: &speccoder_core::acceptance::PreFrozenAcceptance,
    files: &[(String, String)],
    tests: &[ReusedTest],
    failing: &ReusedTest,
    class: RedClass,
) -> PreparedAcceptance {
    use serde_json::json;
    let note = match class {
        RedClass::Passed => "frozen at review, already passing: the acceptance tests already pass before the attempt, so they are no evidence for it".to_owned(),
        RedClass::Flaky => "frozen at review: the acceptance tests failed once and passed when run again: not demonstrated".to_owned(),
        other => format!(
            "frozen at review: the acceptance tests did not fail on an assertion ({}): not demonstrated",
            class_word(other)
        ),
    };
    PreparedAcceptance {
        report: json!({
            "files": files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            "tests": tests.iter().map(|t| json!({ "path": t.path, "scenarios": t.cites })).collect::<Vec<_>>(),
            "argv": failing.argv,
            "red": false,
            "red_class": class,
            "note": note,
            "source": record.producer.source(),
            "pre_frozen": {
                "manifest": crate::pre_frozen::manifest_digest(record),
                "approval": record.approval_digest,
                "wording": record.wording_digest,
                "by": record.decided.by,
                "via": record.decided.via,
            },
            "held_out": { "state": "absent", "reason": "not_red" },
        }),
        argv: Some(failing.argv.clone()),
        more_argv: tests
            .iter()
            .filter(|t| !std::ptr::eq(*t, failing))
            .map(|t| t.argv.clone())
            .collect(),
        red: false,
        red_class: Some(class),
        held_out: None,
        generation_failed: None,
        skip_cause: None,
    }
}
