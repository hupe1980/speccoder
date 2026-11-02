//! A story's oracle in a run: its acceptance tests, written once through
//! the product and committed on the unit branch (`Oracle: US1`) before the
//! story's first code task implements anything; the states of its
//! scenarios on a tree; and the progress each task makes on them
//! ([`speccoder_core::story::progress`]).

use std::collections::BTreeMap;
use std::path::Path;

use speccoder_core::spec::ScenarioRef;
use speccoder_core::story::{Progress, ScenarioState};
use speccoder_core::verify::{CheckStatus, Verdict};

use crate::oracle::FrozenTest;
use crate::sandbox;

/// The trailer naming the story an oracle commit is for.
pub(crate) const ORACLE: &str = "Oracle";

/// A story's oracle as a task is judged by it.
#[derive(Debug, Clone)]
pub(crate) struct StoryOracle {
    /// The story (`US1`; for OpenSpec the change).
    pub(crate) story: String,
    /// Its scenarios, in the story's order.
    pub(crate) scenarios: Vec<ScenarioRef>,
    /// Each test, with its command and the scenarios it cites.
    pub(crate) tests: Vec<FrozenTest>,
    /// What each scenario must hold (`given → must hold`), by id.
    pub(crate) contracts: BTreeMap<String, String>,
    /// The `Oracle:` commit (empty until committed).
    pub(crate) commit: String,
    /// `speccoder run`, or the person who froze it at review.
    pub(crate) decided_by: String,
    /// Its sealed held-out set, run at the story's checkpoint only.
    pub(crate) held_out: Option<speccoder_core::acceptance::SealedRef>,
    /// What became of the held-out set when the oracle was written.
    pub(crate) held_out_summary: Option<speccoder_core::oracle::HeldOutSummary>,
    /// Frozen at review by a person: the record and its manifest digest
    /// (the ticks name them).
    pub(crate) reviewed: Option<(speccoder_core::acceptance::PreFrozenAcceptance, String)>,
    /// Each scenario's state when the oracle was written.
    pub(crate) at_oracle: BTreeMap<String, ScenarioState>,
    /// What writing it now cost (its author calls); none when it was there.
    pub(crate) usage: Option<serde_json::Value>,
    /// The scenarios its author declared untestable (shown; never blocking
    /// the checkpoint), with why.
    pub(crate) untestable: BTreeMap<String, String>,
}

impl StoryOracle {
    /// Every scenario was green before any work of the story: built
    /// earlier (a Setup task, before the feature).
    pub(crate) fn green_from_the_start(&self) -> bool {
        !self.at_oracle.is_empty()
            && self
                .scenarios
                .iter()
                .filter(|s| !self.untestable.contains_key(&s.id))
                .all(|s| self.at_oracle.get(&s.id) == Some(&ScenarioState::Green))
    }
}

impl StoryOracle {
    /// The scenario ids in order.
    pub(crate) fn order(&self) -> Vec<String> {
        self.scenarios.iter().map(|s| s.id.clone()).collect()
    }
}

/// The latest `Oracle: <story>` commit on `HEAD` of `wt_root` — written
/// only by speccoder's own commit (trailers of a model's commit are never
/// read: the implementer never commits).
pub(crate) fn commit_of(wt_root: &Path, story: &str) -> Result<Option<String>, String> {
    let out = crate::git::Git::at(wt_root)
        .line(&[
            "log",
            "--format=%H",
            "-n1",
            &format!("--grep=^{ORACLE}: {story}$"),
            "HEAD",
        ])
        .map_err(|e| e.0)?;
    Ok(Some(out).filter(|s| !s.is_empty()))
}

/// Whether a story with open tasks has its oracle on the unit branch at
/// `wt_root`: a build its red tests break (a symbol a later task adds, in a
/// compiled language) is that story's, not a broken check. Once its tasks
/// are done, a broken check is a broken check again.
pub(crate) fn open_oracle(
    wt_root: &Path,
    snapshot: &speccoder_core::snapshot::ProjectSnapshot,
    format: &dyn crate::format::Adapter,
) -> bool {
    let whole_unit = format.kind() == speccoder_core::spec::FormatKind::Openspec;
    let Some(f) = snapshot.feature.as_ref() else {
        return false;
    };
    let tasks = f
        .tasks
        .as_ref()
        .map(|t| t.tasks.as_slice())
        .unwrap_or_default();
    let mut open: Vec<String> = if whole_unit {
        if tasks.iter().any(|t| !t.done) {
            vec![f.feature.name.clone()]
        } else {
            Vec::new()
        }
    } else {
        // A story's own open tasks, and an open gap judged by its oracle.
        let reqs = format.oracle_requirements(snapshot);
        tasks
            .iter()
            .filter(|t| !t.done)
            .filter_map(|t| {
                t.story.clone().or_else(|| {
                    super::acceptance::linked_story(
                        t,
                        &format.scenarios(snapshot, &t.id),
                        &f.feature.dir,
                        &format.rules().judged,
                        &reqs,
                    )
                    .map(|(r, _)| r.key)
                })
            })
            .collect()
    };
    open.sort();
    open.dedup();
    open.iter()
        .any(|story| matches!(commit_of(wt_root, story), Ok(Some(_))))
}

/// Whether `task`'s story still has its oracle to write (or amend) on the
/// unit's worktree `wt_root`: such a task runs alone, never in a parallel
/// lane — the oracle is written once, on the unit's branch, then read by
/// every lane.
pub(crate) fn oracle_pending(
    wt_root: &Path,
    plane_dir: &Path,
    format: &dyn crate::format::Adapter,
    snapshot: &speccoder_core::snapshot::ProjectSnapshot,
    task: &str,
) -> bool {
    let Some(f) = snapshot.feature.as_ref() else {
        return false;
    };
    let Some(t) = f
        .tasks
        .as_ref()
        .and_then(|t| t.tasks.iter().find(|t| t.id == task))
    else {
        return false;
    };
    let key = match t.story.clone() {
        Some(l) => l,
        None if format.kind() == speccoder_core::spec::FormatKind::Openspec => {
            f.feature.name.clone()
        }
        // A gap judged by one story's oracle writes it when it is missing.
        None => match super::acceptance::linked_story(
            t,
            &format.scenarios(snapshot, task),
            &f.feature.dir,
            &format.rules().judged,
            &format.oracle_requirements(snapshot),
        ) {
            Some((r, _)) => r.key,
            None => return false,
        },
    };
    if format.scenarios(snapshot, task).is_empty() {
        return false;
    }
    !matches!(commit_of(wt_root, &key), Ok(Some(_)))
        || !crate::acceptance_at::rulings_for(plane_dir, &f.feature.name, &key)
            .unwrap_or_default()
            .is_empty()
}

