//! `speccoder bug`: the L2 rung. The person's triad is recorded where the
//! format keeps bugs (its own bug structure first, else its standing
//! bug-fix unit), reproduced by a test-blind author — one test per stated
//! behaviour, red on the untouched code by an assertion — confirmed by the
//! person once, frozen, and fixed as an ordinary task whose verdict also
//! needs the revert rejected. Nothing here is a second pipeline: the
//! stages run inside `speccoder run` (`pipeline::bug`).

pub mod candidate;
pub mod confirm;
pub mod handoff;
pub mod keep;
pub mod named;
pub mod state;
pub mod unchanged;

pub use keep::{keep, keep_with};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use speccoder_core::actions::Via;
use speccoder_core::bug::{
    BugHome, BugReport, Final, Outcome, Reproduction, Revert, Source, Stage, Triad,
};
use speccoder_core::ladder::Origin;
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::RunOptions;
use crate::format::BugRefusal;
use crate::integrity::sha256_hex;
use crate::progress::{BugEvent, Event};

/// What the person asks for.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// What happens (the symptom).
    pub symptom: String,
    /// What should happen instead.
    pub expected: Vec<String>,
    /// What must keep working.
    pub unchanged: Vec<String>,
    /// The issue it comes from.
    pub issue: Option<u64>,
    /// Their own reproduction tests, one per expected statement.
    pub tests: Vec<String>,
    /// Someone else's fix to judge.
    pub candidate: Option<String>,
    /// Continue a bug of the format's own bug structure.
    pub slug: Option<String>,
    /// Reproduce it once recorded.
    pub run: bool,
    /// The GitHub to read the issue or pull request from (default: the
    /// environment's).
    pub github: Option<crate::github::Ctx>,
}

impl Request {
    /// The request a [`speccoder_core::actions::Runnable::BugFix`] carries.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        symptom: &str,
        expected: &[String],
        unchanged: &[String],
        issue: Option<u64>,
        tests: &[String],
        candidate: Option<&str>,
        slug: Option<&str>,
        run: bool,
    ) -> Self {
        Self {
            symptom: symptom.to_owned(),
            expected: expected.to_vec(),
            unchanged: unchanged.to_vec(),
            issue,
            tests: tests.to_vec(),
            candidate: candidate.map(str::to_owned),
            slug: slug.map(str::to_owned),
            run,
            github: None,
        }
    }
}

fn refuse(r: &BugRefusal) -> CommandResult {
    let status =
        speccoder_core::codes::lookup(r.code).map_or(CommandStatus::UsageError, |e| e.status);
    CommandResult::new("bug", status, &r.message).with_code(r.code)
}

fn result(status: CommandStatus, message: &str, bug: &BugReport) -> CommandResult {
    let mut r = CommandResult::new("bug", status, message)
        .with_next(handoff::next(bug, bug.outcome))
        .with_data(CommandData::Bug(Box::new(bug.clone())));
    if let Some(code) = bug.code.as_deref() {
        r = r.with_code(code);
    }
    r
}

fn emit(options: &RunOptions, task: &str, step: BugEvent) {
    if let Some(p) = &options.progress {
        p.event(&Event::Bug {
            task: task.to_owned(),
            step,
        });
    }
}

/// The state directory: `state`, else the environment's.
fn state_or_env(state: Option<&Path>) -> Result<PathBuf, Box<CommandResult>> {
    state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
        .ok_or_else(|| {
            Box::new(
                CommandResult::new(
                    "bug",
                    CommandStatus::ConfigError,
                    "no state directory (set SPECCODER_STATE_DIR or HOME)",
                )
                .with_code("NO-STATE-DIR"),
            )
        })
}

