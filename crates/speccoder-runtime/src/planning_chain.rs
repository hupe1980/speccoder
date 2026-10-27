//! Plan through: one description, planned by the format's own skills in
//! the order its [`Planning`](crate::format::Planning) gives — the
//! creating skill, then toward the task list, then the check — each run
//! unchanged through [`hosting::run_in`](crate::hosting::run_in). It stops
//! only where a person decides: open questions after the spec is written,
//! the person's review of the spec before anything is planned, a failure,
//! or the end of what was asked for.
//!
//! It holds no state of its own. Where it stopped is read again from the
//! unit's files and the review record; the continuation is the same
//! action from the next skill ([`resume`]).

use std::path::Path;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use speccoder_core::actions::{FeatureSource, Through};
use speccoder_core::similar::{self, Requirement, SimilarReport};
use speccoder_core::{
    ChainStep, ChainStop, CommandData, CommandResult, CommandStatus, PlanCheck,
    PlanningChainReport, Readiness,
};

use crate::RunOptions;

const COMMAND: &str = "spec";

/// The format's chain at `root`: its creating skill, the skills toward
/// the task list after it, and its check.
struct Chain {
    prefix: &'static str,
    creates: Option<&'static str>,
    toward: fn(bool) -> &'static [&'static str],
    checks: Option<&'static str>,
}

impl Chain {
    fn at(root: &Path) -> Self {
        let p = crate::format::at(root).planning();
        Self {
            prefix: p.prefix,
            creates: p.creates.first().copied(),
            toward: p.toward_tasks,
            checks: p.checks,
        }
    }

    fn full(&self, short: &str) -> String {
        format!("{}{short}", self.prefix)
    }

    /// The skills that remain after the spec, for `through`, given whether
    /// the unit has a plan and a task list.
    fn remaining(&self, through: Through, has_plan: bool, has_tasks: bool) -> Vec<&'static str> {
        let creates = self.creates;
        let toward: Vec<&'static str> = (self.toward)(has_plan)
            .iter()
            .copied()
            .filter(|s| Some(*s) != creates)
            .collect();
        let mut out: Vec<&'static str> = match through {
            Through::Spec => Vec::new(),
            // The plan only: every skill toward the tasks but the last, when
            // the format has a separate plan step.
            Through::Plan => {
                let all: Vec<&'static str> = (self.toward)(false)
                    .iter()
                    .copied()
                    .filter(|s| Some(*s) != creates)
                    .collect();
                if all.len() > 1 && !has_plan {
                    all[..all.len() - 1].to_vec()
                } else {
                    Vec::new()
                }
            }
            Through::Tasks | Through::Analyze if has_tasks => Vec::new(),
            Through::Tasks | Through::Analyze => toward,
        };
        if through == Through::Analyze {
            out.extend(self.checks);
        }
        out
    }

    /// Whether `short` is one of the chain's skills.
    fn has(&self, short: &str) -> bool {
        self.creates == Some(short)
            || (self.toward)(false).contains(&short)
            || (self.toward)(true).contains(&short)
            || self.checks == Some(short)
    }

    /// The chain's skills, installed names, for a usage error.
    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for s in self
            .creates
            .into_iter()
            .chain((self.toward)(false).iter().copied())
            .chain(self.checks)
        {
            let f = self.full(s);
            if !v.contains(&f) {
                v.push(f);
            }
        }
        v
    }
}

/// Whether `name` (installed or short) is a skill of the format's chain:
/// only those take `--through`; `Ok(true)` for the creating skill.
///
/// # Errors
///
/// `name` is not a skill of the chain: the usage error naming them.
pub fn takes_through(root: &Path, name: &str) -> Result<bool, String> {
    let chain = Chain::at(root);
    let short = name.strip_prefix(chain.prefix).unwrap_or(name);
    if chain.has(short) {
        Ok(short == chain.creates.unwrap_or_default())
    } else {
        Err(format!(
            "--through plans along the format's chain: {} — not `{name}`",
            chain.names().join(", ")
        ))
    }
}

/// The requirement text after its identifier (`- **FR-004**: Users …` →
/// `Users …`).
fn requirement_text(id: &str, line: &str) -> String {
    let after = line.split_once(id).map_or(line, |(_, rest)| rest);
    after
        .trim_start_matches(|c: char| c == '*' || c == ':' || c.is_whitespace())
        .trim()
        .to_owned()
}

