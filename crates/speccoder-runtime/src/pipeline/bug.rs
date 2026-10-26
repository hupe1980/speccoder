//! A bug task inside the pipeline. Before the attempt: reproduce — one
//! test per *Expected* statement, red on the untouched code by an assertion
//! twice, held-out variants sealed — and the person's one confirmation;
//! the confirmed reproductions are then reused at the checkpoint as the
//! visible acceptance set. After an attempt that passed: every reproduction
//! green, the pass-to-pass set green by name, the revert rejected, and each
//! special case rejected (D-111). The rules are `speccoder_core::bug`'s.

use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};
use speccoder_core::bug::{
    self as rules, Basis, BugReport, Final, HeldOutVariant, Outcome, PassToPass, Reproduce,
    ReproduceTry, Reproduction, Revert, RevertResult, Source, SpecialCase, Stage as BugStage,
    TryStatement,
};
use speccoder_core::context::ContextKind;
use speccoder_core::stage::{Stage, StageState};
use speccoder_core::verify::{
    CheckStatus, HeldOut, RedClass, Verdict, VerifyReport, classify_red, classify_red_one,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::{AcceptanceSetup, PreparedAcceptance, ReusedTest, bare_report, class_word, stage};
use crate::approvals::{Approver, ConfirmAnswer, ConfirmRequest, ShownReproduction};
use crate::bug::{confirm, handoff, state};
use crate::integrity::sha256_hex;
use crate::progress::{BugEvent, Event, Progress};
use crate::tools::policy::{Decision, Policy};
use crate::{held_out, sandbox, substrate, tools, worktree};

/// What a bug task's stages need from the run.
pub(crate) struct Ctx<'a> {
    pub(crate) rt: &'a tokio::runtime::Runtime,
    pub(crate) plane: &'a substrate::Plane,
    pub(crate) project: &'a Path,
    pub(crate) wt_root: &'a Path,
    pub(crate) wt_path: &'a str,
    pub(crate) tmp: &'a Path,
    pub(crate) policy: &'a Policy,
    pub(crate) runner: &'a sandbox::Runner,
    /// The project's state directory.
    pub(crate) state: &'a Path,
    pub(crate) feature: &'a str,
    pub(crate) task: &'a str,
    pub(crate) bundle: &'a Value,
    pub(crate) instructions: &'a [crate::instructions::Instruction],
    pub(crate) live: &'a dyn Progress,
    pub(crate) approver: Option<&'a dyn Approver>,
    /// `[acceptance] held_out`.
    pub(crate) held_out: bool,
    /// `[bug] reproduce_tries`.
    pub(crate) max_tries: u32,
    /// `[bug] test`: the command that runs one test of a file.
    pub(crate) bug_test: Option<&'a [String]>,
    /// The tree the task started from.
    pub(crate) start: &'a worktree::Checkpoint,
}

/// A confirmed bug, ready for its fix.
pub(crate) struct Fix {
    pub(crate) bug: BugReport,
    pub(crate) record: speccoder_core::acceptance::PreFrozenAcceptance,
    pub(crate) files: Vec<(String, String)>,
}

/// Where a bug task is.
pub(crate) enum Phase {
    /// It needs reproductions (again).
    Reproduce(Box<BugReport>),
    /// Its reproductions are confirmed.
    Fix(Box<Fix>),
}

fn event(live: &dyn Progress, task: &str, step: BugEvent) {
    live.event(&Event::Bug {
        task: task.to_owned(),
        step,
    });
}

/// The triad as the bug's home holds it now (in `project`).
pub(crate) fn triad_now(
    format: &dyn crate::format::Adapter,
    project: &Path,
    bug: &BugReport,
) -> Result<rules::Triad, crate::format::BugRefusal> {
    let key = if bug.home.kind.native() {
        bug.task.trim_start_matches("bug:").to_owned()
    } else {
        bug.requirement.clone()
    };
    let read = format.read_bug(project, &bug.home, &key)?;
    let current = read.current.first().cloned().unwrap_or_default();
    rules::Triad::new(&current, &read.expected, &read.unchanged, sha256_hex)
        .map_err(|e| crate::format::BugRefusal::new(e.code(), e.message()))
}

/// Whether `feature`'s `task` is a bug, and where it is. `Err`: its
/// confirmed set was tampered with, or its triad cannot be read.
pub(crate) fn phase(
    plane: &Path,
    feature: &str,
    task: &str,
    project: &Path,
    format: &dyn crate::format::Adapter,
) -> Result<Option<Phase>, String> {
    let Some(mut bug) = state::load(plane, feature, task) else {
        return Ok(None);
    };
    let now =
        triad_now(format, project, &bug).map_err(|e| format!("{} ({})", e.message, e.code))?;
    if now.digest != bug.triad.digest {
        // Reworded: the confirmation and the freeze lapse.
        confirm::lapse(plane, &mut bug, now)?;
        return Ok(Some(Phase::Reproduce(Box::new(bug))));
    }
    if bug
        .confirmation
        .as_ref()
        .is_some_and(|c| c.current(&bug.triad.digest))
    {
        match crate::pre_frozen::load(plane, &bug.unit, &bug.requirement) {
            Ok(Some((record, files)))
                if speccoder_core::acceptance::current(&record, &bug.triad.digest) =>
            {
                return Ok(Some(Phase::Fix(Box::new(Fix { bug, record, files }))));
            }
            Ok(_) => {}
            Err(t) => return Err(t.0),
        }
    }
    // A finished stop is tried again with a fresh budget.
    if matches!(
        bug.outcome,
        Outcome::CannotReproduce | Outcome::AlreadySatisfied | Outcome::UnchangedNotHolding
    ) {
        bug.tries.clear();
        bug.rejections.clear();
    }
    Ok(Some(Phase::Reproduce(Box::new(bug))))
}

/// The statements as scenario references: what held-out failures name.
pub(crate) fn scenarios(bug: &BugReport) -> Vec<speccoder_core::spec::ScenarioRef> {
    bug.triad
        .expected
        .iter()
        .map(|s| speccoder_core::spec::ScenarioRef {
            id: s.id.clone(),
            text: s.text.clone(),
        })
        .collect()
}

/// The run's stop for `bug` in `outcome`.
fn stop(
    c: &Ctx<'_>,
    mut bug: BugReport,
    outcome: Outcome,
    code: Option<&str>,
    message: String,
    began: Instant,
) -> CommandResult {
    let _ = worktree::restore(c.wt_root, c.tmp, c.start);
    bug.outcome = outcome;
    bug.stage = if outcome == Outcome::AwaitingConfirmation {
        BugStage::Confirm
    } else {
        BugStage::Done
    };
    bug.code = code.map(str::to_owned);
    if matches!(
        outcome,
        Outcome::CannotReproduce | Outcome::AlreadySatisfied | Outcome::UnchangedNotHolding
    ) {
        let cost = bug
            .tries
            .iter()
            .filter_map(|t| t.usage.as_ref())
            .fold(None, |acc: Option<Value>, u| {
                super::add_usage(acc.as_ref(), Some(u))
            });
        bug.handoff = Some(handoff::compose(
            &bug,
            outcome,
            u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
            cost,
            None,
        ));
    }
    let next = handoff::next(&bug, outcome);
    let _ = state::save(c.state, &bug);
    event(
        c.live,
        c.task,
        BugEvent::Outcome {
            outcome: outcome.as_str().to_owned(),
            verdict: None,
            code: bug.code.clone(),
            next: next.iter().map(|n| n.command.clone()).collect(),
        },
    );
    let status = match outcome {
        Outcome::AlreadySatisfied => CommandStatus::Ok,
        _ => CommandStatus::NeedsHuman,
    };
    let mut report = bare_report(c.feature, c.task, c.wt_path, outcome.as_str());
    report.reason = Some(message.clone());
    report.bug = Some(Box::new(bug));
    c.live.event(&super::finished_event(
        c.task,
        &report,
        outcome.as_str(),
        false,
        message.clone(),
    ));
    let mut r = CommandResult::new("run", status, &message)
        .with_next(next)
        .with_data(CommandData::Run(Box::new(report)));
    if let Some(code) = code {
        r = r.with_code(code);
    }
    r
}

/// One authored test of a try.
struct Authored {
    path: String,
    content: String,
    statement: String,
    test: String,
    argv: Vec<String>,
}

