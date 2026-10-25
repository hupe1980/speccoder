+++
title = "Planning with Spec Kit"
description = "Write the spec, plan and tasks with Spec Kit's own skills run by speccoder, honour Spec Kit extension hooks, and run speccoder as a step of a Spec Kit workflow."
weight = 40

[extra]
group = "Use"
nav = "Planning with Spec Kit"
+++

Spec Kit specifies; speccoder implements. speccoder runs Spec Kit's own planning skills, honours its extension hooks and plugs into its workflow engine. For OpenSpec, see [OpenSpec planning](@/docs/openspec.md#planning).

## Spec Kit's planning skills

`specify init --here --integration claude` installs Spec Kit's commands as Agent Skills (`.claude/skills/speckit-*/SKILL.md`); speccoder finds skills under `.claude/skills/` and `.agents/skills/`. It runs them with its own model, so you can plan without an editor agent:

```sh
speccoder spec                                         # the installed skills
speccoder spec specify "A function greet(name) that …" # spec.md
speccoder spec clarify                                 # asks you about what is unclear
speccoder spec plan                                    # plan.md, research.md, data-model.md, quickstart.md
speccoder spec tasks                                   # tasks.md
speccoder spec analyze                                 # a read-only consistency report
```

A planning skill works **in your checkout**, because the spec belongs on your branch. It may write `specs/…`, `.specify/feature.json` and `.specify/memory/…` and run Spec Kit's scripts (`.specify/scripts/`, and those of installed extensions) without asking; writing anything else — source code included — needs your approval.

Results are left **uncommitted** for review; speccoder [offers the commit](#the-next-step-taken), because `speccoder run` reads committed files only. A clarifying question is asked at the terminal; with `--no-prompt` it is left as `[NEEDS CLARIFICATION]` in the spec.

Not hosted: `speckit-implement` and `speckit-converge` (use `speccoder run` and `speccoder converge`, which record evidence) and `speckit-taskstoissues` (speccoder goes the other way: `speccoder issue start N`).

## Review the spec

The result is judged against the spec, so its requirements are what you sign off, one at a time:

```sh
speccoder review                       # every requirement and its review
speccoder review approve FR-001 FR-002 # on their current wording
speccoder review request FR-003 -c "say what happens to open sessions"
speccoder review approve               # every one still unapproved
```

<figure class="shot"><img class="shot-light" src="../../screenshots/review-light.webp" alt="Reviewing the spec in the app: an open question to answer and requirements approved by name" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/review-dark.webp" alt="Reviewing the spec in the app: an open question to answer and requirements approved by name" width="1440" height="900" loading="lazy" decoding="async"></figure>

```text
✓ FR-001  Users MUST log in.                          approved
            Ada
△ FR-003  Users MUST be logged out after 30 minutes.  changes requested
            Ada — say what happens to open sessions
1/3 requirements approved; 1 to review, 1 with changes requested
```

Answer open questions and reword requirements in place:

```sh
speccoder review answer Q1 "sessions end at once"       # an open question, answered in place
speccoder review edit FR-003 "Users MUST log out of every device." --approve
speccoder spec clarify                                  # or let the format's skill ask you
```

- **Open questions.** Every `[NEEDS CLARIFICATION: …]` in the feature's documents is a question (`Q1`, `Q2`, … with where it is and its requirement). `answer` puts your words where it stood and touches nothing else.
- **Edits.** `edit` rewords a requirement: it keeps its identifier, its approval lapses, and `--approve` signs the new words. A wording that still contains a question cannot be approved; `approve` without IDs skips it and says so.
- **Lapses.** A decision is bound to the exact line and names who made it (git's `user.name`). Reword the line and the decision lapses: it shows again, old wording beside new.
- **Change requests** go to the revising skill (Spec Kit: `clarify`) in one request — `speccoder spec clarify "…"`, or **Revise the Spec** in the app — and you review the result.
- **History.** `speccoder review` also shows what changed since the spec's last commit and every earlier version.

The review is recorded in `review.json` beside the spec; commit it with the spec so the team, the pull request and every later run see the same sign-off. Only `speccoder review` writes it: a change to it by a planning skill is undone and reported. With `[review] required = true`, a run waits (`REVIEW-PENDING`) until every requirement is approved in the committed review. In the [app](@/docs/web.md), the same review is **Spec & Plan › Review**.

## The current feature

Every command works on one feature, the **current feature**. speccoder keeps it where Spec Kit does — `.specify/feature.json`, a per-checkout file git ignores — so speccoder, Spec Kit's scripts and the `/speckit-*` commands in your editor agree.

```text
$ speccoder feature
▸  1  001-greet-function  ■■■■■■■□□□  7/10 tasks    spec · plan · tasks
   2  002-farewell        □□□□□□□□□□  no tasks yet spec
current feature: 001-greet-function (recorded in .specify/feature.json)

$ speccoder feature 2          # or: speccoder feature farewell, speccoder feature 002-farewell
switched to 002-farewell (recorded in .specify/feature.json: Spec Kit's scripts and the /speckit-* commands in your editor read it too)
```

- Order: `--feature NAME`, then `SPECIFY_FEATURE_DIRECTORY`, then `.specify/feature.json`, then the only feature. The first two choose for one command without changing the current one.
- `speccoder spec specify` makes the feature it creates current and says so. Other planning skills' commands are told the feature (`SPECIFY_FEATURE_DIRECTORY`).
- With several features and none current, `run` and `spec` ask at a terminal and remember the answer; elsewhere they stop with the list and the command that picks one. A pointer to a missing feature is reported, also by `speccoder doctor`.
- `report` and `doctor` say which feature and why (`recorded in .specify/feature.json`, `the only feature`, `named with --feature`). In the [dashboard](@/docs/running.md#the-dashboard), `f` opens a picker. `speccoder feature --clear` forgets the current feature.
- A feature with a spec but no `tasks.md` is **being planned**: `report`, bare `speccoder` and the dashboard show the planning skills that come next.

## The next step, taken

At a terminal, after a planning step, a decision on a task or the report, speccoder **offers the next step**, showing the command it stands for:

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

The journey line `spec → plan → tasks → commit → run → review` shows what is done, current and left. A commit takes exactly the feature's planning files, as `spec(<feature>): <files>`, and nothing else you have staged. Answer `n` to stop and do it yourself. Nothing is asked with `--json`, `--events`, `spec … --no-prompt` or without a terminal.

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

A step with `skill:` (and `args:`) runs a planning skill; without it, the step runs `speccoder run --no-prompt`. Options: `converge`, `feature`, `max_tasks`, `timeout` (seconds). When speccoder needs a person (exit 23), the step **pauses** instead of failing, and `resume` continues. The step type needs Spec Kit 1.1 or later; with older releases, a plain `shell` step running `speccoder --json run --no-prompt` works but fails rather than pauses.

## Writing specs that verify well

The strongest evidence comes from the acceptance examples in your user stories. Specs verify best when:

- each user story has concrete **Given / When / Then** examples with real values;
- examples are deterministic — no clocks, network or randomness in expected results;
- tasks name the files they change (`… in src/lib.rs`) and are small enough for one attempt;
- tasks that can run in parallel are marked `[P]` and touch different files.