/// The project's requirements: every unit's, the most recently changed
/// units first, at most [`similar::MAX_UNITS`] of them; and whether that
/// limit cut any off.
fn corpus(root: &Path) -> (Vec<Requirement>, bool) {
    let Ok(opened) = crate::format::open(root, None) else {
        return (Vec::new(), false);
    };
    let adapter = opened.adapter;
    let base = std::path::PathBuf::from(&opened.snapshot.root);
    let mut units: Vec<(std::time::SystemTime, String, String)> = opened
        .snapshot
        .features
        .iter()
        .map(|f| {
            let changed = std::fs::metadata(base.join(&f.dir))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (changed, f.name.clone(), f.dir.clone())
        })
        .collect();
    units.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let limited = units.len() > similar::MAX_UNITS;
    units.truncate(similar::MAX_UNITS);
    let mut out = Vec::new();
    for (_, name, dir) in units {
        if let Some(file) = adapter.requirements_file() {
            let text = std::fs::read_to_string(base.join(&dir).join(file)).unwrap_or_default();
            for (id, line) in adapter.requirements_in(&text) {
                out.push(Requirement {
                    unit: name.clone(),
                    text: requirement_text(&id, &line),
                    id,
                });
            }
        } else if let Ok(o) = crate::format::open(root, Some(&name)) {
            for r in adapter.oracle_requirements(&o.snapshot) {
                out.push(Requirement {
                    unit: name.clone(),
                    id: r.title.clone(),
                    text: format!("{} {}", r.title, r.text),
                });
            }
        }
    }
    (out, limited)
}

/// `speccoder feature similar "…"`: the requirements of the project close
/// to `text`. Read-only; no model.
#[must_use]
pub fn similar_to(root: &Path, text: &str) -> SimilarReport {
    let (corpus, limited) = corpus(root);
    SimilarReport {
        query: text.to_owned(),
        compared: corpus.len(),
        limited,
        matches: similar::matches(text, &corpus),
    }
}

/// [`similar_to`] as a command result.
#[must_use]
pub fn similar_command(root: &Path, text: &str) -> CommandResult {
    let report = similar_to(root, text);
    let message = match report.matches.first() {
        None => "no requirement is close".to_owned(),
        Some(m) => format!(
            "{} close: {} {} ({}, {:.2})",
            report.matches.len(),
            m.unit,
            m.requirement,
            m.level.word(),
            m.score
        ),
    };
    CommandResult::new("feature", CommandStatus::Ok, &message)
        .with_data(CommandData::Similar(Box::new(report)))
}

/// `speccoder suggest "…"`: the rung the text suggests, with the command
/// that takes it in this project's format.
#[must_use]
pub fn suggest(root: &Path, text: &str) -> CommandResult {
    let mut s = speccoder_core::ladder::suggest(text);
    let create = Chain::at(root).creates;
    s.command = speccoder_core::ladder::command_for(s.rung, text, create);
    let message = if s.rung == speccoder_core::ladder::RungPick::None {
        s.reason.clone()
    } else {
        format!("{} — {}", s.rung.label(), s.reason)
    };
    let next: Vec<speccoder_core::next::NextStep> = s
        .command
        .iter()
        .map(|c| speccoder_core::next::NextStep {
            command: c.clone(),
            why: format!("take the {} rung", s.rung.label().to_lowercase()),
        })
        .collect();
    CommandResult::new("suggest", CommandStatus::Ok, &message)
        .with_data(CommandData::RungSuggestion(Box::new(s)))
        .with_next(next)
}

/// The unit's directory name, and whether it has a plan and a task list.
fn unit_facts(root: &Path, feature: Option<&str>) -> Option<(String, bool, bool)> {
    let opened = crate::format::open(root, feature).ok()?;
    let f = opened.snapshot.feature.as_ref()?;
    Some((f.feature.name.clone(), f.artifacts.plan, f.artifacts.tasks))
}

/// The plan checks of the unit's task list (spec 102), as the stop carries them.
fn plan_checks(root: &Path, unit: &str) -> Vec<PlanCheck> {
    let Ok(o) = crate::format::open(root, Some(unit)) else {
        return Vec::new();
    };
    if o.snapshot
        .feature
        .as_ref()
        .is_none_or(|f| f.tasks.is_none())
    {
        return Vec::new();
    }
    crate::plan_lint::lints(o.adapter, &o.snapshot, root)
        .into_iter()
        .map(|l| PlanCheck {
            code: l.code.to_owned(),
            tasks: l.tasks,
            detail: l.detail,
        })
        .collect()
}

