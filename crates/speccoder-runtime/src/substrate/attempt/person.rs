//! The person in an attempt: the honest exit checked against its evidence,
//! the questions a call or a skill puts to them, and their answers.

use super::*;

/// What a `report_blocked` call is checked against: its evidence must be
/// there, never the model's word alone.
#[derive(Default)]
pub(crate) struct StopGate {
    /// The spec text the prompt carried: what a `spec_conflict` quotes.
    pub spec: String,
    /// The tests frozen when the attempt started: what a `wrong_test` names.
    pub frozen: Vec<String>,
    /// A person rejected a call for this task before.
    pub rejected: bool,
}

/// What speccoder does with a `report_blocked` call.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Stop {
    /// The evidence is not there: the attempt goes on, told why.
    Refused(String),
    /// Asked once before stopping, with the turns left.
    Asked(String),
    /// The attempt stops; the reason the person reads.
    Stopped(String),
}

/// Judges a stop by its evidence: a refused call stops at once; a quote,
/// a frozen test or a question is asked about once first.
pub(super) fn judge_stop(
    gate: &StopGate,
    arguments: &Value,
    refused: bool,
    asked: bool,
    turns_left: u32,
) -> Stop {
    let text = |k: &str| arguments[k].as_str().unwrap_or_default().trim();
    let (kind, evidence, reason) = (text("kind"), text("evidence"), text("reason"));
    let words = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let checked = match kind {
        "refused" if refused => format!("refused call `{evidence}`"),
        "refused" => return Stop::Refused("not stopped: no call in this attempt was refused or rejected".to_owned()),
        "spec_conflict" if evidence.chars().count() >= 12 && words(&gate.spec).contains(&words(evidence)) => {
            format!("the spec says “{evidence}”")
        }
        "spec_conflict" => {
            return Stop::Refused("not stopped: the evidence must be the spec's exact words, at least a phrase; these are not in it".to_owned());
        }
        "wrong_test" if gate.frozen.iter().any(|f| f == evidence.trim_start_matches("./")) => {
            format!("frozen test `{evidence}`")
        }
        "wrong_test" => return Stop::Refused("not stopped: the evidence must be the path of a test that existed when the attempt started".to_owned()),
        "needs_information" if !evidence.is_empty() => format!("question: {evidence}"),
        _ => return Stop::Refused("not stopped: `kind` is one of refused, spec_conflict, wrong_test, needs_information, with its `evidence`".to_owned()),
    };
    // The model's reason first, then what speccoder checked.
    let said = match reason {
        "" => checked,
        r if r == evidence => r.to_owned(),
        r => format!("{r} — {checked}"),
    };
    // Asked once only, and never on the last turn: no answer could follow.
    if kind == "refused" || asked || turns_left == 0 {
        return Stop::Stopped(said);
    }
    Stop::Asked(format!(
        "Not stopped yet: speccoder asks once before a stop. {turns_left} turns are left. The task's paths are the plan's guess: wiring an entry point, caller or import the tests run to the task's module is part of the task, and a failing check is a reason to keep going. If the task still cannot be done as written, call report_blocked again."
    ))
}

/// The question tool a hosted skill may use.
pub const ASK_PERSON: &str = "ask_person";

/// The `ask_person` declaration.
#[must_use]
pub fn ask_person() -> ToolDeclaration {
    ToolDeclaration::new(
        ASK_PERSON,
        "Ask the person one question and wait for their answer. If nobody can answer, the reply says so.",
        crate::substrate::answers::schema::<crate::substrate::answers::Question>(),
    )
}

/// How an attempt waits for a person when a call needs one: on an
/// agentplane human task the person decides — inline, from an editor, or
/// later with `speccoder task approve` — while the run is suspended.
#[derive(Debug, Clone)]
pub struct Asker {
    /// `feature/task`, shown to the person.
    pub task: String,
    /// The task's title.
    pub task_title: String,
    /// What the live run knows for the presentation.
    pub context: PresentContext,
    /// How long a call waits before it is denied as expired.
    pub wait_secs: u64,
}

impl Asker {
    /// The asker the journaled settings describe: `asks = "tasks"` waits on
    /// a task; anything else (`"stop"`, a lane) stops the attempt instead.
    #[must_use]
    pub fn from_settings(
        settings: &Value,
        task: String,
        task_title: String,
        context: PresentContext,
    ) -> Option<Self> {
        (settings["asks"] == "tasks").then(|| Self {
            task,
            task_title,
            context,
            wait_secs: settings["wait_secs"].as_u64().unwrap_or(7 * 86_400),
        })
    }
}