/// `f` with the configured model and `state` (else the environment's).
fn with_model(
    path: &Path,
    state: Option<&Path>,
    f: impl FnOnce(&Path, Arc<dyn ModelProvider>, &str) -> CommandResult,
) -> CommandResult {
    let state = match state_or_env(state) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let model = match crate::pipeline::setup(path, None, &state, "bug") {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    match crate::substrate::model_config::open_driver(&model) {
        Ok((provider, key)) => f(&state, provider, &key.unwrap_or_default()),
        Err(e) => crate::substrate::model_config::refused("bug", &e),
    }
}

/// Whether recording or running needs the model.
fn needs_model(path: &Path, req: &Request) -> bool {
    if req.run {
        return true;
    }
    let format = crate::format::at(path);
    format
        .detect(path)
        .and_then(|root| format.bug_unit(&root).ok())
        .is_some_and(|h| h.kind.native() && req.slug.is_none())
}

/// `speccoder bug …` with the configured model.
#[must_use]
pub fn bug(
    path: &Path,
    req: &Request,
    via: Via,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    // What is refused is refused before a model is opened.
    if req.slug.is_none()
        && let Err(e) = Triad::new(&req.symptom, &req.expected, &req.unchanged, sha256_hex)
    {
        return refuse(&BugRefusal::new(e.code(), e.message()));
    }
    if needs_model(path, req) {
        return with_model(path, state, |state, provider, key| {
            bug_with(path, req, via, state, Some(provider), Some(key), options)
        });
    }
    match state_or_env(state) {
        Ok(state) => bug_with(path, req, via, &state, None, None, options),
        Err(r) => *r,
    }
}

/// The project root at `path`, or why there is none (Kiro names F-052).
fn root_of(
    path: &Path,
) -> Result<(PathBuf, &'static dyn crate::format::Adapter), Box<CommandResult>> {
    match crate::format::open(path, None) {
        Ok(o) => Ok((PathBuf::from(&o.snapshot.root), o.adapter)),
        Err(_) if let Some(r) = crate::format::bug_refusal_unsupported(path) => {
            Err(Box::new(refuse(&r)))
        }
        Err(r) => Err(Box::new(CommandResult::new("bug", r.status, &r.message))),
    }
}

/// `[bug]` settings of the project at `root`.
fn settings(root: &Path) -> crate::substrate::run_config::RunConfig {
    crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| crate::substrate::run_config::parse(&t).ok())
        .unwrap_or_default()
}

/// How `read` (the format's file) differs from the person's `triad`: each
/// statement of theirs it lacks, and each it carries that they did not give
/// (an extra line, a longer or reworded one). Empty: exactly their words
/// (whitespace-normalised), the same statements in the same order.
fn drift(triad: &Triad, read: &crate::format::ReadBug) -> Vec<String> {
    let norm = |v: &[String]| -> Vec<String> {
        v.iter()
            .map(|x| speccoder_core::bug::normalise(x))
            .collect()
    };
    let side = |label: &str, given: Vec<String>, found: Vec<String>| -> Vec<String> {
        let mut out: Vec<String> = given
            .iter()
            .filter(|g| !found.contains(g))
            .map(|g| format!("missing {label}: {g}"))
            .chain(
                found
                    .iter()
                    .filter(|f| !given.contains(f))
                    .map(|f| format!("not yours {label}: {f}")),
            )
            .collect();
        if out.is_empty() && given != found {
            out.push(format!(
                "{label}: {} statement(s) given, {} read back (another order or a repeat)",
                given.len(),
                found.len()
            ));
        }
        out
    };
    let mut out = side(
        "Current",
        vec![triad.current.text.clone()],
        norm(&read.current),
    );
    out.extend(side(
        "Expected",
        triad.expected.iter().map(|s| s.text.clone()).collect(),
        norm(&read.expected),
    ));
    out.extend(side(
        "Unchanged",
        triad.unchanged.iter().map(|s| s.text.clone()).collect(),
        norm(&read.unchanged),
    ));
    out
}

fn triad_of(read: &crate::format::ReadBug) -> Result<Triad, BugRefusal> {
    Triad::new(
        read.current.first().map_or("", String::as_str),
        &read.expected,
        &read.unchanged,
        sha256_hex,
    )
    .map_err(|e| BugRefusal::new(e.code(), e.message()))
}