/// Everything the oracle step works with.
pub(crate) struct Ensure<'a> {
    /// The story (`US1`; the unit for OpenSpec).
    pub(crate) key: &'a str,
    /// Its scenarios, in the story's order.
    pub(crate) scenarios: &'a [ScenarioRef],
    pub(crate) feature: &'a str,
    /// The project inside the task's worktree, and the worktree.
    pub(crate) project: &'a Path,
    pub(crate) wt_root: &'a Path,
    /// speccoder's state directory, and the project's.
    pub(crate) state: &'a Path,
    pub(crate) plane_dir: &'a Path,
    pub(crate) runner: &'a sandbox::Runner,
    /// The run's runtime and plane: the author's calls are the run's.
    pub(crate) rt: &'a tokio::runtime::Runtime,
    pub(crate) plane: &'a crate::substrate::Plane,
    pub(crate) options: &'a crate::RunOptions,
    pub(crate) format: &'a dyn crate::format::Adapter,
    pub(crate) snapshot: &'a speccoder_core::snapshot::ProjectSnapshot,
    /// `--fresh-acceptance`: the story's tests are written anew.
    pub(crate) fresh: bool,
    /// An interrupted attempt resumes: the oracle is not amended under it.
    pub(crate) resuming: bool,
}

/// `speccoder story check <story>`: the story's oracle written now (when
/// it has none) and run once on the feature's branch — a story ticked
/// before its oracle is then held, or not, scenario by scenario.
#[must_use]
pub fn check_story(
    path: &Path,
    feature: Option<&str>,
    story: &str,
    options: &crate::RunOptions,
) -> speccoder_core::CommandResult {
    super::feature::with_driver("story", path, feature, |state, provider, _| {
        check_with(path, feature, story, state, provider, options)
    })
}

/// [`check_story`] against `state`, with `provider` for the author's call.
#[must_use]
pub fn check_with(
    path: &Path,
    feature: Option<&str>,
    story: &str,
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    options: &crate::RunOptions,
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus};
    let c = match checked(path, feature, story, state, provider, options) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let (now, scenarios, finished, wrote) = (&c.states, &c.scenarios, c.finished, c.wrote);
    let red: Vec<&str> = scenarios
        .iter()
        .filter(|sc| now.get(&sc.id) == Some(&ScenarioState::Red))
        .map(|sc| sc.id.as_str())
        .collect();
    let green = scenarios
        .iter()
        .filter(|sc| now.get(&sc.id) == Some(&ScenarioState::Green))
        .count();
    let how = if wrote {
        "its oracle written and run"
    } else {
        "its oracle run again"
    };
    let message = if red.is_empty() {
        format!(
            "{story}: {how} — {green} of {} scenario(s) hold",
            scenarios.len()
        )
    } else {
        format!(
            "{story}: {how} — {green} of {} scenario(s) hold; {} {}",
            scenarios.len(),
            red.join(", "),
            if finished {
                "not held by the code"
            } else {
                "still to turn green"
            }
        )
    };
    let f = feature.map_or_else(String::new, |f| format!(" --feature {f}"));
    let step = |command: String, why: &str| speccoder_core::next::NextStep {
        command,
        why: why.to_owned(),
    };
    let next = if red.is_empty() || !finished {
        vec![step(
            format!("speccoder report{f}"),
            "each story on its oracle",
        )]
    } else {
        // A test written after its code may be the wrong one: either way on.
        let mut next = vec![step(
            format!("speccoder converge{f}"),
            "append the tasks that make the code hold them",
        )];
        if let Some(last) = &c.last {
            next.push(step(
                format!(
                    "speccoder task rule {last}{f} --tests-wrong --scenario {} --reason \"…\"",
                    red[0]
                ),
                "when a test asserts what the spec does not say",
            ));
        }
        next
    };
    CommandResult::new("story", CommandStatus::Ok, &message).with_next(next)
}

/// A story's oracle on the feature's branch: ensured (written when it has
/// none) and run once.
pub(crate) struct Checked {
    /// Each scenario's state now.
    pub(crate) states: BTreeMap<String, ScenarioState>,
    /// The scenarios its author called untestable.
    pub(crate) untestable: Vec<String>,
    /// The story's scenarios.
    pub(crate) scenarios: Vec<speccoder_core::spec::ScenarioRef>,
    /// Every task of the story is done.
    pub(crate) finished: bool,
    /// The story's last task.
    pub(crate) last: Option<String>,
    /// The oracle was written now.
    pub(crate) wrote: bool,
    /// What writing it cost (none when it was there).
    pub(crate) usage: Option<serde_json::Value>,
}

/// Ensures `story`'s oracle on the feature's branch and runs it.
pub(crate) fn checked(
    path: &Path,
    feature: Option<&str>,
    story: &str,
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    options: &crate::RunOptions,
) -> Result<Checked, Box<speccoder_core::CommandResult>> {
    use speccoder_core::{CommandData, CommandResult, CommandStatus};
    let refuse = |status, m: String| CommandResult::new("story", status, &m);
    let s = super::setup(path, feature, state, "story")?;
    let Some(name) = s.snapshot.feature.as_ref().map(|f| f.feature.name.clone()) else {
        return Err(Box::new(refuse(
            CommandStatus::SelectionNeeded,
            "no feature selected; pass --feature".to_owned(),
        )));
    };
    // The feature's branch: where its ticks are, never the checkout.
    let wt = super::workspace(path, Some(&name), state, options);
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return Err(Box::new(CommandResult::carried("story", &wt)));
    };
    if wt.status() != CommandStatus::Ok {
        return Err(Box::new(CommandResult::carried("story", &wt)));
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return Err(Box::new(refuse(
            CommandStatus::Error,
            "worktree without a path".to_owned(),
        )));
    };
    let (project, wt_root) = (
        std::path::PathBuf::from(project_dir),
        std::path::PathBuf::from(wt_path),
    );
    let format = crate::format::at(&project);
    let Some(snap) = format.inspect(&project, Some(&name)) else {
        return Err(Box::new(refuse(
            CommandStatus::NotAProject,
            "the feature's branch holds no spec".to_owned(),
        )));
    };
    let tasks: Vec<&speccoder_core::snapshot::Task> = snap
        .feature
        .as_ref()
        .and_then(|f| f.tasks.as_ref())
        .map(|ts| ts.tasks.iter().collect())
        .unwrap_or_default();
    let whole = format.kind() == speccoder_core::spec::FormatKind::Openspec && story == name;
    let members: Vec<&&speccoder_core::snapshot::Task> = tasks
        .iter()
        .filter(|t| whole || t.story.as_deref() == Some(story))
        .collect();
    let Some(first) = members.first() else {
        let mut known: Vec<&str> = tasks.iter().filter_map(|t| t.story.as_deref()).collect();
        known.sort_unstable();
        known.dedup();
        return Err(Box::new(refuse(
            CommandStatus::NotFound,
            format!("{story} is no story of {name} ({})", known.join(", ")),
        )));
    };
    let scenarios = format.scenarios(&snap, &first.id);
    if scenarios.is_empty() {
        return Err(Box::new(refuse(
            CommandStatus::NotFound,
            format!("{story} has no acceptance scenario to check"),
        )));
    }
    let (runner, _) = match sandbox::runner_at(&wt_root) {
        Ok(r) => r,
        Err(e) => return Err(Box::new(refuse(CommandStatus::ConfigError, e))),
    };
    let rt = match super::tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return Err(Box::new(refuse(CommandStatus::RunFailed, e))),
    };
    let plane = match rt.block_on(async {
        crate::substrate::Plane::open_with(
            &s.plane_dir,
            &s.model,
            std::sync::Arc::clone(provider),
            None,
            crate::substrate::Hooks {
                format: Some(format),
                progress: options.progress.clone(),
                ..crate::substrate::Hooks::default()
            },
        )
    }) {
        Ok(p) => p,
        Err(e) => return Err(Box::new(refuse(CommandStatus::RunFailed, e.0))),
    };
    let e = Ensure {
        key: story,
        scenarios: &scenarios,
        feature: &name,
        project: &project,
        wt_root: &wt_root,
        state,
        plane_dir: &s.plane_dir,
        runner: &runner,
        rt: &rt,
        plane: &plane,
        options,
        format,
        snapshot: &snap,
        fresh: false,
        resuming: false,
    };
    let (oracle, wrote, _) = ensure(&e)?;
    let states = states(&project, &oracle, &runner);
    Ok(Checked {
        states,
        untestable: oracle.untestable.keys().cloned().collect(),
        scenarios,
        finished: members.iter().all(|t| t.done),
        last: members.last().map(|t| t.id.clone()),
        wrote,
        usage: oracle.usage.clone(),
    })
}

