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

`specify init --here --integration claude` installs Spec Kit's commands as Agent Skills (`.claude/skills/speckit-*/SKILL.md`) — without `--integration claude`, Spec Kit sets up another editor and these skills are missing. speccoder runs them with its own model, so you can plan without a separate editor agent:

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

## Review the spec

The spec is what the result will be judged against: its requirements are the words you sign off. Review them one at a time:

```sh
speccoder review                       # every requirement and its review
speccoder review approve FR-001 FR-002 # on their wording as it is now
speccoder review request FR-003 -c "say what happens to open sessions"
speccoder review approve               # every one not yet approved
```

```text
✓ FR-001  Users MUST log in.                          approved
            Ada
△ FR-003  Users MUST be logged out after 30 minutes.  changes requested
            Ada — say what happens to open sessions
1/3 requirements approved; 1 to review, 1 with changes requested
```

Each decision is bound to the exact line it was made on and names who made it (git's `user.name`). Reword a requirement and its decision **lapses**: it shows again, with the wording you decided on beside the new one. Change requests go to the format's revising skill (Spec Kit: `clarify`) in one request — **Revise the Spec** in the app, or `speccoder spec clarify "…"` — and you review the result.

The review is recorded in `review.json` beside the spec; commit it with the spec, and the team, the pull request and every later run see the same sign-off. The model never writes it: a planning skill may write the spec, but a change to the review — even through a command it was allowed to run — is undone and reported. The report and the pull request description say how many requirements are approved; [**Needs You**](@/docs/web.md#what-you-see) lists what waits for your review. With `[review] required = true` in `speccoder.toml`, a run waits (`REVIEW-PENDING`) until every requirement is approved in the committed review.

In the [app](@/docs/web.md), **Spec & Plan › Review** shows each requirement as a card with **Approve**, **Request Changes** and **Clear**, a lapsed one word by word.

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
- A feature with a spec but no `tasks.md` yet is **being planned**: `report`, bare `speccoder` and the dashboard show where it is on its journey and the planning skills that come next. A project with no features points to `speccoder spec specify "…"`.

## The next step, taken

At a terminal, after a planning step, a decision on a task or the report, speccoder **offers the next step** instead of only naming it — one yes at a time, each showing the command it stands for:

```text
  ✓ spec ── ✓ plan ── ✓ tasks ── ● commit ── · run ── · review

  ┌─ Next ──────────────────────────────────────────────────────┐
  │ Commit the planning files (1)                               │
  │ speccoder runs from committed files                         │
  │                                                             │
  │ + specs/001-greet-function/tasks.md                         │
  │ only these are committed; `git diff` shows them             │
  │                                                             │
  │ $ git add specs/001-greet-function/tasks.md && git commit … │
  └─────────────────────────────────────────────────────────────┘
  Commit them? [Y/n]
```

The journey — `spec → plan → tasks → commit → run → review` — says what is done, what is now and what is left. A commit takes exactly the feature's planning files, as `spec(<feature>): <files>`, and nothing else you have staged. After it comes the next planning skill or the run; answer `n` to stop and do it yourself. With `--json`, `--events`, without a terminal, or with `spec … --no-prompt`, nothing is asked. In the dashboard, `n` leaves it and offers the same step.
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