/// [`bug()`] against `state` with `provider` (needed to record in a format's
/// own bug structure and to run).
#[allow(clippy::too_many_lines)] // one linear procedure, in reading order
#[must_use]
pub fn bug_with(
    path: &Path,
    req: &Request,
    via: Via,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let (root, format) = match root_of(path) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let plane = crate::worktree::project_state_dir(state, &root);
    let config = settings(&root);
    let home = match format.bug_unit(&root) {
        Ok(h) => h,
        Err(r) => return refuse(&r),
    };
    let by = crate::approvals::who_in(&root);

    // Continue a bug the format's own bug step created.
    if let Some(slug) = req.slug.as_deref() {
        if !speccoder_core::bug::valid_slug(slug) {
            return invalid_slug(slug);
        }
        let Some((dir, unit)) = format.bug_dir(&home, slug) else {
            return CommandResult::new(
                "bug",
                CommandStatus::UsageError,
                &format!("--slug names a bug of the format's own bug structure; this project's bugs live in {}", home.location),
            )
            .with_code("USAGE");
        };
        let task = format!("bug:{slug}");
        if let Some(existing) = state::load(&plane, &unit, &task) {
            return if req.run {
                run_bug(&root, &existing, state, provider, secret, options)
            } else {
                result(
                    CommandStatus::Ok,
                    &format!(
                        "{} is recorded; run it: speccoder bug run --slug {slug}",
                        existing.key()
                    ),
                    &existing,
                )
            };
        }
        let read = match format.read_bug(&root, &home, slug) {
            Ok(r) => r,
            Err(r) => return refuse(&r),
        };
        let triad = match triad_of(&read) {
            Ok(t) => t,
            Err(r) => return refuse(&BugRefusal::new("BUG-ASSESSMENT-UNREADABLE", r.message)),
        };
        if !req.expected.is_empty() {
            match Triad::new(&req.symptom, &req.expected, &req.unchanged, sha256_hex) {
                Ok(given) => {
                    let missing = drift(&given, &read);
                    if !missing.is_empty() {
                        return refuse(&BugRefusal::new(
                            "BUG-ASSESSMENT-DRIFT",
                            format!(
                                "{} does not carry exactly your words: {}",
                                read.file,
                                missing.join("; ")
                            ),
                        ));
                    }
                }
                Err(e) => return refuse(&BugRefusal::new(e.code(), e.message())),
            }
        }
        let home = BugHome {
            location: dir,
            ..home
        };
        let title = read.title.clone().unwrap_or_else(|| triad.title());
        let bug = new_report(
            home,
            unit,
            task.clone(),
            task,
            title,
            triad,
            config.reproduce_tries,
            None,
        );
        return finish_record(
            &root,
            &plane,
            bug,
            Vec::new(),
            state::Extra {
                by: Some(by),
                via: Some(via),
                ..state::Extra::default()
            },
            via,
            state,
            provider,
            secret,
            req.run,
            options,
        );
    }

    // The person's words, checked before anything is written.
    let triad = match Triad::new(&req.symptom, &req.expected, &req.unchanged, sha256_hex) {
        Ok(t) => t,
        Err(e) => return refuse(&BugRefusal::new(e.code(), e.message())),
    };
    let mut person: Vec<(Reproduction, String)> = Vec::new();
    if !req.tests.is_empty() {
        if req.tests.len() != triad.expected.len() {
            return refuse(&BugRefusal::new(
                "BUG-TEST-COVERAGE",
                format!(
                    "{} --test for {} --expected statement(s): one test per stated behaviour, in order",
                    req.tests.len(),
                    triad.expected.len()
                ),
            ));
        }
        let runner = match crate::sandbox::runner_at(&root) {
            Ok((r, _)) => r,
            Err(e) => return CommandResult::new("bug", CommandStatus::ConfigError, &e),
        };
        for (spec, statement) in req.tests.iter().zip(&triad.expected) {
            let n = match named::resolve(&root, spec, config.bug_test.as_deref()) {
                Ok(n) => n,
                Err(r) => return refuse(&r),
            };
            let (class, _) = named::baseline(&root, &n.argv, &runner);
            if class != speccoder_core::verify::RedClass::Assertion {
                return refuse(&named::not_red(spec, class));
            }
            person.push((
                Reproduction {
                    statement: statement.id.clone(),
                    file: n.file,
                    test: n.name,
                    source: Source::Person,
                    baseline: vec![class, class],
                    digest: format!("sha256:{}", sha256_hex(n.content.as_bytes())),
                    argv: n.argv,
                    final_: Final::NotRun,
                    revert: Revert::NotRun,
                },
                n.content,
            ));
        }
    }
    let mut extra = state::Extra {
        by: Some(by.clone()),
        via: Some(via),
        ..state::Extra::default()
    };
    let mut candidate = None;
    if let Some(spec) = req.candidate.as_deref() {
        let Some(c) = candidate::parse(spec) else {
            return CommandResult::new(
                "bug",
                CommandStatus::UsageError,
                "--candidate takes a patch file or pr:N",
            )
            .with_code("USAGE");
        };
        let (diff, head) = match &c {
            candidate::Spec::Patch(p) => match candidate::read_patch(&root, path, p, via) {
                Ok(d) => (d, None),
                Err(e) => {
                    return CommandResult::new("bug", CommandStatus::UsageError, &e)
                        .with_code("USAGE");
                }
            },
            candidate::Spec::Pr(n) => match crate::github::pull_diff(
                &req.github
                    .clone()
                    .unwrap_or_else(crate::github::Ctx::from_env),
                &root,
                *n,
            ) {
                Ok((d, head)) => (d, Some(head)),
                Err(e) => {
                    return CommandResult::new(
                        "bug",
                        CommandStatus::Error,
                        &format!("pull request #{n}: {e}"),
                    )
                    .with_code("GITHUB-FAILED");
                }
            },
        };
        let policy = crate::tools::policy::Policy::new(&[], &[]).rules(format.rules());
        let touched = match candidate::touched(&repo_prefix(&root).0, &diff) {
            Ok(t) => t,
            Err(r) => {
                return refuse(&BugRefusal::new(
                    "BUG-CANDIDATE-APPLY",
                    format!(
                        "the candidate cannot be read as a patch: {}",
                        r.paths.join("; ")
                    ),
                ));
            }
        };
        let judged = candidate::judge_paths(&touched, &policy, &[], &[]);
        if !judged.is_empty() {
            return refuse(&BugRefusal::new(
                "BUG-CANDIDATE-JUDGE",
                format!(
                    "the candidate touches what judges it: {}",
                    judged.join(", ")
                ),
            ));
        }
        candidate = Some(candidate::record(&c, &diff, head));
        extra.candidate_patch = Some(diff);
    }
    let mut issue_ref = None;
    if let Some(n) = req.issue {
        let repo = crate::github::slug_of(&root);
        match crate::github::issue(
            &req.github
                .clone()
                .unwrap_or_else(crate::github::Ctx::from_env),
            &root,
            &n.to_string(),
        ) {
            Ok(i) => {
                let text = format!("{}\n{}", i.title, i.body);
                issue_ref = Some(speccoder_core::bug::IssueRef {
                    number: n,
                    repo,
                    context_digest: Some(sha256_hex(text.as_bytes())),
                });
                extra.issue = Some(state::IssueText {
                    title: i.title,
                    body: i.body,
                });
            }
            // An issue never read is not the bug's origin, and its number
            // must never close it: nothing is recorded.
            Err(e) => {
                return CommandResult::new(
                    "bug",
                    CommandStatus::Error,
                    &format!(
                        "issue #{n} could not be read ({e}); nothing is recorded — check the number, or record the bug without --issue"
                    ),
                )
                .with_code("GITHUB-FAILED");
            }
        }
    }

    // Record: the format's own step, or the standing unit.
    let (bug, committed) = if home.kind.native() {
        let Some(provider) = provider.clone() else {
            return CommandResult::new(
                "bug",
                CommandStatus::ConfigError,
                "recording in the format's own bug structure runs its skill: a model is needed",
            );
        };
        let Some(step) = format.bug_step() else {
            return refuse(&BugRefusal::new(
                "BUG-NO-UNIT",
                "the format has no bug step",
            ));
        };
        let slug = format.bug_slug(&root, &home, &triad.title());
        let Some((dir, unit)) = format.bug_dir(&home, &slug) else {
            return refuse(&BugRefusal::new("BUG-NO-UNIT", "no bug directory"));
        };
        let ran = crate::hosting::run_scoped(
            &root,
            step.skill,
            &(step.arguments)(&triad, &slug),
            state,
            provider,
            secret,
            options,
            step.rules,
        );
        let changes: Vec<String> = match ran.data() {
            Some(CommandData::Skill(s)) => s.changes.iter().map(|(p, _)| p.clone()).collect(),
            _ => Vec::new(),
        };
        if ran.status() != CommandStatus::Ok {
            return CommandResult::carried("bug", &ran);
        }
        let (_, prefix) = repo_prefix(&root);
        let own = format!("{prefix}{dir}/");
        let outside: Vec<&String> = changes.iter().filter(|p| !p.starts_with(&own)).collect();
        if !outside.is_empty() {
            return CommandResult::new(
                "bug",
                CommandStatus::NeedsHuman,
                &format!(
                    "the format's bug step wrote outside its bug's directory ({}): look before anything is committed",
                    outside.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ),
            )
            .with_code("BUG-ASSESSMENT-DRIFT");
        }
        let read = match format.read_bug(&root, &home, &slug) {
            Ok(r) => r,
            Err(r) => {
                return refuse(&BugRefusal::new(
                    r.code,
                    format!("{} (the format's bug step: {})", r.message, ran.message()),
                ));
            }
        };
        let missing = drift(&triad, &read);
        if !missing.is_empty() {
            return refuse(&BugRefusal::new(
                "BUG-ASSESSMENT-DRIFT",
                format!(
                    "{} does not carry exactly your words: {}; speccoder bug --slug {slug} once it does",
                    read.file,
                    missing.join("; ")
                ),
            ));
        }
        let title = triad.title();
        let committed = crate::hosting::commit_exactly(&root, &changes, &format!("bug: {title}"));
        if committed.status() != CommandStatus::Ok {
            return CommandResult::carried("bug", &committed);
        }
        let task = format!("bug:{slug}");
        let home = BugHome {
            location: dir,
            ..home
        };
        (
            new_report(
                home,
                unit,
                task.clone(),
                task,
                title,
                triad,
                config.reproduce_tries,
                issue_ref,
            ),
            committed.message().to_owned(),
        )
    } else {
        // Uncommitted changes in the unit's files would be committed as the bug's.
        let unit_name = home
            .location
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        if root.join(&home.location).is_dir() {
            let dirty = crate::hosting::uncommitted(&root, Some(&unit_name));
            if !dirty.is_empty() {
                return CommandResult::new(
                    "bug",
                    CommandStatus::GitPrecondition,
                    &format!(
                        "{} has uncommitted changes ({}): commit or discard them first; a bug commits exactly its own lines",
                        home.location,
                        dirty.join(", ")
                    ),
                )
                .with_code("UNCOMMITTED-INTENT");
            }
        }
        let rec = match format.record_bug(&root, &home, &triad) {
            Ok(r) => r,
            Err(r) => return refuse(&r),
        };
        // Read back as written: exactly the person's words, or nothing is
        // committed (constitution I/VII).
        match format.read_bug(&root, &home, &rec.requirement) {
            Ok(read) if drift(&triad, &read).is_empty() => {}
            Ok(read) => {
                return refuse(&BugRefusal::new(
                    "BUG-ASSESSMENT-DRIFT",
                    format!(
                        "{} does not read back exactly your words ({}); nothing is committed — look at the uncommitted lines",
                        read.file,
                        drift(&triad, &read).join("; ")
                    ),
                ));
            }
            Err(r) => {
                return refuse(&BugRefusal::new(
                    "BUG-ASSESSMENT-DRIFT",
                    format!(
                        "{} ({}); nothing is committed — look at the uncommitted lines",
                        r.message, r.code
                    ),
                ));
            }
        }
        // The person's own words: approved as they wrote them (where the
        // format has requirement identifiers to review).
        if format.requirements_file().is_some() {
            let approved = crate::review::decide(
                &root,
                Some(&rec.unit),
                std::slice::from_ref(&rec.requirement),
                "approve",
                None,
                via,
            );
            if approved.status() != CommandStatus::Ok {
                return CommandResult::carried("bug", &approved);
            }
        }
        // A unit the person never committed before: its new files too.
        let files = {
            let mut f = crate::hosting::uncommitted(&root, Some(&rec.unit));
            if f.is_empty() {
                let (_, prefix) = repo_prefix(&root);
                f = rec.files.iter().map(|p| format!("{prefix}{p}")).collect();
            }
            f
        };
        let committed =
            crate::hosting::commit_exactly(&root, &files, &format!("bug: {}", rec.title));
        if committed.status() != CommandStatus::Ok {
            return CommandResult::carried("bug", &committed);
        }
        (
            new_report(
                home,
                rec.unit,
                rec.requirement,
                rec.task,
                rec.title,
                triad,
                config.reproduce_tries,
                issue_ref,
            ),
            committed.message().to_owned(),
        )
    };
    let mut bug = bug;
    bug.candidate = candidate;
    bug.recorded_commit = Some(committed);
    bug.reproductions = person.iter().map(|(r, _)| r.clone()).collect();
    let files: Vec<(String, String)> = person
        .iter()
        .map(|(r, c)| (r.file.clone(), c.clone()))
        .collect();
    finish_record(
        &root, &plane, bug, files, extra, via, state, provider, secret, req.run, options,
    )
}