fn stop(code: &str, message: String) -> Box<speccoder_core::CommandResult> {
    Box::new(
        speccoder_core::CommandResult::new(
            "run",
            speccoder_core::CommandStatus::NeedsHuman,
            &message,
        )
        .with_code(code),
    )
}

/// The oracle could not be had: writing the story's tests again is the way on.
fn unwritten(story: &str, message: String) -> Box<speccoder_core::CommandResult> {
    let mut res = stop("ORACLE-UNWRITTEN", message);
    *res = res.with_fix(vec![format!("speccoder review tests {story} --write")]);
    res
}

/// The story's oracle for a task: the one committed on the branch; else
/// the one frozen at review, or — nobody having reviewed it — one the run
/// writes now; either committed before the task implements anything.
/// `true` when it was committed by this call (the tree changed); the
/// notices to show (a lapsed or absent review oracle).
pub(crate) fn ensure(
    e: &Ensure<'_>,
) -> Result<(StoryOracle, bool, Vec<String>), Box<speccoder_core::CommandResult>> {
    let mut notices = Vec::new();
    // The sealed held-out set beside the frozen record.
    // A frozen record changed since it was written is never read as "none".
    if let Err(t) = crate::pre_frozen::load(e.plane_dir, e.feature, e.key) {
        return Err(stop(
            "ORACLE-TAMPERED",
            format!(
                "{}'s frozen oracle changed since it was written ({}): the task stops for a person, nothing is ticked",
                e.key, t.0
            ),
        ));
    }
    let sealed = || {
        crate::pre_frozen::load(e.plane_dir, e.feature, e.key)
            .ok()
            .flatten()
            .and_then(|(r, _)| r.held_out_sealed)
    };
    let oracle = |tests: Vec<FrozenTest>, contracts, commit: String, by: String| StoryOracle {
        story: e.key.to_owned(),
        scenarios: e.scenarios.to_vec(),
        tests,
        contracts,
        commit,
        decided_by: by,
        held_out: sealed(),
        held_out_summary: crate::oracle::load(e.plane_dir, e.feature, e.key)
            .ok()
            .flatten()
            .map(|(d, _)| d.held_out),
        reviewed: None,
        at_oracle: BTreeMap::new(),
        usage: None,
        untestable: BTreeMap::new(),
    };
    if let Some(commit) = commit_of(e.wt_root, e.key).map_err(|m| unwritten(e.key, m))? {
        // An oracle of no test: the author found nothing testable.
        if tests_of(e.wt_root, &commit) == Some(0) {
            return Ok((
                oracle(
                    Vec::new(),
                    BTreeMap::new(),
                    commit,
                    "speccoder run".to_owned(),
                ),
                false,
                notices,
            ));
        }
        let Some(frozen) = crate::oracle::load_frozen(e.plane_dir, e.feature, e.key) else {
            return Err(unwritten(
                e.key,
                format!(
                    "{}'s oracle ({}) was written on another machine: the commands that run its tests are kept there",
                    e.key,
                    commit.get(..7).unwrap_or(&commit)
                ),
            ));
        };
        let w = written(e.wt_root, "HEAD", e.key);
        let (contracts, by, at, untestable) = w.map_or_else(
            || {
                (
                    BTreeMap::new(),
                    "speccoder run".to_owned(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                )
            },
            |w| (w.contracts, w.by, w.states, w.untestable),
        );
        let mut current = oracle(frozen.tests, contracts, commit, by);
        let gone = changed(e.project, &current);
        if !gone.is_empty() {
            return Err(stop(
                "ORACLE-TAMPERED",
                format!(
                    "{}'s oracle tests on the branch are not the frozen ones: {} — the task stops for a person, nothing is ticked",
                    e.key,
                    gone.join(", ")
                ),
            ));
        }
        // As the oracle was first written: amendments change tests, not when
        // the story's scenarios first held.
        current.at_oracle = first_states(e.wt_root, e.key).unwrap_or(at);
        current.untestable = untestable;
        current.reviewed = crate::pre_frozen::load(e.plane_dir, e.feature, e.key)
            .ok()
            .flatten()
            .filter(|(r, _)| r.producer == speccoder_core::acceptance::Producer::Review)
            .map(|(r, _)| (r, frozen.manifest_sha256.clone()));
        // A person ruled scenarios' tests wrong: those are written again,
        // nothing else of the oracle.
        let mut rulings = crate::acceptance_at::rulings_for(e.plane_dir, e.feature, e.key)
            .map_err(|m| {
                stop(
                    "ORACLE-TAMPERED",
                    format!("the rulings cannot be read: {m}"),
                )
            })?;
        // Fresh tests asked for: every scenario is written again.
        if e.fresh {
            rulings = e
                .scenarios
                .iter()
                .map(|s| crate::acceptance_at::Ruling {
                    feature: e.feature.to_owned(),
                    scenario: Some(s.id.clone()),
                    story: e.key.to_owned(),
                    task: String::new(),
                    reason: "fresh tests were asked for".to_owned(),
                    by: "you".to_owned(),
                    via: speccoder_core::actions::Via::Terminal,
                    at: 0,
                })
                .collect();
        }
        if rulings.is_empty() {
            return Ok((current, false, notices));
        }
        // An interrupted attempt resumes against the oracle it began with;
        // the ruling waits for the next fresh one.
        if e.resuming {
            notices.push(format!(
                "a ruling on {}'s tests waits: the interrupted attempt resumes first",
                e.key
            ));
            return Ok((current, false, notices));
        }
        return amend(e, current, &rulings).map(|o| (o, true, notices));
    }
    // Frozen at review, current: its tests, no author call.
    let reviewed = match crate::oracle::for_task(
        e.plane_dir,
        e.project,
        e.wt_root,
        e.format,
        e.snapshot,
        e.scenarios,
    ) {
        Some(crate::oracle::TaskReuse::Ready {
            record,
            files,
            manifest,
            ..
        }) => {
            notices.push(format!(
                "acceptance tests frozen at review by {} ({}) for {}: no author call",
                record.decided.by, record.decided.via, record.requirement
            ));
            let tests = crate::oracle::load_frozen(e.plane_dir, e.feature, e.key)
                .map(|f| f.tests)
                .unwrap_or_default();
            let contracts = crate::oracle::load(e.plane_dir, e.feature, e.key)
                .ok()
                .flatten()
                .map(|(d, _)| contracts_of(&d.expectations.sample1))
                .unwrap_or_default();
            (!tests.is_empty()).then(|| {
                let by = record.decided.by.clone();
                (tests, files, contracts, by, Some((*record, manifest)))
            })
        }
        Some(crate::oracle::TaskReuse::Lapsed {
            req,
            frozen,
            current,
        }) => {
            if crate::oracle::note_lapse(e.plane_dir, e.feature, &req, &frozen)
                && let Some(p) = &e.options.progress
            {
                p.event(&crate::progress::Event::Oracle {
                    step: crate::progress::OracleEvent::Lapsed {
                        unit: e.feature.to_owned(),
                        requirement: req.clone(),
                        frozen_wording_sha256: frozen,
                        current_wording_sha256: current,
                    },
                });
            }
            notices.push(format!(
                "{req}'s wording changed since its tests were frozen at review: the frozen oracle lapsed — authoring"
            ));
            None
        }
        Some(crate::oracle::TaskReuse::Elsewhere { req }) => {
            notices.push(format!("{req}: frozen oracle not present here — authoring"));
            None
        }
        Some(crate::oracle::TaskReuse::Tampered(why)) => {
            return Err(stop(
                "ORACLE-TAMPERED",
                format!("{why}: the task stops for a person, nothing is ticked"),
            ));
        }
        Some(crate::oracle::TaskReuse::Unreadable(why)) => {
            return Err(stop(
                "REVIEW-UNREADABLE",
                format!(
                    "{why}: whether its tests are frozen is unknown, so the task stops for a person, nothing is ticked"
                ),
            ));
        }
        _ => None,
    };
    let mut spent = None;
    let (tests, files, contracts, by, reviewed) = match reviewed {
        Some(r) => r,
        None => {
            let w = crate::oracle::write_for_run(
                e.project,
                e.wt_root,
                e.feature,
                e.state,
                e.key,
                e.runner.clone(),
                (e.rt, e.plane),
                (&[], &[]),
                "",
                e.options,
            )?;
            spent.clone_from(&w.usage);
            (
                w.tests,
                w.contents,
                contracts_of(&w.contracts),
                "speccoder run".to_owned(),
                None,
            )
        }
    };
    // A file at a test's path that is not the test (an implementer committed
    // one there) is never overwritten: the oracle would judge by it.
    for (path, content) in &files {
        if let Ok(there) = std::fs::read_to_string(e.project.join(path))
            && there != *content
        {
            return Err(stop(
                "ORACLE-TAMPERED",
                format!(
                    "{path} exists and differs from the confirmed test: the task stops for a person, nothing is ticked"
                ),
            ));
        }
    }
    for (path, content) in &files {
        let full = e.project.join(path);
        let written = full
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&full, content));
        if let Err(err) = written {
            return Err(unwritten(e.key, format!("{path}: {err}")));
        }
    }
    let mut o = oracle(tests, contracts, String::new(), by);
    o.usage = spent;
    o.reviewed = reviewed;
    // What its author could not test, and why: shown, never blocking.
    o.untestable = crate::oracle::load(e.plane_dir, e.feature, e.key)
        .ok()
        .flatten()
        .map(|(d, _)| {
            d.untestable
                .sample1
                .into_iter()
                .filter(|u| e.scenarios.iter().any(|s| s.id == u.scenario))
                .map(|u| (u.scenario, u.why))
                .collect()
        })
        .unwrap_or_default();
    let now = states(e.project, &o, e.runner);
    o.at_oracle = now.clone();
    let message = commit_message(&o, &now, None, None, None);
    // Only the oracle's files (an oracle of no test is still recorded:
    // later tasks ask no author).
    let paths: Vec<String> = files
        .iter()
        .map(|(p, _)| {
            e.project
                .join(p)
                .strip_prefix(e.wt_root)
                .map_or_else(|_| p.clone(), |r| r.to_string_lossy().into_owned())
        })
        .collect();
    o.commit = super::commit::commit_paths(e.wt_root, &message, &paths, true)
        .map_err(|m| unwritten(e.key, format!("the oracle could not be committed: {m}")))?;
    Ok((o, true, notices))
}

