+++
title = "Planning with Spec Kit"
description = "Write the spec, plan and tasks with Spec Kit's own skills run by speccoder, honour Spec Kit extension hooks, and run speccoder as a step of a Spec Kit workflow."
weight = 40

[extra]
group = "Use"
nav = "Planning with Spec Kit"
+++

speccoder implements; Spec Kit specifies. speccoder does not replace Spec Kit's planning — it runs Spec Kit's own planning skills, honours its extensions and plugs into its workflow engine.

## Spec Kit's planning skills

`specify init` installs Spec Kit's commands as Agent Skills (`.claude/skills/speckit-*/SKILL.md`). speccoder runs them with its own model, so you can plan without a separate editor agent:

```sh
speccoder spec                                         # the installed skills
speccoder spec specify "A function greet(name) that …" # spec.md
speccoder spec clarify                                 # asks you about what is unclear
speccoder spec plan                                    # plan.md, research.md, data-model.md, quickstart.md
speccoder spec tasks                                   # tasks.md
speccoder spec analyze                                 # a read-only consistency report
```

A planning skill works **in your checkout**, because the spec belongs on your branch. It may write `specs/…`, `.specify/feature.json` and `.specify/memory/…` and run Spec Kit's scripts; it may not touch Spec Kit's templates or scripts, your check configuration or secrets. Writing anything else — source code included — needs your approval.

Results are left **uncommitted**: review them with `git diff`, then commit, because `speccoder run` reads committed files only. When a skill asks a clarifying question you answer at the terminal; with `--no-prompt` it records `[NEEDS CLARIFICATION]` in the spec instead.

`speckit-implement` and `speckit-converge` are not hosted — `speccoder run` and `speccoder converge` do that work, with evidence. `speckit-taskstoissues` is not hosted either, because it writes to GitHub.

## The current feature

Every command works on one feature. Which one is the **current feature**, and speccoder keeps it where Spec Kit does — `.specify/feature.json`, a per-checkout file git ignores — so `speccoder`, Spec Kit's scripts and the `/speckit-*` commands in your editor always agree.

```text
$ speccoder feature
▸  1  001-greet-function  ■■■■■■■□□□  7/10 tasks    spec · plan · tasks
   2  002-farewell        □□□□□□□□□□  no tasks yet spec
current feature: 001-greet-function (recorded in .specify/feature.json)

$ speccoder feature 2          # or: speccoder feature farewell, speccoder feature 002-farewell
switched to 002-farewell (recorded in .specify/feature.json: Spec Kit's scripts and the /speckit-* commands in your editor read it too)
```

- `speccoder spec specify` makes the feature it creates current — whether or not the model remembered to write the file — and says so. Every other planning skill's commands are told the feature (`SPECIFY_FEATURE_DIRECTORY`), so Spec Kit's scripts never lose it.
- `--feature NAME` (and `SPECIFY_FEATURE_DIRECTORY`) choose for one command without changing the current one. With a single feature, it is current by itself.
- With several features and none current, `run` and `spec` ask you at a terminal and remember the answer; elsewhere they stop with the list and the command that picks one. A pointer to a feature that no longer exists gets a "did you mean …" and shows up in `speccoder doctor`.
- A feature with a spec but no `tasks.md` yet is **being planned**: `report`, bare `speccoder` and the dashboard show what exists (`✓ spec ✓ plan · tasks`) and the planning skills that come next. A project with no features points to `speccoder spec specify "…"`.
- `report` and `doctor` say which feature and why (`recorded in .specify/feature.json`, `the only feature`, `named with --feature`); in the [dashboard](@/docs/running.md#the-dashboard), `f` opens a picker. `speccoder feature --clear` forgets the current feature.

## Extension hooks

Spec Kit extensions register hooks in `.specify/extensions.yml` — the git extension, for example, creates a feature branch before `specify`. speccoder runs them around what it runs:

| Event | Around |
|---|---|
| `before_<skill>` / `after_<skill>` | a hosted planning skill, with its arguments |
| `before_implement` / `after_implement` | `speccoder run` |
| `before_converge` / `after_converge` | convergence |

A **mandatory** hook (`optional: false`) runs as a hosted skill under the planning policy; if its skill is missing or does not complete, the run stops before any task. An **optional** hook is offered with its prompt and the command to run it. Hooks with a `condition` are left to Spec Kit's own executor; disabled ones are ignored.

## Spec Kit workflows

Spec Kit's workflow engine chains steps with review gates. `speccoder workflow install` adds a `speccoder` step type and an example workflow — specify → plan → tasks, each followed by a gate, then a commit and `run --converge`:

```sh
speccoder workflow install
specify workflow run .specify/workflows/speccoder/workflow.yml \
  --input spec="A Rust function clamp(x, lo, hi) that …"
# paused? decide with speccoder report --approvals / approve / accept / nudge, then:
specify workflow resume <run-id>
```

A step with `skill:` runs a planning skill; without it, the step runs `speccoder run --no-prompt` (options: `converge`, `feature`, `max_tasks`, `timeout`). When speccoder needs a person, the step **pauses** instead of failing, and `resume` continues where it stopped. The step type needs Spec Kit 1.1 or later; with older releases, a plain `shell` step running `speccoder --json run --no-prompt` works but fails rather than pauses.

## Writing specs that verify well

speccoder's strongest evidence comes from the acceptance examples in your user stories. Specs verify best when:

- each user story has concrete **Given / When / Then** examples with real values;
- examples are deterministic — no clocks, network or randomness in expected results;
- tasks name the files they change (`… in src/lib.rs`) and are small enough for one attempt;
- tasks that can run in parallel are marked `[P]` and touch different files.