/// Whether `rel` is a file that configures how tests are collected or run
/// (a `conftest.py`, a runner's configuration): frozen as a reproduction, it
/// would change every later test run, pass-to-pass and checks included.
fn configures_collection(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    matches!(
        name,
        "conftest.py"
            | "__init__.py"
            | "pytest.ini"
            | "tox.ini"
            | "setup.cfg"
            | "pyproject.toml"
            | "package.json"
            | "Cargo.toml"
            | "build.rs"
            | "go.mod"
    ) || [
        "jest.config",
        "jest.setup",
        "vitest.config",
        "vitest.setup",
        "vite.config",
        "vitest.workspace",
        "karma.conf",
        ".mocharc",
        "babel.config",
        ".babelrc",
    ]
    .iter()
    .any(|p| name.starts_with(p))
}

/// The author's files, checked: each a new test file, one per wanted
/// statement, naming its test. The command is speccoder's, never the
/// author's: the one that runs only that test (`[bug] test` or the
/// toolchain's rule), and the policy must allow it. `Err`: why the try is
/// discarded.
fn validate(
    answer: &Value,
    want: &[String],
    project: &Path,
    policy: &Policy,
    template: Option<&[String]>,
) -> Result<Vec<Authored>, String> {
    let protected = policy.protected();
    let mut out: Vec<Authored> = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default();
        let rel = tools::paths::normalise(path).map_err(|e| format!("`{path}`: {}", e.0))?;
        if !tools::paths::is_test_path(&rel) || protected.write_class(&rel).is_some() {
            return Err(format!("wrote outside tests (`{rel}`)"));
        }
        if configures_collection(&rel) {
            return Err(format!(
                "`{rel}` configures how tests run: a reproduction is a test file"
            ));
        }
        if project.join(&rel).exists() || out.iter().any(|a| a.path == rel) {
            return Err(format!("`{rel}` already exists"));
        }
        let statement = f["statement"].as_str().unwrap_or_default().to_owned();
        if !want.contains(&statement) || out.iter().any(|a| a.statement == statement) {
            return Err(format!(
                "`{rel}` cites `{statement}`: one test per statement asked for"
            ));
        }
        let test = f["test"].as_str().unwrap_or_default().trim().to_owned();
        if test.is_empty() {
            return Err(format!("`{rel}` names no test"));
        }
        // Only that test runs: a command that runs others could be red by
        // an unrelated failure.
        let argv = crate::bug::named::argv(project, &rel, &test, template).ok_or_else(|| {
            format!(
                "speccoder has no rule to run only `{test}` of `{rel}`; set `[bug] test` in speccoder.toml"
            )
        })?;
        if policy.classify("run_command", &json!({ "argv": argv }), None) != Decision::Allow {
            return Err(format!("`{}` is not an allowed command", argv.join(" ")));
        }
        out.push(Authored {
            path: rel,
            content: f["content"].as_str().unwrap_or_default().to_owned(),
            statement,
            test,
            argv,
        });
    }
    if let Some(missing) = want
        .iter()
        .find(|w| !out.iter().any(|a| a.statement == **w))
    {
        return Err(format!("no test for {missing}"));
    }
    Ok(out)
}

/// The class and excerpt of one reproduction on the tree as it is: red
/// only by an assertion of `test` itself, twice; an *Unchanged* test runs
/// once and must pass.
fn baseline_class(
    c: &Ctx<'_>,
    argv: &[String],
    test: &str,
    reproduces: bool,
) -> (Vec<RedClass>, String) {
    let (first, summary) = super::acceptance_run(c.project, argv, c.runner);
    let one = classify_red_one(&first, summary.as_ref(), test);
    let excerpt = handoff::cut(&format!("{}\n{}", first.stdout_tail, first.stderr_tail), 20);
    if !reproduces || one != RedClass::Assertion {
        return (vec![one], excerpt);
    }
    let (second, summary) = super::acceptance_run(c.project, argv, c.runner);
    let two = classify_red_one(&second, summary.as_ref(), test);
    (vec![one, rules::bug_class(one, Some(two))], excerpt)
}

/// The context manifest of an author call: the triad (the person's), the
/// plan and constitution, the task, rejections and the issue.
fn manifest(
    c: &Ctx<'_>,
    bug: &BugReport,
    issue: Option<&state::IssueText>,
) -> speccoder_core::context::ContextManifest {
    let mut m = crate::context::Manifest::new();
    m.push(
        ContextKind::Story,
        "person",
        "/triad",
        bug.triad.canonical().as_bytes(),
    )
    .push(
        ContextKind::Constitution,
        "worktree",
        "/bundle/constitution",
        c.bundle["constitution"]
            .as_str()
            .unwrap_or_default()
            .as_bytes(),
    )
    .push(
        ContextKind::Plan,
        "worktree",
        "/bundle/plan",
        c.bundle["plan"].as_str().unwrap_or_default().as_bytes(),
    )
    .push(
        ContextKind::Task,
        "worktree",
        "/bundle/task",
        c.bundle["task"].to_string().as_bytes(),
    );
    for (i, r) in bug.rejections.iter().enumerate() {
        m.push(
            ContextKind::Rejection,
            "person",
            &format!("/rejections/{i}"),
            r.reason.as_bytes(),
        );
    }
    if let (Some(i), Some(issue)) = (bug.issue.as_ref(), issue) {
        m.push(
            ContextKind::Issue,
            &format!("github:{}#{}", i.repo.clone().unwrap_or_default(), i.number),
            &format!("#{}", i.number),
            format!("{}\n{}", issue.title, issue.body).as_bytes(),
        );
    }
    m.finish()
}

/// The project's files and manifest, as an author is given them.
fn project_view(c: &Ctx<'_>) -> (Vec<String>, String) {
    let setup = AcceptanceSetup {
        rt: c.rt,
        plane: c.plane,
        project: c.project,
        worktree: c.wt_root,
        tmp: c.tmp,
        policy: c.policy,
        feature: c.feature,
        task: c.task,
        bundle: c.bundle.clone(),
        scenarios: &[],
        instructions: c.instructions,
        runner: c.runner,
    };
    let files: Vec<String> = worktree::checkpoint(setup.worktree, setup.tmp)
        .and_then(|cp| worktree::files(setup.worktree, &cp))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = super::MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(c.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    (files, manifest)
}

/// One author call; `(answer, run id, usage, notes)`.
fn author(
    c: &Ctx<'_>,
    input: agentplane::core::Tainted<Value>,
) -> Result<(Value, String, Option<Value>, Option<String>), String> {
    let outcome =
        c.rt.block_on(c.plane.acceptance(c.feature, c.task, input))
            .map_err(|e| format!("the author call failed: {}", e.0))?;
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return Err(format!(
            "the author did not answer ({})",
            outcome.status.as_str()
        ));
    }
    let notes = answer["notes"].as_array().map(|n| {
        n.iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    });
    Ok((
        answer,
        outcome.run_id.to_string(),
        Some(output["usage"].clone()).filter(|u| !u.is_null()),
        notes,
    ))
}