/// The oracle with the ruled scenarios' tests written again: every test
/// citing a ruled scenario removed — and so every scenario such a test also
/// cites written again with them — the new ones added, the frozen commands
/// merged, committed (only those files) as an amendment naming the rulings.
/// On any failure the old tests come back from the branch.
fn amend(
    e: &Ensure<'_>,
    mut o: StoryOracle,
    rulings: &[crate::acceptance_at::Ruling],
) -> Result<StoryOracle, Box<speccoder_core::CommandResult>> {
    let mut ruled: Vec<String> = rulings.iter().filter_map(|r| r.scenario.clone()).collect();
    let (old, kept): (Vec<FrozenTest>, Vec<FrozenTest>) = std::mem::take(&mut o.tests)
        .into_iter()
        .partition(|t| t.scenarios.iter().any(|s| ruled.contains(s)));
    for s in old.iter().flat_map(|t| &t.scenarios) {
        if !ruled.contains(s) {
            ruled.push(s.clone());
        }
    }
    let told: Vec<String> = rulings
        .iter()
        .map(|r| {
            format!(
                "{} ruled {}'s tests wrong: {}",
                r.by,
                r.scenario.as_deref().unwrap_or_default(),
                r.reason
            )
        })
        .collect();
    let rel = |path: &str| {
        e.project
            .join(path)
            .strip_prefix(e.wt_root)
            .map_or_else(|_| path.to_owned(), |p| p.to_string_lossy().into_owned())
    };
    let old_paths: Vec<String> = old.iter().map(|t| rel(&t.path)).collect();
    let restore = |r: Box<speccoder_core::CommandResult>| {
        let mut args = vec!["checkout", "HEAD", "--"];
        args.extend(old_paths.iter().map(String::as_str));
        let _ = crate::git::Git::at(e.wt_root).run(&args);
        r
    };
    // Their paths are free for the new ones.
    for t in &old {
        let _ = std::fs::remove_file(e.project.join(&t.path));
    }
    let w = crate::oracle::write_for_run(
        e.project,
        e.wt_root,
        e.feature,
        e.state,
        e.key,
        e.runner.clone(),
        (e.rt, e.plane),
        (&ruled, &told),
        "",
        e.options,
    )
    .map_err(restore)?;
    o.usage = crate::pipeline::commit::add_usage(o.usage.as_ref(), w.usage.as_ref());
    for (path, content) in &w.contents {
        let full = e.project.join(path);
        let written = full
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&full, content));
        if let Err(err) = written {
            return Err(restore(unwritten(e.key, format!("{path}: {err}"))));
        }
    }
    let new_paths: Vec<String> = w.tests.iter().map(|t| rel(&t.path)).collect();
    o.tests = kept.into_iter().chain(w.tests).collect();
    for (id, c) in contracts_of(&w.contracts) {
        o.contracts.insert(id, c);
    }
    for u in w.untestable.iter().filter(|u| ruled.contains(&u.scenario)) {
        o.untestable.insert(u.scenario.clone(), u.why.clone());
    }
    crate::oracle::refreeze(e.plane_dir, e.feature, e.key, o.tests.clone())
        .map_err(|m| restore(unwritten(e.key, m)))?;
    let now = states(e.project, &o, e.runner);
    let message = commit_message(
        &o,
        &now,
        Some(
            &rulings
                .iter()
                .filter_map(|r| r.scenario.as_deref())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        None,
        None,
    );
    let paths: Vec<String> = old_paths.iter().chain(&new_paths).cloned().collect();
    o.commit = super::commit::commit_paths(e.wt_root, &message, &paths, true).map_err(|m| {
        restore(unwritten(
            e.key,
            format!("the amendment could not be committed: {m}"),
        ))
    })?;
    let acted: Vec<String> = rulings.iter().filter_map(|r| r.scenario.clone()).collect();
    crate::acceptance_at::acted_on(e.plane_dir, e.feature, &acted)
        .map_err(|m| unwritten(e.key, m))?;
    Ok(o)
}