/// The refusal of a slug that is not one.
fn invalid_slug(slug: &str) -> CommandResult {
    CommandResult::new(
        "bug",
        CommandStatus::UsageError,
        &format!(
            "`{slug}` is not a bug slug: lowercase letters, digits and `-`, at most {} long",
            speccoder_core::bug::MAX_SLUG
        ),
    )
    .with_code("USAGE")
}

fn repo_prefix(root: &Path) -> (PathBuf, String) {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let top = crate::git::Git::at(root)
        .toplevel()
        .ok()
        .flatten()
        .map_or_else(|| root.to_path_buf(), |t| canonical(&t));
    let prefix = canonical(root)
        .strip_prefix(&top)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let prefix = if prefix.is_empty() {
        prefix
    } else {
        format!("{prefix}/")
    };
    (top, prefix)
}

#[allow(clippy::too_many_arguments)]
fn new_report(
    home: BugHome,
    unit: String,
    requirement: String,
    task: String,
    title: String,
    triad: Triad,
    max_tries: u32,
    issue: Option<speccoder_core::bug::IssueRef>,
) -> BugReport {
    BugReport {
        home,
        unit,
        requirement,
        task,
        title,
        triad,
        stage: Stage::Record,
        outcome: Outcome::Recorded,
        verdict: None,
        code: None,
        reproductions: Vec::new(),
        held_out: Vec::new(),
        held_out_sealed: None,
        held_out_reason: None,
        confirm_digest: None,
        confirmation: None,
        rejections: Vec::new(),
        acceptance: None,
        pass_to_pass: None,
        revert: None,
        special_case: Vec::new(),
        tries: Vec::new(),
        max_tries,
        handoff: None,
        issue,
        candidate: None,
        recorded_commit: None,
        commit: None,
        run_id: None,
    }
}

