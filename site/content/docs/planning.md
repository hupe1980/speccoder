+++
title = "Planning with Spec Kit"
description = "Write the spec, plan and tasks with Spec Kit's own skills run by speccoder, honour Spec Kit extension hooks, and run speccoder as a step of a Spec Kit workflow."
weight = 40

[extra]
group = "Use"
nav = "Planning with Spec Kit"
+++

speccoder runs Spec Kit's own planning skills, honours its extension hooks and plugs into its workflow engine. For OpenSpec, see [OpenSpec planning](@/docs/openspec.md#planning).

## Spec Kit's planning skills

`specify init --here --integration claude` installs Spec Kit's commands as Agent Skills (`.claude/skills/speckit-*/SKILL.md`); speccoder finds skills under `.claude/skills/` and `.agents/skills/` and runs them with its own model:

```sh
speccoder spec                                         # the installed skills
speccoder spec specify "A function greet(name) that …" # spec.md
speccoder spec clarify                                 # asks you about what is unclear
speccoder spec plan                                    # plan.md, research.md, data-model.md, quickstart.md
speccoder spec tasks                                   # tasks.md
speccoder spec analyze                                 # a read-only consistency report
```

A planning skill works **in your checkout**, because the spec belongs on your branch. Without asking, it may write `specs/…`, `.specify/feature.json` and `.specify/memory/…` and run Spec Kit's scripts (`.specify/scripts/`, and those of installed extensions). Writing source code needs your approval; Spec Kit's templates and scripts, `speccoder.toml`, secrets and `review.json` are denied.

