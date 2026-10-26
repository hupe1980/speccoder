+++
title = "How it works"
description = "How speccoder decides a task: readiness, a baseline, spec-derived acceptance tests, a governed attempt, independent verification, and a recorded outcome."
weight = 30

[extra]
group = "Start"
+++

Every task goes through six steps. Commands are on the [run page](@/docs/running.md).

## 1. Is the task ready?

speccoder reads `tasks.md` as Spec Kit writes it: phases, `[P]` markers, user-story labels and references such as `depends on T012`. A task is **ready** when everything it depends on is ticked. Edges come only from that structure and from task IDs in the text; nothing is inferred. `speccoder inspect` shows the graph.

## 2. Baseline

Your **checks** (the test, lint and type-check commands in `speccoder.toml`) run on the untouched worktree. A check that already fails is not blamed on the change; with JUnit reports the comparison is test by test.

## 3. Acceptance tests, red first

If the task's user story has acceptance examples, a **separate model call with no tools** writes tests from them. They must **fail** now, since a test that already passes demonstrates nothing, and are then frozen so the implementing model cannot change them. A second set, [held out](@/docs/evidence.md#held-out-tests) from the model, checks the same scenarios with other inputs.

## 4. The attempt

The implementing model works in the worktree with a small set of tools: read, list, search, write, edit, run a command, git status and diff. Every call is classified before it runs as allowed, needing a person ([approvals](@/docs/approvals.md)) or denied ([the policy](@/docs/security.md#the-policy)). Three denied calls in a row end the attempt. Every command runs in the [OS sandbox](@/docs/security.md#the-os-sandbox). If the task cannot be done as written (the spec contradicts itself, a test is wrong), the model stops with a reason (`blocked`) instead of bending the code.

## 5. Judgement

speccoder [audits the diff](@/docs/evidence.md#the-diff-audit) (deleted or weakened tests, new skip markers, changed check configuration fail the verdict whatever the checks say), then runs the checks and the acceptance tests itself.

| Verdict | Means |
|---|---|
| `verified` | The acceptance tests went from red to green and nothing regressed. |
| `regression_only` | Nothing regressed, but there was nothing task-specific to show (no acceptance examples). |
| `failed` | A check or an acceptance test failed, or the audit found something. |

Optionally, [mutation testing](@/docs/evidence.md#mutation-testing) checks that the tests test the change, and a [gaming monitor](@/docs/evidence.md#the-gaming-monitor) looks for hard-coded results.

## 6. Outcome

- **Passed**: speccoder (never the model) ticks the task in `tasks.md` and commits on `speccoder/<feature>`, with trailers recording the verdict, the run and the tokens.
- **Failed**: [retried](@/docs/running.md#retries) with the verifier's facts, not the model's transcript.
- **Anything else** (blocked, needs approval, suspected gaming): the attempt is saved under `refs/speccoder/attempts/<feature>/<task>/<run>`, undone in the worktree, and the run stops with what it needs from you.

## After the last task: convergence

All tasks can be ticked while the spec is still unmet. [Convergence](@/docs/running.md#convergence) has a read-only model call compare the code with the spec and appends what remains as new tasks, each of which changes code only once its acceptance tests fail first.

## What speccoder does not decide

Whether the feature is right. `verified` is strong evidence that the code does what the spec's examples say, not a review of the design, the security of the code, or what the spec does not mention. Review the branch like any contribution.