/// The origin, the state, the event; then, when asked, the run.
#[allow(clippy::too_many_arguments)]
fn finish_record(
    root: &Path,
    plane: &Path,
    mut bug: BugReport,
    person_files: Vec<(String, String)>,
    extra: state::Extra,
    via: Via,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    secret: Option<&str>,
    run: bool,
    options: &RunOptions,
) -> CommandResult {
    let by = extra.by.clone().unwrap_or_default();
    let mut origins = vec![Origin::Bug {
        unit: bug.unit.clone(),
        requirement: bug.requirement.clone(),
        task: bug.task.clone(),
        triad_sha256: bug.triad.digest.clone(),
        by,
        via,
        at: confirm::now(),
    }];
    if let Some(i) = &bug.issue {
        origins.push(Origin::Issue {
            number: i.number,
            repo: i.repo.clone(),
            context_sha256: i.context_digest.clone(),
        });
    }
    // Exactly one origin (FR-002b): one already there for the same words
    // (a bug read in again) stands; any other failure is said.
    let subject = speccoder_core::ladder::subject(&bug.unit, &bug.requirement);
    if let Err(e) = crate::ladder::record_origin(plane, &subject, origins) {
        let same = crate::ladder::of(plane, &subject).iter().any(
            |o| matches!(o, Origin::Bug { triad_sha256, .. } if *triad_sha256 == bug.triad.digest),
        );
        if !same {
            return CommandResult::new(
                "bug",
                CommandStatus::Error,
                &format!(
                    "the bug's origin could not be written ({e}); its words are committed ({}), nothing else is kept",
                    bug.recorded_commit.as_deref().unwrap_or("not yet")
                ),
            );
        }
    }
    bug.stage = Stage::Reproduce;
    let saved = state::save(plane, &bug)
        .and_then(|()| state::save_extra(plane, &bug.unit, &bug.task, &extra))
        .and_then(|()| state::save_repro(plane, &bug.unit, &bug.task, &person_files));
    if let Err(e) = saved {
        return CommandResult::new(
            "bug",
            CommandStatus::Error,
            &format!("the bug could not be kept: {e}"),
        );
    }
    emit(
        options,
        &bug.task,
        BugEvent::Recorded {
            home: bug.home.kind.as_str().to_owned(),
            unit: bug.unit.clone(),
            requirement: bug.requirement.clone(),
            triad_digest: bug.triad.digest.clone(),
            via: via.as_str().to_owned(),
        },
    );
    let said = format!(
        "recorded \"{}\" as {} and {} in {} ({})",
        bug.title, bug.requirement, bug.task, bug.unit, bug.home.reason
    );
    if !run {
        return result(
            CommandStatus::Ok,
            &format!(
                "{said}; reproduce it: speccoder bug run --task {}",
                bug.key()
            ),
            &bug,
        );
    }
    let r = run_bug(root, &bug, state, provider, secret, options);
    let status = r.status();
    let message = format!("{said}; {}", r.message());
    r.reworded(status, &message)
}