/// The scenario states the story's first oracle commit recorded.
fn first_states(wt_root: &Path, story: &str) -> Option<BTreeMap<String, ScenarioState>> {
    let out = crate::git::Git::at(wt_root)
        .line(&[
            "log",
            "--reverse",
            &format!("--grep=^{ORACLE}: {story}$"),
            "--format=%(trailers:key=Oracle-Scenarios,valueonly,separator=%x2c)",
            "HEAD",
        ])
        .ok()?;
    out.lines().next().map(parse_states)
}

/// `US1-1=green,US1-2=red` → each scenario's state.
fn parse_states(trailer: &str) -> BTreeMap<String, ScenarioState> {
    trailer
        .split(',')
        .filter_map(|p| {
            let (id, w) = p.trim().split_once('=')?;
            let st = match w.trim() {
                "green" => ScenarioState::Green,
                "red" => ScenarioState::Red,
                _ => ScenarioState::Untested,
            };
            Some((unesc(id.trim()), st))
        })
        .collect()
}

/// The `Oracle-Tests:` count an oracle commit records.
fn tests_of(wt_root: &Path, commit: &str) -> Option<usize> {
    crate::git::Git::at(wt_root)
        .line(&[
            "log",
            "-n1",
            "--format=%(trailers:key=Oracle-Tests,valueonly)",
            commit,
        ])
        .ok()
        .and_then(|v| v.trim().parse().ok())
}

/// `given → must hold`, by scenario: the first statement of each.
fn contracts_of(e: &[speccoder_core::oracle::Expectation]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for x in e {
        out.entry(x.scenario.clone())
            .or_insert_with(|| format!("{} → {}", x.given.trim(), x.expect.trim()));
    }
    out
}

/// One oracle test's run on a tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TestRun {
    pub(crate) passed: bool,
    pub(crate) class: speccoder_core::verify::RedClass,
    /// The end of its output (what the red excerpt is cut from).
    pub(crate) output: String,
}

/// The oracle's scenario states on a tree, with each test's run.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Run {
    pub(crate) states: BTreeMap<String, ScenarioState>,
    /// By test path.
    pub(crate) tests: BTreeMap<String, TestRun>,
}

/// The oracle's tests that are not, byte for byte, the frozen ones on the
/// tree at `project`: changed, extended or gone.
pub(crate) fn changed(project: &Path, oracle: &StoryOracle) -> Vec<String> {
    oracle
        .tests
        .iter()
        .filter(|t| {
            std::fs::read(project.join(&t.path)).map_or(true, |b| {
                format!("sha256:{}", crate::integrity::sha256_hex(&b)) != t.sha256
            })
        })
        .map(|t| t.path.clone())
        .collect()
}

/// Each scenario's state on the tree at `project`: green when every test
/// citing it passes (each by its own command, in the sandbox), red when
/// one fails, untested when none cites it.
pub(crate) fn states(
    project: &Path,
    oracle: &StoryOracle,
    runner: &sandbox::Runner,
) -> BTreeMap<String, ScenarioState> {
    run(project, oracle, runner).states
}

/// [`states`], with each test's run kept.
pub(crate) fn run(project: &Path, oracle: &StoryOracle, runner: &sandbox::Runner) -> Run {
    let tests: BTreeMap<String, TestRun> = oracle
        .tests
        .iter()
        .map(|t| {
            let (r, summary) = super::acceptance::acceptance_run(project, &t.argv, runner);
            // Green is a test that ran and passed: a command that passes
            // collecting nothing proves nothing.
            let class = speccoder_core::verify::classify_red(&r, summary.as_ref());
            let run = TestRun {
                passed: r.status == CheckStatus::Passed
                    && class == speccoder_core::verify::RedClass::Passed,
                class,
                output: format!("{}\n{}", r.stdout_tail, r.stderr_tail)
                    .trim()
                    .to_owned(),
            };
            (t.path.clone(), run)
        })
        .collect();
    let passed: Vec<(&FrozenTest, bool)> = oracle
        .tests
        .iter()
        .map(|t| (t, tests.get(&t.path).is_some_and(|r| r.passed)))
        .collect();
    let states = oracle
        .scenarios
        .iter()
        .map(|s| {
            let citing: Vec<bool> = passed
                .iter()
                .filter(|(t, _)| t.scenarios.contains(&s.id))
                .map(|(_, ok)| *ok)
                .collect();
            let state = if citing.is_empty() {
                ScenarioState::Untested
            } else if citing.iter().all(|ok| *ok) {
                ScenarioState::Green
            } else {
                ScenarioState::Red
            };
            (s.id.clone(), state)
        })
        .collect();
    Run { states, tests }
}