fn step_of(result: &CommandResult, skill: &str) -> ChainStep {
    let run_id = match result.data() {
        Some(CommandData::Skill(r)) => Some(r.run_id.clone()).filter(|r| !r.is_empty()),
        _ => None,
    };
    ChainStep {
        skill: skill.to_owned(),
        status: result.status(),
        message: result.message().to_owned(),
        run_id,
    }
}

/// What the person must decide before planning, read from the unit's
/// files and the review record: the open questions, else the requirements
/// not yet approved on their current wording (`None`: nothing).
fn gate(root: &Path, unit: &str) -> Option<ChainStop> {
    let review = crate::review::summary(root, Some(unit))?;
    if !review.questions.is_empty() {
        return Some(ChainStop::Questions {
            questions: review.questions.iter().map(|q| q.id.clone()).collect(),
        });
    }
    (!review.totals.complete() && !review.requirements.is_empty()).then(|| ChainStop::Review {
        requirements: review.requirements.iter().map(|r| r.id.clone()).collect(),
    })
}

/// The result of a stop: its message, status and code, worded for the
/// unit.
fn finish(root: &Path, mut report: PlanningChainReport, options: &RunOptions) -> CommandResult {
    let unit = report.unit.clone();
    report.readiness = unit
        .as_deref()
        .and_then(|u| crate::review::readiness(root, Some(u)));
    let chain = Chain::at(root);
    let facts = unit.as_deref().and_then(|u| unit_facts(root, Some(u)));
    let (has_plan, has_tasks) = facts.as_ref().map_or((false, false), |f| (f.1, f.2));
    report.next_skill = match report.stop {
        ChainStop::Done { .. } => None,
        ChainStop::Failed { ref skill, .. } => Some(skill.clone()),
        ChainStop::Questions { .. } | ChainStop::Review { .. } => chain
            .remaining(report.through, has_plan, has_tasks)
            .first()
            .map(|s| chain.full(s)),
    };
    if let Some(p) = &options.progress {
        p.event(&crate::progress::Event::PlanningStopped {
            unit: unit.clone(),
            kind: report.stop.as_str().to_owned(),
            next_skill: report.next_skill.clone(),
        });
    }
    let unit_name = unit.clone().unwrap_or_default();
    let (status, code, message) = match &report.stop {
        ChainStop::Questions { questions } => (
            CommandStatus::NeedsHuman,
            Some("THROUGH-QUESTIONS"),
            format!(
                "{} open question{} in {unit_name}: answer {}, then review the spec",
                questions.len(),
                if questions.len() == 1 { "" } else { "s" },
                if questions.len() == 1 { "it" } else { "them" }
            ),
        ),
        ChainStop::Review { requirements } => (
            CommandStatus::NeedsHuman,
            Some("THROUGH-REVIEW"),
            format!(
                "review the spec — {} requirement{}; approve them to plan",
                requirements.len(),
                if requirements.len() == 1 { "" } else { "s" }
            ),
        ),
        ChainStop::Failed { skill, message, .. } => (
            CommandStatus::RunFailed,
            None,
            format!("{skill} failed: {message}"),
        ),
        ChainStop::Done { plan_checks } => {
            let steps = ["spec", "plan", "tasks"]
                .iter()
                .filter(|s| match **s {
                    "spec" => true,
                    "plan" => has_plan,
                    _ => has_tasks,
                })
                .map(|s| format!("{s} ✓"))
                .collect::<Vec<_>>()
                .join(" ");
            let checks = if plan_checks.is_empty() {
                String::new()
            } else {
                format!(
                    "; {} plan check{} to look at",
                    plan_checks.len(),
                    if plan_checks.len() == 1 { "" } else { "s" }
                )
            };
            (
                CommandStatus::Ok,
                None,
                format!(
                    "{steps} in {unit_name}{checks} — planning files are left for you to commit"
                ),
            )
        }
    };
    let next = report.next_skill.clone();
    let through = report.through;
    let mut result = CommandResult::new(COMMAND, status, &message)
        .with_data(CommandData::PlanningChain(Box::new(report)));
    if let Some(code) = code {
        result = result.with_code(code);
        let mut fix = Vec::new();
        if code == "THROUGH-QUESTIONS" {
            fix.push("speccoder review".to_owned());
        } else {
            fix.push(format!("speccoder review approve --feature {unit_name}"));
        }
        if let Some(n) = next {
            let short = n.strip_prefix(chain.prefix).unwrap_or(&n).to_owned();
            fix.push(format!(
                "speccoder spec {short} --through {} --feature {unit_name}",
                through.as_str()
            ));
        }
        result = result.with_fix(fix);
    }
    result
}

