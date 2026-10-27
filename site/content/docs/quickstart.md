+++
title = "Quickstart"
description = "From a git repository to a verified branch: a Spec Kit project, speccoder's checks, a spec, a run, the evidence, a pull request."
weight = 20

[extra]
group = "Start"
+++

From a git repository to a pull request whose every tick carries its evidence. Each command prints what to do next.

## Start a project

No folder yet, or one with code but no spec project: `speccoder new` makes it a project speccoder can judge, with the ecosystems' own commands.

```sh
speccoder new md2csv --idea "a CLI that turns Markdown tables into CSV" --dry-run   # the plan; nothing runs
speccoder new md2csv --idea "a CLI that turns Markdown tables into CSV"             # asks once, then runs it
```

```text
New project md2csv in ~/Workspaces/md2csv (new folder)
  stack   Rust (cargo) — "CLI" and cargo 1.91.0 is installed
  specs   Spec Kit 1.2.3 with the speccoder preset
  sandbox seatbelt (writes: this folder and speccoder's cache)

   1  git init                                                              local
   2  cargo init --lib --name md2csv                                        local
   3  cp /dev/null src/lib.rs                                               local
   4  cargo fmt --all                                                       local
   5  specify init --here --force --non-interactive --integration claude …  local
   6  speccoder init --yes                                                  local
   7  git commit -m "new project: md2csv"   as Sam Doe <sam@example.com>    local
```

| | |
|---|---|
| Stack | `--stack rust\|python\|node\|go\|none`; suggested from the idea and the toolchains installed. Rust runs `cargo init --lib` and empties `src/lib.rs` of cargo's placeholder, Python `uv init --package` and `uv add --dev pytest`, TypeScript `npm init -y`, `npm install --save-dev vitest` and sets `npm test` to `vitest run --passWithNoTests`, Go `go mod init` |
| Spec format | `--format speckit` (default) or `--format openspec` (`openspec init --tools claude`, its settings kept in speccoder's cache, not `~/.config`). Spec Kit gets the [speccoder preset](@/docs/planning.md#the-speccoder-preset) (`--no-preset` leaves it out) and `--extension NAME\|PATH\|URL` |
| Network | A step that needs the network says so, with the hosts it is expected to reach; the one question lists them. Such a step gets the full network: the hosts are not enforced |
| Sandbox | Every step runs with stdin closed, a time limit and without the model keys, in the OS sandbox: writes only in the folder and speccoder's cache, `.git` read-only once it exists, no network unless the step is labelled network |
| Existing repository | At its top level, with a clean tree: no `git init` and no scaffold; the format's init, `speccoder init --yes`, one commit of exactly what they wrote |
| The commit | One, as you (your git identity; refused without one), `new project: <name>`, holding exactly what the steps produced |

speccoder writes none of the stack's or the format's files itself; `speccoder.toml` is written by `init`. The plan is bound to a digest over its commands and the folder as seen: a plan that changed before it ran is refused (`NEW-PLAN-CHANGED`). A step that fails stops the run; running the same command again resumes at it — only while the folder is exactly as the run left it, else every refusal applies again. A step may not change `.git` or, in an existing repository, your tracked files; the commit takes only what the steps made. Without a terminal, `speccoder new` prints the plan and stops (`NEW-CONFIRM`) until `--yes`.

`speccoder new --undo` moves a folder speccoder created to the Trash — only while nothing but its commit exists there — or, in an existing repository, reverts speccoder's commit (files that were ignored before it ran don't count). It never trashes a folder speccoder did not make. A new project's first checks run on the untouched code; an empty test suite reads *no tests yet*.

`speccoder sample` copies a recorded run of a small project into speccoder's state directory and replays it strictly — no model, no key, no network — then `speccoder ui -C <its path>` plays it back. A build without the recording answers `SAMPLE-MISSING`.

