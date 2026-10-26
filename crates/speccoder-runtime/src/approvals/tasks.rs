//! A person's decision as an agentplane human task.
//!
//! Where a call needs a person, the attempt waits on a task whose
//! justification is the call exactly as the prompt shows it. Whoever answers
//! — at the terminal, in an editor, or later with `speccoder task approve` —
//! decides that task with an operator; the decision is journal history,
//! bound to the digest of what was shown.

use agentplane::core::{Decided, Decision, Justification, Operator, Tainted, TaskSpec};
use serde_json::{Value, json};

use super::{Answer, Decision as Said, PresentContext, Presentation, Request, Scope, Who};

/// The obligation every wait for a person is bounded by.
pub const DEADLINE: &str = "approval";

/// The task kind of a call waiting for approval.
pub const APPROVAL: &str = "approval";

/// The task kind of a question to a person.
pub const QUESTION: &str = "question";

/// The waiting task for a call: the exact call, the question as the
/// journal holds it, and the class an approval for the task or run would
/// create. Only journaled facts go in — the approval is bound to this
/// task's digest, so replay must rebuild it byte for byte; the presentation
/// (the file as it is, the sandbox) is built live by whoever asks.
#[must_use]
pub fn approval_task(
    request: &Request,
    grant: Option<&crate::tools::policy::Grant>,
    log: &[Value],
) -> TaskSpec {
    TaskSpec::new(
        APPROVAL,
        Justification::new(
            Tainted::trusted(format!("{} — {}", request.task, request.covers)),
            json!({
                "tool": request.tool,
                "arguments": request.arguments,
                "task": request.task,
                "grant": grant,
                "request": request,
                // What the attempt did before it stopped here, as journaled.
                "log": log,
            }),
        ),
        DEADLINE,
    )
}

/// The waiting task for a question from `task` (`skill/<name>`).
#[must_use]
pub fn question_task(task: &str, question: &str) -> TaskSpec {
    TaskSpec::new(
        QUESTION,
        Justification::new(
            Tainted::trusted(format!("{task} asks")),
            json!({ "task": task, "question": question }),
        ),
        DEADLINE,
    )
}

/// The question a waiting approval task carries.
#[must_use]
pub fn request_of(proposed_action: &Value) -> Option<Request> {
    serde_json::from_value(proposed_action["request"].clone()).ok()
}

/// The waiting call as the person sees it here, now.
#[must_use]
pub fn presentation_of(proposed_action: &Value, context: &PresentContext) -> Option<Presentation> {
    request_of(proposed_action).map(|r| super::present(&r, context))
}

/// A person's answer as the decision recorded for its task: the scope, note,
/// channel and any answer text travel in the amendment.
#[must_use]
pub fn decision_for(answer: &Answer, by: &Who, text: Option<&str>) -> Option<Decision> {
    let operator: Operator = by.operator()?;
    let mut amendment = json!({ "scope": answer.scope.name(), "via": by.via });
    if let Some(client) = &by.client {
        amendment["client"] = json!(client);
    }
    if let Some(note) = &answer.note {
        amendment["note"] = json!(note);
    }
    if let Some(text) = text {
        amendment["answer"] = json!(text);
    }
    Some(
        match answer.decision {
            Said::Approve => Decision::approve(operator, answer.note.clone().unwrap_or_default()),
            Said::Reject | Said::Stop => Decision::reject(
                operator,
                answer
                    .reason
                    .clone()
                    .unwrap_or_else(|| "no reason given".to_owned()),
            ),
        }
        .amend(amendment),
    )
}

/// Asks `approver` about a waiting `task` now. `None` when the person
/// answers "later" (the call keeps waiting) or the task is not one of ours.
#[must_use]
pub fn ask(
    approver: &dyn super::Approver,
    task: &agentplane::core::Task,
    context: &PresentContext,
) -> Option<Decision> {
    let action = &task.justification.proposed_action;
    match task.kind.as_str() {
        APPROVAL => {
            let answer = approver.ask(&presentation_of(action, context)?);
            if answer.decision == Said::Stop {
                return None;
            }
            decision_for(&answer, &named(answer.by.clone(), approver, context), None)
        }
        QUESTION => {
            let by = named(None, approver, context);
            match approver.answer(action["question"].as_str()?) {
                Some(text) => decision_for(
                    &Answer::approve(Scope::Once, None, Some(by.clone())),
                    &by,
                    Some(&text),
                ),
                None => decision_for(
                    &Answer::reject("declined to answer", Some(by.clone())),
                    &by,
                    None,
                ),
            }
        }
        _ => None,
    }
}

