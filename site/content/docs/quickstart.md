+++
title = "Quickstart"
description = "From a git repository to a verified branch: a Spec Kit project, speccoder's checks, a spec, a run, the evidence, a pull request."
weight = 20

[extra]
group = "Start"
+++

From a git repository to a pull request whose every tick carries its evidence. Each command below also prints what to do next.

## 1. Make it a Spec Kit project

speccoder implements specs written in [GitHub Spec Kit](https://github.com/github/spec-kit)'s format, and runs Spec Kit's own planning skills. Install Spec Kit once, then in your repository:

```sh
uv tool install specify-cli --from git+https://github.com/github/spec-kit.git
specify init --here --integration claude
```

`--integration claude` installs the planning skills under `.claude/skills/`, which speccoder runs with its own model; Claude Code is not needed. [OpenSpec](@/docs/openspec.md) projects work too.

## 2. Set up speccoder

```sh
speccoder init
```

```text
Checks speccoder runs to decide "nothing regressed":
  test       fast  .venv/bin/python -m pytest -q --junitxml=.speccoder/junit.xml   a Python project (pytest, .venv/bin/python)
On the untouched code  test ✓ 0.4s
Model      anthropic/claude-sonnet-5-5  (ANTHROPIC_API_KEY is not set)
Commit speccoder.toml and .gitignore? speccoder reads committed files only. [Y/n]
```

`init` detects the project (Cargo, npm/pnpm/yarn/bun, Python with its `.venv`, Go, a Makefile `test` target), writes **checks** — the commands that decide whether nothing regressed — to `speccoder.toml`, and runs them on the untouched code. A check that fails there is shown with the line that says why, since every task would be judged against a red baseline. Commit what it wrote (speccoder reads committed files only); change it later with `speccoder config` or **Settings** in the [app](@/docs/web.md).

## 3. A model key

```sh
export ANTHROPIC_API_KEY=…   # or OPENAI_API_KEY
```

The key is read from the environment only. For a local model (Ollama, vLLM, LM Studio), set `model.provider` to `openai-compatible` and its `base_url` ([`[model]`](@/docs/configuration.md#model)). `speccoder doctor` checks git, the sandbox, the key, the spec tools and the config, and changes nothing.

## 4. Write the spec

From an idea, or from a [GitHub issue](@/docs/github.md#a-feature-from-an-issue):

```sh
speccoder spec specify "A function slugify(text) that turns titles into URL slugs"
#   or: speccoder issue start 42
speccoder spec plan
speccoder spec tasks
```

Spec Kit's skills write `spec.md`, `plan.md` and `tasks.md`; after each, speccoder offers the next step and to commit exactly what was written. Read the spec before you go on: the result is judged against it. See [planning](@/docs/planning.md).

## 5. Run

```sh
speccoder run              # in the terminal
speccoder ui --window      # or as an app (macOS; `speccoder ui` opens your browser)
```

speccoder works through the ready tasks: for each it writes acceptance tests from the spec that must fail first, lets the model work in its own worktree, then judges ([how it works](@/docs/how-it-works.md)). At a terminal this runs in the [dashboard](@/docs/running.md#the-dashboard); with `--plain` (abridged):

```text
▶ T002 Implement `slugify(text)` in src/lib.rs  (001-slugify)
  ✓ baseline
  ✓ red assertion
  · attempt 1/2
    turn 1   read_file    src/lib.rs        allow
    turn 3   write_file   src/lib.rs        allow
    turn 4   run_command  cargo test        allow
  ✓ attempt 1/2
  ✓ verify
  ✓ audit
  ✓ commit 9c1e2ab
✓ T002 verified · 10.2k in / 798 out · 9s — verdict: verified
```

When a call needs your approval, speccoder asks. When a task cannot pass, it stops and says why; run again after you have decided and it continues where it stopped.

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

`verified` means the tests derived from the spec went from red to green and nothing regressed. [Reading the evidence](@/docs/evidence.md) explains every state; the app's **Code** view shows each tick's commit.

## 7. Hand it off

The work is on `speccoder/001-slugify`, one commit per task. Merge it, or open a draft pull request with the evidence in its description (and `Closes #42` for the issue it came from):

```sh
speccoder auth login   # once
speccoder pr
```

## Next steps

- What waits for you in every feature: `speccoder inbox`, or **Needs You** in the app.
- Close the gaps between spec and code: `speccoder run --converge` ([convergence](@/docs/running.md#convergence)).
- A small change without planning: `speccoder do "…"` ([quick changes](@/docs/running.md#quick-changes)).
- Tighten or relax what the model may do: [configuration](@/docs/configuration.md).