/// Reproduce: tries until every *Expected* statement is red by an
/// assertion (or the tries run out), the held-out variants sealed, then the
/// confirmation. `Ok`: confirmed (inline, or by naming the tests) — the fix
/// runs now; `Err`: the run stops with the bug's outcome.
#[allow(clippy::too_many_lines)] // one linear procedure, in reading order
pub(crate) fn reproduce(c: &Ctx<'_>, mut bug: BugReport) -> Result<Fix, Box<CommandResult>> {
    let began = Instant::now();
    stage(
        c.live,
        c.task,
        Stage::Reproduce,
        StageState::Started,
        None,
        None,
    );
    bug.stage = BugStage::Reproduce;
    bug.max_tries = c.max_tries;
    let extra = state::extra(c.state, &bug.unit, &bug.task);
    let expected = bug.expected_ids();
    let unchanged: Vec<String> = bug.triad.unchanged.iter().map(|s| s.id.clone()).collect();
    let person: Vec<Reproduction> = bug
        .reproductions
        .iter()
        .filter(|r| r.source == Source::Person)
        .cloned()
        .collect();
    // The person's tests stand for the Expected statements; the author
    // writes the rest.
    let want: Vec<String> = if person.is_empty() {
        expected.iter().chain(&unchanged).cloned().collect()
    } else {
        unchanged.clone()
    };
    let statements = json!(bug.triad.statements().collect::<Vec<_>>());
    let person_files = state::repro(
        c.state,
        &bug.unit,
        &bug.task,
        &person.iter().map(|r| r.file.clone()).collect::<Vec<_>>(),
    );
    let stop_now = |bug: BugReport, outcome, code, message: String| {
        stage(
            c.live,
            c.task,
            Stage::Reproduce,
            if outcome == Outcome::AlreadySatisfied {
                StageState::Passed
            } else {
                StageState::Failed
            },
            Some(outcome_word(outcome)),
            Some(began),
        );
        Box::new(stop(c, bug, outcome, code, message, began))
    };
    let kept: Vec<(Reproduction, String)> = loop {
        // Every try counts, a rejected one too: once they are used up the
        // author is not called again.
        if u32::try_from(bug.tries.len()).unwrap_or(u32::MAX) >= c.max_tries.max(1) {
            let saved = state::save(c.state, &bug);
            match rules::decide_reproduce(&bug.tries, &expected, c.max_tries) {
                // Still waiting for the person: the same reproductions.
                Reproduce::Reproduced { .. } if bug.confirm_digest.is_some() && saved.is_ok() => {
                    let digest = bug.confirm_digest.clone().unwrap_or_default();
                    let message = format!(
                        "the reproductions wait for your confirmation (digest {digest}): speccoder bug confirm {} --digest {digest}",
                        bug.key()
                    );
                    return Err(Box::new(stop(
                        c,
                        bug,
                        Outcome::AwaitingConfirmation,
                        None,
                        message,
                        began,
                    )));
                }
                _ => {
                    let message = format!(
                        "cannot reproduce: the {} tries are used up ({} rejected); nothing is fixed",
                        bug.tries.len(),
                        bug.tries
                            .iter()
                            .filter(|t| t
                                .discarded
                                .as_deref()
                                .is_some_and(|d| d.starts_with("rejected")))
                            .count()
                    );
                    return Err(stop_now(
                        bug,
                        Outcome::CannotReproduce,
                        Some("BUG-CANNOT-REPRODUCE"),
                        message,
                    ));
                }
            }
        }
        let n = u32::try_from(bug.tries.len()).unwrap_or(u32::MAX) + 1;
        let context = manifest(c, &bug, extra.issue.as_ref());
        let rejections: Vec<String> = bug.rejections.iter().map(|r| r.reason.clone()).collect();
        let mut t = ReproduceTry {
            n,
            per_statement: Vec::new(),
            discarded: None,
            run_id: None,
            usage: None,
            rejection: rejections.last().cloned(),
        };
        let mut this: Vec<(Reproduction, String)> = Vec::new();
        // The person's tests: run again on this tree.
        for (r, (_, content)) in person.iter().zip(&person_files) {
            let full = c.project.join(&r.file);
            if !full.exists() {
                if let Some(parent) = full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&full, content);
            }
            let (classes, excerpt) = baseline_class(c, &r.argv, &r.test, true);
            let class = classes.last().copied().unwrap_or(RedClass::Unclassified);
            t.per_statement.push(TryStatement {
                statement: r.statement.clone(),
                class,
                excerpt: (class != RedClass::Assertion).then_some(excerpt),
            });
            let mut r = r.clone();
            r.baseline = classes;
            this.push((r, content.clone()));
        }
        if !want.is_empty() {
            let (files, manifest_text) = project_view(c);
            let input =
                substrate::acceptance::reproduce_input(&substrate::acceptance::ReproduceParts {
                    set: "reproduce",
                    bundle: c.bundle.clone(),
                    statements: statements.clone(),
                    want: &want,
                    files: &files,
                    manifest: &manifest_text,
                    visible: &[],
                    rejections: &rejections,
                    issue: extra
                        .issue
                        .as_ref()
                        .map(|i| (i.title.as_str(), i.body.as_str())),
                    instructions: c.instructions,
                    context: &context,
                });
            match author(c, input) {
                Err(why) => t.discarded = Some(why),
                Ok((answer, run_id, usage, _notes)) => {
                    t.run_id = Some(run_id);
                    t.usage = usage;
                    match validate(&answer, &want, c.project, c.policy, c.bug_test) {
                        Err(why) => t.discarded = Some(why),
                        Ok(files) => {
                            for f in files {
                                let full = c.project.join(&f.path);
                                if let Some(parent) = full.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                if std::fs::write(&full, &f.content).is_err() {
                                    t.discarded =
                                        Some(format!("`{}` could not be written", f.path));
                                    break;
                                }
                                let reproduces = f.statement.starts_with('E');
                                let (classes, excerpt) =
                                    baseline_class(c, &f.argv, &f.test, reproduces);
                                let class =
                                    classes.last().copied().unwrap_or(RedClass::Unclassified);
                                let red_ok = if reproduces {
                                    class == RedClass::Assertion
                                } else {
                                    class == RedClass::Passed
                                };
                                t.per_statement.push(TryStatement {
                                    statement: f.statement.clone(),
                                    class,
                                    excerpt: (!red_ok).then_some(excerpt),
                                });
                                this.push((
                                    Reproduction {
                                        statement: f.statement,
                                        file: f.path,
                                        test: f.test,
                                        source: Source::Author,
                                        baseline: classes,
                                        digest: format!(
                                            "sha256:{}",
                                            sha256_hex(f.content.as_bytes())
                                        ),
                                        argv: f.argv,
                                        final_: Final::NotRun,
                                        revert: Revert::NotRun,
                                    },
                                    f.content,
                                ));
                            }
                        }
                    }
                }
            }
        }
        if t.discarded.is_some() {
            t.per_statement.clear();
        }
        // Nothing of a try stays in the tree.
        let _ = worktree::restore(c.wt_root, c.tmp, c.start);
        event(
            c.live,
            c.task,
            BugEvent::ReproduceTry {
                n,
                of: c.max_tries,
                per_statement: t
                    .per_statement
                    .iter()
                    .map(|s| crate::progress::StatementClass {
                        statement: s.statement.clone(),
                        class: class_word(s.class),
                    })
                    .collect(),
            },
        );
        bug.tries.push(t);
        if let Err(e) = state::save(c.state, &bug) {
            return Err(Box::new(stop(
                c,
                bug,
                Outcome::Recorded,
                None,
                format!("the try could not be kept: {e}"),
                began,
            )));
        }
        match rules::decide_reproduce(&bug.tries, &expected, c.max_tries) {
            Reproduce::Reproduced { .. } => {
                let last = bug
                    .tries
                    .last()
                    .map(|t| t.per_statement.clone())
                    .unwrap_or_default();
                if let Some(u) = last
                    .iter()
                    .find(|s| s.statement.starts_with('U') && s.class != RedClass::Passed)
                {
                    let words = bug
                        .triad
                        .statement(&u.statement)
                        .map(|s| s.text.clone())
                        .unwrap_or_default();
                    return Err(stop_now(
                        bug,
                        Outcome::UnchangedNotHolding,
                        Some("BUG-UNCHANGED-NOT-HOLDING"),
                        format!(
                            "{} does not hold today: its test fails on the untouched code ({}) — \"{words}\"; nothing is fixed",
                            u.statement,
                            class_word(u.class)
                        ),
                    ));
                }
                break this;
            }
            Reproduce::Retry => {}
            Reproduce::AlreadySatisfied => {
                bug.reproductions = person.clone();
                bug.reproductions
                    .extend(this.iter().map(|(r, _)| r.clone()));
                let files: Vec<(String, String)> = this
                    .iter()
                    .map(|(r, c)| (r.file.clone(), c.clone()))
                    .collect();
                let _ = state::save_repro(c.state, &bug.unit, &bug.task, &files);
                return Err(stop_now(
                    bug,
                    Outcome::AlreadySatisfied,
                    Some("BUG-ALREADY-SATISFIED"),
                    format!(
                        "every reproduction passed on the untouched code on each of {} tries: nothing to fix, nothing ticked",
                        c.max_tries
                    ),
                ));
            }
            Reproduce::CannotReproduce {
                reproduced,
                satisfied,
                not_demonstrated,
            } => {
                let mut said = vec![format!("after {} tries", bug.tries.len())];
                if !reproduced.is_empty() {
                    said.push(format!("reproduced {}", reproduced.join(", ")));
                }
                if !satisfied.is_empty() {
                    said.push(format!(
                        "{} already pass (narrow the triad)",
                        satisfied.join(", ")
                    ));
                }
                if !not_demonstrated.is_empty() {
                    said.push(format!("not reproduced {}", not_demonstrated.join(", ")));
                }
                return Err(stop_now(
                    bug,
                    Outcome::CannotReproduce,
                    Some("BUG-CANNOT-REPRODUCE"),
                    format!("cannot reproduce: {}; nothing is fixed", said.join("; ")),
                ));
            }
        }
    };
    stage(
        c.live,
        c.task,
        Stage::Reproduce,
        StageState::Passed,
        Some("assertion".to_owned()),
        Some(began),
    );

    // Held-out variants: other inputs, the same statements; sealed.
    bug.held_out.clear();
    bug.held_out_sealed = None;
    bug.held_out_reason = None;
    if c.held_out {
        let visible: Vec<(String, String)> = kept
            .iter()
            .filter(|(r, _)| r.reproduces())
            .map(|(r, content)| (r.file.clone(), content.clone()))
            .collect();
        match seal_variants(c, &mut bug, &visible, &expected, &[]) {
            Ok(sealed) => bug.held_out_sealed = Some(sealed),
            Err(why) => bug.held_out_reason = Some(why),
        }
    }
    bug.reproductions = kept.iter().map(|(r, _)| r.clone()).collect();
    let files: Vec<(String, String)> = kept
        .iter()
        .map(|(r, c)| (r.file.clone(), c.clone()))
        .collect();
    if let Err(e) = state::save_repro(c.state, &bug.unit, &bug.task, &files) {
        return Err(Box::new(stop(
            c,
            bug,
            Outcome::Recorded,
            None,
            format!("the reproductions could not be kept: {e}"),
            began,
        )));
    }
    let digest = confirm::digest(&bug.reproductions);
    bug.confirm_digest = Some(digest.clone());
    event(
        c.live,
        c.task,
        BugEvent::Reproduced {
            reproductions: bug
                .reproductions
                .iter()
                .map(|r| crate::progress::ShownReproduction {
                    statement: r.statement.clone(),
                    file: r.file.clone(),
                    test: r.test.clone(),
                    digest: r.digest.clone(),
                })
                .collect(),
        },
    );
    let _ = state::save(c.state, &bug);

    // The person named the tests: that is the confirmation.
    if !person.is_empty() {
        let by = extra
            .by
            .clone()
            .unwrap_or_else(|| crate::approvals::who_in(c.project));
        let via = extra.via.unwrap_or(speccoder_core::actions::Via::Terminal);
        return freeze(c, bug, &by, via, Basis::NamedTest, began);
    }
    stage(
        c.live,
        c.task,
        Stage::Confirm,
        StageState::Started,
        None,
        None,
    );
    let request = ConfirmRequest {
        bug: bug.key(),
        title: bug.title.clone(),
        digest: digest.clone(),
        reproductions: bug
            .reproductions
            .iter()
            .zip(&files)
            .map(|(r, (_, content))| ShownReproduction {
                statement: r.statement.clone(),
                words: bug
                    .triad
                    .statement(&r.statement)
                    .map(|s| s.text.clone())
                    .unwrap_or_default(),
                file: r.file.clone(),
                test: r.test.clone(),
                argv: r.argv.clone(),
                excerpt: bug
                    .tries
                    .last()
                    .and_then(|t| t.per_statement.iter().find(|s| s.statement == r.statement))
                    .and_then(|s| s.excerpt.clone())
                    .unwrap_or_default(),
                content: content.clone(),
            })
            .collect(),
        warning: crate::bug::held_out_warning(&bug),
    };
    match c
        .approver
        .and_then(|a| a.confirm(&request).map(|ans| (ans, a.via())))
    {
        Some((ConfirmAnswer::Confirm, via)) => {
            let by = crate::approvals::who_in(c.project);
            freeze(c, bug, &by, via, Basis::Shown, began)
        }
        Some((ConfirmAnswer::Reject(reason), via)) => {
            if let Err(e) = reject(c.state, &mut bug, &reason, via) {
                return Err(Box::new(stop(
                    c,
                    bug,
                    Outcome::Recorded,
                    None,
                    format!("the rejection could not be kept: {e}"),
                    began,
                )));
            }
            event(c.live, c.task, BugEvent::ConfirmRejected { digest, reason });
            stage(
                c.live,
                c.task,
                Stage::Confirm,
                StageState::Failed,
                Some("rejected".to_owned()),
                None,
            );
            if u32::try_from(bug.tries.len()).unwrap_or(u32::MAX) >= c.max_tries {
                let message = format!(
                    "cannot reproduce: the reproductions of {} tries were not accepted; nothing is fixed",
                    bug.tries.len()
                );
                return Err(stop_now(
                    bug,
                    Outcome::CannotReproduce,
                    Some("BUG-CANNOT-REPRODUCE"),
                    message,
                ));
            }
            reproduce(c, bug)
        }
        None => {
            let next = format!("speccoder bug confirm {} --digest {digest}", bug.key());
            event(
                c.live,
                c.task,
                BugEvent::ConfirmWaiting {
                    digest: digest.clone(),
                    next: next.clone(),
                },
            );
            stage(
                c.live,
                c.task,
                Stage::Confirm,
                StageState::Started,
                Some("waits for you".to_owned()),
                None,
            );
            let message = format!(
                "{} reproduction(s) fail on the untouched code by an assertion; they wait for your confirmation (digest {digest}): {next}{}",
                bug.reproductions.iter().filter(|r| r.reproduces()).count(),
                crate::bug::held_out_warning(&bug)
                    .map(|w| format!(" — {w}"))
                    .unwrap_or_default()
            );
            Err(Box::new(stop(
                c,
                bug,
                Outcome::AwaitingConfirmation,
                None,
                message,
                began,
            )))
        }
    }
}