/// The evidence of a tick that turned scenarios green: the oracle's tests
/// of those scenarios, red before the attempt (their class and output) and
/// green after — the acceptance record attestations and the report read.
pub(crate) fn evidence(oracle: &StoryOracle, turned: &[String], before: &Run) -> serde_json::Value {
    let tests: Vec<&FrozenTest> = oracle
        .tests
        .iter()
        .filter(|t| t.scenarios.iter().any(|s| turned.contains(s)))
        .collect();
    let red: Vec<&TestRun> = tests
        .iter()
        .filter_map(|t| before.tests.get(&t.path))
        .filter(|r| r.class.is_red())
        .collect();
    // No run of its own before the attempt: red by the oracle's states,
    // its class not seen.
    let class = red
        .first()
        .map_or(speccoder_core::verify::RedClass::Unclassified, |r| r.class);
    serde_json::json!({
        // Frozen at review by a person, or written by the run.
        "source": if oracle.decided_by == "speccoder run" { "authored" } else { "review" },
        "oracle": oracle.commit,
        "files": tests.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
        "tests": tests
            .iter()
            .map(|t| serde_json::json!({ "path": t.path, "scenarios": t.scenarios, "argv": t.argv }))
            .collect::<Vec<_>>(),
        "argv": tests.first().map(|t| t.argv.clone()).unwrap_or_default(),
        "red": true,
        "red_class": class,
        "red_output": red.first().map(|r| r.output.clone()).unwrap_or_default(),
        "green": true,
        "pre_frozen": oracle.reviewed.as_ref().map(|(r, m)| serde_json::json!({
            "manifest": m,
            "approval": r.approval_digest,
            "wording": r.wording_digest,
            "by": r.decided.by,
            "via": r.decided.via,
        })),
    })
}

