+++
title = "Kiro specs"
description = "Run a Kiro or cc-sdd spec under the same judge: each requirement's criteria become failing tests first, a task is ticked only when the criteria it cites pass, a bugfix spec is proven fixed, and Kiro's own ticks are labelled for what they are."
weight = 46

[extra]
group = "Use"
nav = "Kiro specs"
+++

speccoder runs [Kiro](https://kiro.dev) specs, and specs written with [cc-sdd](https://github.com/gotalab/cc-sdd), under the same [judge](@/docs/how-it-works.md) as Spec Kit features. Kiro marks a task done when its agent stops; speccoder marks it done when its tests went red, then green.

| | Spec Kit | Kiro |
|---|---|---|
| Unit of work | a feature, `specs/NNN-name/` | a spec, `.kiro/specs/<name>/` |
| Acceptance examples | the task's user story | the criteria its `_Requirements: 1.2, 1.3_` cites |
| Task IDs | `T001` | the task's number (`2.1`) |
| Order | phases, `[P]` runs, `depends on` | by number; cc-sdd's `(P)` runs side by side, `_Depends:_` adds an order |
| Intent the model cannot edit | `.specify/`, `specs/` | `.kiro/specs/` |

## Running a spec

```sh
speccoder inspect                 # the spec, its requirements, criteria and tasks
speccoder run                     # work the spec through, task by task
speccoder run --feature checkout  # when there is more than one spec
speccoder run --task 2.1          # one task
speccoder report                  # what each tick rests on
```

Commit the spec first; the work happens on `speccoder/<name>`.

- **Each requirement is a story.** Before its first task, its acceptance criteria (`1.1`, `1.2`, …) become tests, one per criterion, written from the EARS text, committed and shown failing. A task is judged by the criteria it cites: `_Requirements: 1.2, 1.3_` passes when those two tests turn green and nothing else breaks. `_Requirements: 3_` cites every criterion of Requirement 3; free text (`Foundation for all requirements`) cites none, and the checks alone judge the task.
- **Parents close themselves.** A task with sub-tasks (`2.` over `2.1`, `2.2`) gets no attempt: speccoder ticks it with its last required sub-task. cc-sdd's unindented sub-tasks and `## 1.` group headings are found by number.
- **Parallel tasks.** cc-sdd writes `(P)` on a task that does not depend on its peers before it. Consecutive `(P)` siblings (`2.1 (P)`, `2.2 (P)`, or top-level `3 (P)`, `4 (P)`) wait on what comes before them and run side by side with `speccoder run --parallel N`, each in its own lane and judged on its own tests; the next unmarked task waits for all of them. A run never spans two parents, and a requirement whose tests are not written yet runs first, alone. `inspect` lists the runs. A list without `(P)` runs in number order.
- **Optional tasks are left alone.** `- [ ]*` (or `- [ ] 7.9*`) tasks (Kiro's optional property tests) are not run and not ticked; the criteria's own tests do that job.
- **Kiro's ticks are kept, and labelled.** A task already `[x]` is not run again; `report` shows it as ticked without evidence until you check it.
- **Context.** Each attempt reads `design.md` as the plan, the requirements the task cites, and the steering as the project's rules — as Kiro includes it: files without front matter or with `inclusion: always` as they are; `fileMatch` files under "when you change files matching `web/**`"; `auto` files under "when the work is about: …" (their `description`). `manual` files are left out, as Kiro leaves them out until named; `inspect` lists them. `#[[file:docs/api.md]]`, `#[[file:docs/api.md:10-20]]` and `#[[folder:…]]` are inlined from inside the repository; one that does not resolve stays as written and `inspect` warns.
- **Correctness Properties.** The author of a requirement's tests reads `design.md`, its properties included, so a test can check the invariant and not only the example. A task's `**Validates: Requirements 1.2**` cites those criteria.
- **cc-sdd's approvals.** A spec whose `spec.json` does not approve its tasks is not run (`kiro.unapproved`), as cc-sdd's own `spec-impl` would not run it: approve them (`/kiro:spec-tasks <name> -y`), then run again.

In the app, **Spec & Plan** shows `requirements.md` as a living spec: each requirement a card with how many of its criteria are green, each criterion its state and the task that turned it.

A checkpoint task ("Checkpoint - …") is an ordinary task: its attempt runs the checks, and it is ticked `regression_only` at best.

## Bugfix specs

A bugfix spec (`bugfix.md`: Current, Expected and Unchanged behavior) runs as a [bug](@/docs/bugs.md), not task by task:

```sh
speccoder inspect --feature empty-title   # Current 1.1, Expected 2.1 (E1), Unchanged 3.1 (U1)
speccoder run --feature empty-title       # reproduce, then your one confirmation
speccoder run --feature empty-title       # after `speccoder bug confirm …`: the fix
```

1. **Reproduce.** One test per Expected statement (`2.1` is E1, `2.2` E2), written by an author that never sees the fix, red on the untouched code by an assertion. Kiro's "this test MUST FAIL on unfixed code", recorded.
2. **Confirm.** You confirm once that the tests capture the bug.
3. **Unchanged.** The Unchanged statements' tests are green before the fix and must stay green.
4. **Fix.** The attempt reads `design.md` and `tasks.md` as its plan. It is verified only when every reproduction turned green, the Unchanged set held and reverting the fix makes them fail again.
5. **Tick.** The verified fix's commit ticks every required task of `tasks.md` (parents too, optional ones not), each named as `Closes-Task:`; `report` shows them verified by the bug's verdict. A fix that is not verified ticks nothing.

A spec with only `bugfix.md` (before Kiro wrote its design and tasks) runs the same way. In the app, the board shows a bugfix spec as one bug card: its stage, where it stands, the one next step (run it, confirm the reproductions) and its tasks as a checklist that ticks with the verified fix; `report` prints the same as its `Bug` line. `speccoder bug "…"` in a Kiro project asks you to write the bugfix spec in Kiro.

## Not supported yet

Kiro's own "Run all tasks" builds its waves while it runs and writes none of them to `tasks.md`, so a list Kiro wrote runs in number order; only cc-sdd's `(P)` marks tasks to run side by side. Global steering (`~/.kiro/steering/`) is not read: it is not in the repository, so a tick's context could not be checked again. Specs are written in Kiro or with cc-sdd; `speccoder spec` hosts no Kiro skills.