fn outcome_word(o: Outcome) -> String {
    o.as_str().replace('_', " ")
}

/// Records a rejection: the reason goes to the next try, and the try that
/// wrote what was rejected is discarded (it never reproduces again; it
/// still counts against the tries). `Err`: it could not be kept.
pub(crate) fn reject(
    state_dir: &Path,
    bug: &mut BugReport,
    reason: &str,
    via: speccoder_core::actions::Via,
) -> Result<(), String> {
    if let Some(t) = bug.tries.last_mut() {
        t.discarded = Some(rules::rejected_try(reason));
    }
    bug.rejections.push(rules::Rejection {
        digest: bug.confirm_digest.clone().unwrap_or_default(),
        reason: reason.to_owned(),
        by: crate::approvals::who(),
        via,
        at: confirm::now(),
    });
    bug.confirm_digest = None;
    bug.outcome = Outcome::Recorded;
    bug.stage = BugStage::Reproduce;
    state::save(state_dir, bug)
}

fn freeze(
    c: &Ctx<'_>,
    mut bug: BugReport,
    by: &str,
    via: speccoder_core::actions::Via,
    basis: Basis,
    began: Instant,
) -> Result<Fix, Box<CommandResult>> {
    match confirm::freeze(c.state, &mut bug, by, via, basis) {
        Ok((record, files)) => {
            event(
                c.live,
                c.task,
                BugEvent::Confirmed {
                    digest: record.approval_digest.clone(),
                    by: by.to_owned(),
                    via: via.as_str().to_owned(),
                    basis: if basis == Basis::NamedTest {
                        "named_test"
                    } else {
                        "shown"
                    }
                    .to_owned(),
                },
            );
            stage(
                c.live,
                c.task,
                Stage::Confirm,
                StageState::Passed,
                Some(by.to_owned()),
                None,
            );
            Ok(Fix { bug, record, files })
        }
        Err(e) => Err(Box::new(stop(
            c,
            bug,
            Outcome::Recorded,
            None,
            format!("the confirmation could not be recorded: {e}"),
            began,
        ))),
    }
}

