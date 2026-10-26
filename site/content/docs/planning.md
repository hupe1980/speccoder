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

## Review the tests

Approving a spec can freeze an **oracle**: the acceptance tests the run will use, read and shaped by you before any code exists. Per requirement with scenarios (Spec Kit: a user story, `US1`), **Write Tests** — `speccoder review tests US1 --write` — asks the test author twice, independently, compares what each says it asserts per scenario, and checks the first author's tests fail on the checkout's `HEAD` in a scratch worktree — each test alone, by its own command, first without its file and then twice with it, in the OS sandbox; the worktree is removed afterwards. Without the sandbox (`[sandbox] mode = "off"`, or no backend) Write Tests is refused (`SANDBOX-UNAVAILABLE`): model-written tests never run unconfined. A held-out set is written as at run time and sealed in the state directory: you see how many tests it holds and its digest, never a line of it.

```sh
speccoder review tests US1 --write                       # 2 author calls ([review] oracle_samples)
speccoder review tests US1 --reject tests/a.rs --reason "asserts the wrong rounding"
speccoder review tests US1 --again tests/a.rs --comment "check an empty name too"
speccoder review tests US1 --edit tests/a.rs --from my_test.rs   # your words, red-checked
speccoder review tests US1 --reading A US1-2             # which reading of a scenario is meant
```

- Every test must cite a scenario of its requirement; one that cites none is discarded and listed.
- Only a test that is **red** now — on an assertion or an absent symbol, its own and never a neighbour's — can be frozen; one that already passes is shown as *no evidence*. A test whose command another test shares, or whose command is red without it (a failing suite, a command that runs more than the file), is discarded and listed.
- An author's stated expectations that cannot be read, or that name a scenario the requirement does not have, fail the call (`ORACLE-AUTHOR-FAILED`): no readable comparison is never "no ambiguity".
- A rejection's reason, a comment and a chosen reading are told to every later author call for that requirement.
- **Ambiguous.** When the two authors state the same given with different outcomes for a scenario (or one finds it untestable), the requirement is *read two ways*: *Approve and Run* waits, a gated run refuses with `REVIEW-AMBIGUOUS`, and Needs You lists it until you choose a reading (B writes that scenario's tests again) or reword it.
- A quick change's or a bug's unit writes no tests here: its own pipeline authors and confirms them (`REVIEW-TESTS-OWN-PIPELINE`).

**Approve and Run** — `speccoder review approve --run ask|autopilot [--grant ID]…` — approves every decision line that is open or lapsed, freezes each requirement's red tests with the approval's digest (in `review.json`: `oracle` and `approval`, digests only), commits `review.json` and starts the run. In the app it binds to what its sheet showed: if a test changed in between (another tab, a *Write Tests* that finished) it is refused (`REVIEW-CHANGED`) and the sheet is shown again. A frozen requirement's tests are read only — the card shows the frozen set, and writing, rejecting or editing them is refused (`ORACLE-FROZEN`) until a reworded requirement lapses the freeze. *Ask on Risky Calls* asks inline as any run from the interface; *Autopilot* asks nobody (see [autopilot](@/docs/approvals.md#autopilot)). The first task citing a frozen requirement's scenarios writes those tests at its checkpoint, checks them red there with the sealed held-out set, and makes no author call; its tick carries `Acceptance: review manifest=… approval=… wording=… by=…`, and that trailer — never a file at the test's path — is what tells a later task the set was used. The frozen record must name the approval and the held-out set `review.json` records, or the task stops (`ORACLE-TAMPERED`); a `review.json` that cannot be read stops it too (`REVIEW-UNREADABLE`), never read as "nothing frozen". When the requirement's wording changes the frozen tests **lapse** and the run writes tests anew; a checkout without the stored set (a teammate's) says *frozen oracle not present here* and does the same; a stored test that no longer matches its digest stops the task (`ORACLE-TAMPERED`).

### Comments

Comment on what you read instead of rewording it yourself: a passage of a spec document, a line of a shown test, a review finding. Comments wait in a tray (`speccoder review comments`), kept in the state directory, and go out together as **one** revision by the format's revising skill (Spec Kit: `clarify`), limited to the sections they touch:

```sh
speccoder review comments add --path specs/001-x/spec.md --lines 40-42 "say who may log in"
speccoder review comments add --test US1:tests/a.rs:3 "check the error text too"
speccoder review comments send      # one revision; sections it changed outside your comments are flagged
speccoder review comments undo      # the documents back, byte for byte (refused if changed since)
```

A comment whose words moved is *outdated* and sent only if you keep it; a test-line comment asks the author again for that test after the revision (unless the revision lapsed its tests), and goes back to the tray when that did not run. A line in the Review's *since the last commit* list takes a comment too. A batch is sent once: a second send of the same comments is refused.

A section is its document and its heading: the same heading in another document is another section, flagged *outside your comments* when the revision changed it. The tray shows each changed section word by word until you keep or undo the revision. A document someone else changed while the revision ran — your own edit in the editor — is named *changed beside the revision*, and Undo refuses rather than revert it. Undo writes only the unit's own documents. A tray that cannot be read is refused (`REVIEW-UNREADABLE`), never read as empty.

On a bug unit the Tests tab lists the bug's confirmed reproductions, read only, with who confirmed them.

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