/// Who answered, named as git names them in the project they decide for.
fn named(by: Option<Who>, approver: &dyn super::Approver, context: &PresentContext) -> Who {
    let mut by = by.unwrap_or_else(|| match approver.client() {
        Some(client) => Who::connected(&client),
        None => Who::local(approver.via()),
    });
    if let Some(root) = &context.root {
        by.name = super::who_in(root);
    }
    by
}

/// What the attempt reads from a recorded decision. An expiry is a refusal
/// nobody made: `by` is `None` and `expired` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The answer as the tool loop applies it.
    pub answer: Answer,
    /// Who decided; `None` for an expiry.
    pub by: Option<Who>,
    /// The deadline passed and the call was denied.
    pub expired: bool,
    /// A question's answer text.
    pub text: Option<String>,
}

/// Reads a recorded decision back.
#[must_use]
pub fn read(d: &Decision) -> Read {
    let a = &d.amendment;
    let scope = match a["scope"].as_str() {
        Some("once") => Scope::Once,
        Some("run") => Scope::Run,
        _ => Scope::Task,
    };
    // Read as every record is (`cli`, `editor <name>` before origins were
    // one vocabulary).
    let by = d.decided.operator().and_then(|op| {
        serde_json::from_value::<Who>(json!({
            "name": op.actor(),
            "via": a["via"].as_str().unwrap_or("terminal"),
            "client": a["client"],
            "basis": op.basis().to_string(),
        }))
        .ok()
    });
    let expired = matches!(d.decided, Decided::OnExpiry(_));
    let answer = if d.approved {
        Answer::approve(scope, a["note"].as_str().map(str::to_owned), by.clone())
    } else {
        Answer::reject(
            if expired {
                "not approved before its deadline"
            } else {
                d.reason.as_str()
            },
            by.clone(),
        )
    };
    Read {
        answer,
        by,
        expired,
        text: a["answer"].as_str().map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who() -> Who {
        Who {
            name: "dana".to_owned(),
            via: "terminal".to_owned(),
            client: None,
            basis: "asserted".to_owned(),
        }
    }

    #[test]
    fn every_scope_and_note_survive_the_record() {
        for scope in [Scope::Once, Scope::Task, Scope::Run] {
            let answer = Answer::approve(scope, Some("keep it small".to_owned()), Some(who()));
            let d = decision_for(&answer, &who(), None).unwrap();
            let back = read(&d);
            assert_eq!(back.answer.scope, scope);
            assert_eq!(back.answer.note.as_deref(), Some("keep it small"));
            assert_eq!(back.by, Some(who()));
            assert!(!back.expired);
        }
        let rejected = Answer::reject("not that file", Some(who()));
        let back = read(&decision_for(&rejected, &who(), None).unwrap());
        assert_eq!(back.answer.decision, Said::Reject);
        assert_eq!(back.answer.reason.as_deref(), Some("not that file"));
    }

    #[test]
    fn an_expiry_is_a_refusal_nobody_made() {
        let d = Decision::expired(agentplane::core::OnExpiry::Deny);
        let back = read(&d);
        assert!(back.expired && back.by.is_none());
        assert_eq!(back.answer.decision, Said::Reject);
    }

    #[test]
    fn the_task_carries_the_exact_call() {
        let request = super::super::Request {
            task: "001-x/T001".to_owned(),
            task_title: "t".to_owned(),
            turn: 1,
            tool: "write_file".to_owned(),
            arguments: json!({ "path": "src/a.rs", "content": "fn a() {}\n" }),
            kind: "write".to_owned(),
            reason: "r".to_owned(),
            covers: "write `src/a.rs`".to_owned(),
            risk: "low".to_owned(),
            claim: None,
            options: vec!["once".to_owned()],
        };
        let spec = approval_task(&request, None, &[]);
        assert_eq!(spec.kind, APPROVAL);
        let action = &spec.justification.proposed_action;
        assert_eq!(action["arguments"]["content"], "fn a() {}\n");
        assert_eq!(request_of(action), Some(request.clone()));
        // Only journaled facts: the same request gives the same digest.
        assert_eq!(
            spec.justification.digest(),
            approval_task(&request, None, &[]).justification.digest()
        );
    }
}