/// Writes held-out variants for `statements`, red-checks them on the tree
/// as it is (twice, by assertion), and seals the set (keeping variants of
/// other statements already sealed). `told`: what a regeneration is told.
fn seal_variants(
    c: &Ctx<'_>,
    bug: &mut BugReport,
    visible: &[(String, String)],
    statements: &[String],
    told: &[String],
) -> Result<speccoder_core::acceptance::SealedRef, String> {
    let (files, manifest_text) = project_view(c);
    let context = manifest(c, bug, None);
    let input = substrate::acceptance::reproduce_input(&substrate::acceptance::ReproduceParts {
        set: "reproduce_held_out",
        bundle: c.bundle.clone(),
        statements: json!(bug.triad.statements().collect::<Vec<_>>()),
        want: statements,
        files: &files,
        manifest: &manifest_text,
        visible,
        rejections: told,
        issue: None,
        instructions: c.instructions,
        context: &context,
    });
    let (answer, run_id, _, _) =
        author(c, input).map_err(|e| format!("the held-out author: {e}"))?;
    let protected = c.policy.protected();
    let mut kept: Vec<(held_out::HeldOutFile, String)> = Vec::new();
    let mut discarded = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default();
        let content = f["content"].as_str().unwrap_or_default();
        let cited: Vec<String> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        let argv: Vec<String> = f["argv"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        let Ok(rel) = tools::paths::normalise(path) else {
            discarded.push(held_out::Discarded {
                path: path.to_owned(),
                reason: "path".to_owned(),
            });
            continue;
        };
        let ok_path = tools::paths::is_test_path(&rel)
            && protected.write_class(&rel).is_none()
            && !c.project.join(&rel).exists()
            && !visible.iter().any(|(p, _)| *p == rel);
        let ok_cites = cited.len() == 1
            && statements.contains(&cited[0])
            && !kept.iter().any(|(k, _)| k.scenarios == cited);
        let ok_argv = c
            .policy
            .classify("run_command", &json!({ "argv": argv }), None)
            == Decision::Allow;
        if !(ok_path && ok_cites && ok_argv) {
            discarded.push(held_out::Discarded {
                path: rel,
                reason: if ok_path {
                    if ok_cites {
                        "command"
                    } else {
                        "unknown_scenario"
                    }
                } else {
                    "path"
                }
                .to_owned(),
            });
            continue;
        }
        kept.push((
            held_out::HeldOutFile {
                path: rel,
                sha256: sha256_hex(content.as_bytes()),
                scenarios: cited,
                argv,
                composition: false,
            },
            content.to_owned(),
        ));
    }
    // Red twice on the tree as it is, by an assertion.
    let mut red: Vec<(held_out::HeldOutFile, String)> = Vec::new();
    let once = held_out::overlay_run(c.wt_root, c.project, c.tmp, &kept, c.runner, c.policy)
        .map_err(|e| format!("the held-out variants could not be run: {e}"))?;
    let twice = held_out::overlay_run(c.wt_root, c.project, c.tmp, &kept, c.runner, c.policy)
        .map_err(|e| format!("the held-out variants could not be run: {e}"))?;
    for ((f, content), (a, b)) in kept.into_iter().zip(once.iter().zip(&twice)) {
        let class = |r: &held_out::Ran| {
            r.result
                .as_ref()
                .map_or(RedClass::Unclassified, |(res, s)| {
                    classify_red(res, s.as_ref())
                })
        };
        let c2 = rules::bug_class(class(a), Some(class(b)));
        bug.held_out.retain(|h| !f.scenarios.contains(&h.statement));
        bug.held_out.push(HeldOutVariant {
            statement: f.scenarios[0].clone(),
            baseline: c2,
            final_: Final::NotRun,
        });
        if c2 == RedClass::Assertion {
            red.push((f, content));
        } else {
            discarded.push(held_out::Discarded {
                path: f.path,
                reason: "not_red".to_owned(),
            });
        }
    }
    let written = red.len();
    // Variants of statements not written again stay.
    if let Some(old) = bug.held_out_sealed.clone() {
        let (m, contents) = held_out::open_sealed(c.state, &bug.unit, &old)
            .map_err(|e| format!("the sealed held-out set could not be opened: {}", e.0))?;
        for (f, (_, content)) in m.files.into_iter().zip(contents) {
            if !statements.iter().any(|s| f.scenarios.contains(s)) {
                red.push((f, content));
            }
        }
    }
    if written == 0 {
        return Err(if discarded.is_empty() {
            "the held-out author wrote no variant".to_owned()
        } else {
            format!(
                "no held-out variant was red on the untouched code by an assertion ({} discarded: {})",
                discarded.len(),
                discarded
                    .iter()
                    .map(|d| d.reason.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }
    let checkpoint = worktree::checkpoint(c.wt_root, c.tmp)
        .map(|cp| cp.0)
        .unwrap_or_default();
    let manifest = held_out::Manifest {
        version: 1,
        feature: bug.unit.clone(),
        task: bug.task.clone(),
        author_run: run_id,
        checkpoint,
        visible_scenarios: bug.expected_ids(),
        files: red.iter().map(|(f, _)| f.clone()).collect(),
        discarded,
    };
    let contents: Vec<(String, String)> = red
        .iter()
        .map(|(f, c)| (f.path.clone(), c.clone()))
        .collect();
    held_out::seal(c.state, &bug.unit, &bug.requirement, &manifest, &contents)
        .map_err(|e| format!("the held-out variants could not be sealed: {e}"))
}

/// The confirmed set at the checkpoint (the one reuse function every
/// pre-frozen producer uses), with the bug red rule.
pub(crate) fn reuse(c: &Ctx<'_>, fix: &Fix) -> Result<PreparedAcceptance, String> {
    let tests: Vec<ReusedTest> = fix
        .bug
        .reproductions
        .iter()
        .map(|r| ReusedTest {
            path: r.file.clone(),
            cites: vec![r.statement.clone()],
            argv: r.argv.clone(),
            green: !r.reproduces(),
        })
        .collect();
    super::reuse_pre_frozen(
        c.project,
        &fix.record,
        &fix.files,
        &tests,
        c.runner,
        super::RedRule::BugAssertionOnly,
    )
}

/// What judging a bug's attempt decided.
pub(crate) struct Judged {
    /// The verdict, with the bug rules applied.
    pub(crate) verdict: Verdict,
    /// A special case survived even its regenerated variant: the person
    /// rules (`BUG-REPRO-WEAK`).
    pub(crate) weak: Option<speccoder_core::strength::Permitted>,
}

/// What judging needs besides the context.
pub(crate) struct Judging<'a> {
    pub(crate) before: &'a worktree::Checkpoint,
    pub(crate) baseline: &'a VerifyReport,
    pub(crate) verification: Option<&'a VerifyReport>,
    pub(crate) base: Verdict,
    /// The strength directory (rulings on special cases).
    pub(crate) keep: &'a Path,
}

/// The class of reproduction `r` (its one test) on the tree as it is.
fn run_class(c: &Ctx<'_>, r: &Reproduction) -> RedClass {
    let (run, summary) = super::acceptance_run(c.project, &r.argv, c.runner);
    let class = classify_red_one(&run, summary.as_ref(), &r.test);
    if run.status == CheckStatus::Passed && class != RedClass::Unclassified {
        RedClass::Passed
    } else {
        class
    }
}

/// `path` relative to the project, from a repository-relative one.
fn in_project(c: &Ctx<'_>, repo_path: &str) -> Option<String> {
    let prefix = c
        .project
        .strip_prefix(c.wt_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if prefix.is_empty() {
        Some(repo_path.to_owned())
    } else {
        repo_path
            .strip_prefix(&format!("{prefix}/"))
            .map(str::to_owned)
    }
}

/// Judges a bug task's attempt that passed the ordinary rules: finals,
/// pass-to-pass, revert, special cases. `Err`: the tree or a judge input
/// did not come back as it was (the measurement is void).
#[allow(clippy::too_many_lines)] // one linear procedure, in reading order
pub(crate) fn judge(c: &Ctx<'_>, fix: &mut Fix, j: &Judging<'_>) -> Result<Judged, String> {
    let began = Instant::now();
    stage(
        c.live,
        c.task,
        Stage::Unchanged,
        StageState::Started,
        None,
        None,
    );
    let fin = worktree::checkpoint(c.wt_root, c.tmp).map_err(|e| e.0)?;
    let judged_before = crate::integrity::JudgeSnapshot::take(c.wt_root, c.policy);
    // 1. Every reproduction (and Unchanged test) on the final tree.
    let mut unchanged_ok: Vec<String> = Vec::new();
    for r in &mut fix.bug.reproductions {
        let class = run_class(c, r);
        r.final_ = if class == RedClass::Passed {
            Final::Passed
        } else {
            Final::Failed
        };
        if !r.reproduces() && r.final_ == Final::Passed {
            unchanged_ok.push(r.statement.clone());
        }
    }
    // 2. The pass-to-pass set from the diff.
    let changes = worktree::diff(c.wt_root, j.before, &fin).map_err(|e| e.0)?;
    let changed: Vec<String> = changes
        .iter()
        .filter_map(|ch| in_project(c, &ch.path))
        .filter(|p| !tools::paths::is_test_path(p))
        .collect();
    let test_files: Vec<(String, String)> = worktree::files(c.wt_root, &fin)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| in_project(c, &p))
        .filter(|p| tools::paths::is_test_path(p) || changed.contains(p))
        .take(400)
        .filter_map(|p| {
            std::fs::read_to_string(c.project.join(&p))
                .ok()
                .map(|t| (p, t))
        })
        .collect();
    let baseline_tests: Vec<speccoder_core::verify::TestResult> = j
        .baseline
        .checks
        .iter()
        .flat_map(|ch| ch.tests.clone().unwrap_or_default())
        .collect();
    let final_tests: Vec<speccoder_core::verify::TestResult> = j
        .verification
        .map(|v| {
            v.checks
                .iter()
                .flat_map(|ch| ch.tests.clone().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    let crate_name = std::fs::read_to_string(c.project.join("Cargo.toml"))
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.trim().strip_prefix("name").map(|r| {
                    r.trim_start_matches([' ', '='])
                        .trim()
                        .trim_matches('"')
                        .to_owned()
                })
            })
        });
    let unchanged_members: Vec<(String, String, String)> = fix
        .bug
        .reproductions
        .iter()
        .filter(|r| !r.reproduces())
        .map(|r| (r.statement.clone(), r.file.clone(), r.test.clone()))
        .collect();
    let mut p2p: PassToPass = crate::bug::unchanged::select(
        &changed,
        &test_files,
        &baseline_tests,
        crate_name.as_deref(),
        &unchanged_members,
    );
    crate::bug::unchanged::settle(&mut p2p, &final_tests, &|s| {
        unchanged_ok.iter().any(|u| u == s)
    });
    event(
        c.live,
        c.task,
        BugEvent::UnchangedSelected {
            state: if p2p.state == rules::PassToPassState::Selected {
                "selected"
            } else {
                "none_selected"
            }
            .to_owned(),
            count: p2p.members.len(),
        },
    );

    // 3. The revert: the attempt's source changes undone, its tests kept.
    worktree::restore(c.wt_root, c.tmp, j.before).map_err(|e| e.0)?;
    let git = crate::git::Git::at(c.wt_root);
    for ch in &changes {
        if !in_project(c, &ch.path).is_some_and(|p| tools::paths::is_test_path(&p)) {
            continue;
        }
        let full = c.wt_root.join(&ch.path);
        match ch.kind {
            worktree::ChangeKind::Deleted => {
                let _ = std::fs::remove_file(&full);
            }
            _ => {
                let content = git
                    .run(&["show", &format!("{}:{}", fin.0, ch.path)])
                    .map_err(|e| e.0)?;
                if let Some(parent) = full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&full, content).map_err(|e| e.to_string())?;
            }
        }
    }
    let mut rejected_by = Vec::new();
    let mut any_unviable = false;
    for r in fix.bug.reproductions.iter_mut().filter(|r| r.reproduces()) {
        let class = run_class(c, r);
        r.revert = match class {
            RedClass::Assertion => {
                rejected_by.push(r.statement.clone());
                Revert::Rejected
            }
            RedClass::Passed => Revert::Survived,
            _ => {
                any_unviable = true;
                Revert::Unviable
            }
        };
    }
    worktree::restore(c.wt_root, c.tmp, &fin).map_err(|e| e.0)?;
    let revert = RevertResult {
        outcome: if !rejected_by.is_empty() {
            Revert::Rejected
        } else if any_unviable {
            Revert::Unviable
        } else {
            Revert::Survived
        },
        rejected_by,
    };
    stage(
        c.live,
        c.task,
        Stage::Unchanged,
        if p2p.held() && !revert.rejected_by.is_empty() {
            StageState::Passed
        } else {
            StageState::Failed
        },
        Some(format!(
            "{} pass-to-pass, revert {}",
            p2p.members.len(),
            revert_word(revert.outcome)
        )),
        Some(began),
    );

    // 4. Special cases: one per statement, against the combined set.
    let mut special: Vec<SpecialCase> = Vec::new();
    let mut special_error: Option<String> = None;
    let mut weak = None;
    if !revert.rejected_by.is_empty()
        && fix
            .bug
            .reproductions
            .iter()
            .all(|r| r.final_ == Final::Passed)
    {
        let patch = worktree::patch(c.wt_root, c.start, &fin).unwrap_or_default();
        let changed_sources = crate::strength::changed_files(c.wt_root, &patch);
        let shown: Vec<(String, String, String)> = fix
            .bug
            .reproductions
            .iter()
            .filter(|r| r.reproduces())
            .filter_map(|r| {
                fix.files
                    .iter()
                    .find(|(p, _)| *p == r.file)
                    .map(|(p, content)| (r.statement.clone(), p.clone(), content.clone()))
            })
            .collect();
        if changed_sources.is_empty() {
            special_error =
                Some("the fix changes no source file a special case could replace".to_owned());
        } else {
            let context = manifest(c, &fix.bug, None);
            let input = substrate::near_miss::special_case_input(
                c.bundle,
                &changed_sources,
                &crate::mutation::source_patch(&patch),
                &shown,
                &context,
            );
            // A failed or empty answer is said, never read as "none needed".
            let variants = match c.rt.block_on(c.plane.near_miss(c.feature, c.task, input)) {
                Err(e) => {
                    special_error = Some(format!("the special-case author call failed: {}", e.0));
                    Vec::new()
                }
                Ok(o) => {
                    let answer = o
                        .output
                        .as_ref()
                        .map(|v| v.peek()["answer"].clone())
                        .unwrap_or_default();
                    if !matches!(o.status, agentplane::prelude::RunStatus::Succeeded)
                        || answer.is_null()
                    {
                        special_error = Some(format!(
                            "the special-case author did not answer ({})",
                            o.status.as_str()
                        ));
                        Vec::new()
                    } else {
                        let v = special_variants(&answer, &changed_sources);
                        if v.is_empty() {
                            special_error = Some(
                                "the special-case author wrote no candidate that changes the fix's source files".to_owned(),
                            );
                        }
                        v
                    }
                }
            };
            let rulings = crate::strength::rulings(j.keep);
            for v in variants {
                let statement = v.scenario.clone();
                if special.iter().any(|s| s.statement == statement) {
                    continue;
                }
                let digest = v.digest();
                let ruled = rulings.iter().any(|r| r.digest == digest);
                let mut rejected = special_rejected(c, fix, &v, &fin)?;
                let mut regenerated = false;
                if !rejected && !ruled {
                    // One regeneration of the statement's variant, told what got through.
                    regenerated = true;
                    let visible: Vec<(String, String)> = shown
                        .iter()
                        .filter(|(s, _, _)| *s == statement)
                        .map(|(_, p, content)| (p.clone(), content.clone()))
                        .collect();
                    worktree::restore(c.wt_root, c.tmp, j.before).map_err(|e| e.0)?;
                    let told = vec![format!(
                        "a fix that special-cases this input passed the earlier variant: {}",
                        v.rationale
                    )];
                    let resealed = seal_variants(
                        c,
                        &mut fix.bug,
                        &visible,
                        std::slice::from_ref(&statement),
                        &told,
                    );
                    worktree::restore(c.wt_root, c.tmp, &fin).map_err(|e| e.0)?;
                    match resealed {
                        Ok(sealed) => {
                            fix.bug.held_out_sealed = Some(sealed.clone());
                            fix.record.held_out_sealed = Some(sealed);
                            crate::pre_frozen::store(c.state, &fix.record, &fix.files).map_err(
                                |e| format!("the re-sealed held-out set could not be kept: {e}"),
                            )?;
                            rejected = special_rejected(c, fix, &v, &fin)?;
                        }
                        // No new variant: the special case still stands, and why is said.
                        Err(why) => {
                            fix.bug.held_out_reason = Some(why);
                        }
                    }
                }
                if !rejected && !ruled {
                    weak = Some(speccoder_core::strength::Permitted {
                        id: format!("special-case-{statement}"),
                        scenario: statement.clone(),
                        rationale: v.rationale.clone(),
                        digest: digest.clone(),
                        outcome: speccoder_core::strength::PermittedOutcome::Rejected,
                        failing: None,
                        patch: None,
                    });
                }
                special.push(SpecialCase {
                    statement,
                    digest,
                    rejected,
                    regenerated,
                    ruled,
                });
            }
        }
    }
    // Held-out: every sealed variant — a regenerated one too — runs on the
    // final tree and must pass there (FR-016, FR-019).
    let held = match fix.bug.held_out_sealed.clone() {
        Some(sealed) => {
            let (m, contents) = held_out::open_sealed(c.state, &fix.bug.unit, &sealed)
                .map_err(|e| format!("the sealed held-out set could not be opened: {}", e.0))?;
            let files: Vec<(held_out::HeldOutFile, String)> = m
                .files
                .into_iter()
                .zip(contents.into_iter().map(|(_, x)| x))
                .collect();
            let runs =
                held_out::overlay_run(c.wt_root, c.project, c.tmp, &files, c.runner, c.policy)?;
            let mut failed: Vec<String> = Vec::new();
            let mut passed: Vec<String> = Vec::new();
            for ((f, _), r) in files.iter().zip(&runs) {
                let ok = r
                    .result
                    .as_ref()
                    .is_some_and(|(res, _)| res.status == CheckStatus::Passed);
                for st in &f.scenarios {
                    if ok {
                        passed.push(st.clone());
                    } else {
                        failed.push(st.clone());
                    }
                }
            }
            if runs.len() != files.len() {
                failed.push("held-out".to_owned());
            }
            for h in &mut fix.bug.held_out {
                h.final_ = if failed.contains(&h.statement) {
                    Final::Failed
                } else if passed.contains(&h.statement) {
                    Final::Passed
                } else {
                    Final::NotRun
                };
            }
            failed.sort();
            failed.dedup();
            if failed.is_empty() {
                HeldOut::Passed { files: files.len() }
            } else {
                HeldOut::Failed { scenarios: failed }
            }
        }
        None if c.held_out => HeldOut::Absent {
            reason: fix
                .bug
                .held_out_reason
                .clone()
                .unwrap_or_else(|| "none_kept".to_owned()),
        },
        None => HeldOut::Off,
    };
    // The tree and the judge inputs are as they were.
    let now = worktree::checkpoint(c.wt_root, c.tmp).map_err(|e| e.0)?;
    if now != fin {
        return Err("the bug's judging left changes in the worktree".to_owned());
    }
    let drift = judged_before.drift(
        &crate::integrity::JudgeSnapshot::take(c.wt_root, c.policy),
        c.policy,
        &std::collections::BTreeSet::new(),
    );
    if let Some(d) = drift.first() {
        return Err(format!("the bug's judging changed judge input {}", d.path));
    }
    event(
        c.live,
        c.task,
        BugEvent::Discrimination {
            revert: revert_word(revert.outcome),
            special_case: special
                .iter()
                .map(|s| crate::progress::ShownSpecialCase {
                    statement: s.statement.clone(),
                    rejected: s.rejected || s.ruled,
                })
                .collect(),
        },
    );
    let verdict = {
        let d = rules::Discrimination {
            reproductions: &fix.bug.reproductions,
            revert: &revert,
            special_case: &special,
            special_case_error: special_error.as_deref(),
        };
        rules::bug_verdict(j.base.clone(), &d, &p2p, &held)
    };
    fix.bug.pass_to_pass = Some(p2p);
    fix.bug.revert = Some(revert);
    fix.bug.special_case = special;
    state::save(c.state, &fix.bug).map_err(|e| format!("the bug could not be kept: {e}"))?;
    Ok(Judged { verdict, weak })
}

fn revert_word(r: Revert) -> String {
    match r {
        Revert::Rejected => "rejected",
        Revert::Survived => "survived",
        Revert::Unviable => "unviable",
        Revert::NotRun => "not_run",
    }
    .to_owned()
}

/// The author's special cases, checked like any variant: only changed
/// source files, each a real change.
fn special_variants(answer: &Value, changed: &[(String, String)]) -> Vec<crate::strength::Variant> {
    let mut out = Vec::new();
    for v in answer["variants"].as_array().into_iter().flatten() {
        if v["kind"] != "special_case" {
            continue;
        }
        let mut files = Vec::new();
        let mut ok = true;
        for f in v["files"].as_array().into_iter().flatten() {
            let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
                ok = false;
                break;
            };
            match tools::paths::normalise(path).ok().and_then(|p| {
                changed
                    .iter()
                    .find(|(c, _)| *c == p)
                    .map(|(c, now)| (c.clone(), now.clone()))
            }) {
                Some((p, now)) if now != content => files.push((p, content.to_owned())),
                Some(_) => {}
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !files.is_empty() {
            out.push(crate::strength::Variant {
                kind: "special_case".to_owned(),
                scenario: v["scenario"].as_str().unwrap_or_default().to_owned(),
                rationale: v["rationale"].as_str().unwrap_or_default().to_owned(),
                files,
            });
        }
    }
    out
}

/// Whether the combined set (the statement's reproduction and its held-out
/// variants) rejects the special case by an assertion.
fn special_rejected(
    c: &Ctx<'_>,
    fix: &Fix,
    v: &crate::strength::Variant,
    fin: &worktree::Checkpoint,
) -> Result<bool, String> {
    for (p, content) in &v.files {
        std::fs::write(c.wt_root.join(p), content).map_err(|e| format!("{p}: {e}"))?;
    }
    let mut rejected = false;
    for r in fix
        .bug
        .reproductions
        .iter()
        .filter(|r| r.statement == v.scenario)
    {
        if run_class(c, r) == RedClass::Assertion {
            rejected = true;
        }
    }
    if !rejected
        && let Some(sealed) = &fix.bug.held_out_sealed
        && let Ok((m, contents)) = held_out::open_sealed(c.state, &fix.bug.unit, sealed)
    {
        let files: Vec<(held_out::HeldOutFile, String)> = m
            .files
            .into_iter()
            .zip(contents.into_iter().map(|(_, x)| x))
            .filter(|(f, _)| f.scenarios.contains(&v.scenario))
            .collect();
        let runs = held_out::overlay_run(c.wt_root, c.project, c.tmp, &files, c.runner, c.policy)?;
        rejected = runs
            .iter()
            .filter_map(|r| r.result.as_ref())
            .any(|(res, s)| classify_red(res, s.as_ref()) == RedClass::Assertion);
    }
    worktree::restore(c.wt_root, c.tmp, fin).map_err(|e| e.0)?;
    Ok(rejected)
}

/// The trailers a bug task's tick commit carries; `rulings`: the person's
/// rulings on special cases (`task rule --tests-right`), named when one
/// let a special case through.
pub(crate) fn trailers(
    fix: &Fix,
    verdict_word: &str,
    rulings: &[speccoder_core::strength::Ruling],
) -> String {
    let outcome = if matches!(verdict_word, "verified" | "attested") {
        "fixed"
    } else {
        "failed"
    };
    let mut out = format!(
        "Bug: {outcome} ({}) triad={}\n",
        fix.bug.home.kind.as_str(),
        fix.bug.triad.digest
    );
    out.push_str(
        &fix.record
            .trailer(&crate::pre_frozen::manifest_digest(&fix.record)),
    );
    for s in fix
        .bug
        .special_case
        .iter()
        .filter(|s| s.ruled && !s.rejected)
    {
        if let Some(r) = rulings.iter().find(|r| r.digest == s.digest) {
            out.push_str(&format!(
                "Ruling: tests-right {} {} by {} — {}\n",
                s.statement, r.digest, r.by, r.reason
            ));
        }
    }
    if let Some(i) = fix.bug.issue.as_ref().filter(|_| outcome == "fixed") {
        // A trailer git parses (`Fixes #N` without the colon would make
        // git read none of the block); GitHub closes on it either way.
        out.push_str(&format!("Fixes: #{}\n", i.number));
    }
    out
}

/// The predicate's `bug` and `acceptance` fields (spec 094's shape).
pub(crate) fn predicate_fields(
    bug: &BugReport,
    record: &speccoder_core::acceptance::PreFrozenAcceptance,
) -> (Value, Value) {
    let b = json!({
        "home": bug.home.kind.as_str(),
        "triad_sha256": bug.triad.digest,
        "outcome": bug.outcome.as_str(),
        "reproductions": bug.reproductions.iter().map(|r| json!({
            "statement": r.statement, "baseline": r.baseline, "final": r.final_, "revert": r.revert,
        })).collect::<Vec<_>>(),
        "special_case": bug.special_case.iter().map(|s| json!({
            "statement": s.statement, "rejected": s.rejected, "regenerated": s.regenerated,
        })).collect::<Vec<_>>(),
        "pass_to_pass": bug.pass_to_pass.as_ref().map(|p| json!({ "state": p.state, "count": p.members.len() })),
    });
    let a = json!({
        "source": record.producer.source(),
        "pre_frozen": {
            "manifest_sha256": crate::pre_frozen::manifest_digest(record),
            "approval_sha256": record.approval_digest,
            "wording_sha256": record.wording_digest,
            "by": record.decided.by,
            "via": record.decided.via,
        },
    });
    (b, a)
}

/// After the run: the bug's outcome from the task's.
pub(crate) fn finish(
    state_dir: &Path,
    fix: &mut Fix,
    report: &speccoder_core::RunReport,
    passed: bool,
) {
    // What the attempt recorded of the candidate meanwhile.
    if let Some(saved) = state::load(state_dir, &fix.bug.unit, &fix.bug.task) {
        fix.bug.candidate = saved.candidate;
    }
    fix.bug.verdict.clone_from(&report.verdict);
    fix.bug.run_id = (!report.run_id.is_empty()).then(|| report.run_id.clone());
    fix.bug.commit.clone_from(&report.commit);
    fix.bug.outcome = if passed {
        Outcome::Fixed
    } else {
        Outcome::Failed
    };
    fix.bug.stage = if passed {
        BugStage::Done
    } else {
        BugStage::Fix
    };
    fix.bug.code = (!passed
        && fix
            .bug
            .revert
            .as_ref()
            .is_some_and(|r| r.rejected_by.is_empty()))
    .then(|| "BUG-NOT-DISCRIMINATING".to_owned());
    let _ = state::save(state_dir, &fix.bug);
}

/// Someone else's fix as the attempt: refused before it is applied when it
/// touches a frozen reproduction, a held-out path or a judge input, or
/// when it does not apply. `Ok`: `(run id, succeeded, output)` as an
/// attempt reports them.
pub(crate) fn apply_candidate(
    c: &Ctx<'_>,
    diff: &str,
    candidate: &rules::Candidate,
    frozen: &[String],
    held_out_paths: &[String],
) -> Result<(String, bool, Value), Box<CommandResult>> {
    let refused = |code: &str, paths: Vec<String>, status: CommandStatus, message: String| {
        let _ = worktree::restore(c.wt_root, c.tmp, c.start);
        if let Some(mut bug) = state::load(c.state, c.feature, c.task) {
            if let Some(cand) = bug.candidate.as_mut() {
                cand.refused = Some(rules::CandidateRefusal {
                    code: code.to_owned(),
                    paths: paths.clone(),
                });
            }
            bug.outcome = Outcome::Failed;
            bug.code = Some(code.to_owned());
            let _ = state::save(c.state, &bug);
        }
        let mut report = bare_report(c.feature, c.task, c.wt_path, "candidate_refused");
        report.reason = Some(message.clone());
        report.bug = state::load(c.state, c.feature, c.task).map(Box::new);
        Box::new(
            CommandResult::new("run", status, &message)
                .with_code(code)
                .with_data(CommandData::Run(Box::new(report))),
        )
    };
    // The paths as git reads the diff: renames, mode changes and quoted
    // paths included; a binary hunk or a symbolic link is not applied.
    let touched = match crate::bug::candidate::touched(c.wt_root, diff) {
        Ok(t) => t,
        Err(r) => {
            let message = format!("the candidate is not applied: {}", r.paths.join("; "));
            return Err(refused(
                "BUG-CANDIDATE-APPLY",
                r.paths,
                CommandStatus::GitPrecondition,
                message,
            ));
        }
    };
    let judged = crate::bug::candidate::judge_paths(&touched, c.policy, frozen, held_out_paths);
    if !judged.is_empty() {
        return Err(refused(
            "BUG-CANDIDATE-JUDGE",
            judged.clone(),
            CommandStatus::UsageError,
            format!(
                "the candidate touches what judges it, so it is not applied: {}",
                judged.join(", ")
            ),
        ));
    }
    if let Err(r) = crate::bug::candidate::apply(c.wt_root, c.tmp, diff) {
        let message = format!(
            "the candidate does not apply on the branch: {}",
            if r.paths.is_empty() {
                "git refused it".to_owned()
            } else {
                r.paths.join("; ")
            }
        );
        return Err(refused(
            "BUG-CANDIDATE-APPLY",
            r.paths,
            CommandStatus::GitPrecondition,
            message,
        ));
    }
    if let Some(mut bug) = state::load(c.state, c.feature, c.task) {
        if let Some(cand) = bug.candidate.as_mut() {
            cand.applied = true;
        }
        let _ = state::save(c.state, &bug);
    }
    let short: String = candidate
        .digest
        .trim_start_matches("sha256:")
        .chars()
        .take(12)
        .collect();
    let from = match (&candidate.path, candidate.number) {
        (_, Some(n)) => format!("pull request #{n}"),
        (Some(p), None) => p.clone(),
        (None, None) => "a patch".to_owned(),
    };
    Ok((
        format!("candidate_{short}"),
        true,
        json!({
            "status": "completed",
            "report": { "summary": format!("applied {from} as the attempt"), "status": "completed" },
            "tool_calls": [],
        }),
    ))
}

/// The run's result for a bug task: the bug's outcome from the task's, its
/// codes and the event.
pub(crate) fn finished(
    c: &Ctx<'_>,
    fix: &mut Fix,
    mut report: speccoder_core::RunReport,
    attempt_status: &str,
    passed: bool,
    status: CommandStatus,
    message: String,
) -> CommandResult {
    let done = passed && (report.ticked || report.commit.is_some());
    finish(c.state, fix, &report, done);
    let mut status = status;
    let mut code = None;
    if attempt_status == "repro_weak" {
        status = CommandStatus::NeedsHuman;
        code = Some("BUG-REPRO-WEAK");
        fix.bug.code = Some("BUG-REPRO-WEAK".to_owned());
        let _ = state::save(c.state, &fix.bug);
    } else if !done
        && fix
            .bug
            .revert
            .as_ref()
            .is_some_and(|r| r.rejected_by.is_empty())
    {
        code = Some("BUG-NOT-DISCRIMINATING");
    }
    let next = handoff::next(&fix.bug, fix.bug.outcome);
    event(
        c.live,
        c.task,
        BugEvent::Outcome {
            outcome: fix.bug.outcome.as_str().to_owned(),
            verdict: report.verdict.as_ref().map(super::outcome::verdict_word),
            code: code.map(str::to_owned),
            next: next.iter().map(|n| n.command.clone()).collect(),
        },
    );
    report.bug = Some(Box::new(fix.bug.clone()));
    c.live.event(&super::finished_event(
        c.task,
        &report,
        attempt_status,
        false,
        message.clone(),
    ));
    let mut r =
        CommandResult::new("run", status, &message).with_data(CommandData::Run(Box::new(report)));
    if let Some(code) = code {
        r = r.with_code(code);
    }
    r
}

/// A bug in the format's own bug structure, as a run takes it: the unit
/// no task list names, its directory, and what the attempt is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBug {
    /// The unit (`bugs/<slug>`).
    pub unit: String,
    /// Its directory, project-relative.
    pub dir: String,
    /// The task (`bug:<slug>`).
    pub task: String,
    /// The title.
    pub title: String,
    /// What the attempt reads as the requirement: the person's triad.
    pub story: String,
}

impl NativeBug {
    /// The bug of `bug`.
    #[must_use]
    pub fn of(bug: &BugReport) -> Self {
        Self {
            unit: bug.unit.clone(),
            dir: bug.home.location.clone(),
            task: bug.task.clone(),
            title: bug.title.clone(),
            story: format!(
                "A bug, in the reporter's words.\n\n{}",
                bug.triad.labelled()
            ),
        }
    }

    /// The context bundle an attempt at it is given.
    #[must_use]
    pub fn bundle(&self, project: &Path) -> crate::context::Bundle {
        let [rules, ..] =
            crate::format::at(project).convergence_texts(project, &project.join(&self.dir));
        crate::context::Bundle {
            feature: self.unit.clone(),
            constitution: crate::context::cap(&rules),
            plan: String::new(),
            story: crate::context::cap(&self.story),
            task: crate::context::TaskContext {
                id: self.task.clone(),
                title: format!("Fix: {}", self.title),
                phase: String::new(),
                subsection: String::new(),
                story: String::new(),
                parallel: false,
            },
            dependencies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reproduction_is_a_test_file_run_by_speccoders_own_command() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let policy = Policy::new(
            &[],
            &[vec![
                "./run-one".to_owned(),
                "tests/repro.sh".to_owned(),
                "empty".to_owned(),
            ]],
        );
        let template = [
            "./run-one".to_owned(),
            "{file}".to_owned(),
            "{name}".to_owned(),
        ];
        let want = ["E1".to_owned()];
        let answer = |path: &str, test: &str| json!({ "files": [{ "path": path, "content": "x", "statement": "E1", "test": test, "argv": ["sh", "-c", "exit 1"] }] });
        // The author's command is never run: speccoder's runs only the test.
        let ok = validate(
            &answer("tests/repro.sh", "empty"),
            &want,
            dir.path(),
            &policy,
            Some(&template),
        );
        let Ok(files) = ok else {
            panic!("{ok:?}", ok = ok.err());
        };
        assert_eq!(files[0].argv, ["./run-one", "tests/repro.sh", "empty"]);
        // A file that configures how every test runs is not a reproduction.
        for path in [
            "tests/conftest.py",
            "tests/__init__.py",
            "tests/vitest.setup.ts",
        ] {
            let e = validate(
                &answer(path, "t"),
                &want,
                dir.path(),
                &policy,
                Some(&template),
            )
            .err();
            assert!(
                e.as_deref()
                    .is_some_and(|e| e.contains("configures how tests run")),
                "{path}: {e:?}"
            );
        }
        // A test without a name, or with no rule to run it alone, is not one.
        assert!(
            validate(
                &answer("tests/repro.sh", " "),
                &want,
                dir.path(),
                &policy,
                Some(&template)
            )
            .is_err()
        );
        assert!(
            validate(
                &answer("tests/repro.unknown", "t"),
                &want,
                dir.path(),
                &policy,
                None
            )
            .err()
            .is_some_and(|e| e.contains("[bug] test"))
        );
    }
}
