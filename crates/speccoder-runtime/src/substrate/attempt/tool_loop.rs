//! The governed tool loop shared by attempts and convergence.

use super::*;

/// The governed tool loop shared by attempts and convergence: every call is
/// classified first (deny → refused and told; approval → stop; allow →
/// dispatched through agentplane); the closing structured answer ends it.
pub(crate) async fn tool_loop(
    cx: &mut StepCtx<'_>,
    prompt: &Tainted<Value>,
    spec: LoopSpec,
) -> Result<Outcome, SkillError> {
    // What the conversation has read, with its labels: agentplane checks
    // every model call against the join.
    let mut transcript = Transcript::new(prompt);
    tool_turn(cx, prompt, spec, &mut transcript).await
}

/// [`tool_loop`] continuing a conversation the caller holds: Ask's turns
/// share one transcript, each person's message told before the turn.
pub(crate) async fn tool_turn(
    cx: &mut StepCtx<'_>,
    prompt: &Tainted<Value>,
    spec: LoopSpec,
    transcript: &mut Transcript,
) -> Result<Outcome, SkillError> {
    let LoopSpec {
        tools,
        policy,
        max_turns,
        max_tool_calls,
        reasoning,
        schema,
        routes,
        root,
        frozen_additions,
        finish,
        ask,
        run_approved,
        mut once,
        observer,
        run,
        asking,
        mut parts,
        mcp_digests,
        stop,
        max_tokens,
        left,
        reads,
        halt,
    } = spec;
    // What an earlier pass already showed is not shown again: after a
    // person's answer the run is invoked again and every journaled call
    // returns again (set after each journaled step, from `cx.replaying()`).
    let replaying = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(cx.replaying()));
    let observer = observer.map(|(task, next)| {
        let quiet: std::sync::Arc<dyn crate::progress::Progress> = std::sync::Arc::new(Quiet {
            replaying: std::sync::Arc::clone(&replaying),
            next,
        });
        (task, quiet)
    });
    let seen = |cx: &StepCtx<'_>| {
        replaying.store(cx.replaying(), std::sync::atomic::Ordering::SeqCst);
    };
    // The next read id this turn issues, when reads are cited.
    let mut next_read = reads;
    let mut proposed = false;
    // Objects allowed without asking: this run's, then what a person
    // answered "for the task" or "for this run" in this attempt.
    let mut remembered: Vec<(Object, &'static str)> =
        run_approved.into_iter().map(|g| (g, "run-grant")).collect();
    let mut prompted = asking.asked;
    let mut told_budget = false;
    let catalog = crate::tools::catalog();

    let mut conversation: Label = transcript.label().clone();
    let mut calls = 0u32;
    let mut denied_in_a_row = 0u32;
    // Whether the model was asked once before a stop.
    let mut asked_to_stop = false;
    // Whether this attempt registered the `approval` obligation yet.
    let mut obligation = false;
    let mut usage = agentplane::model::Usage::default();
    let mut log: Vec<Value> = Vec::new();

    let end = |status: &str,
               extra: Value,
               log: &[Value],
               usage: agentplane::model::Usage,
               conversation: &Label,
               prompted: u32| {
        let mut out = json!({
            "status": status,
            "tool_calls": log,
            "asked": prompted,
            "usage": {
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
                "cache_read_tokens": usage.cache_read_tokens,
                "cache_write_tokens": usage.cache_write_tokens,
            },
        });
        if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        Ok(Outcome::done(Tainted::with_label(
            out,
            conversation.clone(),
        )))
    };

    for turn in 0..max_turns {
        // A turn's own token limit, checked before each call.
        if let Some(max) = max_tokens
            && usage.input_tokens + usage.output_tokens >= max
        {
            return end(
                "budget",
                json!({ "reason": format!("this turn used {} of its {max} tokens", usage.input_tokens + usage.output_tokens) }),
                &log,
                usage,
                &conversation,
                prompted,
            );
        }
        // The run's cap: what is left of it, checked before each call.
        if let Some(l) = left
            && l.reached(&usage)
        {
            return end(
                "budget_exhausted",
                json!({ "reason": l.reason(&usage) }),
                &log,
                usage,
                &conversation,
                prompted,
            );
        }
        // The prompt was built from labelled parts (`/system` is speccoder's
        // constant instruction); what the conversation read since rides in
        // the transcript, so the request is labelled by its real sources.
        let tools_now = tools.clone();
        let schema_now = schema.clone();
        let read_so_far = &*transcript;
        let stop_now = halt.clone();
        let completion = cx
            .complete_with(prompt, move |call| {
                let call = call.with_tools(tools_now).continuing(read_so_far);
                let call = match stop_now {
                    Some(stop) => call.stopped_by(stop),
                    None => call,
                };
                let call = match schema_now {
                    Some(schema) => call.expecting(schema),
                    None => call,
                };
                if reasoning {
                    call.with_reasoning_summary()
                } else {
                    call
                }
            })
            .await?;
        seen(cx);
        conversation = transcript.label().join(completion.label());
        usage = usage.plus(completion.peek().usage);
        if let Some((task, p)) = &observer {
            p.event(&crate::progress::Event::Usage {
                task: task.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_read_tokens: usage.cache_read_tokens,
                cache_write_tokens: usage.cache_write_tokens,
                usd: None,
            });
        }

        // The model's summary of its reasoning, when asked and given: its
        // account of why, a claim like its words.
        if let (Some((task, p)), Some(reasoning)) =
            (&observer, completion.peek().reasoning.as_deref())
            && !reasoning.trim().is_empty()
        {
            let text: String = crate::approvals::present::escape(reasoning.trim())
                .chars()
                .take(4000)
                .collect();
            p.event(&crate::progress::Event::Reasoning {
                task: task.clone(),
                turn: turn + 1,
                text,
            });
        }
        if completion.peek().stopped
            || halt
                .as_ref()
                .is_some_and(agentplane::model::Stop::is_stopped)
        {
            return end(
                "stopped",
                json!({ "text": completion.peek().text }),
                &log,
                usage,
                &conversation,
                prompted,
            );
        }
        if completion.peek().truncated {
            return Ok(Outcome::fail(
                "the model ran out of output budget mid-turn; nothing it was still writing was executed — raise [model] max_output_tokens",
            ));
        }
        let asked_calls = transcript.asked(&completion);
        // The model's own words beside its calls, live: its account, a
        // claim. A turn without calls is its final answer, reported as such;
        // its commentary, where the provider marks one, is still words.
        let said = crate::substrate::said(completion.peek());
        if let (Some((task, p)), Some(said)) = (observer.as_ref(), said.as_deref()) {
            let text: String = crate::approvals::present::escape(said)
                .chars()
                .take(2000)
                .collect();
            p.event(&crate::progress::Event::Said {
                task: task.clone(),
                turn: turn + 1,
                text,
            });
        }
        if asked_calls.is_empty() && schema.is_none() {
            return end(
                "answered",
                json!({ "text": completion.peek().text }),
                &log,
                usage,
                &conversation,
                prompted,
            );
        }
        if asked_calls.is_empty() {
            let report = completion
                .peek()
                .structured
                .clone()
                .unwrap_or_else(|| json!({ "summary": completion.peek().text, "status": "incomplete", "files_changed": [], "checks_run": [], "notes": ["the answer was not the requested structure"] }));
            let status = finish(&report);
            return end(
                status,
                json!({ "report": report }),
                &log,
                usage,
                &conversation,
                prompted,
            );
        }

        for mut asked in asked_calls {
            relativise(&mut asked.arguments, root.as_deref());
            // Only speccoder sets the object a call is checked against.
            if let Some(o) = asked.arguments.as_object_mut() {
                o.remove(crate::tools::EXPECT);
            }
            calls += 1;
            if calls > max_tool_calls {
                return end(
                    "budget_exhausted",
                    json!({ "reason": format!("more than {max_tool_calls} tool calls (`[budget] max_tool_calls`)") }),
                    &log,
                    usage,
                    &conversation,
                    prompted,
                );
            }
            if asked.name == ASK_PERSON && tools.iter().any(|t| t.name == ASK_PERSON) {
                let question = asked.arguments["question"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                // A person's answer is their input, recorded with who gave it
                // — never speccoder's own trusted text.
                let decided = match &ask {
                    Some(asker) => Some(
                        wait_for_person(
                            cx,
                            asker,
                            &mut obligation,
                            &crate::approvals::tasks::question_task(&asker.task, &question),
                        )
                        .await?,
                    ),
                    None => None,
                };
                let read = decided.as_ref().map(crate::approvals::tasks::read);
                let by = read.as_ref().and_then(|r| r.by.clone());
                let entry = json!({
                    "tool": ASK_PERSON,
                    "arguments": asked.arguments,
                    "decision": "allow",
                    "answered": read.as_ref().is_some_and(|r| r.text.is_some()),
                    // The person's words, as the record of what they said.
                    "answer": read.as_ref().and_then(|r| r.text.clone()),
                    "operator": by.as_ref().map(Who::view),
                    "via": by.as_ref().map(|w| w.via.clone()),
                });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                let text = match (&read, &by) {
                    (Some(r), _) if r.text.is_some() => r.text.clone().unwrap_or_default(),
                    (Some(_), Some(w)) => format!("{} declined to answer.", w.name),
                    _ => "Nobody can answer now. Record the question in the artifact as [NEEDS CLARIFICATION: <question>] and continue.".to_owned(),
                };
                let label = match &by {
                    Some(_) => provenance::from(provenance::PERSON),
                    None => Label::trusted(),
                };
                transcript.answered(
                    asked,
                    &Tainted::with_label(json!({ "answer": text }), label),
                );
                continue;
            }
            if asked.name == REPORT_BLOCKED && tools.iter().any(|t| t.name == REPORT_BLOCKED) {
                let refused = stop.rejected || log.iter().any(|e| e["decision"] == "deny");
                let left = max_turns - turn - 1;
                match judge_stop(&stop, &asked.arguments, refused, asked_to_stop, left) {
                    Stop::Stopped(reason) => {
                        let entry = json!({ "tool": REPORT_BLOCKED, "arguments": asked.arguments, "decision": "stop" });
                        emit(observer.as_ref(), turn + 1, &entry);
                        log.push(entry);
                        return end(
                            "blocked",
                            json!({ "reason": reason }),
                            &log,
                            usage,
                            &conversation,
                            prompted,
                        );
                    }
                    Stop::Asked(text) => {
                        asked_to_stop = true;
                        let entry = json!({ "tool": REPORT_BLOCKED, "arguments": asked.arguments, "decision": "challenged" });
                        emit(observer.as_ref(), turn + 1, &entry);
                        log.push(entry);
                        transcript.answered(
                            asked,
                            &Tainted::trusted(json!({ "stopped": false, "note": text })),
                        );
                    }
                    Stop::Refused(text) => {
                        let entry = json!({ "tool": REPORT_BLOCKED, "arguments": asked.arguments, "decision": "unproven", "reason": text });
                        emit(observer.as_ref(), turn + 1, &entry);
                        log.push(entry);
                        transcript.failed(asked, Tainted::trusted(text));
                    }
                }
                continue;
            }
            if asked.name == crate::ask::PROPOSE && tools.iter().any(|t| t.name == asked.name) {
                // A suggestion for the person: nothing runs, one per turn.
                let entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": if proposed { "deny" } else { "proposed" } });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                if proposed {
                    transcript.failed(
                        asked,
                        Tainted::trusted(
                            "one proposal per turn; this one was not shown".to_owned(),
                        ),
                    );
                } else {
                    proposed = true;
                    transcript.answered(asked, &Tainted::trusted(json!({ "shown": true, "note": "shown to the person as a sheet; nothing ran, and only the person can confirm it" })));
                }
                continue;
            }
            let declared = tools.iter().any(|t| t.name == asked.name);
            let target = catalog
                .iter()
                .find(|t| declared && t.name == asked.name)
                .map(|t| ToolId::new(t.server, t.name))
                .or_else(|| {
                    routes
                        .iter()
                        .find(|(n, _, _)| declared && *n == asked.name)
                        .map(|(_, server, tool)| ToolId::new(server, tool))
                });
            let Some(target) = target else {
                let entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "unknown" });
                emit(observer.as_ref(), turn + 1, &entry);
                log.push(entry);
                transcript.failed(asked, Tainted::trusted("no tool of that name".to_owned()));
                continue;
            };
            let mut decision = policy.base(&asked.name, &asked.arguments);
            // `[tests] frozen = "additions"`: read the frozen test (a
            // journaled call, so replay sees the same content) and allow a
            // write that keeps every existing line.
            let mut additions_only = false;
            if frozen_additions
                && matches!(&decision, Decision::Approval(r) if r.contains("frozen test"))
            {
                let path = asked.arguments["path"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let read = cx
                    .call_tool(
                        ToolId::new("fs", "read_file"),
                        Tainted::with_label(json!({ "path": path }), completion.label().clone()),
                    )
                    .await?;
                seen(cx);
                let current = read.peek().clone();
                // `read_file` joins lines with `\n`; the file's last newline is
                // restored so an edit's `old_string` matches as on disk.
                let lines = current["content"].as_str().unwrap_or_default();
                let old_text = if lines.is_empty() {
                    String::new()
                } else {
                    format!("{lines}\n")
                };
                let old = old_text.as_str();
                let readable = current["ok"] == true && current["truncated"] != true;
                let new = match asked.name.as_str() {
                    "write_file" => asked.arguments["content"].as_str().map(str::to_owned),
                    "edit_file" => {
                        let (from, to) = (
                            asked.arguments["old_string"].as_str().unwrap_or_default(),
                            asked.arguments["new_string"].as_str().unwrap_or_default(),
                        );
                        let all = asked.arguments["replace_all"] == true;
                        (!from.is_empty() && (all || old.matches(from).count() == 1))
                            .then(|| old.replace(from, to))
                    }
                    _ => None,
                };
                if readable && new.is_some_and(|n| crate::integrity::only_additions(old, &n)) {
                    decision = Decision::Allow;
                    additions_only = true;
                }
            }
            // A person would be needed: the object the call reaches is
            // resolved by a journaled step (replay reads the same object
            // back), and only a decision on that very object applies.
            let mut object: Option<Object> = None;
            if matches!(decision, Decision::Approval(_)) {
                if asked.name.starts_with(crate::mcp::PREFIX) {
                    object = mcp_digests.get(&asked.name).map(|digest| Object::Tool {
                        name: asked.name.clone(),
                        digest: digest.clone(),
                    });
                } else {
                    let resolved = cx
                        .call_tool(
                            ToolId::new("fs", "resolve"),
                            Tainted::with_label(
                                json!({ "tool": asked.name, "arguments": asked.arguments }),
                                completion.label().clone(),
                            ),
                        )
                        .await?;
                    seen(cx);
                    let out = resolved.peek();
                    if out["ok"] == true {
                        object = serde_json::from_value(out["object"].clone()).ok();
                    } else {
                        decision = Decision::Deny(
                            out["error"]
                                .as_str()
                                .unwrap_or("the call's object cannot be resolved")
                                .to_owned(),
                        );
                    }
                }
                if matches!(decision, Decision::Approval(_)) {
                    decision = policy.classify(&asked.name, &asked.arguments, object.as_ref());
                }
            }
            let mut person: Option<Value> = None;
            // Allowed by a person's grant for the task (of this run): the
            // person's decision, not a rule's (a contained call is a rule's).
            if decision == Decision::Allow
                && object
                    .as_ref()
                    .is_some_and(|o| policy.approved().contains(o))
            {
                person = Some(json!({
                    "approved_by": "person",
                    "scope": "task-grant",
                    "grant": { "scope": "task", "run": run },
                }));
            }
            let mut note: Option<String> = None;
            // An object a person approved for one call: this one.
            if matches!(decision, Decision::Approval(_))
                && let Some(at) = object
                    .as_ref()
                    .and_then(|o| once.iter().position(|r| r == o))
            {
                once.remove(at);
                person = Some(json!({
                    "approved_by": "person",
                    "scope": "once-grant",
                    "grant": { "scope": "once", "run": run },
                }));
                decision = Decision::Allow;
            }
            // An object a person already approved for this run or task.
            if matches!(decision, Decision::Approval(_))
                && let Some((_, scope)) = object
                    .as_ref()
                    .and_then(|o| remembered.iter().find(|(r, _)| r == o))
            {
                person = Some(json!({
                    "approved_by": "person",
                    "scope": scope,
                    "grant": { "scope": scope.trim_end_matches("-grant"), "run": run },
                }));
                decision = Decision::Allow;
            }
            // On autopilot nobody is asked: what would wait for a person is
            // refused for the run and listed for them afterwards.
            decision = policy.settle(decision);
            // Still a person's: the question, with everything it is decided by.
            let request = match &decision {
                Decision::Approval(reason) => Some(question(&Question {
                    asked: &asked,
                    reason,
                    turn: turn + 1,
                    said: said.as_deref().unwrap_or_default(),
                    asker: ask.as_ref(),
                    policy: &policy,
                    object: object.as_ref(),
                    remembered: &remembered,
                    parts: &parts,
                    run: run.as_deref(),
                })),
                _ => None,
            };
            // ask a person now, as a journaled effect, instead of stopping.
            if let (Some(mut request), Some(asker)) = (request.clone(), &ask) {
                // The run's asking budget: within it a person is prompted
                // (and counted, whatever they answer); past it the call
                // waits in the run's batch, nobody interrupted.
                if asking.prompts {
                    if prompted < asking.budget {
                        prompted += 1;
                        request.asked = prompted;
                    } else {
                        request.batched = true;
                    }
                    request.budget = asking.budget;
                }
                if request.batched {
                    // The batch item is created: the person is notified
                    // once; calls joining it notify no one.
                    if !told_budget {
                        needs_person(observer.as_ref(), &asked, object.as_ref());
                    }
                    if !told_budget && let Some((_, p)) = &observer {
                        p.event(&crate::progress::Event::Notice {
                            text: format!(
                                "approval budget ({}) reached: calls wait in Needs You (speccoder approvals)",
                                asking.budget
                            ),
                        });
                    }
                    told_budget = true;
                } else {
                    needs_person(observer.as_ref(), &asked, object.as_ref());
                }
                let decided = wait_for_person(
                    cx,
                    asker,
                    &mut obligation,
                    &crate::approvals::tasks::approval_task(&request, &log),
                )
                .await?;
                seen(cx);
                let read = crate::approvals::tasks::read(&decided);
                // A batch decision may start the run's asking again.
                if read.resume_asking {
                    prompted = 0;
                }
                let answer: Answer = read.answer.clone();
                // Who decided, and how that is known, is journaled with it.
                let approver = serde_json::to_value(&answer.by).unwrap_or(Value::Null);
                let operator = read.by.as_ref().map(Who::view);
                person_decided(
                    observer.as_ref(),
                    &asked,
                    if answer.decision == Said::Approve {
                        "approve"
                    } else {
                        "reject"
                    },
                    (answer.decision == Said::Approve).then(|| answer.scope.name()),
                    read.by.as_ref().filter(|_| !read.expired),
                    object.as_ref(),
                );
                match answer.decision {
                    Said::Approve => {
                        let mut p = json!({
                            "approved_by": "person",
                            "scope": answer.scope.name(),
                            "note": answer.note,
                            "approver": approver,
                            "operator": operator,
                        });
                        if answer.always {
                            p["always"] = json!(true);
                        }
                        if let (Some(o), Scope::Task | Scope::Run) = (&object, answer.scope) {
                            let remembered_as = if answer.scope == Scope::Run {
                                "run-grant"
                            } else {
                                "task-grant"
                            };
                            p["grant"] = json!({ "scope": answer.scope.name(), "run": run });
                            remembered.push((o.clone(), remembered_as));
                        }
                        person = Some(p);
                        note = answer.note;
                        decision = Decision::Allow;
                    }
                    // "Later" is a task left undecided: the run stays
                    // suspended and never reaches here.
                    Said::Reject | Said::Stop => {
                        let why = answer
                            .reason
                            .unwrap_or_else(|| "no reason given".to_owned());
                        person = Some(if read.expired {
                            json!({ "expired": true, "rejection": why })
                        } else {
                            json!({ "rejected_by": "person", "approver": approver, "operator": operator, "rejection": why })
                        });
                        decision = Decision::Deny(format!("a person rejected this: {why}"));
                    }
                }
            }
            // Who decided: a person (now, or by a grant they gave), or a rule
            // — named, so a rule's decision is never read as a person's.
            let rule = person
                .is_none()
                .then(|| policy.rule(&asked.name, &asked.arguments, object.as_ref()));
            let with_person = |mut entry: Value| {
                if let Some(o) = entry.as_object_mut() {
                    // The object decided on, whoever decided.
                    if let Some(obj) = &object {
                        o.insert("object".to_owned(), json!(obj));
                    }
                    match (person.as_ref().and_then(Value::as_object), &rule) {
                        (Some(p), _) => {
                            // An expiry is nobody's decision.
                            let by = if p.contains_key("expired") {
                                "expiry"
                            } else {
                                "person"
                            };
                            o.insert("by".to_owned(), json!(by));
                            o.extend(p.clone());
                        }
                        (None, Some(r)) => {
                            o.insert("by".to_owned(), json!("rule"));
                            o.insert("rule".to_owned(), json!(r));
                            // Ran without asking: the sandbox confined it.
                            if r.starts_with("contained (") {
                                o.insert("contained".to_owned(), json!(true));
                            }
                        }
                        (None, None) => {}
                    }
                }
                entry
            };
            match decision {
                Decision::Deny(reason) => {
                    let entry = with_person(
                        json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "deny", "reason": reason }),
                    );
                    emit(observer.as_ref(), turn + 1, &entry);
                    log.push(entry);
                    // The reason quotes the call (its tool, path, command, or a
                    // person's words about it): it carries the call's label.
                    transcript.failed(
                        asked,
                        refusal(format!("denied: {reason}"), completion.label()),
                    );
                    // Escalation: a model that keeps asking for what it may
                    // not do is stopped for a person.
                    denied_in_a_row += 1;
                    if denied_in_a_row >= MAX_CONSECUTIVE_DENIALS {
                        return end(
                            "denied",
                            json!({ "reason": format!("{denied_in_a_row} calls in a row were denied; last: {reason}") }),
                            &log,
                            usage,
                            &conversation,
                            prompted,
                        );
                    }
                    continue;
                }
                Decision::Approval(reason) => {
                    let mut entry = json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "approval", "reason": reason });
                    if let Some(obj) = &object {
                        entry["object"] = json!(obj);
                    }
                    emit(observer.as_ref(), turn + 1, &entry);
                    needs_person(observer.as_ref(), &asked, object.as_ref());
                    log.push(entry);
                    return end(
                        "needs_approval",
                        json!({ "pending": {
                            "tool": asked.name,
                            "arguments": asked.arguments,
                            "reason": reason,
                            "object": object,
                            "run": run,
                            "request": request,
                        } }),
                        &log,
                        usage,
                        &conversation,
                        prompted,
                    );
                }
                Decision::Allow => {
                    denied_in_a_row = 0;
                    // The arguments carry the model's label: agentplane's
                    // taint gate checks them against the grant's fields (an
                    // argument-less mutating tool is granted exactly `{}`).
                    // A command gets the call's id (`SPECCODER_CALL_ID`):
                    // a call resumed after a crash has the same one, so a
                    // script can tell a repeat from a new call.
                    let mut arguments = asked.arguments.clone();
                    if asked.name == "run_command"
                        && let Some(o) = arguments.as_object_mut()
                    {
                        o.insert("call_id".to_owned(), Value::String(asked.id.clone()));
                    }
                    // Decided on an object: the tool checks it again at
                    // use, and refuses when the call now reaches another.
                    if let (Some(obj), Some(o)) = (&object, arguments.as_object_mut())
                        && matches!(
                            asked.name.as_str(),
                            "write_file" | "edit_file" | "run_command"
                        )
                    {
                        o.insert(crate::tools::EXPECT.to_owned(), json!(obj));
                    }
                    let args = Tainted::with_label(arguments, completion.label().clone());
                    let result = cx.call_tool(target.clone(), args).await;
                    seen(cx);
                    let result = match result {
                        Ok(r) => r,
                        // A failure the far side reported is the model's to
                        // read, labelled by who wrote it.
                        Err(error) => match cx.relayed(&error) {
                            Some(detail) => {
                                let entry = with_person(
                                    json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "allow", "ok": false }),
                                );
                                emit(observer.as_ref(), turn + 1, &entry);
                                log.push(entry);
                                transcript.failed(asked, detail);
                                continue;
                            }
                            None => return Err(error.into()),
                        },
                    };
                    let output = result.peek().clone();
                    let ok = output["ok"].as_bool() != Some(false);
                    let mut entry = with_person(
                        json!({ "tool": asked.name, "arguments": asked.arguments, "decision": "allow", "ok": ok }),
                    );
                    // A read the answer may cite: its id rides on the result.
                    let result = match next_read.as_mut() {
                        Some(n) if ok => {
                            let id = format!("r{n}");
                            *n += 1;
                            entry["read"] = json!(crate::ask::read_of(
                                &id,
                                &asked.name,
                                &asked.arguments,
                                &output
                            ));
                            result.map(|mut v| {
                                if let Some(o) = v.as_object_mut() {
                                    o.insert("read".to_owned(), json!(id));
                                }
                                v
                            })
                        }
                        _ => result,
                    };
                    if let Some(changed) = output["object_changed"].as_object() {
                        entry["decision"] = json!("refused_changed");
                        entry["expected"] = changed.get("expected").cloned().unwrap_or(Value::Null);
                        entry["found"] = changed.get("found").cloned().unwrap_or(Value::Null);
                    }
                    // What the tool returned is content the attempt read:
                    // arguments copied from it later are named as such.
                    if let Some(part) = crate::approvals::derived::part_of(
                        &asked.name,
                        &asked.arguments,
                        &output,
                        turn + 1,
                        &format!("tool://{}/{}", target.server, target.tool),
                        |p: &str| {
                            policy.protected().write_class(p)
                                == Some(crate::tools::paths::Class::Intent)
                        },
                    ) {
                        parts.push(part);
                    }
                    if additions_only {
                        entry["frozen_additions"] = json!(true);
                    }
                    let tail = if asked.name == "run_command" && !ok {
                        crate::progress::tail(&format!(
                            "{}\n{}",
                            output["stdout"].as_str().unwrap_or_default(),
                            output["stderr"].as_str().unwrap_or_default()
                        ))
                    } else {
                        Vec::new()
                    };
                    emit_with_tail(observer.as_ref(), turn + 1, &entry, tail);
                    log.push(entry);
                    // A command that changed a judge input ends the attempt
                    // before the model sees another turn.
                    if let Some(drift) = output["integrity"].as_array().filter(|d| !d.is_empty()) {
                        let paths: Vec<&str> =
                            drift.iter().filter_map(|d| d["path"].as_str()).collect();
                        return end(
                            "integrity",
                            json!({
                                "reason": format!("a command changed what judges the task: {}", paths.join(", ")),
                                "integrity": drift,
                            }),
                            &log,
                            usage,
                            &conversation,
                            prompted,
                        );
                    }
                    // A tool that refused (`error`) is a failed call; a
                    // command that ran and exited non-zero is a result the
                    // model must read in full — its stdout and stderr are
                    // the diagnostics (found by the first real evaluation,
                    // where the model saw only the word "failed").
                    match output["error"].as_str() {
                        Some(error) if !ok => transcript.failed(
                            asked,
                            Tainted::with_label(error.to_owned(), result.label().clone()),
                        ),
                        // A person's note rides with the result it approved.
                        _ => match &note {
                            Some(n) => {
                                let n = format!("Note from the person who approved this call: {n}");
                                transcript.answered(
                                    asked,
                                    &result.map(|mut v| {
                                        v["note_from_person"] = Value::String(n);
                                        v
                                    }),
                                );
                            }
                            None => transcript.answered(asked, &result),
                        },
                    }
                }
            }
        }
    }
    end(
        "budget_exhausted",
        json!({ "reason": format!("more than {max_turns} model turns (`[budget] max_turns`)") }),
        &log,
        usage,
        &conversation,
        prompted,
    )
}
