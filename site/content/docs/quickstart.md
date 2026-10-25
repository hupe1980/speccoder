+++
title = "Quickstart"
description = "From a git repository to a verified branch: a Spec Kit project, speccoder's checks, a spec, a run, the evidence, a pull request."
weight = 20

[extra]
group = "Start"
+++

From a git repository to a pull request whose every tick carries its evidence. About fifteen minutes, most of it reading what was written. Each command below says what to do next when you run it — you can follow those instead.

## 1. Make it a Spec Kit project

speccoder implements specs written in [GitHub Spec Kit](https://github.com/github/spec-kit)'s format, and runs Spec Kit's own planning skills. Install Spec Kit once, then in your repository:

```sh
uv tool install specify-cli --from git+https://github.com/github/spec-kit.git
specify init --here --integration claude
```

`--integration claude` matters: it installs the planning skills under `.claude/skills/`, which speccoder runs with its own model (it does not need Claude Code). In a script or CI, add `--force --non-interactive`. [OpenSpec](@/docs/openspec.md) works too.

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

`init` looks at your project (Cargo, npm/pnpm/yarn/bun, Python — with its `.venv` when there is one — Go, Make), proposes **checks** — the commands that decide whether nothing regressed — writes `speccoder.toml`, and runs the checks on your untouched code. A check that fails there is shown with the line that says why (`No module named pytest`), because every task would otherwise be judged against a red baseline. Commit what it wrote: speccoder reads committed files only. Change it later with `speccoder config` or **Settings** in the [app](@/docs/web.md).

## 3. A model key

```sh
export ANTHROPIC_API_KEY=…   # or OPENAI_API_KEY
```

speccoder reads the key from the environment only. A local model (Ollama, vLLM, LM Studio) works too: `speccoder config set model.provider openai-compatible` and its `base_url` ([configuration](@/docs/configuration.md#model)). `speccoder doctor` checks the whole setup — git, the project, the key, the sandbox — and changes nothing.

## 4. Write the spec

From an idea, or from a GitHub issue:

```sh
speccoder spec specify "A function slugify(text) that turns titles into URL slugs"
#   or: speccoder issue start 42
speccoder spec plan
speccoder spec tasks
```

Spec Kit's skills write `spec.md`, `plan.md` and `tasks.md`; after each, speccoder offers the next step and to commit exactly what was written. Read the spec before you go on — it is what the result will be judged against. More in [planning](@/docs/planning.md).

## 5. Run

```sh
speccoder run              # in the terminal
speccoder ui --window      # or as an app (macOS; `speccoder ui` opens your browser)
```

speccoder picks the next ready task, writes acceptance tests from the spec that must fail first, lets the model work in its own worktree, then judges:

```text
▶ T002 Implement `slugify(text)` in src/lib.rs  (001-slugify)
  · baseline checks
  · acceptance tests (judge-side)
  · attempt 1
    turn 1   read_file    src/lib.rs        allow
    turn 3   write_file   src/lib.rs        allow
    turn 4   run_command  cargo test        allow
  · verifying
✓ T002 verified · 10.2k in / 798 out · $0.0031 · 9s
```

When a call needs your approval, speccoder asks. When a task cannot pass, it stops and says why. Run again after you have decided; it continues where it stopped.

## 6. Read the evidence

```sh
speccoder report
```

```text
Phase 2: User Story 1 - URL slugs (Priority: P1)
  ✓ T002 Implement `slugify(text)` …            verified
  · T003 Handle Unicode input …                 ready
1/2 done: 1 verified; 1 ready · 1 commit on speccoder/001-slugify not merged
```

`verified` means the tests derived from the spec went from red to green and nothing regressed. [Reading the evidence](@/docs/evidence.md) explains every state; the app's **Code** view shows each tick's commit.

## 7. Hand it off

The work is on `speccoder/001-slugify`, one commit per task. Merge it, or open a draft pull request with the evidence in its description (and `Closes #42` for the issue it came from):

```sh
speccoder pr
```

## Next steps

- What needs you, in every feature: `speccoder inbox` — waiting calls, tests to rule on, tasks that did not pass, plans to commit, branches to hand off — or **Needs You** in the app.
- Close the gaps between spec and code: `speccoder run --converge`.
- Watch a run from a second terminal: `speccoder report --live`, or `speccoder` for the dashboard.
- Tighten or relax what the model may do: [configuration](@/docs/configuration.md).