/// Runs the bug's task (reproduce, or its confirmed fix) and answers with
/// the bug.
#[must_use]
pub fn run_bug(
    root: &Path,
    bug: &BugReport,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let Some(provider) = provider else {
        return CommandResult::new(
            "bug",
            CommandStatus::ConfigError,
            "running a bug needs a model",
        );
    };
    let ran = if bug.home.kind.native() {
        crate::pipeline::run_native_bug(root, bug, state, provider, secret, options)
    } else {
        crate::run_with_options(
            root,
            Some(&bug.unit),
            &bug.task,
            state,
            provider,
            secret,
            options,
        )
    };
    let plane = crate::worktree::project_state_dir(state, root);
    let now = state::load(&plane, &bug.unit, &bug.task).unwrap_or_else(|| bug.clone());
    let mut r = CommandResult::new("bug", ran.status(), ran.message())
        .with_next(if ran.next().is_empty() {
            handoff::next(&now, now.outcome)
        } else {
            ran.next().to_vec()
        })
        .with_data(CommandData::Bug(Box::new(now.clone())));
    let explicit: Option<String> = if ran.has_explicit_code() {
        ran.code().map(|e| e.name.to_owned())
    } else {
        None
    };
    let code: Option<String> = explicit.or_else(|| now.code.clone());
    if let Some(code) = code {
        r = r.with_code(&code);
    }
    r
}

/// `speccoder bug run --task <unit>/<task>` or `--slug S`, with the
/// configured model.
#[must_use]
pub fn run(
    path: &Path,
    slug: Option<&str>,
    task: Option<&str>,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    if slug.is_none()
        && let Err(r) = precheck(path, task, state)
    {
        return *r;
    }
    with_model(path, state, |state, provider, key| {
        run_with(path, slug, task, state, provider, Some(key), options)
    })
}