/// A scenario id as a trailer or body word: `%`, `,`, `=`, `:`, spaces and
/// parentheses escaped (an OpenSpec scenario is named, not numbered).
fn esc(id: &str) -> String {
    let mut out = String::new();
    for c in id.chars() {
        match c {
            '%' | ',' | '=' | ':' | ' ' | '(' | ')' | '\n' | '\r' => {
                for b in c.to_string().bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// [`esc`] undone.
fn unesc(word: &str) -> String {
    let b = word.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(h) = word.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(h, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `US1-1=red, US1-2=green, US1-3=untested`.
pub(crate) fn states_trailer(order: &[String], states: &BTreeMap<String, ScenarioState>) -> String {
    order
        .iter()
        .map(|id| {
            let word = match states.get(id).copied().unwrap_or(ScenarioState::Untested) {
                ScenarioState::Green => "green",
                ScenarioState::Red => "red",
                ScenarioState::Untested => "untested",
            };
            format!("{}={word}", esc(id))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The oracle commit's message: what it is, each scenario's contract, and
/// its trailers.
pub(crate) fn commit_message(
    oracle: &StoryOracle,
    states: &BTreeMap<String, ScenarioState>,
    amends: Option<&str>,
    run: Option<&str>,
    usage: Option<&serde_json::Value>,
) -> String {
    let order = oracle.order();
    let red = order
        .iter()
        .filter(|id| states.get(*id) != Some(&ScenarioState::Green))
        .count();
    let title = match amends {
        Some(s) => format!("speccoder: oracle {} amended ({s})", oracle.story),
        None => format!(
            "speccoder: oracle {} — {} scenario(s), {red} to turn green",
            oracle.story,
            order.len()
        ),
    };
    let contracts: Vec<String> = order
        .iter()
        .filter_map(|id| {
            oracle
                .contracts
                .get(id)
                .map(|c| format!("{}: {}", esc(id), c.replace('\n', " ")))
        })
        .collect();
    let mut trailers = vec![
        format!("{ORACLE}: {}", oracle.story),
        format!("Oracle-Scenarios: {}", states_trailer(&order, states)),
        format!("Oracle-Tests: {}", oracle.tests.len()),
        format!("Oracle-By: {}", oracle.decided_by),
    ];
    for (id, why) in &oracle.untestable {
        trailers.push(format!(
            "Oracle-Untestable: {} ({})",
            esc(id),
            why.replace(['\n', '(', ')'], " ").trim()
        ));
    }
    if let Some(s) = amends {
        trailers.push(format!("Oracle-Amends: {s}"));
    }
    if let Some(r) = run.filter(|r| !r.is_empty()) {
        trailers.push(format!("Run: {r}"));
    }
    let usage = crate::attempts::usage_trailer(usage);
    if !usage.is_empty() {
        trailers.push(usage);
    }
    let body = if contracts.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", contracts.join("\n"))
    };
    format!("{title}\n\n{body}{}\n", trailers.join("\n"))
}

/// What an attempt did to its story, folded into the run report's
/// verdict: a regression, open links or an open story fail it (naming the
/// scenarios and their contracts); progress verifies it; none keeps it
/// regression-only. The checks' own verdict stands when it failed.
pub(crate) fn judge(
    oracle: &StoryOracle,
    before: &BTreeMap<String, ScenarioState>,
    after: &BTreeMap<String, ScenarioState>,
    links: &[String],
    checkpoint: bool,
    report: &mut speccoder_core::RunReport,
) {
    let untestable: Vec<String> = oracle.untestable.keys().cloned().collect();
    let p = speccoder_core::story::progress(
        &oracle.order(),
        before,
        after,
        links,
        checkpoint,
        &untestable,
    );
    let name = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .map(|id| match oracle.contracts.get(id) {
                Some(c) => format!("{id} ({c})"),
                None => id.clone(),
            })
            .collect()
    };
    let failed = match &p {
        Progress::Regressed { scenarios } => Some(format!(
            "scenarios green before the attempt are red after it: {}",
            name(scenarios).join("; ")
        )),
        Progress::LinksOpen { scenarios } => Some(format!(
            "the scenarios this task names are not green: {}",
            name(scenarios).join("; ")
        )),
        Progress::StoryOpen { scenarios } => Some(format!(
            "the story's last task left scenarios red: {}",
            name(scenarios).join("; ")
        )),
        _ => None,
    };
    report.progress = Some(p.clone());
    match (report.verdict.take(), failed) {
        (Some(Verdict::Failed { mut reasons }), Some(why)) => {
            reasons.push(why);
            report.verdict = Some(Verdict::Failed { reasons });
        }
        (Some(v @ Verdict::Failed { .. }), None) => report.verdict = Some(v),
        (_, Some(why)) => report.verdict = Some(Verdict::Failed { reasons: vec![why] }),
        (v, None) => {
            report.verdict = match p {
                Progress::Verified { .. } => Some(Verdict::Verified),
                _ => v.map(|_| Verdict::RegressionOnly),
            };
        }
    }
}

/// Whether the attempt's checks could hide a regression: a check failing
/// before and after it without per-test results.
pub(crate) fn unchecked(report: &speccoder_core::RunReport) -> bool {
    let Some(v) = report.verification.as_ref() else {
        return false;
    };
    let as_before = v
        .comparison
        .as_ref()
        .map(|c| c.failing_as_before.clone())
        .unwrap_or_default();
    v.checks
        .iter()
        .any(|c| as_before.contains(&c.name) && c.tests.is_none())
}

/// The tasks of each story of a unit, in document order: by label (Spec
/// Kit), or the whole unit as one story (OpenSpec, `whole_unit`).
pub(crate) fn stories(
    tasks: &speccoder_core::snapshot::ParsedTasks,
    whole_unit: bool,
) -> Vec<(Option<&str>, Vec<&speccoder_core::snapshot::Task>)> {
    let mut out: Vec<(Option<&str>, Vec<&speccoder_core::snapshot::Task>)> = Vec::new();
    for t in &tasks.tasks {
        let label = if whole_unit { None } else { t.story.as_deref() };
        if !whole_unit && label.is_none() {
            continue;
        }
        match out.iter_mut().find(|(l, _)| *l == label) {
            Some((_, members)) => members.push(t),
            None => out.push((label, vec![t])),
        }
    }
    out
}

/// The story's tasks as the rule sees them.
pub(crate) fn story_tasks<'a>(
    members: &[&'a speccoder_core::snapshot::Task],
    unit_dir: &str,
    judged: &dyn Fn(&str) -> bool,
) -> Vec<speccoder_core::story::StoryTask<'a>> {
    members
        .iter()
        .map(|t| speccoder_core::story::StoryTask {
            id: &t.id,
            checks_only: super::acceptance::checks_only(t, unit_dir, judged),
            done: t.done,
            links: &t.scenarios,
        })
        .collect()
}

/// What an oracle commit on `rev` says: its short id, who froze it, each
/// scenario's state when it was written, and each scenario's contract.
struct Written {
    commit: String,
    by: String,
    states: BTreeMap<String, ScenarioState>,
    contracts: BTreeMap<String, String>,
    untestable: BTreeMap<String, String>,
}

fn written(repo: &Path, rev: &str, story: &str) -> Option<Written> {
    let out = crate::git::Git::at(repo)
        .line(&[
            "log",
            "-n1",
            &format!("--grep=^{ORACLE}: {story}$"),
            "--format=%h%x1f%(trailers:key=Oracle-Scenarios,valueonly,separator=%x2c)%x1f%(trailers:key=Oracle-By,valueonly)%x1f%(trailers:key=Oracle-Untestable,valueonly,separator=%x1d)%x1f%b",
            rev,
            "--",
        ])
        .ok()?;
    let mut parts = out.splitn(5, '\u{1f}');
    let commit = parts.next()?.trim().to_owned();
    if commit.is_empty() {
        return None;
    }
    let states = parse_states(parts.next().unwrap_or_default());
    let by = parts.next().unwrap_or_default().trim().to_owned();
    let untestable = parts
        .next()
        .unwrap_or_default()
        .split('\u{1d}')
        .filter_map(|u| {
            let (id, why) = u.trim().split_once(' ')?;
            Some((
                unesc(id),
                why.trim_matches(|c| c == '(' || c == ')').to_owned(),
            ))
        })
        .collect();
    let contracts = parts
        .next()
        .unwrap_or_default()
        .lines()
        // The body's first paragraph: the contracts (a trailer read as one
        // names no scenario, so no lookup finds it).
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let (id, c) = l.split_once(": ")?;
            Some((unesc(id), c.trim().to_owned()))
        })
        .collect();
    Some(Written {
        commit,
        by: if by.is_empty() {
            "speccoder run".to_owned()
        } else {
            by
        },
        states,
        contracts,
        untestable,
    })
}

/// Who turned each scenario green last, by the unit branch's history read
/// newest first: a tick whose `Scenarios:` names it (by digest), unless an
/// oracle commit rewrote its tests after (`Oracle-Amends:`, or the oracle
/// written anew) — then nobody yet.
/// The scenarios a tick on the branch records a person accepted as
/// untestable (`Untestable-Accepted: US2-2 by …`).
pub(crate) fn accepted_on_branch(repo: &Path, rev: &str) -> Vec<String> {
    crate::git::Git::at(repo)
        .run(&[
            "log",
            "--first-parent",
            "--format=%(trailers:key=Untestable-Accepted,valueonly,separator=%x1e)",
            rev,
            "--",
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
        .split(['\u{1e}', '\n'])
        .filter_map(|v| v.split_whitespace().next().map(str::to_owned))
        .collect()
}

fn turned_by(
    repo: &Path,
    rev: &str,
    feature: &str,
    story: &str,
    digests: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let log = crate::git::Git::at(repo)
        .run(&[
            "log",
            "--first-parent",
            "--format=%(trailers:key=Spec-Task,valueonly,separator=%x20)%x1f%(trailers:key=Scenarios,valueonly,separator=%x20)%x1f%(trailers:key=Oracle,valueonly)%x1f%(trailers:key=Oracle-Amends,valueonly,separator=%x2c)%x1e",
            rev,
            "--",
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    let mut settled: Vec<String> = Vec::new();
    for record in log.split('\u{1e}') {
        let f: Vec<&str> = record.trim_start_matches('\n').split('\u{1f}').collect();
        let [task, marked, oracle, amends] = f[..] else {
            continue;
        };
        if oracle.trim() == story {
            let amended: Vec<String> = amends
                .split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect();
            // Written anew (no amendment named): every scenario is settled.
            if amended.is_empty() {
                break;
            }
            settled.extend(amended);
            continue;
        }
        let Some(task) = task.trim().strip_prefix(&format!("{feature}/")) else {
            continue;
        };
        for token in marked.split(|c: char| c == ',' || c.is_whitespace()) {
            let d = token.rsplit('@').next().unwrap_or_default();
            if let Some((id, _)) = digests.iter().find(|(_, dg)| *dg == d)
                && !settled.contains(id)
                && !out.contains_key(id)
            {
                out.insert(id.clone(), task.to_owned());
            }
        }
    }
    out
}

/// What the report shows of a unit's stories.
pub(crate) struct StoryView<'a> {
    pub(crate) format: &'a dyn crate::format::Adapter,
    pub(crate) snapshot: &'a speccoder_core::snapshot::ProjectSnapshot,
    pub(crate) feature: &'a str,
    pub(crate) unit_dir: &'a str,
    pub(crate) tasks: &'a speccoder_core::snapshot::ParsedTasks,
    /// Where the unit's ticks are read: the repository and the revision of
    /// its branch (or `HEAD` in place).
    pub(crate) repo: &'a Path,
    pub(crate) rev: &'a str,
}

/// What judges each open task of a story, and each story's scenarios with
/// their states at the branch's tip: green when a tick turned it (worded
/// as it is now) or it was green when the oracle was written, else as the
/// oracle commit recorded it (red before any oracle).
pub(crate) fn story_view(
    v: &StoryView<'_>,
) -> (
    BTreeMap<String, speccoder_core::story::Judged>,
    Vec<speccoder_core::story::StoryScenarios>,
) {
    use speccoder_core::story::{OracleMark, ScenarioMark, StoryScenarios, checkpoint, judged};
    let whole_unit = v.format.kind() == speccoder_core::spec::FormatKind::Openspec;
    let rules = v.format.rules().judged;
    let mut out = BTreeMap::new();
    let mut shown = Vec::new();
    // Tasks outside any story that cite scenarios (convergence).
    if !whole_unit {
        for t in v.tasks.tasks.iter().filter(|t| t.story.is_none()) {
            let scenarios = v.format.scenarios(v.snapshot, &t.id);
            if let Some(j) = super::acceptance::outside_story(t, &scenarios, v.unit_dir, &rules) {
                out.insert(t.id.clone(), j);
            }
        }
    }
    for (label, members) in stories(v.tasks, whole_unit) {
        let Some(first) = members.first() else {
            continue;
        };
        let scenarios = v.format.scenarios(v.snapshot, &first.id);
        if scenarios.is_empty() {
            continue;
        }
        let key = label.map_or_else(|| v.feature.to_owned(), str::to_owned);
        let tasks = story_tasks(&members, v.unit_dir, &rules);
        let ids: Vec<String> = scenarios.iter().map(|s| s.id.clone()).collect();
        for t in &members {
            if let Some(j) = judged(&tasks, &t.id, &ids) {
                out.insert(t.id.clone(), j);
            }
        }
        let oracle = written(v.repo, v.rev, &key);
        let by_digest: BTreeMap<String, String> = scenarios
            .iter()
            .map(|s| (s.id.clone(), super::commit::scenario_digest(&s.id, &s.text)))
            .collect();
        let turned = turned_by(v.repo, v.rev, v.feature, &key, &by_digest);
        let marks: Vec<ScenarioMark> = scenarios
            .iter()
            .map(|s| {
                let turned_by = turned.get(&s.id).cloned();
                let at_oracle = oracle.as_ref().and_then(|o| o.states.get(&s.id).copied());
                let state = match (&turned_by, at_oracle) {
                    (Some(_), _) | (None, Some(ScenarioState::Green)) => ScenarioState::Green,
                    (None, Some(ScenarioState::Untested)) => ScenarioState::Untested,
                    _ => ScenarioState::Red,
                };
                ScenarioMark {
                    id: s.id.clone(),
                    state,
                    turned_by,
                    contract: oracle
                        .as_ref()
                        .and_then(|o| o.contracts.get(&s.id).cloned()),
                }
            })
            .collect();
        let done = members.iter().all(|t| t.done);
        // Proven only red→green by a tick: a scenario its oracle, written
        // after its code, found green holds — it was never shown to turn.
        let accepted = accepted_on_branch(v.repo, v.rev);
        let proven = done
            && marks
                .iter()
                .all(|m| m.turned_by.is_some() || accepted.contains(&m.id));
        let before_oracle = done && oracle.is_none();
        shown.push(StoryScenarios {
            story: key,
            scenarios: marks,
            checkpoint: checkpoint(&tasks).map(str::to_owned),
            oracle: oracle.map(|o| OracleMark {
                commit: o.commit,
                decided_by: o.by,
            }),
            proven,
            before_oracle,
            finished: done,
        });
    }
    (out, shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oracle() -> StoryOracle {
        StoryOracle {
            story: "US1".into(),
            scenarios: (1..=2)
                .map(|k| ScenarioRef {
                    id: format!("US1-{k}"),
                    text: String::new(),
                })
                .collect(),
            tests: Vec::new(),
            contracts: [(
                "US1-1".to_owned(),
                "a table on stdin → its CSV on stdout".to_owned(),
            )]
            .into(),
            commit: "abc".into(),
            decided_by: "speccoder run".into(),
            held_out: None,
            held_out_summary: None,
            usage: None,
            reviewed: None,
            at_oracle: BTreeMap::new(),
            untestable: BTreeMap::new(),
        }
    }

    #[test]
    fn a_check_failing_before_and_after_without_per_test_results_hides_regressions() {
        use speccoder_core::verify::{CheckResult, Comparison, VerifyReport};
        let check =
            |name: &str, tests: Option<Vec<speccoder_core::verify::TestResult>>| CheckResult {
                name: name.to_owned(),
                status: CheckStatus::Failed,
                tests,
                ..serde_json::from_value(serde_json::json!({
                    "name": name, "argv": [], "status": "failed", "exit_code": 1, "duration_ms": 0,
                    "junit": false, "stdout_tail": "", "stderr_tail": "", "truncated": false
                }))
                .unwrap()
            };
        let report = |c: CheckResult| speccoder_core::RunReport {
            verification: Some(VerifyReport {
                root: String::new(),
                tier: "fast".to_owned(),
                checks: vec![c],
                comparison: Some(Comparison {
                    failing_as_before: vec!["test".to_owned()],
                    ..Comparison::default()
                }),
                passed: true,
            }),
            ..super::super::outcome::bare_report("001", "T001", "/w", "completed")
        };
        assert!(unchecked(&report(check("test", None))));
        assert!(!unchecked(&report(check("test", Some(Vec::new())))));
    }

    #[test]
    fn the_oracle_commit_names_its_story_states_and_contracts() {
        let o = oracle();
        let st: BTreeMap<String, ScenarioState> = [
            ("US1-1".to_owned(), ScenarioState::Red),
            ("US1-2".to_owned(), ScenarioState::Untested),
        ]
        .into();
        let m = commit_message(&o, &st, None, Some("run_1"), None);
        assert!(m.starts_with("speccoder: oracle US1 — 2 scenario(s), 2 to turn green\n\nUS1-1: a table on stdin → its CSV on stdout\n\n"), "{m}");
        assert!(m.contains("\nOracle: US1\nOracle-Scenarios: US1-1=red, US1-2=untested\nOracle-Tests: 0\nOracle-By: speccoder run\nRun: run_1\n"), "{m}");
    }

    #[test]
    fn a_named_scenario_survives_the_trailer() {
        let id = "Login: wrong password (3 times), 100% = locked";
        assert_eq!(unesc(&esc(id)), id);
        let read = parse_states(&format!("{}=green, US1-2=red", esc(id)));
        assert_eq!(read.get(id), Some(&ScenarioState::Green));
        assert_eq!(read.get("US1-2"), Some(&ScenarioState::Red));
    }
}