Results are left **uncommitted**; speccoder [offers the commit](#the-next-step-taken), because `speccoder run` reads committed files only. A clarifying question is asked at the terminal; with `--no-prompt` it stays in the spec as `[NEEDS CLARIFICATION]`.

Not hosted: `speckit-implement` and `speckit-converge` (use `speccoder run` and `speccoder converge`, which record evidence) and `speckit-taskstoissues` (speccoder goes the other way: `speccoder issue start N` specifies a feature from an issue, see [GitHub](@/docs/github.md#a-feature-from-an-issue)).

## Review the spec

The result is judged against the spec, so you sign off its requirements one at a time:

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
```

- **Open questions.** Every `[NEEDS CLARIFICATION: …]` in the feature's documents is a question (`Q1`, `Q2`, …). `answer` puts your words where it stood and touches nothing else.
- **Edits.** `edit` keeps the identifier; the approval lapses unless `--approve`. A wording that still contains a question cannot be approved; `approve` without IDs skips it and says so.
- **Lapses.** A decision is bound to the exact line and names who made it (git's `user.name`). Reword the line and it lapses: it shows again, old wording beside new.
- **Change requests** go to the revising skill (Spec Kit: `clarify`) in one request (`speccoder spec clarify "…"`, or **Revise the Spec** in the app).
- **History.** `speccoder review` also shows what changed since the spec's last commit, and its earlier versions. `review clear` removes a decision.

The review is kept in `review.json` beside the spec; commit it with the spec so the team, the pull request and every later run see the same sign-off. Only `speccoder review` writes it: a change by a planning skill is undone and reported. With `[review] required = true`, a run waits (`REVIEW-PENDING`) until every requirement is approved in the committed review. In the [app](@/docs/web.md) the review is **Spec & Plan › Review**.

## Review the tests

Before any code exists you can read and shape the **oracle**: the acceptance tests a run will use for a requirement with scenarios (Spec Kit: a user story, `US1`). Approving freezes them.

```sh
speccoder review tests US1 --write                       # write them
speccoder review tests US1 --reject tests/a.rs --reason "asserts the wrong rounding"
speccoder review tests US1 --again tests/a.rs --comment "check an empty name too"
speccoder review tests US1 --edit tests/a.rs --from my_test.rs   # your content, red-checked
speccoder review tests US1 --reading A US1-2             # which reading of a scenario is meant
```

**Write Tests** (`--write`) calls the test author `[review] oracle_samples` times (1 or 2, default 2), independently, and compares what each says it asserts per scenario. The first author's tests are red-checked on `HEAD` in a scratch worktree, in the OS sandbox: each test alone, by its own command, first without its file, then twice with it. Without the sandbox it is refused (`SANDBOX-UNAVAILABLE`). With `[acceptance] held_out` on (the default), a [held-out set](@/docs/evidence.md#held-out-tests) is written and sealed in the state directory; you see its count and digest only.

- Every test must cite a scenario of its requirement; one that cites none is discarded and listed.
- Only a test that is **red** now, on an assertion or an absent symbol, can be frozen; one that already passes is shown as *no evidence*. A test whose command another test shares, or whose command is red without the file (a failing suite), is discarded and listed.
- With two samples, expectations that cannot be read or that name an unknown scenario fail the call (`ORACLE-AUTHOR-FAILED`).
- Rejection reasons, comments and chosen readings are told to every later author call for that requirement.
- **Ambiguous.** When the two authors state the same given with different outcomes (or one finds it untestable), the requirement is *read two ways*: Approve and Run and a gated run wait (`REVIEW-AMBIGUOUS`) until you choose a reading (B rewrites that scenario's tests) or reword it.
- A quick change or a bug writes no tests here (`REVIEW-TESTS-OWN-PIPELINE`): its own pipeline does. On a bug unit the Tests tab lists its confirmed reproductions, read only.

### Approve and Run

```sh
speccoder review approve --run ask                        # risky calls ask inline
speccoder review approve --run autopilot --list-grants    # the grants offered; approves nothing
speccoder review approve --run autopilot --grant ID       # nobody is asked
```

`--run` approves every open or lapsed decision line, freezes each requirement's red tests (`review.json` records `oracle` and `approval`, digests only), commits `review.json` and starts the run. It is refused while a question is open (`REVIEW-PENDING`), a requirement is read two ways, or a run is active. `autopilot` needs the OS sandbox and holds only the grants you name ([autopilot](@/docs/approvals.md#autopilot)).

- In the app it binds to what its sheet showed: a test changed in between is refused (`REVIEW-CHANGED`) and the sheet shown again.
- Frozen tests are read only (`ORACLE-FROZEN`) until a reworded requirement lapses the freeze; the run then writes tests anew. A checkout without the stored set (a teammate's) says *frozen oracle not present here* and does the same.
- The first task citing a frozen requirement's scenarios writes those tests, checks them red with the sealed held-out set, and makes no author call. Its tick carries `Acceptance: review manifest=… approval=… wording=… by=…`; that trailer tells later tasks the set was used.
- A stored test or record that no longer matches its digests stops the task (`ORACLE-TAMPERED`); so does an unreadable `review.json` (`REVIEW-UNREADABLE`).

### Comments

Comment on what you read instead of rewording it: a passage of a spec document, a line of a shown test, a review finding (`--finding SRC:ID`). Comments wait in a tray (`speccoder review comments`, kept in the state directory) and go out together as **one** revision by the format's revising skill (Spec Kit: `clarify`), limited to the sections they touch:

```sh
speccoder review comments add --path specs/001-x/spec.md --lines 40-42 "say who may log in"
speccoder review comments add --test US1:tests/a.rs:3 "check the error text too"
speccoder review comments send      # one revision; sections it changed outside your comments are flagged
speccoder review comments keep t1   # keep the revision (or an outdated comment, by its id)
speccoder review comments undo      # the documents back, byte for byte
```

- A comment whose words moved is *outdated*, sent only if you keep it.
- A test-line comment asks the author again for that test after the revision, unless the revision lapsed its tests.
- A batch is sent once; a second send of the same comments is refused.
- A section is its document and its heading. The tray shows each changed section word by word until you keep or undo the revision.
- A document someone else changed while the revision ran is named *changed beside the revision*. Undo writes only the unit's own documents and refuses one changed since.
- A tray that cannot be read is refused (`REVIEW-UNREADABLE`), never read as empty.

## The current feature

Every command works on one feature, the **current feature**. speccoder keeps it where Spec Kit does, in `.specify/feature.json` (per checkout, ignored by git), so speccoder, Spec Kit's scripts and the `/speckit-*` commands in your editor agree.

```text
$ speccoder feature
▸  1  001-greet-function  ■■■■■■■□□□  7/10 tasks    spec · plan · tasks
   2  002-farewell        □□□□□□□□□□  no tasks yet spec
current feature: 001-greet-function (recorded in .specify/feature.json)

$ speccoder feature 2          # or: speccoder feature farewell, speccoder feature 002-farewell
switched to 002-farewell (recorded in .specify/feature.json: …)
```

- Order: `--feature NAME`, then `SPECIFY_FEATURE_DIRECTORY`, then `.specify/feature.json`, then the only feature. The first two choose for one command without changing the current one.
- `speccoder spec specify` makes the feature it creates current and says so. Other planning skills' commands are told the feature (`SPECIFY_FEATURE_DIRECTORY`).
- With several features and none current, `run` and `spec` ask at a terminal and remember the answer; elsewhere they stop with the list. `speccoder doctor` reports a pointer to a missing feature.
- `report` and `doctor` say which feature and why. In the [dashboard](@/docs/running.md#the-dashboard), `f` opens a picker. `speccoder feature --clear` forgets the current feature.
- A feature with a spec but no `tasks.md` is **being planned**: `report` and the dashboard show the planning skills that come next.

## The next step, taken

At a terminal, after a planning step, a task decision or the report, speccoder **offers the next step** and the command it stands for (abridged):

```text
  ✓ spec ── ✓ plan ── ✓ tasks ── ● commit ── · run ── · review

  ┌─ Next ──────────────────────────────────────────────────────┐
  │ Commit the planning files (1)                               │
  │ + specs/001-greet-function/tasks.md                         │
  │ $ git add specs/001-greet-function/tasks.md && git commit … │
  └─────────────────────────────────────────────────────────────┘
  Commit them? [Y/n]
```

The journey line shows what is done, current and left. A commit takes exactly the feature's planning files, as `spec(<feature>): <files>`, and nothing else you have staged. Answer `n` to do it yourself. Nothing is asked with `--json`, `--events`, `spec … --no-prompt` or without a terminal.

## Extension hooks

Spec Kit extensions register hooks in `.specify/extensions.yml` (the git extension, for example, creates a feature branch before `specify`). speccoder runs them around what it runs:

| Event | Around |
|---|---|
| `before_<skill>` / `after_<skill>` | a hosted planning skill, with its arguments |
| `before_implement` / `after_implement` | `speccoder run` |
| `before_converge` / `after_converge` | convergence |

A **mandatory** hook (`optional: false`) runs as a hosted skill under the planning policy; if its skill is missing or does not complete, the run stops before any task. An **optional** hook is offered with its prompt and command. Hooks with a `condition` are left to Spec Kit's executor; disabled ones are ignored.

## Spec Kit workflows

Spec Kit's workflow engine chains steps with review gates. `speccoder workflow install` adds a `speccoder` step type and an example workflow: specify, plan and tasks, each followed by a gate, then a commit and `run --converge`.

```sh
speccoder workflow install
specify workflow run .specify/workflows/speccoder/workflow.yml \
  --input spec="A Rust function clamp(x, lo, hi) that …"
# paused? decide (speccoder inbox, speccoder task approve|accept|nudge), then:
specify workflow resume <run-id>
```

A step with `skill:` (and `args:`) runs a planning skill; with `bug:` (a slug of Spec Kit's `bug` extension) it runs `speccoder bug run --slug`; otherwise `speccoder run --no-prompt`. Other options: `converge`, `feature`, `max_tasks`, `timeout` (seconds), `executable`. When speccoder needs a person (exit 23), the step **pauses** instead of failing, and `resume` runs it again. The step type needs Spec Kit 1.1 or later; with older releases, a plain `shell` step running `speccoder --json run --no-prompt` works but fails rather than pauses.

## Writing specs that verify well

The strongest evidence comes from the acceptance examples in your user stories. Specs verify best when:

- each user story has concrete **Given / When / Then** examples with real values;
- examples are deterministic — no clocks, network or randomness in expected results;
- tasks name the files they change (`… in src/lib.rs`) and are small enough for one attempt;
- tasks that can run in parallel are marked `[P]` and touch different files.
