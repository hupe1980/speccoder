//! Running a confirmed plan: the steps in order, `init` in process, the
//! one commit as the person, the creation record and `project.created`.

use std::path::{Path, PathBuf};

use speccoder_core::actions::Via;
use speccoder_core::new_project::{
    NewChoices, PlanStepResult, PlanStepState, ProjectMode, ProjectPlan, StepAccess,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::{
    Before, COMMAND, Creation, Kind, Spec, entries_of, format_kind, git_digest, ignored, now,
    path_digest, plan_with, record_dir, refused_result, report, shown, snapshot, steps, undo_offer,
    write_creation,
};
use crate::RunOptions;
use crate::git::Git;

/// A lock on creating the folder: one creation at a time per folder.
struct Lock(PathBuf);

impl Lock {
    fn take(state: &Path, target: &Path) -> Result<Self, String> {
        let dir = record_dir(state, target);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let file = dir.join("lock");
        let mine = std::process::id().to_string();
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
            {
                Ok(mut f) => {
                    use std::io::Write as _;
                    let _ = f.write_all(mine.as_bytes());
                    return Ok(Self(file));
                }
                Err(_) => {
                    let holder = std::fs::read_to_string(&file).unwrap_or_default();
                    let alive = holder
                        .trim()
                        .parse::<u32>()
                        .is_ok_and(crate::progress::alive);
                    if alive {
                        return Err(holder.trim().to_owned());
                    }
                    let _ = std::fs::remove_file(&file);
                }
            }
        }
        Err(String::new())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Whether a step's effect is already in the folder.
fn already(target: &Path, spec: &Spec) -> bool {
    !spec.marker.is_empty() && target.join(&spec.marker).exists()
}

/// The paths the steps produced: every path `git status` lists (the
/// tools' own ignore files respected), build output and dependencies left
/// out, and nothing that was in the folder before the first step.
fn produced(target: &Path, before: &Before) -> Result<Vec<String>, String> {
    let out = Git::at(target)
        .run(&["status", "--porcelain", "--untracked-files=all", "-z"])
        .map_err(|e| e.0)?;
    let text = String::from_utf8_lossy(&out).into_owned();
    let mut paths: Vec<String> = Vec::new();
    let mut it = text.split('\0').filter(|s| !s.is_empty());
    while let Some(entry) = it.next() {
        let (code, path) = entry.split_at(entry.len().min(3));
        // A rename lists its source next.
        if code.starts_with('R') || code.starts_with('C') {
            let _ = it.next();
        }
        let top = path.split('/').next().unwrap_or(path);
        if !speccoder_core::collection::is_build_or_dependency(path)
            && !before.entries.iter().any(|e| e == top || e == path)
        {
            paths.push(path.to_owned());
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// The one commit, as the person: exactly `paths`.
fn commit(target: &Path, paths: &[String], subject: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("the steps produced nothing to commit".to_owned());
    }
    let g = Git::at(target);
    let names: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut add = vec!["add", "--"];
    add.extend(&names);
    g.run(&add).map_err(|e| e.0)?;
    let mut c = vec!["commit", "-q", "-m", subject, "--"];
    c.extend(&names);
    g.run(&c).map_err(|e| e.0)?;
    g.line(&["rev-parse", "--short", "HEAD"]).map_err(|e| e.0)
}

/// Creates the project: the plan recomputed and compared with the one the
/// person confirmed (`plan_digest`), each step run in order (a step whose
/// effect is already there is skipped), `init` in process, one commit as
/// the person, the creation record, `project.created`.
#[must_use]
pub fn perform(
    target: &Path,
    choices: &NewChoices,
    plan_digest: &str,
    via: Via,
    state: &Path,
    options: &RunOptions,
) -> CommandResult {
    let (plan, specs, resumed) = plan_with(target, choices, state, true);
    if plan.refusal.is_some() {
        return refused_result(plan, Vec::new());
    }
    if plan.digest != plan_digest {
        return CommandResult::new(
            COMMAND,
            CommandStatus::UsageError,
            &format!(
                "the plan changed since you saw it ({}); nothing ran — review it again",
                changed_what(&plan)
            ),
        )
        .with_code("NEW-PLAN-CHANGED")
        .with_data(CommandData::NewProject(Box::new(report(
            plan,
            Vec::new(),
            false,
        ))));
    }
    let root = PathBuf::from(&plan.target);
    let _lock = match Lock::take(state, &root) {
        Ok(l) => l,
        Err(holder) => {
            return CommandResult::new(
                COMMAND,
                CommandStatus::UsageError,
                &format!(
                    "a new project is being created in {}{}",
                    shown(&root),
                    if holder.is_empty() {
                        String::new()
                    } else {
                        format!(" (process {holder})")
                    }
                ),
            )
            .with_code("NEW-IN-PROGRESS");
        }
    };
    if let Err(e) = std::fs::create_dir_all(&root) {
        return CommandResult::new(
            COMMAND,
            CommandStatus::Error,
            &format!("cannot create {}: {e}", shown(&root)),
        );
    }
    let existed = entries_of(&root);
    let fresh_record = || Creation {
        path: plan.target.clone(),
        mode: plan.mode,
        commit: None,
        plan_digest: plan.digest.clone(),
        name: plan.name.clone(),
        idea: choices.idea.clone(),
        stack: plan.stack.id,
        format: plan.format.clone(),
        by: plan.identity.clone(),
        via,
        at: now(),
        first_feature_offered: false,
        undone_at: None,
        before: Before {
            made: existed.iter().all(|e| e == ".DS_Store"),
            ignored: if plan.mode == ProjectMode::ExistingRepository {
                ignored(&root)
            } else {
                Vec::new()
            },
            entries: existed.clone(),
        },
        left: Some(snapshot(&root)),
        git: git_digest(&root),
    };
    // Resumed only as the plan said: the folder exactly as it was left.
    let mut record = resumed.unwrap_or_else(fresh_record);
    record.plan_digest.clone_from(&plan.digest);
    if let Err(e) = write_creation(state, &record) {
        return CommandResult::new(COMMAND, CommandStatus::Error, &e);
    }
    if let Some(a) = format_kind(&plan.format).and_then(crate::format::adapter) {
        let tool = a.tool().map_or("format", |(t, _)| t);
        let init = crate::format::FormatInit {
            extensions: plan.extensions.clone(),
            preset: plan.preset,
            state: state.to_path_buf(),
            cache: steps::cache_dir(state, tool),
        };
        if let Err(e) = a.prepare_init(&init) {
            return CommandResult::new(COMMAND, CommandStatus::Error, &e);
        }
    }
    let event = |e: crate::progress::ProjectEvent| {
        if let Some(p) = &options.progress {
            p.event(&crate::progress::Event::Project { step: e });
        }
    };
    let mut results: Vec<PlanStepResult> = Vec::new();
    let mut init_report = None;
    let mut made = None;
    for spec in &specs {
        let access = match spec.step.access {
            StepAccess::Local => "local",
            StepAccess::Network => "network",
        };
        event(crate::progress::ProjectEvent::ScaffoldStarted {
            step: spec.step.id.clone(),
            argv: spec.step.argv.clone(),
            access: access.to_owned(),
        });
        let began = std::time::Instant::now();
        let result = match spec.kind {
            _ if already(&root, spec) => PlanStepResult {
                id: spec.step.id.clone(),
                state: PlanStepState::Skipped,
                exit: None,
                duration_ms: 0,
                output_tail: vec!["already done".to_owned()],
            },
            Kind::Command => {
                let repo = git_digest(&root);
                let mut r = steps::run(
                    &root,
                    state,
                    &steps::Run {
                        id: &spec.step.id,
                        argv: &spec.step.argv,
                        access: spec.step.access,
                        limit_s: spec.step.limit_s,
                        env: &spec.env,
                    },
                );
                // `git init` made the repository: from here on, its own
                // files are speccoder's to keep as they are.
                if spec.step.id == "git_init" && r.state == PlanStepState::Done {
                    record.git = git_digest(&root);
                } else if repo.is_some() && git_digest(&root) != repo {
                    r.state = PlanStepState::Failed;
                    r.output_tail.push("the step changed the repository's own files (.git): speccoder stops before its commit — run the step yourself to see what it does".to_owned());
                }
                // An existing repository: the person's tracked files stay theirs.
                if plan.mode == ProjectMode::ExistingRepository && r.state == PlanStepState::Done {
                    let changed: Vec<String> = snapshot(&root)
                        .porcelain
                        .into_iter()
                        .filter(|l| !l.starts_with("??"))
                        .map(|l| l.get(3..).unwrap_or_default().to_owned())
                        .collect();
                    if !changed.is_empty() {
                        r.state = PlanStepState::Failed;
                        r.output_tail.push(format!(
                            "it changed files of yours: {} — speccoder commits none of them; restore them (git restore) and add the format yourself",
                            changed.join(", ")
                        ));
                    }
                }
                r
            }
            Kind::Init => match crate::init::prepare(&root, None, None, None, false, false) {
                Ok(p) => {
                    let lines = p
                        .report
                        .ran
                        .iter()
                        .map(|r| format!("{} {}", r.name, r.status))
                        .collect();
                    init_report = Some(p.report);
                    PlanStepResult {
                        id: spec.step.id.clone(),
                        state: PlanStepState::Done,
                        exit: Some(0),
                        duration_ms: 0,
                        output_tail: lines,
                    }
                }
                Err(r) => PlanStepResult {
                    id: spec.step.id.clone(),
                    state: PlanStepState::Failed,
                    exit: None,
                    duration_ms: 0,
                    output_tail: vec![r.message().to_owned()],
                },
            },
            Kind::Commit if record.git.is_some() && git_digest(&root) != record.git => {
                PlanStepResult {
                    id: spec.step.id.clone(),
                    state: PlanStepState::Failed,
                    exit: None,
                    duration_ms: 0,
                    output_tail: vec!["the repository's own files (.git) changed since speccoder made or found them: nothing is committed".to_owned()],
                }
            }
            Kind::Commit => {
                let subject = spec.step.argv.last().cloned().unwrap_or_default();
                match produced(&root, &record.before).and_then(|paths| commit(&root, &paths, &subject)) {
                    Ok(id) => {
                        made = Some(id.clone());
                        PlanStepResult {
                            id: spec.step.id.clone(),
                            state: PlanStepState::Done,
                            exit: Some(0),
                            duration_ms: 0,
                            output_tail: vec![id],
                        }
                    }
                    Err(e) => PlanStepResult {
                        id: spec.step.id.clone(),
                        state: PlanStepState::Failed,
                        exit: None,
                        duration_ms: 0,
                        output_tail: steps::tail(&e),
                    },
                }
            }
        };
        let mut result = result;
        if result.duration_ms == 0 && result.state != PlanStepState::Skipped {
            result.duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        }
        event(crate::progress::ProjectEvent::ScaffoldFinished {
            step: result.id.clone(),
            state: result.state.word().to_owned(),
            exit: result.exit,
            duration_ms: result.duration_ms,
        });
        let failed = result.state == PlanStepState::Failed;
        results.push(result);
        if failed {
            break;
        }
    }
    // Steps after a failure wait.
    for spec in specs.iter().skip(results.len()) {
        results.push(PlanStepResult {
            id: spec.step.id.clone(),
            state: PlanStepState::Waiting,
            exit: None,
            duration_ms: 0,
            output_tail: Vec::new(),
        });
    }
    let mut out = report(plan.clone(), results, false);
    out.init = init_report;
    // How this run leaves the folder: what a resume must find.
    record.left = Some(snapshot(&root));
    let kept = write_creation(state, &record);
    if let Some(failed) = out
        .results
        .iter()
        .find(|r| r.state == PlanStepState::Failed)
    {
        let n = out
            .results
            .iter()
            .position(|r| r.id == failed.id)
            .map_or(0, |i| i + 1);
        let argv = specs
            .iter()
            .find(|s| s.step.id == failed.id)
            .map(|s| s.step.argv.join(" "))
            .unwrap_or_default();
        let how = match failed.exit {
            Some(0) => "was stopped".to_owned(),
            Some(e) => format!("exited {e}"),
            None => "failed".to_owned(),
        };
        let mut message = format!("step {n} `{argv}` {how}; later steps did not run");
        if let Some(why) = failed
            .output_tail
            .iter()
            .rev()
            .find(|l| l.contains("speccoder"))
        {
            message = format!("{message}: {why}");
        }
        if let Err(e) = &kept {
            message = format!("{message}; the record to resume from could not be written ({e})");
        }
        out.undo = undo_offer(state, &root);
        return CommandResult::new(COMMAND, CommandStatus::Error, &message)
            .with_code("NEW-STEP-FAILED")
            .with_fix(vec![
                format!("→ retry: run the same `speccoder new` again (it resumes at step {n})"),
                format!("→ or run it yourself in {}: {argv}", shown(&root)),
            ])
            .with_data(CommandData::NewProject(Box::new(out)));
    }
    let Some(commit_id) = made else {
        return CommandResult::new(COMMAND, CommandStatus::Error, "no commit was made")
            .with_data(CommandData::NewProject(Box::new(out)));
    };
    let full = Git::at(&root)
        .line(&["rev-parse", "HEAD"])
        .unwrap_or_else(|_| commit_id.clone());
    record.commit = Some(full);
    record.at = now();
    let recorded = write_creation(state, &record);
    crate::welcome::remember(state, &root, &plan.name, true);
    event(crate::progress::ProjectEvent::Created {
        path_digest: path_digest(&root),
        mode: match plan.mode {
            ProjectMode::NewFolder => "new_folder",
            ProjectMode::ExistingRepository => "existing_repository",
        }
        .to_owned(),
        stack: plan.stack.id.as_str().to_owned(),
        format: plan.format.clone(),
        commit: commit_id.clone(),
        via: via.as_str().to_owned(),
    });
    out.commit = Some(commit_id.clone());
    out.undo = undo_offer(state, &root);
    out.first_feature = (!choices.idea.trim().is_empty()).then(|| choices.idea.clone());
    let checks = out.init.as_ref().map_or_else(String::new, |i| {
        if i.ran.is_empty() {
            "no checks yet: add a test runner before the first run".to_owned()
        } else {
            let passing: Vec<&str> = i
                .ran
                .iter()
                .filter(|r| r.passed && r.status != "no_tests")
                .map(|r| r.name.as_str())
                .collect();
            let empty: Vec<&str> = i
                .ran
                .iter()
                .filter(|r| r.status == "no_tests")
                .map(|r| r.name.as_str())
                .collect();
            let failing: Vec<&str> = i
                .ran
                .iter()
                .filter(|r| !r.passed && r.status != "no_tests")
                .map(|r| r.name.as_str())
                .collect();
            let mut parts = Vec::new();
            if !passing.is_empty() {
                parts.push(format!(
                    "{} pass{} on the untouched code",
                    passing.join(", "),
                    if passing.len() == 1 { "es" } else { "" }
                ));
            }
            if !empty.is_empty() {
                parts.push(format!("{}: no tests yet", empty.join(", ")));
            }
            if !failing.is_empty() {
                parts.push(format!(
                    "{} fail{} before any change",
                    failing.join(", "),
                    if failing.len() == 1 { "s" } else { "" }
                ));
            }
            parts.join("; ")
        }
    });
    let mut message = format!("{} is ready — {checks}; committed {commit_id}", plan.name);
    if let Err(e) = recorded {
        message = format!(
            "{message}; but its creation record could not be written ({e}): Undo is not offered"
        );
    }
    let create = format_kind(&plan.format)
        .and_then(crate::format::adapter)
        .and_then(|a| a.planning().creates.first().copied());
    let mut next = Vec::new();
    if let Some(create) = create.filter(|_| !choices.idea.trim().is_empty()) {
        next.push(speccoder_core::next::NextStep {
            command: format!(
                "speccoder spec {create} {} --through tasks -C {}",
                speccoder_core::ladder::quoted(&choices.idea),
                shown(&root)
            ),
            why: "write the first feature from your idea".to_owned(),
        });
    }
    next.push(speccoder_core::next::NextStep {
        command: format!("speccoder ui -C {}", shown(&root)),
        why: "open it in the app".to_owned(),
    });
    CommandResult::new(COMMAND, CommandStatus::Ok, &message)
        .with_data(CommandData::NewProject(Box::new(out)))
        .with_next(next)
}

/// What made a recomputed plan differ, in words.
fn changed_what(plan: &ProjectPlan) -> String {
    let n = plan.observed.entries.len();
    if plan.observed.exists && n > 0 {
        format!(
            "the folder now has {n} entr{}",
            if n == 1 { "y" } else { "ies" }
        )
    } else {
        "a tool or the folder changed".to_owned()
    }
}