/// [`run()`] against `state` with `provider`.
#[must_use]
pub fn run_with(
    path: &Path,
    slug: Option<&str>,
    task: Option<&str>,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let (root, format) = match root_of(path) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    if let Some(s) = slug.filter(|s| !speccoder_core::bug::valid_slug(s)) {
        return invalid_slug(s);
    }
    let plane = crate::worktree::project_state_dir(state, &root);
    let found = match (slug, task) {
        (Some(s), _) => format
            .bug_unit(&root)
            .ok()
            .and_then(|h| format.bug_dir(&h, s))
            .and_then(|(_, unit)| state::load(&plane, &unit, &format!("bug:{s}"))),
        (None, Some(t)) => state::find(&plane, t),
        (None, None) => None,
    };
    match (found, slug) {
        (Some(bug), _) => run_bug(&root, &bug, state, Some(provider), secret, options),
        // A bug the person assessed with the format's own step: read it in.
        (None, Some(s)) => bug_with(
            path,
            &Request {
                slug: Some(s.to_owned()),
                run: true,
                ..Request::default()
            },
            Via::Terminal,
            state,
            Some(provider),
            secret,
            options,
        ),
        (None, None) => CommandResult::new(
            "bug",
            CommandStatus::NotFound,
            &format!(
                "no bug {}",
                task.unwrap_or("named: pass --task <unit>/<task> or --slug <slug>")
            ),
        ),
    }
}

/// `speccoder bug confirm <unit>/<task> --digest D` (or `--reject`):
/// the person's one decision on the reproductions shown; with `run`, the
/// fix (or the next try) runs at once.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn confirm(
    path: &Path,
    key: &str,
    digest: &str,
    reject: Option<&str>,
    run: bool,
    via: Via,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    // What can be refused is refused before a model is opened.
    if let Some(r) = precheck(path, Some(key), state).err().or_else(|| {
        (reject.is_some_and(|r| r.trim().is_empty())).then(|| {
            Box::new(
                CommandResult::new(
                    "bug",
                    CommandStatus::UsageError,
                    "a reason is needed: the author's next try is told it",
                )
                .with_code("USAGE"),
            )
        })
    }) {
        return *r;
    }
    if run {
        return with_model(path, state, |state, provider, k| {
            confirm_with(
                path,
                key,
                digest,
                reject,
                via,
                state,
                Some(provider),
                Some(k),
                options,
            )
        });
    }
    match state_or_env(state) {
        Ok(state) => confirm_with(path, key, digest, reject, via, &state, None, None, options),
        Err(r) => *r,
    }
}

/// [`confirm()`] against `state`; `provider` runs what follows (none: only
/// the decision is recorded).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn confirm_with(
    path: &Path,
    key: &str,
    digest: &str,
    reject: Option<&str>,
    via: Via,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let (root, format) = match root_of(path) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let plane = crate::worktree::project_state_dir(state, &root);
    let Some(mut bug) = state::find(&plane, key) else {
        return CommandResult::new("bug", CommandStatus::NotFound, &format!("no bug {key}"));
    };
    if bug.outcome != Outcome::AwaitingConfirmation {
        return result(
            CommandStatus::UsageError,
            &format!(
                "{key} is not waiting for a confirmation ({})",
                bug.outcome.as_str()
            ),
            &bug,
        );
    }
    let shown = bug.confirm_digest.clone().unwrap_or_default();
    if digest.trim() != shown {
        return CommandResult::new(
            "bug",
            CommandStatus::UsageError,
            &format!("the digest does not match the reproductions shown ({shown}): look at them again (`speccoder bug confirm {key}` in a terminal shows them)"),
        )
        .with_code("USAGE");
    }
    // The triad as it reads now: a reworded one lapses what was shown.
    match crate::pipeline::bug_triad_now(format, &root, &bug) {
        Ok(now) if now.digest != bug.triad.digest => {
            let _ = confirm::lapse(&plane, &mut bug, now);
            return result(
                CommandStatus::NeedsHuman,
                "the triad was reworded since the reproductions were written: they lapse; reproduce again",
                &bug,
            );
        }
        Err(r) => return refuse(&r),
        Ok(_) => {}
    }
    let by = crate::approvals::who_in(&root);
    if let Some(reason) = reject {
        let reason = reason.trim();
        if reason.is_empty() {
            return CommandResult::new(
                "bug",
                CommandStatus::UsageError,
                "a reason is needed: the author's next try is told it",
            )
            .with_code("USAGE");
        }
        if let Err(e) = crate::pipeline::bug_reject(&plane, &mut bug, reason, via) {
            return CommandResult::new(
                "bug",
                CommandStatus::Error,
                &format!("the rejection could not be kept: {e}; nothing was rejected"),
            );
        }
        emit(
            options,
            &bug.task,
            BugEvent::ConfirmRejected {
                digest: shown,
                reason: reason.to_owned(),
            },
        );
        return match provider {
            Some(p) => run_bug(&root, &bug, state, Some(p), secret, options),
            None => result(
                CommandStatus::Ok,
                &format!(
                    "rejected; the author is told why on the next try: speccoder bug run --task {key}"
                ),
                &bug,
            ),
        };
    }
    match confirm::freeze(
        &plane,
        &mut bug,
        &by,
        via,
        speccoder_core::bug::Basis::Shown,
    ) {
        Ok(_) => {
            emit(
                options,
                &bug.task,
                BugEvent::Confirmed {
                    digest: shown,
                    by: by.clone(),
                    via: via.as_str().to_owned(),
                    basis: "shown".to_owned(),
                },
            );
            match provider {
                Some(p) => run_bug(&root, &bug, state, Some(p), secret, options),
                None => result(
                    CommandStatus::Ok,
                    &format!("confirmed and frozen; fix it: speccoder bug run --task {key}"),
                    &bug,
                ),
            }
        }
        Err(e) => CommandResult::new(
            "bug",
            CommandStatus::Error,
            &format!("the confirmation could not be recorded: {e}"),
        ),
    }
}

