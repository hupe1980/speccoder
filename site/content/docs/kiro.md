+++
title = "Kiro specs"
description = "Run a Kiro or cc-sdd spec task by task under the same judge: each requirement's acceptance criteria become failing tests first, a task is ticked only when the criteria it cites pass, and Kiro's own ticks are labelled for what they are."
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
| Order | phases, `[P]` runs, `depends on` | by number; cc-sdd's `_Depends:_` adds an order |
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
- **Optional tasks are left alone.** `- [ ]*` tasks (Kiro's optional property tests) are not run and not ticked; the criteria's own tests do that job.
- **Kiro's ticks are kept, and labelled.** A task already `[x]` is not run again; `report` shows it as ticked without evidence until you check it.
- **Context.** Each attempt reads the steering that is always included (`.kiro/steering/*.md` without front matter, or with `inclusion: always`) as the project's rules, `design.md` as the plan, and the requirements the task cites.

In the app, **Spec & Plan** shows `requirements.md` as a living spec: each requirement a card with how many of its criteria are green, each criterion its state and the task that turned it.

A checkpoint task ("Checkpoint - …") is an ordinary task: its attempt runs the checks, and it is ticked `regression_only` at best.

## Not supported yet

Bugfix specs (`bugfix.md`) are listed and stop before any model call. Also not read: `fileMatch` and `manual` steering, Correctness Properties, cc-sdd's `spec.json` approvals and `_Boundary:_`. Specs are written in Kiro or with cc-sdd; `speccoder spec` hosts no Kiro skills.
