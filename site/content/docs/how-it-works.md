+++
title = "How it works"
description = "How speccoder decides a task: readiness, a baseline, spec-derived acceptance tests, a governed attempt, independent verification, and a recorded outcome."
weight = 30

[extra]
group = "Start"
+++

Every task goes through the same six steps. Each one exists because of a specific way an unsupervised agent goes wrong.

## 1. Is the task ready?

speccoder reads `tasks.md` the way Spec Kit writes it: phases, `[P]` (parallel) markers, user-story labels and references such as `depends on T012`. A task is **ready** when everything it depends on is ticked. Edges come only from that structure and from task IDs written in the text — nothing is inferred. `speccoder inspect` shows the graph and what blocks what.

## 2. Baseline

Your **checks** — the test, lint and type-check commands in `speccoder.toml` — run on the untouched worktree. A check that already fails is not blamed on the change, and with JUnit reports speccoder compares test by test, so a pre-existing failure stays pre-existing.

## 3. Acceptance tests, red first

If the task's user story has acceptance examples, a **separate model call with no tools** writes tests from them, in the project's test layout. They must **fail** now: a test that already passes demonstrates nothing about the change. From this point they are frozen for the attempt — the implementing model cannot change them.

This is the step that turns `tasks.md` from a to-do list into a contract: the evidence for a task comes from the spec, not from code the implementer wrote to suit itself.

## 4. The attempt

The implementing model works in the worktree with a small set of tools — read, list, search, write, edit, run a command, look at git. **Every call is classified before it runs:**

- **allowed** — reading the project, writing source files, running your checks and commands you allowed;
- **needs a person** — dependency manifests, existing tests, shells, network tools, unlisted commands, MCP tools you have not allowed;
- **denied** — the spec, your check configuration, `.git`, secrets, privilege escalation, git remotes, anything outside the worktree.

A person decides the middle class inline, in an editor, or later. Three denied calls in a row end the attempt. Every command runs inside the [OS sandbox](@/docs/security.md). If the model concludes the task cannot be done as written — the spec contradicts itself, a test is wrong — it stops honestly with a reason instead of bending the code.

## 5. Judgement

speccoder diffs the worktree and audits it: deleted or weakened tests, new skip markers and changed check configuration fail the verdict whatever the checks say. Then it runs the checks and the acceptance tests itself.

| Verdict | Means |
|---|---|
| `verified` | The acceptance tests went from red to green and nothing regressed. |
| `regression_only` | Nothing regressed, but there was nothing task-specific to show (no acceptance examples). |
| `failed` | A check or an acceptance test failed, or the audit found something. |

Optionally, [mutation testing](@/docs/evidence.md#mutation-testing) checks that the new tests really test the change, and a [gaming monitor](@/docs/evidence.md#the-gaming-monitor) looks for results that were hard-coded rather than computed.

## 6. Outcome

- **Passed** — speccoder ticks the task in `tasks.md` (never the model) and commits on `speccoder/<feature>` with trailers recording the verdict, the run and the tokens used.
- **Failed** — it is retried once by default, with the verifier's facts (what failed, which test, the output) rather than the model's own transcript, so the second attempt starts from evidence.
- **Anything else** — blocked, needing approval, suspected gaming — the attempt is saved under `refs/speccoder/attempts/<feature>/<task>/<run>`, undone in the worktree, and the run stops with what it needs from you.

## After the last task: convergence

Tasks can all be ticked while the spec is not yet met. `speccoder converge` (or `run --converge`) asks a read-only model call to compare the code with the spec, plan and tasks, the way Spec Kit's `/speckit-converge` does, and appends what remains as a new phase of tasks. A convergence task only changes code once its acceptance tests fail first — a model's finding is a claim until a test demonstrates it.

## What speccoder does not decide

Whether the feature is right. `verified` is strong evidence that the code does what the spec's examples say; it is not a review of the design, the security of the code, or the parts the spec did not mention. Review the branch like any contribution.