/// The bug named `key` exists (no model needed to say it does not).
fn precheck(
    path: &Path,
    key: Option<&str>,
    state: Option<&Path>,
) -> Result<(), Box<CommandResult>> {
    let (root, _) = root_of(path)?;
    let Some(key) = key else {
        return Err(Box::new(
            CommandResult::new(
                "bug",
                CommandStatus::UsageError,
                "name the bug: --task <unit>/<task> or --slug <slug>",
            )
            .with_code("USAGE"),
        ));
    };
    let state = state_or_env(state)?;
    let plane = crate::worktree::project_state_dir(&state, &root);
    if state::find(&plane, key).is_none() {
        return Err(Box::new(CommandResult::new(
            "bug",
            CommandStatus::NotFound,
            &format!("no bug {key}"),
        )));
    }
    Ok(())
}

/// The bug named `key`, as speccoder keeps it.
#[must_use]
pub fn show(path: &Path, key: &str) -> CommandResult {
    let Ok((root, _)) = root_of(path) else {
        return CommandResult::new("bug", CommandStatus::NotAProject, "not a project");
    };
    let Some(state) = crate::worktree::state_dir() else {
        return CommandResult::new("bug", CommandStatus::ConfigError, "no state directory")
            .with_code("NO-STATE-DIR");
    };
    let plane = crate::worktree::project_state_dir(&state, &root);
    match state::find(&plane, key) {
        Some(b) => result(
            CommandStatus::Ok,
            &format!("{key}: {}", b.outcome.as_str()),
            &b,
        ),
        None => CommandResult::new("bug", CommandStatus::NotFound, &format!("no bug {key}")),
    }
}

/// What a person is shown before confirming `key` in a terminal.
#[must_use]
pub fn shown(state: &Path, root: &Path, key: &str) -> Option<crate::approvals::ConfirmRequest> {
    let plane = crate::worktree::project_state_dir(state, root);
    let bug = state::find(&plane, key)?;
    let digest = bug.confirm_digest.clone()?;
    let paths: Vec<String> = bug.reproductions.iter().map(|r| r.file.clone()).collect();
    let files = state::repro(&plane, &bug.unit, &bug.task, &paths);
    Some(crate::approvals::ConfirmRequest {
        bug: bug.key(),
        title: bug.title.clone(),
        digest,
        reproductions: bug
            .reproductions
            .iter()
            .map(|r| crate::approvals::ShownReproduction {
                statement: r.statement.clone(),
                words: bug
                    .triad
                    .statement(&r.statement)
                    .map(|s| s.text.clone())
                    .unwrap_or_default(),
                file: r.file.clone(),
                test: r.test.clone(),
                argv: r.argv.clone(),
                excerpt: String::new(),
                content: files
                    .iter()
                    .find(|(p, _)| *p == r.file)
                    .map(|(_, c)| c.clone())
                    .unwrap_or_default(),
            })
            .collect(),
        warning: held_out_warning(&bug),
    })
}

/// What a person confirming `bug` must know first: no held-out variant
/// could be kept, so the fix cannot be verified.
#[must_use]
pub fn held_out_warning(bug: &BugReport) -> Option<String> {
    bug.held_out_reason.as_ref().map(|why| {
        format!("no held-out variant could be kept ({why}): the fix cannot be verified until one is — confirming now only leads to a failed verdict")
    })
}