/// Waits for a person's decision on `spec`, registering the `approval`
/// obligation the first time: the run suspends until someone decides or the
/// deadline passes. Journaled — replay reads the decision back.
pub(super) async fn wait_for_person(
    cx: &mut StepCtx<'_>,
    asker: &Asker,
    obligation: &mut bool,
    spec: &agentplane::core::TaskSpec,
) -> Result<agentplane::core::Decision, SkillError> {
    if !*obligation {
        let minutes = u32::try_from(asker.wait_secs.div_ceil(60).max(1)).unwrap_or(u32::MAX);
        cx.deadline(
            crate::approvals::tasks::DEADLINE,
            &agentplane::core::DeadlineSpec::minutes(minutes),
            None,
        )
        .await?;
        *obligation = true;
    }
    Ok(cx.task(spec).await?)
}

/// A person is needed for `asked`: the run tells whoever watches.
pub(super) fn needs_person(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    asked: &agentplane::model::ToolCall,
    object: Option<&Object>,
) {
    if let Some((task, p)) = observer {
        let (tool, arguments) = (asked.name.as_str(), &asked.arguments);
        let target = crate::progress::target(tool, arguments);
        let what: String = format!("{tool} {target}")
            .chars()
            .filter(|c| !c.is_control())
            .take(60)
            .collect();
        p.event(&crate::progress::Event::ApprovalRequested {
            task: task.clone(),
            call: asked.id.clone(),
            tool: tool.to_owned(),
            what,
            class: object
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.map(Object::short).unwrap_or_default(),
        });
    }
}

/// A person decided `asked`: the run tells whoever watches.
pub(super) fn person_decided(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    asked: &agentplane::model::ToolCall,
    decision: &str,
    scope: Option<&str>,
    who: Option<&Who>,
    object: Option<&Object>,
) {
    if let Some((task, p)) = observer {
        p.event(&crate::progress::Event::ApprovalDecided {
            task: task.clone(),
            call: asked.id.clone(),
            decision: decision.to_owned(),
            scope: scope.map(str::to_owned),
            by: who.map_or_else(|| "nobody (expired)".to_owned(), |w| w.name.clone()),
            how: who.map_or_else(|| "expired".to_owned(), |w| w.basis.clone()),
            class: object
                .map(|o| o.class().as_str().to_owned())
                .unwrap_or_default(),
            object: object.map(Object::short).unwrap_or_default(),
        });
    }
}

/// What a call needing a person is asked by.
pub(super) struct Question<'a> {
    pub(super) asked: &'a agentplane::model::ToolCall,
    pub(super) reason: &'a str,
    pub(super) turn: u32,
    pub(super) said: &'a str,
    pub(super) asker: Option<&'a Asker>,
    pub(super) policy: &'a Policy,
    pub(super) object: Option<&'a Object>,
    pub(super) remembered: &'a [(Object, &'static str)],
    pub(super) parts: &'a [crate::approvals::derived::Part],
    pub(super) run: Option<&'a str>,
}

/// The journaled question for a call needing a person: its object, what
/// changed since an approval of the same name, where its arguments were
/// copied from, and the choices that leaves — all from journaled data.
pub(super) fn question(q: &Question<'_>) -> Request {
    let (tool, arguments) = (q.asked.name.as_str(), &q.asked.arguments);
    let kind = crate::approvals::kind_of(tool, arguments);
    let risk = crate::approvals::risk(tool, q.reason);
    // Always Allow commits an exact entry: judged without the network.
    let always_risk = if kind == "network" {
        let mut bare = arguments.clone();
        if let Some(o) = bare.as_object_mut() {
            o.remove("network");
        }
        match q.policy.base(tool, &bare) {
            Decision::Allow => "low",
            Decision::Approval(r) => crate::approvals::risk(tool, &r),
            Decision::Deny(_) => "high",
        }
    } else {
        risk
    };
    let (sources, more) = crate::approvals::derived::sources(tool, arguments, q.parts);
    let changed = q.object.and_then(|o| {
        q.remembered
            .iter()
            .map(|(r, _)| r)
            .chain(q.policy.approved())
            .find_map(|earlier| o.changed_from(earlier))
    });
    Request {
        task: q.asker.map(|a| a.task.clone()).unwrap_or_default(),
        task_title: q.asker.map(|a| a.task_title.clone()).unwrap_or_default(),
        turn: q.turn,
        tool: tool.to_owned(),
        arguments: arguments.clone(),
        kind: kind.to_owned(),
        reason: q.reason.to_owned(),
        object: q.object.cloned(),
        changed,
        risk: risk.to_owned(),
        claim: crate::approvals::claim(q.said),
        options: crate::approvals::options(
            risk,
            q.object.is_some(),
            !sources.is_empty(),
            kind,
            always_risk,
        ),
        sources,
        more,
        run: q.run.map(str::to_owned),
        asked: 0,
        budget: 0,
        batched: false,
    }
}