fn report(through: Through, source: FeatureSource) -> PlanningChainReport {
    PlanningChainReport {
        unit: None,
        through,
        ran: Vec::new(),
        stop: ChainStop::Done {
            plan_checks: Vec::new(),
        },
        readiness: None,
        similar: Vec::new(),
        source,
        next_skill: None,
    }
}

/// Runs the skills of `todo` one by one; stops at the first failure.
#[allow(clippy::too_many_arguments)]
fn run_steps(
    root: &Path,
    unit: &str,
    todo: &[&'static str],
    chain: &Chain,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
    r: &mut PlanningChainReport,
) -> bool {
    for short in todo {
        let name = chain.full(short);
        let result = crate::hosting::run_in(
            root,
            Some(unit),
            &name,
            "",
            state,
            Arc::clone(provider),
            secret,
            options,
        );
        r.ran.push(step_of(&result, &name));
        if result.status() != CommandStatus::Ok {
            r.stop = ChainStop::Failed {
                skill: name,
                message: result.message().to_owned(),
                code: result.code().map(|c| c.name.to_owned()),
            };
            return false;
        }
    }
    true
}

/// The `issue` origin of the new unit, written once.
fn record_issue(root: &Path, state: &Path, unit: &str, number: u64) {
    let repo = crate::github::repo(root, None).ok().map(|r| r.slug());
    let plane = crate::worktree::project_state_dir(state, root);
    let _ = crate::ladder::record_origin(
        &plane,
        unit,
        vec![speccoder_core::ladder::Origin::Issue {
            number,
            repo,
            context_sha256: None,
        }],
    );
}

/// Plan through from a description, against `state` with `provider`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn start_with(
    root: &Path,
    description: &str,
    source: &FeatureSource,
    through: Through,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let description = description.trim();
    if description.is_empty() {
        return CommandResult::new("job", CommandStatus::UsageError, "a description is needed");
    }
    let chain = Chain::at(root);
    let Some(creates) = chain.creates else {
        let format = crate::format::at(root);
        return CommandResult::new(
            COMMAND,
            CommandStatus::UsageError,
            &format!(
                "{} has no skill that creates a {}: {}",
                format.wording().name,
                format.messages().unit,
                format.messages().make_project.1
            ),
        );
    };
    let mut r = report(through, source.clone());
    r.similar = similar_to(root, description).matches;
    if let (Some(m), Some(p)) = (r.similar.first(), &options.progress) {
        p.event(&crate::progress::Event::Notice {
            text: format!(
                "warning[SIMILAR-FEATURE]: close to {} {} \"{}\" — to extend it instead: speccoder feature {} && speccoder review comments",
                m.unit, m.requirement, m.text, m.unit
            ),
        });
    }
    let name = chain.full(creates);
    let made = crate::hosting::run_in(
        root,
        None,
        &name,
        description,
        state,
        Arc::clone(provider),
        secret,
        options,
    );
    r.ran.push(step_of(&made, &name));
    if made.status() != CommandStatus::Ok {
        r.stop = ChainStop::Failed {
            skill: name,
            message: made.message().to_owned(),
            code: made.code().map(|c| c.name.to_owned()),
        };
        return finish(root, r, options);
    }
    let Some((unit, has_plan, has_tasks)) = unit_facts(root, None) else {
        r.stop = ChainStop::Failed {
            skill: name,
            message: "the skill ended without a unit speccoder can find".to_owned(),
            code: None,
        };
        return finish(root, r, options);
    };
    if let FeatureSource::Issue { number } = source {
        record_issue(root, state, &unit, *number);
    }
    r.unit = Some(unit.clone());
    r.stop = match gate(root, &unit) {
        Some(q @ ChainStop::Questions { .. }) => q,
        Some(review) if through != Through::Spec => review,
        _ if through == Through::Spec => ChainStop::Done {
            plan_checks: Vec::new(),
        },
        // Nothing to approve (no requirement ids): planning still waits
        // for the person, unless nothing is left to run.
        _ if chain.remaining(through, has_plan, has_tasks).is_empty() => ChainStop::Done {
            plan_checks: plan_checks(root, &unit),
        },
        _ => ChainStop::Review {
            requirements: Vec::new(),
        },
    };
    finish(root, r, options)
}