In the app, a host started outside a project shows **Welcome to speccoder**: New Project, Open Project…, Clone from GitHub…, Try the Sample and the recent projects ([the app](@/docs/web.md#welcome)).

## 1. Make it a Spec Kit project

```sh
uv tool install specify-cli --from git+https://github.com/github/spec-kit.git
specify init --here --integration claude
```

`--integration claude` installs the planning skills under `.claude/skills/`; speccoder runs them with its own model, so Claude Code is not needed. [OpenSpec](@/docs/openspec.md) projects work too.

## 2. Set up speccoder

```sh
speccoder init
```

```text
Checks speccoder runs to decide "nothing regressed":
  test       fast  uv run --locked --offline python -m pytest -q --junitxml=.speccoder/junit.xml   a Python project (pytest, uv run --locked --offline python)
On the untouched code  test ✓ 0.4s
Model      anthropic/claude-sonnet-5-5  (ANTHROPIC_API_KEY is not set)
Commit speccoder.toml and .gitignore? speccoder reads committed files only. [Y/n]
```

`init` detects the project (Cargo, npm/pnpm/yarn/bun, Python, Go, a Makefile `test` target), writes **checks** (the commands that decide whether nothing regressed) to `speccoder.toml`, and runs them on the untouched code; a check that fails there is shown with the reason. A uv project's checks run through `uv run --locked --offline`: each run's worktree gets its own `.venv` from `uv.lock` and uv's cache, so `.venv/` stays out of git. Commit what it wrote. Change it later with [`speccoder config`](@/docs/configuration.md#changing-it) or **Settings** in the [app](@/docs/web.md).

## 3. A model key

```sh
export ANTHROPIC_API_KEY=…   # or OPENAI_API_KEY
speccoder doctor             # git, the sandbox, the key, the spec tools, the config; changes nothing
```

The key is read from the environment only. For a local model (Ollama, vLLM, LM Studio), set `model.provider = "openai-compatible"` and `model.base_url` ([`[model]`](@/docs/configuration.md#model)).

## 4. Write the spec

From an idea, or from a [GitHub issue](@/docs/github.md#a-feature-from-an-issue):

```sh
speccoder spec specify "A function slugify(text) that turns titles into URL slugs" --through tasks
#   or: speccoder issue start 42 --through tasks
```

`--through tasks` runs Spec Kit's `specify`, then — once you approved the spec's requirements — `plan` and `tasks`, stopping for open questions and your review ([plan through](@/docs/planning.md#plan-through)). Without it, each skill runs on its own (`speccoder spec plan`, `speccoder spec tasks`). After each step speccoder offers the next one and to commit exactly what was written. Read the spec before you go on: the result is judged against it. To sign it off requirement by requirement, use [`speccoder review`](@/docs/planning.md#review-the-spec).

## 5. Run

```sh
speccoder run   # in the terminal
speccoder ui    # or in your browser (`--window`: speccoder's own window)
```

At a terminal this runs in the [dashboard](@/docs/running.md#the-dashboard); with `--plain` (abridged):

```text
▶ T002 Implement `slugify(text)` in src/lib.rs  (001-slugify)
  ✓ baseline
  ✓ red assertion
  · attempt 1/2
    turn 1   read_file    src/lib.rs        allow
    turn 4   run_command  cargo test        allow
  ✓ verify
  ✓ audit
  ✓ commit 9c1e2ab
✓ T002 verified · 10.2k in / 798 out · 9s — verdict: verified
```

When a call needs your approval, speccoder asks. When a task cannot pass, it stops and says why; after you decide, `speccoder run` continues where it stopped. What each step does: [how it works](@/docs/how-it-works.md).

## 6. Read the evidence

```sh
speccoder report
```

```text
Phase 2: User Story 1 - URL slugs (Priority: P1)
  ✓ T002 Implement `slugify(text)` …            verified
  · T003 Handle Unicode input …                 ready
1/2 done: 1 verified; 1 ready
```

Every state: [reading the evidence](@/docs/evidence.md).

## 7. Hand it off

The work is on `speccoder/001-slugify`, one commit per task. Merge it, or open a draft [pull request](@/docs/github.md#its-pull-request) with the evidence in its description:

```sh
speccoder auth login   # once
speccoder pr
```

## Next steps

- What waits for you in every feature: `speccoder inbox`, or **Needs You** in the app.
- Close the gaps between spec and code: `speccoder run --converge` ([convergence](@/docs/running.md#convergence)).
- A small change without planning: `speccoder do "…"` ([quick changes](@/docs/running.md#quick-changes)).
- Tighten or relax what the model may do: [configuration](@/docs/configuration.md).