/// Plan through from a description with the configured model.
#[must_use]
pub fn start(
    root: &Path,
    description: &str,
    source: &FeatureSource,
    through: Through,
    options: &RunOptions,
) -> CommandResult {
    if description.trim().is_empty() {
        return CommandResult::new("job", CommandStatus::UsageError, "a description is needed");
    }
    crate::with_driver(COMMAND, root, None, |state, provider, key| {
        start_with(
            root,
            description,
            source,
            through,
            state,
            provider,
            Some(key),
            options,
        )
    })
}

/// The continuation after a stop: the next skills of the chain, derived
/// from the unit's files — refused while a question is open or a
/// requirement is not approved on its current wording.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn resume_with(
    root: &Path,
    feature: Option<&str>,
    through: Through,
    state: &Path,
    provider: &Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let chain = Chain::at(root);
    let Some((unit, has_plan, has_tasks)) = unit_facts(root, feature) else {
        return CommandResult::new(
            COMMAND,
            CommandStatus::SelectionNeeded,
            "no feature to plan: start one with a description",
        );
    };
    let mut r = report(through, FeatureSource::Description);
    r.unit = Some(unit.clone());
    if let Some(stop) = gate(root, &unit) {
        r.stop = stop;
        return finish(root, r, options);
    }
    let todo = chain.remaining(through, has_plan, has_tasks);
    if run_steps(
        root, &unit, &todo, &chain, state, provider, secret, options, &mut r,
    ) {
        r.stop = ChainStop::Done {
            plan_checks: plan_checks(root, &unit),
        };
    }
    finish(root, r, options)
}

/// The continuation with the configured model.
#[must_use]
pub fn resume(
    root: &Path,
    feature: Option<&str>,
    through: Through,
    options: &RunOptions,
) -> CommandResult {
    // Refused before any model is opened: the gate needs none.
    if let Some((unit, _, _)) = unit_facts(root, feature)
        && let Some(stop) = gate(root, &unit)
    {
        let mut r = report(through, FeatureSource::Description);
        r.unit = Some(unit);
        r.stop = stop;
        return finish(root, r, options);
    }
    crate::with_driver(COMMAND, root, feature, |state, provider, key| {
        resume_with(root, feature, through, state, provider, Some(key), options)
    })
}

/// The readiness of the unit (a re-export for the page's strip).
#[must_use]
pub fn readiness(root: &Path, feature: Option<&str>) -> Option<Readiness> {
    crate::review::readiness(root, feature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speckit() -> Chain {
        Chain {
            prefix: "speckit-",
            creates: Some("specify"),
            toward: |plan| if plan { &["tasks"] } else { &["plan", "tasks"] },
            checks: Some("analyze"),
        }
    }

    fn openspec() -> Chain {
        Chain {
            prefix: "openspec-",
            creates: Some("propose"),
            toward: |_| &["propose"],
            checks: None,
        }
    }

    #[test]
    fn the_remaining_skills_follow_the_format_and_the_files() {
        let s = speckit();
        assert_eq!(s.remaining(Through::Spec, false, false), Vec::<&str>::new());
        assert_eq!(s.remaining(Through::Plan, false, false), vec!["plan"]);
        assert_eq!(s.remaining(Through::Plan, true, false), Vec::<&str>::new());
        assert_eq!(
            s.remaining(Through::Tasks, false, false),
            vec!["plan", "tasks"]
        );
        assert_eq!(s.remaining(Through::Tasks, true, false), vec!["tasks"]);
        assert_eq!(s.remaining(Through::Tasks, true, true), Vec::<&str>::new());
        assert_eq!(s.remaining(Through::Analyze, true, true), vec!["analyze"]);
        assert_eq!(
            s.remaining(Through::Analyze, false, false),
            vec!["plan", "tasks", "analyze"]
        );
        let o = openspec();
        assert_eq!(
            o.remaining(Through::Tasks, false, false),
            Vec::<&str>::new()
        );
        assert_eq!(
            o.remaining(Through::Analyze, false, false),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn only_the_chain_takes_through() {
        let s = speckit();
        assert!(s.has("specify") && s.has("plan") && s.has("tasks") && s.has("analyze"));
        assert!(!s.has("clarify") && !s.has("checklist"));
        assert_eq!(
            s.names(),
            vec![
                "speckit-specify",
                "speckit-plan",
                "speckit-tasks",
                "speckit-analyze"
            ]
        );
    }

    #[test]
    fn a_requirement_line_loses_its_identifier() {
        assert_eq!(
            requirement_text("FR-004", "- **FR-004**: Users can export a report as PDF."),
            "Users can export a report as PDF."
        );
        assert_eq!(requirement_text("FR-1", "nothing"), "nothing");
    }
}
